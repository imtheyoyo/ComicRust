//! Lot L7 « Réglages, aide et configuration » (`docs/tauri/ipc-reglages-aide.md`) : logique sans
//! Tauri, testée partout.
//!
//! - **À propos / aide** : [`collecter`] (version, système, mode, chemins, état de PDFium) et
//!   [`texte_diagnostic`] (le texte que l'utilisateur copie pour un rapport de recette). **Aucun
//!   secret** n'y figure : ni mot de passe, ni identifiant, ni serveur MySQL (seulement « MySQL »).
//! - **Assistant rejouable** : [`configuration`] décrit la bibliothèque actuelle pour préremplir
//!   l'assistant en mode « modification ». Le changement lui-même passe par `Biblio::configurer*`
//!   (la base courante n'est jamais supprimée ; un choix qui échoue n'est pas enregistré).
//! - **Copier les livres** : [`rapport_livres`] (comparaison avec ComicRack, `shown_books_report` de
//!   la version egui).

use crate::biblio::{Biblio, RequeteLivres};
use crate::reglages::{self, Mode};
use crate::{portable, InfosApplication};
use comicrust_core::store::SQLITE_FILE_NAME;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Dépôt du projet (lien de la fenêtre « À propos » ; ouvert par `maj_ouvrir_page`).
pub const DEPOT: &str = "https://github.com/imtheyoyo/ComicRust/";

/// État de PDFium (lecture des PDF) : chargé ou non, et où il a été trouvé.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatPdfium {
    pub charge: bool,
    /// Fichier de la bibliothèque trouvé aux emplacements habituels (absent : bibliothèque du système
    /// ou introuvable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chemin: Option<String>,
    /// Raison de l'échec (PDFium indisponible).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erreur: Option<String>,
}

/// Nom de la bibliothèque PDFium sur ce système (même règle que `pdfium-render`).
pub(crate) const fn nom_bibliotheque_pdfium() -> &'static str {
    if cfg!(windows) {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

/// Emplacements où le cœur cherche PDFium, dans le même ordre (`PDFIUM_DYNAMIC_LIB_PATH`, `lib\` puis
/// dossier de l'exécutable, dossier courant).
fn dossiers_pdfium() -> Vec<PathBuf> {
    let mut dossiers = Vec::new();
    if let Some(p) = std::env::var_os("PDFIUM_DYNAMIC_LIB_PATH") {
        dossiers.push(PathBuf::from(p));
    }
    let exe = portable::exe_dir();
    dossiers.push(exe.join("lib"));
    dossiers.push(exe);
    if let Ok(cwd) = std::env::current_dir() {
        dossiers.push(cwd);
    }
    dossiers
}

/// Premier fichier PDFium présent dans `dossiers`.
pub(crate) fn chemin_pdfium_dans(dossiers: &[PathBuf]) -> Option<PathBuf> {
    dossiers.iter().map(|d| d.join(nom_bibliotheque_pdfium())).find(|p| p.is_file())
}

/// État de PDFium : chargement (idempotent : le cœur ne le charge qu'une fois) et chemin trouvé.
pub fn etat_pdfium() -> EtatPdfium {
    let chemin = chemin_pdfium_dans(&dossiers_pdfium()).map(|p| p.display().to_string());
    match comicrust_core::providers::pdfium_status() {
        Ok(()) => EtatPdfium { charge: true, chemin, erreur: None },
        Err(e) => EtatPdfium { charge: false, chemin, erreur: Some(e) },
    }
}

/// Ce que la session sait de la bibliothèque ouverte (fourni par la commande).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionDiagnostic {
    /// `None` : aucune bibliothèque ouverte (premier démarrage, erreur d'ouverture).
    pub mode: Option<Mode>,
    /// `sqlite`, `mysql` ou `xml` ; vide si rien n'est ouvert.
    pub source: String,
    pub nb_livres: Option<usize>,
}

