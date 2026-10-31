//! Vue Bibliothèque en tableau : une ligne par livre, colonnes triables d'un clic sur
//! leur titre.
//!
//! Souris (v0.9.1) : glisser depuis une ligne **non sélectionnée** sélectionne la plage
//! balayée (Ctrl : ajoutée à la sélection) ; glisser depuis une ligne **sélectionnée**
//! déplace les livres sélectionnés (vers une liste de lecture du panneau, ou entre deux
//! lignes d'une liste de lecture triée dans l'ordre de la liste). Clic sous les lignes :
//! plus de sélection.
//!
//! Clavier (v0.10.1) : ↑/↓, Début/Fin, Page préc./suiv. (Maj : étendre la sélection ;
//! Ctrl : curseur seul, Espace : (dé)sélectionner), Entrée ouvre le livre.
//!
//! Groupes (v0.10.2) : avec « Grouper », une ligne de titre précède chaque groupe, comme
//! dans la vue des couvertures. Un clic dessus sélectionne tous ses livres (Ctrl : ajoutés) ;
//! dans une liste de lecture triée dans l'ordre de la liste, on la glisse pour déplacer la
//! série en bloc entre deux groupes, ou on la monte / descend par son menu.

use comicrust_core::library::LibraryBook;
use eframe::egui::{self, pos2, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::app::ComicRustApp;
use crate::library::keynav::{list_move, KeyFocus, NavKey, SelectMode};
use crate::library::mouse::{
    book_rank_of_row, group_drop_anchor, group_shift_anchor, reorder_allowed, reorder_anchor, sweep_range,
    table_insertion, table_row_at,
};
use crate::library::{Gesture, GroupKey, SortKey};
use crate::ui::book_drag::{dragged_books, edge_scroll, paint_insertion, start_books_drag, start_group_drag, DropHint};
use crate::ui::lists_ui::{book_lists_menu, BookListAction};

/// `2019-10-01T12:58:00.123+02:00` → `01/10/2019`.
pub(crate) fn short_date(iso: Option<&str>) -> String {
    let Some(s) = iso.filter(|s| s.len() >= 10 && !s.starts_with("0001-01-01")) else { return String::new() };
    let (y, m, d) = (&s[0..4], &s[5..7], &s[8..10]);
    format!("{d}/{m}/{y}")
}

/// `2019-10-01T12:58:00…` → `01/10/2019 12:58`.
pub(crate) fn long_date(iso: Option<&str>) -> String {
    let d = short_date(iso);
    match iso {
        Some(s) if !d.is_empty() && s.len() >= 16 => format!("{d} {}", &s[11..16]),
        _ => d,
    }
}

pub(crate) fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    let s = if b >= 1e9 {
        format!("{:.2} Go", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} Mo", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} Ko", b / 1e3)
    } else {
        format!("{bytes} o")
    };
    s.replace('.', ",")
}

fn stars(r: Option<f32>) -> String {
    match r.filter(|r| *r > 0.0) {
        Some(r) => {
            let full = r.round().clamp(0.0, 5.0) as usize;
            format!("{}{}", "★".repeat(full), "☆".repeat(5 - full))
        }
        None => String::new(),
    }
}

struct Col {
    title: &'static str,
    sort: Option<SortKey>,
    width: f32,
    cell: fn(&LibraryBook) -> String,
}

