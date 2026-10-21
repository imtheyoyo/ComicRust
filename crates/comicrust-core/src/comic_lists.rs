//! Listes de la bibliothèque ComicRack (`<ComicLists>` de `ComicDb.xml`).
//!
//! Même quand les livres sont dans MySQL, ComicRack garde ses listes dans `ComicDb.xml`
//! (`DatabaseManager.Save` : la base est enregistrée « sans les livres »). Chaque
//! `<Item>` porte son type dans `xsi:type` :
//!
//! | `xsi:type` | contenu |
//! |---|---|
//! | `ComicLibraryListItem` | toute la bibliothèque |
//! | `ComicListItemFolder` | dossier : `<Items>` + `CombineMode` (`Or`, `And`, `Empty`) |
//! | `ComicIdListItem` | liste de lecture : `<BookIds>` (identifiants, dans l'ordre) |
//! | `ComicSmartListItem` | liste intelligente : `<Matchers>`, et parfois `<CacheStorage>` (identifiants séparés par des virgules, résultat mémorisé par ComicRack) |

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Combine {
    #[default]
    Or,
    And,
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListKind {
    Library,
    Folder {
        combine: Combine,
        children: Vec<ComicList>,
    },
    /// Liste de lecture : identifiants dans l'ordre de la liste.
    Ids(Vec<String>),
    /// Liste intelligente : résultat mémorisé par ComicRack s'il existe.
    Smart {
        cached: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComicList {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: ListKind,
}

/// Livres d'une liste : identifiants (en minuscules), ordonnés pour une liste de lecture.
#[derive(Debug, Clone, PartialEq)]
pub enum Members {
    /// Toute la bibliothèque.
    All,
    Ids(Vec<String>),
    /// Contenu inconnu (liste intelligente sans résultat mémorisé).
    Unknown,
}

impl ComicList {
    /// Contenu de la liste, en combinant les sous-listes d'un dossier comme ComicRack
    /// (`ComicListItemFolder.OnCacheMatch`).
    pub fn members(&self) -> Members {
        match &self.kind {
            ListKind::Library => Members::All,
            ListKind::Ids(ids) => Members::Ids(ids.clone()),
            ListKind::Smart { cached: Some(ids) } => Members::Ids(ids.clone()),
            ListKind::Smart { cached: None } => Members::Unknown,
            ListKind::Folder { combine, children } => {
                let parts: Vec<Members> = children.iter().map(ComicList::members).collect();
                match combine {
                    Combine::Empty => Members::Ids(Vec::new()),
                    Combine::Or => {
                        if parts.contains(&Members::All) {
                            return Members::All;
                        }
                        if parts.contains(&Members::Unknown) {
                            return Members::Unknown;
                        }
                        let mut seen = HashSet::new();
                        let mut out = Vec::new();
                        for p in parts {
                            if let Members::Ids(ids) = p {
                                out.extend(ids.into_iter().filter(|i| seen.insert(i.clone())));
                            }
                        }
                        Members::Ids(out)
                    }
                    Combine::And => {
                        if parts.contains(&Members::Unknown) {
                            return Members::Unknown;
                        }
                        let sets: Vec<Vec<String>> = parts
                            .into_iter()
                            .filter_map(|p| match p {
                                Members::Ids(v) => Some(v),
                                _ => None,
                            })
                            .collect();
                        let Some((first, rest)) = sets.split_first() else { return Members::All };
                        let rest: Vec<HashSet<&String>> = rest.iter().map(|v| v.iter().collect()).collect();
                        Members::Ids(first.iter().filter(|i| rest.iter().all(|s| s.contains(i))).cloned().collect())
                    }
                }
            }
        }
    }

    /// Nombre de listes (sous-dossiers compris).
    pub fn count(&self) -> usize {
        1 + match &self.kind {
            ListKind::Folder { children, .. } => children.iter().map(ComicList::count).sum(),
            _ => 0,
        }
    }

    /// Recherche une liste par identifiant dans un arbre.
    pub fn find<'a>(lists: &'a [ComicList], id: &str) -> Option<&'a ComicList> {
        for l in lists {
            if l.id.eq_ignore_ascii_case(id) {
                return Some(l);
            }
            if let ListKind::Folder { children, .. } = &l.kind {
                if let Some(f) = Self::find(children, id) {
                    return Some(f);
                }
            }
        }
        None
    }
}

fn xml_err(e: impl std::fmt::Display) -> Error {
    Error::Database(format!("ComicDb.xml illisible : {e}"))
}

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name).and_then(|a| a.unescape_value().ok()).map(|v| v.into())
}

fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|&c| c == b':').next().unwrap_or(name)
}

/// Texte d'un élément simple (jusqu'à sa balise fermante).
fn read_text(r: &mut Reader<&[u8]>) -> Result<String, Error> {
    let mut out = String::new();
    let mut depth = 0;
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Text(t) => out.push_str(&t.unescape().map_err(xml_err)?),
            Event::CData(t) => out.push_str(&String::from_utf8_lossy(&t)),
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                if depth == 0 {
                    return Ok(out);
                }
                depth -= 1;
            }
            Event::Eof => return Err(xml_err("fin de fichier inattendue")),
            _ => {}
        }
    }
}

