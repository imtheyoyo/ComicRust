//! Éditeur de listes intelligentes, sur le modèle de la fenêtre « Éditer Liste
//! intelligente » de ComicRack (`SmartListDialog`, `MatcherEditor`, `MatcherGroupEditor`) :
//!
//! - nom et description ;
//! - « Correspond à : tous / au moins un des critères suivants, dans : [liste de départ] »
//!   et « pas dans cette liste » ;
//! - une ligne par critère : case « Non » (le « ! » de ComicRack), champ, opérateur,
//!   valeur(s), menu ☰ (nouveau critère, nouveau groupe, monter, descendre, supprimer) et ✖ ;
//! - groupes imbriqués, avec leur propre « tous / au moins un » et leur case « Non » ;
//! - limite (nombre de livres, Mo ou Go ; sélection au hasard, par position ou par série) ;
//! - aperçu en direct du nombre de livres trouvés (calculé dans un fil).
//!
//! Le modèle ([`Draft`], [`Node`], [`Rule`]) est séparé de la fenêtre et testé : conversion
//! des critères ↔ [`Matcher`], contrôle des valeurs (nombres, dates `AAAA-MM-JJ`, nombre de
//! jours, note de 0 à 5), opérations sur l'arbre des critères.

use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use comicrust_core::comic_lists::{ComicList, ListKind, Members};
use comicrust_core::db::ComicDatabase;
use comicrust_core::matcher_catalog::{matcher_catalog, matcher_info, MatcherInfo, ValueKind};
use comicrust_core::smart::{describe_kind, Limit, LimitKind, Matcher, MatcherXml, Mode, Selection, SmartQuery};
use eframe::egui::{self, RichText, Ui};

use crate::library::lists_ops::{base_candidates, BaseChoice};

/// Délai entre la dernière modification et le calcul de l'aperçu.
const PREVIEW_DELAY: Duration = Duration::from_millis(250);

/// Type de critère d'un nouveau critère (« Série contient … »).
const DEFAULT_KIND: &str = "ComicBookSeriesMatcher";
const DEFAULT_OP: i32 = 1;

// ---------------------------------------------------------------------------
// Modèle
// ---------------------------------------------------------------------------

/// Un critère tel qu'il est saisi.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub not: bool,
    pub kind: String,
    pub op: i32,
    pub v1: String,
    pub v2: String,
    pub ignore_case: bool,
    /// Critère absent du catalogue : affiché sans pouvoir être modifié, gardé tel quel.
    pub raw: Option<Matcher>,
    /// Critère d'origine (liste existante) : tant que la saisie n'est pas modifiée, il est
    /// rendu tel quel (valeurs et XML d'origine, voir `comicrust_core::smart::MatcherXml`) ;
    /// modifié, il garde ses attributs et sous-éléments inconnus de ComicRust.
    pub orig: Option<Box<Matcher>>,
}

impl Rule {
    pub fn new() -> Self {
        Rule {
            not: false,
            kind: DEFAULT_KIND.into(),
            op: DEFAULT_OP,
            v1: String::new(),
            v2: String::new(),
            ignore_case: true,
            raw: None,
            orig: None,
        }
    }

    /// Même saisie (case « Non », champ, opérateur, valeurs, casse) ?
    fn same_input(&self, other: &Rule) -> bool {
        (self.not, &self.kind, self.op, &self.v1, &self.v2, self.ignore_case)
            == (other.not, &other.kind, other.op, &other.v1, &other.v2, other.ignore_case)
    }

    pub fn info(&self) -> Option<&'static MatcherInfo> {
        matcher_info(&self.kind)
    }

    /// Change le champ : opérateur et valeurs gardés si la nature de la valeur est la même.
    pub fn set_kind(&mut self, kind: &str) {
        let old = self.info().map(|i| i.value);
        self.kind = kind.to_string();
        let Some(info) = self.info() else { return };
        if old != Some(info.value) {
            self.op = 0;
            self.v1.clear();
            self.v2.clear();
        } else if info.operator(self.op).is_none() {
            self.op = 0;
        }
    }
}

/// Un critère ou un groupe de critères.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Rule(Rule),
    /// Groupe ; `xml` : XML d'origine (attributs inconnus, `Collapsed`… gardés).
    Group {
        not: bool,
        mode: Mode,
        items: Vec<Node>,
        xml: MatcherXml,
    },
}

impl Node {
    pub fn new_group() -> Self {
        Node::Group { not: false, mode: Mode::And, items: vec![Node::Rule(Rule::new())], xml: MatcherXml::default() }
    }
}

/// Liste intelligente en cours de modification.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub name: String,
    pub description: String,
    pub mode: Mode,
    pub items: Vec<Node>,
    pub base: Option<String>,
    pub not_in_base: bool,
    pub limit_on: bool,
    pub limit_value: String,
    pub limit_kind: LimitKind,
    pub selection: Selection,
    pub seed: i64,
    /// Livres retirés à la main dans ComicRack (gardés, ou effacés par « Réinitialiser »).
    pub filtered: Vec<String>,
}

impl Draft {
    /// Nouvelle liste : un critère vide.
    pub fn new(name: &str) -> Self {
        Draft {
            name: name.to_string(),
            description: String::new(),
            mode: Mode::And,
            items: vec![Node::Rule(Rule::new())],
            base: None,
            not_in_base: false,
            limit_on: false,
            limit_value: "25".into(),
            limit_kind: LimitKind::Count,
            selection: Selection::Random,
            seed: 0,
            filtered: Vec::new(),
        }
    }

    /// Liste existante.
    pub fn from_list(list: &ComicList) -> Self {
        let q = match &list.kind {
            ListKind::Smart { query: Some(q), .. } => q.clone(),
            _ => SmartQuery::default(),
        };
        let mut d = Draft::from_query(&q);
        d.name = list.name.clone();
        d.description = list.description.clone();
        d
    }

    pub fn from_query(q: &SmartQuery) -> Self {
        let limit = q.limit.clone();
        Draft {
            name: String::new(),
            description: String::new(),
            mode: q.mode,
            items: q.matchers.iter().map(node_from_matcher).collect(),
            base: q.base_list.clone().filter(|b| !b.trim().is_empty()).map(|b| b.trim().to_ascii_lowercase()),
            not_in_base: q.not_in_base,
            limit_on: limit.is_some(),
            limit_value: limit.as_ref().map(|l| l.value).unwrap_or(25).to_string(),
            limit_kind: limit.as_ref().map(|l| l.kind).unwrap_or(LimitKind::Count),
            selection: limit.as_ref().map(|l| l.selection).unwrap_or(Selection::Random),
            seed: limit.as_ref().map(|l| l.seed).unwrap_or(0),
            filtered: q.filtered.clone(),
        }
    }

