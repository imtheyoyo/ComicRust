//! Application Tauri : fenêtre et commandes IPC. Les commandes sont **fines** : elles
//! appellent `comicrust-core` (ou les modules portables de ce crate) et renvoient du JSON ;
//! aucune règle métier côté front.

use crate::biblio::{self, Biblio, ChoixPremierDemarrage, EtatBiblio, LivreDetail, PageLivres, RequeteLivres};
use crate::{infos_application, portable, themes, InfosApplication};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};

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

// ───────────────────────────── Bibliothèque ─────────────────────────────
// Contrat : docs/tauri/ipc-bibliotheque.md. Les commandes `async` s'exécutent hors du fil
// principal (ouverture MySQL, lecture de fichiers : ne jamais figer la fenêtre).

/// Ouvre la bibliothèque choisie (ou signale le premier démarrage).
#[tauri::command(async)]
fn ouvrir_bibliotheque(biblio: tauri::State<'_, Biblio>) -> Result<EtatBiblio, String> {
    biblio.ouvrir()
}

/// Enregistre le choix du premier démarrage (autonome ou lié à ComicRack) et ouvre la base.
#[tauri::command(async)]
fn configurer_premier_demarrage(
    biblio: tauri::State<'_, Biblio>,
    choix: ChoixPremierDemarrage,
) -> Result<EtatBiblio, String> {
    biblio.configurer(&choix)
}

/// Une fenêtre de la liste (recherche, tri et regroupement faits ici).
#[tauri::command(async)]
fn lister_livres(biblio: tauri::State<'_, Biblio>, requete: RequeteLivres) -> Result<PageLivres, String> {
    biblio.lister(&requete)
}

/// Fiche complète d'un livre.
#[tauri::command(async)]
fn detail_livre(biblio: tauri::State<'_, Biblio>, id: String) -> Result<LivreDetail, String> {
    biblio.detail(&id)
}

/// Dossiers de BD surveillés.
#[tauri::command(async)]
fn lister_dossiers(biblio: tauri::State<'_, Biblio>) -> Result<Vec<String>, String> {
    biblio.dossiers()
}

/// Ajoute un dossier surveillé ; renvoie la liste à jour.
#[tauri::command(async)]
fn ajouter_dossier(biblio: tauri::State<'_, Biblio>, chemin: String) -> Result<Vec<String>, String> {
    biblio.ajouter_dossier(&chemin)
}

/// Retire un dossier surveillé ; renvoie la liste à jour.
#[tauri::command(async)]
fn retirer_dossier(biblio: tauri::State<'_, Biblio>, chemin: String) -> Result<Vec<String>, String> {
    biblio.retirer_dossier(&chemin)
}

/// Lance l'analyse des dossiers et rend la main ; la suite arrive par les événements
/// `scan-progression`, `scan-termine` et `scan-erreur`.
#[tauri::command(async)]
fn lancer_scan(app: tauri::AppHandle, biblio: tauri::State<'_, Biblio>) -> Result<(), String> {
    biblio.lancer_scan(Arc::new(move |evenement| {
        let (nom, charge) = evenement.pour_le_front();
        let _ = app.emit(nom, charge);
    }))
}

/// Réponse HTTP du protocole `cr-couv`.
fn reponse_couverture(statut: u16, jpeg: Vec<u8>) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(statut)
        .header("Content-Type", "image/jpeg")
        .header("Access-Control-Allow-Origin", "*")
        .body(jpeg)
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
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
        .manage(Biblio::nouvelle(data.clone()))
        .manage(DossierData(data))
        // Couvertures : `cr-couv://localhost/<id>` (Windows : `http://cr-couv.localhost/<id>`), servies
        // depuis le cache `data\couvertures` par les fils de `Biblio::pool` (jamais le fil principal :
        // la première lecture d'une archive sur le NAS peut durer). 404 si illisible.
        .register_asynchronous_uri_scheme_protocol(biblio::PROTOCOLE_COUVERTURES, |ctx, requete, reponse| {
            let bib = ctx.app_handle().state::<Biblio>().inner().clone();
            let id = biblio::id_depuis_uri(requete.uri().path(), requete.uri().host());
            let pour_le_fil = bib.clone();
            bib.pool().executer(move || {
                reponse.respond(match pour_le_fil.couverture(&id) {
                    Ok(jpeg) => reponse_couverture(200, jpeg),
                    Err(_) => reponse_couverture(404, Vec::new()),
                });
            });
        })
        .invoke_handler(tauri::generate_handler![
            infos,
            lister_themes,
            charger_theme,
            ouvrir_bibliotheque,
            configurer_premier_demarrage,
            lister_livres,
            detail_livre,
            lister_dossiers,
            ajouter_dossier,
            retirer_dossier,
            lancer_scan
        ])
        .setup(|app| {
            // Thèmes livrés disponibles dès le premier lancement.
            let data = app.state::<DossierData>();
            let _ = themes::installer_themes_livres(&portable::dossier_themes(&data.0));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de ComicRust-Tauri");
}
