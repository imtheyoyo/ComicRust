//! Livres « manquants » : fiche présente dans la bibliothèque, fichier absent du disque.
//!
//! Correspondance avec ComicRack (`ComicBook.RefreshFileProperties`) : `FileIsMissing =
//! !fileInfo.Exists` (élément `<Missing>` de la fiche, critère « Est manquant » des listes
//! intelligentes, vignette barrée `DeletedStateImage`). ComicRack ne vérifie l'existence
//! qu'en relisant le fichier ; ComicRust la vérifie ici **en arrière-plan**, par dossier :
//!
//! - [`check_files`] : un `read_dir` par dossier (une seule requête réseau pour tous les
//!   livres d'un dossier du NAS). Les noms sont comparés **après normalisation Unicode
//!   NFC** (un NAS macOS ou certains Samba rendent du NFD : « é » en deux caractères) ;
//!   sans distinction de casse sous Windows et macOS seulement (sous Linux, `a.cbz` et
//!   `A.CBZ` sont deux fichiers). Un nom non trouvé est encore vérifié par `exists()`
//!   (système de fichiers insensible à la casse monté sous Linux…). Un fichier renommé en
//!   `.tmp` par ComicRack le temps de l'enregistrer n'est pas manquant.
//! - **Garde-fous** (comme ceux de l'analyse des dossiers, `scanner`) :
//!   - un volume injoignable (partage `\\SERVEUR\partage` absent, lecteur `Z:` non
//!     connecté, point de montage absent) ne marque **rien** et est signalé ;
//!   - le volume est **retesté après la vérification** : s'il a décroché entre-temps, aucun
//!     de ses manquants n'est retenu ; une erreur réseau sur un dossier (Windows :
//!     `ERROR_BAD_NETPATH`, `ERROR_NETNAME_DELETED`… — voir [`classify_io_error`], qui
//!     regarde le code système avant `ErrorKind`, car Rust range certaines de ces erreurs
//!     dans `NotFound`) arrête la vérification du volume, qui est alors injoignable ;
//!   - un dossier n'est déclaré disparu que si un de ses dossiers parents (dans le volume)
//!     est lisible ; sinon le volume est injoignable ;
//!   - disparition massive ([`crate::mass_guard`], règle commune avec l'analyse des
//!     dossiers) : par dossier surveillé (à défaut, racine du volume) et par sous-dossier
//!     de premier niveau ; les livres déjà manquants avant la vérification ne comptent pas.
//! - [`search_folder`] : « Rechercher les fichiers manquants dans un dossier… » : par nom
//!   de fichier (sans casse), départagé par la taille puis par la ressemblance du dossier
//!   (dossier de la série), puis à défaut par taille seule (même extension, candidat
//!   unique), dans le dossier et ses sous-dossiers ; résultat à faire confirmer. Les
//!   fichiers déjà dans la bibliothèque, y compris sous un autre chemin du même partage
//!   (`Z:\` ↔ `\\SERVEUR\…`, lien symbolique : identité physique des dossiers), ne sont
//!   jamais proposés.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use unicode_normalization::UnicodeNormalization;

use crate::mass_guard::MassGuard;
pub use crate::mass_guard::{MASS_MISSING_ABS, MASS_MISSING_MIN};
use crate::scanner::{is_under, path_key};

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

/// Livre à vérifier : identifiant et fichier réel à examiner (le CBZ converti si
/// l'original a été rangé, voir l'appelant).
#[derive(Debug, Clone)]
pub struct CheckItem {
    pub id: String,
    pub path: PathBuf,
    /// Déjà marqué manquant avant la vérification : ne compte pas comme une disparition
    /// « d'un coup » pour le garde-fou.
    pub was_missing: bool,
}

/// Volume injoignable ou disparition massive : livres non marqués.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Racine du volume (`\\SERVEUR\partage`, `Z:\`, `/mnt/nas`), ou dossier bloqué
    /// (disparition massive : dossier surveillé ou sous-dossier de premier niveau).
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
    /// Volumes injoignables (NAS éteint, lecteur réseau déconnecté, décroché pendant la
    /// vérification) : rien de marqué.
    pub unreachable: Vec<Skipped>,
    /// Disparitions massives : rien de marqué pour ces dossiers.
    pub blocked: Vec<Skipped>,
    /// Livres sans fichier, de dossier illisible (droits), d'un volume injoignable ou
    /// d'une disparition massive : ni présents ni manquants.
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

    /// Bilan sûr : rien d'injoignable, de bloqué ni d'indéterminé, et vérification menée à
    /// son terme.
    pub fn is_conclusive(&self) -> bool {
        self.unreachable.is_empty() && self.blocked.is_empty() && self.unknown.is_empty() && !self.cancelled
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

/// Nature d'une erreur d'accès à un dossier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoClass {
    /// Le dossier n'existe pas (le volume répond).
    NotFound,
    /// Erreur réseau : partage décroché, serveur absent, délai dépassé.
    Unreachable,
    /// Autre (droits…) : on ne conclut pas.
    Other,
}

/// Codes d'erreur Windows d'un partage réseau qui ne répond plus. Rust en range plusieurs
/// dans `ErrorKind::NotFound` (`ERROR_BAD_NETPATH`, `ERROR_BAD_NET_NAME`) : sans ce
/// classement, un NAS qui décroche ferait passer des dossiers entiers pour « disparus ».
pub const WINDOWS_NETWORK_ERRORS: &[(i32, &str)] = &[
    (51, "ERROR_REM_NOT_LIST"),
    (53, "ERROR_BAD_NETPATH"),
    (54, "ERROR_NETWORK_BUSY"),
    (55, "ERROR_DEV_NOT_EXIST"),
    (58, "ERROR_BAD_NET_RESP"),
    (59, "ERROR_UNEXP_NET_ERR"),
    (64, "ERROR_NETNAME_DELETED"),
    (67, "ERROR_BAD_NET_NAME"),
    (121, "ERROR_SEM_TIMEOUT"),
    (1203, "ERROR_NO_NET_OR_BAD_PATH"),
    (1222, "ERROR_NO_NETWORK"),
    (1231, "ERROR_NETWORK_UNREACHABLE"),
    (1232, "ERROR_HOST_UNREACHABLE"),
    (1236, "ERROR_CONNECTION_ABORTED"),
    (1311, "ERROR_NO_LOGON_SERVERS"),
    (2250, "ERROR_NOT_CONNECTED"),
];

