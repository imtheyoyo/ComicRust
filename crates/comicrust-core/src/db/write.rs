//! Écriture dans la base MySQL de ComicRack (fiches `comics.data`).
//!
//! Rien n'est écrit tant que l'interface n'appelle pas [`update_books`] : le reste du
//! module `db` ne fait que des `SELECT`.
//!
//! # Fiche modifiée « au plus juste »
//!
//! ComicRust ne modélise pas tous les éléments d'une fiche (`ColorAdjustment`,
//! `ExtraSyncInformation`, éléments inconnus gardés par ComicRack dans
//! `UnparsedElements`…). La fiche n'est donc **jamais** régénérée depuis un
//! [`LibraryBook`] : le XML actuel est relu en base, seuls les éléments demandés sont
//! remplacés, ajoutés ou supprimés ([`patch_comic_book_xml`]), et tout le reste est
//! recopié octet pour octet (déclaration XML, espaces de noms, indentation éventuelle,
//! éléments inconnus, entités…).
//!
//! **Ordre des éléments** : celui de `XmlSerializer` pour `ComicBook`, c'est-à-dire les
//! membres de la classe de base `ComicInfo` d'abord (ordre de déclaration dans
//! `ComicInfo.cs` : `Title` … `Tags`, puis `UnparsedElements` (`[XmlAnyElement]`, les
//! éléments inconnus), puis `Pages`), ensuite ceux de `ComicBook.cs` (`Added`, `Released`,
//! `Opened`, `OpenCount`, `CurrentPage`, `LastPageRead`, `Rating`, … `CustomValuesStore`),
//! voir [`ELEMENTS`]. Un élément ajouté est inséré devant le premier élément existant
//! qui le suit dans cet ordre (ce qui reste correct même si la fiche n'est pas dans
//! l'ordre canonique), avec la même indentation que ses voisins.
//!
//! **Valeurs** au format de `XmlSerializer` / `XmlConvert` : dates « aller-retour »
//! (`2026-10-24T21:05:00.1234567+02:00`, fraction sans zéros de fin), booléens
//! `true`/`false`, réels au format court invariant (`4.5`), et un élément égal à son
//! `[DefaultValue]` est supprimé (comme le ferait `XmlSerializer`). Les valeurs sont
//! typées ([`Value`]) et vérifiées : une fiche que ComicRack ne saurait pas relire
//! bloquerait **toutes** ses actualisations (`Refresh` lève une exception et ne fait
//! plus avancer son compteur), d'où la relecture de contrôle de chaque fiche écrite.
//!
//! # Protocole des compteurs (référence : `ComicStorageBaseSql.cs`)
//!
//! - `changes` n'a qu'**une** ligne, d'identifiant `StorageGuid = Guid.Empty`
//!   (`public static readonly Guid StorageGuid = Guid.Empty;`), commune à toutes les
//!   instances : `WriteCounter` fait `Update changes set update_counter={1} where id='{0}'`
//!   avec `StorageGuid`, et l'insère (`Insert into changes values ('{StorageGuid}', 0, 0)`)
//!   si elle manque ; `ReadCounter` lit `Select update_counter from changes` (première
//!   ligne, sans `where`). Il n'y a donc **pas** de ligne par instance : ComicRust ne crée
//!   pas d'identifiant à lui (une deuxième ligne pourrait même devenir celle que lit
//!   `ReadCounter`) et incrémente la ligne `Guid.Empty`, comme ComicRack.
//! - `Write(book)` : `num = ReadUpdateCounter() + 1`, `Update comics set data=@data,
//!   update_counter={num} where id=…`, puis `WriteUpdateCounter(ReadUpdateCounter() + 1)`.
//!   Le livre reçoit donc la nouvelle valeur du compteur global.
//! - `Refresh` (appelé chaque seconde par `ComicStorage.WorkQueue`) : si
//!   `lastUpdate < ReadUpdateCounter()`, relit `Select id, update_counter, data from comics
//!   where update_counter>{lastUpdate}`, recopie chaque fiche dans le livre en mémoire
//!   (`CopyFrom`), puis `lastUpdate = ReadUpdateCounter()`.
//!
//! ComicRust fait exactement la même chose, mais **atomiquement** dans une transaction :
//! `UPDATE changes SET update_counter = update_counter + 1 WHERE id = Guid.Empty` (verrou
//! de ligne jusqu'à la validation), relecture de la valeur `num`, puis
//! `UPDATE comics SET data = ?, update_counter = num WHERE id = ? AND update_counter <=> lu`
//! (condition optimiste). Si aucune ligne n'est touchée, ComicRack (ou un autre
//! ComicRust) a écrit le livre entre la lecture et l'écriture : annulation (le compteur
//! revient aussi), relecture et nouvel essai (au plus [`MAX_ATTEMPTS`]), puis conflit
//! signalé. Une instance de ComicRack lancée voit donc, à son actualisation suivante,
//! `lastUpdate < num` et relit ce livre (`update_counter = num > lastUpdate`).
//!
//! # Conséquences connues côté ComicRack
//!
//! - `CopyFrom` déclenche `BookChanged`, donc ComicRack **réécrit** ensuite le livre
//!   (`ComicStorage.library_BookChanged` → `AddWriteToQueue`) avec son propre
//!   sérialiseur : un « écho » que la surveillance de ComicRust voit comme une
//!   modification extérieure (contenu équivalent, compteur nouveau).
//! - `CopyFrom` ne recopie pas `ComicInfoIsDirty`, `ComicBookIsDirty`, `Missing`
//!   (`FileIsMissing`), `NewPages` ni `IsDynamicSource` : si ComicRack est lancé, sa
//!   valeur en mémoire de ces indicateurs l'emporte à sa prochaine écriture du livre
//!   (l'écho ci-dessus compris).

