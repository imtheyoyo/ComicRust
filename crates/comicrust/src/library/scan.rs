//! Analyse des dossiers de BD (mode autonome), dans un fil : nouveaux fichiers ajoutés,
//! fichiers modifiés relus, fichiers disparus marqués « manquants », puis enregistrement
//! dans la base de ComicRust.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use comicrust_core::db::{Changes, ComicDatabase};
use comicrust_core::scanner::{self, ScanProgress, ScanReport};
use comicrust_core::store::{new_book_id, Store, StoreConfig};
use eframe::egui;

/// Résultat d'une analyse : bilan et modifications à appliquer à la bibliothèque affichée.
pub struct ScanDone {
    pub report: ScanReport,
    pub changes: Changes,
    pub folders: usize,
}

pub struct ScanJob {
    pub progress: Arc<ScanProgress>,
    pub cancel: Arc<AtomicBool>,
    rx: Receiver<Result<ScanDone, String>>,
}

/// Applique un rapport d'analyse à la base ; renvoie les fiches écrites.
fn apply(store: &mut Store, db: &ComicDatabase, report: &ScanReport) -> Result<Changes, String> {
    let err = |e: comicrust_core::Error| e.to_string();
    let mut written: Vec<comicrust_core::library::LibraryBook> = Vec::new();
    written.extend(report.added.iter().cloned());
    written.extend(report.updated.iter().cloned());
    // Manquants / revenus : seul l'indicateur change (sauf si la fiche vient d'être relue).
    let flag = |ids: &[String], missing: bool, written: &mut Vec<_>| {
        for id in ids {
            if written.iter().any(|b: &comicrust_core::library::LibraryBook| b.id.eq_ignore_ascii_case(id)) {
                continue;
            }
            if let Some(b) = db.get(id) {
                let mut b = b.clone();
                b.missing = missing.then_some(true);
                written.push(b);
            }
        }
    };
    flag(&report.missing, true, &mut written);
    flag(&report.back, false, &mut written);
    for b in written.iter_mut().filter(|b| report.back.iter().any(|id| id.eq_ignore_ascii_case(&b.id))) {
        b.missing = None;
    }
    if written.is_empty() {
        return Ok(Changes::default());
    }
    let counter = store.upsert(&written).map_err(err)?;
    let counters = store.counters().map_err(err)?;
    let rows = written.iter().map(|b| (b.id.to_ascii_lowercase(), counter)).collect();
    Ok(Changes { updated: written, present: None, counters, rows })
}

impl ScanJob {
    /// Lance l'analyse des dossiers enregistrés dans la base `cfg`.
    pub fn start(cfg: StoreConfig, db: Arc<ComicDatabase>, ctx: &egui::Context) -> Self {
        let progress = Arc::new(ScanProgress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c, ctx2) = (progress.clone(), cancel.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("comicrust-analyse".into())
            .spawn(move || {
                let run = || -> Result<ScanDone, String> {
                    let mut store = Store::open(&cfg).map_err(|e| e.to_string())?;
                    let folders = store.folders().map_err(|e| e.to_string())?;
                    let report = scanner::scan(&folders, &db.books, &p, &c, new_book_id);
                    let changes = apply(&mut store, &db, &report)?;
                    Ok(ScanDone { report, changes, folders: folders.len() })
                };
                let _ = tx.send(run());
                ctx2.request_repaint();
            })
            .expect("fil d'analyse des dossiers");
        Self { progress, cancel, rx }
    }

    pub fn poll(&self) -> Option<Result<ScanDone, String>> {
        self.rx.try_recv().ok()
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
