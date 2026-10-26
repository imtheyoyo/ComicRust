//! Formulaires et listes de choix des scripts (v0.10.1).
//!
//! Le script décrit une fenêtre (`comicrust.form`, `comicrust.choose`) par du JSON ; ce
//! module la **vérifie** (types de contrôles connus, clés uniques, dépendances
//! `enabled_if` valides, tailles bornées, images décodées du base64) et fournit la logique
//! indépendante du dessin : valeurs initiales, conversion des réponses, validation
//! (champ obligatoire, nombre borné). Le dessin est dans l'interface (egui).
//!
//! Inspiré des fenêtres WinForms des scripts ComicRack célèbres : options à onglets de
//! Bedetheque Scraper 2 (cases, boutons radio, liste à cocher des champs), fenêtre de
//! choix de l'album avec couverture, configuration de Library Organizer (profils, onglets,
//! modèles avec insertion de champs, règles en tableau, listes d'exceptions, dossiers),
//! fenêtre des doublons (deux couvertures côte à côte et boutons).

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{Map, Value};

/// Taille maximale d'une image (octets décodés).
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
/// Taille maximale cumulée des images d'une fenêtre.
pub const MAX_IMAGES_TOTAL: usize = 12 * 1024 * 1024;
/// Nombre maximal de contrôles d'un formulaire (imbriqués compris).
pub const MAX_CONTROLS: usize = 2_000;
/// Profondeur maximale d'imbrication (groupes, onglets, colonnes).
pub const MAX_DEPTH: usize = 8;
/// Nombre maximal de lignes d'une liste de choix ou d'un tableau.
pub const MAX_ROWS: usize = 10_000;
/// Nombre maximal de colonnes d'une liste de choix ou d'un tableau.
pub const MAX_COLUMNS: usize = 32;
/// Nombre maximal de boutons en bas d'un formulaire.
pub const MAX_BUTTONS: usize = 8;
/// Nombre maximal de choix d'une liste déroulante, de cases…
pub const MAX_CHOICES: usize = 10_000;
/// Longueur maximale d'un texte (libellé, valeur par défaut…).
pub const MAX_TEXT: usize = 1024 * 1024;

/// Élément d'une liste de choix : valeur rendue au script et libellé affiché.
#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceItem {
    pub value: Value,
    pub label: String,
}

/// Type d'une colonne de tableau éditable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Number,
    Checkbox,
    Choice,
}

/// Colonne d'un tableau éditable ([`ControlKind::Table`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TableColumn {
    pub title: String,
    pub kind: ColumnKind,
    /// Choix (colonne [`ColumnKind::Choice`]).
    pub choices: Vec<ChoiceItem>,
    pub width: Option<f32>,
}

/// Apparence d'un texte fixe ([`ControlKind::Label`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelStyle {
    Normal,
    Bold,
    Small,
    Weak,
    Heading,
    Warning,
    Error,
}

/// Contrôle d'un formulaire.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlKind {
    /// Texte (`rows` > 1 : plusieurs lignes ; `secret` : saisie masquée, jamais pré-remplie ;
    /// `inserts` : menu « Insérer » de morceaux de texte, ex. `{series}` dans un modèle).
    Text {
        default: String,
        placeholder: String,
        required: bool,
        rows: u32,
        secret: bool,
        inserts: Vec<ChoiceItem>,
    },
    Checkbox {
        default: bool,
    },
    /// Nombre (`decimals` = 0 : entier).
    Number {
        default: f64,
        min: Option<f64>,
        max: Option<f64>,
        step: f64,
        decimals: u32,
    },
    /// Liste déroulante (`editable` : saisie libre possible).
    Choice {
        choices: Vec<ChoiceItem>,
        default: Value,
        editable: bool,
        required: bool,
    },
    Radio {
        choices: Vec<ChoiceItem>,
        default: Value,
        horizontal: bool,
    },
    /// Liste à cocher (CheckedListBox de ComicRack) : valeur = liste des valeurs cochées.
    MultiChoice {
        choices: Vec<ChoiceItem>,
        default: Vec<Value>,
        height: Option<f32>,
        required: bool,
    },
    /// Liste éditable de textes (exceptions, séries…), ordonnée.
    ListEdit {
        default: Vec<String>,
        /// Suggestions proposées à l'ajout.
        choices: Vec<String>,
        unique: bool,
        required: bool,
        height: Option<f32>,
    },
    /// Tableau éditable (règles, substitutions…) : valeur = liste de lignes (listes).
    Table {
        columns: Vec<TableColumn>,
        default: Vec<Vec<Value>>,
        height: Option<f32>,
    },
    /// Chemin de dossier ou de fichier avec bouton « Parcourir… ».
    Path {
        default: String,
        folder: bool,
        save: bool,
        filters: Vec<(String, Vec<String>)>,
        required: bool,
    },
    Label {
        text: String,
        style: LabelStyle,
    },
    Separator,
    /// Image fixe (couverture…), octets JPEG/PNG/WebP/GIF/BMP.
    Image {
        data: Arc<Vec<u8>>,
        caption: String,
        max_width: f32,
        max_height: f32,
    },
    Group {
        title: String,
        controls: Vec<Control>,
    },
    /// Colonnes côte à côte.
    Columns {
        columns: Vec<Vec<Control>>,
    },
    Tabs {
        tabs: Vec<(String, Vec<Control>)>,
    },
    /// Bouton dans le formulaire : ferme le formulaire et rend sa clé au script (qui peut
    /// rouvrir le formulaire avec d'autres valeurs : formulaires dynamiques simples).
    Button {
        validate: bool,
    },
}

