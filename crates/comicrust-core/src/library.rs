//! Bibliothèque `ComicDB.xml` (base locale par défaut de ComicRack).
//!
//! ComicRack sérialise sa base avec `XmlSerializer` : `<ComicDatabase>` →
//! `<Books>` → `<Book Id=".." File="..">` contenant les champs ComicInfo plus
//! les champs propres à la bibliothèque (date d'ajout, page courante, note…).
//! Le parseur ci-dessous est volontairement tolérant : les éléments inconnus
//! sont ignorés, ce qui permet de lire une vraie base sans tout modéliser.
//! La même fiche `<Book>` (racine `<ComicBook>`) est stockée telle quelle dans la
//! colonne `data` de la table `comics` quand ComicRack utilise MySQL (voir `db`).

use serde::{Deserialize, Serialize};

use crate::cbl::ReadingList;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LibraryBook {
    #[serde(rename = "@Id")]
    pub id: String,
    #[serde(rename = "@File", default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(rename = "Series", default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(rename = "Number", default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(rename = "Volume", default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<i32>,
    #[serde(rename = "Year", default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(rename = "Title", default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "Publisher", default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(rename = "PageCount", default, skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u32>,
    #[serde(rename = "CurrentPage", default, skip_serializing_if = "Option::is_none")]
    pub current_page: Option<u32>,
    #[serde(rename = "LastPageRead", default, skip_serializing_if = "Option::is_none")]
    pub last_page_read: Option<u32>,
    #[serde(rename = "Rating", default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f32>,
    #[serde(rename = "Count", default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i32>,
    #[serde(rename = "Writer", default, skip_serializing_if = "Option::is_none")]
    pub writer: Option<String>,
    #[serde(rename = "Penciller", default, skip_serializing_if = "Option::is_none")]
    pub penciller: Option<String>,
    #[serde(rename = "Genre", default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(rename = "StoryArc", default, skip_serializing_if = "Option::is_none")]
    pub story_arc: Option<String>,
    #[serde(rename = "AlternateSeries", default, skip_serializing_if = "Option::is_none")]
    pub alternate_series: Option<String>,
    /// Date d'ajout à la bibliothèque (texte ISO, tel que stocké par ComicRack).
    #[serde(rename = "Added", default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
    /// Date de dernière lecture.
    #[serde(rename = "Opened", default, skip_serializing_if = "Option::is_none")]
    pub opened: Option<String>,
    /// Date de parution saisie dans ComicRack (« Released »).
    #[serde(rename = "Released", default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    #[serde(rename = "OpenCount", default, skip_serializing_if = "Option::is_none")]
    pub open_count: Option<u32>,
    #[serde(rename = "FileSize", default, skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    /// Date de modification du fichier relevée à l'ajout ou au dernier examen (ISO 8601).
    #[serde(rename = "FileModifiedTime", default, skip_serializing_if = "Option::is_none")]
    pub file_modified_time: Option<String>,
    /// Date de création du fichier (ISO 8601).
    #[serde(rename = "FileCreationTime", default, skip_serializing_if = "Option::is_none")]
    pub file_creation_time: Option<String>,
    #[serde(rename = "AlternateNumber", default, skip_serializing_if = "Option::is_none")]
    pub alternate_number: Option<String>,
    #[serde(rename = "AlternateCount", default, skip_serializing_if = "Option::is_none")]
    pub alternate_count: Option<i32>,
    #[serde(rename = "SeriesGroup", default, skip_serializing_if = "Option::is_none")]
    pub series_group: Option<String>,
    #[serde(rename = "Summary", default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(rename = "Notes", default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(rename = "Review", default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(rename = "Month", default, skip_serializing_if = "Option::is_none")]
    pub month: Option<i32>,
    #[serde(rename = "Day", default, skip_serializing_if = "Option::is_none")]
    pub day: Option<i32>,
    #[serde(rename = "Inker", default, skip_serializing_if = "Option::is_none")]
    pub inker: Option<String>,
    #[serde(rename = "Colorist", default, skip_serializing_if = "Option::is_none")]
    pub colorist: Option<String>,
    #[serde(rename = "Letterer", default, skip_serializing_if = "Option::is_none")]
    pub letterer: Option<String>,
    #[serde(rename = "CoverArtist", default, skip_serializing_if = "Option::is_none")]
    pub cover_artist: Option<String>,
    #[serde(rename = "Editor", default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    #[serde(rename = "Translator", default, skip_serializing_if = "Option::is_none")]
    pub translator: Option<String>,
    #[serde(rename = "Imprint", default, skip_serializing_if = "Option::is_none")]
    pub imprint: Option<String>,
    #[serde(rename = "Web", default, skip_serializing_if = "Option::is_none")]
    pub web: Option<String>,
    #[serde(rename = "LanguageISO", default, skip_serializing_if = "Option::is_none")]
    pub language_iso: Option<String>,
    #[serde(rename = "Format", default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(rename = "AgeRating", default, skip_serializing_if = "Option::is_none")]
    pub age_rating: Option<String>,
    #[serde(rename = "BlackAndWhite", default, skip_serializing_if = "Option::is_none")]
    pub black_and_white: Option<String>,
    #[serde(rename = "Manga", default, skip_serializing_if = "Option::is_none")]
    pub manga: Option<String>,
    #[serde(rename = "Characters", default, skip_serializing_if = "Option::is_none")]
    pub characters: Option<String>,
    #[serde(rename = "Teams", default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<String>,
    #[serde(rename = "MainCharacterOrTeam", default, skip_serializing_if = "Option::is_none")]
    pub main_character_or_team: Option<String>,
    #[serde(rename = "Locations", default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<String>,
    #[serde(rename = "Tags", default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    #[serde(rename = "CommunityRating", default, skip_serializing_if = "Option::is_none")]
    pub community_rating: Option<f32>,
    #[serde(rename = "BookAge", default, skip_serializing_if = "Option::is_none")]
    pub book_age: Option<String>,
    #[serde(rename = "BookCollectionStatus", default, skip_serializing_if = "Option::is_none")]
    pub book_collection_status: Option<String>,
    #[serde(rename = "BookCondition", default, skip_serializing_if = "Option::is_none")]
    pub book_condition: Option<String>,
    #[serde(rename = "BookLocation", default, skip_serializing_if = "Option::is_none")]
    pub book_location: Option<String>,
    #[serde(rename = "BookNotes", default, skip_serializing_if = "Option::is_none")]
    pub book_notes: Option<String>,
    #[serde(rename = "BookOwner", default, skip_serializing_if = "Option::is_none")]
    pub book_owner: Option<String>,
    #[serde(rename = "BookStore", default, skip_serializing_if = "Option::is_none")]
    pub book_store: Option<String>,
    #[serde(rename = "ISBN", default, skip_serializing_if = "Option::is_none")]
    pub isbn: Option<String>,
    #[serde(rename = "BookPrice", default, skip_serializing_if = "Option::is_none")]
    pub book_price: Option<f32>,
    /// Case « cochée » de ComicRack (vraie par défaut).
    #[serde(rename = "@Checked", default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// Informations modifiées dans ComicRack mais pas encore écrites dans le fichier.
    #[serde(rename = "ComicInfoIsDirty", default, skip_serializing_if = "Option::is_none")]
    pub comic_info_is_dirty: Option<bool>,
    #[serde(rename = "ComicBookIsDirty", default, skip_serializing_if = "Option::is_none")]
    pub comic_book_is_dirty: Option<bool>,
    /// « Utiliser les valeurs proposées » (vrai par défaut) : série, numéro… déduits du
    /// nom du fichier quand la fiche est vide.
    #[serde(rename = "EnableProposed", default, skip_serializing_if = "Option::is_none")]
    pub enable_proposed: Option<bool>,
    /// Valeurs proposées, calculées au premier besoin (cache ; laisser par défaut).
    #[serde(skip)]
    pub proposed_cache: std::sync::OnceLock<crate::name_info::NameInfo>,
    /// Fichier signalé manquant par ComicRack.
    #[serde(rename = "Missing", default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<bool>,
    /// « Série complète » : `Yes`, `No` ou `Unknown`.
    #[serde(rename = "SeriesComplete", default, skip_serializing_if = "Option::is_none")]
    pub series_complete: Option<String>,
    #[serde(rename = "ScanInformation", default, skip_serializing_if = "Option::is_none")]
    pub scan_information: Option<String>,
    /// Rang de la couverture préférée parmi les pages de type « FrontCover ».
    #[serde(rename = "PreferredFrontCover", default, skip_serializing_if = "Option::is_none")]
    pub preferred_front_cover: Option<i32>,
    #[serde(rename = "Pages", default, skip_serializing_if = "PageList::is_empty")]
    pub pages: PageList,
}

