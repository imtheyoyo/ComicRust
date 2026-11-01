//! Thèmes de l'interface.
//!
//! - **Style** ([`UiStyle`]) : « Moderne » (par défaut, palette nuit violette inspirée des
//!   maquettes Nebula BD de `docs/design/`, arrondis, ombres douces, espacements aérés) ou
//!   « Classique » (l'apparence d'origine d'egui, celle des versions ≤ 0.10.2).
//! - **Luminosité** ([`Theme`]) : sombre, clair ou comme Windows. Chaque style existe dans
//!   les deux variantes ; egui choisit la bonne selon la luminosité.
//!
//! Les couleurs « sémantiques » (succès, erreur, accent, badge « manquant »…) passent toutes
//! par les fonctions de ce module : elles restent lisibles dans les quatre combinaisons
//! (contraste WCAG AA, 4,5:1, pour le texte).

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, Shadow, Stroke, TextStyle, Ui};
use serde::{Deserialize, Serialize};

/// Luminosité de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    /// Celui de Windows (mode clair ou sombre des applications).
    System,
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Dark, Theme::Light, Theme::System];

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "Comme Windows",
            Theme::Dark => "Sombre",
            Theme::Light => "Clair",
        }
    }
}

/// Style de l'interface (v0.10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiStyle {
    /// Palette nuit violette, arrondis, ombres, espacements aérés.
    #[default]
    Modern,
    /// Apparence d'origine d'egui (versions précédentes).
    Classic,
}

impl UiStyle {
    pub const ALL: [UiStyle; 2] = [UiStyle::Modern, UiStyle::Classic];

    pub fn label(self) -> &'static str {
        match self {
            UiStyle::Modern => "Moderne",
            UiStyle::Classic => "Classique",
        }
    }
}

/// Style actif (lu par les couleurs sémantiques, qui n'ont que le `Ui` sous la main).
static MODERN: AtomicBool = AtomicBool::new(true);

/// Vrai si le style « Moderne » est actif.
pub fn is_modern() -> bool {
    MODERN.load(Ordering::Relaxed)
}

/// Applique le style et la luminosité choisis.
pub fn apply(ctx: &egui::Context, theme: Theme, style: UiStyle) {
    MODERN.store(style == UiStyle::Modern, Ordering::Relaxed);
    match style {
        UiStyle::Modern => {
            ctx.set_style_of(egui::Theme::Dark, modern_style(&Palette::DARK));
            ctx.set_style_of(egui::Theme::Light, modern_style(&Palette::LIGHT));
        }
        UiStyle::Classic => {
            ctx.set_style_of(egui::Theme::Dark, classic_style(egui::Visuals::dark()));
            ctx.set_style_of(egui::Theme::Light, classic_style(egui::Visuals::light()));
        }
    }
    ctx.set_theme(match theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Dark => egui::ThemePreference::Dark,
        Theme::Light => egui::ThemePreference::Light,
    });
}

fn classic_style(visuals: egui::Visuals) -> egui::Style {
    egui::Style { visuals, ..Default::default() }
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Palette du thème « Moderne » (une par luminosité, même identité).
struct Palette {
    dark: bool,
    /// Fond de la zone des couvertures (le plus profond).
    canvas: Color32,
    /// Panneaux (barre d'outils, barre d'état, listes, détails).
    panel: Color32,
    /// Fenêtres et menus (le plus haut).
    window: Color32,
    /// Lignes alternées, cadres de groupe.
    faint: Color32,
    /// Champs de saisie.
    field: Color32,
    /// Boutons au repos, survolés, enfoncés.
    raised: Color32,
    /// Fond des cases à cocher et boutons radio au repos, et leur contour.
    check: Color32,
    check_border: Color32,
    hover: Color32,
    pressed: Color32,
    /// Bordures fines et séparateurs ; bordure plus marquée (survol, fenêtres).
    border: Color32,
    border_strong: Color32,
    text: Color32,
    text_strong: Color32,
    text_weak: Color32,
    /// Accent unique : liens, barres de lecture, curseur du clavier.
    accent: Color32,
    /// Accent en fond plein, sous du texte blanc (badges, pastilles).
    accent_fill: Color32,
    /// Sélection : fond et texte.
    sel_bg: Color32,
    sel_fg: Color32,
    shadow_alpha: u8,
}

impl Palette {
    const DARK: Palette = Palette {
        dark: true,
        canvas: hex(0x0F1220),
        panel: hex(0x151A2C),
        window: hex(0x1B2136),
        faint: hex(0x1A2034),
        field: hex(0x111526),
        raised: hex(0x242B45),
        check: hex(0x2C3454),
        check_border: hex(0x353D60),
        hover: hex(0x2E3657),
        pressed: hex(0x39426A),
        border: hex(0x262D48),
        border_strong: hex(0x3A4366),
        text: hex(0xD9DCEA),
        text_strong: hex(0xF4F5FB),
        text_weak: hex(0x9AA0B8),
        accent: hex(0x8B7CF6),
        accent_fill: hex(0x6C5CE7),
        sel_bg: hex(0x4A3FA8),
        sel_fg: hex(0xEEEBFF),
        shadow_alpha: 120,
    };

    const LIGHT: Palette = Palette {
        dark: false,
        canvas: hex(0xF4F5FA),
        panel: hex(0xECEEF6),
        window: hex(0xFFFFFF),
        faint: hex(0xF1F2F8),
        field: hex(0xFFFFFF),
        raised: hex(0xE1E4F0),
        check: hex(0xE9EBF4),
        check_border: hex(0xC3C8DC),
        hover: hex(0xD6DAEB),
        pressed: hex(0xC9CEE6),
        border: hex(0xD5D9E7),
        border_strong: hex(0xB8BED6),
        text: hex(0x2A2F45),
        text_strong: hex(0x141828),
        text_weak: hex(0x5C6280),
        accent: hex(0x5B4BD6),
        accent_fill: hex(0x5B4BD6),
        sel_bg: hex(0xDCD6FF),
        sel_fg: hex(0x3A2BB0),
        shadow_alpha: 38,
    };
}

/// Rayons des arrondis du thème « Moderne ».
const R_WIDGET: u8 = 6;
const R_MENU: u8 = 8;
const R_WINDOW: u8 = 10;
/// Boutons « pilule » de la barre d'outils.
const R_PILL: u8 = 13;

fn widget(bg: Color32, border: Color32, fg: Color32, expansion: f32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, border),
        corner_radius: CornerRadius::same(R_WIDGET),
        fg_stroke: Stroke::new(1.5, fg),
        expansion,
    }
}