use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, Local, Timelike};
use mysql::prelude::Queryable;
use mysql::{Conn, TxOpts};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{connect, db_err, ComicDatabase, DbConfig, Record, Watcher};
use crate::library::{LibraryBook, PageInfo};
use crate::Error;

/// Identifiant de l'unique ligne de `changes` (`ComicStorageBaseSql.StorageGuid = Guid.Empty`).
pub const STORAGE_GUID: &str = "00000000-0000-0000-0000-000000000000";

/// Nombre d'essais d'écriture d'un livre avant de signaler un conflit.
pub const MAX_ATTEMPTS: usize = 4;

// ---------------------------------------------------------------------------------------
// Valeurs et modifications
// ---------------------------------------------------------------------------------------

/// Nouvelle valeur d'un élément de la fiche.
#[derive(Debug, Clone)]
pub enum Value {
    /// Texte (chaînes et énumérations : `Yes`, `No`, `Unknown`…, GUID).
    Text(String),
    Int(i64),
    Float(f32),
    Bool(bool),
    /// Date (écrite au format aller-retour de .NET, voir [`net_datetime`]).
    Date(DateTime<FixedOffset>),
    /// Liste `<Pages>` complète.
    Pages(Vec<PageInfo>),
    /// Ajoute à la valeur entière actuelle (ou à la valeur par défaut si l'élément est
    /// absent) : `OpenCount` +1 sans perdre une ouverture faite entre-temps dans ComicRack.
    Add(i64),
    /// Garde le maximum de la valeur actuelle et de celle-ci (`LastPageRead`, comme le
    /// setter `ComicBook.CurrentPage`).
    Max(i64),
}

/// Modification d'un élément : `Some(valeur)` le remplace ou l'ajoute, `None` le supprime.
///
/// `element` est le nom XML (`CurrentPage`, `Opened`, `OpenCount`, `File`…) ; les noms
/// de propriété .NET (`OpenedTime`, `OpenedCount`, `FilePath`, `FileIsMissing`…) sont
/// aussi acceptés.
#[derive(Debug, Clone)]
pub struct Change {
    pub element: String,
    pub value: Option<Value>,
}

impl Change {
    pub fn set(element: &str, value: Value) -> Self {
        Self { element: element.to_string(), value: Some(value) }
    }

    pub fn remove(element: &str) -> Self {
        Self { element: element.to_string(), value: None }
    }
}

/// État du fichier d'un livre (après une conversion CBR → CBZ, par exemple).
#[derive(Debug, Clone, Default)]
pub struct FileState {
    /// Chemin complet, tel que ComicRack le voit (attribut `File`).
    pub path: String,
    pub size: u64,
    pub modified: Option<DateTime<FixedOffset>>,
    pub created: Option<DateTime<FixedOffset>>,
    /// `None` : inchangé.
    pub page_count: Option<u32>,
    /// `None` : liste `<Pages>` inchangée.
    pub pages: Option<Vec<PageInfo>>,
}

/// Modifications d'un livre (seulement les champs changés).
#[derive(Debug, Clone, Default)]
pub struct BookEdit {
    /// Identifiant (GUID, casse indifférente).
    pub id: String,
    pub changes: Vec<Change>,
}

impl BookEdit {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string(), changes: Vec::new() }
    }

    /// Forme générique : élément → valeur.
    pub fn set(mut self, element: &str, value: Value) -> Self {
        self.changes.push(Change::set(element, value));
        self
    }

    /// Forme générique : suppression d'un élément (retour à la valeur par défaut).
    pub fn remove(mut self, element: &str) -> Self {
        self.changes.push(Change::remove(element));
        self
    }

    /// Page courante ; `LastPageRead` suit si elle est dépassée (comme ComicRack).
    pub fn progress(self, current_page: u32) -> Self {
        self.set("CurrentPage", Value::Int(current_page.into())).set("LastPageRead", Value::Max(current_page.into()))
    }

    /// Livre ouvert : date de dernière ouverture et compteur d'ouvertures +1.
    pub fn opened(self, at: DateTime<FixedOffset>) -> Self {
        self.set("Opened", Value::Date(at)).set("OpenCount", Value::Add(1))
    }

    /// Note de 0 à 5 (0 : pas de note).
    pub fn rating(self, rating: f32) -> Self {
        self.set("Rating", Value::Float(rating))
    }

    /// Infos du livre à écrire dans le fichier (`ComicInfoIsDirty`, la case
    /// « Mettre à jour les fichiers » de ComicRack) ; `false` après l'écriture.
    pub fn comic_info_dirty(self, dirty: bool) -> Self {
        self.set("ComicInfoIsDirty", Value::Bool(dirty))
    }

    /// Nouveau fichier du livre (chemin, taille, dates, pages).
    pub fn file(mut self, f: &FileState) -> Self {
        self = self.set("File", Value::Text(f.path.clone())).set("FileSize", Value::Int(f.size as i64));
        for (name, date) in [("FileModifiedTime", f.modified), ("FileCreationTime", f.created)] {
            self.changes.push(Change { element: name.into(), value: date.map(Value::Date) });
        }
        if let Some(n) = f.page_count {
            self = self.set("PageCount", Value::Int(n.into()));
        }
        if let Some(p) = &f.pages {
            self = self.set("Pages", Value::Pages(p.clone()));
        }
        self
    }
}

