//! Catalogue des champs modifiables d'une fiche, et validation des valeurs.
//!
//! Le **nom public** d'un champ est celui de l'élément XML de ComicRack (`Series`,
//! `Number`, `BookPrice`…) : c'est aussi le nom de la propriété dans les scripts de
//! ComicRack (sauf `Released` = `ReleasedTime`, accepté aussi). Les noms sont reconnus
//! sans casse, ainsi que le nom de colonne de ComicRust (`book_price`).
//!
//! Types, bornes et valeurs par défaut viennent de `ComicInfo.cs` / `ComicBook.cs`
//! (`[DefaultValue]`) : une valeur égale à la valeur par défaut **vide** le champ, comme
//! le fait `XmlSerializer` dans ComicRack (le champ disparaît de la fiche).

use chrono::{Datelike, NaiveDate, TimeZone};

use crate::library::LibraryBook;

/// Nature d'un champ (détermine la validation et la valeur stockée).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldKind {
    /// Texte sur une ligne (espaces de début et de fin retirés).
    Text,
    /// Texte long, retours à la ligne permis (résumé, notes…), gardé tel quel.
    LongText,
    /// Valeurs multiples séparées par des virgules (auteurs, genres, tags…) : chaque
    /// valeur est nettoyée, les vides et les doublons (sans casse) retirés, le tout
    /// réécrit avec « , » comme ComicRack.
    List,
    /// Entier entre `min` et `max` ; `default` (souvent -1) vide le champ.
    Int { min: i32, max: i32, default: i32 },
    /// Note de 0 à 5 (réel ; 0 = pas de note, vide le champ).
    Rating,
    /// Prix, réel positif ou nul (champ vide = prix inconnu, -1 dans ComicRack).
    Price,
    /// Oui / Non / Inconnu (`YesNo` de ComicRack ; Inconnu vide le champ).
    YesNo,
    /// Manga : Inconnu / Non / Oui / Oui, de droite à gauche (`MangaYesNo`).
    Manga,
    /// Booléen (case à cocher) ; `default` vide le champ.
    Bool { default: bool },
    /// Date seule (enregistrée à minuit, heure locale, au format de .NET).
    Date,
    /// Code de langue ISO (`fr`, `en`, `pt-BR`…).
    Language,
    /// Valeurs personnalisées brutes (`CustomValuesStore`, `,clé=valeur,…`) ; préférer
    /// [`super::FieldEdit::custom_value`].
    CustomValues,
}

impl FieldKind {
    /// Libellé du type, en français.
    pub fn label(&self) -> &'static str {
        match self {
            FieldKind::Text => "texte",
            FieldKind::LongText => "texte long",
            FieldKind::List => "liste (valeurs séparées par des virgules)",
            FieldKind::Int { .. } => "entier",
            FieldKind::Rating => "note de 0 à 5",
            FieldKind::Price => "prix",
            FieldKind::YesNo => "oui / non / inconnu",
            FieldKind::Manga => "manga (oui / non / droite à gauche / inconnu)",
            FieldKind::Bool { .. } => "oui / non",
            FieldKind::Date => "date",
            FieldKind::Language => "code de langue ISO",
            FieldKind::CustomValues => "valeurs personnalisées",
        }
    }
}

/// Valeur fournie pour une modification (scripts, interface). Les conversions sont
/// tolérantes : un texte est accepté pour tous les types (`"4,5"` pour une note,
/// `"oui"` pour un booléen, `"2024-05-17"` ou `"17/05/2024"` pour une date…).
#[derive(Debug, Clone, PartialEq)]
pub enum EditValue {
    Text(String),
    Int(i64),
    Real(f64),
    Bool(bool),
    /// Liste de valeurs (champs [`FieldKind::List`]).
    List(Vec<String>),
}

impl From<&str> for EditValue {
    fn from(s: &str) -> Self {
        EditValue::Text(s.to_string())
    }
}
impl From<String> for EditValue {
    fn from(s: String) -> Self {
        EditValue::Text(s)
    }
}
impl From<i64> for EditValue {
    fn from(i: i64) -> Self {
        EditValue::Int(i)
    }
}
impl From<i32> for EditValue {
    fn from(i: i32) -> Self {
        EditValue::Int(i.into())
    }
}
impl From<f64> for EditValue {
    fn from(f: f64) -> Self {
        EditValue::Real(f)
    }
}
impl From<f32> for EditValue {
    fn from(f: f32) -> Self {
        EditValue::Real(f.into())
    }
}
impl From<bool> for EditValue {
    fn from(b: bool) -> Self {
        EditValue::Bool(b)
    }
}
impl From<Vec<String>> for EditValue {
    fn from(l: Vec<String>) -> Self {
        EditValue::List(l)
    }
}

