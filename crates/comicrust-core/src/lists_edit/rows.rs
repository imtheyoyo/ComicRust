//! Découpage de `<ComicLists>` en lignes (une par liste, tous niveaux) et réassemblage.
//!
//! Partagé par le transfert depuis ComicRack ([`crate::store::import`]), la base de
//! ComicRust (table `comicrust_lists`) et l'édition des listes ([`super::ListsEditor`]).

use std::collections::{HashMap, HashSet};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::comic_lists::{attr, local, xml_err};
use crate::Error;

/// Une liste (à n'importe quel niveau) : une ligne de `comicrust_lists`.
#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    /// Identifiant (GUID en minuscules).
    pub id: String,
    /// Dossier parent (`None` : premier niveau).
    pub parent: Option<String>,
    /// Rang parmi les listes du même dossier.
    pub position: i64,
    pub name: String,
    /// `xsi:type` de ComicRack (`ComicIdListItem`, `ComicSmartListItem`…).
    pub kind: String,
    /// Élément `<Item>` d'origine, octet pour octet, sans les enfants d'un dossier
    /// (il reste `<Items></Items>`).
    pub xml: String,
}

/// Espace de noms des identifiants de liste calculés ([`stable_list_id`], UUID v5).
const LIST_ID_NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x8c1f_4a52_6d3e_4b0a_9e57_c0b1_c5a7_0001);

/// Identifiant **stable** d'une liste sans identifiant (ou dont l'identifiant est déjà
/// pris) : UUID v5 de (parent, position, nom, rang d'occurrence). Le même `ComicDb.xml`
/// donne donc les mêmes identifiants à chaque transfert (sinon un transfert relancé
/// ajouterait une copie de ces listes). Le rang d'occurrence départage deux calculs
/// identiques (identifiant déjà vu).
fn stable_list_id(parent: Option<&str>, position: i64, name: &str, seen: &HashSet<String>) -> String {
    (0u32..)
        .map(|n| {
            let key = format!("{}\u{1f}{position}\u{1f}{name}\u{1f}{n}", parent.unwrap_or(""));
            uuid::Uuid::new_v5(&LIST_ID_NAMESPACE, key.as_bytes()).to_string()
        })
        .find(|id| !seen.contains(id))
        .unwrap_or_default()
}

/// Découpe `<ComicLists>` d'un `ComicDb.xml` en lignes (une par `<Item>`, tous niveaux).
/// Identifiant absent ou en double : remplacé par un identifiant calculé, stable d'un
/// transfert à l'autre (UUID v5), **pour la ligne** seulement (le XML garde
/// l'original), avec un avertissement.
pub fn split_lists(comicdb_xml: &str, warnings: &mut Vec<String>) -> Result<Vec<ListRow>, Error> {
    let text = comicdb_xml.trim_start_matches('\u{feff}');
    let mut r = Reader::from_str(text);
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    let mut depth = 0;
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 && local(e.name().as_ref()) == b"ComicLists" {
                    let mut sp = Splitter { text, rows: &mut rows, seen: &mut seen, warnings };
                    sp.items(&mut r, None)?;
                    return Ok(rows);
                }
                if depth == 2 {
                    let e = e.into_owned();
                    r.read_to_end(e.name()).map_err(xml_err)?;
                    depth -= 1;
                }
            }
            Event::End(_) => depth -= 1,
            Event::Eof => return Ok(rows),
            _ => {}
        }
    }
}

struct Splitter<'a, 'w> {
    text: &'a str,
    rows: &'a mut Vec<ListRow>,
    seen: &'a mut HashSet<String>,
    warnings: &'w mut Vec<String>,
}

impl Splitter<'_, '_> {
    /// Lit les `<Item>` d'un conteneur jusqu'à sa balise fermante ; renvoie la position
    /// (octets) du début de cette balise fermante.
    fn items(&mut self, r: &mut Reader<&[u8]>, parent: Option<&str>) -> Result<usize, Error> {
        let mut position = 0i64;
        loop {
            let before = r.buffer_position() as usize;
            match r.read_event().map_err(xml_err)? {
                Event::Start(e) if local(e.name().as_ref()) == b"Item" => {
                    let e = e.into_owned();
                    self.item(r, &e, before, false, parent, position)?;
                    position += 1;
                }
                Event::Empty(e) if local(e.name().as_ref()) == b"Item" => {
                    let e = e.into_owned();
                    self.item(r, &e, before, true, parent, position)?;
                    position += 1;
                }
                Event::Start(e) => {
                    let e = e.into_owned();
                    r.read_to_end(e.name()).map_err(xml_err)?;
                }
                Event::End(_) => return Ok(before),
                Event::Eof => return Err(xml_err("fin de fichier inattendue")),
                _ => {}
            }
        }
    }

