//! Modifications demandées par un script : aperçu (avant → après), choix des livres,
//! application dans la base (fiches, puis renommages) et bilan. Sans interface : testé
//! ici, dessiné par `ui/scripts_ui.rs`.
//!
//! Ordre d'application (fil d'arrière-plan, [`apply_plan`]) :
//! 1. fiches : `edit::apply_edits` sur la bonne cible ([`ApplyTarget`]) ;
//! 2. renommages / déplacements : `edit::rename_or_move` (fichier remis à sa place si la
//!    fiche ne peut pas être mise à jour) ;
//! 3. écriture des infos dans les fichiers : rendue à l'interface ([`ApplyOutcome::write_info`]),
//!    qui réutilise « Écrire les infos dans le fichier » (`library/files.rs`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use comicrust_core::db::{ComicDatabase, DbConfig, WriteReport};
use comicrust_core::edit::{self, EditReport, EditTarget, FieldEdit, MoveReport, CUSTOM_VALUE_PREFIX};
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{field_edit, ScriptChange};
use comicrust_core::store::{Store, StoreConfig};

use crate::library::files::FileAction;
use crate::library::LibrarySource;
use crate::settings::LibraryMode;

// ---------------------------------------------------------------------------------------
// Cible
// ---------------------------------------------------------------------------------------

/// Où appliquer les modifications des scripts, selon la bibliothèque affichée.
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyTarget {
    /// Base de ComicRust (mode autonome).
    Store(StoreConfig),
    /// Base MySQL de ComicRack, écriture autorisée.
    ComicRack(DbConfig),
    /// Rien n'est modifiable : message pour l'utilisateur.
    ReadOnly(String),
}

/// Lecture seule : base ComicRack sans écriture.
pub const READ_ONLY_COMICRACK: &str = "Lecture seule : active l'écriture dans la base ComicRack (⚙ ▸ Assistant de \
     configuration, « Lecture et écriture ») pour appliquer les modifications des scripts.";
/// Lecture seule : `ComicDb.xml`.
pub const READ_ONLY_XML: &str = "Lecture seule : la bibliothèque ComicDb.xml de ComicRack ne se modifie pas depuis \
     ComicRust (ComicRack la réécrit lui-même). Utilise la base MySQL de ComicRack en écriture, ou transfère la \
     bibliothèque dans ComicRust (⚙ ▸ Transférer).";
/// Pas de bibliothèque.
pub const NO_LIBRARY: &str = "Aucune bibliothèque chargée : rien à modifier.";

impl ApplyTarget {
    /// Cible de la bibliothèque chargée (`source`), d'après le mode choisi.
    pub fn of(mode: LibraryMode, comicrack_write: bool, source: Option<&LibrarySource>) -> ApplyTarget {
        match (mode, source) {
            (LibraryMode::Standalone, Some(LibrarySource::Standalone(c))) => ApplyTarget::Store(c.clone()),
            (LibraryMode::ComicRackMySql, Some(LibrarySource::ComicRackMySql(c))) if comicrack_write => {
                ApplyTarget::ComicRack(c.clone())
            }
            (LibraryMode::ComicRackMySql, Some(_)) => ApplyTarget::ReadOnly(READ_ONLY_COMICRACK.into()),
            (LibraryMode::ComicRackXml, Some(_)) => ApplyTarget::ReadOnly(READ_ONLY_XML.into()),
            _ => ApplyTarget::ReadOnly(NO_LIBRARY.into()),
        }
    }

    pub fn read_only(&self) -> Option<&str> {
        match self {
            ApplyTarget::ReadOnly(m) => Some(m),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Aperçu
// ---------------------------------------------------------------------------------------

/// Un champ modifié d'un livre.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldRow {
    /// Nom ComicRack (`Series`) ou `CustomValue:<clé>`.
    pub field: String,
    /// Libellé français.
    pub label: String,
    pub before: String,
    pub after: String,
    /// Valeur refusée (le champ n'est pas appliqué).
    pub error: Option<String>,
    pub edit: FieldEdit,
}

/// Renommage / déplacement demandé.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameRow {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Conflit ou chemin refusé : le renommage n'est pas fait.
    pub problem: Option<String>,
}

/// Modifications d'un livre.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewBook {
    /// Identifiant tel que dans la bibliothèque.
    pub id: String,
    pub caption: String,
    /// Case « appliquer » de ce livre.
    pub checked: bool,
    pub fields: Vec<FieldRow>,
    pub rename: Option<RenameRow>,
    /// Écrire les infos dans le fichier : `Some(None)` demandé, `Some(Some(problème))`
    /// impossible (format qui ne reçoit pas les infos).
    pub write_info: Option<Option<String>>,
}

