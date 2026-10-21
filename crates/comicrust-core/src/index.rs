//! Index des fichiers de BD d'un dossier, pour retrouver les livres d'une liste `.cbl`.
//!
//! Une liste `.cbl` désigne les livres par **série + numéro (+ volume, année)**, tels que
//! ComicRack les connaît. Ces valeurs viennent de ses métadonnées, que ComicRack écrit
//! aussi dans le `ComicInfo.xml` de chaque archive. L'index lit donc d'abord ce
//! `ComicInfo.xml`, et à défaut devine série et numéro à partir du nom du fichier.
//!
//! Lire le `ComicInfo.xml` coûte peu pour un ZIP/CBZ (quelques Ko lus en fin d'archive),
//! un TAR ou un RAR non solide. Pour les 7z et les RAR solides, il faudrait tout
//! décompresser : seul le nom du fichier est utilisé. Les résultats sont gardés dans un
//! cache (chemin + taille + date) : une deuxième recherche ne relit aucune archive.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::library::{normalize_number, normalize_series};
use crate::providers::{is_book_path, BookFormat, PageProvider, RarProvider, TarProvider};
use crate::ComicInfo;

/// Nombre maximal de fichiers non reconnus gardés pour le diagnostic.
const MAX_UNRECOGNIZED: usize = 30;
const CACHE_VERSION: u32 = 1;

/// Devine (série, numéro) à partir d'un nom de fichier. Formes reconnues, dans l'ordre :
///
/// - `COSMIC COLLECTION #061 1990_03---INFINITY GAUNTLET---….cbz` (numéro après `#`) ;
/// - `Blacksad T03 - L'Enfer, le silence.cbz`, `Thorgal Tome 12.cbr`, `Spirou n°45.cbz` ;
/// - `Infinity Gauntlet 001 (1991) (Digital).cbz`, `Iron Man v1 055.cbr` (numéro final) ;
/// - `Silver Surfer 034 - Le retour.cbz` (série sans chiffres, numéro, puis « - titre »).
///
/// Les groupes entre parenthèses ou crochets sont ignorés.
pub fn parse_file_name(file_name: &str) -> Option<(String, String)> {
    static GROUPS: OnceLock<Regex> = OnceLock::new();
    static HASH: OnceLock<Regex> = OnceLock::new();
    static TOME: OnceLock<Regex> = OnceLock::new();
    static END: OnceLock<Regex> = OnceLock::new();
    static TITLED: OnceLock<Regex> = OnceLock::new();
    let groups = GROUPS.get_or_init(|| Regex::new(r"\([^)]*\)|\[[^\]]*\]|\{[^}]*\}").unwrap());
    let hash = HASH.get_or_init(|| Regex::new(r"^(?<series>[^#]+?)\s*#\s*(?<num>\d+(?:\.\d+)?[a-zA-Z]?)\b").unwrap());
    let tome = TOME.get_or_init(|| {
        Regex::new(
            r"(?i)^(?<series>.+?)[\s_\-.,]+(?:t|tome|vol\.?|volume|n°|no\.?|num[ée]ro)\s*(?<num>\d+(?:\.\d+)?)\b",
        )
        .unwrap()
    });
    let end = END.get_or_init(|| {
        Regex::new(r"(?i)^(?<series>.*?)[\s_.\-]*(?:n[°o]\.?\s*)?(?<num>\d+(?:\.\d+)?[a-z]?)(?:\s*of\s*\d+)?\s*$")
            .unwrap()
    });
    let titled = TITLED.get_or_init(|| Regex::new(r"^(?<series>[^\d]+?)[\s_\-]+(?<num>\d{1,4})\s*-+\s*\S").unwrap());

    let stem = Path::new(file_name).file_stem()?.to_string_lossy().replace('_', " ");
    let clean = groups.replace_all(&stem, " ");
    let clean = clean.trim();
    let caps = hash
        .captures(clean)
        .or_else(|| tome.captures(clean))
        .or_else(|| end.captures(clean))
        .or_else(|| titled.captures(clean))?;
    let series = caps["series"].trim().trim_end_matches(['-', '#', ',', '.']).trim().to_string();
    // Un nom qui n'est qu'un numéro (ex. "001.cbz") n'apporte pas de série.
    if series.is_empty() {
        return None;
    }
    Some((series, caps["num"].to_string()))
}

