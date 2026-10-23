//! Base de données **propre à ComicRust** (mode autonome, sans ComicRack).
//!
//! Contrairement à la base MySQL de ComicRack (une fiche XML par ligne, voir [`crate::db`]),
//! le schéma de ComicRust a **une colonne par champ** de [`LibraryBook`]. Deux moteurs :
//!
//! - **SQLite** : un fichier local (par défaut `data\comicrust.db` à côté de l'exécutable,
//!   voir [`StoreConfig::portable_sqlite`]) ;
//! - **MySQL / MariaDB** : une base partagée, éventuellement la même que ComicRack — les
//!   tables sont préfixées `comicrust_` et ne touchent pas à `comics` / `changes`.
//!
//! Tables :
//!
//! | table               | contenu                                                        |
//! |---------------------|----------------------------------------------------------------|
//! | `comicrust_books`   | `id` (GUID, clé), `update_counter`, `pages` (JSON), un champ par colonne |
//! | `comicrust_folders` | `path` : dossiers de BD surveillés                             |
//! | `comicrust_meta`    | `key` / `value` : `schema_version`, `update_counter`, `delete_counter` |
//!
//! **Suivi des modifications** — même principe que ComicRack : chaque écriture incrémente
//! `update_counter` de `comicrust_meta` et donne la nouvelle valeur aux lignes écrites ;
//! chaque suppression incrémente `delete_counter`. Un [`StoreWatcher`] relit donc les
//! lignes dont le compteur dépasse le dernier vu et produit un [`Changes`] applicable
//! avec [`ComicDatabase::apply`] : deux ComicRust peuvent partager une base.

mod conn;
mod schema;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::db::{Changes, ComicDatabase, Counters, DbConfig, LoadProgress, FULL_CHECK};
use crate::library::LibraryBook;
use crate::Error;
use conn::{Conn, Val};
use schema::{BOOKS, FOLDERS};

pub use schema::SCHEMA_VERSION;

/// Nom du fichier SQLite dans le dossier `data` (mode portable).
pub const SQLITE_FILE_NAME: &str = "comicrust.db";

/// Où se trouve la base ComicRust. Le `Debug` ne montre jamais le mot de passe MySQL
/// (celui de [`DbConfig`] le masque).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreConfig {
    /// Fichier SQLite (créé s'il n'existe pas, dossier compris).
    Sqlite(PathBuf),
    /// Base MySQL / MariaDB (doit exister ; les tables sont créées).
    MySql(DbConfig),
}

impl StoreConfig {
    /// `data\comicrust.db` dans le dossier de l'exécutable (installation portable).
    pub fn portable_sqlite() -> Result<Self, Error> {
        let exe = std::env::current_exe()?;
        let dir = exe.parent().unwrap_or(Path::new("."));
        Ok(StoreConfig::Sqlite(dir.join("data").join(SQLITE_FILE_NAME)))
    }

    /// Description lisible, sans mot de passe.
    pub fn describe(&self) -> String {
        match self {
            StoreConfig::Sqlite(p) => format!("SQLite {}", p.display()),
            StoreConfig::MySql(c) => format!("MySQL {}", c.describe()),
        }
    }
}

