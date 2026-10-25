//! Vue Bibliothèque : livres de la base ComicRack en couvertures ou en tableau, avec
//! recherche, tri et regroupement au choix. Seules les rangées visibles sont dessinées
//! (une bibliothèque peut compter des dizaines de milliers de livres).
//!
//! Un clic sélectionne un livre (panneau de détails), un double-clic ou Entrée l'ouvre.
//!
//! Souris (v0.9.1) : rectangle de sélection tiré depuis un espace vide (Ctrl : ajouté à la
//! sélection), clic dans le vide pour désélectionner ; glisser une couverture déplace la
//! sélection : vers une liste de lecture du panneau (ajout) ou, dans une liste de lecture
//! triée dans l'ordre de la liste, entre deux livres (réordonnancement). Clavier : Ctrl+A,
//! Échap.

use std::path::PathBuf;

use comicrust_core::library::LibraryBook;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui};

use crate::app::{ComicRustApp, Start};
use crate::library::covers::{Cover, CoverCache};
use crate::library::mouse::{grid_insertion, marquee_hits, reorder_allowed, reorder_anchor, CoverRow, GridGeom};
use crate::library::{Gesture, GroupKey, Row, SortKey, Status};
use crate::ui::book_drag::{dragged_books, edge_scroll, paint_insertion, start_books_drag, DropHint};
use crate::ui::lists_ui::{book_lists_menu, BookListAction};

const HEADER_H: f32 = 34.0;
const CAPTION_H: f32 = 40.0;
const GAP: f32 = 14.0;
/// Rapport largeur/hauteur des emplacements de couverture (format comics).
pub(crate) const COVER_RATIO: f32 = 0.66;
pub(crate) const ACCENT: Color32 = Color32::from_rgb(214, 110, 40);

/// Dessine une couverture dans `rect` (alignée en bas, proportions gardées).
pub(crate) fn paint_cover(ui: &Ui, covers: &mut CoverCache, book: &LibraryBook, rect: Rect) -> Rect {
    let painter = ui.painter();
    match covers.get(&book.id, book.file.as_deref(), book.front_cover_image()) {
        Cover::Ready(tex) => {
            let s = tex.size_vec2();
            let k = (rect.width() / s.x).min(rect.height() / s.y);
            let r = Rect::from_center_size(pos2(rect.center().x, rect.max.y - s.y * k / 2.0), s * k);
            painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            painter.rect_stroke(r, 0.0, (1.0, Color32::from_black_alpha(90)), StrokeKind::Inside);
            r
        }
        Cover::Pending => {
            painter.rect_filled(rect.shrink(2.0), 3.0, crate::ui::theme::placeholder(ui));
            rect
        }
        Cover::Failed => {
            painter.rect_filled(rect.shrink(2.0), 3.0, crate::ui::theme::placeholder(ui));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "⚠",
                FontId::proportional(26.0),
                Color32::from_rgb(200, 120, 90),
            );
            rect
        }
    }
}

/// Infobulle d'un livre.
pub(crate) fn book_tooltip(ui: &mut Ui, book: &LibraryBook) {
    ui.label(RichText::new(book.caption()).strong());
    if let Some(t) = book.title.as_deref().filter(|t| !t.is_empty()) {
        ui.label(RichText::new(t).italics());
    }
    if let Some(w) = &book.writer {
        ui.label(format!("Scénario : {w}"));
    }
    if let Some(p) = &book.penciller {
        ui.label(format!("Dessin : {p}"));
    }
    if let Some(y) = book.year.filter(|y| *y > 0) {
        ui.label(format!("Année : {y}"));
    }
    if let Some(arc) = &book.story_arc {
        ui.label(format!("Arc : {arc}"));
    }
    if let Some(f) = &book.file {
        ui.label(RichText::new(f).weak().small());
    }
}

impl ComicRustApp {
    pub(crate) fn library_shown(&self) -> usize {
        self.library.shown()
    }

