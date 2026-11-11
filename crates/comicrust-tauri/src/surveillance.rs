//! Surveillance de la base et comportement hors ligne (étape 6, `tauri-v0.7.0`).
//!
//! Un fil de sondage (toutes les 5 s) interroge la base de la bibliothèque ouverte :
//! - base MySQL de ComicRack : `comicrust_core::db::Watcher` (compteurs de `changes`) ;
//! - base de ComicRust (SQLite ou MySQL) : `comicrust_core::store::StoreWatcher` ;
//! - `ComicDb.xml` : pas de sondage (ComicRack réécrit ce fichier lui-même).
//!
//! Les modifications trouvées (faites dans ComicRack, ou par un autre poste) sont appliquées à la
//! bibliothèque affichée ([`Biblio::appliquer_changements`]) puis signalées au front par
//! `bibliotheque-modifiee` ; l'état en ligne / hors ligne l'est par `base-etat`. Logique reprise de
//! `crates/comicrust/src/library/watch.rs` (version egui), dupliquée : on ne touche pas à ce crate.
//!
//! **Écritures de ComicRust** : elles reviennent aussi par le sondage (le compteur a bougé). Les
//! sites d'écriture les signalent à [`Connexion`] (`note_written` / `note_deleted` des
//! surveillants), qui les écarte ; sans signalement, la fiche serait seulement relue (rafraîchie)
//! une fois, sans autre effet.
//!
//! **Hors ligne** : une erreur de sondage (serveur arrêté, réseau coupé, base disparue) passe
//! l'état à « hors ligne ». Les écritures dans la base sont alors refusées avec un message
//! lisible ([`Biblio::motif_hors_ligne`], utilisé par l'édition et les listes) ; la lecture reste
//! mémorisée localement (`avancement.json`). Dès qu'un sondage réussit, l'état revient « en
//! ligne » et les modifications survenues entre-temps sont relues. Aucune donnée n'est perdue
//! et rien ne plante : le sondage rouvre sa connexion à chaque essai.
//!
//! **Mot de passe MySQL** : jamais dans l'état, les événements ni les messages ; les causes
//! d'erreur passent par [`masquer_secret`].
//!
//! Aucune dépendance à Tauri : tout est testé ici (SQLite autonome ; MySQL réel seulement si
//! `COMICRUST_TEST_MYSQL` est définie). Contrat : `docs/tauri/ipc-surveillance.md`.

use crate::biblio::{Biblio, SourceBiblio};
use comicrust_core::db::{Changes, Watcher, WriteReport};
use comicrust_core::store::{StoreConfig, StoreWatcher};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Période du sondage (celle de la version egui, `DB_INTERVAL`).
pub const PERIODE_SONDAGE: Duration = Duration::from_secs(5);
/// Pas de réveil du fil (arrêt et « réessayer » réactifs).
const PAS: Duration = Duration::from_millis(100);

/// Message affiché quand une écriture dans la base est refusée parce qu'elle est injoignable.
pub const MOTIF_HORS_LIGNE: &str = "Base injoignable : modifications impossibles pour le moment. ComicRust réessaie \
     automatiquement ; rien n'est perdu (la lecture reste mémorisée sur ce poste).";

/// Configuration MySQL de ComicRack incomplète (saisie à la main : serveur, utilisateur ou base vide).
pub const CONFIG_MYSQL_INCOMPLETE: &str = "Configuration MySQL de ComicRack incomplète (serveur, utilisateur ou \
     base manquant) : écriture impossible. Corrige-la dans les réglages.";

// ───────────────────────────── Types de l'IPC ─────────────────────────────

/// Nature de la bibliothèque ouverte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceBase {
    ComicrackMysql,
    Comicrust,
    ComicrackXml,
    Aucune,
}

/// État de la connexion à la base (commande `etat_base`, événement `base-etat`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatBase {
    pub source: SourceBase,
    /// La base est interrogée régulièrement (MySQL ComicRack, base de ComicRust).
    pub surveillee: bool,
    /// Faux : le dernier sondage a échoué, un nouvel essai a lieu toutes les 5 s.
    pub en_ligne: bool,
    /// Cause de l'échec (sans mot de passe), quand `en_ligne` est faux.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Échecs consécutifs depuis la perte de la connexion (0 : en ligne).
    pub essais: u32,
}