/// Condition d'activation (`enabled_if`) : `"clé"` (valeur vraie), `"!clé"`,
/// `"clé=valeur"`, `"clé!=valeur"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub key: String,
    pub negate: bool,
    pub equals: Option<String>,
}

impl Condition {
    pub fn parse(text: &str) -> Result<Condition, String> {
        let t = text.trim();
        let (key, negate, equals) = if let Some((k, v)) = t.split_once("!=") {
            (k.trim(), true, Some(v.trim().to_string()))
        } else if let Some((k, v)) = t.split_once('=') {
            (k.trim(), false, Some(v.trim().to_string()))
        } else if let Some(k) = t.strip_prefix('!') {
            (k.trim(), true, None)
        } else {
            (t, false, None)
        };
        if key.is_empty() {
            return Err(format!("condition « {text} » sans clé"));
        }
        Ok(Condition { key: key.to_string(), negate, equals })
    }

    /// Évalue la condition sur les valeurs actuelles du formulaire.
    pub fn eval(&self, values: &Map<String, Value>) -> bool {
        let v = values.get(&self.key).unwrap_or(&Value::Null);
        let r = match &self.equals {
            Some(e) => value_text(v) == *e,
            None => truthy(v),
        };
        r != self.negate
    }
}

/// Valeur « vraie » (case cochée, texte non vide, nombre non nul, liste non vide).
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Texte d'une valeur scalaire (`"abc"` → `abc`, `3` → `3`).
pub fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Contrôle d'un formulaire.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    /// Numéro unique dans le formulaire (ordre de lecture) : identifiant d'interface.
    pub id: usize,
    /// Clé de la valeur (vide pour un texte fixe, un séparateur, une image, un conteneur).
    pub key: String,
    pub label: String,
    /// Aide (infobulle).
    pub help: String,
    pub enabled_if: Option<Condition>,
    /// Largeur souhaitée du champ (pixels).
    pub width: Option<f32>,
    pub kind: ControlKind,
}

impl Control {
    /// Contrôle qui porte une valeur rendue au script ?
    pub fn has_value(&self) -> bool {
        !matches!(
            self.kind,
            ControlKind::Label { .. }
                | ControlKind::Separator
                | ControlKind::Image { .. }
                | ControlKind::Group { .. }
                | ControlKind::Columns { .. }
                | ControlKind::Tabs { .. }
                | ControlKind::Button { .. }
        )
    }

    /// Contrôles enfants (conteneurs), dans l'ordre.
    pub fn children(&self) -> Vec<&Control> {
        match &self.kind {
            ControlKind::Group { controls, .. } => controls.iter().collect(),
            ControlKind::Columns { columns } => columns.iter().flatten().collect(),
            ControlKind::Tabs { tabs } => tabs.iter().flat_map(|(_, c)| c.iter()).collect(),
            _ => Vec::new(),
        }
    }

    /// Valeur par défaut (forme rendue au script).
    pub fn default_value(&self) -> Value {
        match &self.kind {
            ControlKind::Text { default, secret, .. } => {
                Value::String(if *secret { String::new() } else { default.clone() })
            }
            ControlKind::Checkbox { default } => Value::Bool(*default),
            ControlKind::Number { default, decimals, min, max, .. } => {
                let mut d = *default;
                if let Some(m) = min {
                    d = d.max(*m);
                }
                if let Some(m) = max {
                    d = d.min(*m);
                }
                number_value(d, *decimals)
            }
            ControlKind::Choice { default, .. } | ControlKind::Radio { default, .. } => default.clone(),
            ControlKind::MultiChoice { default, .. } => Value::Array(default.clone()),
            ControlKind::ListEdit { default, .. } => {
                Value::Array(default.iter().map(|s| Value::String(s.clone())).collect())
            }
            ControlKind::Table { default, .. } => {
                Value::Array(default.iter().map(|r| Value::Array(r.clone())).collect())
            }
            ControlKind::Path { default, .. } => Value::String(default.clone()),
            _ => Value::Null,
        }
    }

    /// Convertit `v` dans la forme attendue par ce contrôle ; `None` si incompatible.
    pub fn coerce(&self, v: &Value) -> Option<Value> {
        match &self.kind {
            ControlKind::Text { .. } | ControlKind::Path { .. } => match v {
                Value::String(s) => Some(Value::String(s.clone())),
                Value::Number(_) | Value::Bool(_) => Some(Value::String(v.to_string())),
                Value::Null => Some(Value::String(String::new())),
                _ => None,
            },
            ControlKind::Checkbox { .. } => match v {
                Value::Bool(b) => Some(Value::Bool(*b)),
                Value::Number(_) | Value::String(_) | Value::Null => Some(Value::Bool(truthy(v))),
                _ => None,
            },
            ControlKind::Number { decimals, .. } => {
                let f = match v {
                    Value::Number(n) => n.as_f64()?,
                    Value::String(s) => s.trim().replace(',', ".").parse::<f64>().ok()?,
                    _ => return None,
                };
                f.is_finite().then(|| number_value(f, *decimals))
            }
            ControlKind::Choice { choices, editable, .. } => match find_choice(choices, v) {
                Some(i) => Some(choices[i].value.clone()),
                None if v.is_null() => Some(Value::Null),
                None if *editable => match v {
                    Value::String(_) | Value::Number(_) | Value::Bool(_) => Some(Value::String(value_text(v))),
                    _ => None,
                },
                None => None,
            },
            ControlKind::Radio { choices, .. } => match find_choice(choices, v) {
                Some(i) => Some(choices[i].value.clone()),
                None if v.is_null() => Some(Value::Null),
                None => None,
            },
            ControlKind::MultiChoice { choices, .. } => {
                let a = v.as_array()?;
                // Ordre des choix gardé, valeurs inconnues ignorées.
                let picked: Vec<usize> = a.iter().filter_map(|x| find_choice(choices, x)).collect();
                Some(Value::Array(
                    choices
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| picked.contains(i))
                        .map(|(_, c)| c.value.clone())
                        .collect(),
                ))
            }
            ControlKind::ListEdit { .. } => {
                let a = v.as_array()?;
                Some(Value::Array(a.iter().filter(|x| !x.is_null()).map(|x| Value::String(value_text(x))).collect()))
            }
            ControlKind::Table { columns, .. } => {
                let rows = v.as_array()?;
                let mut out = Vec::with_capacity(rows.len());
                for r in rows {
                    let cells = r.as_array()?;
                    out.push(Value::Array(
                        columns
                            .iter()
                            .enumerate()
                            .map(|(i, c)| coerce_cell(c, cells.get(i).unwrap_or(&Value::Null)))
                            .collect(),
                    ));
                }
                Some(Value::Array(out))
            }
            _ => None,
        }
    }
}