impl PreviewBook {
    /// Quelque chose est-il applicable pour ce livre ?
    pub fn applicable(&self) -> bool {
        self.fields.iter().any(|f| f.error.is_none())
            || self.rename.as_ref().is_some_and(|r| r.problem.is_none())
            || matches!(self.write_info, Some(None))
    }

    /// Problèmes relevés (valeurs refusées, conflits de renommage…).
    pub fn problems(&self) -> usize {
        self.fields.iter().filter(|f| f.error.is_some()).count()
            + self.rename.as_ref().is_some_and(|r| r.problem.is_some()) as usize
            + matches!(self.write_info, Some(Some(_))) as usize
    }
}

/// Aperçu des modifications d'une exécution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    pub books: Vec<PreviewBook>,
    /// Livres dont les demandes ne changent rien (déjà dans l'état demandé).
    pub unchanged: usize,
    /// Livres disparus de la bibliothèque depuis le lancement.
    pub missing: Vec<String>,
}

/// Texte d'un champ (ou d'une valeur personnalisée) pour l'aperçu.
pub fn field_text(field: &str, book: &LibraryBook) -> String {
    if let Some(key) = field.strip_prefix(CUSTOM_VALUE_PREFIX) {
        return edit::custom_value(book.custom_values_store.as_deref(), key).unwrap_or_default();
    }
    edit::field(field).map(|d| d.text(book)).unwrap_or_default()
}

/// Libellé français d'un champ.
pub fn field_label(field: &str) -> String {
    if let Some(key) = field.strip_prefix(CUSTOM_VALUE_PREFIX) {
        return format!("Valeur perso. « {key} »");
    }
    edit::field(field).map(|d| d.label.to_string()).unwrap_or_else(|| field.to_string())
}

/// Chemin cible d'un renommage, **normalisé** comme il sera écrit dans la fiche
/// (`edit::files::normalize_rename` : chemin complet ou relatif au dossier actuel du
/// fichier, séparateurs unifiés, `.`/`..` et chemins Windows sans lecteur refusés, style
/// de la racine d'origine gardé). `Err` : chemin refusé (message pour l'aperçu).
pub fn resolve_rename(current: &Path, requested: &str) -> Result<PathBuf, String> {
    edit::files::normalize_rename(&current.to_string_lossy(), requested).map(PathBuf::from)
}

/// Textes d'un renommage pour l'aperçu : noms de fichiers seuls s'il reste dans son
/// dossier, chemin complet de la destination sinon.
pub fn rename_texts(from: &Path, to: &Path) -> (String, String) {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if from.parent() == to.parent() {
        (name(from), name(to))
    } else {
        (name(from), to.display().to_string())
    }
}

/// Mêmes chemins (sans casse sous Windows, où les noms de fichiers l'ignorent) ?
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

