//! Livres « manquants » : fiche présente dans la bibliothèque, fichier absent du disque.
//!
//! Correspondance avec ComicRack (`ComicBook.RefreshFileProperties`) : `FileIsMissing =
//! !fileInfo.Exists` (élément `<Missing>` de la fiche, critère « Est manquant » des listes
//! intelligentes, vignette barrée `DeletedStateImage`). ComicRack ne vérifie l'existence
//! qu'en relisant le fichier ; ComicRust la vérifie ici **en arrière-plan**, par dossier :
//!
//! - [`check_files`] : un `read_dir` par dossier (une seule requête réseau pour tous les
//!   livres d'un dossier du NAS), puis `exists()` pour les noms non trouvés (normalisation
//!   Unicode, casse) ; un fichier renommé en `.tmp` par ComicRack le temps de l'enregistrer
//!   n'est pas manquant.
//! - **Garde-fous** (comme ceux de l'analyse des dossiers, `scanner`) : un volume
//!   injoignable (partage `\\SERVEUR\partage` absent, lecteur `Z:` non connecté, point de
//!   montage absent) ne marque **rien** et est signalé ; une disparition massive sous un
//!   même volume (au moins [`MASS_MISSING_MIN`] livres et plus de la moitié de ceux du
//!   volume) non plus (partage partiellement monté…).
//! - [`search_folder`] : « Rechercher les fichiers manquants dans un dossier… » : par nom
//!   de fichier (sans casse), départagé par la taille, puis à défaut par taille seule (même
//!   extension, candidat unique), dans le dossier et ses sous-dossiers ; résultat à faire
//!   confirmer.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::scanner::path_key;
pub use crate::scanner::MASS_MISSING_MIN;

/// Avancement d'une vérification ou d'une recherche (lu depuis l'interface).
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
}

impl Progress {
    pub fn get(&self) -> (usize, usize) {
        (self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed))
    }
}

/// Livre à vérifier : identifiant, chemin de la fiche (pour les messages) et fichier réel à
/// examiner (le CBZ converti si l'original a été rangé).
#[derive(Debug, Clone)]
pub struct CheckItem {
    pub id: String,
    pub path: PathBuf,
}

/// Volume injoignable ou disparition massive : livres non marqués.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Racine du volume (`\\SERVEUR\partage`, `Z:\`, `/mnt/nas`).
    pub root: String,
    /// Livres concernés (non marqués manquants).
    pub books: usize,
    /// Livres que la vérification aurait marqués manquants (disparition massive), sinon 0.
    pub gone: usize,
}

/// Bilan de [`check_files`].
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    /// Fichiers absents (livres à marquer manquants).
    pub missing: Vec<String>,
    /// Fichiers présents.
    pub present: Vec<String>,
    /// Volumes injoignables (NAS éteint, lecteur réseau déconnecté) : rien de marqué.
    pub unreachable: Vec<Skipped>,
    /// Disparitions massives : rien de marqué pour ces volumes.
    pub blocked: Vec<Skipped>,
    /// Livres sans fichier ou de dossier illisible (droits) : ni présents ni manquants.
    pub unknown: Vec<String>,
    pub cancelled: bool,
}

impl CheckReport {
    /// Résumé pour l'utilisateur.
    pub fn summary(&self) -> String {
        let mut s =
            format!("{} fichier(s) manquant(s) sur {}", self.missing.len(), self.missing.len() + self.present.len());
        for u in &self.unreachable {
            s.push_str(&format!(" ; {} inaccessible ({} livre(s) non vérifié(s))", u.root, u.books));
        }
        for b in &self.blocked {
            s.push_str(&format!(
                " ; {} : {} livre(s) sur {} auraient disparu d'un coup, non marqués (partage partiellement monté ?)",
                b.root, b.gone, b.books
            ));
        }
        if self.cancelled {
            s.push_str(" — interrompu");
        }
        s
    }
}