/// Textes des sous-éléments (`<BookIds><guid>…</guid>…</BookIds>`).
fn read_child_texts(r: &mut Reader<&[u8]>) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(_) => {
                let t = read_text(r)?;
                let t = t.trim();
                if !t.is_empty() {
                    out.push(t.to_ascii_lowercase());
                }
            }
            Event::End(_) => return Ok(out),
            Event::Eof => return Err(xml_err("fin de fichier inattendue")),
            _ => {}
        }
    }
}

fn skip(r: &mut Reader<&[u8]>, start: &BytesStart) -> Result<(), Error> {
    r.read_to_end(start.name()).map_err(xml_err)?;
    Ok(())
}

/// Lit les `<Item>` jusqu'à la balise fermante du conteneur.
fn read_items(r: &mut Reader<&[u8]>) -> Result<Vec<ComicList>, Error> {
    let mut out = Vec::new();
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) if local(e.name().as_ref()) == b"Item" => {
                let e = e.into_owned();
                out.push(read_item(r, &e, false)?);
            }
            Event::Empty(e) if local(e.name().as_ref()) == b"Item" => {
                let e = e.into_owned();
                out.push(read_item(r, &e, true)?);
            }
            Event::Start(e) => {
                let e = e.into_owned();
                skip(r, &e)?;
            }
            Event::End(_) => return Ok(out),
            Event::Eof => return Err(xml_err("fin de fichier inattendue")),
            _ => {}
        }
    }
}

fn read_item(r: &mut Reader<&[u8]>, e: &BytesStart, empty: bool) -> Result<ComicList, Error> {
    let ty = attr(e, b"xsi:type").unwrap_or_default();
    let combine = match attr(e, b"CombineMode").as_deref() {
        Some("And") => Combine::And,
        Some("Empty") => Combine::Empty,
        _ => Combine::Or,
    };
    let mut list = ComicList {
        id: attr(e, b"Id").unwrap_or_default().to_ascii_lowercase(),
        name: attr(e, b"Name").unwrap_or_default(),
        description: String::new(),
        kind: ListKind::Ids(Vec::new()),
    };
    let mut children = Vec::new();
    let mut ids = Vec::new();
    let mut cache = None;
    if !empty {
        loop {
            match r.read_event().map_err(xml_err)? {
                Event::Start(c) => {
                    let c = c.into_owned();
                    match local(c.name().as_ref()) {
                        b"Items" => children = read_items(r)?,
                        b"BookIds" => ids = read_child_texts(r)?,
                        b"CacheStorage" => cache = Some(read_text(r)?),
                        b"Description" => list.description = read_text(r)?,
                        _ => skip(r, &c)?,
                    }
                }
                Event::End(_) => break,
                Event::Eof => return Err(xml_err("fin de fichier inattendue")),
                _ => {}
            }
        }
    }
    list.kind = match ty.as_str() {
        "ComicLibraryListItem" => ListKind::Library,
        "ComicListItemFolder" => ListKind::Folder { combine, children },
        "ComicSmartListItem" => ListKind::Smart {
            // « Custom » : résultat dépendant de la date, non mémorisé.
            cached: cache
                .filter(|c| !c.trim().is_empty() && c.trim() != "Custom")
                .map(|c| c.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect()),
        },
        _ => ListKind::Ids(ids),
    };
    Ok(list)
}

/// Listes de ComicRack contenues dans un `ComicDb.xml`.
pub fn parse_comic_lists(xml: &str) -> Result<Vec<ComicList>, Error> {
    let mut r = Reader::from_str(xml.trim_start_matches('\u{feff}'));
    let mut depth = 0;
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 && local(e.name().as_ref()) == b"ComicLists" {
                    return read_items(&mut r);
                }
                if depth == 2 {
                    // <Books> peut être énorme : sauté d'un bloc.
                    let e = e.into_owned();
                    skip(&mut r, &e)?;
                    depth -= 1;
                }
            }
            Event::End(_) => depth -= 1,
            Event::Eof => return Ok(Vec::new()),
            _ => {}
        }
    }
}

pub fn load_comic_lists(path: &Path) -> Result<Vec<ComicList>, Error> {
    let bytes = std::fs::read(path)?;
    parse_comic_lists(&String::from_utf8_lossy(&bytes))
}

