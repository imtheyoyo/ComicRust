//! Bibliothèque `ComicDB.xml` (base locale par défaut de ComicRack).
//!
//! ComicRack sérialise sa base avec `XmlSerializer` : `<ComicDatabase>` →
//! `<Books>` → `<Book Id=".." File="..">` contenant les champs ComicInfo plus
//! les champs propres à la bibliothèque (date d'ajout, page courante, note…).
//! Le parseur ci-dessous est volontairement tolérant : les éléments inconnus
//! sont ignorés, ce qui permet de lire une vraie base sans tout modéliser.
//! La même fiche `<Book>` (racine `<ComicBook>`) est stockée telle quelle dans la
//! colonne `data` de la table `comics` quand ComicRack utilise MySQL (voir `db`).

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
    #[serde(rename = "Count", default)]
    pub count: Option<i32>,
    #[serde(rename = "Writer", default)]
    pub writer: Option<String>,
    #[serde(rename = "Penciller", default)]
    pub penciller: Option<String>,
    #[serde(rename = "Genre", default)]
    pub genre: Option<String>,
    #[serde(rename = "StoryArc", default)]
    pub story_arc: Option<String>,
    #[serde(rename = "AlternateSeries", default)]
    pub alternate_series: Option<String>,
    /// Date d'ajout à la bibliothèque (texte ISO, tel que stocké par ComicRack).
    #[serde(rename = "Added", default)]
    pub added: Option<String>,
    /// Date de dernière lecture.
    #[serde(rename = "Opened", default)]
    pub opened: Option<String>,
    /// Date de parution saisie dans ComicRack (« Released »).
    #[serde(rename = "Released", default)]
    pub released: Option<String>,
    #[serde(rename = "OpenCount", default)]
    pub open_count: Option<u32>,
    #[serde(rename = "FileSize", default)]
    pub file_size: Option<u64>,
    #[serde(rename = "AlternateNumber", default)]
    pub alternate_number: Option<String>,
    #[serde(rename = "AlternateCount", default)]
    pub alternate_count: Option<i32>,
    #[serde(rename = "SeriesGroup", default)]
    pub series_group: Option<String>,
    #[serde(rename = "Summary", default)]
    pub summary: Option<String>,
    #[serde(rename = "Notes", default)]
    pub notes: Option<String>,
    #[serde(rename = "Review", default)]
    pub review: Option<String>,
    #[serde(rename = "Month", default)]
    pub month: Option<i32>,
    #[serde(rename = "Day", default)]
    pub day: Option<i32>,
    #[serde(rename = "Inker", default)]
    pub inker: Option<String>,
    #[serde(rename = "Colorist", default)]
    pub colorist: Option<String>,
    #[serde(rename = "Letterer", default)]
    pub letterer: Option<String>,
    #[serde(rename = "CoverArtist", default)]
    pub cover_artist: Option<String>,
    #[serde(rename = "Editor", default)]
    pub editor: Option<String>,
    #[serde(rename = "Translator", default)]
    pub translator: Option<String>,
    #[serde(rename = "Imprint", default)]
    pub imprint: Option<String>,
    #[serde(rename = "Web", default)]
    pub web: Option<String>,
    #[serde(rename = "LanguageISO", default)]
    pub language_iso: Option<String>,
    #[serde(rename = "Format", default)]
    pub format: Option<String>,
    #[serde(rename = "AgeRating", default)]
    pub age_rating: Option<String>,
    #[serde(rename = "BlackAndWhite", default)]
    pub black_and_white: Option<String>,
    #[serde(rename = "Manga", default)]
    pub manga: Option<String>,
    #[serde(rename = "Characters", default)]
    pub characters: Option<String>,
    #[serde(rename = "Teams", default)]
    pub teams: Option<String>,
    #[serde(rename = "MainCharacterOrTeam", default)]
    pub main_character_or_team: Option<String>,
    #[serde(rename = "Locations", default)]
    pub locations: Option<String>,
    #[serde(rename = "Tags", default)]
    pub tags: Option<String>,
    #[serde(rename = "CommunityRating", default)]
    pub community_rating: Option<f32>,
    #[serde(rename = "ScanInformation", default)]
    pub scan_information: Option<String>,
    /// Rang de la couverture préférée parmi les pages de type « FrontCover ».
    #[serde(rename = "PreferredFrontCover", default)]
    pub preferred_front_cover: Option<i32>,
    #[serde(rename = "Pages", default)]
    pub pages: PageList,
}

