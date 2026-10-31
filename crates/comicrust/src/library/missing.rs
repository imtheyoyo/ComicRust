//! Livres manquants (v0.10.2) : fiche dans la bibliothèque, fichier introuvable.
//!
//! - **Repérage** : base ComicRust, colonne `missing` (analyse des dossiers) ; ComicRack,
//!   élément `<Missing>` de la fiche (tenu par ComicRack quand il relit le fichier), complété
//!   par une **vérification en arrière-plan** après le chargement
//!   (`comicrust_core::missing::check_files` : un fil, par dossier, jamais sur le fil de
//!   l'interface ; NAS injoignable ou disparition massive : rien de marqué, et c'est dit).
//!   Le résultat de cette vérification est gardé en mémoire (`overlay`), jamais écrit dans
//!   la base de ComicRack (ComicRack recalcule `Missing` lui-même ; son `CopyFrom` ne le
//!   recopie pas). Une ouverture qui échoue sur « Fichier introuvable » marque aussi le
//!   livre (et, base ComicRust, enregistre `missing`).
//! - **Retrouver le fichier** : fiche mise à jour comme pour un déplacement trouvé par
//!   l'analyse (chemin, taille, dates ; lecture, note et infos gardées) : base ComicRust
//!   (`Store::apply_scan`, colonnes ciblées) ou ComicRack en écriture (`BookEdit::file` +
//!   `Missing` retiré) ; ComicRack en lecture seule ou `ComicDb.xml` : refus.
//! - **Retirer de la bibliothèque** : la **fiche seulement, jamais le fichier**. Base
//!   ComicRust : `Store::delete` ; ComicRack MySQL en écriture : `db::delete_books`
//!   (comme `ComicStorageBaseSql.Delete`). Les suppressions sont signalées à la
//!   surveillance **avant** d'être faites, pour qu'elle ne les prenne pas pour des
//!   suppressions extérieures.
//!
//! Listes de lecture : comme ComicRack (`ComicListLibraryBrowser.RemoveBookHandler` appelle
//! seulement `library.Remove(book)`), elles ne sont pas réécrites : le livre n'y apparaît
//! plus (identifiant absent de la bibliothèque), et ComicRack purge l'identifiant au calcul
//! suivant de la liste (`ComicIdListItem.OnGetBooks`). ComicRack peut en plus mettre le
//! chemin sur une « liste noire » (option « Ne pas réajouter les fichiers supprimés »,
//! désactivée par défaut) : pas reproduit, un fichier encore présent dans un dossier
//! surveillé réapparaît donc à l'analyse suivante (sous une nouvelle fiche).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use comicrust_core::db::{BookEdit, DbConfig, WriteReport};
use comicrust_core::library::LibraryBook;
use comicrust_core::missing::{CheckItem, CheckReport, MatchKind, Progress, SearchReport, Wanted};
use comicrust_core::scanner::path_key;
use comicrust_core::store::{Store, StoreConfig};
use eframe::egui;

use super::watch::{DeletedSink, WrittenSink};
use super::LibraryState;

/// Le livre est-il manquant ? (Comme `ComicBookIsMissingMatcher` : livre avec fichier.)
pub fn is_missing(b: &LibraryBook) -> bool {
    b.missing == Some(true) && b.file.as_deref().is_some_and(|f| !f.is_empty())
}

/// Où modifier les fiches (relocalisation, suppression).
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// Base de ComicRust.
    Store(StoreConfig),
    /// Base MySQL de ComicRack, en écriture.
    ComicRack(DbConfig),
}

/// Vérification en arrière-plan de l'existence des fichiers.
pub struct CheckJob {
    pub progress: Arc<Progress>,
    pub cancel: Arc<AtomicBool>,
    /// Lancée à la demande (son bilan s'affiche).
    pub manual: bool,
    /// Chemin de la fiche de chaque livre vérifié (le résultat ne vaut que pour lui).
    files: HashMap<String, String>,
    rx: Receiver<CheckReport>,
}

