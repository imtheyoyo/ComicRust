//! ComicRust, version Tauri 2 : couche fine au-dessus de `comicrust-core`.
//!
//! Règle : **aucune logique métier ici ni dans le front**. Ce crate ne contient que
//! - l'emplacement portable des données ([`portable`]),
//! - les thèmes CSS de l'utilisateur ([`themes`]),
//! - les commandes IPC (module `app`, fonction `app` uniquement) qui appellent `comicrust-core`.
//!
//! Le code sans dépendance à Tauri est compilé et testé partout (Linux compris) ; la fenêtre
//! n'est compilée qu'avec `--features app`.

pub mod portable;
pub mod themes;

use serde::Serialize;
use std::path::Path;

/// Informations générales affichées par la page d'accueil.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InfosApplication {
    /// Version de ComicRust (celle du workspace).
    pub version: String,
    /// Dossier `data\` portable (à côté de l'exécutable).
    pub dossier_data: String,
    /// Dossier des thèmes CSS (`data\themes`).
    pub dossier_themes: String,
    /// Dossier du profil WebView2 (`data\webview2`).
    pub dossier_webview: String,
}

/// Rassemble les informations de l'application pour un dossier `data` donné.
pub fn infos_application(data: &Path) -> InfosApplication {
    InfosApplication {
        version: env!("CARGO_PKG_VERSION").to_string(),
        dossier_data: data.display().to_string(),
        dossier_themes: portable::dossier_themes(data).display().to_string(),
        dossier_webview: portable::dossier_webview(data).display().to_string(),
    }
}

#[cfg(feature = "app")]
pub mod app;