/// Accès au champ correspondant de [`LibraryBook`].
#[derive(Clone, Copy)]
pub(crate) enum Slot {
    Text(fn(&LibraryBook) -> &Option<String>, fn(&mut LibraryBook) -> &mut Option<String>),
    I32(fn(&LibraryBook) -> &Option<i32>, fn(&mut LibraryBook) -> &mut Option<i32>),
    F32(fn(&LibraryBook) -> &Option<f32>, fn(&mut LibraryBook) -> &mut Option<f32>),
    Bool(fn(&LibraryBook) -> &Option<bool>, fn(&mut LibraryBook) -> &mut Option<bool>),
}

/// Champ modifiable.
#[derive(Clone, Copy)]
pub struct FieldDef {
    /// Nom public stable : élément XML de ComicRack.
    pub name: &'static str,
    /// Libellé français (interface, messages d'erreur).
    pub label: &'static str,
    pub kind: FieldKind,
    /// Colonne de la base ComicRust (= nom du champ de [`LibraryBook`]).
    pub column: &'static str,
    /// Champ du ComicInfo.xml : le modifier marque la fiche « à écrire dans le fichier »
    /// (`ComicInfoIsDirty`), comme ComicRack.
    pub in_comic_info: bool,
    pub(crate) slot: Slot,
}

impl std::fmt::Debug for FieldDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldDef")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("column", &self.column)
            .field("in_comic_info", &self.in_comic_info)
            .finish()
    }
}

macro_rules! slot {
    (Text, $f:ident) => {
        Slot::Text(|b| &b.$f, |b| &mut b.$f)
    };
    (I32, $f:ident) => {
        Slot::I32(|b| &b.$f, |b| &mut b.$f)
    };
    (F32, $f:ident) => {
        Slot::F32(|b| &b.$f, |b| &mut b.$f)
    };
    (Bool, $f:ident) => {
        Slot::Bool(|b| &b.$f, |b| &mut b.$f)
    };
}

macro_rules! fields {
    ($($name:literal, $label:literal, $kind:expr, $slot:ident $col:ident, $ci:literal;)*) => {
        /// Tous les champs modifiables, dans l'ordre de `ComicInfo.cs` puis `ComicBook.cs`.
        pub static FIELDS: &[FieldDef] = &[$(
            FieldDef {
                name: $name,
                label: $label,
                kind: $kind,
                column: stringify!($col),
                in_comic_info: $ci,
                slot: slot!($slot, $col),
            },
        )*];
    };
}

const fn int(min: i32, max: i32) -> FieldKind {
    FieldKind::Int { min, max, default: -1 }
}