    fn item(
        &mut self,
        r: &mut Reader<&[u8]>,
        e: &quick_xml::events::BytesStart,
        start: usize,
        empty: bool,
        parent: Option<&str>,
        position: i64,
    ) -> Result<(), Error> {
        let name = attr(e, b"Name").unwrap_or_default();
        let kind = attr(e, b"xsi:type").unwrap_or_default();
        let mut id = attr(e, b"Id").unwrap_or_default().trim().to_ascii_lowercase();
        if id.is_empty() || !self.seen.insert(id.clone()) {
            let new = stable_list_id(parent, position, &name, self.seen);
            self.warnings.push(format!(
                "liste « {name} » : identifiant {} — enregistrée sous {new}",
                if id.is_empty() { "absent".to_string() } else { format!("{id} en double") }
            ));
            self.seen.insert(new.clone());
            id = new;
        }
        let index = self.rows.len();
        self.rows.push(ListRow {
            id: id.clone(),
            parent: parent.map(str::to_string),
            position,
            name,
            kind,
            xml: String::new(),
        });
        let mut hole: Option<(usize, usize)> = None;
        let end = if empty {
            r.buffer_position() as usize
        } else {
            loop {
                match r.read_event().map_err(xml_err)? {
                    Event::Start(c) if local(c.name().as_ref()) == b"Items" && hole.is_none() => {
                        let inner_start = r.buffer_position() as usize;
                        let inner_end = self.items(r, Some(&id))?;
                        hole = Some((inner_start, inner_end));
                    }
                    Event::Start(c) => {
                        let c = c.into_owned();
                        r.read_to_end(c.name()).map_err(xml_err)?;
                    }
                    Event::End(_) => break r.buffer_position() as usize,
                    Event::Eof => return Err(xml_err("fin de fichier inattendue")),
                    _ => {}
                }
            }
        };
        let xml = match hole {
            Some((a, b)) => format!("{}{}", &self.text[start..a], &self.text[b..end]),
            None => self.text[start..end].to_string(),
        };
        self.rows[index].xml = xml;
        Ok(())
    }
}

/// Insère `children` (XML des enfants, déjà assemblé) dans le `<Item>` `xml` d'un
/// dossier : dans son `<Items>` direct (vide ou `<Items/>`), sinon dans un `<Items>`
/// ajouté avant la fermeture de l'élément.
fn insert_children(xml: &str, children: &str) -> String {
    let mut r = Reader::from_str(xml);
    let mut depth = 0;
    let mut last_end = None;
    loop {
        let before = r.buffer_position() as usize;
        let Ok(ev) = r.read_event() else { break };
        match ev {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 && local(e.name().as_ref()) == b"Items" {
                    let at = r.buffer_position() as usize;
                    return format!("{}{children}{}", &xml[..at], &xml[at..]);
                }
            }
            Event::Empty(e) => {
                if depth == 1 && local(e.name().as_ref()) == b"Items" {
                    let after = r.buffer_position() as usize;
                    return format!("{}<Items>{children}</Items>{}", &xml[..before], &xml[after..]);
                }
                if depth == 0 {
                    // `<Item … />` : élément vide transformé en élément avec enfants.
                    let after = r.buffer_position() as usize;
                    let tag = xml[before..after].trim_end_matches('>').trim_end_matches('/').trim_end();
                    let qname = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    return format!("{tag}><Items>{children}</Items></{qname}>{}", &xml[after..]);
                }
            }
            Event::End(_) => {
                if depth == 1 {
                    last_end = Some(before);
                }
                depth -= 1;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    match last_end {
        Some(at) => format!("{}<Items>{children}</Items>{}", &xml[..at], &xml[at..]),
        None => xml.to_string(),
    }
}

/// Reconstruit le contenu de `<ComicLists>` à partir des lignes (ordre `position`, puis
/// identifiant ; parent introuvable = premier niveau).
pub fn assemble_lists(rows: &[ListRow]) -> String {
    let ids: HashSet<String> = rows.iter().map(|r| r.id.to_ascii_lowercase()).collect();
    let mut children: HashMap<Option<String>, Vec<&ListRow>> = HashMap::new();
    for r in rows {
        let parent = r.parent.as_ref().map(|p| p.to_ascii_lowercase()).filter(|p| ids.contains(p));
        children.entry(parent).or_default().push(r);
    }
    for v in children.values_mut() {
        v.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
    }
    fn render(
        row: &ListRow,
        children: &HashMap<Option<String>, Vec<&ListRow>>,
        visiting: &mut HashSet<String>,
    ) -> String {
        let id = row.id.to_ascii_lowercase();
        let kids = match children.get(&Some(id.clone())) {
            Some(k) if visiting.insert(id.clone()) => {
                let s: String = k.iter().map(|c| render(c, children, visiting)).collect();
                visiting.remove(&id);
                s
            }
            _ => String::new(),
        };
        if kids.is_empty() {
            row.xml.clone()
        } else {
            insert_children(&row.xml, &kids)
        }
    }
    let mut visiting = HashSet::new();
    children.get(&None).map(|top| top.iter().map(|r| render(r, &children, &mut visiting)).collect()).unwrap_or_default()
}
