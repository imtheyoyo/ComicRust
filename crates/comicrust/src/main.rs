//! ComicRust — lecteur de BD.
//!
//!   ComicRust.exe [fichier.cbz | dossier | liste.cbl]
//!
//! Mode portable : si un fichier `ComicRust.portable` (ou un dossier `data`)
//! se trouve à côté de l'exécutable, les réglages sont enregistrés dans
//! `data\` au lieu de `%APPDATA%\ComicRust`. Pratique sur une clé USB.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod loader;
mod reading_list;
mod settings;

use std::path::PathBuf;

fn portable_dir() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    (dir.join("ComicRust.portable").exists() || dir.join("data").is_dir()).then(|| dir.join("data"))
}

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
    let data_dir = match portable_dir() {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            options.persistence_path = Some(dir.join("comicrust.ron"));
            Some(dir)
        }
        None => eframe::storage_dir("ComicRust"),
    };

    eframe::run_native(
        "ComicRust",
        options,
        Box::new(move |cc| Ok(Box::new(app::ComicRustApp::new(cc, initial, data_dir)))),
    )
}
