//! Édition : fenêtre « Infos du livre » (un livre / plusieurs), écriture du `ComicInfo.xml` dans
//! les CBZ, conversion CBR / CB7 / CBT → CBZ, renommage et déplacement d'après un modèle de nom,
//! garde-fou des opérations en masse. Contrat : `docs/tauri/ipc-edition.md`.
//!
//! Logique reprise de la version egui, **dupliquée** (le plan interdit de toucher à
//! `crates/comicrust`) :
//! - `library/info_form.rs` : valeurs affichées, valeurs multiples, validation, suggestions,
//!   onglets ([`ONGLETS`]) ;
//! - `ui/info_editor.rs` + `scripts/apply.rs` : enregistrement (fiches d'abord, puis infos dans
//!   les fichiers), cible d'écriture selon la bibliothèque ([`Cible`]), aperçu des renommages ;
//! - `library/files.rs` : « Écrire les infos dans le fichier » et « Convertir en CBZ » ;
//! - `library/comicrack_write.rs` : report dans la base MySQL de ComicRack (option « Lecture et
//!   écriture ») des infos écrites et des fichiers convertis.
//!
//! Le travail lui-même est fait par `comicrust-core` (`edit`, `writer`, `db::update_books`,
//! `Store`). Aucune dépendance à Tauri : tout est testé ici.