impl CheckJob {
    /// `items` : identifiant, chemin de la fiche, fichier réel à examiner.
    pub fn start(items: Vec<(String, String, PathBuf)>, manual: bool, ctx: &egui::Context) -> Self {
        let progress = Arc::new(Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let files = items.iter().map(|(id, f, _)| (id.to_ascii_lowercase(), f.clone())).collect();
        let check: Vec<CheckItem> = items.into_iter().map(|(id, _, path)| CheckItem { id, path }).collect();
        let (tx, rx) = mpsc::channel();
        let (p, c, ctx) = (progress.clone(), cancel.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("comicrust-manquants".into())
            .spawn(move || {
                let _ = tx.send(comicrust_core::missing::check_files(&check, &p, &c));
                ctx.request_repaint();
            })
            .expect("fil de vérification des fichiers");
        Self { progress, cancel, manual, files, rx }
    }

    pub fn poll(&self) -> Option<CheckReport> {
        self.rx.try_recv().ok()
    }
}

/// Recherche des fichiers manquants dans un dossier.
pub struct SearchJob {
    pub progress: Arc<Progress>,
    pub cancel: Arc<AtomicBool>,
    pub dir: PathBuf,
    rx: Receiver<SearchReport>,
}

impl SearchJob {
    pub fn start(dir: PathBuf, wanted: Vec<Wanted>, exclude: HashSet<String>, ctx: &egui::Context) -> Self {
        let progress = Arc::new(Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c, d, ctx) = (progress.clone(), cancel.clone(), dir.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("comicrust-recherche".into())
            .spawn(move || {
                let _ = tx.send(comicrust_core::missing::search_folder(&d, &wanted, &exclude, &p, &c));
                ctx.request_repaint();
            })
            .expect("fil de recherche des fichiers");
        Self { progress, cancel, dir, rx }
    }

    pub fn poll(&self) -> Option<SearchReport> {
        self.rx.try_recv().ok()
    }
}

/// Fichier proposé pour un livre (fenêtre de confirmation).
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub id: String,
    pub caption: String,
    /// Chemin actuel de la fiche.
    pub old: String,
    pub new: PathBuf,
    pub kind: MatchKind,
    /// Autres fichiers de même nom trouvés.
    pub others: usize,
    /// Livre de la bibliothèque qui a déjà ce fichier (doublon après une analyse).
    pub duplicate: Option<String>,
    pub checked: bool,
}

/// Résultat d'une opération sur les fiches, faite dans un fil.
#[derive(Debug)]
pub enum OpResult {
    /// Base ComicRust : fiches réécrites (relues), et leur compteur.
    Store { books: Vec<LibraryBook>, counter: i64 },
    /// Base ComicRack : bilan d'écriture.
    ComicRack(WriteReport),
    /// Fiches supprimées ; `absent` : déjà absentes de la base.
    Removed { ids: Vec<String>, absent: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Relocate,
    Remove,
    /// Enregistrement de `missing` (base ComicRust).
    Mark,
}

pub struct OpDone {
    pub kind: OpKind,
    pub result: Result<OpResult, String>,
}

/// État des livres manquants (gardé dans [`LibraryState`]).
#[derive(Default)]
pub struct MissingState {
    /// Résultats de la vérification et des ouvertures ratées : identifiant (minuscules) →
    /// (chemin de la fiche vérifié, manquant ?). Réappliqués quand la base est relue.
    overlay: HashMap<String, (String, bool)>,
    pub check: Option<CheckJob>,
    /// Bilan de la dernière vérification.
    pub last_check: Option<CheckReport>,
    pub search: Option<SearchJob>,
    ops: Vec<Receiver<OpDone>>,
}

impl MissingState {
    /// Opération en cours (relocalisation, suppression) ?
    pub fn busy(&self) -> bool {
        !self.ops.is_empty()
    }

    pub fn push_op(&mut self, rx: Receiver<OpDone>) {
        self.ops.push(rx);
    }

    /// Opérations terminées.
    pub fn poll_ops(&mut self) -> Vec<OpDone> {
        let mut out = Vec::new();
        self.ops.retain(|rx| match rx.try_recv() {
            Ok(d) => {
                out.push(d);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        out
    }
}

impl LibraryState {
    /// Nombre de livres manquants de la bibliothèque.
    pub fn missing_count(&self) -> usize {
        self.database().map_or(0, |db| db.books.iter().filter(|b| is_missing(b)).count())
    }

    /// Applique `overlay` à la base en mémoire (après une relecture de la base). Renvoie
    /// `true` si un livre a changé.
    pub(super) fn reapply_missing(&mut self) -> bool {
        if self.missing.overlay.is_empty() {
            return false;
        }
        let Some(db) = self.database() else { return false };
        let todo: Vec<(usize, Option<bool>)> = self
            .missing
            .overlay
            .iter()
            .filter_map(|(id, (file, missing))| {
                let i = db.index_of(id)?;
                let b = &db.books[i];
                if b.file.as_deref() != Some(file.as_str()) {
                    return None; // fiche relocalisée entre-temps : résultat périmé
                }
                let want = match (*missing, b.missing) {
                    (true, _) => Some(true),
                    (false, Some(true)) => Some(false),
                    (false, other) => other,
                };
                (want != b.missing).then_some((i, want))
            })
            .collect();
        if todo.is_empty() {
            return false;
        }
        if let super::Status::Loaded(db) = &mut self.status {
            let db = Arc::make_mut(db);
            for (i, m) in todo {
                db.books[i].missing = m;
            }
        }
        true
    }

    /// Marque des livres manquants (ou présents) en mémoire, pour leur fichier actuel.
    pub fn set_missing(&mut self, ids: &[String], missing: bool) {
        let Some(db) = self.database() else { return };
        for id in ids {
            if let Some(f) = db.get(id).and_then(|b| b.file.clone()) {
                self.missing.overlay.insert(id.to_ascii_lowercase(), (f, missing));
            }
        }
        if self.reapply_missing() {
            self.generation += 1;
            self.view_key = None;
        }
    }

    /// Résultat d'une vérification : livres marqués manquants ou présents (seulement ceux
    /// dont la fiche a encore le chemin vérifié).
    pub fn apply_check(&mut self, files: &HashMap<String, String>, report: &CheckReport) {
        for (ids, missing) in [(&report.missing, true), (&report.present, false)] {
            for id in ids {
                let key = id.to_ascii_lowercase();
                if let Some(f) = files.get(&key) {
                    self.missing.overlay.insert(key, (f.clone(), missing));
                }
            }
        }
        if self.reapply_missing() {
            self.generation += 1;
            self.view_key = None;
        }
    }

    /// Fiches retirées de la bibliothèque par ComicRust (déjà supprimées de la base).
    pub fn remove_books(&mut self, ids: &[String]) -> usize {
        let super::Status::Loaded(db) = &mut self.status else { return 0 };
        let n = Arc::make_mut(db).remove(ids);
        let gone: HashSet<String> = ids.iter().map(|i| i.to_ascii_lowercase()).collect();
        for id in &gone {
            self.missing.overlay.remove(id);
        }
        self.selected_books.retain(|b| !gone.contains(&b.to_ascii_lowercase()));
        if self.selected_book.as_ref().is_some_and(|b| gone.contains(&b.to_ascii_lowercase())) {
            self.selected_book = None;
        }
        if self.cursor.as_ref().is_some_and(|b| gone.contains(&b.to_ascii_lowercase())) {
            self.cursor = None;
        }
        self.generation += 1;
        self.view_key = None;
        n
    }

    /// Lance la vérification des fichiers ; `real` : fichier réel d'un chemin de fiche (CBZ
    /// converti). Sans effet si une vérification tourne déjà.
    pub fn start_check(&mut self, real: impl Fn(&str) -> PathBuf, manual: bool, ctx: &egui::Context) {
        if self.missing.check.is_some() {
            return;
        }
        let Some(db) = self.database() else { return };
        let items: Vec<(String, String, PathBuf)> = db
            .books
            .iter()
            .filter_map(|b| {
                let f = b.file.clone().filter(|f| !f.is_empty())?;
                let real = real(&f);
                Some((b.id.clone(), f, real))
            })
            .collect();
        self.missing.check = Some(CheckJob::start(items, manual, ctx));
    }

    /// Vérification terminée : appliquée ; renvoie le bilan (et si elle était demandée).
    pub fn poll_check(&mut self) -> Option<(CheckReport, bool)> {
        let job = self.missing.check.as_ref()?;
        let report = job.poll()?;
        let job = self.missing.check.take()?;
        if !report.cancelled {
            self.apply_check(&job.files, &report);
        }
        self.missing.last_check = Some(report.clone());
        Some((report, job.manual))
    }

    /// Livres à retrouver (fichier et taille de la fiche).
    pub fn wanted(&self, ids: &[String]) -> Vec<Wanted> {
        let Some(db) = self.database() else { return Vec::new() };
        ids.iter()
            .filter_map(|id| db.get(id))
            .filter_map(|b| Some(Wanted { id: b.id.clone(), file: b.file.clone()?, size: b.file_size }))
            .collect()
    }

    /// Fichiers déjà dans la bibliothèque (clés de chemin) → identifiant du livre.
    pub fn files_index(&self) -> HashMap<String, String> {
        let Some(db) = self.database() else { return HashMap::new() };
        db.books.iter().filter_map(|b| Some((path_key(b.file.as_deref()?), b.id.clone()))).collect()
    }

    /// Propositions de la fenêtre de confirmation, d'après le résultat d'une recherche.
    pub fn proposals(&self, report: &SearchReport) -> Vec<Proposal> {
        let Some(db) = self.database() else { return Vec::new() };
        report
            .found
            .iter()
            .filter_map(|c| {
                let b = db.get(&c.id)?;
                Some(Proposal {
                    id: b.id.clone(),
                    caption: b.caption(),
                    old: b.file.clone().unwrap_or_default(),
                    new: c.path.clone(),
                    kind: c.kind,
                    others: c.others,
                    duplicate: None,
                    checked: c.kind.sure(),
                })
            })
            .collect()
    }

    /// Proposition pour un fichier choisi à la main.
    pub fn chosen_proposal(&self, id: &str, new: PathBuf, size: u64) -> Option<Proposal> {
        let db = self.database()?;
        let b = db.get(id)?;
        let old = b.file.clone().unwrap_or_default();
        let key = path_key(&new.to_string_lossy());
        let duplicate = db
            .books
            .iter()
            .find(|o| o.id != b.id && o.file.as_deref().is_some_and(|f| path_key(f) == key))
            .map(|o| o.caption());
        let kind = comicrust_core::missing::chosen_kind(&old, b.file_size, &new, size);
        Some(Proposal {
            id: b.id.clone(),
            caption: b.caption(),
            old,
            new,
            kind,
            others: 0,
            checked: duplicate.is_none(),
            duplicate,
        })
    }
}

/// Lance une relocalisation (fiches mises à jour dans un fil).
pub fn relocate(
    target: Target,
    books: Vec<(LibraryBook, PathBuf)>,
    written: Option<WrittenSink>,
    ctx: &egui::Context,
) -> Receiver<OpDone> {
    spawn(ctx, OpKind::Relocate, move || match target {
        Target::Store(cfg) => {
            let report = comicrust_core::missing::relocation_report(&books).map_err(|e| e.to_string())?;
            let mut st = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
            let w = st.apply_scan(&report).map_err(|e| e.to_string())?;
            if let Some(s) = &written {
                s.lock().unwrap().extend(w.books.iter().map(|b| (b.id.to_ascii_lowercase(), w.counter)));
            }
            Ok(OpResult::Store { books: w.books, counter: w.counter })
        }
        Target::ComicRack(cfg) => {
            let mut edits = Vec::new();
            for (b, path) in &books {
                let f = super::comicrack_write::file_state(&path.to_string_lossy(), path, None)
                    .map_err(|e| format!("{} : {e}", path.display()))?;
                edits.push(BookEdit::new(&b.id).file(&f).remove("Missing"));
            }
            let report = comicrust_core::db::update_books(&cfg, &edits).map_err(|e| e.to_string())?;
            if let Some(s) = &written {
                s.lock().unwrap().extend(report.written.iter().map(|w| (w.id.clone(), w.record.counter)));
            }
            Ok(OpResult::ComicRack(report))
        }
    })
}

/// Lance le retrait de fiches de la bibliothèque (jamais les fichiers). `deleted` : signalé
/// à la surveillance avant la suppression.
pub fn remove(target: Target, ids: Vec<String>, deleted: Option<DeletedSink>, ctx: &egui::Context) -> Receiver<OpDone> {
    spawn(ctx, OpKind::Remove, move || {
        if let Some(s) = &deleted {
            s.lock().unwrap().extend(ids.iter().map(|i| i.to_ascii_lowercase()));
            // Laisse la surveillance prendre la note avant sa prochaine interrogation.
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        match target {
            Target::Store(cfg) => {
                let mut st = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
                let present: HashSet<String> =
                    st.get(&ids).map_err(|e| e.to_string())?.into_iter().map(|b| b.id.to_ascii_lowercase()).collect();
                st.delete(&ids).map_err(|e| e.to_string())?;
                let (ids, absent) = ids.into_iter().partition(|i| present.contains(&i.to_ascii_lowercase()));
                Ok(OpResult::Removed { ids, absent })
            }
            Target::ComicRack(cfg) => {
                let r = comicrust_core::db::delete_books(&cfg, &ids).map_err(|e| e.to_string())?;
                if let Some((id, e)) = r.failed.first() {
                    return Err(format!(
                        "{} fiche(s) supprimée(s), {} en erreur (ex. {id} : {e})",
                        r.deleted.len(),
                        r.failed.len()
                    ));
                }
                Ok(OpResult::Removed { ids: r.deleted, absent: r.missing })
            }
        }
    })
}

/// Base ComicRust : enregistre `missing` (vrai pour `missing`, effacé pour `back`).
pub fn persist_marks(
    cfg: StoreConfig,
    missing: Vec<String>,
    back: Vec<String>,
    written: Option<WrittenSink>,
    ctx: &egui::Context,
) -> Receiver<OpDone> {
    spawn(ctx, OpKind::Mark, move || {
        let report = comicrust_core::scanner::ScanReport { missing, back, ..Default::default() };
        let mut st = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
        let w = st.apply_scan(&report).map_err(|e| e.to_string())?;
        if let Some(s) = &written {
            s.lock().unwrap().extend(w.books.iter().map(|b| (b.id.to_ascii_lowercase(), w.counter)));
        }
        Ok(OpResult::Store { books: w.books, counter: w.counter })
    })
}

fn spawn(
    ctx: &egui::Context,
    kind: OpKind,
    f: impl FnOnce() -> Result<OpResult, String> + Send + 'static,
) -> Receiver<OpDone> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("comicrust-fiches".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
                .unwrap_or_else(|_| Err("plantage pendant la mise à jour des fiches".into()));
            let _ = tx.send(OpDone { kind, result });
            ctx.request_repaint();
        })
        .expect("fil de mise à jour des fiches");
    rx
}

#[cfg(test)]
#[path = "missing_tests.rs"]
mod tests;
