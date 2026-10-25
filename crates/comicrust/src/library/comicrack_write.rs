//! Écriture dans la base MySQL de ComicRack (mode « Avec ComicRack », option « Lecture et
//! écriture » de l'assistant, `Settings::comicrack_write`).
//!
//! Seuls les éléments concernés de la fiche sont réécrits, par
//! `comicrust_core::db::update_books` (voir l'en-tête de `db/write.rs` : fiche modifiée au
//! plus juste, compteurs de ComicRack, conflits). Les écritures passent par un seul fil,
//! dans l'ordre où elles sont demandées. Chaque bilan est :
//! - signalé à la surveillance (`WatchHandle::written`) : ces livres ne sont pas pris pour
//!   des modifications faites dans ComicRack ;
//! - appliqué à la bibliothèque affichée (`LibraryState::apply_written`, fiches relues) ;
//! - les livres non écrits (absents, en conflit, en erreur) sont signalés dans `bg_errors`.
//!
//! **Lecture** (à la fermeture d'un livre ou de la fenêtre), comme ComicRack :
//! `NavigatorManager.Open` fait `OpenedCount++` à chaque ouverture, `MainForm.OnBookOpened`
//! met `OpenedTime = DateTime.Now` (heure d'ouverture) et `NewPages = 0`, le navigateur
//! met `CurrentPage` (et `LastPageRead` s'il est dépassé) et `PageCount` (nombre de pages
//! du fichier). ComicRust écrit tout cela en une fois, à la fermeture ; `LastPageRead`
//! garde le maximum (la seconde page d'une double page compte, comme en mode autonome) et
//! `PageCount` n'est écrit que s'il manque. La lecture reste aussi gardée dans les réglages
//! de ComicRust (`last_pages`, `read_progress`) jusqu'à ce que l'écriture réussisse : rien
//! n'est perdu si la base est injoignable.
//!
//! **Fichiers** : après « Convertir en CBZ », le nouveau fichier (chemin, taille, dates,
//! `PageCount`) ; après « Écrire les infos dans le fichier », `ComicInfoIsDirty = false` et
//! la taille et les dates du fichier (comme `QueueManager.WriteInfoToFileWithCacheUpdate` :
//! `RefreshFileProperties` puis `ComicInfoIsDirty = false`).
//!
//! **Lecture des versions précédentes** : à l'ouverture de la bibliothèque en mode
//! écriture, la lecture gardée dans les réglages et plus avancée que celle de ComicRack est
//! proposée au report (rien n'est écrit sans accord) ; celle que ComicRack a déjà est
//! oubliée des réglages.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use comicrust_core::db::{BookEdit, ComicDatabase, DbConfig, FileState, Value, WriteReport};
use comicrust_core::library::LibraryBook;
use eframe::egui::{self, Context, RichText};

use super::files::Done;
use super::watch::WrittenSink;
use crate::app::ComicRustApp;
use crate::settings::{LibraryMode, Settings};

/// Attente maximale des écritures en cours à la fermeture de la fenêtre.
pub const CLOSE_WAIT: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------------------
// Modifications (testables sans interface)
// ---------------------------------------------------------------------------------------

/// Lecture d'un livre dans le lecteur.
#[derive(Debug, Clone, Copy)]
pub struct Reading {
    /// Page courante (à partir de 0).
    pub current: u32,
    /// Page la plus loin affichée pendant cette lecture (seconde page d'une double page).
    pub shown_max: u32,
    /// Pages du fichier (0 : inconnu).
    pub pages: u32,
    /// Heure d'ouverture du livre.
    pub opened_at: chrono::DateTime<chrono::FixedOffset>,
}