use crate::biblio::{Biblio, GardeEdition, SourceBiblio};
use crate::lecteur::Lecteur;
use crate::reglages::{self, ReglagesEdition};
use comicrust_core::db::{self, BookEdit, ComicDatabase, DbConfig, FileState, Record, WriteReport};
use comicrust_core::edit::CUSTOM_VALUE_PREFIX;
use comicrust_core::edit::{self, files, EditReport, EditTarget, EditValue, FieldDef, FieldEdit, FieldKind};
use comicrust_core::library::LibraryBook;
use comicrust_core::store::{Store, StoreConfig};
use comicrust_core::writer::{self, Outcome};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Au-delà de ce nombre de livres, une opération demande confirmation (seuil des retraits en
/// masse de la version egui, `REMOVE_CONFIRM_ABOVE`).
pub const SEUIL_CONFIRMATION: usize = 100;
/// Dossier (à côté de l'original) où l'original converti est rangé, si demandé.
pub const DOSSIER_ORIGINAUX: &str = "_originaux";
/// Suggestions affichées au plus pendant la saisie (comme la version egui).
pub const MAX_PROPOSITIONS: usize = 8;
/// Période minimale entre deux `edition-progression`.
const PERIODE_PROGRESSION: Duration = Duration::from_millis(100);

/// Texte du champ d'un livre dont les valeurs diffèrent (affiché par le front).
pub const VALEURS_MULTIPLES: &str = "(valeurs multiples)";

/// Lecture seule : base ComicRack sans écriture.
pub const LECTURE_SEULE_COMICRACK: &str = "Lecture seule : active l'écriture dans la base ComicRack (Réglages ▸ \
     Édition, « Lecture et écriture ») pour modifier les fiches. Les fichiers (infos, conversion) restent modifiables.";
/// Lecture seule : `ComicDb.xml`.
pub const LECTURE_SEULE_XML: &str = "Lecture seule : la bibliothèque ComicDb.xml de ComicRack ne se modifie pas \
     depuis ComicRust (ComicRack la réécrit lui-même). Utilise la base MySQL de ComicRack en écriture, ou une \
     bibliothèque autonome.";
/// Livre non traité parce que l'arrêt du lot a été demandé.
const NON_FAIT_ARRET: &str = "non fait : arrêt demandé";
/// Pas de bibliothèque.
pub const AUCUNE_BIBLIOTHEQUE: &str = "Aucune bibliothèque ouverte : rien à modifier.";

// ───────────────────────────── Onglets ─────────────────────────────

/// Onglets de la fenêtre et leurs champs, dans l'ordre (`TABS` de la version egui, d'après
/// `ComicBookDialog` de ComicRack). Les valeurs personnalisées ont leur onglet à part.
pub const ONGLETS: &[(&str, &[&str])] = &[
    (
        "Résumé",
        &[
            "Series",
            "Number",
            "Count",
            "Volume",
            "Title",
            "AlternateSeries",
            "AlternateNumber",
            "AlternateCount",
            "Year",
            "Month",
            "Day",
            "Writer",
            "Penciller",
            "Inker",
            "Colorist",
            "Letterer",
            "CoverArtist",
            "Editor",
            "Translator",
            "Rating",
        ],
    ),
    (
        "Détails",
        &[
            "Publisher",
            "Imprint",
            "Genre",
            "Format",
            "AgeRating",
            "LanguageISO",
            "BlackAndWhite",
            "Manga",
            "SeriesComplete",
            "StoryArc",
            "SeriesGroup",
            "Tags",
            "CommunityRating",
            "Web",
            "PreferredFrontCover",
            "EnableProposed",
        ],
    ),
    (
        "Intrigue et notes",
        &["Characters", "Teams", "MainCharacterOrTeam", "Locations", "ScanInformation", "Summary", "Notes", "Review"],
    ),
    (
        "Catalogue",
        &[
            "BookStore",
            "BookOwner",
            "BookAge",
            "BookCondition",
            "BookCollectionStatus",
            "BookPrice",
            "BookLocation",
            "ISBN",
            "Released",
            "Checked",
            "BookNotes",
        ],
    ),
];

/// Nom de l'onglet des valeurs personnalisées.
pub const ONGLET_PERSONNALISE: &str = "Personnalisé";

// ───────────────────────────── Types de l'IPC ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TypeChamp {
    Texte,
    TexteLong,
    Liste,
    Entier,
    Note,
    Prix,
    OuiNon,
    Manga,
    Booleen,
    Date,
    Langue,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choix {
    pub valeur: String,
    pub libelle: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampFiche {
    pub nom: String,
    pub libelle: String,
    #[serde(rename = "type")]
    pub type_champ: TypeChamp,
    pub valeur: String,
    pub multiple: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choix: Option<Vec<Choix>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i32>,
    pub suggestions: bool,
    pub dans_comic_info: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indication: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Onglet {
    pub nom: String,
    pub champs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivreEdite {
    pub id: String,
    pub titre: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fichier: Option<String>,
    pub url_couverture: String,
    pub pages: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taille_fichier: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progression: Option<u32>,
    pub infos_a_ecrire: bool,
    pub cbz: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FicheEdition {
    pub ids: Vec<String>,
    pub multiple: bool,
    pub livres: Vec<LivreEdite>,
    pub onglets: Vec<Onglet>,
    pub champs: Vec<ChampFiche>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lecture_seule: Option<String>,
    pub ecrire_dans_fichier_possible: bool,
    pub ecrire_dans_fichier: bool,
    pub seuil_confirmation: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChampModifie {
    pub nom: String,
    pub valeur: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErreurChamp {
    pub nom: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposition {
    pub valeur: String,
    pub texte: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeModification {
    pub ids: Vec<String>,
    pub champs: Vec<ChampModifie>,
    #[serde(default)]
    pub ecrire_dans_fichier: bool,
    #[serde(default)]
    pub confirme: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeRenommage {
    pub ids: Vec<String>,
    pub modele: String,
    #[serde(default)]
    pub dossier_cible: Option<String>,
    pub apercu: bool,
    #[serde(default)]
    pub confirme: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModeleNom {
    pub nom: String,
    pub modele: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneRenommage {
    pub id: String,
    pub titre: String,
    pub de: String,
    pub vers: String,
    pub de_nom: String,
    pub vers_nom: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probleme: Option<String>,
    pub inchange: bool,
}

impl LigneRenommage {
    fn applicable(&self) -> bool {
        self.probleme.is_none() && !self.inchange
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatRenommage {
    pub lignes: Vec<LigneRenommage>,
    pub a_renommer: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bilan: Option<BilanOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Operation {
    Modification,
    EcritureInfos,
    Conversion,
    Renommage,
}

impl Operation {
    fn titre(self) -> &'static str {
        match self {
            Operation::Modification => "Modifier les infos",
            Operation::EcritureInfos => "Écrire les infos dans les fichiers",
            Operation::Conversion => "Convertir en CBZ",
            Operation::Renommage => "Renommer / déplacer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Statut {
    Ok,
    Inchange,
    Ignore,
    Erreur,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResultatLivre {
    pub id: String,
    pub titre: String,
    pub statut: Statut,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fichier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Confirmation {
    pub nb_livres: usize,
    pub seuil: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BilanOperation {
    pub operation: Operation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_requise: Option<Confirmation>,
    pub resume: String,
    pub complet: bool,
    pub interrompu: bool,
    pub livres: Vec<ResultatLivre>,
    pub rapport: String,
}

/// Événements transmis au front par `app.rs`.
#[derive(Debug, Clone, PartialEq)]
pub enum EvenementEdition {
    Progression { operation: Operation, fait: usize, total: usize, livre: String },
    BibliothequeModifiee { ids: Vec<String> },
}

impl EvenementEdition {
    /// Nom de l'événement Tauri et sa charge JSON (camelCase), selon le contrat IPC.
    pub fn pour_le_front(&self) -> (&'static str, serde_json::Value) {
        use serde_json::json;
        match self {
            EvenementEdition::Progression { operation, fait, total, livre } => {
                ("edition-progression", json!({"operation": operation, "fait": fait, "total": total, "livre": livre}))
            }
            EvenementEdition::BibliothequeModifiee { ids } => ("bibliotheque-modifiee", json!({ "ids": ids })),
        }
    }
}

/// Sortie des événements (fermeture appelée depuis le fil de l'opération).
pub type Sortie<'a> = &'a (dyn Fn(EvenementEdition) + Send + Sync);

// ───────────────────────────── Valeurs affichées (info_form.rs) ─────────────────────────────

/// Texte affiché (et modifiable) d'un champ du catalogue pour `b` (`display` de la version egui).
pub fn affichage(def: &FieldDef, b: &LibraryBook) -> String {
    let v = def.get(b);
    match (def.kind, v) {
        (FieldKind::Bool { default }, v) => {
            let on = match v {
                Some(EditValue::Bool(x)) => x,
                _ => default,
            };
            if on { "Oui" } else { "Non" }.to_string()
        }
        (_, None) => String::new(),
        (FieldKind::Date, Some(EditValue::Text(t))) => match (t.get(0..4), t.get(5..7), t.get(8..10)) {
            (Some(y), Some(m), Some(d)) if !t.starts_with("0001-01-01") => format!("{d}/{m}/{y}"),
            _ => String::new(),
        },
        (FieldKind::YesNo | FieldKind::Manga, Some(EditValue::Text(t))) if t.eq_ignore_ascii_case("unknown") => {
            String::new()
        }
        (_, Some(EditValue::Real(f))) => format!("{}", f as f32).replace('.', ","),
        _ => def.text(b),
    }
}

/// Texte d'un champ (catalogue ou valeur personnalisée) pour `b`.
pub fn affichage_champ(nom: &str, b: &LibraryBook) -> String {
    match nom.strip_prefix(CUSTOM_VALUE_PREFIX) {
        Some(cle) => edit::custom_value(b.custom_values_store.as_deref(), cle).unwrap_or_default(),
        None => edit::field(nom).map(|d| affichage(d, b)).unwrap_or_default(),
    }
}

/// Champs du catalogue présentés (tous sauf le texte brut des valeurs personnalisées).
pub fn champs_du_formulaire() -> impl Iterator<Item = &'static FieldDef> {
    edit::FIELDS.iter().filter(|d| d.kind != FieldKind::CustomValues)
}

/// Le champ propose-t-il les valeurs de la bibliothèque ? (`has_suggestions`)
pub fn a_des_suggestions(def: &FieldDef) -> bool {
    matches!(def.kind, FieldKind::Text | FieldKind::List | FieldKind::Language)
        && !matches!(def.name, "Title" | "Number" | "AlternateNumber" | "Web" | "ISBN")
}

/// Valeurs existantes du champ dans la bibliothèque (chaque valeur d'une liste à part), sans
/// doublon (casse ignorée), en ordre naturel ; plus les valeurs usuelles du champ.
pub fn valeurs_existantes(base: &ComicDatabase, def: &FieldDef) -> Vec<String> {
    let mut vus = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut ajouter = |v: &str| {
        let v = v.trim();
        if !v.is_empty() && v != "Unknown" && vus.insert(v.to_lowercase()) {
            out.push(v.to_string());
        }
    };
    for s in def.suggestions() {
        ajouter(s);
    }
    for b in &base.books {
        let t = def.text(b);
        if def.kind == FieldKind::List {
            t.split(',').for_each(&mut ajouter);
        } else {
            ajouter(&t);
        }
    }
    out.sort_by(|a, b| natord::compare_ignore_case(a, b));
    out
}

/// Partie en cours de saisie : tout le texte, ou la dernière valeur d'une liste.
pub fn jeton_courant(texte: &str, liste: bool) -> &str {
    let t = if liste { texte.rsplit(',').next().unwrap_or(texte) } else { texte };
    t.trim_start()
}

/// Suggestions pour `jeton` : celles qui commencent par lui, puis celles qui le contiennent
/// (casse ignorée), au plus `max` ; rien pour une saisie vide ou déjà complète (`matching`).
pub fn correspondances<'a>(jeton: &str, toutes: &'a [String], max: usize) -> Vec<&'a str> {
    let t = jeton.trim().to_lowercase();
    if t.is_empty() {
        return Vec::new();
    }
    let min: Vec<String> = toutes.iter().map(|s| s.to_lowercase()).collect();
    if min.contains(&t) && min.iter().filter(|s| s.starts_with(&t)).count() == 1 {
        return Vec::new();
    }
    let debut = toutes.iter().zip(&min).filter(|(_, l)| l.starts_with(&t)).map(|(s, _)| s.as_str());
    let dedans = toutes.iter().zip(&min).filter(|(_, l)| !l.starts_with(&t) && l.contains(&t)).map(|(s, _)| s.as_str());
    debut.chain(dedans).take(max).collect()
}

/// Texte après le choix de `choix` : remplace la partie en cours de saisie (`accept`).
pub fn accepter(texte: &str, liste: bool, choix: &str) -> String {
    if !liste {
        return choix.to_string();
    }
    match texte.rfind(',') {
        Some(i) => format!("{}, {choix}", texte[..i].trim_end()),
        None => choix.to_string(),
    }
}

fn type_champ(kind: FieldKind) -> TypeChamp {
    match kind {
        FieldKind::Text | FieldKind::CustomValues => TypeChamp::Texte,
        FieldKind::LongText => TypeChamp::TexteLong,
        FieldKind::List => TypeChamp::Liste,
        FieldKind::Int { .. } => TypeChamp::Entier,
        FieldKind::Rating => TypeChamp::Note,
        FieldKind::Price => TypeChamp::Prix,
        FieldKind::YesNo => TypeChamp::OuiNon,
        FieldKind::Manga => TypeChamp::Manga,
        FieldKind::Bool { .. } => TypeChamp::Booleen,
        FieldKind::Date => TypeChamp::Date,
        FieldKind::Language => TypeChamp::Langue,
    }
}

/// Choix des listes Oui / Non / Inconnu : (valeur du champ, libellé) (`yes_no_choices`).
fn choix_de(kind: FieldKind) -> Option<Vec<Choix>> {
    let c: &[(&str, &str)] = match kind {
        FieldKind::Manga => {
            &[("", "Inconnu"), ("No", "Non"), ("Yes", "Oui"), ("YesAndRightToLeft", "Oui, de droite à gauche")]
        }
        FieldKind::Bool { .. } => &[("Oui", "Oui"), ("Non", "Non")],
        FieldKind::YesNo => &[("", "Inconnu"), ("No", "Non"), ("Yes", "Oui")],
        _ => return None,
    };
    Some(c.iter().map(|(v, l)| Choix { valeur: v.to_string(), libelle: l.to_string() }).collect())
}

fn indication(kind: FieldKind) -> Option<String> {
    match kind {
        FieldKind::Date => Some("JJ/MM/AAAA".into()),
        FieldKind::List => Some("valeurs séparées par des virgules".into()),
        FieldKind::Rating => Some("note de 0 à 5 (décimales permises : 3,5)".into()),
        FieldKind::Language => Some("code de langue ISO (fr, en…)".into()),
        _ => None,
    }
}

/// Champ du formulaire pour les livres `livres` (valeur commune, sinon multiple).
fn champ(nom: &str, livres: &[&LibraryBook]) -> ChampFiche {
    let mut valeurs = livres.iter().map(|b| affichage_champ(nom, b));
    let premiere = valeurs.next().unwrap_or_default();
    let multiple = !valeurs.all(|v| v == premiere);
    let valeur = if multiple { String::new() } else { premiere };
    match nom.strip_prefix(CUSTOM_VALUE_PREFIX) {
        Some(cle) => ChampFiche {
            nom: nom.to_string(),
            libelle: cle.to_string(),
            type_champ: TypeChamp::Texte,
            valeur,
            multiple,
            choix: None,
            min: None,
            max: None,
            suggestions: false,
            dans_comic_info: false,
            indication: None,
        },
        None => {
            let def = edit::field(nom).expect("champ du catalogue");
            let (min, max) = match def.kind {
                FieldKind::Int { min, max, .. } => (Some(min), Some(max)),
                _ => (None, None),
            };
            ChampFiche {
                nom: def.name.to_string(),
                libelle: def.label.to_string(),
                type_champ: type_champ(def.kind),
                valeur,
                multiple,
                choix: choix_de(def.kind),
                min,
                max,
                suggestions: a_des_suggestions(def),
                dans_comic_info: def.in_comic_info,
                indication: indication(def.kind),
            }
        }
    }
}

/// Nom canonique d'un champ modifiable (`Series`, `CustomValue:<clé>`), ou erreur.
fn nom_canonique(nom: &str) -> Result<String, String> {
    let nom = nom.trim();
    if let Some(cle) = nom.strip_prefix(CUSTOM_VALUE_PREFIX) {
        if cle.trim().is_empty() {
            return Err("valeur personnalisée sans nom".into());
        }
        return Ok(format!("{CUSTOM_VALUE_PREFIX}{cle}"));
    }
    match edit::field(nom) {
        Some(d) if d.kind != FieldKind::CustomValues => Ok(d.name.to_string()),
        _ => Err(format!("champ « {nom} » non modifiable dans la fenêtre")),
    }
}

/// Modification d'un champ pour le livre `id` (`FormField::edit_for`) : texte vide = vider.
fn modification(id: &str, nom: &str, valeur: &str) -> FieldEdit {
    let value = (!valeur.trim().is_empty()).then(|| EditValue::Text(valeur.to_string()));
    FieldEdit { book_id: id.to_string(), field: nom.to_string(), value }
}

/// Erreurs des valeurs saisies (`FormField::error`, libellé retiré du message).
pub fn verifier_champs(champs: &[ChampModifie]) -> Vec<ErreurChamp> {
    let mut out = Vec::new();
    for c in champs {
        let nom = match nom_canonique(&c.nom) {
            Ok(n) => n,
            Err(message) => {
                out.push(ErreurChamp { nom: c.nom.clone(), message });
                continue;
            }
        };
        // Garde-fou : le texte affiché d'un champ aux valeurs différentes ne doit jamais être
        // écrit tel quel dans les fiches (front qui l'aurait pris pour une saisie).
        if c.valeur.trim() == VALEURS_MULTIPLES {
            out.push(ErreurChamp {
                nom: c.nom.clone(),
                message: format!(
                    "« {VALEURS_MULTIPLES} » n'est pas une valeur : laisse le champ vide pour ne pas le modifier"
                ),
            });
            continue;
        }
        if let Err(e) = edit::validate(&modification("verification", &nom, &c.valeur)) {
            let m = e.message;
            let message = match m.find("» : ") {
                Some(i) if m.starts_with('«') => m[i + "» : ".len()..].to_string(),
                _ => m,
            };
            out.push(ErreurChamp { nom: c.nom.clone(), message });
        }
    }
    out
}

/// Nouvelle valeur personnalisée (vide) ; `Err` : clé refusée (`InfoForm::add_custom`).
pub fn nouvelle_valeur_perso(cle: &str, existantes: &[String]) -> Result<ChampFiche, String> {
    let cle = cle.trim();
    if cle.is_empty() {
        return Err("nom vide".into());
    }
    if let Some(c) = cle.chars().find(|c| matches!(c, ',' | '=') || c.is_control()) {
        return Err(format!("caractère « {c} » interdit dans le nom"));
    }
    let deja = existantes.iter().any(|e| {
        let k = e.strip_prefix(CUSTOM_VALUE_PREFIX).unwrap_or(e);
        k.to_lowercase() == cle.to_lowercase()
    });
    if deja {
        return Err(format!("« {cle} » existe déjà"));
    }
    Ok(champ(&format!("{CUSTOM_VALUE_PREFIX}{cle}"), &[]))
}

/// Modèles de nom proposés.
pub fn modeles_de_nom() -> Vec<ModeleNom> {
    vec![
        ModeleNom { nom: "Série v2 #12 (2024)".into(), modele: files::DEFAULT_NAME_TEMPLATE.into() },
        ModeleNom { nom: "Export ComicRack".into(), modele: files::COMICRACK_EXPORT_TEMPLATE.into() },
        ModeleNom {
            nom: "Éditeur / Série / Série #012".into(),
            modele: "{publisher}/{series}/{series}[ #{number:000}]".into(),
        },
    ]
}

// ───────────────────────────── Fichiers (library/files.rs) ─────────────────────────────

/// Le fichier reçoit-il les infos (CBZ / ZIP) ?
pub fn accepte_infos(chemin: &Path) -> bool {
    matches!(extension(chemin).as_str(), "cbz" | "zip")
}

/// Le fichier se convertit-il en CBZ (CBR / RAR, CB7 / 7Z, CBT / TAR) ?
pub fn accepte_conversion(chemin: &Path) -> bool {
    matches!(extension(chemin).as_str(), "cbr" | "rar" | "cb7" | "7z" | "cbt" | "tar")
}

fn extension(chemin: &Path) -> String {
    chemin.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

/// Range l'original dans `_originaux` (même dossier) ; un nom libre est choisi au besoin.
pub fn ranger_original(chemin: &Path) -> Result<PathBuf, String> {
    let dossier = chemin.parent().unwrap_or(Path::new(".")).join(DOSSIER_ORIGINAUX);
    std::fs::create_dir_all(&dossier).map_err(|e| format!("dossier {} : {e}", dossier.display()))?;
    let nom = chemin.file_name().unwrap_or_default();
    let mut dest = dossier.join(nom);
    let mut n = 2;
    while dest.exists() {
        let base = chemin.file_stem().unwrap_or_default().to_string_lossy();
        let ext = chemin.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        dest = dossier.join(format!("{base} ({n}){ext}"));
        n += 1;
    }
    std::fs::rename(chemin, &dest).map_err(|e| format!("original non déplacé ({e}) ; il est resté à sa place"))?;
    Ok(dest)
}

fn utc(t: std::io::Result<std::time::SystemTime>) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    t.ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).fixed_offset())
}

/// État d'un fichier sur le disque (`file_state` de la version egui).
fn etat_fichier(chemin: &Path, pages: Option<u32>) -> std::io::Result<FileState> {
    let meta = std::fs::metadata(chemin)?;
    Ok(FileState {
        path: chemin.to_string_lossy().into_owned(),
        size: meta.len(),
        modified: utc(meta.modified()),
        created: utc(meta.created()),
        page_count: pages,
        pages: None,
    })
}

/// Fiche ComicRack après l'écriture des infos dans le fichier (`file_edit`, `Done::Info`) :
/// `ComicInfoIsDirty = false` seulement si la fiche a encore le compteur lu ; puis, si le fichier
/// a été réécrit, sa taille et ses dates.
fn editions_infos_ecrites(id: &str, ecrit: bool, compteur: i64, chemin: &Path) -> Result<Vec<BookEdit>, String> {
    let mut e = vec![BookEdit::new(id).comic_info_dirty(false).if_counter(compteur)];
    if ecrit {
        let f = etat_fichier(chemin, None).map_err(|e| format!("{} : {e}", chemin.display()))?;
        let mut be = BookEdit::new(id).file(&f);
        be.changes.retain(|c| c.value.is_some());
        e.push(be);
    }
    Ok(e)
}

/// Livres non écrits d'un bilan ComicRack : identifiant (minuscules) → message.
fn problemes_comicrack(r: &WriteReport) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for id in &r.missing {
        out.insert(id.to_ascii_lowercase(), "livre absent de la base ComicRack (retiré dans ComicRack ?)".into());
    }
    for id in &r.conflicts {
        out.insert(id.to_ascii_lowercase(), "fiche modifiée en même temps dans ComicRack (plusieurs essais)".into());
    }
    for id in &r.stale {
        out.insert(
            id.to_ascii_lowercase(),
            "fiche modifiée dans ComicRack depuis l'écriture du fichier : elle reste « à mettre à jour »".into(),
        );
    }
    for (id, m) in &r.failed {
        out.insert(id.to_ascii_lowercase(), m.clone());
    }
    out
}

// ───────────────────────────── Cible d'écriture ─────────────────────────────

/// Où écrire les fiches (`ApplyTarget` de la version egui).
#[derive(Debug, Clone, PartialEq)]
pub enum Cible {
    Store(StoreConfig),
    Comicrack(DbConfig),
    LectureSeule(String),
}

impl Cible {
    fn lecture_seule(&self) -> Option<&str> {
        match self {
            Cible::LectureSeule(m) => Some(m),
            _ => None,
        }
    }
}

/// Progression limitée à ~10 émissions par seconde.
struct Avance<'a> {
    sortie: Sortie<'a>,
    operation: Operation,
    total: usize,
    derniere: Option<Instant>,
}

impl Avance<'_> {
    fn noter(&mut self, fait: usize, livre: &str) {
        let due = self.derniere.is_none_or(|t| t.elapsed() >= PERIODE_PROGRESSION) || fait == self.total;
        if due {
            self.derniere = Some(Instant::now());
            (self.sortie)(EvenementEdition::Progression {
                operation: self.operation,
                fait,
                total: self.total,
                livre: livre.to_string(),
            });
        }
    }
}

/// Résultats d'une opération, rangés par livre (ordre de la demande).
#[derive(Default)]
struct Resultats {
    ordre: Vec<String>,
    par_id: HashMap<String, ResultatLivre>,
}

impl Resultats {
    fn noter(&mut self, r: ResultatLivre) {
        let k = r.id.to_ascii_lowercase();
        match self.par_id.get_mut(&k) {
            // Plusieurs étapes pour un livre (fiche puis fichier) : la plus grave l'emporte,
            // les messages s'ajoutent.
            Some(a) => {
                let rang = |s: Statut| match s {
                    Statut::Ignore => 0,
                    Statut::Inchange => 1,
                    Statut::Ok => 2,
                    Statut::Erreur => 3,
                };
                if rang(r.statut) > rang(a.statut) {
                    a.statut = r.statut;
                }
                if let Some(m) = r.message {
                    a.message = Some(match a.message.take() {
                        Some(x) => format!("{x} ; {m}"),
                        None => m,
                    });
                }
                if r.fichier.is_some() {
                    a.fichier = r.fichier;
                }
            }
            None => {
                self.ordre.push(k.clone());
                self.par_id.insert(k, r);
            }
        }
    }

    fn livres(mut self) -> Vec<ResultatLivre> {
        self.ordre.iter().filter_map(|k| self.par_id.remove(k)).collect()
    }
}

fn resultat(b: &LibraryBook, statut: Statut, message: Option<String>) -> ResultatLivre {
    ResultatLivre { id: b.id.clone(), titre: b.caption(), statut, message, fichier: None }
}

/// Bilan d'une opération (résumé d'une ligne et rapport à copier, comme `FileJob::report`).
fn bilan(
    operation: Operation,
    livres: Vec<ResultatLivre>,
    interrompu: bool,
    en_tete: Option<String>,
) -> BilanOperation {
    let n = |s: Statut| livres.iter().filter(|l| l.statut == s).count();
    let (ok, inchange, ignore, erreur) = (n(Statut::Ok), n(Statut::Inchange), n(Statut::Ignore), n(Statut::Erreur));
    let verbe = match operation {
        Operation::Modification => "livre(s) modifié(s)",
        Operation::EcritureInfos => "fichier(s) écrit(s)",
        Operation::Conversion => "livre(s) converti(s)",
        Operation::Renommage => "fichier(s) renommé(s) ou déplacé(s)",
    };
    let mut parts = vec![format!("{ok} {verbe}")];
    if inchange > 0 {
        parts.push(format!("{inchange} déjà à jour"));
    }
    if ignore > 0 {
        parts.push(format!("{ignore} ignoré(s)"));
    }
    if erreur > 0 {
        parts.push(format!("{erreur} erreur(s)"));
    }
    if interrompu {
        parts.push("interrompu".into());
    }
    let resume = parts.join(", ");
    let mut rapport = format!(
        "ComicRust-Tauri v{} — {} — {}\n{} livre(s) : {resume}\n",
        env!("CARGO_PKG_VERSION"),
        operation.titre(),
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
        livres.len()
    );
    if let Some(t) = en_tete {
        rapport.push_str(&t);
        rapport.push('\n');
    }
    rapport.push('\n');
    for l in &livres {
        let etat = match l.statut {
            Statut::Ok => "fait",
            Statut::Inchange => "déjà à jour",
            Statut::Ignore => "ignoré",
            Statut::Erreur => "ERREUR",
        };
        let mut ligne = format!("- {} : {etat}", l.titre);
        if let Some(f) = &l.fichier {
            ligne.push_str(&format!(" → {f}"));
        }
        if let Some(m) = &l.message {
            ligne.push_str(&format!(" ({m})"));
        }
        rapport.push_str(&ligne);
        rapport.push('\n');
    }
    BilanOperation {
        operation,
        confirmation_requise: None,
        complet: erreur == 0 && !interrompu,
        resume,
        interrompu,
        livres,
        rapport,
    }
}

/// Bilan « rien n'a été fait, confirmation demandée ».
fn confirmation(operation: Operation, nb: usize) -> BilanOperation {
    let message = format!(
        "{} : {nb} livres concernés (plus de {SEUIL_CONFIRMATION}). Confirme pour lancer l'opération sur tous ces livres.",
        operation.titre()
    );
    BilanOperation {
        operation,
        confirmation_requise: Some(Confirmation { nb_livres: nb, seuil: SEUIL_CONFIRMATION, message: message.clone() }),
        resume: message,
        complet: false,
        interrompu: false,
        livres: Vec::new(),
        rapport: String::new(),
    }
}

/// Mêmes chemins (sans casse pour un chemin Windows, où les noms de fichiers l'ignorent) ?
fn meme_chemin(a: &Path, b: &Path) -> bool {
    let windows = |p: &Path| {
        let s = p.to_string_lossy();
        s.starts_with("\\\\") || s.as_bytes().get(1) == Some(&b':') || s.contains('\\')
    };
    if cfg!(windows) || windows(a) || windows(b) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Textes d'un renommage : noms seuls si le dossier ne change pas, sinon chemin complet.
fn textes_renommage(de: &Path, vers: &Path) -> (String, String) {
    let nom = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if de.parent() == vers.parent() {
        (nom(de), nom(vers))
    } else {
        (nom(de), vers.display().to_string())
    }
}

// ───────────────────────────── Service ─────────────────────────────

/// Suggestions mises en cache : (bibliothèque pour laquelle elles valent, par champ).
type CacheSuggestions = Option<(usize, HashMap<&'static str, Arc<Vec<String>>>)>;

/// Service d'édition, partageable entre les commandes (clonage bon marché).
#[derive(Clone)]
pub struct Edition {
    biblio: Biblio,
    lecteur: Lecteur,
    annule: Arc<AtomicBool>,
    suggestions: Arc<Mutex<CacheSuggestions>>,
}

impl Edition {
    pub fn nouvelle(biblio: Biblio, lecteur: Lecteur) -> Self {
        Edition { biblio, lecteur, annule: Arc::default(), suggestions: Arc::default() }
    }

    /// Cible d'écriture des fiches, d'après la bibliothèque ouverte et les réglages.
    pub fn cible(&self) -> Cible {
        match self.biblio.source() {
            SourceBiblio::Autonome(c) => Cible::Store(c),
            SourceBiblio::ComicrackMysql(_) => match self.biblio.config_comicrack_ecriture() {
                Some(c) => Cible::Comicrack(c),
                None => Cible::LectureSeule(LECTURE_SEULE_COMICRACK.into()),
            },
            SourceBiblio::ComicrackXml => Cible::LectureSeule(LECTURE_SEULE_XML.into()),
            SourceBiblio::Aucune => Cible::LectureSeule(AUCUNE_BIBLIOTHEQUE.into()),
        }
    }

    /// Une opération à la fois, jamais pendant une analyse des dossiers (et l'analyse attend
    /// la fin de l'opération) : garde rendue tant que l'opération dure.
    fn occuper(&self) -> Result<GardeEdition, String> {
        let garde = self.biblio.prendre_edition()?;
        self.annule.store(false, Ordering::SeqCst);
        Ok(garde)
    }

    /// Demande l'arrêt du lot en cours (entre deux livres).
    pub fn annuler(&self) {
        self.annule.store(true, Ordering::SeqCst);
    }

    fn reglages_edition(&self) -> ReglagesEdition {
        reglages::lire(self.biblio.data()).map(|r| r.edition).unwrap_or_default()
    }

    fn memoriser(&self, m: ReglagesEdition) {
        // Un réglage non mémorisé ne doit pas faire échouer une opération faite.
        let _ = reglages::enregistrer_reglages_edition(self.biblio.data(), m);
    }

    fn livres(&self, base: &ComicDatabase, ids: &[String]) -> Vec<LibraryBook> {
        let mut vus = HashSet::new();
        ids.iter().filter_map(|i| base.get(i)).filter(|b| vus.insert(b.id.to_ascii_lowercase())).cloned().collect()
    }

    // ───────────── Fenêtre « Infos » ─────────────

    /// Fiche(s) à modifier.
    pub fn lire_fiches(&self, ids: &[String]) -> Result<FicheEdition, String> {
        let base = self.biblio.base()?;
        let livres = self.livres(&base, ids);
        if livres.is_empty() {
            return Err("aucun livre connu dans la sélection".into());
        }
        let refs: Vec<&LibraryBook> = livres.iter().collect();
        let mut champs: Vec<ChampFiche> = champs_du_formulaire().map(|d| champ(d.name, &refs)).collect();
        // Valeurs personnalisées : toutes les clés des livres (sans casse), triées.
        let mut cles: Vec<String> = Vec::new();
        for b in &livres {
            for (k, _) in edit::custom_values(b.custom_values_store.as_deref()) {
                if !cles.iter().any(|x| x.to_lowercase() == k.to_lowercase()) {
                    cles.push(k);
                }
            }
        }
        cles.sort_by_key(|k| k.to_lowercase());
        let perso: Vec<String> = cles.iter().map(|k| format!("{CUSTOM_VALUE_PREFIX}{k}")).collect();
        champs.extend(perso.iter().map(|n| champ(n, &refs)));
        let mut onglets: Vec<Onglet> = ONGLETS
            .iter()
            .map(|(nom, c)| Onglet { nom: nom.to_string(), champs: c.iter().map(|s| s.to_string()).collect() })
            .collect();
        onglets.push(Onglet { nom: ONGLET_PERSONNALISE.into(), champs: perso });
        let livres_edites: Vec<LivreEdite> = livres
            .iter()
            .map(|b| LivreEdite {
                id: b.id.clone(),
                titre: b.caption(),
                fichier: b.file.clone().filter(|f| !f.trim().is_empty()),
                url_couverture: self.biblio.url_couverture(&b.id),
                pages: b.page_count.unwrap_or(0),
                taille_fichier: b.file_size.filter(|s| *s > 0),
                progression: b.read_percent().filter(|p| *p > 0),
                infos_a_ecrire: b.comic_info_is_dirty == Some(true),
                cbz: b.file.as_deref().is_some_and(|f| accepte_infos(Path::new(f))),
            })
            .collect();
        let possible = livres_edites.iter().any(|l| l.cbz);
        Ok(FicheEdition {
            ids: livres.iter().map(|b| b.id.clone()).collect(),
            multiple: livres.len() > 1,
            livres: livres_edites,
            onglets,
            champs,
            lecture_seule: self.cible().lecture_seule().map(str::to_string),
            ecrire_dans_fichier_possible: possible,
            ecrire_dans_fichier: self.reglages_edition().ecrire_dans_fichier.unwrap_or(false),
            seuil_confirmation: SEUIL_CONFIRMATION,
        })
    }

    /// Valeurs proposées pour la saisie `texte` du champ `champ`.
    pub fn proposer_valeurs(
        &self,
        champ: &str,
        texte: &str,
        tout: bool,
        max: Option<usize>,
    ) -> Result<Vec<Proposition>, String> {
        let Some(def) = edit::field(champ).filter(|d| a_des_suggestions(d)) else { return Ok(Vec::new()) };
        let base = self.biblio.base()?;
        let cle = Arc::as_ptr(&base) as usize;
        let toutes = {
            let mut c = self.suggestions.lock().unwrap_or_else(|e| e.into_inner());
            if c.as_ref().is_none_or(|(k, _)| *k != cle) {
                *c = Some((cle, HashMap::new()));
            }
            let (_, table) = c.as_mut().expect("cache initialisé");
            table.entry(def.name).or_insert_with(|| Arc::new(valeurs_existantes(&base, def))).clone()
        };
        let liste = def.kind == FieldKind::List;
        let jeton = jeton_courant(texte, liste).to_lowercase();
        let choisies: Vec<&str> = if tout {
            toutes.iter().map(String::as_str).filter(|v| v.to_lowercase().contains(&jeton)).collect()
        } else {
            correspondances(&jeton, &toutes, max.unwrap_or(MAX_PROPOSITIONS))
        };
        Ok(choisies
            .into_iter()
            .map(|v| Proposition { valeur: v.to_string(), texte: accepter(texte, liste, v) })
            .collect())
    }

    /// Enregistre les champs modifiés (fiches), puis écrit les infos dans les fichiers si demandé.
    pub fn appliquer_modifications(&self, d: &DemandeModification, sortie: Sortie) -> Result<BilanOperation, String> {
        let _occupe = self.occuper()?;
        let cible = self.cible();
        if let Some(m) = cible.lecture_seule() {
            return Err(m.to_string());
        }
        let erreurs = verifier_champs(&d.champs);
        if !erreurs.is_empty() {
            let lignes: Vec<String> = erreurs
                .iter()
                .map(|e| {
                    let libelle = edit::field(&e.nom).map(|f| f.label.to_string()).unwrap_or_else(|| e.nom.clone());
                    format!("- {libelle} : {}", e.message)
                })
                .collect();
            return Err(format!("valeurs refusées, rien n'est enregistré :\n{}", lignes.join("\n")));
        }
        let base = self.biblio.base()?;
        let livres = self.livres(&base, &d.ids);
        if livres.is_empty() {
            return Err("aucun livre connu dans la sélection".into());
        }
        if livres.len() > SEUIL_CONFIRMATION && !d.confirme {
            return Ok(confirmation(Operation::Modification, livres.len()));
        }
        self.memoriser(ReglagesEdition { ecrire_dans_fichier: Some(d.ecrire_dans_fichier), ..Default::default() });
        // Un champ envoyé deux fois : la dernière valeur l'emporte.
        let mut champs: Vec<(String, String)> = Vec::new();
        for c in &d.champs {
            let nom = nom_canonique(&c.nom)?;
            champs.retain(|(n, _)| !n.eq_ignore_ascii_case(&nom));
            champs.push((nom, c.valeur.clone()));
        }
        let mut res = Resultats::default();
        let mut modifies: Vec<String> = Vec::new();
        let mut avance = Avance { sortie, operation: Operation::Modification, total: livres.len(), derniere: None };
        if !champs.is_empty() {
            avance.noter(0, &livres[0].caption());
            let edits: Vec<FieldEdit> =
                livres.iter().flat_map(|b| champs.iter().map(|(n, v)| modification(&b.id, n, v))).collect();
            let rapport = match &cible {
                Cible::Store(cfg) => Store::open_existing(cfg)
                    .and_then(|mut st| edit::apply_edits(EditTarget::Store(&mut st), &edits))
                    .map_err(|e| format!("base ComicRust inaccessible, fiches non modifiées : {e}"))?,
                Cible::Comicrack(cfg) => edit::apply_edits(EditTarget::ComicRack(cfg), &edits)
                    .map_err(|e| format!("base ComicRack inaccessible, fiches non modifiées : {e}"))?,
                Cible::LectureSeule(m) => return Err(m.clone()),
            };
            avance.noter(livres.len(), "");
            let couv = champs.iter().any(|(n, _)| n == "PreferredFrontCover");
            modifies.extend(self.biblio.remplacer_fiches(&rapport.changed, couv));
            noter_rapport_fiches(&mut res, &livres, &rapport);
        }
        let mut interrompu = false;
        if d.ecrire_dans_fichier {
            // Fiches relues après l'écriture : ce sont elles qui vont dans les fichiers.
            let base = self.biblio.base()?;
            let fraiches = self.livres(&base, &d.ids);
            let cibles: Vec<LibraryBook> = fraiches
                .iter()
                .filter(|b| b.file.as_deref().is_some_and(|f| accepte_infos(Path::new(f))))
                .cloned()
                .collect();
            let (r, m, i) = self.ecrire_infos_interne(&base, &cibles, sortie);
            for x in r {
                res.noter(x);
            }
            modifies.extend(m);
            interrompu = i;
        }
        self.signaler(sortie, modifies);
        Ok(bilan(Operation::Modification, res.livres(), interrompu, None))
    }

    fn signaler(&self, sortie: Sortie, mut ids: Vec<String>) {
        let mut vus = HashSet::new();
        ids.retain(|i| vus.insert(i.to_ascii_lowercase()));
        if !ids.is_empty() {
            sortie(EvenementEdition::BibliothequeModifiee { ids });
        }
    }

    // ───────────── Fichiers ─────────────

    /// Fiches à écrire dans les fichiers (`RecordSource` de la version egui) : relues dans MySQL
    /// en mode ComicRack MySQL, sinon prises dans la bibliothèque chargée. `Err` : base illisible.
    fn fiches_pour_fichiers(
        &self,
        base: &ComicDatabase,
        livres: &[LibraryBook],
    ) -> Result<HashMap<String, Record>, String> {
        let ids: Vec<String> = livres.iter().map(|b| b.id.clone()).collect();
        match self.biblio.source() {
            SourceBiblio::ComicrackMysql(cfg) => db::fetch_records(&cfg, &ids)
                .map(|m| m.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v)).collect())
                .map_err(|e| e.to_string()),
            _ => Ok(livres
                .iter()
                .filter_map(|b| {
                    let data = b.to_record_xml().ok()?;
                    let counter = base.rows.get(&b.id.to_ascii_lowercase()).copied().unwrap_or(0);
                    Some((b.id.to_ascii_lowercase(), Record { data, counter }))
                })
                .collect()),
        }
    }

    /// Écrit les infos dans le `ComicInfo.xml` des CBZ.
    pub fn ecrire_infos_fichiers(
        &self,
        ids: &[String],
        confirme: bool,
        sortie: Sortie,
    ) -> Result<BilanOperation, String> {
        let _occupe = self.occuper()?;
        let base = self.biblio.base()?;
        let livres = self.livres(&base, ids);
        if livres.is_empty() {
            return Err("aucun livre connu dans la sélection".into());
        }
        let mut res = Resultats::default();
        let mut cibles = Vec::new();
        for b in &livres {
            match b.file.as_deref().filter(|f| !f.trim().is_empty()) {
                None => res.noter(resultat(b, Statut::Ignore, Some("livre sans fichier".into()))),
                Some(f) if !accepte_infos(Path::new(f)) => res.noter(resultat(
                    b,
                    Statut::Ignore,
                    Some("seuls les CBZ reçoivent les infos (convertir d'abord en CBZ)".into()),
                )),
                Some(_) => cibles.push(b.clone()),
            }
        }
        if cibles.len() > SEUIL_CONFIRMATION && !confirme {
            return Ok(confirmation(Operation::EcritureInfos, cibles.len()));
        }
        let (r, modifies, interrompu) = self.ecrire_infos_interne(&base, &cibles, sortie);
        for x in r {
            res.noter(x);
        }
        self.signaler(sortie, modifies);
        Ok(bilan(Operation::EcritureInfos, res.livres(), interrompu, None))
    }

    /// Corps de l'écriture des infos (livres en CBZ) : résultats, livres modifiés, interrompu.
    fn ecrire_infos_interne(
        &self,
        base: &ComicDatabase,
        livres: &[LibraryBook],
        sortie: Sortie,
    ) -> (Vec<ResultatLivre>, Vec<String>, bool) {
        let mut out = Vec::new();
        if livres.is_empty() {
            return (out, Vec::new(), false);
        }
        let mut avance = Avance { sortie, operation: Operation::EcritureInfos, total: livres.len(), derniere: None };
        avance.noter(0, "lecture des fiches dans la bibliothèque…");
        let fiches = self.fiches_pour_fichiers(base, livres);
        // (livre, écrit, compteur, chemin) des fichiers traités.
        let mut faits: Vec<(LibraryBook, bool, i64, PathBuf)> = Vec::new();
        let mut interrompu = false;
        for (i, b) in livres.iter().enumerate() {
            if interrompu || self.annule.load(Ordering::SeqCst) {
                interrompu = true;
                out.push(resultat(b, Statut::Ignore, Some(NON_FAIT_ARRET.into())));
                continue;
            }
            avance.noter(i, &b.caption());
            let chemin = PathBuf::from(b.file.as_deref().unwrap_or_default());
            let fiche = match &fiches {
                Ok(f) => f.get(&b.id.to_ascii_lowercase()),
                Err(e) => {
                    out.push(resultat(b, Statut::Erreur, Some(format!("fiche non relue dans la bibliothèque : {e}"))));
                    continue;
                }
            };
            let Some(fiche) = fiche else {
                out.push(resultat(b, Statut::Erreur, Some("livre absent de la bibliothèque".into())));
                continue;
            };
            match writer::write_comic_info(&chemin, &fiche.data) {
                Ok(o) => {
                    let ecrit = o == Outcome::Written;
                    if ecrit {
                        self.lecteur.oublier_fichier(&chemin);
                        self.biblio.oublier_couverture(&b.id, b.front_cover_image());
                    }
                    let mut r = resultat(b, if ecrit { Statut::Ok } else { Statut::Inchange }, None);
                    r.fichier = None;
                    out.push(r);
                    faits.push((b.clone(), ecrit, fiche.counter, chemin));
                }
                Err(e) => out.push(resultat(b, Statut::Erreur, Some(e.to_string()))),
            }
        }
        avance.noter(livres.len(), "");
        let mut modifies = Vec::new();
        // La fiche suit : « infos écrites » (et taille / dates du fichier en mode ComicRack).
        match self.cible() {
            Cible::Store(cfg) if !faits.is_empty() => {
                let ids: Vec<String> = faits.iter().map(|(b, ..)| b.id.clone()).collect();
                let maj = Store::open_existing(&cfg).and_then(|mut st| {
                    st.set_comic_info_written(&ids)?;
                    st.get(&ids)
                });
                match maj {
                    Ok(fraiches) => modifies.extend(self.biblio.remplacer_fiches(&fraiches, false)),
                    Err(e) => ajouter_erreur(
                        &mut out,
                        &ids,
                        &format!("fichier écrit, mais base ComicRust non mise à jour : {e}"),
                    ),
                }
            }
            Cible::Comicrack(cfg) if !faits.is_empty() => {
                let mut editions = Vec::new();
                for (b, ecrit, compteur, chemin) in &faits {
                    match editions_infos_ecrites(&b.id.to_ascii_lowercase(), *ecrit, *compteur, chemin) {
                        Ok(e) => editions.extend(e),
                        Err(e) => ajouter_erreur(&mut out, std::slice::from_ref(&b.id), &e),
                    }
                }
                let ids: Vec<String> = faits.iter().map(|(b, ..)| b.id.clone()).collect();
                match db::update_books(&cfg, &editions) {
                    Ok(rapport) => {
                        let fraiches: Vec<LibraryBook> = rapport.written.iter().map(|w| w.book.clone()).collect();
                        modifies.extend(self.biblio.remplacer_fiches(&fraiches, false));
                        for (id, m) in problemes_comicrack(&rapport) {
                            ajouter_erreur(&mut out, &[id], &format!("fichier écrit, mais {m}"));
                        }
                    }
                    Err(e) => ajouter_erreur(
                        &mut out,
                        &ids,
                        &format!("fichier écrit, mais fiche ComicRack non mise à jour : {e}"),
                    ),
                }
            }
            _ => {}
        }
        (out, modifies, interrompu)
    }

    /// Convertit les CBR / CB7 / CBT en CBZ.
    pub fn convertir_cbr_cbz(
        &self,
        ids: &[String],
        ranger: Option<bool>,
        confirme: bool,
        sortie: Sortie,
    ) -> Result<BilanOperation, String> {
        let _occupe = self.occuper()?;
        let ranger = ranger.unwrap_or_else(|| self.reglages_edition().ranger_original.unwrap_or(false));
        let cible = self.cible();
        if ranger && cible.lecture_seule().is_some() {
            return Err("bibliothèque en lecture seule : l'original ne peut pas être rangé dans « _originaux » \
                        (la fiche continuerait de pointer sur lui). Décoche « ranger l'original », ou active \
                        l'écriture dans la base ComicRack."
                .into());
        }
        let base = self.biblio.base()?;
        let livres = self.livres(&base, ids);
        if livres.is_empty() {
            return Err("aucun livre connu dans la sélection".into());
        }
        let mut res = Resultats::default();
        let mut cibles = Vec::new();
        for b in &livres {
            match b.file.as_deref().filter(|f| !f.trim().is_empty()) {
                None => res.noter(resultat(b, Statut::Ignore, Some("livre sans fichier".into()))),
                Some(f) if !accepte_conversion(Path::new(f)) => {
                    res.noter(resultat(b, Statut::Ignore, Some("seuls les CBR, CB7 et CBT se convertissent".into())))
                }
                Some(_) => cibles.push(b.clone()),
            }
        }
        if cibles.len() > SEUIL_CONFIRMATION && !confirme {
            return Ok(confirmation(Operation::Conversion, cibles.len()));
        }
        self.memoriser(ReglagesEdition { ranger_original: Some(ranger), ..Default::default() });
        let mut avance = Avance { sortie, operation: Operation::Conversion, total: cibles.len(), derniere: None };
        avance.noter(0, "lecture des fiches dans la bibliothèque…");
        let fiches = if cibles.is_empty() { Ok(HashMap::new()) } else { self.fiches_pour_fichiers(&base, &cibles) };
        // (livre, original, CBZ, pages)
        let mut faits: Vec<(LibraryBook, PathBuf, PathBuf, usize)> = Vec::new();
        let mut interrompu = false;
        for (i, b) in cibles.iter().enumerate() {
            if interrompu || self.annule.load(Ordering::SeqCst) {
                interrompu = true;
                res.noter(resultat(b, Statut::Ignore, Some(NON_FAIT_ARRET.into())));
                continue;
            }
            avance.noter(i, &b.caption());
            let chemin = PathBuf::from(b.file.as_deref().unwrap_or_default());
            // Fiche illisible (base ComicRack injoignable) : pas de CBZ sans les infos de la fiche.
            let fiche = match &fiches {
                Ok(f) => f.get(&b.id.to_ascii_lowercase()).map(|r| r.data.as_str()),
                Err(e) => {
                    res.noter(resultat(b, Statut::Erreur, Some(format!("fiche non relue dans la bibliothèque : {e}"))));
                    continue;
                }
            };
            match writer::convert_to_cbz(&chemin, fiche) {
                Ok(c) => {
                    let mut r = resultat(b, Statut::Ok, Some(format!("{} pages", c.pages)));
                    r.fichier = Some(c.path.display().to_string());
                    res.noter(r);
                    faits.push((b.clone(), chemin, c.path, c.pages));
                }
                Err(e) => res.noter(resultat(b, Statut::Erreur, Some(e.to_string()))),
            }
        }
        avance.noter(cibles.len(), "");
        let mut modifies = Vec::new();
        let mut erreurs: Vec<(String, String)> = Vec::new();
        // Livres dont la fiche pointe maintenant sur le CBZ (identifiants en minuscules) : seul
        // leur original peut être rangé (sinon la fiche pointerait sur un fichier disparu).
        let mut suivis: HashSet<String> = HashSet::new();
        match &cible {
            Cible::Store(cfg) if !faits.is_empty() => {
                let maj = (|| -> Result<Vec<LibraryBook>, String> {
                    let mut st = Store::open_existing(cfg).map_err(|e| e.to_string())?;
                    let mut ids = Vec::new();
                    for (b, _, cbz, _) in &faits {
                        let neuf = comicrust_core::scanner::book_from_file(cbz, b.id.clone())
                            .and_then(|f| st.replace_file(&b.id, &f));
                        match neuf {
                            Ok(_) => ids.push(b.id.clone()),
                            Err(e) => erreurs.push((b.id.clone(), format!("fiche non mise à jour : {e}"))),
                        }
                    }
                    st.get(&ids).map_err(|e| e.to_string())
                })();
                match maj {
                    Ok(fraiches) => {
                        suivis.extend(fraiches.iter().map(|f| f.id.to_ascii_lowercase()));
                        modifies.extend(self.biblio.remplacer_fiches(&fraiches, true));
                    }
                    Err(e) => {
                        erreurs.extend(faits.iter().map(|(b, ..)| (b.id.clone(), format!("base ComicRust : {e}"))))
                    }
                }
            }
            Cible::Comicrack(cfg) if !faits.is_empty() => {
                let mut editions = Vec::new();
                for (b, _, cbz, pages) in &faits {
                    // Les pages sont recopiées dans le même ordre : la liste <Pages> reste valable.
                    match etat_fichier(cbz, Some(*pages as u32)) {
                        Ok(f) => editions.push(BookEdit::new(&b.id).file(&f)),
                        Err(e) => erreurs.push((b.id.clone(), format!("{} : {e}", cbz.display()))),
                    }
                }
                match db::update_books(cfg, &editions) {
                    Ok(rapport) => {
                        let fraiches: Vec<LibraryBook> = rapport.written.iter().map(|w| w.book.clone()).collect();
                        suivis.extend(fraiches.iter().map(|f| f.id.to_ascii_lowercase()));
                        modifies.extend(self.biblio.remplacer_fiches(&fraiches, true));
                        erreurs.extend(problemes_comicrack(&rapport));
                    }
                    Err(e) => erreurs.extend(
                        faits.iter().map(|(b, ..)| (b.id.clone(), format!("fiche ComicRack non mise à jour : {e}"))),
                    ),
                }
            }
            _ => {}
        }
        for (b, original, cbz, pages) in &faits {
            self.lecteur.oublier_fichier(original);
            if !ranger {
                continue;
            }
            if !suivis.contains(&b.id.to_ascii_lowercase()) {
                erreurs.push((b.id.clone(), "original laissé à sa place (la fiche pointe toujours sur lui)".into()));
                continue;
            }
            let mut r = resultat(b, Statut::Ok, None);
            match ranger_original(original) {
                Ok(p) => r.message = Some(format!("{pages} pages, original rangé dans {}", p.display())),
                Err(e) => {
                    r.statut = Statut::Erreur;
                    r.message = Some(format!("CBZ créé ({}), mais {e}", cbz.display()));
                }
            }
            // Remplace le message « n pages » noté plus haut.
            if let Some(x) = res.par_id.get_mut(&b.id.to_ascii_lowercase()) {
                x.statut = r.statut;
                x.message = r.message;
            }
        }
        for (id, m) in erreurs {
            if let Some(b) = livres.iter().find(|b| b.id.eq_ignore_ascii_case(&id)) {
                res.noter(resultat(b, Statut::Erreur, Some(format!("CBZ créé, mais {m}"))));
            }
        }
        self.signaler(sortie, modifies);
        Ok(bilan(Operation::Conversion, res.livres(), interrompu, None))
    }

    // ───────────── Renommage ─────────────

    /// Lignes de l'aperçu d'un renommage (rien n'est fait), comme `Preview::build` des scripts.
    fn lignes_renommage(&self, livres: &[LibraryBook], modele: &str, dossier: Option<&Path>) -> Vec<LigneRenommage> {
        let mut destinations: HashMap<String, String> = HashMap::new();
        let mut lignes = Vec::new();
        for b in livres {
            let de = PathBuf::from(b.file.as_deref().unwrap_or_default().trim());
            let mut ligne = LigneRenommage {
                id: b.id.clone(),
                titre: b.caption(),
                de: de.display().to_string(),
                vers: String::new(),
                de_nom: de.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                vers_nom: String::new(),
                probleme: None,
                inchange: false,
            };
            if de.as_os_str().is_empty() {
                ligne.probleme = Some("livre sans fichier".into());
                lignes.push(ligne);
                continue;
            }
            let dossier = dossier
                .map(Path::to_path_buf)
                .unwrap_or_else(|| de.parent().map(Path::to_path_buf).unwrap_or_default());
            let vers = files::target_path(&dossier, modele, b)
                .and_then(|p| files::normalize_rename(&de.to_string_lossy(), &p.to_string_lossy()).map(PathBuf::from));
            let vers = match vers {
                Ok(v) => v,
                Err(e) => {
                    ligne.probleme = Some(e);
                    lignes.push(ligne);
                    continue;
                }
            };
            let (dn, vn) = textes_renommage(&de, &vers);
            ligne.de_nom = dn;
            ligne.vers_nom = vn;
            ligne.vers = vers.display().to_string();
            if vers == de {
                ligne.inchange = true;
                lignes.push(ligne);
                continue;
            }
            let mut probleme = files::check_path(&vers).err();
            if probleme.is_none() && !de.is_file() {
                probleme = Some("fichier introuvable".into());
            }
            if probleme.is_none() && vers.exists() && !meme_chemin(&de, &vers) {
                probleme = Some("la destination existe déjà".into());
            }
            let k = vers.to_string_lossy().to_lowercase();
            if probleme.is_none() {
                if let Some(autre) = destinations.get(&k) {
                    probleme = Some(format!("même destination que « {autre} »"));
                }
            }
            if probleme.is_none() {
                destinations.insert(k, b.caption());
            }
            ligne.probleme = probleme;
            lignes.push(ligne);
        }
        lignes
    }

    /// Renomme / déplace les fichiers d'après un modèle (aperçu d'abord).
    pub fn renommer_deplacer(&self, d: &DemandeRenommage, sortie: Sortie) -> Result<ResultatRenommage, String> {
        let _occupe = self.occuper()?;
        let cible = self.cible();
        if let Some(m) = cible.lecture_seule() {
            return Err(m.to_string());
        }
        let modele = d.modele.trim();
        if modele.is_empty() {
            return Err("modèle de nom vide".into());
        }
        let dossier = d.dossier_cible.as_deref().map(|s| s.trim().trim_matches('"').trim()).filter(|s| !s.is_empty());
        if let Some(dos) = dossier {
            let p = Path::new(dos);
            if !p.is_absolute() && !dos.starts_with("\\\\") && dos.as_bytes().get(1) != Some(&b':') {
                return Err(format!("dossier cible « {dos} » : chemin complet attendu"));
            }
        }
        let base = self.biblio.base()?;
        let livres = self.livres(&base, &d.ids);
        if livres.is_empty() {
            return Err("aucun livre connu dans la sélection".into());
        }
        let lignes = self.lignes_renommage(&livres, modele, dossier.map(Path::new));
        let a_renommer = lignes.iter().filter(|l| l.applicable()).count();
        if d.apercu {
            return Ok(ResultatRenommage { lignes, a_renommer, bilan: None });
        }
        if a_renommer > SEUIL_CONFIRMATION && !d.confirme {
            let b = confirmation(Operation::Renommage, a_renommer);
            return Ok(ResultatRenommage { lignes, a_renommer, bilan: Some(b) });
        }
        self.memoriser(ReglagesEdition {
            modele_nom: Some(modele.to_string()),
            dossier_cible: Some(dossier.unwrap_or("").to_string()),
            ..Default::default()
        });
        let mut store = match &cible {
            Cible::Store(cfg) => {
                Some(Store::open_existing(cfg).map_err(|e| format!("base ComicRust inaccessible : {e}"))?)
            }
            _ => None,
        };
        let mut res = Resultats::default();
        let mut fraiches = Vec::new();
        let mut avance = Avance { sortie, operation: Operation::Renommage, total: a_renommer, derniere: None };
        let mut fait = 0;
        let mut interrompu = false;
        for (b, l) in livres.iter().zip(&lignes) {
            if !l.applicable() {
                let (statut, message) = match &l.probleme {
                    Some(p) => (Statut::Erreur, Some(p.clone())),
                    None => (Statut::Inchange, Some("déjà à cet emplacement".into())),
                };
                res.noter(resultat(b, statut, message));
                continue;
            }
            // Arrêt entre deux livres seulement : un fichier déplacé a toujours sa fiche à jour.
            if self.annule.load(Ordering::SeqCst) {
                interrompu = true;
                res.noter(resultat(b, Statut::Ignore, Some(NON_FAIT_ARRET.into())));
                continue;
            }
            avance.noter(fait, &b.caption());
            let cible_ecriture = match (&mut store, &cible) {
                (Some(s), _) => EditTarget::Store(s),
                (None, Cible::Comicrack(cfg)) => EditTarget::ComicRack(cfg),
                _ => EditTarget::ReadOnly,
            };
            match edit::rename_or_move(cible_ecriture, b, Path::new(&l.vers)) {
                Ok(m) => {
                    self.lecteur.oublier_fichier(Path::new(&l.de));
                    let mut r = resultat(b, Statut::Ok, None);
                    r.fichier = m.book.file.clone();
                    res.noter(r);
                    fraiches.push(m.book);
                }
                Err(e) => res.noter(resultat(b, Statut::Erreur, Some(format!("renommage refusé — {e}")))),
            }
            fait += 1;
        }
        avance.noter(a_renommer, "");
        let modifies = self.biblio.remplacer_fiches(&fraiches, false);
        self.signaler(sortie, modifies);
        let b = bilan(Operation::Renommage, res.livres(), interrompu, Some(format!("Modèle : {modele}")));
        Ok(ResultatRenommage { lignes, a_renommer, bilan: Some(b) })
    }
}

// ───────────────────────────── Ouvrir le dossier du fichier ─────────────────────────────

/// Commande système qui montre un fichier dans son dossier. Sous Windows, les arguments sont
/// passés **tels quels** (`raw_arg`) : `explorer.exe` lit lui-même `/select,"chemin"`, que la
/// mise entre guillemets automatique de Rust rendrait illisible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandeDossier {
    pub programme: String,
    pub arguments: Vec<String>,
    pub brut: bool,
}

/// Système visé par [`commande_ouvrir_dossier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Systeme {
    Windows,
    MacOs,
    Autre,
}

impl Systeme {
    pub fn courant() -> Self {
        if cfg!(windows) {
            Systeme::Windows
        } else if cfg!(target_os = "macos") {
            Systeme::MacOs
        } else {
            Systeme::Autre
        }
    }
}

/// Commande qui montre `chemin` : fichier présent, sélectionné dans l'Explorateur
/// (`explorer.exe /select,"chemin"`) ou le Finder (`open -R`) ; ailleurs (ou fichier absent),
/// ouverture du dossier (`xdg-open`, `explorer.exe "dossier"`, `open`).
pub fn commande_ouvrir_dossier(chemin: &Path, fichier_present: bool, systeme: Systeme) -> CommandeDossier {
    let dossier = chemin.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(chemin);
    match systeme {
        Systeme::Windows => {
            // L'Explorateur ne comprend que les « \ » (chemins écrits avec « / » dans la fiche).
            let w = |p: &Path| p.to_string_lossy().replace('/', "\\");
            let arg =
                if fichier_present { format!("/select,\"{}\"", w(chemin)) } else { format!("\"{}\"", w(dossier)) };
            CommandeDossier { programme: "explorer.exe".into(), arguments: vec![arg], brut: true }
        }
        Systeme::MacOs if fichier_present => CommandeDossier {
            programme: "open".into(),
            arguments: vec!["-R".into(), chemin.to_string_lossy().into_owned()],
            brut: false,
        },
        Systeme::MacOs => CommandeDossier {
            programme: "open".into(),
            arguments: vec![dossier.to_string_lossy().into_owned()],
            brut: false,
        },
        Systeme::Autre => CommandeDossier {
            programme: "xdg-open".into(),
            arguments: vec![dossier.to_string_lossy().into_owned()],
            brut: false,
        },
    }
}

/// Lance la commande sans l'attendre (l'Explorateur rend 1 même quand tout va bien).
pub fn lancer_commande(c: &CommandeDossier) -> Result<(), String> {
    let mut cmd = std::process::Command::new(&c.programme);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if c.brut {
            for a in &c.arguments {
                cmd.raw_arg(a);
            }
        } else {
            cmd.args(&c.arguments);
        }
    }
    #[cfg(not(windows))]
    cmd.args(&c.arguments);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    let mut enfant = cmd.spawn().map_err(|e| format!("{} introuvable ou refusé : {e}", c.programme))?;
    // Le processus est récupéré en arrière-plan (pas de processus zombie sous Linux).
    std::thread::spawn(move || {
        let _ = enfant.wait();
    });
    Ok(())
}

impl Edition {
    /// Commande qui montre le fichier du livre `id` dans son dossier (rien n'est lancé).
    /// Erreur lisible : livre inconnu, sans fichier, dossier introuvable (disque débranché,
    /// partage réseau qui ne répond pas).
    pub fn commande_dossier_fichier(&self, id: &str) -> Result<CommandeDossier, String> {
        let b = self.biblio.livre(id).ok_or("livre inconnu")?;
        let f = b.file.as_deref().map(str::trim).filter(|f| !f.is_empty()).ok_or("ce livre n'a pas de fichier")?;
        let chemin = PathBuf::from(f);
        let present = crate::biblio::fichier_existe(chemin.clone(), Duration::from_secs(2));
        if !present {
            let dossier = chemin.parent().map(Path::to_path_buf).unwrap_or_default();
            let (tx, rx) = std::sync::mpsc::channel();
            let d = dossier.clone();
            let _ = std::thread::Builder::new().name("comicrust-dossier".into()).spawn(move || {
                let _ = tx.send(d.is_dir());
            });
            if !rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false) {
                return Err(format!("dossier introuvable : {}", dossier.display()));
            }
        }
        Ok(commande_ouvrir_dossier(&chemin, present, Systeme::courant()))
    }

    /// « Ouvrir le dossier du fichier » : Explorateur de Windows, le fichier sélectionné.
    pub fn ouvrir_dossier_fichier(&self, id: &str) -> Result<(), String> {
        lancer_commande(&self.commande_dossier_fichier(id)?)
    }
}

/// Ajoute une erreur aux résultats des livres `ids`.
fn ajouter_erreur(out: &mut [ResultatLivre], ids: &[String], message: &str) {
    for r in out.iter_mut().filter(|r| ids.iter().any(|i| i.eq_ignore_ascii_case(&r.id))) {
        r.statut = Statut::Erreur;
        r.message = Some(match r.message.take() {
            Some(m) => format!("{m} ; {message}"),
            None => message.to_string(),
        });
    }
}

/// Résultats par livre du bilan des fiches (`EditReport`).
fn noter_rapport_fiches(res: &mut Resultats, livres: &[LibraryBook], r: &EditReport) {
    let a = |ids: &[String], id: &str| ids.iter().any(|i| i.eq_ignore_ascii_case(id));
    for b in livres {
        let erreurs: Vec<String> = r
            .errors
            .iter()
            .filter(|e| e.book_id.eq_ignore_ascii_case(&b.id))
            .map(|e| {
                let champ = edit::field(&e.field).map(|f| format!("{} : ", f.label)).unwrap_or_default();
                format!("{champ}{}", e.message)
            })
            .collect();
        let x = if !erreurs.is_empty() {
            resultat(b, Statut::Erreur, Some(erreurs.join(" ; ")))
        } else if a(&r.conflicts, &b.id) {
            resultat(b, Statut::Erreur, Some("fiche modifiée ailleurs entre-temps (ComicRack) : non écrite".into()))
        } else if a(&r.missing, &b.id) {
            resultat(b, Statut::Erreur, Some("livre absent de la base".into()))
        } else if r.changed.iter().any(|c| c.id.eq_ignore_ascii_case(&b.id)) {
            resultat(b, Statut::Ok, None)
        } else {
            resultat(b, Statut::Inchange, None)
        };
        res.noter(x);
    }
}

#[cfg(test)]
#[path = "tests_edition.rs"]
mod tests;