/// Codes `errno` de Linux d'un partage réseau qui ne répond plus (certains n'ont pas
/// d'`ErrorKind` dédié : `EHOSTDOWN`, `ENOLINK`, `ECOMM`, `EREMOTEIO`).
pub const LINUX_NETWORK_ERRORS: &[(i32, &str)] = &[
    (67, "ENOLINK"),
    (70, "ECOMM"),
    (100, "ENETDOWN"),
    (101, "ENETUNREACH"),
    (103, "ECONNABORTED"),
    (104, "ECONNRESET"),
    (107, "ENOTCONN"),
    (110, "ETIMEDOUT"),
    (112, "EHOSTDOWN"),
    (113, "EHOSTUNREACH"),
    (116, "ESTALE"),
    (121, "EREMOTEIO"),
];

/// Classement d'une erreur d'après son code système (`raw`, interprété comme un code
/// Windows si `windows`, comme un `errno` de Linux sinon sous Linux) puis son `ErrorKind`.
/// Voir [`classify_io_error`].
pub fn classify_error_code(raw: Option<i32>, kind: io::ErrorKind, windows: bool) -> IoClass {
    use io::ErrorKind as K;
    let list = if windows {
        WINDOWS_NETWORK_ERRORS
    } else if cfg!(target_os = "linux") {
        LINUX_NETWORK_ERRORS
    } else {
        &[]
    };
    if raw.is_some_and(|c| list.iter().any(|(n, _)| *n == c)) {
        return IoClass::Unreachable;
    }
    match kind {
        K::NotFound => IoClass::NotFound,
        K::TimedOut
        | K::HostUnreachable
        | K::NetworkUnreachable
        | K::NetworkDown
        | K::ConnectionReset
        | K::ConnectionAborted
        | K::NotConnected
        | K::StaleNetworkFileHandle => IoClass::Unreachable,
        _ => IoClass::Other,
    }
}

/// Classement d'une erreur d'accès à un dossier : code Windows réseau (quel que soit
/// `ErrorKind`) → injoignable ; sinon d'après `ErrorKind` (délai, réseau, `ESTALE`…).
pub fn classify_io_error(e: &io::Error) -> IoClass {
    classify_error_code(e.raw_os_error(), e.kind(), cfg!(windows))
}

/// Accès au disque de [`check_files_with`] (remplacé dans les tests pour simuler un NAS
/// qui décroche).
pub trait CheckFs {
    /// Noms des entrées d'un dossier.
    fn list(&self, dir: &Path) -> io::Result<Vec<OsString>>;
    fn exists(&self, path: &Path) -> bool;
}

/// Le vrai disque.
pub struct RealFs;

