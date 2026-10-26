//! Fiche en cours de modification dans la fenêtre « Infos du livre » (v0.10.1), sans
//! interface : valeurs affichées, valeurs multiples, champs modifiés, validation,
//! modifications à écrire, résultats d'un script `Editor`, suggestions.
//!
//! Le texte de chaque champ est celui que l'utilisateur voit et modifie ; il est converti
//! en [`FieldEdit`] (texte, validé et normalisé par `comicrust_core::edit` : « 4,5 » pour
//! une note, « 17/05/2024 » pour une date, « Oui » / « Non » pour un booléen…). Un champ
//! vide vide la fiche.
//!
//! Plusieurs livres (comme `MultipleComicBooksDialog` de ComicRack) : un champ dont les
//! valeurs diffèrent est « (valeurs multiples) » et n'est écrit que si l'utilisateur le
//! change (ou le vide explicitement) ; les champs non touchés ne sont jamais réécrits.

use comicrust_core::db::ComicDatabase;
use comicrust_core::edit::{self, EditValue, FieldDef, FieldEdit, FieldKind, CUSTOM_VALUE_PREFIX};
use comicrust_core::library::LibraryBook;

/// Valeur d'origine d'un champ sur les livres modifiés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Original {
    /// Même valeur (texte affiché) pour tous les livres.
    Same(String),
    /// Valeurs différentes selon les livres.
    Mixed,
}

/// Champ du formulaire.
#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    /// Nom ComicRack (`Series`) ou `CustomValue:<clé>`.
    pub name: String,
    pub original: Original,
    /// Texte saisi.
    pub text: String,
    /// Le champ sera écrit.
    pub touched: bool,
    /// Valeurs multiples vidées explicitement (bouton « vider »).
    pub clear_all: bool,
}

impl FormField {
    /// Recalcule `touched` après une saisie.
    pub fn update(&mut self) {
        if !self.text.is_empty() {
            self.clear_all = false;
        }
        self.touched = match &self.original {
            Original::Same(o) => self.text != *o,
            Original::Mixed => !self.text.is_empty() || self.clear_all,
        };
    }

    /// Valeur d'origine (texte vide si multiple), sans modification.
    pub fn revert(&mut self) {
        self.text = match &self.original {
            Original::Same(o) => o.clone(),
            Original::Mixed => String::new(),
        };
        self.clear_all = false;
        self.touched = false;
    }

    /// Vide le champ (pour tous les livres).
    pub fn clear(&mut self) {
        self.text.clear();
        self.clear_all = matches!(self.original, Original::Mixed);
        self.update();
    }

    pub fn mixed(&self) -> bool {
        self.original == Original::Mixed
    }

    /// Clé d'une valeur personnalisée.
    pub fn custom_key(&self) -> Option<&str> {
        self.name.strip_prefix(CUSTOM_VALUE_PREFIX)
    }

    /// Modification de ce champ pour le livre `id`.
    pub fn edit_for(&self, id: &str) -> FieldEdit {
        let value = (!self.text.trim().is_empty()).then(|| EditValue::Text(self.text.clone()));
        FieldEdit { book_id: id.to_string(), field: self.name.clone(), value }
    }

    /// Message d'erreur de la valeur saisie (champ modifié seulement).
    pub fn error(&self) -> Option<String> {
        if !self.touched {
            return None;
        }
        // Identifiant quelconque : seule la valeur est vérifiée.
        let e = edit::validate(&self.edit_for("verification")).err()?;
        // « « Titre » : message » → « message » (le libellé est déjà à côté du champ).
        let m = e.message;
        Some(match m.find("» : ") {
            Some(i) if m.starts_with('«') => m[i + "» : ".len()..].to_string(),
            _ => m,
        })
    }
}

/// Texte affiché d'un champ du catalogue pour `b`.
pub fn display(def: &FieldDef, b: &LibraryBook) -> String {
    let v = def.get(b);
    match (def.kind, v) {
        (FieldKind::Bool { default }, v) => {
            let on = match v {
                Some(EditValue::Bool(x)) => x,
                _ => default,
            };
            if on { "Oui" } else { "Non" }.to_string()
        }
        (_, None) => String::new(),
        (FieldKind::Date, Some(EditValue::Text(t))) => match (t.get(0..4), t.get(5..7), t.get(8..10)) {
            (Some(y), Some(m), Some(d)) if !t.starts_with("0001-01-01") => format!("{d}/{m}/{y}"),
            _ => String::new(),
        },
        (FieldKind::YesNo | FieldKind::Manga, Some(EditValue::Text(t))) if t.eq_ignore_ascii_case("unknown") => {
            String::new()
        }
        (_, Some(EditValue::Real(f))) => {
            let s = format!("{}", f as f32);
            s.replace('.', ",")
        }
        _ => def.text(b),
    }
}

