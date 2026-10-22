//! Travaux sur les fichiers BD, faits dans un fil : écrire les infos de la base dans les
//! CBZ, convertir les CBR / CB7 / CBT en CBZ. La base ComicRack n'est jamais modifiée.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use comicrust_core::db::{self, DbConfig, Record};
use comicrust_core::library::LibraryBook;
use comicrust_core::writer::{self, Outcome};
use eframe::egui;

/// Dossier (à côté de l'original) où l'original converti est rangé, si demandé.
pub const ORIGINALS_DIR: &str = "_originaux";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    WriteInfo,
    Convert,
}

impl FileAction {
    pub fn title(self) -> &'static str {
        match self {
            FileAction::WriteInfo => "Écrire les infos dans les fichiers",
            FileAction::Convert => "Convertir en CBZ",
        }
    }

    /// Le fichier se prête-t-il à l'action (d'après son extension) ?
    pub fn accepts(self, path: &Path) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        match self {
            FileAction::WriteInfo => matches!(ext.as_str(), "cbz" | "zip"),
            FileAction::Convert => matches!(ext.as_str(), "cbr" | "rar" | "cb7" | "7z" | "cbt" | "tar"),
        }
    }
}

/// Livre concerné.
#[derive(Debug, Clone)]
pub struct Target {
    pub id: String,
    pub caption: String,
    pub path: PathBuf,
}

impl Target {
    pub fn of(book: &LibraryBook, path: PathBuf) -> Self {
        Self { id: book.id.clone(), caption: book.caption(), path }
    }
}

/// Ce qui a été fait pour un livre.
#[derive(Debug, Clone, PartialEq)]
pub enum Done {
    /// Infos écrites (`written`) ou déjà à jour ; `counter` : compteur de la fiche lue.
    Info { written: bool, counter: i64 },
    /// CBZ créé ; `moved` : nouvel emplacement de l'original, s'il a été rangé.
    Converted { cbz: PathBuf, pages: usize, moved: Option<PathBuf> },
}

#[derive(Debug, Clone)]
pub struct ItemResult {
    pub target: Target,
    pub outcome: Result<Done, String>,
}

/// Confirmation demandée avant de lancer un travail.
pub struct FileDialog {
    pub action: FileAction,
    pub targets: Vec<Target>,
    /// Livres écartés (format qui ne convient pas, sans fichier).
    pub ignored: usize,
    pub move_original: bool,
}

/// Travail en cours ou terminé.
pub struct FileJob {
    pub action: FileAction,
    pub total: usize,
    pub done: Arc<AtomicUsize>,
    pub cancel: Arc<AtomicBool>,
    pub current: Arc<Mutex<String>>,
    rx: Receiver<ItemResult>,
    pub results: Vec<ItemResult>,
    /// Base illisible (les fiches n'ont pas pu être relues).
    pub db_error: Arc<Mutex<Option<String>>>,
    pub finished: Arc<AtomicBool>,
    pub started: chrono::DateTime<chrono::Local>,
}

