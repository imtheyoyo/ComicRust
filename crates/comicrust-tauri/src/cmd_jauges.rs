//! Commande IPC des jauges des listes (lot jauges). Contrat : `docs/tauri/ipc-jauges.md`.
//! Fine : la logique (testée sans Tauri) est dans [`crate::jauges`]. `async` : la première demande
//! après un changement résout le contenu des listes intelligentes.

use crate::biblio::Biblio;
use crate::jauges::JaugesListes;

/// Nouveaux / non lus / total de la bibliothèque et de chaque liste.
#[tauri::command(async)]
pub fn jauges_listes(biblio: tauri::State<'_, Biblio>) -> Result<JaugesListes, String> {
    biblio.jauges_listes().map(|j| (*j).clone())
}
