//! Connexion à l'un des deux moteurs (SQLite ou MySQL), derrière une même interface.
//!
//! Les deux moteurs acceptent les paramètres `?`, `REPLACE INTO` et les noms entre
//! accents graves (`` `key` ``) : le SQL est donc commun, à quelques fragments près
//! (types de colonnes, `INSERT OR IGNORE` / `INSERT IGNORE`…) fournis par [`Engine`].

use std::time::Duration;

use mysql::prelude::Queryable;

use crate::Error;

/// Moteur de la base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engine {
    Sqlite,
    MySql,
}

impl Engine {
    /// `INSERT` qui ignore une clé déjà présente.
    pub(crate) fn insert_ignore(self) -> &'static str {
        match self {
            Engine::Sqlite => "INSERT OR IGNORE",
            Engine::MySql => "INSERT IGNORE",
        }
    }

    /// Conversion d'un texte en entier dans une expression SQL.
    pub(crate) fn int_cast(self, expr: &str) -> String {
        match self {
            Engine::Sqlite => format!("CAST({expr} AS INTEGER)"),
            Engine::MySql => format!("CAST({expr} AS SIGNED)"),
        }
    }
}

/// Valeur d'une cellule, commune aux deux moteurs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Val {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
}

impl rusqlite::ToSql for Val {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, ValueRef};
        Ok(ToSqlOutput::Borrowed(match self {
            Val::Null => ValueRef::Null,
            Val::Int(i) => ValueRef::Integer(*i),
            Val::Real(r) => ValueRef::Real(*r),
            Val::Text(s) => ValueRef::Text(s.as_bytes()),
        }))
    }
}

impl From<rusqlite::types::ValueRef<'_>> for Val {
    fn from(v: rusqlite::types::ValueRef<'_>) -> Self {
        use rusqlite::types::ValueRef;
        match v {
            ValueRef::Null => Val::Null,
            ValueRef::Integer(i) => Val::Int(i),
            ValueRef::Real(r) => Val::Real(r),
            ValueRef::Text(t) | ValueRef::Blob(t) => Val::Text(String::from_utf8_lossy(t).into_owned()),
        }
    }
}

impl From<&Val> for mysql::Value {
    fn from(v: &Val) -> Self {
        match v {
            Val::Null => mysql::Value::NULL,
            Val::Int(i) => mysql::Value::Int(*i),
            Val::Real(r) => mysql::Value::Double(*r),
            Val::Text(s) => mysql::Value::Bytes(s.as_bytes().to_vec()),
        }
    }
}

impl From<mysql::Value> for Val {
    fn from(v: mysql::Value) -> Self {
        use mysql::Value as V;
        match v {
            V::NULL => Val::Null,
            V::Int(i) => Val::Int(i),
            V::UInt(u) => Val::Int(i64::try_from(u).unwrap_or(i64::MAX)),
            V::Float(f) => Val::Real(f as f64),
            V::Double(d) => Val::Real(d),
            V::Bytes(b) => Val::Text(String::from_utf8_lossy(&b).into_owned()),
            // Pas de colonne date/heure dans le schéma : rendu texte par sécurité.
            other => Val::Text(other.as_sql(true).trim_matches('\'').to_string()),
        }
    }
}

impl Val {
    pub(crate) fn as_i64(&self) -> Option<i64> {
        match self {
            Val::Int(i) => Some(*i),
            Val::Real(r) => Some(*r as i64),
            Val::Text(s) => s.trim().parse().ok(),
            Val::Null => None,
        }
    }

    pub(crate) fn into_text(self) -> Option<String> {
        match self {
            Val::Null => None,
            Val::Int(i) => Some(i.to_string()),
            Val::Real(r) => Some(r.to_string()),
            Val::Text(s) => Some(s),
        }
    }
}

pub(crate) fn sqlite_err(e: rusqlite::Error) -> Error {
    Error::Database(format!("SQLite : {e}"))
}

pub(crate) fn mysql_err(e: mysql::Error) -> Error {
    Error::Database(format!("MySQL : {e}"))
}

/// Connexion ouverte.
pub(crate) enum Conn {
    Sqlite(rusqlite::Connection),
    MySql(mysql::Conn),
}

fn mysql_params(params: &[Val]) -> mysql::Params {
    if params.is_empty() {
        mysql::Params::Empty
    } else {
        mysql::Params::Positional(params.iter().map(mysql::Value::from).collect())
    }
}