fields! {
    // ComicInfo.cs (champs écrits dans le ComicInfo.xml)
    "Title", "Titre", FieldKind::Text, Text title, true;
    "Series", "Série", FieldKind::Text, Text series, true;
    "Number", "Numéro", FieldKind::Text, Text number, true;
    "Count", "Nombre de numéros", int(0, 1_000_000), I32 count, true;
    "Volume", "Volume", int(0, 1_000_000), I32 volume, true;
    "AlternateSeries", "Série alternative", FieldKind::Text, Text alternate_series, true;
    "AlternateNumber", "Numéro alternatif", FieldKind::Text, Text alternate_number, true;
    "StoryArc", "Arc narratif", FieldKind::Text, Text story_arc, true;
    "SeriesGroup", "Groupe de séries", FieldKind::Text, Text series_group, true;
    "AlternateCount", "Nombre de numéros (série alternative)", int(0, 1_000_000), I32 alternate_count, true;
    "Summary", "Résumé", FieldKind::LongText, Text summary, true;
    "Notes", "Notes", FieldKind::LongText, Text notes, true;
    "Review", "Critique", FieldKind::LongText, Text review, true;
    "Year", "Année", int(0, 9999), I32 year, true;
    "Month", "Mois", int(1, 12), I32 month, true;
    "Day", "Jour", int(1, 31), I32 day, true;
    "Writer", "Scénariste", FieldKind::List, Text writer, true;
    "Penciller", "Dessinateur", FieldKind::List, Text penciller, true;
    "Inker", "Encreur", FieldKind::List, Text inker, true;
    "Colorist", "Coloriste", FieldKind::List, Text colorist, true;
    "Letterer", "Lettreur", FieldKind::List, Text letterer, true;
    "CoverArtist", "Dessinateur de couverture", FieldKind::List, Text cover_artist, true;
    "Editor", "Directeur de collection", FieldKind::List, Text editor, true;
    "Translator", "Traducteur", FieldKind::List, Text translator, true;
    "Publisher", "Éditeur", FieldKind::Text, Text publisher, true;
    "Imprint", "Label", FieldKind::Text, Text imprint, true;
    "Genre", "Genre", FieldKind::List, Text genre, true;
    "Web", "Site web", FieldKind::Text, Text web, true;
    "LanguageISO", "Langue", FieldKind::Language, Text language_iso, true;
    "Format", "Format", FieldKind::Text, Text format, true;
    "AgeRating", "Public", FieldKind::Text, Text age_rating, true;
    "BlackAndWhite", "Noir et blanc", FieldKind::YesNo, Text black_and_white, true;
    "Manga", "Manga", FieldKind::Manga, Text manga, true;
    "PreferredFrontCover", "Couverture préférée", FieldKind::Int { min: 0, max: 1_000_000, default: 0 }, I32 preferred_front_cover, true;
    "Characters", "Personnages", FieldKind::List, Text characters, true;
    "Teams", "Équipes", FieldKind::List, Text teams, true;
    "MainCharacterOrTeam", "Personnage ou équipe principal", FieldKind::Text, Text main_character_or_team, true;
    "Locations", "Lieux", FieldKind::List, Text locations, true;
    "CommunityRating", "Note de la communauté", FieldKind::Rating, F32 community_rating, true;
    "ScanInformation", "Informations de numérisation", FieldKind::LongText, Text scan_information, true;
    "Tags", "Étiquettes", FieldKind::List, Text tags, true;
    // ComicBook.cs (propres à la bibliothèque)
    "Released", "Date de parution", FieldKind::Date, Text released, false;
    "Rating", "Note", FieldKind::Rating, F32 rating, false;
    "EnableProposed", "Utiliser les valeurs proposées", FieldKind::Bool { default: true }, Bool enable_proposed, false;
    "SeriesComplete", "Série complète", FieldKind::YesNo, Text series_complete, false;
    "Checked", "Coché", FieldKind::Bool { default: true }, Bool checked, false;
    "BookPrice", "Prix", FieldKind::Price, F32 book_price, false;
    "BookAge", "Âge du livre", FieldKind::Text, Text book_age, false;
    "BookCondition", "État", FieldKind::Text, Text book_condition, false;
    "BookStore", "Magasin", FieldKind::Text, Text book_store, false;
    "BookOwner", "Propriétaire", FieldKind::Text, Text book_owner, false;
    "BookCollectionStatus", "Statut dans la collection", FieldKind::Text, Text book_collection_status, false;
    "BookNotes", "Notes sur le livre", FieldKind::LongText, Text book_notes, false;
    "BookLocation", "Emplacement", FieldKind::Text, Text book_location, false;
    "ISBN", "ISBN", FieldKind::Text, Text isbn, false;
    "CustomValuesStore", "Valeurs personnalisées", FieldKind::CustomValues, Text custom_values_store, false;
}

/// Éléments de la fiche qui **ne** se modifient **pas** par cette API, avec la raison.
pub static READ_ONLY_FIELDS: &[(&str, &str)] = &[
    ("Id", "identifiant du livre"),
    ("File", "passer par edit::rename_or_move"),
    ("FileSize", "décrit le fichier"),
    ("FileModifiedTime", "décrit le fichier"),
    ("FileCreationTime", "décrit le fichier"),
    ("PageCount", "nombre réel de pages du fichier"),
    ("Pages", "liste des pages du fichier"),
    ("Added", "date d'ajout à la bibliothèque"),
    ("Opened", "lecture : Store::set_progress / BookEdit::opened"),
    ("OpenCount", "compteur de lectures"),
    ("CurrentPage", "lecture : Store::set_progress / BookEdit::progress"),
    ("LastPageRead", "lecture : Store::set_progress / BookEdit::progress"),
    ("NewPages", "compteur interne"),
    ("Missing", "état du fichier, tenu par l'analyse des dossiers"),
    ("ComicInfoIsDirty", "tenu par ComicRust (modification / écriture du fichier)"),
    ("ComicBookIsDirty", "tenu par ComicRack"),
    ("CustomThumbnailKey", "vignette choisie dans ComicRack"),
    ("LastOpenedFromListId", "lecture"),
];