/// Informations de la fenêtre « À propos » (rien de secret).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfosDiagnostic {
    pub version: String,
    /// « windows x86_64 », « linux x86_64 »…
    pub systeme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nb_livres: Option<usize>,
    pub dossier_exe: String,
    pub dossier_data: String,
    pub dossier_themes: String,
    pub dossier_webview: String,
    /// Fichier ComicRack.ini / ComicDb.xml choisi (mode lié), s'il y en a un.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chemin_ini: Option<String>,
    /// Fichier de la base SQLite de ComicRust (mode autonome) et sa présence.
    pub base_sqlite: String,
    pub base_sqlite_presente: bool,
    /// La base de ComicRust est MySQL (serveur, identifiants et mot de passe : jamais donnés).
    pub store_mysql: bool,
    /// Le mode lié passe par une connexion MySQL saisie à la main (jamais donnée).
    pub comicrack_mysql: bool,
    pub pdfium: EtatPdfium,
    pub depot: String,
}

/// Rassemble les informations ; un fichier de réglages illisible n'empêche pas le diagnostic.
pub fn collecter(data: &Path, session: &SessionDiagnostic, pdfium: EtatPdfium) -> InfosDiagnostic {
    let base = InfosApplication {
        version: env!("CARGO_PKG_VERSION").to_string(),
        dossier_data: data.display().to_string(),
        dossier_themes: portable::dossier_themes(data).display().to_string(),
        dossier_webview: portable::dossier_webview(data).display().to_string(),
    };
    let r = reglages::lire(data).unwrap_or_default();
    let sqlite = data.join(SQLITE_FILE_NAME);
    InfosDiagnostic {
        version: base.version,
        systeme: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        mode: session.mode,
        source: session.source.clone(),
        nb_livres: session.nb_livres,
        dossier_exe: portable::exe_dir().display().to_string(),
        dossier_data: base.dossier_data,
        dossier_themes: base.dossier_themes,
        dossier_webview: base.dossier_webview,
        chemin_ini: r.chemin_ini.map(|p| p.display().to_string()),
        base_sqlite_presente: sqlite.is_file(),
        base_sqlite: sqlite.display().to_string(),
        store_mysql: r.store_mysql.is_some(),
        comicrack_mysql: r.comicrack_mysql.is_some(),
        pdfium,
        depot: DEPOT.to_string(),
    }
}

fn libelle_mode(m: Option<Mode>) -> &'static str {
    match m {
        Some(Mode::Autonome) => "autonome",
        Some(Mode::Comicrack) => "lié à ComicRack",
        None => "aucune bibliothèque ouverte",
    }
}

/// Texte à copier pour un rapport (recette, signalement) : version, système, mode, chemins, état de
/// PDFium. **Sans secret** : ni mot de passe, ni identifiant, ni nom de serveur MySQL.
pub fn texte_diagnostic(i: &InfosDiagnostic) -> String {
    let mut t = String::new();
    t.push_str(&format!("ComicRust (Tauri) v{} — informations de diagnostic\n", i.version));
    t.push_str(&format!("Système : {}\n", i.systeme));
    t.push_str(&format!("Mode : {}\n", libelle_mode(i.mode)));
    if !i.source.is_empty() {
        t.push_str(&format!("Source des livres : {}\n", i.source));
    }
    if let Some(n) = i.nb_livres {
        t.push_str(&format!("Livres : {n}\n"));
    }
    if i.store_mysql {
        t.push_str("Base de ComicRust : MySQL (paramètres non inclus)\n");
    } else {
        t.push_str(&format!(
            "Base de ComicRust : SQLite {} ({})\n",
            i.base_sqlite,
            if i.base_sqlite_presente { "présente" } else { "absente" }
        ));
    }
    if let Some(p) = &i.chemin_ini {
        t.push_str(&format!("Fichier ComicRack : {p}\n"));
    }
    if i.comicrack_mysql {
        t.push_str("Connexion MySQL de ComicRack saisie à la main (paramètres non inclus)\n");
    }
    t.push_str(&format!("Dossier de l'application : {}\n", i.dossier_exe));
    t.push_str(&format!("Dossier data : {}\n", i.dossier_data));
    t.push_str(&format!("Dossier des thèmes : {}\n", i.dossier_themes));
    t.push_str(&format!("Profil WebView2 : {}\n", i.dossier_webview));
    match (&i.pdfium.charge, &i.pdfium.chemin, &i.pdfium.erreur) {
        (true, Some(c), _) => t.push_str(&format!("PDFium : chargé ({c})\n")),
        (true, None, _) => t.push_str("PDFium : chargé (bibliothèque du système)\n"),
        (false, _, e) => t.push_str(&format!("PDFium : non chargé — {}\n", e.as_deref().unwrap_or("raison inconnue"))),
    }
    t.push_str(&format!("Dépôt : {}\n", i.depot));
    t
}

