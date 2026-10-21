//! Listes intelligentes de ComicRack : lecture de leurs critères (`<Matchers>` d'un
//! `ComicSmartListItem` dans `ComicDb.xml`) et calcul de leur contenu, en reprenant le
//! comportement des classes `ComicBook…Matcher` de ComicRack CE.
//!
//! Chaque critère porte un type (`xsi:type`, par ex. `ComicBookWriterMatcher`), un
//! opérateur numérique (`MatchOperator`), une ou deux valeurs (`MatchValue`,
//! `MatchValue2`) et peut être inversé (`Not`). Les critères se combinent en « tous »
//! (`MatcherMode="And"`) ou « au moins un » (`Or`), et peuvent être regroupés
//! (`ComicBookGroupMatcher`). La liste peut partir d'une autre liste (`BaseListId`,
//! `NotInBaseList`) et être limitée (`Limit`, `LimitType`, `LimitValue`, `LimitSelectionType`).
//!
//! Les critères que ComicRust ne sait pas (encore) évaluer — statistiques de série,
//! valeurs personnalisées, doublons… — sont signalés : la liste n'est alors pas calculée
//! plutôt que calculée faussement.

use std::collections::{HashMap, HashSet};

use chrono::{Local, NaiveDate};

use crate::library::LibraryBook;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    And,
    Or,
}

impl Mode {
    pub fn parse(s: Option<&str>) -> Self {
        if s.is_some_and(|s| s.eq_ignore_ascii_case("Or")) {
            Mode::Or
        } else {
            Mode::And
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    Count,
    Mb,
    Gb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Position,
    SortedBySeries,
    Random,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    pub kind: LimitKind,
    pub value: i64,
    pub selection: Selection,
    pub seed: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Matcher {
    Value { kind: String, op: i32, value: String, value2: String, not: bool, ignore_case: bool },
    Group { mode: Mode, not: bool, matchers: Vec<Matcher> },
}

/// Critères d'une liste intelligente.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SmartQuery {
    pub mode: Mode,
    pub matchers: Vec<Matcher>,
    /// Liste de départ (identifiant en minuscules).
    pub base_list: Option<String>,
    pub not_in_base: bool,
    pub limit: Option<Limit>,
    /// Livres retirés à la main de la liste dans ComicRack.
    pub filtered: Vec<String>,
}

// ---------------------------------------------------------------------------
// Valeurs d'un livre, comme les propriétés lues par les critères de ComicRack.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YesNo {
    Unknown,
    No,
    Yes,
}

fn yes_no(v: &Option<String>) -> YesNo {
    match v.as_deref() {
        Some("Yes") => YesNo::Yes,
        Some("No") => YesNo::No,
        _ => YesNo::Unknown,
    }
}

fn bool_yes_no(b: bool) -> YesNo {
    if b {
        YesNo::Yes
    } else {
        YesNo::No
    }
}

enum Getter {
    Str(fn(&LibraryBook) -> String),
    Num(fn(&LibraryBook) -> f32),
    /// `None` : date absente (`DateTime.MinValue` dans ComicRack).
    Date(fn(&LibraryBook) -> Option<NaiveDate>),
    YesNo(fn(&LibraryBook) -> YesNo),
    Manga,
    AllProperties,
    /// « Série : toutes complètes » (statistique sur toute la série).
    SeriesAllComplete,
    /// « Doublons » (même série, numéro, volume… ou même fichier).
    Duplicate,
}

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

/// Nombre au format invariant (« 12 », « 1.5 ») ; -1 si illisible, comme `TryParse`.
fn parse_num(v: &str) -> f32 {
    v.trim().parse().unwrap_or(-1.0)
}

fn file_stem(b: &LibraryBook) -> String {
    let f = b.file.as_deref().unwrap_or("");
    let name = f.rsplit(['/', '\\']).next().unwrap_or(f);
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_string(),
        _ => name.to_string(),
    }
}

/// Série et numéro « proposés » d'après le nom du fichier quand la fiche est vide
/// (`ShadowSeries`, `ShadowNumber` de ComicRack).
fn proposed(b: &LibraryBook) -> Option<(String, String)> {
    crate::index::parse_file_name(&file_stem(b))
}

fn shadow_series(b: &LibraryBook) -> String {
    match b.series.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(x) => x.to_string(),
        None => proposed(b).map(|p| p.0).unwrap_or_default(),
    }
}

fn shadow_number(b: &LibraryBook) -> String {
    match b.number.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(x) => x.to_string(),
        None => proposed(b).map(|p| p.1).unwrap_or_default(),
    }
}

