//! Fenêtre principale et lancement : logique pure (sans Tauri), donc testée partout.
//!
//! - géométrie mémorisée (section `fenetre` de `data\comicrust-tauri.json`) : restauration au
//!   démarrage avec contrôle de la position (écran débranché = fenêtre centrée) et sauvegarde
//!   limitée dans le temps pendant les redimensionnements ([`Suivi`]) ;
//! - titre dynamique « <livre> — ComicRust » (`update_title` de la version egui) ;
//! - analyse de la ligne de commande : `ComicRust.exe [fichier | dossier | liste.cbl]`, compatible
//!   avec `--mcp`, `--mcp-stdio` et `--attendre-pid <pid>` (`main.rs` de la version egui).
//!
//! Les appels à Tauri (création de la fenêtre, événements) sont dans `cmd_fenetre` (fonction `app`).
//! Contrat : `docs/tauri/ipc-fenetre.md`.

use crate::reglages::ReglagesFenetre;
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Taille de la première ouverture (celle de l'ancienne configuration).
pub const LARGEUR_DEFAUT: u32 = 1280;
pub const HAUTEUR_DEFAUT: u32 = 800;
/// Taille minimale de la fenêtre (identique à `min_inner_size` de la création).
pub const LARGEUR_MIN: u32 = 800;
pub const HAUTEUR_MIN: u32 = 500;

/// Intervalle minimal entre deux écritures du fichier pendant un redimensionnement ou un
/// déplacement (Windows envoie des dizaines d'événements par seconde). La dernière géométrie est
/// toujours écrite à la fermeture.
pub const INTERVALLE_SAUVEGARDE: Duration = Duration::from_millis(1000);

/// Bande de la barre de titre testée pour juger une position « encore atteignable ».
const BANDE_TITRE: i64 = 32;
/// Recouvrement minimal de cette bande avec un écran : de quoi attraper la fenêtre à la souris.
const RECOUVREMENT_LARGEUR: i64 = 100;
const RECOUVREMENT_HAUTEUR: i64 = 16;

/// Géométrie « normale » (non maximisée) de la fenêtre, en pixels physiques.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometrie {
    pub x: i32,
    pub y: i32,
    pub largeur: u32,
    pub hauteur: u32,
}

/// Zone d'un écran, en pixels physiques (coordonnées du bureau virtuel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ecran {
    pub x: i32,
    pub y: i32,
    pub largeur: u32,
    pub hauteur: u32,
}

/// Où placer la fenêtre à l'ouverture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Position mémorisée, toujours sur un écran existant.
    A(i32, i32),
    /// Position mémorisée inutilisable (écran absent) : fenêtre centrée.
    Centrer,
}

/// Résultat de [`restaurer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Restauration {
    pub largeur: u32,
    pub hauteur: u32,
    pub placement: Placement,
    pub maximisee: bool,
}

impl Geometrie {
    fn en_reglages(self, maximisee: bool) -> ReglagesFenetre {
        ReglagesFenetre { x: Some(self.x), y: Some(self.y), largeur: Some(self.largeur), hauteur: Some(self.hauteur), maximisee }
    }
}

/// Recouvrement (en pixels) de la bande de titre de `g` avec l'écran `e` : (largeur, hauteur).
fn recouvrement_titre(g: &Geometrie, e: &Ecran) -> (i64, i64) {
    let (gx, gy) = (i64::from(g.x), i64::from(g.y));
    let (ex, ey) = (i64::from(e.x), i64::from(e.y));
    let l = (gx + i64::from(g.largeur)).min(ex + i64::from(e.largeur)) - gx.max(ex);
    let h = (gy + BANDE_TITRE).min(ey + i64::from(e.hauteur)) - gy.max(ey);
    (l.max(0), h.max(0))
}

/// La position de `g` est-elle encore atteignable sur l'un des `ecrans` ? Oui si sa barre de titre
/// recouvre un écran d'au moins 100 px de large et 16 px de haut. Sans liste d'écrans (système qui
/// n'en dit rien), on fait confiance à la position mémorisée.
pub fn position_valide(g: &Geometrie, ecrans: &[Ecran]) -> bool {
    ecrans.is_empty() || ecrans.iter().any(|e| {
        let (l, h) = recouvrement_titre(g, e);
        l >= RECOUVREMENT_LARGEUR && h >= RECOUVREMENT_HAUTEUR
    })
}