// ───────────────────────────── Assistant rejouable ─────────────────────────────

/// Bibliothèque actuelle, pour préremplir l'assistant en mode « modification ». Les connexions MySQL
/// (sans mot de passe) viennent de `lire_connexions_mysql`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationBiblio {
    /// `None` : rien n'est encore choisi.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    /// Mode lié : fichier `ComicRack.ini` ou `ComicDb.xml` choisi.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chemin_ini: Option<String>,
    /// Mode autonome : la base est MySQL (sinon SQLite `data\comicrust.db`).
    pub store_mysql: bool,
    /// Mode lié par une connexion MySQL saisie (sans `ComicRack.ini`).
    pub comicrack_mysql: bool,
    /// Fichier SQLite de ComicRust : chemin, présence et taille (octets). Jamais supprimé par un
    /// changement de bibliothèque.
    pub base_sqlite: String,
    pub base_sqlite_presente: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_sqlite_taille: Option<u64>,
}

/// Description de la bibliothèque enregistrée (lecture seule ; ne crée rien).
pub fn configuration(data: &Path) -> Result<ConfigurationBiblio, String> {
    let r = reglages::lire(data)?;
    let sqlite = data.join(SQLITE_FILE_NAME);
    let meta = std::fs::metadata(&sqlite).ok().filter(|m| m.is_file());
    Ok(ConfigurationBiblio {
        mode: r.mode,
        chemin_ini: r.chemin_ini.map(|p| p.display().to_string()),
        store_mysql: r.store_mysql.is_some(),
        comicrack_mysql: r.comicrack_mysql.is_some(),
        base_sqlite: sqlite.display().to_string(),
        base_sqlite_presente: meta.is_some(),
        base_sqlite_taille: meta.map(|m| m.len()),
    })
}

// ───────────────────────────── Copier les livres ─────────────────────────────

/// Rapport des livres affichés (série, numéro, volume, année, fichier) pour comparer avec ComicRack :
/// même présentation que `shown_books_report` de la version egui. `titre` : nom de la liste affichée
/// (« Bibliothèque » si vide). Respecte la recherche, le tri et la pile ouverte de la requête.
pub fn rapport_livres(biblio: &Biblio, requete: &RequeteLivres, titre: &str) -> Result<String, String> {
    let (base, vue) = biblio.vue(requete)?;
    let titre = if titre.trim().is_empty() { "Bibliothèque" } else { titre.trim() };
    let mut sortie = format!("{titre} — {} livres (ComicRust v{})\n\n", vue.indices.len(), env!("CARGO_PKG_VERSION"));
    for &i in &vue.indices {
        let b = &base.books[i];
        sortie.push_str(&format!(
            "{} #{} (v{}, {}) — {}\n",
            b.shadow_series(),
            b.shadow_number(),
            b.shadow_volume(),
            b.shadow_year(),
            b.file.as_deref().unwrap_or("")
        ));
    }
    Ok(sortie)
}
