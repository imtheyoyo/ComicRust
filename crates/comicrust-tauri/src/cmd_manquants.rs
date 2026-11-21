//! Commandes IPC des livres manquants (lot L3). Contrat : `docs/tauri/ipc-manquants.md`.
//!
//! Fines comme celles de `app.rs` : elles appellent [`Manquants`] (module `manquants`, où est toute
//! la logique, testée sans Tauri). Les commandes qui parcourent des disques (vérification,
//! recherche, examen d'un fichier, retrait) sont `async` : elles s'exécutent hors du fil
//! principal (un NAS éteint ne fige jamais la fenêtre).

use crate::manquants::{
    BilanOperation, BilanVerification, DemandeRelocalisation, EtatManquants, Manquants, PreparationRetrait, Proposition,
    ResultatRecherche, SondeLivre,
};

/// État léger : nombre de livres manquants, avancement de la vérification ou de la recherche en
/// cours, dernier bilan, raison éventuelle de lecture seule.
#[tauri::command(async)]
pub fn etat_manquants(manquants: tauri::State<'_, Manquants>) -> EtatManquants {
    manquants.etat()
}

/// Vérifie les fichiers de tous les livres (long : avancement par `etat_manquants`).
#[tauri::command(async)]
pub fn verifier_manquants(manquants: tauri::State<'_, Manquants>) -> Result<BilanVerification, String> {
    manquants.verifier()
}

/// Arrête la vérification et la recherche en cours.
#[tauri::command]
pub fn arreter_manquants(manquants: tauri::State<'_, Manquants>) {
    manquants.arreter();
}

/// Examine le fichier d'un livre avant de l'ouvrir (marque le livre manquant si besoin).
#[tauri::command(async)]
pub fn sonder_livre(manquants: tauri::State<'_, Manquants>, id: String) -> Result<SondeLivre, String> {
    manquants.sonder_livre(&id)
}

/// Examine un fichier saisi pour « Retrouver le fichier » : taille, doublon, ressemblance.
#[tauri::command(async)]
pub fn proposer_fichier(
    manquants: tauri::State<'_, Manquants>,
    id: String,
    chemin: String,
) -> Result<Proposition, String> {
    manquants.proposer_fichier(&id, &chemin)
}

/// « Chercher dans un dossier » : propositions pour les livres `ids` (long : arrêt possible).
#[tauri::command(async)]
pub fn chercher_manquants(
    manquants: tauri::State<'_, Manquants>,
    ids: Vec<String>,
    dossier: String,
) -> Result<ResultatRecherche, String> {
    manquants.chercher(&ids, &dossier)
}

/// Met à jour les fiches vers les nouveaux fichiers confirmés.
#[tauri::command(async)]
pub fn appliquer_relocalisations(
    manquants: tauri::State<'_, Manquants>,
    demandes: Vec<DemandeRelocalisation>,
) -> Result<BilanOperation, String> {
    manquants.appliquer_relocalisations(&demandes)
}

/// Fenêtre « Retirer de la bibliothèque » : légendes et vérification des fichiers.
#[tauri::command(async)]
pub fn preparer_retrait(manquants: tauri::State<'_, Manquants>, ids: Vec<String>) -> Result<PreparationRetrait, String> {
    manquants.preparer_retrait(&ids)
}

/// Retire les fiches de la bibliothèque (jamais les fichiers).
#[tauri::command(async)]
pub fn retirer_livres(
    manquants: tauri::State<'_, Manquants>,
    ids: Vec<String>,
    confirme: bool,
) -> Result<BilanOperation, String> {
    manquants.retirer(&ids, confirme)
}
