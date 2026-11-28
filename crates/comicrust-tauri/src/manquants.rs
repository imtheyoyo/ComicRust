//! Livres manquants : fiche présente dans la bibliothèque, fichier introuvable (lot L3).
//!
//! Porte la fonction de la version egui (`ui/missing_ui.rs`, `library/missing.rs`) au-dessus du
//! cœur (`comicrust_core::missing`), dupliquée ici : `crates/comicrust` n'est pas modifié.
//!
//! - **Repérage** : colonne `missing` de la base de ComicRust (écrite par l'analyse des dossiers),
//!   élément `<Missing>` de ComicRack, complétés par [`Manquants::verifier`] (vérification de
//!   tous les fichiers, par dossier) et par [`Manquants::sonder_livre`] (ouverture d'un livre).
//!   **Garde-fous du cœur** : un volume injoignable (NAS éteint, lecteur réseau absent) ne marque
//!   **rien** et est signalé ; une disparition massive est bloquée ; une erreur réseau arrête la
//!   vérification du volume. Voir `comicrust_core::missing`.
//! - **Écritures** : l'état « manquant » n'est enregistré que dans la base de ComicRust, hors
//!   ligne exclu (jamais dans ComicRack : il recalcule `<Missing>` lui-même ; en mode lié, la
//!   marque reste en mémoire). « Retrouver le fichier » et « Retirer de la bibliothèque » suivent
//!   les règles d'écriture de l'édition (lecture seule, base injoignable, configuration MySQL
//!   incomplète : refus avec un message lisible, **avant** toute écriture).
//! - **Liste « Livres manquants »** : liste virtuelle [`ID_LISTE_MANQUANTS`], résolue par
//!   [`Biblio::membres_manquants`] (appelée par `membres_liste`) : tri, recherche et groupe de la
//!   vue s'appliquent comme à n'importe quelle liste.
//!
//! Aucune dépendance à Tauri : tout est testé ici (dossiers temporaires). Les commandes sont dans
//! `cmd_manquants.rs` ; contrat : `docs/tauri/ipc-manquants.md`.

use crate::biblio::{Biblio, SourceBiblio};
use crate::edition::{Cible, AUCUNE_BIBLIOTHEQUE};
use crate::listes::Source;
use crate::surveillance::CONFIG_MYSQL_INCOMPLETE;
use comicrust_core::comic_lists::Members;
use comicrust_core::db::{self, BookEdit, ComicDatabase, DbConfig, FileState};
use comicrust_core::library::LibraryBook;
use comicrust_core::missing::{
    chosen_kind, name_key, relocation_report, same_physical_file, search_folder, volume_root, CheckItem, CheckReport,
    KnownFiles, Progress, SearchReport, Wanted,
};
use comicrust_core::scanner::{path_key, ScanReport};
use comicrust_core::store::{read_watch_folders, Store};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Identifiant de la liste virtuelle « Livres manquants » (jamais celui d'une vraie liste : les
/// identifiants des listes de ComicRack sont des GUID).
pub const ID_LISTE_MANQUANTS: &str = "@manquants";

/// Au-delà de ce nombre de livres retirés d'un coup, la case de confirmation est exigée même si
/// la vérification des fichiers est sûre (`REMOVE_CONFIRM_ABOVE` de la version egui).
pub const SEUIL_CONFIRMATION_RETRAIT: usize = 100;

/// Légendes de livres montrées par la fenêtre « Retirer de la bibliothèque ».
const LEGENDES_MAX: usize = 6;
/// Erreurs de dossier détaillées au plus dans le bilan d'une recherche.
const ERREURS_MAX: usize = 5;

/// Lecture seule : base MySQL de ComicRack sans l'option « Lecture et écriture ».
pub const LECTURE_SEULE_MYSQL: &str = "Base de ComicRack en lecture seule : ComicRust ne modifie pas ses fiches. \
     Fais-le dans ComicRack, ou active « Lecture et écriture » (Réglages ▸ Bibliothèque ComicRack).";
/// Lecture seule : `ComicDb.xml`.
pub const LECTURE_SEULE_XML: &str = "Bibliothèque ComicDb.xml de ComicRack : ses livres sont en lecture seule pour \
     ComicRust (ComicRack réécrit ce fichier). Fais-le dans ComicRack.";

// ───────────────────────────── Types de l'IPC ─────────────────────────────

/// Le livre est-il manquant ? (Comme `ComicBookIsMissingMatcher` : livre avec fichier.)
pub fn est_manquant(b: &LibraryBook) -> bool {
    b.missing == Some(true) && b.file.as_deref().is_some_and(|f| !f.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Avancement {
    pub fait: usize,
    pub total: usize,
}

/// Recherche dans un dossier en cours : `fichiers` parcourus, puis `comparaison`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RechercheEnCours {
    pub dossier: String,
    pub fichiers: usize,
    pub comparaison: bool,
}

/// Volume injoignable (ou dossier bloqué) : racine et nombre de livres non vérifiés.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSignale {
    pub racine: String,
    pub livres: usize,
}

/// Disparition massive : `disparus` livres sur `livres` auraient disparu d'un coup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocage {
    pub racine: String,
    pub livres: usize,
    pub disparus: usize,
}

/// Bilan d'une vérification des fichiers (`CheckReport` du cœur).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BilanVerification {
    /// Résumé lisible (`CheckReport::summary`).
    pub resume: String,
    pub manquants: usize,
    pub presents: usize,
    pub injoignables: Vec<VolumeSignale>,
    pub bloques: Vec<Blocage>,
    /// Livres ni présents ni manquants (volume injoignable, dossier illisible, blocage).
    pub indetermines: usize,
    pub interrompu: bool,
    /// Bilan sûr : rien d'injoignable, de bloqué ni d'indéterminé, vérification menée à son terme.
    pub concluant: bool,
    /// Marque non enregistrée dans la base (base injoignable, erreur d'écriture).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avertissement: Option<String>,
}