    /// Critères prêts à enregistrer, ou la liste des erreurs de saisie.
    pub fn to_query(&self) -> Result<SmartQuery, Vec<String>> {
        let mut errors = Vec::new();
        let mut counter = 0;
        let matchers = nodes_to_matchers(&self.items, &mut counter, &mut errors);
        let limit = if self.limit_on {
            match self.limit_value.trim().parse::<i64>() {
                Ok(v) if v >= 0 => {
                    Some(Limit { kind: self.limit_kind, value: v, selection: self.selection, seed: self.seed })
                }
                _ => {
                    errors.push(format!("Limite : « {} » n'est pas un nombre entier positif", self.limit_value.trim()));
                    None
                }
            }
        } else {
            None
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(SmartQuery {
            mode: self.mode,
            matchers,
            base_list: self.base.clone(),
            not_in_base: self.not_in_base && self.base.is_some(),
            limit,
            filtered: self.filtered.clone(),
        })
    }
}

fn node_from_matcher(m: &Matcher) -> Node {
    match m {
        Matcher::Group { mode, not, matchers, xml } => Node::Group {
            not: *not,
            mode: *mode,
            items: matchers.iter().map(node_from_matcher).collect(),
            xml: xml.clone(),
        },
        Matcher::Value { kind, op, value, value2, not, ignore_case, .. } => {
            let Some(info) = matcher_info(kind).filter(|i| i.operator(*op).is_some()) else {
                return Node::Rule(Rule {
                    not: *not,
                    kind: kind.clone(),
                    op: *op,
                    v1: value.clone(),
                    v2: value2.clone(),
                    ignore_case: *ignore_case,
                    raw: Some(m.clone()),
                    orig: None,
                });
            };
            let days = info.operator(*op).is_some_and(|o| o.unit.is_some());
            let show = |v: &str| if info.value == ValueKind::Date && !days { display_date(v) } else { v.to_string() };
            Node::Rule(Rule {
                not: *not,
                kind: kind.clone(),
                op: *op,
                v1: show(value),
                v2: show(value2),
                ignore_case: *ignore_case,
                raw: None,
                orig: Some(Box::new(m.clone())),
            })
        }
    }
}

fn nodes_to_matchers(items: &[Node], counter: &mut usize, errors: &mut Vec<String>) -> Vec<Matcher> {
    let mut out = Vec::new();
    for n in items {
        match n {
            Node::Rule(r) => {
                *counter += 1;
                match rule_to_matcher(r) {
                    Ok(m) => out.push(m),
                    Err(e) => {
                        let label = r.info().map(|i| i.label.to_string()).unwrap_or_else(|| describe_kind(&r.kind));
                        errors.push(format!("Critère {} ({label}) : {e}", *counter));
                    }
                }
            }
            Node::Group { not, mode, items, xml } => out.push(Matcher::Group {
                mode: *mode,
                not: *not,
                matchers: nodes_to_matchers(items, counter, errors),
                xml: xml.clone(),
            }),
        }
    }
    out
}

/// Notes (0 à 5).
fn is_rating(kind: &str) -> bool {
    matches!(kind, "ComicBookRatingMatcher" | "ComicBookCommunityRatingMatcher")
        || kind.starts_with("SmartListSeriesAverage")
}

/// Contrôle et conversion d'un critère saisi.
pub fn rule_to_matcher(r: &Rule) -> Result<Matcher, String> {
    if let Some(raw) = &r.raw {
        return Ok(raw.clone());
    }
    // Critère existant non modifié : rendu tel quel (valeur « 4,5 » ou date « 01/10/2019 »
    // de ComicRack comprises), jamais réécrit.
    if let Some(orig) = &r.orig {
        if matches!(node_from_matcher(orig), Node::Rule(o) if o.same_input(r)) {
            return Ok((**orig).clone());
        }
    }
    let info = r.info().ok_or_else(|| format!("critère inconnu ({})", r.kind))?;
    let op = info.operator(r.op).ok_or_else(|| format!("opérateur {} inconnu", r.op))?;
    let check = |v: &str| -> Result<String, String> {
        match info.value {
            ValueKind::Number => {
                let n = parse_number(v)?;
                if is_rating(&r.kind) && !(0.0..=5.0).contains(&n.replace(',', ".").parse::<f64>().unwrap_or(-1.0)) {
                    return Err(format!("la note doit être entre 0 et 5 (« {} »)", v.trim()));
                }
                Ok(n)
            }
            ValueKind::Date if op.unit.is_some() => parse_days(v),
            ValueKind::Date => parse_date(v),
            ValueKind::Text if r.op == 7 => {
                regex_ok(v)?;
                Ok(v.to_string())
            }
            _ => Ok(v.to_string()),
        }
    };
    let (value, value2) = match (info.value, op.values) {
        // Valeur personnalisée : nom puis valeur cherchée.
        (ValueKind::CustomValue, _) => {
            if r.v1.trim().is_empty() {
                return Err("nom de la valeur personnalisée manquant".into());
            }
            if r.op == 7 {
                regex_ok(&r.v2)?;
            }
            (r.v1.trim().to_string(), r.v2.clone())
        }
        (_, 0) => (String::new(), String::new()),
        (_, 1) => (check(&r.v1)?, String::new()),
        _ => (check(&r.v1)?, check(&r.v2)?),
    };
    let m = Matcher::value(&r.kind, r.op, &value, &value2, r.not, r.ignore_case);
    // Critère modifié : ses attributs et sous-éléments inconnus (`Name`…) sont gardés.
    Ok(match &r.orig {
        Some(o) => m.with_xml(o.xml().clone()),
        None => m,
    })
}

fn regex_ok(v: &str) -> Result<(), String> {
    regex_lite_check(v).map_err(|e| format!("expression régulière invalide : {e}"))
}

/// Vérification sommaire d'une expression régulière (parenthèses et crochets équilibrés) :
/// le moteur exact est celui de .NET dans ComicRack.
fn regex_lite_check(v: &str) -> Result<(), String> {
    let (mut paren, mut bracket, mut escaped) = (0i32, false, false);
    for c in v.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '[' if !bracket => bracket = true,
            ']' if bracket => bracket = false,
            '(' if !bracket => paren += 1,
            ')' if !bracket => {
                paren -= 1;
                if paren < 0 {
                    return Err("« ) » sans « ( »".into());
                }
            }
            _ => {}
        }
    }
    if paren > 0 {
        Err("« ( » non fermée".into())
    } else if bracket {
        Err("« [ » non fermé".into())
    } else if escaped {
        Err("« \\ » en fin d'expression".into())
    } else {
        Ok(())
    }
}