/// Décide de l'ouverture à partir des réglages : `None` = rien de mémorisé (ouverture par défaut).
///
/// La taille est ramenée entre le minimum de la fenêtre et la taille du plus grand écran ; la
/// position n'est gardée que si elle est atteignable ([`position_valide`]), sinon la fenêtre est
/// centrée. `maximisee` est rendu tel quel.
pub fn restaurer(r: &ReglagesFenetre, ecrans: &[Ecran]) -> Option<Restauration> {
    let (Some(l), Some(h)) = (r.largeur, r.hauteur) else {
        // Seul l'état maximisé est connu : ouverture par défaut, maximisée.
        return r.maximisee.then_some(Restauration {
            largeur: LARGEUR_DEFAUT,
            hauteur: HAUTEUR_DEFAUT,
            placement: Placement::Centrer,
            maximisee: true,
        });
    };
    let max_l = ecrans.iter().map(|e| e.largeur).max().unwrap_or(u32::MAX).max(LARGEUR_MIN);
    let max_h = ecrans.iter().map(|e| e.hauteur).max().unwrap_or(u32::MAX).max(HAUTEUR_MIN);
    let largeur = l.clamp(LARGEUR_MIN, max_l);
    let hauteur = h.clamp(HAUTEUR_MIN, max_h);
    let placement = match (r.x, r.y) {
        (Some(x), Some(y)) if position_valide(&Geometrie { x, y, largeur, hauteur }, ecrans) => Placement::A(x, y),
        _ => Placement::Centrer,
    };
    Some(Restauration { largeur, hauteur, placement, maximisee: r.maximisee })
}

/// Suivi de la géométrie pendant la session, avec limitation des écritures.
///
/// À chaque événement de fenêtre (redimensionnement, déplacement), l'appelant donne la géométrie
/// normale courante (`None` si la fenêtre est maximisée : on garde alors la dernière géométrie
/// normale) et l'état maximisé ; [`Suivi::observer`] dit s'il faut écrire maintenant.
#[derive(Debug)]
pub struct Suivi {
    courant: ReglagesFenetre,
    ecrit: ReglagesFenetre,
    derniere_ecriture: Option<Instant>,
}

impl Suivi {
    /// `lus` : réglages lus au démarrage (considérés comme déjà écrits).
    pub fn nouveau(lus: ReglagesFenetre) -> Self {
        Self { courant: lus, ecrit: lus, derniere_ecriture: None }
    }

    /// Prend en compte l'état de la fenêtre. Renvoie les réglages à écrire si l'état a changé et
    /// qu'au moins [`INTERVALLE_SAUVEGARDE`] s'est écoulé depuis la dernière écriture.
    pub fn observer(&mut self, normale: Option<Geometrie>, maximisee: bool, maintenant: Instant) -> Option<ReglagesFenetre> {
        match normale {
            Some(g) => self.courant = g.en_reglages(maximisee),
            None => self.courant.maximisee = maximisee,
        }
        if self.courant == self.ecrit {
            return None;
        }
        if let Some(t) = self.derniere_ecriture {
            if maintenant.saturating_duration_since(t) < INTERVALLE_SAUVEGARDE {
                return None;
            }
        }
        Some(self.marquer(maintenant))
    }

    /// Écriture finale (fermeture) : sans limitation, seulement si quelque chose a changé.
    pub fn terminer(&mut self, maintenant: Instant) -> Option<ReglagesFenetre> {
        (self.courant != self.ecrit).then(|| self.marquer(maintenant))
    }

    fn marquer(&mut self, maintenant: Instant) -> ReglagesFenetre {
        self.ecrit = self.courant;
        self.derniere_ecriture = Some(maintenant);
        self.courant
    }
}

// ───────────────────────────── Titre ─────────────────────────────