impl BilanVerification {
    fn depuis(r: &CheckReport) -> Self {
        Self {
            resume: r.summary(),
            manquants: r.missing.len(),
            presents: r.present.len(),
            injoignables: r
                .unreachable
                .iter()
                .map(|s| VolumeSignale { racine: s.root.clone(), livres: s.books })
                .collect(),
            bloques: r
                .blocked
                .iter()
                .map(|s| Blocage { racine: s.root.clone(), livres: s.books, disparus: s.gone })
                .collect(),
            indetermines: r.unknown.len(),
            interrompu: r.cancelled,
            concluant: r.is_conclusive(),
            avertissement: None,
        }
    }
}

/// État léger, interrogé par le front (`etat_manquants`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatManquants {
    pub nb_manquants: usize,
    /// Vérification en cours : avancement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<Avancement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recherche: Option<RechercheEnCours>,
    /// Bilan de la dernière vérification terminée de la session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derniere: Option<BilanVerification>,
    /// Pourquoi les fiches ne sont pas modifiables maintenant (lecture seule, base injoignable) ;
    /// absent : « Retrouver » et « Retirer » sont possibles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raison_lecture_seule: Option<String>,
}

/// Résultat de l'examen du fichier d'un livre avant son ouverture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SondeLivre {
    pub id: String,
    pub titre: String,
    pub fichier: String,
    /// `present` (à ouvrir), `introuvable` (volume joignable : livre marqué manquant) ou
    /// `volumeInjoignable` (rien de marqué : le fichier est peut-être toujours là).
    pub etat: EtatFichier,
    /// Volume qui ne répond pas (`volumeInjoignable`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub racine: Option<String>,
    /// Marque non enregistrée dans la base (base injoignable, erreur d'écriture).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avertissement: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EtatFichier {
    Present,
    Introuvable,
    VolumeInjoignable,
}

/// Fichier proposé pour un livre manquant (fenêtre de confirmation).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposition {
    pub id: String,
    pub legende: String,
    /// Chemin actuel de la fiche (renvoyé tel quel à `appliquer_relocalisations`).
    pub ancien: String,
    pub nouveau: String,
    /// Comment le fichier a été reconnu (`MatchKind::label`).
    pub correspondance: String,
    /// La correspondance suffit à elle seule (nom, taille).
    pub sure: bool,
    /// Autres fichiers de même nom trouvés.
    pub autres: usize,
    /// Livre de la bibliothèque qui a déjà ce fichier (doublon).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doublon: Option<String>,
    /// Coché d'office : correspondance sûre, fichier unique, pas de doublon.
    pub coche: bool,
}

/// Bilan d'une recherche dans un dossier.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatRecherche {
    pub dossier: String,
    pub propositions: Vec<Proposition>,
    pub non_retrouves: usize,
    pub fichiers_examines: usize,
    /// Dossiers illisibles (les premiers seulement).
    pub erreurs: Vec<String>,
    pub interrompu: bool,
    /// Rien à proposer : texte à afficher (« Aucun fichier retrouvé… »).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
}

/// Une fiche à pointer vers un nouveau fichier.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeRelocalisation {
    pub id: String,
    /// Chemin de la fiche tel qu'affiché par la proposition : la fiche n'est réécrite que si elle
    /// l'a encore (déplacée entre-temps : ignorée).
    pub ancien: String,
    pub nouveau: String,
}

/// Bilan d'une opération sur les fiches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BilanOperation {
    /// Fiches réellement mises à jour (ou retirées).
    pub nb: usize,
    /// Texte à afficher.
    pub info: String,
    /// Retrait incomplet : cause (les fiches de `nb` sont bien retirées, les autres non).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erreur: Option<String>,
}

/// Fenêtre « Retirer de la bibliothèque » : ce qu'elle montre avant la confirmation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationRetrait {
    /// Livres retirés (identifiants connus de la bibliothèque, sans doublon).
    pub ids: Vec<String>,
    pub legendes: Vec<String>,
    /// Livres de plus que les `legendes`.
    pub autres: usize,
    /// Vérification des fichiers : présents, volumes injoignables, disparitions massives.
    pub bilan: BilanVerification,
    /// La case « je confirme » est exigée (`remove_needs_confirmation`).
    pub confirmation_requise: bool,
    /// Bibliothèque ComicRack en écriture : avertissement « fermez ComicRack ».
    pub comicrack: bool,
}

// ───────────────────────────── Événements ─────────────────────────────

/// Événements envoyés au front.
#[derive(Debug, Clone, PartialEq)]
pub enum EvenementManquants {
    /// Des fiches ont changé (marques, relocalisation, retrait) : les vues se rafraîchissent.
    Modifiee { ids: Vec<String>, retires: usize },
}

impl EvenementManquants {
    /// Même événement que l'édition (`bibliotheque-modifiee`, la vue se rafraîchit déjà ainsi) ;
    /// `origine: "manquants"` les distingue.
    pub fn pour_le_front(&self) -> (&'static str, serde_json::Value) {
        match self {
            EvenementManquants::Modifiee { ids, retires } => {
                ("bibliotheque-modifiee", serde_json::json!({"ids": ids, "origine": "manquants", "supprimes": retires}))
            }
        }
    }
}

/// Émetteur d'événements (l'application y branche `app.emit`).
pub type Sortie = Arc<dyn Fn(EvenementManquants) + Send + Sync>;

