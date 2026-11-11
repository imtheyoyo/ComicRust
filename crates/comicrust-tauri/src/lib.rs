//! ComicRust, version Tauri 2 : couche fine au-dessus de `comicrust-core`.
//!
//! Règle : **aucune logique métier ici ni dans le front**. Ce crate ne contient que
//! - l'emplacement portable des données ([`portable`]),
//! - les thèmes CSS de l'utilisateur ([`themes`]),
//! - la bibliothèque ([`biblio`] : ouverture, liste, détail, dossiers, scan ; [`couvertures`] ;
//!   [`tri`] ; [`reglages`]),
//! - le lecteur ([`lecteur`] : livres ouverts, pages du protocole `cr-page`, avancement ;
//!   [`avancement`] : mémoire locale de lecture),
//! - l'onglet Pages ([`pages_livre`] : pages d'un livre, opérations, vignettes) et la libération
//!   des fichiers avant écriture ([`liberation`]),
//! - l'édition ([`edition`] : infos des livres, ComicInfo.xml, conversion en CBZ, renommage),
//! - les listes ComicRack ([`listes`] : arbre, gestion, livres des listes de lecture ;
//!   [`listes_criteres`] : éditeur de listes intelligentes ; [`listes_cbl`] : import / export `.cbl`),
//! - le transfert de la bibliothèque ComicRack vers la base de ComicRust ([`transfert`]),
//! - les commandes IPC (module `app`, fonction `app` uniquement) qui appellent `comicrust-core`.
//!
//! Le code sans dépendance à Tauri est compilé et testé partout (Linux compris) ; la fenêtre
//! n'est compilée qu'avec `--features app`.

pub mod avancement;
pub mod biblio;
pub mod couvertures;
pub mod edition;
pub mod lecteur;
pub mod liberation;
pub mod listes;
pub mod listes_cbl;
pub mod listes_criteres;
pub mod pages_livre;
pub mod portable;
pub mod reglages;
pub mod themes;
pub mod transfert;
pub mod tri;

#[cfg(test)]
mod tests_biblio;
#[cfg(test)]
mod tests_listes;
#[cfg(test)]
mod tests_transfert;

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
#[cfg(feature = "mcp")]
pub mod mcp;
// Logique de la passerelle MCP stdio (testable sans WebView2, donc aussi hors fonction `mcp`).
#[cfg(any(test, feature = "mcp"))]
mod pont_mcp;
