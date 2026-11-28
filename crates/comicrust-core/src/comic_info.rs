//! `ComicInfo.xml` — métadonnées embarquées dans les archives CBZ/CBR/CB7.
//!
//! Les noms de champs reprennent exactement les chaînes relevées par REA dans
//! `ComicRack.Engine.dll` (classe `ComicInfo`), dans le même ordre.

use serde::{Deserialize, Serialize};

/// Type de page (attribut `Type` d'un `<Page>`).
/// Type d'une page (`ComicPageType` de ComicRack), gardé **en texte brut**.
///
/// ComicRack écrit une énumération à drapeaux (`[Flags]`) : un type peut être combiné
/// (`FrontCover, Story`) et la publicité s'écrit `Advertisment` (orthographe de
/// ComicRack). Une énumération Rust stricte refusait ces fiches et, avec elles, tout le
/// `ComicInfo.xml` (écriture des infos, conversion en CBZ, analyse) : le texte est donc
/// gardé tel quel (espaces de début et de fin retirés ; vide = `Story`).
///
/// `Debug` rend le texte brut (sans guillemets) : `format!("{:?}", type)` donne le texte
/// à écrire, comme le faisait l'ancienne énumération. Préférer [`PageType::as_str`].
#[derive(Clone, PartialEq, Eq)]
pub struct PageType(std::borrow::Cow<'static, str>);

#[allow(non_upper_case_globals)]
impl PageType {
    pub const FrontCover: PageType = PageType::fixed("FrontCover");
    pub const InnerCover: PageType = PageType::fixed("InnerCover");
    pub const Roundup: PageType = PageType::fixed("Roundup");
    pub const Story: PageType = PageType::fixed("Story");
    pub const Advertisement: PageType = PageType::fixed("Advertisement");
    /// Orthographe de ComicRack (`ComicPageType.Advertisment`).
    pub const Advertisment: PageType = PageType::fixed("Advertisment");
    pub const Editorial: PageType = PageType::fixed("Editorial");
    pub const Letters: PageType = PageType::fixed("Letters");
    pub const Preview: PageType = PageType::fixed("Preview");
    pub const BackCover: PageType = PageType::fixed("BackCover");
    pub const Other: PageType = PageType::fixed("Other");
    pub const Deleted: PageType = PageType::fixed("Deleted");

    const fn fixed(s: &'static str) -> Self {
        PageType(std::borrow::Cow::Borrowed(s))
    }

    /// Type d'après le texte de l'attribut `Type` (vide : `Story`).
    pub fn new(text: &str) -> Self {
        let t = text.trim();
        if t.is_empty() || t == "Story" {
            PageType::Story
        } else {
            PageType(std::borrow::Cow::Owned(t.to_string()))
        }
    }

    /// Texte de l'attribut `Type` (`FrontCover`, `FrontCover, Story`, `Advertisment`…).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for PageType {
    fn default() -> Self {
        PageType::Story
    }
}

impl std::fmt::Debug for PageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for PageType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PageType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(PageType::new(&String::deserialize(d)?))
    }
}

/// Lecture tolérante d'un champ facultatif : pour un nombre, une valeur **vide**
/// (`<Count></Count>`, `ImageSize=""`) donne `None` au lieu de faire échouer tout le
/// `ComicInfo.xml` (espaces autour acceptés) ; un nombre illisible (`<Year>abc</Year>`)
/// reste une erreur, comme pour ComicRack. Un texte est rendu tel quel.
pub trait Lenient: Sized {
    fn from_text(text: Option<String>) -> Result<Self, String>;
}

impl Lenient for Option<String> {
    fn from_text(text: Option<String>) -> Result<Self, String> {
        Ok(text)
    }
}

macro_rules! lenient_number {
    ($($t:ty),*) => {$(
        impl Lenient for Option<$t> {
            fn from_text(text: Option<String>) -> Result<Self, String> {
                match text.as_deref().map(str::trim) {
                    None | Some("") => Ok(None),
                    Some(t) => t.parse().map(Some).map_err(|_| format!("nombre invalide : « {t} »")),
                }
            }
        }
    )*};
}
lenient_number!(i32, u32, u64, f32);

