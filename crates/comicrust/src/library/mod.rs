//! Bibliothèque : livres de la base MySQL de ComicRack, chargés en arrière-plan,
//! filtrés et triés pour la vue Bibliothèque.

pub mod covers;

use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use comicrust_core::db::{ComicDatabase, DbConfig, LoadProgress};
use comicrust_core::library::LibraryBook;
use eframe::egui;

pub enum Status {
    /// Aucune base configurée.
    NotConfigured,
    Loading {
        rx: Receiver<Result<ComicDatabase, String>>,
        progress: Arc<LoadProgress>,
        what: String,
    },
    Loaded(Arc<ComicDatabase>),
    Failed(String),
}

/// Ligne de la vue : titre de série ou rangée de couvertures.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Header { series: String, count: usize },
    Covers(Vec<usize>),
}

pub struct LibraryState {
    pub status: Status,
    pub search: String,
    /// Indices des livres affichés, dans l'ordre (cache du filtre et du tri).
    view: Vec<usize>,
    view_key: Option<(String, bool, usize)>,
    /// Indique au panneau d'afficher le résultat d'un test de connexion.
    pub last_loaded_from: Option<String>,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            status: Status::NotConfigured,
            search: String::new(),
            view: Vec::new(),
            view_key: None,
            last_loaded_from: None,
        }
    }
}

/// Clé de tri : série, volume, numéro (ordre naturel), titre.
fn sort_key(b: &LibraryBook) -> (String, i32, String, String) {
    (
        b.series.clone().unwrap_or_default().to_lowercase(),
        b.volume.unwrap_or(-1),
        b.number.clone().unwrap_or_default(),
        b.title.clone().unwrap_or_default().to_lowercase(),
    )
}

fn matches(b: &LibraryBook, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let hay = format!(
        "{} {} {} {} {} {} {}",
        b.caption(),
        b.series.as_deref().unwrap_or(""),
        b.writer.as_deref().unwrap_or(""),
        b.penciller.as_deref().unwrap_or(""),
        b.story_arc.as_deref().unwrap_or(""),
        b.year.map(|y| y.to_string()).unwrap_or_default(),
        b.file.as_deref().unwrap_or(""),
    )
    .to_lowercase();
    words.iter().all(|w| hay.contains(w))
}

impl LibraryState {
    /// Lance le chargement de la base dans un fil (lecture seule).
    pub fn start_loading(&mut self, cfg: DbConfig, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(LoadProgress::default());
        let p = progress.clone();
        let ctx = ctx.clone();
        let what = cfg.describe();
        std::thread::Builder::new()
            .name("comicrust-db".into())
            .spawn(move || {
                let _ = tx.send(ComicDatabase::load(&cfg, Some(&p)).map_err(|e| e.to_string()));
                ctx.request_repaint();
            })
            .expect("fil de chargement de la base");
        self.status = Status::Loading { rx, progress, what: what.clone() };
        self.last_loaded_from = Some(what);
        self.view_key = None;
    }

    /// À appeler à chaque image : renvoie `true` quand la base vient d'arriver.
    pub fn poll(&mut self) -> bool {
        if let Status::Loading { rx, .. } = &self.status {
            if let Ok(res) = rx.try_recv() {
                self.status = match res {
                    Ok(db) => Status::Loaded(Arc::new(db)),
                    Err(e) => Status::Failed(e),
                };
                self.view_key = None;
                return matches!(self.status, Status::Loaded(_));
            }
        }
        false
    }

    pub fn database(&self) -> Option<Arc<ComicDatabase>> {
        match &self.status {
            Status::Loaded(db) => Some(db.clone()),
            _ => None,
        }
    }

    /// Nombre de livres affichés lors du dernier calcul de la vue.
    pub fn shown(&self) -> usize {
        self.view.len()
    }

