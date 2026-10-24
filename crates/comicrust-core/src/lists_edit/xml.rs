//! Écriture XML au format de ComicRack (`XmlSerializer` .NET, `XmlTextWriter` indenté de
//! deux espaces) et modification **locale** d'un élément `<Item>` : seuls les attributs et
//! sous-éléments touchés sont réécrits, tout le reste est gardé octet pour octet.
//!
//! Règles de `XmlSerializer` reprises (vérifiées dans les classes de ComicRack CE) :
//! - membres de la classe de base d'abord (attributs comme éléments) : `ComicListItem`
//!   (`Favorite` ; `BookCount`, `NewBookCount`, `NewBookCountDate`, `UnreadBookCount`,
//!   `Description`, `CacheStorage`, `Display`), puis `ShareableComicListItem` (`QuickOpen`),
//!   puis la classe finale (dossier : `Collapsed`, `Temporary`, `CombineMode` ; `Items` —
//!   liste de lecture : `BookIds` — liste intelligente : `MatcherMode` ; `Matchers`,
//!   `Limit`, `LimitType`, `LimitValue`, `LimitSelectionType`, `LimitRandomSeed`,
//!   `BaseListId`, `NotInBaseList`, `FilteredIds`, `ShowFiltered`) ;
//! - valeur égale au `[DefaultValue]` : omise ; collection en lecture seule (`Items`,
//!   `BookIds`, `Matchers`) : toujours écrite, `<Matchers />` si vide ; `BaseListId` omis
//!   quand il vaut `Guid.Empty` (`BaseListIdSpecified`) ; `FilteredIds` omis s'il est vide
//!   (`ShouldSerializeFilteredIds`) ;
//! - critère (`ComicBookMatcher`) : attributs `xsi:type`, `Not` (défaut `false`), `Name`
//!   (défaut vide), `MatchOperator` (défaut 0), `IgnoreCase` (critères texte seulement,
//!   défaut `true`) ; éléments `<MatchValue>` (toujours écrit, `<MatchValue />` si vide) et
//!   `<MatchValue2>` (omis si vide) ; groupe (`ComicBookGroupMatcher`) : `Not`,
//!   `MatcherMode` (défaut `And`), `<Matchers>` ;
//! - échappement de `XmlTextWriter` : `&`, `<`, `>` dans le texte (et `\r` en `&#xD;`) ;
//!   en plus `"`, tabulation, retours à la ligne (`&#x9;`, `&#xA;`, `&#xD;`) dans les
//!   attributs ; GUID en minuscules.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::comic_lists::{local, xml_err};
use crate::smart::{LimitKind, Matcher, Mode, Selection, SmartQuery};
use crate::Error;