/// Date ComicRack sérialisée (`2019-10-01T12:58:00.123+02:00`).
fn iso_date(v: &Option<String>) -> Option<NaiveDate> {
    let t = v.as_deref()?;
    if t.len() < 10 || t.starts_with("0001-01-01") {
        return None;
    }
    NaiveDate::parse_from_str(&t[..10], "%Y-%m-%d").ok()
}

fn published(b: &LibraryBook) -> Option<NaiveDate> {
    let y = b.year.filter(|y| *y > 0)?.clamp(1, 9999);
    let m = b.month.unwrap_or(1).clamp(1, 12) as u32;
    let last = (28..=31).rev().find(|d| NaiveDate::from_ymd_opt(y, m, *d).is_some()).unwrap_or(28);
    let d = b.day.unwrap_or(1).clamp(1, last as i32) as u32;
    NaiveDate::from_ymd_opt(y, m, d)
}

fn getter(kind: &str) -> Option<Getter> {
    use Getter::*;
    let name = kind.strip_prefix("ComicBook").and_then(|k| k.strip_suffix("Matcher")).unwrap_or(kind);
    Some(match name {
        // Texte
        "AgeRating" => Str(|b| s(&b.age_rating)),
        "AlternateSeries" => Str(|b| s(&b.alternate_series)),
        "BookAge" => Str(|b| s(&b.book_age)),
        "BookCollectionStatus" => Str(|b| s(&b.book_collection_status)),
        "BookCondition" => Str(|b| s(&b.book_condition)),
        "BookLocation" => Str(|b| s(&b.book_location)),
        "BookNotes" => Str(|b| s(&b.book_notes)),
        "BookOwner" => Str(|b| s(&b.book_owner)),
        "BookStore" => Str(|b| s(&b.book_store)),
        "Characters" => Str(|b| s(&b.characters)),
        "Colorist" => Str(|b| s(&b.colorist)),
        "CoverArtist" => Str(|b| s(&b.cover_artist)),
        "Directory" => Str(|b| {
            let f = b.file.as_deref().unwrap_or("");
            f.rfind(['/', '\\']).map(|i| f[..i].to_string()).unwrap_or_default()
        }),
        "Editor" => Str(|b| s(&b.editor)),
        "File" => Str(file_stem),
        "FullPath" => Str(|b| s(&b.file)),
        "Format" => Str(|b| s(&b.format)),
        "Genre" => Str(|b| s(&b.genre)),
        "ISBN" => Str(|b| s(&b.isbn)),
        "Imprint" => Str(|b| s(&b.imprint)),
        "Inker" => Str(|b| s(&b.inker)),
        "Letterer" => Str(|b| s(&b.letterer)),
        "Locations" => Str(|b| s(&b.locations)),
        "MainCharacterOrTeam" => Str(|b| s(&b.main_character_or_team)),
        "Notes" => Str(|b| s(&b.notes)),
        "Penciller" => Str(|b| s(&b.penciller)),
        "Publisher" => Str(|b| s(&b.publisher)),
        "Review" => Str(|b| s(&b.review)),
        "ScanInformation" => Str(|b| s(&b.scan_information)),
        "SeriesGroup" => Str(|b| s(&b.series_group)),
        "Series" => Str(shadow_series),
        "StoryArc" => Str(|b| s(&b.story_arc)),
        "Summary" => Str(|b| s(&b.summary)),
        "Tags" => Str(|b| s(&b.tags)),
        "Teams" => Str(|b| s(&b.teams)),
        "Title" => Str(|b| s(&b.title)),
        "Translator" => Str(|b| s(&b.translator)),
        "Web" => Str(|b| s(&b.web)),
        "Writer" => Str(|b| s(&b.writer)),
        // Nombres (valeurs absentes : -1, comme les valeurs par défaut de ComicRack)
        "AlternateCount" => Num(|b| b.alternate_count.unwrap_or(-1) as f32),
        "AlternateNumber" => Num(|b| parse_num(b.alternate_number.as_deref().unwrap_or(""))),
        "BookPrice" => Num(|b| b.book_price.unwrap_or(-1.0)),
        "CommunityRating" => Num(|b| b.community_rating.unwrap_or(0.0)),
        "Count" => Num(|b| b.count.unwrap_or(-1) as f32),
        "Day" => Num(|b| b.day.unwrap_or(-1) as f32),
        "FileSize" => Num(|b| b.file_size.unwrap_or(0) as f32 / 1024.0 / 1024.0),
        "Month" => Num(|b| b.month.unwrap_or(-1) as f32),
        "Number" => Num(|b| parse_num(&shadow_number(b))),
        "PageCount" => Num(|b| b.page_count.unwrap_or(0) as f32),
        "Rating" => Num(|b| b.rating.unwrap_or(0.0)),
        "ReadPercentage" => Num(|b| b.read_percent().unwrap_or(0) as f32),
        "Volume" => Num(|b| b.volume.unwrap_or(-1) as f32),
        "Year" => Num(|b| b.year.unwrap_or(-1) as f32),
        // Dates
        "Added" => Date(|b| iso_date(&b.added)),
        "Opened" => Date(|b| iso_date(&b.opened)),
        "Released" => Date(|b| iso_date(&b.released)),
        "Published" => Date(published),
        // Oui / non
        "BlackAndWhite" => YesNo(|b| yes_no(&b.black_and_white)),
        "SeriesComplete" => YesNo(|b| yes_no(&b.series_complete)),
        "Checked" => YesNo(|b| bool_yes_no(b.checked.unwrap_or(true))),
        "IsLinked" => YesNo(|b| bool_yes_no(b.file.as_deref().is_some_and(|f| !f.is_empty()))),
        "IsMissing" => {
            YesNo(|b| bool_yes_no(b.file.as_deref().is_some_and(|f| !f.is_empty()) && b.missing.unwrap_or(false)))
        }
        "ModifiedInfo" => YesNo(|b| bool_yes_no(b.comic_info_is_dirty.unwrap_or(false))),
        "ModifiedLibraryInfo" => YesNo(|b| bool_yes_no(b.comic_book_is_dirty.unwrap_or(false))),
        "Manga" => Manga,
        "AllProperties" => AllProperties,
        "Duplicate" => Duplicate,
        "SmartListSeriesAllCompleteMatcher" => SeriesAllComplete,
        _ => return None,
    })
}

