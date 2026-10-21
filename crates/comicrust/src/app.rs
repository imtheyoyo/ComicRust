//! Fenêtre principale de ComicRust : lecteur, vignettes, liste de lecture.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use comicrust_core::providers::BOOK_EXTENSIONS;
use comicrust_core::{BookFormat, ComicInfo};
use eframe::egui::{
    self, pos2, vec2, Align, Color32, Context, Key, Layout, Rect, RichText, Sense, TextureHandle,
    TextureOptions, Ui, Vec2, ViewportCommand,
};

use crate::loader::{BookLoader, Event, Request, THUMB_HEIGHT};
use crate::reading_list::ListState;
use comicrust_core::index::ScanProgress;
use crate::settings::{FitMode, Settings};

const SETTINGS_KEY: &str = "comicrust.settings";
/// Pages gardées en mémoire graphique autour de la page courante.
const KEEP_BEHIND: usize = 3;
const KEEP_AHEAD: usize = 6;
const PRELOAD_AHEAD: usize = 3;
const THUMB_ROW: f32 = THUMB_HEIGHT as f32 * 0.6 + 26.0;

struct Book {
    path: PathBuf,
    title: String,
    loader: BookLoader,
    format: Option<BookFormat>,
    pages: Vec<String>,
    info: Option<ComicInfo>,
    /// Taille d'origine des pages déjà décodées.
    sizes: HashMap<usize, [u32; 2]>,
    textures: HashMap<usize, TextureHandle>,
    errors: HashMap<usize, String>,
    requested: HashSet<usize>,
    thumbs: HashMap<usize, Option<TextureHandle>>,
    thumbs_requested: HashSet<usize>,
    current: usize,
    opened: bool,
}

impl Book {
    fn page_count(&self) -> usize {
        self.pages.len()
    }
}

pub struct ComicRustApp {
    settings: Settings,
    book: Option<Book>,
    list: Option<ListState>,
    list_loading: Option<Receiver<Result<ListState, String>>>,
    list_progress: std::sync::Arc<ScanProgress>,
    /// Dossier des données de l'application (cache de l'index des BD).
    data_dir: Option<PathBuf>,
    zoom: f32,
    fullscreen: bool,
    show_info: bool,
    show_about: bool,
    show_list_panel: bool,
    error: Option<String>,
    last_wheel_flip: Instant,
    title: String,
    scroll_to_thumb: bool,
}

fn texture_options() -> TextureOptions {
    TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR }
}