impl Preview {
    /// Construit l'aperçu des modifications `changes` (dans l'ordre des demandes ; la
    /// dernière valeur d'un champ l'emporte) sur les fiches de `db`. Rien n'est écrit.
    pub fn build(changes: &[ScriptChange], db: &ComicDatabase) -> Preview {
        #[derive(Default)]
        struct Wanted {
            fields: Vec<(String, serde_json::Value)>,
            rename: Option<String>,
            write_info: bool,
        }
        let mut order: Vec<String> = Vec::new();
        let mut wanted: HashMap<String, Wanted> = HashMap::new();
        for c in changes {
            let key = c.book_id().to_ascii_lowercase();
            let w = wanted.entry(key.clone()).or_insert_with(|| {
                order.push(key.clone());
                Wanted::default()
            });
            match c {
                ScriptChange::SetField { field, value, .. } => match w.fields.iter_mut().find(|(f, _)| f == field) {
                    Some(slot) => slot.1 = value.clone(),
                    None => w.fields.push((field.clone(), value.clone())),
                },
                ScriptChange::Rename { new_path, .. } => w.rename = Some(new_path.clone()),
                ScriptChange::WriteInfo { .. } => w.write_info = true,
            }
        }
        let mut out = Preview::default();
        // Destinations déjà prises par un autre livre de l'aperçu.
        let mut destinations: HashMap<String, String> = HashMap::new();
        for key in order {
            let w = wanted.remove(&key).unwrap_or_default();
            let Some(book) = db.get(&key) else {
                out.missing.push(key);
                continue;
            };
            let mut after = book.clone();
            let mut fields = Vec::new();
            for (field, value) in &w.fields {
                let edit = field_edit(&book.id, field, value);
                let before = field_text(field, book);
                let error = edit::apply_to_book(&mut after, &edit).err().map(|e| e.message);
                let new = field_text(field, &after);
                if error.is_some() || new != before {
                    fields.push(FieldRow {
                        field: field.clone(),
                        label: field_label(field),
                        before,
                        after: if error.is_some() { value_text(value) } else { new },
                        error,
                        edit,
                    });
                }
            }
            let rename = w.rename.as_deref().and_then(|req| {
                let from = PathBuf::from(book.file.as_deref().unwrap_or_default());
                if from.as_os_str().is_empty() {
                    return Some(RenameRow {
                        from,
                        to: PathBuf::from(req),
                        problem: Some("livre sans fichier".into()),
                    });
                }
                let to = match resolve_rename(&from, req) {
                    Ok(to) => to,
                    Err(e) => return Some(RenameRow { from, to: PathBuf::from(req), problem: Some(e) }),
                };
                if to == from {
                    return None; // déjà à cet emplacement
                }
                let mut problem = edit::files::check_path(&to).err();
                if problem.is_none() && !from.is_file() {
                    problem = Some("fichier introuvable".into());
                }
                if problem.is_none() && to.exists() && !same_path(&from, &to) {
                    problem = Some("la destination existe déjà".into());
                }
                let k = to.to_string_lossy().to_lowercase();
                if problem.is_none() {
                    if let Some(other) = destinations.get(&k) {
                        problem = Some(format!("même destination que « {other} »"));
                    }
                }
                if problem.is_none() {
                    destinations.insert(k, book.caption());
                }
                Some(RenameRow { from, to, problem })
            });
            let write_info = w.write_info.then(|| {
                let path = PathBuf::from(book.file.as_deref().unwrap_or_default());
                let final_path = rename.as_ref().filter(|r| r.problem.is_none()).map(|r| r.to.clone()).unwrap_or(path);
                (!FileAction::WriteInfo.accepts(&final_path))
                    .then(|| "seuls les CBZ reçoivent les infos (convertir d'abord en CBZ)".to_string())
            });
            let pb =
                PreviewBook { id: book.id.clone(), caption: book.caption(), checked: true, fields, rename, write_info };
            if pb.fields.is_empty() && pb.rename.is_none() && pb.write_info.is_none() {
                out.unchanged += 1;
                continue;
            }
            let applicable = pb.applicable();
            out.books.push(PreviewBook { checked: applicable, ..pb });
        }
        out
    }

    /// Coche ou décoche tous les livres (seuls ceux qui ont quelque chose d'applicable).
    pub fn set_all(&mut self, checked: bool) {
        for b in &mut self.books {
            b.checked = checked && b.applicable();
        }
    }

    pub fn checked(&self) -> usize {
        self.books.iter().filter(|b| b.checked).count()
    }

    /// Ce qui sera appliqué : livres cochés, sans les valeurs refusées ni les renommages en
    /// conflit. `db` : fiches actuelles (pour les renommages).
    pub fn plan(&self, db: &ComicDatabase) -> ApplyPlan {
        let mut plan = ApplyPlan::default();
        for b in self.books.iter().filter(|b| b.checked) {
            plan.edits.extend(b.fields.iter().filter(|f| f.error.is_none()).map(|f| f.edit.clone()));
            if let (Some(r), Some(book)) = (b.rename.as_ref().filter(|r| r.problem.is_none()), db.get(&b.id)) {
                plan.renames.push((book.clone(), r.to.clone()));
            }
            if matches!(b.write_info, Some(None)) {
                plan.write_info.push(b.id.clone());
            }
        }
        plan
    }

    /// Résumé : (livres, champs, renommages, écritures d'infos, problèmes).
    pub fn counts(&self) -> (usize, usize, usize, usize, usize) {
        let mut c = (self.books.len(), 0, 0, 0, 0);
        for b in &self.books {
            c.1 += b.fields.iter().filter(|f| f.error.is_none()).count();
            c.2 += b.rename.as_ref().is_some_and(|r| r.problem.is_none()) as usize;
            c.3 += matches!(b.write_info, Some(None)) as usize;
            c.4 += b.problems();
        }
        c
    }
}

/// Valeur demandée, en texte (affichée pour une valeur refusée).
fn value_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------------------

/// Modifications à appliquer.
#[derive(Debug, Clone, Default)]
pub struct ApplyPlan {
    pub edits: Vec<FieldEdit>,
    /// Fiche (actuelle) et nouveau chemin.
    pub renames: Vec<(LibraryBook, PathBuf)>,
    /// Livres dont les infos sont à écrire dans le fichier (après les fiches).
    pub write_info: Vec<String>,
}

