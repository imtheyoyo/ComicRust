//! « Transférer la bibliothèque ComicRack » (étape 6, `docs/tauri/ipc-transfert.md`) : copie des
//! livres (avec leur lecture, leurs notes…), des listes et des dossiers de ComicRack dans la base
//! autonome de ComicRust, pour pouvoir se passer de ComicRack.
//!
//! Logique reprise de `ui/transfer_window.rs` (version egui) ; la copie elle-même est celle du
//! cœur ([`Store::import_comicrack`], idempotente : relancer ne duplique rien).
//!
//! Garanties :
//! - **ComicRack n'est que lu** : `SELECT` sur sa base MySQL, lecture de `ComicDb.xml`, jamais
//!   d'écriture ;
//! - **le mot de passe MySQL ne sort pas d'ici** : la connexion est relue dans `ComicRack.ini`
//!   (ou prise de la bibliothèque ouverte), jamais écrite ni renvoyée au front ; les messages
//!   d'erreur sont expurgés de toute occurrence du mot de passe ([`masquer`]).

use crate::biblio::{self, Biblio, EtatBiblio, SourceBiblio};
use crate::reglages::{self, Mode};
use comicrust_core::db::{ComicDatabase, DbConfig, LoadProgress};
use comicrust_core::scanner::path_key;
use comicrust_core::store::{ComicRackSource, ImportOptions, Store, StoreConfig, SQLITE_FILE_NAME};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Période d'envoi de la progression au front.
const PERIODE_PROGRESSION: Duration = Duration::from_millis(200);

// ───────────────────────────── Types de l'IPC ─────────────────────────────

/// D'où viennent les livres de ComicRack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceTransfert {
    /// Base MySQL de ComicRack (livres) ; `ComicDb.xml` sert aux listes et aux dossiers.
    Mysql,
    /// `ComicDb.xml` contenant les livres (ComicRack sans MySQL).
    Comicdb,
}

/// État de départ du transfert : ce qui est proposé, sans rien lire de lourd.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApercuTransfert {
    /// Base MySQL de ComicRack trouvée (`utilisateur@hôte:port/base`, **sans mot de passe**).
    pub mysql: Option<String>,
    /// Source proposée par défaut.
    pub source_par_defaut: SourceTransfert,
    /// `ComicDb.xml` proposé (vide : introuvable, à indiquer).
    pub comicdb: String,
    /// Le fichier proposé existe.
    pub comicdb_present: bool,
    /// Base de ComicRust qui recevra les livres.
    pub destination: String,
    /// La base de destination existe déjà.
    pub destination_existe: bool,
    /// Livres déjà dans la base de destination (si elle existe et se lit).
    pub nb_livres_destination: Option<usize>,
    /// Mode de la bibliothèque affichée.
    pub mode: Mode,
    /// Présent : le transfert ne peut pas démarrer maintenant (texte à afficher).
    pub bloque: Option<String>,
}

fn vrai() -> bool {
    true
}

/// Demande de transfert.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeTransfert {
    pub source: SourceTransfert,
    /// `ComicDb.xml` : livres (source `comicdb`) ou listes et dossiers (source `mysql`).
    /// Absent ou vide : celui de l'aperçu.
    pub comicdb: Option<String>,
    /// Reprendre les listes (défaut : oui).
    #[serde(default = "vrai")]
    pub listes: bool,
    /// Reprendre les dossiers surveillés (défaut : oui).
    #[serde(default = "vrai")]
    pub dossiers: bool,
    /// Ensuite, afficher la base de ComicRust en autonome, sans ComicRack (défaut : oui).
    #[serde(default = "vrai")]
    pub basculer: bool,
}

