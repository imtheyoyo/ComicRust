//! Listes ComicRack (étape 5, contrat `docs/tauri/ipc-listes.md`) : arbre des listes, comptes,
//! création / renommage / suppression / déplacement, livres des listes de lecture (séries entières
//! comprises), état de ComicRack (lancé ou fermé) et lecture seule.
//!
//! Logique **dupliquée** de la version egui (`library/lists_ops.rs`, `ui/lists_ui.rs`,
//! `library/mouse.rs`, `ui/library_panels.rs` ; décision du plan : on ne touche pas à
//! `crates/comicrust`) ; les modifications elles-mêmes sont faites par `comicrust-core`
//! (`lists_edit::ListsEditor`) et enregistrées :
//!
//! - dans la base de ComicRust (`Store::save_list_rows` : refus si un autre poste a modifié les
//!   listes entre-temps) en mode autonome ;
//! - dans le `ComicDb.xml` de ComicRack (`ComicDbLists::save_checked` : refus si ComicRack est
//!   lancé — vérifié avant et juste avant le remplacement —, ou si le fichier a changé depuis sa
//!   lecture ; sauvegarde d'origine + 10 dernières ; remplacement atomique) en mode lié, avec
//!   l'option « Lecture et écriture » (`ecritureComicrack`).
//!
//! Report de correctif : une correction de ces fichiers sur `main` se reporte ici à la main.
//! Éditeur de listes intelligentes : [`crate::listes_criteres`] ; `.cbl` : [`crate::listes_cbl`].

use crate::biblio::{Biblio, RequeteLivres, SensTri, SourceBiblio};
use crate::reglages;
use crate::tri::ClefTri;
use comicrust_core::comic_lists::{find_comicdb, load_comic_lists, resolve_all, Combine, ComicList, ListKind, Members};
use comicrust_core::db::ComicDatabase;
use comicrust_core::lists_edit::{
    comicdb::COMICRACK_RUNNING, ComicDbLists, ListRow, ListsEditor, NewList, COMICRACK_UNKNOWN,
};
use comicrust_core::store::{Store, StoreConfig};
use comicrust_core::Error;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Listes de ComicRack non modifiables faute de l'option d'écriture.
pub const LECTURE_SEULE: &str = "Lecture seule : active « Lecture et écriture » dans les réglages de l'édition pour \
                                 modifier les listes de ComicRack (ComicRack fermé).";
/// Réordonnancement refusé : vue pas triée dans l'ordre de la liste.
pub const TRIER_POUR_REORDONNER: &str = "Trie la vue dans l'ordre de la liste (« # » croissant) pour réordonner";
/// Une opération est déjà en cours.
pub const DEJA_EN_COURS: &str = "un enregistrement des listes est déjà en cours";
/// Intervalle minimal entre deux recherches du processus ComicRack (`etat_listes`).
const INTERVALLE_SONDE: Duration = Duration::from_secs(3);

// ───────────────────────────── Types de l'IPC ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeListe {
    Bibliotheque,
    Dossier,
    Lecture,
    Intelligente,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Combinaison {
    Ou,
    Et,
    Vide,
}

impl Combinaison {
    pub fn depuis(c: Combine) -> Self {
        match c {
            Combine::Or => Combinaison::Ou,
            Combine::And => Combinaison::Et,
            Combine::Empty => Combinaison::Vide,
        }
    }