/// Date et heure locales actuelles (pour `Opened`).
pub fn now() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

/// Date au format de `XmlConvert.ToString(DateTime, RoundtripKind)` pour une date
/// locale : `yyyy-MM-ddTHH:mm:ss.FFFFFFFzzz` (fraction en ticks de 100 ns, zéros de fin
/// et point omis).
pub fn net_datetime(dt: &DateTime<FixedOffset>) -> String {
    let ticks = dt.nanosecond() % 1_000_000_000 / 100;
    let mut s = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
    if ticks != 0 {
        s.push('.');
        s.push_str(format!("{ticks:07}").trim_end_matches('0'));
    }
    s.push_str(&dt.format("%:z").to_string());
    s
}

// ---------------------------------------------------------------------------------------
// Schéma de la fiche `<ComicBook>`
// ---------------------------------------------------------------------------------------

/// Type d'un élément et sa valeur par défaut (`[DefaultValue]`).
#[derive(Debug, Clone, Copy)]
enum Kind {
    /// Chaîne, défaut `""`.
    Text,
    /// Chaîne, défaut `null` : une chaîne vide est écrite (`<X />`).
    TextOrNull,
    Int(i64),
    Float(f32),
    Bool(bool),
    /// Date, défaut `DateTime.MinValue` (supprimer l'élément).
    Date,
    /// Énumération : valeurs permises, la première est le défaut.
    Choice(&'static [&'static str]),
    /// GUID, défaut `Guid.Empty`.
    Guid,
    Pages,
    /// Élément complexe que ComicRust ne sait pas écrire.
    Unsupported,
}

const YES_NO: &[&str] = &["Unknown", "No", "Yes"];
const MANGA: &[&str] = &["Unknown", "No", "Yes", "YesAndRightToLeft"];

/// Place des éléments inconnus (`ComicInfo.UnparsedElements`, `[XmlAnyElement]`).
const UNPARSED: &str = "*";

/// Éléments de `<ComicBook>` dans l'ordre de `XmlSerializer` (membres de `ComicInfo`,
/// puis de `ComicBook`, chacun dans l'ordre de déclaration).
pub const ELEMENTS: &[&str] = &[
    // ComicInfo.cs
    "Title",
    "Series",
    "Number",
    "Count",
    "Volume",
    "AlternateSeries",
    "AlternateNumber",
    "StoryArc",
    "SeriesGroup",
    "AlternateCount",
    "Summary",
    "Notes",
    "Review",
    "Year",
    "Month",
    "Day",
    "Writer",
    "Penciller",
    "Inker",
    "Colorist",
    "Letterer",
    "CoverArtist",
    "Editor",
    "Translator",
    "Publisher",
    "Imprint",
    "Genre",
    "Web",
    "PageCount",
    "LanguageISO",
    "Format",
    "AgeRating",
    "BlackAndWhite",
    "Manga",
    "PreferredFrontCover",
    "Characters",
    "Teams",
    "MainCharacterOrTeam",
    "Locations",
    "CommunityRating",
    "ScanInformation",
    "Tags",
    UNPARSED,
    "Pages",
    // ComicBook.cs
    "Added",
    "Released",
    "Opened",
    "OpenCount",
    "CurrentPage",
    "LastPageRead",
    "Rating",
    "ColorAdjustment",
    "EnableProposed",
    "SeriesComplete",
    "EnableDynamicUpdate",
    "LastOpenedFromListId",
    "ComicInfoIsDirty",
    "ComicBookIsDirty",
    "FileSize",
    "Missing",
    "FileModifiedTime",
    "FileCreationTime",
    "CustomThumbnailKey",
    "BookPrice",
    "BookAge",
    "BookCondition",
    "BookStore",
    "BookOwner",
    "BookCollectionStatus",
    "BookNotes",
    "BookLocation",
    "ISBN",
    "NewPages",
    "ExtraSyncInformation",
    "CustomValuesStore",
];

/// Attributs de `<ComicBook>`, dans l'ordre de `XmlSerializer` (après les `xmlns`).
const ATTRIBUTES: &[&str] = &["Id", "Checked", "File", "IsDynamicSource"];

