//! Livres manquants (v0.10.2) : badge des couvertures, entrée « Livres manquants (N) » du
//! panneau des listes, fenêtre « Fichier introuvable », « Retrouver le fichier… »,
//! « Rechercher les fichiers manquants dans un dossier… », « Retirer de la
//! bibliothèque… ». Logique et opérations : `library/missing.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use comicrust_core::library::LibraryBook;
use comicrust_core::missing::{volume_root, CheckItem, Progress};
use comicrust_core::providers::BOOK_EXTENSIONS;
use eframe::egui::{self, Color32, Context, FontId, Rect, RichText, Ui};

use crate::app::ComicRustApp;
use crate::library::missing::{is_missing, OpKind, OpResult, Proposal, SearchJob, Target};

/// Demande venue d'un menu, d'un bouton ou du clavier (traitée après le dessin).
#[derive(Debug, Clone, PartialEq)]
pub enum MissingRequest {
    /// Choisir le nouveau fichier d'un livre.
    Locate(String),
    /// Chercher les fichiers de ces livres (manquants) dans un dossier à choisir.
    SearchFolder(Vec<String>),
    /// Retirer ces livres de la bibliothèque (après confirmation).
    Remove(Vec<String>),
    /// Vérifier tous les fichiers maintenant.
    Check,
}

/// État du volume du fichier introuvable (vérifié dans un fil : un NAS éteint peut faire
/// attendre plusieurs secondes).
#[derive(Debug, Clone, PartialEq)]
pub enum Volume {
    Checking,
    /// Le volume répond : le fichier n'est plus à cet endroit (livre marqué manquant).
    Reachable,
    /// Le volume ne répond pas : rien de marqué.
    Unreachable(String),
}

/// Fenêtre « Fichier introuvable ».
pub struct NotFound {
    pub id: String,
    pub caption: String,
    pub file: String,
    pub volume: Volume,
    rx: Option<Receiver<Volume>>,
}

/// Confirmation du retrait.
pub struct RemoveDialog {
    pub ids: Vec<String>,
    pub captions: Vec<String>,
    /// Fichiers encore présents (`None` : vérification en cours).
    pub existing: Option<usize>,
    rx: Option<Receiver<usize>>,
}

/// Confirmation d'une relocalisation.
pub struct RelocateDialog {
    pub proposals: Vec<Proposal>,
    /// Livres non retrouvés (recherche dans un dossier).
    pub not_found: usize,
    /// Où l'on a cherché (texte).
    pub source: String,
}

#[derive(Default)]
pub struct MissingUi {
    pub pending: Option<MissingRequest>,
    pub not_found: Option<NotFound>,
    pub remove: Option<RemoveDialog>,
    pub relocate: Option<RelocateDialog>,
    /// Fenêtre de la vérification demandée (avancement, bilan).
    pub show_check: bool,
    /// Message d'information (résultat d'une opération).
    pub info: Option<String>,
}

/// Données des menus contextuels (calculées avant le dessin).
pub struct MissingMenu {
    /// Fiches non modifiables : raison.
    pub reason: Option<String>,
}

/// Grise une couverture et ajoute le badge « manquant ».
pub fn paint_missing(ui: &Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, ui.visuals().panel_fill.gamma_multiply(0.7));
    let h = (rect.height() * 0.13).clamp(16.0, 24.0);
    let band = Rect::from_min_max(
        egui::pos2(rect.min.x, rect.center().y - h / 2.0),
        egui::pos2(rect.max.x, rect.center().y + h / 2.0),
    );
    p.rect_filled(band, 2.0, Color32::from_rgb(170, 40, 40));
    p.text(band.center(), egui::Align2::CENTER_CENTER, "⚠ manquant", FontId::proportional(h * 0.62), Color32::WHITE);
}