impl CheckFs for RealFs {
    fn list(&self, dir: &Path) -> io::Result<Vec<OsString>> {
        Ok(std::fs::read_dir(dir)?.flatten().map(|e| e.file_name()).collect())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// Clé de comparaison d'un nom de fichier : NFC, et minuscules là où le système ne
/// distingue pas la casse (Windows, macOS).
fn name_cmp_key(name: &str) -> String {
    let n: String = name.nfc().collect();
    if cfg!(any(windows, target_os = "macos")) {
        n.to_lowercase()
    } else {
        n
    }
}

/// Fichier réel d'un livre d'après le chemin de sa fiche : le CBZ converti par ComicRust
/// si l'original a été rangé (`converted` : réglages), sinon le chemin lui-même. Utilisé par
/// la vérification et par la fenêtre « Retirer de la bibliothèque ».
pub fn real_file(file: &str, converted: &HashMap<PathBuf, PathBuf>) -> PathBuf {
    let p = PathBuf::from(file);
    converted.get(&p).cloned().unwrap_or(p)
}

/// Dossier surveillé le plus précis qui contient `path` (texte d'origine), sinon la racine
/// du volume ; `""` pour un chemin relatif hors de tout dossier surveillé.
fn guard_root(path: &str, folders: &[(String, String)]) -> String {
    let key = path_key(path);
    folders
        .iter()
        .filter(|(k, _)| is_under(&key, k))
        .max_by_key(|(k, _)| k.len())
        .map(|(_, f)| f.clone())
        .or_else(|| volume_root(path))
        .unwrap_or_default()
}

/// Vérifie l'existence des fichiers (à appeler dans un fil : peut prendre du temps sur un
/// NAS). `folders` : dossiers surveillés (pour le garde-fou « disparition massive » ; vide :
/// par racine de volume). Voir l'en-tête du module pour les garde-fous.
pub fn check_files(items: &[CheckItem], folders: &[PathBuf], progress: &Progress, cancel: &AtomicBool) -> CheckReport {
    check_files_with(&RealFs, items, folders, progress, cancel)
}

/// [`check_files`] avec un accès au disque donné.
pub fn check_files_with(
    fs: &dyn CheckFs,
    items: &[CheckItem],
    folders: &[PathBuf],
    progress: &Progress,
    cancel: &AtomicBool,
) -> CheckReport {
    let mut report = CheckReport::default();
    progress.total.store(items.len(), Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    let folders: Vec<(String, String)> = folders
        .iter()
        .map(|f| {
            let t = f.to_string_lossy().into_owned();
            (path_key(&t), t)
        })
        .collect();
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
        let root_path = root.as_deref().map(Path::new);
        if let Some(r) = root.as_deref().filter(|r| fs.list(Path::new(r)).is_err()) {
            report.unreachable.push(Skipped { root: r.to_string(), books: count, gone: 0 });
            report.unknown.extend(dirs.values().flatten().map(|b| b.id.clone()));
            progress.done.fetch_add(count, Ordering::Relaxed);
            continue;
        }
        let (mut missing, mut present): (Vec<&CheckItem>, Vec<&CheckItem>) = (Vec::new(), Vec::new());
        let mut undecided: Vec<&CheckItem> = Vec::new();
        // Dossiers déjà lus (pour les dossiers parents d'un dossier disparu).
        let mut readable: HashMap<PathBuf, bool> = HashMap::new();
        let mut lost = false;
        let mut dirs: Vec<_> = dirs.into_iter().collect();
        dirs.sort_by(|a, b| a.0.cmp(&b.0));
        for (dir, books) in dirs {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                return report;
            }
            let n = books.len();
            if lost {
                undecided.extend(books);
                progress.done.fetch_add(n, Ordering::Relaxed);
                continue;
            }
            match fs.list(&dir) {
                Ok(entries) => {
                    readable.insert(dir.clone(), true);
                    let names: HashSet<String> = entries.iter().map(|e| name_cmp_key(&e.to_string_lossy())).collect();
                    for b in books {
                        let name = b.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let here = names.contains(&name_cmp_key(&name))
                            || names.contains(&name_cmp_key(&format!("{name}.tmp")))
                            || fs.exists(&b.path);
                        if here { &mut present } else { &mut missing }.push(b);
                    }
                }
                Err(e) => match classify_io_error(&e) {
                    IoClass::Unreachable => {
                        lost = true;
                        undecided.extend(books);
                    }
                    IoClass::NotFound => {
                        // Dossier disparu (renommé, déplacé) : seulement si un dossier parent
                        // du volume répond ; sinon le volume est injoignable.
                        match parent_readable(fs, &dir, root_path, &mut readable) {
                            Some(true) => {
                                for b in books {
                                    if fs.exists(&b.path) { &mut present } else { &mut missing }.push(b);
                                }
                            }
                            Some(false) => report.unknown.extend(books.iter().map(|b| b.id.clone())),
                            None => {
                                lost = true;
                                undecided.extend(books);
                            }
                        }
                    }
                    // Droits : on ne conclut pas.
                    IoClass::Other => report.unknown.extend(books.iter().map(|b| b.id.clone())),
                },
            }
            progress.done.fetch_add(n, Ordering::Relaxed);
        }
        // Le volume répond-il encore ? (NAS qui décroche pendant la vérification : ses
        // derniers dossiers auraient paru disparus.)
        if !lost {
            if let Some(r) = root_path {
                lost = fs.list(r).is_err();
            }
        }
        if lost {
            let r = root.clone().unwrap_or_default();
            let books = missing.len() + undecided.len();
            report.unreachable.push(Skipped { root: r, books, gone: 0 });
            report.unknown.extend(missing.drain(..).chain(undecided.drain(..)).map(|b| b.id.clone()));
        }
        // Garde-fou « disparition massive » (règle commune avec l'analyse des dossiers).
        let mut guard = MassGuard::new();
        let roots: HashMap<&str, String> = missing
            .iter()
            .chain(&present)
            .map(|b| (b.id.as_str(), guard_root(&b.path.to_string_lossy(), &folders)))
            .collect();
        for b in missing.iter().chain(&present).filter(|b| !b.was_missing) {
            guard.add_known(&roots[b.id.as_str()], &b.path.to_string_lossy());
        }
        for b in missing.iter().filter(|b| !b.was_missing) {
            guard.add_gone(&roots[b.id.as_str()], &b.path.to_string_lossy());
        }
        for blk in guard.blocked() {
            report.blocked.push(Skipped { root: blk.label, books: blk.known, gone: blk.gone });
        }
        let (blocked, ok): (Vec<&CheckItem>, Vec<&CheckItem>) = missing
            .into_iter()
            .partition(|b| !b.was_missing && guard.is_blocked(&roots[b.id.as_str()], &b.path.to_string_lossy()));
        report.unknown.extend(blocked.iter().map(|b| b.id.clone()));
        report.missing.extend(ok.iter().map(|b| b.id.clone()));
        report.present.extend(present.iter().map(|b| b.id.clone()));
    }
    progress.done.store(items.len(), Ordering::Relaxed);
    report
}

/// Un dossier parent de `dir` (jusqu'à la racine du volume comprise) est-il lisible ?
/// `Some(true)` : oui (le dossier a donc vraiment disparu) ; `Some(false)` : un parent
/// refuse l'accès (droits) ; `None` : erreur réseau, ou plus rien ne répond (volume
/// injoignable).
fn parent_readable(
    fs: &dyn CheckFs,
    dir: &Path,
    root: Option<&Path>,
    readable: &mut HashMap<PathBuf, bool>,
) -> Option<bool> {
    let mut cur = dir.parent();
    while let Some(p) = cur.filter(|p| !p.as_os_str().is_empty()) {
        if let Some(&ok) = readable.get(p) {
            if ok {
                return Some(true);
            }
        } else {
            match fs.list(p) {
                Ok(_) => {
                    readable.insert(p.to_path_buf(), true);
                    return Some(true);
                }
                Err(e) => match classify_io_error(&e) {
                    IoClass::NotFound => {
                        readable.insert(p.to_path_buf(), false);
                    }
                    IoClass::Unreachable => return None,
                    IoClass::Other => return Some(false),
                },
            }
        }
        if root.is_some_and(|r| path_key(&p.to_string_lossy()) == path_key(&r.to_string_lossy())) {
            break;
        }
        cur = p.parent();
    }
    // Chemin relatif sans parent lisible, ou volume muet.
    None
}

// ---------------------------------------------------------------------------------------
// Recherche dans un dossier
// ---------------------------------------------------------------------------------------

/// Livre manquant à retrouver.
#[derive(Debug, Clone)]
pub struct Wanted {
    pub id: String,
    /// Chemin de la fiche : son nom de fichier, et ses dossiers pour départager plusieurs
    /// fichiers de même nom.
    pub file: String,
    /// Taille connue du fichier (fiche), pour départager ou chercher par taille.
    pub size: Option<u64>,
}

/// Comment le fichier a été reconnu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Même nom et même taille : sûr.
    NameAndSize,
    /// Même nom, taille de la fiche inconnue.
    Name,
    /// Même nom, taille différente de celle de la fiche (fichier réécrit ? autre album de
    /// même nom ?) : à vérifier.
    NameOtherSize,
    /// Même taille et même extension, nom différent (fichier renommé) : à vérifier.
    Size,
    /// Fichier choisi à la main.
    Chosen,
}

