//! Base de données **propre à ComicRust** (mode autonome, sans ComicRack).
//!
//! Contrairement à la base MySQL de ComicRack (une fiche XML par ligne, voir [`crate::db`]),
//! le schéma de ComicRust a **une colonne par champ** de [`LibraryBook`]. Deux moteurs :
//!
//! - **SQLite** : un fichier local (par défaut `data\comicrust.db` à côté de l'exécutable,
//!   chemin recalculé à chaque ouverture, voir [`StoreConfig::PortableSqlite`]) ;
//! - **MySQL / MariaDB** : une base partagée, éventuellement la même que ComicRack — les
//!   tables sont préfixées `comicrust_` et ne touchent pas à `comics` / `changes`.
//!
//! Tables :
//!
//! | table               | contenu                                                        |
//! |---------------------|----------------------------------------------------------------|
//! | `comicrust_books`   | `id` (GUID, clé), `update_counter`, `pages` (JSON), un champ par colonne |
//! | `comicrust_folders` | `path` : dossiers de BD surveillés (MySQL : + `path_hash`, clé) |
//! | `comicrust_lists`   | listes (une ligne par liste, tous niveaux : `id`, `parent`, `position`, `name`, `kind`, `xml`), voir [`import`] ; modifiées avec [`crate::lists_edit`], enregistrées par [`Store::save_list_rows`] |
//! | `comicrust_meta`    | `key` / `value` : `schema_version`, `update_counter`, `delete_counter`, `lists_counter` |
//!
//! **Suivi des modifications** — même principe que ComicRack : chaque écriture incrémente
//! `update_counter` de `comicrust_meta` et donne la nouvelle valeur aux lignes écrites ;
//! chaque suppression incrémente `delete_counter`. Un [`StoreWatcher`] relit donc les
//! lignes dont le compteur dépasse le dernier vu et produit un [`Changes`] applicable
//! avec [`ComicDatabase::apply`] : deux ComicRust peuvent partager une base.
//!
//! **Écritures** — [`Store::upsert`] remplace des fiches **complètes** : à réserver aux
//! modifications faites par l'utilisateur sur une fiche à jour. Les écritures venant d'une
//! analyse ou d'une opération sur les fichiers passent par des écritures **ciblées**, qui
//! ne touchent qu'aux colonnes concernées et ne peuvent donc pas écraser une lecture
//! enregistrée entre-temps ([`Store::set_progress`]) : [`Store::apply_scan`],
//! [`Store::scan_into`], [`Store::replace_file`], [`Store::set_comic_info_written`].

mod conn;
pub mod import;
#[cfg(test)]
mod import_tests;
mod lists;
#[cfg(test)]
mod lists_tests;
mod schema;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::db::{Changes, ComicDatabase, Counters, DbConfig, LoadProgress, FULL_CHECK};
use crate::library::LibraryBook;
use crate::scanner::{self, ScanProgress, ScanReport};
use crate::Error;
use conn::{Conn, Engine, Val};
use schema::{BOOKS, FOLDERS};

pub use import::{read_watch_folders, ComicRackSource, ImportOptions, ImportReport};
pub use lists::ListsSnapshot;
pub use schema::SCHEMA_VERSION;

/// Nom du fichier SQLite dans le dossier `data` (mode portable).
pub const SQLITE_FILE_NAME: &str = "comicrust.db";

/// Où se trouve la base ComicRust. Le `Debug` ne montre jamais le mot de passe MySQL
/// (celui de [`DbConfig`] le masque).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreConfig {
    /// Fichier SQLite à un chemin fixe (créé par [`Store::open`] s'il n'existe pas).
    /// Gardé pour la compatibilité : préférer [`StoreConfig::PortableSqlite`] pour la base
    /// installée à côté de ComicRust.
    Sqlite(PathBuf),
    /// `data\comicrust.db` dans le dossier de l'exécutable, **recalculé à chaque
    /// ouverture** : la base suit ComicRust si son dossier est déplacé (clé USB…).
    PortableSqlite,
    /// Base MySQL / MariaDB (doit exister ; les tables sont créées).
    MySql(DbConfig),
}

impl StoreConfig {
    /// Base SQLite portable ([`StoreConfig::PortableSqlite`]). Ne peut plus échouer ; le
    /// `Result` est gardé pour la compatibilité.
    pub fn portable_sqlite() -> Result<Self, Error> {
        Ok(StoreConfig::PortableSqlite)
    }

