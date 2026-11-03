//! Application Tauri : fenêtre et commandes IPC. Les commandes sont **fines** : elles
//! appellent `comicrust-core` (ou les modules portables de ce crate) et renvoient du JSON ;
//! aucune règle métier côté front.

use crate::biblio::{self, Biblio, ChoixPremierDemarrage, EtatBiblio, LivreDetail, PageLivres, RequeteLivres};
use crate::lecteur::{self, Lecteur, LivreOuvert, ReponsePage};
use crate::reglages::{self, PreferencesVue};
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

/// Préférences d'affichage mémorisées dans `data\comicrust-tauri.json` (section `vue`).
#[tauri::command(async)]
fn lire_preferences_vue(data: tauri::State<'_, DossierData>) -> Result<PreferencesVue, String> {
    reglages::lire_preferences_vue(&data.0)
}

/// Fusionne les champs fournis dans la section `vue` (écriture atomique) ; renvoie l'ensemble à
/// jour. Les valeurs inconnues sont ignorées.
#[tauri::command(async)]
fn enregistrer_preferences_vue(
    data: tauri::State<'_, DossierData>,
    preferences: PreferencesVue,
) -> Result<PreferencesVue, String> {
    reglages::enregistrer_preferences_vue(&data.0, preferences)
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

// ───────────────────────────── Lecteur ─────────────────────────────
// Contrat : docs/tauri/ipc-lecteur.md. Pages servies par le protocole `cr-page`.

/// Ouvre un livre de la bibliothèque (ordre des pages de sa fiche, page de reprise).
#[tauri::command(async)]
fn ouvrir_livre(lecteur: tauri::State<'_, Lecteur>, id: String) -> Result<LivreOuvert, String> {
    lecteur.ouvrir_livre(&id)
}

/// Ouvre un fichier ou un dossier d'images hors bibliothèque.
#[tauri::command(async)]
fn ouvrir_fichier(lecteur: tauri::State<'_, Lecteur>, chemin: String) -> Result<LivreOuvert, String> {
    lecteur.ouvrir_fichier(&chemin)
}

/// Préfixe des URL de pages d'un livre ouvert (`urlBase` de `LivreOuvert`).
#[tauri::command]
fn url_page_base(lecteur: tauri::State<'_, Lecteur>, jeton: String) -> Result<String, String> {
    lecteur.url_page_base(&jeton)
}

/// Note la page courante ; `termine` : livre lu jusqu'au bout.
#[tauri::command(async)]
fn enregistrer_avancement(
    lecteur: tauri::State<'_, Lecteur>,
    jeton: String,
    page: usize,
    termine: bool,
) -> Result<(), String> {
    lecteur.enregistrer_avancement(&jeton, page, termine)
}

/// Ferme un livre (lecture écrite dans la base en mode autonome).
#[tauri::command(async)]
fn fermer_livre(lecteur: tauri::State<'_, Lecteur>, jeton: String) -> Result<(), String> {
    lecteur.fermer_livre(&jeton)
}

/// Réponse HTTP du protocole `cr-page`.
fn reponse_page(p: ReponsePage) -> tauri::http::Response<Vec<u8>> {
    // Une URL de page ne vaut que pour une séance (jeton) : gardée par la WebView tant que
    // la séance dure ; les erreurs (404, 504…) ne sont jamais gardées.
    let cache = if p.statut == 200 { "private, max-age=3600" } else { "no-store" };
    tauri::http::Response::builder()
        .status(p.statut)
        .header("Cache-Control", cache)
        .header("Content-Type", p.mime)
        .header("Access-Control-Allow-Origin", "*")
        .body(p.octets)
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

/// Réponse HTTP du protocole `cr-couv`.
fn reponse_couverture(statut: u16, jpeg: Vec<u8>) -> tauri::http::Response<Vec<u8>> {
    // 200 : couverture gardée par la WebView une heure ; 404 : jamais gardée.
    let cache = if statut == 200 { "private, max-age=3600" } else { "no-store" };
    tauri::http::Response::builder()
        .status(statut)
        .header("Cache-Control", cache)
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

    let bibliotheque = Biblio::nouvelle(data.clone());
    tauri::Builder::default()
        .manage(Lecteur::nouveau(data.clone(), bibliotheque.clone()))
        .manage(bibliotheque)
        .manage(DossierData(data))
        // Couvertures : `cr-couv://localhost/<id>` (Windows : `http://cr-couv.localhost/<id>`), servies
        // depuis le cache `data\couvertures` par les fils de `Biblio::pool` (jamais le fil principal :
        // la première lecture d'une archive sur le NAS peut durer). 404 si illisible.
        .register_asynchronous_uri_scheme_protocol(biblio::PROTOCOLE_COUVERTURES, |ctx, requete, reponse| {
            let bib = ctx.app_handle().state::<Biblio>().inner().clone();
            let id = biblio::id_depuis_uri(requete.uri().path(), requete.uri().host());
            bib.servir(&id, move |jpeg| {
                reponse.respond(match jpeg {
                    Some(jpeg) => reponse_couverture(200, jpeg),
                    None => reponse_couverture(404, Vec::new()),
                });
            });
        })
        // Pages du lecteur : `cr-page://localhost/<jeton>/<index>?l=<largeur>` (Windows :
        // `http://cr-page.localhost/...`), produites par le fil du livre (jamais le fil
        // principal) ; 404 si jeton ou page inconnus, 504 après le délai (NAS muet).
        .register_asynchronous_uri_scheme_protocol(lecteur::PROTOCOLE_PAGES, |ctx, requete, reponse| {
            let lect = ctx.app_handle().state::<Lecteur>().inner().clone();
            let uri = requete.uri();
            match lecteur::analyser_uri(uri.path(), uri.query()) {
                Some((jeton, index, largeur)) => {
                    lect.servir(&jeton, index, largeur, move |p| reponse.respond(reponse_page(p)))
                }
                None => reponse.respond(reponse_page(ReponsePage {
                    statut: 404,
                    mime: "text/plain; charset=utf-8",
                    octets: b"adresse de page invalide".to_vec(),
                })),
            }
        })
        .invoke_handler(tauri::generate_handler![
            infos,
            lister_themes,
            charger_theme,
            lire_preferences_vue,
            enregistrer_preferences_vue,
            ouvrir_bibliotheque,
            configurer_premier_demarrage,
            lister_livres,
            detail_livre,
            lister_dossiers,
            ajouter_dossier,
            retirer_dossier,
            lancer_scan,
            ouvrir_livre,
            ouvrir_fichier,
            url_page_base,
            enregistrer_avancement,
            fermer_livre
        ])
        .setup(|app| {
            // Thèmes livrés disponibles dès le premier lancement.
            let data = app.state::<DossierData>();
            let _ = themes::installer_themes_livres(&portable::dossier_themes(&data.0));
            // Fenêtre créée ici (et non dans tauri.conf.json) pour fixer le profil WebView2
            // dans `data\webview2` (chemin absolu) : cache, cookies et stockage local restent
            // dans le dossier portable. Titre et tailles : celles de l'ancienne configuration.
            let profil = portable::dossier_webview(&data.0);
            tauri::WebviewWindowBuilder::new(&*app, "main", tauri::WebviewUrl::default())
                .title("ComicRust")
                .inner_size(1280.0, 800.0)
                .min_inner_size(800.0, 500.0)
                .data_directory(profil)
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erreur au lancement de ComicRust-Tauri")
        .run(|app, evenement| {
            // Fermeture de l'application : la lecture des livres ouverts est enregistrée
            // (comme la version egui à la fermeture de la fenêtre).
            if let tauri::RunEvent::Exit = evenement {
                app.state::<Lecteur>().fermer_tout();
            }
        });
}