/// `<Pages><Page Image="0" Type="FrontCover" …/></Pages>` : informations par page.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PageList {
    #[serde(rename = "Page", default)]
    pub page: Vec<PageInfo>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PageInfo {
    /// Rang de l'image dans le fichier (ordre naturel des noms).
    #[serde(rename = "@Image", default)]
    pub image: i32,
    /// Type de page (`FrontCover`, `Story`, `Other`, `Deleted`…), éventuellement combinés.
    #[serde(rename = "@Type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl PageList {
    pub fn is_empty(&self) -> bool {
        self.page.is_empty()
    }
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

    /// Valeurs déduites du nom du fichier (`ComicNameInfo` de ComicRack).
    pub fn proposed(&self) -> &crate::name_info::NameInfo {
        self.proposed_cache.get_or_init(|| crate::name_info::from_file_path(self.file.as_deref().unwrap_or("")))
    }

    fn use_proposed(&self) -> bool {
        self.enable_proposed.unwrap_or(true)
    }

    /// Série, ou celle proposée si elle est vide (`ShadowSeries`).
    pub fn shadow_series(&self) -> String {
        match self.series.as_deref().filter(|s| !s.is_empty()) {
            Some(x) => x.to_string(),
            None if self.use_proposed() => self.proposed().series.clone(),
            None => String::new(),
        }
    }

    /// Numéro, ou celui proposé s'il est vide (`ShadowNumber` ; « - » = pas de numéro).
    pub fn shadow_number(&self) -> String {
        match self.number.as_deref().filter(|s| !s.is_empty()) {
            Some(x) => x.to_string(),
            None if self.use_proposed() => self.proposed().number.clone(),
            None => String::new(),
        }
    }

    pub fn shadow_volume(&self) -> i32 {
        match self.volume.unwrap_or(-1) {
            -1 if self.use_proposed() => self.proposed().volume,
            v => v,
        }
    }

    pub fn shadow_year(&self) -> i32 {
        match self.year.unwrap_or(-1) {
            -1 if self.use_proposed() => self.proposed().year,
            y => y,
        }
    }

    pub fn shadow_count(&self) -> i32 {
        match self.count.unwrap_or(-1) {
            -1 if self.use_proposed() => self.proposed().count,
            c => c,
        }
    }

    pub fn shadow_format(&self) -> String {
        match self.format.as_deref().filter(|s| !s.is_empty()) {
            Some(x) => x.to_string(),
            None if self.use_proposed() => self.proposed().format.clone(),
            None => String::new(),
        }
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
        Some((((read + 1) * 100) / total).clamp(1, 100))
    }

    /// Fiche d'un livre telle que stockée dans la colonne `data` de MySQL.
    /// Fiche au format de ComicRack (`<ComicBook Id=".." File="..">…`), comme la colonne
    /// `data` de MySQL : sert à écrire les infos dans un fichier quelle que soit la base.
    pub fn to_record_xml(&self) -> Result<String, crate::Error> {
        Ok(quick_xml::se::to_string_with_root("ComicBook", self)?)
    }

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
    fn record_xml_round_trip() {
        let b = LibraryBook::from_xml(
            r#"<ComicBook Id="x" File="Z:\BD\a &amp; b.cbz"><Series>Thanos &amp; Cie</Series><Number>1</Number><Rating>4.5</Rating><Checked>true</Checked><Pages><Page Image="0" Type="FrontCover"/><Page Image="1"/></Pages></ComicBook>"#,
        )
        .unwrap();
        let xml = b.to_record_xml().unwrap();
        assert!(!xml.contains("<Title"), "champs vides absents : {xml}");
        let again = LibraryBook::from_xml(&xml).unwrap();
        assert_eq!(again.file, b.file);
        assert_eq!(again.series.as_deref(), Some("Thanos & Cie"));
        assert_eq!(again.rating, Some(4.5));
        assert_eq!(again.pages.page.len(), 2);
        assert_eq!(again.pages.page[0].kind.as_deref(), Some("FrontCover"));
        let info = crate::writer::comic_info_from_record(&xml, None).unwrap();
        assert!(info.contains("<Series>Thanos &amp; Cie</Series>"));
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
