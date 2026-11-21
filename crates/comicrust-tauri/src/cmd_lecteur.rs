//! Commandes IPC des fichiers récents du lecteur (lot L6). Contrat : `docs/tauri/ipc-lecteur.md`.
//!
//! Fines : toute la logique (dédoublonnage, plafond de 15, vérification des fichiers) est dans
//! [`crate::reglages`], testée sans Tauri. Les commandes sont `async` : vérifier qu'un fichier
//! existe encore peut attendre un disque réseau éteint, ce qui ne doit jamais figer la fenêtre.

use crate::app::DossierData;
use crate::reglages::{self, Recent};

/// Fichiers récents (le plus récent d'abord), chacun vérifié (`existe`).
#[tauri::command(async)]
pub fn lire_recents(data: tauri::State<'_, DossierData>) -> Result<Vec<Recent>, String> {
    reglages::lire_recents(&data.0)
}

/// Un fichier vient d'être ouvert : il passe en tête. Renvoie la liste à jour.
#[tauri::command(async)]
pub fn ajouter_recent(data: tauri::State<'_, DossierData>, chemin: String) -> Result<Vec<Recent>, String> {
    reglages::ajouter_recent(&data.0, &chemin)
}

/// Retire un fichier de la liste (il ne s'ouvre plus). Renvoie la liste à jour.
#[tauri::command(async)]
pub fn retirer_recent(data: tauri::State<'_, DossierData>, chemin: String) -> Result<Vec<Recent>, String> {
    reglages::retirer_recent(&data.0, &chemin)
}

/// Vide la liste.
#[tauri::command(async)]
pub fn vider_recents(data: tauri::State<'_, DossierData>) -> Result<Vec<Recent>, String> {
    reglages::vider_recents(&data.0)
}