/// Valeurs usuelles de `AgeRating` (schéma ComicInfo 2.x ; ComicRack accepte tout texte).
pub const AGE_RATINGS: &[&str] = &[
    "Unknown",
    "Adults Only 18+",
    "Early Childhood",
    "Everyone",
    "Everyone 10+",
    "G",
    "Kids to Adults",
    "M",
    "MA15+",
    "Mature 17+",
    "PG",
    "R18+",
    "Rating Pending",
    "Teen",
    "X18+",
];

/// Champ d'après son nom : élément ComicRack, propriété .NET (`ReleasedTime`) ou colonne
/// ComicRust (`book_price`), sans casse.
pub fn field(name: &str) -> Option<&'static FieldDef> {
    let n = name.trim();
    let n = if n.eq_ignore_ascii_case("ReleasedTime") { "Released" } else { n };
    FIELDS.iter().find(|f| f.name.eq_ignore_ascii_case(n) || f.column.eq_ignore_ascii_case(n))
}

impl FieldDef {
    /// Valeur actuelle du champ dans `b` (`None` : vide). Les listes et dates sont rendues
    /// en texte, tels que stockés.
    pub fn get(&self, b: &LibraryBook) -> Option<EditValue> {
        match self.slot {
            Slot::Text(g, _) => g(b).clone().map(EditValue::Text),
            Slot::I32(g, _) => g(b).map(|i| EditValue::Int(i.into())),
            Slot::F32(g, _) => g(b).map(|f| EditValue::Real(f.into())),
            Slot::Bool(g, _) => g(b).map(EditValue::Bool),
        }
    }

    /// Valeur actuelle en texte (vide si le champ est vide), pour un aperçu.
    pub fn text(&self, b: &LibraryBook) -> String {
        match self.get(b) {
            None => String::new(),
            Some(EditValue::Text(s)) => s,
            Some(EditValue::Int(i)) => i.to_string(),
            Some(EditValue::Real(f)) => (f as f32).to_string(),
            Some(EditValue::Bool(b)) => b.to_string(),
            Some(EditValue::List(l)) => l.join(", "),
        }
    }

    /// Valeurs proposées pour ce champ (vide : texte libre).
    pub fn suggestions(&self) -> &'static [&'static str] {
        match (self.kind, self.name) {
            (_, "AgeRating") => AGE_RATINGS,
            (FieldKind::YesNo, _) => &["Unknown", "No", "Yes"],
            (FieldKind::Manga, _) => &["Unknown", "No", "Yes", "YesAndRightToLeft"],
            _ => &[],
        }
    }

    /// Valide et normalise `value` (sans livre) : `Ok(None)` = vider le champ.
    pub fn normalize(&self, value: Option<&EditValue>) -> Result<Option<Norm>, String> {
        let Some(v) = value else { return Ok(None) };
        normalize(self.kind, v)
    }

    /// Les deux fiches ont-elles la même valeur pour ce champ ? (Dates : même jour.)
    pub(crate) fn same(&self, a: &LibraryBook, b: &LibraryBook) -> bool {
        match self.slot {
            Slot::Text(g, _) if self.kind == FieldKind::Date => {
                let day = |s: &Option<String>| s.as_deref().map(|t| t.get(..10).unwrap_or(t).to_string());
                day(g(a)) == day(g(b))
            }
            Slot::Text(g, _) => g(a) == g(b),
            Slot::I32(g, _) => g(a) == g(b),
            Slot::F32(g, _) => g(a) == g(b),
            Slot::Bool(g, _) => g(a) == g(b),
        }
    }

    /// Écrit la valeur normalisée dans `b`. Renvoie `true` si la valeur a changé.
    pub(crate) fn store(&self, b: &mut LibraryBook, v: Option<&Norm>) -> bool {
        let before = b.clone();
        match (self.slot, v) {
            (Slot::Text(_, s), None) => *s(b) = None,
            (Slot::Text(_, s), Some(Norm::Text(t))) => *s(b) = Some(t.clone()),
            (Slot::Text(_, s), Some(Norm::Date(d))) => *s(b) = Some(date_text(*d)),
            (Slot::I32(_, s), v) => *s(b) = v.and_then(Norm::as_int),
            (Slot::F32(_, s), v) => *s(b) = v.and_then(Norm::as_real),
            (Slot::Bool(_, s), v) => *s(b) = v.and_then(Norm::as_bool),
            // Impossible : `normalize` rend le type du champ.
            (Slot::Text(_, _), Some(_)) => return false,
        }
        if self.same(&before, b) {
            *b = before;
            return false;
        }
        true
    }
}