/// Nouvel identifiant de livre : GUID en minuscules avec tirets, comme ComicRack
/// (`30815f16-abb9-4c38-8c24-ecae11e25b1b`).
pub fn new_book_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Nombre de lignes par `REPLACE` multiple en MySQL (moins d'allers-retours réseau ;
/// 200 × 71 paramètres reste loin de la limite de 65 535).
const MYSQL_ROWS_PER_STATEMENT: usize = 200;

/// Connexion à la base ComicRust. `Send` : utilisable depuis un fil d'arrière-plan.
pub struct Store {
    conn: Conn,
}

impl Store {
    /// Ouvre la base et crée le schéma s'il est absent (ou le complète / migre).
    pub fn open(cfg: &StoreConfig) -> Result<Store, Error> {
        let mut conn = match cfg {
            StoreConfig::Sqlite(p) => Conn::open_sqlite(p)?,
            StoreConfig::MySql(c) => Conn::open_mysql(c)?,
        };
        conn.write_tx(schema::ensure)?;
        Ok(Store { conn })
    }

    /// Vérifie l'accès à la base (en créant le schéma si besoin) et renvoie le nombre de livres.
    pub fn test(cfg: &StoreConfig) -> Result<usize, Error> {
        Store::open(cfg)?.count()
    }

    /// Nombre de livres.
    pub fn count(&mut self) -> Result<usize, Error> {
        let rows = self.conn.query(&format!("SELECT COUNT(*) FROM {BOOKS}"), &[])?;
        Ok(rows.into_iter().next().and_then(|r| r.into_iter().next()).and_then(|v| v.as_i64()).unwrap_or(0) as usize)
    }

    /// Compteurs actuels (`update_counter`, `delete_counter` de `comicrust_meta`).
    pub fn counters(&mut self) -> Result<Counters, Error> {
        read_counters(&mut self.conn)
    }

    /// Charge tous les livres, comme [`ComicDatabase::load`] pour la base de ComicRack :
    /// `rows` = `update_counter` de chaque ligne (identifiant en minuscules), `counters`
    /// = compteurs lus dans la même transaction.
    pub fn load(&mut self, progress: Option<&LoadProgress>) -> Result<ComicDatabase, Error> {
        self.conn.read_tx(|conn| {
            let counters = read_counters(conn)?;
            if let Some(p) = progress {
                let n = conn.query(&format!("SELECT COUNT(*) FROM {BOOKS}"), &[])?;
                let n = n.into_iter().next().and_then(|r| r.into_iter().next()).and_then(|v| v.as_i64());
                p.total.store(n.unwrap_or(0) as usize, Ordering::Relaxed);
                p.done.store(0, Ordering::Relaxed);
            }
            let mut books = Vec::new();
            let mut rows = HashMap::new();
            conn.query_each(&format!("SELECT {} FROM {BOOKS}", schema::column_list()), &[], |row| {
                let (book, counter) = schema::book_from_row(row);
                rows.insert(book.id.to_ascii_lowercase(), counter);
                books.push(book);
                if let Some(p) = progress {
                    p.done.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            })?;
            let mut db = ComicDatabase::from_books(books);
            db.counters = counters;
            db.rows = rows;
            Ok(db)
        })
    }

    /// Livres d'après leurs identifiants (sans casse) ; les absents sont ignorés.
    pub fn get(&mut self, ids: &[String]) -> Result<Vec<LibraryBook>, Error> {
        let mut out = Vec::new();
        for chunk in ids.chunks(200) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT {} FROM {BOOKS} WHERE `id` IN ({marks})", schema::column_list());
            let params: Vec<Val> = chunk.iter().map(|i| Val::Text(i.clone())).collect();
            self.conn.query_each(&sql, &params, |row| {
                out.push(schema::book_from_row(row).0);
                Ok(())
            })?;
        }
        Ok(out)
    }

    /// Insère ou remplace des livres (fiche complète, identifiés par `id`), en une
    /// transaction. Le compteur global est incrémenté une fois et sa nouvelle valeur,
    /// renvoyée, est donnée à toutes les lignes écrites. Un livre sans `id` est refusé.
    pub fn upsert(&mut self, books: &[LibraryBook]) -> Result<i64, Error> {
        if let Some(b) = books.iter().find(|b| b.id.trim().is_empty()) {
            return Err(Error::Database(format!("livre sans identifiant : {}", b.caption())));
        }
        let mysql = matches!(self.conn, Conn::MySql(_));
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "update_counter")?;
            if books.is_empty() {
                return Ok(counter);
            }
            let cols = schema::all_columns();
            let one = format!("({})", vec!["?"; cols.len()].join(","));
            let head = format!("REPLACE INTO {BOOKS} ({}) VALUES ", schema::column_list());
            let per = if mysql { MYSQL_ROWS_PER_STATEMENT } else { 1 };
            let mut params = Vec::with_capacity(cols.len() * per);
            for chunk in books.chunks(per) {
                params.clear();
                for b in chunk {
                    schema::book_to_row(b, counter, &mut params);
                }
                let sql = format!("{head}{}", vec![one.as_str(); chunk.len()].join(","));
                conn.exec(&sql, &params)?;
            }
            Ok(counter)
        })
    }

    /// Supprime des livres ; incrémente `delete_counter`. Renvoie le nombre supprimé.
    pub fn delete(&mut self, ids: &[String]) -> Result<usize, Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        self.conn.write_tx(|conn| {
            let mut n = 0;
            for chunk in ids.chunks(200) {
                let marks = vec!["?"; chunk.len()].join(",");
                let params: Vec<Val> = chunk.iter().map(|i| Val::Text(i.clone())).collect();
                n += conn.exec(&format!("DELETE FROM {BOOKS} WHERE `id` IN ({marks})"), &params)? as usize;
            }
            schema::bump(conn, "delete_counter")?;
            Ok(n)
        })
    }

    /// Enregistre une lecture faite dans ComicRust : page courante, dernière page lue,
    /// date d'ouverture (texte ISO 8601) et `open_count + 1`. À appeler une fois par
    /// séance de lecture (à la fermeture du livre), pas à chaque page. Renvoie le nouveau
    /// compteur ; erreur si le livre n'existe pas.
    pub fn set_progress(
        &mut self,
        id: &str,
        current_page: u32,
        last_page_read: u32,
        opened: &str,
    ) -> Result<i64, Error> {
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "update_counter")?;
            let n = conn.exec(
                &format!(
                    "UPDATE {BOOKS} SET `current_page` = ?, `last_page_read` = ?, `opened` = ?, \
                     `open_count` = COALESCE(`open_count`, 0) + 1, `update_counter` = ? WHERE `id` = ?"
                ),
                &[
                    Val::Int(current_page.into()),
                    Val::Int(last_page_read.into()),
                    Val::Text(opened.into()),
                    Val::Int(counter),
                    Val::Text(id.into()),
                ],
            )?;
            if n == 0 {
                // Annule aussi l'incrémentation du compteur.
                return Err(Error::Database(format!("livre {id} absent de la base")));
            }
            Ok(counter)
        })
    }

    /// Dossiers de BD surveillés, triés.
    pub fn folders(&mut self) -> Result<Vec<PathBuf>, Error> {
        let rows = self.conn.query(&format!("SELECT `path` FROM {FOLDERS} ORDER BY `path`"), &[])?;
        Ok(rows.into_iter().filter_map(|r| r.into_iter().next()?.into_text()).map(PathBuf::from).collect())
    }

    /// Remplace la liste des dossiers surveillés (doublons ignorés).
    pub fn set_folders(&mut self, folders: &[PathBuf]) -> Result<(), Error> {
        self.conn.write_tx(|conn| {
            conn.exec(&format!("DELETE FROM {FOLDERS}"), &[])?;
            let ins = conn.engine().insert_ignore();
            for f in folders {
                conn.exec(
                    &format!("{ins} INTO {FOLDERS} (`path`) VALUES (?)"),
                    &[Val::Text(f.to_string_lossy().into_owned())],
                )?;
            }
            Ok(())
        })
    }

    /// Modifications depuis `seen` (voir [`StoreWatcher::poll`]).
    fn changes_since(&mut self, seen: Counters, known: Option<&HashMap<String, i64>>) -> Result<Changes, Error> {
        self.conn.read_tx(|conn| {
            let now = read_counters(conn)?;
            let mut ch = Changes { counters: now, ..Default::default() };
            if now.update != seen.update {
                read_books(
                    conn,
                    &format!("SELECT {} FROM {BOOKS} WHERE `update_counter` > ?", schema::column_list()),
                    &[Val::Int(seen.update)],
                    &mut ch,
                )?;
            }
            if let Some(known) = known {
                // Vérification complète : lignes nouvelles, modifiées ou disparues.
                let all = conn.query(&format!("SELECT `id`, `update_counter` FROM {BOOKS}"), &[])?;
                let all: Vec<(String, i64)> = all
                    .into_iter()
                    .filter_map(|r| {
                        let mut it = r.into_iter();
                        let id = it.next()?.into_text()?;
                        Some((id, it.next().and_then(|v| v.as_i64()).unwrap_or(0)))
                    })
                    .collect();
                let fetched: HashSet<String> = ch.rows.iter().map(|(id, _)| id.clone()).collect();
                let stale: Vec<String> = all
                    .iter()
                    .filter(|(id, c)| {
                        let low = id.to_ascii_lowercase();
                        known.get(&low) != Some(c) && !fetched.contains(&low)
                    })
                    .map(|(id, _)| id.clone())
                    .collect();
                for chunk in stale.chunks(200) {
                    let marks = vec!["?"; chunk.len()].join(",");
                    let sql = format!("SELECT {} FROM {BOOKS} WHERE `id` IN ({marks})", schema::column_list());
                    let params: Vec<Val> = chunk.iter().map(|i| Val::Text(i.clone())).collect();
                    read_books(conn, &sql, &params, &mut ch)?;
                }
                let present: HashSet<String> = all.into_iter().map(|(id, _)| id.to_ascii_lowercase()).collect();
                if known.keys().any(|id| !present.contains(id)) {
                    ch.present = Some(present);
                }
            } else if now.delete != seen.delete {
                let ids = conn.query(&format!("SELECT `id` FROM {BOOKS}"), &[])?;
                ch.present = Some(
                    ids.into_iter()
                        .filter_map(|r| r.into_iter().next()?.into_text())
                        .map(|i| i.to_ascii_lowercase())
                        .collect(),
                );
            }
            Ok(ch)
        })
    }
}

