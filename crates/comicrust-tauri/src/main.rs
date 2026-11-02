//! Point d'entrée de ComicRust-Tauri (compilé avec `--features app`).
// Pas de console sous Windows en release.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

fn main() {
    comicrust_tauri::app::run();
}
