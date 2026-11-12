//! Application Tauri : fenêtre et commandes IPC. Les commandes sont **fines** : elles
//! appellent `comicrust-core` (ou les modules portables de ce crate) et renvoient du JSON ;
//! aucune règle métier côté front.

use crate::biblio::{
    self, Biblio, ChoixPremierDemarrage, EtatBiblio, LivreDetail, PageLivres, PagePiles, RequeteLivres,
};
use crate::edition::{
    self, BilanOperation, ChampFiche, ChampModifie, DemandeModification, DemandeRenommage, Edition, ErreurChamp,
    EvenementEdition, FicheEdition, ModeleNom, Proposition, ResultatRenommage,
};
use crate::lecteur::{self, Lecteur, LivreOuvert, ReponsePage};
use crate::listes::{
    ArbreListes, Combinaison, DemandeCreation, Destination, EtatListes, GroupeVue, InfosConversion, InfosSuppression,
    PlaceDepot, ResultatListes,
};
use crate::listes_cbl::{ExportCbl, ImportCbl, RapportCbl};
use crate::listes_criteres::{self, Apercu, CatalogueCriteres, Critere, CriteresListe, EditeurListe};
use crate::maj::{self, EtatMaj, Maj};
use crate::pages_livre::{OperationPages, PageLivre, PagesLivre};
use crate::reglages::{self, PreferencesVue, ReglagesEdition, ReglagesEditionComplets};
use crate::surveillance::{self, EtatBase, Surveillance};
use crate::transfert::{ApercuTransfert, DemandeTransfert, ResumeTransfert, Transfert};
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

/// Teste une connexion MySQL saisie (mot de passe jamais renvoyé ni inclus dans les erreurs).
#[tauri::command(async)]
fn test_connexion_mysql(
    data: tauri::State<'_, DossierData>,
    cible: crate::connexion::CibleTest,
    parametres: crate::connexion::ParametresMysql,
) -> Result<crate::connexion::ResultatTest, String> {
    crate::connexion::tester(&data.0, cible, &parametres)
}

/// Connexions MySQL enregistrées, sans mot de passe (« défini oui / non » seulement).
#[tauri::command(async)]
fn lire_connexions_mysql(data: tauri::State<'_, DossierData>) -> Result<crate::connexion::ConnexionsMysql, String> {
    crate::connexion::lire_connexions(&data.0)
}

/// Premier démarrage avec MySQL : base de ComicRust (SQLite ou MySQL) ou base ComicRack saisie.
#[tauri::command(async)]
fn configurer_mysql(
    biblio: tauri::State<'_, Biblio>,
    choix: crate::connexion::ChoixMysql,
) -> Result<EtatBiblio, String> {
    biblio.configurer_mysql(&choix)
}

/// Une fenêtre de la liste (recherche, tri et regroupement faits ici).
#[tauri::command(async)]
fn lister_livres(biblio: tauri::State<'_, Biblio>, requete: RequeteLivres) -> Result<PageLivres, String> {
    biblio.lister(&requete)
}

/// Piles de la vue empilée par série (même requête que `lister_livres`).
#[tauri::command(async)]
fn lister_piles(biblio: tauri::State<'_, Biblio>, requete: RequeteLivres) -> Result<PagePiles, String> {
    biblio.lister_piles(&requete)
}