/// Nombre saisi (virgule ou point acceptés) → texte écrit dans la liste : entier tel quel,
/// décimal avec le séparateur décimal du poste
/// ([`comicrust_core::locale::decimal_separator`]) : ComicRack relit la valeur avec la
/// culture de Windows (`float.TryParse`), « 4,5 » sur un poste français. ComicRust, lui,
/// lit les deux.
pub fn parse_number(v: &str) -> Result<String, String> {
    parse_number_with(v, comicrust_core::locale::decimal_separator())
}

/// [`parse_number`] avec le séparateur décimal donné.
pub fn parse_number_with(v: &str, separator: char) -> Result<String, String> {
    let t = v.trim().replace(',', ".");
    if t.is_empty() {
        return Err("nombre manquant".into());
    }
    if let Ok(i) = t.parse::<i64>() {
        return Ok(i.to_string());
    }
    match t.parse::<f64>() {
        Ok(f) if f.is_finite() && t.matches('.').count() == 1 => Ok(t.replace('.', &separator.to_string())),
        _ => Err(format!("« {} » n'est pas un nombre", v.trim())),
    }
}

/// Date `AAAA-MM-JJ` (lue par ComicRack quelle que soit la langue du poste).
pub fn parse_date(v: &str) -> Result<String, String> {
    let t = v.trim();
    NaiveDate::parse_from_str(t, "%Y-%m-%d")
        .map(|d| d.format("%Y-%m-%d").to_string())
        .map_err(|_| format!("date attendue au format AAAA-MM-JJ (« {t} »)"))
}

/// Nombre de jours (« dans les derniers … jours »).
pub fn parse_days(v: &str) -> Result<String, String> {
    match v.trim().parse::<u32>() {
        Ok(n) => Ok(n.to_string()),
        Err(_) => Err(format!("nombre de jours attendu (« {} »)", v.trim())),
    }
}

/// Date enregistrée par ComicRack (culture du poste : `01/10/2019`, `2019-10-01T…`)
/// montrée au format `AAAA-MM-JJ` ; laissée telle quelle si elle n'est pas reconnue.
pub fn display_date(v: &str) -> String {
    let t = v.trim();
    let first = t.split([' ', 'T']).next().unwrap_or(t);
    for fmt in ["%Y-%m-%d", "%d/%m/%Y", "%d.%m.%Y", "%d-%m-%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(first, fmt) {
            return d.format("%Y-%m-%d").to_string();
        }
    }
    t.to_string()
}

/// Modification de l'arbre des critères (chemin : rangs depuis la racine).
#[derive(Debug, Clone, PartialEq)]
pub enum NodeAction {
    Delete(Vec<usize>),
    Up(Vec<usize>),
    Down(Vec<usize>),
    /// Ajoute un critère à la fin du groupe (chemin du groupe ; vide : racine).
    AddRule(Vec<usize>),
    AddGroup(Vec<usize>),
}

fn group_mut<'a>(items: &'a mut Vec<Node>, path: &[usize]) -> Option<&'a mut Vec<Node>> {
    let Some((first, rest)) = path.split_first() else { return Some(items) };
    match items.get_mut(*first)? {
        Node::Group { items, .. } => group_mut(items, rest),
        Node::Rule(_) => None,
    }
}

pub fn apply_action(items: &mut Vec<Node>, action: &NodeAction) {
    match action {
        NodeAction::AddRule(g) => {
            if let Some(v) = group_mut(items, g) {
                v.push(Node::Rule(Rule::new()));
            }
        }
        NodeAction::AddGroup(g) => {
            if let Some(v) = group_mut(items, g) {
                v.push(Node::new_group());
            }
        }
        NodeAction::Delete(p) | NodeAction::Up(p) | NodeAction::Down(p) => {
            let Some((&i, parent)) = p.split_last() else { return };
            let Some(v) = group_mut(items, parent) else { return };
            if i >= v.len() {
                return;
            }
            match action {
                NodeAction::Delete(_) => {
                    v.remove(i);
                }
                NodeAction::Up(_) if i > 0 => v.swap(i, i - 1),
                NodeAction::Down(_) if i + 1 < v.len() => v.swap(i, i + 1),
                _ => {}
            }
        }
    }
}

/// Catégorie d'un critère dans la liste des champs.
fn category(i: &MatcherInfo) -> &'static str {
    if i.series_stat {
        return "Statistiques de série";
    }
    match i.value {
        ValueKind::Text | ValueKind::CustomValue => "Texte",
        ValueKind::Number => "Nombres",
        ValueKind::Date => "Dates",
        ValueKind::YesNo | ValueKind::Manga | ValueKind::OnOff => "Oui / Non",
        ValueKind::Expression => "Modules de ComicRack",
    }
}

const CATEGORIES: [&str; 6] =
    ["Texte", "Nombres", "Dates", "Oui / Non", "Statistiques de série", "Modules de ComicRack"];

