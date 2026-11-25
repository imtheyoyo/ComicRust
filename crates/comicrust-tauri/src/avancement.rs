//! Avancement de lecture mémorisé **localement**, par fichier : `data\avancement.json`.
//!
//! Équivalent des champs `last_pages` et `read_progress` des réglages de la version egui
//! (`comicrust.ron`) : page courante et page la plus loin atteinte de chaque fichier lu dans
//! ComicRust, quel que soit le mode. En mode lié à ComicRack (lecture seule), c'est le seul
//! endroit où la lecture est gardée ; en mode autonome, la base de ComicRust est écrite en plus
//! (voir [`crate::lecteur`]).
//!
//! Fichier séparé de `comicrust-tauri.json` : il change à chaque page tournée, les réglages
//! presque jamais. Écriture atomique (fichier temporaire renommé), sous verrou, **différée**
//! dans l'application (une écriture au plus toutes les [`DELAI_ECRITURE`], plus une écriture
//! finale à la fermeture du livre et de l'application : [`Memoire::vider`]).
//!
//! Lecture prudente : une table vide n'est prise que si le fichier **n'existe pas**. Fichier
//! illisible (JSON invalide, encodage, lecture refusée par un antivirus…) : il est mis de côté sous
//! `avancement.json.illisible` ; si même cela échoue, **rien n'est écrit** (les pages notées restent
//! en mémoire) et la lecture est retentée à l'écriture suivante, puis fusionnée.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// Nom du fichier dans `data\`.
pub const FICHIER_AVANCEMENT: &str = "avancement.json";
/// Délai de l'écriture différée dans l'application.
pub const DELAI_ECRITURE: Duration = Duration::from_secs(2);

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

type Table = HashMap<String, Avancement>;

/// État en mémoire.
#[derive(Default)]
struct Etat {
    /// `None` : pas encore lu.
    table: Option<Table>,
    /// La dernière lecture du fichier a échoué sans qu'on puisse le mettre de côté : ne pas
    /// l'écraser, le relire avant d'écrire.
    a_relire: bool,
    /// Des changements attendent d'être écrits.
    sale: bool,
}

struct Interne {
    data: PathBuf,
    etat: Mutex<Etat>,
    /// Une écriture différée est déjà prévue.
    planifiee: AtomicBool,
}

/// Mémoire des avancements, chargée une fois puis tenue à jour en mémoire (clonage bon marché).
#[derive(Clone)]
pub struct Memoire {
    interne: Arc<Interne>,
    /// `ZERO` : écriture immédiate ([`Memoire::nouvelle`], tests).
    delai: Duration,
}

/// Résultat de la lecture du fichier.
enum Lu {
    Table(Table),
    /// Illisible et impossible à mettre de côté : ne rien écrire.
    Bloque,
}

impl Interne {
    fn chemin(&self) -> PathBuf {
        self.data.join(FICHIER_AVANCEMENT)
    }

    /// Met le fichier illisible de côté ; vrai si c'est fait (un fichier neuf peut alors être écrit).
    fn mettre_de_cote(&self) -> bool {
        let p = self.chemin();
        std::fs::rename(&p, p.with_extension("json.illisible")).is_ok()
    }

    fn lire_disque(&self) -> Lu {
        let octets = match std::fs::read(self.chemin()) {
            Ok(o) => o,
            Err(e) if e.kind() == ErrorKind::NotFound => return Lu::Table(Table::new()),
            Err(_) => return if self.mettre_de_cote() { Lu::Table(Table::new()) } else { Lu::Bloque },
        };
        let texte = std::str::from_utf8(&octets).ok().map(|t| t.trim_start_matches('\u{feff}'));
        match texte.and_then(|t| serde_json::from_str::<Value>(t).ok()) {
            Some(Value::Object(o)) => Lu::Table(
                o.into_iter().filter_map(|(k, v)| Some((k, serde_json::from_value::<Avancement>(v).ok()?))).collect(),
            ),
            _ if self.mettre_de_cote() => Lu::Table(Table::new()),
            _ => Lu::Bloque,
        }
    }

