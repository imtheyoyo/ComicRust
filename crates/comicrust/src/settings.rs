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

/// Présentation propre à une liste (ou à toute la bibliothèque) : tri, regroupement, piles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListView {
    pub sort: SortKey,
    pub descending: bool,
    pub group: GroupKey,
    pub stack: bool,
}

/// Clé de la présentation de toute la bibliothèque dans `Settings::list_views`.
pub const LIBRARY_VIEW_KEY: &str = "";

/// Lecture faite dans ComicRust et gardée dans ses réglages (base ComicRack en lecture
/// seule, ou écriture dans ComicRack pas encore faite / échouée).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReadProgress {
    /// Page la plus loin atteinte (à partir de 0).
    pub max_page: usize,
    pub pages: usize,
}

impl ReadProgress {
    /// Pourcentage lu, avec la formule de ComicRack (`ReadPercentage`).
    pub fn percent(&self) -> Option<u32> {
        if self.pages == 0 || self.max_page == 0 {
            return None;
        }
        Some((((self.max_page + 1) * 100 / self.pages) as u32).clamp(1, 100))
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

/// Bibliothèque utilisée (choisie au premier démarrage, modifiable par l'assistant).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryMode {
    /// Pas encore choisi : l'assistant de premier démarrage s'ouvre.
    #[default]
    Unset,
    /// ComicRust seul : sa propre base (`Settings::store`) et ses dossiers de BD.
    Standalone,
    /// Lié à ComicRack, base MySQL de ComicRack (`Settings::db_source`).
    ComicRackMySql,
    /// Lié à ComicRack, bibliothèque `ComicDb.xml` de ComicRack (`Settings::library_db`).
    ComicRackXml,
}

impl LibraryMode {
    pub fn is_comicrack(self) -> bool {
        matches!(self, LibraryMode::ComicRackMySql | LibraryMode::ComicRackXml)
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
    /// Paramètres saisis dans ComicRust (gardés dans `data\comicrust.ron`, mot de passe chiffré).
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
    /// Avancement de lecture dans ComicRust, par fichier.
    pub read_progress: HashMap<PathBuf, ReadProgress>,
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
    /// Bibliothèque : une pile par série (vue en couvertures).
    pub library_stack: bool,
    /// Présentation de chaque liste (identifiant de liste ; `""` : toute la bibliothèque).
    /// Les champs `library_sort`, `library_descending`, `library_group` et `library_stack`
    /// sont ceux de la liste affichée.
    pub list_views: HashMap<String, ListView>,
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
    /// Infos écrites par ComicRust dans le fichier : identifiant (minuscules) → compteur
    /// de la fiche en base à ce moment. Tant que la fiche n'a pas changé, le livre n'est
    /// plus « à mettre à jour » dans ComicRust (ComicRack, lui, n'est pas prévenu).
    pub info_written: HashMap<String, i64>,
    /// Livres convertis en CBZ : ancien chemin (celui de la base) → CBZ créé. Si l'ancien
    /// fichier n'existe plus, ComicRust ouvre le CBZ à sa place.
    pub converted: HashMap<PathBuf, PathBuf>,
    /// Conversion : ranger l'original dans le sous-dossier `_originaux`.
    pub move_converted_original: bool,
    /// Bibliothèque utilisée.
    pub library_mode: LibraryMode,
    /// Mode autonome : base propre à ComicRust (SQLite dans `data\` ou MySQL).
    pub store: Option<comicrust_core::store::StoreConfig>,
    /// Mode lié : fichiers de ComicRack retrouvés ou choisis dans l'assistant.
    pub comicrack_ini: Option<PathBuf>,
    pub comicrack_config: Option<PathBuf>,
    /// Mode lié (base MySQL) : écrire dans la base de ComicRack la lecture, les fichiers
    /// convertis et les infos écrites dans les fichiers. Faux par défaut (lecture seule) :
    /// une configuration existante garde son comportement.
    pub comicrack_write: bool,
    /// Mode écriture : ne plus proposer de reporter dans ComicRack la lecture gardée dans
    /// ComicRust par les versions précédentes (« Ne plus demander »).
    pub comicrack_write_migration_declined: bool,
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
            read_progress: HashMap::new(),
            library_db: None,
            comics_root: None,
            last_dir: None,
            db_source: DbSource::None,
            library_sort: SortKey::Series,
            library_descending: false,
            library_group: GroupKey::Series,
            library_table: false,
            library_stack: false,
            list_views: HashMap::new(),
            show_comic_lists: true,
            show_details: true,
            auto_refresh: true,
            resume: ResumeChoice::Ask,
            library_cover_height: 190.0,
            check_updates: true,
            skipped_version: None,
            info_written: HashMap::new(),
            converted: HashMap::new(),
            move_converted_original: false,
            library_mode: LibraryMode::Unset,
            store: None,
            comicrack_ini: None,
            comicrack_config: None,
            comicrack_write: false,
            comicrack_write_migration_declined: false,
        }
    }
}

impl Settings {
    pub const MAX_RECENT: usize = 15;

    /// Présentation actuelle (celle de la liste affichée).
    pub fn current_view(&self) -> ListView {
        ListView {
            sort: self.library_sort,
            descending: self.library_descending,
            group: self.library_group,
            stack: self.library_stack,
        }
    }

    pub fn set_current_view(&mut self, v: ListView) {
        self.library_sort = v.sort;
        self.library_descending = v.descending;
        self.library_group = v.group;
        self.library_stack = v.stack;
    }

    pub fn view_options(&self) -> ViewOptions {
        ViewOptions { sort: self.library_sort, descending: self.library_descending, group: self.library_group }
    }

    /// Réglages d'une version précédente (base MySQL déjà configurée) : mode lié à ComicRack.
    pub fn migrate(&mut self) {
        if self.library_mode == LibraryMode::Unset && self.db_source != DbSource::None {
            self.library_mode = LibraryMode::ComicRackMySql;
            if let DbSource::ComicRackIni(ini) = &self.db_source {
                self.comicrack_ini.get_or_insert_with(|| ini.clone());
            }
        }
    }

    /// Fichier à ouvrir pour un chemin de la base : le CBZ converti si l'original n'est plus là.
    pub fn effective_path(&self, path: &std::path::Path) -> PathBuf {
        match self.converted.get(path) {
            Some(cbz) if !path.exists() && cbz.exists() => cbz.clone(),
            _ => path.to_path_buf(),
        }
    }

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(Self::MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_version_settings_stay_linked_to_comicrack() {
        let mut s = Settings { db_source: DbSource::ComicRackIni("C:/CR/ComicRack.ini".into()), ..Default::default() };
        s.migrate();
        assert_eq!(s.library_mode, LibraryMode::ComicRackMySql);
        assert_eq!(s.comicrack_ini.as_deref(), Some(std::path::Path::new("C:/CR/ComicRack.ini")));
        let mut fresh = Settings::default();
        fresh.migrate();
        assert_eq!(fresh.library_mode, LibraryMode::Unset, "premier démarrage : assistant");
    }

    #[test]
    fn comicrust_read_percent_like_comicrack() {
        let p = |max_page, pages| ReadProgress { max_page, pages }.percent();
        assert_eq!(p(0, 20), None, "jamais avancé");
        assert_eq!(p(9, 20), Some(50));
        assert_eq!(p(19, 20), Some(100));
        assert_eq!(p(1, 500), Some(1), "au moins 1 %");
    }
}
