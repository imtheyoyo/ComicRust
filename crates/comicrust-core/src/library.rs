//! Bibliothèque `ComicDB.xml` (base locale par défaut de ComicRack).
//!
//! ComicRack sérialise sa base avec `XmlSerializer` : `<ComicDatabase>` →
//! `<Books>` → `<Book Id=".." File="..">` contenant les champs ComicInfo plus
//! les champs propres à la bibliothèque (date d'ajout, page courante, note…).
//! Le parseur ci-dessous est volontairement tolérant : les éléments inconnus
//! sont ignorés, ce qui permet de lire une vraie base sans tout modéliser.
//! (Ta base actuelle est sur MySQL : voir README, étape "backend MySQL".)

use serde::Deserialize;

use crate::cbl::ReadingList;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibraryBook {
    #[serde(rename = "@Id")]
    pub id: String,
    #[serde(rename = "@File", default)]
    pub file: Option<String>,
    #[serde(rename = "Series", default)]
    pub series: Option<String>,
    #[serde(rename = "Number", default)]
    pub number: Option<String>,
    #[serde(rename = "Volume", default)]
    pub volume: Option<i32>,
    #[serde(rename = "Year", default)]
    pub year: Option<i32>,
    #[serde(rename = "Title", default)]
    pub title: Option<String>,
    #[serde(rename = "Publisher", default)]
    pub publisher: Option<String>,
    #[serde(rename = "PageCount", default)]
    pub page_count: Option<u32>,
    #[serde(rename = "CurrentPage", default)]
    pub current_page: Option<u32>,
    #[serde(rename = "LastPageRead", default)]
    pub last_page_read: Option<u32>,
    #[serde(rename = "Rating", default)]
    pub rating: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibraryBooks {
    #[serde(rename = "Book", default)]
    pub book: Vec<LibraryBook>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename = "ComicDatabase")]
pub struct Library {
    #[serde(rename = "@Id", default)]
    pub id: Option<String>,
    #[serde(rename = "Books", default)]
    pub books: LibraryBooks,
}

/// Normalisation de série, inspirée des regex relevées dans ComicInfo :
/// `\bv(ol(ume)?)?\.?\s?\d+\b\s*` et `[^a-z0-9]|\bthe\b|\band\b`.
pub fn normalize_series(s: &str) -> String {
    use std::sync::OnceLock;
    static VOL: OnceLock<regex::Regex> = OnceLock::new();
    static JUNK: OnceLock<regex::Regex> = OnceLock::new();
    let vol = VOL.get_or_init(|| regex::Regex::new(r"\bv(ol(ume)?)?\.?\s?\d+\b\s*").unwrap());
    let junk = JUNK.get_or_init(|| regex::Regex::new(r"[^a-z0-9]|\bthe\b|\band\b").unwrap());
    let lower = s.to_lowercase();
    let no_vol = vol.replace_all(&lower, "");
    junk.replace_all(&no_vol, "").into_owned()
}

pub fn normalize_number(n: &str) -> String {
    let t = n.trim().trim_start_matches('0');
    if t.is_empty() { "0".into() } else { t.to_lowercase() }
}

impl Library {
    pub fn from_xml(xml: &str) -> Result<Self, crate::Error> {
        Ok(quick_xml::de::from_str(xml.trim_start_matches('\u{feff}'))?)
    }

    /// Résout une liste CBL contre la bibliothèque : par GUID d'abord, puis
    /// par (série normalisée, numéro, volume). Renvoie pour chaque entrée le
    /// livre trouvé ou `None` (= numéro manquant).
    pub fn resolve<'a>(&'a self, list: &ReadingList) -> Vec<Option<&'a LibraryBook>> {
        list.books
            .book
            .iter()
            .map(|e| {
                if let Some(id) = &e.id {
                    if let Some(b) = self.books.book.iter().find(|b| b.id.eq_ignore_ascii_case(id)) {
                        return Some(b);
                    }
                }
                let s = normalize_series(&e.series);
                let n = normalize_number(&e.number);
                self.books.book.iter().find(|b| {
                    b.series.as_deref().map(normalize_series).as_deref() == Some(s.as_str())
                        && b.number.as_deref().map(normalize_number).as_deref() == Some(n.as_str())
                        && (e.volume.is_none() || b.volume.is_none() || e.volume == b.volume)
                })
            })
            .collect()
    }
}

