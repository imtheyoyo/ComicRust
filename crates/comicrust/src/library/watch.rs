//! Surveillance en arrière-plan : modifications de la bibliothèque (toutes les
//! `DB_INTERVAL`) — base MySQL de ComicRack ou base propre à ComicRust (compteurs de
//! modifications), ou `ComicDb.xml` de ComicRack relu quand il change — et du `ComicDb.xml`
//! qui contient les listes de ComicRack.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use comicrust_core::comic_lists::{find_comicdb, load_comic_lists, ComicList};
use comicrust_core::db::{Changes, ComicDatabase, Watcher};
use comicrust_core::store::StoreWatcher;

use super::LibrarySource;
use eframe::egui;

pub const DB_INTERVAL: Duration = Duration::from_secs(5);
const LISTS_INTERVAL: Duration = Duration::from_secs(10);

pub enum WatchEvent {
    Db(Changes),
    /// Bibliothèque `ComicDb.xml` relue en entier.
    Replace(ComicDatabase),
    /// La base ne répond plus (`Some`) ou répond à nouveau (`None`).
    DbError(Option<String>),
    Lists {
        path: Option<PathBuf>,
        result: Result<Vec<ComicList>, String>,
    },
}

/// Où lire les listes.
#[derive(Debug, Clone, PartialEq)]
pub enum ListsSource {
    /// `ComicDb.xml` choisi à la main.
    File(PathBuf),
    /// À chercher d'après ce `ComicRack.ini`.
    FromIni(PathBuf),
    /// Listes enregistrées dans la base de ComicRust (mode autonome, après transfert).
    Store(comicrust_core::store::StoreConfig),
}

impl ListsSource {
    fn resolve(&self) -> Option<PathBuf> {
        match self {
            ListsSource::File(p) => Some(p.clone()),
            ListsSource::FromIni(ini) => find_comicdb(ini),
            ListsSource::Store(_) => None,
        }
    }
}

/// Livres écrits par ComicRust dans la base MySQL de ComicRack (`id`, nouveau
/// `comics.update_counter`), à signaler à la surveillance pour qu'elle ne les prenne pas
/// pour des modifications faites dans ComicRack (`Watcher::note_written`).
pub type WrittenSink = Arc<Mutex<Vec<(String, i64)>>>;

/// Arrête le fil quand elle est libérée.
pub struct WatchHandle {
    pub rx: Receiver<WatchEvent>,
    stop: Arc<AtomicBool>,
    /// Écritures de ComicRust à signaler (prises en compte avant chaque interrogation).
    pub written: WrittenSink,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Ce qui est surveillé pour la bibliothèque elle-même.
enum DbWatch {
    MySql(Box<Watcher>),
    Store(Box<StoreWatcher>),
    /// `ComicDb.xml` : date de modification vue au chargement.
    Xml(PathBuf, Option<SystemTime>),
}

impl DbWatch {
    fn new(source: LibrarySource, db: &ComicDatabase) -> Self {
        match source {
            LibrarySource::ComicRackMySql(cfg) => DbWatch::MySql(Box::new(Watcher::new(cfg, db))),
            LibrarySource::Standalone(cfg) => DbWatch::Store(Box::new(StoreWatcher::new(cfg, db))),
            LibrarySource::ComicRackXml(p) => {
                let m = mtime(&p);
                DbWatch::Xml(p, m)
            }
        }
    }

    /// Écritures faites par ComicRust (base MySQL de ComicRack ou base de ComicRust :
    /// modifications des scripts…), à ne pas prendre pour des modifications extérieures.
    fn note_written(&mut self, sink: &WrittenSink) {
        let written: Vec<(String, i64)> = std::mem::take(&mut *sink.lock().unwrap());
        match self {
            DbWatch::MySql(w) => w.note_written(written.iter().map(|(id, c)| (id.as_str(), *c))),
            DbWatch::Store(w) => {
                for (id, c) in &written {
                    w.note_written([id.as_str()], *c);
                }
            }
            DbWatch::Xml(..) => {}
        }
    }

