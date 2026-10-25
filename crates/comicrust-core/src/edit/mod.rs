//! Modification des fiches et des fichiers : API commune aux scripts et à l'interface.
//!
//! - [`catalog`] : champs modifiables ([`FIELDS`], nom public = élément ComicRack),
//!   types, validation ;
//! - [`FieldEdit`] : une modification (`livre`, `champ`, `valeur` ; `None` = vider) ;
//! - [`apply_to_book`] : aperçu sans rien écrire ;
//! - [`apply_edits`] : écriture dans la base, selon le mode de la bibliothèque
//!   ([`EditTarget`]) — base ComicRust ([`Store::apply_edits`]) ou base MySQL de
//!   ComicRack ([`apply_edits_comicrack`]) ; en lecture seule, tout est refusé ;
//! - [`files`] : déplacement / renommage d'un fichier ([`rename_or_move`]) et modèles de
//!   nom ([`files::expand_template`]).
//!
//! # Règles
//!
//! - Les modifications d'un même livre sont appliquées dans l'ordre, ensemble : si l'une
//!   d'elles est invalide, **aucune** n'est appliquée à ce livre (erreur par champ dans
//!   [`EditReport::errors`]) ; les autres livres sont écrits.
//! - Une valeur égale à la valeur actuelle n'écrit rien (livre dans `unchanged`).
//! - Modifier un champ du ComicInfo.xml ([`FieldDef::in_comic_info`]) met
//!   `ComicInfoIsDirty` à vrai, comme ComicRack (le fichier est « à mettre à jour »).
//!   `ComicBookIsDirty` (infos de bibliothèque à écrire dans le fichier, option de
//!   ComicRack CE) n'est pas touché : ComicRust n'écrit pas ces infos dans les fichiers.
//! - Valeurs personnalisées : champ `CustomValue:<clé>` ([`FieldEdit::custom_value`]),
//!   appliqué sur la valeur **en base** au moment de l'écriture.

pub mod catalog;
pub mod files;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::HashMap;

pub use catalog::{
    custom_value, custom_values, field, set_custom_value, EditValue, FieldDef, FieldKind, Norm, FIELDS,
    READ_ONLY_FIELDS,
};
pub use files::{rename_or_move, MoveReport};

use crate::db::{BookEdit, DbConfig, Value, WriteReport};
use crate::library::LibraryBook;
use crate::store::Store;
use crate::Error;

/// Préfixe du nom de champ d'une valeur personnalisée : `CustomValue:<clé>`.
pub const CUSTOM_VALUE_PREFIX: &str = "CustomValue:";

/// Une modification de fiche.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldEdit {
    /// Identifiant du livre (GUID, casse indifférente).
    pub book_id: String,
    /// Nom du champ ([`catalog::field`]) ou `CustomValue:<clé>`.
    pub field: String,
    /// Nouvelle valeur ; `None` vide le champ (valeur par défaut de ComicRack).
    pub value: Option<EditValue>,
}

impl FieldEdit {
    pub fn set(book_id: &str, field: &str, value: impl Into<EditValue>) -> Self {
        Self { book_id: book_id.to_string(), field: field.to_string(), value: Some(value.into()) }
    }

    pub fn clear(book_id: &str, field: &str) -> Self {
        Self { book_id: book_id.to_string(), field: field.to_string(), value: None }
    }

    /// Valeur personnalisée (`ComicBook.SetCustomValue`) ; `None` ou vide la supprime.
    pub fn custom_value(book_id: &str, key: &str, value: Option<&str>) -> Self {
        Self {
            book_id: book_id.to_string(),
            field: format!("{CUSTOM_VALUE_PREFIX}{key}"),
            value: value.map(|v| EditValue::Text(v.to_string())),
        }
    }
}

/// Erreur sur une modification (ou sur un livre entier si `field` est vide).
#[derive(Debug, Clone, PartialEq)]
pub struct EditError {
    /// Identifiant en minuscules.
    pub book_id: String,
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.field.is_empty() {
            write!(f, "livre {} : {}", self.book_id, self.message)
        } else {
            write!(f, "livre {}, {} : {}", self.book_id, self.field, self.message)
        }
    }
}