/// Titre de la fenêtre : « <livre> — ComicRust » avec un livre ouvert, « ComicRust » sinon.
/// Le titre est nettoyé (caractères de contrôle remplacés par une espace) et borné.
pub fn titre_fenetre(livre: Option<&str>) -> String {
    let propre: String = livre.unwrap_or("").chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let propre = propre.split_whitespace().collect::<Vec<_>>().join(" ");
    if propre.is_empty() {
        return "ComicRust".to_string();
    }
    let borne: String = propre.chars().take(200).collect();
    format!("{borne} — ComicRust")
}

// ───────────────────────────── Ligne de commande ─────────────────────────────

/// Arguments reconnus au lancement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lancement {
    /// Fichier, dossier ou `.cbl` à ouvrir (premier argument qui n'est pas une option).
    pub chemin: Option<PathBuf>,
    /// `--attendre-pid <pid>` : relance après une mise à jour (traité par `maj`).
    pub attendre_pid: Option<u32>,
    /// `--mcp` : pilotage par une IA (variante `ComicRust-Tauri-mcp`).
    pub mcp: bool,
}

/// Analyse les arguments de la ligne de commande (**sans** le nom du programme).
///
/// - `--attendre-pid <pid>` et `--attendre-pid=<pid>` : pid d'une ancienne instance (la valeur
///   n'est jamais prise pour un chemin) ;
/// - `--mcp` : mode pilotage ; toute autre option commençant par `--` est ignorée (`--mcp-stdio`…) ;
/// - le premier argument restant, non vide, est le chemin à ouvrir (comme `args_os().nth(1)` de la
///   version egui, mais sans prendre une option pour un fichier).
pub fn analyser_arguments(args: impl IntoIterator<Item = OsString>) -> Lancement {
    let mut l = Lancement::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--attendre-pid") => {
                if let Some(v) = it.next() {
                    l.attendre_pid = v.to_str().and_then(|s| s.trim().parse().ok()).or(l.attendre_pid);
                }
            }
            Some(s) if s.starts_with("--attendre-pid=") => {
                l.attendre_pid = s["--attendre-pid=".len()..].trim().parse().ok().or(l.attendre_pid);
            }
            Some("--mcp") => l.mcp = true,
            Some(s) if s.starts_with("--") => {}
            _ => {
                if l.chemin.is_none() && !a.is_empty() {
                    l.chemin = Some(PathBuf::from(a));
                }
            }
        }
    }
    l
}

/// Chemin absolu (relatif au dossier courant du lancement) ; inchangé si le système ne sait pas.
pub fn rendre_absolu(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Nature d'un chemin reçu en argument ou déposé sur la fenêtre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GenreChemin {
    /// Fichier à lire (CBZ, CBR, CB7, PDF, image…) : ouvert dans le lecteur.
    Livre,
    /// Dossier : ouvert comme livre (dossier d'images) ou ajouté aux dossiers de la bibliothèque.
    Dossier,
    /// Liste de lecture `.cbl` : importée dans les listes.
    Liste,
    /// Rien à cet emplacement.
    Absent,
}