    /// État avec la table chargée (lecture tolérante : entrée invalide ignorée). Après un échec
    /// bloquant, le fichier est relu et fusionné (la mémoire l'emporte : pages notées entre-temps).
    fn charge(&self) -> MutexGuard<'_, Etat> {
        let mut g = self.etat.lock().unwrap_or_else(|e| e.into_inner());
        if g.table.is_none() || g.a_relire {
            match self.lire_disque() {
                Lu::Table(mut disque) => {
                    if let Some(memoire) = g.table.take() {
                        disque.extend(memoire);
                    }
                    g.table = Some(disque);
                    g.a_relire = false;
                }
                Lu::Bloque => {
                    g.table.get_or_insert_with(Table::new);
                    g.a_relire = true;
                }
            }
        }
        g
    }

    /// Écrit la table si elle a changé. Les écritures sont sérialisées et chacune prend la table
    /// **au moment d'écrire** : deux appels concurrents ne peuvent pas laisser sur le disque l'état
    /// le plus ancien. Fichier illisible et non mis de côté : rien n'est écrit (changements gardés).
    fn ecrire(&self) -> Result<(), String> {
        let _v = VERROU.lock().unwrap_or_else(|e| e.into_inner());
        let texte = {
            let mut g = self.charge();
            if g.a_relire || !g.sale {
                return Ok(());
            }
            g.sale = false;
            serde_json::to_string(g.table.as_ref().unwrap_or(&Table::new())).map_err(|e| e.to_string())?
        };
        let r = ecrire_atomique(&self.data, &self.chemin(), &texte);
        if r.is_err() {
            self.etat.lock().unwrap_or_else(|e| e.into_inner()).sale = true;
        }
        r
    }
}

impl Memoire {
    /// Écriture immédiate à chaque changement.
    pub fn nouvelle(data: PathBuf) -> Self {
        Self::avec_delai(data, Duration::ZERO)
    }

    /// Écriture différée de `delai` (application : [`DELAI_ECRITURE`]).
    pub fn avec_delai(data: PathBuf, delai: Duration) -> Self {
        Memoire {
            interne: Arc::new(Interne { data, etat: Mutex::default(), planifiee: AtomicBool::new(false) }),
            delai,
        }
    }

    /// Avancement mémorisé pour `fichier` (chemin tel qu'ouvert).
    pub fn lire(&self, fichier: &Path) -> Option<Avancement> {
        self.interne.charge().table.as_ref()?.get(&cle(fichier)).copied()
    }

    /// Note la page courante et la page vue la plus loin (la plus loin connue est gardée) et si
    /// c'est une relecture (voir [`Avancement::relecture`]) ; le fichier est écrit (tout de suite ou
    /// après le délai) si quelque chose change.
    pub fn noter(
        &self,
        fichier: &Path,
        page: usize,
        page_vue: usize,
        pages: usize,
        relecture: bool,
    ) -> Result<Avancement, String> {
        let mut g = self.interne.charge();
        let table = g.table.get_or_insert_with(Table::new);
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
        g.sale = true;
        drop(g);
        self.planifier()?;
        Ok(a)
    }

    /// Remplace (ou, avec `None`, oublie) l'avancement de `fichier`, sans garder « la page la plus
    /// loin » : sert à « Marquer comme lu / non lu » (`docs/tauri/ipc-lecture-etat.md`). Écrit le
    /// fichier tout de suite si quelque chose change.
    pub fn fixer(&self, fichier: &Path, valeur: Option<Avancement>) -> Result<(), String> {
        let mut g = self.interne.charge();
        let table = g.table.get_or_insert_with(Table::new);
        let change = match valeur {
            Some(a) => table.insert(cle(fichier), a) != Some(a),
            None => table.remove(&cle(fichier)).is_some(),
        };
        g.sale |= change;
        drop(g);
        if change {
            self.interne.ecrire()?;
        }
        Ok(())
    }

    /// Écrit tout de suite les changements en attente (fermeture d'un livre, de l'application).
    pub fn vider(&self) -> Result<(), String> {
        self.interne.ecrire()
    }