/// Échappe un texte d'élément comme `XmlTextWriter`.
pub(crate) fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#xD;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => out.push_str(&format!("&#x{:X};", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Échappe une valeur d'attribut comme `XmlTextWriter` (guillemets doubles).
pub(crate) fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            c if (c as u32) < 0x20 => out.push_str(&format!("&#x{:X};", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Élément simple `<Nom>texte</Nom>` (`<Nom />` si le texte est vide, comme `XmlSerializer`).
pub(crate) fn text_element(name: &str, text: &str) -> String {
    if text.is_empty() {
        format!("<{name} />")
    } else {
        format!("<{name}>{}</{name}>", escape_text(text))
    }
}

/// Élément contenant des sous-éléments, un par ligne (fin de ligne `nl`, `\r\n` dans un
/// `ComicDb.xml` écrit sous Windows), indenté sous `ind` (indentation de l'élément
/// lui-même) ; `<Nom />` sans sous-élément.
pub(crate) fn block_element(name: &str, attrs: &str, children: &[String], ind: &str, nl: &str) -> String {
    if children.is_empty() {
        return format!("<{name}{attrs} />");
    }
    let inner = format!("{ind}  ");
    let mut out = format!("<{name}{attrs}>");
    for c in children {
        out.push_str(nl);
        out.push_str(&inner);
        out.push_str(c);
    }
    out.push_str(nl);
    out.push_str(ind);
    out.push_str(&format!("</{name}>"));
    out
}

/// `<BookIds>` / `<FilteredIds>` : un `<guid>` par livre, en minuscules.
pub(crate) fn guid_list(name: &str, ids: &[String], ind: &str, nl: &str) -> String {
    let kids: Vec<String> = ids.iter().map(|i| text_element("guid", &i.trim().to_ascii_lowercase())).collect();
    block_element(name, "", &kids, ind, nl)
}

/// Critères qui portent l'attribut `IgnoreCase` (classes dérivées de `ComicBookStringMatcher`).
fn has_ignore_case(kind: &str) -> bool {
    match crate::matcher_catalog::matcher_info(kind) {
        Some(info) => info.value.is_text(),
        // Type inconnu : écrit seulement s'il diffère du défaut (attribut ignoré sinon).
        None => true,
    }
}

/// Un critère (`<ComicBookMatcher xsi:type=…>`), indenté sous `ind`.
pub(crate) fn matcher_xml(m: &Matcher, ind: &str, nl: &str) -> String {
    match m {
        Matcher::Value { kind, op, value, value2, not, ignore_case } => {
            let mut attrs = format!(" xsi:type=\"{}\"", escape_attr(kind));
            if *not {
                attrs.push_str(" Not=\"true\"");
            }
            if *op != 0 {
                attrs.push_str(&format!(" MatchOperator=\"{op}\""));
            }
            if !*ignore_case && has_ignore_case(kind) {
                attrs.push_str(" IgnoreCase=\"false\"");
            }
            let mut kids = vec![text_element("MatchValue", value)];
            if !value2.is_empty() {
                kids.push(text_element("MatchValue2", value2));
            }
            block_element("ComicBookMatcher", &attrs, &kids, ind, nl)
        }
        Matcher::Group { mode, not, matchers } => {
            let mut attrs = String::from(" xsi:type=\"ComicBookGroupMatcher\"");
            if *not {
                attrs.push_str(" Not=\"true\"");
            }
            if *mode == Mode::Or {
                attrs.push_str(" MatcherMode=\"Or\"");
            }
            let inner = format!("{ind}  ");
            let kids = vec![matchers_xml(matchers, &inner, nl)];
            block_element("ComicBookMatcher", &attrs, &kids, ind, nl)
        }
    }
}

/// `<Matchers>` (toujours écrit : `<Matchers />` sans critère).
pub(crate) fn matchers_xml(matchers: &[Matcher], ind: &str, nl: &str) -> String {
    let inner = format!("{ind}  ");
    let kids: Vec<String> = matchers.iter().map(|m| matcher_xml(m, &inner, nl)).collect();
    block_element("Matchers", "", &kids, ind, nl)
}

pub(crate) fn limit_kind_name(k: LimitKind) -> &'static str {
    match k {
        LimitKind::Count => "Count",
        LimitKind::Mb => "MB",
        LimitKind::Gb => "GB",
    }
}

pub(crate) fn selection_name(s: Selection) -> &'static str {
    match s {
        Selection::Position => "Position",
        Selection::SortedBySeries => "SortedBySeries",
        Selection::Random => "Random",
    }
}

// ---------------------------------------------------------------------------
// Élément `<Item>` découpé
// ---------------------------------------------------------------------------

/// Ordre des attributs d'un `<Item>` (classe de base d'abord).
const ATTR_ORDER: [&str; 9] =
    ["xsi:type", "Id", "Name", "Favorite", "QuickOpen", "Collapsed", "Temporary", "CombineMode", "MatcherMode"];

/// Ordre des sous-éléments d'un `<Item>` (classe de base d'abord).
const CHILD_ORDER: [&str; 19] = [
    "BookCount",
    "NewBookCount",
    "NewBookCountDate",
    "UnreadBookCount",
    "Description",
    "CacheStorage",
    "Display",
    "Items",
    "BookIds",
    "Matchers",
    "Limit",
    "LimitType",
    "LimitValue",
    "LimitSelectionType",
    "LimitRandomSeed",
    "BaseListId",
    "NotInBaseList",
    "FilteredIds",
    "ShowFiltered",
];

fn rank(order: &[&str], name: &str) -> Option<usize> {
    order.iter().position(|n| *n == name)
}

#[derive(Debug, Clone)]
struct Child {
    /// Texte avant l'élément (indentation, commentaires).
    lead: String,
    /// Nom local de l'élément.
    name: String,
    /// Élément complet, tel quel.
    raw: String,
}

