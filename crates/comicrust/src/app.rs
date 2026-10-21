//! Application ComicRust : état global, ouverture des fichiers et boucle d'affichage.
//! Le dessin et la navigation sont répartis dans `ui/`, `book/`, `input.rs` et `events.rs`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Instant;

use comicrust_core::index::ScanProgress;
use comicrust_core::providers::BOOK_EXTENSIONS;
use eframe::egui::{self, Context, Ui};

use crate::book::loader::BookLoader;
use crate::book::Book;
use crate::library::covers::CoverCache;
use crate::library::LibraryState;
use crate::reading_list::ListState;
use crate::settings::{DbSource, Settings};
use crate::ui::db_window::DbWindow;

const SETTINGS_KEY: &str = "comicrust.settings";

/// Vue principale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Library,
    Reader,
}

pub struct ComicRustApp {
    pub(crate) settings: Settings,
    pub(crate) book: Option<Book>,
    pub(crate) list: Option<ListState>,
    pub(crate) list_loading: Option<Receiver<Result<ListState, String>>>,
    pub(crate) list_progress: std::sync::Arc<ScanProgress>,
    /// Dossier des données de l'application (cache de l'index des BD).
    pub(crate) data_dir: Option<PathBuf>,
    pub(crate) zoom: f32,
    pub(crate) fullscreen: bool,
    pub(crate) show_info: bool,
    pub(crate) show_about: bool,
    pub(crate) show_list_panel: bool,
    pub(crate) error: Option<String>,
    pub(crate) last_wheel_flip: Instant,
    pub(crate) title: String,
    pub(crate) scroll_to_thumb: bool,
    pub(crate) mode: Mode,
    pub(crate) library: LibraryState,
    /// Créé à la première ouverture de la vue Bibliothèque.
    pub(crate) covers: Option<CoverCache>,
    pub(crate) db_window: Option<DbWindow>,
    pub(crate) update: crate::update::UpdateState,
}

impl ComicRustApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>, data_dir: Option<PathBuf>) -> Self {
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, SETTINGS_KEY)).unwrap_or_default();
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self {
            settings,
            book: None,
            list: None,
            list_loading: None,
            list_progress: Default::default(),
            data_dir,
            zoom: 1.0,
            fullscreen: false,
            show_info: false,
            show_about: false,
            show_list_panel: true,
            error: None,
            last_wheel_flip: Instant::now(),
            title: String::new(),
            scroll_to_thumb: true,
            mode: Mode::Reader,
            library: LibraryState::default(),
            covers: None,
            db_window: None,
            update: Default::default(),
        };
        crate::update::cleanup_previous_install();
        if app.settings.db_source != DbSource::None {
            app.load_library(&cc.egui_ctx);
            app.mode = Mode::Library;
        }
        if app.settings.check_updates {
            app.update.check(&cc.egui_ctx, false);
        }
        if app.data_dir.is_none() {
            app.error = Some(format!(
                "Le dossier de ComicRust n'est pas modifiable ({}).\n\nComicRust est une application portable : \
                 place-la dans un dossier où tu peux écrire (pas dans « Program Files ») pour que tes réglages \
                 soient gardés.",
                crate::paths::exe_dir().display()
            ));
        }
        if let Some(p) = initial {
            app.open_path(&cc.egui_ctx, p);
        }
        app
    }

    pub(crate) fn open_path(&mut self, ctx: &Context, path: PathBuf) {
        let is_cbl = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cbl"));
        self.settings.last_dir = path.parent().map(Path::to_path_buf);
        if is_cbl {
            self.load_list(ctx, path);
        } else {
            self.open_book(ctx, path);
        }
    }

    pub(crate) fn open_book(&mut self, ctx: &Context, path: PathBuf) {
        self.open_book_at(ctx, path, None);
    }

    /// Ouvre un livre ; `start` (page lue d'après la base ComicRack) sert si ComicRust
    /// n'a pas encore de page mémorisée pour ce fichier.
    pub(crate) fn open_book_at(&mut self, ctx: &Context, path: PathBuf, start: Option<usize>) {
        self.remember_position();
        self.mode = Mode::Reader;
        let max_side = ctx.input(|i| i.max_texture_side).min(16384) as u32;
        let loader = BookLoader::open(path.clone(), ctx.clone(), max_side);
        // Page de reprise : celle mémorisée par ComicRust, sinon celle de la base ComicRack.
        let from_db = || {
            let db = self.library.database()?;
            let p = path.to_string_lossy();
            db.books.iter().find(|b| b.file.as_deref() == Some(p.as_ref()))?.current_page.map(|c| c as usize)
        };
        let current = self.settings.last_pages.get(&path).copied().or(start).or_else(from_db).unwrap_or(0);
        self.settings.push_recent(path.clone());
        if let Some(list) = &mut self.list {
            list.current = list.position_of(&path);
        }
        self.book = Some(Book::new(path, loader, current));
        self.zoom = 1.0;
        self.scroll_to_thumb = true;
    }

    pub(crate) fn load_list(&mut self, ctx: &Context, path: PathBuf) {
        self.settings.push_recent(path.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let db = self.settings.library_db.clone();
        let root = self.settings.comics_root.clone();
        let cache = self.data_dir.as_ref().map(|d| d.join(crate::paths::INDEX_CACHE_FILE));
        let progress = std::sync::Arc::new(ScanProgress::default());
        self.list_progress = progress.clone();
        let comics = self.library.database();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ListState::load(
                &path,
                comics.as_deref(),
                db.as_deref(),
                root.as_deref(),
                cache.as_deref(),
                Some(&progress),
            ));
            ctx.request_repaint();
        });
        self.list_loading = Some(rx);
        self.show_list_panel = true;
    }

    /// (Re)lance la surveillance de la base (modifications faites dans ComicRack) et la
    /// lecture des listes (`ComicDb.xml`), une fois la base chargée.
    pub(crate) fn restart_watch(&mut self, ctx: &Context) {
        use crate::library::watch::{self, ListsSource};
        self.library.watcher = None;
        let Some(db) = self.library.database() else { return };
        let db_watch =
            if self.settings.auto_refresh { self.library.config.clone().map(|c| (c, db.counters)) } else { None };
        let lists = match (&self.settings.library_db, &self.settings.db_source) {
            (Some(p), _) => Some(ListsSource::File(p.clone())),
            (None, DbSource::ComicRackIni(ini)) => Some(ListsSource::FromIni(ini.clone())),
            _ => None,
        };
        if lists.is_none() {
            self.library.lists.set(None, Err("ComicDb.xml non indiqué (base saisie à la main)".into()));
        }
        self.library.watch_error = None;
        self.library.watcher = Some(watch::spawn(db_watch, lists, ctx));
    }

    /// (Re)charge la base ComicRack configurée.
    pub(crate) fn load_library(&mut self, ctx: &Context) {
        match self.settings.db_source.config() {
            Ok(Some(cfg)) => self.library.start_loading(cfg, ctx),
            Ok(None) => self.library = LibraryState::default(),
            Err(e) => self.library.status = crate::library::Status::Failed(e),
        }
    }

    pub(crate) fn reload_list(&mut self, ctx: &Context) {
        if let Some(path) = self.list.as_ref().map(|l| l.path.clone()) {
            self.load_list(ctx, path);
        }
    }

    pub(crate) fn open_dialog(&mut self, ctx: &Context) {
        let mut exts: Vec<&str> = BOOK_EXTENSIONS.to_vec();
        exts.push("cbl");
        let mut dlg = rfd::FileDialog::new()
            .set_title("Ouvrir une BD ou une liste de lecture")
            .add_filter("BD et listes de lecture", &exts)
            .add_filter("Listes de lecture ComicRack", &["cbl"])
            .add_filter("Tous les fichiers", &["*"]);
        if let Some(d) = &self.settings.last_dir {
            dlg = dlg.set_directory(d);
        }
        if let Some(p) = dlg.pick_file() {
            self.open_path(ctx, p);
        }
    }

    pub(crate) fn open_folder_dialog(&mut self, ctx: &Context) {
        if let Some(p) = rfd::FileDialog::new().set_title("Ouvrir un dossier d'images").pick_folder() {
            self.open_path(ctx, p);
        }
    }

    pub(crate) fn remember_position(&mut self) {
        if let Some(b) = &self.book {
            if b.opened {
                self.settings.last_pages.insert(b.path.clone(), b.current);
            }
        }
    }
}