/// Champ visé par une modification validée.
#[derive(Debug, Clone)]
pub(crate) enum Target {
    Field(&'static FieldDef),
    Custom(String),
}

/// Modification validée.
#[derive(Debug, Clone)]
pub(crate) struct Valid {
    pub target: Target,
    pub value: Option<Norm>,
}

/// Valide une modification (sans livre) : `Ok` si elle peut être appliquée.
pub fn validate(edit: &FieldEdit) -> Result<(), EditError> {
    check(edit).map(|_| ())
}

fn check(edit: &FieldEdit) -> Result<Valid, EditError> {
    let err = |field: &str, message: String| EditError {
        book_id: edit.book_id.trim().to_ascii_lowercase(),
        field: field.to_string(),
        message,
    };
    if edit.book_id.trim().is_empty() {
        return Err(err(&edit.field, "identifiant de livre vide".into()));
    }
    let name = edit.field.trim();
    if let Some(key) = name.strip_prefix(CUSTOM_VALUE_PREFIX) {
        if key.is_empty() {
            return Err(err(name, "clé de valeur personnalisée vide".into()));
        }
        let value = match &edit.value {
            None => None,
            Some(EditValue::Text(s)) => Some(s.clone()),
            Some(EditValue::Int(i)) => Some(i.to_string()),
            Some(EditValue::Real(f)) => Some(f.to_string()),
            Some(EditValue::Bool(b)) => Some(b.to_string()),
            Some(EditValue::List(l)) => Some(l.join(", ")),
        };
        for s in std::iter::once(key).chain(value.as_deref()) {
            if let Some(c) = s.chars().find(|&c| !crate::lists_edit::xml::is_xml_char(c)) {
                return Err(err(name, format!("caractère interdit (U+{:04X})", c as u32)));
            }
        }
        return Ok(Valid { target: Target::Custom(key.to_string()), value: value.map(Norm::Text) });
    }
    let Some(def) = catalog::field(name) else {
        let why = READ_ONLY_FIELDS
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, why)| format!("champ non modifiable ({why})"))
            .unwrap_or_else(|| "champ inconnu".into());
        return Err(err(name, why));
    };
    match def.normalize(edit.value.as_ref()) {
        Ok(value) => Ok(Valid { target: Target::Field(def), value }),
        Err(m) => Err(err(def.name, format!("« {} » : {m}", def.label))),
    }
}

/// Applique une modification validée à `b` ; `true` si la fiche a changé.
pub(crate) fn apply_valid(b: &mut LibraryBook, v: &Valid) -> bool {
    let changed_ci = match &v.target {
        Target::Field(def) => def.store(b, v.value.as_ref()).then_some(def.in_comic_info),
        Target::Custom(key) => {
            let value = match &v.value {
                Some(Norm::Text(t)) => Some(t.as_str()),
                _ => None,
            };
            let new = set_custom_value(b.custom_values_store.as_deref(), key, value);
            // Comparaison sur le contenu décodé (l'ordre ou l'encodage d'origine peuvent
            // différer de celui de ComicRack sans que les valeurs changent).
            let same = custom_values(new.as_deref()).len() == custom_values(b.custom_values_store.as_deref()).len()
                && custom_values(new.as_deref())
                    .iter()
                    .all(|(k, v)| custom_value(b.custom_values_store.as_deref(), k).as_deref() == Some(v.as_str()));
            (!same).then(|| {
                b.custom_values_store = new;
                false
            })
        }
    };
    match changed_ci {
        None => false,
        Some(ci) => {
            if ci {
                b.comic_info_is_dirty = Some(true);
            }
            true
        }
    }
}

/// Aperçu : applique `edit` à `book` (en mémoire seulement). `Ok(true)` si la fiche a
/// changé (y compris `ComicInfoIsDirty` pour un champ du ComicInfo.xml).
pub fn apply_to_book(book: &mut LibraryBook, edit: &FieldEdit) -> Result<bool, EditError> {
    let v = check(edit)?;
    Ok(apply_valid(book, &v))
}

