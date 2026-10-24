//! « Transférer la bibliothèque ComicRack » (menu ⚙) : copie des livres (avec leur
//! lecture, leurs notes…), des listes et des dossiers de ComicRack dans la base de ComicRust,
//! pour pouvoir ne plus utiliser ComicRack. La base de ComicRack n'est que lue.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use comicrust_core::db::{ComicDatabase, DbConfig, LoadProgress};
use comicrust_core::store::{ComicRackSource, ImportOptions, ImportReport, Store, StoreConfig};
use eframe::egui::{self, Context, RichText};

use crate::app::ComicRustApp;
use crate::settings::{DbSource, LibraryMode};

/// Résultat du transfert, renvoyé par le fil.
struct Done {
    report: ImportReport,
    dest: StoreConfig,
    total: usize,
}

pub struct TransferWindow {
    /// Base MySQL de ComicRack (si ComicRack est relié à MySQL) : source des livres.
    mysql: Option<DbConfig>,
    use_mysql: bool,
    /// ComicDb.xml : livres (sans MySQL), listes et dossiers.
    comicdb: String,
    /// Destination : base ComicRust actuelle (`Some`) ou nouveau fichier portable.
    current_store: Option<StoreConfig>,
    use_current: bool,
    lists: bool,
    folders: bool,
    switch: bool,
    // Travail en cours
    rx: Option<Receiver<Result<Done, String>>>,
    progress: Arc<LoadProgress>,
    phase: Arc<Mutex<String>>,
    cancel: Arc<AtomicBool>,
    result: Option<Result<(String, Vec<String>), String>>,
}

fn path_of(s: &str) -> Option<PathBuf> {
    let t = s.trim().trim_matches('"');
    (!t.is_empty()).then(|| PathBuf::from(t))
}

impl TransferWindow {
    fn new(app: &ComicRustApp) -> Self {
        let s = &app.settings;
        let mysql = s.db_source.config().ok().flatten();
        let ini = s.comicrack_ini.clone().or_else(|| match &s.db_source {
            DbSource::ComicRackIni(p) => Some(p.clone()),
            _ => None,
        });
        let comicdb =
            s.library_db.clone().or_else(|| ini.as_deref().and_then(comicrust_core::comic_lists::find_comicdb));
        Self {
            use_mysql: mysql.is_some(),
            mysql,
            comicdb: comicdb.map(|p| p.display().to_string()).unwrap_or_default(),
            use_current: s.store.is_some(),
            current_store: s.store.clone(),
            lists: true,
            folders: true,
            switch: true,
            rx: None,
            progress: Arc::default(),
            phase: Arc::default(),
            cancel: Arc::default(),
            result: None,
        }
    }