fn element_kind(name: &str) -> Option<Kind> {
    Some(match name {
        "Count" | "Volume" | "AlternateCount" | "Year" | "Month" | "Day" | "FileSize" => Kind::Int(-1),
        "PageCount" | "PreferredFrontCover" | "OpenCount" | "CurrentPage" | "LastPageRead" | "NewPages" => Kind::Int(0),
        "CommunityRating" | "Rating" => Kind::Float(0.0),
        "BookPrice" => Kind::Float(-1.0),
        "BlackAndWhite" | "SeriesComplete" => Kind::Choice(YES_NO),
        "Manga" => Kind::Choice(MANGA),
        "Pages" => Kind::Pages,
        "Added" | "Released" | "Opened" | "FileModifiedTime" | "FileCreationTime" => Kind::Date,
        "EnableProposed" | "EnableDynamicUpdate" => Kind::Bool(true),
        "ComicInfoIsDirty" | "ComicBookIsDirty" | "Missing" => Kind::Bool(false),
        "LastOpenedFromListId" => Kind::Guid,
        "CustomThumbnailKey" => Kind::TextOrNull,
        "ColorAdjustment" | "ExtraSyncInformation" => Kind::Unsupported,
        UNPARSED => return None,
        n if ELEMENTS.contains(&n) => Kind::Text,
        _ => return None,
    })
}

fn attribute_kind(name: &str) -> Option<Kind> {
    Some(match name {
        "Checked" => Kind::Bool(true),
        "IsDynamicSource" => Kind::Bool(false),
        "File" => Kind::Text,
        "Id" => Kind::Unsupported,
        _ => return None,
    })
}

/// Nom XML pour un nom de propriété .NET.
fn canonical(name: &str) -> &str {
    match name {
        "AddedTime" => "Added",
        "ReleasedTime" => "Released",
        "OpenedTime" => "Opened",
        "OpenedCount" => "OpenCount",
        "FilePath" => "File",
        "FileIsMissing" => "Missing",
        n => n,
    }
}

fn rank(name: &str) -> usize {
    let unparsed = ELEMENTS.iter().position(|e| *e == UNPARSED).unwrap_or(0);
    ELEMENTS.iter().position(|e| *e == name).unwrap_or(unparsed)
}

fn attr_rank(name: &str) -> usize {
    // Espaces de noms et attributs inconnus : en tête (XmlSerializer écrit les xmlns d'abord).
    ATTRIBUTES.iter().position(|a| *a == name).map_or(0, |p| p + 1)
}

// ---------------------------------------------------------------------------------------
// Rendu des valeurs
// ---------------------------------------------------------------------------------------

fn bad(msg: impl std::fmt::Display) -> Error {
    Error::Decode(format!("fiche ComicRack : {msg}"))
}

/// Caractères interdits en XML 1.0 (ComicRack ne relirait pas la fiche).
fn check_chars(s: &str, what: &str) -> Result<(), Error> {
    match s.chars().find(|&c| (c < ' ' && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{FFFE}' | '\u{FFFF}')) {
        Some(c) => Err(bad(format!("caractère interdit en XML (U+{:04X}) dans {what}", c as u32))),
        None => Ok(()),
    }
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
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
            c => out.push(c),
        }
    }
    out
}

fn type_mismatch(name: &str, v: &Value) -> Error {
    let t = match v {
        Value::Text(_) => "texte",
        Value::Int(_) => "entier",
        Value::Float(_) => "réel",
        Value::Bool(_) => "booléen",
        Value::Date(_) => "date",
        Value::Pages(_) => "liste de pages",
        Value::Add(_) | Value::Max(_) => "opération entière",
    };
    bad(format!("valeur de type {t} impossible pour « {name} »"))
}

/// Valeur (non échappée) à écrire, ou `None` si c'est la valeur par défaut (élément ou
/// attribut à supprimer). `current` : texte actuel (pour `Add` / `Max`).
fn scalar(name: &str, kind: Kind, value: &Value, current: Option<&str>) -> Result<Option<String>, Error> {
    let mismatch = || type_mismatch(name, value);
    Ok(match kind {
        Kind::Text | Kind::TextOrNull => {
            let Value::Text(s) = value else { return Err(mismatch()) };
            check_chars(s, name)?;
            (matches!(kind, Kind::TextOrNull) || !s.is_empty()).then(|| s.clone())
        }
        Kind::Int(def) => {
            let cur = || -> Result<i64, Error> {
                match current.map(str::trim) {
                    None | Some("") => Ok(def),
                    Some(t) => t.parse().map_err(|_| bad(format!("« {name} » actuel n'est pas un entier : {t}"))),
                }
            };
            let v = match value {
                Value::Int(v) => *v,
                Value::Add(d) => cur()?.saturating_add(*d),
                Value::Max(m) => cur()?.max(*m),
                _ => return Err(mismatch()),
            };
            if !matches!(name, "FileSize") && i32::try_from(v).is_err() {
                return Err(bad(format!("« {name} » hors limites : {v}")));
            }
            (v != def).then(|| v.to_string())
        }
        Kind::Float(def) => {
            let Value::Float(mut f) = *value else { return Err(mismatch()) };
            if !f.is_finite() {
                return Err(bad(format!("« {name} » invalide : {f}")));
            }
            if name == "Rating" {
                f = f.clamp(0.0, 5.0); // comme le setter ComicBook.Rating
            }
            (f != def).then(|| f.to_string())
        }
        Kind::Bool(def) => {
            let Value::Bool(b) = *value else { return Err(mismatch()) };
            (b != def).then(|| b.to_string())
        }
        Kind::Date => {
            let Value::Date(d) = value else { return Err(mismatch()) };
            Some(net_datetime(d))
        }
        Kind::Choice(list) => {
            let Value::Text(s) = value else { return Err(mismatch()) };
            if !list.contains(&s.as_str()) {
                return Err(bad(format!("« {name} » : valeur « {s} » inconnue (permises : {})", list.join(", "))));
            }
            (s != list[0]).then(|| s.clone())
        }
        Kind::Guid => {
            let Value::Text(s) = value else { return Err(mismatch()) };
            let g = uuid::Uuid::parse_str(s.trim()).map_err(|_| bad(format!("« {name} » : GUID invalide « {s} »")))?;
            (!g.is_nil()).then(|| g.hyphenated().to_string())
        }
        Kind::Pages | Kind::Unsupported => return Err(bad(format!("« {name} » ne s'écrit pas ainsi"))),
    })
}