/// Modifications d'un livre, validées.
pub(crate) struct BookEdits {
    /// Identifiant en minuscules.
    pub id: String,
    pub edits: Vec<Valid>,
}

/// Valide et regroupe par livre (ordre de première apparition). Un livre dont une
/// modification est invalide est écarté (erreurs rendues).
pub(crate) fn prepare(edits: &[FieldEdit]) -> (Vec<BookEdits>, Vec<EditError>) {
    let mut groups: Vec<BookEdits> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut bad: Vec<String> = Vec::new();
    let mut errors = Vec::new();
    for e in edits {
        let id = e.book_id.trim().to_ascii_lowercase();
        match check(e) {
            Ok(v) => {
                let i = *index.entry(id.clone()).or_insert_with(|| {
                    groups.push(BookEdits { id: id.clone(), edits: Vec::new() });
                    groups.len() - 1
                });
                groups[i].edits.push(v);
            }
            Err(err) => {
                if !bad.contains(&id) {
                    bad.push(id);
                }
                errors.push(err);
            }
        }
    }
    groups.retain(|g| !bad.contains(&g.id));
    (groups, errors)
}

/// Bilan d'une écriture de modifications.
#[derive(Debug, Clone, Default)]
pub struct EditReport {
    /// Fiches modifiées, **relues** après écriture (pour mettre l'affichage à jour).
    pub changed: Vec<LibraryBook>,
    /// Livres déjà dans l'état demandé (identifiants en minuscules).
    pub unchanged: Vec<String>,
    /// Livres absents de la base.
    pub missing: Vec<String>,
    /// Livres modifiés ailleurs pendant l'écriture (ComicRack) : non écrits.
    pub conflicts: Vec<String>,
    /// Valeurs refusées et erreurs d'écriture.
    pub errors: Vec<EditError>,
    /// Base ComicRust : compteur des lignes écrites (pour [`crate::store::StoreWatcher::note_written`]).
    pub store_counter: Option<i64>,
    /// Base ComicRack : bilan brut (pour `Watcher::note_report` et
    /// `ComicDatabase::apply_written`).
    pub comicrack: Option<WriteReport>,
}

impl EditReport {
    /// Tout a été écrit (ou était déjà à jour).
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.conflicts.is_empty() && self.errors.is_empty()
    }

    /// Résumé pour l'utilisateur.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{} livre(s) modifié(s)", self.changed.len())];
        if !self.unchanged.is_empty() {
            parts.push(format!("{} déjà à jour", self.unchanged.len()));
        }
        if !self.missing.is_empty() {
            parts.push(format!("{} absent(s) de la base", self.missing.len()));
        }
        if !self.conflicts.is_empty() {
            parts.push(format!("{} modifié(s) ailleurs entre-temps", self.conflicts.len()));
        }
        if !self.errors.is_empty() {
            parts.push(format!("{} erreur(s)", self.errors.len()));
        }
        parts.join(", ")
    }
}