// ───────────────────────────── Logique sans état ─────────────────────────────

/// Où écrire les fiches (relocalisation, retrait), comme `Edition::cible`.
pub fn cible_ecriture(biblio: &Biblio) -> Cible {
    match biblio.source() {
        SourceBiblio::Autonome(c) => Cible::Store(c),
        SourceBiblio::ComicrackMysql(_) => match biblio.config_comicrack_ecriture() {
            Some(c) => Cible::Comicrack(c),
            None => Cible::LectureSeule(LECTURE_SEULE_MYSQL.into()),
        },
        SourceBiblio::ComicrackXml => Cible::LectureSeule(LECTURE_SEULE_XML.into()),
        SourceBiblio::Aucune => Cible::LectureSeule(AUCUNE_BIBLIOTHEQUE.into()),
    }
}

/// Cible d'écriture des fiches, ou la raison pour laquelle on n'écrit pas : lecture seule,
/// configuration MySQL incomplète, base injoignable (surveillance). Rien n'est tenté avant.
pub fn cible_modifiable(biblio: &Biblio) -> Result<Cible, String> {
    let cible = cible_ecriture(biblio);
    match &cible {
        Cible::LectureSeule(m) => return Err(m.clone()),
        Cible::Comicrack(cfg) if !cfg.is_complete() => return Err(CONFIG_MYSQL_INCOMPLETE.to_string()),
        _ => {}
    }
    match biblio.motif_hors_ligne() {
        Some(m) => Err(m),
        None => Ok(cible),
    }
}

/// Examen du fichier d'un livre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sonde {
    /// Le fichier existe (ou ComicRack le réécrit : `.tmp` présent).
    Present,
    /// Introuvable, volume joignable : déplacé, renommé ou supprimé.
    Introuvable,
    /// Le volume ne répond pas : rien à conclure.
    Injoignable(String),
}

/// Examine `fichier` (chemin de la fiche). À appeler hors du fil de l'interface : un NAS éteint
/// peut faire attendre plusieurs secondes.
pub fn sonder_fichier(fichier: &str) -> Sonde {
    let chemin = Path::new(fichier);
    if chemin.exists() || comicrust_core::providers::is_being_rewritten(chemin) {
        return Sonde::Present;
    }
    match volume_root(fichier) {
        Some(racine) if std::fs::read_dir(&racine).is_err() => Sonde::Injoignable(racine),
        _ => Sonde::Introuvable,
    }
}

/// Livres à vérifier : ceux qui ont un fichier, et le chemin de leur fiche (le résultat ne vaut
/// que pour lui). `comme_presents` : aucun n'est compté comme déjà manquant (retrait : un retrait
/// massif sous un même dossier doit être confirmé).
fn elements_verification<'a>(
    livres: impl IntoIterator<Item = &'a LibraryBook>,
    comme_presents: bool,
) -> (Vec<CheckItem>, HashMap<String, String>) {
    let mut vus = HashMap::new();
    let items = livres
        .into_iter()
        .filter_map(|b| {
            let f = b.file.clone().filter(|f| !f.is_empty())?;
            vus.insert(b.id.to_ascii_lowercase(), f.clone());
            Some(CheckItem {
                id: b.id.clone(),
                path: PathBuf::from(f),
                was_missing: !comme_presents && b.missing == Some(true),
            })
        })
        .collect();
    (items, vus)
}

/// Fenêtre « Retirer de la bibliothèque » : la case « je confirme » est-elle exigée ? Oui si un
/// volume est injoignable, une disparition massive bloquée, des livres non vérifiés, des
/// fichiers encore présents, ou plus de [`SEUIL_CONFIRMATION_RETRAIT`] livres.
pub fn retrait_exige_confirmation(n: usize, verif: &CheckReport) -> bool {
    !verif.is_conclusive() || !verif.present.is_empty() || n > SEUIL_CONFIRMATION_RETRAIT
}

/// Retrait permis ? Jamais avant la vérification ; ensuite seulement si la confirmation exigée
/// est donnée.
pub fn retrait_permis(verif: Option<&CheckReport>, n: usize, confirme: bool) -> bool {
    verif.is_some_and(|v| confirme || !retrait_exige_confirmation(n, v))
}

/// Proposition pour un fichier choisi à la main : même fichier physique sous un autre chemin
/// (`Z:\` ↔ `\\SERVEUR\…`, lien symbolique) pour les fiches de même nom de fichier. Lit le disque.
pub fn proposition_choisie(base: &ComicDatabase, id: &str, nouveau: PathBuf, taille: u64) -> Option<Proposition> {
    let b = base.get(id)?;
    let ancien = b.file.clone().unwrap_or_default();
    let texte = nouveau.to_string_lossy();
    let (cle, nom) = (path_key(&texte), name_key(&texte));
    let doublon = base
        .books
        .iter()
        .find(|o| {
            o.id != b.id
                && o.file.as_deref().is_some_and(|f| {
                    path_key(f) == cle || (name_key(f) == nom && same_physical_file(Path::new(f), &nouveau))
                })
        })
        .map(LibraryBook::caption);
    let genre = chosen_kind(&ancien, b.file_size, &nouveau, taille);
    Some(Proposition {
        id: b.id.clone(),
        legende: b.caption(),
        ancien,
        nouveau: nouveau.to_string_lossy().into_owned(),
        correspondance: genre.label().to_string(),
        sure: genre.sure(),
        autres: 0,
        coche: genre.sure() && doublon.is_none(),
        doublon,
    })
}