/// Classe un chemin comme `open_path` de la version egui : `.cbl` = liste, le reste = livre ;
/// en plus, un dossier est reconnu comme tel (le front propose alors le choix).
pub fn classer_chemin(p: &Path) -> GenreChemin {
    if p.is_dir() {
        GenreChemin::Dossier
    } else if !p.exists() {
        GenreChemin::Absent
    } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cbl")) {
        GenreChemin::Liste
    } else {
        GenreChemin::Livre
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ecran(x: i32, y: i32, l: u32, h: u32) -> Ecran {
        Ecran { x, y, largeur: l, hauteur: h }
    }
    fn reg(x: i32, y: i32, l: u32, h: u32, max: bool) -> ReglagesFenetre {
        ReglagesFenetre { x: Some(x), y: Some(y), largeur: Some(l), hauteur: Some(h), maximisee: max }
    }
    fn args(a: &[&str]) -> Vec<OsString> {
        a.iter().map(OsString::from).collect()
    }

    #[test]
    fn position_sur_un_ecran_existant() {
        let principal = ecran(0, 0, 1920, 1080);
        let droite = ecran(1920, 0, 1920, 1080);
        let g = |x, y| Geometrie { x, y, largeur: 1280, hauteur: 800 };
        assert!(position_valide(&g(100, 100), &[principal]));
        // Deuxième écran débranché : la fenêtre était dessus, elle n'est plus atteignable.
        assert!(position_valide(&g(2000, 50), &[principal, droite]));
        assert!(!position_valide(&g(2000, 50), &[principal]));
        // Barre de titre sous le bas de l'écran, ou à peine visible à droite : refusée.
        assert!(!position_valide(&g(100, 1070), &[principal]));
        assert!(!position_valide(&g(1900, 100), &[principal]));
        // Écran à gauche de l'écran principal (coordonnées négatives) : acceptée.
        assert!(position_valide(&g(-1500, 20), &[ecran(-1920, 0, 1920, 1080), principal]));
        // Le coin minimisé de Windows (-32000) n'est jamais valide.
        assert!(!position_valide(&g(-32000, -32000), &[principal]));
        // Système qui ne liste aucun écran : on fait confiance.
        assert!(position_valide(&g(5000, 5000), &[]));
    }

    #[test]
    fn restauration_taille_position_et_maximise() {
        let ecrans = [ecran(0, 0, 1920, 1080)];
        assert_eq!(restaurer(&ReglagesFenetre::default(), &ecrans), None);
        assert_eq!(
            restaurer(&reg(50, 60, 1000, 700, false), &ecrans),
            Some(Restauration { largeur: 1000, hauteur: 700, placement: Placement::A(50, 60), maximisee: false })
        );
        assert!(restaurer(&reg(50, 60, 1000, 700, true), &ecrans).unwrap().maximisee);
        // Écran disparu : taille gardée, fenêtre centrée.
        let r = restaurer(&reg(3000, 60, 1000, 700, false), &ecrans).unwrap();
        assert_eq!((r.largeur, r.hauteur, r.placement), (1000, 700, Placement::Centrer));
        // Taille trop petite ou plus grande que le plus grand écran : ramenée dans les bornes.
        let r = restaurer(&reg(0, 0, 100, 100, false), &ecrans).unwrap();
        assert_eq!((r.largeur, r.hauteur), (LARGEUR_MIN, HAUTEUR_MIN));
        let r = restaurer(&reg(0, 0, 8000, 8000, false), &ecrans).unwrap();
        assert_eq!((r.largeur, r.hauteur), (1920, 1080));
        // Position absente : centrée. Seul « maximisée » connu : ouverture par défaut maximisée.
        let r = restaurer(&ReglagesFenetre { largeur: Some(900), hauteur: Some(600), ..Default::default() }, &ecrans).unwrap();
        assert_eq!(r.placement, Placement::Centrer);
        let r = restaurer(&ReglagesFenetre { maximisee: true, ..Default::default() }, &ecrans).unwrap();
        assert_eq!((r.largeur, r.hauteur, r.maximisee), (LARGEUR_DEFAUT, HAUTEUR_DEFAUT, true));
    }

    #[test]
    fn suivi_limite_les_ecritures_et_garde_la_taille_normale() {
        let t0 = Instant::now();
        let g = |l| Geometrie { x: 10, y: 10, largeur: l, hauteur: 600 };
        let mut s = Suivi::nouveau(ReglagesFenetre::default());
        // Première modification : écrite tout de suite.
        assert_eq!(s.observer(Some(g(900)), false, t0), Some(reg(10, 10, 900, 600, false)));
        // Rafale de redimensionnements dans la seconde : rien d'écrit.
        for i in 1..20u64 {
            assert_eq!(s.observer(Some(g(900 + i as u32)), false, t0 + Duration::from_millis(i * 20)), None);
        }
        // Après l'intervalle : la dernière géométrie.
        assert_eq!(s.observer(Some(g(1000)), false, t0 + Duration::from_millis(1100)), Some(reg(10, 10, 1000, 600, false)));
        // Même état : pas d'écriture.
        assert_eq!(s.observer(Some(g(1000)), false, t0 + Duration::from_secs(5)), None);
        // Maximisée : la géométrie normale est gardée, seul l'indicateur change.
        assert_eq!(s.observer(None, true, t0 + Duration::from_secs(6)), Some(reg(10, 10, 1000, 600, true)));
        // Modification dans l'intervalle, puis écriture finale forcée à la fermeture.
        assert_eq!(s.observer(Some(g(1100)), false, t0 + Duration::from_millis(6100)), None);
        assert_eq!(s.terminer(t0 + Duration::from_millis(6200)), Some(reg(10, 10, 1100, 600, false)));
        assert_eq!(s.terminer(t0 + Duration::from_millis(6300)), None);
    }

    #[test]
    fn suivi_ne_reecrit_pas_les_reglages_lus() {
        let lus = reg(1, 2, 1000, 700, false);
        let mut s = Suivi::nouveau(lus);
        let g = Geometrie { x: 1, y: 2, largeur: 1000, hauteur: 700 };
        assert_eq!(s.observer(Some(g), false, Instant::now()), None);
        assert_eq!(s.terminer(Instant::now()), None);
    }

    #[test]
    fn titre_dynamique() {
        assert_eq!(titre_fenetre(None), "ComicRust");
        assert_eq!(titre_fenetre(Some("")), "ComicRust");
        assert_eq!(titre_fenetre(Some("  \n ")), "ComicRust");
        assert_eq!(titre_fenetre(Some("Thanos 01")), "Thanos 01 — ComicRust");
        assert_eq!(titre_fenetre(Some("Un\ntitre\t  piégé")), "Un titre piégé — ComicRust");
        assert!(titre_fenetre(Some(&"x".repeat(500))).chars().count() <= 200 + " — ComicRust".chars().count());
    }

    #[test]
    fn arguments_chemin_et_options() {
        assert_eq!(analyser_arguments(args(&[])), Lancement::default());
        let l = analyser_arguments(args(&["D:\\BD\\Thanos 01.cbz"]));
        assert_eq!(l.chemin, Some(PathBuf::from("D:\\BD\\Thanos 01.cbz")));
        assert!(!l.mcp && l.attendre_pid.is_none());
        // Option avant le chemin, valeur de --attendre-pid jamais prise pour un chemin.
        let l = analyser_arguments(args(&["--mcp", "--attendre-pid", "4242", "liste.cbl"]));
        assert_eq!(l, Lancement { chemin: Some("liste.cbl".into()), attendre_pid: Some(4242), mcp: true });
        let l = analyser_arguments(args(&["--attendre-pid=77", "dossier"]));
        assert_eq!((l.attendre_pid, l.chemin), (Some(77), Some(PathBuf::from("dossier"))));
        // Relance de mise à jour sans fichier : aucun chemin.
        assert_eq!(analyser_arguments(args(&["--attendre-pid", "12"])).chemin, None);
        // Options inconnues ignorées, argument vide ignoré, seul le premier chemin compte.
        let l = analyser_arguments(args(&["--mcp-stdio", "", "a.cbz", "b.cbz"]));
        assert_eq!(l.chemin, Some(PathBuf::from("a.cbz")));
        assert!(!l.mcp);
        // pid illisible : ignoré, le dernier valide l'emporte.
        let l = analyser_arguments(args(&["--attendre-pid", "x", "--attendre-pid", "9"]));
        assert_eq!(l.attendre_pid, Some(9));
    }

    #[test]
    fn classement_des_chemins() {
        let d = tempfile::tempdir().unwrap();
        let cbz = d.path().join("a.cbz");
        let cbl = d.path().join("Liste.CBL");
        std::fs::write(&cbz, b"x").unwrap();
        std::fs::write(&cbl, b"<x/>").unwrap();
        assert_eq!(classer_chemin(&cbz), GenreChemin::Livre);
        assert_eq!(classer_chemin(&cbl), GenreChemin::Liste);
        assert_eq!(classer_chemin(d.path()), GenreChemin::Dossier);
        assert_eq!(classer_chemin(&d.path().join("absent.cbz")), GenreChemin::Absent);
        assert!(rendre_absolu(Path::new("x.cbz")).is_absolute());
    }
}