/// Nombre JSON : entier si `decimals` = 0 (arrondi), sinon réel arrondi à `decimals`.
pub fn number_value(f: f64, decimals: u32) -> Value {
    if decimals == 0 {
        let r = f.round();
        if r.abs() < 9.0e15 {
            return Value::from(r as i64);
        }
    }
    let p = 10f64.powi(decimals.min(10) as i32);
    let r = (f * p).round() / p;
    serde_json::Number::from_f64(r).map(Value::Number).unwrap_or(Value::Null)
}

/// Rang du choix de valeur `v` (égalité JSON, sinon même texte).
pub fn find_choice(choices: &[ChoiceItem], v: &Value) -> Option<usize> {
    if v.is_null() {
        return None;
    }
    choices.iter().position(|c| c.value == *v).or_else(|| {
        let t = value_text(v);
        choices.iter().position(|c| value_text(&c.value) == t)
    })
}

/// Cellule d'un tableau dans la forme de sa colonne (valeur vide si incompatible).
pub fn coerce_cell(c: &TableColumn, v: &Value) -> Value {
    match c.kind {
        ColumnKind::Text => Value::String(value_text(v)),
        ColumnKind::Checkbox => Value::Bool(truthy(v)),
        ColumnKind::Number => match v {
            Value::Number(n) => Value::Number(n.clone()),
            Value::String(s) => s
                .trim()
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::from(0)),
            _ => Value::from(0),
        },
        ColumnKind::Choice => match find_choice(&c.choices, v) {
            Some(i) => c.choices[i].value.clone(),
            None => c.choices.first().map(|c| c.value.clone()).unwrap_or(Value::Null),
        },
    }
}

/// Formulaire demandé par `comicrust.form`.
#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub title: String,
    /// Texte d'introduction (facultatif).
    pub message: String,
    pub controls: Vec<Control>,
    /// Boutons du bas, dans l'ordre.
    pub buttons: Vec<String>,
    /// Bouton qui annule (le script reçoit `None`).
    pub cancel: Option<usize>,
    /// Largeur souhaitée de la fenêtre (pixels).
    pub width: Option<f32>,
    /// Valeurs initiales données par le script (remplacent les valeurs par défaut).
    pub values: Map<String, Value>,
    /// Nombre total de contrôles (imbriqués compris).
    pub count: usize,
}

/// Réponse à un formulaire : bouton (libellé d'un bouton du bas, ou clé d'un bouton du
/// formulaire) et valeurs.
#[derive(Debug, Clone, PartialEq)]
pub struct FormAnswer {
    pub button: String,
    pub values: Map<String, Value>,
}

/// Erreur de validation d'un champ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub key: String,
    pub message: String,
}

/// Compteur partagé pendant la lecture d'une description.
struct Reader {
    count: usize,
    image_bytes: usize,
    keys: HashSet<String>,
    conditions: Vec<(String, String)>,
}

impl Reader {
    fn new() -> Reader {
        Reader { count: 0, image_bytes: 0, keys: HashSet::new(), conditions: Vec::new() }
    }
}

fn get_str(m: &Map<String, Value>, k: &str) -> Result<String, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) if s.len() <= MAX_TEXT => Ok(s.clone()),
        Some(Value::String(_)) => Err(format!("« {k} » trop long")),
        Some(v @ (Value::Number(_) | Value::Bool(_))) => Ok(v.to_string()),
        Some(_) => Err(format!("« {k} » : texte attendu")),
    }
}

fn get_bool(m: &Map<String, Value>, k: &str, default: bool) -> bool {
    match m.get(k) {
        None | Some(Value::Null) => default,
        Some(v) => truthy(v),
    }
}

fn get_f64(m: &Map<String, Value>, k: &str) -> Result<Option<f64>, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => match n.as_f64().filter(|f| f.is_finite()) {
            Some(f) => Ok(Some(f)),
            None => Err(format!("« {k} » : nombre fini attendu")),
        },
        Some(_) => Err(format!("« {k} » : nombre attendu")),
    }
}

fn get_size(m: &Map<String, Value>, k: &str) -> Result<Option<f32>, String> {
    Ok(get_f64(m, k)?.filter(|f| *f > 0.0).map(|f| f.min(4000.0) as f32))
}

fn get_array<'a>(m: &'a Map<String, Value>, k: &str) -> Result<&'a [Value], String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(format!("« {k} » : liste attendue")),
    }
}