/// Racine du volume d'un chemin, d'après son texte (séparateurs `\` et `/`) :
/// `\\SERVEUR\partage`, `Z:\`, ou les deux premiers dossiers d'un chemin absolu Unix
/// (`/mnt/nas`). `None` : chemin relatif.
pub fn volume_root(path: &str) -> Option<String> {
    let unc = path.starts_with("\\\\") || path.starts_with("//");
    if unc {
        let parts: Vec<&str> = path[2..].split(['\\', '/']).filter(|p| !p.is_empty()).take(2).collect();
        let sep = if path.starts_with("\\\\") { "\\" } else { "/" };
        return (!parts.is_empty()).then(|| format!("{sep}{sep}{}", parts.join(sep)));
    }
    let b = path.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return Some(format!("{}:\\", (b[0] as char).to_ascii_uppercase()));
    }
    if path.starts_with('/') {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        // Le dernier élément est le fichier : jamais pris comme racine.
        let dirs = parts.len().saturating_sub(1).min(2);
        return Some(if dirs == 0 { "/".to_string() } else { format!("/{}", parts[..dirs].join("/")) });
    }
    None
}

/// Le volume répond-il ? (`read_dir` : un partage absent ou un lecteur déconnecté échoue.)
fn reachable(root: &str) -> bool {
    std::fs::read_dir(root).is_ok()
}

/// Vérifie l'existence des fichiers (à appeler dans un fil : peut prendre du temps sur un
/// NAS). Voir l'en-tête du module pour les garde-fous.
pub fn check_files(items: &[CheckItem], progress: &Progress, cancel: &AtomicBool) -> CheckReport {
    let mut report = CheckReport::default();
    progress.total.store(items.len(), Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    // Par volume, puis par dossier.
    let mut volumes: HashMap<Option<String>, HashMap<PathBuf, Vec<&CheckItem>>> = HashMap::new();
    for it in items {
        if it.path.as_os_str().is_empty() {
            report.unknown.push(it.id.clone());
            progress.done.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let root = volume_root(&it.path.to_string_lossy());
        let dir = it.path.parent().map(Path::to_path_buf).unwrap_or_default();
        volumes.entry(root).or_default().entry(dir).or_default().push(it);
    }
    let mut volumes: Vec<_> = volumes.into_iter().collect();
    volumes.sort_by(|a, b| a.0.cmp(&b.0));
    for (root, dirs) in volumes {
        let count: usize = dirs.values().map(Vec::len).sum();
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return report;
        }
        if let Some(r) = root.as_deref().filter(|r| !reachable(r)) {
            report.unreachable.push(Skipped { root: r.to_string(), books: count, gone: 0 });
            progress.done.fetch_add(count, Ordering::Relaxed);
            continue;
        }
        let (mut missing, mut present) = (Vec::new(), Vec::new());
        let mut dirs: Vec<_> = dirs.into_iter().collect();
        dirs.sort_by(|a, b| a.0.cmp(&b.0));
        for (dir, books) in dirs {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                return report;
            }
            let n = books.len();
            match std::fs::read_dir(&dir) {
                Ok(entries) => {
                    let names: HashSet<String> =
                        entries.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect();
                    for b in books {
                        let name = b.path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                        let here =
                            names.contains(&name) || b.path.exists() || crate::providers::is_being_rewritten(&b.path);
                        if here { &mut present } else { &mut missing }.push(b.id.clone());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Dossier disparu (renommé, déplacé) : ses livres sont manquants, sauf
                    // fichier présent malgré tout (chemin que `read_dir` refuse).
                    for b in books {
                        if b.path.exists() { &mut present } else { &mut missing }.push(b.id.clone());
                    }
                }
                // Droits, erreur réseau passagère : on ne conclut pas.
                Err(_) => report.unknown.extend(books.iter().map(|b| b.id.clone())),
            }
            progress.done.fetch_add(n, Ordering::Relaxed);
        }
        if missing.len() >= MASS_MISSING_MIN && missing.len() * 2 > count {
            report.blocked.push(Skipped {
                root: root.clone().unwrap_or_else(|| "(chemins relatifs)".into()),
                books: count,
                gone: missing.len(),
            });
            report.unknown.append(&mut missing);
        }
        report.missing.append(&mut missing);
        report.present.append(&mut present);
    }
    progress.done.store(items.len(), Ordering::Relaxed);
    report
}

// ---------------------------------------------------------------------------------------
// Recherche dans un dossier
// ---------------------------------------------------------------------------------------

/// Livre manquant à retrouver.
#[derive(Debug, Clone)]
pub struct Wanted {
    pub id: String,
    /// Chemin de la fiche (seul son nom de fichier sert).
    pub file: String,
    /// Taille connue du fichier (fiche), pour départager ou chercher par taille.
    pub size: Option<u64>,
}

/// Comment le fichier a été reconnu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Même nom et même taille : sûr.
    NameAndSize,
    /// Même nom, taille différente ou inconnue (fichier réécrit ?).
    Name,
    /// Même taille et même extension, nom différent (fichier renommé) : à vérifier.
    Size,
    /// Fichier choisi à la main.
    Chosen,
}