    fn poll(&mut self) -> Result<Option<WatchEvent>, String> {
        match self {
            DbWatch::MySql(w) => w.poll().map(|c| c.map(WatchEvent::Db)).map_err(|e| e.to_string()),
            DbWatch::Store(w) => w.poll().map(|c| c.map(WatchEvent::Db)).map_err(|e| e.to_string()),
            DbWatch::Xml(p, seen) => {
                let now = mtime(p);
                if now == *seen {
                    return Ok(None);
                }
                // ComicRack peut être en train d'écrire : on attend que la date se stabilise.
                std::thread::sleep(Duration::from_millis(1500));
                if mtime(p) != now {
                    return Ok(None);
                }
                let db = super::load_comicdb_xml(p)?;
                *seen = now;
                Ok(Some(WatchEvent::Replace(db)))
            }
        }
    }
}

/// `db` : base à surveiller (si l'actualisation automatique est active) ;
/// `lists` : `ComicDb.xml` à lire puis à surveiller.
pub fn spawn(
    db: Option<(LibrarySource, Arc<ComicDatabase>)>,
    lists: Option<ListsSource>,
    ctx: &egui::Context,
) -> WatchHandle {
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let written: WrittenSink = Default::default();
    let sink = written.clone();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("comicrust-surveillance".into())
        .spawn(move || {
            let mut watcher = db.map(|(src, db)| DbWatch::new(src, &db));
            let mut failing = false;
            let mut lists_seen: Option<(Option<PathBuf>, Option<SystemTime>)> = None;
            let mut store_lists_seen: Option<i64> = None;
            let mut next_db = Instant::now() + DB_INTERVAL;
            let mut next_lists = Instant::now();
            while !s.load(Ordering::Relaxed) {
                let now = Instant::now();
                if let Some(ListsSource::Store(cfg)) = lists.as_ref().filter(|_| now >= next_lists) {
                    // Base de ComicRust : relues quand `lists_counter` change (transfert fait
                    // ici ou sur un autre poste) ou après une erreur (MySQL momentanément absent).
                    next_lists = now + LISTS_INTERVAL;
                    let read = comicrust_core::store::Store::open_existing(cfg).and_then(|mut st| {
                        let counter = st.lists_counter()?;
                        if store_lists_seen == Some(counter) {
                            return Ok(None);
                        }
                        Ok(Some((counter, st.comic_lists()?)))
                    });
                    let result = match read {
                        Ok(None) => None,
                        Ok(Some((counter, lists))) => {
                            store_lists_seen = Some(counter);
                            Some(Ok(lists))
                        }
                        Err(e) => {
                            store_lists_seen = None;
                            Some(Err(e.to_string()))
                        }
                    };
                    if let Some(result) = result {
                        if tx.send(WatchEvent::Lists { path: None, result }).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                }
                if let Some(src) = lists.as_ref().filter(|l| now >= next_lists && !matches!(l, ListsSource::Store(_))) {
                    next_lists = now + LISTS_INTERVAL;
                    let path = src.resolve();
                    let m = path.as_ref().and_then(mtime);
                    let state = Some((path.clone(), m));
                    if lists_seen != state {
                        lists_seen = state;
                        let result = match &path {
                            Some(p) => load_comic_lists(p).map_err(|e| e.to_string()),
                            None => Err("ComicDb.xml introuvable à côté de ComicRack.ini ni dans %APPDATA%".into()),
                        };
                        if tx.send(WatchEvent::Lists { path, result }).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                }
                if let Some(w) = watcher.as_mut() {
                    w.note_written(&sink);
                }
                if let Some(w) = watcher.as_mut().filter(|_| now >= next_db) {
                    next_db = now + DB_INTERVAL;
                    let ev = match w.poll() {
                        Ok(Some(ev)) => {
                            let back = failing.then_some(WatchEvent::DbError(None));
                            failing = false;
                            back.into_iter().chain([ev]).collect()
                        }
                        Ok(None) if failing => {
                            failing = false;
                            vec![WatchEvent::DbError(None)]
                        }
                        Ok(None) => Vec::new(),
                        Err(e) if !failing => {
                            failing = true;
                            vec![WatchEvent::DbError(Some(e))]
                        }
                        Err(_) => Vec::new(),
                    };
                    for e in ev {
                        if tx.send(e).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        })
        .expect("fil de surveillance");
    WatchHandle { rx, stop, written }
}