/// Choix : `"texte"`, `[valeur, "libellé"]` ou `{"value":…, "label":…}`.
fn parse_choices(m: &Map<String, Value>, k: &str) -> Result<Vec<ChoiceItem>, String> {
    let a = get_array(m, k)?;
    if a.len() > MAX_CHOICES {
        return Err(format!("« {k} » : plus de {MAX_CHOICES} choix"));
    }
    a.iter()
        .map(|c| match c {
            Value::Array(p) if p.len() == 2 && !p[0].is_array() && !p[0].is_object() => {
                Ok(ChoiceItem { value: p[0].clone(), label: value_text(&p[1]) })
            }
            Value::Object(o) => {
                let value = o.get("value").cloned().unwrap_or(Value::Null);
                let label = o.get("label").map(value_text).unwrap_or_else(|| value_text(&value));
                Ok(ChoiceItem { value, label })
            }
            Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                Ok(ChoiceItem { value: c.clone(), label: value_text(c) })
            }
            _ => Err(format!("« {k} » : choix illisible ({c})")),
        })
        .collect()
}

/// Décode le base64 standard (avec ou sans `=`, blancs ignorés).
pub fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in text.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = val(c).ok_or("base64 invalide")?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// Format d'image reconnu d'après les premiers octets.
pub fn image_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("JPEG")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("PNG")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("WebP")
    } else if bytes.starts_with(b"GIF8") {
        Some("GIF")
    } else if bytes.starts_with(b"BM") {
        Some("BMP")
    } else {
        None
    }
}

/// Image du script (base64) : décodée, taille et format vérifiés, comptée dans le total.
fn read_image(v: &Value, what: &str, total: &mut usize) -> Result<Option<Arc<Vec<u8>>>, String> {
    let text = match v {
        Value::Null => return Ok(None),
        Value::String(s) if s.is_empty() => return Ok(None),
        Value::String(s) => s,
        _ => return Err(format!("{what} : image en base64 attendue")),
    };
    // Borne avant décodage (4 caractères → 3 octets).
    if text.len() / 4 * 3 > MAX_IMAGE_BYTES + 3 {
        return Err(format!("{what} : image trop grosse (plus de {} Mo)", MAX_IMAGE_BYTES / (1024 * 1024)));
    }
    let bytes = decode_base64(text).map_err(|e| format!("{what} : {e}"))?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!("{what} : image trop grosse (plus de {} Mo)", MAX_IMAGE_BYTES / (1024 * 1024)));
    }
    if image_format(&bytes).is_none() {
        return Err(format!("{what} : format d'image non reconnu (JPEG, PNG, WebP, GIF ou BMP attendus)"));
    }
    *total += bytes.len();
    if *total > MAX_IMAGES_TOTAL {
        return Err(format!(
            "images trop grosses au total (plus de {} Mo pour une fenêtre)",
            MAX_IMAGES_TOTAL / (1024 * 1024)
        ));
    }
    Ok(Some(Arc::new(bytes)))
}

fn parse_controls(list: &[Value], r: &mut Reader, depth: usize) -> Result<Vec<Control>, String> {
    if depth > MAX_DEPTH {
        return Err(format!("contrôles imbriqués trop profondément (plus de {MAX_DEPTH} niveaux)"));
    }
    list.iter().map(|c| parse_control(c, r, depth)).collect()
}