/// D'où viennent la série et le numéro d'un fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    ComicInfo,
    FileName,
}

#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub path: PathBuf,
    pub series: String,
    pub number: String,
    pub volume: Option<i32>,
    pub year: Option<i32>,
    pub source: Source,
}

/// Métadonnées lues dans une archive (ce qui est gardé en cache).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct InfoSummary {
    pub series: Option<String>,
    pub number: Option<String>,
    pub volume: Option<i32>,
    pub year: Option<i32>,
}

impl From<&ComicInfo> for InfoSummary {
    fn from(ci: &ComicInfo) -> Self {
        Self { series: ci.series.clone(), number: ci.number.clone(), volume: ci.volume, year: ci.year }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    size: u64,
    mtime: u64,
    /// `None` : l'archive n'a pas de ComicInfo.xml lisible.
    info: Option<InfoSummary>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    files: HashMap<String, CacheEntry>,
}

/// Avancement d'une indexation, lisible depuis un autre fil (barre de progression).
#[derive(Debug, Default)]
pub struct ScanProgress {
    pub total: AtomicUsize,
    pub done: AtomicUsize,
}

impl ScanProgress {
    pub fn get(&self) -> (usize, usize) {
        (self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed))
    }
}

/// Bilan d'une indexation (affiché et copié dans le diagnostic).
#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub files: usize,
    pub via_comic_info: usize,
    pub via_file_name: usize,
    pub from_cache: usize,
    pub unrecognized: Vec<PathBuf>,
    pub unrecognized_count: usize,
}

/// Résultat de la lecture d'un fichier pendant l'indexation.
struct Scanned {
    path: PathBuf,
    /// (taille, date de modification) pour le cache.
    stamp: Option<(u64, u64)>,
    info: Option<InfoSummary>,
    from_cache: bool,
}

#[derive(Debug, Default)]
pub struct FileIndex {
    entries: Vec<IndexedFile>,
    by_key: HashMap<(String, String), Vec<usize>>,
    pub stats: ScanStats,
}

fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some((meta.len(), mtime))
}

/// Lit le `ComicInfo.xml` d'une archive quand c'est bon marché.
pub fn read_comic_info_quick(path: &Path) -> Option<InfoSummary> {
    let ci = match BookFormat::detect(path)? {
        BookFormat::Zip => {
            let mut zip = ::zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
            let name = zip
                .file_names()
                .filter(|n| {
                    let l = n.replace('\\', "/").to_ascii_lowercase();
                    l == "comicinfo.xml" || l.ends_with("/comicinfo.xml")
                })
                .min_by_key(|n| n.len())?
                .to_string();
            let mut buf = Vec::new();
            zip.by_name(&name).ok()?.take(4 * 1024 * 1024).read_to_end(&mut buf).ok()?;
            crate::providers::parse_comic_info(&buf).ok()?
        }
        BookFormat::Tar => TarProvider::open(path).ok()?.comic_info().ok()??,
        BookFormat::Rar => {
            let mut rar = RarProvider::open(path).ok()?;
            if rar.is_solid() {
                return None;
            }
            rar.comic_info().ok()??
        }
        _ => return None,
    };
    let s = InfoSummary::from(&ci);
    (s.series.is_some() || s.number.is_some()).then_some(s)
}

fn collect_books(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(path);
            } else if is_book_path(&path) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

impl FileIndex {
    /// Indexe `root` sans cache ni suivi (tests, ligne de commande).
    pub fn scan(root: &Path) -> Self {
        Self::scan_with(root, None, None)
    }