impl MatchKind {
    pub fn label(self) -> &'static str {
        match self {
            MatchKind::NameAndSize => "même nom et même taille",
            MatchKind::Name => "même nom (taille de la fiche inconnue)",
            MatchKind::NameOtherSize => "même nom, taille différente (à vérifier)",
            MatchKind::Size => "même taille, nom différent (à vérifier)",
            MatchKind::Chosen => "choisi à la main",
        }
    }

    /// La correspondance suffit-elle, à elle seule, pour cocher d'office ? (Voir aussi
    /// [`Candidate::sure`] : jamais quand plusieurs fichiers conviennent.)
    pub fn sure(self) -> bool {
        matches!(self, MatchKind::NameAndSize | MatchKind::Name | MatchKind::Chosen)
    }
}

/// Fichier proposé pour un livre manquant.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub path: PathBuf,
    pub size: u64,
    pub kind: MatchKind,
    /// Autres fichiers de même nom trouvés (le plus ressemblant est proposé : même taille,
    /// puis dossier le plus proche de l'ancien, puis taille la plus proche).
    pub others: usize,
}

impl Candidate {
    /// Coché d'office dans la fenêtre de confirmation : correspondance sûre **et** fichier
    /// unique (« Tome 01.cbz » trouvé dans plusieurs séries : à choisir soi-même).
    pub fn sure(&self) -> bool {
        self.kind.sure() && self.others == 0
    }
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

/// Clé du nom de fichier d'un chemin (séparateurs `\` et `/`) : NFC (comme
/// [`check_files`] : un nom écrit en NFD par macOS ou un NAS reste le même nom), minuscules.
pub fn name_key(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).nfc().collect::<String>().to_lowercase()
}

/// Dossiers parents d'un chemin, du plus proche au plus lointain (minuscules, texte).
fn parent_names(p: &str) -> Vec<String> {
    let mut parts: Vec<String> = p.split(['/', '\\']).filter(|c| !c.is_empty()).map(str::to_lowercase).collect();
    parts.pop(); // le fichier
    parts.reverse();
    parts
}

/// Ressemblance du dossier d'un candidat avec l'ancien dossier du livre (dossier de la
/// série) : dossiers parents de même nom, le plus proche comptant le plus. 0 : aucune.
fn folder_likeness(old: &str, new: &Path) -> usize {
    let old = parent_names(old);
    let new = parent_names(&new.to_string_lossy());
    let mut score = 0;
    for (i, o) in old.iter().take(3).enumerate() {
        let weight = 3 - i; // dossier de la série : 3, au-dessus : 2, puis 1
        if new.iter().take(3).any(|n| n == o) {
            score += weight * 2;
        } else if new.first().is_some_and(|n| n.contains(o.as_str()) || o.contains(n.as_str())) {
            score += weight;
        }
    }
    score
}

/// Fichiers déjà dans la bibliothèque, pour ne jamais les proposer : par chemin
/// ([`path_key`]) et, pour un chemin écrit autrement (`Z:\BD` ↔ `\\SERVEUR\BD`, lien
/// symbolique), par **identité physique du dossier** (crate `same-file`, comme le transfert
/// depuis ComicRack) avec le même nom de fichier.
pub struct KnownFiles {
    keys: HashSet<String>,
    /// Nom de fichier (minuscules) → dossiers des fichiers de ce nom.
    by_name: HashMap<String, Vec<PathBuf>>,
    /// Identité de chaque dossier déjà ouvert (`None` : inaccessible).
    handles: std::cell::RefCell<HashMap<PathBuf, Option<same_file::Handle>>>,
}

impl KnownFiles {
    /// `files` : chemins des fiches de la bibliothèque.
    pub fn new<'a>(files: impl IntoIterator<Item = &'a str>) -> Self {
        let mut keys = HashSet::new();
        let mut by_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for f in files.into_iter().filter(|f| !f.is_empty()) {
            keys.insert(path_key(f));
            let p = Path::new(f);
            if let Some(dir) = p.parent() {
                by_name.entry(name_key(f)).or_default().push(dir.to_path_buf());
            }
        }
        Self { keys, by_name, handles: Default::default() }
    }

    fn same_dir(&self, a: &Path, b: &Path) -> bool {
        if a == b {
            return true;
        }
        let mut h = self.handles.borrow_mut();
        for p in [a, b] {
            h.entry(p.to_path_buf()).or_insert_with(|| same_file::Handle::from_path(p).ok());
        }
        matches!((h.get(a), h.get(b)), (Some(Some(x)), Some(Some(y))) if x == y)
    }

    /// `path` est-il un fichier de la bibliothèque (même chemin, ou même fichier sous un
    /// autre chemin) ?
    pub fn contains(&self, path: &Path) -> bool {
        let text = path.to_string_lossy();
        if self.keys.contains(&path_key(&text)) {
            return true;
        }
        let (Some(dirs), Some(dir)) = (self.by_name.get(&name_key(&text)), path.parent()) else { return false };
        dirs.iter().any(|d| self.same_dir(dir, d))
    }
}

