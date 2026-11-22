//! Fenêtre principale et lancement : partie Tauri (fonction `app`). La logique (géométrie,
//! arguments, rapport de plantage) est dans [`crate::fenetre`] et [`crate::plantage`], testée sans
//! Tauri. Contrat IPC : `docs/tauri/ipc-fenetre.md`.
//!
//! - [`EtatFenetre`] : géométrie lue au démarrage, suivi des écritures, chemin passé en argument ;
//! - [`restaurer_et_afficher`] : fenêtre créée masquée, repositionnée puis montrée (pas de saut) ;
//! - [`evenement_fenetre`] : mémorisation au redimensionnement, au déplacement et à la fermeture ;
//! - commandes : titre, lancement initial, nature d'un chemin, rapport de plantage.

use crate::fenetre::{self, Ecran, GenreChemin, Placement, Suivi};
use crate::plantage::{self, RapportPlantage};
use crate::reglages::{self, ReglagesFenetre};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;
use tauri::{Manager, PhysicalPosition, PhysicalSize};

/// État géré par Tauri pour la fenêtre et le lancement.
pub struct EtatFenetre {
    data: PathBuf,
    /// Géométrie lue au démarrage.
    lus: ReglagesFenetre,
    /// Chemin reçu en argument, rendu une seule fois par `lancement_initial`.
    initial: Mutex<Option<PathBuf>>,
    suivi: Mutex<Suivi>,
}

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl EtatFenetre {
    /// `initial` : fichier, dossier ou `.cbl` passé en argument de ligne de commande.
    pub fn nouveau(data: PathBuf, initial: Option<PathBuf>) -> Self {
        // Réglages illisibles : on ouvre par défaut, sans rien écraser (la lecture échouera de
        // même à la sauvegarde et l'erreur est ignorée : les réglages de l'utilisateur sont gardés).
        let lus = reglages::lire_fenetre(&data).unwrap_or_default();
        Self { data, lus, initial: Mutex::new(initial), suivi: Mutex::new(Suivi::nouveau(lus)) }
    }
}

/// Positionne la fenêtre d'après la géométrie mémorisée, puis la montre. La fenêtre est créée
/// masquée (`visible(false)`) : sans cela, on la verrait d'abord à sa place par défaut.
/// Quoi qu'il arrive, la fenêtre est montrée à la fin.
pub fn restaurer_et_afficher(fen: &tauri::WebviewWindow, etat: &EtatFenetre) {
    let ecrans: Vec<Ecran> = fen
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Ecran { x: m.position().x, y: m.position().y, largeur: m.size().width, hauteur: m.size().height })
        .collect();
    if let Some(r) = fenetre::restaurer(&etat.lus, &ecrans) {
        let _ = fen.set_size(PhysicalSize::new(r.largeur, r.hauteur));
        match r.placement {
            Placement::A(x, y) => {
                let _ = fen.set_position(PhysicalPosition::new(x, y));
            }
            Placement::Centrer => {
                let _ = fen.center();
            }
        }
        if r.maximisee {
            let _ = fen.maximize();
        }
    }
    let _ = fen.show();
}

/// À brancher sur `Builder::on_window_event` : mémorise la géométrie de la fenêtre principale.
/// Redimensionnement et déplacement : écriture limitée à une par seconde ; fermeture : écriture
/// finale. Réduite ou en plein écran, la fenêtre ne change pas la géométrie mémorisée.
pub fn evenement_fenetre(fen: &tauri::Window, evenement: &tauri::WindowEvent) {
    use tauri::WindowEvent as E;
    let fin = match evenement {
        E::Resized(_) | E::Moved(_) => false,
        E::CloseRequested { .. } | E::Destroyed => true,
        _ => return,
    };
    if fen.label() != "main" {
        return;
    }
    let Some(etat) = fen.try_state::<EtatFenetre>() else { return };
    let maintenant = Instant::now();
    let a_ecrire = if fin {
        verrou(&etat.suivi).terminer(maintenant)
    } else {
        if fen.is_minimized().unwrap_or(false) || fen.is_fullscreen().unwrap_or(false) {
            return;
        }
        let maximisee = fen.is_maximized().unwrap_or(false);
        let normale = if maximisee {
            None
        } else {
            match (fen.outer_position(), fen.inner_size()) {
                (Ok(p), Ok(t)) => Some(fenetre::Geometrie { x: p.x, y: p.y, largeur: t.width, hauteur: t.height }),
                _ => None,
            }
        };
        verrou(&etat.suivi).observer(normale, maximisee, maintenant)
    };
    if let Some(f) = a_ecrire {
        let _ = reglages::enregistrer_fenetre(&etat.data, f);
    }
}

/// Titre de la fenêtre : `Some(titre)` = « titre — ComicRust 1.0.1 » ; `None` = « ComicRust 1.0.1 »
/// (la version est celle du paquet, ajoutée ici : le front ne la connaît pas).
#[tauri::command]
pub fn definir_titre_fenetre(fen: tauri::WebviewWindow, titre: Option<String>) -> Result<(), String> {
    fen.set_title(&fenetre::titre_fenetre(titre.as_deref())).map_err(|e| e.to_string())
}

/// Chemin passé en argument de ligne de commande, avec sa nature. Rendu **une seule fois**
/// (l'appel suivant, par exemple après un rechargement de la page, rend `null`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LancementInitial {
    pub chemin: String,
    pub genre: GenreChemin,
}

#[tauri::command(async)]
pub fn lancement_initial(etat: tauri::State<'_, EtatFenetre>) -> Option<LancementInitial> {
    let chemin = verrou(&etat.initial).take()?;
    let genre = fenetre::classer_chemin(&chemin);
    Some(LancementInitial { chemin: chemin.display().to_string(), genre })
}

/// Nature d'un chemin déposé sur la fenêtre : `livre`, `dossier`, `liste` ou `absent`.
#[tauri::command(async)]
pub fn classer_chemin(chemin: String) -> GenreChemin {
    fenetre::classer_chemin(Path::new(chemin.trim()))
}

/// Rapport de plantage de la session précédente, s'il y en a un.
#[tauri::command]
pub fn lire_plantage(etat: tauri::State<'_, EtatFenetre>) -> Option<RapportPlantage> {
    plantage::lire(&etat.data)
}

/// Le rapport a été vu : le fichier est supprimé.
#[tauri::command]
pub fn acquitter_plantage(etat: tauri::State<'_, EtatFenetre>) -> Result<(), String> {
    plantage::supprimer(&etat.data)
}
