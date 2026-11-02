//! Application Tauri : fenêtre et commandes IPC. Les commandes sont **fines** : elles
//! appellent `comicrust-core` (ou les modules portables de ce crate) et renvoient du JSON ;
//! aucune règle métier côté front.

use crate::{infos_application, portable, themes, InfosApplication};
use std::path::PathBuf;
use tauri::Manager;

/// Dossier `data\` portable, résolu une fois au démarrage et gardé dans l'état Tauri.
struct DossierData(PathBuf);

/// Version, dossier `data\` portable et dossiers dérivés.
#[tauri::command]
fn infos(data: tauri::State<'_, DossierData>) -> InfosApplication {
    infos_application(&data.0)
}

/// Thèmes disponibles dans `data\themes` (les thèmes livrés manquants sont recopiés d'abord,
/// pour qu'un fichier supprimé par erreur revienne).
#[tauri::command]
fn lister_themes(data: tauri::State<'_, DossierData>) -> Result<Vec<themes::Theme>, String> {
    let dossier = portable::dossier_themes(&data.0);
    themes::installer_themes_livres(&dossier).map_err(|e| e.to_string())?;
    themes::lister_themes(&dossier).map_err(|e| e.to_string())
}

/// Contenu CSS d'un thème (le front l'injecte dans la page).
#[tauri::command]
fn charger_theme(data: tauri::State<'_, DossierData>, id: String) -> Result<String, String> {
    themes::charger_theme(&portable::dossier_themes(&data.0), &id).map_err(|e| e.to_string())
}

/// Lance l'application.
pub fn run() {
    // Portable : `data\` à côté de l'exécutable. Sans dossier inscriptible, on s'arrête avec
    // un message (aucun repli sur AppData, comme la version egui).
    let data = match portable::data_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Le dossier de ComicRust n'est pas modifiable ({e}). ComicRust est portable : déplacez-le dans un dossier accessible en écriture.");
            std::process::exit(1);
        }
    };
    // Profil WebView2 dans `data\webview2` : à positionner AVANT la création de la fenêtre,
    // sinon WebView2 écrit dans %LOCALAPPDATA%. Sans effet hors Windows.
    let webview = portable::dossier_webview(&data);
    let _ = std::fs::create_dir_all(&webview);
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview);

    tauri::Builder::default()
        .manage(DossierData(data))
        .invoke_handler(tauri::generate_handler![infos, lister_themes, charger_theme])
        .setup(|app| {
            // Thèmes livrés disponibles dès le premier lancement.
            let data = app.state::<DossierData>();
            let _ = themes::installer_themes_livres(&portable::dossier_themes(&data.0));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de ComicRust-Tauri");
}
