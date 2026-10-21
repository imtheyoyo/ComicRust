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

#[cfg(test)]
mod tests {
    use super::*;

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