    pub fn vers(self) -> Combine {
        match self {
            Combinaison::Ou => Combine::Or,
            Combinaison::Et => Combine::And,
            Combinaison::Vide => Combine::Empty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceListes {
    Comicrust,
    Comicdb,
    Aucune,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatListes {
    pub source: SourceListes,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chemin: Option<String>,
    pub modifiable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lecture_seule: Option<String>,
    pub comicrack_lance: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erreur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub en_cours: Option<String>,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoeudListe {
    pub id: String,
    pub nom: String,
    #[serde(rename = "type")]
    pub type_liste: TypeListe,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nb_livres: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inconnu: Option<String>,
    pub nb_comicrack: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combinaison: Option<Combinaison>,
    pub accepte_livres: bool,
    pub peut_monter: bool,
    pub peut_descendre: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enfants: Option<Vec<NoeudListe>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbreListes {
    pub etat: EtatListes,
    pub listes: Vec<NoeudListe>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatListes {
    pub arbre: ArbreListes,
    /// `Some(None)` : sélectionner toute la bibliothèque (`null`) ; `None` : champ absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renommer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creee: Option<String>,
    pub info: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sauvegarde: Option<String>,
    pub avertissements: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeNouvelle {
    Lecture,
    Dossier,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeCreation {
    #[serde(rename = "type")]
    pub type_liste: TypeNouvelle,
    #[serde(default)]
    pub nom: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub position: Option<usize>,
    #[serde(default)]
    pub livres: Option<Vec<String>>,
    #[serde(default)]
    pub apres_liste: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfosSuppression {
    pub nom: String,
    pub sous_listes: usize,
    pub dependantes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfosConversion {
    pub nom: String,
    pub nb_livres: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub id: Option<String>,
    pub nom: String,
    pub profondeur: usize,
    pub actuel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaceDepot {
    Avant,
    Dans,
    Apres,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupeVue {
    pub titre: String,
    pub nombre: usize,
}

// ───────────────────────────── État ─────────────────────────────

/// Détection de ComicRack (`Err` : impossible de savoir, ce qui compte comme lancé).
pub type Detecteur = Arc<dyn Fn() -> Result<bool, String> + Send + Sync>;

/// Où sont les listes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Source {
    /// Base de ComicRust (mode autonome).
    Store(StoreConfig),
    /// `ComicDb.xml` de ComicRack (mode lié).
    ComicDb(PathBuf),
}

/// Date et taille d'un fichier (relecture quand il change).
type Empreinte = (Option<SystemTime>, u64);

/// Contenu de chaque liste, par identifiant de liste.
pub(crate) type MembresListes = Arc<HashMap<String, Members>>;

/// Vue groupée d'une liste de lecture : livres de la liste, livres de chaque groupe affiché,
/// titres des groupes.
type VueGroupee = (Vec<String>, Vec<Vec<String>>, Vec<String>);

/// Contenu calculé de toutes les listes, pour un état de la base, des listes et un jour donnés.
struct Resolu {
    cle: (u64, u64, u64, chrono::NaiveDate),
    membres: Arc<HashMap<String, Members>>,
}

/// Listes lues, contenu calculé, opération en cours (dans [`Biblio`]).
#[derive(Default)]
pub struct CacheListes {
    source: Option<Source>,
    items: Arc<Vec<ComicList>>,
    erreur: Option<String>,
    version: u64,
    empreinte: Option<Empreinte>,
    resolu: Option<Resolu>,
    en_cours: Option<String>,
    sonde: Option<(Instant, Result<bool, String>)>,
    detecteur: Option<Detecteur>,
}

/// Remet `en_cours` à vide quand elle disparaît (erreur comprise).
struct GardeOperation(Arc<std::sync::Mutex<CacheListes>>);

impl Drop for GardeOperation {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).en_cours = None;
    }
}

/// Ce que l'opération demande au front après un enregistrement réussi.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Issue {
    pub selection: Option<Option<String>>,
    pub renommer: Option<String>,
    pub creee: Option<String>,
    pub info: Option<String>,
}

impl Issue {
    pub fn info(msg: impl Into<String>) -> Self {
        Issue { info: Some(msg.into()), ..Default::default() }
    }
}

/// Message d'erreur pour l'utilisateur (refus du cœur montrés tels quels, comme egui).
pub(crate) fn texte_erreur(e: &Error) -> String {
    match e {
        Error::Conflict(m) | Error::Database(m) => m.clone(),
        autre => autre.to_string(),
    }
}

/// Raison de refuser la modification des listes de ComicRack d'après la détection.
pub fn refus_comicrack(detecte: &Result<bool, String>) -> Option<String> {
    match detecte {
        Ok(false) => None,
        Ok(true) => Some(COMICRACK_RUNNING.to_string()),
        Err(e) => Some(format!("{COMICRACK_UNKNOWN} ({e}) : par prudence, ses listes ne sont pas modifiables.")),
    }
}

/// Détection par défaut : `COMICRUST_SIMULER_COMICRACK` (recette hors Windows) ou le processus
/// `ComicRack.exe`.
fn detecter_comicrack() -> Result<bool, String> {
    if std::env::var_os("COMICRUST_SIMULER_COMICRACK").is_some() {
        return Ok(true);
    }
    comicrust_core::lists_edit::comicrack_running()
}

fn empreinte(p: &Path) -> Option<Empreinte> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok(), m.len()))
}

fn pluriel(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

fn type_de(l: &ComicList) -> TypeListe {
    match l.kind {
        ListKind::Library => TypeListe::Bibliotheque,
        ListKind::Folder { .. } => TypeListe::Dossier,
        ListKind::Ids(_) => TypeListe::Lecture,
        ListKind::Smart { .. } => TypeListe::Intelligente,
    }
}

/// Nombre de livres présents dans la base (`None` : contenu inconnu).
fn compte(m: &Members, db: &ComicDatabase) -> Option<usize> {
    match m {
        Members::All => Some(db.len()),
        Members::Ids(ids) => Some(ids.iter().filter(|i| db.index_of(i).is_some()).count()),
        Members::Unknown(_) => None,
    }
}

// ───────────────────────────── Lecture des listes ─────────────────────────────

impl Biblio {
    fn cache(&self) -> std::sync::MutexGuard<'_, CacheListes> {
        self.listes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Remplace la détection de ComicRack (tests).
    #[cfg(test)]
    pub(crate) fn simuler_comicrack(&self, d: Detecteur) {
        let mut c = self.cache();
        c.detecteur = Some(d);
        c.sonde = None;
    }

    fn detecteur(&self) -> Detecteur {
        self.cache().detecteur.clone().unwrap_or_else(|| Arc::new(detecter_comicrack))
    }

    /// ComicRack lancé ? Résultat gardé [`INTERVALLE_SONDE`] sauf si `fraiche`.
    fn sonde(&self, fraiche: bool) -> Result<bool, String> {
        if !fraiche {
            if let Some((t, r)) = &self.cache().sonde {
                if t.elapsed() < INTERVALLE_SONDE {
                    return r.clone();
                }
            }
        }
        let r = (self.detecteur())();
        self.cache().sonde = Some((Instant::now(), r.clone()));
        r
    }

    /// Où sont les listes de la bibliothèque ouverte.
    pub(crate) fn source_listes(&self) -> Result<Source, String> {
        match self.source() {
            SourceBiblio::Autonome(cfg) => Ok(Source::Store(cfg)),
            SourceBiblio::ComicrackXml => {
                let r = reglages::lire(self.data())?;
                r.chemin_ini.map(Source::ComicDb).ok_or_else(|| "ComicDb.xml non indiqué".to_string())
            }
            SourceBiblio::ComicrackMysql(_) => {
                let r = reglages::lire(self.data())?;
                let ini = r.chemin_ini.ok_or("ComicRack.ini non indiqué")?;
                find_comicdb(&ini).map(Source::ComicDb).ok_or_else(|| {
                    "ComicDb.xml de ComicRack introuvable (à côté de ComicRack.ini, ou dans \
                     %APPDATA%\\cYo\\ComicRack Community Edition) : listes indisponibles"
                        .to_string()
                })
            }
            SourceBiblio::Aucune => Err("aucune bibliothèque ouverte".into()),
        }
    }

    /// Option « Lecture et écriture » du mode lié.
    fn ecriture_comicrack(&self) -> bool {
        reglages::lire(self.data()).ok().and_then(|r| r.edition.ecriture_comicrack).unwrap_or(false)
    }

    /// Relit les listes si la source a changé, si `ComicDb.xml` a changé (date, taille), ou
    /// (`store_aussi`) depuis la base de ComicRust. Lecture hors du verrou.
    pub(crate) fn rafraichir_listes(&self, store_aussi: bool) {
        let voulue = self.source_listes();
        let a_lire: Option<(Source, Option<Empreinte>)> = {
            let mut c = self.cache();
            match &voulue {
                Err(e) => {
                    if c.source.is_some() || c.erreur.as_deref() != Some(e) {
                        c.source = None;
                        c.items = Arc::default();
                        c.erreur = Some(e.clone());
                        c.empreinte = None;
                        c.version += 1;
                    }
                    None
                }
                Ok(src) => {
                    let changee = c.source.as_ref() != Some(src);
                    match src {
                        Source::ComicDb(p) => {
                            let e = empreinte(p);
                            (changee || e != c.empreinte || e.is_none()).then(|| (src.clone(), e))
                        }
                        Source::Store(_) => (changee || store_aussi).then(|| (src.clone(), None)),
                    }
                }
            }
        };
        let Some((src, e)) = a_lire else { return };
        let lu: Result<Vec<ComicList>, String> = match &src {
            Source::ComicDb(p) => {
                load_comic_lists(p).map_err(|err| format!("{} : {}", p.display(), texte_erreur(&err)))
            }
            Source::Store(cfg) => {
                Store::open_existing(cfg).and_then(|mut s| s.comic_lists()).map_err(|e| texte_erreur(&e))
            }
        };
        let mut c = self.cache();
        let meme = c.source.as_ref() == Some(&src);
        match lu {
            Ok(items) => {
                if !meme || c.erreur.is_some() || *c.items != items {
                    c.items = Arc::new(items);
                    c.version += 1;
                }
                c.erreur = None;
            }
            Err(err) => {
                if !meme || c.erreur.as_deref() != Some(&err) {
                    c.items = Arc::default();
                    c.version += 1;
                }
                c.erreur = Some(err);
            }
        }
        c.source = Some(src);
        c.empreinte = e;
    }

    /// Listes affichées et leur version.
    pub(crate) fn listes_lues(&self) -> (Arc<Vec<ComicList>>, u64) {
        let c = self.cache();
        (c.items.clone(), c.version)
    }

    /// Contenu calculé de toutes les listes (en cache tant que la base, les listes et le jour
    /// ne changent pas).
    pub(crate) fn membres_toutes(&self) -> Result<(MembresListes, Arc<ComicDatabase>), String> {
        let (base, (generation, contenu)) = self.base_et_contenu().ok_or("aucune bibliothèque ouverte")?;
        let (items, version) = self.listes_lues();
        let cle = (generation, contenu, version, chrono::Local::now().date_naive());
        if let Some(r) = self.cache().resolu.as_ref().filter(|r| r.cle == cle) {
            return Ok((r.membres.clone(), base));
        }
        let membres = Arc::new(resolve_all(&items, &base));
        let mut c = self.cache();
        if c.version == version {
            c.resolu = Some(Resolu { cle, membres: membres.clone() });
        }
        Ok((membres, base))
    }

    /// Livres d'une liste (dans l'ordre de la liste) et version des listes.
    pub(crate) fn membres_liste(&self, id: &str) -> Result<(Members, u64), String> {
        self.rafraichir_listes(false);
        let (items, version) = self.listes_lues();
        let l = ComicList::find(&items, id).ok_or_else(|| format!("liste introuvable : {id}"))?;
        let (membres, base) = self.membres_toutes()?;
        let m = membres.get(&l.id).cloned().unwrap_or_else(|| l.resolve(&base, &items));
        Ok((m, version))
    }

    /// Pourquoi les listes ne sont pas modifiables maintenant (`None` : modifiables).
    fn raison_lecture_seule(&self, fraiche: bool) -> (Option<String>, bool) {
        let src = match self.source_listes() {
            Ok(s) => s,
            Err(e) => return (Some(e), false),
        };
        let (erreur, en_cours) = {
            let c = self.cache();
            (c.erreur.clone(), c.en_cours.clone())
        };
        match src {
            Source::Store(_) => {
                if let Some(e) = erreur {
                    return (Some(format!("Listes illisibles : {e}")), false);
                }
                (en_cours.map(|_| "Enregistrement en cours…".to_string()), false)
            }
            Source::ComicDb(_) => {
                let detecte = self.sonde(fraiche);
                let lance = !matches!(detecte, Ok(false));
                if !self.ecriture_comicrack() {
                    return (Some(LECTURE_SEULE.into()), lance);
                }
                if let Some(r) = refus_comicrack(&detecte) {
                    return (Some(r), lance);
                }
                if let Some(e) = erreur {
                    return (Some(format!("Listes de ComicRack illisibles : {e}")), lance);
                }
                (en_cours.map(|_| "Enregistrement en cours…".to_string()), lance)
            }
        }
    }

    // ───────────── Commandes : arbre ─────────────

    /// État des listes (léger : à appeler régulièrement).
    pub fn etat_listes(&self) -> EtatListes {
        self.rafraichir_listes(false);
        self.etat_courant()
    }

    fn etat_courant(&self) -> EtatListes {
        let (raison, lance) = self.raison_lecture_seule(false);
        let src = self.source_listes();
        let c = self.cache();
        let (source, chemin) = match &src {
            Ok(Source::Store(_)) => (SourceListes::Comicrust, None),
            Ok(Source::ComicDb(p)) => (SourceListes::Comicdb, Some(p.display().to_string())),
            Err(_) => (SourceListes::Aucune, None),
        };
        EtatListes {
            source,
            chemin,
            modifiable: raison.is_none(),
            lecture_seule: raison,
            comicrack_lance: lance,
            erreur: c.erreur.clone(),
            en_cours: c.en_cours.clone(),
            version: c.version,
        }
    }

    /// Arbre des listes avec leurs comptes.
    pub fn arbre_listes(&self) -> Result<ArbreListes, String> {
        self.rafraichir_listes(true);
        self.arbre_courant()
    }

    fn arbre_courant(&self) -> Result<ArbreListes, String> {
        let etat = self.etat_courant();
        let (items, _) = self.listes_lues();
        let (membres, base) = match self.membres_toutes() {
            Ok(x) => (Some(x.0), Some(x.1)),
            Err(_) => (None, None),
        };
        fn noeuds(
            l: &[ComicList],
            membres: Option<&HashMap<String, Members>>,
            base: Option<&ComicDatabase>,
            modifiable: bool,
        ) -> Vec<NoeudListe> {
            let n = l.len();
            l.iter()
                .enumerate()
                .map(|(i, x)| {
                    let m = membres.and_then(|m| m.get(&x.id));
                    let (nb_livres, inconnu) = match (m, base) {
                        (Some(Members::Unknown(why)), _) => (None, Some(why.clone())),
                        (Some(m), Some(db)) => (compte(m, db), None),
                        _ => (None, Some("bibliothèque non ouverte".to_string())),
                    };
                    let (combinaison, enfants) = match &x.kind {
                        ListKind::Folder { combine, children } => {
                            (Some(Combinaison::depuis(*combine)), Some(noeuds(children, membres, base, modifiable)))
                        }
                        _ => (None, None),
                    };
                    NoeudListe {
                        id: x.id.clone(),
                        nom: x.name.clone(),
                        type_liste: type_de(x),
                        description: Some(x.description.clone()).filter(|d| !d.trim().is_empty()),
                        nb_livres,
                        inconnu,
                        nb_comicrack: x.comicrack_count,
                        combinaison,
                        accepte_livres: modifiable && matches!(x.kind, ListKind::Ids(_)),
                        peut_monter: i > 0,
                        peut_descendre: i + 1 < n,
                        enfants,
                    }
                })
                .collect()
        }
        let listes = noeuds(&items, membres.as_deref(), base.as_deref(), etat.modifiable);
        Ok(ArbreListes { etat, listes })
    }

    /// Bilan texte : pour chaque liste, livres comptés par ComicRust et par ComicRack.
    pub fn bilan_listes(&self) -> Result<String, String> {
        let arbre = self.arbre_listes()?;
        fn icone(t: TypeListe) -> &'static str {
            match t {
                TypeListe::Bibliotheque => "📚",
                TypeListe::Dossier => "📁",
                TypeListe::Lecture => "📄",
                TypeListe::Intelligente => "🔍",
            }
        }
        fn parcourir(l: &[NoeudListe], prof: usize, out: &mut String) {
            for x in l {
                let nous = x.nb_livres.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
                let diff = if nous == x.nb_comicrack.to_string() { "" } else { "  ≠" };
                out.push_str(&format!(
                    "{}{} {} — ComicRust {nous} / ComicRack {}{diff}\n",
                    "  ".repeat(prof),
                    icone(x.type_liste),
                    x.nom,
                    x.nb_comicrack
                ));
                if let Some(e) = &x.enfants {
                    parcourir(e, prof + 1, out);
                }
            }
        }
        let mut out = format!(
            "Bilan des listes ComicRust (Tauri) v{} — {}\n(ComicRack : nombre enregistré dans ComicDb.xml à sa \
             dernière sauvegarde)\n\n",
            env!("CARGO_PKG_VERSION"),
            arbre.etat.chemin.clone().unwrap_or_else(|| "base de ComicRust".into())
        );
        parcourir(&arbre.listes, 0, &mut out);
        Ok(out)
    }

    // ───────────── Opérations ─────────────

    /// Lit, modifie et enregistre les listes (voir le module) ; une opération à la fois.
    pub(crate) fn executer(
        &self,
        quoi: impl Into<String>,
        op: impl FnOnce(&mut ListsEditor) -> Result<Issue, Error>,
    ) -> Result<ResultatListes, String> {
        let quoi = quoi.into();
        self.rafraichir_listes(false);
        let src = self.source_listes()?;
        if let Source::ComicDb(_) = src {
            if !self.ecriture_comicrack() {
                return Err(LECTURE_SEULE.into());
            }
        }
        let garde = {
            let mut c = self.cache();
            if c.en_cours.is_some() {
                return Err(DEJA_EN_COURS.into());
            }
            c.en_cours = Some(quoi.clone());
            GardeOperation(self.listes.clone())
        };
        let detecteur = self.detecteur();
        let (lists, issue, sauvegarde, avertissements) = match &src {
            Source::Store(cfg) => {
                let r = (|| -> Result<_, Error> {
                    let mut store = Store::open_existing(cfg)?;
                    let snap = store.list_rows()?;
                    let mut ed = ListsEditor::new(snap.rows.clone());
                    let issue = op(&mut ed)?;
                    let lists = ed.lists()?;
                    controler_identifiants(&snap.rows, ed.rows(), &lists)?;
                    // Rien n'a changé (livres déjà présents…) : rien n'est écrit.
                    if ed.rows() != snap.rows.as_slice() {
                        store.save_list_rows(ed.rows(), snap.counter)?;
                    }
                    Ok((lists, issue))
                })()
                .map_err(|e| texte_erreur(&e))?;
                (r.0, r.1, None, Vec::new())
            }
            Source::ComicDb(p) => {
                let r = (|| -> Result<_, Error> {
                    comicrust_core::lists_edit::ensure_comicrack_closed(&*detecteur)?;
                    let fichier = ComicDbLists::read(p)?;
                    let mut ed = ListsEditor::new(fichier.rows.clone());
                    let issue = op(&mut ed)?;
                    let lists = ed.lists()?;
                    controler_identifiants(&fichier.rows, ed.rows(), &lists)?;
                    // Rien n'a changé : ComicDb.xml n'est pas réécrit (ni sauvegardé).
                    if ed.rows() == fichier.rows.as_slice() {
                        return Ok((lists, issue, None));
                    }
                    let d = detecteur.clone();
                    let rapport = fichier.save_checked(ed.rows(), move || d())?;
                    Ok((lists, issue, Some(rapport)))
                })()
                .map_err(|e| texte_erreur(&e))?;
                let nom = r.2.as_ref().and_then(|x| x.backup.file_name()).map(|n| n.to_string_lossy().into_owned());
                (r.0, r.1, nom, r.2.map(|x| x.warnings).unwrap_or_default())
            }
        };
        {
            let mut c = self.cache();
            c.items = Arc::new(lists);
            c.version += 1;
            c.erreur = None;
            if let Source::ComicDb(p) = &src {
                c.empreinte = empreinte(p);
            }
            c.source = Some(src);
        }
        drop(garde);
        let arbre = self.arbre_courant()?;
        Ok(ResultatListes {
            arbre,
            selection: issue.selection,
            renommer: issue.renommer,
            creee: issue.creee,
            info: issue.info.unwrap_or_else(|| format!("✔ {quoi}")),
            sauvegarde,
            avertissements,
        })
    }

    /// Réponse sans enregistrement (rien à faire).
    fn sans_ecriture(&self) -> Result<ResultatListes, String> {
        Ok(ResultatListes {
            arbre: self.arbre_courant()?,
            selection: None,
            renommer: None,
            creee: None,
            info: String::new(),
            sauvegarde: None,
            avertissements: Vec::new(),
        })
    }

    /// Listes à jour (relues si besoin) pour préparer une opération.
    fn items_frais(&self) -> Arc<Vec<ComicList>> {
        self.rafraichir_listes(false);
        self.listes_lues().0
    }

    fn nom_de(&self, items: &[ComicList], id: &str) -> Result<String, String> {
        ComicList::find(items, id).map(|l| l.name.clone()).ok_or_else(|| format!("liste introuvable : {id}"))
    }

    /// Emplacement d'une nouvelle liste : `parent` + `position`, ou juste après `apres_liste`.
    pub(crate) fn emplacement(
        &self,
        parent: Option<String>,
        position: Option<usize>,
        apres_liste: Option<&str>,
    ) -> (Option<String>, Option<usize>) {
        if let Some(a) = apres_liste.filter(|a| !a.trim().is_empty()) {
            if let Some((p, i)) = locate(&self.items_frais(), a) {
                return (p, Some(i + 1));
            }
        }
        (parent.filter(|p| !p.trim().is_empty()), position)
    }

    // ───────────── Commandes : gestion de l'arbre ─────────────

    pub fn creer_liste(&self, d: &DemandeCreation) -> Result<ResultatListes, String> {
        let (parent, position) = self.emplacement(d.parent.clone(), d.position, d.apres_liste.as_deref());
        let livres = d.livres.clone().unwrap_or_default();
        let nombre = livres.len();
        let (defaut, nouvelle) = match d.type_liste {
            TypeNouvelle::Lecture => ("Nouvelle liste", NewList::Reading { books: livres }),
            TypeNouvelle::Dossier => ("Nouveau dossier", NewList::Folder { combine: Combine::Or }),
        };
        let nom = d.nom.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| defaut.to_string());
        let avec_livres = d.livres.is_some();
        let nom_info = nom.trim().to_string();
        self.executer(format!("Création de « {nom_info} »"), move |ed| {
            let id = ed.create(parent.as_deref(), position, &nom, nouvelle)?;
            let info =
                avec_livres.then(|| format!("Liste « {nom_info} » créée avec {nombre} livre{}", pluriel(nombre)));
            Ok(Issue {
                selection: Some(Some(id.clone())),
                renommer: (!avec_livres).then(|| id.clone()),
                creee: Some(id),
                info,
            })
        })
    }

    pub fn renommer_liste(&self, id: &str, nom: &str) -> Result<ResultatListes, String> {
        let (id, nom) = (id.to_string(), nom.to_string());
        self.executer(format!("Renommage en « {} »", nom.trim()), move |ed| {
            ed.rename(&id, &nom)?;
            Ok(Issue::default())
        })
    }

    pub fn modifier_liste(
        &self,
        id: &str,
        nom: Option<String>,
        description: Option<String>,
        combinaison: Option<Combinaison>,
    ) -> Result<ResultatListes, String> {
        let items = self.items_frais();
        let l = ComicList::find(&items, id).ok_or_else(|| format!("liste introuvable : {id}"))?;
        let nom = nom.filter(|n| n.trim() != l.name);
        let description = description.filter(|d| *d != l.description);
        let ancien = match &l.kind {
            ListKind::Folder { combine, .. } => Some(Combinaison::depuis(*combine)),
            _ => None,
        };
        let combinaison = combinaison.filter(|c| Some(*c) != ancien);
        if nom.is_none() && description.is_none() && combinaison.is_none() {
            return self.sans_ecriture();
        }
        let quoi = format!("Modification de « {} »", nom.as_deref().unwrap_or(&l.name).trim());
        let id = l.id.clone();
        self.executer(quoi, move |ed| {
            if let Some(n) = &nom {
                ed.rename(&id, n)?;
            }
            if let Some(d) = &description {
                ed.set_description(&id, d)?;
            }
            if let Some(c) = combinaison {
                ed.set_combine(&id, c.vers())?;
            }
            Ok(Issue::default())
        })
    }

    pub fn dupliquer_liste(&self, id: &str) -> Result<ResultatListes, String> {
        let nom = self.nom_de(&self.items_frais(), id)?;
        let id = id.to_string();
        self.executer(format!("Copie de « {nom} »"), move |ed| {
            let n = ed.copy(&id)?;
            Ok(Issue { selection: Some(Some(n.clone())), creee: Some(n), ..Default::default() })
        })
    }

    pub fn infos_suppression(&self, id: &str) -> Result<InfosSuppression, String> {
        let items = self.items_frais();
        let nom = self.nom_de(&items, id)?;
        Ok(InfosSuppression {
            nom,
            sous_listes: subtree(&items, id).len().saturating_sub(1),
            dependantes: base_dependents(&items, id),
        })
    }

    pub fn supprimer_liste(&self, id: &str, affichee: Option<&str>) -> Result<ResultatListes, String> {
        let nom = self.nom_de(&self.items_frais(), id)?;
        let id = id.to_string();
        let affichee = affichee.map(str::to_ascii_lowercase);
        self.executer(format!("Suppression de « {nom} »"), move |ed| {
            let r = ed.delete(&id)?;
            let partie = affichee.as_deref().is_some_and(|s| r.removed.iter().any(|x| x == s));
            let mut info = format!("« {nom} » supprimée");
            if r.removed.len() > 1 {
                info.push_str(&format!(" (avec {} sous-listes)", r.removed.len() - 1));
            }
            if !r.broken_base.is_empty() {
                info.push_str(&format!(
                    " ; partent maintenant de toute la bibliothèque : {}",
                    r.broken_base.iter().map(|(_, n)| format!("« {n} »")).collect::<Vec<_>>().join(", ")
                ));
            }
            Ok(Issue { selection: partie.then_some(None), info: Some(info), ..Default::default() })
        })
    }

    pub fn destinations_liste(&self, id: &str) -> Result<Vec<Destination>, String> {
        let items = self.items_frais();
        let (parent, _) = locate(&items, id).ok_or_else(|| format!("liste introuvable : {id}"))?;
        let mut v =
            vec![Destination { id: None, nom: "(premier niveau)".into(), profondeur: 0, actuel: parent.is_none() }];
        for (d, nom, profondeur) in move_targets(&items, id) {
            let actuel = parent.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(&d));
            v.push(Destination { id: Some(d), nom, profondeur, actuel });
        }
        Ok(v)
    }

    pub fn deplacer_liste(
        &self,
        id: &str,
        parent: Option<String>,
        position: Option<usize>,
    ) -> Result<ResultatListes, String> {
        let nom = self.nom_de(&self.items_frais(), id)?;
        let id = id.to_string();
        let parent = parent.filter(|p| !p.trim().is_empty());
        self.executer(format!("Déplacement de « {nom} »"), move |ed| {
            ed.move_to(&id, parent.as_deref(), position)?;
            Ok(Issue::default())
        })
    }

    pub fn decaler_liste(&self, id: &str, haut: bool) -> Result<ResultatListes, String> {
        let items = self.items_frais();
        let nom = self.nom_de(&items, id)?;
        let Some((parent, to)) = step_position(&items, id, haut) else { return self.sans_ecriture() };
        let id = id.to_string();
        self.executer(format!("Déplacement de « {nom} »"), move |ed| {
            ed.move_to(&id, parent.as_deref(), Some(to))?;
            Ok(Issue::default())
        })
    }

    pub fn place_depot_liste(&self, glissee: &str, cible: &str, place: PlaceDepot) -> bool {
        !glissee.eq_ignore_ascii_case(cible) && drop_position(&self.items_frais(), glissee, cible, place).is_some()
    }

    pub fn deposer_liste(&self, glissee: &str, cible: &str, place: PlaceDepot) -> Result<ResultatListes, String> {
        let items = self.items_frais();
        let nom = self.nom_de(&items, glissee)?;
        self.nom_de(&items, cible)?;
        let (parent, position) = drop_position(&items, glissee, cible, place).ok_or_else(|| {
            format!(
                "« {nom} » ne peut pas être déposée là (sur elle-même, dans sa propre branche, ou dans une liste \
                 qui n'est pas un dossier)"
            )
        })?;
        let id = glissee.to_string();
        self.executer(format!("Déplacement de « {nom} »"), move |ed| {
            ed.move_to(&id, parent.as_deref(), position)?;
            Ok(Issue::default())
        })
    }

    /// Livres actuels d'une liste intelligente (pour la convertir).
    fn livres_pour_conversion(&self, id: &str) -> Result<(String, Vec<String>), String> {
        let items = self.items_frais();
        let l = ComicList::find(&items, id).ok_or_else(|| format!("liste introuvable : {id}"))?;
        if !matches!(l.kind, ListKind::Smart { .. }) {
            return Err(format!("« {} » n'est pas une liste intelligente", l.name));
        }
        let (m, _) = self.membres_liste(id)?;
        let livres = match m {
            Members::Ids(ids) => {
                let base = self.base()?;
                ids.into_iter().filter(|i| base.index_of(i).is_some()).collect()
            }
            Members::All => self.base()?.books.iter().map(|b| b.id.clone()).collect(),
            Members::Unknown(why) => {
                return Err(format!(
                    "« {} » ne peut pas être convertie : ComicRust ne connaît pas son contenu ({why}).",
                    l.name
                ))
            }
        };
        Ok((l.name.clone(), livres))
    }

    pub fn infos_conversion(&self, id: &str) -> Result<InfosConversion, String> {
        let (nom, livres) = self.livres_pour_conversion(id)?;
        Ok(InfosConversion { nom, nb_livres: livres.len() })
    }

    pub fn convertir_en_lecture(&self, id: &str) -> Result<ResultatListes, String> {
        let (nom, livres) = self.livres_pour_conversion(id)?;
        let id = id.to_string();
        self.executer(format!("Conversion de « {nom} »"), move |ed| {
            let n = livres.len();
            ed.convert_to_reading_list(&id, livres)?;
            Ok(Issue::info(format!("« {nom} » est maintenant une liste de lecture ({n} livre{})", pluriel(n))))
        })
    }

    // ───────────── Commandes : livres des listes de lecture ─────────────

    pub fn ajouter_livres_liste(
        &self,
        id: &str,
        livres: Vec<String>,
        position: Option<usize>,
    ) -> Result<ResultatListes, String> {
        if livres.is_empty() {
            return self.sans_ecriture();
        }
        let nom = self.nom_de(&self.items_frais(), id)?;
        let n = livres.len();
        let id = id.to_string();
        self.executer(format!("Ajout à « {nom} »"), move |ed| {
            let ajoutes = ed.add_books(&id, &livres, position)?;
            let mut info = format!("{ajoutes} livre{} ajouté{} à « {nom} »", pluriel(ajoutes), pluriel(ajoutes));
            if ajoutes < n {
                info.push_str(&format!(" ({} déjà dans la liste)", n - ajoutes));
            }
            Ok(Issue::info(info))
        })
    }

    /// Dépôt de livres sur une liste du panneau : `None` = accepté, sinon la raison.
    pub fn depot_livres_possible(&self, id: &str, affichee: Option<&str>) -> Result<Option<String>, String> {
        let items = self.items_frais();
        let l = ComicList::find(&items, id).ok_or_else(|| format!("liste introuvable : {id}"))?;
        let (raison, _) = self.raison_lecture_seule(false);
        Ok(books_drop_on_list(l, raison.as_deref(), affichee).err())
    }

    pub fn retirer_livres_liste(&self, id: &str, livres: Vec<String>) -> Result<ResultatListes, String> {
        if livres.is_empty() {
            return self.sans_ecriture();
        }
        let nom = self.nom_de(&self.items_frais(), id)?;
        let id = id.to_string();
        self.executer(format!("Retrait de « {nom} »"), move |ed| {
            let n = ed.remove_books(&id, &livres)?;
            Ok(Issue::info(format!("{n} livre{} retiré{} de « {nom} »", pluriel(n), pluriel(n))))
        })
    }

    pub fn decaler_livres_liste(&self, id: &str, livres: Vec<String>, haut: bool) -> Result<ResultatListes, String> {
        let items = self.items_frais();
        let nom = self.nom_de(&items, id)?;
        let actuels = livres_de_lecture(&items, id)?;
        if shift_books(&actuels, &livres, haut).is_none() {
            return self.sans_ecriture();
        }
        let id = id.to_string();
        self.executer(format!("Réordonnancement de « {nom} »"), move |ed| {
            let actuels = ed.books(&id)?;
            if let Some(to) = shift_books(&actuels, &livres, haut) {
                ed.move_books(&id, &livres, to)?;
            }
            Ok(Issue::default())
        })
    }

    /// Contrôle commun du réordonnancement dans la vue : liste de lecture affichée, triée dans
    /// l'ordre de la liste croissant.
    fn controler_vue(&self, id: &str, requete: &RequeteLivres) -> Result<Vec<String>, String> {
        let items = self.items_frais();
        let actuels = livres_de_lecture(&items, id)?;
        if !requete.liste.as_deref().is_some_and(|l| l.trim().eq_ignore_ascii_case(id.trim())) {
            return Err("la vue n'affiche pas cette liste".into());
        }
        if ClefTri::depuis_texte(&requete.tri)? != ClefTri::Ordre || requete.sens_tri != SensTri::Asc {
            return Err(TRIER_POUR_REORDONNER.into());
        }
        Ok(actuels)
    }

    /// Réordonne la liste d'après un repère (enregistré seulement s'il change quelque chose).
    fn reordonner(
        &self,
        id: &str,
        actuels: &[String],
        livres: Vec<String>,
        repere: Repere,
    ) -> Result<ResultatListes, String> {
        if reorder_target(actuels, &livres, &repere).is_none() {
            return self.sans_ecriture();
        }
        let nom = self.nom_de(&self.items_frais(), id)?;
        let id = id.to_string();
        self.executer(format!("Réordonnancement de « {nom} »"), move |ed| {
            let actuels = ed.books(&id)?;
            if let Some(to) = reorder_target(&actuels, &livres, &repere) {
                ed.move_books(&id, &livres, to)?;
            }
            Ok(Issue::default())
        })
    }

    pub fn deplacer_livres_liste(
        &self,
        id: &str,
        livres: Vec<String>,
        requete: &RequeteLivres,
        insertion: usize,
    ) -> Result<ResultatListes, String> {
        let actuels = self.controler_vue(id, requete)?;
        let (base, vue) = self.vue(requete)?;
        let ids: Vec<String> = vue.indices.iter().map(|&i| base.books[i].id.clone()).collect();
        let Some(repere) = reorder_anchor(&ids, &livres, insertion) else { return self.sans_ecriture() };
        self.reordonner(id, &actuels, livres, repere)
    }

    /// Groupes de la vue (titre, livres), dans l'ordre affiché.
    fn groupes_de(&self, requete: &RequeteLivres) -> Result<Vec<(String, Vec<String>)>, String> {
        let groupe = crate::biblio::regroupement(requete.groupe);
        let (base, vue) = self.vue(requete)?;
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for &i in &vue.indices {
            let b = &base.books[i];
            let titre = groupe.titre(b);
            match out.last_mut() {
                Some((t, v)) if *t == titre => v.push(b.id.clone()),
                _ => out.push((titre, vec![b.id.clone()])),
            }
        }
        Ok(out)
    }

    pub fn groupes_vue(&self, requete: &RequeteLivres) -> Result<Vec<GroupeVue>, String> {
        Ok(self.groupes_de(requete)?.into_iter().map(|(titre, v)| GroupeVue { titre, nombre: v.len() }).collect())
    }

    fn controler_groupes(&self, id: &str, requete: &RequeteLivres) -> Result<VueGroupee, String> {
        let actuels = self.controler_vue(id, requete)?;
        if crate::biblio::regroupement(requete.groupe) == crate::tri::Regroupement::Aucun {
            return Err("vue non groupée : groupe la vue par série pour déplacer des séries entières".into());
        }
        let g = self.groupes_de(requete)?;
        let titres = g.iter().map(|(t, _)| t.clone()).collect();
        Ok((actuels, g.into_iter().map(|(_, v)| v).collect(), titres))
    }

    pub fn deplacer_groupes_liste(
        &self,
        id: &str,
        requete: &RequeteLivres,
        groupes: &[String],
        insertion: usize,
    ) -> Result<ResultatListes, String> {
        let (actuels, livres_groupes, titres) = self.controler_groupes(id, requete)?;
        let mut livres = Vec::new();
        for (t, v) in titres.iter().zip(&livres_groupes) {
            if groupes.iter().any(|g| g == t) {
                livres.extend(v.iter().cloned());
            }
        }
        if livres.is_empty() {
            return Err("aucun des groupes demandés n'est affiché".into());
        }
        let Some(repere) = group_drop_anchor(&livres_groupes, &livres, insertion) else { return self.sans_ecriture() };
        self.reordonner(id, &actuels, livres, repere)
    }

    pub fn decaler_groupe_liste(
        &self,
        id: &str,
        requete: &RequeteLivres,
        groupe: &str,
        haut: bool,
    ) -> Result<ResultatListes, String> {
        let (actuels, livres_groupes, titres) = self.controler_groupes(id, requete)?;
        let k = titres.iter().position(|t| t == groupe).ok_or_else(|| format!("groupe « {groupe} » non affiché"))?;
        let Some(repere) = group_shift_anchor(&livres_groupes, k, haut) else { return self.sans_ecriture() };
        self.reordonner(id, &actuels, livres_groupes[k].clone(), repere)
    }
}

/// GUID au format écrit par ComicRack (`8-4-4-4-12` chiffres hexadécimaux).
pub fn est_guid(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Contrôle final avant écriture : dans les listes créées ou modifiées par l'opération, chaque
/// identifiant (liste, livres d'une liste de lecture, livres retirés à la main d'une liste
/// intelligente) est un GUID. `XmlSerializer` lit ces valeurs comme des `Guid` : une seule valeur
/// invalide et ComicRack ne pourrait plus charger `ComicDb.xml` (ni ses listes transférées depuis
/// la base de ComicRust). Les listes non touchées ne sont pas contrôlées (écrites par ComicRack).
pub(crate) fn controler_identifiants(avant: &[ListRow], apres: &[ListRow], listes: &[ComicList]) -> Result<(), Error> {
    let inchangees: HashSet<(&str, &str)> = avant.iter().map(|r| (r.id.as_str(), r.xml.as_str())).collect();
    let touchees: HashSet<String> = apres
        .iter()
        .filter(|r| !inchangees.contains(&(r.id.as_str(), r.xml.as_str())))
        .map(|r| r.id.trim().to_ascii_lowercase())
        .collect();
    for (l, _) in flatten(listes) {
        if !touchees.contains(&l.id.trim().to_ascii_lowercase()) {
            continue;
        }
        let refus = |quoi: &str, v: &str| {
            Error::Database(format!(
                "listes non enregistrées : {quoi} de « {} » invalide (« {} », GUID attendu) ; ComicRack ne pourrait \
                 plus lire ses listes",
                l.name,
                v.chars().take(60).collect::<String>()
            ))
        };
        if !est_guid(&l.id) {
            return Err(refus("identifiant", &l.id));
        }
        let livres: &[String] = match &l.kind {
            ListKind::Ids(v) => v,
            ListKind::Smart { query: Some(q), .. } => &q.filtered,
            _ => &[],
        };
        if let Some(v) = livres.iter().find(|v| !est_guid(v)) {
            return Err(refus("identifiant de livre", v));
        }
    }
    Ok(())
}

/// Livres d'une liste de lecture (identifiants, dans l'ordre).
fn livres_de_lecture(items: &[ComicList], id: &str) -> Result<Vec<String>, String> {
    match ComicList::find(items, id).map(|l| (&l.kind, &l.name)) {
        Some((ListKind::Ids(ids), _)) => Ok(ids.clone()),
        Some((_, nom)) => Err(format!("« {nom} » n'est pas une liste de lecture")),
        None => Err(format!("liste introuvable : {id}")),
    }
}

// ───────────────────────────── Calculs sur l'arbre (dupliqués de lists_ops.rs) ─────────────────────────────

/// Toutes les listes de l'arbre, à plat (parcours en profondeur), avec leur profondeur.
pub fn flatten(items: &[ComicList]) -> Vec<(&ComicList, usize)> {
    fn walk<'a>(l: &'a [ComicList], depth: usize, out: &mut Vec<(&'a ComicList, usize)>) {
        for x in l {
            out.push((x, depth));
            if let ListKind::Folder { children, .. } = &x.kind {
                walk(children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(items, 0, &mut out);
    out
}

/// Identifiants de la liste et de toutes ses sous-listes (en minuscules).
pub fn subtree(items: &[ComicList], id: &str) -> Vec<String> {
    match ComicList::find(items, id) {
        Some(l) => flatten(std::slice::from_ref(l)).into_iter().map(|(x, _)| x.id.to_ascii_lowercase()).collect(),
        None => Vec::new(),
    }
}

/// Dossier parent (`None` : premier niveau) et rang de la liste parmi ses voisines.
pub fn locate(items: &[ComicList], id: &str) -> Option<(Option<String>, usize)> {
    fn walk(l: &[ComicList], parent: Option<&str>, id: &str) -> Option<(Option<String>, usize)> {
        for (i, x) in l.iter().enumerate() {
            if x.id.eq_ignore_ascii_case(id) {
                return Some((parent.map(str::to_string), i));
            }
            if let ListKind::Folder { children, .. } = &x.kind {
                if let Some(r) = walk(children, Some(&x.id), id) {
                    return Some(r);
                }
            }
        }
        None
    }
    walk(items, None, id.trim())
}

/// Listes d'un dossier (`None` : premier niveau).
pub fn siblings<'a>(items: &'a [ComicList], parent: Option<&str>) -> &'a [ComicList] {
    match parent {
        None => items,
        Some(p) => match ComicList::find(items, p).map(|l| &l.kind) {
            Some(ListKind::Folder { children, .. }) => children,
            _ => &[],
        },
    }
}

fn base_of(l: &ComicList) -> Option<String> {
    match &l.kind {
        ListKind::Smart { query: Some(q), .. } => {
            q.base_list.as_deref().map(|b| b.trim().to_ascii_lowercase()).filter(|b| !b.is_empty())
        }
        _ => None,
    }
}

/// Le calcul de la liste `a` a-t-il besoin de la liste `b` ? (même règle que
/// `ListsEditor::depends_on`, sur l'arbre affiché).
pub fn depends_on(items: &[ComicList], a: &str, b: &str) -> bool {
    let b = b.to_ascii_lowercase();
    let mut todo = vec![a.to_ascii_lowercase()];
    let mut seen = HashSet::new();
    while let Some(cur) = todo.pop() {
        if cur == b {
            return true;
        }
        if !seen.insert(cur.clone()) {
            continue;
        }
        let Some(l) = ComicList::find(items, &cur) else { continue };
        if let ListKind::Folder { children, .. } = &l.kind {
            todo.extend(children.iter().map(|c| c.id.to_ascii_lowercase()));
        }
        if let Some(base) = base_of(l) {
            todo.push(base);
        }
    }
    false
}

/// Listes intelligentes (noms) qui partent de la liste `id` ou d'une de ses sous-listes, hors de
/// cette branche.
pub fn base_dependents(items: &[ComicList], id: &str) -> Vec<String> {
    let branch: HashSet<String> = subtree(items, id).into_iter().collect();
    flatten(items)
        .into_iter()
        .filter(|(l, _)| !branch.contains(&l.id.to_ascii_lowercase()))
        .filter(|(l, _)| base_of(l).is_some_and(|b| branch.contains(&b)))
        .map(|(l, _)| l.name.clone())
        .collect()
}

/// Dossiers où la liste `id` peut être déplacée (pas elle-même ni ses sous-dossiers).
pub fn move_targets(items: &[ComicList], id: &str) -> Vec<(String, String, usize)> {
    let branch: HashSet<String> = subtree(items, id).into_iter().collect();
    flatten(items)
        .into_iter()
        .filter(|(l, _)| matches!(l.kind, ListKind::Folder { .. }))
        .filter(|(l, _)| !branch.contains(&l.id.to_ascii_lowercase()))
        .map(|(l, d)| (l.id.clone(), l.name.clone(), d))
        .collect()
}

/// Dossier et rang pour `ListsEditor::move_to` quand `dragged` est déposée sur `target`.
pub fn drop_position(
    items: &[ComicList],
    dragged: &str,
    target: &str,
    place: PlaceDepot,
) -> Option<(Option<String>, Option<usize>)> {
    if subtree(items, dragged).contains(&target.to_ascii_lowercase()) {
        return None;
    }
    if place == PlaceDepot::Dans {
        let t = ComicList::find(items, target)?;
        return matches!(t.kind, ListKind::Folder { .. }).then(|| (Some(t.id.clone()), None));
    }
    let (parent, _) = locate(items, target)?;
    let others: Vec<&ComicList> =
        siblings(items, parent.as_deref()).iter().filter(|l| !l.id.eq_ignore_ascii_case(dragged)).collect();
    let at = others.iter().position(|l| l.id.eq_ignore_ascii_case(target))?;
    Some((parent, Some(if place == PlaceDepot::Apres { at + 1 } else { at })))
}

/// Rang cible pour monter (`up`) ou descendre une liste d'un cran parmi ses voisines.
pub fn step_position(items: &[ComicList], id: &str, up: bool) -> Option<(Option<String>, usize)> {
    let (parent, i) = locate(items, id)?;
    let n = siblings(items, parent.as_deref()).len();
    match up {
        true if i > 0 => Some((parent, i - 1)),
        false if i + 1 < n => Some((parent, i + 1)),
        _ => None,
    }
}

/// Livres d'une liste de lecture déplacés d'un cran : position `to` pour `move_books`.
pub fn shift_books(list: &[String], moving: &[String], up: bool) -> Option<usize> {
    let moving: HashSet<String> = moving.iter().map(|b| b.trim().to_ascii_lowercase()).collect();
    let pos: Vec<usize> =
        list.iter().enumerate().filter(|(_, b)| moving.contains(&b.to_ascii_lowercase())).map(|(i, _)| i).collect();
    let (&first, &last) = (pos.first()?, pos.last()?);
    if up {
        (first > 0).then(|| first - 1)
    } else {
        (last + 1 < list.len()).then(|| last + 2 - pos.len())
    }
}

/// Où insérer des livres déplacés (`mouse.rs` de la version egui).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repere {
    /// Juste avant ce livre.
    Avant(String),
    /// Juste après ce livre (dépôt en fin de vue).
    Apres(String),
    /// En fin de liste.
    Fin,
}

fn ensemble(ids: &[String]) -> HashSet<String> {
    ids.iter().map(|b| b.trim().to_ascii_lowercase()).collect()
}

/// Dépôt au rang `ins` de la vue : premier livre non déplacé à partir de `ins`, sinon dernier
/// livre non déplacé avant lui.
pub fn reorder_anchor(view: &[String], moving: &[String], ins: usize) -> Option<Repere> {
    let moving = ensemble(moving);
    let libre = |b: &&String| !moving.contains(&b.to_ascii_lowercase());
    let ins = ins.min(view.len());
    if let Some(b) = view[ins..].iter().find(libre) {
        return Some(Repere::Avant(b.clone()));
    }
    view[..ins].iter().rev().find(libre).map(|b| Repere::Apres(b.clone()))
}

/// Position `to` à passer à `move_books`, ou `None` si le dépôt ne change rien.
pub fn reorder_target(list: &[String], moving: &[String], anchor: &Repere) -> Option<usize> {
    let set = ensemble(moving);
    let deplace = |b: &String| set.contains(&b.to_ascii_lowercase());
    let (moved, rest): (Vec<&String>, Vec<&String>) = list.iter().partition(|b| deplace(b));
    if moved.is_empty() {
        return None;
    }
    let find = |id: &str| rest.iter().position(|b| b.eq_ignore_ascii_case(id));
    let to = match anchor {
        Repere::Avant(id) => find(id)?,
        Repere::Apres(id) => find(id)? + 1,
        Repere::Fin => rest.len(),
    };
    let mut new = rest.clone();
    new.splice(to..to, moved);
    let inchange = new.iter().zip(list).all(|(a, b)| a.eq_ignore_ascii_case(b));
    (!inchange).then_some(to)
}

/// Séries déplacées en bloc (v0.10.2) : dépôt à la frontière `ins` entre groupes.
pub fn group_drop_anchor(groups: &[Vec<String>], moving: &[String], ins: usize) -> Option<Repere> {
    let flat: Vec<String> = groups.concat();
    let rank = groups[..ins.min(groups.len())].iter().map(Vec::len).sum();
    match reorder_anchor(&flat, moving, rank)? {
        Repere::Apres(_) => Some(Repere::Fin),
        a => Some(a),
    }
}

/// « Monter / Descendre la série » : repère pour déplacer le groupe `k` d'un cran.
pub fn group_shift_anchor(groups: &[Vec<String>], k: usize, up: bool) -> Option<Repere> {
    let moving = groups.get(k)?;
    let ins = if up {
        k.checked_sub(1)?
    } else if k + 1 < groups.len() {
        k + 2
    } else {
        return None;
    };
    group_drop_anchor(groups, moving, ins)
}

/// Livres déposés sur une liste du panneau : acceptés ? Sinon la raison.
pub fn books_drop_on_list(list: &ComicList, reason: Option<&str>, shown: Option<&str>) -> Result<(), String> {
    match list.kind {
        ListKind::Library => return Err("La bibliothèque contient déjà tous les livres".into()),
        ListKind::Folder { .. } => return Err("Dossier : dépose les livres sur une liste de lecture".into()),
        ListKind::Smart { .. } => return Err("Liste intelligente : son contenu dépend de ses critères".into()),
        ListKind::Ids(_) => {}
    }
    if shown.is_some_and(|s| s.eq_ignore_ascii_case(&list.id)) {
        return Err("Les livres sont déjà dans cette liste".into());
    }
    match reason {
        Some(r) => Err(r.to_string()),
        None => Ok(()),
    }
}