fn parse_control(v: &Value, r: &mut Reader, depth: usize) -> Result<Control, String> {
    r.count += 1;
    if r.count > MAX_CONTROLS {
        return Err(format!("formulaire trop grand (plus de {MAX_CONTROLS} contrôles)"));
    }
    let id = r.count - 1;
    let m = v.as_object().ok_or_else(|| format!("contrôle n°{} : objet attendu", id + 1))?;
    let kind_name = get_str(m, "type")?;
    let key = get_str(m, "key")?;
    let label = get_str(m, "label")?;
    let what = if key.is_empty() {
        format!("contrôle n°{} ({kind_name})", id + 1)
    } else {
        format!("contrôle « {key} » ({kind_name})")
    };
    let ctx = |e: String| format!("{what} : {e}");
    let help = get_str(m, "help").map_err(ctx)?;
    let width = get_size(m, "width").map_err(ctx)?;
    let enabled_if = match get_str(m, "enabled_if").map_err(ctx)? {
        s if s.is_empty() => None,
        s => {
            let c = Condition::parse(&s).map_err(ctx)?;
            r.conditions.push((what.clone(), c.key.clone()));
            Some(c)
        }
    };
    let required = get_bool(m, "required", false);
    let height = get_size(m, "height").map_err(ctx)?;
    let kind = match kind_name.as_str() {
        "text" | "textarea" | "secret" => {
            let rows = match get_f64(m, "rows").map_err(ctx)? {
                Some(n) => n.clamp(1.0, 100.0) as u32,
                None if kind_name == "textarea" => 4,
                None => 1,
            };
            let inserts = parse_choices(m, "inserts").map_err(ctx)?;
            ControlKind::Text {
                default: get_str(m, "default").map_err(ctx)?,
                placeholder: get_str(m, "placeholder").map_err(ctx)?,
                required,
                rows,
                secret: kind_name == "secret" || get_bool(m, "secret", false),
                inserts,
            }
        }
        "checkbox" => ControlKind::Checkbox { default: get_bool(m, "default", false) },
        "number" => {
            let min = get_f64(m, "min").map_err(ctx)?;
            let max = get_f64(m, "max").map_err(ctx)?;
            if let (Some(a), Some(b)) = (min, max) {
                if a > b {
                    return Err(ctx(format!("minimum {a} supérieur au maximum {b}")));
                }
            }
            ControlKind::Number {
                default: get_f64(m, "default").map_err(ctx)?.unwrap_or(0.0),
                min,
                max,
                step: get_f64(m, "step").map_err(ctx)?.filter(|s| *s > 0.0).unwrap_or(1.0),
                decimals: get_f64(m, "decimals").map_err(ctx)?.map(|d| d.clamp(0.0, 10.0) as u32).unwrap_or(0),
            }
        }
        "choice" | "radio" => {
            let choices = parse_choices(m, "choices").map_err(ctx)?;
            let editable = kind_name == "choice" && get_bool(m, "editable", false);
            if choices.is_empty() && !editable {
                return Err(ctx("aucun choix".into()));
            }
            let raw = m.get("default").cloned().unwrap_or(Value::Null);
            let default = match find_choice(&choices, &raw) {
                Some(i) => choices[i].value.clone(),
                None if editable && !raw.is_null() => Value::String(value_text(&raw)),
                // Boutons radio : premier choix par défaut.
                None if kind_name == "radio" => choices[0].value.clone(),
                None => Value::Null,
            };
            if kind_name == "radio" {
                ControlKind::Radio { choices, default, horizontal: get_bool(m, "horizontal", false) }
            } else {
                ControlKind::Choice { choices, default, editable, required }
            }
        }
        "multichoice" => {
            let choices = parse_choices(m, "choices").map_err(ctx)?;
            let wanted = get_array(m, "default").map_err(ctx)?;
            let default: Vec<Value> = choices
                .iter()
                .filter(|c| wanted.iter().any(|w| find_choice(std::slice::from_ref(*c), w).is_some()))
                .map(|c| c.value.clone())
                .collect();
            ControlKind::MultiChoice { choices, default, height, required }
        }
        "list_edit" => {
            let default: Vec<String> = get_array(m, "default").map_err(ctx)?.iter().map(value_text).collect();
            let choices: Vec<String> = get_array(m, "choices").map_err(ctx)?.iter().map(value_text).collect();
            if default.len() > MAX_ROWS || choices.len() > MAX_CHOICES {
                return Err(ctx("liste trop longue".into()));
            }
            ControlKind::ListEdit { default, choices, unique: get_bool(m, "unique", true), required, height }
        }
        "table" => {
            let cols = get_array(m, "columns").map_err(ctx)?;
            if cols.is_empty() || cols.len() > MAX_COLUMNS {
                return Err(ctx(format!("1 à {MAX_COLUMNS} colonnes attendues")));
            }
            let mut columns = Vec::new();
            for c in cols {
                let cm = match c {
                    Value::Object(o) => o.clone(),
                    Value::String(s) => {
                        let mut o = Map::new();
                        o.insert("title".into(), Value::String(s.clone()));
                        o
                    }
                    _ => return Err(ctx("colonne illisible".into())),
                };
                let kind = match get_str(&cm, "type").map_err(ctx)?.as_str() {
                    "" | "text" => ColumnKind::Text,
                    "number" => ColumnKind::Number,
                    "checkbox" => ColumnKind::Checkbox,
                    "choice" => ColumnKind::Choice,
                    other => return Err(ctx(format!("type de colonne inconnu « {other} »"))),
                };
                let choices = parse_choices(&cm, "choices").map_err(ctx)?;
                if kind == ColumnKind::Choice && choices.is_empty() {
                    return Err(ctx("colonne à choix sans choix".into()));
                }
                columns.push(TableColumn {
                    title: get_str(&cm, "title").map_err(ctx)?,
                    kind,
                    choices,
                    width: get_size(&cm, "width").map_err(ctx)?,
                });
            }
            let rows = get_array(m, "default").map_err(ctx)?;
            if rows.len() > MAX_ROWS {
                return Err(ctx(format!("plus de {MAX_ROWS} lignes")));
            }
            let mut default = Vec::new();
            for row in rows {
                let cells = row.as_array().ok_or_else(|| ctx("ligne : liste attendue".into()))?;
                default.push(
                    columns
                        .iter()
                        .enumerate()
                        .map(|(i, c)| coerce_cell(c, cells.get(i).unwrap_or(&Value::Null)))
                        .collect(),
                );
            }
            ControlKind::Table { columns, default, height }
        }
        "folder" | "file" => {
            let mut filters = Vec::new();
            for f in get_array(m, "filters").map_err(ctx)? {
                // ["Images", ["jpg", "png"]] ou ["Images", "jpg;png"]
                let Some(p) = f.as_array().filter(|p| p.len() == 2) else {
                    return Err(ctx("filtre : [nom, extensions] attendu".into()));
                };
                let exts: Vec<String> = match &p[1] {
                    Value::Array(a) => a.iter().map(value_text).collect(),
                    other => value_text(other).split([';', ',', ' ']).map(str::to_string).collect(),
                };
                let exts = exts
                    .into_iter()
                    .map(|e| e.trim().trim_start_matches("*.").trim_start_matches('.').to_string())
                    .filter(|e| !e.is_empty())
                    .collect();
                filters.push((value_text(&p[0]), exts));
            }
            ControlKind::Path {
                default: get_str(m, "default").map_err(ctx)?,
                folder: kind_name == "folder",
                save: get_bool(m, "save", false),
                filters,
                required,
            }
        }
        "label" => ControlKind::Label {
            text: get_str(m, "text").map_err(ctx)?,
            style: match get_str(m, "style").map_err(ctx)?.as_str() {
                "" | "normal" => LabelStyle::Normal,
                "bold" => LabelStyle::Bold,
                "small" => LabelStyle::Small,
                "weak" => LabelStyle::Weak,
                "heading" => LabelStyle::Heading,
                "warning" => LabelStyle::Warning,
                "error" => LabelStyle::Error,
                other => return Err(ctx(format!("style inconnu « {other} »"))),
            },
        },
        "separator" => ControlKind::Separator,
        "image" => {
            let data = read_image(m.get("data").unwrap_or(&Value::Null), &what, &mut r.image_bytes)?
                .ok_or_else(|| ctx("image vide".into()))?;
            ControlKind::Image {
                data,
                caption: get_str(m, "caption").map_err(ctx)?,
                max_width: get_size(m, "max_width").map_err(ctx)?.unwrap_or(240.0),
                max_height: get_size(m, "max_height").map_err(ctx)?.unwrap_or(320.0),
            }
        }
        "group" => ControlKind::Group {
            title: get_str(m, "title").map_err(ctx)?,
            controls: parse_controls(get_array(m, "controls").map_err(ctx)?, r, depth + 1)?,
        },
        "columns" => {
            let cols = get_array(m, "columns").map_err(ctx)?;
            if cols.is_empty() || cols.len() > 6 {
                return Err(ctx("1 à 6 colonnes attendues".into()));
            }
            let mut columns = Vec::new();
            for c in cols {
                let list = c.as_array().ok_or_else(|| ctx("colonne : liste de contrôles attendue".into()))?;
                columns.push(parse_controls(list, r, depth + 1)?);
            }
            ControlKind::Columns { columns }
        }
        "tabs" => {
            let list = get_array(m, "tabs").map_err(ctx)?;
            if list.is_empty() {
                return Err(ctx("aucun onglet".into()));
            }
            let mut tabs = Vec::new();
            for t in list {
                let to = t.as_object().ok_or_else(|| ctx("onglet : {title, controls} attendu".into()))?;
                let title = get_str(to, "title").map_err(ctx)?;
                let controls = parse_controls(get_array(to, "controls").map_err(ctx)?, r, depth + 1)?;
                tabs.push((title, controls));
            }
            ControlKind::Tabs { tabs }
        }
        "button" => {
            if key.is_empty() {
                return Err(ctx("bouton sans clé".into()));
            }
            ControlKind::Button { validate: get_bool(m, "validate", false) }
        }
        "" => return Err(ctx("type manquant".into())),
        other => return Err(ctx(format!("type de contrôle inconnu « {other} »"))),
    };
    let control = Control { id, key: key.clone(), label, help, enabled_if, width, kind };
    if control.has_value() || matches!(control.kind, ControlKind::Button { .. }) {
        if key.is_empty() {
            return Err(ctx("clé manquante".into()));
        }
        if !r.keys.insert(key.clone()) {
            return Err(format!("clé « {key} » utilisée deux fois"));
        }
    }
    Ok(control)
}