/// Clé de tri d'un libellé (casse et accents ignorés).
fn sort_key(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Champs par catégorie, triés par libellé.
pub fn grouped_catalog() -> Vec<(&'static str, Vec<&'static MatcherInfo>)> {
    CATEGORIES
        .iter()
        .map(|cat| {
            let mut v: Vec<&MatcherInfo> = matcher_catalog().iter().filter(|i| category(i) == *cat).collect();
            v.sort_by_key(|i| sort_key(i.label));
            (*cat, v)
        })
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

/// Copie des listes où la liste éditée (ou une nouvelle liste) a les critères `q` ;
/// renvoie aussi son identifiant (pour l'aperçu).
pub fn lists_with(items: &[ComicList], id: Option<&str>, q: &SmartQuery) -> (Vec<ComicList>, String) {
    fn replace(l: &mut [ComicList], id: &str, q: &SmartQuery) -> bool {
        for x in l {
            if x.id.eq_ignore_ascii_case(id) {
                x.kind = ListKind::Smart { query: Some(q.clone()), cached: None };
                return true;
            }
            if let ListKind::Folder { children, .. } = &mut x.kind {
                if replace(children, id, q) {
                    return true;
                }
            }
        }
        false
    }
    let mut out = items.to_vec();
    if let Some(id) = id {
        if replace(&mut out, id, q) {
            return (out, id.to_string());
        }
    }
    let tmp = "comicrust-apercu".to_string();
    out.push(ComicList {
        id: tmp.clone(),
        name: String::new(),
        description: String::new(),
        kind: ListKind::Smart { query: Some(q.clone()), cached: None },
        comicrack_count: 0,
    });
    (out, tmp)
}

// ---------------------------------------------------------------------------
// Aperçu
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum PreviewResult {
    Count(usize),
    Unknown(String),
}

#[derive(Default)]
struct Preview {
    last: Option<SmartQuery>,
    changed: Option<Instant>,
    rx: Option<Receiver<PreviewResult>>,
    result: Option<PreviewResult>,
}

impl Preview {
    fn poll(
        &mut self,
        q: Option<&SmartQuery>,
        items: &[ComicList],
        id: Option<&str>,
        db: Option<Arc<ComicDatabase>>,
        ctx: &egui::Context,
    ) {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.result = Some(r);
                    self.rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.rx = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let Some(q) = q else { return };
        if self.last.as_ref() != Some(q) {
            self.last = Some(q.clone());
            self.changed = Some(Instant::now());
        }
        let Some(t) = self.changed else { return };
        if self.rx.is_some() {
            return;
        }
        if t.elapsed() < PREVIEW_DELAY {
            ctx.request_repaint_after(PREVIEW_DELAY);
            return;
        }
        let Some(db) = db else { return };
        self.changed = None;
        let (lists, id) = lists_with(items, id, q);
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("comicrust-apercu".into()).spawn(move || {
            let r = match ComicList::find(&lists, &id).map(|l| l.resolve(&db, &lists)) {
                Some(Members::All) => PreviewResult::Count(db.len()),
                Some(Members::Ids(ids)) => {
                    PreviewResult::Count(ids.iter().filter(|i| db.index_of(i).is_some()).count())
                }
                Some(Members::Unknown(why)) => PreviewResult::Unknown(why),
                None => PreviewResult::Unknown("liste introuvable".into()),
            };
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            self.rx = Some(rx);
        }
    }

    fn computing(&self) -> bool {
        self.rx.is_some() || self.changed.is_some()
    }
}

// ---------------------------------------------------------------------------
// Fenêtre
// ---------------------------------------------------------------------------

/// Ce que la fenêtre demande à l'application.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorRequest {
    None,
    /// Enregistrer (et fermer ensuite si `close`).
    Save {
        close: bool,
    },
    Close,
}

/// Données de l'application utiles à la fenêtre.
pub struct EditorEnv<'a> {
    pub items: &'a [ComicList],
    pub db: Option<Arc<ComicDatabase>>,
    /// `Err` : listes en lecture seule, avec la raison.
    pub editable: Result<(), String>,
    pub busy: bool,
}

pub struct SmartEditor {
    /// Liste modifiée (`None` : nouvelle liste, pas encore créée).
    pub id: Option<String>,
    /// Emplacement d'une nouvelle liste.
    pub parent: Option<String>,
    pub position: Option<usize>,
    pub draft: Draft,
    /// État enregistré (pour n'écrire que ce qui a changé).
    pub saved: Draft,
    /// Erreur du dernier enregistrement.
    pub error: Option<String>,
    /// Enregistrement en cours (`Some(fermer ensuite)`).
    pub saving: Option<bool>,
    serial: u64,
    preview: Preview,
}

impl SmartEditor {
    pub fn edit(list: &ComicList, serial: u64) -> Self {
        let draft = Draft::from_list(list);
        SmartEditor {
            id: Some(list.id.clone()),
            parent: None,
            position: None,
            saved: draft.clone(),
            draft,
            error: None,
            saving: None,
            serial,
            preview: Preview::default(),
        }
    }

    pub fn create(parent: Option<String>, position: Option<usize>, serial: u64) -> Self {
        let draft = Draft::new("Nouvelle liste intelligente");
        SmartEditor {
            id: None,
            parent,
            position,
            saved: draft.clone(),
            draft,
            error: None,
            saving: None,
            serial,
            preview: Preview::default(),
        }
    }

    pub fn dirty(&self) -> bool {
        self.id.is_none() || self.draft != self.saved
    }

    /// Dessine la fenêtre ; renvoie la demande de l'utilisateur.
    pub fn show(&mut self, ctx: &egui::Context, env: &EditorEnv) -> EditorRequest {
        let query = self.draft.to_query();
        self.preview.poll(query.as_ref().ok(), env.items, self.id.as_deref(), env.db.clone(), ctx);
        let title = if self.id.is_some() {
            format!("Liste intelligente — {}", self.saved.name)
        } else {
            "Nouvelle liste intelligente".to_string()
        };
        let mut open = true;
        let mut req = EditorRequest::None;
        egui::Window::new(title)
            .id(egui::Id::unique(("editeur-liste-intelligente", self.serial)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([860.0, 560.0])
            .min_width(640.0)
            .show(ctx, |ui| req = self.contents(ui, env, &query));
        if !open {
            return EditorRequest::Close;
        }
        req
    }

    fn contents(&mut self, ui: &mut Ui, env: &EditorEnv, query: &Result<SmartQuery, Vec<String>>) -> EditorRequest {
        let serial = self.serial;
        let d = &mut self.draft;
        egui::Grid::new(("liste-entete", serial)).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Nom :");
            ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
            ui.end_row();
            ui.label("Description :");
            ui.add(egui::TextEdit::multiline(&mut d.description).desired_rows(2).desired_width(f32::INFINITY));
            ui.end_row();
        });
        ui.add_space(4.0);
        // « Correspond à [tous] des critères suivants, dans : [liste de départ] ».
        let bases = base_candidates(env.items, self.id.as_deref());
        ui.horizontal_wrapped(|ui| {
            ui.label("Correspond à");
            mode_combo(ui, ("mode-racine", serial), &mut d.mode);
            ui.label("des critères suivants, dans :");
            base_combo(ui, serial, &bases, &mut d.base, env.items);
            ui.add_enabled(d.base.is_some(), egui::Checkbox::new(&mut d.not_in_base, "pas dans cette liste"))
                .on_hover_text("Livres de toute la bibliothèque qui ne sont pas dans la liste de départ");
        });
        if d.base.as_ref().is_some_and(|b| !bases.iter().any(|c| c.id == *b)) {
            ui.label(
                RichText::new("⚠ Liste de départ introuvable (supprimée ?) : la liste part de toute la bibliothèque.")
                    .color(crate::ui::theme::warning(ui)),
            );
        }
        ui.separator();

        // Critères.
        let mut actions = Vec::new();
        let footer = 150.0;
        egui::ScrollArea::vertical()
            .id_salt(("criteres", serial))
            .max_height((ui.available_height() - footer).max(120.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let mut path = Vec::new();
                nodes_ui(ui, serial, &mut d.items, &mut path, &mut actions);
                if d.items.is_empty() {
                    ui.label(RichText::new("Aucun critère : tous les livres de la liste de départ.").weak());
                }
                ui.horizontal(|ui| {
                    if ui.button("＋ Critère").on_hover_text("Nouvelle règle").clicked() {
                        actions.push(NodeAction::AddRule(Vec::new()));
                    }
                    if ui.button("＋ Groupe").on_hover_text("Nouveau groupe de critères").clicked() {
                        actions.push(NodeAction::AddGroup(Vec::new()));
                    }
                });
            });
        for a in &actions {
            apply_action(&mut d.items, a);
        }
        ui.separator();

        // Limite.
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut d.limit_on, "Limiter à");
            ui.add_enabled(d.limit_on, egui::TextEdit::singleline(&mut d.limit_value).desired_width(60.0));
            ui.add_enabled_ui(d.limit_on, |ui| {
                egui::ComboBox::from_id_salt(("limite-type", serial))
                    .selected_text(limit_kind_label(d.limit_kind))
                    .show_ui(ui, |ui| {
                        for k in [LimitKind::Count, LimitKind::Mb, LimitKind::Gb] {
                            ui.selectable_value(&mut d.limit_kind, k, limit_kind_label(k));
                        }
                    });
                ui.label("sélection :");
                let before = d.selection;
                egui::ComboBox::from_id_salt(("limite-selection", serial))
                    .selected_text(selection_label(d.selection))
                    .show_ui(ui, |ui| {
                        for s in [Selection::Random, Selection::Position, Selection::SortedBySeries] {
                            ui.selectable_value(&mut d.selection, s, selection_label(s));
                        }
                    });
                if d.selection == Selection::Random && before != Selection::Random && d.seed == 0 {
                    d.seed = chrono::Local::now().timestamp_subsec_nanos() as i64 % 1_000_000 + 1;
                }
            });
        });
        if !d.filtered.is_empty() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} livre(s) retiré(s) à la main de cette liste dans ComicRack.",
                        d.filtered.len()
                    ))
                    .weak(),
                );
                if ui.small_button("Réinitialiser").on_hover_text("Les remettre dans la liste").clicked() {
                    d.filtered.clear();
                }
            });
        }

        // Aperçu et avertissements.
        ui.horizontal_wrapped(|ui| match query {
            Err(_) => {
                ui.label(RichText::new("Aperçu : corrige les critères signalés.").weak());
            }
            Ok(q) => {
                if env.db.is_none() {
                    ui.label(RichText::new("Aperçu : bibliothèque non chargée.").weak());
                } else {
                    match &self.preview.result {
                        Some(PreviewResult::Count(n)) => {
                            ui.label(
                                RichText::new(format!("Aperçu : {n} livre{} trouvé{}", plural(*n), plural(*n)))
                                    .strong(),
                            );
                        }
                        Some(PreviewResult::Unknown(why)) => {
                            ui.label(
                                RichText::new(format!("Aperçu impossible : {why}"))
                                    .color(crate::ui::theme::warning(ui)),
                            );
                        }
                        None => {}
                    }
                    if self.preview.computing() {
                        ui.spinner();
                    }
                }
                let missing = q.unsupported();
                if !missing.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "⚠ ComicRust ne sait pas encore évaluer : {} — la liste est enregistrée telle quelle \
                             (ComicRack la calcule), mais ComicRust ne pourra pas l'afficher.",
                            missing.join(", ")
                        ))
                        .color(crate::ui::theme::warning(ui)),
                    );
                }
                if q.is_approximate() {
                    ui.label(
                        RichText::new(
                            "Sélection « au hasard » : le tirage de ComicRust peut différer de celui de ComicRack.",
                        )
                        .weak(),
                    );
                }
            }
        });
        if let Err(errors) = query {
            for e in errors {
                ui.label(RichText::new(format!("✖ {e}")).color(crate::ui::theme::error(ui)));
            }
        }
        if d.name.trim().is_empty() {
            ui.label(RichText::new("✖ Le nom ne peut pas être vide.").color(crate::ui::theme::error(ui)));
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(format!("Enregistrement refusé : {e}")).color(crate::ui::theme::error(ui)));
        }

        // Boutons.
        let valid = query.is_ok() && !d.name.trim().is_empty();
        let can_save = valid && env.editable.is_ok() && !env.busy && self.saving.is_none();
        let reason = match (&env.editable, env.busy || self.saving.is_some(), valid) {
            (Err(r), _, _) => r.clone(),
            (_, true, _) => "Enregistrement en cours…".to_string(),
            _ => "Corrige d'abord les erreurs signalées".to_string(),
        };
        let mut req = EditorRequest::None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if self.saving.is_some() {
                ui.spinner();
                ui.label("Enregistrement…");
            } else if let Err(r) = &env.editable {
                ui.label(RichText::new(format!("🔒 {r}")).color(crate::ui::theme::warning(ui)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(can_save, egui::Button::new("Appliquer")).on_disabled_hover_text(&reason).clicked() {
                    req = EditorRequest::Save { close: false };
                }
                if ui.button("Annuler").clicked() {
                    req = EditorRequest::Close;
                }
                let ok = ui.add_enabled(can_save || (valid && !self.dirty()), egui::Button::new("   OK   "));
                if ok.on_disabled_hover_text(&reason).clicked() {
                    req = if self.dirty() { EditorRequest::Save { close: true } } else { EditorRequest::Close };
                }
            });
        });
        req
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

