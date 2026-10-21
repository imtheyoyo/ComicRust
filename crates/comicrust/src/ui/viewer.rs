//! Zone de lecture (affichage des pages, clics, molette) et écran d'accueil.

use std::time::{Duration, Instant};

use eframe::egui::{self, pos2, vec2, Color32, Rect, RichText, Sense, Ui, Vec2};

use crate::app::ComicRustApp;
use crate::settings::FitMode;

impl ComicRustApp {
    pub(crate) fn viewer(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let bg = crate::ui::theme::reader_background(ui);
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
        egui::ScrollArea::both().id_salt(("page", book.path.as_os_str(), book.current)).auto_shrink(false).show(
            ui,
            |ui| {
                let area = display.max(avail);
                let (rect, resp) = ui.allocate_exact_size(area, Sense::click());
                let content = Rect::from_center_size(rect.center(), display);
                let painter = ui.painter();
                let order: Vec<usize> =
                    if self.settings.manga { (0..spread.len()).rev().collect() } else { (0..spread.len()).collect() };
                let mut x = content.min.x;
                for k in order {
                    let idx = spread[k];
                    let size = scaled[k] * scale;
                    let r = Rect::from_min_size(pos2(x, content.min.y), size);
                    x += size.x;
                    if let Some(tex) = book.textures.get(&idx) {
                        painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    } else if let Some(err) = book.errors.get(&idx) {
                        painter.rect_stroke(
                            r.shrink(8.0),
                            4.0,
                            (1.0, Color32::from_rgb(160, 60, 60)),
                            egui::StrokeKind::Inside,
                        );
                        painter.text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("⚠ Page {} illisible\n{err}", idx + 1),
                            egui::FontId::proportional(15.0),
                            Color32::LIGHT_RED,
                        );
                    } else {
                        painter.rect_filled(r.shrink(4.0), 2.0, crate::ui::theme::placeholder(ui));
                        painter.text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            "Chargement…",
                            egui::FontId::proportional(16.0),
                            Color32::GRAY,
                        );
                    }
                }
                if resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        clicked_at = Some((p.x, ui.clip_rect()));
                    }
                }
            },
        );
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

    pub(crate) fn welcome(&mut self, ui: &mut Ui) {
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
}
