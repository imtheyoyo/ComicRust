//! Commandes IPC du lot « Scan et dossiers » (`docs/tauri/ipc-scan.md`) : arrêt de l'analyse,
//! analyse automatique du démarrage, réglages « Analyse et suivi ». Compilé avec la feature `app`
//! seulement ; la logique (testée sans Tauri) est dans `biblio.rs` et `reglages.rs`.

use crate::biblio::{Biblio, EvenementScan};
use crate::reglages::{self, ReglagesAnalyse, ReglagesAnalyseComplets};
use crate::scripts::Scripts;
use comicrust_core::scripting::Hook;
use std::sync::Arc;
use tauri::Emitter;

/// « Arrêter l'analyse » : vrai si une analyse était en cours. La suite (bilan partiel, fin) arrive
/// par `scan-bilan` (`arretee` vrai) puis `scan-termine`.
#[tauri::command]
pub fn arreter_scan(biblio: tauri::State<'_, Biblio>) -> bool {
    biblio.arreter_scan()
}

/// Analyse du démarrage : même événements que `lancer_scan`. Renvoie faux (sans erreur) quand elle
/// n'a pas lieu : mode lié, hors ligne, sans dossier, ou désactivée dans les réglages.
#[tauri::command(async)]
pub fn lancer_scan_auto(
    app: tauri::AppHandle,
    biblio: tauri::State<'_, Biblio>,
    scripts: tauri::State<'_, Scripts>,
) -> Result<bool, String> {
    let (bib, sc) = (biblio.inner().clone(), scripts.inner().clone());
    biblio.lancer_scan_auto(Arc::new(move |evenement| {
        let (nom, charge) = evenement.pour_le_front();
        let _ = app.emit(nom, charge);
        if let EvenementScan::Termine { ajoutes, .. } = &evenement {
            let ids = bib.prendre_ajoutes();
            if *ajoutes > 0 && !ids.is_empty() {
                sc.declencher(Hook::NewBooks, ids);
            }
        }
    }))
}

/// Réglages « Analyse et suivi » (valeurs par défaut appliquées).
#[tauri::command]
pub fn lire_reglages_analyse(biblio: tauri::State<'_, Biblio>) ->Result<ReglagesAnalyseComplets, String> {
    reglages::lire_reglages_analyse(biblio.data())
}

/// Enregistre (fusionne) les réglages « Analyse et suivi » ; renvoie l'ensemble à jour.
#[tauri::command]
pub fn enregistrer_reglages_analyse(
    biblio: tauri::State<'_, Biblio>,
    reglages: ReglagesAnalyse,
) -> Result<ReglagesAnalyseComplets, String> {
    reglages::enregistrer_reglages_analyse(biblio.data(), reglages)
}