/// Un `<Item>` découpé en balise ouvrante, sous-éléments directs et fin. Rendu sans
/// modification, il redonne exactement le texte d'origine.
#[derive(Debug, Clone)]
pub(crate) struct ItemXml {
    /// Balise ouvrante (`<Item …>` ou `<Item … />`), réécrite seulement si un attribut change.
    start: String,
    /// Attributs (nom qualifié, valeur **déjà échappée**) : renseignés à la lecture.
    attrs: Vec<(String, String)>,
    tag: String,
    empty: bool,
    start_dirty: bool,
    children: Vec<Child>,
    /// Texte après le dernier sous-élément, balise fermante comprise.
    tail: String,
    /// Indentation de l'élément lui-même (sans retour à la ligne).
    indent: String,
    /// Fin de ligne des éléments ajoutés : celle du texte d'origine.
    nl: &'static str,
}

impl ItemXml {
    /// Découpe le XML d'une ligne. `depth` (0 = premier niveau) sert à indenter les
    /// éléments ajoutés quand l'indentation d'origine n'est pas connue.
    pub(crate) fn parse(xml: &str, depth: usize) -> Result<Self, Error> {
        let mut r = Reader::from_str(xml);
        let default_indent = "  ".repeat(depth + 2);
        let nl = newline_of(xml);
        loop {
            let before = r.buffer_position() as usize;
            match r.read_event().map_err(xml_err)? {
                Event::Start(e) | Event::Empty(e) if before < xml.len() => {
                    let empty = xml[..r.buffer_position() as usize].ends_with("/>");
                    let after = r.buffer_position() as usize;
                    let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let attrs = read_attrs(&e)?;
                    let start = xml[before..after].to_string();
                    let mut item = ItemXml {
                        start,
                        attrs,
                        tag,
                        empty,
                        start_dirty: false,
                        children: Vec::new(),
                        tail: String::new(),
                        indent: default_indent,
                        nl,
                    };
                    if !empty {
                        item.read_children(&mut r, xml, after)?;
                    }
                    return Ok(item);
                }
                Event::Eof => return Err(xml_err("liste sans élément <Item>")),
                _ => {}
            }
        }
    }

    fn read_children(&mut self, r: &mut Reader<&[u8]>, xml: &str, mut last: usize) -> Result<(), Error> {
        loop {
            let before = r.buffer_position() as usize;
            match r.read_event().map_err(xml_err)? {
                Event::Start(c) => {
                    let c = c.into_owned();
                    r.read_to_end(c.name()).map_err(xml_err)?;
                    let after = r.buffer_position() as usize;
                    self.children.push(Child {
                        lead: xml[last..before].to_string(),
                        name: String::from_utf8_lossy(local(c.name().as_ref())).into_owned(),
                        raw: xml[before..after].to_string(),
                    });
                    last = after;
                }
                Event::Empty(c) => {
                    let after = r.buffer_position() as usize;
                    self.children.push(Child {
                        lead: xml[last..before].to_string(),
                        name: String::from_utf8_lossy(local(c.name().as_ref())).into_owned(),
                        raw: xml[before..after].to_string(),
                    });
                    last = after;
                }
                Event::End(_) => {
                    self.tail = xml[last..].to_string();
                    // Indentation de l'élément : celle de sa balise fermante.
                    let ws = &xml[last..before];
                    if let Some(i) = ws.rfind('\n') {
                        if ws[i + 1..].chars().all(|c| c == ' ' || c == '\t') {
                            self.indent = ws[i + 1..].to_string();
                        }
                    }
                    return Ok(());
                }
                Event::Eof => return Err(xml_err("fin de liste inattendue")),
                _ => {}
            }
        }
    }

    /// Nouvel élément vide `<Item xsi:type=… Id=… Name=…>`.
    pub(crate) fn new(kind: &str, id: &str, name: &str, depth: usize, nl: &'static str) -> Self {
        let mut item = ItemXml {
            start: String::new(),
            attrs: vec![
                ("xsi:type".into(), escape_attr(kind)),
                ("Id".into(), escape_attr(id)),
                ("Name".into(), escape_attr(name)),
            ],
            tag: "Item".into(),
            empty: false,
            start_dirty: true,
            children: Vec::new(),
            tail: String::new(),
            indent: "  ".repeat(depth + 2),
            nl,
        };
        item.tail = format!("{nl}{}</Item>", item.indent);
        item
    }

