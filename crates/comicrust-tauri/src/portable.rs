//! Emplacements portables. Comme la version egui, ComicRust-Tauri est **exclusivement
//! portable** : tout est dans `data\` à côté de l'exécutable, rien dans `%APPDATA%`, ni dans
//! `%LOCALAPPDATA%`, ni dans le registre. Le profil du WebView2 (cache, cookies, stockage local)
//! est redirigé vers `data\webview2`.

use std::path::{Path, PathBuf};

/// Dossier des thèmes CSS de l'utilisateur.
pub const THEMES_DIR: &str = "themes";
/// Profil WebView2.
pub const WEBVIEW_DIR: &str = "webview2";

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

/// `data\themes`.
pub fn dossier_themes(data: &Path) -> PathBuf {
    data.join(THEMES_DIR)
}

/// `data\webview2`.
pub fn dossier_webview(data: &Path) -> PathBuf {
    data.join(WEBVIEW_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sous_dossiers_dans_data() {
        let d = Path::new("data");
        assert_eq!(dossier_themes(d), Path::new("data").join("themes"));
        assert_eq!(dossier_webview(d), Path::new("data").join("webview2"));
    }
}