/// Propositions d'après le résultat d'une recherche dans un dossier.
pub fn propositions_recherche(base: &ComicDatabase, rapport: &SearchReport) -> Vec<Proposition> {
    rapport
        .found
        .iter()
        .filter_map(|c| {
            let b = base.get(&c.id)?;
            Some(Proposition {
                id: b.id.clone(),
                legende: b.caption(),
                ancien: b.file.clone().unwrap_or_default(),
                nouveau: c.path.to_string_lossy().into_owned(),
                correspondance: c.kind.label().to_string(),
                sure: c.kind.sure(),
                autres: c.others,
                doublon: None,
                // Jamais coché quand plusieurs fichiers conviennent.
                coche: c.sure(),
            })
        })
        .collect()
}

/// Texte du dossier ou du fichier tapé ou collé : espaces et guillemets autour retirés.
fn nettoyer_chemin(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

fn pluriel(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

/// État d'un fichier sur le disque (`file_state` de la version egui), pour l'écriture ComicRack.
fn etat_fichier(chemin: &Path) -> std::io::Result<FileState> {
    let meta = std::fs::metadata(chemin)?;
    let utc = |t: std::io::Result<std::time::SystemTime>| {
        t.ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).fixed_offset())
    };
    Ok(FileState {
        path: chemin.to_string_lossy().into_owned(),
        size: meta.len(),
        modified: utc(meta.modified()),
        created: utc(meta.created()),
        page_count: None,
        pages: None,
    })
}

// ───────────────────────────── Liste virtuelle ─────────────────────────────

impl Biblio {
    /// Contenu de la liste virtuelle « Livres manquants » (version 0 : la vue est invalidée à
    /// chaque changement de marque, sa clé comprenant la révision des fiches).
    ///
    /// Gardé en cache pour l'état de la base `(génération, révision des fiches)` : une marque
    /// « manquant » posée ou retirée change la révision, donc le contenu est recalculé.
    pub(crate) fn membres_manquants(&self) -> Result<(Members, u64), String> {
        let (base, cle) = self.base_et_contenu().ok_or("aucune bibliothèque ouverte")?;
        let mut cache = self.manquants_en_cache.lock().unwrap_or_else(|e| e.into_inner());
        let ids = match cache.as_ref().filter(|(c, _)| *c == cle) {
            Some((_, ids)) => ids.clone(),
            None => {
                let ids: Arc<Vec<String>> =
                    Arc::new(base.books.iter().filter(|b| est_manquant(b)).map(|b| b.id.clone()).collect());
                *cache = Some((cle, ids.clone()));
                ids
            }
        };
        Ok((Members::Ids(ids.as_ref().clone()), 0))
    }
}

// ───────────────────────────── Service ─────────────────────────────

/// Travail long en cours : avancement lisible par le front et arrêt demandé.
struct Travail {
    progression: Arc<Progress>,
    annule: Arc<AtomicBool>,
}

impl Travail {
    fn nouveau() -> Self {
        Travail { progression: Arc::default(), annule: Arc::default() }
    }
}

#[derive(Default)]
struct Etat {
    verification: Option<Travail>,
    recherche: Option<(Travail, String)>,
    derniere: Option<BilanVerification>,
}

/// Quel emplacement libérer à la fin d'un travail (même en cas de panique du fil).
#[derive(Clone, Copy)]
enum Genre {
    Verification,
    Recherche,
}

struct Occupation {
    etat: Arc<Mutex<Etat>>,
    genre: Genre,
}

impl Drop for Occupation {
    fn drop(&mut self) {
        let mut g = self.etat.lock().unwrap_or_else(|e| e.into_inner());
        match self.genre {
            Genre::Verification => g.verification = None,
            Genre::Recherche => g.recherche = None,
        }
    }
}

/// Service des livres manquants, partageable entre les commandes (clonage bon marché).
#[derive(Clone)]
pub struct Manquants {
    biblio: Biblio,
    etat: Arc<Mutex<Etat>>,
    sortie: Sortie,
}

impl Manquants {
    pub fn nouveau(biblio: Biblio, sortie: Sortie) -> Self {
        Manquants { biblio, etat: Arc::default(), sortie }
    }