    /// Livres affichés (filtrés par la recherche, triés).
    pub fn view(&mut self, group: bool) -> &[usize] {
        let Some(db) = self.database() else {
            self.view.clear();
            return &self.view;
        };
        let key = (self.search.clone(), group, db.len());
        if self.view_key.as_ref() != Some(&key) {
            let words: Vec<String> = self.search.to_lowercase().split_whitespace().map(str::to_string).collect();
            let mut v: Vec<usize> = (0..db.books.len()).filter(|&i| matches(&db.books[i], &words)).collect();
            v.sort_by(|&a, &b| {
                let (ka, kb) = (sort_key(&db.books[a]), sort_key(&db.books[b]));
                ka.0.cmp(&kb.0).then(ka.1.cmp(&kb.1)).then_with(|| natord::compare(&ka.2, &kb.2)).then(ka.3.cmp(&kb.3))
            });
            self.view = v;
            self.view_key = Some(key);
        }
        &self.view
    }

    /// Découpe la vue en rangées de `columns` couvertures, avec un titre par série si `group`.
    pub fn rows(&mut self, group: bool, columns: usize) -> Vec<Row> {
        let columns = columns.max(1);
        let db = self.database();
        let view = self.view(group).to_vec();
        let Some(db) = db else { return Vec::new() };
        let mut rows = Vec::new();
        if !group {
            rows.extend(view.chunks(columns).map(|c| Row::Covers(c.to_vec())));
            return rows;
        }
        let mut i = 0;
        while i < view.len() {
            let series = db.books[view[i]].series.clone().unwrap_or_default();
            let start = i;
            while i < view.len() && db.books[view[i]].series.clone().unwrap_or_default() == series {
                i += 1;
            }
            let group_items = &view[start..i];
            rows.push(Row::Header {
                series: if series.is_empty() { "(sans série)".into() } else { series },
                count: group_items.len(),
            });
            rows.extend(group_items.chunks(columns).map(|c| Row::Covers(c.to_vec())));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: &str, series: &str, number: &str, writer: &str) -> LibraryBook {
        LibraryBook {
            id: id.into(),
            series: Some(series.into()),
            number: Some(number.into()),
            writer: Some(writer.into()),
            file: Some(format!("Z:\\{series} {number}.cbz")),
            ..Default::default()
        }
    }

    fn state() -> LibraryState {
        let db = ComicDatabase::from_books(vec![
            book("a", "Thanos", "10", "Jim Starlin"),
            book("b", "Thanos", "2", "Jim Starlin"),
            book("c", "COSMIC COLLECTION", "224", "Jason Aaron"),
            book("d", "Warlock", "1", "Jim Starlin"),
            book("e", "Thanos", "1", "Jim Starlin"),
        ]);
        LibraryState { status: Status::Loaded(Arc::new(db)), ..Default::default() }
    }

    #[test]
    fn sorted_by_series_then_natural_number() {
        let mut s = state();
        let db = s.database().unwrap();
        let ids: Vec<&str> = s.view(true).iter().map(|&i| db.books[i].id.as_str()).collect();
        assert_eq!(ids, ["c", "e", "b", "a", "d"]);
    }

    #[test]
    fn search_all_words_case_insensitive() {
        let mut s = state();
        s.search = "starlin THANOS".into();
        assert_eq!(s.view(true).len(), 3);
        s.search = "aaron".into();
        assert_eq!(s.view(true).len(), 1);
    }

    #[test]
    fn rows_with_series_headers() {
        let mut s = state();
        let rows = s.rows(true, 2);
        assert_eq!(rows[0], Row::Header { series: "COSMIC COLLECTION".into(), count: 1 });
        assert!(matches!(&rows[1], Row::Covers(v) if v.len() == 1));
        assert_eq!(rows[2], Row::Header { series: "Thanos".into(), count: 3 });
        assert!(matches!(&rows[3], Row::Covers(v) if v.len() == 2));
        assert!(matches!(&rows[4], Row::Covers(v) if v.len() == 1));
        assert_eq!(s.rows(false, 2).len(), 3);
    }
}
