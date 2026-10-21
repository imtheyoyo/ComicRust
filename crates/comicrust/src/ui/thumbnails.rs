//! Panneau des vignettes.

use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Ui};

use crate::app::ComicRustApp;
use crate::book::loader::{Request, THUMB_HEIGHT};

const THUMB_ROW: f32 = THUMB_HEIGHT as f32 * 0.6 + 26.0;

impl ComicRustApp {
    pub(crate) fn thumbnails_panel(&mut self, ui: &mut Ui) {
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
            let offset = (current as f32 * (THUMB_ROW + ui.spacing().item_spacing.y) - viewport_h / 2.0
                + THUMB_ROW / 2.0)
                .max(0.0);
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
                        painter.text(
                            img_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "⚠",
                            egui::FontId::proportional(20.0),
                            Color32::LIGHT_RED,
                        );
                    }
                    None => {
                        painter.rect_stroke(
                            img_rect.shrink(img_rect.width() * 0.2),
                            2.0,
                            (1.0, Color32::DARK_GRAY),
                            egui::StrokeKind::Inside,
                        );
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
}
