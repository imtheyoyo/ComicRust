//! Livre ouvert dans le lecteur : état des pages, textures et vignettes.

pub mod loader;
mod navigation;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use comicrust_core::{BookFormat, ComicInfo};
use eframe::egui::{self, TextureHandle, TextureOptions};

use loader::BookLoader;

/// Pages gardées en mémoire graphique autour de la page courante.
pub const KEEP_BEHIND: usize = 3;
pub const KEEP_AHEAD: usize = 6;
pub const PRELOAD_AHEAD: usize = 3;

pub fn texture_options() -> TextureOptions {
    TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR }
}

pub struct Book {
    pub path: PathBuf,
    pub title: String,
    pub loader: BookLoader,
    pub format: Option<BookFormat>,
    /// Noms des images, dans l'ordre de lecture (pages supprimées masquées).
    pub pages: Vec<String>,
    /// Numéro de page du livre (ComicRack : rang dans la liste complète des pages) de
    /// chaque page lue ; `current` est un rang dans `pages`.
    pub numbers: Vec<usize>,
    /// Nombre d'images du fichier.
    pub total: usize,
    pub info: Option<ComicInfo>,
    /// Taille d'origine des pages déjà décodées.
    pub sizes: HashMap<usize, [u32; 2]>,
    pub textures: HashMap<usize, TextureHandle>,
    pub errors: HashMap<usize, String>,
    pub requested: HashSet<usize>,
    pub thumbs: HashMap<usize, Option<TextureHandle>>,
    pub thumbs_requested: HashSet<usize>,
    pub current: usize,
    pub opened: bool,
    /// Lecture déjà enregistrée (base de ComicRust en mode autonome, base de ComicRack en
    /// mode écriture) : une fois par livre ouvert.
    pub stored: bool,
    /// Heure d'ouverture (`Opened` de ComicRack, qui la note à l'ouverture du livre).
    pub opened_at: chrono::DateTime<chrono::FixedOffset>,
}

impl Book {
    pub fn new(path: PathBuf, loader: BookLoader, current: usize) -> Self {
        let title = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        Self {
            path,
            title,
            loader,
            format: None,
            pages: Vec::new(),
            numbers: Vec::new(),
            total: 0,
            info: None,
            sizes: HashMap::new(),
            textures: HashMap::new(),
            errors: HashMap::new(),
            requested: HashSet::new(),
            thumbs: HashMap::new(),
            thumbs_requested: HashSet::new(),
            current,
            opened: false,
            stored: false,
            opened_at: comicrust_core::db::write::now(),
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Numéro de page du livre (ComicRack) du rang de lecture `pos`.
    pub fn number(&self, pos: usize) -> usize {
        self.numbers.get(pos).copied().unwrap_or(pos)
    }

    /// Rang de lecture de la page n° `number` du livre (la suivante visible si elle est
    /// supprimée).
    pub fn position_of(&self, number: usize) -> usize {
        if self.numbers.is_empty() {
            return number;
        }
        self.numbers.iter().position(|&n| n >= number).unwrap_or(self.numbers.len() - 1)
    }
}