/// `#[serde(deserialize_with = "lenient")]` (voir [`Lenient`]).
pub fn lenient<'de, D: serde::Deserializer<'de>, T: Lenient>(d: D) -> Result<T, D::Error> {
    T::from_text(Option::<String>::deserialize(d)?).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "@Image")]
    pub image: u32,
    #[serde(rename = "@Type", default, skip_serializing_if = "is_story")]
    pub page_type: PageType,
    #[serde(rename = "@DoublePage", default, skip_serializing_if = "is_false")]
    pub double_page: bool,
    #[serde(rename = "@ImageSize", default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub image_size: Option<u64>,
    #[serde(rename = "@Key", default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(rename = "@ImageWidth", default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub image_width: Option<u32>,
    #[serde(rename = "@ImageHeight", default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub image_height: Option<u32>,
    /// Signet (favori) posé sur la page.
    #[serde(rename = "@Bookmark", default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
    /// Rotation (`None`, `Rotate90`, `Rotate180`, `Rotate270`).
    #[serde(rename = "@Rotation", default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<String>,
    /// Position en double page (`Default`, `Near`, `Far`).
    #[serde(rename = "@PagePosition", default, skip_serializing_if = "Option::is_none")]
    pub page_position: Option<String>,
}

fn is_story(t: &PageType) -> bool {
    *t == PageType::Story
}
fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pages {
    #[serde(rename = "Page", default)]
    pub page: Vec<PageInfo>,
}

/// Valeurs "Yes"/"No"/"YesAndRightToLeft"/"Unknown" stockées en texte.
pub type YesNo = Option<String>;

macro_rules! comic_info {
    ($( $field:ident : $ty:ty => $xml:literal ),* $(,)?) => {
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename = "ComicInfo")]
        pub struct ComicInfo {
            $(
                #[serde(rename = $xml, default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
                pub $field: $ty,
            )*
            #[serde(rename = "Pages", default, skip_serializing_if = "Option::is_none")]
            pub pages: Option<Pages>,
        }
    };
}

comic_info! {
    title: Option<String> => "Title",
    series: Option<String> => "Series",
    number: Option<String> => "Number",
    count: Option<i32> => "Count",
    volume: Option<i32> => "Volume",
    alternate_series: Option<String> => "AlternateSeries",
    alternate_number: Option<String> => "AlternateNumber",
    alternate_count: Option<i32> => "AlternateCount",
    story_arc: Option<String> => "StoryArc",
    series_group: Option<String> => "SeriesGroup",
    summary: Option<String> => "Summary",
    notes: Option<String> => "Notes",
    review: Option<String> => "Review",
    year: Option<i32> => "Year",
    month: Option<i32> => "Month",
    day: Option<i32> => "Day",
    writer: Option<String> => "Writer",
    penciller: Option<String> => "Penciller",
    inker: Option<String> => "Inker",
    colorist: Option<String> => "Colorist",
    letterer: Option<String> => "Letterer",
    cover_artist: Option<String> => "CoverArtist",
    editor: Option<String> => "Editor",
    translator: Option<String> => "Translator",
    publisher: Option<String> => "Publisher",
    imprint: Option<String> => "Imprint",
    genre: Option<String> => "Genre",
    tags: Option<String> => "Tags",
    web: Option<String> => "Web",
    page_count: Option<u32> => "PageCount",
    language_iso: Option<String> => "LanguageISO",
    format: Option<String> => "Format",
    age_rating: Option<String> => "AgeRating",
    black_and_white: YesNo => "BlackAndWhite",
    manga: YesNo => "Manga",
    characters: Option<String> => "Characters",
    teams: Option<String> => "Teams",
    main_character_or_team: Option<String> => "MainCharacterOrTeam",
    locations: Option<String> => "Locations",
    scan_information: Option<String> => "ScanInformation",
    community_rating: Option<f32> => "CommunityRating",
}

impl ComicInfo {
    pub fn from_xml(xml: &str) -> Result<Self, crate::Error> {
        Ok(quick_xml::de::from_str(xml)?)
    }