/// Devine (série, numéro) à partir d'un nom de fichier de BD, à la manière
/// des conventions de nommage courantes :
/// `Infinity Gauntlet 001 (1991) (Digital).cbz`, `Iron Man v1 #055.cbr`,
/// `Thanos - 12.cb7`. Les groupes entre parenthèses/crochets sont ignorés.
pub fn parse_file_name(file_name: &str) -> Option<(String, String)> {
    use std::sync::OnceLock;
    static GROUPS: OnceLock<regex::Regex> = OnceLock::new();
    static NUM: OnceLock<regex::Regex> = OnceLock::new();
    let groups = GROUPS.get_or_init(|| regex::Regex::new(r"\([^)]*\)|\[[^\]]*\]|\{[^}]*\}").unwrap());
    let num = NUM.get_or_init(|| {
        regex::Regex::new(r"(?i)^(?<series>.*?)[\s_.\-]*(?:#|n[°o]\.?\s*)?(?<num>\d+(?:\.\d+)?[a-z]?)(?:\s*of\s*\d+)?\s*$").unwrap()
    });
    let stem = std::path::Path::new(file_name).file_stem()?.to_string_lossy().replace('_', " ");
    let clean = groups.replace_all(&stem, " ");
    let clean = clean.trim();
    let c = num.captures(clean)?;
    let series = c["series"].trim().trim_end_matches(['-', '#']).trim().to_string();
    // Un nom qui n'est qu'un numéro (ex. "001.cbz") n'apporte pas de série.
    if series.is_empty() {
        return None;
    }
    Some((series, c["num"].to_string()))
}

/// Index de fichiers de BD (série normalisée + numéro → chemin), construit
/// en parcourant un dossier. Sert à résoudre une liste `.cbl` sans base ComicRack.
#[derive(Debug, Default)]
pub struct FileIndex {
    map: std::collections::HashMap<(String, String), Vec<std::path::PathBuf>>,
    pub file_count: usize,
}

impl FileIndex {
    pub fn scan(root: &std::path::Path) -> Self {
        let mut idx = FileIndex::default();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let path = entry.path();
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push(path);
                } else if crate::providers::is_book_path(&path) {
                    idx.add(path);
                }
            }
        }
        idx
    }

    pub fn add(&mut self, path: std::path::PathBuf) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some((series, number)) = parse_file_name(&name) {
            self.map
                .entry((normalize_series(&series), normalize_number(&number)))
                .or_default()
                .push(path);
            self.file_count += 1;
        }
    }

    /// Fichier correspondant à une entrée de liste ; en cas d'homonymes, celui
    /// dont le nom contient l'année ou le volume demandés.
    pub fn find(&self, series: &str, number: &str, year: Option<i32>, volume: Option<i32>) -> Option<&std::path::PathBuf> {
        let hits = self.map.get(&(normalize_series(series), normalize_number(number)))?;
        let hint = |p: &&std::path::PathBuf, v: Option<i32>| {
            v.is_some_and(|v| p.to_string_lossy().contains(&v.to_string()))
        };
        hits.iter()
            .find(|p| hint(p, year))
            .or_else(|| hits.iter().find(|p| hint(p, volume)))
            .or_else(|| hits.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        let p = parse_file_name;
        assert_eq!(p("Infinity Gauntlet 001 (1991) (Digital).cbz"), Some(("Infinity Gauntlet".into(), "001".into())));
        assert_eq!(p("Iron Man v1 #055.cbr"), Some(("Iron Man v1".into(), "055".into())));
        assert_eq!(p("Thanos - 12.cb7"), Some(("Thanos".into(), "12".into())));
        assert_eq!(p("Warlock_09_[scan].cbz"), Some(("Warlock".into(), "09".into())));
        assert_eq!(p("Silver Surfer 3 of 6.cbz"), Some(("Silver Surfer".into(), "3".into())));
        assert_eq!(p("001.cbz"), None);
    }

    #[test]
    fn file_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Marvel")).unwrap();
        for f in ["Marvel/Iron Man 055 (1973).cbz", "Marvel/Iron Man 055 (2002).cbz", "The Thanos Quest 02.cbr", "notes.txt"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        let idx = FileIndex::scan(dir.path());
        assert_eq!(idx.file_count, 3);
        assert!(idx.find("Iron Man", "55", Some(2002), None).unwrap().ends_with("Iron Man 055 (2002).cbz"));
        assert!(idx.find("Thanos Quest", "2", None, None).is_some());
        assert!(idx.find("Warlock", "9", None, None).is_none());
    }

    #[test]
    fn normalize() {
        assert_eq!(normalize_series("The Avengers Vol. 3"), "avengers");
        assert_eq!(normalize_series("Thanos and the Gauntlet"), "thanosgauntlet");
    }

    #[test]
    fn resolve_list() {
        let db = r#"<ComicDatabase Id="x"><Books>
  <Book Id="AAA" File="F:\BD\Iron Man 055.cbz"><Series>Iron Man</Series><Number>55</Number><Volume>1968</Volume></Book>
  <Book Id="BBB" File="F:\BD\cm25.cbz"><Series>Captain Marvel</Series><Number>025</Number><Unknown>ignoré</Unknown></Book>
</Books></ComicDatabase>"#;
        let lib = Library::from_xml(db).unwrap();
        let cbl = ReadingList::from_xml(r#"<ReadingList><Name>T</Name><Books>
  <Book Series="The Iron Man" Number="55" Volume="1968"/>
  <Book Series="Captain Marvel" Number="25"/>
  <Book Series="Warlock" Number="9"/>
</Books></ReadingList>"#).unwrap();
        let r = lib.resolve(&cbl);
        assert_eq!(r[0].unwrap().id, "AAA");
        assert_eq!(r[1].unwrap().id, "BBB");
        assert!(r[2].is_none());
    }
}