/// Fiche complète d'un livre.
#[tauri::command(async)]
fn detail_livre(
    biblio: tauri::State<'_, Biblio>,
    lecteur: tauri::State<'_, Lecteur>,
    id: String,
) -> Result<LivreDetail, String> {
    let mut d = biblio.detail(&id)?;
    // La lecture faite dans ComicRust prime quand elle est plus avancée (parité avec egui).
    lecteur.completer_detail(&mut d);
    Ok(d)
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

// ───────────────────────────── Surveillance de la base ─────────────────────────────
// Contrat : docs/tauri/ipc-surveillance.md. Le fil de sondage est démarré dans `setup`.

/// État de la connexion à la base (en ligne / hors ligne), pour l'affichage initial.
#[tauri::command]
fn etat_base(biblio: tauri::State<'_, Biblio>) -> EtatBase {
    biblio.etat_base()
}

/// Bouton « Réessayer » : sondage immédiat de la base ; renvoie l'état à jour.
#[tauri::command(async)]
fn reessayer_base(surveillance: tauri::State<'_, Surveillance>) -> EtatBase {
    surveillance.reessayer()
}

// ───────────────────────────── Listes ComicRack ─────────────────────────────
// Contrat : docs/tauri/ipc-listes.md. Une commande = un appel à `listes.rs` (logique dupliquée
// de la version egui) ; les modifications passent par `comicrust_core::lists_edit`.

/// État des listes (source, lecture seule, ComicRack lancé, version).
#[tauri::command(async)]
fn etat_listes(biblio: tauri::State<'_, Biblio>) -> EtatListes {
    biblio.etat_listes()
}

/// Arbre des listes avec leurs comptes.
#[tauri::command(async)]
fn arbre_listes(biblio: tauri::State<'_, Biblio>) -> Result<ArbreListes, String> {
    biblio.arbre_listes()
}

/// Bilan texte ComicRust / ComicRack de chaque liste.
#[tauri::command(async)]
fn bilan_listes(biblio: tauri::State<'_, Biblio>) -> Result<String, String> {
    biblio.bilan_listes()
}

#[tauri::command(async)]
fn creer_liste(biblio: tauri::State<'_, Biblio>, demande: DemandeCreation) -> Result<ResultatListes, String> {
    biblio.creer_liste(&demande)
}

#[tauri::command(async)]
fn renommer_liste(biblio: tauri::State<'_, Biblio>, id: String, nom: String) -> Result<ResultatListes, String> {
    biblio.renommer_liste(&id, &nom)
}

#[tauri::command(async)]
fn modifier_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    nom: Option<String>,
    description: Option<String>,
    combinaison: Option<Combinaison>,
) -> Result<ResultatListes, String> {
    biblio.modifier_liste(&id, nom, description, combinaison)
}

#[tauri::command(async)]
fn dupliquer_liste(biblio: tauri::State<'_, Biblio>, id: String) -> Result<ResultatListes, String> {
    biblio.dupliquer_liste(&id)
}

#[tauri::command(async)]
fn infos_suppression(biblio: tauri::State<'_, Biblio>, id: String) -> Result<InfosSuppression, String> {
    biblio.infos_suppression(&id)
}

#[tauri::command(async)]
fn supprimer_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    liste_affichee: Option<String>,
) -> Result<ResultatListes, String> {
    biblio.supprimer_liste(&id, liste_affichee.as_deref())
}

#[tauri::command(async)]
fn destinations_liste(biblio: tauri::State<'_, Biblio>, id: String) -> Result<Vec<Destination>, String> {
    biblio.destinations_liste(&id)
}

#[tauri::command(async)]
fn deplacer_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    parent: Option<String>,
    position: Option<usize>,
) -> Result<ResultatListes, String> {
    biblio.deplacer_liste(&id, parent, position)
}

#[tauri::command(async)]
fn decaler_liste(biblio: tauri::State<'_, Biblio>, id: String, haut: bool) -> Result<ResultatListes, String> {
    biblio.decaler_liste(&id, haut)
}

#[tauri::command(async)]
fn place_depot_liste(biblio: tauri::State<'_, Biblio>, glissee: String, cible: String, place: PlaceDepot) -> bool {
    biblio.place_depot_liste(&glissee, &cible, place)
}

#[tauri::command(async)]
fn deposer_liste(
    biblio: tauri::State<'_, Biblio>,
    glissee: String,
    cible: String,
    place: PlaceDepot,
) -> Result<ResultatListes, String> {
    biblio.deposer_liste(&glissee, &cible, place)
}