impl EtatBase {
    /// Égal à `autre` hors compteur d'essais (seules les transitions sont émises).
    fn meme_etat(&self, autre: &EtatBase) -> bool {
        self.source == autre.source
            && self.surveillee == autre.surveillee
            && self.en_ligne == autre.en_ligne
            && self.message == autre.message
    }
}

/// Événements envoyés au front.
#[derive(Debug, Clone, PartialEq)]
pub enum EvenementBase {
    /// L'état de la connexion a changé.
    Etat(EtatBase),
    /// Des modifications extérieures ont été appliquées à la bibliothèque affichée.
    Modifiee { ids: Vec<String>, ajoutes: usize, modifies: usize, supprimes: usize },
}

impl EvenementBase {
    /// Nom de l'événement Tauri et sa charge JSON (camelCase), selon le contrat IPC.
    pub fn pour_le_front(&self) -> (&'static str, serde_json::Value) {
        use serde_json::json;
        match self {
            EvenementBase::Etat(e) => ("base-etat", json!(e)),
            // Même événement que l'édition (`ids`) : la vue se rafraîchit déjà ainsi ; les
            // champs en plus distinguent l'origine (modification extérieure).
            EvenementBase::Modifiee { ids, ajoutes, modifies, supprimes } => (
                "bibliotheque-modifiee",
                json!({"ids": ids, "origine": "externe", "ajoutes": ajoutes, "modifies": modifies, "supprimes": supprimes}),
            ),
        }
    }
}

// ───────────────────────────── État partagé ─────────────────────────────

/// Écritures de ComicRust à signaler au surveillant (prises en compte avant chaque sondage).
#[derive(Debug)]
enum Ecriture {
    /// Base MySQL de ComicRack : (identifiant, nouveau `comics.update_counter`).
    Mysql(Vec<(String, i64)>),
    /// Base de ComicRust : identifiants et compteur de la ligne écrite.
    Autonome(Vec<String>, i64),
}

#[derive(Debug, Clone)]
struct HorsLigne {
    message: String,
    essais: u32,
}

/// État de la connexion et écritures à signaler, partagés entre la bibliothèque, l'édition, le
/// lecteur et le fil de sondage (un par [`Biblio`]).
#[derive(Default)]
pub struct Connexion {
    hors_ligne: Mutex<Option<HorsLigne>>,
    ecrites: Mutex<Vec<Ecriture>>,
    /// Livres que ComicRust supprime, à signaler à la surveillance (voir [`Connexion::supprimant`]).
    supprimees: Mutex<Vec<String>>,
}

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Connexion {
    /// Cause de la perte de connexion (`None` : en ligne).
    pub fn message_hors_ligne(&self) -> Option<String> {
        verrou(&self.hors_ligne).as_ref().map(|h| h.message.clone())
    }

    /// Échecs consécutifs (0 : en ligne).
    pub fn essais(&self) -> u32 {
        verrou(&self.hors_ligne).as_ref().map_or(0, |h| h.essais)
    }

    fn passer_hors_ligne(&self, message: String) {
        let mut g = verrou(&self.hors_ligne);
        let essais = g.as_ref().map_or(0, |h| h.essais) + 1;
        *g = Some(HorsLigne { message, essais });
    }

    fn revenir_en_ligne(&self) {
        *verrou(&self.hors_ligne) = None;
    }

    /// Signale des lignes écrites dans la base MySQL de ComicRack.
    pub fn ecrit_mysql(&self, ecrits: impl IntoIterator<Item = (String, i64)>) {
        let v: Vec<(String, i64)> = ecrits.into_iter().collect();
        if !v.is_empty() {
            verrou(&self.ecrites).push(Ecriture::Mysql(v));
        }
    }

    /// Signale des lignes écrites dans la base de ComicRust (`compteur` : valeur renvoyée par
    /// l'écriture).
    pub fn ecrit_autonome(&self, ids: impl IntoIterator<Item = String>, compteur: i64) {
        let v: Vec<String> = ids.into_iter().collect();
        if !v.is_empty() {
            verrou(&self.ecrites).push(Ecriture::Autonome(v, compteur));
        }
    }

    /// Signale `ids` puis exécute `supprimer`, sans que le sondage interroge la base entre-temps
    /// (le verrou reste pris pendant la suppression ; le sondage prend le même). Une
    /// interrogation a donc lieu avant la suppression, ou après, une fois signalée.
    pub fn supprimant<R>(&self, ids: impl IntoIterator<Item = String>, supprimer: impl FnOnce() -> R) -> R {
        let mut en_attente = verrou(&self.supprimees);
        en_attente.extend(ids.into_iter().map(|i| i.to_ascii_lowercase()));
        supprimer()
    }
}