fn modern_style(p: &Palette) -> egui::Style {
    let mut style = egui::Style::default();
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };

    v.panel_fill = p.panel;
    v.window_fill = p.window;
    v.faint_bg_color = p.faint;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.code_bg_color = p.raised;
    v.hyperlink_color = if p.dark { hex(0xA99CFF) } else { p.accent };
    v.warn_fg_color = if p.dark { hex(0xF2B05E) } else { hex(0xA15C00) };
    v.error_fg_color = if p.dark { hex(0xFF7A7A) } else { hex(0xC22B2B) };
    v.weak_text_color = Some(p.text_weak);
    v.selection.bg_fill = p.sel_bg;
    v.selection.stroke = Stroke::new(1.0, p.sel_fg);

    v.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: p.window,
        weak_bg_fill: p.faint,
        bg_stroke: Stroke::new(1.0, p.border),
        corner_radius: CornerRadius::same(R_WIDGET),
        fg_stroke: Stroke::new(1.0, p.text),
        expansion: 0.0,
    };
    // Boutons : `weak_bg_fill` ; cases à cocher, radios, curseurs : `bg_fill` (plus visible).
    v.widgets.inactive = widget(p.raised, p.check_border, p.text, 0.0);
    v.widgets.inactive.bg_fill = p.check;
    v.widgets.hovered = widget(p.hover, p.border_strong, p.text_strong, 0.5);
    v.widgets.active = widget(p.pressed, p.accent, p.text_strong, 0.5);
    v.widgets.open = widget(p.hover, p.border_strong, p.text_strong, 0.0);

    v.window_corner_radius = CornerRadius::same(R_WINDOW);
    v.menu_corner_radius = CornerRadius::same(R_MENU);
    v.window_stroke = Stroke::new(1.0, p.border_strong);
    v.window_shadow = Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(p.shadow_alpha) };
    v.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(p.shadow_alpha) };
    v.window_highlight_topmost = true;
    v.slider_trailing_fill = true;
    v.striped = true;
    v.indent_has_left_vline = false;
    v.disabled_alpha = 0.45;
    v.handle_shape = egui::style::HandleShape::Circle;
    style.visuals = v;

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(9.0, 4.0);
    s.interact_size = egui::vec2(40.0, 24.0);
    s.window_margin = Margin::same(14);
    s.menu_margin = Margin::same(6);
    s.menu_spacing = 3.0;
    s.indent = 18.0;
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.icon_spacing = 6.0;
    s.combo_height = 300.0;
    s.slider_rail_height = 6.0;
    s.scroll.bar_width = 8.0;
    s.scroll.floating_width = 3.0;

    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ]
    .into();
    style
}

/// Cadre d'un panneau (barres d'outils et d'état, panneaux latéraux) : fond des panneaux,
/// marges aérées. `None` en Classique (cadre par défaut d'egui).
pub fn panel_frame(ui: &Ui, margin_x: i8, margin_y: i8) -> Option<egui::Frame> {
    is_modern()
        .then(|| egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(Margin::symmetric(margin_x, margin_y)))
}

