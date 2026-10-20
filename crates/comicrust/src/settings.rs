//! Réglages conservés entre deux lancements (stockés par eframe).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FitMode {
    /// Page entière visible.
    Page,
    /// Largeur de la fenêtre.
    Width,
    /// Hauteur de la fenêtre.
    Height,
    /// Taille réelle (100 %).
    Original,
}

impl FitMode {
    pub const ALL: [FitMode; 4] = [FitMode::Page, FitMode::Width, FitMode::Height, FitMode::Original];

    pub fn label(self) -> &'static str {
        match self {
            FitMode::Page => "Page entière",
            FitMode::Width => "Largeur",
            FitMode::Height => "Hauteur",
            FitMode::Original => "Taille réelle",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub fit: FitMode,
    pub double_page: bool,
    /// En double page, la couverture reste seule (comme un livre ouvert).
    pub cover_alone: bool,
    /// Lecture de droite à gauche (manga).
    pub manga: bool,
    pub show_thumbnails: bool,
    pub show_status_bar: bool,
    pub background: [u8; 3],
    pub recent: Vec<PathBuf>,
    /// Dernière page lue par fichier (équivalent de `CurrentPage` dans ComicRack).
    pub last_pages: HashMap<PathBuf, usize>,
    /// Base ComicRack (`ComicDB.xml`) pour résoudre les listes `.cbl`.
    pub library_db: Option<PathBuf>,
    /// Dossier racine des BD, parcouru pour résoudre les listes `.cbl`.
    pub comics_root: Option<PathBuf>,
    pub last_dir: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fit: FitMode::Page,
            double_page: false,
            cover_alone: true,
            manga: false,
            show_thumbnails: true,
            show_status_bar: true,
            background: [24, 24, 26],
            recent: Vec::new(),
            last_pages: HashMap::new(),
            library_db: None,
            comics_root: None,
            last_dir: None,
        }
    }
}

impl Settings {
    pub const MAX_RECENT: usize = 15;

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(Self::MAX_RECENT);
    }
}