/// Range l'original dans `_originaux` (même dossier) ; un nom libre est choisi au besoin.
fn move_original(path: &Path) -> Result<PathBuf, String> {
    let dir = path.parent().unwrap_or(Path::new(".")).join(ORIGINALS_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("dossier {} : {e}", dir.display()))?;
    let name = path.file_name().unwrap_or_default();
    let mut dest = dir.join(name);
    let mut n = 2;
    while dest.exists() {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        dest = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    std::fs::rename(path, &dest).map_err(|e| format!("original non déplacé ({e}) ; il est resté à sa place"))?;
    Ok(dest)
}

fn run_one(
    action: FileAction,
    t: &Target,
    rec: Option<&Record>,
    db_error: Option<&str>,
    move_orig: bool,
) -> Result<Done, String> {
    match action {
        FileAction::WriteInfo => {
            let Some(rec) = rec else {
                return Err(match db_error {
                    Some(e) => format!("fiche non relue dans la base ComicRack : {e}"),
                    None => "livre absent de la base ComicRack".into(),
                });
            };
            let o = writer::write_comic_info(&t.path, &rec.data).map_err(|e| e.to_string())?;
            Ok(Done::Info { written: o == Outcome::Written, counter: rec.counter })
        }
        FileAction::Convert => {
            let c = writer::convert_to_cbz(&t.path, rec.map(|r| r.data.as_str())).map_err(|e| e.to_string())?;
            let moved = if move_orig {
                match move_original(&t.path) {
                    Ok(p) => Some(p),
                    Err(e) => return Err(format!("CBZ créé ({}), mais {e}", c.path.display())),
                }
            } else {
                None
            };
            Ok(Done::Converted { cbz: c.path, pages: c.pages, moved })
        }
    }
}

impl FileJob {
    pub fn start(
        action: FileAction,
        targets: Vec<Target>,
        cfg: Option<DbConfig>,
        move_orig: bool,
        ctx: &egui::Context,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let current = Arc::new(Mutex::new(String::new()));
        let db_error = Arc::new(Mutex::new(None));
        let finished = Arc::new(AtomicBool::new(false));
        let job = Self {
            action,
            total: targets.len(),
            done: done.clone(),
            cancel: cancel.clone(),
            current: current.clone(),
            rx,
            results: Vec::new(),
            db_error: db_error.clone(),
            finished: finished.clone(),
            started: chrono::Local::now(),
        };
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("comicrust-fichiers".into())
            .spawn(move || {
                *current.lock().unwrap() = "lecture des fiches dans la base ComicRack…".into();
                ctx.request_repaint();
                let ids: Vec<String> = targets.iter().map(|t| t.id.clone()).collect();
                let records: HashMap<String, Record> = match cfg {
                    Some(cfg) => match db::fetch_records(&cfg, &ids) {
                        Ok(r) => r,
                        Err(e) => {
                            *db_error.lock().unwrap() = Some(e.to_string());
                            HashMap::new()
                        }
                    },
                    None => {
                        *db_error.lock().unwrap() = Some("aucune base ComicRack configurée".into());
                        HashMap::new()
                    }
                };
                let db_err = db_error.lock().unwrap().clone();
                for t in targets {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    *current.lock().unwrap() = t.caption.clone();
                    ctx.request_repaint();
                    let rec = records.get(&t.id.to_ascii_lowercase());
                    let outcome = run_one(action, &t, rec, db_err.as_deref(), move_orig);
                    done.fetch_add(1, Ordering::Relaxed);
                    if tx.send(ItemResult { target: t, outcome }).is_err() {
                        break;
                    }
                }
                finished.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            })
            .expect("fil des fichiers");
        job
    }

    /// Résultats arrivés depuis le dernier appel.
    pub fn poll(&mut self) -> Vec<ItemResult> {
        let new: Vec<ItemResult> = self.rx.try_iter().collect();
        self.results.extend(new.iter().cloned());
        new
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    /// (écrits ou convertis, déjà à jour, erreurs)
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut c = (0, 0, 0);
        for r in &self.results {
            match &r.outcome {
                Ok(Done::Info { written: false, .. }) => c.1 += 1,
                Ok(_) => c.0 += 1,
                Err(_) => c.2 += 1,
            }
        }
        c
    }

    /// Bilan à copier (pour la recette).
    pub fn report(&self) -> String {
        let (ok, same, err) = self.counts();
        let mut out = format!(
            "ComicRust v{} — {} — {}\n",
            env!("CARGO_PKG_VERSION"),
            self.action.title(),
            self.started.format("%Y-%m-%d %H:%M")
        );
        let verb = if self.action == FileAction::Convert { "convertis" } else { "écrits" };
        out.push_str(&format!("{} livres traités sur {} : {ok} {verb}", self.results.len(), self.total));
        if self.action == FileAction::WriteInfo {
            out.push_str(&format!(", {same} déjà à jour"));
        }
        out.push_str(&format!(", {err} erreur(s)\n"));
        if let Some(e) = self.db_error.lock().unwrap().as_deref() {
            out.push_str(&format!("Base ComicRack : {e}\n"));
        }
        if self.cancel.load(Ordering::Relaxed) {
            out.push_str("Interrompu.\n");
        }
        out.push('\n');
        for r in &self.results {
            let line = match &r.outcome {
                Ok(Done::Info { written: true, .. }) => "écrit".to_string(),
                Ok(Done::Info { written: false, .. }) => "déjà à jour".to_string(),
                Ok(Done::Converted { cbz, pages, moved }) => {
                    let mut s = format!("→ {} ({pages} pages)", cbz.display());
                    if let Some(m) = moved {
                        s.push_str(&format!(", original rangé dans {}", m.display()));
                    }
                    s
                }
                Err(e) => format!("ERREUR : {e}"),
            };
            out.push_str(&format!("- {} — {} : {line}\n", r.target.caption, r.target.path.display()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_by_extension() {
        assert!(FileAction::WriteInfo.accepts(Path::new(r"\\NAS\BD\a.CBZ")));
        assert!(!FileAction::WriteInfo.accepts(Path::new("a.cbr")));
        assert!(FileAction::Convert.accepts(Path::new("a.cbr")));
        assert!(FileAction::Convert.accepts(Path::new("a.cb7")));
        assert!(!FileAction::Convert.accepts(Path::new("a.pdf")));
    }

    #[test]
    fn original_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.cbr");
        std::fs::write(&a, b"1").unwrap();
        let moved = move_original(&a).unwrap();
        assert_eq!(moved, dir.path().join(ORIGINALS_DIR).join("a.cbr"));
        std::fs::write(&a, b"2").unwrap();
        let again = move_original(&a).unwrap();
        assert_eq!(again, dir.path().join(ORIGINALS_DIR).join("a (2).cbr"));
        assert!(!a.exists());
    }
}
