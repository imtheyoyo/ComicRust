//! Garde-fou « disparition massive », commun à l'analyse des dossiers (`scanner`) et à la
//! vérification des livres manquants (`missing`).
//!
//! Un partage partiellement monté (branche mergerfs absente, disque d'un pool débranché,
//! Samba qui sert un point de montage vide) fait « disparaître » d'un coup une partie de la
//! bibliothèque. Les livres concernés ne doivent pas être marqués manquants (ni retirés
//! sans confirmation). La règle est appliquée **par groupe** :
//!
//! - le dossier surveillé lui-même (ou, à défaut, la racine du volume : `\\SERVEUR\partage`,
//!   `Z:\`, `/mnt/nas`) ;
//! - chacun de ses **sous-dossiers de premier niveau** (`…\BD\Marvel`) : une branche
//!   mergerfs absente vide souvent quelques dossiers entiers sans atteindre la moitié de
//!   la bibliothèque.
//!
//! Un groupe est bloqué si, d'un coup, **au moins [`MASS_MISSING_ABS`] livres** y
//! disparaissent, ou **au moins [`MASS_MISSING_MIN`] livres et plus de la moitié** de ceux
//! qui y étaient présents (voir [`is_mass_disappearance`]). Un livre est bloqué si l'un de
//! ses groupes l'est.

use std::collections::HashMap;

use crate::scanner::path_key;

/// Seuil relatif : au moins ce nombre de livres **et** plus de la moitié des livres
/// présents d'un groupe.
pub const MASS_MISSING_MIN: usize = 20;

/// Seuil absolu : à partir de ce nombre de livres disparus d'un coup sous un même groupe,
/// c'est bloqué quelle que soit la proportion (2 000 livres d'une branche mergerfs sur
/// 5 300, par exemple).
pub const MASS_MISSING_ABS: usize = 200;

/// Disparition massive ? `known` : livres présents avant (disparus compris) ; `gone` :
/// livres disparus.
pub fn is_mass_disappearance(known: usize, gone: usize) -> bool {
    gone >= MASS_MISSING_ABS || (gone >= MASS_MISSING_MIN && gone * 2 > known)
}

/// Éléments d'un chemin (séparateurs `\` et `/`, éléments vides ignorés).
fn components(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|c| !c.is_empty()).collect()
}

/// Sous-dossier de premier niveau de `file` sous `root` (texte d'origine), s'il y en a un
/// (`None` : fichier directement dans `root`, ou hors de `root`).
fn first_level<'a>(root: &str, file: &'a str) -> Option<&'a str> {
    let n = components(root).len();
    let parts = components(file);
    // Au moins un dossier entre la racine et le fichier.
    (parts.len() > n + 1).then(|| parts[n])
}

/// Groupe bloqué (pour les messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MassBlock {
    /// Dossier (texte d'origine : `\\NAS\BD`, `\\NAS\BD\Marvel`).
    pub label: String,
    /// Livres présents avant.
    pub known: usize,
    /// Livres disparus.
    pub gone: usize,
}

#[derive(Debug, Default)]
struct Group {
    label: String,
    /// Racine du groupe (clé), pour ne pas répéter les sous-groupes d'une racine bloquée.
    root: String,
    known: usize,
    gone: usize,
}

/// Compteurs par groupe. `root` : dossier surveillé (ou racine du volume) qui contient le
/// fichier, tel qu'écrit ; `file` : chemin du fichier.
#[derive(Debug, Default)]
pub struct MassGuard {
    groups: HashMap<String, Group>,
}