    fn verrou(&self) -> MutexGuard<'_, Etat> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn signaler(&self, ids: Vec<String>, retires: usize) {
        if !ids.is_empty() || retires > 0 {
            (self.sortie)(EvenementManquants::Modifiee { ids, retires });
        }
    }

    // ───────────── État ─────────────

    /// Pourquoi les fiches ne sont pas modifiables maintenant (`None` : modifiables).
    pub fn raison_lecture_seule(&self) -> Option<String> {
        cible_modifiable(&self.biblio).err()
    }

    /// Nombre de livres manquants de la bibliothèque ouverte.
    pub fn compter(&self) -> usize {
        self.biblio.base().map_or(0, |b| b.books.iter().filter(|l| est_manquant(l)).count())
    }

    pub fn etat(&self) -> EtatManquants {
        let (verification, recherche, derniere) = {
            let g = self.verrou();
            (
                g.verification.as_ref().map(|t| {
                    let (fait, total) = t.progression.get();
                    Avancement { fait, total }
                }),
                g.recherche.as_ref().map(|(t, dossier)| {
                    let (fait, total) = t.progression.get();
                    RechercheEnCours { dossier: dossier.clone(), fichiers: total, comparaison: fait > 0 }
                }),
                g.derniere.clone(),
            )
        };
        EtatManquants {
            nb_manquants: self.compter(),
            verification,
            recherche,
            derniere,
            raison_lecture_seule: self.raison_lecture_seule(),
        }
    }

    /// Arrêt demandé de la vérification et de la recherche en cours.
    pub fn arreter(&self) {
        let g = self.verrou();
        if let Some(t) = &g.verification {
            t.annule.store(true, Ordering::Relaxed);
        }
        if let Some((t, _)) = &g.recherche {
            t.annule.store(true, Ordering::Relaxed);
        }
    }

    fn occuper(
        &self,
        genre: Genre,
        dossier: Option<String>,
    ) -> Result<(Occupation, Arc<Progress>, Arc<AtomicBool>), String> {
        let mut g = self.verrou();
        let t = Travail::nouveau();
        let (p, a) = (t.progression.clone(), t.annule.clone());
        match genre {
            Genre::Verification => {
                if g.verification.is_some() {
                    return Err("une vérification des fichiers est déjà en cours".into());
                }
                g.verification = Some(t);
            }
            Genre::Recherche => {
                if g.recherche.is_some() {
                    return Err("une recherche de fichiers est déjà en cours".into());
                }
                g.recherche = Some((t, dossier.unwrap_or_default()));
            }
        }
        Ok((Occupation { etat: self.etat.clone(), genre }, p, a))
    }

    // ───────────── Vérification ─────────────

    /// Dossiers surveillés (garde-fou « disparition massive ») : ceux de la base de ComicRust, ou
    /// `<WatchFolders>` du `ComicDb.xml` de ComicRack. Vide si illisible.
    fn dossiers_surveilles(&self) -> Vec<PathBuf> {
        match self.biblio.source_listes() {
            Ok(Source::Store(cfg)) => Store::open_existing(&cfg).and_then(|mut s| s.folders()).unwrap_or_default(),
            Ok(Source::ComicDb(p)) => std::fs::read_to_string(p)
                .ok()
                .map(|xml| read_watch_folders(xml.trim_start_matches('\u{feff}')))
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Vérifie l'existence des fichiers de tous les livres (bloquant : à appeler dans un fil ;
    /// l'avancement se lit par [`Manquants::etat`], l'arrêt se demande par [`Manquants::arreter`]).
    ///
    /// Un volume injoignable ou une disparition massive ne marque rien et est signalé dans le
    /// bilan. Le résultat est posé en mémoire, puis enregistré dans la base de ComicRust (jamais
    /// dans celle de ComicRack) quand elle est modifiable. Refusé pendant une analyse des dossiers
    /// (elle fait déjà cette vérification et recharge la base).
    pub fn verifier(&self) -> Result<BilanVerification, String> {
        if self.biblio.analyse_en_cours() {
            return Err("analyse des dossiers en cours : la vérification des fichiers en fait partie".into());
        }
        let (items, vus) = {
            let base = self.biblio.base()?;
            elements_verification(&base.books, false)
        };
        let dossiers = self.dossiers_surveilles();
        let (_occupation, progression, annule) = self.occuper(Genre::Verification, None)?;
        let rapport = comicrust_core::missing::check_files(&items, &dossiers, &progression, &annule);
        let mut bilan = BilanVerification::depuis(&rapport);
        if !rapport.cancelled {
            bilan.avertissement = self.appliquer_resultat(&vus, &rapport.missing, &rapport.present);
        }
        self.verrou().derniere = Some(bilan.clone());
        Ok(bilan)
    }

    /// Pose le résultat d'une vérification en mémoire (seulement pour les fiches qui ont encore le
    /// chemin vérifié) et l'enregistre dans la base de ComicRust. Renvoie l'avertissement éventuel.
    fn appliquer_resultat(
        &self,
        vus: &HashMap<String, String>,
        manquants: &[String],
        presents: &[String],
    ) -> Option<String> {
        // Marqués (vrai ou faux) avant : seuls ceux-là sont à remettre à zéro dans la base.
        let retours: Vec<String> = {
            let base = self.biblio.base().ok()?;
            presents.iter().filter(|id| matches!(base.get(id).map(|b| b.missing), Some(Some(_)))).cloned().collect()
        };
        let marques: Vec<_> = manquants
            .iter()
            .map(|id| (id, true))
            .chain(presents.iter().map(|id| (id, false)))
            .filter_map(|(id, m)| Some((id.clone(), vus.get(&id.to_ascii_lowercase())?.clone(), m)))
            .collect();
        let modifies = self.biblio.marquer_manquants(&marques);
        let avertissement = self.enregistrer_marques(manquants.to_vec(), retours, vus);
        self.signaler(modifies, 0);
        avertissement
    }

    /// Enregistre `missing` dans la base de ComicRust (vrai pour `manquants`, effacé pour
    /// `retours`), pour les seules fiches qui ont encore le chemin vérifié (`vus`). Base de
    /// ComicRack : rien (ComicRack tient `<Missing>` lui-même ; la marque reste en mémoire). Base
    /// injoignable : rien d'écrit, avertissement.
    fn enregistrer_marques(
        &self,
        manquants: Vec<String>,
        retours: Vec<String>,
        vus: &HashMap<String, String>,
    ) -> Option<String> {
        let Cible::Store(cfg) = cible_ecriture(&self.biblio) else { return None };
        let connu = |ids: Vec<String>| -> Vec<String> {
            ids.into_iter().filter(|id| vus.contains_key(&id.to_ascii_lowercase())).collect()
        };
        let rapport = ScanReport { missing: connu(manquants), back: connu(retours), ..Default::default() };
        if rapport.missing.is_empty() && rapport.back.is_empty() {
            return None;
        }
        if self.biblio.motif_hors_ligne().is_some() {
            return Some(
                "Base injoignable : l'état « manquant » n'est pas enregistré dans la base (il reste affiché \
                 jusqu'au rechargement)."
                    .into(),
            );
        }
        let ecrit = Store::open_existing(&cfg).and_then(|mut st| st.apply_scan_seen(&rapport, Some(vus)));
        match ecrit {
            Ok(w) => {
                self.biblio.signaler_ecriture_autonome(w.books.iter().map(|b| b.id.clone()), Some(w.counter));
                None
            }
            Err(e) => Some(format!("État « manquant » non enregistré dans la base : {e}")),
        }
    }

    // ───────────── Ouverture d'un livre ─────────────

    /// Examine le fichier du livre `id` avant de l'ouvrir (bloquant : à appeler dans un fil, un
    /// NAS éteint fait attendre). Introuvable sur un volume joignable : le livre est marqué
    /// manquant ; volume injoignable : rien n'est marqué ; présent alors que le livre était
    /// marqué : la marque est retirée.
    pub fn sonder_livre(&self, id: &str) -> Result<SondeLivre, String> {
        let fiche = self.biblio.livre(id).ok_or_else(|| format!("livre introuvable : {id}"))?;
        let fichier = fiche.file.clone().filter(|f| !f.trim().is_empty()).ok_or("livre sans fichier")?;
        let sonde = sonder_fichier(&fichier);
        let (etat, racine, manquant) = match &sonde {
            Sonde::Present => (EtatFichier::Present, None, Some(false)),
            Sonde::Introuvable => (EtatFichier::Introuvable, None, Some(true)),
            Sonde::Injoignable(r) => (EtatFichier::VolumeInjoignable, Some(r.clone()), None),
        };
        let mut avertissement = None;
        if let Some(m) = manquant {
            // Rien à changer (présent et pas marqué, ou déjà marqué manquant) : aucune écriture.
            let deja = matches!((m, fiche.missing), (true, Some(true)) | (false, None | Some(false)));
            if !deja {
                let vus: HashMap<String, String> = [(fiche.id.to_ascii_lowercase(), fichier.clone())].into();
                let (manquants, retours) =
                    if m { (vec![fiche.id.clone()], vec![]) } else { (vec![], vec![fiche.id.clone()]) };
                let modifies = self.biblio.marquer_manquants(&[(fiche.id.clone(), fichier.clone(), m)]);
                avertissement = self.enregistrer_marques(manquants, retours, &vus);
                self.signaler(modifies, 0);
            }
        }
        Ok(SondeLivre { id: fiche.id.clone(), titre: fiche.caption(), fichier, etat, racine, avertissement })
    }

    // ───────────── Retrouver le fichier ─────────────

    /// Proposition pour un fichier choisi à la main (chemin saisi) : taille, doublon éventuel.
    /// Refusé si les fiches ne sont pas modifiables (message de lecture seule).
    pub fn proposer_fichier(&self, id: &str, chemin: &str) -> Result<Proposition, String> {
        cible_modifiable(&self.biblio)?;
        let nouveau = PathBuf::from(nettoyer_chemin(chemin));
        if nouveau.as_os_str().is_empty() {
            return Err("chemin vide".into());
        }
        let meta = std::fs::metadata(&nouveau).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("fichier introuvable : {}", nouveau.display())
            } else {
                format!("{} : {e}", nouveau.display())
            }
        })?;
        if !meta.is_file() {
            return Err(format!("ce n'est pas un fichier : {}", nouveau.display()));
        }
        let base = self.biblio.base()?;
        proposition_choisie(&base, id, nouveau, meta.len()).ok_or_else(|| format!("livre introuvable : {id}"))
    }

    /// « Chercher dans un dossier » : cherche les fichiers de `ids` dans `dossier` et ses
    /// sous-dossiers, par nom puis par taille (bloquant : à appeler dans un fil ; arrêt par
    /// [`Manquants::arreter`]). Plusieurs livres : seulement les manquants ; un seul : tel quel
    /// (volume injoignable compris). Les fichiers déjà dans la bibliothèque ne sont jamais
    /// proposés. Rien n'est écrit : le résultat est à faire confirmer.
    pub fn chercher(&self, ids: &[String], dossier: &str) -> Result<ResultatRecherche, String> {
        cible_modifiable(&self.biblio)?;
        let dossier = PathBuf::from(nettoyer_chemin(dossier));
        if dossier.as_os_str().is_empty() {
            return Err("dossier vide".into());
        }
        if !dossier.is_dir() {
            return Err(format!("dossier introuvable : {}", dossier.display()));
        }
        let base = self.biblio.base()?;
        let mut vus = HashSet::new();
        let ids: Vec<&String> = ids.iter().filter(|i| vus.insert(i.to_ascii_lowercase())).collect();
        // Aucun identifiant : tous les livres manquants de la bibliothèque.
        let candidats: Vec<&LibraryBook> = if ids.is_empty() {
            base.books.iter().filter(|b| est_manquant(b)).collect()
        } else {
            ids.iter().filter_map(|id| base.get(id)).filter(|b| ids.len() == 1 || est_manquant(b)).collect()
        };
        let voulus: Vec<Wanted> = candidats
            .into_iter()
            .filter_map(|b| Some(Wanted { id: b.id.clone(), file: b.file.clone()?, size: b.file_size }))
            .collect();
        let affiche = dossier.display().to_string();
        if voulus.is_empty() {
            return Ok(ResultatRecherche {
                dossier: affiche,
                propositions: Vec::new(),
                non_retrouves: 0,
                fichiers_examines: 0,
                erreurs: Vec::new(),
                interrompu: false,
                info: Some("Aucun livre manquant à rechercher.".into()),
            });
        }
        let connus = KnownFiles::new(base.books.iter().filter_map(|b| b.file.as_deref()));
        let (_occupation, progression, annule) = self.occuper(Genre::Recherche, Some(affiche.clone()))?;
        let rapport = search_folder(&dossier, &voulus, &connus, &progression, &annule);
        let propositions = if rapport.cancelled { Vec::new() } else { propositions_recherche(&base, &rapport) };
        let n = rapport.files_seen;
        let info = (propositions.is_empty() && !rapport.cancelled).then(|| {
            format!("Aucun fichier retrouvé dans {affiche} ({n} fichier{} examiné{}).", pluriel(n), pluriel(n))
        });
        Ok(ResultatRecherche {
            dossier: affiche,
            non_retrouves: rapport.not_found.len(),
            fichiers_examines: n,
            erreurs: rapport.errors.iter().take(ERREURS_MAX).map(|(d, e)| format!("{} : {e}", d.display())).collect(),
            interrompu: rapport.cancelled,
            propositions,
            info,
        })
    }

    /// Met à jour les fiches : chemin, taille et dates du nouveau fichier ; lecture, note et infos
    /// gardées, `missing` effacé. Base de ComicRust (`apply_scan_seen`, colonnes ciblées) ou base
    /// MySQL de ComicRack en écriture ; lecture seule, base injoignable : refus avant toute
    /// écriture. Une fiche qui n'a plus le chemin affiché est ignorée.
    pub fn appliquer_relocalisations(&self, demandes: &[DemandeRelocalisation]) -> Result<BilanOperation, String> {
        let cible = cible_modifiable(&self.biblio)?;
        let _garde = self.biblio.prendre_edition()?;
        let mut livres: Vec<(LibraryBook, PathBuf)> = Vec::new();
        let mut ignorees = 0usize;
        // Deux fiches d'un même lot ne peuvent pas viser le même fichier (cases cochées à la main).
        let mut cibles_prises: std::collections::HashSet<String> = std::collections::HashSet::new();
        for d in demandes {
            let nouveau = PathBuf::from(nettoyer_chemin(&d.nouveau));
            if !cibles_prises.insert(nouveau.to_string_lossy().to_lowercase()) {
                ignorees += 1;
                continue;
            }
            match self.biblio.livre(&d.id) {
                Some(b) if b.file.as_deref() == Some(d.ancien.as_str()) => livres.push((b, nouveau)),
                _ => ignorees += 1,
            }
        }
        let maj: Vec<LibraryBook> = match &cible {
            Cible::Store(cfg) => {
                let rapport = relocation_report(&livres).map_err(|e| e.to_string())?;
                let vus: HashMap<String, String> =
                    livres.iter().filter_map(|(b, _)| Some((b.id.to_ascii_lowercase(), b.file.clone()?))).collect();
                let mut st = Store::open_existing(cfg).map_err(|e| format!("base ComicRust inaccessible : {e}"))?;
                let w = st.apply_scan_seen(&rapport, Some(&vus)).map_err(|e| e.to_string())?;
                self.biblio.signaler_ecriture_autonome(w.books.iter().map(|b| b.id.clone()), Some(w.counter));
                w.books
            }
            Cible::Comicrack(cfg) => {
                let (ecrites, erreur) = self.relocaliser_comicrack(cfg, &livres)?;
                if let Some(e) = erreur {
                    // Écriture partielle : les fiches réellement écrites sont reportées et signalées
                    // avant de rendre l'erreur (déjà signalées à la surveillance, elles ne
                    // reviendraient jamais d'elles-mêmes).
                    let ids = self.biblio.remplacer_fiches(&ecrites, true);
                    self.signaler(ids, 0);
                    return Err(e);
                }
                ecrites
            }
            Cible::LectureSeule(m) => return Err(m.clone()),
        };
        // Couvertures refaites : le fichier a changé.
        let ids = self.biblio.remplacer_fiches(&maj, true);
        self.signaler(ids.clone(), 0);
        let n = ids.len();
        let mut info = format!("{n} fiche{} mise{} à jour.", pluriel(n), pluriel(n));
        if ignorees > 0 {
            info.push_str(&format!(
                " {ignorees} ignorée{} (fiche modifiée entre-temps ou fichier visé deux fois).",
                pluriel(ignorees)
            ));
        }
        Ok(BilanOperation { nb: n, info, erreur: None })
    }

    /// Fiches écrites dans la base MySQL de ComicRack et, si une partie a échoué, l'erreur (les
    /// fiches écrites le sont quand même : voir [`fiches_et_erreur`]).
    fn relocaliser_comicrack(
        &self,
        cfg: &DbConfig,
        livres: &[(LibraryBook, PathBuf)],
    ) -> Result<(Vec<LibraryBook>, Option<String>), String> {
        let mut editions = Vec::new();
        for (b, chemin) in livres {
            let f = etat_fichier(chemin).map_err(|e| format!("{} : {e}", chemin.display()))?;
            editions.push(BookEdit::new(&b.id).file(&f).remove("Missing"));
        }
        let rapport = db::update_books(cfg, &editions)
            .map_err(|e| format!("base ComicRack inaccessible, fiches non modifiées : {e}"))?;
        self.biblio.signaler_rapport_comicrack(&rapport);
        Ok(fiches_et_erreur(&rapport))
    }

    // ───────────── Retirer de la bibliothèque ─────────────

    /// Ce que montre la fenêtre « Retirer de la bibliothèque » : légendes, et vérification des
    /// fichiers (bloquant : un NAS éteint fait attendre). Refusé si les fiches ne sont pas
    /// modifiables. Tous les livres sont comptés comme présents avant (un retrait massif sous un
    /// même dossier doit être confirmé).
    pub fn preparer_retrait(&self, ids: &[String]) -> Result<PreparationRetrait, String> {
        let cible = cible_modifiable(&self.biblio)?;
        let (livres, rapport) = self.verifier_retrait(ids)?;
        let n = livres.len();
        let legendes: Vec<String> = livres.iter().take(LEGENDES_MAX).map(LibraryBook::caption).collect();
        Ok(PreparationRetrait {
            ids: livres.iter().map(|b| b.id.clone()).collect(),
            autres: n - legendes.len(),
            legendes,
            confirmation_requise: retrait_exige_confirmation(n, &rapport),
            bilan: BilanVerification::depuis(&rapport),
            comicrack: matches!(cible, Cible::Comicrack(_)),
        })
    }

    /// Livres à retirer (identifiants connus, sans doublon) et vérification de leurs fichiers.
    fn verifier_retrait(&self, ids: &[String]) -> Result<(Vec<LibraryBook>, CheckReport), String> {
        let base = self.biblio.base()?;
        let mut vus = HashSet::new();
        let livres: Vec<LibraryBook> =
            ids.iter().filter_map(|i| base.get(i)).filter(|b| vus.insert(b.id.to_ascii_lowercase())).cloned().collect();
        if livres.is_empty() {
            return Err("aucun livre connu dans la sélection".into());
        }
        let (items, _) = elements_verification(&livres, true);
        let dossiers = self.dossiers_surveilles();
        let rapport =
            comicrust_core::missing::check_files(&items, &dossiers, &Progress::default(), &AtomicBool::new(false));
        Ok((livres, rapport))
    }

    /// Retire les fiches de la bibliothèque (**jamais les fichiers**). La vérification des fichiers
    /// est refaite ici : sans `confirme`, le retrait est refusé quand la confirmation est exigée
    /// (volume injoignable, fichiers encore présents, plus de [`SEUIL_CONFIRMATION_RETRAIT`]
    /// livres…). Base de ComicRust : `Store::delete` ; base MySQL de ComicRack en écriture :
    /// `delete_books`. Les suppressions sont signalées à la surveillance. Les listes de lecture ne
    /// sont pas réécrites (le livre n'y apparaît plus : identifiant absent de la bibliothèque).
    pub fn retirer(&self, ids: &[String], confirme: bool) -> Result<BilanOperation, String> {
        let cible = cible_modifiable(&self.biblio)?;
        let (livres, rapport) = self.verifier_retrait(ids)?;
        let n = livres.len();
        if !retrait_permis(Some(&rapport), n, confirme) {
            return Err(format!(
                "confirmation requise : {} Coche la confirmation de la fenêtre pour retirer ces livres.",
                BilanVerification::depuis(&rapport).resume
            ));
        }
        let _garde = self.biblio.prendre_edition()?;
        let a_retirer: Vec<String> = livres.iter().map(|b| b.id.clone()).collect();
        let retrait = self.biblio.connexion().supprimant(a_retirer.clone(), || supprimer(&cible, &a_retirer))?;
        // Suppression partielle comprise : les fiches réellement supprimées quittent l'affichage.
        let tous: Vec<String> = retrait.supprimes.iter().chain(&retrait.absents).cloned().collect();
        let retires = self.biblio.retirer_fiches_memoire(&tous);
        self.signaler(tous, retires);
        let k = retrait.supprimes.len();
        let mut info =
            format!("{k} livre{} retiré{} de la bibliothèque (fichiers non touchés).", pluriel(k), pluriel(k));
        if let Some(e) = &retrait.erreur {
            info.push_str(&format!(" Retrait incomplet : {e}"));
        }
        Ok(BilanOperation { nb: k, info, erreur: retrait.erreur })
    }
}