    /// Chemin actuel de la base portable : `<dossier de l'exécutable>\data\comicrust.db`.
    pub fn portable_sqlite_path() -> Result<PathBuf, Error> {
        let exe = std::env::current_exe()?;
        let dir = exe.parent().unwrap_or(Path::new("."));
        Ok(dir.join("data").join(SQLITE_FILE_NAME))
    }

    /// Fichier SQLite visé (`None` pour MySQL).
    pub fn sqlite_path(&self) -> Result<Option<PathBuf>, Error> {
        match self {
            StoreConfig::Sqlite(p) => Ok(Some(p.clone())),
            StoreConfig::PortableSqlite => Ok(Some(Self::portable_sqlite_path()?)),
            StoreConfig::MySql(_) => Ok(None),
        }
    }

    /// Description lisible, sans mot de passe.
    pub fn describe(&self) -> String {
        match self {
            StoreConfig::Sqlite(p) => format!("SQLite {}", p.display()),
            StoreConfig::PortableSqlite => match Self::portable_sqlite_path() {
                Ok(p) => format!("SQLite portable {}", p.display()),
                Err(_) => "SQLite portable (dossier de ComicRust)".into(),
            },
            StoreConfig::MySql(c) => format!("MySQL {}", c.describe()),
        }
    }
}

/// Nouvel identifiant de livre : GUID en minuscules avec tirets, comme ComicRack
/// (`30815f16-abb9-4c38-8c24-ecae11e25b1b`).
pub fn new_book_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Nombre maximal de lignes par `REPLACE` multiple en MySQL (moins d'allers-retours
/// réseau ; 200 × 71 paramètres reste loin de la limite de 65 535).
const MYSQL_ROWS_PER_STATEMENT: usize = 200;

/// Taille maximale (estimée) des valeurs d'un `REPLACE` multiple en MySQL : 1 Mio, bien
/// sous le `max_allowed_packet` de 4 Mio des anciens serveurs (MySQL 5.5 / 5.6), en
/// laissant de la marge pour l'encodage du protocole. Une fiche seule plus grosse part
/// dans sa propre requête.
const MYSQL_BYTES_PER_STATEMENT: usize = 1024 * 1024;

/// Taille approximative d'une ligne dans le paquet envoyé au serveur.
fn row_bytes(row: &[Val]) -> usize {
    row.iter()
        .map(|v| match v {
            Val::Text(t) => t.len() + 9,
            _ => 9,
        })
        .sum()
}

/// Découpe des lignes en lots d'au plus `max_rows` lignes et `max_bytes` octets estimés
/// (au moins une ligne par lot).
fn statement_batches(rows: &[Vec<Val>], max_rows: usize, max_bytes: usize) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let (mut start, mut bytes) = (0, 0);
    for (i, row) in rows.iter().enumerate() {
        let n = row_bytes(row);
        if i > start && (i - start >= max_rows || bytes + n > max_bytes) {
            out.push(start..i);
            start = i;
            bytes = 0;
        }
        bytes += n;
    }
    if start < rows.len() {
        out.push(start..rows.len());
    }
    out
}

/// Connexion à la base ComicRust. `Send` : utilisable depuis un fil d'arrière-plan.
pub struct Store {
    conn: Conn,
}

impl Store {
    /// Ouvre la base et crée le schéma s'il est absent (ou le complète / migre).
    pub fn open(cfg: &StoreConfig) -> Result<Store, Error> {
        let mut conn = match cfg {
            StoreConfig::MySql(c) => Conn::open_mysql(c)?,
            _ => Conn::open_sqlite(&cfg.sqlite_path()?.unwrap_or_default())?,
        };
        conn.write_tx(schema::ensure)?;
        Ok(Store { conn })
    }