/// Où écrire, selon le mode de la bibliothèque.
pub enum EditTarget<'a> {
    /// Base ComicRust (mode autonome).
    Store(&'a mut Store),
    /// Base MySQL de ComicRack, écriture autorisée (`settings.comicrack_write`).
    ComicRack(&'a DbConfig),
    /// ComicRack MySQL sans écriture, ou `ComicDb.xml` : rien n'est modifiable.
    ReadOnly,
}

/// Message des refus en lecture seule.
pub const READ_ONLY_MESSAGE: &str =
    "bibliothèque en lecture seule : activer l'écriture dans la base ComicRack (assistant) pour modifier les fiches";

/// Écrit les modifications dans la base de `target`. `Err` seulement si la base est
/// inaccessible ; le détail par livre est dans le bilan.
pub fn apply_edits(target: EditTarget<'_>, edits: &[FieldEdit]) -> Result<EditReport, Error> {
    match target {
        EditTarget::Store(s) => s.apply_edits(edits),
        EditTarget::ComicRack(cfg) => apply_edits_comicrack(cfg, edits),
        EditTarget::ReadOnly => {
            let (groups, mut errors) = prepare(edits);
            errors.extend(groups.into_iter().map(|g| EditError {
                book_id: g.id,
                field: String::new(),
                message: READ_ONLY_MESSAGE.into(),
            }));
            Ok(EditReport { errors, ..Default::default() })
        }
    }
}

/// Valeur à écrire dans la fiche ComicRack pour le champ `def` de `b`.
fn comicrack_value(def: &FieldDef, b: &LibraryBook) -> Result<Option<Value>, String> {
    Ok(match def.get(b) {
        None => None,
        Some(EditValue::Text(t)) if def.kind == FieldKind::Date => {
            let d = chrono::DateTime::parse_from_rfc3339(&t).map_err(|_| format!("date illisible « {t} »"))?;
            Some(Value::Date(d))
        }
        Some(EditValue::Text(t)) => Some(Value::Text(t)),
        Some(EditValue::Int(i)) => Some(Value::Int(i)),
        Some(EditValue::Real(f)) => Some(Value::Float(f as f32)),
        Some(EditValue::Bool(v)) => Some(Value::Bool(v)),
        Some(EditValue::List(l)) => Some(Value::Text(l.join(", "))),
    })
}

/// Écrit les modifications dans la base MySQL de ComicRack : la fiche actuelle est relue
/// (`comics.data`), les modifications y sont appliquées pour trouver les champs qui
/// changent, puis seuls ces éléments sont réécrits par [`crate::db::update_books`]
/// (`ComicInfoIsDirty` à vrai si un champ du ComicInfo change), à condition que la fiche
/// n'ait pas été modifiée depuis sa lecture (sinon : `conflicts`).
pub fn apply_edits_comicrack(cfg: &DbConfig, edits: &[FieldEdit]) -> Result<EditReport, Error> {
    let (groups, errors) = prepare(edits);
    let mut report = EditReport { errors, ..Default::default() };
    if groups.is_empty() {
        return Ok(report);
    }
    let ids: Vec<String> = groups.iter().map(|g| g.id.clone()).collect();
    let records = crate::db::fetch_records(cfg, &ids)?;
    let mut book_edits = Vec::new();
    for g in &groups {
        let Some(rec) = records.get(&g.id) else {
            report.missing.push(g.id.clone());
            continue;
        };
        let before = match LibraryBook::from_xml(&rec.data) {
            Ok(b) => b,
            Err(e) => {
                report.errors.push(EditError {
                    book_id: g.id.clone(),
                    field: String::new(),
                    message: format!("fiche ComicRack illisible : {e}"),
                });
                continue;
            }
        };
        let mut after = before.clone();
        for v in &g.edits {
            apply_valid(&mut after, v);
        }
        let mut be = BookEdit::new(&g.id).if_counter(rec.counter);
        let mut dirty = false;
        let mut failed = None;
        for def in FIELDS.iter().filter(|d| !d.same(&before, &after)) {
            match comicrack_value(def, &after) {
                Ok(Some(v)) => be = be.set(def.name, v),
                Ok(None) => be = be.remove(def.name),
                Err(m) => failed = Some((def.name, m)),
            }
            dirty |= def.in_comic_info;
        }
        if let Some((field, message)) = failed {
            report.errors.push(EditError { book_id: g.id.clone(), field: field.into(), message });
            continue;
        }
        if be.changes.is_empty() {
            report.unchanged.push(g.id.clone());
            continue;
        }
        if dirty {
            be = be.comic_info_dirty(true);
        }
        book_edits.push(be);
    }
    let w = crate::db::update_books(cfg, &book_edits)?;
    report.changed.extend(w.written.iter().map(|x| x.book.clone()));
    report.unchanged.extend(w.unchanged.iter().cloned());
    report.missing.extend(w.missing.iter().cloned());
    report.conflicts.extend(w.conflicts.iter().chain(&w.stale).cloned());
    report.errors.extend(w.failed.iter().map(|(id, m)| EditError {
        book_id: id.clone(),
        field: String::new(),
        message: m.clone(),
    }));
    report.comicrack = Some(w);
    Ok(report)
}