    /// Nombre de piles affichées (vue empilée, aucune pile ouverte).
    pub(crate) fn library_stacks_shown(&self) -> Option<usize> {
        (self.settings.library_stack && !self.settings.library_table && self.library.open_stack.is_none())
            .then(|| self.library.stacks_shown())
    }

    pub(crate) fn ensure_covers(&mut self, ctx: &egui::Context) {
        if self.covers.is_none() {
            let dir = self.data_dir.as_ref().map(|d| d.join(crate::paths::COVERS_DIR));
            let mut covers = CoverCache::new(dir, ctx);
            covers.redirects = self.settings.converted.clone();
            self.covers = Some(covers);
        }
        if let Some(c) = &mut self.covers {
            c.poll(ctx);
        }
    }

    /// Livres affichés, un par ligne : « Série #n (volume, année) — fichier ».
    pub(crate) fn shown_books_report(&mut self) -> String {
        let Some(db) = self.library.database() else { return String::new() };
        let opts = self.settings.view_options();
        let name = self.library.selected_list().map(|l| l.name.clone()).unwrap_or_else(|| "Bibliothèque".into());
        let view = self.library.view(&opts).to_vec();
        let mut out = format!("{name} — {} livres (ComicRust v{})\n\n", view.len(), env!("CARGO_PKG_VERSION"));
        for i in view {
            let b = &db.books[i];
            out.push_str(&format!(
                "{} #{} (v{}, {}) — {}\n",
                b.shadow_series(),
                b.shadow_number(),
                b.shadow_volume(),
                b.shadow_year(),
                b.file.as_deref().unwrap_or("")
            ));
        }
        out
    }

    /// Ouvre un livre de la base : `Start::Auto` (double-clic, Entrée) applique la règle de
    /// reprise ; `Start::Page` ouvre à la page demandée (boutons de la fiche).
    pub(crate) fn open_library_book(&mut self, ctx: &egui::Context, id: &str, start: Start) {
        let Some(db) = self.library.database() else { return };
        let Some(book) = db.get(id) else { return };
        let Some(file) = book.file.clone() else {
            self.error = Some(format!("« {} » n'a pas de fichier associé dans la base.", book.caption()));
            return;
        };
        // Livre converti en CBZ par ComicRust et original rangé : le CBZ est ouvert.
        let path = self.settings.effective_path(&PathBuf::from(&file));
        let start = match start {
            Start::Auto => Start::Resume(book.current_page.map(|p| p as usize)),
            other => other,
        };
        // Absent mais « .tmp » présent : ComicRack réécrit le fichier, le lecteur patiente.
        if !path.exists() && !comicrust_core::providers::is_being_rewritten(&path) {
            self.error = Some(format!(
                "Fichier introuvable :\n{file}\n\nIl a peut-être été déplacé ou supprimé, ou le NAS n'est pas accessible."
            ));
            return;
        }
        self.open_book_at(ctx, path, start);
    }

