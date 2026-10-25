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
use crate::settings::{DbSource, ResumeChoice, Settings};

const SETTINGS_KEY: &str = "comicrust.settings";

/// Page d'ouverture d'un livre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Page mémorisée (ComicRust ou ComicRack, voir `Settings::resume`) ; la page de
    /// ComicRack est cherchée d'après le chemin du fichier.
    Auto,
    /// Comme `Auto`, avec la page de ComicRack déjà connue (livre de la bibliothèque).
    Resume(Option<usize>),
    Page(usize),
}

/// Question « reprendre à quelle page ? » en attente.
pub struct ResumePrompt {
    pub path: PathBuf,
    pub comicrack: usize,
    pub comicrust: usize,
    pub remember: bool,
}

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
    pub(crate) update: crate::update::UpdateState,
    pub(crate) resume_prompt: Option<ResumePrompt>,
    /// Travaux sur les fichiers BD : confirmation, travail en cours, fenêtre du bilan.
    pub(crate) file_dialog: Option<crate::library::files::FileDialog>,
    pub(crate) file_job: Option<crate::library::files::FileJob>,
    pub(crate) show_file_job: bool,
    /// Assistant de configuration (premier démarrage ou menu ⚙).
    pub(crate) setup: Option<crate::ui::setup_window::SetupWindow>,
    /// Dernière analyse des dossiers (mode autonome) : bilan affiché sur demande.
    pub(crate) last_scan: Option<crate::library::scan::ScanDone>,
    pub(crate) show_last_scan: bool,
    /// Analyser les dossiers dès que la bibliothèque est chargée (démarrage, assistant).
    pub(crate) scan_after_load: bool,
    pub(crate) scan_manual: bool,
    /// Erreurs des écritures faites en arrière-plan (lecture, fiches), affichées ensuite.
    pub(crate) bg_errors: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Enregistrer les réglages tout de suite (après l'assistant de configuration).
    pub(crate) save_now: bool,
    /// Transfert de la bibliothèque ComicRack vers la base de ComicRust.
    pub(crate) transfer: Option<crate::ui::transfer_window::TransferWindow>,
    /// Gestion des listes (v0.9.0) : opérations, éditeur, dialogues.
    pub(crate) lists_ui: crate::ui::lists_ui::ListsUi,
    /// Écritures dans la base MySQL de ComicRack (mode écriture).
    pub(crate) cr_writes: crate::library::comicrack_write::ComicRackWrites,
}

