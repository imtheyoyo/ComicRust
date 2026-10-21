//! Base de livres ComicRack sur MySQL — **lecture seule**.
//!
//! Schéma créé par ComicRack CE (`ComicStorageMySql.CreateTables`) :
//!
//! ```sql
//! create table changes (id varchar(40) primary key, update_counter bigint, delete_counter bigint);
//! create table comics  (id varchar(40) primary key, update_counter bigint, data mediumtext);
//! ```
//!
//! `comics.id` est l'identifiant du livre (le même que les `<Id>` des listes `.cbl`) et
//! `comics.data` sa fiche XML complète (`<ComicBook Id=".." File="..">…`), identique à un
//! `<Book>` de `ComicDB.xml`. ComicRust ne fait que des `SELECT`.
//!
//! **Suivi des modifications** (comme `ComicStorageBaseSql.Refresh`) : la table `changes`
//! a une seule ligne dont `update_counter` augmente à chaque écriture d'un livre (le
//! livre reçoit la nouvelle valeur dans `comics.update_counter`) et `delete_counter` à
//! chaque suppression. Il suffit donc de relire les livres dont le compteur dépasse le
//! dernier vu, et la liste des identifiants quand `delete_counter` a changé.

mod config;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use mysql::prelude::Queryable;
use mysql::{Conn, OptsBuilder};

pub use config::DbConfig;

use crate::cbl::ReadingList;
use crate::library::LibraryBook;
use crate::Error;

/// Avancement du chargement (lu depuis l'interface).
#[derive(Debug, Default)]
pub struct LoadProgress {
    pub total: AtomicUsize,
    pub done: AtomicUsize,
}

impl LoadProgress {
    pub fn get(&self) -> (usize, usize) {
        (self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed))
    }
}

/// Livres de la base, indexés par identifiant.
#[derive(Debug, Default, Clone)]
pub struct ComicDatabase {
    pub books: Vec<LibraryBook>,
    by_id: HashMap<String, usize>,
    /// Fiches illisibles (XML invalide) : ignorées mais comptées.
    pub unreadable: usize,
    /// Compteurs de la table `changes` au moment de la lecture.
    pub counters: Counters,
}

/// `changes.update_counter` et `changes.delete_counter`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counters {
    pub update: i64,
    pub delete: i64,
}

/// Modifications faites dans ComicRack depuis la dernière lecture.
#[derive(Debug, Default, Clone)]
pub struct Changes {
    /// Livres ajoutés ou modifiés (fiche complète).
    pub updated: Vec<LibraryBook>,
    /// Identifiants encore présents (seulement si des livres ont été supprimés).
    pub present: Option<HashSet<String>>,
    pub counters: Counters,
}

/// Bilan d'une actualisation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
}

fn read_counters(conn: &mut Conn) -> Result<Counters, Error> {
    // ComicRack lit la première ligne (une seule ligne, id = Guid.Empty).
    let row: Option<(Option<i64>, Option<i64>)> =
        conn.query_first("SELECT update_counter, delete_counter FROM changes").map_err(db_err)?;
    let (u, d) = row.unwrap_or_default();
    Ok(Counters { update: u.unwrap_or(0), delete: d.unwrap_or(0) })
}

/// Connexion gardée ouverte pour surveiller les modifications.
pub struct Watcher {
    cfg: DbConfig,
    conn: Option<Conn>,
    seen: Counters,
}

impl Watcher {
    pub fn new(cfg: DbConfig, seen: Counters) -> Self {
        Self { cfg, conn: None, seen }
    }

    /// Interroge la base ; `None` si rien n'a changé. En cas d'erreur, la connexion
    /// est refaite à l'appel suivant.
    pub fn poll(&mut self) -> Result<Option<Changes>, Error> {
        if self.conn.is_none() {
            self.conn = Some(connect(&self.cfg)?);
        }
        let res = Self::poll_on(self.conn.as_mut().unwrap(), self.seen);
        match res {
            Ok(Some(ch)) => {
                self.seen = ch.counters;
                Ok(Some(ch))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.conn = None;
                Err(e)
            }
        }
    }