/// `<Pages><Page Image="0" Type="FrontCover" …/></Pages>` : informations par page.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageList {
    #[serde(rename = "Page", default)]
    pub page: Vec<PageInfo>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageInfo {
    /// Rang de l'image dans le fichier (ordre naturel des noms).
    #[serde(rename = "@Image", default)]
    pub image: i32,
    /// Type de page (`FrontCover`, `Story`, `Other`, `Deleted`…), éventuellement combinés.
    #[serde(rename = "@Type", default)]
    pub kind: Option<String>,
}

impl PageInfo {
    fn is(&self, t: &str) -> bool {
        self.kind.as_deref().is_some_and(|k| k.split(',').any(|p| p.trim() == t))
    }
}

impl LibraryBook {
    /// Libellé d'affichage : « Série v2 #12 - Titre », ou le nom du fichier à défaut.
    pub fn caption(&self) -> String {
        let ci = crate::ComicInfo {
            series: self.series.clone(),
            number: self.number.clone(),
            volume: self.volume.filter(|v| *v >= 0),
            title: self.title.clone(),
            ..Default::default()
        };
        let c = ci.caption();
        if !c.is_empty() {
            return c;
        }
        self.file
            .as_deref()
            .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f).to_string())
            .unwrap_or_else(|| self.id.clone())
    }

    /// Image du fichier à utiliser comme couverture, comme `ComicInfo.FrontCoverPageIndex` :
    /// la n-ième page « FrontCover » (n = `PreferredFrontCover`), sinon la première page
    /// qui n'est pas de type « Other », sinon l'image 0.
    pub fn front_cover_image(&self) -> usize {
        let pages = &self.pages.page;
        let covers: Vec<&PageInfo> = pages.iter().filter(|p| p.is("FrontCover")).collect();
        let chosen = if covers.is_empty() {
            pages.iter().find(|p| !p.is("Other"))
        } else {
            let n = self.preferred_front_cover.unwrap_or(0).clamp(0, covers.len() as i32 - 1);
            Some(covers[n as usize])
        };
        chosen.map(|p| p.image.max(0) as usize).unwrap_or(0)
    }

    /// Pourcentage lu (0–100) d'après la dernière page lue enregistrée par ComicRack.
    pub fn read_percent(&self) -> Option<u32> {
        let (read, total) = (self.last_page_read?, self.page_count?);
        if total == 0 {
            return None;
        }
        // ComicRack compte la dernière page lue à partir de 0 ; 0 = jamais ouvert.
        if read == 0 {
            return Some(0);
        }
        Some((((read + 1) * 100) / total).min(100))
    }

    /// Fiche d'un livre telle que stockée dans la colonne `data` de MySQL.
    pub fn from_xml(xml: &str) -> Result<Self, crate::Error> {
        Ok(quick_xml::de::from_str(xml.trim_start_matches('\u{feff}'))?)
    }
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
    if t.is_empty() {
        "0".into()
    } else {
        t.to_lowercase()
    }
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
    fn front_cover_from_pages() {
        let b = LibraryBook::from_xml(
            r#"<ComicBook Id="x"><Pages><Page Image="0" Type="Other"/><Page Image="1"/><Page Image="2" Type="FrontCover"/></Pages></ComicBook>"#,
        )
        .unwrap();
        assert_eq!(b.front_cover_image(), 2);
        let b = LibraryBook::from_xml(
            r#"<ComicBook Id="x"><PreferredFrontCover>1</PreferredFrontCover><Pages><Page Image="0" Type="FrontCover"/><Page Image="5" Type="FrontCover, InnerCover"/></Pages></ComicBook>"#,
        )
        .unwrap();
        assert_eq!(b.front_cover_image(), 5);
        let b = LibraryBook::from_xml(
            r#"<ComicBook Id="x"><Pages><Page Image="0" Type="Other"/><Page Image="1"/></Pages></ComicBook>"#,
        )
        .unwrap();
        assert_eq!(b.front_cover_image(), 1);
        assert_eq!(LibraryBook::from_xml(r#"<ComicBook Id="x"/>"#).unwrap().front_cover_image(), 0);
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
        let cbl = ReadingList::from_xml(
            r#"<ReadingList><Name>T</Name><Books>
  <Book Series="The Iron Man" Number="55" Volume="1968"/>
  <Book Series="Captain Marvel" Number="25"/>
  <Book Series="Warlock" Number="9"/>
</Books></ReadingList>"#,
        )
        .unwrap();
        let r = lib.resolve(&cbl);
        assert_eq!(r[0].unwrap().id, "AAA");
        assert_eq!(r[1].unwrap().id, "BBB");
        assert!(r[2].is_none());
    }
}