    /// Ouvre une base **qui doit déjà exister** (démarrage normal, surveillance, fils
    /// d'arrière-plan — tout sauf l'assistant de premier démarrage, qui utilise
    /// [`open`](Self::open)) : pour SQLite, refuse un fichier absent (« base ComicRust
    /// introuvable : … ») au lieu de créer une base vide — dossier de ComicRust déplacé
    /// avec un chemin fixe, disque non monté… ; le schéma est ensuite complété / migré
    /// comme par `open`. Pour MySQL, identique à `open` (la base doit exister sur le
    /// serveur dans les deux cas).
    pub fn open_existing(cfg: &StoreConfig) -> Result<Store, Error> {
        if let Some(p) = cfg.sqlite_path()? {
            if !p.is_file() {
                return Err(Error::Database(format!("base ComicRust introuvable : {}", p.display())));
            }
        }
        Store::open(cfg)
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
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "update_counter")?;
            let refs: Vec<&LibraryBook> = books.iter().collect();
            write_rows(conn, &refs, counter, "REPLACE")?;
            Ok(counter)
        })
    }

    /// Applique le rapport d'une analyse ([`scanner::scan`]) par des écritures **ciblées**,
    /// en une transaction (compteur incrémenté une fois, comme [`upsert`](Self::upsert)) :
    ///
    /// - `added` : insérés, sauf si la base contient déjà un livre de même identifiant ou
    ///   de même fichier (clé [`scanner::path_key`] : sans casse ni différence `\` / `/`) —
    ///   cas de deux postes qui analysent le même partage, ou d'un lot déjà enregistré par
    ///   [`scan_into`](Self::scan_into). Ces livres non insérés sont rendus dans `skipped`.
    /// - `updated` déplacé (`moved`) : seuls `file`, `file_size`, `file_modified_time`,
    ///   `file_creation_time` sont écrits ;
    /// - `updated` relu : ces colonnes + `page_count`, et — si la fiche **en base** n'a ni
    ///   `comic_info_is_dirty` ni `comic_book_is_dirty` — les champs ComicInfo
    ///   ([`scanner::COMIC_INFO_FIELDS`]) et `pages` ;
    /// - `updated` et `back` : `missing` à NULL ; `missing` : `missing` à vrai.
    ///
    /// Les autres colonnes (lecture : `current_page`, `last_page_read`, `opened`,
    /// `open_count` ; bibliothèque : `rating`, `checked`, `added`, `book_*`…) gardent la
    /// valeur **de la base**, même si les fiches du rapport ont été construites à partir
    /// d'une copie plus ancienne. Un identifiant absent de la base est ignoré.
    ///
    /// Rend les fiches écrites, **relues** après écriture (à mettre dans la bibliothèque
    /// en mémoire, et à signaler au [`StoreWatcher`] avec `counter`).
    pub fn apply_scan(&mut self, report: &ScanReport) -> Result<StoreWrite, Error> {
        if let Some(b) = report.added.iter().chain(&report.updated).find(|b| b.id.trim().is_empty()) {
            return Err(Error::Database(format!("livre sans identifiant : {}", b.caption())));
        }
        let moved: HashSet<String> = report.moved.iter().map(|i| i.to_ascii_lowercase()).collect();
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "update_counter")?;
            let c = Val::Int(counter);
            for u in &report.updated {
                if moved.contains(&u.id.to_ascii_lowercase()) {
                    update_columns(conn, u, MOVE_COLUMNS, false, counter, "")?;
                } else {
                    update_columns(conn, u, FILE_COLUMNS, false, counter, "")?;
                    update_columns(conn, u, scanner::COMIC_INFO_FIELDS, true, counter, NOT_DIRTY)?;
                }
            }
            for (ids, sql) in [
                (&report.missing, "`missing` = 1, `update_counter` = ? WHERE COALESCE(`missing`, 0) = 0 AND"),
                (&report.back, "`missing` = NULL, `update_counter` = ? WHERE `missing` IS NOT NULL AND"),
            ] {
                for chunk in ids.chunks(200) {
                    let marks = vec!["?"; chunk.len()].join(",");
                    let mut params = vec![c.clone()];
                    params.extend(chunk.iter().map(|i| Val::Text(i.clone())));
                    conn.exec(&format!("UPDATE {BOOKS} SET {sql} `id` IN ({marks})"), &params)?;
                }
            }
            let (insert, skipped) = new_books(conn, &report.added)?;
            write_rows(conn, &insert, counter, "INSERT")?;
            let mut books = Vec::new();
            conn.query_each(
                &format!("SELECT {} FROM {BOOKS} WHERE `update_counter` = ?", schema::column_list()),
                &[c],
                |row| {
                    books.push(schema::book_from_row(row).0);
                    Ok(())
                },
            )?;
            Ok(StoreWrite { counter, books, skipped })
        })
    }

    /// Analyse les dossiers ([`scanner::scan_batched`]) et enregistre le résultat **au fil
    /// de l'eau** : les nouveaux livres par lots de [`scanner::ADD_BATCH`] (une
    /// transaction par lot, voir [`apply_scan`](Self::apply_scan)), puis le reste du
    /// rapport (mis à jour, manquants, revenus) à la fin. Une fermeture pendant la première
    /// analyse d'une grosse bibliothèque ne perd donc que le lot en cours ; l'analyse
    /// suivante reprend là où elle s'est arrêtée (les fichiers enregistrés sont connus).
    ///
    /// Rend le rapport complet (pour le résumé et les erreurs) et la liste des écritures
    /// faites, dans l'ordre (chacune avec son compteur, pour le [`StoreWatcher`]). Si un
    /// lot échoue, ses livres sont retentés à la fin ; si l'écriture finale échoue,
    /// l'erreur est rendue (les lots déjà écrits restent en base).
    pub fn scan_into(
        &mut self,
        folders: &[PathBuf],
        known: &[LibraryBook],
        progress: &ScanProgress,
        cancel: &AtomicBool,
        new_id: impl Fn() -> String + Sync,
    ) -> Result<(ScanReport, Vec<StoreWrite>), Error> {
        let mut writes = Vec::new();
        let mut flushed: HashSet<String> = HashSet::new();
        let mut failed = false;
        let mut report = scanner::scan_batched(folders, known, progress, cancel, new_id, |batch| {
            if failed {
                return;
            }
            let part = ScanReport { added: batch.to_vec(), ..Default::default() };
            match self.apply_scan(&part) {
                Ok(w) => {
                    flushed.extend(batch.iter().map(|b| b.id.clone()));
                    writes.push(w);
                }
                // Base momentanément indisponible : on retentera tout à la fin.
                Err(_) => failed = true,
            }
        });
        let all = std::mem::take(&mut report.added);
        report.added = all.iter().filter(|b| !flushed.contains(&b.id)).cloned().collect();
        let last = self.apply_scan(&report);
        report.added = all;
        writes.push(last?);
        Ok((report, writes))
    }

    /// Marque des livres comme écrits dans leur fichier (`comic_info_is_dirty` à NULL),
    /// en une transaction ; rien d'autre ne change. Renvoie le nouveau compteur.
    pub fn set_comic_info_written(&mut self, ids: &[String]) -> Result<i64, Error> {
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "update_counter")?;
            for chunk in ids.chunks(200) {
                let marks = vec!["?"; chunk.len()].join(",");
                let mut params = vec![Val::Int(counter)];
                params.extend(chunk.iter().map(|i| Val::Text(i.clone())));
                conn.exec(
                    &format!(
                        "UPDATE {BOOKS} SET `comic_info_is_dirty` = NULL, `update_counter` = ? WHERE `id` IN ({marks})"
                    ),
                    &params,
                )?;
            }
            Ok(counter)
        })
    }

    /// Remplace le fichier d'un livre (après conversion en CBZ…) : colonnes du fichier
    /// (`file`, `file_size`, dates, `page_count`), champs ComicInfo et `pages` pris dans
    /// `fresh` (typiquement [`scanner::book_from_file`] du nouveau fichier), `missing` à
    /// NULL ; tout le reste (lecture, note, `book_*`…) garde la valeur de la base.
    /// Renvoie le nouveau compteur ; erreur si le livre n'existe pas.
    pub fn replace_file(&mut self, id: &str, fresh: &LibraryBook) -> Result<i64, Error> {
        let mut fresh = fresh.clone();
        fresh.id = id.to_string();
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "update_counter")?;
            let n = update_columns(conn, &fresh, FILE_COLUMNS, false, counter, "")?;
            if n == 0 {
                return Err(Error::Database(format!("livre {id} absent de la base")));
            }
            update_columns(conn, &fresh, scanner::COMIC_INFO_FIELDS, true, counter, "")?;
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

    /// Dossiers de BD surveillés, triés (ordre des octets, le même pour les deux moteurs),
    /// sans doublon.
    pub fn folders(&mut self) -> Result<Vec<PathBuf>, Error> {
        let rows = self.conn.query(&format!("SELECT `path` FROM {FOLDERS}"), &[])?;
        let mut paths: Vec<String> = rows.into_iter().filter_map(|r| r.into_iter().next()?.into_text()).collect();
        paths.sort();
        paths.dedup();
        Ok(paths.into_iter().map(PathBuf::from).collect())
    }

    /// Remplace la liste des dossiers surveillés (doublons exacts ignorés ; en MySQL, la
    /// clé est le SHA-256 du chemin, calculé par le serveur, voir `schema`).
    pub fn set_folders(&mut self, folders: &[PathBuf]) -> Result<(), Error> {
        let mut seen = HashSet::new();
        let paths: Vec<String> =
            folders.iter().map(|f| f.to_string_lossy().into_owned()).filter(|p| seen.insert(p.clone())).collect();
        self.conn.write_tx(|conn| {
            conn.exec(&format!("DELETE FROM {FOLDERS}"), &[])?;
            let sql = match conn.engine() {
                Engine::Sqlite => format!("INSERT OR IGNORE INTO {FOLDERS} (`path`) VALUES (?)"),
                Engine::MySql => {
                    format!("INSERT IGNORE INTO {FOLDERS} (`path_hash`, `path`) VALUES (SHA2(?, 256), ?)")
                }
            };
            let mysql = conn.engine() == Engine::MySql;
            for p in &paths {
                let v = Val::Text(p.clone());
                conn.exec(&sql, &if mysql { vec![v.clone(), v] } else { vec![v] })?;
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

/// Résultat d'une écriture ciblée ([`Store::apply_scan`]…).
#[derive(Debug, Default)]
pub struct StoreWrite {
    /// Compteur donné aux lignes écrites (pour [`StoreWatcher::note_written`]).
    pub counter: i64,
    /// Fiches écrites, telles qu'elles sont en base après l'écriture.
    pub books: Vec<LibraryBook>,
    /// Identifiants des livres de `added` non insérés : déjà présents en base (même
    /// identifiant ou même fichier).
    pub skipped: Vec<String>,
}

/// Colonnes qui décrivent le fichier d'un livre déplacé (contenu identique).
const MOVE_COLUMNS: &[&str] = &["file", "file_size", "file_modified_time", "file_creation_time"];

/// Colonnes qui décrivent le fichier d'un livre relu.
const FILE_COLUMNS: &[&str] = &["file", "file_size", "file_modified_time", "file_creation_time", "page_count"];

/// Condition : fiche sans modification en attente d'écriture dans le fichier (comme
/// `ComicBook.RefreshInfoFromFile` de ComicRack, qui ne relit pas les infos sinon).
const NOT_DIRTY: &str = "AND COALESCE(`comic_info_is_dirty`, 0) = 0 AND COALESCE(`comic_book_is_dirty`, 0) = 0";

/// `UPDATE` des seules colonnes `cols` (+ `pages` si `with_pages`) de la fiche `b`, avec
/// `missing` à NULL et `update_counter` = `counter`, pour la ligne `b.id` (+ `cond`).
/// Renvoie le nombre de lignes touchées.
fn update_columns(
    conn: &mut Conn,
    b: &LibraryBook,
    cols: &[&str],
    with_pages: bool,
    counter: i64,
    cond: &str,
) -> Result<u64, Error> {
    let mut set = Vec::new();
    let mut params = Vec::new();
    for (name, v) in schema::field_values(b) {
        if cols.contains(&name) {
            set.push(format!("`{name}` = ?"));
            params.push(v);
        }
    }
    if with_pages {
        set.push("`pages` = ?".into());
        params.push(schema::pages_value(b));
    }
    params.push(Val::Int(counter));
    params.push(Val::Text(b.id.clone()));
    let sql =
        format!("UPDATE {BOOKS} SET {}, `missing` = NULL, `update_counter` = ? WHERE `id` = ? {cond}", set.join(", "));
    conn.exec(&sql, &params)
}

/// Écrit des fiches complètes (`verb` = `REPLACE` ou `INSERT`) avec le compteur `counter`.
fn write_rows(conn: &mut Conn, books: &[&LibraryBook], counter: i64, verb: &str) -> Result<(), Error> {
    if books.is_empty() {
        return Ok(());
    }
    let cols = schema::all_columns();
    let one = format!("({})", vec!["?"; cols.len()].join(","));
    let head = format!("{verb} INTO {BOOKS} ({}) VALUES ", schema::column_list());
    if conn.engine() == Engine::Sqlite {
        let sql = format!("{head}{one}");
        let mut params = Vec::with_capacity(cols.len());
        for b in books {
            params.clear();
            schema::book_to_row(b, counter, &mut params);
            conn.exec(&sql, &params)?;
        }
        return Ok(());
    }
    let rows: Vec<Vec<Val>> = books
        .iter()
        .map(|b| {
            let mut row = Vec::with_capacity(cols.len());
            schema::book_to_row(b, counter, &mut row);
            row
        })
        .collect();
    for range in statement_batches(&rows, MYSQL_ROWS_PER_STATEMENT, MYSQL_BYTES_PER_STATEMENT) {
        let sql = format!("{head}{}", vec![one.as_str(); range.len()].join(","));
        let params: Vec<Val> = rows[range].iter().flatten().cloned().collect();
        conn.exec(&sql, &params)?;
    }
    Ok(())
}

/// Sépare les livres `added` à insérer de ceux déjà présents en base (même identifiant,
/// ou même fichier au sens de [`scanner::path_key`] : sans casse, `\\` = `/`) ou en double
/// dans la liste.
///
/// Recherche des fichiers déjà connus :
/// - SQLite (base locale) : tous les chemins sont relus (un parcours de table, quelques
///   dizaines de millisecondes pour 50 000 livres) et comparés par `path_key`, ce qui
///   traite aussi la casse des lettres accentuées (que `COLLATE NOCASE` ignore) ;
/// - MySQL : chaque chemin est cherché tel quel et avec les séparateurs `/` et `\\`,
///   par l'index `file`(191) ; l'interclassement de la colonne (insensible à la casse et
///   aux accents) ramène les candidats, `path_key` tranche ensuite.
fn new_books<'a>(conn: &mut Conn, added: &'a [LibraryBook]) -> Result<(Vec<&'a LibraryBook>, Vec<String>), Error> {
    let mut known_ids: HashSet<String> = HashSet::new();
    let mut known_keys: HashSet<String> = HashSet::new();
    if added.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mysql = conn.engine() == Engine::MySql;
    if !mysql {
        conn.query_each(&format!("SELECT `file` FROM {BOOKS} WHERE `file` IS NOT NULL"), &[], |r| {
            if let Some(f) = r.into_iter().next().and_then(Val::into_text) {
                known_keys.insert(scanner::path_key(&f));
            }
            Ok(())
        })?;
    }
    for chunk in added.chunks(200) {
        let marks = vec!["?"; chunk.len()].join(",");
        let ids: Vec<Val> = chunk.iter().map(|b| Val::Text(b.id.clone())).collect();
        for r in conn.query(&format!("SELECT `id` FROM {BOOKS} WHERE `id` IN ({marks})"), &ids)? {
            if let Some(id) = r.into_iter().next().and_then(Val::into_text) {
                known_ids.insert(id.to_ascii_lowercase());
            }
        }
        if !mysql {
            continue;
        }
        let mut files: Vec<Val> = Vec::new();
        for f in chunk.iter().filter_map(|b| b.file.as_deref()) {
            for v in [f.to_string(), f.replace('\\', "/"), f.replace('/', "\\")] {
                let v = Val::Text(v);
                if !files.contains(&v) {
                    files.push(v);
                }
            }
        }
        if files.is_empty() {
            continue;
        }
        let marks = vec!["?"; files.len()].join(",");
        for r in conn.query(&format!("SELECT `file` FROM {BOOKS} WHERE `file` IN ({marks})"), &files)? {
            if let Some(f) = r.into_iter().next().and_then(Val::into_text) {
                known_keys.insert(scanner::path_key(&f));
            }
        }
    }
    let (mut insert, mut skipped) = (Vec::new(), Vec::new());
    for b in added {
        let new_id = known_ids.insert(b.id.to_ascii_lowercase());
        let new_file = b.file.as_deref().is_none_or(|f| known_keys.insert(scanner::path_key(f)));
        if new_id && new_file {
            insert.push(b);
        } else {
            skipped.push(b.id.clone());
        }
    }
    Ok((insert, skipped))
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
            // Jamais de création ici : une base SQLite disparue est une erreur.
            self.store = Some(Store::open_existing(&self.cfg)?);
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