fn limit_kind_label(k: LimitKind) -> &'static str {
    match k {
        LimitKind::Count => "livres",
        LimitKind::Mb => "Mo",
        LimitKind::Gb => "Go",
    }
}

fn selection_label(s: Selection) -> &'static str {
    match s {
        Selection::Random => "au hasard",
        Selection::Position => "dans l'ordre (position)",
        Selection::SortedBySeries => "triés par série",
    }
}

fn mode_combo(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug, mode: &mut Mode) {
    let label = |m: Mode| if m == Mode::And { "tous" } else { "au moins un" };
    egui::ComboBox::from_id_salt(salt).selected_text(label(*mode)).width(110.0).show_ui(ui, |ui| {
        ui.selectable_value(mode, Mode::And, label(Mode::And));
        ui.selectable_value(mode, Mode::Or, label(Mode::Or));
    });
}

fn base_combo(ui: &mut Ui, serial: u64, bases: &[BaseChoice], base: &mut Option<String>, items: &[ComicList]) {
    let current = match base {
        None => "📚 Toute la bibliothèque".to_string(),
        Some(b) => match ComicList::find(items, b) {
            Some(l) => format!("{} {}", crate::library::lists_ops::icon(l), l.name),
            None => "(liste introuvable)".to_string(),
        },
    };
    egui::ComboBox::from_id_salt(("liste-depart", serial)).selected_text(current).width(240.0).height(420.0).show_ui(
        ui,
        |ui| {
            ui.selectable_value(base, None, "📚 Toute la bibliothèque");
            for c in bases {
                let text = format!("{}{} {}", "    ".repeat(c.depth), c.icon, c.name);
                ui.selectable_value(base, Some(c.id.clone()), text);
            }
        },
    );
}