    fn poll_on(conn: &mut Conn, seen: Counters) -> Result<Option<Changes>, Error> {
        let now = read_counters(conn)?;
        if now == seen {
            return Ok(None);
        }
        let mut ch = Changes { counters: now, ..Default::default() };
        if now.update != seen.update {
            let rows: Vec<(String, Option<String>)> =
                conn.exec("SELECT id, data FROM comics WHERE update_counter > ?", (seen.update,)).map_err(db_err)?;
            for (id, data) in rows {
                if let Some(Ok(mut book)) = data.as_deref().map(LibraryBook::from_xml) {
                    if book.id.is_empty() {
                        book.id = id;
                    }
                    ch.updated.push(book);
                }
            }
        }
        if now.delete != seen.delete {
            let ids: Vec<String> = conn.query("SELECT id FROM comics").map_err(db_err)?;
            ch.present = Some(ids.into_iter().map(|i| i.to_ascii_lowercase()).collect());
        }
        Ok(Some(ch))
    }
}

fn db_err(e: impl std::fmt::Display) -> Error {
    Error::Database(e.to_string())
}

fn connect(cfg: &DbConfig) -> Result<Conn, Error> {
    let opts = OptsBuilder::new()
        .ip_or_hostname(Some(cfg.host.clone()))
        .tcp_port(cfg.port)
        .user(Some(cfg.user.clone()))
        .pass(Some(cfg.password.clone()))
        .db_name(Some(cfg.database.clone()))
        .tcp_connect_timeout(Some(Duration::from_secs(8)))
        .read_timeout(Some(Duration::from_secs(120)))
        .prefer_socket(false);
    Conn::new(opts).map_err(|e| Error::Database(format!("connexion à {} impossible : {e}", cfg.describe())))
}

/// Vérifie la connexion et renvoie le nombre de livres de la base.
pub fn test_connection(cfg: &DbConfig) -> Result<usize, Error> {
    let mut conn = connect(cfg)?;
    let n: Option<u64> = conn
        .query_first("SELECT COUNT(*) FROM comics")
        .map_err(|e| Error::Database(format!("table « comics » illisible : {e}")))?;
    Ok(n.unwrap_or(0) as usize)
}

