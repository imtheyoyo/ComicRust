//! Thème clair / sombre : préférence, et couleurs « sémantiques » qui restent lisibles
//! dans les deux thèmes (succès, erreur, avertissement, emplacements vides…).

use eframe::egui::{self, Color32, Ui};
use serde::{Deserialize, Serialize};

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

    pub fn apply(self, ctx: &egui::Context) {
        ctx.set_theme(match self {
            Theme::System => egui::ThemePreference::System,
            Theme::Dark => egui::ThemePreference::Dark,
            Theme::Light => egui::ThemePreference::Light,
        });
    }
}

fn dark(ui: &Ui) -> bool {
    ui.visuals().dark_mode
}

pub fn ok(ui: &Ui) -> Color32 {
    if dark(ui) {
        Color32::LIGHT_GREEN
    } else {
        Color32::from_rgb(20, 130, 40)
    }
}

pub fn error(ui: &Ui) -> Color32 {
    if dark(ui) {
        Color32::LIGHT_RED
    } else {
        Color32::from_rgb(190, 30, 30)
    }
}

pub fn warning(ui: &Ui) -> Color32 {
    if dark(ui) {
        Color32::from_rgb(240, 170, 90)
    } else {
        Color32::from_rgb(170, 90, 0)
    }
}

/// Emplacement d'image pas encore prête.
pub fn placeholder(ui: &Ui) -> Color32 {
    if dark(ui) {
        Color32::from_gray(40)
    } else {
        Color32::from_gray(215)
    }
}

/// Fond du lecteur autour des pages.
pub fn reader_background(ui: &Ui) -> Color32 {
    if dark(ui) {
        Color32::from_rgb(24, 24, 26)
    } else {
        Color32::from_rgb(226, 226, 228)
    }
}