impl MatchKind {
    pub fn label(self) -> &'static str {
        match self {
            MatchKind::NameAndSize => "même nom et même taille",
            MatchKind::Name => "même nom, taille différente",
            MatchKind::Size => "même taille, nom différent (à vérifier)",
            MatchKind::Chosen => "choisi à la main",
        }
    }

    /// Coché d'office dans la fenêtre de confirmation ?
    pub fn sure(self) -> bool {
        !matches!(self, MatchKind::Size)
    }
}

/// Fichier proposé pour un livre manquant.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub path: PathBuf,
    pub size: u64,
    pub kind: MatchKind,
    /// Autres fichiers de même nom trouvés (le premier, dans l'ordre des chemins, est proposé).
    pub others: usize,
}

/// Bilan de [`search_folder`].
#[derive(Debug, Clone, Default)]
pub struct SearchReport {
    pub found: Vec<Candidate>,
    /// Livres non retrouvés.
    pub not_found: Vec<String>,
    /// Fichiers examinés.
    pub files_seen: usize,
    /// Dossiers illisibles.
    pub errors: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

/// Extension en minuscules.
fn ext(p: &str) -> String {
    Path::new(p).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn name_key(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_lowercase()
}

/// Cherche les fichiers des livres `wanted` dans `dir` et ses sous-dossiers (à appeler dans
/// un fil). Un même fichier n'est proposé qu'à un livre ; `exclude` : fichiers déjà dans la
/// bibliothèque (clés [`path_key`]), jamais proposés.
pub fn search_folder(
    dir: &Path,
    wanted: &[Wanted],
    exclude: &HashSet<String>,
    progress: &Progress,
    cancel: &AtomicBool,
) -> SearchReport {
    let mut report = SearchReport::default();
    progress.total.store(0, Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    // Parcours (pile, sans suivre les liens symboliques de dossiers).
    let mut files: Vec<(PathBuf, u64)> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return report;
        }
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) => {
                report.errors.push((d, e.to_string()));
                continue;
            }
        };
        for e in entries.flatten() {
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                if m.is_file() {
                    files.push((e.path(), m.len()));
                    progress.total.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    files.sort();
    files.retain(|(p, _)| !exclude.contains(&path_key(&p.to_string_lossy())));
    report.files_seen = files.len();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_size: HashMap<(u64, String), Vec<usize>> = HashMap::new();
    for (i, (p, size)) in files.iter().enumerate() {
        let s = p.to_string_lossy();
        by_name.entry(name_key(&s)).or_default().push(i);
        by_size.entry((*size, ext(&s))).or_default().push(i);
    }
    let mut claimed: HashSet<usize> = HashSet::new();
    // D'abord les correspondances par nom (sûres), puis par taille.
    let mut pending: Vec<&Wanted> = Vec::new();
    for w in wanted {
        let cands: Vec<usize> = by_name
            .get(&name_key(&w.file))
            .map(|v| v.iter().copied().filter(|i| !claimed.contains(i)).collect())
            .unwrap_or_default();
        let same_size = cands.iter().copied().find(|&i| Some(files[i].1) == w.size);
        let pick = same_size.map(|i| (i, MatchKind::NameAndSize)).or(cands.first().map(|&i| (i, MatchKind::Name)));
        match pick {
            Some((i, kind)) => {
                claimed.insert(i);
                report.found.push(Candidate {
                    id: w.id.clone(),
                    path: files[i].0.clone(),
                    size: files[i].1,
                    kind,
                    others: cands.len() - 1,
                });
            }
            None => pending.push(w),
        }
        progress.done.fetch_add(1, Ordering::Relaxed);
    }
    for w in pending {
        let unique = w.size.filter(|s| *s > 0).and_then(|s| {
            let v: Vec<usize> = by_size
                .get(&(s, ext(&w.file)))
                .map(|v| v.iter().copied().filter(|i| !claimed.contains(i)).collect())
                .unwrap_or_default();
            (v.len() == 1).then(|| v[0])
        });
        match unique {
            Some(i) => {
                claimed.insert(i);
                report.found.push(Candidate {
                    id: w.id.clone(),
                    path: files[i].0.clone(),
                    size: files[i].1,
                    kind: MatchKind::Size,
                    others: 0,
                });
            }
            None => report.not_found.push(w.id.clone()),
        }
    }
    report
}

/// Base ComicRust : écritures d'une relocalisation, comme un déplacement trouvé par
/// l'analyse des dossiers (`ScanReport::moved`) : seuls le chemin, la taille et les dates
/// du fichier changent (et `missing` repasse à NULL) ; lecture, note, infos gardées. À
/// passer à [`crate::store::Store::apply_scan`].
pub fn relocation_report(
    relocations: &[(crate::library::LibraryBook, PathBuf)],
) -> Result<crate::scanner::ScanReport, crate::Error> {
    let mut report = crate::scanner::ScanReport::default();
    for (book, path) in relocations {
        let meta = std::fs::metadata(path)
            .map_err(|e| crate::Error::Io(std::io::Error::new(e.kind(), format!("{} : {e}", path.display()))))?;
        let mut b = book.clone();
        b.file = Some(path.to_string_lossy().into_owned());
        crate::scanner::set_file_props(&mut b, &meta);
        b.missing = None;
        report.moved.push(b.id.clone());
        report.updated.push(b);
    }
    Ok(report)
}

/// Correspondance d'un fichier choisi à la main pour un livre : comment il se rapproche de
/// l'ancien (nom, taille).
pub fn chosen_kind(old_file: &str, old_size: Option<u64>, new: &Path, new_size: u64) -> MatchKind {
    let same_name = name_key(old_file) == name_key(&new.to_string_lossy());
    match (same_name, old_size == Some(new_size)) {
        (true, true) => MatchKind::NameAndSize,
        (true, false) => MatchKind::Name,
        (false, true) => MatchKind::Size,
        (false, false) => MatchKind::Chosen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_roots() {
        let r = |p: &str| volume_root(p);
        assert_eq!(
            r(r"\\SERVEURNAS\Disque_DATA_1\_BDZ\_BD\Séries_Temp\29 - Marvel Team-Up #55.cbz").as_deref(),
            Some(r"\\SERVEURNAS\Disque_DATA_1")
        );
        assert_eq!(r("//nas/bd/a.cbz").as_deref(), Some("//nas/bd"));
        assert_eq!(r(r"z:\BD\a.cbz").as_deref(), Some(r"Z:\"));
        assert_eq!(r("/mnt/nas/bd/a.cbz").as_deref(), Some("/mnt/nas"));
        assert_eq!(r("/bd/a.cbz").as_deref(), Some("/bd"));
        assert_eq!(r("/a.cbz").as_deref(), Some("/"));
        assert_eq!(r("bd/a.cbz"), None);
    }

    fn touch(p: &Path, bytes: usize) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![0u8; bytes]).unwrap();
    }

    fn item(id: &str, p: PathBuf) -> CheckItem {
        CheckItem { id: id.into(), path: p }
    }

    #[test]
    fn check_marks_missing_but_never_an_unreachable_volume() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        touch(&d.join("Séries/a.cbz"), 10);
        touch(&d.join("Séries/B.CBZ"), 10);
        // Fichier en cours de réécriture par ComicRack (`.tmp`) : pas manquant.
        touch(&d.join("Séries/c.cbz.tmp"), 10);
        let items = vec![
            item("a", d.join("Séries/a.cbz")),
            item("b", d.join("Séries/b.cbz")), // casse différente : présent (Windows) ou non
            item("c", d.join("Séries/c.cbz")),
            item("x", d.join("Séries/x.cbz")),
            item("y", d.join("Disparu/y.cbz")),
            item("nas", PathBuf::from("/nas-absent-comicrust/partage/z.cbz")),
            item("vide", PathBuf::new()),
        ];
        let p = Progress::default();
        let r = check_files(&items, &p, &AtomicBool::new(false));
        assert!(r.present.contains(&"a".into()) && r.present.contains(&"c".into()));
        assert!(r.present.contains(&"b".into()), "nom trouvé sans casse dans le dossier");
        let mut m = r.missing.clone();
        m.sort();
        assert_eq!(m, ["x", "y"]);
        assert_eq!(
            r.unreachable,
            [Skipped { root: "/nas-absent-comicrust/partage".into(), books: 1, gone: 0 }],
            "NAS injoignable : rien de marqué"
        );
        assert_eq!(r.unknown, ["vide"]);
        assert_eq!(p.get(), (7, 7));
        assert!(r.summary().contains("2 fichier(s) manquant(s) sur 5"), "{}", r.summary());
        assert!(r.summary().contains("inaccessible"), "{}", r.summary());
    }

    #[test]
    fn check_blocks_a_mass_disappearance() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let mut items = Vec::new();
        for i in 0..30 {
            if i < 5 {
                touch(&d.join(format!("bd/{i}.cbz")), 1);
            }
            items.push(item(&i.to_string(), d.join(format!("bd/{i}.cbz"))));
        }
        let r = check_files(&items, &Progress::default(), &AtomicBool::new(false));
        assert!(r.missing.is_empty(), "25 sur 30 disparus d'un coup : rien de marqué");
        assert_eq!(r.blocked.len(), 1);
        assert_eq!((r.blocked[0].books, r.blocked[0].gone), (30, 25));
        assert_eq!(r.present.len(), 5);
        let cancel = AtomicBool::new(true);
        assert!(check_files(&items, &Progress::default(), &cancel).cancelled);
    }

    #[test]
    fn search_by_name_then_size() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        touch(&d.join("Rangés/Marvel/29 - Marvel Team-Up #55.cbz"), 100);
        touch(&d.join("Autre/29 - marvel team-up #55.cbz"), 99); // même nom, autre taille
        touch(&d.join("Rangés/Thanos 01.cbz"), 50); // taille différente de la fiche
        touch(&d.join("Rangés/renommé.cbr"), 777); // même taille et extension que « vieux.cbr »
        touch(&d.join("Rangés/x.cbz"), 777); // même taille, autre extension
        touch(&d.join("Rangés/déjà.cbz"), 5); // déjà dans la bibliothèque
        let wanted = vec![
            Wanted { id: "mtu".into(), file: r"\\SERVEURNAS\BD\29 - Marvel Team-Up #55.cbz".into(), size: Some(100) },
            Wanted { id: "thanos".into(), file: r"Z:\BD\Thanos 01.cbz".into(), size: Some(51) },
            Wanted { id: "vieux".into(), file: "/bd/vieux.cbr".into(), size: Some(777) },
            Wanted { id: "deja".into(), file: "/bd/déjà.cbz".into(), size: Some(5) },
            Wanted { id: "absent".into(), file: "/bd/absent.cbz".into(), size: Some(12345) },
        ];
        let exclude: HashSet<String> = [path_key(&d.join("Rangés/déjà.cbz").to_string_lossy())].into();
        let r = search_folder(d, &wanted, &exclude, &Progress::default(), &AtomicBool::new(false));
        let get = |id: &str| r.found.iter().find(|c| c.id == id).cloned();
        let mtu = get("mtu").unwrap();
        assert_eq!((mtu.kind, mtu.others), (MatchKind::NameAndSize, 1));
        assert!(mtu.path.ends_with("Rangés/Marvel/29 - Marvel Team-Up #55.cbz"));
        assert_eq!(get("thanos").unwrap().kind, MatchKind::Name);
        let v = get("vieux").unwrap();
        assert_eq!((v.kind, v.path.file_name().unwrap().to_str().unwrap()), (MatchKind::Size, "renommé.cbr"));
        assert!(!v.kind.sure() && mtu.kind.sure());
        assert!(get("deja").is_none(), "fichier déjà dans la bibliothèque : jamais proposé");
        let mut nf = r.not_found.clone();
        nf.sort();
        assert_eq!(nf, ["absent", "deja"]);
        assert_eq!(r.files_seen, 5);
        // Le même fichier n'est proposé qu'une fois.
        let twice = vec![
            Wanted { id: "1".into(), file: "/a/Thanos 01.cbz".into(), size: None },
            Wanted { id: "2".into(), file: "/b/Thanos 01.cbz".into(), size: None },
        ];
        let r = search_folder(d, &twice, &HashSet::new(), &Progress::default(), &AtomicBool::new(false));
        assert_eq!((r.found.len(), r.not_found.as_slice()), (1, ["2".to_string()].as_slice()));
    }

