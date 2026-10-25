//! Hôte de scripts Python (v0.10.0).
//!
//! Inspiré du système de scripts de ComicRack (`ScriptUtility`, `PythonPluginInitializer`,
//! en-têtes `#@Name`, `#@Hook`…), mais avec de **nouveaux** scripts Python 3 : aucune
//! compatibilité avec les scripts IronPython de ComicRack.
//!
//! # Architecture
//!
//! - [`discover`] : liste les points d'entrée des fichiers `.py` du dossier `scripts\` en
//!   **lisant leur texte** (aucun code Python n'est exécuté) ; voir [`discover`](mod@discover).
//! - [`ScriptRunner::start`] : lance une fonction d'un script dans un **processus Python
//!   séparé** (un script qui plante ou boucle ne fait pas tomber ComicRust), sur un fil
//!   dédié ; [`RunHandle`] permet de suivre (`poll`), d'attendre (`wait`) ou d'annuler
//!   (`cancel` : demande polie, puis arrêt forcé du processus après un délai de grâce).
//! - Protocole ([`protocol`]) : JSON par lignes sur stdin (ComicRust → script) et sur la
//!   sortie standard d'origine (script → ComicRust). Le module Python redirige `sys.stdout`
//!   (et le descripteur 1) vers stderr : un `print()` va au **journal** sans casser le
//!   protocole. stderr = journal du script.
//! - Le script ne modifie jamais lui-même la base ni les fichiers : il **demande** des
//!   modifications ([`ScriptChange`]) que ComicRust vérifie (livre du contexte, champ connu)
//!   puis rend dans le [`RunResult`] (aperçu avant application pour un script lancé du menu)
//!   ou transmet au fil de l'eau à [`ScriptHost::apply_live`] (automatismes).
//! - Module Python `comicrust` : fichier `comicrust.py` embarqué dans le binaire
//!   (`include_str!`), écrit au lancement dans un dossier de travail (par défaut
//!   `<temp>/comicrust-py-<version>`, voir [`RunOptions::module_dir`]) et ajouté à
//!   `sys.path` par la ligne de commande (`-c`), car le Python « embeddable » de Windows
//!   ignore `PYTHONPATH` (fichier `python3xx._pth`).
//!
//! # Interpréteur
//!
//! Voir [`find_python`] : `COMICRUST_PYTHON`, puis `python\python.exe` à côté de
//! l'exécutable (Python embarqué du zip Windows), puis, hors Windows, `python3` du système.

mod discover;
pub mod protocol;
mod runner;

#[cfg(test)]
mod tests;

pub use discover::{discover, discover_with_warnings, entries_for_hook, parse_script, DiscoveryWarning, ScriptEntry};
pub use runner::{
    book_json, BookChanges, JournalKind, JournalLine, Question, QuestionKind, RejectedChange, RunContext, RunHandle,
    RunOptions, RunResult, RunState, ScriptChange, ScriptFailure, ScriptHost, ScriptRunner, ScriptSettings,
};

use std::path::{Path, PathBuf};

/// Texte du module Python `comicrust` fourni aux scripts.
pub const PYTHON_MODULE: &str = include_str!("comicrust.py");

/// Version du protocole échangée au démarrage (`hello` du script).
pub const PROTOCOL_VERSION: u32 = 1;

/// Moments où ComicRust appelle un script (en-tête `#@Hook`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Hook {
    /// Menu « Scripts » sur la sélection : `fonction(books)`.
    Books,
    /// Menu « Scripts » sur toute la bibliothèque : `fonction(books)`.
    Library,
    /// Livre ouvert dans le lecteur : `fonction(book)`.
    BookOpened,
    /// Livre fermé : `fonction(book)`.
    BookClosed,
    /// Après une analyse qui a ajouté des livres : `fonction(books)`.
    NewBooks,
    /// Au démarrage de ComicRust : `fonction()`.
    Startup,
    /// Bouton « Réglages » du script : `fonction()`.
    ConfigScript,
}

impl Hook {
    pub const ALL: [Hook; 7] = [
        Hook::Books,
        Hook::Library,
        Hook::BookOpened,
        Hook::BookClosed,
        Hook::NewBooks,
        Hook::Startup,
        Hook::ConfigScript,
    ];

    /// Nom tel qu'écrit dans `#@Hook`.
    pub fn name(self) -> &'static str {
        match self {
            Hook::Books => "Books",
            Hook::Library => "Library",
            Hook::BookOpened => "BookOpened",
            Hook::BookClosed => "BookClosed",
            Hook::NewBooks => "NewBooks",
            Hook::Startup => "Startup",
            Hook::ConfigScript => "ConfigScript",
        }
    }

    /// Nom d'en-tête → hook (casse ignorée, comme les attributs de ComicRack).
    pub fn parse(s: &str) -> Option<Hook> {
        Hook::ALL.into_iter().find(|h| h.name().eq_ignore_ascii_case(s.trim()))
    }

    /// Hook déclenché sans action de l'utilisateur (automatisme) : délai maximum conseillé,
    /// pas de question à l'utilisateur.
    pub fn is_automatic(self) -> bool {
        matches!(self, Hook::BookOpened | Hook::BookClosed | Hook::NewBooks | Hook::Startup)
    }

    /// Libellé français pour l'interface.
    pub fn description(self) -> &'static str {
        match self {
            Hook::Books => "Livres sélectionnés",
            Hook::Library => "Toute la bibliothèque",
            Hook::BookOpened => "À l'ouverture d'un livre",
            Hook::BookClosed => "À la fermeture d'un livre",
            Hook::NewBooks => "Nouveaux livres (après une analyse)",
            Hook::Startup => "Au démarrage",
            Hook::ConfigScript => "Réglages du script",
        }
    }
}

