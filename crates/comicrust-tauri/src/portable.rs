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
///
/// Échec franc si l'emplacement de l'exécutable est introuvable : jamais de repli sur un
/// chemin relatif (les données iraient dans le dossier courant, profil WebView2 compris).
pub fn data_dir() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dossier = exe
        .parent()
        .filter(|d| d.is_absolute())
        .ok_or_else(|| std::io::Error::other("dossier de l'exécutable introuvable"))?;
    let dir = dossier.join("data");
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

// ───────────────────────────── Instance unique ─────────────────────────────

/// Identifiant d'instance propre au dossier `data\` : deux copies portables dans des dossiers
/// différents restent indépendantes, mais deux lancements sur le même `data\` (exe normal et exe
/// `-mcp` compris) sont une seule instance. `identifiant` : celui de `tauri.conf.json`.
pub fn cle_instance(identifiant: &str, data: &Path) -> String {
    // FNV-1a 64 bits du chemin en minuscules (Windows ignore la casse) : court et stable.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.to_string_lossy().to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{identifiant}.{h:016x}")
}

/// Garde de la première instance (tenue jusqu'à la fin du processus).
pub struct GardeInstance {
    #[cfg(all(windows, feature = "app"))]
    _poignee: isize,
}

/// Vrai si ce processus est la **première** instance sur ce `data\` (mutex nommé `cle`, gardé tant
/// que le processus vit). Une deuxième instance ne doit rien nettoyer (`maj::cleanup` supprimerait le
/// téléchargement en cours de l'autre) : elle transmet son fichier et s'arrête (greffon
/// `single-instance`). Hors Windows : toujours vrai (développement).
pub fn premiere_instance(cle: &str) -> (bool, GardeInstance) {
    imp_instance::prendre(cle)
}

#[cfg(all(windows, feature = "app"))]
mod imp_instance {
    use super::GardeInstance;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    pub fn prendre(cle: &str) -> (bool, GardeInstance) {
        let nom: Vec<u16> = format!("Local\\{cle}-premiere").encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY : nom terminé par un zéro, vivant pendant l'appel ; la poignée est volontairement
        // gardée jusqu'à la fin du processus (le système la libère alors).
        let (h, existe) = unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, nom.as_ptr());
            (h, GetLastError() == ERROR_ALREADY_EXISTS)
        };
        (h.is_null() || !existe, GardeInstance { _poignee: h as isize })
    }
}

#[cfg(not(all(windows, feature = "app")))]
mod imp_instance {
    use super::GardeInstance;

    pub fn prendre(_cle: &str) -> (bool, GardeInstance) {
        (true, GardeInstance {})
    }
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

    #[test]
    fn cle_d_instance_par_dossier_data() {
        let a = cle_instance("fr.imtheyoyo.comicrust", Path::new("D:/BD/ComicRust/data"));
        assert_eq!(a, cle_instance("fr.imtheyoyo.comicrust", Path::new("d:/bd/comicrust/DATA")), "casse ignorée");
        assert_ne!(a, cle_instance("fr.imtheyoyo.comicrust", Path::new("E:/ComicRust/data")));
        assert!(a.starts_with("fr.imtheyoyo.comicrust.") && a.len() == "fr.imtheyoyo.comicrust.".len() + 16);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'), "nom de mutex sûr : {a}");
        assert!(premiere_instance(&a).0, "hors Windows : toujours la première");
    }
}