/// Menu ☰ d'un critère ou d'un groupe (repris du menu d'édition de ComicRack).
fn node_menu(ui: &mut Ui, path: &[usize], index: usize, count: usize, actions: &mut Vec<NodeAction>) {
    let parent = path[..path.len() - 1].to_vec();
    ui.menu_button("☰", |ui| {
        if ui.button("Nouvelle règle").clicked() {
            actions.push(NodeAction::AddRule(parent.clone()));
        }
        if ui.button("Nouveau groupe").clicked() {
            actions.push(NodeAction::AddGroup(parent.clone()));
        }
        ui.separator();
        if ui.add_enabled(index > 0, egui::Button::new("Monter")).clicked() {
            actions.push(NodeAction::Up(path.to_vec()));
        }
        if ui.add_enabled(index + 1 < count, egui::Button::new("Descendre")).clicked() {
            actions.push(NodeAction::Down(path.to_vec()));
        }
        ui.separator();
        if ui.button("Supprimer").clicked() {
            actions.push(NodeAction::Delete(path.to_vec()));
        }
    })
    .response
    .on_hover_text("Nouvelle règle, nouveau groupe, monter, descendre, supprimer");
}

fn nodes_ui(ui: &mut Ui, serial: u64, items: &mut [Node], path: &mut Vec<usize>, actions: &mut Vec<NodeAction>) {
    let count = items.len();
    for (i, node) in items.iter_mut().enumerate() {
        path.push(i);
        match node {
            Node::Rule(r) => rule_ui(ui, serial, r, path, i, count, actions),
            Node::Group { not, mode, items, .. } => {
                egui::Frame::group(ui.style()).inner_margin(6.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(not, "Non").on_hover_text("Inverse le groupe (« ! » dans ComicRack)");
                        ui.label("Groupe : correspond à");
                        mode_combo(ui, ("mode-groupe", serial, path.clone()), mode);
                        ui.label("des critères suivants :");
                        node_menu(ui, path, i, count, actions);
                        if ui.small_button("✖").on_hover_text("Supprimer le groupe").clicked() {
                            actions.push(NodeAction::Delete(path.clone()));
                        }
                    });
                    ui.indent(("groupe", serial, path.clone()), |ui| {
                        nodes_ui(ui, serial, items, path, actions);
                        ui.horizontal(|ui| {
                            if ui.small_button("＋ Critère").clicked() {
                                actions.push(NodeAction::AddRule(path.clone()));
                            }
                            if ui.small_button("＋ Groupe").clicked() {
                                actions.push(NodeAction::AddGroup(path.clone()));
                            }
                        });
                    });
                });
            }
        }
        path.pop();
    }
}

fn rule_ui(
    ui: &mut Ui,
    serial: u64,
    r: &mut Rule,
    path: &[usize],
    index: usize,
    count: usize,
    actions: &mut Vec<NodeAction>,
) {
    let salt = |what: &str| egui::Id::unique(("critere", serial, what.to_string(), path.to_vec()));
    let check = rule_to_matcher(r);
    ui.horizontal(|ui| {
        if let Some(Matcher::Value { kind, op, value, value2, not, .. }) = &r.raw {
            ui.add_enabled(false, egui::Checkbox::new(&mut not.clone(), "Non"));
            ui.label(
                RichText::new(format!(
                    "Critère inconnu de ComicRust : {} (opérateur {op}) « {value} »{}",
                    describe_kind(kind),
                    if value2.is_empty() { String::new() } else { format!(" « {value2} »") }
                ))
                .italics(),
            )
            .on_hover_text("Conservé tel quel à l'enregistrement ; modifiable seulement dans ComicRack.");
        } else {
            ui.checkbox(&mut r.not, "Non").on_hover_text("Inverse le critère (« ! » dans ComicRack)");
            field_combo(ui, r);
            let Some(info) = r.info() else { return };
            egui::ComboBox::from_id_salt(salt("operateur"))
                .selected_text(info.operator(r.op).map(|o| o.label).unwrap_or("?"))
                .width(170.0)
                .show_ui(ui, |ui| {
                    for o in info.operators() {
                        ui.selectable_value(&mut r.op, o.op, o.label);
                    }
                });
            let op = info.operator(r.op);
            let n = op.map(|o| o.values).unwrap_or(0);
            let unit = op.and_then(|o| o.unit).or(info.unit);
            let days = op.is_some_and(|o| o.unit.is_some());
            let (hint, width) = match info.value {
                ValueKind::Date if days => ("nombre", 60.0),
                ValueKind::Date => ("AAAA-MM-JJ", 100.0),
                ValueKind::Number if is_rating(&r.kind) => ("0 à 5", 60.0),
                ValueKind::Number => ("nombre", 80.0),
                ValueKind::Expression => ("expression", 260.0),
                _ => ("", 200.0),
            };
            if info.value == ValueKind::CustomValue {
                ui.add(egui::TextEdit::singleline(&mut r.v1).hint_text("nom de la valeur").desired_width(120.0));
                ui.add(egui::TextEdit::singleline(&mut r.v2).hint_text("valeur").desired_width(160.0));
            } else if n >= 1 {
                ui.add(egui::TextEdit::singleline(&mut r.v1).hint_text(hint).desired_width(width));
                if n >= 2 {
                    ui.label("et");
                    ui.add(egui::TextEdit::singleline(&mut r.v2).hint_text(hint).desired_width(width));
                }
            }
            if let Some(u) = unit {
                ui.label(u);
            }
            if let Err(e) = &check {
                ui.label(RichText::new("⚠").color(crate::ui::theme::error(ui))).on_hover_text(e);
            }
            if !info.evaluated() {
                ui.label(RichText::new("non évalué").small().color(crate::ui::theme::warning(ui)))
                    .on_hover_text("ComicRust ne sait pas encore évaluer ce critère (ComicRack, oui)");
            }
        }
        node_menu(ui, path, index, count, actions);
        if ui.small_button("✖").on_hover_text("Supprimer le critère").clicked() {
            actions.push(NodeAction::Delete(path.to_vec()));
        }
    });
}