const PAGE_TYPES: &[&str] = &[
    "FrontCover",
    "InnerCover",
    "Roundup",
    "Story",
    "Advertisement",
    "Advertisment",
    "Editorial",
    "Letters",
    "Preview",
    "BackCover",
    "Other",
    "Deleted",
];

/// `<Page …/>` dans l'ordre des attributs de `ComicPageInfo` (Image, Bookmark, ImageSize,
/// ImageWidth, ImageHeight, Rotation, PagePosition, Key, Type), valeurs par défaut omises.
fn render_page(p: &PageInfo) -> Result<String, Error> {
    let mut s = format!("<Page Image=\"{}\"", p.image);
    let mut attr = |name: &str, v: &str| {
        s.push_str(&format!(" {name}=\"{}\"", escape_attr(v)));
    };
    if let Some(b) = &p.bookmark {
        check_chars(b, "un signet")?;
        attr("Bookmark", b);
    }
    for (name, v) in [("ImageSize", &p.image_size), ("ImageWidth", &p.image_width), ("ImageHeight", &p.image_height)] {
        if let Some(v) = v {
            let n: i32 = v.trim().parse().map_err(|_| bad(format!("page {} : {name} invalide « {v} »", p.image)))?;
            if n != 0 {
                attr(name, &n.to_string());
            }
        }
    }
    for (name, v, list) in [
        ("Rotation", &p.rotation, &["None", "Rotate90", "Rotate180", "Rotate270"][..]),
        ("PagePosition", &p.page_position, &["Default", "Near", "Far"][..]),
    ] {
        if let Some(v) = v {
            if !list.contains(&v.as_str()) {
                return Err(bad(format!("page {} : {name} inconnu « {v} »", p.image)));
            }
            if v != list[0] {
                attr(name, v);
            }
        }
    }
    if let Some(k) = &p.key {
        check_chars(k, "une clé de page")?;
        attr("Key", k);
    }
    if let Some(t) = &p.kind {
        if t.split(',').any(|part| !PAGE_TYPES.contains(&part.trim())) {
            return Err(bad(format!("page {} : type inconnu « {t} »", p.image)));
        }
        if t.trim() != "Story" {
            attr("Type", t);
        }
    }
    s.push_str(" />");
    Ok(s)
}

/// `<Pages>…</Pages>` (vide : `<Pages />`, comme `XmlSerializer` pour une collection
/// vide). `ws` : espace qui précède l'élément ; s'il contient un saut de ligne, la fiche
/// est indentée et les `<Page>` le sont d'un cran de plus.
fn render_pages(pages: &[PageInfo], ws: &str) -> Result<String, Error> {
    if pages.is_empty() {
        return Ok("<Pages />".into());
    }
    let indented = ws.contains('\n');
    let mut s = String::from("<Pages>");
    for p in pages {
        if indented {
            s.push_str(ws);
            s.push_str("  ");
        }
        s.push_str(&render_page(p)?);
    }
    if indented {
        s.push_str(ws);
    }
    s.push_str("</Pages>");
    Ok(s)
}

fn render_element(name: &str, content: &str) -> String {
    if content.is_empty() {
        format!("<{name} />")
    } else {
        format!("<{name}>{}</{name}>", escape_text(content))
    }
}

// ---------------------------------------------------------------------------------------
// Repérage dans le XML (positions en octets)
// ---------------------------------------------------------------------------------------

struct Child {
    name: String,
    /// Début de l'espace qui précède l'élément (= `start` s'il n'y en a pas).
    lead: usize,
    start: usize,
    end: usize,
    /// Contenu entre les balises (absent pour `<X/>`).
    inner: Option<(usize, usize)>,
}

struct Doc {
    /// Balise ouvrante de la racine.
    root_tag: (usize, usize),
    root_name: String,
    /// `<ComicBook … />` sans contenu.
    self_closing: bool,
    /// Début de `</ComicBook>`.
    root_close: usize,
    children: Vec<Child>,
}