/// Parcours de tous les contrôles (ordre de lecture, conteneurs avant leurs enfants), avec
/// l'état « activé » hérité des conteneurs.
fn walk<'a>(
    controls: &'a [Control],
    values: &Map<String, Value>,
    parent_enabled: bool,
    f: &mut dyn FnMut(&'a Control, bool),
) {
    for c in controls {
        let enabled = parent_enabled && c.enabled_if.as_ref().is_none_or(|cond| cond.eval(values));
        f(c, enabled);
        match &c.kind {
            ControlKind::Group { controls, .. } => walk(controls, values, enabled, f),
            ControlKind::Columns { columns } => {
                for col in columns {
                    walk(col, values, enabled, f);
                }
            }
            ControlKind::Tabs { tabs } => {
                for (_, col) in tabs {
                    walk(col, values, enabled, f);
                }
            }
            _ => {}
        }
    }
}

impl FormSpec {
    /// Lit la demande `form` du script.
    pub fn parse(p: &Map<String, Value>) -> Result<FormSpec, String> {
        let mut r = Reader::new();
        let controls = parse_controls(get_array(p, "controls")?, &mut r, 0)?;
        for (what, key) in &r.conditions {
            if !r.keys.contains(key) {
                return Err(format!("{what} : enabled_if désigne une clé inconnue « {key} »"));
            }
        }
        let mut buttons: Vec<String> = get_array(p, "buttons")?.iter().map(value_text).collect();
        buttons.retain(|b| !b.trim().is_empty());
        if buttons.len() > MAX_BUTTONS {
            return Err(format!("plus de {MAX_BUTTONS} boutons"));
        }
        if buttons.is_empty() {
            buttons = vec!["OK".into(), "Annuler".into()];
        }
        let mut seen = HashSet::new();
        if let Some(b) = buttons.iter().find(|b| !seen.insert(b.as_str())) {
            return Err(format!("bouton « {b} » en double"));
        }
        let cancel = match p.get("cancel") {
            None => (buttons.len() > 1).then(|| buttons.len() - 1),
            Some(Value::Null) => None,
            Some(v) => {
                let t = value_text(v);
                Some(
                    buttons
                        .iter()
                        .position(|b| *b == t)
                        .ok_or_else(|| format!("bouton d'annulation « {t} » absent"))?,
                )
            }
        };
        let values = match p.get("values") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(o)) => o.clone(),
            Some(_) => return Err("« values » : dictionnaire attendu".into()),
        };
        Ok(FormSpec {
            title: get_str(p, "title")?,
            message: get_str(p, "message")?,
            controls,
            buttons,
            cancel,
            width: get_size(p, "width")?,
            values,
            count: r.count,
        })
    }

    /// Contrôles qui portent une valeur, dans l'ordre.
    pub fn value_controls(&self) -> Vec<&Control> {
        let mut out = Vec::new();
        walk(&self.controls, &Map::new(), true, &mut |c, _| {
            if c.has_value() {
                out.push(c);
            }
        });
        out
    }

    /// Contrôle de clé `key`.
    pub fn control(&self, key: &str) -> Option<&Control> {
        let mut out = None;
        walk(&self.controls, &Map::new(), true, &mut |c, _| {
            if out.is_none() && c.key == key {
                out = Some(c);
            }
        });
        out
    }

    /// Valeurs initiales : valeurs par défaut, remplacées par celles données par le script
    /// (`values=`) quand elles conviennent au contrôle.
    pub fn initial_values(&self) -> Map<String, Value> {
        let mut out = Map::new();
        for c in self.value_controls() {
            let v = match self.values.get(&c.key) {
                // Saisie secrète : jamais pré-remplie.
                Some(v) if !matches!(c.kind, ControlKind::Text { secret: true, .. }) => {
                    c.coerce(v).unwrap_or_else(|| c.default_value())
                }
                _ => c.default_value(),
            };
            out.insert(c.key.clone(), v);
        }
        out
    }

    /// Réponse de l'interface remise en forme : chaque valeur convertie dans la forme de son
    /// contrôle (valeur initiale si absente ou incompatible), clés inconnues retirées.
    pub fn sanitize(&self, answer: &Map<String, Value>) -> Map<String, Value> {
        let initial = self.initial_values();
        let mut out = Map::new();
        for c in self.value_controls() {
            let v = answer.get(&c.key).and_then(|v| c.coerce(v)).or_else(|| initial.get(&c.key).cloned());
            out.insert(c.key.clone(), v.unwrap_or(Value::Null));
        }
        out
    }

    /// Contrôle activé (conditions `enabled_if` du contrôle et de ses conteneurs) ?
    pub fn enabled_keys(&self, values: &Map<String, Value>) -> HashSet<String> {
        let mut out = HashSet::new();
        walk(&self.controls, values, true, &mut |c, enabled| {
            if enabled && !c.key.is_empty() {
                out.insert(c.key.clone());
            }
        });
        out
    }

    /// Erreurs de saisie (contrôles activés seulement) : champ obligatoire vide, nombre hors
    /// bornes, doublon dans une liste sans doublon.
    pub fn validate(&self, values: &Map<String, Value>) -> Vec<FieldError> {
        let mut errors = Vec::new();
        walk(&self.controls, values, true, &mut |c, enabled| {
            if !enabled || !c.has_value() {
                return;
            }
            let v = values.get(&c.key).unwrap_or(&Value::Null);
            let mut err = |m: String| errors.push(FieldError { key: c.key.clone(), message: m });
            match &c.kind {
                ControlKind::Text { required: true, secret, .. } => {
                    // Secret : vide = garder la valeur actuelle (pas d'erreur).
                    if !secret && value_text(v).trim().is_empty() {
                        err("champ obligatoire".into());
                    }
                }
                ControlKind::Path { required: true, .. } if value_text(v).trim().is_empty() => {
                    err("chemin obligatoire".into());
                }
                ControlKind::Number { min, max, .. } => match v.as_f64() {
                    None => err("nombre attendu".into()),
                    Some(f) => {
                        if let Some(m) = min.filter(|m| f < *m) {
                            err(format!("au moins {}", fmt_num(m)));
                        } else if let Some(m) = max.filter(|m| f > *m) {
                            err(format!("au plus {}", fmt_num(m)));
                        }
                    }
                },
                ControlKind::Choice { required: true, .. } if value_text(v).trim().is_empty() => {
                    err("choix obligatoire".into());
                }
                ControlKind::MultiChoice { required: true, .. } if !truthy(v) => {
                    err("cocher au moins un élément".into());
                }
                ControlKind::ListEdit { required, unique, .. } => {
                    let items: Vec<String> =
                        v.as_array().map(|a| a.iter().map(value_text).collect()).unwrap_or_default();
                    if *required && items.is_empty() {
                        err("liste vide".into());
                    } else if *unique {
                        let mut seen = HashSet::new();
                        if let Some(d) = items.iter().find(|i| !seen.insert(i.to_lowercase())) {
                            err(format!("« {d} » en double"));
                        }
                    }
                }
                _ => {}
            }
        });
        errors
    }

    /// Bouton rendu par l'interface : `Ok(true)` = formulaire validé (valeurs rendues au
    /// script), `Ok(false)` = annulation, `Err` = bouton inconnu.
    pub fn check_button(&self, button: &str) -> Result<bool, String> {
        if let Some(i) = self.buttons.iter().position(|b| b == button) {
            return Ok(Some(i) != self.cancel);
        }
        match self.control(button) {
            Some(Control { kind: ControlKind::Button { .. }, .. }) => Ok(true),
            _ => Err(format!("bouton inconnu « {button} »")),
        }
    }

    /// Le bouton demande-t-il la validation des champs ? (boutons du bas : oui ; bouton du
    /// formulaire : selon `validate`).
    pub fn button_validates(&self, button: &str) -> bool {
        if self.buttons.iter().any(|b| b == button) {
            return true;
        }
        matches!(self.control(button), Some(Control { kind: ControlKind::Button { validate: true }, .. }))
    }

    /// Images du formulaire (identifiant du contrôle → octets).
    pub fn images(&self) -> Vec<(usize, Arc<Vec<u8>>)> {
        let mut out = Vec::new();
        walk(&self.controls, &Map::new(), true, &mut |c, _| {
            if let ControlKind::Image { data, .. } = &c.kind {
                out.push((c.id, data.clone()));
            }
        });
        out
    }
}