    pub(crate) fn render(&self) -> String {
        let mut out = if self.start_dirty {
            let mut s = format!("<{}", self.tag);
            for (k, v) in &self.attrs {
                s.push_str(&format!(" {k}=\"{v}\""));
            }
            s.push_str(if self.empty { " />" } else { ">" });
            s
        } else {
            self.start.clone()
        };
        if self.empty {
            return out;
        }
        for c in &self.children {
            out.push_str(&c.lead);
            out.push_str(&c.raw);
        }
        out.push_str(&self.tail);
        out
    }

    pub(crate) fn newline(&self) -> &'static str {
        self.nl
    }

    /// Indentation d'un sous-élément.
    pub(crate) fn child_indent(&self) -> String {
        format!("{}  ", self.indent)
    }

    pub(crate) fn attr(&self, name: &str) -> Option<String> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| unescape(v))
    }

    /// Fixe (`Some`) ou retire (`None`) un attribut ; un attribut ajouté prend sa place
    /// dans l'ordre de `XmlSerializer`.
    pub(crate) fn set_attr(&mut self, name: &str, value: Option<&str>) {
        let pos = self.attrs.iter().position(|(k, _)| k == name);
        match (pos, value) {
            (Some(i), Some(v)) => {
                let v = escape_attr(v);
                if self.attrs[i].1 != v {
                    self.attrs[i].1 = v;
                    self.start_dirty = true;
                }
            }
            (Some(i), None) => {
                self.attrs.remove(i);
                self.start_dirty = true;
            }
            (None, Some(v)) => {
                let at = match rank(&ATTR_ORDER, name) {
                    Some(r) => self
                        .attrs
                        .iter()
                        .position(|(k, _)| rank(&ATTR_ORDER, k).is_some_and(|x| x > r))
                        .unwrap_or(self.attrs.len()),
                    None => self.attrs.len(),
                };
                self.attrs.insert(at, (name.to_string(), escape_attr(v)));
                self.start_dirty = true;
            }
            (None, None) => {}
        }
    }

    pub(crate) fn has_child(&self, name: &str) -> bool {
        self.children.iter().any(|c| c.name == name)
    }

    /// Élément brut d'un sous-élément.
    pub(crate) fn child_raw(&self, name: &str) -> Option<&str> {
        self.children.iter().find(|c| c.name == name).map(|c| c.raw.as_str())
    }

    /// Texte d'un sous-élément simple.
    pub(crate) fn child_text(&self, name: &str) -> Option<String> {
        let raw = self.child_raw(name)?;
        let mut r = Reader::from_str(raw);
        let mut out = String::new();
        loop {
            match r.read_event() {
                Ok(Event::Text(t)) => out.push_str(&t.unescape().ok()?),
                Ok(Event::CData(t)) => out.push_str(&String::from_utf8_lossy(&t)),
                Ok(Event::Eof) | Err(_) => return Some(out),
                _ => {}
            }
        }
    }

    /// Textes des sous-éléments d'un sous-élément (`<BookIds><guid>…`).
    pub(crate) fn child_texts(&self, name: &str) -> Vec<String> {
        let Some(raw) = self.child_raw(name) else { return Vec::new() };
        let mut r = Reader::from_str(raw);
        let mut out = Vec::new();
        let mut depth = 0;
        let mut cur = String::new();
        loop {
            match r.read_event() {
                Ok(Event::Start(_)) => {
                    depth += 1;
                    cur.clear();
                }
                Ok(Event::Text(t)) if depth == 2 => cur.push_str(&t.unescape().unwrap_or_default()),
                Ok(Event::End(_)) => {
                    if depth == 2 {
                        let t = cur.trim();
                        if !t.is_empty() {
                            out.push(t.to_ascii_lowercase());
                        }
                    }
                    depth -= 1;
                }
                Ok(Event::Eof) | Err(_) => return out,
                _ => {}
            }
        }
    }

    /// Remplace (ou ajoute à sa place) un sous-élément par `raw` ; `None` le retire.
    pub(crate) fn set_child(&mut self, name: &str, raw: Option<String>) {
        let pos = self.children.iter().position(|c| c.name == name);
        match (pos, raw) {
            (Some(i), Some(raw)) => self.children[i].raw = raw,
            (Some(i), None) => {
                self.children.remove(i);
                // Plusieurs occurrences (ne devrait pas arriver) : toutes retirées.
                self.children.retain(|c| c.name != name);
            }
            (None, Some(raw)) => {
                if self.empty {
                    self.empty = false;
                    self.start_dirty = true;
                    self.tail = format!("{}{}</{}>", self.nl, self.indent, self.tag);
                }
                let lead = self
                    .children
                    .first()
                    .map(|c| c.lead.clone())
                    .filter(|l| l.contains('\n'))
                    .unwrap_or_else(|| format!("{}{}", self.nl, self.child_indent()));
                let at = match rank(&CHILD_ORDER, name) {
                    Some(r) => self
                        .children
                        .iter()
                        .position(|c| rank(&CHILD_ORDER, &c.name).is_some_and(|x| x > r))
                        .unwrap_or(self.children.len()),
                    None => self.children.len(),
                };
                self.children.insert(at, Child { lead, name: name.to_string(), raw });
            }
            (None, None) => {}
        }
    }

    /// Sous-élément simple : texte (`None` ou valeur par défaut : retiré).
    pub(crate) fn set_text_child(&mut self, name: &str, text: Option<&str>) {
        self.set_child(name, text.map(|t| text_element(name, t)));
    }

    /// Change le type (`xsi:type`) et retire les membres propres à l'ancien type.
    pub(crate) fn retype(&mut self, kind: &str, drop_attrs: &[&str], drop_children: &[&str]) {
        self.set_attr("xsi:type", Some(kind));
        for a in drop_attrs {
            self.set_attr(a, None);
        }
        for c in drop_children {
            self.set_child(c, None);
        }
    }

    /// Écrit tous les critères d'une liste intelligente. Sans limite, les réglages de
    /// limite existants (`LimitValue`…) sont gardés tels quels (ComicRack les garde aussi
    /// quand la limite est désactivée) ; seul `<Limit>` est retiré.
    pub(crate) fn set_query(&mut self, q: &SmartQuery) {
        self.set_attr("MatcherMode", (q.mode == Mode::Or).then_some("Or"));
        let (ind, nl) = (self.child_indent(), self.nl);
        self.set_child("Matchers", Some(matchers_xml(&q.matchers, &ind, nl)));
        match &q.limit {
            Some(l) => {
                self.set_text_child("Limit", Some("true"));
                self.set_text_child("LimitType", (l.kind != LimitKind::Count).then(|| limit_kind_name(l.kind)));
                self.set_text_child("LimitValue", (l.value != 25).then(|| l.value.to_string()).as_deref());
                self.set_text_child(
                    "LimitSelectionType",
                    (l.selection != Selection::Random).then(|| selection_name(l.selection)),
                );
                self.set_text_child("LimitRandomSeed", (l.seed != 0).then(|| l.seed.to_string()).as_deref());
            }
            None => self.set_text_child("Limit", None),
        }
        let base = q.base_list.as_deref().map(|b| b.trim().to_ascii_lowercase()).filter(|b| !b.is_empty());
        self.set_text_child("BaseListId", base.as_deref());
        self.set_text_child("NotInBaseList", q.not_in_base.then_some("true"));
        self.set_child(
            "FilteredIds",
            (!q.filtered.is_empty()).then(|| guid_list("FilteredIds", &q.filtered, &ind, nl)),
        );
    }
}

/// Fin de ligne d'un texte : `\r\n` s'il en contient (fichier écrit sous Windows).
pub(crate) fn newline_of(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn read_attrs(e: &BytesStart) -> Result<Vec<(String, String)>, Error> {
    let mut out = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(xml_err)?;
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        // Valeur ré-échappée au format de `XmlTextWriter` (guillemets doubles).
        let v = a.unescape_value().map_err(xml_err)?;
        out.push((key, escape_attr(&v)));
    }
    Ok(out)
}

fn unescape(v: &str) -> String {
    quick_xml::escape::unescape(v).map(|c| c.into_owned()).unwrap_or_else(|_| v.to_string())
}
