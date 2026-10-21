//! ComicRust — lecteur de BD.
//!
//!   ComicRust.exe [fichier.cbz | dossier | liste.cbl]
//!
//! Application **portable** : toutes les données sont dans `data\` à côté de l'exécutable
//! (voir `paths.rs`). Le dossier peut être copié tel quel sur une clé USB.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod book;
mod events;
mod input;
mod paths;
mod reading_list;
mod settings;
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).filter(|a| !a.is_empty()).map(PathBuf::from);

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("ComicRust")
        .with_app_id("comicrust")
        .with_inner_size([1280.0, 900.0])
        .with_min_inner_size([480.0, 360.0])
        .with_drag_and_drop(true);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // Si le dossier de l'exe n'est pas modifiable, l'application démarre quand même
    // (sans mémoriser les réglages) et le signale.
    let data_dir = paths::data_dir().ok();
    if let Some(dir) = &data_dir {
        paths::migrate_old_settings(dir);
        options.persistence_path = Some(dir.join(paths::SETTINGS_FILE));
    }

    eframe::run_native(
        "ComicRust",
        options,
        Box::new(move |cc| Ok(Box::new(app::ComicRustApp::new(cc, initial, data_dir)))),
    )
}