/// Libellé français d'un type de critère (pour les messages).
pub fn describe_kind(kind: &str) -> String {
    let name = kind
        .strip_prefix("ComicBook")
        .or_else(|| kind.strip_prefix("SmartListSeries").map(|_| kind))
        .unwrap_or(kind)
        .trim_end_matches("Matcher");
    match name {
        "CustomValues" | "HasCustomValues" => "valeurs personnalisées".into(),
        "Duplicate" => "doublons".into(),
        "VirtualTag" => "étiquettes virtuelles".into(),
        "Week" => "semaine".into(),
        "NewPages" => "nouvelles pages".into(),
        "BookmarkCount" => "nombre de signets".into(),
        "Creation" | "Modified" => "date du fichier".into(),
        "ModifiedInfo" | "ModifiedLibraryInfo" => "informations modifiées".into(),
        "FileFormat" => "format de fichier".into(),
        "Language" => "langue".into(),
        _ if kind.starts_with("SmartListSeries") => {
            format!("statistique de série ({})", kind.trim_start_matches("SmartListSeries").trim_end_matches("Matcher"))
        }
        _ => name.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Évaluation
// ---------------------------------------------------------------------------

/// Découpage de « contient l'un de / tous » : sur `,` ou `;` s'il y en a, sinon sur les
/// espaces (même découpage que ComicRack).
fn split_values(v: &str) -> Vec<String> {
    let parts: Vec<&str> =
        if v.contains([',', ';']) { v.split([',', ';']).collect() } else { v.split_whitespace().collect() };
    parts.into_iter().map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect()
}

fn num_match(op: i32, n: f32, v1: f32, v2: f32) -> bool {
    match op {
        0 => n == v1,
        1 => n > v1,
        2 => n < v1,
        3 => n >= v1 && n <= v2,
        _ => false,
    }
}

/// Date saisie dans ComicRack (culture française : jour d'abord), ou nombre de jours.
fn parse_match_date(v: &str, today: NaiveDate) -> Option<NaiveDate> {
    let t = v.trim();
    if t.is_empty() {
        return None;
    }
    let first = t.split([' ', 'T']).next().unwrap_or(t);
    for fmt in ["%d/%m/%Y", "%Y-%m-%d", "%d.%m.%Y", "%d-%m-%Y", "%m/%d/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(first, fmt) {
            return Some(d);
        }
    }
    let days: i64 = t.parse().unwrap_or(0);
    today.checked_sub_signed(chrono::Duration::days(days))
}

/// Mots ignorés et séparateurs de `GroupInfo.CompressedName` (ComicRack).
const ARTICLES: [&str; 8] = ["the", "der", "die", "das", "le", "la", "les", "l'"];
const SEPARATORS: [char; 15] = [' ', '\t', '\n', '\r', '-', '~', ',', '.', ';', ':', '/', '\\', '\'', '\u{b4}', '`'];

fn compressed_name(s: &str) -> String {
    s.split(SEPARATORS).filter(|w| !w.is_empty() && !ARTICLES.iter().any(|a| a.eq_ignore_ascii_case(w))).collect()
}

/// Informations comparées pour trouver les doublons (`ComicBookDuplicateComparer`) :
/// série compressée, format, volume, numéro, langue, année, mois, jour, noir et blanc.
type DuplicateKey = (String, String, i32, String, String, i32, i32, i32, String);

/// Données calculées sur l'ensemble des livres, seulement si un critère en a besoin.
struct Ctx<'a> {
    books: &'a [LibraryBook],
    /// « Série complète » commune à tous les livres d'une série (série, volume).
    series_complete: Option<HashMap<(String, i32), YesNo>>,
    /// Livres en double parmi les candidats.
    duplicates: Option<HashSet<usize>>,
}