impl MassGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clés (et libellés) des groupes d'un fichier : la racine, puis son sous-dossier de
    /// premier niveau.
    fn keys(root: &str, file: &str) -> Vec<(String, String)> {
        let rk = path_key(root);
        let mut out = vec![(rk.clone(), root.to_string())];
        if let Some(sub) = first_level(root, file) {
            let sep = if root.contains('\\') { '\\' } else { '/' };
            let label = format!("{}{sep}{sub}", root.trim_end_matches(['\\', '/']));
            out.push((format!("{}/{}", rk.trim_end_matches('/'), sub.to_lowercase()), label));
        }
        out
    }

    fn bump(&mut self, root: &str, file: &str, gone: bool) {
        let rk = path_key(root);
        for (k, label) in Self::keys(root, file) {
            let g = self.groups.entry(k).or_insert_with(|| Group { label, root: rk.clone(), ..Default::default() });
            if gone {
                g.gone += 1;
            } else {
                g.known += 1;
            }
        }
    }

    /// Livre présent avant (disparu ou non) : à compter une fois par livre.
    pub fn add_known(&mut self, root: &str, file: &str) {
        self.bump(root, file, false);
    }

    /// Livre disparu (compté aussi par [`add_known`](Self::add_known)).
    pub fn add_gone(&mut self, root: &str, file: &str) {
        self.bump(root, file, true);
    }

    /// Le livre `file` (sous `root`) est-il dans un groupe bloqué ?
    pub fn is_blocked(&self, root: &str, file: &str) -> bool {
        Self::keys(root, file)
            .iter()
            .any(|(k, _)| self.groups.get(k).is_some_and(|g| is_mass_disappearance(g.known, g.gone)))
    }

    /// Groupes bloqués, triés ; les sous-dossiers d'une racine elle-même bloquée ne sont pas
    /// répétés.
    pub fn blocked(&self) -> Vec<MassBlock> {
        let bad = |g: &Group| is_mass_disappearance(g.known, g.gone);
        let blocked_roots: Vec<&str> =
            self.groups.iter().filter(|(k, g)| **k == g.root && bad(g)).map(|(k, _)| k.as_str()).collect();
        let mut out: Vec<(&String, MassBlock)> = self
            .groups
            .iter()
            .filter(|(k, g)| bad(g) && (**k == g.root || !blocked_roots.contains(&g.root.as_str())))
            .map(|(k, g)| (k, MassBlock { label: g.label.clone(), known: g.known, gone: g.gone }))
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out.into_iter().map(|(_, b)| b).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seuils() {
        assert!(!is_mass_disappearance(40, 19));
        assert!(!is_mass_disappearance(40, 20), "la moitié pile : pas bloqué");
        assert!(is_mass_disappearance(40, 21));
        assert!(!is_mass_disappearance(5300, 199));
        assert!(is_mass_disappearance(5300, 200), "seuil absolu");
        assert!(is_mass_disappearance(5300, 2000));
    }

    #[test]
    fn premier_niveau() {
        assert_eq!(first_level(r"\\NAS\BD", r"\\NAS\BD\Marvel\Thanos\01.cbz"), Some("Marvel"));
        assert_eq!(first_level(r"\\NAS\BD\", r"\\NAS\BD\a.cbz"), None);
        assert_eq!(first_level("/mnt/nas", "/mnt/nas/DC/b.cbz"), Some("DC"));
        assert_eq!(first_level(r"Z:\", r"Z:\Séries\x.cbz"), Some("Séries"));
    }

    /// Branche mergerfs absente : 2 000 livres sur 5 300 disparaissent, répartis dans des
    /// séries entières. Moins de la moitié du dossier : seul le seuil absolu (et les
    /// sous-dossiers vidés) le voient.
    #[test]
    fn branche_mergerfs_absente() {
        let root = r"\\NAS\BD";
        let mut g = MassGuard::new();
        for s in 0..53 {
            for i in 0..100 {
                let f = format!(r"\\NAS\BD\Série {s:02}\Tome {i:03}.cbz");
                g.add_known(root, &f);
                if s < 20 {
                    g.add_gone(root, &f);
                }
            }
        }
        assert!(g.is_blocked(root, r"\\NAS\BD\Série 00\Tome 000.cbz"));
        let b = g.blocked();
        assert_eq!(b.len(), 1, "la racine bloquée : ses sous-dossiers ne sont pas répétés ({b:?})");
        assert_eq!((b[0].label.as_str(), b[0].known, b[0].gone), (root, 5300, 2000));

        // Sous le seuil absolu au niveau de la racine, mais un dossier de premier niveau vidé.
        let mut g = MassGuard::new();
        for s in ["Marvel", "DC", "Franco-belge"] {
            for i in 0..60 {
                let f = format!("/srv/bd/{s}/Série {}/{i}.cbz", i % 6);
                g.add_known("/srv/bd", &f);
                if s == "Marvel" && i < 50 {
                    g.add_gone("/srv/bd", &f);
                }
            }
        }
        assert!(g.is_blocked("/srv/bd", "/srv/bd/Marvel/Série 0/0.cbz"));
        assert!(!g.is_blocked("/srv/bd", "/srv/bd/DC/Série 0/0.cbz"));
        assert_eq!(g.blocked(), [MassBlock { label: "/srv/bd/Marvel".into(), known: 60, gone: 50 }]);
    }
}