impl ComicRustApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>, data_dir: Option<PathBuf>) -> Self {
        let mut settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, SETTINGS_KEY)).unwrap_or_default();
        settings.migrate();
        // Au démarrage, toute la bibliothèque est affichée : sa présentation s'applique.
        if let Some(v) = settings.list_views.get(crate::settings::LIBRARY_VIEW_KEY).copied() {
            settings.set_current_view(v);
        }
        settings.theme.apply(&cc.egui_ctx);
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
            update: Default::default(),
            resume_prompt: None,
            file_dialog: None,
            file_job: None,
            show_file_job: false,
            setup: None,
            last_scan: None,
            show_last_scan: false,
            scan_after_load: true,
            scan_manual: false,
            bg_errors: Default::default(),
            save_now: false,
            transfer: None,
            lists_ui: Default::default(),
            cr_writes: Default::default(),
        };
        crate::update::cleanup_previous_install();
        if app.settings.library_mode == crate::settings::LibraryMode::Unset {
            // Premier démarrage : ComicRust seul ou avec ComicRack ?
            app.setup = Some(crate::ui::setup_window::SetupWindow::new(&app.settings));
            app.mode = Mode::Library;
        } else {
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
        // Recette : vérifier le rapport de plantage.
        if std::env::var_os("COMICRUST_SIMULER_PLANTAGE").is_some() {
            panic!("plantage simulé (COMICRUST_SIMULER_PLANTAGE)");
        }
        // Plantage lors de la session précédente : rapport montré une fois, puis renommé.
        if let Some(dir) = &app.data_dir {
            let crash = dir.join(crate::paths::CRASH_FILE);
            if let Ok(text) = std::fs::read_to_string(&crash) {
                let seen = dir.join(format!("plantage-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M%S")));
                let _ = std::fs::rename(&crash, &seen);
                let first: String = text.lines().take(4).collect::<Vec<_>>().join("\n");
                app.error = Some(format!(
                    "ComicRust s'est fermé de façon inattendue lors de la dernière utilisation.\n\n{first}\n\n\
                     Rapport complet : {}\n(à joindre au rapport de recette)",
                    seen.display()
                ));
            }
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
        self.open_book_at(ctx, path, Start::Auto);
    }

    /// Page où ComicRack a laissé ce fichier (base ComicRack), si elle est au-delà de la
    /// première.
    pub(crate) fn comicrack_page(&self, path: &Path) -> Option<usize> {
        let db = self.library.database()?;
        let p = path.to_string_lossy();
        let book = db.books.iter().find(|b| b.file.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&p)))?;
        book.current_page.map(|c| c as usize).filter(|c| *c > 0)
    }

    /// Ouvre un livre. `Start::Auto` reprend à la page mémorisée : celle de ComicRust ou
    /// celle de ComicRack ; si les deux existent et diffèrent, la préférence décide (ou
    /// une question est posée).
    pub(crate) fn open_book_at(&mut self, ctx: &Context, path: PathBuf, start: Start) {
        // Position du livre en cours d'abord : on peut rouvrir le même.
        self.remember_position();
        let _ = self.store_progress();
        let current = match start {
            Start::Page(p) => p,
            Start::Auto | Start::Resume(_) => {
                let crust = self.settings.last_pages.get(&path).copied().filter(|p| *p > 0);
                let crack = match start {
                    Start::Resume(p) => p.filter(|p| *p > 0),
                    _ => self.comicrack_page(&path),
                };
                match (crack, crust) {
                    (Some(a), Some(b)) if a != b => match self.settings.resume {
                        ResumeChoice::ComicRack => a,
                        ResumeChoice::ComicRust => b,
                        ResumeChoice::Ask => {
                            self.resume_prompt =
                                Some(ResumePrompt { path, comicrack: a, comicrust: b, remember: false });
                            return;
                        }
                    },
                    (a, b) => b.or(a).unwrap_or(0),
                }
            }
        };
        self.resume_prompt = None;
        self.mode = Mode::Reader;
        let max_side = ctx.input(|i| i.max_texture_side).min(16384) as u32;
        let loader = BookLoader::open(path.clone(), ctx.clone(), max_side);
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

    /// Affiche une liste ComicRack (`None` : toute la bibliothèque). Chaque liste garde sa
    /// présentation (tri, regroupement, piles) ; une liste jamais ouverte part de celle de
    /// la bibliothèque, ou de l'ordre de la liste et sans groupes pour une liste de lecture.
    pub(crate) fn select_list(&mut self, id: Option<String>) {
        use crate::settings::LIBRARY_VIEW_KEY;
        let key = |id: &Option<String>| id.clone().unwrap_or_else(|| LIBRARY_VIEW_KEY.to_string());
        let old = key(&self.library.selected_list);
        let current = self.settings.current_view();
        self.settings.list_views.insert(old, current);
        let new_key = key(&id);
        let view = match self.settings.list_views.get(&new_key) {
            Some(v) => *v,
            None => {
                let base = self.settings.list_views.get(LIBRARY_VIEW_KEY).copied().unwrap_or(current);
                let reading = id
                    .as_deref()
                    .and_then(|i| comicrust_core::comic_lists::ComicList::find(&self.library.lists.items, i))
                    .is_some_and(|l| matches!(l.kind, comicrust_core::comic_lists::ListKind::Ids(_)));
                if reading {
                    crate::settings::ListView {
                        sort: crate::library::SortKey::Position,
                        descending: false,
                        group: crate::library::GroupKey::None,
                        stack: false,
                    }
                } else {
                    base
                }
            }
        };
        self.settings.set_current_view(view);
        self.library.selected_list = id;
        self.library.open_stack = None;
        self.library.selected_stack = None;
        if let Some(c) = &mut self.covers {
            c.cancel_pending();
        }
    }

    /// (Re)lance la surveillance de la bibliothèque et la lecture des listes de ComicRack
    /// (`ComicDb.xml`), une fois la bibliothèque chargée.
    pub(crate) fn restart_watch(&mut self, ctx: &Context) {
        use crate::library::watch::{self, ListsSource};
        use crate::settings::LibraryMode;
        self.library.watcher = None;
        let Some(db) = self.library.database() else { return };
        let standalone = self.settings.library_mode == LibraryMode::Standalone;
        let db_watch = if self.settings.auto_refresh || standalone {
            self.library.source.clone().map(|c| (c, db.clone()))
        } else {
            None
        };
        let ini = self.settings.comicrack_ini.clone().or_else(|| match &self.settings.db_source {
            DbSource::ComicRackIni(ini) => Some(ini.clone()),
            _ => None,
        });
        let lists = if standalone {
            self.library.standalone().cloned().map(ListsSource::Store)
        } else {
            match (&self.settings.library_db, ini) {
                (Some(p), _) => Some(ListsSource::File(p.clone())),
                (None, Some(ini)) => Some(ListsSource::FromIni(ini)),
                _ => None,
            }
        };
        if lists.is_none() {
            self.library.lists.set(None, Err("ComicDb.xml non indiqué (base saisie à la main)".into()));
        }
        self.library.watch_error = None;
        self.library.watcher = Some(watch::spawn(db_watch, lists, ctx));
    }

    /// Bibliothèque choisie dans les réglages.
    pub(crate) fn library_source(&self) -> Result<Option<crate::library::LibrarySource>, String> {
        use crate::library::LibrarySource;
        use crate::settings::LibraryMode;
        Ok(match self.settings.library_mode {
            LibraryMode::Unset => None,
            LibraryMode::ComicRackMySql => self.settings.db_source.config()?.map(LibrarySource::ComicRackMySql),
            LibraryMode::ComicRackXml => self.settings.library_db.clone().map(LibrarySource::ComicRackXml),
            LibraryMode::Standalone => self.settings.store.clone().map(LibrarySource::Standalone),
        })
    }

    /// (Re)charge la bibliothèque configurée.
    pub(crate) fn load_library(&mut self, ctx: &Context) {
        match self.library_source() {
            Ok(Some(src)) => self.library.start_loading(src, ctx),
            Ok(None) => self.library = LibraryState::default(),
            Err(e) => self.library.status = crate::library::Status::Failed(e),
        }
    }

    /// Mode autonome : analyse des dossiers de BD (ajouts, modifications, manquants).
    /// `manual` : lancée par l'utilisateur (le bilan s'affiche s'il y a des erreurs).
    pub(crate) fn start_scan(&mut self, ctx: &Context, manual: bool) {
        // Pas pendant un transfert : les fichiers vus par l'analyse seraient ajoutés une
        // seconde fois sous un autre identifiant que celui de leur fiche ComicRack.
        if self.library.scan.is_some() || self.transfer_busy() {
            return;
        }
        self.scan_manual = manual;
        let (Some(cfg), Some(db)) = (self.library.standalone().cloned(), self.library.database()) else { return };
        self.library.scan = Some(crate::library::scan::ScanJob::start(cfg, db, ctx));
    }

    /// Résultat de l'analyse des dossiers.
    pub(crate) fn poll_scan(&mut self) {
        let Some(job) = &self.library.scan else { return };
        let Some(res) = job.poll() else { return };
        self.library.scan = None;
        match res {
            Ok(done) => {
                if !done.changes.is_empty() {
                    self.library.apply_changes(done.changes.clone());
                }
                if self.scan_manual && !done.report.errors.is_empty() {
                    self.show_last_scan = true;
                }
                self.last_scan = Some(done);
            }
            Err(e) => self.error = Some(format!("Analyse des dossiers impossible :\n{e}")),
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

    /// Ferme le livre ouvert (sa page est mémorisée) et revient à la bibliothèque si
    /// une base est chargée.
    pub(crate) fn close_book(&mut self, ctx: &Context) {
        self.remember_position();
        let _ = self.store_progress();
        self.book = None;
        if self.fullscreen {
            self.fullscreen = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        if self.library.database().is_some() {
            self.mode = Mode::Library;
        }
    }

    pub(crate) fn remember_position(&mut self) {
        if let Some(b) = &self.book {
            if b.opened {
                self.settings.last_pages.insert(b.path.clone(), b.current);
                // Page la plus loin atteinte (en double page, la seconde page compte).
                let shown = self.spread(b, b.current).into_iter().max().unwrap_or(b.current);
                let p = self.settings.read_progress.entry(b.path.clone()).or_default();
                p.max_page = p.max_page.max(shown);
                p.pages = b.page_count();
            }
        }
    }

    /// Mode autonome : enregistre dans la base de ComicRust la lecture du livre ouvert
    /// (page courante, page la plus loin, date ; une fois par livre fermé ou quitté).
    /// Avec ComicRack en mode écriture : dans la base de ComicRack (`comicrack_write`).
    pub(crate) fn store_progress(&mut self) -> Option<std::thread::JoinHandle<()>> {
        if self.comicrack_write_config().is_some() {
            return self.write_reading_to_comicrack();
        }
        let b = self.book.as_ref()?;
        if !b.opened || b.stored {
            return None;
        }
        let (cfg, db) = (self.library.standalone().cloned()?, self.library.database()?);
        let p = b.path.to_string_lossy().into_owned();
        let book = db.books.iter().find(|x| {
            x.file.as_deref().is_some_and(|f| {
                f.eq_ignore_ascii_case(&p) || self.settings.effective_path(Path::new(f)).as_path() == b.path.as_path()
            })
        })?;
        let shown = self.spread(b, b.current).into_iter().max().unwrap_or(b.current) as u32;
        let current = b.current as u32;
        let last = book.last_page_read.unwrap_or(0).max(shown);
        let id = book.id.clone();
        if let Some(b) = &mut self.book {
            b.stored = true;
        }
        let errors = self.bg_errors.clone();
        Some(std::thread::spawn(move || {
            let opened = comicrust_core::scanner::now_iso();
            if let Err(e) = comicrust_core::store::Store::open_existing(&cfg)
                .and_then(|mut s| s.set_progress(&id, current, last, &opened))
            {
                errors.lock().unwrap().push(format!("Lecture non enregistrée dans la base ComicRust :\n{e}"));
            }
        }))
    }

    /// Avancement lu dans ComicRust pour un livre de la base : (pourcentage, page).
    pub(crate) fn comicrust_progress(&self, book: &comicrust_core::library::LibraryBook) -> Option<(u32, usize)> {
        let path = PathBuf::from(book.file.as_deref()?);
        let pct = self.settings.read_progress.get(&path)?.percent()?;
        Some((pct, self.settings.last_pages.get(&path).copied().unwrap_or(0)))
    }

    /// Avancement affiché : le plus avancé entre ComicRack (base) et ComicRust.
    pub(crate) fn read_percent(&self, book: &comicrust_core::library::LibraryBook) -> Option<u32> {
        let db = book.read_percent().filter(|p| *p > 0);
        let ours = self.comicrust_progress(book).map(|p| p.0);
        db.max(ours)
    }
}

impl eframe::App for ComicRustApp {
    /// Appelé avant chaque `ui`, et aussi quand la fenêtre est masquée : le
    /// chargement continue donc même fenêtre réduite.
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Fermeture de la fenêtre : la lecture en cours est enregistrée avant de quitter, et
        // les écritures dans ComicRack en cours (lecture, fichiers) sont toujours attendues,
        // même sans livre ouvert ; leur bilan est appliqué aux réglages (enregistrés ensuite
        // par eframe) : une écriture non aboutie y reste gardée.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.remember_position();
            if let Some(h) = self.store_progress() {
                let _ = h.join();
            }
            if !self.cr_writes.wait_idle(crate::library::comicrack_write::CLOSE_WAIT) {
                eprintln!(
                    "ComicRust : {} écriture(s) dans ComicRack encore en cours à la fermeture (gardées pour le \
                     prochain lancement si elles n'aboutissent pas)",
                    self.cr_writes.pending()
                );
            }
            self.poll_comicrack_writes(ctx);
        }
        self.poll_loader(ctx);
        self.handle_input(ctx);
        self.schedule_loading();
        self.update_title(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if std::mem::take(&mut self.save_now) {
            if let Some(storage) = frame.storage_mut() {
                eframe::App::save(self, storage);
                storage.flush();
            }
        }

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
        // Lecteur : fond peint par la vue ; bibliothèque : fond du thème (clair ou sombre).
        let frame = match self.mode {
            Mode::Reader => egui::Frame::NONE,
            Mode::Library => egui::Frame::NONE.fill(ui.visuals().panel_fill),
        };
        egui::CentralPanel::default().frame(frame).show(ui, |ui| match self.mode {
            Mode::Library => self.library_view(ui),
            Mode::Reader => self.viewer(ui),
        });

        self.resume_window(&ctx);
        self.files_windows(&ctx);
        self.setup_window(&ctx);
        self.comicrack_write_window(&ctx);
        self.transfer_window(&ctx);
        self.lists_windows(&ctx);
        self.scan_window(&ctx);
        self.update_window(&ctx);
        self.info_window(&ctx);
        self.about_window(&ctx);
        self.error_window(&ctx);
        // Les textures nouvellement demandées arriveront par `request_repaint`.
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.remember_position();
        // La présentation actuelle est celle de la liste affichée.
        let key = self.library.selected_list.clone().unwrap_or_default();
        let current = self.settings.current_view();
        self.settings.list_views.insert(key, current);
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}