/// Fiche à écrire après une lecture (`book` : fiche actuelle de la bibliothèque).
pub fn reading_edit(book: &LibraryBook, r: &Reading) -> BookEdit {
    let mut e = BookEdit::new(&book.id)
        .set("CurrentPage", Value::Int(r.current.into()))
        .set("LastPageRead", Value::Max(r.shown_max.max(r.current).into()))
        .opened(r.opened_at)
        .set("NewPages", Value::Int(0));
    if r.pages > 0 && book.page_count.unwrap_or(0) == 0 {
        e = e.set("PageCount", Value::Int(r.pages.into()));
    }
    e
}

fn utc(t: std::io::Result<std::time::SystemTime>) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    t.ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).fixed_offset())
}

/// État d'un fichier sur le disque. `as_seen` : chemin tel que ComicRack l'écrit (attribut
/// `File`), `real` : fichier à examiner.
pub fn file_state(as_seen: &str, real: &Path, page_count: Option<u32>) -> std::io::Result<FileState> {
    let meta = std::fs::metadata(real)?;
    Ok(FileState {
        path: as_seen.to_string(),
        size: meta.len(),
        modified: utc(meta.modified()),
        created: utc(meta.created()),
        page_count,
        pages: None,
    })
}

/// Fiche à écrire après une action sur les fichiers ; `target` : fichier traité (celui
/// de la fiche, ou le CBZ qui le remplace).
pub fn file_edit(id: &str, done: &Done, target: &Path) -> Result<BookEdit, String> {
    match done {
        Done::Converted { cbz, pages, .. } => {
            // Les pages sont recopiées dans le même ordre : la liste <Pages> reste valable.
            let f = file_state(&cbz.to_string_lossy(), cbz, Some(*pages as u32))
                .map_err(|e| format!("{} : {e}", cbz.display()))?;
            Ok(BookEdit::new(id).file(&f))
        }
        Done::Info { written, .. } => {
            let mut e = BookEdit::new(id).comic_info_dirty(false);
            if *written {
                let f = file_state(&target.to_string_lossy(), target, None)
                    .map_err(|e| format!("{} : {e}", target.display()))?;
                e = e.file(&f);
                // Même fichier : une date que le système ne donne pas reste celle de la fiche.
                e.changes.retain(|c| c.value.is_some());
            }
            Ok(e)
        }
    }
}

/// Lecture gardée dans les réglages de ComicRust, plus avancée que celle de ComicRack.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: String,
    pub caption: String,
    /// Clé dans les réglages.
    pub path: PathBuf,
    pub current: usize,
    pub max_page: usize,
    pub pages: usize,
}

impl Pending {
    /// Rien n'est reculé : `LastPageRead` garde le maximum, `OpenCount` au moins 1. La date
    /// d'ouverture n'est pas connue (elle n'était pas gardée) : `Opened` n'est pas écrit.
    pub fn edit(&self, book: &LibraryBook) -> BookEdit {
        let mut e = BookEdit::new(&self.id)
            .set("CurrentPage", Value::Int(self.current as i64))
            .set("LastPageRead", Value::Max(self.max_page as i64))
            .set("OpenCount", Value::Max(1));
        if self.pages > 0 && book.page_count.unwrap_or(0) == 0 {
            e = e.set("PageCount", Value::Int(self.pages as i64));
        }
        e
    }
}

/// Livres de la base par chemin de fichier (minuscules).
fn by_path(db: &ComicDatabase) -> HashMap<String, usize> {
    db.books.iter().enumerate().filter_map(|(i, b)| Some((b.file.as_deref()?.to_lowercase(), i))).collect()
}

/// La lecture gardée dans les réglages pour `path` est-elle déjà dans la fiche `book` ?
fn synced(settings: &Settings, path: &Path, book: &LibraryBook) -> bool {
    let max = settings.read_progress.get(path).map_or(0, |p| p.max_page);
    let current = settings.last_pages.get(path).copied();
    max <= book.last_page_read.unwrap_or(0) as usize
        && current.is_none_or(|c| c == book.current_page.unwrap_or(0) as usize)
}