/// Texte d'un champ (catalogue ou valeur personnalisée) pour `b`.
pub fn field_display(name: &str, b: &LibraryBook) -> String {
    match name.strip_prefix(CUSTOM_VALUE_PREFIX) {
        Some(key) => edit::custom_value(b.custom_values_store.as_deref(), key).unwrap_or_default(),
        None => edit::field(name).map(|d| display(d, b)).unwrap_or_default(),
    }
}

/// Champs du catalogue présentés dans le formulaire (tous sauf le texte brut des valeurs
/// personnalisées, remplacé par l'onglet « Personnalisé »).
pub fn form_defs() -> impl Iterator<Item = &'static FieldDef> {
    edit::FIELDS.iter().filter(|d| d.kind != FieldKind::CustomValues)
}

/// Onglets de la fenêtre et leurs champs, dans l'ordre (d'après `ComicBookDialog` de
/// ComicRack). Les valeurs personnalisées ont leur onglet à part (« Personnalisé »).
pub const TABS: &[(&str, &[&str])] = &[
    (
        "Résumé",
        &[
            "Series",
            "Number",
            "Count",
            "Volume",
            "Title",
            "AlternateSeries",
            "AlternateNumber",
            "AlternateCount",
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
            "Rating",
        ],
    ),
    (
        "Détails",
        &[
            "Publisher",
            "Imprint",
            "Genre",
            "Format",
            "AgeRating",
            "LanguageISO",
            "BlackAndWhite",
            "Manga",
            "SeriesComplete",
            "StoryArc",
            "SeriesGroup",
            "Tags",
            "CommunityRating",
            "Web",
            "PreferredFrontCover",
            "EnableProposed",
        ],
    ),
    (
        "Intrigue et notes",
        &["Characters", "Teams", "MainCharacterOrTeam", "Locations", "ScanInformation", "Summary", "Notes", "Review"],
    ),
    (
        "Catalogue",
        &[
            "BookStore",
            "BookOwner",
            "BookAge",
            "BookCondition",
            "BookCollectionStatus",
            "BookPrice",
            "BookLocation",
            "ISBN",
            "Released",
            "Checked",
            "BookNotes",
        ],
    ),
];

/// Fiche(s) en cours de modification.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoForm {
    /// Livres modifiés (identifiants tels que dans la bibliothèque).
    pub ids: Vec<String>,
    pub fields: Vec<FormField>,
}

impl InfoForm {
    /// Formulaire des livres `books` (au moins un).
    pub fn new(books: &[&LibraryBook]) -> InfoForm {
        let original = |name: &str| {
            let mut values = books.iter().map(|b| field_display(name, b));
            let first = values.next().unwrap_or_default();
            if values.all(|v| v == first) {
                Original::Same(first)
            } else {
                Original::Mixed
            }
        };
        let mut fields: Vec<FormField> = form_defs()
            .map(|d| {
                let o = original(d.name);
                FormField { name: d.name.to_string(), text: text_of(&o), original: o, touched: false, clear_all: false }
            })
            .collect();
        // Valeurs personnalisées : toutes les clés des livres (sans casse), triées.
        let mut keys: Vec<String> = Vec::new();
        for b in books {
            for (k, _) in edit::custom_values(b.custom_values_store.as_deref()) {
                if !keys.iter().any(|x| x.to_lowercase() == k.to_lowercase()) {
                    keys.push(k);
                }
            }
        }
        keys.sort_by_key(|k| k.to_lowercase());
        for k in keys {
            let name = format!("{CUSTOM_VALUE_PREFIX}{k}");
            let o = original(&name);
            fields.push(FormField { name, text: text_of(&o), original: o, touched: false, clear_all: false });
        }
        InfoForm { ids: books.iter().map(|b| b.id.clone()).collect(), fields }
    }

    pub fn multiple(&self) -> bool {
        self.ids.len() > 1
    }

