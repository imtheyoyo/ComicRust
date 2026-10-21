//! Vue Bibliothèque : grille de couvertures des livres de la base ComicRack, recherche,
//! regroupement par série. Seules les rangées visibles sont dessinées (une bibliothèque
//! peut compter des dizaines de milliers de livres).

use std::path::PathBuf;

use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui};

use crate::app::ComicRustApp;
use crate::library::covers::{Cover, CoverCache};
use crate::library::{Row, Status};

const HEADER_H: f32 = 34.0;
const CAPTION_H: f32 = 40.0;
const GAP: f32 = 14.0;
/// Rapport largeur/hauteur des emplacements de couverture (format comics).
const COVER_RATIO: f32 = 0.66;

impl ComicRustApp {
    pub(crate) fn library_shown(&self) -> usize {
        self.library.shown()
    }

    pub(crate) fn library_view(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.covers.is_none() {
            let dir = self.data_dir.as_ref().map(|d| d.join(crate::paths::COVERS_DIR));
            self.covers = Some(CoverCache::new(dir, &ctx));
        }
        if let Some(c) = &mut self.covers {
            c.poll(&ctx);
        }

        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.library.search)
                        .hint_text("Rechercher : série, numéro, auteur, titre, année…")
                        .desired_width(320.0),
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
                ui.checkbox(&mut self.settings.library_group_by_series, "Grouper par série");
                ui.add(
                    egui::Slider::new(&mut self.settings.library_cover_height, 120.0..=320.0)
                        .show_value(false)
                        .text("Taille"),
                );
                ui.separator();
                if ui.button("⟳ Recharger").on_hover_text("Relire la base ComicRack").clicked() {
                    self.load_library(&ctx);
                }
                if ui.button("⚙ Base ComicRack…").clicked() {
                    self.open_db_window();
                }
            });
        });
        ui.separator();

        match &self.library.status {
            Status::NotConfigured => {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.25);
                    ui.heading("Bibliothèque ComicRack");
                    ui.label("Indique où se trouve ta base ComicRack (MySQL) pour afficher tous tes livres.");
                    ui.label(RichText::new("Lecture seule : ComicRust ne modifie jamais ta base.").weak());
                    ui.add_space(12.0);
                    if ui.button(RichText::new("⚙  Configurer la base ComicRack…").size(17.0)).clicked() {
                        self.open_db_window();
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
                    ui.heading("⚠ Base ComicRack inaccessible");
                    ui.label(RichText::new(err).color(Color32::LIGHT_RED));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Réessayer").clicked() {
                            self.load_library(&ctx);
                        }
                        if ui.button("Paramètres…").clicked() {
                            self.open_db_window();
                        }
                    });
                });
            }
            Status::Loaded(_) => self.library_grid(ui),
        }
    }

    fn library_grid(&mut self, ui: &mut Ui) {
        let Some(db) = self.library.database() else { return };
        let group = self.settings.library_group_by_series;
        let cover_h = self.settings.library_cover_height;
        let cell = vec2(cover_h * COVER_RATIO, cover_h + CAPTION_H);
        let width = ui.available_width() - 2.0 * GAP;
        let columns = (((width + GAP) / (cell.x + GAP)).floor() as usize).max(1);
        let rows = self.library.rows(group, columns);
        let shown = self.library.view(group).len();
        if shown == 0 {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(if db.is_empty() { "La base ne contient aucun livre." } else { "Aucun livre ne correspond." });
            });
            return;
        }

        let heights: Vec<f32> = rows
            .iter()
            .map(|r| match r {
                Row::Header { .. } => HEADER_H,
                Row::Covers(_) => cell.y + GAP,
            })
            .collect();
        let total: f32 = heights.iter().sum::<f32>() + GAP;
        let mut open: Option<(PathBuf, Option<usize>)> = None;

        egui::ScrollArea::vertical().auto_shrink(false).id_salt("bibliotheque").show_viewport(ui, |ui, viewport| {
            ui.set_height(total);
            let origin = ui.min_rect().min + vec2(GAP, GAP * 0.5);
            let mut y = 0.0;
            for (row, h) in rows.iter().zip(&heights) {
                let top = y;
                y += h;
                if y < viewport.min.y || top > viewport.max.y {
                    continue;
                }
                match row {
                    Row::Header { series, count } => {
                        let r = Rect::from_min_size(origin + vec2(0.0, top), vec2(width, HEADER_H));
                        let p = ui.painter();
                        p.text(
                            pos2(r.min.x, r.center().y),
                            Align2::LEFT_CENTER,
                            series,
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
                            let resp = ui.interact(rect, ui.scope_id().with(("livre", &book.id)), Sense::click());
                            let cover_rect = Rect::from_min_size(min, vec2(cell.x, cover_h));
                            let painter = ui.painter();
                            if resp.hovered() {
                                painter.rect_filled(rect.expand(4.0), 6.0, ui.visuals().widgets.hovered.bg_fill);
                            }
                            let covers = self.covers.as_mut().unwrap();
                            match covers.get(&book.id, book.file.as_deref()) {
                                Cover::Ready(tex) => {
                                    let s = tex.size_vec2();
                                    let k = (cover_rect.width() / s.x).min(cover_rect.height() / s.y);
                                    let r = Rect::from_center_size(
                                        pos2(cover_rect.center().x, cover_rect.max.y - s.y * k / 2.0),
                                        s * k,
                                    );
                                    painter.image(
                                        tex.id(),
                                        r,
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                    painter.rect_stroke(
                                        r,
                                        0.0,
                                        (1.0, Color32::from_black_alpha(90)),
                                        StrokeKind::Inside,
                                    );
                                }
                                Cover::Pending => {
                                    painter.rect_filled(cover_rect.shrink(2.0), 3.0, Color32::from_gray(40));
                                }
                                Cover::Failed => {
                                    painter.rect_filled(cover_rect.shrink(2.0), 3.0, Color32::from_gray(34));
                                    painter.text(
                                        cover_rect.center(),
                                        Align2::CENTER_CENTER,
                                        "⚠",
                                        FontId::proportional(26.0),
                                        Color32::from_rgb(200, 120, 90),
                                    );
                                }
                            }
                            // Avancement de lecture enregistré par ComicRack.
                            if let (Some(read), Some(total)) = (book.last_page_read, book.page_count) {
                                if read > 0 && total > 0 {
                                    let f = ((read + 1) as f32 / total as f32).min(1.0);
                                    let bar = Rect::from_min_size(
                                        pos2(cover_rect.min.x, cover_rect.max.y - 4.0),
                                        vec2(cover_rect.width() * f, 4.0),
                                    );
                                    painter.rect_filled(bar, 0.0, Color32::from_rgb(214, 110, 40));
                                }
                            }
                            let caption = if group {
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
                            let galley =
                                painter.layout(caption, FontId::proportional(12.5), ui.visuals().text_color(), cell.x);
                            let text_rect =
                                Rect::from_min_size(pos2(min.x, cover_rect.max.y + 4.0), vec2(cell.x, CAPTION_H - 6.0));
                            painter.with_clip_rect(text_rect).galley(text_rect.min, galley, Color32::WHITE);

                            let resp = resp.on_hover_ui(|ui| {
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
                                if let Some(y) = book.year {
                                    ui.label(format!("Année : {y}"));
                                }
                                if let Some(arc) = &book.story_arc {
                                    ui.label(format!("Arc : {arc}"));
                                }
                                if let Some(f) = &book.file {
                                    ui.label(RichText::new(f).weak().small());
                                }
                            });
                            if resp.clicked() {
                                if let Some(f) = &book.file {
                                    let start = book.current_page.map(|p| p as usize);
                                    open = Some((PathBuf::from(f), start));
                                }
                            }
                        }
                    }
                }
            }
        });

        if let Some((path, start)) = open {
            self.open_book_at(ui.ctx(), path, start);
        }
    }
}