const COLS: [Col; 11] = [
    Col { title: "Série", sort: Some(SortKey::Series), width: 220.0, cell: |b| b.series.clone().unwrap_or_default() },
    Col {
        title: "N°",
        sort: None,
        width: 50.0,
        cell: |b| {
            let n = b.number.clone().unwrap_or_default();
            match b.volume.filter(|v| *v >= 0) {
                Some(v) if !n.is_empty() => format!("v{v} #{n}"),
                Some(v) => format!("v{v}"),
                None if n.is_empty() => n,
                None => format!("#{n}"),
            }
        },
    },
    Col { title: "Titre", sort: Some(SortKey::Title), width: 220.0, cell: |b| b.title.clone().unwrap_or_default() },
    Col {
        title: "Parution",
        sort: Some(SortKey::Published),
        width: 70.0,
        cell: |b| match (b.year.filter(|y| *y > 0), b.month.filter(|m| *m > 0)) {
            (Some(y), Some(m)) => format!("{m:02}/{y}"),
            (Some(y), None) => y.to_string(),
            _ => String::new(),
        },
    },
    Col { title: "Éditeur", sort: None, width: 110.0, cell: |b| b.publisher.clone().unwrap_or_default() },
    Col { title: "Scénariste", sort: None, width: 140.0, cell: |b| b.writer.clone().unwrap_or_default() },
    Col {
        title: "Pages",
        sort: Some(SortKey::PageCount),
        width: 50.0,
        cell: |b| b.page_count.map(|p| p.to_string()).unwrap_or_default(),
    },
    Col {
        title: "Lu",
        sort: Some(SortKey::ReadPercent),
        width: 45.0,
        cell: |b| b.read_percent().filter(|p| *p > 0).map(|p| format!("{p} %")).unwrap_or_default(),
    },
    Col { title: "Note", sort: Some(SortKey::Rating), width: 70.0, cell: |b| stars(b.rating) },
    Col { title: "Ajouté", sort: Some(SortKey::Added), width: 80.0, cell: |b| short_date(b.added.as_deref()) },
    Col { title: "Fichier", sort: Some(SortKey::File), width: 300.0, cell: |b| b.file.clone().unwrap_or_default() },
];