fn parse_doc(xml: &str) -> Result<Doc, Error> {
    let mut reader = Reader::from_str(xml);
    let mut depth = 0usize;
    let mut pending_ws: Option<usize> = None;
    let mut root: Option<((usize, usize), String, bool)> = None;
    let mut root_close = None;
    let mut cur: Option<(Child, usize)> = None;
    let mut children = Vec::new();
    loop {
        let pos = reader.buffer_position() as usize;
        let ev = reader.read_event().map_err(|e| bad(format!("XML illisible : {e}")))?;
        let end = reader.buffer_position() as usize;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if depth == 0 {
                    if root.is_some() {
                        return Err(bad("plusieurs éléments racine"));
                    }
                    if name != "ComicBook" {
                        return Err(bad(format!("racine <{name}> au lieu de <ComicBook>")));
                    }
                    if empty {
                        root_close = Some(end);
                    }
                    root = Some(((pos, end), name, empty));
                } else if depth == 1 {
                    let child = Child { name, lead: pending_ws.unwrap_or(pos), start: pos, end, inner: None };
                    if empty {
                        children.push(child);
                    } else {
                        cur = Some((child, end));
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    root_close = Some(pos);
                } else if depth == 1 {
                    if let Some((mut c, inner_start)) = cur.take() {
                        c.end = end;
                        c.inner = Some((inner_start, pos));
                        children.push(c);
                    }
                }
            }
            Event::Text(ref t) if depth == 1 && t.iter().all(u8::is_ascii_whitespace) => {
                pending_ws.get_or_insert(pos);
                continue;
            }
            Event::Eof => break,
            _ => {}
        }
        pending_ws = None;
    }
    let ((root_tag, root_name, self_closing), Some(root_close)) = (root.ok_or_else(|| bad("fiche vide"))?, root_close)
    else {
        return Err(bad("document incomplet"));
    };
    if depth != 0 {
        return Err(bad("document incomplet"));
    }
    Ok(Doc { root_tag, root_name, self_closing, root_close, children })
}

struct Attr {
    name: String,
    /// Début de l'espace qui précède l'attribut.
    lead: usize,
    /// Début du nom.
    start: usize,
    /// Après le guillemet fermant.
    end: usize,
}

/// Attributs de la balise ouvrante `xml[s..e]` (positions absolues). Renvoie aussi la
/// position de fin du nom de la balise.
fn tag_attrs(xml: &str, (s, e): (usize, usize)) -> Result<(usize, Vec<Attr>), Error> {
    let b = xml.as_bytes();
    let mut i = s + 1;
    while i < e && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let name_end = i;
    let mut out = Vec::new();
    loop {
        let lead = i;
        while i < e && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= e || b[i] == b'>' || b[i] == b'/' {
            break;
        }
        let start = i;
        while i < e && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let name = xml[start..i].to_string();
        while i < e && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= e || b[i] != b'=' {
            return Err(bad("attribut mal formé"));
        }
        i += 1;
        while i < e && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let q = *b.get(i).ok_or_else(|| bad("attribut mal formé"))?;
        if q != b'"' && q != b'\'' {
            return Err(bad("attribut mal formé"));
        }
        i += 1;
        while i < e && b[i] != q {
            i += 1;
        }
        if i >= e {
            return Err(bad("attribut mal formé"));
        }
        i += 1;
        out.push(Attr { name, lead, start, end: i });
    }
    Ok((name_end, out))
}

/// Applique des remplacements `(début, fin, texte)` disjoints.
fn splice(xml: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = xml.to_string();
    for (s, e, t) in edits {
        out.replace_range(s..e, &t);
    }
    out
}

fn apply_attr(xml: &str, name: &str, kind: Kind, value: Option<&Value>) -> Result<String, Error> {
    if matches!(kind, Kind::Unsupported) {
        return Err(bad("l'identifiant d'un livre ne se modifie pas"));
    }
    let doc = parse_doc(xml)?;
    let (name_end, attrs) = tag_attrs(xml, doc.root_tag)?;
    let text = match value {
        Some(v) => scalar(name, kind, v, None)?,
        None => None,
    };
    let existing: Vec<&Attr> = attrs.iter().filter(|a| a.name == name).collect();
    let mut edits = Vec::new();
    for (i, a) in existing.iter().enumerate() {
        match (&text, i) {
            (Some(t), 0) => edits.push((a.start, a.end, format!("{name}=\"{}\"", escape_attr(t)))),
            _ => edits.push((a.lead, a.end, String::new())),
        }
    }
    if existing.is_empty() {
        if let Some(t) = &text {
            let r = attr_rank(name);
            let at = attrs.iter().find(|a| attr_rank(&a.name) > r).map(|a| a.lead);
            let at = at.unwrap_or_else(|| attrs.last().map_or(name_end, |a| a.end));
            edits.push((at, at, format!(" {name}=\"{}\"", escape_attr(t))));
        }
    }
    Ok(splice(xml, edits))
}

