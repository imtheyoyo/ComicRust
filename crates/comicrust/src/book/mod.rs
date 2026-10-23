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
    pub pages: Vec<String>,
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
    /// Lecture déjà enregistrée dans la base de ComicRust (mode autonome).
    pub stored: bool,
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
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}