impl ComicRustApp {
    pub(crate) fn library_table(&mut self, ui: &mut Ui) {
        let Some(db) = self.library.database() else { return };
        let opts = self.settings.view_options();
        let view = self.library.view(&opts).to_vec();
        if view.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                let none = self.library.missing_only && self.library.search.is_empty();
                ui.label(if none { "Aucun livre manquant." } else { "Aucun livre ne correspond." });
            });
            return;
        }
        let in_list = self.library.reading_list_shown();
        // Lignes affichées : titres de groupe (rangs dans `rows`) et livres (rangs dans la vue).
        let mut headers: Vec<usize> = Vec::new();
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        let mut display: Vec<Option<usize>> = Vec::with_capacity(view.len());
        if opts.group != GroupKey::None {
            let mut last: Option<String> = None;
            for (rank, &bi) in view.iter().enumerate() {
                let title = opts.group.label(&db.books[bi]);
                if last.as_ref() != Some(&title) {
                    headers.push(display.len());
                    display.push(None);
                    groups.push((title.clone(), Vec::new()));
                    last = Some(title);
                }
                groups.last_mut().unwrap().1.push(db.books[bi].id.clone());
                display.push(Some(rank));
            }
        } else {
            display.extend((0..view.len()).map(Some));
        }
        let series_word = if opts.group == GroupKey::Series { "la série" } else { "le groupe" };
        let mut group_click: Option<usize> = None;
        let mut group_drag_start: Option<usize> = None;
        let mut group_shift: Option<(usize, bool)> = None;
        let mut open = None;
        let mut select = None;
        let mut file_request: Option<(crate::library::files::FileAction, String)> = None;
        let mut sort_by = None;
        // Listes de lecture : menu des livres, sélection multiple (Ctrl, Maj).
        let lists_menu = self.book_menu_data(ui.ctx());
        let selection = self.library.selection();
        let mods = ui.input(|i| i.modifiers);
        let mut select_plain: Option<String> = None;
        let mut list_request: Option<(BookListAction, Vec<String>)> = None;
        let scripts_menu = self.book_scripts();
        let missing_menu = self.missing_menu_data();
        let selection_missing = selection.iter().any(|id| db.get(id).is_some_and(crate::library::missing::is_missing));
        let mut missing_request: Option<crate::ui::missing_ui::MissingRequest> = None;
        let mut script_request: Option<(String, Vec<String>)> = None;
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        // Clavier : déplacement dans les lignes.
        if self.nav_keys_for(ui.ctx(), KeyFocus::Books) {
            let (keys, space) = crate::ui::library_view::nav_presses(ui.ctx());
            let pitch = row_h + ui.spacing().item_spacing.y;
            let page = ((ui.available_height() / pitch).floor() as usize).saturating_sub(2).max(1);
            for (k, m) in keys {
                if matches!(k, NavKey::Left | NavKey::Right) {
                    continue; // ←/→ : défilement horizontal des colonnes, pas la sélection
                }
                if let Some(t) = list_move(view.len(), self.library.cursor_index(), k, page) {
                    self.library.key_select(t, SelectMode::of(m));
                }
            }
            if space {
                self.library.toggle_cursor();
            }
        }
        let scroll_row = if std::mem::take(&mut self.library.nav.scroll_to_cursor) {
            self.library.cursor_index().and_then(|r| display.iter().position(|x| *x == Some(r)))
        } else {
            None
        };
        let cursor_mark: Option<String> = if self.library.nav.focus == KeyFocus::Books {
            self.library.cursor.clone().filter(|c| !self.library.is_selected(c) || self.library.selection().len() > 1)
        } else {
            None
        };
        let mut info_request: Option<Vec<String>> = None;
        // Souris : glisser en cours, réordonnancement possible ou non, balayage.
        let n = view.len();
        let n_rows = display.len();
        let ctx = ui.ctx().clone();
        let drag = dragged_books(&ctx);
        let sorted = opts.sort == SortKey::Position && !opts.descending;
        let reorder = reorder_allowed(in_list, sorted, lists_menu.reason.as_deref());
        let (pointer, released, primary_down, press_origin) = ctx.input(|i| {
            (i.pointer.latest_pos(), i.pointer.any_released(), i.pointer.primary_down(), i.pointer.press_origin())
        });
        let sweeping = matches!(self.library.gesture, Gesture::Sweep { .. });
        // Haut de la ligne 0 (écran), pas entre deux lignes, zone visible du corps.
        let mut top0: Option<f32> = None;
        let mut pitch = row_h;
        let mut body_clip: Option<egui::Rect> = None;
        let mut drag_start: Option<(usize, String, bool)> = None;
        // Fond (sous les lignes) : clic dans le vide.
        let bg = ui.interact(ui.available_rect_before_wrap(), ui.scope_id().with("fond-tableau"), Sense::click());
        // Colonnes plus larges que la fenêtre : défilement horizontal.
        egui::ScrollArea::horizontal().id_salt("tableau-h").auto_shrink(false).show(ui, |ui| {
            let mut t = TableBuilder::new(ui)
                .id_salt("tableau-bibliotheque")
                .striped(true)
                .resizable(true)
                .sense(Sense::click_and_drag())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            if let Some(r) = scroll_row {
                t = t.scroll_to_row(r, None);
            }
            if in_list {
                t = t.column(Column::initial(40.0).at_least(30.0));
            }
            for c in &COLS[..COLS.len() - 1] {
                t = t.column(Column::initial(c.width).at_least(30.0).clip(true));
            }
            t = t.column(Column::initial(COLS[COLS.len() - 1].width).at_least(80.0).clip(true));
            t.header(row_h + 2.0, |mut h| {
                if in_list {
                    h.col(|ui| {
                        if ui.button(RichText::new("#").strong()).on_hover_text("Ordre de la liste").clicked() {
                            sort_by = Some(SortKey::Position);
                        }
                    });
                }
                for c in &COLS {
                    h.col(|ui| {
                        let mut title = c.title.to_string();
                        if c.sort == Some(opts.sort) {
                            title.push_str(if opts.descending { " ⬇" } else { " ⬆" });
                        }
                        match c.sort {
                            Some(k) => {
                                if ui.add(egui::Button::new(RichText::new(title).strong()).frame(false)).clicked() {
                                    sort_by = Some(k);
                                }
                            }
                            None => {
                                ui.label(RichText::new(title).strong());
                            }
                        }
                    });
                }
            })
            .body(|mut body| {
                // Texte des cellules non sélectionnable : le glisser sert aux lignes (balayage,
                // déplacement des livres), pas à sélectionner du texte.
                body.ui_mut().style_mut().interaction.selectable_labels = false;
                pitch = row_h + body.ui_mut().spacing().item_spacing.y;
                let clip = body.ui_mut().clip_rect();
                body_clip = Some(clip);
                // Défilement automatique : balayage, ou livres glissés au-dessus des lignes.
                if let Some(p) = pointer {
                    if (sweeping && primary_down) || (drag.is_some() && reorder.is_ok() && clip.contains(p)) {
                        edge_scroll(body.ui_mut(), clip, p);
                    }
                }
                body.rows(row_h, n_rows, |mut row| {
                    let d = row.index();
                    let Some(rank) = display[d] else {
                        // Ligne de titre de groupe.
                        let gi = headers.binary_search(&d).unwrap_or(0);
                        let (title, ids) = &groups[gi];
                        row.set_selected(self.library.group_selected(ids));
                        if in_list {
                            row.col(|_| {});
                        }
                        let count = ids.len();
                        for k in 0..COLS.len() {
                            row.col(|ui| {
                                if k == 0 {
                                    // Titre peint sur toute la largeur (cellules suivantes vides).
                                    let r = ui.max_rect();
                                    let text =
                                        format!("▾ {title}   ({count} livre{})", if count > 1 { "s" } else { "" });
                                    ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(clip).text(
                                        egui::pos2(r.left(), r.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        text,
                                        egui::FontId::proportional(14.5),
                                        ui.visuals().strong_text_color(),
                                    );
                                }
                            });
                        }
                        let resp = row.response();
                        if top0.is_none() {
                            top0 = Some(resp.rect.top() - d as f32 * pitch);
                        }
                        let resp = resp.on_hover_text(if in_list {
                            format!("Clic : sélectionner {series_word} (Ctrl : ajouter) ; glisser pour la déplacer")
                        } else {
                            format!("Clic : sélectionner {series_word} (Ctrl : ajouter)")
                        });
                        if resp.clicked() {
                            group_click = Some(gi);
                        }
                        if resp.drag_started_by(egui::PointerButton::Primary) {
                            group_drag_start = Some(gi);
                        }
                        let n_groups = groups.len();
                        resp.context_menu(|ui| {
                            crate::ui::library_view::group_menu(
                                ui,
                                series_word,
                                count,
                                in_list,
                                &reorder,
                                gi,
                                n_groups,
                                &mut group_click,
                                &mut group_shift,
                            );
                        });
                        return;
                    };
                    let bi = view[rank];
                    let book = &db.books[bi];
                    let selected = self.library.is_selected(&book.id);
                    row.set_selected(selected);
                    if in_list {
                        row.col(|ui| {
                            ui.label(self.library.position(bi).map(|p| (p + 1).to_string()).unwrap_or_default());
                        });
                    }
                    // Livre manquant : ligne grisée, « ⚠ » devant la première colonne.
                    let missing = crate::library::missing::is_missing(book);
                    for (k, c) in COLS.iter().enumerate() {
                        row.col(|ui| {
                            // « Lu » : le plus avancé entre ComicRack et ComicRust.
                            let text = if c.sort == Some(SortKey::ReadPercent) {
                                self.read_percent(book).map(|p| format!("{p} %")).unwrap_or_default()
                            } else {
                                (c.cell)(book)
                            };
                            if missing {
                                let text = if k == 0 { format!("⚠ {text}") } else { text };
                                ui.label(RichText::new(text).color(ui.visuals().weak_text_color()).italics())
                                    .on_hover_text("Fichier introuvable");
                            } else {
                                ui.label(text);
                            }
                        });
                    }
                    let resp = row.response();
                    if cursor_mark.as_deref() == Some(book.id.as_str()) {
                        cursor_stroke(&resp, clip);
                    }
                    if top0.is_none() {
                        top0 = Some(resp.rect.top() - d as f32 * pitch);
                    }
                    if resp.drag_started_by(egui::PointerButton::Primary) {
                        drag_start = Some((rank, book.id.clone(), selected));
                    }
                    if resp.secondary_clicked() && !selected {
                        select_plain = Some(book.id.clone());
                    }
                    resp.context_menu(|ui| {
                        Self::book_file_menu(ui, &self.settings, &book.id, book.file.as_deref(), &mut file_request);
                        let books = if selected { selection.clone() } else { vec![book.id.clone()] };
                        crate::ui::library_view::info_menu_item(ui, &books, &mut info_request);
                        book_lists_menu(ui, &lists_menu, &books, &mut list_request);
                        crate::ui::scripts_ui::book_scripts_menu(ui, &scripts_menu, &books, &mut script_request);
                        let any = if selected { selection_missing } else { missing };
                        crate::ui::missing_ui::missing_menu(ui, &missing_menu, book, &books, any, &mut missing_request);
                    });
                    if resp.double_clicked() {
                        open = Some(book.id.clone());
                    } else if resp.clicked() {
                        select = Some(book.id.clone());
                    }
                });
            });
        });
        let mut hint = None;
        if bg.clicked() && !mods.command {
            self.library.clear_selection();
        }
        if let Some((index, id, selected)) = drag_start {
            if selected {
                // Ligne sélectionnée : glisser les livres.
                start_books_drag(&ctx, selection.clone());
            } else {
                // Ligne non sélectionnée : balayage depuis la ligne où le bouton a été enfoncé.
                let anchor = match (press_origin, top0) {
                    (Some(p), Some(t0)) => {
                        table_row_at(p.y, t0, pitch, n_rows).map(|d| book_rank_of_row(&headers, d)).unwrap_or(index)
                    }
                    _ => index,
                };
                let base = if mods.command { self.library.selection_set() } else { Default::default() };
                self.library.selected_book = view.get(anchor).map(|&i| db.books[i].id.clone()).or(Some(id));
                self.library.select_view_indices([anchor], &base);
                self.library.gesture = Gesture::Sweep { anchor, base };
            }
        }
        if let Gesture::Sweep { anchor, base } = self.library.gesture.clone() {
            if !primary_down {
                self.library.gesture = Gesture::None;
            } else if let (Some(p), Some(t0)) = (pointer, top0) {
                if let Some(d) = table_row_at(p.y, t0, pitch, n_rows) {
                    let cur = book_rank_of_row(&headers, d).min(n.saturating_sub(1));
                    self.library.select_view_indices(sweep_range(anchor, cur), &base);
                }
            }
        }
        // Livres glissés au-dessus des lignes : trait d'insertion (réordonnancement) ou refus.
        if let (Some(d), Some(p), Some(clip), Some(t0)) = (&drag, pointer, body_clip, top0) {
            if clip.contains(p) {
                match &reorder {
                    Ok(()) => {
                        let at = table_insertion(p.y, t0, pitch, n_rows);
                        // Séries glissées dans un tableau groupé : frontière entre deux groupes
                        // la plus proche (titre d'un groupe, ou fin).
                        let group_ins = (d.groups > 0 && !headers.is_empty()).then(|| {
                            let mut b: Vec<usize> = headers.clone();
                            b.push(n_rows);
                            (0..b.len()).min_by_key(|&k| b[k].abs_diff(at)).map(|k| (k, b[k])).unwrap_or((0, 0))
                        });
                        let line = group_ins.map(|(_, row)| row).unwrap_or(at);
                        // Gardé dans la zone visible (trait d'une insertion en tête de liste).
                        let y = (t0 + line as f32 * pitch - (pitch - row_h) / 2.0)
                            .clamp(clip.top() + 2.0, clip.bottom() - 2.0);
                        paint_insertion(ui, clip, pos2(clip.left() + 2.0, y), pos2(clip.right() - 2.0, y));
                        hint = Some(DropHint::Move);
                        if released {
                            egui::DragAndDrop::clear_payload(&ctx);
                            let anchor = match group_ins {
                                Some((k, _)) => {
                                    let ids: Vec<Vec<String>> = groups.iter().map(|g| g.1.clone()).collect();
                                    group_drop_anchor(&ids, &d.books, k)
                                }
                                None => {
                                    reorder_anchor(&self.library.view_ids(), &d.books, book_rank_of_row(&headers, at))
                                }
                            };
                            if let Some(anchor) = anchor {
                                list_request = Some((BookListAction::Reorder(anchor), d.books.clone()));
                            }
                        }
                    }
                    Err(why) => hint = Some(DropHint::Refused(why.clone())),
                }
            }
        }
        if hint.is_some() {
            self.lists_ui.drag_hint = hint;
        }
        // Séries : clic (sélection), glisser, monter / descendre (v0.10.2).
        if let Some(gi) = group_click {
            self.library.select_group(&groups[gi].1, mods.command);
        }
        if let Some(gi) = group_drag_start {
            let ids = &groups[gi].1;
            if !self.library.group_selected(ids) {
                self.library.select_group(ids, mods.command);
            }
            let books = self.library.selection();
            let set: std::collections::HashSet<&String> = books.iter().collect();
            let count = groups.iter().filter(|g| g.1.iter().all(|b| set.contains(b))).count();
            start_group_drag(&ctx, books, count);
        }
        if let Some((gi, up)) = group_shift {
            let ids: Vec<Vec<String>> = groups.iter().map(|g| g.1.clone()).collect();
            if let Some(anchor) = group_shift_anchor(&ids, gi, up) {
                list_request = Some((BookListAction::Reorder(anchor), ids[gi].clone()));
            }
        }
        if let Some(k) = sort_by {
            if self.settings.library_sort == k {
                self.settings.library_descending = !self.settings.library_descending;
            } else {
                self.settings.library_sort = k;
                self.settings.library_descending = false;
            }
        }
        if let Some((action, id)) = file_request {
            self.library.selected_book = Some(id.clone());
            self.ask_file_action(action, &[id]);
        }
        if let Some(id) = select {
            self.library.click_book(&id, mods.command, mods.shift);
        }
        if let Some(id) = select_plain {
            self.library.click_book(&id, false, false);
        }
        if let Some((action, books)) = list_request {
            self.book_list_request(ui.ctx(), action, books);
        }
        if let Some(id) = open {
            self.library.selected_book = Some(id.clone());
            self.open_library_book(ui.ctx(), &id, crate::app::Start::Auto);
        }
        if let Some((script, books)) = script_request {
            self.run_script(ui.ctx(), &script, comicrust_core::scripting::Hook::Books, books);
        }
        if let Some(books) = info_request {
            self.open_info_editor(books);
        }
        if missing_request.is_some() {
            self.missing_ui.pending = missing_request;
        }
    }
}

/// Cadre du curseur du clavier autour d'une ligne.
fn cursor_stroke(resp: &egui::Response, clip: egui::Rect) {
    let p = resp.ctx.layer_painter(resp.layer_id).with_clip_rect(clip);
    p.rect_stroke(
        resp.rect.shrink(1.0),
        2.0,
        egui::Stroke::new(1.5, crate::ui::library_view::ACCENT),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_sizes() {
        assert_eq!(short_date(Some("2019-10-01T12:58:00.1234567+02:00")), "01/10/2019");
        assert_eq!(long_date(Some("2019-10-01T12:58:00")), "01/10/2019 12:58");
        assert_eq!(short_date(Some("0001-01-01T00:00:00")), "");
        assert_eq!(human_size(48_173_972), "48,2 Mo");
        assert_eq!(stars(Some(3.5)), "★★★★☆");
    }
}