    /// Base ComicRust (SQLite) : relocalisation (lecture et note gardées, plus manquant) et
    /// retrait de la bibliothèque (fiche seulement : le fichier reste), vus par une autre
    /// instance mais pas par notre propre surveillance.
    #[test]
    fn store_relocate_and_remove() {
        use crate::library::LibraryBook;
        use crate::store::{Store, StoreConfig, StoreWatcher};
        let dir = tempfile::tempdir().unwrap();
        let cfg = StoreConfig::Sqlite(dir.path().join("comicrust.db"));
        let mut st = Store::open(&cfg).unwrap();
        let new_file = dir.path().join("Rangés/Thanos 01.cbz");
        let kept_file = dir.path().join("Thanos 02.cbz");
        touch(&new_file, 321);
        touch(&kept_file, 9);
        let a = LibraryBook {
            id: "a".into(),
            series: Some("Thanos".into()),
            file: Some(dir.path().join("Ancien/Thanos 01.cbz").to_string_lossy().into_owned()),
            missing: Some(true),
            current_page: Some(5),
            last_page_read: Some(7),
            rating: Some(4.0),
            notes: Some("prêté à Océane".into()),
            ..Default::default()
        };
        let b =
            LibraryBook { id: "b".into(), file: Some(kept_file.to_string_lossy().into_owned()), ..Default::default() };
        st.upsert(&[a.clone(), b]).unwrap();
        let db = st.load(None).unwrap();
        let mut w_mine = StoreWatcher::new(cfg.clone(), &db);
        let mut w_other = StoreWatcher::new(cfg.clone(), &db);

        let report = relocation_report(&[(a.clone(), new_file.clone())]).unwrap();
        let w = st.apply_scan(&report).unwrap();
        w_mine.note_written(w.books.iter().map(|b| b.id.as_str()), w.counter);
        let got = st.get(&["a".into()]).unwrap().remove(0);
        assert_eq!(got.file.as_deref(), Some(new_file.to_string_lossy().as_ref()));
        assert_eq!((got.missing, got.file_size), (None, Some(321)));
        assert_eq!((got.current_page, got.last_page_read, got.rating), (Some(5), Some(7), Some(4.0)));
        assert_eq!(got.notes.as_deref(), Some("prêté à Océane"));
        assert!(relocation_report(&[(a, dir.path().join("absent.cbz"))]).is_err());

        w_mine.note_deleted(["b"]);
        assert_eq!(st.delete(&["b".into()]).unwrap(), 1);
        assert!(kept_file.exists(), "jamais de fichier supprimé");
        assert!(w_mine.poll().unwrap().is_none(), "nos propres écritures");
        let ch = w_other.poll().unwrap().unwrap();
        let mut other = db.clone();
        let applied = other.apply(ch);
        assert_eq!((applied.modified, applied.removed), (1, 1));
        assert_eq!(st.count().unwrap(), 1);
    }

    #[test]
    fn chosen_file_kind() {
        let p = Path::new("/x/Thanos 01.cbz");
        assert_eq!(chosen_kind(r"Z:\BD\thanos 01.CBZ", Some(5), p, 5), MatchKind::NameAndSize);
        assert_eq!(chosen_kind("/bd/Thanos 01.cbz", Some(4), p, 5), MatchKind::Name);
        assert_eq!(chosen_kind("/bd/autre.cbz", Some(5), p, 5), MatchKind::Size);
        assert_eq!(chosen_kind("/bd/autre.cbz", None, p, 5), MatchKind::Chosen);
    }
}