/// Valeur validée et normalisée, prête à être stockée.
#[derive(Debug, Clone, PartialEq)]
pub enum Norm {
    Text(String),
    Int(i32),
    Real(f32),
    Bool(bool),
    Date(NaiveDate),
}

impl Norm {
    fn as_int(&self) -> Option<i32> {
        match self {
            Norm::Int(i) => Some(*i),
            _ => None,
        }
    }
    fn as_real(&self) -> Option<f32> {
        match self {
            Norm::Real(f) => Some(*f),
            _ => None,
        }
    }
    fn as_bool(&self) -> Option<bool> {
        match self {
            Norm::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Date au format des fiches ComicRack (`XmlConvert`, aller-retour) : minuit, heure locale.
pub(crate) fn date_text(d: NaiveDate) -> String {
    let midnight = d.and_hms_opt(0, 0, 0).unwrap_or_default();
    let local = chrono::Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.fixed_offset())
        .unwrap_or_else(|| chrono::Utc.from_utc_datetime(&midnight).fixed_offset());
    crate::db::write::net_datetime(&local)
}

fn as_text(v: &EditValue) -> String {
    match v {
        EditValue::Text(s) => s.clone(),
        EditValue::Int(i) => i.to_string(),
        EditValue::Real(f) => f.to_string(),
        EditValue::Bool(b) => b.to_string(),
        EditValue::List(l) => l.join(", "),
    }
}

/// Caractères interdits : hors XML 1.0 (ComicRack ne relirait pas la fiche).
fn check_chars(s: &str) -> Result<(), String> {
    match s.chars().find(|&c| !crate::lists_edit::xml::is_xml_char(c)) {
        Some(c) => Err(format!("caractère interdit (U+{:04X})", c as u32)),
        None => Ok(()),
    }
}

fn one_line(s: &str) -> Result<String, String> {
    check_chars(s)?;
    let t = s.trim();
    if t.contains(['\n', '\r']) {
        return Err("retour à la ligne interdit dans ce champ".into());
    }
    Ok(t.to_string())
}

fn parse_int(v: &EditValue) -> Result<Option<i64>, String> {
    match v {
        EditValue::Int(i) => Ok(Some(*i)),
        EditValue::Real(f) if f.fract() == 0.0 && f.is_finite() => Ok(Some(*f as i64)),
        EditValue::Real(f) => Err(format!("{f} n'est pas un entier")),
        EditValue::Text(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Ok(None);
            }
            t.parse().map(Some).map_err(|_| format!("« {t} » n'est pas un entier"))
        }
        EditValue::Bool(_) | EditValue::List(_) => Err("un entier est attendu".into()),
    }
}

fn parse_real(v: &EditValue) -> Result<Option<f64>, String> {
    let f = match v {
        EditValue::Int(i) => *i as f64,
        EditValue::Real(f) => *f,
        EditValue::Text(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Ok(None);
            }
            t.replace(',', ".").parse().map_err(|_| format!("« {t} » n'est pas un nombre"))?
        }
        EditValue::Bool(_) | EditValue::List(_) => return Err("un nombre est attendu".into()),
    };
    if !f.is_finite() {
        return Err(format!("{f} n'est pas un nombre valide"));
    }
    Ok(Some(f))
}