impl<'a> Ctx<'a> {
    fn new(books: &'a [LibraryBook], needs: &Needs, candidates: &[usize]) -> Self {
        let series_complete = needs.series.then(|| {
            // ComicBookSeriesStatistics : groupes (série proposée, volume) sur toute la bibliothèque.
            let mut m: HashMap<(String, i32), Option<YesNo>> = HashMap::new();
            for b in books {
                let v = yes_no(&b.series_complete);
                m.entry((shadow_series(b), b.volume.unwrap_or(-1)))
                    .and_modify(|e| {
                        if *e != Some(v) {
                            *e = Some(YesNo::Unknown);
                        }
                    })
                    .or_insert(Some(v));
            }
            m.into_iter().map(|(k, v)| (k, v.unwrap_or(YesNo::Unknown))).collect()
        });
        let duplicates = needs.duplicates.then(|| {
            let mut by_meta: HashMap<DuplicateKey, Vec<usize>> = HashMap::new();
            let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
            for &i in candidates {
                let b = &books[i];
                let key = (
                    // Casse respectée : ComicRack regroupe d'abord par une empreinte (hash)
                    // sensible à la casse, avant de comparer sans la casse.
                    compressed_name(&shadow_series(b)),
                    s(&b.format),
                    b.volume.unwrap_or(-1),
                    shadow_number(b),
                    s(&b.language_iso),
                    b.year.filter(|y| *y >= 0).unwrap_or(-1),
                    b.month.filter(|m| *m >= 0).unwrap_or(-1),
                    b.day.filter(|d| *d >= 0).unwrap_or(-1),
                    s(&b.black_and_white),
                );
                by_meta.entry(key).or_default().push(i);
                if let Some(f) = b.file.as_deref().filter(|f| !f.is_empty()) {
                    by_path.entry(f.to_string()).or_default().push(i);
                }
            }
            by_meta.into_values().chain(by_path.into_values()).filter(|g| g.len() > 1).flatten().collect()
        });
        Self { books, series_complete, duplicates }
    }
}

/// Calculs d'ensemble demandés par les critères.
#[derive(Default)]
struct Needs {
    series: bool,
    duplicates: bool,
}

impl Matcher {
    fn needs(&self, n: &mut Needs) {
        match self {
            Matcher::Value { kind, .. } => match getter(kind) {
                Some(Getter::SeriesAllComplete) => n.series = true,
                Some(Getter::Duplicate) => n.duplicates = true,
                _ => {}
            },
            Matcher::Group { matchers, .. } => matchers.iter().for_each(|m| m.needs(n)),
        }
    }

    /// Types de critères non pris en charge (libellés), à ce niveau et en dessous.
    pub fn unsupported(&self, out: &mut Vec<String>) {
        match self {
            Matcher::Value { kind, value, value2, .. } => {
                let field_ref = |v: &str| v.trim_start().starts_with('{') && v.trim_end().ends_with('}');
                if getter(kind).is_none() {
                    out.push(describe_kind(kind));
                } else if field_ref(value) || field_ref(value2) {
                    out.push(format!("{} comparé à un autre champ", describe_kind(kind)));
                }
            }
            Matcher::Group { matchers, .. } => matchers.iter().for_each(|m| m.unsupported(out)),
        }
    }

    pub fn is_time_dependent(&self) -> bool {
        match self {
            Matcher::Value { kind, op, .. } => op == &3 && matches!(getter(kind), Some(Getter::Date(_))),
            Matcher::Group { matchers, .. } => matchers.iter().any(Matcher::is_time_dependent),
        }
    }

    /// Un livre seul (sans statistiques de série ni doublons).
    pub fn matches(&self, b: &LibraryBook, today: NaiveDate) -> bool {
        let books = std::slice::from_ref(b);
        let ctx = Ctx::new(books, &Needs::default(), &[]);
        prepare(self, today).eval(0, &ctx)
    }
}