/// Champs de fiche qu'un script peut modifier (noms ComicRack ; en Python, aussi en
/// snake_case : `book.series`, `book.language_iso`…).
pub const EDITABLE_FIELDS: &[&str] = &[
    "Series",
    "Number",
    "Count",
    "Volume",
    "Year",
    "Month",
    "Day",
    "Title",
    "Publisher",
    "Imprint",
    "Writer",
    "Penciller",
    "Inker",
    "Colorist",
    "Letterer",
    "CoverArtist",
    "Editor",
    "Translator",
    "Genre",
    "StoryArc",
    "SeriesGroup",
    "AlternateSeries",
    "AlternateNumber",
    "AlternateCount",
    "Summary",
    "Notes",
    "Review",
    "Web",
    "LanguageISO",
    "Format",
    "AgeRating",
    "BlackAndWhite",
    "Manga",
    "Characters",
    "Teams",
    "MainCharacterOrTeam",
    "Locations",
    "Tags",
    "Rating",
    "CommunityRating",
    "BookAge",
    "BookCollectionStatus",
    "BookCondition",
    "BookLocation",
    "BookNotes",
    "BookOwner",
    "BookStore",
    "BookPrice",
    "ISBN",
    "SeriesComplete",
    "ScanInformation",
    "Released",
];

/// Champs lisibles mais non modifiables par un script.
pub const READ_ONLY_FIELDS: &[&str] = &[
    "Id",
    "File",
    "FileSize",
    "FileModifiedTime",
    "FileCreationTime",
    "PageCount",
    "CurrentPage",
    "LastPageRead",
    "Added",
    "Opened",
    "OpenCount",
    "Missing",
    "Checked",
    "ComicInfoIsDirty",
    "ComicBookIsDirty",
    "EnableProposed",
    "PreferredFrontCover",
    "CustomValuesStore",
    "CustomThumbnailKey",
    "LastOpenedFromListId",
    "NewPages",
];

/// Erreurs de mise en route (avant l'exécution du script proprement dite).
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error(
        "Python introuvable : ni la variable COMICRUST_PYTHON, ni le Python embarqué ({0}). \
         Réinstallez ComicRust (dossier « python » à côté de ComicRust.exe) ou indiquez un \
         interpréteur dans COMICRUST_PYTHON."
    )]
    PythonNotFound(String),
    #[error("COMICRUST_PYTHON désigne un fichier absent : {0}")]
    BadPythonVariable(String),
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
}

/// Chemin du Python embarqué à côté de l'exécutable.
fn embedded_python(exe_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        exe_dir.join("python").join("python.exe")
    } else {
        exe_dir.join("python").join("bin").join("python3")
    }
}

/// Recherche un exécutable dans le `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

/// Trouve l'interpréteur Python, dans l'ordre :
/// 1. variable d'environnement `COMICRUST_PYTHON` ;
/// 2. `python\python.exe` à côté de l'exécutable (`exe_dir`, par défaut le dossier de
///    l'exécutable courant) — la distribution « embeddable » livrée dans le zip Windows ;
/// 3. hors Windows : `python3` du système (développement, tests).
pub fn find_python(exe_dir: Option<&Path>) -> Result<PathBuf, ScriptError> {
    if let Some(v) = std::env::var_os("COMICRUST_PYTHON").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(&v);
        if p.is_file() {
            return Ok(p);
        }
        // Un simple nom (« python3 ») est cherché dans le PATH.
        if p.components().count() == 1 {
            if let Some(found) = which(&p.to_string_lossy()) {
                return Ok(found);
            }
        }
        return Err(ScriptError::BadPythonVariable(p.display().to_string()));
    }
    let exe_dir = match exe_dir {
        Some(d) => Some(d.to_path_buf()),
        None => std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)),
    };
    let embedded = exe_dir.as_deref().map(embedded_python);
    if let Some(p) = embedded.as_ref().filter(|p| p.is_file()) {
        return Ok(p.clone());
    }
    if !cfg!(windows) {
        if let Some(p) = which("python3") {
            return Ok(p);
        }
    }
    Err(ScriptError::PythonNotFound(
        embedded.map(|p| p.display().to_string()).unwrap_or_else(|| "python\\python.exe".into()),
    ))
}

/// Dossier `scripts` à côté de l'exécutable (là où [`discover`] cherche les scripts).
pub fn default_scripts_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("scripts")))
}
