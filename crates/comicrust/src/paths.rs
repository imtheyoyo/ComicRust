//! Emplacements des données. ComicRust est **exclusivement portable** : réglages, cache de
//! l'index, couvertures et fichiers de mise à jour sont dans `data\` à côté de l'exécutable.
//! Rien n'est écrit dans `%APPDATA%` ni dans le registre.

use std::path::{Path, PathBuf};

/// Fichier des réglages (format eframe/RON).
pub const SETTINGS_FILE: &str = "comicrust.ron";
/// Cache de l'index des dossiers de BD (listes `.cbl`).
pub const INDEX_CACHE_FILE: &str = "index-bd.json";
/// Dossier des couvertures en cache (vue Bibliothèque).
pub const COVERS_DIR: &str = "couvertures";

/// Rapport du dernier plantage (affiché au lancement suivant, puis renommé).
pub const CRASH_FILE: &str = "plantage.txt";

/// Dossier de l'exécutable.
pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."))
}

/// `data\` à côté de l'exécutable, créé si besoin.
pub fn data_dir() -> std::io::Result<PathBuf> {
    let dir = exe_dir().join("data");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Reprend une seule fois les réglages d'une ancienne version installée (v0.2.x sans
/// mode portable, qui écrivait dans `%APPDATA%\ComicRust`).
pub fn migrate_old_settings(data: &Path) {
    let target = data.join(SETTINGS_FILE);
    if target.exists() {
        return;
    }
    if let Some(old) = eframe::storage_dir("ComicRust") {
        let old_settings = old.join("app.ron");
        if old_settings.exists() {
            let _ = std::fs::copy(&old_settings, &target);
        }
        let old_index = old.join(INDEX_CACHE_FILE);
        if old_index.exists() && !data.join(INDEX_CACHE_FILE).exists() {
            let _ = std::fs::copy(&old_index, data.join(INDEX_CACHE_FILE));
        }
    }
}