/// Bilan d'un transfert.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeTransfert {
    /// Fiches lues dans ComicRack.
    pub livres_source: usize,
    /// Livres absents de la base, ajoutés.
    pub ajoutes: usize,
    /// Livres déjà ajoutés par l'analyse des dossiers (même fichier), remplacés par la fiche
    /// ComicRack : un doublon évité.
    pub remplaces: usize,
    /// Livres déjà transférés (transfert relancé), mis à jour : un doublon évité.
    pub mis_a_jour: usize,
    /// Fiches écartées (sans identifiant ou identifiant en double dans ComicRack).
    pub ecartes: usize,
    /// `remplaces + misAJour` : livres qui auraient été dupliqués.
    pub doublons_evites: usize,
    /// Listes enregistrées (tous niveaux, dossiers de listes compris).
    pub listes: usize,
    /// Dossiers surveillés ajoutés.
    pub dossiers: usize,
    /// Chemins de fichiers ramenés sous un dossier surveillé équivalent.
    pub chemins_ramenes: usize,
    /// Fichiers convertis ou déplacés dans ComicRust depuis un transfert précédent, gardés.
    pub fichiers_gardes: usize,
    /// Arrêté par l'utilisateur : les lots déjà écrits sont gardés, ni listes ni dossiers.
    pub interrompu: bool,
    /// Problèmes non bloquants (tous, dans l'ordre).
    pub avertissements: Vec<String>,
    /// Livres dans la base de ComicRust après le transfert.
    pub nb_livres_destination: Option<usize>,
    /// Livres de la base de ComicRust dont le fichier est celui d'un autre livre (critère de
    /// recette : 0). Les livres sans fichier ne comptent pas.
    pub doublons_restants: Option<usize>,
    /// Phrase de bilan prête à afficher.
    pub resume: String,
    /// La bibliothèque affichée est maintenant la base autonome de ComicRust.
    pub bascule: bool,
    /// État de la bibliothèque rouverte (présent si elle a été rechargée : le front se met à jour).
    pub etat: Option<EtatBiblio>,
}

/// Événements transmis au front par `app.rs`.
#[derive(Debug, Clone, PartialEq)]
pub enum EvenementTransfert {
    Progression { phase: String, fait: usize, total: usize },
}

impl EvenementTransfert {
    /// Nom de l'événement Tauri et sa charge JSON (camelCase), selon le contrat IPC.
    pub fn pour_le_front(&self) -> (&'static str, serde_json::Value) {
        use serde_json::json;
        match self {
            EvenementTransfert::Progression { phase, fait, total } => {
                ("transfert-progression", json!({"phase": phase, "fait": fait, "total": total}))
            }
        }
    }
}

/// Sortie des événements (appelée depuis le fil de surveillance de la progression).
pub type Sortie<'a> = &'a (dyn Fn(EvenementTransfert) + Send + Sync);

// ───────────────────────────── Outils ─────────────────────────────

/// Retire de `texte` toute occurrence du mot de passe (les erreurs MySQL peuvent citer la
/// connexion). Un mot de passe vide ne masque rien.
pub fn masquer(texte: &str, cfg: &DbConfig) -> String {
    if cfg.password.is_empty() {
        texte.to_string()
    } else {
        texte.replace(&cfg.password, "***")
    }
}

fn est_xml(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("xml"))
}

fn nettoyer(s: &str) -> Option<PathBuf> {
    let t = s.trim().trim_matches('"').trim();
    (!t.is_empty()).then(|| PathBuf::from(t))
}

/// Phrase de bilan (comme `summary` de la version egui).
fn phrase(r: &ResumeTransfert) -> String {
    let mut s = format!(
        "{} livres de ComicRack : {} ajoutés, {} remplacés (déjà ajoutés par l'analyse des dossiers), {} mis à jour",
        r.livres_source, r.ajoutes, r.remplaces, r.mis_a_jour
    );
    if r.ecartes > 0 {
        s.push_str(&format!(", {} écartés", r.ecartes));
    }
    s.push_str(&format!(" ; {} listes ; {} dossiers ajoutés.", r.listes, r.dossiers));
    if r.chemins_ramenes > 0 {
        s.push_str(&format!(" {} chemins ramenés sous un dossier surveillé.", r.chemins_ramenes));
    }
    if r.fichiers_gardes > 0 {
        s.push_str(&format!(" {} fichiers convertis ou déplacés dans ComicRust gardés.", r.fichiers_gardes));
    }
    if r.interrompu {
        s.push_str(
            " Interrompu : les lots déjà écrits sont gardés, ComicRust n'a pas changé de bibliothèque. \
             Relance le transfert pour le terminer.",
        );
    }
    s
}