    /// Indexe `root`. `cache` : fichier JSON réutilisé et mis à jour ;
    /// `progress` : compteurs mis à jour pendant la lecture.
    pub fn scan_with(root: &Path, cache: Option<&Path>, progress: Option<&ScanProgress>) -> Self {
        let mut cache_file: CacheFile = cache
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .filter(|c: &CacheFile| c.version == CACHE_VERSION)
            .unwrap_or_default();

        let paths = collect_books(root);
        if let Some(p) = progress {
            p.total.store(paths.len(), Ordering::Relaxed);
            p.done.store(0, Ordering::Relaxed);
        }

        // Lecture en parallèle : sur un partage réseau, la latence domine.
        let cached = &cache_file.files;
        let results: Vec<Scanned> = paths
            .into_par_iter()
            .map(|path| {
                let key = path.to_string_lossy().into_owned();
                let stamp = file_stamp(&path);
                let hit = stamp.and_then(|(size, mtime)| {
                    cached.get(&key).filter(|e| e.size == size && e.mtime == mtime).map(|e| e.info.clone())
                });
                let (info, from_cache) = match hit {
                    Some(info) => (info, true),
                    None => (read_comic_info_quick(&path), false),
                };
                if let Some(p) = progress {
                    p.done.fetch_add(1, Ordering::Relaxed);
                }
                Scanned { path, stamp, info, from_cache }
            })
            .collect();

        let mut idx = FileIndex::default();
        for Scanned { path, stamp, info, from_cache } in results {
            idx.stats.files += 1;
            if from_cache {
                idx.stats.from_cache += 1;
            }
            if let Some((size, mtime)) = stamp {
                cache_file
                    .files
                    .insert(path.to_string_lossy().into_owned(), CacheEntry { size, mtime, info: info.clone() });
            }
            idx.add_with_info(path, info);
        }

        if let Some(p) = cache {
            cache_file.version = CACHE_VERSION;
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_vec(&cache_file) {
                let _ = std::fs::write(p, json);
            }
        }
        idx
    }

    /// Ajoute un fichier en ne regardant que son nom.
    pub fn add(&mut self, path: PathBuf) {
        self.stats.files += 1;
        self.add_with_info(path, None);
    }

    fn push(&mut self, f: IndexedFile) {
        let key = (normalize_series(&f.series), normalize_number(&f.number));
        self.entries.push(f);
        self.by_key.entry(key).or_default().push(self.entries.len() - 1);
    }