impl ComicRustApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>, data_dir: Option<PathBuf>) -> Self {
        let settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
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
        };
        if let Some(p) = initial {
            app.open_path(&cc.egui_ctx, p);
        }
        app
    }

    // ------------------------------------------------------------------
    // Ouverture

    fn open_path(&mut self, ctx: &Context, path: PathBuf) {
        let is_cbl = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cbl"));
        self.settings.last_dir = path.parent().map(Path::to_path_buf);
        if is_cbl {
            self.load_list(ctx, path);
        } else {
            self.open_book(ctx, path);
        }
    }

    fn open_book(&mut self, ctx: &Context, path: PathBuf) {
        self.remember_position();
        let max_side = ctx.input(|i| i.max_texture_side).min(16384) as u32;
        let loader = BookLoader::open(path.clone(), ctx.clone(), max_side);
        let title = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let current = self.settings.last_pages.get(&path).copied().unwrap_or(0);
        self.settings.push_recent(path.clone());
        if let Some(list) = &mut self.list {
            list.current = list.position_of(&path);
        }
        self.book = Some(Book {
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
        });
        self.zoom = 1.0;
        self.scroll_to_thumb = true;
    }

    fn load_list(&mut self, ctx: &Context, path: PathBuf) {
        self.settings.push_recent(path.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let db = self.settings.library_db.clone();
        let root = self.settings.comics_root.clone();
        let cache = self.data_dir.as_ref().map(|d| d.join("index-bd.json"));
        let progress = std::sync::Arc::new(ScanProgress::default());
        self.list_progress = progress.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ListState::load(&path, db.as_deref(), root.as_deref(), cache.as_deref(), Some(&progress)));
            ctx.request_repaint();
        });
        self.list_loading = Some(rx);
        self.show_list_panel = true;
    }

    fn reload_list(&mut self, ctx: &Context) {
        if let Some(path) = self.list.as_ref().map(|l| l.path.clone()) {
            self.load_list(ctx, path);
        }
    }

    fn open_dialog(&mut self, ctx: &Context) {
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

    fn open_folder_dialog(&mut self, ctx: &Context) {
        if let Some(p) = rfd::FileDialog::new().set_title("Ouvrir un dossier d'images").pick_folder() {
            self.open_path(ctx, p);
        }
    }

    fn remember_position(&mut self) {
        if let Some(b) = &self.book {
            if b.opened {
                self.settings.last_pages.insert(b.path.clone(), b.current);
            }
        }
    }

    // ------------------------------------------------------------------
    // Événements du chargeur

    fn poll_loader(&mut self, ctx: &Context) {
        if let Some(rx) = &self.list_loading {
            if let Ok(res) = rx.try_recv() {
                self.list_loading = None;
                match res {
                    Ok(mut list) => {
                        let current_path = self.book.as_ref().map(|b| b.path.clone());
                        list.current = current_path.as_deref().and_then(|p| list.position_of(p));
                        let first = list.entries.iter().position(|e| e.file.is_some());
                        let open_first = list.current.is_none();
                        let file = first.and_then(|i| list.entries[i].file.clone());
                        self.list = Some(list);
                        if open_first {
                            if let Some(f) = file {
                                self.open_book(ctx, f);
                            }
                        }
                    }
                    Err(e) => self.error = Some(format!("Liste de lecture illisible : {e}")),
                }
            }
        }

        let Some(book) = &mut self.book else { return };
        let mut failed = None;
        for ev in book.loader.poll() {
            match ev {
                Event::Opened { format, pages, info } => {
                    book.format = Some(format);
                    book.pages = pages;
                    book.info = info.map(|b| *b);
                    book.opened = true;
                    if book.current >= book.pages.len() {
                        book.current = 0;
                    }
                    if let Some(caption) = book.info.as_ref().map(ComicInfo::caption).filter(|c| !c.is_empty()) {
                        book.title = caption;
                    }
                }
                Event::Page { index, result } => {
                    book.requested.remove(&index);
                    match result {
                        Ok((img, size)) => {
                            book.sizes.insert(index, size);
                            let tex = ctx.load_texture(format!("page-{index}"), img, texture_options());
                            book.textures.insert(index, tex);
                            book.errors.remove(&index);
                        }
                        Err(e) => {
                            book.errors.insert(index, e);
                        }
                    }
                }
                Event::Thumb { index, image } => {
                    let tex = image.map(|img| ctx.load_texture(format!("thumb-{index}"), img, TextureOptions::LINEAR));
                    book.thumbs.insert(index, tex);
                }
                Event::Failed(e) => failed = Some(e),
            }
        }
        if let Some(e) = failed {
            let name = book.path.display().to_string();
            self.settings.recent.retain(|p| p != &book.path);
            self.book = None;
            self.error = Some(format!("Impossible d'ouvrir {name}\n\n{e}"));
        }
    }

    // ------------------------------------------------------------------
    // Pagination

    fn is_wide(book: &Book, i: usize) -> bool {
        book.sizes.get(&i).is_some_and(|[w, h]| *w > *h)
    }

    /// Pages affichées ensemble à partir de `start` (ordre de lecture).
    fn spread(&self, book: &Book, start: usize) -> Vec<usize> {
        let n = book.page_count();
        if n == 0 {
            return vec![];
        }
        let start = start.min(n - 1);
        if !self.settings.double_page
            || start + 1 >= n
            || (self.settings.cover_alone && start == 0)
            || Self::is_wide(book, start)
            || Self::is_wide(book, start + 1)
        {
            return vec![start];
        }
        vec![start, start + 1]
    }

    fn go_to(&mut self, page: usize) {
        if let Some(b) = &mut self.book {
            if b.page_count() > 0 {
                b.current = page.min(b.page_count() - 1);
                self.scroll_to_thumb = true;
            }
        }
    }

    fn next_page(&mut self, ctx: &Context) {
        let Some(book) = &self.book else { return };
        let step = self.spread(book, book.current).len().max(1);
        let target = book.current + step;
        if target < book.page_count() {
            self.go_to(target);
        } else {
            self.next_book(ctx, false);
        }
    }

    fn prev_page(&mut self, ctx: &Context) {
        let Some(book) = &self.book else { return };
        if book.current == 0 {
            self.next_book(ctx, true);
            return;
        }
        let mut target = book.current - 1;
        if self.settings.double_page && target > 0 && self.spread(book, target - 1).len() == 2 {
            target -= 1;
        }
        self.go_to(target);
    }

    /// Livre suivant/précédent de la liste de lecture, s'il y en a une.
    fn next_book(&mut self, ctx: &Context, backward: bool) {
        let Some(list) = &self.list else { return };
        let Some(cur) = list.current else { return };
        if let Some(i) = list.neighbour(cur, backward) {
            if let Some(f) = list.entries[i].file.clone() {
                self.open_book(ctx, f);
            }
        }
    }

    /// Demande au chargeur les pages visibles et proches, libère les autres.
    fn schedule_loading(&mut self) {
        let Some(book) = &self.book else { return };
        if !book.opened {
            return;
        }
        let visible = self.spread(book, book.current);
        let last_visible = *visible.last().unwrap_or(&book.current);
        let mut wanted: Vec<usize> = (last_visible + 1..(last_visible + 1 + PRELOAD_AHEAD).min(book.page_count()))
            .rev()
            .collect();
        if book.current > 0 {
            wanted.insert(0, book.current - 1);
        }
        wanted.extend(visible.iter().rev()); // en dernier = traité en premier
        let lo = book.current.saturating_sub(KEEP_BEHIND);
        let hi = book.current + KEEP_AHEAD;

        let book = self.book.as_mut().unwrap();
        book.textures.retain(|&i, _| i >= lo && i <= hi);
        for i in wanted {
            if !book.textures.contains_key(&i) && !book.errors.contains_key(&i) && book.requested.insert(i) {
                book.loader.request(Request::Page(i));
            }
        }
    }

    // ------------------------------------------------------------------
    // Clavier, souris, glisser-déposer

    fn handle_input(&mut self, ctx: &Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(p) = dropped.into_iter().next() {
            self.open_path(ctx, p);
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let manga = self.settings.manga;
        let (mut next, mut prev) = (false, false);
        let mut actions: Vec<Key> = Vec::new();
        ctx.input(|i| {
            for k in [
                Key::ArrowRight, Key::ArrowLeft, Key::Space, Key::Backspace, Key::PageDown, Key::PageUp,
                Key::Home, Key::End, Key::F11, Key::Escape, Key::Num1, Key::Num2, Key::Num3, Key::Num4,
                Key::D, Key::M, Key::T, Key::I, Key::L, Key::F, Key::Plus, Key::Equals, Key::Minus, Key::Num0, Key::O,
            ] {
                if i.key_pressed(k) {
                    actions.push(k);
                }
            }
            if i.modifiers.command && i.key_pressed(Key::O) {
                actions.push(Key::F1); // marqueur "ouvrir"
            }
        });
        for k in actions {
            match k {
                Key::ArrowRight => if manga { prev = true } else { next = true },
                Key::ArrowLeft => if manga { next = true } else { prev = true },
                Key::Space | Key::PageDown => next = true,
                Key::Backspace | Key::PageUp => prev = true,
                Key::Home => self.go_to(0),
                Key::End => self.go_to(usize::MAX),
                Key::F11 | Key::F => self.toggle_fullscreen(ctx),
                Key::Escape if self.fullscreen => self.toggle_fullscreen(ctx),
                Key::Num1 => self.settings.fit = FitMode::Page,
                Key::Num2 => self.settings.fit = FitMode::Width,
                Key::Num3 => self.settings.fit = FitMode::Height,
                Key::Num4 => self.settings.fit = FitMode::Original,
                Key::D => self.settings.double_page ^= true,
                Key::M => self.settings.manga ^= true,
                Key::T => self.settings.show_thumbnails ^= true,
                Key::I => self.show_info ^= true,
                Key::L => self.show_list_panel ^= true,
                Key::Plus | Key::Equals => self.zoom = (self.zoom * 1.25).min(8.0),
                Key::Minus => self.zoom = (self.zoom / 1.25).max(0.1),
                Key::Num0 => self.zoom = 1.0,
                Key::F1 => self.open_dialog(ctx),
                _ => {}
            }
        }
        if next {
            self.next_page(ctx);
        }
        if prev {
            self.prev_page(ctx);
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn update_title(&mut self, ctx: &Context) {
        let title = match &self.book {
            Some(b) if b.opened => format!("{} — ComicRust", b.title),
            Some(b) => format!("{} (ouverture…) — ComicRust", b.title),
            None => "ComicRust".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    // ------------------------------------------------------------------
    // Interface

    fn toolbar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            if ui.button("📂 Ouvrir").on_hover_text("Ouvrir une BD ou une liste .cbl (Ctrl+O)").clicked() {
                self.open_dialog(&ctx);
            }
            ui.menu_button("🕘 Récents", |ui| {
                if self.settings.recent.is_empty() {
                    ui.label("Aucun fichier récent");
                }
                let mut chosen = None;
                for p in &self.settings.recent {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    if ui.button(name.as_ref()).on_hover_text(p.display().to_string()).clicked() {
                        chosen = Some(p.clone());
                    }
                }
                ui.separator();
                if ui.button("📁 Ouvrir un dossier d'images…").clicked() {
                    self.open_folder_dialog(&ctx);
                    ui.close();
                }
                if let Some(p) = chosen {
                    self.open_path(&ctx, p);
                    ui.close();
                }
            });
            ui.separator();

            let has_book = self.book.as_ref().is_some_and(|b| b.opened && b.page_count() > 0);
            ui.add_enabled_ui(has_book, |ui| {
                let (first, prev, next, last) = if self.settings.manga { ("⏭", "▶", "◀", "⏮") } else { ("⏮", "◀", "▶", "⏭") };
                if ui.button(first).on_hover_text("Première page (Début)").clicked() {
                    self.go_to(0);
                }
                if ui.button(prev).on_hover_text("Page précédente").clicked() {
                    self.prev_page(&ctx);
                }
                if let Some(book) = &self.book {
                    let n = book.page_count().max(1);
                    let mut page = book.current + 1;
                    let resp = ui.add(egui::Slider::new(&mut page, 1..=n).show_value(false).trailing_fill(true));
                    ui.label(format!("{} / {}", book.current + 1, book.page_count()));
                    if resp.changed() {
                        self.go_to(page - 1);
                    }
                }
                if ui.button(next).on_hover_text("Page suivante").clicked() {
                    self.next_page(&ctx);
                }
                if ui.button(last).on_hover_text("Dernière page (Fin)").clicked() {
                    self.go_to(usize::MAX);
                }
            });
            ui.separator();

            egui::ComboBox::from_id_salt("fit")
                .selected_text(self.settings.fit.label())
                .show_ui(ui, |ui| {
                    for (i, m) in FitMode::ALL.into_iter().enumerate() {
                        ui.selectable_value(&mut self.settings.fit, m, format!("{}  ({})", m.label(), i + 1));
                    }
                });
            if ui.button("➖").on_hover_text("Zoom arrière (-)").clicked() {
                self.zoom = (self.zoom / 1.25).max(0.1);
            }
            if ui.button(format!("{:.0} %", self.zoom * 100.0)).on_hover_text("Zoom 100 % (0)").clicked() {
                self.zoom = 1.0;
            }
            if ui.button("➕").on_hover_text("Zoom avant (+)").clicked() {
                self.zoom = (self.zoom * 1.25).min(8.0);
            }
            ui.separator();
            ui.toggle_value(&mut self.settings.double_page, "📖 Double page").on_hover_text("D");
            ui.toggle_value(&mut self.settings.manga, "🈁 Manga").on_hover_text("Lecture de droite à gauche (M)");
            ui.toggle_value(&mut self.settings.show_thumbnails, "▦ Vignettes").on_hover_text("T");
            if self.list.is_some() || self.list_loading.is_some() {
                ui.toggle_value(&mut self.show_list_panel, "☰ Liste").on_hover_text("L");
            }
            ui.toggle_value(&mut self.show_info, "ℹ Infos").on_hover_text("I");
            if ui.button("⛶ Plein écran").on_hover_text("F11 / F, Échap pour sortir").clicked() {
                self.toggle_fullscreen(&ctx);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("?").on_hover_text("À propos et raccourcis").clicked() {
                    self.show_about = true;
                }
            });
        });
    }

    fn status_bar(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            match &self.book {
                None => {
                    ui.label("Aucune BD ouverte — glissez un fichier dans la fenêtre ou cliquez sur « Ouvrir ».");
                }
                Some(b) if !b.opened => {
                    ui.spinner();
                    ui.label(format!("Ouverture de {}…", b.path.display()));
                }
                Some(b) => {
                    ui.label(RichText::new(&b.title).strong());
                    ui.separator();
                    ui.label(b.format.map(|f| f.label()).unwrap_or(""));
                    ui.separator();
                    let visible = self.spread(b, b.current);
                    let pages: Vec<String> = visible.iter().map(|i| (i + 1).to_string()).collect();
                    ui.label(format!("Page {} sur {}", pages.join("-"), b.page_count()));
                    if let Some(name) = b.pages.get(b.current) {
                        ui.separator();
                        ui.label(RichText::new(name).weak());
                    }
                    if let Some([w, h]) = b.sizes.get(&b.current) {
                        ui.separator();
                        ui.label(RichText::new(format!("{w} × {h}")).weak());
                    }
                    if !b.requested.is_empty() {
                        ui.separator();
                        ui.spinner();
                    }
                }
            }
        });
    }

    fn thumbnails_panel(&mut self, ui: &mut Ui) {
        let Some(book) = &self.book else { return };
        if !book.opened {
            return;
        }
        let n = book.page_count();
        let current = book.current;
        let scroll_to = std::mem::take(&mut self.scroll_to_thumb);
        let mut clicked = None;
        let mut needed = Vec::new();
        let mut area = egui::ScrollArea::vertical().auto_shrink(false);
        if scroll_to {
            let viewport_h = ui.available_height();
            let offset = (current as f32 * (THUMB_ROW + ui.spacing().item_spacing.y) - viewport_h / 2.0 + THUMB_ROW / 2.0).max(0.0);
            area = area.vertical_scroll_offset(offset);
        }
        area.show_rows(ui, THUMB_ROW, n, |ui, range| {
            for i in range {
                let selected = self.spread(book, current).contains(&i);
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), THUMB_ROW), Sense::click());
                let painter = ui.painter_at(rect);
                if selected {
                    painter.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.6));
                } else if resp.hovered() {
                    painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
                }
                let img_rect = Rect::from_min_max(rect.min + vec2(6.0, 4.0), rect.max - vec2(6.0, 22.0));
                match book.thumbs.get(&i) {
                    Some(Some(tex)) => {
                        let s = tex.size_vec2();
                        let k = (img_rect.width() / s.x).min(img_rect.height() / s.y);
                        let r = Rect::from_center_size(img_rect.center(), s * k);
                        painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    Some(None) => {
                        painter.text(img_rect.center(), egui::Align2::CENTER_CENTER, "⚠", egui::FontId::proportional(20.0), Color32::LIGHT_RED);
                    }
                    None => {
                        painter.rect_stroke(img_rect.shrink(img_rect.width() * 0.2), 2.0, (1.0, Color32::DARK_GRAY), egui::StrokeKind::Inside);
                        needed.push(i);
                    }
                }
                painter.text(
                    pos2(rect.center().x, rect.max.y - 11.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", i + 1),
                    egui::FontId::proportional(12.0),
                    if selected { Color32::WHITE } else { Color32::GRAY },
                );
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
        });
        let book = self.book.as_mut().unwrap();
        for i in needed {
            if book.thumbs_requested.insert(i) {
                book.loader.request(Request::Thumb(i));
            }
        }
        if let Some(i) = clicked {
            book.current = i;
        }
    }

    fn list_panel(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.list_loading.is_some() {
            let (done, total) = self.list_progress.get();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Recherche des fichiers de la liste…");
            });
            if total > 0 {
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).text(format!("{done} / {total} fichiers lus")));
            } else {
                ui.label(RichText::new("Parcours des dossiers…").weak());
            }
            ctx.request_repaint_after(Duration::from_millis(150));
            return;
        }
        let Some(list) = &self.list else { return };
        ui.heading(&list.name);
        ui.label(format!("{} / {} numéros trouvés", list.found(), list.entries.len()));
        ui.label(RichText::new(&list.source).weak().small());
        if let Some(summary) = &list.index_summary {
            ui.label(RichText::new(summary).weak().small());
        }
        if list.found() == 0 && list.default_root {
            ui.label(RichText::new("Aucun fichier trouvé dans le dossier de la liste : choisis le dossier où sont rangées tes BD.").color(Color32::from_rgb(230, 170, 60)));
        }
        let mut action = None::<ListAction>;
        ui.horizontal_wrapped(|ui| {
            if ui.button("📁 Dossier des BD…").on_hover_text("Dossier parcouru pour retrouver les fichiers").clicked() {
                action = Some(ListAction::PickRoot);
            }
            if ui.button("🗄 ComicDB.xml…").on_hover_text("Base ComicRack (optionnelle)").clicked() {
                action = Some(ListAction::PickDb);
            }
            if ui.button("📋 Copier le diagnostic").on_hover_text("Bilan, numéros manquants et fichiers non reconnus").clicked() {
                ui.ctx().copy_text(list.diagnostic());
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, e) in list.entries.iter().enumerate() {
                let text = format!("{:>3}. {}", i + 1, e.label());
                let rich = if e.file.is_some() { RichText::new(text) } else { RichText::new(text).weak().strikethrough() };
                let resp = ui.selectable_label(list.current == Some(i), rich);
                let resp = match &e.file {
                    Some(f) => resp.on_hover_text(f.display().to_string()),
                    None => resp.on_hover_text("Fichier introuvable"),
                };
                if resp.clicked() {
                    if let Some(f) = &e.file {
                        action = Some(ListAction::Open(f.clone()));
                    }
                }
            }
        });
        match action {
            Some(ListAction::Open(f)) => self.open_book(&ctx, f),
            Some(ListAction::PickRoot) => {
                if let Some(d) = rfd::FileDialog::new().set_title("Dossier des BD").pick_folder() {
                    self.settings.comics_root = Some(d);
                    self.reload_list(&ctx);
                }
            }
            Some(ListAction::PickDb) => {
                if let Some(f) = rfd::FileDialog::new().add_filter("ComicDB.xml", &["xml"]).pick_file() {
                    self.settings.library_db = Some(f);
                    self.reload_list(&ctx);
                }
            }
            None => {}
        }
    }

    fn viewer(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let bg = Color32::from_rgb(self.settings.background[0], self.settings.background[1], self.settings.background[2]);
        ui.painter().rect_filled(ui.max_rect(), 0.0, bg);

        let Some(book) = &self.book else {
            self.welcome(ui);
            return;
        };
        if !book.opened {
            ui.centered_and_justified(|ui| ui.spinner());
            return;
        }
        if book.page_count() == 0 {
            ui.centered_and_justified(|ui| ui.label("Ce livre ne contient aucune page lisible."));
            return;
        }

        let mut spread = self.spread(book, book.current);
        // Taille de chaque page ramenée à une hauteur commune.
        let sizes: Vec<Vec2> = spread
            .iter()
            .map(|i| match book.sizes.get(i) {
                Some([w, h]) => vec2(*w as f32, *h as f32),
                None => vec2(1000.0, 1500.0),
            })
            .collect();
        let ref_h = sizes.iter().map(|s| s.y).fold(1.0, f32::max);
        let scaled: Vec<Vec2> = sizes.iter().map(|s| vec2(s.x * ref_h / s.y, ref_h)).collect();
        let total = vec2(scaled.iter().map(|s| s.x).sum(), ref_h);
        let avail = ui.available_size();
        let fit = match self.settings.fit {
            FitMode::Page => (avail.x / total.x).min(avail.y / total.y),
            FitMode::Width => avail.x / total.x,
            FitMode::Height => avail.y / total.y,
            FitMode::Original => 1.0,
        };
        let scale = fit * self.zoom;
        let display = total * scale;
        let scrollable = display.x > avail.x + 1.0 || display.y > avail.y + 1.0;

        let mut clicked_at: Option<(f32, Rect)> = None;
        egui::ScrollArea::both()
            .id_salt(("page", book.path.as_os_str(), book.current))
            .auto_shrink(false)
            .show(ui, |ui| {
                let area = display.max(avail);
                let (rect, resp) = ui.allocate_exact_size(area, Sense::click());
                let content = Rect::from_center_size(rect.center(), display);
                let painter = ui.painter();
                let order: Vec<usize> = if self.settings.manga { (0..spread.len()).rev().collect() } else { (0..spread.len()).collect() };
                let mut x = content.min.x;
                for k in order {
                    let idx = spread[k];
                    let size = scaled[k] * scale;
                    let r = Rect::from_min_size(pos2(x, content.min.y), size);
                    x += size.x;
                    if let Some(tex) = book.textures.get(&idx) {
                        painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    } else if let Some(err) = book.errors.get(&idx) {
                        painter.rect_stroke(r.shrink(8.0), 4.0, (1.0, Color32::from_rgb(160, 60, 60)), egui::StrokeKind::Inside);
                        painter.text(r.center(), egui::Align2::CENTER_CENTER, format!("⚠ Page {} illisible\n{err}", idx + 1), egui::FontId::proportional(15.0), Color32::LIGHT_RED);
                    } else {
                        painter.rect_filled(r.shrink(4.0), 2.0, Color32::from_gray(36));
                        painter.text(r.center(), egui::Align2::CENTER_CENTER, "Chargement…", egui::FontId::proportional(16.0), Color32::GRAY);
                    }
                }
                if resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        clicked_at = Some((p.x, ui.clip_rect()));
                    }
                }
            });
        spread.clear();

        // Clic sur le tiers gauche/droit de la vue : page précédente/suivante.
        if let Some((x, view)) = clicked_at {
            let rel = (x - view.min.x) / view.width();
            let forward = if self.settings.manga { rel < 0.33 } else { rel > 0.67 };
            let backward = if self.settings.manga { rel > 0.67 } else { rel < 0.33 };
            if forward {
                self.next_page(&ctx);
            } else if backward {
                self.prev_page(&ctx);
            }
        }

        // Molette : change de page quand il n'y a rien à faire défiler.
        if !scrollable && ui.rect_contains_pointer(ui.max_rect()) {
            let dy: f32 = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .sum()
            });
            if dy != 0.0 && self.last_wheel_flip.elapsed() > Duration::from_millis(180) {
                self.last_wheel_flip = Instant::now();
                if dy < 0.0 {
                    self.next_page(&ctx);
                } else {
                    self.prev_page(&ctx);
                }
            }
        }
    }

    fn welcome(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.2);
            ui.heading(RichText::new("ComicRust").size(36.0).strong());
            ui.label("Lecteur de BD — CBZ, CBR, CB7, CBT, PDF, DjVu, CBW, ZIP, RAR, 7z, TAR, dossiers et listes .cbl");
            ui.add_space(16.0);
            if ui.button(RichText::new("📂  Ouvrir une BD…").size(18.0)).clicked() {
                self.open_dialog(&ctx);
            }
            ui.label(RichText::new("ou glissez-déposez un fichier dans la fenêtre").weak());
            if !self.settings.recent.is_empty() {
                ui.add_space(24.0);
                ui.label(RichText::new("Récemment ouverts").strong());
                let mut chosen = None;
                for p in self.settings.recent.iter().take(8) {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    if ui.link(name.as_ref()).on_hover_text(p.display().to_string()).clicked() {
                        chosen = Some(p.clone());
                    }
                }
                if let Some(p) = chosen {
                    self.open_path(&ctx, p);
                }
            }
        });
    }

    fn info_window(&mut self, ctx: &Context) {
        let Some(book) = &self.book else { return };
        let mut open = self.show_info;
        egui::Window::new("Informations").open(&mut open).default_width(360.0).show(ctx, |ui| {
            egui::Grid::new("info").num_columns(2).striped(true).show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    if !v.is_empty() {
                        ui.label(RichText::new(k).strong());
                        ui.label(v);
                        ui.end_row();
                    }
                };
                row("Fichier", book.path.display().to_string());
                row("Format", book.format.map(|f| f.label().to_string()).unwrap_or_default());
                row("Pages", book.page_count().to_string());
                if let Some(ci) = &book.info {
                    let o = |v: &Option<String>| v.clone().unwrap_or_default();
                    let n = |v: Option<i32>| v.map(|x| x.to_string()).unwrap_or_default();
                    row("Série", o(&ci.series));
                    row("Numéro", o(&ci.number));
                    row("Volume", n(ci.volume));
                    row("Titre", o(&ci.title));
                    row("Année", n(ci.year));
                    row("Scénario", o(&ci.writer));
                    row("Dessin", o(&ci.penciller));
                    row("Encrage", o(&ci.inker));
                    row("Couleurs", o(&ci.colorist));
                    row("Couverture", o(&ci.cover_artist));
                    row("Éditeur", o(&ci.publisher));
                    row("Genre", o(&ci.genre));
                    row("Personnages", o(&ci.characters));
                    row("Arc", o(&ci.story_arc));
                    row("Langue", o(&ci.language_iso));
                    row("Manga", o(&ci.manga));
                }
            });
            if let Some(summary) = book.info.as_ref().and_then(|c| c.summary.clone()) {
                ui.separator();
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| ui.label(summary));
            }
        });
        self.show_info = open;
    }

    fn about_window(&mut self, ctx: &Context) {
        let mut open = self.show_about;
        egui::Window::new("À propos de ComicRust").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(format!("ComicRust {}", env!("CARGO_PKG_VERSION"))).strong());
            ui.label("Lecteur de BD en Rust, inspiré de ComicRack.");
            ui.hyperlink("https://github.com/imtheyoyo/ComicRust");
            ui.separator();
            match comicrust_core::providers::pdfium_status() {
                Ok(()) => ui.label("PDF : PDFium chargé ✔"),
                Err(e) => ui.label(RichText::new(format!("PDF : {e}")).color(Color32::LIGHT_RED)),
            };
            ui.separator();
            egui::Grid::new("keys").num_columns(2).show(ui, |ui| {
                for (k, v) in [
                    ("→ / Espace / Pg↓", "Page suivante"),
                    ("← / Retour arrière / Pg↑", "Page précédente"),
                    ("Début / Fin", "Première / dernière page"),
                    ("Clic à gauche / à droite", "Page précédente / suivante"),
                    ("Molette", "Changer de page (mode page entière)"),
                    ("1 2 3 4", "Page entière, largeur, hauteur, taille réelle"),
                    ("+ / - / 0", "Zoom"),
                    ("D", "Double page"),
                    ("M", "Sens manga"),
                    ("T / L / I", "Vignettes / liste / infos"),
                    ("F11 ou F, Échap", "Plein écran"),
                    ("Ctrl+O", "Ouvrir"),
                ] {
                    ui.label(RichText::new(k).monospace());
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
        self.show_about = open;
    }

    fn error_window(&mut self, ctx: &Context) {
        let Some(msg) = self.error.clone() else { return };
        let mut close = false;
        egui::Modal::new(egui::Id::unique("comicrust-error")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.heading("⚠ Erreur");
            ui.label(msg);
            ui.add_space(8.0);
            if ui.button("OK").clicked() {
                close = true;
            }
        });
        if close {
            self.error = None;
        }
    }
}

enum ListAction {
    Open(PathBuf),
    PickRoot,
    PickDb,
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
            if self.settings.show_thumbnails && self.book.as_ref().is_some_and(|b| b.opened) {
                egui::Panel::left("thumbs").resizable(true).default_size(150.0).min_size(90.0).show(ui, |ui| self.thumbnails_panel(ui));
            }
            if self.show_list_panel && (self.list.is_some() || self.list_loading.is_some()) {
                egui::Panel::right("list").resizable(true).default_size(260.0).show(ui, |ui| self.list_panel(ui));
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.viewer(ui));

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