fn parse_bool(v: &EditValue) -> Result<Option<bool>, String> {
    match v {
        EditValue::Bool(b) => Ok(Some(*b)),
        EditValue::Int(0) => Ok(Some(false)),
        EditValue::Int(1) => Ok(Some(true)),
        EditValue::Text(s) => match s.trim().to_lowercase().as_str() {
            "" => Ok(None),
            "true" | "vrai" | "oui" | "yes" | "1" => Ok(Some(true)),
            "false" | "faux" | "non" | "no" | "0" => Ok(Some(false)),
            t => Err(format!("« {t} » : oui ou non attendu")),
        },
        _ => Err("oui ou non attendu".into()),
    }
}

fn parse_date(v: &EditValue) -> Result<Option<NaiveDate>, String> {
    let EditValue::Text(s) = v else { return Err("une date est attendue (AAAA-MM-JJ)".into()) };
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(t) {
        return Ok(Some(d.date_naive()));
    }
    if let Ok(d) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(Some(d.date()));
    }
    for f in ["%Y-%m-%d", "%d/%m/%Y", "%d.%m.%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, f) {
            if !(1..=9999).contains(&d.year()) {
                break;
            }
            return Ok(Some(d));
        }
    }
    Err(format!("« {t} » n'est pas une date (AAAA-MM-JJ ou JJ/MM/AAAA)"))
}

/// Valeurs personnalisées : `(clé, valeur)` dans l'ordre du texte (décodage de
/// `ValuesStore.GetValues` : segments séparés par `,`, `clé=valeur`, `&#61;` = `=` et
/// `&#44;` = `,` ; les segments mal formés sont ignorés comme dans ComicRack).
pub fn custom_values(store: Option<&str>) -> Vec<(String, String)> {
    let dec = |s: &str| s.replace("&#61;", "=").replace("&#44;", ",");
    store
        .unwrap_or("")
        .split(',')
        .filter_map(|seg| {
            let mut it = seg.split('=');
            let (k, v) = (it.next()?, it.next()?);
            it.next().is_none().then(|| (dec(k), dec(v)))
        })
        .collect()
}

/// Valeur personnalisée `key` (sans casse, comme `ComicBook.GetCustomValue`).
pub fn custom_value(store: Option<&str>, key: &str) -> Option<String> {
    custom_values(store).into_iter().find(|(k, _)| k.to_lowercase() == key.to_lowercase()).map(|(_, v)| v)
}

/// `ComicBook.SetCustomValueInStore` : ajoute, remplace (`Some`, non vide) ou supprime
/// (`None` ou vide) la clé, puis réécrit le texte comme `ValuesStore.ToString` (clés
/// triées, une virgule **devant** chaque paire, `=` et `,` encodés). Vide → `None`.
pub fn set_custom_value(store: Option<&str>, key: &str, value: Option<&str>) -> Option<String> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (k, v) in custom_values(store) {
        match pairs.iter_mut().find(|(x, _)| x.to_lowercase() == k.to_lowercase()) {
            Some(p) => p.1 = v,
            None => pairs.push((k, v)),
        }
    }
    if !key.is_empty() {
        pairs.retain(|(k, _)| k.to_lowercase() != key.to_lowercase());
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            pairs.push((key.to_string(), v.to_string()));
        }
    }
    // Ordre proche de `OrderBy(s => s)` (comparaison culturelle) : sans casse d'abord.
    pairs.sort_by(|a, b| (a.0.to_lowercase(), &a.0).cmp(&(b.0.to_lowercase(), &b.0)));
    let enc = |s: &str| s.replace('=', "&#61;").replace(',', "&#44;");
    let out: String = pairs.iter().map(|(k, v)| format!(",{}={}", enc(k), enc(v))).collect();
    (!out.is_empty()).then_some(out)
}

/// Contrôle d'un texte `CustomValuesStore` saisi tel quel.
fn check_custom_store(s: &str) -> Result<(), String> {
    let mut keys: Vec<String> = Vec::new();
    for seg in s.split(',').filter(|s| !s.is_empty()) {
        if seg.split('=').count() != 2 {
            return Err(format!("segment « {seg} » : « clé=valeur » attendu (« = » et « , » encodés &#61; et &#44;)"));
        }
        let k = seg.split('=').next().unwrap_or("").to_lowercase();
        if keys.contains(&k) {
            // `new ValuesStore(…)` utilise `Dictionary.Add` : une clé en double lève une
            // exception dans ComicRack.
            return Err(format!("clé « {k} » en double"));
        }
        keys.push(k);
    }
    Ok(())
}

