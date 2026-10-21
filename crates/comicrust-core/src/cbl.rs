//! Listes de lecture `.cbl` ("ComicRack Reading List", chaîne relevée dans
//! `Program.ShowComicOpenDialog`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CblBook {
    #[serde(rename = "@Series", default)]
    pub series: String,
    #[serde(rename = "@Number", default)]
    pub number: String,
    #[serde(rename = "@Volume", default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<i32>,
    #[serde(rename = "@Year", default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(rename = "@Format", default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// GUID du livre dans la base ComicRack (optionnel).
    #[serde(rename = "Id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CblBooks {
    #[serde(rename = "Book", default)]
    pub book: Vec<CblBook>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "ReadingList")]
pub struct ReadingList {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Books", default)]
    pub books: CblBooks,
}

impl ReadingList {
    pub fn from_xml(xml: &str) -> Result<Self, crate::Error> {
        Ok(quick_xml::de::from_str(xml.trim_start_matches('\u{feff}'))?)
    }
    pub fn to_xml(&self) -> Result<String, crate::Error> {
        Ok(format!("<?xml version=\"1.0\"?>\n{}", quick_xml::se::to_string(self)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cbl() {
        let xml = r#"<?xml version="1.0"?>
<ReadingList xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <Name>Thanos</Name>
  <Books>
    <Book Series="Iron Man" Number="55" Volume="1968" Year="1973">
      <Id>a3f1c2d4-0000-0000-0000-000000000001</Id>
    </Book>
    <Book Series="Captain Marvel" Number="25" Volume="1968" Year="1973" />
  </Books>
  <Matchers />
</ReadingList>"#;
        let rl = ReadingList::from_xml(xml).unwrap();
        assert_eq!(rl.name, "Thanos");
        assert_eq!(rl.books.book.len(), 2);
        assert_eq!(rl.books.book[0].number, "55");
        assert!(rl.books.book[1].id.is_none());
        assert_eq!(ReadingList::from_xml(&rl.to_xml().unwrap()).unwrap(), rl);
    }
}