/// Entrées du menu contextuel d'un livre (ou de la sélection `books`).
pub fn missing_menu(
    ui: &mut Ui,
    menu: &MissingMenu,
    book: &LibraryBook,
    books: &[String],
    any_missing: bool,
    request: &mut Option<MissingRequest>,
) {
    ui.separator();
    let enabled = menu.reason.is_none();
    if is_missing(book) {
        let r = ui.add_enabled(enabled, egui::Button::new("📂 Retrouver le fichier…"));
        if let Some(why) = &menu.reason {
            r.clone().on_disabled_hover_text(why);
        }
        if r.clicked() {
            *request = Some(MissingRequest::Locate(book.id.clone()));
            ui.close();
        }
    }
    if any_missing {
        let r = ui.add_enabled(enabled, egui::Button::new("🔍 Rechercher les fichiers manquants dans un dossier…"));
        if let Some(why) = &menu.reason {
            r.clone().on_disabled_hover_text(why);
        }
        if r.clicked() {
            *request = Some(MissingRequest::SearchFolder(books.to_vec()));
            ui.close();
        }
    }
    let text = if books.len() > 1 {
        format!("🗑 Retirer les {} livres de la bibliothèque…", books.len())
    } else {
        "🗑 Retirer de la bibliothèque…".to_string()
    };
    let r = ui.add_enabled(enabled, egui::Button::new(text).shortcut_text("Suppr"));
    if let Some(why) = &menu.reason {
        r.clone().on_disabled_hover_text(why);
    }
    if r.clicked() {
        *request = Some(MissingRequest::Remove(books.to_vec()));
        ui.close();
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

impl ComicRustApp {
    /// Où modifier les fiches, ou pourquoi c'est impossible.
    pub(crate) fn missing_target(&self) -> Result<Target, String> {
        use crate::library::LibrarySource;
        if let Some(cfg) = self.library.standalone() {
            return Ok(Target::Store(cfg.clone()));
        }
        if let Some(cfg) = self.comicrack_write_config() {
            return Ok(Target::ComicRack(cfg));
        }
        Err(match &self.library.source {
            Some(LibrarySource::ComicRackXml(_)) => {
                "Bibliothèque ComicDb.xml de ComicRack : ses livres sont en lecture \
                 seule pour ComicRust (ComicRack réécrit ce fichier). Fais-le dans ComicRack."
                    .into()
            }
            _ => "Base de ComicRack en lecture seule : ComicRust ne modifie pas ses fiches. Fais-le dans ComicRack, \
                  ou choisis « Lecture et écriture » dans l'assistant de configuration (⚙)."
                .into(),
        })
    }

    pub(crate) fn missing_menu_data(&self) -> MissingMenu {
        let busy = self.library.missing.busy().then(|| "Mise à jour des fiches en cours…".to_string());
        MissingMenu { reason: self.missing_target().err().or(busy) }
    }

    /// Ouverture d'un livre de la bibliothèque dont le fichier est introuvable.
    pub(crate) fn missing_not_found(&mut self, id: &str) {
        let Some(book) = self.library.database().and_then(|db| db.get(id).cloned()) else { return };
        let file = book.file.clone().unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let f = file.clone();
        std::thread::spawn(move || {
            let v = match volume_root(&f) {
                Some(root) if std::fs::read_dir(&root).is_err() => Volume::Unreachable(root),
                _ => Volume::Reachable,
            };
            let _ = tx.send(v);
        });
        self.missing_ui.not_found = Some(NotFound {
            id: book.id.clone(),
            caption: book.caption(),
            file,
            volume: Volume::Checking,
            rx: Some(rx),
        });
    }

    /// Vérification automatique après le chargement (bibliothèque ComicRack : la base
    /// ComicRust, elle, est tenue à jour par l'analyse des dossiers).
    pub(crate) fn missing_after_load(&mut self, ctx: &Context) {
        if self.settings.library_mode.is_comicrack() {
            self.start_missing_check(ctx, false);
        }
    }

    fn start_missing_check(&mut self, ctx: &Context, manual: bool) {
        let converted = self.settings.converted.clone();
        self.library.start_check(
            |f| {
                let p = PathBuf::from(f);
                converted.get(&p).cloned().unwrap_or(p)
            },
            manual,
            ctx,
        );
        if manual {
            self.missing_ui.show_check = true;
        }
    }

    /// Résultats des fils : vérification, volume, recherche, opérations sur les fiches.
    pub(crate) fn poll_missing(&mut self, ctx: &Context) {
        if let Some((report, _manual)) = self.library.poll_check() {
            // Base ComicRust : le résultat est enregistré (comme l'analyse des dossiers).
            if let (Some(cfg), false) = (self.library.standalone().cloned(), report.cancelled) {
                let db = self.library.database();
                let was = |id: &String| db.as_ref().and_then(|d| d.get(id)).map(|b| b.missing);
                let missing: Vec<String> = report.missing.clone();
                let back: Vec<String> =
                    report.present.iter().filter(|id| was(id).flatten().is_some()).cloned().collect();
                if !missing.is_empty() || !back.is_empty() {
                    let sink = self.library.watcher.as_ref().map(|w| w.written.clone());
                    let rx = crate::library::missing::persist_marks(cfg, missing, back, sink, ctx);
                    self.library.missing.push_op(rx);
                }
            }
        }
        if let Some(nf) = &mut self.missing_ui.not_found {
            if let Some(v) = nf.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                nf.rx = None;
                nf.volume = v.clone();
                if v == Volume::Reachable {
                    let id = nf.id.clone();
                    self.library.set_missing(std::slice::from_ref(&id), true);
                    if let Some(cfg) = self.library.standalone().cloned() {
                        let sink = self.library.watcher.as_ref().map(|w| w.written.clone());
                        let rx = crate::library::missing::persist_marks(cfg, vec![id], vec![], sink, ctx);
                        self.library.missing.push_op(rx);
                    }
                }
            } else if nf.rx.is_some() {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        if let Some(d) = &mut self.missing_ui.remove {
            if let Some(n) = d.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                d.existing = Some(n);
                d.rx = None;
            }
        }
        if let Some(job) = &self.library.missing.search {
            if let Some(report) = job.poll() {
                let dir = job.dir.clone();
                self.library.missing.search = None;
                if report.cancelled {
                    // Arrêtée : rien.
                } else if report.found.is_empty() {
                    self.missing_ui.info = Some(format!(
                        "Aucun fichier retrouvé dans {} ({} fichier{} examiné{}).",
                        dir.display(),
                        report.files_seen,
                        plural(report.files_seen),
                        plural(report.files_seen)
                    ));
                } else {
                    let proposals = self.library.proposals(&report);
                    self.missing_ui.relocate = Some(RelocateDialog {
                        proposals,
                        not_found: report.not_found.len(),
                        source: format!("{} ({} fichiers examinés)", dir.display(), report.files_seen),
                    });
                }
            }
        }
        for done in self.library.missing.poll_ops() {
            let result = match done.result {
                Ok(r) => r,
                Err(e) => {
                    let what = match done.kind {
                        OpKind::Relocate => "Fiches non mises à jour",
                        OpKind::Remove => "Livres non retirés de la bibliothèque",
                        OpKind::Mark => "État « manquant » non enregistré dans la base",
                    };
                    self.bg_errors.lock().unwrap().push(format!("{what} :\n{e}"));
                    continue;
                }
            };
            match result {
                OpResult::Store { books, counter } => {
                    let n = books.len();
                    if let Some(c) = &mut self.covers {
                        for b in &books {
                            c.forget_failure(&b.id, b.front_cover_image());
                        }
                    }
                    let rows = books.iter().map(|b| (b.id.to_ascii_lowercase(), counter)).collect();
                    self.library.update_books(books, rows);
                    if done.kind == OpKind::Relocate {
                        self.missing_ui.info = Some(format!("{n} fiche{} mise{} à jour.", plural(n), plural(n)));
                    }
                }
                OpResult::ComicRack(report) => {
                    if let Some(c) = &mut self.covers {
                        for w in &report.written {
                            c.forget_failure(&w.id, w.book.front_cover_image());
                        }
                    }
                    self.library.apply_written(&report);
                    self.missing_ui.info = Some(report.summary());
                }
                OpResult::Removed { ids, absent } => {
                    let all: Vec<String> = ids.iter().chain(&absent).cloned().collect();
                    self.library.remove_books(&all);
                    self.missing_ui.info = Some(format!(
                        "{} livre{} retiré{} de la bibliothèque (fichiers non touchés).",
                        ids.len(),
                        plural(ids.len()),
                        plural(ids.len())
                    ));
                }
            }
        }
        if let Some(r) = self.missing_ui.pending.take() {
            self.handle_missing_request(ctx, r);
        }
    }

    pub(crate) fn handle_missing_request(&mut self, ctx: &Context, r: MissingRequest) {
        match r {
            MissingRequest::Check => self.start_missing_check(ctx, true),
            MissingRequest::Locate(id) => {
                if let Err(e) = self.missing_target() {
                    self.error = Some(e);
                    return;
                }
                let Some(old) = self.library.database().and_then(|db| db.get(&id).and_then(|b| b.file.clone())) else {
                    return;
                };
                let name = Path::new(&old.replace('\\', "/")).file_name().map(|n| n.to_string_lossy().into_owned());
                let mut dlg = rfd::FileDialog::new()
                    .set_title(format!("Retrouver le fichier de « {} »", name.as_deref().unwrap_or("?")))
                    .add_filter("BD", BOOK_EXTENSIONS)
                    .add_filter("Tous les fichiers", &["*"]);
                if let Some(d) = &self.settings.last_dir {
                    dlg = dlg.set_directory(d);
                }
                if let Some(n) = &name {
                    dlg = dlg.set_file_name(n);
                }
                let Some(new) = dlg.pick_file() else { return };
                self.settings.last_dir = new.parent().map(Path::to_path_buf);
                let size = std::fs::metadata(&new).map(|m| m.len()).unwrap_or(0);
                if let Some(p) = self.library.chosen_proposal(&id, new, size) {
                    self.missing_ui.relocate =
                        Some(RelocateDialog { proposals: vec![p], not_found: 0, source: "fichier choisi".into() });
                }
            }
            MissingRequest::SearchFolder(ids) => {
                if let Err(e) = self.missing_target() {
                    self.error = Some(e);
                    return;
                }
                // Plusieurs livres : seulement les manquants ; un seul (fenêtre « Fichier
                // introuvable », NAS injoignable compris) : tel quel.
                let db = self.library.database();
                let ids: Vec<String> = if ids.len() > 1 {
                    ids.into_iter().filter(|id| db.as_ref().and_then(|d| d.get(id)).is_some_and(is_missing)).collect()
                } else {
                    ids
                };
                if ids.is_empty() {
                    self.missing_ui.info = Some("Aucun livre manquant à rechercher.".into());
                    return;
                }
                let mut dlg = rfd::FileDialog::new().set_title(format!(
                    "Dossier où chercher {} fichier{} manquant{}",
                    ids.len(),
                    plural(ids.len()),
                    plural(ids.len())
                ));
                if let Some(d) = &self.settings.last_dir {
                    dlg = dlg.set_directory(d);
                }
                let Some(dir) = dlg.pick_folder() else { return };
                self.settings.last_dir = Some(dir.clone());
                let wanted = self.library.wanted(&ids);
                let exclude = self.library.files_index().into_keys().collect();
                self.library.missing.search = Some(SearchJob::start(dir, wanted, exclude, ctx));
            }
            MissingRequest::Remove(ids) => {
                if ids.is_empty() {
                    return;
                }
                if let Err(e) = self.missing_target() {
                    self.error = Some(e);
                    return;
                }
                let Some(db) = self.library.database() else { return };
                let books: Vec<&LibraryBook> = ids.iter().filter_map(|id| db.get(id)).collect();
                let items: Vec<CheckItem> = books
                    .iter()
                    .filter_map(|b| Some(CheckItem { id: b.id.clone(), path: PathBuf::from(b.file.clone()?) }))
                    .collect();
                let (tx, rx) = mpsc::channel();
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let r = comicrust_core::missing::check_files(&items, &Progress::default(), &AtomicBool::new(false));
                    let _ = tx.send(r.present.len());
                    ctx2.request_repaint();
                });
                self.missing_ui.remove = Some(RemoveDialog {
                    ids: books.iter().map(|b| b.id.clone()).collect(),
                    captions: books.iter().take(6).map(|b| b.caption()).collect(),
                    existing: None,
                    rx: Some(rx),
                });
            }
        }
    }

    /// Entrée « Livres manquants (N) » du panneau des listes (sous « Toute la bibliothèque »).
    pub(crate) fn missing_list_entry(&mut self, ui: &mut Ui) {
        let n = self.library.missing_count();
        let checking = self.library.missing.check.is_some();
        let warn =
            self.library.missing.last_check.clone().filter(|r| !r.unreachable.is_empty() || !r.blocked.is_empty());
        if n == 0 && !self.library.missing_only && !checking && warn.is_none() {
            return;
        }
        ui.horizontal(|ui| {
            let r = ui.selectable_label(self.library.missing_only, format!("⚠ Livres manquants ({n})"));
            if checking {
                ui.spinner().on_hover_text("Vérification des fichiers en arrière-plan…");
            }
            if r.on_hover_text("Livres dont le fichier est introuvable (déplacé, supprimé, renommé)").clicked() {
                if self.library.selected_list.is_some() {
                    self.select_list(None);
                }
                self.library.missing_only = true;
                self.library.open_stack = None;
            }
        });
        if let Some(r) = &warn {
            let mut text = String::new();
            for u in &r.unreachable {
                text.push_str(&format!("⚠ {} inaccessible : {} livre(s) non vérifié(s).\n", u.root, u.books));
            }
            for b in &r.blocked {
                text.push_str(&format!(
                    "⚠ {} : {} livres sur {} auraient disparu d'un coup ; non marqués (partage partiellement monté ?).\n",
                    b.root, b.gone, b.books
                ));
            }
            ui.label(RichText::new(text.trim_end()).small().color(crate::ui::theme::warning(ui)));
        }
    }

    /// Bandeau de la vue quand le filtre « Livres manquants » est actif.
    pub(crate) fn missing_view_header(&mut self, ui: &mut Ui) {
        if !self.library.missing_only {
            return;
        }
        let n = self.library.missing_count();
        let reason = self.missing_target().err();
        let mut req = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new(format!("⚠ Livres manquants — {n}")).strong());
            if ui.small_button("✖ Toute la bibliothèque").clicked() {
                self.library.missing_only = false;
            }
            let ids = self.library.view_ids();
            ui.add_enabled_ui(reason.is_none() && n > 0, |ui| {
                if ui
                    .small_button("🔍 Rechercher dans un dossier…")
                    .on_hover_text("Cherche les fichiers des livres affichés par nom (puis taille) dans un dossier et ses sous-dossiers")
                    .clicked()
                {
                    req = Some(MissingRequest::SearchFolder(ids.clone()));
                }
                if ui.small_button("🗑 Retirer de la bibliothèque…").on_hover_text("Les livres affichés ; les fichiers ne sont jamais supprimés").clicked() {
                    req = Some(MissingRequest::Remove(ids.clone()));
                }
            });
            if ui
                .add_enabled(self.library.missing.check.is_none(), egui::Button::new("🔄 Vérifier à nouveau").small())
                .clicked()
            {
                req = Some(MissingRequest::Check);
            }
            if let Some(r) = &reason {
                ui.label(RichText::new(format!("🔒 {r}")).small().color(crate::ui::theme::warning(ui)));
            }
        });
        if req.is_some() {
            self.missing_ui.pending = req;
        }
    }

    /// Avertissement de la fiche d'un livre manquant (panneau de détails).
    pub(crate) fn missing_details(&mut self, ui: &mut Ui, book: &LibraryBook) {
        if !is_missing(book) {
            return;
        }
        let reason = self.missing_target().err();
        ui.add_space(4.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("⚠ Fichier introuvable").strong().color(crate::ui::theme::error(ui)));
            ui.label(RichText::new(book.file.as_deref().unwrap_or_default()).small().weak());
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(reason.is_none(), |ui| {
                    if ui.button("📂 Retrouver le fichier…").clicked() {
                        self.missing_ui.pending = Some(MissingRequest::Locate(book.id.clone()));
                    }
                    if ui
                        .button("🗑 Retirer…")
                        .on_hover_text("Retirer de la bibliothèque (le fichier n'est pas touché)")
                        .clicked()
                    {
                        self.missing_ui.pending = Some(MissingRequest::Remove(vec![book.id.clone()]));
                    }
                });
            });
            if let Some(r) = reason {
                ui.label(RichText::new(r).small().weak());
            }
        });
    }

    /// Section « Livres manquants » du menu « 🛠 Fichiers ».
    pub(crate) fn missing_files_menu(&mut self, ui: &mut Ui) {
        ui.separator();
        let n = self.library.missing_count();
        ui.label(RichText::new(format!("Livres manquants ({n})")).strong());
        if ui
            .add_enabled(self.library.missing.check.is_none(), egui::Button::new("🔄 Vérifier les fichiers"))
            .on_hover_text("Cherche, en arrière-plan, les livres dont le fichier n'existe plus")
            .clicked()
        {
            self.missing_ui.pending = Some(MissingRequest::Check);
            ui.close();
        }
        let reason = self.missing_target().err();
        let r = ui.add_enabled(
            reason.is_none() && n > 0,
            egui::Button::new("🔍 Rechercher les fichiers manquants dans un dossier…"),
        );
        if let Some(why) = &reason {
            r.clone().on_disabled_hover_text(why);
        }
        if r.clicked() {
            let db = self.library.database();
            let ids = db
                .map(|d| d.books.iter().filter(|b| is_missing(b)).map(|b| b.id.clone()).collect())
                .unwrap_or_default();
            self.missing_ui.pending = Some(MissingRequest::SearchFolder(ids));
            ui.close();
        }
        let sel = self.library.selection();
        let r = ui.add_enabled(
            reason.is_none() && !sel.is_empty(),
            egui::Button::new(format!("🗑 Retirer la sélection de la bibliothèque ({})…", sel.len())),
        );
        if let Some(why) = &reason {
            r.clone().on_disabled_hover_text(why);
        }
        if r.clicked() {
            self.missing_ui.pending = Some(MissingRequest::Remove(sel));
            ui.close();
        }
    }

    pub(crate) fn missing_windows(&mut self, ctx: &Context) {
        self.not_found_window(ctx);
        self.remove_window(ctx);
        self.relocate_window(ctx);
        self.search_window(ctx);
        self.check_window(ctx);
        self.missing_info_window(ctx);
    }

    fn not_found_window(&mut self, ctx: &Context) {
        let Some(nf) = &self.missing_ui.not_found else { return };
        let reason = self.missing_target().err();
        let mut close = false;
        let mut req = None;
        egui::Modal::new(egui::Id::unique("comicrust-fichier-introuvable")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading("⚠ Fichier introuvable");
            ui.add_space(4.0);
            ui.label(RichText::new(&nf.caption).strong());
            ui.add(egui::Label::new(RichText::new(&nf.file).monospace().small()).wrap());
            ui.add_space(4.0);
            match &nf.volume {
                Volume::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Vérification de l'accès au disque…");
                    });
                }
                Volume::Reachable => {
                    ui.label("Il a peut-être été déplacé, renommé ou supprimé. Le livre est marqué « manquant ».");
                }
                Volume::Unreachable(root) => {
                    ui.label(
                        RichText::new(format!(
                            "{root} ne répond pas (NAS éteint, lecteur réseau déconnecté ?) : le livre n'est pas \
                             marqué manquant. Réessaie quand il sera accessible."
                        ))
                        .color(crate::ui::theme::warning(ui)),
                    );
                }
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(reason.is_none(), |ui| {
                    if ui
                        .button("📂 Retrouver le fichier…")
                        .on_hover_text("Choisir le nouveau fichier du livre")
                        .clicked()
                    {
                        req = Some(MissingRequest::Locate(nf.id.clone()));
                    }
                    if ui
                        .button("🔍 Chercher dans un dossier…")
                        .on_hover_text("Par nom de fichier (puis taille), dans un dossier et ses sous-dossiers")
                        .clicked()
                    {
                        req = Some(MissingRequest::SearchFolder(vec![nf.id.clone()]));
                    }
                    if ui.button("🗑 Retirer de la bibliothèque…").clicked() {
                        req = Some(MissingRequest::Remove(vec![nf.id.clone()]));
                    }
                });
                if ui.button("Fermer").clicked() {
                    close = true;
                }
            });
            if let Some(r) = &reason {
                ui.label(RichText::new(r).small().weak());
            }
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if req.is_some() {
            self.missing_ui.pending = req;
            close = true;
        }
        if close {
            self.missing_ui.not_found = None;
        }
    }

    fn remove_window(&mut self, ctx: &Context) {
        let Some(d) = &self.missing_ui.remove else { return };
        let comicrack = self.comicrack_write_config().is_some();
        let n = d.ids.len();
        let mut go = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("comicrust-retirer")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading("Retirer de la bibliothèque");
            ui.add_space(4.0);
            ui.label(format!(
                "{n} livre{} : {} de la bibliothèque{}. Le fichier n'est jamais supprimé.",
                plural(n),
                if n > 1 { "leurs fiches seront supprimées" } else { "sa fiche sera supprimée" },
                if comicrack { " de ComicRack" } else { "" }
            ));
            ui.label(
                RichText::new(
                    "Les infos qui ne sont pas dans le fichier (lecture, note, nombre d'ouvertures…) seront perdues, et \
                     le livre disparaîtra des listes de lecture.",
                )
                .weak(),
            );
            for c in &d.captions {
                ui.label(RichText::new(format!("• {c}")).small());
            }
            if n > d.captions.len() {
                ui.label(RichText::new(format!("… et {} autre{}", n - d.captions.len(), plural(n - d.captions.len()))).small().weak());
            }
            ui.add_space(4.0);
            match d.existing {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Vérification des fichiers…").weak());
                    });
                }
                Some(0) => {}
                Some(k) => {
                    let again = if comicrack {
                        "ComicRack les ajoutera de nouveau s'il analyse leur dossier"
                    } else {
                        "ils réapparaîtront à la prochaine analyse s'ils sont dans un dossier surveillé"
                    };
                    ui.label(
                        RichText::new(format!(
                            "⚠ {k} fichier{} existe{} encore : {again}.",
                            plural(k),
                            if k > 1 { "nt" } else { "" }
                        ))
                        .color(crate::ui::theme::warning(ui)),
                    );
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = format!("Retirer {n} livre{}", plural(n));
                if ui.button(RichText::new(label).strong()).clicked() {
                    go = true;
                }
                if ui.button("Annuler").clicked() {
                    cancel = true;
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if go {
            let d = self.missing_ui.remove.take().unwrap();
            match self.missing_target() {
                Ok(t) => {
                    let sink = self.library.watcher.as_ref().map(|w| w.deleted.clone());
                    let rx = crate::library::missing::remove(t, d.ids, sink, ctx);
                    self.library.missing.push_op(rx);
                }
                Err(e) => self.error = Some(e),
            }
        } else if cancel {
            self.missing_ui.remove = None;
        }
    }

    fn relocate_window(&mut self, ctx: &Context) {
        let reason = self.missing_target().err();
        let Some(d) = &mut self.missing_ui.relocate else { return };
        let mut go = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("comicrust-retrouver")).show(ctx, |ui| {
            ui.set_max_width(640.0);
            ui.heading("Retrouver les fichiers");
            ui.label(RichText::new(format!("Source : {}", d.source)).weak().small());
            ui.add_space(4.0);
            let n = d.proposals.len();
            ui.label(format!(
                "{n} fichier{} retrouvé{}. La fiche de chaque livre coché indiquera le nouveau fichier ; lecture, note \
                 et infos sont gardées.",
                plural(n),
                plural(n)
            ));
            if d.not_found > 0 {
                ui.label(
                    RichText::new(format!(
                        "{} livre{} non retrouvé{}.",
                        d.not_found,
                        plural(d.not_found),
                        plural(d.not_found)
                    ))
                    .color(crate::ui::theme::warning(ui)),
                );
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for p in &mut d.proposals {
                    ui.checkbox(&mut p.checked, RichText::new(&p.caption).strong());
                    ui.indent(("prop", &p.id), |ui| {
                        ui.label(RichText::new(format!("avant : {}", p.old)).small().weak());
                        ui.label(RichText::new(format!("après : {}", p.new.display())).small());
                        let mut how = p.kind.label().to_string();
                        if p.others > 0 {
                            how.push_str(&format!(
                                " ; {} autre{} fichier{} de même nom",
                                p.others,
                                plural(p.others),
                                plural(p.others)
                            ));
                        }
                        let color =
                            if p.kind.sure() { ui.visuals().weak_text_color() } else { crate::ui::theme::warning(ui) };
                        ui.label(RichText::new(how).small().color(color));
                        if let Some(dup) = &p.duplicate {
                            ui.label(
                                RichText::new(format!("⚠ Ce fichier est déjà dans la bibliothèque : « {dup} »"))
                                    .small()
                                    .color(crate::ui::theme::warning(ui)),
                            );
                        }
                    });
                }
            });
            if let Some(r) = &reason {
                ui.label(RichText::new(format!("🔒 {r}")).color(crate::ui::theme::warning(ui)));
            }
            ui.add_space(8.0);
            let k = d.proposals.iter().filter(|p| p.checked).count();
            ui.horizontal(|ui| {
                let label = format!("Mettre à jour {k} fiche{}", plural(k));
                if ui.add_enabled(k > 0 && reason.is_none(), egui::Button::new(RichText::new(label).strong())).clicked()
                {
                    go = true;
                }
                if ui.button("Annuler").clicked() {
                    cancel = true;
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if go {
            let d = self.missing_ui.relocate.take().unwrap();
            let Some(db) = self.library.database() else { return };
            let books: Vec<(LibraryBook, PathBuf)> = d
                .proposals
                .into_iter()
                .filter(|p| p.checked)
                .filter_map(|p| Some((db.get(&p.id)?.clone(), p.new)))
                .collect();
            match self.missing_target() {
                Ok(t) => {
                    let sink = self.library.watcher.as_ref().map(|w| w.written.clone());
                    let rx = crate::library::missing::relocate(t, books, sink, ctx);
                    self.library.missing.push_op(rx);
                }
                Err(e) => self.error = Some(e),
            }
        } else if cancel {
            self.missing_ui.relocate = None;
        }
    }

    fn search_window(&mut self, ctx: &Context) {
        let Some(job) = &self.library.missing.search else { return };
        let (done, total) = job.progress.get();
        let mut stop = false;
        egui::Window::new("Recherche des fichiers manquants")
            .id(egui::Id::unique("comicrust-recherche-manquants"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Dossier : {}", job.dir.display()));
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(if done == 0 {
                        format!("{total} fichiers parcourus…")
                    } else {
                        "Comparaison…".to_string()
                    });
                });
                if ui.button("⏹ Arrêter").clicked() {
                    stop = true;
                }
            });
        if stop {
            job.cancel.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    fn check_window(&mut self, ctx: &Context) {
        if !self.missing_ui.show_check {
            return;
        }
        let mut open = true;
        let mut show = false;
        egui::Window::new("Vérification des fichiers")
            .id(egui::Id::unique("comicrust-verification-manquants"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                if let Some(job) = &self.library.missing.check {
                    let (done, total) = job.progress.get();
                    let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                    ui.add(egui::ProgressBar::new(frac).text(format!("{done} / {total}")).desired_width(360.0));
                    if ui.button("⏹ Arrêter").clicked() {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(150));
                } else if let Some(r) = &self.library.missing.last_check {
                    ui.label(RichText::new(r.summary()).strong());
                    if !r.missing.is_empty() && ui.button("⚠ Voir les livres manquants").clicked() {
                        show = true;
                    }
                }
            });
        if show {
            if self.library.selected_list.is_some() {
                self.select_list(None);
            }
            self.library.missing_only = true;
            open = false;
        }
        self.missing_ui.show_check = open;
    }

    fn missing_info_window(&mut self, ctx: &Context) {
        let Some(msg) = self.missing_ui.info.clone() else { return };
        let mut close = false;
        egui::Modal::new(egui::Id::unique("comicrust-manquants-info")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.label(msg);
            ui.add_space(8.0);
            if ui.button("OK").clicked() {
                close = true;
            }
        });
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)) {
            self.missing_ui.info = None;
        }
    }
}