#[tauri::command(async)]
fn infos_conversion(biblio: tauri::State<'_, Biblio>, id: String) -> Result<InfosConversion, String> {
    biblio.infos_conversion(&id)
}

#[tauri::command(async)]
fn convertir_en_lecture(biblio: tauri::State<'_, Biblio>, id: String) -> Result<ResultatListes, String> {
    biblio.convertir_en_lecture(&id)
}

#[tauri::command(async)]
fn ajouter_livres_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    livres: Vec<String>,
    position: Option<usize>,
) -> Result<ResultatListes, String> {
    biblio.ajouter_livres_liste(&id, livres, position)
}

#[tauri::command(async)]
fn depot_livres_possible(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    liste_affichee: Option<String>,
) -> Result<Option<String>, String> {
    biblio.depot_livres_possible(&id, liste_affichee.as_deref())
}

#[tauri::command(async)]
fn retirer_livres_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    livres: Vec<String>,
) -> Result<ResultatListes, String> {
    biblio.retirer_livres_liste(&id, livres)
}

#[tauri::command(async)]
fn decaler_livres_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    livres: Vec<String>,
    haut: bool,
) -> Result<ResultatListes, String> {
    biblio.decaler_livres_liste(&id, livres, haut)
}

#[tauri::command(async)]
fn deplacer_livres_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    livres: Vec<String>,
    requete: RequeteLivres,
    insertion: usize,
) -> Result<ResultatListes, String> {
    biblio.deplacer_livres_liste(&id, livres, &requete, insertion)
}

#[tauri::command(async)]
fn deplacer_groupes_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    requete: RequeteLivres,
    groupes: Vec<String>,
    insertion: usize,
) -> Result<ResultatListes, String> {
    biblio.deplacer_groupes_liste(&id, &requete, &groupes, insertion)
}

#[tauri::command(async)]
fn decaler_groupe_liste(
    biblio: tauri::State<'_, Biblio>,
    id: String,
    requete: RequeteLivres,
    groupe: String,
    haut: bool,
) -> Result<ResultatListes, String> {
    biblio.decaler_groupe_liste(&id, &requete, &groupe, haut)
}

#[tauri::command(async)]
fn groupes_vue(biblio: tauri::State<'_, Biblio>, requete: RequeteLivres) -> Result<Vec<GroupeVue>, String> {
    biblio.groupes_vue(&requete)
}

#[tauri::command(async)]
fn catalogue_criteres() -> CatalogueCriteres {
    listes_criteres::catalogue_criteres()
}

#[tauri::command(async)]
fn nouveau_critere(type_critere: String, ancien: Option<Critere>) -> Result<Critere, String> {
    listes_criteres::nouveau_critere(&type_critere, ancien)
}

#[tauri::command(async)]
fn lire_criteres(biblio: tauri::State<'_, Biblio>, id: Option<String>) -> Result<EditeurListe, String> {
    biblio.lire_criteres(id.as_deref())
}

#[tauri::command(async)]
fn apercu_criteres(
    biblio: tauri::State<'_, Biblio>,
    id: Option<String>,
    criteres: CriteresListe,
) -> Result<Apercu, String> {
    biblio.apercu_criteres(id.as_deref(), &criteres)
}

#[tauri::command(async)]
fn enregistrer_criteres(
    biblio: tauri::State<'_, Biblio>,
    id: Option<String>,
    criteres: CriteresListe,
    parent: Option<String>,
    position: Option<usize>,
    apres_liste: Option<String>,
) -> Result<ResultatListes, String> {
    biblio.enregistrer_criteres(id.as_deref(), &criteres, parent, position, apres_liste.as_deref())
}

#[tauri::command(async)]
fn analyser_cbl(biblio: tauri::State<'_, Biblio>, chemin: String) -> Result<RapportCbl, String> {
    biblio.analyser_cbl(&chemin)
}