    pub fn get(&self, name: &str) -> Option<&FormField> {
        self.fields.iter().find(|f| f.name.eq_ignore_ascii_case(name))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut FormField> {
        self.fields.iter_mut().find(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// Saisit `text` dans le champ `name`.
    #[cfg(test)]
    pub fn set(&mut self, name: &str, text: &str) {
        if let Some(f) = self.get_mut(name) {
            f.text = text.to_string();
            f.update();
        }
    }

    /// Valeurs personnalisées du formulaire.
    pub fn custom_fields(&self) -> impl Iterator<Item = &FormField> {
        self.fields.iter().filter(|f| f.custom_key().is_some())
    }

    /// Ajoute une valeur personnalisée (vide) ; `Err` : clé refusée.
    pub fn add_custom(&mut self, key: &str) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("nom vide".into());
        }
        if let Some(c) = key.chars().find(|c| matches!(c, ',' | '=') || c.is_control()) {
            return Err(format!("caractère « {c} » interdit dans le nom"));
        }
        if self.custom_fields().any(|f| f.custom_key().is_some_and(|k| k.to_lowercase() == key.to_lowercase())) {
            return Err(format!("« {key} » existe déjà"));
        }
        self.fields.push(FormField {
            name: format!("{CUSTOM_VALUE_PREFIX}{key}"),
            original: Original::Same(String::new()),
            text: String::new(),
            touched: false,
            clear_all: false,
        });
        Ok(())
    }

    /// Quelque chose à écrire ?
    pub fn dirty(&self) -> bool {
        self.fields.iter().any(|f| f.touched)
    }

    /// Champs modifiés dont la valeur est refusée : (nom, message).
    pub fn errors(&self) -> Vec<(String, String)> {
        self.fields.iter().filter_map(|f| f.error().map(|e| (f.name.clone(), e))).collect()
    }

    /// Modifications à écrire : chaque champ modifié, pour chaque livre.
    pub fn edits(&self) -> Vec<FieldEdit> {
        let mut out = Vec::new();
        for id in &self.ids {
            out.extend(self.fields.iter().filter(|f| f.touched).map(|f| f.edit_for(id)));
        }
        out
    }

    /// Reporte dans le formulaire la valeur `value` qu'un script `Editor` a donnée au champ
    /// `field` du livre `book` (rien n'est écrit). La valeur passe par les conversions de
    /// `comicrust_core::edit` ; refusée, elle est mise telle quelle (l'erreur s'affiche
    /// sous le champ). Rend le nom du champ rempli, `Err` pour un champ inconnu.
    pub fn apply_script_value(
        &mut self,
        book: &LibraryBook,
        field: &str,
        value: &serde_json::Value,
    ) -> Result<String, String> {
        let fe = comicrust_core::scripting::field_edit(&book.id, field, value);
        let custom = fe.field.strip_prefix(CUSTOM_VALUE_PREFIX).map(str::to_string);
        let name = match &custom {
            Some(k) => format!("{CUSTOM_VALUE_PREFIX}{k}"),
            None => match edit::field(&fe.field) {
                Some(d) if d.kind != FieldKind::CustomValues => d.name.to_string(),
                _ => return Err(format!("champ « {field} » non modifiable dans la fenêtre")),
            },
        };
        let mut after = book.clone();
        let text = match edit::apply_to_book(&mut after, &fe) {
            Ok(_) => field_display(&name, &after),
            Err(_) => match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => String::new(),
                v => v.to_string(),
            },
        };
        if self.get(&name).is_none() {
            if let Some(k) = &custom {
                self.add_custom(k)?;
            }
        }
        let f = self.get_mut(&name).ok_or_else(|| format!("champ « {field} » absent"))?;
        f.text = text;
        if f.text.is_empty() && f.mixed() {
            f.clear_all = true;
        }
        f.update();
        Ok(name)
    }
}

fn text_of(o: &Original) -> String {
    match o {
        Original::Same(s) => s.clone(),
        Original::Mixed => String::new(),
    }
}

// ---------------------------------------------------------------------------------------
// Suggestions
// ---------------------------------------------------------------------------------------

/// Champs qui proposent les valeurs déjà présentes dans la bibliothèque.
pub fn has_suggestions(def: &FieldDef) -> bool {
    matches!(def.kind, FieldKind::Text | FieldKind::List | FieldKind::Language)
        && !matches!(def.name, "Title" | "Number" | "AlternateNumber" | "Web" | "ISBN")
}