/// Critère préparé une fois pour tous les livres : type résolu, valeurs converties,
/// texte mis en minuscules, expression régulière compilée.
enum Prepared {
    Value {
        getter: Getter,
        op: i32,
        not: bool,
        ignore_case: bool,
        /// Valeur cherchée (en minuscules si la casse est ignorée).
        text: String,
        /// Valeurs de « contient l'un de / tous ».
        parts: Vec<String>,
        n1: f32,
        n2: f32,
        d1: Option<NaiveDate>,
        d2: Option<NaiveDate>,
        regex: Option<regex::Regex>,
    },
    Group {
        mode: Mode,
        not: bool,
        items: Vec<Prepared>,
    },
    /// Critère inconnu (jamais vrai ; la liste est de toute façon signalée).
    Never {
        not: bool,
    },
}

fn prepare(m: &Matcher, today: NaiveDate) -> Prepared {
    match m {
        Matcher::Group { mode, not, matchers } => {
            Prepared::Group { mode: *mode, not: *not, items: matchers.iter().map(|x| prepare(x, today)).collect() }
        }
        Matcher::Value { kind, op, value, value2, not, ignore_case } => {
            let Some(getter) = getter(kind) else { return Prepared::Never { not: *not } };
            let low = |v: &str| if *ignore_case { v.to_lowercase() } else { v.to_string() };
            let regex = (*op == 7 && matches!(getter, Getter::Str(_) | Getter::AllProperties))
                .then(|| regex::Regex::new(value).ok())
                .flatten();
            Prepared::Value {
                op: *op,
                not: *not,
                ignore_case: *ignore_case,
                text: if *op == 6 { value.trim().to_lowercase() } else { low(value) },
                parts: split_values(value).iter().map(|p| low(p)).collect(),
                n1: parse_num(value),
                n2: parse_num(value2),
                d1: parse_match_date(value, today),
                d2: parse_match_date(value2, today),
                regex,
                getter,
            }
        }
    }
}

/// Comparaison de texte (opérateurs de `ComicBookStringMatcher`) ; `mv`/`parts` sont déjà
/// en minuscules si la casse est ignorée.
fn str_test(op: i32, value: &str, mv: &str, parts: &[String], ignore_case: bool, regex: Option<&regex::Regex>) -> bool {
    if op == 7 {
        return regex.is_some_and(|r| r.is_match(value));
    }
    if op == 6 {
        // « la liste contient » : un des éléments séparés par , ou ; vaut exactement la valeur.
        return value.split([',', ';']).any(|p| p.trim().to_lowercase() == mv);
    }
    let lowered;
    let v = if ignore_case {
        lowered = value.to_lowercase();
        lowered.as_str()
    } else {
        value
    };
    match op {
        0 => v == mv,
        1 => mv.is_empty() || v.contains(mv),
        2 => parts.is_empty() || parts.iter().any(|p| v.contains(p.as_str())),
        3 => !v.is_empty() && parts.iter().all(|p| v.contains(p.as_str())),
        4 => mv.is_empty() || v.starts_with(mv),
        5 => mv.is_empty() || v.ends_with(mv),
        _ => false,
    }
}

fn yes_no_test(op: i32, v: YesNo) -> bool {
    match op {
        1 => v == YesNo::No,
        2 => v == YesNo::Unknown,
        _ => v == YesNo::Yes,
    }
}

impl Prepared {
    fn not(&self) -> bool {
        match self {
            Prepared::Value { not, .. } | Prepared::Group { not, .. } | Prepared::Never { not } => *not,
        }
    }

    /// Le livre `i` satisfait-il le critère (en tenant compte de `Not`) ?
    fn eval(&self, i: usize, ctx: &Ctx) -> bool {
        self.test(i, ctx) != self.not()
    }