fn fmt_num(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

/// Liste de choix demandée par `comicrust.choose`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChooseSpec {
    pub title: String,
    pub message: String,
    pub columns: Vec<String>,
    /// Cellules (textes), une liste par ligne, autant que de colonnes.
    pub rows: Vec<Vec<String>>,
    /// Vignettes (vide : pas de colonne de vignettes ; sinon une par ligne, `None` = sans).
    pub images: Vec<Option<Arc<Vec<u8>>>>,
    pub multi: bool,
    /// Lignes choisies au départ.
    pub default: Vec<usize>,
    /// Champ de filtre au-dessus de la liste.
    pub filter: bool,
    /// Délai (secondes) après lequel le choix par défaut est validé tout seul (comme
    /// l'option « choix automatique après N s » de Bedetheque Scraper) ; toute action de
    /// l'utilisateur arrête le compte à rebours.
    pub timeout: Option<f64>,
    /// Hauteur des vignettes (pixels).
    pub thumb_size: f32,
}

impl ChooseSpec {
    /// Lit la demande `choose` du script.
    pub fn parse(p: &Map<String, Value>) -> Result<ChooseSpec, String> {
        let mut columns: Vec<String> = get_array(p, "columns")?.iter().map(value_text).collect();
        if columns.len() > MAX_COLUMNS {
            return Err(format!("plus de {MAX_COLUMNS} colonnes"));
        }
        let raw = get_array(p, "rows")?;
        if raw.len() > MAX_ROWS {
            return Err(format!("plus de {MAX_ROWS} lignes"));
        }
        let mut rows = Vec::with_capacity(raw.len());
        for (i, r) in raw.iter().enumerate() {
            let cells: Vec<String> = match r {
                Value::Array(a) => a.iter().map(value_text).collect(),
                Value::Object(_) => return Err(format!("ligne {i} : liste de cellules attendue")),
                other => vec![value_text(other)],
            };
            if cells.len() > MAX_COLUMNS {
                return Err(format!("ligne {i} : plus de {MAX_COLUMNS} cellules"));
            }
            rows.push(cells);
        }
        let n_cols = rows.iter().map(Vec::len).max().unwrap_or(0).max(columns.len()).max(1);
        columns.resize(n_cols, String::new());
        for r in &mut rows {
            r.resize(n_cols, String::new());
        }
        let raw_images = get_array(p, "images")?;
        let mut images = Vec::new();
        if !raw_images.is_empty() {
            if raw_images.len() != rows.len() {
                return Err(format!("{} images pour {} lignes", raw_images.len(), rows.len()));
            }
            let mut total = 0;
            for (i, v) in raw_images.iter().enumerate() {
                images.push(read_image(v, &format!("image de la ligne {i}"), &mut total)?);
            }
            if images.iter().all(Option::is_none) {
                images.clear();
            }
        }
        let multi = get_bool(p, "multi", false);
        let mut default: Vec<usize> = match p.get("default") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_u64).map(|u| u as usize).collect(),
            Some(v) => v.as_u64().map(|u| vec![u as usize]).ok_or("« default » : rang ou liste de rangs attendu")?,
        };
        default.retain(|i| *i < rows.len());
        default.dedup();
        if !multi {
            default.truncate(1);
        }
        let timeout = get_f64(p, "timeout")?.filter(|t| *t > 0.0).map(|t| t.min(3600.0));
        Ok(ChooseSpec {
            title: get_str(p, "title")?,
            message: get_str(p, "message")?,
            columns,
            filter: get_bool(p, "filter", rows.len() > 8),
            rows,
            images,
            multi,
            default,
            timeout,
            thumb_size: get_size(p, "thumb_size")?.unwrap_or(64.0).clamp(24.0, 256.0),
        })
    }

    /// Réponse de l'interface remise en forme : rangs valides, sans doublon, triés ; un
    /// seul rang sans `multi`. `None` si rien d'utilisable (choix simple vide).
    pub fn sanitize(&self, picked: &[usize]) -> Option<Vec<usize>> {
        let mut out: Vec<usize> = picked.iter().copied().filter(|i| *i < self.rows.len()).collect();
        out.sort_unstable();
        out.dedup();
        if !self.multi && out.len() != 1 {
            return None;
        }
        Some(out)
    }
}