/// Livres dont le fichier est aussi celui d'un autre livre (clé de chemin sans casse ni sens des
/// barres) ; les livres sans fichier ne comptent pas.
fn compter_doublons(base: &ComicDatabase) -> usize {
    let mut par_chemin: HashMap<String, usize> = HashMap::new();
    for b in &base.books {
        if let Some(f) = b.file.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
            *par_chemin.entry(path_key(f)).or_default() += 1;
        }
    }
    par_chemin.values().filter(|n| **n > 1).sum()
}

// ───────────────────────────── Transfert ─────────────────────────────

/// Transfert de la bibliothèque ComicRack, partageable entre les commandes (clonage bon marché).
#[derive(Clone)]
pub struct Transfert {
    biblio: Biblio,
    annule: Arc<AtomicBool>,
}

impl Transfert {
    pub fn nouveau(biblio: Biblio) -> Self {
        Transfert { biblio, annule: Arc::default() }
    }

    /// Demande l'arrêt du transfert en cours (entre deux lots de livres).
    pub fn annuler(&self) {
        self.annule.store(true, Ordering::SeqCst);
    }

    /// Base de ComicRust qui reçoit les livres : la base autonome (`data\comicrust.db`).
    fn destination(&self) -> StoreConfig {
        self.biblio.store_autonome().unwrap_or_else(|| StoreConfig::Sqlite(self.biblio.data().join(SQLITE_FILE_NAME)))
    }

    /// `ComicRack.ini` (ou `ComicDb.xml`) du réglage, sinon recherché aux emplacements habituels.
    fn chemin_ini(&self) -> Option<PathBuf> {
        let r = reglages::lire(self.biblio.data()).ok()?;
        r.chemin_ini.filter(|p| p.is_file()).or_else(biblio::chercher_ini)
    }

    /// Connexion MySQL de ComicRack : celle de la bibliothèque liée ouverte, sinon celle de
    /// `ComicRack.ini`. **Reste dans ce module** : jamais renvoyée au front.
    fn config_mysql(&self) -> Option<DbConfig> {
        if let SourceBiblio::ComicrackMysql(c) = self.biblio.source() {
            return Some(c);
        }
        let ini = self.chemin_ini().filter(|p| !est_xml(p))?;
        DbConfig::from_comicrack_ini(&ini).ok().filter(DbConfig::is_complete)
    }

    /// `ComicDb.xml` proposé : celui du réglage (mode `ComicDb.xml`), sinon celui que désigne
    /// `ComicRack.ini`.
    fn comicdb_propose(&self) -> Option<PathBuf> {
        let ini = self.chemin_ini()?;
        if est_xml(&ini) {
            Some(ini)
        } else {
            comicrust_core::comic_lists::find_comicdb(&ini)
        }
    }

    /// Ce qui est proposé au départ (rien de lourd n'est lu).
    pub fn apercu(&self) -> ApercuTransfert {
        let mysql = self.config_mysql();
        let comicdb = self.comicdb_propose();
        let dest = self.destination();
        let existe = dest.sqlite_path().ok().flatten().is_some_and(|p| p.is_file());
        let nb = if existe { Store::open_existing(&dest).and_then(|mut s| s.count()).ok() } else { None };
        ApercuTransfert {
            source_par_defaut: if mysql.is_some() { SourceTransfert::Mysql } else { SourceTransfert::Comicdb },
            mysql: mysql.map(|c| c.describe()),
            comicdb_present: comicdb.as_ref().is_some_and(|p| p.is_file()),
            comicdb: comicdb.map(|p| p.display().to_string()).unwrap_or_default(),
            destination: dest.describe(),
            destination_existe: existe,
            nb_livres_destination: nb,
            mode: self.biblio.mode().unwrap_or(Mode::Autonome),
            bloque: self
                .biblio
                .analyse_en_cours()
                .then(|| "Analyse des dossiers en cours : attends sa fin (ou arrête-la) avant de transférer.".into()),
        }
    }