    pub fn to_xml(&self) -> Result<String, crate::Error> {
        let body = quick_xml::se::to_string(self)?;
        Ok(format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{body}"))
    }

    /// Champs multi-valeurs séparés par des virgules (Writer, Characters, Tags…).
    pub fn split_list(value: &Option<String>) -> Vec<&str> {
        value.as_deref().map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
    }

    /// Libellé d'affichage proche de ComicRack : "Series v2 #12 - Title".
    pub fn caption(&self) -> String {
        let mut s = self.series.clone().unwrap_or_default();
        if let Some(v) = self.volume {
            s.push_str(&format!(" v{v}"));
        }
        if let Some(n) = &self.number {
            s.push_str(&format!(" #{n}"));
        }
        if let Some(t) = self.title.as_deref().filter(|t| !t.trim().is_empty()) {
            if s.trim().is_empty() {
                return t.trim().to_string();
            }
            s.push_str(&format!(" - {t}"));
        }
        s.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<ComicInfo xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Title>Infinity Gauntlet</Title>
  <Series>The Infinity Gauntlet</Series>
  <Number>1</Number>
  <Count>6</Count>
  <Year>1991</Year>
  <Writer>Jim Starlin</Writer>
  <Characters>Thanos, Adam Warlock, Silver Surfer</Characters>
  <Manga>No</Manga>
  <Pages>
    <Page Image="0" Type="FrontCover" ImageWidth="1988" ImageHeight="3056"/>
    <Page Image="1"/>
  </Pages>
</ComicInfo>"#;

    #[test]
    fn roundtrip() {
        let ci = ComicInfo::from_xml(SAMPLE).unwrap();
        assert_eq!(ci.series.as_deref(), Some("The Infinity Gauntlet"));
        assert_eq!(ci.count, Some(6));
        assert_eq!(ComicInfo::split_list(&ci.characters).len(), 3);
        let pages = ci.pages.as_ref().unwrap();
        assert_eq!(pages.page[0].page_type, PageType::FrontCover);
        assert_eq!(pages.page[1].page_type, PageType::Story);
        let again = ComicInfo::from_xml(&ci.to_xml().unwrap()).unwrap();
        assert_eq!(ci, again);
        assert_eq!(ci.caption(), "The Infinity Gauntlet #1 - Infinity Gauntlet");
        let only_title = ComicInfo { title: Some("Silver Surfer 03".into()), ..Default::default() };
        assert_eq!(only_title.caption(), "Silver Surfer 03");
    }

    /// Fiche écrite par ComicRack : page de publicité (`Advertisment`, orthographe de
    /// ComicRack), types combinés (`[Flags]`) et nombres vides. Avant la v1.0.2, tout le
    /// `ComicInfo.xml` était refusé.
    #[test]
    fn comicrack_page_types_and_empty_numbers() {
        let xml = r#"<?xml version="1.0"?>
<ComicInfo xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Series>Thanos</Series>
  <Count></Count>
  <Volume> 2 </Volume>
  <Year/>
  <CommunityRating></CommunityRating>
  <Pages>
    <Page Image="0" Type="FrontCover, Story" ImageSize="" />
    <Page Image="1" Type="Advertisment" ImageWidth="800" />
    <Page Image="2" />
  </Pages>
</ComicInfo>"#;
        let ci = ComicInfo::from_xml(xml).unwrap();
        assert_eq!(ci.series.as_deref(), Some("Thanos"));
        assert_eq!((ci.count, ci.volume, ci.year, ci.community_rating), (None, Some(2), None, None));
        // Un nombre illisible reste une erreur (fiche réellement abîmée).
        assert!(ComicInfo::from_xml("<ComicInfo><Year>abc</Year></ComicInfo>").is_err());
        let p = &ci.pages.as_ref().unwrap().page;
        assert_eq!(p[0].page_type.as_str(), "FrontCover, Story");
        assert_eq!(p[0].image_size, None);
        assert_eq!(p[1].page_type, PageType::Advertisment);
        assert_eq!(format!("{:?}", p[1].page_type), "Advertisment", "Debug = texte brut");
        assert_eq!(p[1].image_width, Some(800));
        assert_eq!(p[2].page_type, PageType::Story);
        // Aller-retour : types gardés tels quels.
        let again = ComicInfo::from_xml(&ci.to_xml().unwrap()).unwrap();
        assert_eq!(again, ci);
        assert!(ci.to_xml().unwrap().contains(r#"Type="FrontCover, Story""#));
    }
}