    fn test(&self, i: usize, ctx: &Ctx) -> bool {
        let b = &ctx.books[i];
        let (getter, op, ic, text, parts, n1, n2, d1, d2, regex) = match self {
            Prepared::Never { .. } => return false,
            Prepared::Group { mode, items, .. } => return combine(*mode, items, i, ctx),
            Prepared::Value { getter, op, ignore_case, text, parts, n1, n2, d1, d2, regex, .. } => {
                (getter, *op, *ignore_case, text, parts, *n1, *n2, *d1, *d2, regex.as_ref())
            }
        };
        match getter {
            Getter::Str(f) => str_test(op, &f(b), text, parts, ic, regex),
            Getter::Num(f) => num_match(op, f(b), n1, n2),
            Getter::Date(f) => {
                // Date absente = DateTime.MinValue : avant toute autre date.
                let c = f(b).cmp(&d1);
                match op {
                    0 => c.is_eq(),
                    1 => c.is_gt(),
                    2 => c.is_lt(),
                    3 => c.is_ge(),
                    4 => c.is_ge() && f(b).cmp(&d2).is_le(),
                    _ => false,
                }
            }
            Getter::YesNo(f) => yes_no_test(op, f(b)),
            Getter::Manga => {
                let m = b.manga.as_deref().unwrap_or("Unknown");
                match op {
                    1 => m == "YesAndRightToLeft",
                    2 => m == "No",
                    3 => m == "Unknown" || m.is_empty(),
                    _ => m == "Yes",
                }
            }
            Getter::SeriesAllComplete => {
                let v = ctx
                    .series_complete
                    .as_ref()
                    .and_then(|m| m.get(&(shadow_series(b), b.volume.unwrap_or(-1))))
                    .copied()
                    .unwrap_or(YesNo::Unknown);
                yes_no_test(op, v)
            }
            // « Activé » (0) : seulement les doublons ; « désactivé » : tous.
            Getter::Duplicate => op != 0 || ctx.duplicates.as_ref().is_some_and(|d| d.contains(&i)),
            Getter::AllProperties => {
                let fields = [
                    shadow_series(b),
                    s(&b.title),
                    s(&b.writer),
                    s(&b.penciller),
                    s(&b.inker),
                    s(&b.colorist),
                    s(&b.cover_artist),
                    s(&b.publisher),
                    s(&b.genre),
                    s(&b.story_arc),
                    s(&b.characters),
                    s(&b.teams),
                    s(&b.locations),
                    s(&b.tags),
                    s(&b.summary),
                    s(&b.file),
                ];
                fields.iter().any(|f| str_test(op, f, text, parts, ic, regex))
            }
        }
    }
}

/// Combinaison de critères (`MatcherSet` de ComicRack) : « Et » = tous, « Ou » = au moins un ;
/// aucun critère = tous les livres.
fn combine(mode: Mode, items: &[Prepared], i: usize, ctx: &Ctx) -> bool {
    if items.is_empty() {
        return true;
    }
    match mode {
        Mode::And => items.iter().all(|m| m.eval(i, ctx)),
        Mode::Or => items.iter().any(|m| m.eval(i, ctx)),
    }
}