/// Fenêtre modale : en Moderne, cadre de fenêtre (marges, arrondi, ombre) plutôt que le
/// cadre serré des menus.
pub fn modal(ctx: &egui::Context, id: egui::Id) -> egui::Modal {
    let modal = egui::Modal::new(id);
    if is_modern() {
        modal.frame(egui::Frame::window(&ctx.global_style()))
    } else {
        modal
    }
}

/// Boutons « pilule » dans `ui` (barre d'outils) : arrondi complet, fond discret au repos.
pub fn pill_buttons(ui: &mut Ui) {
    if !is_modern() {
        return;
    }
    let w = &mut ui.style_mut().visuals.widgets;
    for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.corner_radius = CornerRadius::same(R_PILL);
    }
}

fn dark(ui: &Ui) -> bool {
    ui.visuals().dark_mode
}

/// Accent du style Classique (orange des versions précédentes).
const CLASSIC_ACCENT: Color32 = Color32::from_rgb(214, 110, 40);

/// Couleur d'accent (sélection en bord, barres de lecture, curseur du clavier).
pub fn accent(ui: &Ui) -> Color32 {
    accent_for(dark(ui))
}

/// [`accent`] hors d'un `Ui` (dessin dans un calque).
pub fn accent_of(ctx: &egui::Context) -> Color32 {
    accent_for(ctx.global_style().visuals.dark_mode)
}

fn accent_for(dark: bool) -> Color32 {
    match (is_modern(), dark) {
        (false, _) => CLASSIC_ACCENT,
        (true, true) => Palette::DARK.accent,
        (true, false) => Palette::LIGHT.accent,
    }
}

/// Accent en fond plein, sous un texte [`on_accent`] (badges).
pub fn accent_fill(ui: &Ui) -> Color32 {
    accent_fill_for(dark(ui))
}

/// [`accent_fill`] hors d'un `Ui`.
pub fn accent_fill_of(ctx: &egui::Context) -> Color32 {
    accent_fill_for(ctx.global_style().visuals.dark_mode)
}

fn accent_fill_for(dark: bool) -> Color32 {
    match (is_modern(), dark) {
        (false, _) => CLASSIC_ACCENT,
        (true, true) => Palette::DARK.accent_fill,
        (true, false) => Palette::LIGHT.accent_fill,
    }
}

/// Texte sur [`accent_fill`] ou sur [`missing_fill`].
pub fn on_accent() -> Color32 {
    Color32::WHITE
}

/// Fond du badge « manquant ».
pub fn missing_fill(ui: &Ui) -> Color32 {
    if is_modern() && !dark(ui) {
        hex(0xB83232)
    } else {
        Color32::from_rgb(170, 40, 40)
    }
}

/// Fond de la zone des couvertures et du tableau de la bibliothèque.
pub fn canvas(ui: &Ui) -> Color32 {
    if !is_modern() {
        return ui.visuals().panel_fill;
    }
    if dark(ui) {
        Palette::DARK.canvas
    } else {
        Palette::LIGHT.canvas
    }
}

pub fn ok(ui: &Ui) -> Color32 {
    match (is_modern(), dark(ui)) {
        (true, true) => hex(0x5FD08A),
        (true, false) => hex(0x1A7F3C),
        (false, true) => Color32::LIGHT_GREEN,
        (false, false) => Color32::from_rgb(20, 130, 40),
    }
}

pub fn error(ui: &Ui) -> Color32 {
    match (is_modern(), dark(ui)) {
        (true, true) => hex(0xFF7A7A),
        (true, false) => hex(0xC22B2B),
        (false, true) => Color32::LIGHT_RED,
        (false, false) => Color32::from_rgb(190, 30, 30),
    }
}

pub fn warning(ui: &Ui) -> Color32 {
    match (is_modern(), dark(ui)) {
        (true, true) => hex(0xF2B05E),
        (true, false) => hex(0xA15C00),
        (false, true) => Color32::from_rgb(240, 170, 90),
        (false, false) => Color32::from_rgb(170, 90, 0),
    }
}

/// Emplacement d'image pas encore prête.
pub fn placeholder(ui: &Ui) -> Color32 {
    match (is_modern(), dark(ui)) {
        (true, true) => hex(0x20263C),
        (true, false) => hex(0xDDE0EC),
        (false, true) => Color32::from_gray(40),
        (false, false) => Color32::from_gray(215),
    }
}

/// Contour d'une vignette pas encore chargée.
pub fn placeholder_stroke(ui: &Ui) -> Color32 {
    if is_modern() {
        ui.visuals().widgets.noninteractive.bg_stroke.color
    } else {
        Color32::DARK_GRAY
    }
}