/// Oublie la lecture gardée dans les réglages pour `path` si ComicRack l'a (fiche relue
/// après l'écriture) : la reprise de lecture suit alors ComicRack seul.
pub fn forget_if_synced(settings: &mut Settings, path: &Path, book: &LibraryBook) -> bool {
    if !synced(settings, path, book) {
        return false;
    }
    settings.last_pages.remove(path).is_some() | settings.read_progress.remove(path).is_some()
}

/// Ménage à l'ouverture en mode écriture : oublie les lectures que ComicRack a déjà et
/// renvoie celles qui sont plus avancées dans ComicRust.
pub fn sort_local_progress(settings: &mut Settings, db: &ComicDatabase) -> Vec<Pending> {
    let index = by_path(db);
    let mut paths: Vec<PathBuf> = settings.read_progress.keys().chain(settings.last_pages.keys()).cloned().collect();
    paths.sort();
    paths.dedup();
    let mut pending = Vec::new();
    for path in paths {
        let Some(&i) = index.get(&path.to_string_lossy().to_lowercase()) else { continue };
        let book = &db.books[i];
        if forget_if_synced(settings, &path, book) {
            continue;
        }
        let progress = settings.read_progress.get(&path).copied().unwrap_or_default();
        if progress.max_page <= book.last_page_read.unwrap_or(0) as usize {
            // Seule la page courante diffère (relecture) : pas de quoi reporter, la reprise
            // de lecture demande comme avant.
            continue;
        }
        pending.push(Pending {
            id: book.id.clone(),
            caption: book.caption(),
            current: settings.last_pages.get(&path).copied().unwrap_or(progress.max_page),
            max_page: progress.max_page,
            pages: progress.pages,
            path,
        });
    }
    pending
}

/// Message pour les livres non écrits (`None` : tout est écrit). `what` : ce qui n'a pas
/// été enregistré (« Lecture », « Fichier converti »…), `kept` : ce qu'il advient.
pub fn problems(report: &WriteReport, captions: &HashMap<String, String>, what: &str, kept: &str) -> Option<String> {
    if report.is_complete() {
        return None;
    }
    let name = |id: &str| captions.get(id).cloned().unwrap_or_else(|| id.to_string());
    let mut lines = vec![format!("{what} non enregistré(e) dans la base de ComicRack pour :")];
    for id in &report.missing {
        lines.push(format!("• {} : livre absent de la base (retiré dans ComicRack ?)", name(id)));
    }
    for id in &report.conflicts {
        lines.push(format!("• {} : fiche modifiée en même temps dans ComicRack (plusieurs essais)", name(id)));
    }
    for (id, e) in &report.failed {
        lines.push(format!("• {} : {e}", name(id)));
    }
    lines.push(kept.to_string());
    Some(lines.join("\n"))
}

// ---------------------------------------------------------------------------------------
// Fil d'écriture
// ---------------------------------------------------------------------------------------

/// Ce qui a été demandé (pour appliquer le bilan).
#[derive(Debug, Clone)]
pub enum WriteKind {
    /// Lecture d'un livre ; `path` : clé dans les réglages.
    Reading { path: PathBuf },
    /// Report de la lecture gardée dans les réglages.
    Migration { paths: Vec<(String, PathBuf)> },
    /// Fichiers convertis ou infos écrites.
    Files,
}

struct Job {
    cfg: DbConfig,
    edits: Vec<BookEdit>,
    kind: WriteKind,
    captions: HashMap<String, String>,
    sink: Option<WrittenSink>,
}

pub struct WriteDone {
    pub kind: WriteKind,
    pub captions: HashMap<String, String>,
    pub result: Result<WriteReport, String>,
}

/// Écritures en cours (compteur partagé avec le fil, pour attendre à la fermeture).
type Busy = Arc<(Mutex<usize>, Condvar)>;

#[derive(Default)]
pub struct ComicRackWrites {
    worker: Option<(Sender<Job>, Receiver<WriteDone>)>,
    busy: Busy,
    /// Lecture des versions précédentes à reporter (fenêtre de proposition).
    pub prompt: Option<Vec<Pending>>,
    /// Ménage et proposition faits pour la bibliothèque chargée.
    checked: bool,
    /// Pour réveiller l'interface quand un bilan arrive.
    ctx: Option<Context>,
}

