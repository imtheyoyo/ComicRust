//! Glisser-déposer des livres (v0.9.1) : charge utile, retour visuel (curseur, badge
//! « N livres » qui suit le pointeur, message) et défilement automatique.
//!
//! Les calculs (positions, règles d'acceptation) sont dans [`crate::library::mouse`].

use eframe::egui::{self, pos2, vec2, Color32, Context, CursorIcon, FontId, Id, LayerId, Order, Rect, Ui};

use crate::app::ComicRustApp;
use crate::library::mouse::edge_scroll_speed;

/// Charge utile : livres glissés depuis la vue (dans l'ordre de la vue).
#[derive(Debug, Clone)]
pub struct BooksDrag {
    pub books: Vec<String>,
}

/// Ce que ferait un dépôt à l'endroit survolé (calculé à chaque image par la zone survolée).
#[derive(Debug, Clone, PartialEq)]
pub enum DropHint {
    /// Réordonner la liste affichée.
    Move,
    /// Ajouter à la liste nommée.
    Add(String),
    /// Refusé, avec la raison.
    Refused(String),
}

/// Livres en cours de glisser, s'il y en a.
pub fn dragged_books(ctx: &Context) -> Option<std::sync::Arc<BooksDrag>> {
    egui::DragAndDrop::payload::<BooksDrag>(ctx)
}

/// Commence le glisser des livres.
pub fn start_books_drag(ctx: &Context, books: Vec<String>) {
    if !books.is_empty() {
        egui::DragAndDrop::set_payload(ctx, BooksDrag { books });
    }
}

/// Défilement automatique de la zone `area` quand le pointeur approche de ses bords (ou les
/// dépasse) ; à appeler dans le contenu de la zone qui défile.
pub fn edge_scroll(ui: &Ui, area: Rect, pointer: egui::Pos2) {
    let speed = edge_scroll_speed(area.top(), area.bottom(), pointer.y);
    if speed != 0.0 {
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.1);
        ui.scroll_with_delta_animation(vec2(0.0, speed * dt), egui::style::ScrollAnimation::none());
        ui.ctx().request_repaint();
    }
}

/// Trait d'insertion (réordonnancement) entre `a` et `b`, en coordonnées de l'écran.
pub fn paint_insertion(ui: &Ui, clip: Rect, a: egui::Pos2, b: egui::Pos2) {
    let color = ui.visuals().selection.stroke.color;
    let painter =
        ui.ctx().layer_painter(LayerId::new(Order::Foreground, Id::unique("insertion-livres"))).with_clip_rect(clip);
    painter.line_segment([a, b], egui::Stroke::new(3.0, color));
    // Petites barres aux extrémités, comme dans l'explorateur.
    let d = (b - a).normalized();
    let n = vec2(-d.y, d.x) * 5.0;
    for p in [a, b] {
        painter.line_segment([p - n, p + n], egui::Stroke::new(3.0, color));
    }
}

impl ComicRustApp {
    /// Retour visuel du glisser de livres, après le dessin de toutes les zones : curseur et
    /// badge « N livres » (avec ce que ferait le dépôt) qui suit le pointeur.
    pub(crate) fn books_drag_overlay(&mut self, ctx: &Context) {
        let hint = self.lists_ui.drag_hint.take();
        let Some(drag) = dragged_books(ctx) else { return };
        let Some(pointer) = ctx.pointer_latest_pos() else { return };
        ctx.set_cursor_icon(match hint {
            Some(DropHint::Move) => CursorIcon::Grabbing,
            Some(DropHint::Add(_)) => CursorIcon::Copy,
            Some(DropHint::Refused(_)) | None => CursorIcon::NotAllowed,
        });
        let n = drag.books.len();
        let title = format!("📚 {n} livre{}", if n > 1 { "s" } else { "" });
        let detail = match &hint {
            Some(DropHint::Move) => Some("Déplacer ici".to_string()),
            Some(DropHint::Add(name)) => Some(format!("Ajouter à « {name} »")),
            Some(DropHint::Refused(why)) => Some(format!("⛔ {why}")),
            None => None,
        };
        let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("badge-livres")));
        let visuals = ctx.global_style().visuals.clone();
        let strong = painter.layout_no_wrap(title, FontId::proportional(13.0), Color32::WHITE);
        let detail_color = match hint {
            Some(DropHint::Refused(_)) => visuals.error_fg_color,
            _ => visuals.text_color(),
        };
        let small = detail.map(|d| painter.layout(d, FontId::proportional(12.0), detail_color, 320.0));
        let pad = vec2(8.0, 5.0);
        let badge_size = strong.size() + pad * 2.0;
        let mut size = badge_size;
        if let Some(g) = &small {
            size.x = size.x.max(g.size().x + pad.x * 2.0);
            size.y += g.size().y + pad.y;
        }
        let mut min = pointer + vec2(18.0, 20.0);
        // Reste dans la fenêtre.
        let screen = ctx.content_rect();
        if min.x + size.x > screen.right() {
            min.x = pointer.x - 10.0 - size.x;
        }
        if min.y + size.y > screen.bottom() {
            min.y = pointer.y - 10.0 - size.y;
        }
        let frame = Rect::from_min_size(min, size);
        painter.rect_filled(frame, 6.0, visuals.window_fill.gamma_multiply(0.96));
        painter.rect_stroke(frame, 6.0, visuals.window_stroke, egui::StrokeKind::Inside);
        let badge = Rect::from_min_size(min, badge_size);
        painter.rect_filled(badge.shrink(2.0), 5.0, super::library_view::ACCENT);
        painter.galley(badge.min + pad, strong, Color32::WHITE);
        if let Some(g) = small {
            painter.galley(pos2(min.x + pad.x, badge.max.y + pad.y * 0.5), g, detail_color);
        }
        ctx.request_repaint();
    }
}
