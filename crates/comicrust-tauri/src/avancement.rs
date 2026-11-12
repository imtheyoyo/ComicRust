//! Avancement de lecture mémorisé **localement**, par fichier : `data\avancement.json`.
//!
//! Équivalent des champs `last_pages` et `read_progress` des réglages de la version egui
//! (`comicrust.ron`) : page courante et page la plus loin atteinte de chaque fichier lu dans
//! ComicRust, quel que soit le mode. En mode lié à ComicRack (lecture seule), c'est le seul
//! endroit où la lecture est gardée ; en mode autonome, la base de ComicRust est écrite en plus
//! (voir [`crate::lecteur`]).
//!
//! Fichier séparé de `comicrust-tauri.json` : il change à chaque page tournée, les réglages
//! presque jamais. Écriture atomique (fichier temporaire renommé), sous verrou.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Nom du fichier dans `data\`.
pub const FICHIER_AVANCEMENT: &str = "avancement.json";

/// Lecture d'un fichier, en numéros de page du livre (ComicRack : rang dans la liste complète
/// des pages, pages supprimées comprises, à partir de 0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Avancement {
    /// Page courante (page de reprise).
    pub page: usize,
    /// Page la plus loin atteinte.
    pub page_max: usize,
    /// Nombre d'images du fichier.
    pub pages: usize,
    /// **Relecture en cours** : le livre était déjà lu (fiche à 100 % ou fin vue dans ComicRust)
    /// à l'ouverture de la séance qui a noté `page`, et cette séance n'a pas encore revu la fin.
    /// `page` est alors la position de la relecture ; `page_max` reste la fin (le livre reste lu).
    /// Absent des anciens fichiers : `false`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub relecture: bool,
}

impl Avancement {
    /// Pourcentage lu, comme `ReadProgress::percent` de la version egui (`None` : jamais avancé).
    pub fn pourcentage(&self) -> Option<u32> {
        if self.pages == 0 || self.page_max == 0 {
            return None;
        }
        Some((((self.page_max + 1) * 100 / self.pages) as u32).clamp(1, 100))
    }
}

/// Mémoire des avancements, chargée une fois puis tenue à jour en mémoire.
pub struct Memoire {
    data: PathBuf,
    fichiers: Mutex<Option<HashMap<String, Avancement>>>,
}

impl Memoire {
    pub fn nouvelle(data: PathBuf) -> Self {
        Memoire { data, fichiers: Mutex::new(None) }
    }

    fn chemin(&self) -> PathBuf {
        self.data.join(FICHIER_AVANCEMENT)
    }

    /// Table chargée (lecture tolérante : entrée invalide ignorée ; fichier illisible mis de
    /// côté sous `avancement.json.illisible` plutôt qu'écrasé en silence).
    fn charge(&self) -> MutexGuard<'_, Option<HashMap<String, Avancement>>> {
        let mut g = self.fichiers.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_none() {
            let p = self.chemin();
            let table = match std::fs::read_to_string(&p) {
                Ok(t) => match serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')) {
                    Ok(Value::Object(o)) => o
                        .into_iter()
                        .filter_map(|(k, v)| Some((k, serde_json::from_value::<Avancement>(v).ok()?)))
                        .collect(),
                    _ => {
                        let _ = std::fs::rename(&p, p.with_extension("json.illisible"));
                        HashMap::new()
                    }
                },
                Err(_) => HashMap::new(),
            };
            *g = Some(table);
        }
        g
    }

    /// Avancement mémorisé pour `fichier` (chemin tel qu'ouvert).
    pub fn lire(&self, fichier: &Path) -> Option<Avancement> {
        self.charge().as_ref()?.get(&cle(fichier)).copied()
    }

    /// Note la page courante et la page vue la plus loin (la plus loin connue est gardée) et si
    /// c'est une relecture (voir [`Avancement::relecture`]) ; écrit le fichier si quelque chose
    /// change.
    pub fn noter(
        &self,
        fichier: &Path,
        page: usize,
        page_vue: usize,
        pages: usize,
        relecture: bool,
    ) -> Result<Avancement, String> {
        let mut g = self.charge();
        let table = g.get_or_insert_with(HashMap::new);
        let ancien = table.get(&cle(fichier)).copied();
        let mut a = ancien.unwrap_or_default();
        a.page = page;
        a.page_max = a.page_max.max(page_vue).max(page);
        a.pages = pages;
        a.relecture = relecture;
        if ancien == Some(a) {
            return Ok(a);
        }
        table.insert(cle(fichier), a);
        drop(g);
        self.ecrire()?;
        Ok(a)
    }

    /// Écrit la table. Les écritures sont sérialisées et chacune prend la table **au moment
    /// d'écrire** : deux appels concurrents ne peuvent pas laisser sur le disque l'état le plus
    /// ancien (la table n'est pas verrouillée pendant l'écriture disque).
    fn ecrire(&self) -> Result<(), String> {
        let _v = VERROU.lock().unwrap_or_else(|e| e.into_inner());
        let texte = {
            let g = self.charge();
            serde_json::to_string(g.as_ref().unwrap_or(&HashMap::new())).map_err(|e| e.to_string())?
        };
        ecrire_atomique(&self.data, &self.chemin(), &texte)
    }
}