impl ApplyPlan {
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.renames.is_empty() && self.write_info.is_empty()
    }

    /// Plan d'un lot de modifications appliquées au fil de l'eau (automatismes) : les
    /// renommages relatifs partent du fichier actuel ; un livre inconnu est ignoré.
    pub fn from_changes(changes: &[ScriptChange], book: impl Fn(&str) -> Option<LibraryBook>) -> ApplyPlan {
        let mut plan = ApplyPlan::default();
        for c in changes {
            match c {
                ScriptChange::SetField { book_id, field, value } => plan.edits.push(field_edit(book_id, field, value)),
                ScriptChange::Rename { book_id, new_path } => {
                    if let Some(b) = book(book_id) {
                        let from = PathBuf::from(b.file.as_deref().unwrap_or_default());
                        // Chemin refusé : gardé tel quel, `rename_or_move` le refusera avec
                        // le même message (bilan de l'automatisme).
                        let to = resolve_rename(&from, new_path).unwrap_or_else(|_| PathBuf::from(new_path));
                        plan.renames.retain(|(x, _)| !x.id.eq_ignore_ascii_case(book_id));
                        plan.renames.push((b, to));
                    }
                }
                ScriptChange::WriteInfo { book_id } => {
                    if !plan.write_info.iter().any(|i| i.eq_ignore_ascii_case(book_id)) {
                        plan.write_info.push(book_id.clone());
                    }
                }
            }
        }
        plan
    }
}

/// Bilan d'une application.
#[derive(Debug, Default)]
pub struct ApplyOutcome {
    /// Bilan des fiches.
    pub edit: Option<EditReport>,
    /// Renommages faits.
    pub moves: Vec<MoveReport>,
    /// Renommages refusés : (livre, message).
    pub rename_errors: Vec<(String, String)>,
    /// Livres dont les infos sont à écrire dans le fichier.
    pub write_info: Vec<String>,
    /// Base inaccessible, lecture seule… : rien (ou pas tout) n'a été appliqué.
    pub fatal: Option<String>,
    /// Renommages non faits : arrêt demandé (fermeture de ComicRust) avant leur tour.
    pub interrupted: usize,
}

impl ApplyOutcome {
    /// Écritures à signaler à la surveillance (identifiant, compteur de la ligne).
    pub fn written(&self) -> Vec<(String, i64)> {
        let mut out = Vec::new();
        let from_cr = |w: &WriteReport, out: &mut Vec<(String, i64)>| {
            out.extend(w.written.iter().map(|x| (x.id.to_ascii_lowercase(), x.record.counter)));
        };
        if let Some(r) = &self.edit {
            if let Some(c) = r.store_counter {
                out.extend(r.changed.iter().map(|b| (b.id.to_ascii_lowercase(), c)));
            }
            if let Some(w) = &r.comicrack {
                from_cr(w, &mut out);
            }
        }
        for m in &self.moves {
            if let Some(c) = m.store_counter {
                out.push((m.book.id.to_ascii_lowercase(), c));
            }
            if let Some(w) = &m.comicrack {
                from_cr(w, &mut out);
            }
        }
        out
    }

    /// Fiches relues après écriture (la plus récente l'emporte), pour la bibliothèque affichée.
    pub fn fresh_books(&self) -> Vec<LibraryBook> {
        let mut out: Vec<LibraryBook> = Vec::new();
        let mut push = |b: &LibraryBook| {
            out.retain(|x| !x.id.eq_ignore_ascii_case(&b.id));
            out.push(b.clone());
        };
        if let Some(r) = &self.edit {
            r.changed.iter().for_each(&mut push);
        }
        self.moves.iter().for_each(|m| push(&m.book));
        out
    }

    /// Livres modifiés (fiches ou fichiers).
    pub fn modified(&self) -> usize {
        let ids: HashSet<String> = self.fresh_books().iter().map(|b| b.id.to_ascii_lowercase()).collect();
        ids.len()
    }