impl ComicRackWrites {
    fn submit(&mut self, job: Job) {
        let ctx = self.ctx.clone();
        let (tx, _) = self.worker.get_or_insert_with(|| spawn_worker(self.busy.clone(), ctx));
        *self.busy.0.lock().unwrap() += 1;
        if let Err(mpsc::SendError(job)) = tx.send(job) {
            // Fil arrêté (plantage) : un nouveau reprend le travail.
            *self.busy.0.lock().unwrap() -= 1;
            self.worker = None;
            self.submit(job);
        }
    }

    fn poll(&mut self) -> Vec<WriteDone> {
        self.worker.as_ref().map(|(_, rx)| rx.try_iter().collect()).unwrap_or_default()
    }

    /// Fil qui attend la fin des écritures en cours (au plus `CLOSE_WAIT`).
    fn waiter(&self) -> std::thread::JoinHandle<()> {
        let busy = self.busy.clone();
        std::thread::spawn(move || {
            let (lock, cv) = &*busy;
            let guard = lock.lock().unwrap();
            let _ = cv.wait_timeout_while(guard, CLOSE_WAIT, |n| *n > 0);
        })
    }

    /// Bibliothèque (re)chargée : ménage et proposition à refaire.
    pub fn library_loaded(&mut self) {
        self.checked = false;
        self.prompt = None;
    }
}

fn spawn_worker(busy: Busy, ctx: Option<Context>) -> (Sender<Job>, Receiver<WriteDone>) {
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("comicrust-ecriture-comicrack".into())
        .spawn(move || {
            for job in job_rx {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    comicrust_core::db::update_books(&job.cfg, &job.edits)
                }))
                .unwrap_or_else(|_| Err(comicrust_core::Error::Database("plantage pendant l'écriture".into())))
                .map_err(|e| e.to_string());
                if let (Ok(report), Some(sink)) = (&result, &job.sink) {
                    sink.lock().unwrap().extend(report.written.iter().map(|w| (w.id.clone(), w.record.counter)));
                }
                let _ = done_tx.send(WriteDone { kind: job.kind, captions: job.captions, result });
                let (lock, cv) = &*busy;
                *lock.lock().unwrap() -= 1;
                cv.notify_all();
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
            }
        })
        .expect("fil d'écriture dans ComicRack");
    (job_tx, done_rx)
}

// ---------------------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------------------

impl ComicRustApp {
    /// Base MySQL de ComicRack où écrire, si le mode écriture est choisi.
    pub(crate) fn comicrack_write_config(&self) -> Option<DbConfig> {
        if self.settings.library_mode != LibraryMode::ComicRackMySql || !self.settings.comicrack_write {
            return None;
        }
        match &self.library.source {
            Some(super::LibrarySource::ComicRackMySql(c)) => Some(c.clone()),
            _ => None,
        }
    }