impl eframe::App for ComicRustApp {
    /// Appelé avant chaque `ui`, et aussi quand la fenêtre est masquée : le
    /// chargement continue donc même fenêtre réduite.
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_loader(ctx);
        self.handle_input(ctx);
        self.schedule_loading();
        self.update_title(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if !self.fullscreen {
            egui::Panel::top("toolbar").show(ui, |ui| {
                ui.add_space(2.0);
                self.toolbar(ui);
                ui.add_space(2.0);
            });
            if self.settings.show_status_bar {
                egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
            }
            self.update_banner(ui);
            if self.mode == Mode::Reader
                && self.settings.show_thumbnails
                && self.book.as_ref().is_some_and(|b| b.opened)
            {
                egui::Panel::left("thumbs")
                    .resizable(true)
                    .default_size(150.0)
                    .min_size(90.0)
                    .show(ui, |ui| self.thumbnails_panel(ui));
            }
            let library_ready = self.mode == Mode::Library && self.library.database().is_some();
            if library_ready && self.settings.show_comic_lists {
                egui::Panel::left("listes-comicrack")
                    .resizable(true)
                    .default_size(230.0)
                    .min_size(150.0)
                    .show(ui, |ui| self.comic_lists_panel(ui));
            }
            if library_ready && self.settings.show_details {
                egui::Panel::right("details")
                    .resizable(true)
                    .default_size(300.0)
                    .min_size(220.0)
                    .show(ui, |ui| self.details_panel(ui));
            }
            if self.show_list_panel && (self.list.is_some() || self.list_loading.is_some()) {
                egui::Panel::right("list").resizable(true).default_size(260.0).show(ui, |ui| self.list_panel(ui));
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| match self.mode {
            Mode::Library => self.library_view(ui),
            Mode::Reader => self.viewer(ui),
        });

        self.db_window_ui(&ctx);
        self.update_window(&ctx);
        self.info_window(&ctx);
        self.about_window(&ctx);
        self.error_window(&ctx);
        // Les textures nouvellement demandées arriveront par `request_repaint`.
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.remember_position();
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}