#[tauri::command(async)]
fn importer_cbl(
    biblio: tauri::State<'_, Biblio>,
    chemin: String,
    nom: Option<String>,
    parent: Option<String>,
    position: Option<usize>,
    apres_liste: Option<String>,
) -> Result<ImportCbl, String> {
    biblio.importer_cbl(&chemin, nom, parent, position, apres_liste.as_deref())
}

#[tauri::command(async)]
fn exporter_cbl(biblio: tauri::State<'_, Biblio>, id: String, chemin: String) -> Result<ExportCbl, String> {
    biblio.exporter_cbl(&id, &chemin)
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

// ───────────────────────────── Édition ─────────────────────────────
// Contrat : docs/tauri/ipc-edition.md. Les lots s'exécutent hors du fil principal (commandes
// `async`) ; la progression et les changements arrivent par événements.

/// Sortie des événements de l'édition vers le front.
fn sortie_edition(app: tauri::AppHandle) -> impl Fn(EvenementEdition) + Send + Sync {
    move |e| {
        let (nom, charge) = e.pour_le_front();
        let _ = app.emit(nom, charge);
    }
}

/// Réglages de l'édition (section `edition` de `data\comicrust-tauri.json`).
#[tauri::command(async)]
fn lire_reglages_edition(data: tauri::State<'_, DossierData>) -> Result<ReglagesEditionComplets, String> {
    reglages::lire_reglages_edition(&data.0)
}

/// Fusionne les champs fournis dans les réglages de l'édition.
#[tauri::command(async)]
fn enregistrer_reglages_edition(
    data: tauri::State<'_, DossierData>,
    reglages: ReglagesEdition,
) -> Result<ReglagesEditionComplets, String> {
    reglages::enregistrer_reglages_edition(&data.0, reglages)
}

/// Fiche(s) à modifier (valeurs communes / multiples, onglets, lecture seule).
#[tauri::command(async)]
fn lire_fiches(edition: tauri::State<'_, Edition>, ids: Vec<String>) -> Result<FicheEdition, String> {
    edition.lire_fiches(&ids)
}

/// Valeurs de la bibliothèque proposées pendant la saisie.
#[tauri::command(async)]
fn proposer_valeurs(
    edition: tauri::State<'_, Edition>,
    champ: String,
    texte: String,
    tout: Option<bool>,
    max: Option<usize>,
) -> Result<Vec<Proposition>, String> {
    edition.proposer_valeurs(&champ, &texte, tout.unwrap_or(false), max)
}

/// Erreurs des valeurs saisies (vide : tout est accepté).
#[tauri::command]
fn verifier_champs(champs: Vec<ChampModifie>) -> Vec<ErreurChamp> {
    edition::verifier_champs(&champs)
}

/// Nouvelle valeur personnalisée (vide).
#[tauri::command]
fn ajouter_valeur_perso(cle: String, existantes: Vec<String>) -> Result<ChampFiche, String> {
    edition::nouvelle_valeur_perso(&cle, &existantes)
}

/// Enregistre les champs modifiés (et les infos dans les fichiers si demandé).
#[tauri::command(async)]
fn appliquer_modifications(
    app: tauri::AppHandle,
    edition: tauri::State<'_, Edition>,
    demande: DemandeModification,
) -> Result<BilanOperation, String> {
    edition.appliquer_modifications(&demande, &sortie_edition(app))
}

/// Onglet « Pages » : pages du livre et séance de vignettes (`docs/tauri/ipc-pages.md`).
#[tauri::command(async)]
fn lire_pages(edition: tauri::State<'_, Edition>, id: String, remplace: Option<String>) -> Result<PagesLivre, String> {
    edition.lire_pages(&id, remplace.as_deref())
}

/// Ferme une séance de vignettes de l'onglet « Pages ».
#[tauri::command(async)]
fn fermer_pages(edition: tauri::State<'_, Edition>, jeton: String) {
    edition.fermer_pages(&jeton)
}

/// Opération de l'onglet « Pages » (rien n'est enregistré) : nouvelle liste.
#[tauri::command(async)]
fn transformer_pages(pages: Vec<PageLivre>, operation: OperationPages) -> Result<Vec<PageLivre>, String> {
    crate::pages_livre::transformer(&pages, &operation)
}

/// Écrit les infos dans le ComicInfo.xml des CBZ.
#[tauri::command(async)]
fn ecrire_infos_fichiers(
    app: tauri::AppHandle,
    edition: tauri::State<'_, Edition>,
    ids: Vec<String>,
    confirme: Option<bool>,
) -> Result<BilanOperation, String> {
    edition.ecrire_infos_fichiers(&ids, confirme.unwrap_or(false), &sortie_edition(app))
}

/// Convertit les CBR / CB7 / CBT en CBZ.
#[tauri::command(async)]
fn convertir_cbr_cbz(
    app: tauri::AppHandle,
    edition: tauri::State<'_, Edition>,
    ids: Vec<String>,
    ranger_original: Option<bool>,
    confirme: Option<bool>,
) -> Result<BilanOperation, String> {
    edition.convertir_cbr_cbz(&ids, ranger_original, confirme.unwrap_or(false), &sortie_edition(app))
}

/// Modèles de nom proposés.
#[tauri::command]
fn modeles_de_nom() -> Vec<ModeleNom> {
    edition::modeles_de_nom()
}

/// Renomme / déplace d'après un modèle (aperçu d'abord).
#[tauri::command(async)]
fn renommer_deplacer(
    app: tauri::AppHandle,
    edition: tauri::State<'_, Edition>,
    demande: DemandeRenommage,
) -> Result<ResultatRenommage, String> {
    edition.renommer_deplacer(&demande, &sortie_edition(app))
}

/// Montre le fichier du livre dans son dossier (Explorateur de Windows, fichier sélectionné).
#[tauri::command(async)]
fn ouvrir_dossier_fichier(edition: tauri::State<'_, Edition>, id: String) -> Result<(), String> {
    edition.ouvrir_dossier_fichier(&id)
}

/// Demande l'arrêt du lot en cours (entre deux livres).
#[tauri::command]
fn annuler_operation(edition: tauri::State<'_, Edition>) {
    edition.annuler();
}

// ───────────────────────────── Transfert ComicRack → ComicRust ─────────────────────────────
// Contrat : docs/tauri/ipc-transfert.md (module `transfert`). Le mot de passe MySQL ne passe jamais ici.

/// Ce que le transfert propose (source MySQL sans mot de passe, ComicDb.xml, destination).
#[tauri::command(async)]
fn apercu_transfert(transfert: tauri::State<'_, Transfert>) -> ApercuTransfert {
    transfert.apercu()
}

/// Lance le transfert et rend le bilan à la fin ; progression par l'événement
/// `transfert-progression`.
#[tauri::command(async)]
fn lancer_transfert(
    app: tauri::AppHandle,
    transfert: tauri::State<'_, Transfert>,
    demande: DemandeTransfert,
) -> Result<ResumeTransfert, String> {
    transfert.lancer(&demande, &move |e| {
        let (nom, charge) = e.pour_le_front();
        let _ = app.emit(nom, charge);
    })
}

/// Arrête le transfert en cours (entre deux lots de livres).
#[tauri::command]
fn annuler_transfert(transfert: tauri::State<'_, Transfert>) {
    transfert.annuler();
}

// ───────────────────────────── Mise à jour ─────────────────────────────
// Contrat : docs/tauri/ipc-maj.md (module `maj`). Vérification et installation dans des fils à
// part ; résultat et progression par les événements `maj://statut` et `maj://progression`.

/// Version, réglages, statut courant, installation possible ou non.
#[tauri::command]
fn maj_etat(maj: tauri::State<'_, Maj>) -> EtatMaj {
    maj.etat()
}

/// Lance la vérification en arrière-plan (`manuel` : afficher aussi « à jour » et les erreurs).
#[tauri::command]
fn maj_verifier(maj: tauri::State<'_, Maj>, manuel: Option<bool>) {
    maj.verifier(manuel.unwrap_or(true));
}

/// Télécharge, vérifie (SHA-256) et installe la release `tag` de la dernière vérification.
#[tauri::command]
fn maj_installer(maj: tauri::State<'_, Maj>, tag: String) -> Result<(), String> {
    maj.installer(&tag)
}

/// Relance le même exe avec les mêmes arguments puis ferme l'application (la lecture des livres
/// ouverts est enregistrée par `RunEvent::Exit`).
#[tauri::command]
fn maj_redemarrer(app: tauri::AppHandle, maj: tauri::State<'_, Maj>) -> Result<(), String> {
    maj.redemarrer()?;
    app.exit(0);
    Ok(())
}

/// Ouvre une page de `https://github.com/imtheyoyo/ComicRust/` dans le navigateur par défaut
/// (page de la release, liens des notes de version) ; toute autre adresse est refusée.
#[tauri::command]
fn maj_ouvrir_page(url: String) -> Result<(), String> {
    maj::ouvrir_page(&url)
}

/// Enregistre `verifierAuDemarrage` et/ou `versionIgnoree` (champ absent = inchangé, `null` efface
/// la version ignorée) : arguments lus dans le JSON brut de l'appel pour distinguer absent et `null`.
#[tauri::command]
fn maj_regler(maj: tauri::State<'_, Maj>, request: tauri::ipc::Request<'_>) -> Result<EtatMaj, String> {
    let demande = match request.body() {
        tauri::ipc::InvokeBody::Json(v) => maj::analyser_demande_reglage(v)?,
        tauri::ipc::InvokeBody::Raw(_) => return Err("maj_regler : arguments JSON attendus".into()),
    };
    maj.regler(demande)
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
    // Relance après une mise à jour (`--attendre-pid <pid>`) : attendre (10 s au plus) que
    // l'ancienne instance ait libéré le profil WebView2 et le port MCP. Tout au début.
    maj::traiter_attendre_pid();
    // `--mcp-stdio` : passerelle pour les clients MCP qui lancent un programme (Claude Desktop).
    #[cfg(feature = "mcp")]
    if crate::mcp::passerelle_stdio() {
        return;
    }
    // Portable : `data\` à côté de l'exécutable. Sans dossier inscriptible, on s'arrête avec
    // un message (aucun repli sur AppData, comme la version egui).
    let data = match portable::data_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Le dossier de ComicRust n'est pas modifiable ({e}). ComicRust est portable : déplacez-le dans un dossier accessible en écriture.");
            std::process::exit(1);
        }
    };
    // Mise à jour précédente : suppression des `.old` notés dans `data\maj\anciens.txt` et des
    // téléchargements, avant la création de la fenêtre.
    let dossier_app = portable::exe_dir();
    maj::cleanup(&dossier_app);
    // Profil WebView2 dans `data\webview2` : à positionner AVANT la création de la fenêtre,
    // sinon WebView2 écrit dans %LOCALAPPDATA%. Sans effet hors Windows.
    let webview = portable::dossier_webview(&data);
    let _ = std::fs::create_dir_all(&webview);
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview);

    let bibliotheque = Biblio::nouvelle(data.clone());
    let lecteur = Lecteur::nouveau(data.clone(), bibliotheque.clone());
    tauri::Builder::default()
        .manage(Edition::nouvelle(bibliotheque.clone(), lecteur.clone()))
        .manage(Transfert::nouveau(bibliotheque.clone()))
        .manage(lecteur)
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
            etat_base,
            reessayer_base,
            test_connexion_mysql,
            lire_connexions_mysql,
            configurer_mysql,
            lister_livres,
            lister_piles,
            detail_livre,
            lister_dossiers,
            ajouter_dossier,
            retirer_dossier,
            lancer_scan,
            ouvrir_livre,
            ouvrir_fichier,
            url_page_base,
            enregistrer_avancement,
            fermer_livre,
            lire_reglages_edition,
            enregistrer_reglages_edition,
            lire_fiches,
            proposer_valeurs,
            verifier_champs,
            ajouter_valeur_perso,
            appliquer_modifications,
            lire_pages,
            fermer_pages,
            transformer_pages,
            ecrire_infos_fichiers,
            convertir_cbr_cbz,
            modeles_de_nom,
            renommer_deplacer,
            annuler_operation,
            ouvrir_dossier_fichier,
            etat_listes,
            arbre_listes,
            bilan_listes,
            creer_liste,
            renommer_liste,
            modifier_liste,
            dupliquer_liste,
            infos_suppression,
            supprimer_liste,
            destinations_liste,
            deplacer_liste,
            decaler_liste,
            place_depot_liste,
            deposer_liste,
            infos_conversion,
            convertir_en_lecture,
            ajouter_livres_liste,
            depot_livres_possible,
            retirer_livres_liste,
            decaler_livres_liste,
            deplacer_livres_liste,
            deplacer_groupes_liste,
            decaler_groupe_liste,
            groupes_vue,
            catalogue_criteres,
            nouveau_critere,
            lire_criteres,
            apercu_criteres,
            enregistrer_criteres,
            analyser_cbl,
            importer_cbl,
            exporter_cbl,
            apercu_transfert,
            lancer_transfert,
            annuler_transfert,
            maj_etat,
            maj_verifier,
            maj_installer,
            maj_redemarrer,
            maj_regler,
            maj_ouvrir_page
        ])
        .setup(move |app| {
            // Thèmes livrés disponibles dès le premier lancement.
            let data = app.state::<DossierData>();
            let _ = themes::installer_themes_livres(&portable::dossier_themes(&data.0));
            // Mise à jour : état géré avant la fenêtre (les commandes `maj_*` le trouvent dès le
            // chargement de la page) ; événements `maj://statut` et `maj://progression`.
            let emetteur_maj = app.handle().clone();
            app.manage(Maj::nouveau(
                data.0.clone(),
                dossier_app,
                maj::installable(),
                Arc::new(move |evenement| {
                    let (nom, charge) = evenement.pour_le_front();
                    let _ = emetteur_maj.emit(nom, charge);
                }),
            ));
            // Fenêtre créée ici (et non dans tauri.conf.json) pour fixer le profil WebView2
            // dans `data\webview2` (chemin absolu) : cache, cookies et stockage local restent
            // dans le dossier portable. Titre et tailles : celles de l'ancienne configuration.
            let profil = portable::dossier_webview(&data.0);
            let fenetre = tauri::WebviewWindowBuilder::new(&*app, "main", tauri::WebviewUrl::default())
                .title("ComicRust")
                .inner_size(1280.0, 800.0)
                .min_inner_size(800.0, 500.0)
                .data_directory(profil);
            // Pilotage MCP : script de la page (journal de la console dès le chargement).
            #[cfg(feature = "mcp")]
            let fenetre = if crate::mcp::actif() { fenetre.initialization_script(crate::mcp::SCRIPT) } else { fenetre };
            fenetre.build()?;
            // Surveillance de la base : modifications extérieures et état en ligne / hors ligne.
            let emetteur = app.handle().clone();
            app.manage(Surveillance::demarrer(
                app.state::<Biblio>().inner().clone(),
                surveillance::PERIODE_SONDAGE,
                Arc::new(move |evenement| {
                    let (nom, charge) = evenement.pour_le_front();
                    let _ = emetteur.emit(nom, charge);
                }),
            ));
            #[cfg(feature = "mcp")]
            crate::mcp::demarrer(app.handle());
            // Vérification automatique au démarrage (réglage `verifierAuDemarrage`, vrai par
            // défaut), dans un fil à part : ne bloque jamais l'ouverture.
            app.state::<Maj>().verifier_au_demarrage();
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
