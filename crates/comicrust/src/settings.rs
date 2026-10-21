//! Réglages conservés entre deux lancements (stockés par eframe).

use std::collections::HashMap;
use std::path::PathBuf;

use comicrust_core::db::DbConfig;

use crate::library::{GroupKey, SortKey, ViewOptions};
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

/// Page de reprise quand ComicRack et ComicRust ont mémorisé des pages différentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ResumeChoice {
    #[default]
    Ask,
    ComicRack,
    ComicRust,
}

impl ResumeChoice {
    pub fn label(self) -> &'static str {
        match self {
            ResumeChoice::Ask => "Demander",
            ResumeChoice::ComicRack => "Page de ComicRack",
            ResumeChoice::ComicRust => "Page de ComicRust",
        }
    }
}

/// Où trouver la base MySQL de ComicRack.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum DbSource {
    /// Pas de base : la vue Bibliothèque propose de la configurer.
    #[default]
    None,
    /// Paramètres relus à chaque connexion dans ce `ComicRack.ini` (rien n'est recopié).
    ComicRackIni(PathBuf),
    /// Paramètres saisis dans ComicRust (gardés dans `data\comicrust.ron`).
    Manual(DbConfig),
}

impl DbSource {
    pub fn config(&self) -> Result<Option<DbConfig>, String> {
        match self {
            DbSource::None => Ok(None),
            DbSource::ComicRackIni(p) => DbConfig::from_comicrack_ini(p).map(Some).map_err(|e| e.to_string()),
            DbSource::Manual(c) => Ok(Some(c.clone())),
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
    /// Thème de l'interface.
    pub theme: crate::ui::theme::Theme,
    pub recent: Vec<PathBuf>,
    /// Dernière page lue par fichier (équivalent de `CurrentPage` dans ComicRack).
    pub last_pages: HashMap<PathBuf, usize>,
    /// `ComicDB.xml` de ComicRack choisi à la main : listes de la bibliothèque et
    /// résolution des listes `.cbl`. Sans lui, celui du `ComicRack.ini` est cherché.
    pub library_db: Option<PathBuf>,
    /// Dossier racine des BD, parcouru pour résoudre les listes `.cbl`.
    pub comics_root: Option<PathBuf>,
    pub last_dir: Option<PathBuf>,
    /// Base MySQL de ComicRack (vue Bibliothèque, listes par identifiant).
    pub db_source: DbSource,
    /// Bibliothèque : tri, sens et regroupement.
    pub library_sort: SortKey,
    pub library_descending: bool,
    pub library_group: GroupKey,
    /// Bibliothèque : vue en tableau plutôt qu'en couvertures.
    pub library_table: bool,
    /// Panneaux de la bibliothèque : listes ComicRack (gauche) et détails (droite).
    pub show_comic_lists: bool,
    pub show_details: bool,
    /// Relire automatiquement les modifications faites dans ComicRack.
    pub auto_refresh: bool,
    /// Reprise de lecture quand ComicRack et ComicRust divergent.
    pub resume: ResumeChoice,
    /// Bibliothèque : hauteur des couvertures (points).
    pub library_cover_height: f32,
    /// Vérifier les nouvelles versions au démarrage.
    pub check_updates: bool,
    /// Version dont la mise à jour a été refusée (« Ignorer cette version »).
    pub skipped_version: Option<String>,
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
            theme: crate::ui::theme::Theme::Dark,
            recent: Vec::new(),
            last_pages: HashMap::new(),
            library_db: None,
            comics_root: None,
            last_dir: None,
            db_source: DbSource::None,
            library_sort: SortKey::Series,
            library_descending: false,
            library_group: GroupKey::Series,
            library_table: false,
            show_comic_lists: true,
            show_details: true,
            auto_refresh: true,
            resume: ResumeChoice::Ask,
            library_cover_height: 190.0,
            check_updates: true,
            skipped_version: None,
        }
    }
}

impl Settings {
    pub const MAX_RECENT: usize = 15;

    pub fn view_options(&self) -> ViewOptions {
        ViewOptions { sort: self.library_sort, descending: self.library_descending, group: self.library_group }
    }

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(Self::MAX_RECENT);
    }
}
