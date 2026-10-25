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
//!
//! Ce module **lit** les listes. Leur modification (création, renommage, suppression,
//! copie, déplacement, livres d'une liste de lecture, critères d'une liste intelligente)
//! se fait sur les lignes de [`crate::lists_edit`], qui réécrit le XML au format de
//! ComicRack ; l'enregistrement va dans la base de ComicRust
//! ([`crate::store::Store::save_list_rows`]) ou dans `ComicDb.xml`
//! ([`crate::lists_edit::ComicDbLists`], seulement ComicRack fermé : il réécrit ce fichier
//! en se fermant et toutes les 10 minutes). Libellés et opérateurs des critères pour
//! l'éditeur : [`crate::matcher_catalog`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::db::ComicDatabase;
use crate::smart::{Limit, LimitKind, Matcher, MatcherSource, MatcherXml, Mode, Selection, SmartQuery, UnknownChild};
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
    /// Liste intelligente : critères, et résultat mémorisé par ComicRack s'il existe.
    Smart {
        query: Option<SmartQuery>,
        cached: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComicList {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: ListKind,
    /// Nombre de livres compté par ComicRack lors de son dernier enregistrement
    /// (`<BookCount>`, absent = 0) : sert à comparer avec le calcul de ComicRust.
    pub comicrack_count: usize,
}

/// Livres d'une liste : identifiants (en minuscules), ordonnés pour une liste de lecture.
#[derive(Debug, Clone, PartialEq)]
pub enum Members {
    /// Toute la bibliothèque.
    All,
    Ids(Vec<String>),
    /// Contenu inconnu, avec la raison (critère non pris en charge…).
    Unknown(String),
}

/// Combinaison des sous-listes d'un dossier (`ComicListItemFolder.OnCacheMatch`).
fn combine_parts(combine: Combine, parts: Vec<Members>) -> Members {
    if let Some(Members::Unknown(why)) = parts.iter().find(|p| matches!(p, Members::Unknown(_))) {
        if combine != Combine::Empty {
            return Members::Unknown(why.clone());
        }
    }
    match combine {
        Combine::Empty => Members::Ids(Vec::new()),
        Combine::Or => {
            if parts.contains(&Members::All) {
                return Members::All;
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

/// Contenu de toutes les listes (sous-dossiers compris), par identifiant de liste.
pub fn resolve_all(lists: &[ComicList], db: &ComicDatabase) -> HashMap<String, Members> {
    fn walk(l: &[ComicList], db: &ComicDatabase, all: &[ComicList], memo: &mut Memo) {
        for x in l {
            x.resolve_inner(Some(db), all, 0, memo);
            if let ListKind::Folder { children, .. } = &x.kind {
                walk(children, db, all, memo);
            }
        }
    }
    let mut memo = Memo::new();
    walk(lists, db, lists, &mut memo);
    memo
}

/// Profondeur maximale (une liste intelligente qui part d'elle-même, par exemple).
const MAX_DEPTH: usize = 24;

/// Résultats déjà calculés pendant une passe (chaque liste n'est calculée qu'une fois,
/// même quand un dossier ou une autre liste en dépend).
type Memo = HashMap<String, Members>;

impl ComicList {
    /// Contenu de la liste sans la base : les listes intelligentes ne sont connues que si
    /// ComicRack a mémorisé leur résultat.
    pub fn members(&self) -> Members {
        self.resolve_inner(None, &[], 0, &mut Memo::new())
    }

    /// Contenu de la liste d'après la base : les listes intelligentes sont calculées
    /// (`all` : toutes les listes, pour celles qui partent d'une autre liste).
    pub fn resolve(&self, db: &ComicDatabase, all: &[ComicList]) -> Members {
        self.resolve_inner(Some(db), all, 0, &mut Memo::new())
    }

    fn resolve_inner(&self, db: Option<&ComicDatabase>, all: &[ComicList], depth: usize, memo: &mut Memo) -> Members {
        if let Some(m) = memo.get(&self.id) {
            return m.clone();
        }
        let m = self.resolve_uncached(db, all, depth, memo);
        if !self.id.is_empty() {
            memo.insert(self.id.clone(), m.clone());
        }
        m
    }

    fn resolve_uncached(
        &self,
        db: Option<&ComicDatabase>,
        all: &[ComicList],
        depth: usize,
        memo: &mut Memo,
    ) -> Members {
        if depth > MAX_DEPTH {
            return Members::Unknown("liste qui dépend d'elle-même".into());
        }
        match &self.kind {
            ListKind::Library => Members::All,
            ListKind::Ids(ids) => Members::Ids(ids.clone()),
            ListKind::Folder { combine, children } => {
                let parts = children.iter().map(|c| c.resolve_inner(db, all, depth + 1, memo)).collect();
                combine_parts(*combine, parts)
            }
            ListKind::Smart { query, cached } => {
                let computed = match (db, query) {
                    (Some(db), Some(q)) => {
                        let missing = q.unsupported();
                        if missing.is_empty() {
                            Some(Self::compute(q, db, all, depth, memo))
                        } else {
                            // Résultat mémorisé par ComicRack s'il existe, sinon la raison.
                            if cached.is_none() {
                                return Members::Unknown(format!(
                                    "critère pas encore pris en charge : {}",
                                    missing.join(", ")
                                ));
                            }
                            None
                        }
                    }
                    _ => None,
                };
                match (computed, cached) {
                    (Some(m), _) => m,
                    (None, Some(ids)) => Members::Ids(ids.clone()),
                    (None, None) => Members::Unknown("contenu non mémorisé par ComicRack".into()),
                }
            }
        }
    }

    fn compute(q: &SmartQuery, db: &ComicDatabase, all: &[ComicList], depth: usize, memo: &mut Memo) -> Members {
        let n = db.books.len();
        let base: Option<Members> = q
            .base_list
            .as_deref()
            .and_then(|id| ComicList::find(all, id))
            .map(|l| l.resolve_inner(Some(db), all, depth + 1, memo));
        let in_base: Option<Vec<bool>> = match base {
            Some(Members::Unknown(why)) => return Members::Unknown(format!("liste de départ inconnue ({why})")),
            Some(Members::Ids(ids)) => {
                let mut v = vec![false; n];
                for id in ids {
                    if let Some(i) = db.index_of(&id) {
                        v[i] = true;
                    }
                }
                Some(v)
            }
            Some(Members::All) | None => None,
        };
        let candidates = (0..n).filter(|&i| match &in_base {
            Some(v) => v[i] != q.not_in_base,
            None => !q.not_in_base || q.base_list.is_none(),
        });
        let picked = q.select(&db.books, candidates);
        Members::Ids(picked.into_iter().map(|i| db.books[i].id.to_ascii_lowercase()).collect())
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

pub(crate) fn xml_err(e: impl std::fmt::Display) -> Error {
    Error::Database(format!("ComicDb.xml illisible : {e}"))
}

pub(crate) fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name).and_then(|a| a.unescape_value().ok()).map(|v| v.into())
}

pub(crate) fn local(name: &[u8]) -> &[u8] {
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
fn read_items(r: &mut Reader<&[u8]>, src: &str) -> Result<Vec<ComicList>, Error> {
    let mut out = Vec::new();
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) if local(e.name().as_ref()) == b"Item" => {
                let e = e.into_owned();
                out.push(read_item(r, &e, false, src)?);
            }
            Event::Empty(e) if local(e.name().as_ref()) == b"Item" => {
                let e = e.into_owned();
                out.push(read_item(r, &e, true, src)?);
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

fn read_item(r: &mut Reader<&[u8]>, e: &BytesStart, empty: bool, src: &str) -> Result<ComicList, Error> {
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
        comicrack_count: 0,
    };
    let mut children = Vec::new();
    let mut ids = Vec::new();
    let mut cache = None;
    let mut query = SmartQuery { mode: Mode::parse(attr(e, b"MatcherMode").as_deref()), ..Default::default() };
    let (mut limit_on, mut limit_kind, mut limit_value, mut limit_sel, mut limit_seed) =
        (false, LimitKind::Count, 25i64, Selection::Random, 0i64);
    if !empty {
        loop {
            match r.read_event().map_err(xml_err)? {
                Event::Start(c) => {
                    let c = c.into_owned();
                    match local(c.name().as_ref()) {
                        b"Items" => children = read_items(r, src)?,
                        b"BookIds" => ids = read_child_texts(r)?,
                        b"CacheStorage" => cache = Some(read_text(r)?),
                        b"Description" => list.description = read_text(r)?,
                        b"BookCount" => list.comicrack_count = read_text(r)?.trim().parse().unwrap_or(0),
                        b"Matchers" => query.matchers = read_matchers(r, src)?,
                        b"Limit" => limit_on = read_text(r)?.trim().eq_ignore_ascii_case("true"),
                        b"LimitType" => {
                            limit_kind = match read_text(r)?.trim() {
                                "MB" => LimitKind::Mb,
                                "GB" => LimitKind::Gb,
                                _ => LimitKind::Count,
                            }
                        }
                        b"LimitValue" => limit_value = read_text(r)?.trim().parse().unwrap_or(25),
                        b"LimitSelectionType" => {
                            limit_sel = match read_text(r)?.trim() {
                                "Position" => Selection::Position,
                                "SortedBySeries" => Selection::SortedBySeries,
                                _ => Selection::Random,
                            }
                        }
                        b"LimitRandomSeed" => limit_seed = read_text(r)?.trim().parse().unwrap_or(0),
                        b"BaseListId" => {
                            let id = read_text(r)?.trim().to_ascii_lowercase();
                            if !id.is_empty() && id != "00000000-0000-0000-0000-000000000000" {
                                query.base_list = Some(id);
                            }
                        }
                        b"NotInBaseList" => query.not_in_base = read_text(r)?.trim().eq_ignore_ascii_case("true"),
                        b"FilteredIds" => query.filtered = read_child_texts(r)?,
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
            query: Some(SmartQuery {
                limit: limit_on.then_some(Limit {
                    kind: limit_kind,
                    value: limit_value,
                    selection: limit_sel,
                    seed: limit_seed,
                }),
                ..query
            }),
            // « Custom » : résultat dépendant de la date, non mémorisé.
            cached: cache
                .filter(|c| !c.trim().is_empty() && c.trim() != "Custom")
                .map(|c| c.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect()),
        },
        _ => ListKind::Ids(ids),
    };
    Ok(list)
}

/// Critères d'une liste intelligente (`<Matchers>` : un élément par critère, son type
/// dans `xsi:type`). `src` : texte lu par `r` (pour garder le XML d'origine des critères).
fn read_matchers(r: &mut Reader<&[u8]>, src: &str) -> Result<Vec<Matcher>, Error> {
    let mut out = Vec::new();
    loop {
        let before = r.buffer_position() as usize;
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                let e = e.into_owned();
                out.push(read_matcher(r, &e, false, before, src)?);
            }
            Event::Empty(e) => {
                let e = e.into_owned();
                out.push(read_matcher(r, &e, true, before, src)?);
            }
            Event::End(_) => return Ok(out),
            Event::Eof => return Err(xml_err("fin de fichier inattendue")),
            _ => {}
        }
    }
}

/// Attributs d'un critère que ComicRust modélise (les autres sont gardés tels quels).
fn known_matcher_attr(group: bool, name: &str) -> bool {
    matches!(name, "xsi:type" | "Not")
        || if group { name == "MatcherMode" } else { matches!(name, "MatchOperator" | "IgnoreCase") }
}

/// Texte d'origine `[début, fin)` (positions du lecteur dans `src`), blancs de tête retirés.
fn source_text(src: &str, start: usize, end: usize) -> String {
    src.get(start..end).unwrap_or_default().trim_start().to_string()
}

/// Un critère ; `start` : position de sa balise ouvrante (pour garder son XML d'origine).
fn read_matcher(r: &mut Reader<&[u8]>, e: &BytesStart, empty: bool, start: usize, src: &str) -> Result<Matcher, Error> {
    let kind = attr(e, b"xsi:type").unwrap_or_default();
    let group = kind == "ComicBookGroupMatcher";
    let flag =
        |name: &[u8], default: bool| attr(e, name).map(|v| v.trim().eq_ignore_ascii_case("true")).unwrap_or(default);
    let not = flag(b"Not", false);
    let ignore_case = flag(b"IgnoreCase", true);
    let op = attr(e, b"MatchOperator").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    let mode = Mode::parse(attr(e, b"MatcherMode").as_deref());
    let mut source = MatcherSource { kind: kind.clone(), ..Default::default() };
    for a in e.attributes().flatten() {
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        if !known_matcher_attr(group, &key) {
            let v = a.unescape_value().map(|v| v.into_owned()).unwrap_or_default();
            source.attrs.push((key, v));
        }
    }
    let (mut value, mut value2, mut inner) = (String::new(), String::new(), Vec::new());
    if !empty {
        loop {
            let before = r.buffer_position() as usize;
            match r.read_event().map_err(xml_err)? {
                Event::Start(c) => {
                    let c = c.into_owned();
                    match (group, local(c.name().as_ref())) {
                        (false, b"MatchValue") => value = read_text(r)?,
                        (false, b"MatchValue2") => value2 = read_text(r)?,
                        (true, b"Matchers") => inner = read_matchers(r, src)?,
                        (_, name) => {
                            let name = String::from_utf8_lossy(name).into_owned();
                            let text = read_text(r)?;
                            let raw = source_text(src, before, r.buffer_position() as usize);
                            source.children.push(UnknownChild { name, raw, text });
                        }
                    }
                }
                Event::Empty(c) => {
                    let name = String::from_utf8_lossy(local(c.name().as_ref())).into_owned();
                    if !matches!((group, name.as_str()), (false, "MatchValue" | "MatchValue2") | (true, "Matchers")) {
                        let raw = source_text(src, before, r.buffer_position() as usize);
                        source.children.push(UnknownChild { name, raw, text: String::new() });
                    }
                }
                Event::End(_) => break,
                Event::Eof => return Err(xml_err("fin de fichier inattendue")),
                _ => {}
            }
        }
    }
    source.raw = source_text(src, start, r.buffer_position() as usize);
    let xml = MatcherXml::new(source);
    Ok(if group {
        Matcher::Group { mode, not, matchers: inner, xml }
    } else {
        Matcher::Value { kind, op, value, value2, not, ignore_case, xml }
    })
}

/// Un critère seul (élément `<ComicBookMatcher …>` complet), comme dans `<Matchers>`.
pub(crate) fn parse_matcher(raw: &str) -> Result<Matcher, Error> {
    let mut r = Reader::from_str(raw);
    loop {
        let before = r.buffer_position() as usize;
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                let e = e.into_owned();
                return read_matcher(&mut r, &e, false, before, raw);
            }
            Event::Empty(e) => {
                let e = e.into_owned();
                return read_matcher(&mut r, &e, true, before, raw);
            }
            Event::Eof => return Err(xml_err("critère vide")),
            _ => {}
        }
    }
}

/// Critères d'un élément `<Matchers>` complet.
pub(crate) fn parse_matchers(raw: &str) -> Result<Vec<Matcher>, Error> {
    let mut r = Reader::from_str(raw);
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(_) => return read_matchers(&mut r, raw),
            Event::Empty(_) | Event::Eof => return Ok(Vec::new()),
            _ => {}
        }
    }
}

/// Listes de ComicRack contenues dans un `ComicDb.xml`.
pub fn parse_comic_lists(xml: &str) -> Result<Vec<ComicList>, Error> {
    let text = xml.trim_start_matches('\u{feff}');
    let mut r = Reader::from_str(text);
    let mut depth = 0;
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 && local(e.name().as_ref()) == b"ComicLists" {
                    return read_items(&mut r, text);
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
        assert!(matches!(lists[3].members(), Members::Unknown(_)));
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

    #[test]
    fn smart_lists_computed_from_the_database() {
        use crate::library::LibraryBook;
        let mk = |id: &str, series: &str, writer: &str, read: u32| LibraryBook {
            id: id.into(),
            series: Some(series.into()),
            writer: Some(writer.into()),
            page_count: Some(20),
            last_page_read: Some(read),
            ..Default::default()
        };
        let db = ComicDatabase::from_books(vec![
            mk("A", "Thanos Quest", "Jim Starlin", 0),
            mk("B", "Warlock", "Jim Starlin", 19),
            mk("C", "Avengers", "Stan Lee", 0),
            mk("D", "Silver Surfer", "Jim Starlin, Ron Lim", 5),
        ]);
        // Format exact de XmlSerializer pour ComicSmartListItem / ComicBookGroupMatcher.
        let xml = r#"<?xml version="1.0"?>
<ComicDatabase xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <ComicLists>
    <Item xsi:type="ComicSmartListItem" Id="S1" Name="Starlin non lus">
      <Matchers>
        <ComicBookMatcher xsi:type="ComicBookWriterMatcher" MatchOperator="1"><MatchValue>Starlin</MatchValue></ComicBookMatcher>
        <ComicBookMatcher xsi:type="ComicBookReadPercentageMatcher" MatchOperator="1" Not="true"><MatchValue>0</MatchValue></ComicBookMatcher>
      </Matchers>
    </Item>
    <Item xsi:type="ComicSmartListItem" Id="S2" Name="Groupe Ou" MatcherMode="Or">
      <Matchers>
        <ComicBookMatcher xsi:type="ComicBookGroupMatcher" MatcherMode="And">
          <Matchers>
            <ComicBookMatcher xsi:type="ComicBookSeriesMatcher" MatchOperator="4"><MatchValue>Silver</MatchValue></ComicBookMatcher>
            <ComicBookMatcher xsi:type="ComicBookReadPercentageMatcher" MatchOperator="1"><MatchValue>0</MatchValue></ComicBookMatcher>
          </Matchers>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type="ComicBookSeriesMatcher"><MatchValue>avengers</MatchValue></ComicBookMatcher>
      </Matchers>
    </Item>
    <Item xsi:type="ComicSmartListItem" Id="S3" Name="Hors de S1, limité">
      <Matchers />
      <Limit>true</Limit><LimitValue>1</LimitValue><LimitSelectionType>SortedBySeries</LimitSelectionType>
      <BaseListId>S1</BaseListId><NotInBaseList>true</NotInBaseList>
    </Item>
    <Item xsi:type="ComicSmartListItem" Id="S4" Name="Stats">
      <Matchers><ComicBookMatcher xsi:type="SmartListSeriesCountMatcher" MatchOperator="1"><MatchValue>2</MatchValue></ComicBookMatcher></Matchers>
    </Item>
  </ComicLists>
</ComicDatabase>"#;
        let lists = parse_comic_lists(xml).unwrap();
        let ids = |l: &ComicList| match l.resolve(&db, &lists) {
            Members::Ids(v) => v,
            other => panic!("{other:?}"),
        };
        assert_eq!(ids(&lists[0]), ["a"], "Starlin et pas « lu > 0 % »");
        assert_eq!(ids(&lists[1]), ["c", "d"]);
        assert_eq!(ids(&lists[2]), ["c"], "hors de S1 (b, c, d), 1 livre trié par série → Avengers");
        assert!(matches!(lists[3].resolve(&db, &lists), Members::Unknown(w) if w.contains("statistique de série")));
    }

    /// `COMICRUST_TEST_COMICDB` = chemin d'un vrai `ComicDb.xml` : toutes les listes sont lues
    /// et tous les critères des listes intelligentes sont pris en charge.
    #[test]
    fn real_comicdb() {
        let Ok(path) = std::env::var("COMICRUST_TEST_COMICDB") else { return };
        let lists = load_comic_lists(Path::new(&path)).unwrap();
        fn walk(l: &[ComicList], depth: usize, smart: &mut usize, missing: &mut Vec<String>) {
            for x in l {
                let kind = match &x.kind {
                    ListKind::Library => "bibliothèque",
                    ListKind::Folder { .. } => "dossier",
                    ListKind::Ids(_) => "liste",
                    ListKind::Smart { query, .. } => {
                        *smart += 1;
                        let q = query.as_ref().unwrap();
                        for m in q.unsupported() {
                            missing.push(format!("{} : {m}", x.name));
                        }
                        "intelligente"
                    }
                };
                eprintln!("{}{} [{kind}] ComicRack : {}", "  ".repeat(depth), x.name, x.comicrack_count);
                if let ListKind::Folder { children, .. } = &x.kind {
                    walk(children, depth + 1, smart, missing);
                }
            }
        }
        let (mut smart, mut missing) = (0, Vec::new());
        walk(&lists, 0, &mut smart, &mut missing);
        eprintln!("{} listes intelligentes, critères non pris en charge : {missing:?}", smart);
        // Temps de calcul de toutes les listes sur une bibliothèque de la taille de celle de Yoyo.
        use crate::library::LibraryBook;
        let books: Vec<LibraryBook> = (0..5400)
            .map(|i| LibraryBook {
                id: format!("{i:08}-0000-0000-0000-000000000000"),
                series: Some(format!("Série {}", i / 12)),
                number: Some((i % 12 + 1).to_string()),
                file: Some(format!(r"\\NAS\_BD\Séries Terminées\Série {}\T{:02}.cbz", i / 12, i % 12 + 1)),
                series_complete: Some(if i % 3 == 0 { "Yes" } else { "Unknown" }.into()),
                page_count: Some(48),
                last_page_read: Some((i % 50) as u32),
                ..Default::default()
            })
            .collect();
        let db = ComicDatabase::from_books(books);
        let t = std::time::Instant::now();
        fn each(l: &[ComicList], db: &ComicDatabase) {
            for x in l {
                if let ListKind::Smart { query: Some(q), .. } = &x.kind {
                    let t = std::time::Instant::now();
                    let n = q.select(&db.books, 0..db.books.len()).len();
                    eprintln!("    {:>10.2?}  {n:>5}  {}", t.elapsed(), x.name);
                }
                if let ListKind::Folder { children, .. } = &x.kind {
                    each(children, db);
                }
            }
        }
        each(&lists, &db);
        let resolved = resolve_all(&lists, &db);
        assert!(resolved.len() >= 40);
        eprintln!("toutes les listes calculées sur 5400 livres en {:?}", t.elapsed());
        assert!(smart > 0);
        assert!(missing.is_empty(), "{missing:?}");
    }
}