    fn add_with_info(&mut self, path: PathBuf, info: Option<InfoSummary>) {
        let mut recognized = false;
        if let Some(InfoSummary { series: Some(series), number: Some(number), volume, year }) = info {
            if !series.trim().is_empty() && !number.trim().is_empty() {
                self.push(IndexedFile { path: path.clone(), series, number, volume, year, source: Source::ComicInfo });
                self.stats.via_comic_info += 1;
                recognized = true;
            }
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some((series, number)) = parse_file_name(&name) {
            self.push(IndexedFile {
                path: path.clone(),
                series,
                number,
                volume: None,
                year: None,
                source: Source::FileName,
            });
            if !recognized {
                self.stats.via_file_name += 1;
                recognized = true;
            }
        }
        if !recognized {
            self.stats.unrecognized_count += 1;
            if self.stats.unrecognized.len() < MAX_UNRECOGNIZED {
                self.stats.unrecognized.push(path);
            }
        }
    }

    /// Fichier correspondant à une entrée de liste. Chaque candidat reçoit un score :
    /// ComicInfo.xml concordant (+4), même volume (+2), même année (+1), nom de fichier
    /// qui concorde aussi (+2), année ou volume présents dans le chemin (+1).
    /// À score égal, le premier par ordre alphabétique du chemin.
    pub fn find(&self, series: &str, number: &str, year: Option<i32>, volume: Option<i32>) -> Option<&PathBuf> {
        let hits: Vec<&IndexedFile> = self
            .by_key
            .get(&(normalize_series(series), normalize_number(number)))?
            .iter()
            .map(|&i| &self.entries[i])
            .collect();
        let name_agrees = |p: &PathBuf| hits.iter().any(|h| h.source == Source::FileName && &h.path == p);
        let in_path = |p: &PathBuf, v: Option<i32>| v.is_some_and(|v| p.to_string_lossy().contains(&v.to_string()));
        let score = |f: &IndexedFile| -> i32 {
            let mut s = 0;
            if f.source == Source::ComicInfo {
                s += 4;
                if volume.is_some() && f.volume == volume {
                    s += 2;
                }
                if year.is_some() && f.year == year {
                    s += 1;
                }
            }
            if name_agrees(&f.path) {
                s += 2;
            }
            if in_path(&f.path, year) || in_path(&f.path, volume) {
                s += 1;
            }
            s
        };
        let mut best: Option<(&IndexedFile, i32)> = None;
        for f in hits.iter().copied() {
            let sc = score(f);
            if best.is_none_or(|(b, bs)| sc > bs || (sc == bs && f.path < b.path)) {
                best = Some((f, sc));
            }
        }
        best.map(|(f, _)| &f.path)
    }

    /// Résumé lisible du bilan (panneau de liste, diagnostic copié).
    pub fn summary(&self) -> String {
        let s = &self.stats;
        format!(
            "{} fichiers : {} via ComicInfo.xml, {} via le nom, {} non reconnus{}",
            s.files,
            s.via_comic_info,
            s.via_file_name,
            s.unrecognized_count,
            if s.from_cache > 0 { format!(" ({} déjà en cache)", s.from_cache) } else { String::new() }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn file_names() {
        let p = parse_file_name;
        let s = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        // Noms réels de la collection de Yoyo (recette v0.2.0, test C-06).
        assert_eq!(
            p("COSMIC COLLECTION #061 1990_03---INFINITY GAUNTLET---Silver surfer v3 34-38 (Jim Starlin).cbz"),
            s("COSMIC COLLECTION", "061")
        );
        assert_eq!(
            p("COSMIC COLLECTION #224 2012_05 - Thanos Rising 1-5 (Jason Aaron)(2013).cbz"),
            s("COSMIC COLLECTION", "224")
        );
        assert_eq!(
            p("COSMIC COLLECTION #005 1966_01---THE GALACTUS TRILOGY---Fantastic Four 48-50 (Stan Lee).cbz"),
            s("COSMIC COLLECTION", "005")
        );
        // Formes classiques.
        assert_eq!(p("Infinity Gauntlet 001 (1991) (Digital).cbz"), s("Infinity Gauntlet", "001"));
        assert_eq!(p("Iron Man v1 #055.cbr"), s("Iron Man v1", "055"));
        assert_eq!(p("Thanos - 12.cb7"), s("Thanos", "12"));
        assert_eq!(p("Warlock_09_[scan].cbz"), s("Warlock", "09"));
        assert_eq!(p("Silver Surfer 3 of 6.cbz"), s("Silver Surfer", "3"));
        assert_eq!(p("Blacksad T03 - L'Enfer, le silence.cbz"), s("Blacksad", "03"));
        assert_eq!(p("Thorgal - Tome 12 - La Cité du dieu perdu.cbr"), s("Thorgal", "12"));
        assert_eq!(p("Spirou n°45.cbz"), s("Spirou", "45"));
        assert_eq!(p("Silver Surfer 034 - Le retour.cbz"), s("Silver Surfer", "034"));
        assert_eq!(p("001.cbz"), None);
    }

    fn cbz(path: &Path, info: Option<&str>) {
        let mut w = ::zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opt = ::zip::write::SimpleFileOptions::default();
        w.start_file("001.jpg", opt).unwrap();
        w.write_all(b"x").unwrap();
        if let Some(xml) = info {
            w.start_file("ComicInfo.xml", opt).unwrap();
            w.write_all(xml.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    fn info(series: &str, number: u32, volume: i32) -> String {
        format!("<ComicInfo><Series>{series}</Series><Number>{number}</Number><Volume>{volume}</Volume></ComicInfo>")
    }

    /// Reproduit la liste réelle de Yoyo (recette v0.2.0, C-06) contre des fichiers nommés
    /// comme sa collection COSMIC COLLECTION, avec leurs ComicInfo.xml.
    #[test]
    fn real_list_cosmic_collection() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (sub, num, vol, rest) in [
            ("02_1988_2003", 61, 2, "1990_03---INFINITY GAUNTLET---Silver surfer v3 34-38 (Jim Starlin)"),
            ("02_1988_2003", 62, 2, "1990_04---INFINITY GAUNTLET---La quete de Thanos (Jim Starlin)"),
            ("02_1988_2003", 63, 2, "1990_05---INFINITY GAUNTLET---Silver surfer v3 40-50 (Jim Starlin)"),
            ("02_1988_2003", 64, 2, "1991_01---INFINITY GAUNTLET---Le Gant de l'infini 1-6 (Jim Starlin)"),
            ("02_1988_2003", 66, 2, "1992_02---INFINITY WAR---1 (Jim Starlin)"),
            ("04_2011_2016", 224, 3, "2012_05 - Thanos Rising 1-5 (Jason Aaron)(2013)"),
            ("01_1961_1985", 23, 1, "1974_02 - Galactus - Thor 224-227 - Herauts (Gerry Conway)"),
        ] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
            let name = format!("COSMIC COLLECTION #{num:03} {rest}.cbz");
            cbz(&root.join(sub).join(name), Some(&info("COSMIC COLLECTION", num, vol)));
        }
        let list = crate::ReadingList::from_xml(include_str!("../tests/fixtures/liste_thanos_yoyo.cbl")).unwrap();
        let idx = FileIndex::scan(root);
        assert_eq!(idx.stats.files, 7);
        assert_eq!(idx.stats.via_comic_info, 7);
        let found: Vec<Option<String>> = list
            .books
            .book
            .iter()
            .map(|b| {
                idx.find(&b.series, &b.number, b.year, b.volume)
                    .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            })
            .collect();
        // 6 livres COSMIC COLLECTION trouvés ; « Compil by Yo Thanos » et « Spider-Man »
        // ne sont pas dans ce dossier.
        assert_eq!(found.iter().filter(|f| f.is_some()).count(), 6, "{found:#?}");
        // #224 et non #023 (dont le titre contient « 224 »).
        assert!(found[0].as_deref().unwrap().starts_with("COSMIC COLLECTION #224"));
        assert!(found[2].as_deref().unwrap().starts_with("COSMIC COLLECTION #061"));
        assert!(found[1].is_none() && found[5].is_none());
    }

    #[test]
    fn duplicates_prefer_the_file_whose_name_also_agrees() {
        let dir = tempfile::tempdir().unwrap();
        let ci = info("Thanos Quest", 1, 1990);
        // Trois archives avec le même ComicInfo : seule la dernière a le bon nom.
        cbz(&dir.path().join("A copie de travail.cbz"), Some(&ci));
        cbz(&dir.path().join("Infinity Gauntlet 02.cbz"), Some(&ci));
        cbz(&dir.path().join("Thanos Quest 01 (1990).cbz"), Some(&ci));
        let idx = FileIndex::scan(dir.path());
        let hit = idx.find("Thanos Quest", "1", Some(1990), Some(1990)).unwrap();
        assert!(hit.ends_with("Thanos Quest 01 (1990).cbz"), "{hit:?}");
        assert!(idx.find("Infinity Gauntlet", "2", None, None).unwrap().ends_with("Infinity Gauntlet 02.cbz"));
    }

    #[test]
    fn comic_info_wins_over_file_name_and_cache_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        // Nom trompeur, ComicInfo correct.
        cbz(&dir.path().join("Divers 999.cbz"), Some(&info("Thanos Quest", 2, 1990)));
        // Sans ComicInfo : nom seulement.
        cbz(&dir.path().join("Warlock 09.cbz"), None);
        // Ni l'un ni l'autre.
        cbz(&dir.path().join("scan sans numero.cbz"), None);
        let cache = dir.path().join("cache").join("index.json");

        let progress = ScanProgress::default();
        let idx = FileIndex::scan_with(dir.path(), Some(&cache), Some(&progress));
        assert_eq!(progress.get(), (3, 3));
        assert_eq!(idx.stats.via_comic_info, 1);
        assert_eq!(idx.stats.via_file_name, 1);
        assert_eq!(idx.stats.unrecognized_count, 1);
        assert!(idx.find("Thanos Quest", "2", None, None).unwrap().ends_with("Divers 999.cbz"));
        assert!(idx.find("Warlock", "9", None, None).is_some());
        assert!(cache.exists());

        let again = FileIndex::scan_with(dir.path(), Some(&cache), None);
        assert_eq!(again.stats.from_cache, 3);
        assert!(again.find("Thanos Quest", "2", None, None).is_some());
        assert!(again.summary().contains("3 déjà en cache"));
    }
}