    /// Écriture immédiate, ou différée (un seul fil d'écriture en attente à la fois).
    fn planifier(&self) -> Result<(), String> {
        if self.delai.is_zero() {
            return self.interne.ecrire();
        }
        if self.interne.planifiee.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let (interne, delai) = (self.interne.clone(), self.delai);
        let lance = std::thread::Builder::new().name("comicrust-avancement".into()).spawn(move || {
            std::thread::sleep(delai);
            interne.planifiee.store(false, Ordering::SeqCst);
            if let Err(e) = interne.ecrire() {
                eprintln!("ComicRust : avancement non enregistré : {e}");
            }
        });
        if lance.is_err() {
            self.interne.planifiee.store(false, Ordering::SeqCst);
            return self.interne.ecrire();
        }
        Ok(())
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

    /// Preuve de la relecture : un fichier illisible en UTF-8 donnait une table vide, réécrite à la
    /// page suivante (tout l'historique perdu, sans copie `.illisible`).
    #[test]
    fn fichier_mal_encode_mis_de_cote_et_non_ecrase() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(FICHIER_AVANCEMENT);
        let mut contenu = br#"{"Z:/BD/A.cbz":{"page":10,"pageMax":20,"pages":30},"Z:/BD/"#.to_vec();
        contenu.push(0xE9); // « é » en Latin-1
        contenu.extend_from_slice(br#"t.cbz":{"page":1,"pageMax":1,"pages":2}}"#);
        std::fs::write(&p, &contenu).unwrap();
        let m = Memoire::nouvelle(d.path().to_path_buf());
        m.noter(Path::new("Z:/BD/B.cbz"), 3, 3, 10, false).unwrap();
        assert_eq!(std::fs::read(d.path().join("avancement.json.illisible")).unwrap(), contenu, "original gardé");
        assert!(std::fs::read_to_string(&p).unwrap().contains("B.cbz"));
    }

    /// Fichier illisible qu'on ne peut même pas mettre de côté : rien n'est écrit tant que la
    /// lecture n'a pas réussi ; les pages notées restent en mémoire puis sont fusionnées.
    #[test]
    fn rien_n_est_ecrit_tant_que_la_lecture_echoue() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(FICHIER_AVANCEMENT);
        std::fs::write(&p, b"{ pas du json").unwrap();
        // Empêche la mise de côté : la cible est un dossier non vide.
        let bloqueur = d.path().join("avancement.json.illisible");
        std::fs::create_dir_all(bloqueur.join("x")).unwrap();
        let m = Memoire::nouvelle(d.path().to_path_buf());
        m.noter(Path::new("B.cbz"), 3, 3, 10, false).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"{ pas du json", "fichier non écrasé");
        assert_eq!(m.lire(Path::new("B.cbz")).map(|a| a.page), Some(3), "gardé en mémoire");
        // Le fichier redevient lisible (antivirus parti, réparé à la main) : fusion, puis écriture.
        std::fs::remove_dir_all(&bloqueur).unwrap();
        std::fs::write(&p, br#"{"A.cbz":{"page":1,"pageMax":2,"pages":3}}"#).unwrap();
        m.vider().unwrap();
        let texte = std::fs::read_to_string(&p).unwrap();
        assert!(texte.contains("A.cbz") && texte.contains("B.cbz"), "{texte}");
    }

    #[test]
    fn ecriture_differee_puis_finale() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(FICHIER_AVANCEMENT);
        let m = Memoire::avec_delai(d.path().to_path_buf(), Duration::from_millis(150));
        for page in 1..=5 {
            m.noter(Path::new("A.cbz"), page, page, 10, false).unwrap();
        }
        assert!(!p.exists(), "pas d'écriture à chaque page");
        std::thread::sleep(Duration::from_millis(600));
        assert!(std::fs::read_to_string(&p).unwrap().contains(r#""page":5"#));
        // Écriture finale (fermeture) sans attendre le délai.
        m.noter(Path::new("A.cbz"), 7, 7, 10, false).unwrap();
        m.vider().unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().contains(r#""page":7"#));
    }
}