/// Fichier physiquement identique (`same-file`) ? `false` si l'un est inaccessible.
pub fn same_physical_file(a: &Path, b: &Path) -> bool {
    same_file::is_same_file(a, b).unwrap_or(false)
}

/// Cherche les fichiers des livres `wanted` dans `dir` et ses sous-dossiers (à appeler dans
/// un fil). Un même fichier n'est proposé qu'à un livre ; `known` : fichiers déjà dans la
/// bibliothèque, jamais proposés.
pub fn search_folder(
    dir: &Path,
    wanted: &[Wanted],
    known: &KnownFiles,
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
    files.retain(|(p, _)| !known.contains(p));
    report.files_seen = files.len();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_size: HashMap<(u64, String), Vec<usize>> = HashMap::new();
    for (i, (p, size)) in files.iter().enumerate() {
        let s = p.to_string_lossy();
        by_name.entry(name_key(&s)).or_default().push(i);
        by_size.entry((*size, ext(&s))).or_default().push(i);
    }
    let mut claimed: HashSet<usize> = HashSet::new();
    // D'abord les correspondances par nom, puis par taille.
    let mut pending: Vec<&Wanted> = Vec::new();
    for w in wanted {
        let cands: Vec<usize> = by_name
            .get(&name_key(&w.file))
            .map(|v| v.iter().copied().filter(|i| !claimed.contains(i)).collect())
            .unwrap_or_default();
        // Le plus ressemblant : même taille d'abord, puis dossier le plus proche de l'ancien
        // (dossier de la série), puis taille la plus proche, puis ordre des chemins.
        let pick = cands.iter().copied().min_by_key(|&i| {
            let (p, size) = &files[i];
            let same = Some(*size) == w.size;
            let diff = w.size.map_or(0, |s| s.abs_diff(*size));
            (!same, std::cmp::Reverse(folder_likeness(&w.file, p)), diff, i)
        });
        match pick {
            Some(i) => {
                let kind = match w.size {
                    Some(s) if s == files[i].1 => MatchKind::NameAndSize,
                    Some(_) => MatchKind::NameOtherSize,
                    None => MatchKind::Name,
                };
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
        (true, false) if old_size.is_some() => MatchKind::NameOtherSize,
        (true, false) => MatchKind::Name,
        (false, true) => MatchKind::Size,
        (false, false) => MatchKind::Chosen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn volume_roots() {
        let r = |p: &str| volume_root(p);
        assert_eq!(r(r"\\NAS\Partage\BD\Séries\29 - Marvel Team-Up #55.cbz").as_deref(), Some(r"\\NAS\Partage"));
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
        CheckItem { id: id.into(), path: p, was_missing: false }
    }

    fn check(items: &[CheckItem]) -> CheckReport {
        check_files(items, &[], &Progress::default(), &AtomicBool::new(false))
    }

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
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
            item("b", d.join("Séries/b.cbz")), // casse différente
            item("c", d.join("Séries/c.cbz")),
            item("x", d.join("Séries/x.cbz")),
            item("y", d.join("Disparu/y.cbz")),
            item("nas", PathBuf::from("/nas-absent-comicrust/partage/z.cbz")),
            item("vide", PathBuf::new()),
        ];
        let p = Progress::default();
        let r = check_files(&items, &[], &p, &AtomicBool::new(false));
        assert!(r.present.contains(&"a".into()) && r.present.contains(&"c".into()));
        // Sous Windows et macOS, la casse ne compte pas ; sous Linux (système sensible à la
        // casse), `b.cbz` n'est pas `B.CBZ`.
        let case_insensitive = cfg!(any(windows, target_os = "macos")) || d.join("Séries/b.cbz").exists();
        let mut expected = vec!["x", "y"];
        if case_insensitive {
            assert!(r.present.contains(&"b".into()), "nom trouvé sans casse dans le dossier");
        } else {
            assert!(!r.present.contains(&"b".into()), "Linux : casse différente = autre fichier");
            expected.insert(0, "b");
        }
        assert_eq!(sorted(r.missing.clone()), expected);
        assert_eq!(
            r.unreachable,
            [Skipped { root: "/nas-absent-comicrust/partage".into(), books: 1, gone: 0 }],
            "NAS injoignable : rien de marqué"
        );
        assert_eq!(sorted(r.unknown.clone()), ["nas", "vide"]);
        assert!(!r.is_conclusive());
        assert_eq!(p.get(), (7, 7));
        assert!(r.summary().contains("manquant(s) sur 5"), "{}", r.summary());
        assert!(r.summary().contains("inaccessible"), "{}", r.summary());
    }

    /// Noms en NFD (NAS macOS, certains Samba) : comparés en NFC.
    #[test]
    fn unicode_names_are_normalized() {
        let dir = tempfile::tempdir().unwrap();
        let nfd = "Se\u{301}ries 01.cbz"; // « Séries » : e + accent combinant
        touch(&dir.path().join("bd").join(nfd), 1);
        let nfc = dir.path().join("bd").join("S\u{e9}ries 01.cbz");
        let r = check(&[item("a", nfc)]);
        assert_eq!((r.present.as_slice(), r.missing.len()), (["a".to_string()].as_slice(), 0));
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
        let r = check(&items);
        assert!(r.missing.is_empty(), "25 sur 30 disparus d'un coup : rien de marqué");
        assert_eq!(r.blocked.len(), 1);
        assert_eq!((r.blocked[0].books, r.blocked[0].gone), (30, 25));
        assert_eq!(r.present.len(), 5);
        assert_eq!(r.unknown.len(), 25);
        // Les livres déjà manquants ne comptent pas : 15 nouveaux disparus sur 20 présents.
        for it in items.iter_mut().skip(20) {
            it.was_missing = true;
        }
        let r = check(&items);
        assert!(r.blocked.is_empty(), "{:?}", r.blocked);
        assert_eq!(r.missing.len(), 25);
        let cancel = AtomicBool::new(true);
        assert!(check_files(&items, &[], &Progress::default(), &cancel).cancelled);
    }

    /// Disque simulé : dossiers et leurs fichiers, erreurs par dossier, et décrochage du
    /// volume après un nombre d'accès.
    #[derive(Default)]
    struct FakeFs {
        dirs: HashMap<PathBuf, Vec<String>>,
        errors: HashMap<PathBuf, io::ErrorKind>,
        /// Après ce nombre d'appels à `list`, tout échoue par délai dépassé.
        fail_after: Option<usize>,
        calls: Cell<usize>,
    }

    impl FakeFs {
        fn file(&mut self, p: &str) {
            let p = Path::new(p);
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            self.dirs.entry(p.parent().unwrap().to_path_buf()).or_default().push(name);
            // Dossiers parents (avec l'élément suivant comme entrée).
            let mut cur = p.parent();
            while let Some(c) = cur.filter(|c| c.parent().is_some()) {
                let parent = c.parent().unwrap().to_path_buf();
                let n = c.file_name().unwrap().to_string_lossy().into_owned();
                let e = self.dirs.entry(parent).or_default();
                if !e.contains(&n) {
                    e.push(n);
                }
                cur = c.parent();
            }
        }
    }

    impl CheckFs for FakeFs {
        fn list(&self, dir: &Path) -> io::Result<Vec<OsString>> {
            self.calls.set(self.calls.get() + 1);
            if self.fail_after.is_some_and(|n| self.calls.get() > n) {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "le NAS ne répond plus"));
            }
            if let Some(k) = self.errors.get(dir) {
                return Err(io::Error::new(*k, "erreur simulée"));
            }
            self.dirs
                .get(dir)
                .map(|v| v.iter().map(OsString::from).collect())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "absent"))
        }

        fn exists(&self, path: &Path) -> bool {
            let (Some(d), Some(n)) = (path.parent(), path.file_name()) else { return false };
            self.dirs.get(d).is_some_and(|v| v.iter().any(|x| std::ffi::OsStr::new(x) == n))
        }
    }

    fn fake_check(fs: &FakeFs, items: &[CheckItem], folders: &[&str]) -> CheckReport {
        let folders: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();
        check_files_with(fs, items, &folders, &Progress::default(), &AtomicBool::new(false))
    }

    /// Cinq séries de quatre tomes sous `/nas/bd` (tous présents sauf `absents`).
    fn nas(absents: &[&str]) -> (FakeFs, Vec<CheckItem>) {
        let mut fs = FakeFs::default();
        let mut items = Vec::new();
        for s in ["A", "B", "C", "D", "E"] {
            for t in 1..=4 {
                let p = format!("/nas/bd/{s}/{t}.cbz");
                if !absents.contains(&p.as_str()) {
                    fs.file(&p);
                }
                items.push(item(&format!("{s}{t}"), PathBuf::from(p)));
            }
        }
        (fs, items)
    }

    #[test]
    fn nas_dropping_during_the_check_marks_nothing() {
        // Le NAS décroche au 4e accès (racine, A et B lus ; C, D, E non).
        let (mut fs, items) = nas(&["/nas/bd/A/1.cbz"]);
        fs.fail_after = Some(3);
        let r = fake_check(&fs, &items, &[]);
        assert!(r.missing.is_empty(), "rien de retenu d'un volume qui a décroché : {:?}", r.missing);
        assert_eq!(r.unreachable.len(), 1);
        assert_eq!(r.unreachable[0].root, "/nas/bd");
        assert_eq!(r.present.len(), 7, "A2-A4 et B : vus présents");
        assert_eq!(r.unknown.len(), 13);

        // Tous les dossiers lus, mais le NAS ne répond plus au nouvel essai de la racine
        // (après la boucle) : les manquants ne sont pas retenus.
        let (mut fs, items) = nas(&["/nas/bd/E/4.cbz"]);
        fs.fail_after = Some(6);
        let r = fake_check(&fs, &items, &[]);
        assert!(r.missing.is_empty());
        assert_eq!((r.unreachable.len(), r.unknown.as_slice()), (1, ["E4".to_string()].as_slice()));
        assert_eq!(r.present.len(), 19);

        // Sans décrochage : E4 manquant.
        let (fs, items) = nas(&["/nas/bd/E/4.cbz"]);
        let r = fake_check(&fs, &items, &[]);
        assert_eq!(r.missing, ["E4"]);
        assert!(r.is_conclusive());
    }

    #[test]
    fn network_error_on_a_folder_makes_the_volume_unreachable() {
        let (mut fs, items) = nas(&[]);
        fs.errors.insert("/nas/bd/B".into(), io::ErrorKind::ConnectionReset);
        let r = fake_check(&fs, &items, &[]);
        assert!(r.missing.is_empty());
        assert_eq!(r.unreachable.len(), 1);
        assert_eq!(r.present.len(), 4, "A seulement : la vérification du volume s'arrête");
        // Droits refusés : ni présent ni manquant, la suite continue.
        let (mut fs, items) = nas(&[]);
        fs.errors.insert("/nas/bd/B".into(), io::ErrorKind::PermissionDenied);
        let r = fake_check(&fs, &items, &[]);
        assert_eq!((r.present.len(), r.unknown.len(), r.unreachable.len()), (16, 4, 0));
    }

    #[test]
    fn vanished_folder_needs_a_readable_parent() {
        // Série C supprimée en entier : son parent répond → manquants.
        let mut absents: Vec<String> = (1..=4).map(|t| format!("/nas/bd/C/{t}.cbz")).collect();
        let refs: Vec<&str> = absents.iter().map(String::as_str).collect();
        let (mut fs, items) = nas(&refs);
        fs.dirs.get_mut(Path::new("/nas/bd")).unwrap().retain(|n| n != "C");
        let r = fake_check(&fs, &items, &[]);
        assert_eq!(sorted(r.missing.clone()), ["C1", "C2", "C3", "C4"]);
        // Dossier introuvable dont le parent ne répond pas (délai) : injoignable, rien de marqué.
        absents = vec!["/nas/bd/x/Tome/1.cbz".into()];
        let mut fs = FakeFs::default();
        fs.file("/nas/bd/a.cbz");
        fs.errors.insert("/nas/bd/x".into(), io::ErrorKind::TimedOut);
        let items = vec![item("t", PathBuf::from(&absents[0])), item("a", "/nas/bd/a.cbz".into())];
        let r = fake_check(&fs, &items, &[]);
        assert!(r.missing.is_empty());
        assert_eq!((r.unreachable.len(), r.unknown.as_slice()), (1, ["t".to_string()].as_slice()));
    }

    #[test]
    fn windows_network_errors_are_unreachable_whatever_their_kind() {
        use io::ErrorKind as K;
        for (code, name) in WINDOWS_NETWORK_ERRORS {
            for kind in [K::NotFound, K::Other, K::PermissionDenied] {
                assert_eq!(classify_error_code(Some(*code), kind, true), IoClass::Unreachable, "{name}");
            }
        }
        for code in [53, 64, 1231] {
            // Hors Windows, ces nombres ne sont pas des codes réseau.
            assert_eq!(classify_error_code(Some(code), K::NotFound, false), IoClass::NotFound);
        }
        assert_eq!(classify_error_code(Some(2), K::NotFound, true), IoClass::NotFound, "ERROR_FILE_NOT_FOUND");
        assert_eq!(classify_error_code(Some(3), K::NotFound, true), IoClass::NotFound, "ERROR_PATH_NOT_FOUND");
        assert_eq!(classify_error_code(Some(5), K::PermissionDenied, true), IoClass::Other);
        assert_eq!(classify_error_code(None, K::TimedOut, false), IoClass::Unreachable);
        #[cfg(target_os = "linux")]
        {
            // ESTALE (partage NFS/CIFS périmé), EHOSTDOWN.
            assert_eq!(classify_io_error(&io::Error::from_raw_os_error(116)), IoClass::Unreachable);
            assert_eq!(classify_io_error(&io::Error::from_raw_os_error(112)), IoClass::Unreachable);
            assert_eq!(classify_io_error(&io::Error::from_raw_os_error(2)), IoClass::NotFound);
        }
    }

    /// Branche mergerfs absente : 2 000 livres sur 5 300 (20 séries entières) disparaissent
    /// sous le dossier surveillé : bloqué (seuil absolu), alors que c'est moins de la moitié.
    #[test]
    fn mergerfs_branch_missing_is_blocked() {
        let mut fs = FakeFs::default();
        let mut items = Vec::new();
        for s in 0..53 {
            for t in 0..100 {
                let p = format!("/srv/bd/Série {s:02}/Tome {t:03}.cbz");
                if s >= 20 {
                    fs.file(&p);
                }
                items.push(item(&format!("{s}-{t}"), PathBuf::from(p)));
            }
        }
        let r = fake_check(&fs, &items, &["/srv/bd"]);
        assert!(r.missing.is_empty(), "{} marqués", r.missing.len());
        assert_eq!(r.blocked, [Skipped { root: "/srv/bd".into(), books: 5300, gone: 2000 }]);
        assert_eq!((r.present.len(), r.unknown.len()), (3300, 2000));
        assert!(r.summary().contains("2000 livre(s) sur 5300"), "{}", r.summary());

        // Dossier de premier niveau vidé (« Marvel », 60 sur 60) dans un dossier surveillé
        // bien plus grand : bloqué pour ce sous-dossier seulement ; 3 suppressions ailleurs
        // restent marquées.
        let mut fs = FakeFs::default();
        let mut items = Vec::new();
        for (e, n) in [("Marvel", 60), ("DC", 300), ("Franco-belge", 400)] {
            for t in 0..n {
                let p = format!("/srv/bd/{e}/Série {}/{t}.cbz", t % 10);
                if e != "Marvel" && !(e == "DC" && t < 3) {
                    fs.file(&p);
                }
                items.push(item(&format!("{e}-{t}"), PathBuf::from(p)));
            }
        }
        let r = fake_check(&fs, &items, &["/srv/bd"]);
        assert_eq!(r.blocked, [Skipped { root: "/srv/bd/Marvel".into(), books: 60, gone: 60 }]);
        assert_eq!(sorted(r.missing.clone()), ["DC-0", "DC-1", "DC-2"]);
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
            Wanted { id: "mtu".into(), file: r"\\NAS\BD\29 - Marvel Team-Up #55.cbz".into(), size: Some(100) },
            Wanted { id: "thanos".into(), file: r"Z:\BD\Thanos 01.cbz".into(), size: Some(51) },
            Wanted { id: "vieux".into(), file: "/bd/vieux.cbr".into(), size: Some(777) },
            Wanted { id: "deja".into(), file: "/bd/déjà.cbz".into(), size: Some(5) },
            Wanted { id: "absent".into(), file: "/bd/absent.cbz".into(), size: Some(12345) },
        ];
        let lib = d.join("Rangés/déjà.cbz").to_string_lossy().into_owned();
        let known = KnownFiles::new([lib.as_str()]);
        let r = search_folder(d, &wanted, &known, &Progress::default(), &AtomicBool::new(false));
        let get = |id: &str| r.found.iter().find(|c| c.id == id).cloned();
        let mtu = get("mtu").unwrap();
        assert_eq!((mtu.kind, mtu.others), (MatchKind::NameAndSize, 1));
        assert!(mtu.path.ends_with("Rangés/Marvel/29 - Marvel Team-Up #55.cbz"));
        assert!(!mtu.sure(), "deux fichiers de même nom : pas coché d'office");
        let thanos = get("thanos").unwrap();
        assert_eq!(thanos.kind, MatchKind::NameOtherSize);
        assert!(!thanos.sure(), "taille connue et différente : pas coché d'office");
        let v = get("vieux").unwrap();
        assert_eq!((v.kind, v.path.file_name().unwrap().to_str().unwrap()), (MatchKind::Size, "renommé.cbr"));
        assert!(!v.sure());
        assert!(get("deja").is_none(), "fichier déjà dans la bibliothèque : jamais proposé");
        assert_eq!(sorted(r.not_found.clone()), ["absent", "deja"]);
        assert_eq!(r.files_seen, 5);
        // Le même fichier n'est proposé qu'une fois ; taille inconnue et fichier unique : sûr.
        let twice = vec![
            Wanted { id: "1".into(), file: "/a/Thanos 01.cbz".into(), size: None },
            Wanted { id: "2".into(), file: "/b/Thanos 01.cbz".into(), size: None },
        ];
        let r = search_folder(d, &twice, &KnownFiles::new([]), &Progress::default(), &AtomicBool::new(false));
        assert_eq!((r.found.len(), r.not_found.as_slice()), (1, ["2".to_string()].as_slice()));
        assert_eq!(r.found[0].kind, MatchKind::Name);
        assert!(r.found[0].sure());
    }

    /// « Tome 01.cbz » dans plusieurs séries : le candidat de même taille d'abord, sinon celui
    /// dont le dossier ressemble à l'ancien (dossier de la série), sinon la taille la plus
    /// proche ; jamais coché d'office.
    #[test]
    fn same_name_in_several_series() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        touch(&d.join("BD/Blacksad/Tome 01.cbz"), 300);
        touch(&d.join("BD/Thorgal/Tome 01.cbz"), 480);
        touch(&d.join("BD/Largo Winch/Tome 01.cbz"), 900);
        let search = |w: Wanted| {
            let r = search_folder(d, &[w], &KnownFiles::new([]), &Progress::default(), &AtomicBool::new(false));
            r.found.into_iter().next().unwrap()
        };
        let parent = |c: &Candidate| c.path.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned();
        // Même taille : celui-là, même dans une autre série.
        let c = search(Wanted { id: "a".into(), file: r"\\NAS\BD\Thorgal\Tome 01.cbz".into(), size: Some(900) });
        assert_eq!((parent(&c).as_str(), c.kind, c.others), ("Largo Winch", MatchKind::NameAndSize, 2));
        assert!(!c.sure());
        // Aucune taille identique : le dossier de la série.
        let c = search(Wanted { id: "a".into(), file: r"\\NAS\BD\Thorgal\Tome 01.cbz".into(), size: Some(500) });
        assert_eq!((parent(&c).as_str(), c.kind), ("Thorgal", MatchKind::NameOtherSize));
        let c = search(Wanted { id: "a".into(), file: r"Z:\Séries\blacksad\Tome 01.cbz".into(), size: None });
        assert_eq!((parent(&c).as_str(), c.kind), ("Blacksad", MatchKind::Name));
        assert!(!c.sure(), "plusieurs candidats");
        // Dossier sans rapport : la taille la plus proche.
        let c = search(Wanted { id: "a".into(), file: "/vieux/Tome 01.cbz".into(), size: Some(850) });
        assert_eq!(parent(&c), "Largo Winch");
    }

    /// Fichier déjà en bibliothèque sous un autre chemin du même dossier physique (lecteur
    /// réseau ↔ chemin UNC ; ici un lien symbolique) : jamais proposé.
    #[cfg(unix)]
    #[test]
    fn library_file_under_an_alias_is_not_proposed() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        touch(&d.join("réel/Série/Tome 01.cbz"), 10);
        touch(&d.join("réel/Série/Tome 02.cbz"), 20);
        std::os::unix::fs::symlink(d.join("réel"), d.join("alias")).unwrap();
        let lib = d.join("alias/Série/tome 01.cbz").to_string_lossy().into_owned();
        let known = KnownFiles::new([lib.as_str()]);
        assert!(known.contains(&d.join("réel/Série/Tome 01.cbz")));
        assert!(!known.contains(&d.join("réel/Série/Tome 02.cbz")));
        let wanted = vec![Wanted { id: "x".into(), file: "/bd/Série/Tome 01.cbz".into(), size: Some(10) }];
        let r = search_folder(&d.join("réel"), &wanted, &known, &Progress::default(), &AtomicBool::new(false));
        assert!(r.found.is_empty(), "{:?}", r.found);
        assert!(same_physical_file(&d.join("alias/Série/Tome 02.cbz"), &d.join("réel/Série/Tome 02.cbz")));
        assert!(!same_physical_file(&d.join("réel/Série/Tome 01.cbz"), &d.join("réel/Série/Tome 02.cbz")));
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
            notes: Some("prêté à Alex".into()),
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
        assert_eq!(got.notes.as_deref(), Some("prêté à Alex"));
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
        assert_eq!(chosen_kind("/bd/Thanos 01.cbz", Some(4), p, 5), MatchKind::NameOtherSize);
        assert_eq!(chosen_kind("/bd/Thanos 01.cbz", None, p, 5), MatchKind::Name);
        assert_eq!(chosen_kind("/bd/autre.cbz", Some(5), p, 5), MatchKind::Size);
        assert_eq!(chosen_kind("/bd/autre.cbz", None, p, 5), MatchKind::Chosen);
        // Nom en NFD (macOS, NAS) contre NFC : même nom (comme `check_files`).
        let nfc = Path::new("/rangé/Thanos é.cbz");
        assert_eq!(chosen_kind("/bd/Thanos e\u{301}.cbz", Some(5), nfc, 5), MatchKind::NameAndSize);
        assert_eq!(name_key("Z:\\BD\\E\u{301}TÉ.cbz"), name_key("/x/été.CBZ"));
    }
}