fn read_counters(conn: &mut Conn) -> Result<Counters, Error> {
    Ok(Counters {
        update: schema::meta_int(conn, "update_counter")?,
        delete: schema::meta_int(conn, "delete_counter")?,
    })
}

fn read_books(conn: &mut Conn, sql: &str, params: &[Val], ch: &mut Changes) -> Result<(), Error> {
    conn.query_each(sql, params, |row| {
        let (book, counter) = schema::book_from_row(row);
        ch.rows.push((book.id.to_ascii_lowercase(), counter));
        ch.updated.push(book);
        Ok(())
    })
}

/// Surveillance de la base ComicRust (équivalent de [`crate::db::Watcher`]) : connexion
/// propre, refaite après une erreur, et vérification complète toutes les [`FULL_CHECK`].
///
/// Les écritures faites par **ce** ComicRust reviennent aussi par le suivi (le compteur a
/// bougé) : signaler-les avec [`note_written`](Self::note_written) /
/// [`note_deleted`](Self::note_deleted) pour qu'elles ne soient pas renvoyées comme
/// modifications extérieures.
pub struct StoreWatcher {
    cfg: StoreConfig,
    store: Option<Store>,
    seen: Counters,
    rows: HashMap<String, i64>,
    last_full: Instant,
}

impl StoreWatcher {
    /// `db` : la base telle qu'elle a été chargée par [`Store::load`].
    pub fn new(cfg: StoreConfig, db: &ComicDatabase) -> Self {
        Self { cfg, store: None, seen: db.counters, rows: db.rows.clone(), last_full: Instant::now() }
    }