fn cle(fichier: &Path) -> String {
    fichier.to_string_lossy().into_owned()
}

/// Sérialise les écritures du fichier (deux lecteurs dans le même processus).
static VERROU: Mutex<()> = Mutex::new(());

fn ecrire_atomique(data: &Path, p: &Path, texte: &str) -> Result<(), String> {
    std::fs::create_dir_all(data).map_err(|e| format!("{} : {e}", data.display()))?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, texte).map_err(|e| format!("{} : {e}", tmp.display()))?;
    std::fs::rename(&tmp, p).map_err(|e| format!("{} : {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noter_relire_et_garder_la_page_la_plus_loin() {
        let d = tempfile::tempdir().unwrap();
        let f = Path::new("Z:/BD/Thanos 01.cbz");
        let m = Memoire::nouvelle(d.path().to_path_buf());
        assert_eq!(m.lire(f), None);
        m.noter(f, 5, 6, 20, false).unwrap();
        m.noter(f, 2, 2, 20, false).unwrap();
        // Relu depuis le disque par une autre instance (redémarrage).
        let m2 = Memoire::nouvelle(d.path().to_path_buf());
        assert_eq!(m2.lire(f), Some(Avancement { page: 2, page_max: 6, pages: 20, relecture: false }));
        assert_eq!(m2.lire(f).unwrap().pourcentage(), Some(35));
        assert_eq!(Avancement { page: 0, page_max: 0, pages: 20, ..Default::default() }.pourcentage(), None);
        assert_eq!(Avancement { page: 19, page_max: 19, pages: 20, ..Default::default() }.pourcentage(), Some(100));
    }

    #[test]
    fn fichier_illisible_mis_de_cote() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(FICHIER_AVANCEMENT), "{ pas du json").unwrap();
        let m = Memoire::nouvelle(d.path().to_path_buf());
        assert_eq!(m.lire(Path::new("a.cbz")), None);
        assert!(d.path().join("avancement.json.illisible").exists());
        m.noter(Path::new("a.cbz"), 1, 1, 3, false).unwrap();
        // Entrée invalide ignorée, les autres gardées.
        std::fs::write(d.path().join(FICHIER_AVANCEMENT), r#"{"a.cbz":{"page":1,"pageMax":2,"pages":3},"b.cbz":"x"}"#)
            .unwrap();
        let m = Memoire::nouvelle(d.path().to_path_buf());
        assert_eq!(m.lire(Path::new("a.cbz")).map(|a| a.page_max), Some(2));
        assert_eq!(m.lire(Path::new("b.cbz")), None);
    }

    #[test]
    fn relecture_memorisee_et_compatible_avec_les_anciens_fichiers() {
        let d = tempfile::tempdir().unwrap();
        let f = Path::new("Z:/BD/Thanos 02.cbz");
        let m = Memoire::nouvelle(d.path().to_path_buf());
        m.noter(f, 19, 19, 20, false).unwrap();
        // Relecture : la page courante recule, la plus loin reste la fin.
        m.noter(f, 6, 6, 20, true).unwrap();
        let m2 = Memoire::nouvelle(d.path().to_path_buf());
        assert_eq!(m2.lire(f), Some(Avancement { page: 6, page_max: 19, pages: 20, relecture: true }));
        // Champ absent quand il est faux (fichier inchangé pour les anciennes versions).
        let texte = std::fs::read_to_string(d.path().join(FICHIER_AVANCEMENT)).unwrap();
        assert!(texte.contains(r#""relecture":true"#), "{texte}");
        m2.noter(f, 19, 19, 20, false).unwrap();
        let texte = std::fs::read_to_string(d.path().join(FICHIER_AVANCEMENT)).unwrap();
        assert!(!texte.contains("relecture"), "{texte}");
    }
}
