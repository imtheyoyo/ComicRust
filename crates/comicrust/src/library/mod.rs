//! Bibliothèque : livres de la base MySQL de ComicRack, chargés en arrière-plan puis
//! tenus à jour (voir `watch`), filtrés, triés et regroupés pour la vue Bibliothèque.

pub mod covers;
pub mod sorting;
pub mod watch;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use comicrust_core::comic_lists::{ComicList, Members};
use comicrust_core::db::{Applied, ComicDatabase, DbConfig, LoadProgress};
use eframe::egui;

pub use sorting::{GroupKey, SortKey};

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

/// Ligne de la vue : titre de groupe ou rangée de couvertures.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Header {
        title: String,
        count: usize,
    },
    Covers(Vec<usize>),
    /// Piles (une par série) quand la vue est empilée.
    Stacks(Vec<Stack>),
}

/// Pile de livres d'une même série (vue « Empiler par série »).
#[derive(Debug, Clone, PartialEq)]
pub struct Stack {
    /// Nom de la série (« (sans série) » si vide).
    pub series: String,
    /// Livres de la série, dans l'ordre de la vue ; le premier sert de couverture.
    pub books: Vec<usize>,
}

/// Contenu calculé de toutes les listes, pour un état donné de la base et des listes.
pub struct Resolved {
    pub key: (u64, u64, chrono::NaiveDate),
    pub members: HashMap<String, Members>,
    /// Nombre de livres présents dans la base (`None` : liste non calculable).
    pub counts: HashMap<String, Option<usize>>,
}

fn compute_resolved(key: (u64, u64, chrono::NaiveDate), lists: &[ComicList], db: &ComicDatabase) -> Resolved {
    let members = comicrust_core::comic_lists::resolve_all(lists, db);
    let counts = members
        .iter()
        .map(|(id, m)| {
            let n = match m {
                Members::All => Some(db.len()),
                Members::Ids(ids) => Some(ids.iter().filter(|i| db.index_of(i).is_some()).count()),
                Members::Unknown(_) => None,
            };
            (id.clone(), n)
        })
        .collect();
    Resolved { key, members, counts }
}

/// Listes de ComicRack (`ComicDb.xml`).
#[derive(Default)]
pub struct Lists {
    pub path: Option<PathBuf>,
    pub items: Vec<ComicList>,
    pub error: Option<String>,
    /// Augmente à chaque relecture de `ComicDb.xml`.
    version: u64,
    /// Dernier calcul terminé (éventuellement un peu ancien pendant qu'un autre tourne).
    resolved: Option<Arc<Resolved>>,
    /// Calcul en cours dans un fil.
    pending: Option<((u64, u64, chrono::NaiveDate), Receiver<Resolved>)>,
}

impl Lists {
    pub fn set(&mut self, path: Option<PathBuf>, result: Result<Vec<ComicList>, String>) {
        self.path = path;
        match result {
            Ok(items) => {
                self.items = items;
                self.error = None;
            }
            Err(e) => {
                self.items.clear();
                self.error = Some(e);
            }
        }
        self.version += 1;
    }

    pub fn count(&self, id: &str) -> Option<usize> {
        self.resolved.as_ref()?.counts.get(id).copied().flatten()
    }

    /// Calcul en cours (affiché par un petit indicateur).
    pub fn computing(&self) -> bool {
        self.pending.is_some()
    }
}

/// Ce dont dépend la vue : recherche, options, liste, version de la base et des listes,
/// pile ouverte.
type ViewKey = (String, ViewOptions, Option<String>, u64, Option<String>);

/// Options d'affichage qui déterminent la vue (cache invalidé quand elles changent).
#[derive(Debug, Clone, PartialEq)]
pub struct ViewOptions {
    pub sort: SortKey,
    pub descending: bool,
    pub group: GroupKey,
}

pub struct LibraryState {
    pub status: Status,
    pub search: String,
    /// Liste ComicRack affichée (`None` : toute la bibliothèque).
    pub selected_list: Option<String>,
    /// Livre sélectionné (panneau de détails).
    pub selected_book: Option<String>,
    /// Pile ouverte (série) dans la vue empilée.
    pub open_stack: Option<String>,
    pub lists: Lists,
    /// Dernière actualisation automatique (bilan affiché dans la barre d'état).
    pub last_refresh: Option<Applied>,
    /// Erreur de la surveillance (base injoignable…), effacée au retour.
    pub watch_error: Option<String>,
    /// Augmente à chaque modification de la base (invalide le cache de la vue).
    generation: u64,
    view: Vec<usize>,
    /// Rang de chaque livre dans la liste affichée (ordre de lecture).
    positions: HashMap<usize, usize>,
    view_key: Option<ViewKey>,
    pub last_loaded_from: Option<String>,
    pub watcher: Option<watch::WatchHandle>,
    /// Paramètres de la base chargée (pour la surveillance).
    pub config: Option<DbConfig>,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            status: Status::NotConfigured,
            search: String::new(),
            selected_list: None,
            selected_book: None,
            open_stack: None,
            lists: Lists::default(),
            last_refresh: None,
            watch_error: None,
            generation: 0,
            view: Vec::new(),
            positions: HashMap::new(),
            view_key: None,
            last_loaded_from: None,
            watcher: None,
            config: None,
        }
    }
}