fn normalize(kind: FieldKind, v: &EditValue) -> Result<Option<Norm>, String> {
    let some_text = |t: String| (!t.is_empty()).then_some(Norm::Text(t));
    Ok(match kind {
        FieldKind::Text => some_text(one_line(&as_text(v))?),
        FieldKind::LongText => {
            let s = as_text(v);
            check_chars(&s)?;
            (!s.trim().is_empty()).then_some(Norm::Text(s))
        }
        FieldKind::List => {
            let items: Vec<String> = match v {
                EditValue::List(l) => l.clone(),
                v => as_text(v).split(',').map(str::to_string).collect(),
            };
            let mut out: Vec<String> = Vec::new();
            for it in items {
                if it.contains(',') {
                    return Err(format!("« {it} » : une valeur de liste ne peut pas contenir de virgule"));
                }
                let t = one_line(&it)?;
                if !t.is_empty() && !out.iter().any(|o| o.to_lowercase() == t.to_lowercase()) {
                    out.push(t);
                }
            }
            some_text(out.join(", "))
        }
        FieldKind::Int { min, max, default } => match parse_int(v)? {
            None => None,
            Some(i) if i == default as i64 => None,
            Some(i) if i < min as i64 || i > max as i64 => {
                return Err(format!("{i} hors limites (de {min} à {max})"));
            }
            Some(i) => Some(Norm::Int(i as i32)),
        },
        FieldKind::Rating => match parse_real(v)? {
            None => None,
            Some(f) if !(0.0..=5.0).contains(&f) => return Err(format!("{f} hors limites (de 0 à 5)")),
            Some(0.0) => None,
            Some(f) => Some(Norm::Real(f as f32)),
        },
        FieldKind::Price => match parse_real(v)? {
            None => None,
            Some(f) if f < 0.0 => return Err(format!("{f} : le prix ne peut pas être négatif (vider le champ)")),
            Some(f) if f > 1e9 => return Err(format!("{f} : prix trop grand")),
            Some(f) => Some(Norm::Real(f as f32)),
        },
        FieldKind::YesNo | FieldKind::Manga => {
            let t = match v {
                EditValue::Bool(true) => "yes".to_string(),
                EditValue::Bool(false) => "no".to_string(),
                v => as_text(v).trim().to_lowercase(),
            };
            let manga = kind == FieldKind::Manga;
            match t.as_str() {
                "" | "unknown" | "inconnu" | "?" => None,
                "yes" | "oui" | "true" | "vrai" => Some(Norm::Text("Yes".into())),
                "no" | "non" | "false" | "faux" => Some(Norm::Text("No".into())),
                "yesandrighttoleft" | "rtl" | "droite à gauche" | "oui, de droite à gauche" if manga => {
                    Some(Norm::Text("YesAndRightToLeft".into()))
                }
                _ => {
                    let allowed = if manga { "Yes, No, YesAndRightToLeft, Unknown" } else { "Yes, No, Unknown" };
                    return Err(format!("« {t} » inconnu (permis : {allowed}, ou oui / non)"));
                }
            }
        }
        FieldKind::Bool { default } => match parse_bool(v)? {
            None => None,
            Some(b) if b == default => None,
            Some(b) => Some(Norm::Bool(b)),
        },
        FieldKind::Date => parse_date(v)?.map(Norm::Date),
        FieldKind::Language => {
            let t = one_line(&as_text(v))?.replace('_', "-");
            if t.is_empty() {
                return Ok(None);
            }
            let mut parts = t.split('-');
            let first = parts.next().unwrap_or("");
            let ok_first = (2..=3).contains(&first.len()) && first.chars().all(|c| c.is_ascii_alphabetic());
            let ok_rest = parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()));
            if !ok_first || !ok_rest {
                return Err(format!("« {t} » n'est pas un code de langue ISO (fr, en, pt-BR…)"));
            }
            let rest = &t[first.len()..];
            Some(Norm::Text(format!("{}{rest}", first.to_ascii_lowercase())))
        }
        FieldKind::CustomValues => {
            let s = as_text(v);
            check_chars(&s)?;
            check_custom_store(&s)?;
            (!s.is_empty()).then_some(Norm::Text(s))
        }
    })
}
