//! Fichiers **en cours d'écriture** : registre partagé par le lecteur (pages, vignettes) et les
//! couvertures, pour qu'aucune lecture ne tienne un livre ouvert pendant qu'on le remplace.
//!
//! Sous Windows, un fichier ouvert ne peut pas être remplacé ni renommé (« Accès refusé »,
//! os error 5), surtout sur un partage SMB : défaut P-01 de la recette v0.10.7 de la version egui,
//! corrigé là-bas en libérant l'onglet « Pages » avant l'écriture (`PagesTab::release`) puis en le
//! rechargeant (`PagesTab::reload`). Ici, l'écriture prend une [`Reservation`] sur le fichier :
//! tant qu'elle vit, les lectures de ce fichier **attendent** ([`attendre_libre`]) au lieu de
//! l'ouvrir. Le lecteur y ajoute la libération de ses séances (voir `Lecteur::liberer`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Fichiers réservés (clé : chemin en minuscules) → nombre de réservations.
static REGISTRE: LazyLock<(Mutex<HashMap<String, usize>>, Condvar)> = LazyLock::new(Default::default);

/// Clé d'un chemin (casse ignorée, comme les chemins Windows de la bibliothèque).
fn cle(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// Réservation d'écriture : les lectures des fichiers attendent jusqu'à sa fin (`Drop`).
#[must_use = "la réservation prend fin dès qu'elle est abandonnée"]
pub struct Reservation {
    cles: Vec<String>,
}

/// Réserve `chemins` pour une écriture.
pub fn reserver<'a>(chemins: impl IntoIterator<Item = &'a Path>) -> Reservation {
    let cles: Vec<String> = chemins.into_iter().map(cle).collect();
    let (verrou, _) = &*REGISTRE;
    let mut g = verrou.lock().unwrap_or_else(|e| e.into_inner());
    for c in &cles {
        *g.entry(c.clone()).or_default() += 1;
    }
    Reservation { cles }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let (verrou, cv) = &*REGISTRE;
        let mut g = verrou.lock().unwrap_or_else(|e| e.into_inner());
        for c in &self.cles {
            if let Some(n) = g.get_mut(c) {
                *n -= 1;
                if *n == 0 {
                    g.remove(c);
                }
            }
        }
        cv.notify_all();
    }
}

/// Le fichier est-il en cours d'écriture ?
pub fn en_ecriture(chemin: &Path) -> bool {
    let (verrou, _) = &*REGISTRE;
    verrou.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&cle(chemin))
}

/// Attend que `chemin` ne soit plus en écriture, au plus `max`. `false` : toujours en écriture.
pub fn attendre_libre(chemin: &Path, max: Duration) -> bool {
    let c = cle(chemin);
    let fin = Instant::now() + max;
    let (verrou, cv) = &*REGISTRE;
    let mut g = verrou.lock().unwrap_or_else(|e| e.into_inner());
    while g.contains_key(&c) {
        let reste = fin.saturating_duration_since(Instant::now());
        if reste.is_zero() {
            return false;
        }
        g = cv.wait_timeout(g, reste).unwrap_or_else(|e| e.into_inner()).0;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lectures_attendent_la_fin_de_l_ecriture() {
        let p = Path::new("/essai/Liberation 01.CBZ");
        assert!(attendre_libre(p, Duration::ZERO));
        let r = reserver([p]);
        let r2 = reserver([Path::new("/essai/liberation 01.cbz")]);
        assert!(en_ecriture(Path::new("/ESSAI/liberation 01.cbz")), "casse ignorée");
        assert!(!attendre_libre(p, Duration::from_millis(20)));
        drop(r);
        assert!(en_ecriture(p), "encore une réservation");
        let t =
            std::thread::spawn(move || attendre_libre(Path::new("/essai/Liberation 01.CBZ"), Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(50));
        drop(r2);
        assert!(t.join().unwrap(), "réveillé à la fin de l'écriture");
        assert!(!en_ecriture(p));
    }
}
