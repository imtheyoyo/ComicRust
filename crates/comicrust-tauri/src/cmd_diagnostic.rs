//! Commandes IPC du lot L7 « Réglages, aide et configuration » (`docs/tauri/ipc-reglages-aide.md`) :
//! fenêtre « À propos / Aide », diagnostic copiable, description de la bibliothèque pour l'assistant
//! rejouable, « Relire » et « Copier les livres ». Compilé avec la feature `app` seulement ; la
//! logique (testée sans Tauri) est dans `diagnostic.rs`.

use crate::biblio::{Biblio, EtatBiblio, RequeteLivres, SourceBiblio};
use crate::diagnostic::{self, ConfigurationBiblio, InfosDiagnostic, SessionDiagnostic};
use comicrust_core::store::StoreConfig;

/// Ce que la session sait de la bibliothèque ouverte (mode, source, nombre de livres).
fn session(biblio: &Biblio) -> SessionDiagnostic {
    let source = match biblio.source() {
        SourceBiblio::Autonome(StoreConfig::MySql(_)) | SourceBiblio::ComicrackMysql(_) => "mysql",
        SourceBiblio::Autonome(_) => "sqlite",
        SourceBiblio::ComicrackXml => "xml",
        SourceBiblio::Aucune => "",
    };
    let mode = if source.is_empty() { None } else { biblio.mode() };
    SessionDiagnostic { mode, source: source.into(), nb_livres: biblio.base().ok().map(|b| b.len()) }
}

/// Informations de la fenêtre « À propos » : version, système, mode, chemins, état de PDFium.
/// Aucun secret (jamais de mot de passe, d'identifiant ni de serveur MySQL).
#[tauri::command(async)]
pub fn infos_diagnostic(biblio: tauri::State<'_, Biblio>) -> InfosDiagnostic {
    diagnostic::collecter(biblio.data(), &session(&biblio), diagnostic::etat_pdfium())
}

/// Texte « Copier les informations de diagnostic » (même contenu que la fenêtre, sans secret).
#[tauri::command(async)]
pub fn texte_diagnostic(biblio: tauri::State<'_, Biblio>) -> String {
    diagnostic::texte_diagnostic(&diagnostic::collecter(biblio.data(), &session(&biblio), diagnostic::etat_pdfium()))
}

/// Bibliothèque enregistrée (mode, fichier ComicRack, base SQLite) : préremplit l'assistant en mode
/// « modification ». Ne crée ni ne modifie rien.
#[tauri::command]
pub fn configuration_biblio(biblio: tauri::State<'_, Biblio>) -> Result<ConfigurationBiblio, String> {
    diagnostic::configuration(biblio.data())
}

/// « ⟳ Relire » : recharge toute la bibliothèque depuis sa source (base SQLite, MySQL ou
/// `ComicDb.xml`). Refusé pendant une analyse ou une opération d'édition.
#[tauri::command(async)]
pub fn relire_bibliotheque(biblio: tauri::State<'_, Biblio>) -> Result<EtatBiblio, String> {
    biblio.ouvrir()
}

/// « 📋 Copier les livres » : rapport texte des livres affichés (série, numéro, volume, année, fichier).
#[tauri::command(async)]
pub fn rapport_livres(biblio: tauri::State<'_, Biblio>, requete: RequeteLivres, titre: String) -> Result<String, String> {
    diagnostic::rapport_livres(&biblio, &requete, &titre)
}