/// Bilan d'une écriture MySQL de ComicRack : fiches écrites (toujours rendues, même si d'autres ont
/// échoué) et première erreur éventuelle.
pub(crate) fn fiches_et_erreur(rapport: &db::WriteReport) -> (Vec<LibraryBook>, Option<String>) {
    let ecrites = rapport.written.iter().map(|w| w.book.clone()).collect();
    (ecrites, rapport.failed.first().map(|(id, m)| format!("{id} : {m}")))
}

/// Fiches supprimées d'une base.
struct Retrait {
    supprimes: Vec<String>,
    /// Déjà absentes de la base.
    absents: Vec<String>,
    erreur: Option<String>,
}

fn supprimer(cible: &Cible, ids: &[String]) -> Result<Retrait, String> {
    match cible {
        Cible::Store(cfg) => {
            let mut st = Store::open_existing(cfg).map_err(|e| format!("base ComicRust inaccessible : {e}"))?;
            let presents: HashSet<String> =
                st.get(ids).map_err(|e| e.to_string())?.into_iter().map(|b| b.id.to_ascii_lowercase()).collect();
            st.delete(ids).map_err(|e| e.to_string())?;
            let (supprimes, absents) = ids.iter().cloned().partition(|i| presents.contains(&i.to_ascii_lowercase()));
            Ok(Retrait { supprimes, absents, erreur: None })
        }
        Cible::Comicrack(cfg) => {
            let r = db::delete_books(cfg, ids)
                .map_err(|e| format!("base ComicRack inaccessible, fiches non retirées : {e}"))?;
            let erreur = r.failed.first().map(|(id, e)| {
                format!("{} fiche(s) supprimée(s), {} en erreur (ex. {id} : {e})", r.deleted.len(), r.failed.len())
            });
            Ok(Retrait { supprimes: r.deleted, absents: r.missing, erreur })
        }
        Cible::LectureSeule(m) => Err(m.clone()),
    }
}

#[cfg(test)]
#[path = "tests_manquants.rs"]
mod tests;