    /// Livres écrits par ce ComicRust (`counter` = valeur renvoyée par [`Store::upsert`]
    /// ou [`Store::set_progress`]).
    pub fn note_written<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>, counter: i64) {
        for id in ids {
            self.rows.insert(id.to_ascii_lowercase(), counter);
        }
    }

    /// Livres supprimés par ce ComicRust.
    pub fn note_deleted<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) {
        for id in ids {
            self.rows.remove(&id.to_ascii_lowercase());
        }
    }

    /// Vérification complète immédiate (sinon faite toutes les `FULL_CHECK`).
    pub fn full_check(&mut self) -> Result<Option<Changes>, Error> {
        self.last_full = Instant::now() - FULL_CHECK;
        self.poll()
    }

    /// Interroge la base ; `None` si rien n'a changé (hors écritures signalées).
    pub fn poll(&mut self) -> Result<Option<Changes>, Error> {
        if self.store.is_none() {
            self.store = Some(Store::open(&self.cfg)?);
        }
        let full = self.last_full.elapsed() >= FULL_CHECK;
        let res = self.store.as_mut().unwrap().changes_since(self.seen, full.then_some(&self.rows));
        if full {
            self.last_full = Instant::now();
        }
        let mut ch = match res {
            Ok(ch) => ch,
            Err(e) => {
                self.store = None;
                return Err(e);
            }
        };
        self.seen = ch.counters;
        // Lignes déjà connues avec le même compteur (nos propres écritures) : écartées.
        let keep: Vec<bool> = ch.rows.iter().map(|(id, c)| self.rows.get(id) != Some(c)).collect();
        let mut k = keep.iter();
        ch.updated.retain(|_| *k.next().unwrap());
        let mut k = keep.iter();
        ch.rows.retain(|_| *k.next().unwrap());
        for (id, c) in &ch.rows {
            self.rows.insert(id.clone(), *c);
        }
        if let Some(present) = &ch.present {
            if self.rows.keys().all(|id| present.contains(id)) {
                ch.present = None; // seulement nos propres suppressions
            } else {
                self.rows.retain(|id, _| present.contains(id));
            }
        }
        if ch.is_empty() {
            return Ok(None);
        }
        Ok(Some(ch))
    }
}