impl LibraryState {
    /// Lance le chargement de la base dans un fil (lecture seule).
    pub fn start_loading(&mut self, cfg: DbConfig, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(LoadProgress::default());
        let p = progress.clone();
        let ctx = ctx.clone();
        let what = cfg.describe();
        let c = cfg.clone();
        std::thread::Builder::new()
            .name("comicrust-db".into())
            .spawn(move || {
                let _ = tx.send(ComicDatabase::load(&c, Some(&p)).map_err(|e| e.to_string()));
                ctx.request_repaint();
            })
            .expect("fil de chargement de la base");
        self.status = Status::Loading { rx, progress, what: what.clone() };
        self.config = Some(cfg);
        self.watcher = None;
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

    /// Applique des modifications lues dans la base ComicRack.
    pub fn apply_changes(&mut self, ch: comicrust_core::db::Changes) -> Applied {
        let Status::Loaded(db) = &mut self.status else { return Applied::default() };
        let applied = Arc::make_mut(db).apply(ch);
        self.generation += 1;
        self.last_refresh = Some(applied);
        applied
    }

    /// Nombre de séries (piles) parmi les livres affichés.
    pub fn stacks_shown(&self) -> usize {
        let Some(db) = self.database() else { return 0 };
        self.view.iter().map(|&i| GroupKey::Series.label(&db.books[i])).collect::<std::collections::HashSet<_>>().len()
    }

    /// Nombre de livres affichés lors du dernier calcul de la vue.
    pub fn shown(&self) -> usize {
        self.view.len()
    }

    /// Recalcule le contenu des listes dans un fil quand la base, les listes ou le jour
    /// (listes « des N derniers jours ») ont changé ; récupère le résultat terminé.
    pub fn poll_lists(&mut self, ctx: &egui::Context) {
        let Some(db) = self.database() else { return };
        let key = (self.generation, self.lists.version, chrono::Local::now().date_naive());
        if let Some((k, rx)) = &self.lists.pending {
            match rx.try_recv() {
                Ok(r) => {
                    let done = *k;
                    self.lists.resolved = Some(Arc::new(r));
                    self.lists.pending = None;
                    if done == key {
                        self.view_key = None; // la liste affichée a pu changer
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => self.lists.pending = None,
            }
        }
        if self.lists.resolved.as_ref().is_some_and(|r| r.key == key) || self.lists.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let items = self.lists.items.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("comicrust-listes".into())
            .spawn(move || {
                let _ = tx.send(compute_resolved(key, &items, &db));
                ctx.request_repaint();
            })
            .expect("fil de calcul des listes");
        self.lists.pending = Some((key, rx));
    }

    /// Contenu de la liste affichée : celui du calcul en arrière-plan s'il est à jour,
    /// sinon calculé ici (une seule liste, rapide).
    fn selected_members(&self, db: &ComicDatabase) -> Members {
        let Some(list) = self.selected_list() else { return Members::All };
        let key = (self.generation, self.lists.version, chrono::Local::now().date_naive());
        if let Some(m) = self.lists.resolved.as_ref().filter(|r| r.key == key).and_then(|r| r.members.get(&list.id)) {
            return m.clone();
        }
        list.resolve(db, &self.lists.items)
    }

    /// Contenu d'une liste (pour l'affichage de son état).
    pub fn list_members(&self, id: &str) -> Option<Members> {
        let db = self.database()?;
        let list = ComicList::find(&self.lists.items, id)?;
        Some(match self.lists.resolved.as_ref().and_then(|r| r.members.get(&list.id)) {
            Some(m) => m.clone(),
            None => list.resolve(&db, &self.lists.items),
        })
    }

    pub fn selected_list(&self) -> Option<&ComicList> {
        let id = self.selected_list.as_deref()?;
        ComicList::find(&self.lists.items, id)
    }

    /// La liste affichée est-elle une liste de lecture (ordre significatif) ?
    pub fn reading_list_shown(&self) -> bool {
        self.selected_list().is_some_and(|l| matches!(l.kind, comicrust_core::comic_lists::ListKind::Ids(_)))
    }

    /// Rang d'un livre dans la liste ComicRack affichée.
    pub fn position(&self, index: usize) -> Option<usize> {
        self.positions.get(&index).copied()
    }

    /// Livres affichés (liste choisie, recherche, tri).
    pub fn view(&mut self, opts: &ViewOptions) -> &[usize] {
        let Some(db) = self.database() else {
            self.view.clear();
            return &self.view;
        };
        let key = (
            self.search.clone(),
            opts.clone(),
            self.selected_list.clone(),
            self.generation + self.lists.version,
            self.open_stack.clone(),
        );
        if self.view_key.as_ref() == Some(&key) {
            return &self.view;
        }
        // Livres de la liste choisie, dans l'ordre de la liste.
        let members = self.selected_members(&db);
        let base: Vec<usize> = match members {
            Members::All => (0..db.books.len()).collect(),
            Members::Ids(ids) => ids.iter().filter_map(|id| db.index_of(id)).collect(),
            Members::Unknown(_) => Vec::new(),
        };
        self.positions = base.iter().enumerate().map(|(pos, &i)| (i, pos)).collect();
        let words: Vec<String> = self.search.to_lowercase().split_whitespace().map(str::to_string).collect();
        let stack = self.open_stack.clone();
        let mut v: Vec<usize> = base
            .into_iter()
            .filter(|&i| sorting::matches(&db.books[i], &words))
            .filter(|&i| stack.as_ref().is_none_or(|s| GroupKey::Series.label(&db.books[i]) == *s))
            .collect();
        let positions = &self.positions;
        sorting::sort(&mut v, &db.books, opts, |i| positions.get(&i).copied());
        self.view = v;
        self.view_key = Some(key);
        &self.view
    }

    /// Découpe la vue en rangées de `columns` couvertures, avec un titre par groupe.
    /// Piles de la vue (une par série, dans l'ordre d'apparition).
    pub fn stacks(&mut self, opts: &ViewOptions) -> Vec<Stack> {
        let db = self.database();
        let view = self.view(opts).to_vec();
        let Some(db) = db else { return Vec::new() };
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut out: Vec<Stack> = Vec::new();
        for i in view {
            let series = GroupKey::Series.label(&db.books[i]);
            match index.get(&series) {
                Some(&k) => out[k].books.push(i),
                None => {
                    index.insert(series.clone(), out.len());
                    out.push(Stack { series, books: vec![i] });
                }
            }
        }
        out
    }

    /// Rangées de la vue empilée : des piles, avec un titre par groupe (sauf groupe
    /// « Série », redondant avec les piles).
    pub fn stacked_rows(&mut self, opts: &ViewOptions, columns: usize) -> Vec<Row> {
        let columns = columns.max(1);
        let stacks = self.stacks(opts);
        let Some(db) = self.database() else { return Vec::new() };
        if matches!(opts.group, GroupKey::None | GroupKey::Series) {
            return stacks.chunks(columns).map(|c| Row::Stacks(c.to_vec())).collect();
        }
        let mut rows = Vec::new();
        let mut i = 0;
        while i < stacks.len() {
            let title = opts.group.label(&db.books[stacks[i].books[0]]);
            let start = i;
            while i < stacks.len() && opts.group.label(&db.books[stacks[i].books[0]]) == title {
                i += 1;
            }
            let items = &stacks[start..i];
            rows.push(Row::Header { title, count: items.iter().map(|s| s.books.len()).sum() });
            rows.extend(items.chunks(columns).map(|c| Row::Stacks(c.to_vec())));
        }
        rows
    }

    pub fn rows(&mut self, opts: &ViewOptions, columns: usize) -> Vec<Row> {
        let columns = columns.max(1);
        let db = self.database();
        let view = self.view(opts).to_vec();
        let Some(db) = db else { return Vec::new() };
        if opts.group == GroupKey::None {
            return view.chunks(columns).map(|c| Row::Covers(c.to_vec())).collect();
        }
        let mut rows = Vec::new();
        let mut i = 0;
        while i < view.len() {
            let title = opts.group.label(&db.books[view[i]]);
            let start = i;
            while i < view.len() && opts.group.label(&db.books[view[i]]) == title {
                i += 1;
            }
            let items = &view[start..i];
            rows.push(Row::Header { title, count: items.len() });
            rows.extend(items.chunks(columns).map(|c| Row::Covers(c.to_vec())));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use comicrust_core::comic_lists::ListKind;
    use comicrust_core::library::LibraryBook;

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

    fn series() -> ViewOptions {
        ViewOptions { sort: SortKey::Series, descending: false, group: GroupKey::Series }
    }

    fn ids(s: &mut LibraryState, o: &ViewOptions) -> Vec<String> {
        let db = s.database().unwrap();
        s.view(o).iter().map(|&i| db.books[i].id.clone()).collect()
    }

    #[test]
    fn sorted_by_series_then_natural_number() {
        let mut s = state();
        assert_eq!(ids(&mut s, &series()), ["c", "e", "b", "a", "d"]);
        let desc = ViewOptions { descending: true, group: GroupKey::None, ..series() };
        assert_eq!(ids(&mut s, &desc), ["d", "a", "b", "e", "c"]);
    }

    #[test]
    fn search_all_words_case_insensitive() {
        let mut s = state();
        s.search = "starlin THANOS".into();
        assert_eq!(s.view(&series()).len(), 3);
        s.search = "aaron".into();
        assert_eq!(s.view(&series()).len(), 1);
    }

    #[test]
    fn rows_with_group_headers() {
        let mut s = state();
        let rows = s.rows(&series(), 2);
        assert_eq!(rows[0], Row::Header { title: "COSMIC COLLECTION".into(), count: 1 });
        assert!(matches!(&rows[1], Row::Covers(v) if v.len() == 1));
        assert_eq!(rows[2], Row::Header { title: "Thanos".into(), count: 3 });
        assert!(matches!(&rows[3], Row::Covers(v) if v.len() == 2));
        assert!(matches!(&rows[4], Row::Covers(v) if v.len() == 1));
        let flat = ViewOptions { group: GroupKey::None, ..series() };
        assert_eq!(s.rows(&flat, 2).len(), 3);
        let by_writer = ViewOptions { group: GroupKey::Writer, ..series() };
        let rows = s.rows(&by_writer, 10);
        assert_eq!(rows[0], Row::Header { title: "Jason Aaron".into(), count: 1 });
        assert_eq!(rows[2], Row::Header { title: "Jim Starlin".into(), count: 4 });
    }

    #[test]
    fn reading_list_keeps_its_order() {
        let mut s = state();
        s.lists.items = vec![ComicList {
            id: "l".into(),
            name: "Ordre".into(),
            description: String::new(),
            kind: ListKind::Ids(vec!["d".into(), "zzz".into(), "a".into(), "c".into()]),
            comicrack_count: 0,
        }];
        s.selected_list = Some("l".into());
        let o = ViewOptions { sort: SortKey::Position, descending: false, group: GroupKey::None };
        assert_eq!(ids(&mut s, &o), ["d", "a", "c"], "identifiant absent de la base ignoré");
        let db = s.database().unwrap();
        assert_eq!(s.position(db.index_of("c").unwrap()), Some(2));
    }

    #[test]
    fn changes_refresh_the_view() {
        let mut s = state();
        assert_eq!(s.view(&series()).len(), 5);
        let mut ch = comicrust_core::db::Changes::default();
        ch.updated.push(book("f", "Adam Warlock", "1", "Jim Starlin"));
        let applied = s.apply_changes(ch);
        assert_eq!(applied.added, 1);
        assert_eq!(ids(&mut s, &series())[0], "f");
    }

    #[test]
    fn stacks_by_series() {
        let mut s = state();
        let flat = ViewOptions { group: GroupKey::None, ..series() };
        let stacks = s.stacks(&flat);
        let names: Vec<(&str, usize)> = stacks.iter().map(|x| (x.series.as_str(), x.books.len())).collect();
        assert_eq!(names, [("COSMIC COLLECTION", 1), ("Thanos", 3), ("Warlock", 1)]);
        let db = s.database().unwrap();
        assert_eq!(db.books[stacks[1].books[0]].id, "e", "couverture de la pile : Thanos #1");
        assert_eq!(s.stacked_rows(&flat, 2).len(), 2);
        // Pile ouverte : seulement ses livres.
        s.open_stack = Some("Thanos".into());
        assert_eq!(ids(&mut s, &flat), ["e", "b", "a"]);
        let by_writer = ViewOptions { group: GroupKey::Writer, ..series() };
        s.open_stack = None;
        let rows = s.stacked_rows(&by_writer, 5);
        assert_eq!(rows[0], Row::Header { title: "Jason Aaron".into(), count: 1 });
        assert!(matches!(&rows[3], Row::Stacks(v) if v.len() == 2), "Starlin : Thanos et Warlock");
    }
}