impl ComicDatabase {
    /// Charge toutes les fiches (lecture seule).
    pub fn load(cfg: &DbConfig, progress: Option<&LoadProgress>) -> Result<Self, Error> {
        let mut conn = connect(cfg)?;
        // Compteurs lus avant les livres (comme ComicRack) : une écriture pendant la
        // lecture sera revue à la prochaine actualisation plutôt que perdue.
        let counters = read_counters(&mut conn).unwrap_or_default();
        if let Some(p) = progress {
            let n: Option<u64> = conn.query_first("SELECT COUNT(*) FROM comics").map_err(db_err)?;
            p.total.store(n.unwrap_or(0) as usize, Ordering::Relaxed);
            p.done.store(0, Ordering::Relaxed);
        }
        let mut db = ComicDatabase { counters, ..Default::default() };
        let rows = conn.query_iter("SELECT id, data FROM comics").map_err(db_err)?;
        for row in rows {
            let (id, data): (String, Option<String>) = mysql::from_row_opt(row.map_err(db_err)?).map_err(db_err)?;
            match data.as_deref().map(LibraryBook::from_xml) {
                Some(Ok(mut book)) => {
                    if book.id.is_empty() {
                        book.id = id;
                    }
                    db.push(book);
                }
                _ => db.unreadable += 1,
            }
            if let Some(p) = progress {
                p.done.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(db)
    }

    pub fn from_books(books: Vec<LibraryBook>) -> Self {
        let mut db = ComicDatabase::default();
        for b in books {
            db.push(b);
        }
        db
    }

    fn push(&mut self, book: LibraryBook) {
        self.by_id.insert(book.id.to_ascii_lowercase(), self.books.len());
        self.books.push(book);
    }

    fn reindex(&mut self) {
        self.by_id = self.books.iter().enumerate().map(|(i, b)| (b.id.to_ascii_lowercase(), i)).collect();
    }

    /// Applique les modifications lues par un [`Watcher`].
    pub fn apply(&mut self, ch: Changes) -> Applied {
        let mut out = Applied::default();
        for book in ch.updated {
            match self.by_id.get(&book.id.to_ascii_lowercase()) {
                Some(&i) => {
                    self.books[i] = book;
                    out.modified += 1;
                }
                None => {
                    self.push(book);
                    out.added += 1;
                }
            }
        }
        if let Some(present) = ch.present {
            let before = self.books.len();
            self.books.retain(|b| present.contains(&b.id.to_ascii_lowercase()));
            out.removed = before - self.books.len();
            if out.removed > 0 {
                self.reindex();
            }
        }
        self.counters = ch.counters;
        out
    }

    pub fn get(&self, id: &str) -> Option<&LibraryBook> {
        self.index_of(id).map(|i| &self.books[i])
    }

    /// Rang du livre dans `books` (identifiant sans casse).
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id.get(&id.trim().to_ascii_lowercase()).copied()
    }

    pub fn len(&self) -> usize {
        self.books.len()
    }

    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    /// Livre de la base pour chaque entrée d'une liste `.cbl`, par identifiant.
    pub fn resolve_ids<'a>(&'a self, list: &ReadingList) -> Vec<Option<&'a LibraryBook>> {
        list.books.book.iter().map(|e| e.id.as_deref().and_then(|id| self.get(id))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fiche au format exact de ComicRack CE (`XmlUtility.ToString(ComicBook)`).
    pub const SAMPLE: &str = r#"<?xml version="1.0"?>
<ComicBook xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Id="30815f16-abb9-4c38-8c24-ecae11e25b1b" File="Z:\04_2011_2016\COSMIC COLLECTION #224 2012_05 - Thanos Rising 1-5 (Jason Aaron)(2013).cbz">
  <Series>COSMIC COLLECTION</Series>
  <Number>224</Number>
  <Volume>3</Volume>
  <Year>2012</Year>
  <Writer>Jason Aaron</Writer>
  <StoryArc>Quatrième époque : 2011-2016</StoryArc>
  <PageCount>115</PageCount>
  <Added>2019-10-01T12:58:00.1234567+02:00</Added>
  <FileSize>48173972</FileSize>
  <CurrentPage>12</CurrentPage>
  <LastPageRead>12</LastPageRead>
  <Pages><Page Image="0" Type="FrontCover" ImageWidth="1600" ImageHeight="2449" /></Pages>
</ComicBook>"#;

    #[test]
    fn parse_mysql_row() {
        let b = LibraryBook::from_xml(SAMPLE).unwrap();
        assert_eq!(b.id, "30815f16-abb9-4c38-8c24-ecae11e25b1b");
        assert!(b.file.as_deref().unwrap().ends_with("(2013).cbz"));
        assert_eq!((b.volume, b.page_count, b.current_page), (Some(3), Some(115), Some(12)));
        assert_eq!(b.caption(), "COSMIC COLLECTION v3 #224");
        let db = ComicDatabase::from_books(vec![b]);
        let list = ReadingList::from_xml(include_str!("../../tests/fixtures/liste_thanos_yoyo.cbl")).unwrap();
        let hits = db.resolve_ids(&list);
        assert!(hits[0].is_some(), "#224 retrouvé par son Id");
        assert!(hits[1..].iter().all(Option::is_none));
        assert!(db.get("30815F16-ABB9-4C38-8C24-ECAE11E25B1B").is_some(), "Id sans casse");
    }

    /// Test d'intégration contre un vrai serveur : `COMICRUST_TEST_MYSQL` =
    /// chaîne de connexion d'une base de test (créée et remplie par le test).
    #[test]
    fn mysql_server() {
        let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
            eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL ignoré");
            return;
        };
        let cfg = DbConfig::from_connection_string(&url).unwrap();
        let mut conn = connect(&cfg).unwrap();
        conn.query_drop("DROP TABLE IF EXISTS comics").unwrap();
        conn.query_drop("DROP TABLE IF EXISTS changes").unwrap();
        // Tables identiques à ComicStorageMySql.CreateTables.
        conn.query_drop("Create table changes (id varchar(40) not null primary key, update_counter bigint default 0, delete_counter bigint default 0)").unwrap();
        conn.query_drop("Create table comics (id varchar(40) not null primary key, update_counter bigint default 0, data mediumtext)").unwrap();
        conn.exec_drop("Insert into comics values (?, 1, ?)", ("30815f16-abb9-4c38-8c24-ecae11e25b1b", SAMPLE))
            .unwrap();
        conn.exec_drop("Insert into comics values (?, 2, ?)", ("aaaaaaaa-0000-0000-0000-000000000001", "<pas du xml"))
            .unwrap();
        for i in 0..250 {
            let id = format!("bbbbbbbb-0000-0000-0000-{i:012}");
            let xml = format!(
                r#"<ComicBook Id="{id}" File="/bd/serie {i:03}.cbz"><Series>Série é</Series><Number>{i}</Number></ComicBook>"#
            );
            conn.exec_drop("Insert into comics values (?, 3, ?)", (&id, &xml)).unwrap();
        }
        assert_eq!(test_connection(&cfg).unwrap(), 252);
        let progress = LoadProgress::default();
        let db = ComicDatabase::load(&cfg, Some(&progress)).unwrap();
        assert_eq!(progress.get(), (252, 252));
        assert_eq!((db.len(), db.unreadable), (251, 1));
        assert_eq!(db.get("bbbbbbbb-0000-0000-0000-000000000042").unwrap().series.as_deref(), Some("Série é"));
        assert_eq!(db.get("30815f16-abb9-4c38-8c24-ecae11e25b1b").unwrap().number.as_deref(), Some("224"));

        // ComicRack modifie un livre, en ajoute un et en supprime un (mêmes requêtes que
        // ComicStorageBaseSql.Write / Delete).
        conn.query_drop("Insert into changes values ('00000000-0000-0000-0000-000000000000', 3, 0)").unwrap();
        let mut db = ComicDatabase::load(&cfg, None).unwrap();
        assert_eq!(db.counters, Counters { update: 3, delete: 0 });
        let mut w = Watcher::new(cfg.clone(), db.counters);
        assert!(w.poll().unwrap().is_none(), "rien n'a changé");
        let id42 = "bbbbbbbb-0000-0000-0000-000000000042";
        let xml42 = format!(
            r#"<ComicBook Id="{id42}" File="/bd/x.cbz"><Series>Modifiée</Series><CurrentPage>7</CurrentPage></ComicBook>"#
        );
        conn.exec_drop("Update comics set data=?, update_counter=4 where id=?", (&xml42, id42)).unwrap();
        conn.exec_drop(
            "Insert into comics values ('cccccccc-0000-0000-0000-000000000001', 5, ?)",
            (r#"<ComicBook Id="cccccccc-0000-0000-0000-000000000001"><Series>Nouveau</Series></ComicBook>"#,),
        )
        .unwrap();
        conn.query_drop("Update changes set update_counter=5").unwrap();
        conn.query_drop("Delete from comics where id='bbbbbbbb-0000-0000-0000-000000000001'").unwrap();
        conn.query_drop("Update changes set delete_counter=1").unwrap();
        let ch = w.poll().unwrap().expect("modifications vues");
        assert_eq!(ch.updated.len(), 2);
        let applied = db.apply(ch);
        assert_eq!(applied, Applied { added: 1, modified: 1, removed: 1 });
        assert_eq!(db.len(), 251);
        assert_eq!(db.get(id42).unwrap().current_page, Some(7));
        assert!(db.get("bbbbbbbb-0000-0000-0000-000000000001").is_none());
        assert_eq!(db.get("cccccccc-0000-0000-0000-000000000001").unwrap().series.as_deref(), Some("Nouveau"));
        assert!(w.poll().unwrap().is_none());

        let mut bad = cfg.clone();
        bad.password = "mauvais".into();
        let err = test_connection(&bad).unwrap_err().to_string();
        assert!(!err.contains("mauvais"), "le mot de passe ne doit pas apparaître : {err}");
    }
}
