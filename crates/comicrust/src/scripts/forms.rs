//! État des formulaires et listes de choix des scripts dans l'interface (v0.10.1) :
//! conversion des contrôles décrits par le script ([`FormSpec`]) en champs éditables,
//! retour aux valeurs JSON, validation avant fermeture ; filtre et tri des listes de
//! choix ; décodage borné des images. Le dessin est dans `ui/script_forms.rs`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use comicrust_core::scripting::forms::{
    find_choice, number_value, value_text, ChoiceItem, ColumnKind, Control, ControlKind, TableColumn,
};
use comicrust_core::scripting::{ChooseSpec, FormAnswer, FormSpec};
use eframe::egui::{self, ColorImage, TextureHandle};
use serde_json::{Map, Value};

/// Côté maximal d'une image de formulaire (pixels, après réduction).
pub const FORM_IMAGE_SIDE: u32 = 1024;
/// Côté maximal d'une vignette de liste de choix (aussi utilisée pour l'aperçu agrandi).
pub const CHOOSE_IMAGE_SIDE: u32 = 480;
/// Côté maximal d'une image de la fenêtre de progression.
pub const PROGRESS_IMAGE_SIDE: u32 = 480;
/// Dimensions maximales acceptées avant décodage (protection contre les images géantes).
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_IMAGE_PIXELS: u64 = 80_000_000;

/// Décode une image du script (JPEG, PNG, WebP, GIF, BMP) et la réduit à `max_side`
/// pixels au plus ; `None` si illisible ou démesurée (dimensions lues avant décodage).
pub fn decode_image(bytes: &[u8], max_side: u32) -> Option<ColorImage> {
    let (w, h) =
        image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()?;
    if w == 0 || h == 0 || w > MAX_IMAGE_DIMENSION || h > MAX_IMAGE_DIMENSION || w as u64 * h as u64 > MAX_IMAGE_PIXELS
    {
        return None;
    }
    let mut img = comicrust_core::providers::decode_image(bytes).ok()?;
    let side = img.width().max(img.height());
    if side > max_side {
        let r = max_side as f64 / side as f64;
        let nw = ((img.width() as f64 * r).round() as u32).max(1);
        let nh = ((img.height() as f64 * r).round() as u32).max(1);
        img = image::imageops::thumbnail(&img, nw, nh);
    }
    Some(ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

/// Cellule éditable d'un tableau.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Number(f64),
    Bool(bool),
    Choice(usize),
}

impl Cell {
    /// Cellule vide d'une colonne.
    pub fn empty(col: &TableColumn) -> Cell {
        match col.kind {
            ColumnKind::Text => Cell::Text(String::new()),
            ColumnKind::Number => Cell::Number(0.0),
            ColumnKind::Checkbox => Cell::Bool(false),
            ColumnKind::Choice => Cell::Choice(0),
        }
    }

    fn from_value(col: &TableColumn, v: &Value) -> Cell {
        match col.kind {
            ColumnKind::Text => Cell::Text(value_text(v)),
            ColumnKind::Number => Cell::Number(v.as_f64().unwrap_or(0.0)),
            ColumnKind::Checkbox => Cell::Bool(v.as_bool().unwrap_or(false)),
            ColumnKind::Choice => Cell::Choice(find_choice(&col.choices, v).unwrap_or(0)),
        }
    }

    fn to_value(&self, col: &TableColumn) -> Value {
        match self {
            Cell::Text(s) => Value::String(s.clone()),
            Cell::Number(f) => number_value(*f, if f.fract() == 0.0 { 0 } else { 6 }),
            Cell::Bool(b) => Value::Bool(*b),
            Cell::Choice(i) => col.choices.get(*i).map(|c| c.value.clone()).unwrap_or(Value::Null),
        }
    }
}

/// Champ éditable d'un contrôle.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Text(String),
    Bool(bool),
    Number(f64),
    /// Liste déroulante ou boutons radio : rang du choix ; texte saisi (liste éditable).
    Choice {
        index: Option<usize>,
        text: String,
    },
    Multi(Vec<bool>),
    List {
        items: Vec<String>,
        input: String,
        selected: Option<usize>,
    },
    Table {
        rows: Vec<Vec<Cell>>,
        selected: Option<usize>,
    },
}