/// Valeurs existantes du champ `def` dans la bibliothèque (chaque valeur d'une liste à
/// part), sans doublon (casse ignorée), triées ; plus les valeurs usuelles du champ.
pub fn suggestions(db: &ComicDatabase, def: &FieldDef) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut add = |v: &str| {
        let v = v.trim();
        if !v.is_empty() && v != "Unknown" && seen.insert(v.to_lowercase()) {
            out.push(v.to_string());
        }
    };
    for s in def.suggestions() {
        add(s);
    }
    for b in &db.books {
        let t = def.text(b);
        if def.kind == FieldKind::List {
            t.split(',').for_each(&mut add);
        } else {
            add(&t);
        }
    }
    out.sort_by(|a, b| natord::compare_ignore_case(a, b));
    out
}

/// Partie en cours de saisie : tout le texte, ou la dernière valeur d'une liste.
pub fn current_token(text: &str, list: bool) -> &str {
    let t = if list { text.rsplit(',').next().unwrap_or(text) } else { text };
    t.trim_start()
}

/// Suggestions pour `token` : celles qui commencent par lui, puis celles qui le
/// contiennent (casse ignorée), au plus `max` ; rien pour une saisie vide ou exacte.
pub fn matching<'a>(token: &str, all: &'a [String], max: usize) -> Vec<&'a str> {
    let t = token.trim().to_lowercase();
    if t.is_empty() {
        return Vec::new();
    }
    let lower: Vec<String> = all.iter().map(|s| s.to_lowercase()).collect();
    if lower.contains(&t) && lower.iter().filter(|s| s.starts_with(&t)).count() == 1 {
        return Vec::new();
    }
    let starts = all.iter().zip(&lower).filter(|(_, l)| l.starts_with(&t)).map(|(s, _)| s.as_str());
    let contains =
        all.iter().zip(&lower).filter(|(_, l)| !l.starts_with(&t) && l.contains(&t)).map(|(s, _)| s.as_str());
    starts.chain(contains).take(max).collect()
}