    /// Lance le transfert (bloque jusqu'à la fin ; appelé hors du fil principal) : progression
    /// par `sortie`, arrêt par [`annuler`](Self::annuler). Ensuite, si demandé (ou si la base
    /// affichée est celle qui vient de changer), la bibliothèque autonome est rouverte.
    pub fn lancer(&self, demande: &DemandeTransfert, sortie: Sortie<'_>) -> Result<ResumeTransfert, String> {
        let mysql = match demande.source {
            SourceTransfert::Mysql => Some(self.config_mysql().ok_or(
                "aucune base MySQL de ComicRack configurée : ComicRack.ini sans ligne DataSource=mysql:… \
                 (ou introuvable). Choisis « ComicDb.xml » ou indique son chemin.",
            )?),
            SourceTransfert::Comicdb => None,
        };
        let comicdb = demande.comicdb.as_deref().and_then(nettoyer).or_else(|| self.comicdb_propose());
        // Réserve la bibliothèque : ni analyse ni édition pendant le transfert.
        let garde = self.biblio.prendre_edition()?;
        let dest = self.destination();
        let affichee = self.biblio.mode() == Some(Mode::Autonome);
        self.annule.store(false, Ordering::SeqCst);
        let resultat = self.executer(mysql, comicdb, &dest, demande, sortie);
        drop(garde); // `ouvrir` refuse de recharger tant qu'une opération est réservée
        let mut r = resultat?;
        // Bascule : seulement si le transfert est complet ; une base autonome déjà affichée est
        // rechargée dans tous les cas, pour montrer les livres déjà écrits.
        let bascule = !r.interrompu && demande.basculer;
        if bascule || affichee {
            let rouvrir = || -> Result<EtatBiblio, String> {
                if bascule {
                    reglages::modifier(self.biblio.data(), |g| g.mode = Some(Mode::Autonome))?;
                }
                self.biblio.ouvrir()
            };
            match rouvrir() {
                Ok(e) => {
                    r.bascule = e.mode == Mode::Autonome && (bascule || affichee);
                    r.etat = Some(e);
                }
                Err(e) => r.avertissements.push(format!(
                    "transfert terminé, mais la bibliothèque de ComicRust n'a pas pu être rouverte : {e}"
                )),
            }
        }
        Ok(r)
    }