    fn start(&mut self, ctx: &Context) {
        let dest = match (&self.current_store, self.use_current) {
            (Some(c), true) => c.clone(),
            _ => match StoreConfig::portable_sqlite() {
                Ok(c) => c,
                Err(e) => return self.result = Some(Err(e.to_string())),
            },
        };
        let mysql = self.mysql.clone().filter(|_| self.use_mysql);
        let comicdb = path_of(&self.comicdb);
        if mysql.is_none() && comicdb.is_none() {
            return self.result = Some(Err("indique le ComicDb.xml de ComicRack".into()));
        }
        let opts = ImportOptions { lists: self.lists, folders: self.folders };
        let (tx, rx) = mpsc::channel();
        let (progress, phase, cancel, ctx2) =
            (self.progress.clone(), self.phase.clone(), self.cancel.clone(), ctx.clone());
        self.cancel.store(false, Ordering::Relaxed);
        self.progress.total.store(0, Ordering::Relaxed);
        self.progress.done.store(0, Ordering::Relaxed);
        *self.phase.lock().unwrap() = "préparation…".into();
        std::thread::Builder::new()
            .name("comicrust-transfert".into())
            .spawn(move || {
                let set = |t: &str| *phase.lock().unwrap() = t.to_string();
                let run = || -> Result<Done, String> {
                    // 1. Lecture de ComicRack (seulement des lectures).
                    let mut read_warning = None;
                    let xml = match &comicdb {
                        Some(p) => match std::fs::read_to_string(p) {
                            Ok(x) => Some(x),
                            // Avec MySQL, ComicDb.xml ne sert qu'aux listes et aux dossiers :
                            // les livres sont transférés quand même.
                            Err(e) if mysql.is_some() => {
                                read_warning = Some(format!("{} illisible ({e}) : ni listes ni dossiers", p.display()));
                                None
                            }
                            Err(e) => return Err(format!("{} : {e}", p.display())),
                        },
                        None => None,
                    };
                    let src = match &mysql {
                        Some(cfg) => {
                            set("lecture de la base MySQL de ComicRack…");
                            let db = ComicDatabase::load(cfg, Some(&progress)).map_err(|e| e.to_string())?;
                            ComicRackSource { books: db.books, comicdb_xml: xml }
                        }
                        None => {
                            set("lecture de ComicDb.xml…");
                            ComicRackSource::from_comicdb_xml(xml.unwrap_or_default()).map_err(|e| e.to_string())?
                        }
                    };
                    if src.books.is_empty() {
                        return Err(if mysql.is_some() {
                            "la base MySQL de ComicRack ne contient aucun livre".into()
                        } else {
                            "ce ComicDb.xml ne contient aucun livre : ComicRack range sans doute ses livres dans \
                             MySQL. Passe d'abord par ⚙ → « Assistant de configuration… » → « Avec ComicRack » → \
                             MySQL, puis reviens ici : la base MySQL de ComicRack sera proposée."
                                .into()
                        });
                    }
                    // 2. Écriture dans la base de ComicRust.
                    set("écriture dans la base de ComicRust (livres, puis listes et dossiers)…");
                    let mut store = Store::open(&dest).map_err(|e| e.to_string())?;
                    let mut report = store
                        .import_comicrack(&src, &opts, Some(&progress), Some(&cancel))
                        .map_err(|e| e.to_string())?;
                    report.warnings.splice(0..0, read_warning);
                    Ok(Done { report, dest, total: src.books.len() })
                };
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
                    .unwrap_or_else(|_| Err("erreur interne (détails dans data\\plantage.txt)".into()));
                let _ = tx.send(res);
                ctx2.request_repaint();
            })
            .expect("fil du transfert");
        self.rx = Some(rx);
        self.result = None;
    }
}

fn summary(d: &Done) -> (String, Vec<String>) {
    let r = &d.report;
    let mut s = format!(
        "{} livres de ComicRack : {} ajoutés, {} remplacés (déjà ajoutés par l'analyse des dossiers), {} mis à jour",
        d.total, r.books_added, r.books_replaced, r.books_updated
    );
    if r.books_skipped > 0 {
        s.push_str(&format!(", {} écartés", r.books_skipped));
    }
    s.push_str(&format!(" ; {} listes ; {} dossiers ajoutés.", r.lists, r.folders.len()));
    if r.paths_rewritten > 0 {
        s.push_str(&format!(" {} chemins ramenés sous un dossier surveillé.", r.paths_rewritten));
    }
    if r.files_kept > 0 {
        s.push_str(&format!(" {} fichiers convertis ou déplacés dans ComicRust gardés.", r.files_kept));
    }
    if r.cancelled {
        s.push_str(" Interrompu : les lots déjà écrits sont gardés, ComicRust n'a pas changé de bibliothèque. Relance le transfert pour le terminer.");
    }
    (s, r.warnings.clone())
}

impl ComicRustApp {
    /// Transfert en cours (l'analyse des dossiers attend qu'il soit fini).
    pub(crate) fn transfer_busy(&self) -> bool {
        self.transfer.as_ref().is_some_and(|t| t.rx.is_some())
    }

    pub(crate) fn open_transfer(&mut self) {
        if self.transfer.is_some() {
            return; // déjà ouverte (et peut-être en cours) : ne pas lancer un second transfert
        }
        self.transfer = Some(TransferWindow::new(self));
    }