/// Fond du lecteur autour des pages : gris neutre dans tous les styles (ne colore pas la BD).
pub fn reader_background(ui: &Ui) -> Color32 {
    if dark(ui) {
        Color32::from_rgb(24, 24, 26)
    } else {
        Color32::from_rgb(226, 226, 228)
    }
}

/// Arrondi des couvertures (0 en Classique).
pub fn cover_radius() -> u8 {
    if is_modern() {
        5
    } else {
        0
    }
}

/// Fond d'une carte de couverture selon son état (survol, sélection) ; rien au repos.
pub fn paint_card_state(ui: &Ui, rect: Rect, selected: bool, hovered: bool) {
    let p = ui.painter();
    let v = ui.visuals();
    if !is_modern() {
        if selected {
            p.rect_filled(rect.expand(5.0), 6.0, v.selection.bg_fill);
        } else if hovered {
            p.rect_filled(rect.expand(4.0), 6.0, v.widgets.hovered.bg_fill);
        }
        return;
    }
    let r = rect.expand(6.0);
    if selected {
        let a = accent(ui);
        p.rect_filled(r, 10.0, a.gamma_multiply(if dark(ui) { 0.22 } else { 0.16 }));
        p.rect_stroke(r, 10.0, Stroke::new(2.0, a), egui::StrokeKind::Inside);
    } else if hovered {
        p.rect_filled(r, 10.0, v.widgets.hovered.bg_fill.gamma_multiply(0.7));
        p.rect_stroke(r, 10.0, Stroke::new(1.0, v.widgets.hovered.bg_stroke.color), egui::StrokeKind::Inside);
    }
}

/// Bande d'en-tête d'un tableau de hauteur `height`, à dessiner avant lui (Moderne : fond
/// des panneaux et trait fin dessous, sur toute la largeur visible).
pub fn paint_table_header(ui: &Ui, height: f32) {
    if !is_modern() {
        return;
    }
    let top = ui.cursor().top();
    let bottom = top + height + ui.spacing().item_spacing.y / 2.0;
    let r = Rect::from_x_y_ranges(ui.clip_rect().x_range(), top..=bottom);
    let v = ui.visuals();
    ui.painter().rect_filled(r, 0.0, v.panel_fill);
    ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color));
}

/// Ombre douce sous une couverture (Moderne seulement).
pub fn paint_cover_shadow(ui: &Ui, rect: Rect) {
    if !is_modern() {
        return;
    }
    let alpha = if dark(ui) { 150 } else { 60 };
    let shadow = Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(alpha) };
    ui.painter().add(shadow.as_shape(rect, CornerRadius::same(cover_radius())));
}

/// Barre d'avancement de lecture en bas de `cover` (part `fraction`, de 0 à 1).
pub fn paint_read_bar(ui: &Ui, cover: Rect, fraction: f32) {
    let p = ui.painter();
    let fraction = fraction.clamp(0.0, 1.0);
    if !is_modern() {
        let bar =
            Rect::from_min_size(egui::pos2(cover.min.x, cover.max.y - 4.0), egui::vec2(cover.width() * fraction, 4.0));
        p.rect_filled(bar, 0.0, accent(ui));
        return;
    }
    // Rail sombre translucide sur toute la largeur, remplissage d'accent arrondi.
    let track = Rect::from_min_max(
        egui::pos2(cover.min.x + 5.0, cover.max.y - 9.0),
        egui::pos2(cover.max.x - 5.0, cover.max.y - 5.0),
    );
    p.rect_filled(track, 2.0, Color32::from_black_alpha(150));
    let fill = Rect::from_min_size(track.min, egui::vec2(track.width() * fraction, track.height()));
    let color = if dark(ui) { Palette::DARK.accent } else { hex(0x8B7CF6) };
    p.rect_filled(fill, 2.0, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contraste WCAG entre deux couleurs.
    fn contraste(a: Color32, b: Color32) -> f32 {
        fn lum(c: Color32) -> f32 {
            let f = |x: u8| {
                let x = x as f32 / 255.0;
                if x <= 0.03928 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn textes_du_theme_moderne_lisibles() {
        for p in [&Palette::DARK, &Palette::LIGHT] {
            for fond in [p.canvas, p.panel, p.window, p.faint, p.field] {
                assert!(contraste(p.text, fond) >= 7.0, "texte sur {fond:?}");
                assert!(contraste(p.text_weak, fond) >= 4.5, "texte atténué sur {fond:?}");
            }
            assert!(contraste(p.text, p.raised) >= 4.5);
            assert!(contraste(p.text_strong, p.pressed) >= 4.5);
            assert!(contraste(p.sel_fg, p.sel_bg) >= 4.5, "texte sélectionné");
            assert!(contraste(Color32::WHITE, p.accent_fill) >= 4.5, "texte sur l'accent");
        }
        assert!(contraste(Color32::WHITE, hex(0xB83232)) >= 4.5, "badge « manquant »");
    }
}