    /// Bibliothèque utilisée, pour la barre d'état et le menu ⚙.
    pub(crate) fn library_mode_label(&self) -> Option<&'static str> {
        match self.settings.library_mode {
            LibraryMode::ComicRackMySql if self.settings.comicrack_write => Some("ComicRack (MySQL) — écriture"),
            LibraryMode::ComicRackMySql => Some("ComicRack (MySQL) — lecture seule"),
            LibraryMode::ComicRackXml => Some("ComicRack (ComicDb.xml) — lecture seule"),
            _ => None,
        }
    }

    fn submit_comicrack(&mut self, cfg: DbConfig, edits: Vec<BookEdit>, kind: WriteKind) {
        let captions = match self.library.database() {
            Some(db) => {
                edits.iter().filter_map(|e| Some((e.id.to_ascii_lowercase(), db.get(&e.id)?.caption()))).collect()
            }
            None => HashMap::new(),
        };
        let sink = self.library.watcher.as_ref().map(|w| w.written.clone());
        self.cr_writes.submit(Job { cfg, edits, kind, captions, sink });
    }

    /// Mode écriture : enregistre dans ComicRack la lecture du livre ouvert (une fois par
    /// livre ouvert). Renvoie un fil qui attend la fin des écritures (fermeture de la
    /// fenêtre) ; `None` si rien n'est à écrire.
    pub(crate) fn write_reading_to_comicrack(&mut self) -> Option<std::thread::JoinHandle<()>> {
        let cfg = self.comicrack_write_config()?;
        let b = self.book.as_ref()?;
        if !b.opened || b.stored {
            return None;
        }
        let db = self.library.database()?;
        let p = b.path.to_string_lossy().into_owned();
        let book = db.books.iter().find(|x| {
            x.file.as_deref().is_some_and(|f| {
                f.eq_ignore_ascii_case(&p) || self.settings.effective_path(Path::new(f)).as_path() == b.path.as_path()
            })
        })?;
        let shown = self.spread(b, b.current).into_iter().max().unwrap_or(b.current);
        let reading = Reading {
            current: b.current as u32,
            shown_max: shown as u32,
            pages: b.page_count() as u32,
            opened_at: b.opened_at,
        };
        let edit = reading_edit(book, &reading);
        let path = b.path.clone();
        if let Some(b) = &mut self.book {
            b.stored = true;
        }
        self.submit_comicrack(cfg, vec![edit], WriteKind::Reading { path });
        Some(self.cr_writes.waiter())
    }

    /// Mode écriture : fichiers convertis et infos écrites (`id`, résultat, fichier traité).
    pub(crate) fn write_files_to_comicrack(&mut self, done: Vec<(String, Done, PathBuf)>) {
        let Some(cfg) = self.comicrack_write_config() else { return };
        let mut edits = Vec::new();
        let mut errors = Vec::new();
        for (id, d, target) in &done {
            match file_edit(id, d, target) {
                Ok(e) => edits.push(e),
                Err(e) => errors.push(e),
            }
        }
        if !errors.is_empty() {
            self.bg_errors.lock().unwrap().push(format!(
                "Fichiers non enregistrés dans la base de ComicRack (fichier illisible) :\n{}",
                errors.join("\n")
            ));
        }
        if !edits.is_empty() {
            self.submit_comicrack(cfg, edits, WriteKind::Files);
        }
    }

    /// Bilans des écritures : bibliothèque, réglages locaux, erreurs.
    pub(crate) fn poll_comicrack_writes(&mut self, ctx: &Context) {
        if self.cr_writes.ctx.is_none() {
            self.cr_writes.ctx = Some(ctx.clone());
        }
        for done in self.cr_writes.poll() {
            let report = match done.result {
                Ok(r) => r,
                Err(e) => {
                    let what = match done.kind {
                        WriteKind::Reading { .. } | WriteKind::Migration { .. } => {
                            "Lecture non enregistrée dans la base de ComicRack (elle reste gardée dans ComicRust)"
                        }
                        WriteKind::Files => "Fichiers non enregistrés dans la base de ComicRack",
                    };
                    self.bg_errors.lock().unwrap().push(format!("{what} :\n{e}"));
                    continue;
                }
            };
            if let Some(c) = &mut self.covers {
                for w in &report.written {
                    c.forget_failure(&w.id, w.book.front_cover_image());
                }
            }
            self.library.apply_written(&report);
            let written: HashMap<&str, &LibraryBook> =
                report.written.iter().map(|w| (w.id.as_str(), &w.book)).collect();
            let db = self.library.database();
            let fresh = |id: &str| -> Option<LibraryBook> {
                let id = id.to_ascii_lowercase();
                written.get(id.as_str()).map(|b| (*b).clone()).or_else(|| db.as_ref()?.get(&id).cloned())
            };
            let (what, kept) = match &done.kind {
                WriteKind::Reading { path } => {
                    if let Some(id) = report.written.first().map(|w| w.id.clone()).or(report.unchanged.first().cloned())
                    {
                        if let Some(b) = fresh(&id) {
                            forget_if_synced(&mut self.settings, path, &b);
                        }
                    }
                    ("Lecture", "Elle reste gardée dans ComicRust.")
                }
                WriteKind::Migration { paths } => {
                    for (id, path) in paths {
                        if let Some(b) = fresh(id) {
                            forget_if_synced(&mut self.settings, path, &b);
                        }
                    }
                    ("Lecture", "Elle reste gardée dans ComicRust (report proposé au prochain démarrage).")
                }
                WriteKind::Files => (
                    "Fichier",
                    "Le fichier est bien traité ; ComicRust l'utilise, mais ComicRack indique l'ancien état.",
                ),
            };
            if let Some(msg) = problems(&report, &done.captions, what, kept) {
                self.bg_errors.lock().unwrap().push(msg);
            }
        }
        // Bibliothèque chargée en mode écriture : ménage des réglages et proposition de
        // report (une fois par chargement).
        if !self.cr_writes.checked && self.comicrack_write_config().is_some() {
            if let Some(db) = self.library.database() {
                self.cr_writes.checked = true;
                let pending = sort_local_progress(&mut self.settings, &db);
                if !pending.is_empty() && !self.settings.comicrack_write_migration_declined {
                    self.cr_writes.prompt = Some(pending);
                }
            }
        }
    }

    /// Proposition de reporter dans ComicRack la lecture gardée dans ComicRust.
    pub(crate) fn comicrack_write_window(&mut self, ctx: &Context) {
        let Some(pending) = &self.cr_writes.prompt else { return };
        let n = pending.len();
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("comicrust-report-lecture")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading("Reporter la lecture dans ComicRack ?");
            ui.add_space(4.0);
            ui.label(format!(
                "ComicRust a gardé dans ses réglages la lecture de {n} livre{s} de la bibliothèque, plus avancée \
                 que celle enregistrée dans ComicRack (lecture faite quand ComicRust était en lecture seule, ou \
                 écriture qui n'avait pas abouti).",
                s = if n > 1 { "s" } else { "" }
            ));
            ui.label(
                RichText::new(
                    "Le report écrit la page courante et la dernière page lue (jamais en arrière) ; la date \
                     d'ouverture, inconnue, n'est pas modifiée. ComicRack lancé le voit en quelques secondes.",
                )
                .weak(),
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for p in pending {
                    let pct = crate::settings::ReadProgress { max_page: p.max_page, pages: p.pages }
                        .percent()
                        .map(|pct| format!(" — {pct} %"))
                        .unwrap_or_default();
                    ui.label(RichText::new(format!("• {} : page {}{pct}", p.caption, p.max_page + 1)).small());
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Reporter dans ComicRack").strong()).clicked() {
                    choice = Some(0);
                }
                if ui.button("Plus tard").on_hover_text("Proposé de nouveau au prochain démarrage").clicked() {
                    choice = Some(1);
                }
                if ui.button("Ne plus demander").on_hover_text("La lecture reste gardée dans ComicRust").clicked() {
                    choice = Some(2);
                }
            });
        });
        let Some(choice) = choice else { return };
        let pending = self.cr_writes.prompt.take().unwrap_or_default();
        match choice {
            0 => {
                let (Some(cfg), Some(db)) = (self.comicrack_write_config(), self.library.database()) else { return };
                let mut edits = Vec::new();
                let mut paths = Vec::new();
                for p in &pending {
                    if let Some(b) = db.get(&p.id) {
                        edits.push(p.edit(b));
                        paths.push((p.id.clone(), p.path.clone()));
                    }
                }
                self.submit_comicrack(cfg, edits, WriteKind::Migration { paths });
            }
            2 => {
                self.settings.comicrack_write_migration_declined = true;
                self.save_now = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "comicrack_write_tests.rs"]
mod tests;