    pub(crate) fn transfer_window(&mut self, ctx: &Context) {
        let scanning = self.library.scan.is_some();
        let Some(w) = &mut self.transfer else { return };
        let mut finished: Option<Done> = None;
        if let Some(rx) = &w.rx {
            if let Ok(r) = rx.try_recv() {
                w.rx = None;
                match r {
                    Ok(d) => {
                        w.result = Some(Ok(summary(&d)));
                        finished = Some(d);
                    }
                    Err(e) => w.result = Some(Err(e)),
                }
            }
        }
        let busy = w.rx.is_some();
        let mut open = true;
        let mut go = false;
        egui::Window::new("Transférer la bibliothèque ComicRack")
            .id(egui::Id::unique("comicrust-transfert"))
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    "Copie les livres de ComicRack (infos, lecture, notes…), ses listes et ses dossiers dans la \
                     base de ComicRust, pour pouvoir se passer de ComicRack. ComicRack n'est pas modifié.",
                );
                ui.add_space(6.0);
                ui.add_enabled_ui(!busy, |ui| {
                    ui.label(RichText::new("Depuis ComicRack").strong());
                    if let Some(c) = &w.mysql {
                        ui.radio_value(&mut w.use_mysql, true, format!("Base MySQL de ComicRack ({})", c.describe()));
                    }
                    ui.radio_value(&mut w.use_mysql, false, "ComicDb.xml (ComicRack sans MySQL)");
                    ui.horizontal(|ui| {
                        ui.label(if w.use_mysql { "ComicDb.xml (listes, dossiers) :" } else { "ComicDb.xml :" });
                        ui.add(egui::TextEdit::singleline(&mut w.comicdb).desired_width(300.0));
                        if ui.button("Parcourir…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("ComicDb.xml", &["xml"]).pick_file() {
                                w.comicdb = p.display().to_string();
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Vers ComicRust").strong());
                    if let Some(c) = &w.current_store {
                        ui.radio_value(&mut w.use_current, true, format!("Base actuelle : {}", c.describe()));
                    }
                    ui.radio_value(
                        &mut w.use_current,
                        false,
                        "Fichier local portable (data\\comicrust.db, à côté de l'exe)",
                    );
                    ui.label(
                        RichText::new(
                            "Pour une base MySQL : la configurer d'abord dans l'assistant (ComicRust autonome), \
                             puis la choisir ici comme base actuelle.",
                        )
                        .weak()
                        .small(),
                    );
                    ui.add_space(6.0);
                    ui.checkbox(&mut w.lists, "Listes (dossiers, listes de lecture, listes intelligentes)");
                    ui.checkbox(&mut w.folders, "Dossiers de BD surveillés par ComicRack");
                    ui.checkbox(&mut w.switch, "Ensuite, utiliser ComicRust en autonome (sans ComicRack)");
                    ui.label(
                        RichText::new(
                            "Un transfert peut être relancé : les fiches sont reprises de ComicRack, mais la lecture \
                             faite dans ComicRust entre-temps et les fichiers convertis ou déplacés sont gardés. \
                             Les modifications de fiches faites dans ComicRust seront remplacées.",
                        )
                        .weak()
                        .small(),
                    );
                });
                ui.separator();
                if busy {
                    let (done, total) = w.progress.get();
                    let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                    ui.label(w.phase.lock().unwrap().clone());
                    ui.add(egui::ProgressBar::new(frac).text(format!("{done} / {total}")));
                    if ui.button("⏹ Arrêter").clicked() {
                        w.cancel.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                } else if scanning {
                    ui.add_enabled(false, egui::Button::new(RichText::new("Transférer").strong()));
                    ui.label(
                        RichText::new(
                            "Analyse des dossiers en cours : attendre sa fin (ou l'arrêter) avant de transférer.",
                        )
                        .color(crate::ui::theme::warning(ui)),
                    );
                } else if ui.button(RichText::new("Transférer").strong()).clicked() {
                    go = true;
                }
                match &w.result {
                    Some(Ok((s, warnings))) => {
                        ui.label(RichText::new(format!("✔ {s}")).color(crate::ui::theme::ok(ui)));
                        for wn in warnings.iter().take(10) {
                            ui.label(RichText::new(format!("⚠ {wn}")).color(crate::ui::theme::warning(ui)).small());
                        }
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(format!("✖ {e}")).color(crate::ui::theme::error(ui)));
                    }
                    None => {}
                }
            });
        if go {
            w.start(ctx);
        }
        let switch = w.switch;
        if let Some(d) = finished {
            if d.report.cancelled {
                // Bibliothèque incomplète : on ne bascule pas ; on recharge seulement si c'est
                // la base affichée, pour voir les livres déjà écrits.
                if self.library.standalone() == Some(&d.dest) {
                    self.load_library(ctx);
                }
            } else if switch || self.library.standalone() == Some(&d.dest) {
                self.settings.library_mode = LibraryMode::Standalone;
                self.settings.store = Some(d.dest);
                self.save_now = true;
                self.select_list(None);
                self.scan_after_load = true;
                self.load_library(ctx);
                self.mode = crate::app::Mode::Library;
            }
        }
        if !open && !busy {
            self.transfer = None;
        }
    }
}