    /// Transfert proprement dit (lectures dans ComicRack, écritures dans `dest`), avec un fil de
    /// surveillance qui envoie la progression. Une panique est ramenée à une erreur lisible.
    pub(crate) fn executer(
        &self,
        mysql: Option<DbConfig>,
        comicdb: Option<PathBuf>,
        dest: &StoreConfig,
        demande: &DemandeTransfert,
        sortie: Sortie<'_>,
    ) -> Result<ResumeTransfert, String> {
        let progression = LoadProgress::default();
        let phase = Mutex::new(String::from("préparation…"));
        let fini = AtomicBool::new(false);
        let opts = ImportOptions { lists: demande.listes, folders: demande.dossiers };
        std::thread::scope(|s| {
            s.spawn(|| {
                while !fini.load(Ordering::Relaxed) {
                    let (fait, total) = progression.get();
                    let phase = phase.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    sortie(EvenementTransfert::Progression { phase, fait, total });
                    std::thread::sleep(PERIODE_PROGRESSION);
                }
            });
            let marquer = |t: &str| *phase.lock().unwrap_or_else(|e| e.into_inner()) = t.to_string();
            let travail = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.copier(mysql.as_ref(), comicdb.as_deref(), dest, opts, &progression, &marquer)
            }));
            fini.store(true, Ordering::Relaxed);
            travail.unwrap_or_else(|_| Err("erreur interne pendant le transfert".into()))
        })
    }

    /// Lecture de ComicRack puis écriture dans `dest` (dans le fil de l'appelant).
    fn copier(
        &self,
        mysql: Option<&DbConfig>,
        comicdb: Option<&Path>,
        dest: &StoreConfig,
        opts: ImportOptions,
        progression: &LoadProgress,
        marquer: &dyn Fn(&str),
    ) -> Result<ResumeTransfert, String> {
        let erreur = |e: String| mysql.map_or(e.clone(), |c| masquer(&e, c));
        // 1. Lecture de ComicRack (seulement des lectures).
        let mut avertissement_lecture = None;
        let xml = match comicdb {
            Some(p) => match std::fs::read_to_string(p) {
                Ok(x) => Some(x.trim_start_matches('\u{feff}').to_string()),
                // Avec MySQL, ComicDb.xml ne sert qu'aux listes et aux dossiers : les livres
                // sont transférés quand même.
                Err(e) if mysql.is_some() => {
                    avertissement_lecture = Some(format!("{} illisible ({e}) : ni listes ni dossiers", p.display()));
                    None
                }
                Err(e) => return Err(format!("{} : {e}", p.display())),
            },
            None => None,
        };
        let src = match mysql {
            Some(cfg) => {
                marquer("lecture de la base MySQL de ComicRack…");
                let base = ComicDatabase::load(cfg, Some(progression)).map_err(|e| erreur(e.to_string()))?;
                ComicRackSource { books: base.books, comicdb_xml: xml }
            }
            None => {
                if xml.is_none() {
                    return Err("indique le ComicDb.xml de ComicRack".into());
                }
                marquer("lecture de ComicDb.xml…");
                ComicRackSource::from_comicdb_xml(xml.unwrap_or_default()).map_err(|e| e.to_string())?
            }
        };
        if src.books.is_empty() {
            return Err(if mysql.is_some() {
                "la base MySQL de ComicRack ne contient aucun livre".into()
            } else {
                "ce ComicDb.xml ne contient aucun livre : ComicRack range sans doute ses livres dans MySQL. \
                 Choisis la source « Base MySQL de ComicRack » (elle est proposée quand ComicRack.ini en \
                 indique une)."
                    .into()
            });
        }
        // 2. Écriture dans la base de ComicRust (livres, puis listes et dossiers).
        marquer("écriture dans la base de ComicRust (livres, puis listes et dossiers)…");
        progression.total.store(0, Ordering::Relaxed);
        progression.done.store(0, Ordering::Relaxed);
        let mut base = Store::open(dest).map_err(|e| e.to_string())?;
        let mut rapport =
            base.import_comicrack(&src, &opts, Some(progression), Some(&self.annule)).map_err(|e| e.to_string())?;
        rapport.warnings.splice(0..0, avertissement_lecture);
        // 3. Contrôle : combien de livres, et aucun fichier en double.
        marquer("vérification de la base de ComicRust…");
        let (nb, doublons) = match base.load(None) {
            Ok(b) => (Some(b.len()), Some(compter_doublons(&b))),
            Err(e) => {
                rapport.warnings.push(format!("vérification de la base impossible : {e}"));
                (None, None)
            }
        };
        let mut r = ResumeTransfert {
            livres_source: src.books.len(),
            ajoutes: rapport.books_added,
            remplaces: rapport.books_replaced,
            mis_a_jour: rapport.books_updated,
            ecartes: rapport.books_skipped,
            doublons_evites: rapport.books_replaced + rapport.books_updated,
            listes: rapport.lists,
            dossiers: rapport.folders.len(),
            chemins_ramenes: rapport.paths_rewritten,
            fichiers_gardes: rapport.files_kept,
            interrompu: rapport.cancelled,
            avertissements: rapport.warnings,
            nb_livres_destination: nb,
            doublons_restants: doublons,
            resume: String::new(),
            bascule: false,
            etat: None,
        };
        r.resume = phrase(&r);
        Ok(r)
    }
}