/// Mise à jour de la fenêtre de progression (`comicrust.progress(..., detail=, image=)`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProgressDetails {
    /// Sous-texte (`None` : inchangé).
    pub detail: Option<String>,
    /// Image (`None` : inchangée ; `Some(None)` : retirée).
    pub image: Option<Option<Arc<Vec<u8>>>>,
}

impl ProgressDetails {
    /// Lit les champs facultatifs d'une notification `progress`.
    pub fn parse(detail: Option<String>, image: Option<&Value>) -> Result<ProgressDetails, String> {
        let image = match image {
            None => None,
            Some(v) => Some(read_image(v, "image de progression", &mut 0)?),
        };
        Ok(ProgressDetails { detail, image })
    }

    pub fn is_empty(&self) -> bool {
        self.detail.is_none() && self.image.is_none()
    }
}

/// Adresse acceptée par `comicrust.open_url` : http ou https seulement, avec un hôte.
pub fn check_url(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.len() > 8192 {
        return Err("adresse trop longue".into());
    }
    let u = url::Url::parse(t).map_err(|e| format!("adresse invalide « {t} » : {e}"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(format!("adresse refusée « {t} » : seules http et https sont permises"));
    }
    if u.host_str().is_none_or(str::is_empty) {
        return Err(format!("adresse sans hôte « {t} »"));
    }
    Ok(u.to_string())
}