/// Remplace le secret par `***` dans un message (le mot de passe ne doit apparaître nulle part).
pub fn masquer_secret(message: &str, secret: &str) -> String {
    if secret.is_empty() {
        message.to_string()
    } else {
        message.replace(secret, "***")
    }
}

/// Mot de passe de la base de la bibliothèque (pour masquer les messages), vide s'il n'y en a pas.
fn secret_de(source: &SourceBiblio) -> String {
    match source {
        SourceBiblio::ComicrackMysql(c) | SourceBiblio::Autonome(StoreConfig::MySql(c)) => c.password.clone(),
        _ => String::new(),
    }
}

// ───────────────────────────── Sondage ─────────────────────────────

/// Surveillant de la base, d'après la source de la bibliothèque.
enum Veilleur {
    Mysql(Box<Watcher>),
    Autonome(Box<StoreWatcher>),
}

/// Un sondage à la fois ; reconstruit quand la bibliothèque est rechargée ou change de source.
pub struct Sondeur {
    biblio: Biblio,
    /// (génération de la base, source) pour lesquelles `veilleur` a été créé.
    cle: Option<(u64, SourceBiblio)>,
    veilleur: Option<Veilleur>,
    /// Dernier état émis ou lu (pour n'émettre que les transitions).
    dernier: Option<EtatBase>,
}

impl Sondeur {
    pub fn nouveau(biblio: Biblio) -> Self {
        Sondeur { biblio, cle: None, veilleur: None, dernier: None }
    }

    /// Un sondage. Renvoie les événements à émettre (état changé, modifications appliquées).
    /// Rien n'est fait pendant une analyse ou une opération d'édition (elles rechargent ou
    /// reportent elles-mêmes la base ; le sondage les prendrait pour des modifications extérieures).
    pub fn sonder(&mut self) -> Vec<EvenementBase> {
        let mut evenements = Vec::new();
        let Some(inst) = self.biblio.instantane_surveillance() else {
            self.cle = None;
            self.veilleur = None;
            self.noter_etat(&mut evenements);
            return evenements;
        };
        if inst.occupe {
            return evenements;
        }
        let cle = (inst.generation, inst.source.clone());
        if self.cle.as_ref() != Some(&cle) {
            self.veilleur = match &inst.source {
                SourceBiblio::ComicrackMysql(cfg) => {
                    Some(Veilleur::Mysql(Box::new(Watcher::new(cfg.clone(), &inst.base))))
                }
                SourceBiblio::Autonome(cfg) => {
                    Some(Veilleur::Autonome(Box::new(StoreWatcher::new(cfg.clone(), &inst.base))))
                }
                SourceBiblio::ComicrackXml | SourceBiblio::Aucune => None,
            };
            self.cle = Some(cle);
            // Bibliothèque (re)chargée : elle vient d'être lue, donc la base répond.
            self.biblio.connexion().revenir_en_ligne();
        }
        let secret = secret_de(&inst.source);
        let connexion = self.biblio.connexion();
        if let Some(v) = self.veilleur.as_mut() {
            let resultat = {
                // Suppressions de ComicRust relevées puis base interrogée sous le même verrou.
                let mut garde = verrou(&connexion.supprimees);
                let supprimees = std::mem::take(&mut *garde);
                let ecrites = std::mem::take(&mut *verrou(&connexion.ecrites));
                sonder_veilleur(v, ecrites, supprimees)
            };
            match resultat {
                Ok(Some(ch)) => {
                    connexion.revenir_en_ligne();
                    if let Some((ap, ids)) = self.biblio.appliquer_changements(inst.generation, ch) {
                        if ap.added + ap.modified + ap.removed > 0 {
                            evenements.push(EvenementBase::Modifiee {
                                ids,
                                ajoutes: ap.added,
                                modifies: ap.modified,
                                supprimes: ap.removed,
                            });
                        }
                    }
                }
                Ok(None) => connexion.revenir_en_ligne(),
                Err(e) => connexion.passer_hors_ligne(masquer_secret(&e, &secret)),
            }
        }
        self.noter_etat(&mut evenements);
        evenements
    }