/// Choix du champ : menu par catégorie (Texte ▸, Nombres ▸…), libellés triés, critères
/// non évalués par ComicRust marqués « ⚠ » (ComicRack présente lui aussi un menu).
fn field_combo(ui: &mut Ui, r: &mut Rule) {
    let current = r.info().map(|i| i.label).unwrap_or("?");
    let mut chosen: Option<&'static str> = None;
    let button = egui::Button::new(format!("{current}  ⏷")).min_size(egui::vec2(200.0, 0.0)).truncate();
    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        for (cat, infos) in grouped_catalog() {
            ui.menu_button(cat, |ui| {
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    for i in infos {
                        let text = if i.evaluated() { i.label.to_string() } else { format!("{}  ⚠", i.label) };
                        let resp = ui.selectable_label(r.kind == i.kind, text);
                        let resp = if i.evaluated() {
                            resp
                        } else {
                            resp.on_hover_text("ComicRust ne sait pas encore évaluer ce critère (ComicRack, oui)")
                        };
                        if resp.clicked() {
                            chosen = Some(i.kind);
                            ui.close();
                        }
                    }
                });
            });
        }
    });
    if let Some(k) = chosen {
        r.set_kind(k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(kind: &str, op: i32, v1: &str, v2: &str) -> Matcher {
        Matcher::value(kind, op, v1, v2, false, true)
    }

    #[test]
    fn values_are_checked_and_converted() {
        assert_eq!(parse_number(" 12 "), Ok("12".into()));
        assert_eq!(parse_number_with("1,5", '.'), Ok("1.5".into()));
        assert_eq!(parse_number_with("1.5", ','), Ok("1,5".into()), "séparateur du poste");
        assert_eq!(parse_number_with("4,5", ','), Ok("4,5".into()));
        assert!(parse_number_with("1.000,5", ',').is_err());
        assert!(parse_number("douze").is_err());
        assert!(parse_number("").is_err());
        assert_eq!(parse_date("2024-02-29"), Ok("2024-02-29".into()));
        assert!(parse_date("2023-02-29").is_err());
        assert!(parse_date("29/02/2024").is_err(), "format AAAA-MM-JJ seulement à la saisie");
        assert_eq!(display_date("01/10/2019"), "2019-10-01", "date française de ComicRack");
        assert_eq!(display_date("2019-10-01T00:00:00"), "2019-10-01");
        assert_eq!(display_date("n'importe"), "n'importe");
        assert_eq!(parse_days("30"), Ok("30".into()));
        assert!(parse_days("-3").is_err());
        assert!(regex_lite_check(r"#\d{3}").is_ok());
        assert!(regex_lite_check("(abc").is_err());
        assert!(regex_lite_check("[(]").is_ok());
    }

    #[test]
    fn rules_to_matchers() {
        let mut r = Rule::new();
        r.v1 = "Thanos".into();
        assert_eq!(rule_to_matcher(&r), Ok(value("ComicBookSeriesMatcher", 1, "Thanos", "")));
        r.set_kind("ComicBookYearMatcher");
        assert_eq!((r.op, r.v1.as_str()), (0, ""), "nature différente : opérateur et valeur remis à zéro");
        r.op = 3;
        r.v1 = "1990".into();
        r.v2 = "1995".into();
        assert_eq!(rule_to_matcher(&r), Ok(value("ComicBookYearMatcher", 3, "1990", "1995")));
        r.v2 = "x".into();
        assert!(rule_to_matcher(&r).is_err());
        r.set_kind("ComicBookVolumeMatcher");
        assert_eq!(r.op, 3, "même nature (nombre) : opérateur gardé");
        r.set_kind("ComicBookRatingMatcher");
        r.op = 1;
        r.v1 = "6".into();
        assert!(rule_to_matcher(&r).unwrap_err().contains("entre 0 et 5"));
        r.set_kind("ComicBookAddedMatcher");
        r.op = 3;
        r.v1 = "30".into();
        assert_eq!(rule_to_matcher(&r), Ok(value("ComicBookAddedMatcher", 3, "30", "")), "nombre de jours");
        r.op = 1;
        assert!(rule_to_matcher(&r).is_err(), "« 30 » n'est pas une date");
        r.v1 = "2020-01-31".into();
        assert!(rule_to_matcher(&r).is_ok());
        r.set_kind("ComicBookMangaMatcher");
        r.op = 1;
        r.v1 = "reste".into();
        assert_eq!(rule_to_matcher(&r), Ok(value("ComicBookMangaMatcher", 1, "", "")), "aucune valeur");
        r.set_kind("ComicBookCustomValuesMatcher");
        assert!(rule_to_matcher(&r).is_err(), "nom manquant");
        r.v1 = "Lu par".into();
        r.v2 = "Yoyo".into();
        assert_eq!(rule_to_matcher(&r), Ok(value("ComicBookCustomValuesMatcher", 0, "Lu par", "Yoyo")));
    }

    #[test]
    fn draft_round_trip_keeps_unknown_criteria() {
        let unknown = value("ComicBookVirtualTagMatcher", 2, "x", "");
        let q = SmartQuery {
            mode: Mode::Or,
            matchers: vec![
                value("ComicBookWriterMatcher", 1, "Starlin", ""),
                Matcher::group(
                    Mode::And,
                    true,
                    vec![value("ComicBookAddedMatcher", 2, "01/01/2020", ""), unknown.clone()],
                ),
            ],
            base_list: Some("ABC".into()),
            not_in_base: true,
            limit: Some(Limit { kind: LimitKind::Mb, value: 500, selection: Selection::SortedBySeries, seed: 0 }),
            filtered: vec!["b1".into()],
        };
        let d = Draft::from_query(&q);
        let Node::Group { items, .. } = &d.items[1] else { panic!() };
        let Node::Rule(date) = &items[0] else { panic!() };
        assert_eq!(date.v1, "2020-01-01", "date affichée au format AAAA-MM-JJ");
        let Node::Rule(raw) = &items[1] else { panic!() };
        assert_eq!(raw.raw.as_ref(), Some(&unknown));
        let back = d.to_query().unwrap();
        let mut expected = q.clone();
        expected.base_list = Some("abc".into());
        assert_eq!(back, expected, "critères non modifiés : rendus tels quels (date de ComicRack comprise)");
        // Date modifiée : écrite au format AAAA-MM-JJ.
        let mut d3 = d.clone();
        if let Node::Group { items, .. } = &mut d3.items[1] {
            if let Node::Rule(r) = &mut items[0] {
                r.v1 = "2021-02-03".into();
            }
        }
        if let Matcher::Group { matchers, .. } = &mut expected.matchers[1] {
            matchers[0] = value("ComicBookAddedMatcher", 2, "2021-02-03", "");
        }
        assert_eq!(d3.to_query().unwrap(), expected);
        // Sans liste de départ, « pas dans la liste » n'a pas de sens.
        let mut d2 = d.clone();
        d2.base = None;
        assert!(!d2.to_query().unwrap().not_in_base);
        d2.limit_value = "beaucoup".into();
        assert_eq!(d2.to_query().unwrap_err().len(), 1);
    }

    /// `ComicDb.xml` avec des membres de critères que ComicRust ne modélise pas.
    const RICH: &str = "<?xml version=\"1.0\"?>
<ComicDatabase xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">
  <ComicLists>
    <Item xsi:type=\"ComicSmartListItem\" Id=\"36e9430d-e7bd-4660-90bb-611380d042b5\" Name=\"Riche\">
      <Matchers>
        <ComicBookMatcher xsi:type=\"ComicBookSeriesMatcher\" Name=\"Ma série\" MatchOperator=\"1\">
          <MatchValue>Thanos</MatchValue>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type=\"ComicBookPluginMatcher\" Name=\"Module\" PluginKey=\"cle-123\" MatchOperator=\"2\">
          <MatchValue>a</MatchValue>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type=\"ComicBookAllPropertiesMatcher\" MatchOperator=\"1\">
          <MatchValue>Starlin</MatchValue>
          <Option>Writer</Option>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type=\"ComicBookGroupMatcher\" Collapsed=\"true\">
          <Matchers>
            <ComicBookMatcher xsi:type=\"ComicBookRatingMatcher\" MatchOperator=\"1\">
              <MatchValue>4,5</MatchValue>
            </ComicBookMatcher>
            <ComicBookMatcher xsi:type=\"ComicBookAddedMatcher\" MatchOperator=\"2\">
              <MatchValue>01/10/2019</MatchValue>
            </ComicBookMatcher>
          </Matchers>
        </ComicBookMatcher>
      </Matchers>
    </Item>
  </ComicLists>
</ComicDatabase>";
    const RICH_ID: &str = "36e9430d-e7bd-4660-90bb-611380d042b5";

    fn rich() -> (comicrust_core::lists_edit::ListsEditor, Draft) {
        let mut w = Vec::new();
        let e = comicrust_core::lists_edit::ListsEditor::from_comicdb_xml(RICH, &mut w).unwrap();
        let lists = e.lists().unwrap();
        let d = Draft::from_list(ComicList::find(&lists, RICH_ID).unwrap());
        (e, d)
    }

    #[test]
    fn untouched_editor_keeps_xml_identical() {
        let (mut e, d) = rich();
        let before = e.row(RICH_ID).unwrap().xml.clone();
        e.set_query(RICH_ID, &d.to_query().unwrap()).unwrap();
        assert_eq!(e.row(RICH_ID).unwrap().xml, before, "aller-retour par l'éditeur sans modification");
    }

    #[test]
    fn edited_neighbour_keeps_unknown_members_and_values() {
        let (mut e, mut d) = rich();
        let before = e.row(RICH_ID).unwrap().xml.clone();
        // Seul le premier critère change.
        let Node::Rule(r) = &mut d.items[0] else { panic!() };
        r.v1 = "Warlock".into();
        e.set_query(RICH_ID, &d.to_query().unwrap()).unwrap();
        let x = e.row(RICH_ID).unwrap().xml.clone();
        assert!(
            x.contains("<ComicBookMatcher xsi:type=\"ComicBookSeriesMatcher\" Name=\"Ma série\" MatchOperator=\"1\">"),
            "critère modifié : Name gardé\n{x}"
        );
        assert!(x.contains("<MatchValue>Warlock</MatchValue>"));
        // Les autres critères : recopiés octet pour octet (PluginKey, Option, Collapsed, « 4,5 »,
        // date de ComicRack).
        let tail = |t: &str| t[t.find("<ComicBookMatcher xsi:type=\"ComicBookPluginMatcher\"").unwrap()..].to_string();
        assert_eq!(tail(&x), tail(&before));
        // Valeur décimale modifiée : séparateur du poste, jamais la valeur d'origine réécrite.
        let Node::Group { items, .. } = &mut d.items[3] else { panic!() };
        let Node::Rule(rating) = &mut items[0] else { panic!() };
        assert_eq!(rating.v1, "4,5", "affichée telle qu'écrite par ComicRack");
        rating.v1 = "3.5".into();
        let q = d.to_query().unwrap();
        let Matcher::Group { matchers, .. } = &q.matchers[3] else { panic!() };
        let Matcher::Value { value, .. } = &matchers[0] else { panic!() };
        assert_eq!(*value, parse_number("3.5").unwrap());
        let Matcher::Value { value: date, .. } = &matchers[1] else { panic!() };
        assert_eq!(date, "01/10/2019", "critère voisin non modifié : date d'origine gardée");
        e.set_query(RICH_ID, &q).unwrap();
        let x = e.row(RICH_ID).unwrap().xml.clone();
        assert!(x.contains("<ComicBookMatcher xsi:type=\"ComicBookGroupMatcher\" Collapsed=\"true\">"), "{x}");
        assert!(x.contains("<Option>Writer</Option>") && x.contains("PluginKey=\"cle-123\""));
    }

    #[test]
    fn editing_the_tree() {
        let mut d = Draft::new("x");
        apply_action(&mut d.items, &NodeAction::AddGroup(vec![]));
        apply_action(&mut d.items, &NodeAction::AddRule(vec![1]));
        let Node::Group { items, .. } = &d.items[1] else { panic!() };
        assert_eq!(items.len(), 2);
        apply_action(&mut d.items, &NodeAction::Up(vec![1]));
        assert!(matches!(d.items[0], Node::Group { .. }));
        apply_action(&mut d.items, &NodeAction::Delete(vec![0, 1]));
        let Node::Group { items, .. } = &d.items[0] else { panic!() };
        assert_eq!(items.len(), 1);
        apply_action(&mut d.items, &NodeAction::Down(vec![0]));
        assert!(matches!(d.items[1], Node::Group { .. }));
        apply_action(&mut d.items, &NodeAction::AddRule(vec![0]));
        assert_eq!(d.items.len(), 2, "un critère n'est pas un groupe : rien");
        let mut counter = 0;
        let mut errors = Vec::new();
        let m = nodes_to_matchers(&d.items, &mut counter, &mut errors);
        assert!(matches!(&m[1], Matcher::Group { matchers, .. } if matchers.len() == 1));
    }

    #[test]
    fn catalog_grouped_and_sorted() {
        let g = grouped_catalog();
        assert_eq!(g[0].0, "Texte");
        let labels: Vec<&str> = g[0].1.iter().map(|i| i.label).collect();
        let mut sorted = labels.clone();
        sorted.sort_by_key(|l| sort_key(l));
        assert_eq!(labels, sorted);
        let total: usize = g.iter().map(|(_, v)| v.len()).sum();
        assert_eq!(total, matcher_catalog().len(), "chaque critère dans une catégorie");
        // « Éditeur » trié avec les « E ».
        let e = labels.iter().position(|l| *l == "Éditeur").unwrap();
        assert!(sort_key(labels[e - 1]).as_str() < "f");
    }

    #[test]
    fn preview_lists_replace_or_add() {
        let lists = vec![ComicList {
            id: "S".into(),
            name: "s".into(),
            description: String::new(),
            kind: ListKind::Smart { query: None, cached: Some(vec!["x".into()]) },
            comicrack_count: 0,
        }];
        let q = SmartQuery::default();
        let (l, id) = lists_with(&lists, Some("s"), &q);
        assert_eq!((l.len(), id.as_str()), (1, "s"));
        assert_eq!(l[0].kind, ListKind::Smart { query: Some(q.clone()), cached: None });
        let (l, id) = lists_with(&lists, None, &q);
        assert_eq!(l.len(), 2);
        assert_eq!(id, "comicrust-apercu");
    }
}