fn apply_one(xml: &str, change: &Change) -> Result<String, Error> {
    let name = canonical(change.element.trim());
    if let Some(kind) = attribute_kind(name) {
        return apply_attr(xml, name, kind, change.value.as_ref());
    }
    let kind = element_kind(name).ok_or_else(|| bad(format!("élément « {name} » inconnu de ComicRack")))?;
    if matches!(kind, Kind::Unsupported) {
        return Err(bad(format!("l'élément « {name} » n'est pas modifiable par ComicRust")));
    }
    let doc = parse_doc(xml)?;
    let existing: Vec<&Child> = doc.children.iter().filter(|c| c.name == name).collect();

    // Emplacement d'un nouvel élément et espace (indentation) à reproduire.
    let r = rank(name);
    let next = doc.children.iter().find(|c| rank(&c.name) > r);
    let ws = match (existing.first().copied(), next, doc.children.last()) {
        (Some(c), _, _) | (None, Some(c), _) | (None, None, Some(c)) => &xml[c.lead..c.start],
        _ => "",
    };

    let new_elem = match (&change.value, kind) {
        (None, _) => None,
        (Some(Value::Pages(p)), Kind::Pages) => Some(render_pages(p, ws)?),
        (Some(v), Kind::Pages) => return Err(type_mismatch(name, v)),
        (Some(v), _) => {
            let current = existing.first().and_then(|c| c.inner).map(|(s, e)| &xml[s..e]);
            scalar(name, kind, v, current)?.map(|t| render_element(name, &t))
        }
    };

    let mut edits = Vec::new();
    for (i, c) in existing.iter().enumerate() {
        match (&new_elem, i) {
            (Some(n), 0) => edits.push((c.start, c.end, n.clone())),
            _ => edits.push((c.lead, c.end, String::new())),
        }
    }
    if existing.is_empty() {
        let Some(n) = new_elem else { return Ok(xml.to_string()) };
        if let Some(c) = next {
            edits.push((c.start, c.start, format!("{n}{ws}")));
        } else if let Some(c) = doc.children.last() {
            edits.push((c.end, c.end, format!("{ws}{n}")));
        } else if doc.self_closing {
            let (s, e) = doc.root_tag;
            let open = xml[s..e].trim_end_matches('>').trim_end_matches('/').trim_end();
            edits.push((s, e, format!("{open}>{n}</{}>", doc.root_name)));
        } else {
            edits.push((doc.root_close, doc.root_close, n));
        }
    }
    Ok(splice(xml, edits))
}

/// Modifie une fiche `<ComicBook>` : seuls les éléments de `changes` sont remplacés,
/// ajoutés (à leur place dans l'ordre de `XmlSerializer`) ou supprimés ; le reste du
/// texte est gardé tel quel. Le résultat est relu par [`LibraryBook::from_xml`] avant
/// d'être renvoyé.
pub fn patch_comic_book_xml(xml: &str, changes: &[Change]) -> Result<String, Error> {
    if changes.is_empty() {
        return Ok(xml.to_string());
    }
    let mut out = xml.to_string();
    for ch in changes {
        out = apply_one(&out, ch)?;
    }
    // Contrôle : document bien formé, racine unique, lisible comme une fiche.
    parse_doc(&out)?;
    LibraryBook::from_xml(&out).map_err(|e| bad(format!("fiche modifiée illisible : {e}")))?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// Écriture en base
// ---------------------------------------------------------------------------------------

/// Livre écrit : nouvelle fiche et nouveau compteur, et le livre relu.
#[derive(Debug, Clone)]
pub struct Written {
    /// Identifiant en minuscules.
    pub id: String,
    pub record: Record,
    pub book: LibraryBook,
    /// Nouveaux essais après une écriture concurrente (0 en temps normal).
    pub retries: usize,
}

/// Bilan de [`update_books`].
#[derive(Debug, Clone, Default)]
pub struct WriteReport {
    pub written: Vec<Written>,
    /// Livres déjà dans l'état demandé : rien d'écrit.
    pub unchanged: Vec<String>,
    /// Livres absents de la base (supprimés dans ComicRack) : ignorés.
    pub missing: Vec<String>,
    /// Livres modifiés par ailleurs à chaque essai : non écrits.
    pub conflicts: Vec<String>,
    /// Erreurs (valeur refusée, fiche illisible, erreur SQL) : identifiant, message.
    pub failed: Vec<(String, String)>,
}

impl WriteReport {
    /// Tout a été écrit (ou était déjà à jour).
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.conflicts.is_empty() && self.failed.is_empty()
    }

    /// Résumé pour l'utilisateur.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{} livre(s) mis à jour dans ComicRack", self.written.len())];
        if !self.unchanged.is_empty() {
            parts.push(format!("{} déjà à jour", self.unchanged.len()));
        }
        if !self.missing.is_empty() {
            parts.push(format!("{} absent(s) de la base", self.missing.len()));
        }
        if !self.conflicts.is_empty() {
            parts.push(format!("{} en conflit (modifié(s) en même temps dans ComicRack)", self.conflicts.len()));
        }
        if !self.failed.is_empty() {
            parts.push(format!("{} en erreur", self.failed.len()));
        }
        parts.join(", ")
    }
}

enum Outcome {
    Written(Record, usize),
    Unchanged,
    Missing,
    Conflict,
}

/// Écrit les modifications dans la base MySQL de ComicRack (voir l'en-tête du module
/// pour le protocole). Chaque livre est écrit dans sa propre transaction ; une erreur
/// sur un livre n'empêche pas les autres. `Err` seulement si la connexion échoue.
pub fn update_books(cfg: &DbConfig, edits: &[BookEdit]) -> Result<WriteReport, Error> {
    update_books_with(cfg, edits, &mut |_, _| {})
}