/// Mélange déterministe (xorshift) pour la sélection « au hasard ».
fn shuffle<T>(v: &mut [T], seed: i64) {
    let mut x = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
    if x == 0 {
        x = 1;
    }
    for i in (1..v.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

impl SmartQuery {
    /// Critères non pris en charge (vide : la liste peut être calculée).
    pub fn unsupported(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.matchers.iter().for_each(|m| m.unsupported(&mut out));
        out.sort();
        out.dedup();
        out
    }

    /// La sélection « au hasard » de ComicRack ne peut pas être reproduite à l'identique.
    pub fn is_approximate(&self) -> bool {
        self.limit.as_ref().is_some_and(|l| l.selection == Selection::Random)
    }

    pub fn is_time_dependent(&self) -> bool {
        self.matchers.iter().any(Matcher::is_time_dependent)
    }

    pub fn matches(&self, b: &LibraryBook) -> bool {
        self.matches_on(b, Local::now().date_naive())
    }

    /// Un livre seul (sans statistiques de série ni doublons).
    pub fn matches_on(&self, b: &LibraryBook, today: NaiveDate) -> bool {
        let books = std::slice::from_ref(b);
        let ctx = Ctx::new(books, &Needs::default(), &[]);
        let items: Vec<Prepared> = self.matchers.iter().map(|m| prepare(m, today)).collect();
        combine(self.mode, &items, 0, &ctx)
    }

    /// Indices des livres retenus parmi `candidates` (ordre conservé), limite appliquée.
    pub fn select(&self, books: &[LibraryBook], candidates: impl IntoIterator<Item = usize>) -> Vec<usize> {
        let today = Local::now().date_naive();
        let candidates: Vec<usize> = candidates.into_iter().collect();
        let mut needs = Needs::default();
        self.matchers.iter().for_each(|m| m.needs(&mut needs));
        let ctx = Ctx::new(books, &needs, &candidates);
        let filtered: HashSet<&str> = self.filtered.iter().map(String::as_str).collect();
        let items: Vec<Prepared> = self.matchers.iter().map(|m| prepare(m, today)).collect();
        let mut out: Vec<usize> = candidates
            .into_iter()
            .filter(|&i| combine(self.mode, &items, i, &ctx))
            .filter(|&i| !filtered.contains(books[i].id.to_ascii_lowercase().as_str()))
            .collect();
        if let Some(limit) = &self.limit {
            match limit.selection {
                Selection::Position => {}
                Selection::SortedBySeries => out.sort_by(|&a, &b| {
                    let (x, y) = (&books[a], &books[b]);
                    shadow_series(x)
                        .to_lowercase()
                        .cmp(&shadow_series(y).to_lowercase())
                        .then(s(&x.format).cmp(&s(&y.format)))
                        .then(x.volume.unwrap_or(-1).cmp(&y.volume.unwrap_or(-1)))
                        .then_with(|| natord::compare(&shadow_number(x), &shadow_number(y)))
                }),
                Selection::Random => {
                    out.sort_by(|&a, &b| books[a].id.cmp(&books[b].id));
                    shuffle(&mut out, limit.seed);
                }
            }
            match limit.kind {
                LimitKind::Count => out.truncate(limit.value.max(0) as usize),
                LimitKind::Mb | LimitKind::Gb => {
                    let max = limit.value * 1024 * 1024 * if limit.kind == LimitKind::Gb { 1024 } else { 1 };
                    // `TakeWhile` : on s'arrête au premier dépassement.
                    let mut total = 0i64;
                    let cut = out
                        .iter()
                        .position(|&i| {
                            total += books[i].file_size.unwrap_or(0) as i64;
                            total >= max
                        })
                        .unwrap_or(out.len());
                    out.truncate(cut);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(kind: &str, op: i32, value: &str) -> Matcher {
        Matcher::Value {
            kind: kind.into(),
            op,
            value: value.into(),
            value2: String::new(),
            not: false,
            ignore_case: true,
        }
    }

    fn book() -> LibraryBook {
        LibraryBook::from_xml(
            r#"<ComicBook Id="A" File="Z:\BD\COSMIC COLLECTION #224 2012_05 - Thanos Rising.cbz">
<Series>COSMIC COLLECTION</Series><Number>224</Number><Volume>3</Volume><Year>2012</Year><Month>5</Month>
<Writer>Jason Aaron, Jim Starlin</Writer><Characters>Thanos; Mort</Characters><PageCount>100</PageCount>
<LastPageRead>49</LastPageRead><Rating>4</Rating><Added>2019-10-01T12:58:00+02:00</Added>
<BlackAndWhite>No</BlackAndWhite><FileSize>52428800</FileSize></ComicBook>"#,
        )
        .unwrap()
    }

    #[test]
    fn string_operators() {
        let b = book();
        let today = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let t = |m: Matcher| m.matches(&b, today);
        assert!(t(v("ComicBookSeriesMatcher", 0, "cosmic collection")));
        assert!(t(v("ComicBookWriterMatcher", 1, "starlin")));
        assert!(t(v("ComicBookWriterMatcher", 2, "Byrne, Starlin")));
        assert!(!t(v("ComicBookWriterMatcher", 3, "Byrne Starlin")));
        assert!(t(v("ComicBookWriterMatcher", 3, "Aaron Starlin")));
        assert!(t(v("ComicBookWriterMatcher", 4, "jason")));
        assert!(t(v("ComicBookCharactersMatcher", 6, "mort")));
        assert!(!t(v("ComicBookCharactersMatcher", 6, "mor")));
        assert!(t(v("ComicBookFileMatcher", 7, r"#\d{3}")));
        assert!(t(v("ComicBookTitleMatcher", 1, "")), "« contient » vide : vrai, comme ComicRack");
        assert!(!t(v("ComicBookTitleMatcher", 3, "x")), "« contient tous » sur un champ vide : faux");
    }

    #[test]
    fn numbers_dates_yes_no() {
        let b = book();
        let today = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let t = |m: Matcher| m.matches(&b, today);
        assert!(t(v("ComicBookNumberMatcher", 0, "224")));
        assert!(t(v("ComicBookYearMatcher", 1, "2011")));
        assert!(t(Matcher::Value {
            kind: "ComicBookReadPercentageMatcher".into(),
            op: 3,
            value: "1".into(),
            value2: "99".into(),
            not: false,
            ignore_case: true
        }));
        assert!(t(v("ComicBookFileSizeMatcher", 1, "49.5")), "taille en Mo");
        assert!(t(v("ComicBookAddedMatcher", 2, "01/01/2020")), "date française jour/mois/année");
        assert!(t(v("ComicBookAddedMatcher", 0, "2019-10-01")));
        assert!(!t(v("ComicBookAddedMatcher", 3, "30")), "pas ajouté dans les 30 derniers jours");
        assert!(t(v("ComicBookPublishedMatcher", 1, "30/04/2012")));
        assert!(t(v("ComicBookBlackAndWhiteMatcher", 1, "")));
        assert!(t(v("ComicBookSeriesCompleteMatcher", 2, "")), "absent = inconnu");
        assert!(t(v("ComicBookCheckedMatcher", 0, "")), "coché par défaut");
    }

    #[test]
    fn groups_not_and_unsupported() {
        let b = book();
        let today = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let q = SmartQuery {
            mode: Mode::And,
            matchers: vec![
                v("ComicBookSeriesMatcher", 1, "cosmic"),
                Matcher::Group {
                    mode: Mode::Or,
                    not: false,
                    matchers: vec![v("ComicBookYearMatcher", 0, "1990"), v("ComicBookVolumeMatcher", 0, "3")],
                },
                Matcher::Value {
                    kind: "ComicBookWriterMatcher".into(),
                    op: 1,
                    value: "Byrne".into(),
                    value2: String::new(),
                    not: true,
                    ignore_case: true,
                },
            ],
            ..Default::default()
        };
        assert!(q.matches_on(&b, today));
        assert!(q.unsupported().is_empty());
        let q2 = SmartQuery {
            matchers: vec![v("SmartListSeriesCountMatcher", 1, "3"), v("ComicBookCustomValuesMatcher", 0, "x")],
            ..Default::default()
        };
        assert_eq!(q2.unsupported(), ["statistique de série (Count)", "valeurs personnalisées"]);
        let q3 = SmartQuery { matchers: vec![v("ComicBookSeriesMatcher", 0, "{Title}")], ..Default::default() };
        assert_eq!(q3.unsupported().len(), 1);
    }

    #[test]
    fn series_complete_and_duplicates() {
        let mk = |id: &str, series: &str, number: &str, complete: &str, file: &str| LibraryBook {
            id: id.into(),
            series: Some(series.into()),
            number: Some(number.into()),
            series_complete: Some(complete.into()),
            file: Some(file.into()),
            ..Default::default()
        };
        let books = vec![
            mk("a", "Blacksad", "1", "Yes", "X:/Blacksad 1.cbz"),
            mk("b", "Blacksad", "2", "Yes", "X:/Blacksad 2.cbz"),
            mk("c", "Thorgal", "1", "Yes", "X:/Thorgal 1.cbz"),
            mk("d", "Thorgal", "2", "No", "X:/Thorgal 2.cbz"),
            mk("e", "The Blacksad", "2", "Yes", "X:/autre/Blacksad 2.cbz"),
            mk("f", "Lanfeust", "1", "No", "X:/Lanfeust 1.cbz"),
            mk("g", "Lanfeust", "9", "No", "X:/Lanfeust 1.cbz"),
            // Casse différente : pas des doublons pour ComicRack (empreinte sensible à la casse).
            mk("h", "BLACKSAD", "1", "Yes", "x:/BLACKSAD 1.cbz"),
            mk("i", "Thorgal", "3", "Yes", "x:/thorgal 1.cbz"),
        ];
        let q = |kind: &str, op: i32| SmartQuery { matchers: vec![v(kind, op, "")], ..Default::default() };
        // Toute la série « Oui » : Blacksad (a, b) ; « The Blacksad » est une autre série.
        assert_eq!(q("SmartListSeriesAllCompleteMatcher", 0).select(&books, 0..7), [0, 1, 4]);
        let all = 0..books.len();
        assert_eq!(
            q("SmartListSeriesAllCompleteMatcher", 2).select(&books, 0..7),
            [2, 3],
            "Thorgal : mélange → inconnu"
        );
        // Doublons : Blacksad 2 (« The » ignoré) par les infos, Lanfeust par le chemin.
        assert_eq!(q("ComicBookDuplicateMatcher", 0).select(&books, all), [1, 4, 5, 6]);
        assert_eq!(q("ComicBookDuplicateMatcher", 1).select(&books, 0..7).len(), 7, "désactivé : tous");
    }

    #[test]
    fn limits() {
        let mut books = Vec::new();
        for i in 0..10 {
            let mut b = book();
            b.id = format!("id{i}");
            b.number = Some((10 - i).to_string());
            books.push(b);
        }
        let q = |limit| SmartQuery { limit: Some(limit), ..Default::default() };
        let pos = q(Limit { kind: LimitKind::Count, value: 3, selection: Selection::Position, seed: 0 });
        assert_eq!(pos.select(&books, 0..10), [0, 1, 2]);
        let series = q(Limit { kind: LimitKind::Count, value: 2, selection: Selection::SortedBySeries, seed: 0 });
        assert_eq!(series.select(&books, 0..10), [9, 8], "numéros 1 et 2");
        let mb = q(Limit { kind: LimitKind::Mb, value: 120, selection: Selection::Position, seed: 0 });
        assert_eq!(mb.select(&books, 0..10).len(), 2, "50 Mo chacun : 2 sous 120 Mo");
        let rnd = q(Limit { kind: LimitKind::Count, value: 4, selection: Selection::Random, seed: 7 });
        assert_eq!(rnd.select(&books, 0..10), rnd.select(&books, 0..10), "même graine, même tirage");
        assert!(rnd.is_approximate());
        let f = SmartQuery { filtered: vec!["id1".into()], ..Default::default() };
        assert_eq!(f.select(&books, 0..3), [0, 2]);
    }
}