/// Emplacements possibles de `ComicDb.xml` d'après un `ComicRack.ini` (même logique que
/// `SystemPaths` : `DatabasePath`, puis `UseLocalSettings` → dossier `Data` à côté de
/// l'exe, puis `%APPDATA%\cYo\ComicRack Community Edition`), le plus probable d'abord.
pub fn comicdb_candidates(ini_path: &Path, ini_text: &str, appdata: Option<&Path>) -> Vec<PathBuf> {
    let value = |key: &str| {
        ini_text.lines().map(str::trim).filter(|l| !l.starts_with(';') && !l.starts_with('#')).find_map(|l| {
            let (k, v) = l.split_once('=')?;
            k.trim().eq_ignore_ascii_case(key).then(|| v.trim().trim_matches('"').to_string())
        })
    };
    let alt = value("AlternateConfig").filter(|v| !v.is_empty());
    let with_alt = |d: PathBuf| match &alt {
        Some(a) => d.join("Configurations").join(a),
        None => d,
    };
    let exe_dir = ini_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let local = value("UseLocalSettings").is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1");
    let mut out = Vec::new();
    if let Some(db) = value("DatabasePath").filter(|v| !v.is_empty()) {
        out.push(PathBuf::from(db).join("ComicDb.xml"));
    }
    let local_dir = with_alt(exe_dir.join("Data")).join("ComicDb.xml");
    let roaming = appdata.map(|a| with_alt(a.join("cYo").join("ComicRack Community Edition")).join("ComicDb.xml"));
    if local {
        out.push(local_dir);
        out.extend(roaming);
    } else {
        out.extend(roaming);
        out.push(local_dir);
    }
    out
}

/// Premier `ComicDb.xml` existant pour ce `ComicRack.ini`.
pub fn find_comicdb(ini_path: &Path) -> Option<PathBuf> {
    let bytes = std::fs::read(ini_path).ok()?;
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        let u16s: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    comicdb_candidates(ini_path, &text, appdata.as_deref()).into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrait au format exact de `XmlSerializer` (ComicRack CE, stockage MySQL : pas de livres).
    const DB: &str = r#"<?xml version="1.0"?>
<ComicDatabase xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Id="9a7e">
  <Books />
  <ComicLists>
    <Item xsi:type="ComicLibraryListItem" Id="LIB" Name="Bibliothèque" />
    <Item xsi:type="ComicListItemFolder" Id="F1" Name="Lectures &amp; arcs" CombineMode="Or">
      <Items>
        <Item xsi:type="ComicIdListItem" Id="L1" Name="Thanos">
          <Description>Ordre de lecture</Description>
          <BookIds><guid>B</guid><guid>A</guid></BookIds>
        </Item>
        <Item xsi:type="ComicIdListItem" Id="L2" Name="Warlock"><BookIds><guid>C</guid><guid>a</guid></BookIds></Item>
      </Items>
    </Item>
    <Item xsi:type="ComicSmartListItem" Id="S1" Name="Non lus" MatcherMode="And">
      <Matchers><ComicBookMatcher xsi:type="ComicBookReadPercentageMatcher" MatchOperator="1"><MatchValue>0</MatchValue></ComicBookMatcher></Matchers>
      <CacheStorage>D,E</CacheStorage>
    </Item>
    <Item xsi:type="ComicSmartListItem" Id="S2" Name="Récents"><CacheStorage>Custom</CacheStorage></Item>
    <Item xsi:type="ComicListItemFolder" Id="F2" Name="Croisement" CombineMode="And">
      <Items>
        <Item xsi:type="ComicIdListItem" Id="L3" Name="x"><BookIds><guid>A</guid><guid>B</guid></BookIds></Item>
        <Item xsi:type="ComicIdListItem" Id="L4" Name="y"><BookIds><guid>B</guid></BookIds></Item>
      </Items>
    </Item>
  </ComicLists>
</ComicDatabase>"#;

    #[test]
    fn parse_tree_and_members() {
        let lists = parse_comic_lists(DB).unwrap();
        assert_eq!(lists.len(), 5);
        assert_eq!(lists[0].kind, ListKind::Library);
        assert_eq!(lists[1].name, "Lectures & arcs");
        assert_eq!(lists[1].count(), 3);
        let thanos = ComicList::find(&lists, "l1").unwrap();
        assert_eq!(thanos.description, "Ordre de lecture");
        assert_eq!(thanos.members(), Members::Ids(vec!["b".into(), "a".into()]));
        // Dossier « Ou » : union sans doublon, dans l'ordre.
        assert_eq!(lists[1].members(), Members::Ids(vec!["b".into(), "a".into(), "c".into()]));
        assert_eq!(lists[2].members(), Members::Ids(vec!["d".into(), "e".into()]));
        assert_eq!(lists[3].members(), Members::Unknown);
        // Dossier « Et » : intersection.
        assert_eq!(lists[4].members(), Members::Ids(vec!["b".into()]));
        assert_eq!(lists[0].members(), Members::All);
    }

    #[test]
    fn comicdb_location() {
        let dir = Path::new("NAS").join("BD").join("ComicRack");
        let ini = dir.join("ComicRack.ini");
        let ini = ini.as_path();
        let appdata = Path::new("Roaming");
        let c = comicdb_candidates(ini, "DataSource=mysql:Server=x\r\nUseLocalSettings=true\r\n", Some(appdata));
        assert_eq!(c[0], dir.join("Data").join("ComicDb.xml"));
        let c = comicdb_candidates(ini, "DataSource=mysql:Server=x\r\n", Some(appdata));
        assert_eq!(c[0], appdata.join("cYo").join("ComicRack Community Edition").join("ComicDb.xml"));
        let c = comicdb_candidates(ini, "DatabasePath=D:\\Base\r\n", Some(appdata));
        assert_eq!(c[0], Path::new("D:\\Base").join("ComicDb.xml"));
    }
}