/// [`update_books`] avec un point d'arrêt appelé entre la lecture et l'écriture de
/// chaque essai (`identifiant`, `numéro d'essai`) : sert aux tests de concurrence.
fn update_books_with(
    cfg: &DbConfig,
    edits: &[BookEdit],
    before_write: &mut dyn FnMut(&str, usize),
) -> Result<WriteReport, Error> {
    let mut report = WriteReport::default();
    if edits.is_empty() {
        return Ok(report);
    }
    let mut conn = connect(cfg)?;
    for edit in edits {
        let id = edit.id.trim().to_ascii_lowercase();
        match write_one(&mut conn, &id, &edit.changes, before_write) {
            Ok(Outcome::Written(record, retries)) => match LibraryBook::from_xml(&record.data) {
                Ok(mut book) => {
                    if book.id.is_empty() {
                        book.id = id.clone();
                    }
                    report.written.push(Written { id, record, book, retries });
                }
                // Impossible en pratique : la fiche a été relue avant l'écriture.
                Err(e) => report.failed.push((id, e.to_string())),
            },
            Ok(Outcome::Unchanged) => report.unchanged.push(id),
            Ok(Outcome::Missing) => report.missing.push(id),
            Ok(Outcome::Conflict) => report.conflicts.push(id),
            Err(e) => report.failed.push((id, e.to_string())),
        }
    }
    Ok(report)
}

fn write_one(
    conn: &mut Conn,
    id: &str,
    changes: &[Change],
    before_write: &mut dyn FnMut(&str, usize),
) -> Result<Outcome, Error> {
    for attempt in 0..MAX_ATTEMPTS {
        let row: Option<(Option<i64>, Option<String>)> =
            conn.exec_first("SELECT update_counter, data FROM comics WHERE id = ?", (id,)).map_err(db_err)?;
        let Some((counter, data)) = row else { return Ok(Outcome::Missing) };
        let data = data.ok_or_else(|| bad("fiche vide en base"))?;
        let new = patch_comic_book_xml(&data, changes)?;
        if new == data {
            return Ok(Outcome::Unchanged);
        }
        before_write(id, attempt);
        let mut tx = conn.start_transaction(TxOpts::default()).map_err(db_err)?;
        let num = bump_update_counter(&mut tx)?;
        tx.exec_drop(
            "UPDATE comics SET data = ?, update_counter = ? WHERE id = ? AND update_counter <=> ?",
            (&new, num, id, counter),
        )
        .map_err(db_err)?;
        if tx.affected_rows() == 0 {
            // Écrit (ou supprimé) entre-temps : on annule, compteur compris, et on recommence.
            tx.rollback().map_err(db_err)?;
            continue;
        }
        tx.commit().map_err(db_err)?;
        return Ok(Outcome::Written(Record { data: new, counter: num }, attempt));
    }
    Ok(Outcome::Conflict)
}

/// `changes.update_counter + 1` sur la ligne `StorageGuid` (créée si besoin, comme
/// `WriteCounter`), dans la transaction : renvoie la nouvelle valeur.
fn bump_update_counter(tx: &mut mysql::Transaction<'_>) -> Result<i64, Error> {
    const BUMP: &str = "UPDATE changes SET update_counter = COALESCE(update_counter, 0) + 1 WHERE id = ?";
    tx.exec_drop(BUMP, (STORAGE_GUID,)).map_err(db_err)?;
    if tx.affected_rows() == 0 {
        tx.exec_drop("INSERT INTO changes VALUES (?, 0, 0)", (STORAGE_GUID,)).map_err(db_err)?;
        tx.exec_drop(BUMP, (STORAGE_GUID,)).map_err(db_err)?;
    }
    let n: Option<Option<i64>> =
        tx.exec_first("SELECT update_counter FROM changes WHERE id = ?", (STORAGE_GUID,)).map_err(db_err)?;
    n.flatten().ok_or_else(|| Error::Database("compteur de la table « changes » illisible".into()))
}

impl Watcher {
    /// Livres écrits par ce ComicRust (`id`, nouveau `comics.update_counter`) : ils ne
    /// seront pas renvoyés comme modifications extérieures (une réécriture ultérieure par
    /// ComicRack, avec un autre compteur, le sera).
    pub fn note_written<'a>(&mut self, written: impl IntoIterator<Item = (&'a str, i64)>) {
        for (id, counter) in written {
            self.rows.insert(id.to_ascii_lowercase(), counter);
        }
    }

    /// [`note_written`](Self::note_written) pour tout un bilan d'écriture.
    pub fn note_report(&mut self, report: &WriteReport) {
        self.note_written(report.written.iter().map(|w| (w.id.as_str(), w.record.counter)));
    }
}

impl ComicDatabase {
    /// Reporte dans la base en mémoire les livres écrits (sans tout recharger). Renvoie
    /// le nombre de livres remplacés.
    pub fn apply_written(&mut self, report: &WriteReport) -> usize {
        let mut n = 0;
        let mut seen = HashSet::new();
        for w in &report.written {
            self.rows.insert(w.id.clone(), w.record.counter);
            if seen.insert(w.id.clone()) {
                if let Some(b) = self.get_mut(&w.id) {
                    *b = w.book.clone();
                    n += 1;
                }
            }
        }
        n
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