/// Champ d'un contrôle à partir de sa valeur.
pub fn field_from_value(c: &Control, v: &Value) -> Option<Field> {
    Some(match &c.kind {
        ControlKind::Text { .. } | ControlKind::Path { .. } => Field::Text(value_text(v)),
        ControlKind::Checkbox { .. } => Field::Bool(v.as_bool().unwrap_or(false)),
        ControlKind::Number { .. } => Field::Number(v.as_f64().unwrap_or(0.0)),
        ControlKind::Choice { choices, editable, .. } => {
            let index = find_choice(choices, v);
            let text = match index {
                Some(i) => choices[i].label.clone(),
                None if *editable => value_text(v),
                None => String::new(),
            };
            Field::Choice { index, text }
        }
        ControlKind::Radio { choices, .. } => Field::Choice { index: find_choice(choices, v), text: String::new() },
        ControlKind::MultiChoice { choices, .. } => {
            let picked = v.as_array().cloned().unwrap_or_default();
            Field::Multi(choices.iter().map(|c| picked.contains(&c.value)).collect())
        }
        ControlKind::ListEdit { .. } => Field::List {
            items: v.as_array().map(|a| a.iter().map(value_text).collect()).unwrap_or_default(),
            input: String::new(),
            selected: None,
        },
        ControlKind::Table { columns, .. } => Field::Table {
            rows: v
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|r| {
                            let cells = r.as_array().cloned().unwrap_or_default();
                            columns
                                .iter()
                                .enumerate()
                                .map(|(i, col)| Cell::from_value(col, cells.get(i).unwrap_or(&Value::Null)))
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default(),
            selected: None,
        },
        _ => return None,
    })
}

/// Valeur (forme rendue au script) d'un champ.
pub fn field_value(c: &Control, f: &Field) -> Value {
    match (&c.kind, f) {
        (ControlKind::Number { decimals, .. }, Field::Number(n)) => number_value(*n, *decimals),
        (ControlKind::Choice { choices, editable: true, .. }, Field::Choice { text, .. }) => {
            match choices.iter().find(|ch| ch.label == *text) {
                Some(ch) => ch.value.clone(),
                None if text.is_empty() => Value::Null,
                None => Value::String(text.clone()),
            }
        }
        (ControlKind::Choice { choices, .. } | ControlKind::Radio { choices, .. }, Field::Choice { index, .. }) => {
            index.and_then(|i| choices.get(i)).map(|ch| ch.value.clone()).unwrap_or(Value::Null)
        }
        (ControlKind::MultiChoice { choices, .. }, Field::Multi(checked)) => {
            Value::Array(choices.iter().zip(checked).filter(|(_, on)| **on).map(|(ch, _)| ch.value.clone()).collect())
        }
        (ControlKind::Table { columns, .. }, Field::Table { rows, .. }) => Value::Array(
            rows.iter()
                .map(|r| Value::Array(columns.iter().zip(r).map(|(col, cell)| cell.to_value(col)).collect()))
                .collect(),
        ),
        (_, Field::Text(s)) => Value::String(s.clone()),
        (_, Field::Bool(b)) => Value::Bool(*b),
        (_, Field::Number(n)) => number_value(*n, 6),
        (_, Field::List { items, .. }) => Value::Array(items.iter().map(|s| Value::String(s.clone())).collect()),
        _ => Value::Null,
    }
}

/// Formulaire affiché.
pub struct FormState {
    pub spec: FormSpec,
    pub fields: HashMap<String, Field>,
    /// Onglet choisi de chaque contrôle `tabs` (identifiant → rang).
    pub tabs: HashMap<usize, usize>,
    /// Erreurs de validation (clé → message), après un clic sur un bouton.
    pub errors: HashMap<String, String>,
    /// Images décodées en attente de texture (identifiant du contrôle → image).
    pub images: HashMap<usize, ColorImage>,
    pub textures: HashMap<usize, TextureHandle>,
    /// Onglets à ouvrir pour voir chaque contrôle : clé → [(identifiant des onglets, rang)].
    key_tabs: HashMap<String, Vec<(usize, usize)>>,
}

impl FormState {
    pub fn new(spec: FormSpec, images: HashMap<usize, ColorImage>) -> FormState {
        let initial = spec.initial_values();
        let mut fields = HashMap::new();
        for c in spec.value_controls() {
            if let Some(f) = field_from_value(c, initial.get(&c.key).unwrap_or(&Value::Null)) {
                fields.insert(c.key.clone(), f);
            }
        }
        let mut key_tabs = HashMap::new();
        fn index(cs: &[Control], path: &mut Vec<(usize, usize)>, out: &mut HashMap<String, Vec<(usize, usize)>>) {
            for c in cs {
                if !c.key.is_empty() {
                    out.insert(c.key.clone(), path.clone());
                }
                match &c.kind {
                    ControlKind::Tabs { tabs } => {
                        for (i, (_, sub)) in tabs.iter().enumerate() {
                            path.push((c.id, i));
                            index(sub, path, out);
                            path.pop();
                        }
                    }
                    ControlKind::Group { controls, .. } => index(controls, path, out),
                    ControlKind::Columns { columns } => {
                        for col in columns {
                            index(col, path, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        index(&spec.controls, &mut Vec::new(), &mut key_tabs);
        FormState {
            spec,
            fields,
            tabs: HashMap::new(),
            errors: HashMap::new(),
            images,
            textures: HashMap::new(),
            key_tabs,
        }
    }

    /// Valeurs actuelles (clé → valeur JSON).
    pub fn values(&self) -> Map<String, Value> {
        let mut out = Map::new();
        for c in self.spec.value_controls() {
            if let Some(f) = self.fields.get(&c.key) {
                out.insert(c.key.clone(), field_value(c, f));
            }
        }
        out
    }

    /// Clic sur un bouton (`button` : libellé d'un bouton du bas ou clé d'un bouton du
    /// formulaire). `None` : saisie à corriger (erreurs notées, onglet de la première
    /// erreur ouvert) ; sinon la réponse à rendre au script.
    pub fn submit(&mut self, button: &str) -> Option<FormAnswer> {
        let values = self.values();
        self.errors.clear();
        let cancel = self.spec.cancel.and_then(|i| self.spec.buttons.get(i)).is_some_and(|b| b == button);
        if !cancel && self.spec.button_validates(button) {
            let errors = self.spec.validate(&values);
            if let Some(first) = errors.first() {
                if let Some(path) = self.key_tabs.get(&first.key) {
                    for (tabs, i) in path {
                        self.tabs.insert(*tabs, *i);
                    }
                }
                self.errors = errors.into_iter().map(|e| (e.key, e.message)).collect();
                return None;
            }
        }
        Some(FormAnswer { button: button.to_string(), values })
    }

    /// Libellé du contrôle de clé `key` (pour le résumé des erreurs).
    pub fn label_of(&self, key: &str) -> String {
        self.spec
            .control(key)
            .map(|c| if c.label.is_empty() { key.to_string() } else { c.label.clone() })
            .unwrap_or_default()
    }
}

/// Ajoute `item` à une liste éditable (vide ou doublon refusé si `unique`) ; vrai si ajouté.
pub fn list_add(items: &mut Vec<String>, item: &str, unique: bool) -> bool {
    let item = item.trim();
    if item.is_empty() || (unique && items.iter().any(|i| i.to_lowercase() == item.to_lowercase())) {
        return false;
    }
    items.push(item.to_string());
    true
}

/// Déplace l'élément `i` d'un cran (`up` : vers le haut) ; nouveau rang.
pub fn move_item<T>(items: &mut [T], i: usize, up: bool) -> usize {
    if up && i > 0 && i < items.len() {
        items.swap(i, i - 1);
        i - 1
    } else if !up && i + 1 < items.len() {
        items.swap(i, i + 1);
        i + 1
    } else {
        i
    }
}

/// Insère `snippet` dans `text` au rang de caractère `at` (fin si `None`) ; renvoie le rang
/// du curseur après l'insertion.
pub fn insert_at(text: &mut String, at: Option<usize>, snippet: &str) -> usize {
    let n = text.chars().count();
    let at = at.unwrap_or(n).min(n);
    let byte = text.char_indices().nth(at).map(|(b, _)| b).unwrap_or(text.len());
    text.insert_str(byte, snippet);
    at + snippet.chars().count()
}

/// Libellé d'un choix (rang) ; vide si absent.
pub fn choice_label(choices: &[ChoiceItem], i: Option<usize>) -> &str {
    i.and_then(|i| choices.get(i)).map(|c| c.label.as_str()).unwrap_or("")
}

/// Liste de choix affichée.
pub struct ChooseState {
    pub spec: ChooseSpec,
    /// Vignettes décodées en attente de texture.
    pub images: Vec<Option<ColorImage>>,
    pub textures: Vec<Option<TextureHandle>>,
    pub filter: String,
    /// Colonne de tri et ordre décroissant.
    pub sort: Option<(usize, bool)>,
    /// Lignes choisies.
    pub selected: Vec<bool>,
    /// Dernière ligne cliquée (aperçu de sa vignette).
    pub current: Option<usize>,
    /// Validation automatique (option `timeout`) ; `None` dès que l'utilisateur agit.
    pub deadline: Option<Instant>,
}

impl ChooseState {
    pub fn new(spec: ChooseSpec, images: Vec<Option<ColorImage>>) -> ChooseState {
        let mut selected = vec![false; spec.rows.len()];
        for &i in &spec.default {
            selected[i] = true;
        }
        let deadline = spec.timeout.map(|t| Instant::now() + Duration::from_secs_f64(t));
        let n = images.len();
        ChooseState {
            current: spec.default.first().copied(),
            spec,
            images,
            textures: (0..n).map(|_| None).collect(),
            filter: String::new(),
            sort: None,
            selected,
            deadline,
        }
    }

    /// Rangs des lignes affichées : filtre (tous les mots, sans casse, dans une cellule
    /// quelconque), puis tri (numérique si les deux cellules sont des nombres, sinon
    /// « naturel » : 2 avant 10).
    pub fn view(&self) -> Vec<usize> {
        let words: Vec<String> = self.filter.split_whitespace().map(str::to_lowercase).collect();
        let mut out: Vec<usize> = (0..self.spec.rows.len())
            .filter(|&i| {
                words.is_empty() || {
                    let row = self.spec.rows[i].join("\u{1}").to_lowercase();
                    words.iter().all(|w| row.contains(w.as_str()))
                }
            })
            .collect();
        if let Some((col, desc)) = self.sort {
            out.sort_by(|&a, &b| {
                let (x, y) = (&self.spec.rows[a][col], &self.spec.rows[b][col]);
                let o = match (num(x), num(y)) {
                    (Some(p), Some(q)) => p.total_cmp(&q),
                    _ => natord::compare_ignore_case(x, y),
                };
                if desc {
                    o.reverse()
                } else {
                    o
                }
            });
        }
        out
    }

    /// Clic sur l'en-tête d'une colonne : tri croissant, puis décroissant.
    pub fn toggle_sort(&mut self, col: usize) {
        self.sort = match self.sort {
            Some((c, false)) if c == col => Some((col, true)),
            _ => Some((col, false)),
        };
        self.deadline = None;
    }

    /// Clic sur une ligne : choix simple → cette ligne seule ; multiple → bascule.
    pub fn click(&mut self, row: usize) {
        if self.spec.multi {
            self.selected[row] = !self.selected[row];
        } else {
            self.selected.iter_mut().for_each(|s| *s = false);
            self.selected[row] = true;
        }
        self.current = Some(row);
        self.deadline = None;
    }

    /// Lignes choisies (rangs d'origine, croissants).
    pub fn picked(&self) -> Vec<usize> {
        self.selected.iter().enumerate().filter(|(_, s)| **s).map(|(i, _)| i).collect()
    }

    /// Réponse possible (au moins une ligne en choix simple) ?
    pub fn can_validate(&self) -> bool {
        self.spec.multi || self.selected.iter().any(|s| *s)
    }

    /// Secondes restantes avant la validation automatique.
    pub fn remaining(&self) -> Option<u64> {
        self.deadline.map(|d| d.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64)
    }

    /// Délai écoulé : la sélection actuelle est validée (`Some(None)` si rien de choisi en
    /// choix simple : le script reçoit `None`).
    pub fn expired(&self) -> Option<Option<Vec<usize>>> {
        let d = self.deadline?;
        (Instant::now() >= d).then(|| self.can_validate().then(|| self.picked()))
    }

    /// Texture de la vignette de la ligne `i` (créée à la première demande).
    pub fn texture(&mut self, ctx: &egui::Context, i: usize) -> Option<TextureHandle> {
        if self.textures.get(i)?.is_none() {
            let img = self.images.get_mut(i)?.take()?;
            self.textures[i] = Some(ctx.load_texture(format!("script-choix-{i}"), img, egui::TextureOptions::LINEAR));
        }
        self.textures[i].clone()
    }
}

fn num(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    t.parse::<f64>().ok().filter(|f| f.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec(v: Value) -> FormSpec {
        FormSpec::parse(v.as_object().unwrap()).unwrap()
    }

    #[test]
    fn champs_aller_retour() {
        let f = spec(json!({"controls": [
            {"type": "text", "key": "t", "default": "a"},
            {"type": "checkbox", "key": "c", "default": true},
            {"type": "number", "key": "n", "default": 2.25, "decimals": 1},
            {"type": "choice", "key": "ch", "choices": [[1, "Un"], [2, "Deux"]], "default": 2},
            {"type": "choice", "key": "ed", "choices": ["x"], "editable": true, "default": "libre"},
            {"type": "radio", "key": "r", "choices": ["a", "b"]},
            {"type": "multichoice", "key": "m", "choices": ["a", "b", "c"], "default": ["c", "a"]},
            {"type": "list_edit", "key": "l", "default": ["p", "q"]},
            {"type": "table", "key": "tb", "columns": [{"title": "N", "type": "number"},
                {"title": "C", "type": "choice", "choices": ["est", "n'est pas"]}],
             "default": [[3, "n'est pas"]]},
            {"type": "file", "key": "f", "default": "/x.txt"},
        ]}));
        let st = FormState::new(f.clone(), HashMap::new());
        let v = st.values();
        assert_eq!(v["t"], "a");
        assert_eq!(v["c"], true);
        assert_eq!(v["n"], 2.3);
        assert_eq!(v["ch"], 2);
        assert_eq!(v["ed"], "libre");
        assert_eq!(v["r"], "a");
        assert_eq!(v["m"], json!(["a", "c"]));
        assert_eq!(v["l"], json!(["p", "q"]));
        assert_eq!(v["tb"], json!([[3, "n'est pas"]]));
        assert_eq!(v["f"], "/x.txt");
        // Les valeurs de l'interface repassent telles quelles la remise en forme du cœur.
        assert_eq!(f.sanitize(&v), v);
        assert_eq!(st.fields["ch"], Field::Choice { index: Some(1), text: "Deux".into() });
        // Liste éditable : texte égal à un libellé → valeur du choix.
        let mut st = st;
        st.fields.insert("ed".into(), Field::Choice { index: None, text: "x".into() });
        assert_eq!(st.values()["ed"], "x");
    }

    #[test]
    fn validation_et_onglet_de_l_erreur() {
        let f = spec(json!({"controls": [
            {"type": "tabs", "tabs": [
                {"title": "A", "controls": [{"type": "text", "key": "a"}]},
                {"title": "B", "controls": [
                    {"type": "checkbox", "key": "on", "default": true},
                    {"type": "number", "key": "n", "min": 1, "max": 5, "default": 3, "enabled_if": "on"},
                    {"type": "text", "key": "req", "required": true},
                ]},
            ]},
            {"type": "button", "key": "ajout", "label": "Ajouter"},
            {"type": "button", "key": "verif", "label": "Vérifier", "validate": true},
        ], "buttons": ["Enregistrer", "Fermer"]}));
        let tabs_id = f.controls[0].id;
        let mut st = FormState::new(f, HashMap::new());
        // Annuler et bouton sans validation : toujours possibles.
        assert!(st.submit("Fermer").is_some());
        assert_eq!(st.submit("ajout").unwrap().button, "ajout");
        assert!(st.submit("verif").is_none());
        assert!(st.submit("Enregistrer").is_none());
        assert_eq!(st.errors["req"], "champ obligatoire");
        assert_eq!(st.tabs[&tabs_id], 1, "onglet de l'erreur ouvert");
        st.fields.insert("req".into(), Field::Text("ok".into()));
        st.fields.insert("n".into(), Field::Number(9.0));
        assert!(st.submit("Enregistrer").is_none());
        assert!(st.errors["n"].contains("au plus 5"));
        // Case décochée : le nombre n'est plus vérifié.
        st.fields.insert("on".into(), Field::Bool(false));
        let a = st.submit("Enregistrer").unwrap();
        assert_eq!(a.values["n"], 9);
        assert!(st.errors.is_empty());
        assert_eq!(st.label_of("req"), "req");
    }

    #[test]
    fn outils_de_liste_et_insertion() {
        let mut items = vec!["A".to_string()];
        assert!(!list_add(&mut items, " a ", true));
        assert!(list_add(&mut items, " a ", false));
        assert!(!list_add(&mut items, "  ", false));
        assert!(list_add(&mut items, "B", true));
        assert_eq!(items, ["A", "a", "B"]);
        assert_eq!(move_item(&mut items, 2, true), 1);
        assert_eq!(items, ["A", "B", "a"]);
        assert_eq!(move_item(&mut items, 0, true), 0);
        assert_eq!(move_item(&mut items, 2, false), 2);
        let mut t = "Série #".to_string();
        assert_eq!(insert_at(&mut t, Some(6), "{n}"), 9);
        assert_eq!(t, "Série {n}#");
        insert_at(&mut t, None, "!");
        assert_eq!(t, "Série {n}#!");
    }

    #[test]
    fn liste_de_choix_filtre_tri_et_delai() {
        let c = ChooseSpec::parse(
            json!({"columns": ["Titre", "N°"], "rows": [["Tome 10", "10"], ["Tome 2", "2"], ["Hors-série", "1,5"]],
                   "default": 1, "timeout": 0.05})
            .as_object()
            .unwrap(),
        )
        .unwrap();
        let mut st = ChooseState::new(c, vec![]);
        assert_eq!(st.picked(), [1]);
        assert_eq!(st.view(), [0, 1, 2]);
        st.filter = "TOME".into();
        assert_eq!(st.view(), [0, 1]);
        st.filter = "tome 1".into();
        assert_eq!(st.view(), [0]);
        st.filter.clear();
        assert!(st.remaining().is_some());
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(st.expired(), Some(Some(vec![1])));
        st.toggle_sort(1);
        assert!(st.deadline.is_none(), "une action arrête le compte à rebours");
        assert_eq!(st.view(), [2, 1, 0], "tri numérique (1,5 < 2 < 10)");
        st.toggle_sort(1);
        assert_eq!(st.view(), [0, 1, 2]);
        st.toggle_sort(0);
        assert_eq!(st.view(), [2, 1, 0], "tri naturel : Hors-série, Tome 2, Tome 10");
        st.click(0);
        assert_eq!(st.picked(), [0]);

        let m = ChooseSpec::parse(json!({"rows": ["a", "b"], "multi": true}).as_object().unwrap()).unwrap();
        let mut st = ChooseState::new(m, vec![]);
        assert!(st.can_validate());
        st.click(1);
        st.click(0);
        st.click(1);
        assert_eq!(st.picked(), [0]);
    }

    #[test]
    fn decodage_des_images_borne() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_pixel(1000, 500, image::Rgba([200, 10, 10, 255]));
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let c = decode_image(&png, 100).unwrap();
        assert_eq!(c.size, [100, 50]);
        assert!(decode_image(b"\x89PNG\r\n\x1a\n0000IHDR", 100).is_none());
        // En-tête PNG annonçant 100 000 × 100 000 : refusé sans décoder.
        let mut huge = png.clone();
        huge[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        huge[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        assert!(decode_image(&huge, 100).is_none());
    }
}