    fn library_toolbar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.library.search)
                    .hint_text("Rechercher : série, auteur, titre, personnage, année…")
                    .desired_width(300.0),
            );
            if search.changed() {
                if let Some(c) = &mut self.covers {
                    c.cancel_pending();
                }
            }
            if !self.library.search.is_empty() && ui.button("✖").on_hover_text("Effacer la recherche").clicked() {
                self.library.search.clear();
            }
            ui.separator();
            ui.selectable_value(&mut self.settings.library_table, false, "▦ Couvertures");
            ui.selectable_value(&mut self.settings.library_table, true, "☰ Tableau");
            ui.separator();
            ui.label("Trier :");
            let in_list = self.library.reading_list_shown();
            egui::ComboBox::from_id_salt("tri").selected_text(self.settings.library_sort.label()).show_ui(ui, |ui| {
                for k in SortKey::ALL {
                    if k == SortKey::Position && !in_list {
                        continue;
                    }
                    ui.selectable_value(&mut self.settings.library_sort, k, k.label());
                }
            });
            let dir = if self.settings.library_descending { "⬇" } else { "⬆" };
            if ui
                .button(dir)
                .on_hover_text(if self.settings.library_descending { "Ordre décroissant" } else { "Ordre croissant" })
                .clicked()
            {
                self.settings.library_descending = !self.settings.library_descending;
            }
            if !self.settings.library_table
                && ui
                    .toggle_value(&mut self.settings.library_stack, "🗂 Empiler par série")
                    .on_hover_text("Une pile par série ; double-clic sur une pile pour voir ses livres")
                    .changed()
            {
                self.library.open_stack = None;
            }
            ui.label("Grouper :");
            egui::ComboBox::from_id_salt("groupe").selected_text(self.settings.library_group.label_of_key()).show_ui(
                ui,
                |ui| {
                    for g in GroupKey::ALL {
                        ui.selectable_value(&mut self.settings.library_group, g, g.label_of_key());
                    }
                },
            );
            if !self.settings.library_table {
                ui.add(
                    egui::Slider::new(&mut self.settings.library_cover_height, 120.0..=320.0)
                        .show_value(false)
                        .text("Taille"),
                );
            }
            ui.separator();
            ui.toggle_value(&mut self.settings.show_comic_lists, "📂 Listes")
                .on_hover_text("Listes de ComicRack (ComicDb.xml)");
            ui.toggle_value(&mut self.settings.show_details, "ℹ Détails").on_hover_text("Fiche du livre sélectionné");
            self.files_menu(ui);
            if self.library.standalone().is_some() {
                if let Some(job) = &self.library.scan {
                    if ui.button("⏹ Arrêter l'analyse").clicked() {
                        job.stop();
                    }
                } else if ui
                    .button("🔍 Analyser les dossiers")
                    .on_hover_text("Ajouter les nouveaux fichiers, relire les fichiers modifiés, repérer les manquants")
                    .clicked()
                {
                    self.start_scan(&ctx, true);
                }
                if self.last_scan.is_some()
                    && ui.small_button("📋").on_hover_text("Bilan de la dernière analyse").clicked()
                {
                    self.show_last_scan = true;
                }
            }
            if ui.button("⟳").on_hover_text("Relire toute la bibliothèque").clicked() {
                self.load_library(&ctx);
            }
        });
        // « Ordre de la liste » n'a de sens que dans une liste.
        if !self.library.reading_list_shown() && self.settings.library_sort == SortKey::Position {
            self.settings.library_sort = SortKey::Series;
        }
    }

    pub(crate) fn library_view(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // Avancement du livre ouvert à jour dès le retour à la bibliothèque.
        self.remember_position();
        self.ensure_covers(&ctx);

        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| self.library_toolbar(ui));
        ui.separator();

        match &self.library.status {
            Status::NotConfigured => {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.25);
                    ui.heading("Bibliothèque");
                    ui.label("Choisis comment utiliser ComicRust : seul, avec tes dossiers de BD, ou avec ComicRack.");
                    ui.add_space(12.0);
                    if ui.button(RichText::new("⚙  Assistant de configuration…").size(17.0)).clicked() {
                        self.open_setup();
                    }
                });
            }
            Status::Loading { progress, what, .. } => {
                let (done, total) = progress.get();
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.3);
                    ui.label(format!("Chargement de la bibliothèque ({what})…"));
                    let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                    let text = if total > 0 { format!("{done} / {total} livres") } else { "connexion…".to_string() };
                    ui.add(egui::ProgressBar::new(frac).text(text).desired_width(380.0));
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
            Status::Failed(err) => {
                let err = err.clone();
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.25);
                    ui.heading("⚠ Bibliothèque inaccessible");
                    ui.label(RichText::new(err).color(crate::ui::theme::error(ui)));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Réessayer").clicked() {
                            self.load_library(&ctx);
                        }
                        if ui.button("Assistant de configuration…").clicked() {
                            self.open_setup();
                        }
                    });
                });
            }
            Status::Loaded(_) => {
                if let Some(list) = self.library.selected_list() {
                    let unknown = match self
                        .library
                        .list_members(&list.id)
                        .unwrap_or(comicrust_core::comic_lists::Members::All)
                    {
                        comicrust_core::comic_lists::Members::Unknown(why) => Some(why),
                        _ => None,
                    };
                    let approximate = matches!(&list.kind,
                        comicrust_core::comic_lists::ListKind::Smart { query: Some(q), .. } if q.is_approximate());
                    let name = list.name.clone();
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("📂 {name}")).strong());
                        if ui.small_button("✖ Toute la bibliothèque").clicked() {
                            self.select_list(None);
                        }
                        if ui
                            .small_button("📋 Copier les livres")
                            .on_hover_text(
                                "Liste des livres affichés (série, numéro, fichier), pour comparer avec ComicRack",
                            )
                            .clicked()
                        {
                            let text = self.shown_books_report();
                            ui.ctx().copy_text(text);
                        }
                        if approximate {
                            ui.label(
                                RichText::new("⚠ sélection « au hasard » : peut différer de celle de ComicRack")
                                    .color(crate::ui::theme::warning(ui)),
                            );
                        }
                    });
                    if let Some(why) = unknown {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label("ComicRust ne peut pas encore afficher cette liste intelligente :");
                            ui.label(RichText::new(why).strong());
                        });
                        return;
                    }
                }
                // Pile ouverte : rappel de la série et retour aux piles.
                if self.settings.library_stack && !self.settings.library_table {
                    if let Some(series) = self.library.open_stack.clone() {
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            if ui.button("◀ Toutes les piles").on_hover_text("Retour (Échap)").clicked() {
                                self.library.open_stack = None;
                            }
                            ui.label(RichText::new(format!("🗂 {series}")).strong());
                        });
                    }
                }
                self.library_keys(ui);
                if self.settings.library_table {
                    self.library_table(ui);
                } else {
                    self.library_grid(ui);
                }
            }
        }
    }

    /// Entrée ouvre le livre sélectionné ; Échap le désélectionne.
    fn library_keys(&mut self, ui: &mut Ui) {
        if ui.ctx().memory(|m| m.focused().is_some()) {
            return; // saisie dans la recherche
        }
        let (enter, esc, delete) = ui.input(|i| {
            (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Delete))
        });
        // Ctrl+A : tous les livres affichés (pas sur les piles fermées).
        let piles = self.settings.library_stack && !self.settings.library_table && self.library.open_stack.is_none();
        if !piles && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
            self.library.select_all();
        }
        // Suppr dans une liste de lecture : retirer les livres de la liste (jamais les fichiers).
        if delete && self.library.reading_list_shown() && self.library.selected_book.is_some() {
            let books = self.library.selection();
            if self.book_menu_data(ui.ctx()).reason.is_none() {
                self.book_list_request(ui.ctx(), BookListAction::Remove, books);
            }
        }
        let stacked = self.settings.library_stack && !self.settings.library_table && self.library.open_stack.is_none();
        if enter && stacked {
            if let Some(s) = self.library.selected_stack.clone() {
                self.library.open_stack = Some(s);
            }
        } else if enter {
            if let Some(id) = self.library.selected_book.clone() {
                self.open_library_book(ui.ctx(), &id, Start::Auto);
            }
        }
        if esc {
            self.library.gesture = Gesture::None;
            if self.library.selected_book.is_some() {
                self.library.clear_selection();
            } else {
                self.library.open_stack = None;
            }
        }
    }

    fn empty_message(&self, ui: &mut Ui, db_empty: bool) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(if db_empty {
                "La base ne contient aucun livre."
            } else if self.library.selected_list.is_some() && self.library.search.is_empty() {
                "Aucun livre de cette liste n'est dans la base."
            } else {
                "Aucun livre ne correspond."
            });
        });
    }

    fn library_grid(&mut self, ui: &mut Ui) {
        let Some(db) = self.library.database() else { return };
        let opts = self.settings.view_options();
        let cover_h = self.settings.library_cover_height;
        let cell = vec2(cover_h * COVER_RATIO, cover_h + CAPTION_H);
        let width = ui.available_width() - 2.0 * GAP;
        let columns = (((width + GAP) / (cell.x + GAP)).floor() as usize).max(1);
        // Vue empilée : une pile par série, sauf quand une pile est ouverte.
        let stacked = self.settings.library_stack && self.library.open_stack.is_none();
        let rows = if stacked { self.library.stacked_rows(&opts, columns) } else { self.library.rows(&opts, columns) };
        if self.library.shown() == 0 {
            self.empty_message(ui, db.is_empty());
            return;
        }
        let in_list = self.library.reading_list_shown();
        let compact = opts.group == GroupKey::Series;

        let heights: Vec<f32> = rows
            .iter()
            .map(|r| match r {
                Row::Header { .. } => HEADER_H,
                Row::Covers(_) | Row::Stacks(_) => cell.y + GAP,
            })
            .collect();
        let total: f32 = heights.iter().sum::<f32>() + GAP;
        let mut open: Option<String> = None;
        let mut select: Option<String> = None;
        let mut open_stack: Option<String> = None;
        let mut select_stack: Option<String> = None;
        let mut file_request: Option<(crate::library::files::FileAction, String)> = None;
        // Listes de lecture : menu des livres, sélection multiple (Ctrl, Maj).
        let lists_menu = self.book_menu_data(ui.ctx());
        let selection = self.library.selection();
        let mods = ui.input(|i| i.modifiers);
        let mut select_plain: Option<String> = None;
        let mut list_request: Option<(BookListAction, Vec<String>)> = None;

        // Souris : géométrie des rangées de couvertures (coordonnées du contenu), glisser en
        // cours, réordonnancement possible ou non.
        let geom = GridGeom { cell, gap: GAP };
        let mut cover_rows: Vec<CoverRow> = Vec::new();
        {
            let (mut y, mut first) = (0.0, 0);
            for (row, h) in rows.iter().zip(&heights) {
                if let Row::Covers(items) = row {
                    cover_rows.push(CoverRow { top: y, first, len: items.len() });
                    first += items.len();
                }
                y += h;
            }
        }
        let ctx = ui.ctx().clone();
        let drag = dragged_books(&ctx);
        let sorted = opts.sort == SortKey::Position && !opts.descending;
        let reorder = reorder_allowed(in_list, sorted, lists_menu.reason.as_deref());
        let (pointer, released, primary_down, press_origin) = ctx.input(|i| {
            (i.pointer.latest_pos(), i.pointer.any_released(), i.pointer.primary_down(), i.pointer.press_origin())
        });
        let mut clear = false;
        let mut new_gesture: Option<Gesture> = None;
        let mut marquee_sel: Option<(Vec<usize>, std::collections::HashSet<String>)> = None;
        let mut drag_start: Option<(String, bool)> = None;
        let mut reorder_drop: Option<usize> = None;
        let mut hint: Option<DropHint> = None;

        egui::ScrollArea::vertical().auto_shrink(false).id_salt("bibliotheque").show_viewport(ui, |ui, viewport| {
            ui.set_height(total);
            let origin = ui.min_rect().min + vec2(GAP, GAP * 0.5);
            let clip = ui.clip_rect();
            // Fond (sous les vignettes) : clic dans le vide, rectangle de sélection.
            let bg = ui.interact(ui.max_rect(), ui.scope_id().with("fond-couvertures"), Sense::click_and_drag());
            if bg.clicked() && !mods.command {
                clear = true;
            }
            if !stacked && bg.drag_started_by(egui::PointerButton::Primary) {
                if let Some(p) = press_origin {
                    let base = if mods.command { self.library.selection_set() } else { Default::default() };
                    new_gesture = Some(Gesture::Marquee { start: (p - origin).to_pos2(), base });
                }
            }
            let mut marquee: Option<Rect> = None;
            if let (Gesture::Marquee { start, base }, Some(p)) = (&self.library.gesture, pointer) {
                if primary_down {
                    let r = Rect::from_two_pos(*start, (p - origin).to_pos2());
                    marquee_sel = Some((marquee_hits(&cover_rows, &geom, r), base.clone()));
                    marquee = Some(r.translate(origin.to_vec2()));
                    edge_scroll(ui, clip, p);
                } else {
                    new_gesture = Some(Gesture::None);
                }
            }
            let mut y = 0.0;
            for (row, h) in rows.iter().zip(&heights) {
                let top = y;
                y += h;
                if y < viewport.min.y || top > viewport.max.y {
                    continue;
                }
                match row {
                    Row::Header { title, count } => {
                        let r = Rect::from_min_size(origin + vec2(0.0, top), vec2(width, HEADER_H));
                        let p = ui.painter();
                        p.text(
                            pos2(r.min.x, r.center().y),
                            Align2::LEFT_CENTER,
                            title,
                            FontId::proportional(18.0),
                            ui.visuals().strong_text_color(),
                        );
                        p.text(
                            pos2(r.max.x, r.center().y),
                            Align2::RIGHT_CENTER,
                            format!("{count} livre{}", if *count > 1 { "s" } else { "" }),
                            FontId::proportional(13.0),
                            ui.visuals().weak_text_color(),
                        );
                        p.line_segment(
                            [pos2(r.min.x, r.max.y - 3.0), pos2(r.max.x, r.max.y - 3.0)],
                            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                        );
                    }
                    Row::Covers(items) => {
                        for (k, &bi) in items.iter().enumerate() {
                            let book = &db.books[bi];
                            let min = origin + vec2(k as f32 * (cell.x + GAP), top);
                            let rect = Rect::from_min_size(min, cell);
                            let resp =
                                ui.interact(rect, ui.scope_id().with(("livre", &book.id)), Sense::click_and_drag());
                            let cover_rect = Rect::from_min_size(min, vec2(cell.x, cover_h));
                            let selected = self.library.is_selected(&book.id);
                            if selected {
                                ui.painter().rect_filled(rect.expand(5.0), 6.0, ui.visuals().selection.bg_fill);
                            } else if resp.hovered() {
                                ui.painter().rect_filled(rect.expand(4.0), 6.0, ui.visuals().widgets.hovered.bg_fill);
                            }
                            let covers = self.covers.as_mut().unwrap();
                            paint_cover(ui, covers, book, cover_rect);
                            let painter = ui.painter();
                            // Avancement de lecture enregistré par ComicRack.
                            if let Some(pct) = self.read_percent(book) {
                                let bar = Rect::from_min_size(
                                    pos2(cover_rect.min.x, cover_rect.max.y - 4.0),
                                    vec2(cover_rect.width() * pct as f32 / 100.0, 4.0),
                                );
                                painter.rect_filled(bar, 0.0, ACCENT);
                            }
                            let mut caption = if compact {
                                let mut c = book.number.as_deref().map(|n| format!("#{n}")).unwrap_or_default();
                                if let Some(t) = book.title.as_deref().filter(|t| !t.is_empty()) {
                                    c = if c.is_empty() { t.to_string() } else { format!("{c} · {t}") };
                                }
                                if c.is_empty() {
                                    book.caption()
                                } else {
                                    c
                                }
                            } else {
                                book.caption()
                            };
                            if in_list {
                                if let Some(pos) = self.library.position(bi) {
                                    caption = format!("{}. {caption}", pos + 1);
                                }
                            }
                            let galley =
                                painter.layout(caption, FontId::proportional(12.5), ui.visuals().text_color(), cell.x);
                            let text_rect =
                                Rect::from_min_size(pos2(min.x, cover_rect.max.y + 4.0), vec2(cell.x, CAPTION_H - 6.0));
                            painter.with_clip_rect(text_rect).galley(text_rect.min, galley, Color32::WHITE);

                            let resp = resp.on_hover_ui(|ui| book_tooltip(ui, book));
                            if resp.secondary_clicked() && !selected {
                                select_plain = Some(book.id.clone());
                            }
                            resp.context_menu(|ui| {
                                Self::book_file_menu(
                                    ui,
                                    &self.settings,
                                    &book.id,
                                    book.file.as_deref(),
                                    &mut file_request,
                                );
                                let books = if selected { selection.clone() } else { vec![book.id.clone()] };
                                book_lists_menu(ui, &lists_menu, &books, &mut list_request);
                            });
                            if resp.double_clicked() {
                                open = Some(book.id.clone());
                            } else if resp.clicked() {
                                select = Some(book.id.clone());
                            }
                            if resp.drag_started_by(egui::PointerButton::Primary) {
                                drag_start = Some((book.id.clone(), selected));
                            }
                        }
                    }
                    Row::Stacks(stacks) => {
                        for (k, stack) in stacks.iter().enumerate() {
                            let first = &db.books[stack.books[0]];
                            let min = origin + vec2(k as f32 * (cell.x + GAP), top);
                            let rect = Rect::from_min_size(min, cell);
                            let resp = ui.interact(rect, ui.scope_id().with(("pile", &stack.series)), Sense::click());
                            let selected = self.library.selected_stack.as_deref() == Some(stack.series.as_str());
                            if selected {
                                ui.painter().rect_filled(rect.expand(5.0), 6.0, ui.visuals().selection.bg_fill);
                            } else if resp.hovered() {
                                ui.painter().rect_filled(rect.expand(4.0), 6.0, ui.visuals().widgets.hovered.bg_fill);
                            }
                            // Feuilles décalées derrière la couverture : effet de pile.
                            let off = (cell.x * 0.05).clamp(3.0, 8.0);
                            let cover_rect = Rect::from_min_size(
                                min + vec2(0.0, off * 2.0),
                                vec2(cell.x - off * 2.0, cover_h - off * 2.0),
                            );
                            if stack.books.len() > 1 {
                                let stroke = (1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                                for d in [2.0, 1.0] {
                                    let r = cover_rect.translate(vec2(off * d, -off * d));
                                    ui.painter().rect_filled(r, 2.0, crate::ui::theme::placeholder(ui));
                                    ui.painter().rect_stroke(r, 2.0, stroke, StrokeKind::Inside);
                                }
                            }
                            let drawn = paint_cover(ui, self.covers.as_mut().unwrap(), first, cover_rect);
                            let painter = ui.painter();
                            // Nombre de livres de la pile.
                            let badge = egui::pos2(drawn.max.x - 4.0, drawn.min.y + 4.0);
                            let label = stack.books.len().to_string();
                            let r = 9.0 + 3.0 * label.len().saturating_sub(1) as f32;
                            painter.circle_filled(badge, r, ACCENT);
                            painter.text(
                                badge,
                                Align2::CENTER_CENTER,
                                label,
                                FontId::proportional(12.0),
                                Color32::WHITE,
                            );
                            // Avancement : part des livres de la série déjà lus.
                            let read = stack
                                .books
                                .iter()
                                .filter(|&&i| self.read_percent(&db.books[i]).is_some_and(|p| p >= 95))
                                .count();
                            if read > 0 {
                                let bar = Rect::from_min_size(
                                    pos2(drawn.min.x, drawn.max.y - 4.0),
                                    vec2(drawn.width() * read as f32 / stack.books.len() as f32, 4.0),
                                );
                                painter.rect_filled(bar, 0.0, ACCENT);
                            }
                            let caption = format!(
                                "{}\n{} livre{}",
                                stack.series,
                                stack.books.len(),
                                if stack.books.len() > 1 { "s" } else { "" }
                            );
                            let galley =
                                painter.layout(caption, FontId::proportional(12.5), ui.visuals().text_color(), cell.x);
                            let text_rect =
                                Rect::from_min_size(pos2(min.x, min.y + cover_h + 4.0), vec2(cell.x, CAPTION_H - 6.0));
                            painter.with_clip_rect(text_rect).galley(text_rect.min, galley, Color32::WHITE);
                            let resp = resp.on_hover_ui(|ui| {
                                ui.label(RichText::new(&stack.series).strong());
                                ui.label(format!("{} livres, {} lus", stack.books.len(), read));
                                ui.label(RichText::new("Double-clic (ou Entrée) : ouvrir la pile").weak());
                            });
                            if resp.double_clicked() {
                                open_stack = Some(stack.series.clone());
                            } else if resp.clicked() {
                                select_stack = Some(stack.series.clone());
                            }
                        }
                    }
                }
            }
            // Livres glissés au-dessus de la vue : trait d'insertion (réordonnancement) ou refus.
            if let (Some(_), Some(p)) = (&drag, pointer) {
                if clip.contains(p) {
                    match &reorder {
                        Ok(()) => {
                            if let Some((ins, line)) = grid_insertion(&cover_rows, &geom, (p - origin).to_pos2()) {
                                paint_insertion(ui, clip, line[0] + origin.to_vec2(), line[1] + origin.to_vec2());
                                hint = Some(DropHint::Move);
                                edge_scroll(ui, clip, p);
                                if released {
                                    reorder_drop = Some(ins);
                                }
                            }
                        }
                        Err(why) => hint = Some(DropHint::Refused(why.clone())),
                    }
                }
            }
            if let Some(r) = marquee {
                let v = ui.visuals();
                let painter = ui.painter().with_clip_rect(clip);
                painter.rect_filled(r, 2.0, v.selection.bg_fill.gamma_multiply(0.3));
                painter.rect_stroke(r, 2.0, Stroke::new(1.0, v.selection.stroke.color), StrokeKind::Inside);
            }
        });

        if let Some(g) = new_gesture {
            self.library.gesture = g;
        }
        if let Some((hits, base)) = marquee_sel {
            self.library.select_view_indices(hits, &base);
        }
        if clear {
            if stacked {
                self.library.selected_stack = None;
            } else {
                self.library.clear_selection();
            }
        }
        if let Some((id, selected)) = drag_start {
            // Livre non sélectionné : il devient la sélection (ajouté avec Ctrl), comme dans
            // l'explorateur ; puis toute la sélection est glissée.
            if !selected {
                self.library.click_book(&id, mods.command, false);
            }
            let books = if selected || mods.command { self.library.selection() } else { vec![id] };
            start_books_drag(&ctx, books);
        }
        if let (Some(ins), Some(d)) = (reorder_drop, &drag) {
            egui::DragAndDrop::clear_payload(&ctx);
            if let Some(anchor) = reorder_anchor(&self.library.view_ids(), &d.books, ins) {
                list_request = Some((BookListAction::Reorder(anchor), d.books.clone()));
            }
        }
        if hint.is_some() {
            self.lists_ui.drag_hint = hint;
        }

        if let Some(s) = select_stack {
            self.library.selected_stack = Some(s);
        }
        if let Some(s) = open_stack {
            self.library.selected_stack = Some(s.clone());
            self.library.open_stack = Some(s);
            if let Some(c) = &mut self.covers {
                c.cancel_pending();
            }
        }
        if let Some(id) = select {
            self.library.click_book(&id, mods.command, mods.shift);
        }
        if let Some(id) = select_plain {
            self.library.click_book(&id, false, false);
        }
        if let Some(id) = open {
            self.library.selected_book = Some(id.clone());
            self.open_library_book(ui.ctx(), &id, Start::Auto);
        }
        if let Some((action, id)) = file_request {
            self.library.selected_book = Some(id.clone());
            self.ask_file_action(action, &[id]);
        }
        if let Some((action, books)) = list_request {
            self.book_list_request(ui.ctx(), action, books);
        }
    }
}
