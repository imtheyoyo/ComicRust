//! Commande IPC « Marquer comme lu / non lu » (lot menu du livre). Contrat :
//! `docs/tauri/ipc-lecture-etat.md`. Fine : la logique (testée sans Tauri) est dans
//! [`crate::lecture_etat`]. `async` : l'écriture dans une base distante peut prendre du temps.

use crate::biblio::Biblio;
use crate::edition::Edition;
use crate::lecteur::Lecteur;
use crate::lecture_etat::{self, BilanLecture};
use tauri::Emitter;

/// Marque les livres `ids` comme lus (`lu` vrai) ou non lus. Émet `bibliotheque-modifiee` (comme
/// l'édition) pour que les vues, filtres et listes intelligentes se rafraîchissent.
#[tauri::command(async)]
pub fn marquer_lecture(
    app: tauri::AppHandle,
    biblio: tauri::State<'_, Biblio>,
    edition: tauri::State<'_, Edition>,
    lecteur: tauri::State<'_, Lecteur>,
    ids: Vec<String>,
    lu: bool,
) -> Result<BilanLecture, String> {
    let bilan = lecture_etat::marquer_lecture(&biblio, &edition, &lecteur, &ids, lu)?;
    if !bilan.modifies.is_empty() {
        let _ = app.emit("bibliotheque-modifiee", serde_json::json!({ "ids": bilan.modifies }));
    }
    Ok(bilan)
}