/// Texte après le choix de `choice` : remplace la partie en cours de saisie.
pub fn accept(text: &str, list: bool, choice: &str) -> String {
    if !list {
        return choice.to_string();
    }
    match text.rfind(',') {
        Some(i) => format!("{}, {choice}", text[..i].trim_end()),
        None => choice.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: &str) -> LibraryBook {
        LibraryBook { id: id.into(), file: Some(format!("Z:\\{id}.cbz")), ..Default::default() }
    }

    fn two() -> (LibraryBook, LibraryBook) {
        let mut a = book("a");
        a.series = Some("Thanos".into());
        a.number = Some("1".into());
        a.writer = Some("Jim Starlin".into());
        a.rating = Some(4.5);
        a.released = Some("2024-05-17T00:00:00+02:00".into());
        a.manga = Some("Unknown".into());
        a.custom_values_store = Some(",Source=Bedetheque".into());
        let mut b = a.clone();
        b.id = "b".into();
        b.number = Some("2".into());
        b.checked = Some(false);
        b.custom_values_store = Some(",Note perso=bof,source=Bedetheque".into());
        (a, b)
    }

    #[test]
    fn every_editable_field_is_in_exactly_one_tab() {
        let mut placed: Vec<&str> = TABS.iter().flat_map(|(_, f)| f.iter().copied()).collect();
        for n in &placed {
            assert!(edit::field(n).is_some_and(|d| d.name == *n), "{n} : nom inconnu");
        }
        placed.sort();
        let mut all: Vec<&str> = form_defs().map(|d| d.name).collect();
        all.sort();
        assert_eq!(placed, all);
    }

    #[test]
    fn display_texts_are_editable_texts() {
        let (a, b) = two();
        let f = InfoForm::new(&[&a]);
        let t = |n: &str| f.get(n).unwrap().text.clone();
        assert_eq!(t("Series"), "Thanos");
        assert_eq!(t("Rating"), "4,5");
        assert_eq!(t("Released"), "17/05/2024");
        assert_eq!(t("Manga"), "", "Unknown = vide");
        assert_eq!(t("Checked"), "Oui", "booléen absent : valeur par défaut");
        assert_eq!(field_display("Checked", &b), "Non");
        assert_eq!(t("Count"), "");
        assert!(f.get("CustomValuesStore").is_none(), "texte brut remplacé par l'onglet Personnalisé");
        assert_eq!(t("CustomValue:Source"), "Bedetheque");
        // Un champ non modifié n'écrit rien, même affiché autrement que stocké.
        assert!(!f.dirty() && f.edits().is_empty());
    }

    #[test]
    fn every_untouched_displayed_value_round_trips() {
        // La valeur affichée, réécrite telle quelle, ne change pas la fiche.
        let (mut a, _) = two();
        // (« Unknown » écrit en toutes lettres par ComicRack s'affiche vide : le réécrire
        // vide le champ, ce qui ne se produit que si l'utilisateur le modifie.)
        a.manga = None;
        let f = InfoForm::new(&[&a]);
        for field in &f.fields {
            let mut after = a.clone();
            let changed = edit::apply_to_book(&mut after, &field.edit_for("a"));
            assert_eq!(changed, Ok(false), "{} : « {} »", field.name, field.text);
        }
    }

    #[test]
    fn several_books_show_mixed_values_and_only_write_touched_fields() {
        let (a, b) = two();
        let mut f = InfoForm::new(&[&a, &b]);
        assert!(f.multiple());
        assert_eq!(f.get("Series").unwrap().original, Original::Same("Thanos".into()));
        let n = f.get("Number").unwrap();
        assert_eq!((n.mixed(), n.text.as_str()), (true, ""));
        assert!(f.get("Checked").unwrap().mixed());
        // Clés personnalisées réunies sans casse : « Source » commune, « Note perso » d'un seul.
        let keys: Vec<&str> = f.custom_fields().filter_map(|c| c.custom_key()).collect();
        assert_eq!(keys, ["Note perso", "Source"]);
        assert!(f.get("CustomValue:Note perso").unwrap().mixed());
        assert!(!f.get("CustomValue:Source").unwrap().mixed());

        // Saisir puis effacer une valeur multiple : rien à écrire.
        f.set("Number", "7");
        assert!(f.get("Number").unwrap().touched);
        f.set("Number", "");
        assert!(!f.dirty());
        // Vider explicitement : écrit (vide) pour les deux livres.
        f.get_mut("Number").unwrap().clear();
        let e = f.edits();
        assert_eq!(e.len(), 2);
        assert!(e.iter().all(|x| x.field == "Number" && x.value.is_none()));
        f.get_mut("Number").unwrap().revert();
        assert!(!f.dirty());

        f.set("Writer", "Jim Starlin, Ron Lim");
        let e = f.edits();
        let ids: Vec<&str> = e.iter().map(|x| x.book_id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert!(e.iter().all(|x| x.field == "Writer"));
        // Même texte que l'origine : plus modifié.
        f.set("Writer", "Jim Starlin");
        assert!(!f.dirty());
    }

    #[test]
    fn values_are_validated_field_by_field() {
        let (a, _) = two();
        let mut f = InfoForm::new(&[&a]);
        f.set("Year", "deux mille");
        f.set("Month", "13");
        f.set("Rating", "3,5");
        f.set("Released", "31/02/2024");
        f.set("LanguageISO", "français");
        let errors: Vec<String> = f.errors().into_iter().map(|(n, _)| n).collect();
        assert_eq!(errors, ["Year", "Month", "LanguageISO", "Released"], "ordre du catalogue");
        let (_, m) = &f.errors()[1];
        assert!(m.starts_with("13 hors limites"), "libellé retiré : {m}");
        f.set("Year", "1990");
        f.set("Month", "");
        f.set("Released", "01/02/2024");
        f.set("LanguageISO", "FR");
        assert!(f.errors().is_empty());
        // Les modifications passent la validation du cœur et s'appliquent.
        let mut after = a.clone();
        for e in f.edits() {
            edit::apply_to_book(&mut after, &e).unwrap();
        }
        assert_eq!((after.year, after.month, after.rating), (Some(1990), None, Some(3.5)));
        assert_eq!(after.language_iso.as_deref(), Some("fr"));
        assert_eq!(field_display("Released", &after), "01/02/2024");
        assert_eq!(after.comic_info_is_dirty, Some(true));
    }

    #[test]
    fn custom_values_can_be_added_changed_and_removed() {
        let (a, _) = two();
        let mut f = InfoForm::new(&[&a]);
        assert!(f.add_custom("source").is_err(), "existe déjà (casse ignorée)");
        assert!(f.add_custom("a=b").is_err());
        assert!(f.add_custom("  ").is_err());
        f.add_custom("Lu en").unwrap();
        f.set("CustomValue:Lu en", "2025");
        f.get_mut("CustomValue:Source").unwrap().clear();
        let mut after = a.clone();
        for e in f.edits() {
            edit::apply_to_book(&mut after, &e).unwrap();
        }
        assert_eq!(after.custom_values_store.as_deref(), Some(",Lu en=2025"));
    }

    #[test]
    fn script_values_fill_the_form_without_writing() {
        let (a, _) = two();
        let mut f = InfoForm::new(&[&a]);
        use serde_json::json;
        assert_eq!(f.apply_script_value(&a, "Series", &json!("Thanos Quest")), Ok("Series".into()));
        assert_eq!(f.apply_script_value(&a, "year", &json!(1990)), Ok("Year".into()));
        assert_eq!(f.apply_script_value(&a, "Rating", &json!(3.5)), Ok("Rating".into()));
        assert_eq!(f.apply_script_value(&a, "Manga", &json!(true)), Ok("Manga".into()));
        assert_eq!(f.apply_script_value(&a, "Month", &json!(14)), Ok("Month".into()));
        assert_eq!(f.apply_script_value(&a, "CustomValue:Album", &json!("12")), Ok("CustomValue:Album".into()));
        assert!(f.apply_script_value(&a, "Inconnu", &json!("x")).is_err());
        let t = |f: &InfoForm, n: &str| f.get(n).unwrap().text.clone();
        assert_eq!(t(&f, "Series"), "Thanos Quest");
        assert_eq!(t(&f, "Year"), "1990");
        assert_eq!(t(&f, "Rating"), "3,5");
        assert_eq!(t(&f, "Manga"), "Yes");
        assert_eq!(t(&f, "Month"), "14", "valeur refusée gardée telle quelle…");
        assert_eq!(f.errors().len(), 1, "… et signalée");
        assert_eq!(t(&f, "CustomValue:Album"), "12");
        assert!(f.get("CustomValue:Album").unwrap().touched);
    }

    #[test]
    fn suggestions_come_from_the_library() {
        let mut a = book("a");
        a.writer = Some("Jim Starlin, Ron Lim".into());
        a.publisher = Some("Marvel".into());
        let mut b = book("b");
        b.writer = Some("jim starlin, Jason Aaron".into());
        b.publisher = Some("Dargaud".into());
        let db = ComicDatabase::from_books(vec![a, b]);
        let w = suggestions(&db, edit::field("Writer").unwrap());
        assert_eq!(w, ["Jason Aaron", "Jim Starlin", "Ron Lim"]);
        assert_eq!(suggestions(&db, edit::field("Publisher").unwrap()), ["Dargaud", "Marvel"]);
        let age = suggestions(&db, edit::field("AgeRating").unwrap());
        assert!(age.contains(&"Teen".to_string()) && !age.contains(&"Unknown".to_string()));
        assert!(has_suggestions(edit::field("Writer").unwrap()));
        assert!(!has_suggestions(edit::field("Title").unwrap()));
        assert!(!has_suggestions(edit::field("Year").unwrap()));
    }

    #[test]
    fn completion_of_the_value_being_typed() {
        let all: Vec<String> = ["Jason Aaron", "Jim Starlin", "Ron Lim"].iter().map(|s| s.to_string()).collect();
        assert_eq!(current_token("Jim Starlin, ro", true), "ro");
        assert_eq!(current_token("Jim Starlin, ro", false), "Jim Starlin, ro");
        assert_eq!(matching("ro", &all, 10), ["Ron Lim", "Jason Aaron"], "commence par, puis contient");
        assert_eq!(matching("li", &all, 10), ["Jim Starlin", "Ron Lim"], "contient");
        assert_eq!(matching("j", &all, 1), ["Jason Aaron"], "au plus max");
        assert!(matching("", &all, 10).is_empty());
        assert!(matching("ron lim", &all, 10).is_empty(), "déjà complet");
        assert_eq!(accept("Jim Starlin, ro", true, "Ron Lim"), "Jim Starlin, Ron Lim");
        assert_eq!(accept("ro", true, "Ron Lim"), "Ron Lim");
        assert_eq!(accept("Mar", false, "Marvel"), "Marvel");
    }
}