    /// État actuel (lu dans la bibliothèque : le dernier sondage fait foi).
    pub fn etat(&self) -> EtatBase {
        self.biblio.etat_base()
    }

    /// Émet l'état en tête des événements s'il a changé depuis le dernier émis.
    fn noter_etat(&mut self, evenements: &mut Vec<EvenementBase>) {
        let etat = self.biblio.etat_base();
        if self.dernier.as_ref().is_none_or(|d| !d.meme_etat(&etat)) {
            // Hors ligne d'abord : l'indicateur s'affiche avant le rafraîchissement.
            evenements.insert(0, EvenementBase::Etat(etat.clone()));
        }
        self.dernier = Some(etat);
    }
}

/// Signale les écritures et suppressions de ComicRust au surveillant, puis l'interroge (l'appelant
/// garde le verrou des suppressions pendant l'appel).
fn sonder_veilleur(
    v: &mut Veilleur,
    ecrites: Vec<Ecriture>,
    supprimees: Vec<String>,
) -> Result<Option<Changes>, String> {
    match v {
        Veilleur::Mysql(w) => {
            for e in ecrites {
                if let Ecriture::Mysql(l) = e {
                    w.note_written(l.iter().map(|(id, c)| (id.as_str(), *c)));
                }
            }
            w.note_deleted(supprimees.iter().map(String::as_str));
            w.poll().map_err(|e| e.to_string())
        }
        Veilleur::Autonome(w) => {
            for e in ecrites {
                if let Ecriture::Autonome(ids, c) = e {
                    w.note_written(ids.iter().map(String::as_str), c);
                }
            }
            w.note_deleted(supprimees.iter().map(String::as_str));
            w.poll().map_err(|e| e.to_string())
        }
    }
}

/// Émetteur d'événements (l'application y branche `app.emit`).
pub type Sortie = Arc<dyn Fn(EvenementBase) + Send + Sync>;

/// Fil de sondage ; arrêté quand il est libéré.
pub struct Surveillance {
    sondeur: Arc<Mutex<Sondeur>>,
    sortie: Sortie,
    arret: Arc<AtomicBool>,
}

impl Surveillance {
    /// Démarre le fil : un sondage toutes les `periode` (le premier après une période, la
    /// bibliothèque venant d'être lue).
    pub fn demarrer(biblio: Biblio, periode: Duration, sortie: Sortie) -> Self {
        let sondeur = Arc::new(Mutex::new(Sondeur::nouveau(biblio)));
        let arret = Arc::new(AtomicBool::new(false));
        let (s, a, o) = (sondeur.clone(), arret.clone(), sortie.clone());
        std::thread::Builder::new()
            .name("comicrust-surveillance".into())
            .spawn(move || {
                let mut prochain = Instant::now() + periode;
                while !a.load(Ordering::Relaxed) {
                    if Instant::now() >= prochain {
                        let evenements = verrou(&s).sonder();
                        for e in evenements {
                            o(e);
                        }
                        // La période court depuis la fin du sondage (une connexion qui expire
                        // ne provoque pas de rafale d'essais).
                        prochain = Instant::now() + periode;
                    }
                    std::thread::sleep(PAS);
                }
            })
            .expect("fil de surveillance");
        Surveillance { sondeur, sortie, arret }
    }

    /// Bouton « Réessayer » : sondage immédiat (dans le fil de l'appelant), événements émis,
    /// état renvoyé.
    pub fn reessayer(&self) -> EtatBase {
        let (evenements, etat) = {
            let mut s = verrou(&self.sondeur);
            let ev = s.sonder();
            (ev, s.etat())
        };
        for e in evenements {
            (self.sortie)(e);
        }
        etat
    }
}

impl Drop for Surveillance {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
    }
}

/// Écritures d'un bilan MySQL de ComicRack (signalement après `update_books`).
pub(crate) fn ecrits_du_bilan(r: &WriteReport) -> Vec<(String, i64)> {
    r.written.iter().map(|w| (w.id.clone(), w.record.counter)).collect()
}

#[cfg(test)]
#[path = "tests_surveillance.rs"]
mod tests;