    /// Erreurs et conflits, une ligne chacun (`caption` : nom d'un livre d'après son id).
    pub fn problems(&self, caption: impl Fn(&str) -> String) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(f) = &self.fatal {
            out.push(f.clone());
        }
        if let Some(r) = &self.edit {
            out.extend(r.missing.iter().map(|id| format!("{} : livre absent de la base", caption(id))));
            out.extend(r.conflicts.iter().map(|id| format!("{} : fiche modifiée ailleurs entre-temps", caption(id))));
            out.extend(r.errors.iter().map(|e| {
                let what = if e.field.is_empty() { String::new() } else { format!(" ({})", field_label(&e.field)) };
                format!("{}{what} : {}", caption(&e.book_id), e.message)
            }));
        }
        out.extend(self.rename_errors.iter().map(|(id, m)| format!("{} : renommage refusé — {m}", caption(id))));
        if self.interrupted > 0 {
            out.push(format!("{} renommage(s) non fait(s) : arrêt demandé avant leur tour", self.interrupted));
        }
        out
    }

    /// Résumé d'une ligne.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{} livre(s) modifié(s)", self.modified())];
        if let Some(r) = &self.edit {
            if !r.unchanged.is_empty() {
                parts.push(format!("{} déjà à jour", r.unchanged.len()));
            }
            if !r.conflicts.is_empty() {
                parts.push(format!("{} conflit(s)", r.conflicts.len()));
            }
        }
        if !self.moves.is_empty() {
            parts.push(format!("{} fichier(s) renommé(s)", self.moves.len()));
        }
        let errors = self.edit.as_ref().map_or(0, |r| r.errors.len() + r.missing.len())
            + self.rename_errors.len()
            + self.fatal.is_some() as usize;
        if errors > 0 {
            parts.push(format!("{errors} erreur(s)"));
        }
        if self.interrupted > 0 {
            parts.push(format!("{} renommage(s) non fait(s) (arrêt)", self.interrupted));
        }
        if !self.write_info.is_empty() {
            parts.push(format!("écriture des infos lancée pour {} fichier(s)", self.write_info.len()));
        }
        parts.join(", ")
    }

    pub fn is_complete(&self) -> bool {
        self.fatal.is_none()
            && self.rename_errors.is_empty()
            && self.interrupted == 0
            && self.edit.as_ref().is_none_or(|r| r.is_complete())
    }
}

/// Cible d'écriture de `edit` (base ComicRust ouverte, ou base ComicRack).
fn edit_target<'a>(store: &'a mut Option<Store>, target: &'a ApplyTarget) -> EditTarget<'a> {
    match (store.as_mut(), target) {
        (Some(s), _) => EditTarget::Store(s),
        (None, ApplyTarget::ComicRack(cfg)) => EditTarget::ComicRack(cfg),
        _ => EditTarget::ReadOnly,
    }
}

/// Applique `plan` (dans un fil d'arrière-plan : accès à la base et aux fichiers).
///
/// `stop` (fermeture de ComicRust) est vérifié **entre deux livres**, jamais au milieu
/// d'un renommage : un fichier déplacé a toujours sa fiche à jour (ou est remis à sa place).
/// Les fiches (étape 1) sont écrites d'un seul tenant ; les renommages restants sont
/// comptés dans [`ApplyOutcome::interrupted`].
pub fn apply_plan(target: &ApplyTarget, plan: &ApplyPlan, stop: &AtomicBool) -> ApplyOutcome {
    let mut out = ApplyOutcome::default();
    if plan.is_empty() {
        return out;
    }
    let mut store = match target {
        ApplyTarget::ReadOnly(m) => {
            out.fatal = Some(m.clone());
            return out;
        }
        ApplyTarget::Store(cfg) => match Store::open_existing(cfg) {
            Ok(s) => Some(s),
            Err(e) => {
                out.fatal = Some(format!("Base ComicRust inaccessible : {e}"));
                return out;
            }
        },
        ApplyTarget::ComicRack(_) => None,
    };
    // 1. Fiches.
    let mut fresh: HashMap<String, LibraryBook> = HashMap::new();
    if !plan.edits.is_empty() {
        match edit::apply_edits(edit_target(&mut store, target), &plan.edits) {
            Ok(r) => {
                fresh.extend(r.changed.iter().map(|b| (b.id.to_ascii_lowercase(), b.clone())));
                out.edit = Some(r);
            }
            Err(e) => {
                out.fatal = Some(format!("Fiches non modifiées : {e}"));
                return out;
            }
        }
    }
    // 2. Fichiers (fiche relue après l'étape 1 si elle a changé).
    for (i, (book, to)) in plan.renames.iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            out.interrupted = plan.renames.len() - i;
            break;
        }
        let current = fresh.get(&book.id.to_ascii_lowercase()).unwrap_or(book);
        match edit::rename_or_move(edit_target(&mut store, target), current, to) {
            Ok(m) => out.moves.push(m),
            Err(e) => out.rename_errors.push((book.id.clone(), e.to_string())),
        }
    }
    // 3. Infos dans les fichiers : faites par l'interface (travail « Fichiers »).
    out.write_info = plan.write_info.clone();
    out
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