impl Conn {
    pub(crate) fn open_sqlite(path: &std::path::Path) -> Result<Self, Error> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::Database(format!("dossier {} impossible à créer : {e}", dir.display())))?;
        }
        let c = rusqlite::Connection::open(path)
            .map_err(|e| Error::Database(format!("base SQLite {} impossible à ouvrir : {e}", path.display())))?;
        // Plusieurs connexions (surveillance, deuxième ComicRust) : attendre le verrou
        // plutôt que d'échouer aussitôt.
        c.busy_timeout(Duration::from_secs(15)).map_err(sqlite_err)?;
        c.set_prepared_statement_cache_capacity(32);
        Ok(Conn::Sqlite(c))
    }

    pub(crate) fn open_mysql(cfg: &crate::db::DbConfig) -> Result<Self, Error> {
        let mut c = crate::db::connect(cfg)?;
        c.query_drop("SET NAMES utf8mb4").map_err(mysql_err)?;
        Ok(Conn::MySql(c))
    }

    pub(crate) fn engine(&self) -> Engine {
        match self {
            Conn::Sqlite(_) => Engine::Sqlite,
            Conn::MySql(_) => Engine::MySql,
        }
    }

    /// Instruction sans paramètre (création de table…).
    pub(crate) fn batch(&mut self, sql: &str) -> Result<(), Error> {
        match self {
            Conn::Sqlite(c) => c.execute_batch(sql).map_err(sqlite_err),
            Conn::MySql(c) => c.query_drop(sql).map_err(mysql_err),
        }
    }

    /// Instruction paramétrée ; renvoie le nombre de lignes touchées.
    pub(crate) fn exec(&mut self, sql: &str, params: &[Val]) -> Result<u64, Error> {
        match self {
            Conn::Sqlite(c) => {
                let mut st = c.prepare_cached(sql).map_err(sqlite_err)?;
                Ok(st.execute(rusqlite::params_from_iter(params.iter())).map_err(sqlite_err)? as u64)
            }
            Conn::MySql(c) => {
                c.exec_drop(sql, mysql_params(params)).map_err(mysql_err)?;
                Ok(c.affected_rows())
            }
        }
    }

    /// Requête lue ligne par ligne (sans tout garder en mémoire).
    pub(crate) fn query_each(
        &mut self,
        sql: &str,
        params: &[Val],
        mut f: impl FnMut(Vec<Val>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        match self {
            Conn::Sqlite(c) => {
                let mut st = c.prepare_cached(sql).map_err(sqlite_err)?;
                let n = st.column_count();
                let mut rows = st.query(rusqlite::params_from_iter(params.iter())).map_err(sqlite_err)?;
                while let Some(r) = rows.next().map_err(sqlite_err)? {
                    let mut vals = Vec::with_capacity(n);
                    for i in 0..n {
                        vals.push(Val::from(r.get_ref(i).map_err(sqlite_err)?));
                    }
                    f(vals)?;
                }
                Ok(())
            }
            Conn::MySql(c) => {
                // `exec_iter` : protocole binaire, les nombres restent typés.
                let res = c.exec_iter(sql, mysql_params(params)).map_err(mysql_err)?;
                for row in res {
                    let row = row.map_err(mysql_err)?;
                    f(row.unwrap().into_iter().map(Val::from).collect())?;
                }
                Ok(())
            }
        }
    }

    pub(crate) fn query(&mut self, sql: &str, params: &[Val]) -> Result<Vec<Vec<Val>>, Error> {
        let mut out = Vec::new();
        self.query_each(sql, params, |r| {
            out.push(r);
            Ok(())
        })?;
        Ok(out)
    }

    /// Exécute `f` dans une transaction d'écriture (annulée en cas d'erreur).
    pub(crate) fn write_tx<T>(&mut self, f: impl FnOnce(&mut Conn) -> Result<T, Error>) -> Result<T, Error> {
        // SQLite : IMMEDIATE prend le verrou d'écriture dès le début (pas d'échec
        // « base occupée » au moment de passer de la lecture à l'écriture).
        self.batch(match self.engine() {
            Engine::Sqlite => "BEGIN IMMEDIATE",
            Engine::MySql => "START TRANSACTION",
        })?;
        match f(self) {
            Ok(v) => {
                self.batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Exécute `f` dans une transaction de lecture (lecture cohérente de plusieurs requêtes).
    pub(crate) fn read_tx<T>(&mut self, f: impl FnOnce(&mut Conn) -> Result<T, Error>) -> Result<T, Error> {
        self.batch(match self.engine() {
            Engine::Sqlite => "BEGIN",
            Engine::MySql => "START TRANSACTION WITH CONSISTENT SNAPSHOT",
        })?;
        let res = f(self);
        let end = self.batch(if res.is_ok() { "COMMIT" } else { "ROLLBACK" });
        let v = res?;
        end?;
        Ok(v)
    }
}
