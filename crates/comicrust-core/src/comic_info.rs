//! `ComicInfo.xml` — métadonnées embarquées dans les archives CBZ/CBR/CB7.
//!
//! Les noms de champs reprennent exactement les chaînes relevées par REA dans
//! `ComicRack.Engine.dll` (classe `ComicInfo`), dans le même ordre.

use serde::{Deserialize, Serialize};

/// Type de page (attribut `Type` d'un `<Page>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PageType {
    FrontCover,
    InnerCover,
    Roundup,
    #[default]
    Story,
    Advertisement,
    Editorial,
    Letters,
    Preview,
    BackCover,
    Other,
    Deleted,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "@Image")]
    pub image: u32,
    #[serde(rename = "@Type", default, skip_serializing_if = "is_story")]
    pub page_type: PageType,
    #[serde(rename = "@DoublePage", default, skip_serializing_if = "is_false")]
    pub double_page: bool,
    #[serde(rename = "@ImageSize", default, skip_serializing_if = "Option::is_none")]
    pub image_size: Option<u64>,
    #[serde(rename = "@Key", default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(rename = "@ImageWidth", default, skip_serializing_if = "Option::is_none")]
    pub image_width: Option<u32>,
    #[serde(rename = "@ImageHeight", default, skip_serializing_if = "Option::is_none")]
    pub image_height: Option<u32>,
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
                #[serde(rename = $xml, default, skip_serializing_if = "Option::is_none")]
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
}
