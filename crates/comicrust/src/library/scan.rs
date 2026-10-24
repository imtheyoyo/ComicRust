//! Analyse des dossiers de BD (mode autonome), dans un fil : nouveaux fichiers ajoutés,
//! fichiers modifiés relus, fichiers disparus marqués « manquants », puis enregistrement
//! dans la base de ComicRust.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use comicrust_core::db::{Changes, ComicDatabase};
use comicrust_core::scanner::{ScanProgress, ScanReport};
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
                    let mut store = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
                    let folders = store.folders().map_err(|e| e.to_string())?;
                    // Enregistré au fil de l'eau (lots de 200) ; seules les colonnes venant des
                    // fichiers sont écrites : une lecture faite pendant l'analyse est gardée.
                    let (report, writes) =
                        store.scan_into(&folders, &db.books, &p, &c, new_book_id).map_err(|e| e.to_string())?;
                    let mut changes =
                        Changes { counters: store.counters().map_err(|e| e.to_string())?, ..Default::default() };
                    for w in writes {
                        changes.rows.extend(w.books.iter().map(|b| (b.id.to_ascii_lowercase(), w.counter)));
                        changes.updated.extend(w.books);
                    }
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
