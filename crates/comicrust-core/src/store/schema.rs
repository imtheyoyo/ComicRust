//! Schéma de la base ComicRust : **une colonne par champ** de [`LibraryBook`].
//!
//! La liste des colonnes n'est écrite qu'une fois, dans la macro `book_columns!` :
//! création des tables, ajout des colonnes manquantes, écriture et lecture en dérivent.
//! Ajouter un champ à `LibraryBook` sans l'ajouter ici fait échouer la compilation
//! (déstructuration exhaustive dans `all_fields_listed`).

use super::conn::{Conn, Engine, Val};
use crate::library::{LibraryBook, PageInfo, PageList};
use crate::Error;

/// Version du schéma écrite dans `comicrust_meta.schema_version`.
///
/// - 1 : v0.7.0 ;
/// - 2 : MySQL, `comicrust_folders` indexée par `path_hash` (SHA-256 du chemin) au lieu
///   d'une clé `VARCHAR(700)` refusée par les anciens serveurs.
pub const SCHEMA_VERSION: i64 = 2;

pub(crate) const BOOKS: &str = "comicrust_books";
pub(crate) const FOLDERS: &str = "comicrust_folders";
pub(crate) const META: &str = "comicrust_meta";

/// Nature d'une colonne (le type SQL en dépend selon le moteur).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Texte court ou moyen.
    Text,
    /// Texte pouvant être long (résumé, notes…) : MEDIUMTEXT en MySQL.
    Long,
    Int,
    Real,
    Bool,
}

impl Kind {
    pub(crate) fn sql_type(self, engine: Engine) -> &'static str {
        match (engine, self) {
            (Engine::Sqlite, Kind::Text | Kind::Long) => "TEXT",
            (Engine::Sqlite, Kind::Int | Kind::Bool) => "INTEGER",
            (Engine::Sqlite, Kind::Real) => "REAL",
            (Engine::MySql, Kind::Text) => "TEXT",
            (Engine::MySql, Kind::Long) => "MEDIUMTEXT",
            (Engine::MySql, Kind::Int) => "BIGINT",
            (Engine::MySql, Kind::Real) => "DOUBLE",
            (Engine::MySql, Kind::Bool) => "TINYINT",
        }
    }
}

/// Conversion d'un champ `Option<…>` de `LibraryBook` vers / depuis une cellule.
/// La lecture est tolérante : une valeur inattendue (base modifiée à la main) donne `None`.
trait Cell: Sized {
    fn to_val(&self) -> Val;
    fn from_val(v: Val) -> Self;
}

impl Cell for Option<String> {
    fn to_val(&self) -> Val {
        self.as_ref().map_or(Val::Null, |s| Val::Text(s.clone()))
    }
    fn from_val(v: Val) -> Self {
        v.into_text()
    }
}

macro_rules! int_cell {
    ($($t:ty),*) => {$(
        impl Cell for Option<$t> {
            fn to_val(&self) -> Val {
                // u64 au-delà de i64::MAX : impossible pour une taille de fichier réelle.
                self.map_or(Val::Null, |x| Val::Int(i64::try_from(x).unwrap_or(i64::MAX)))
            }
            fn from_val(v: Val) -> Self {
                v.as_i64().and_then(|i| <$t>::try_from(i).ok())
            }
        }
    )*};
}
int_cell!(i32, u32, u64);

impl Cell for Option<f32> {
    fn to_val(&self) -> Val {
        // f32 → f64 est exact, f64 → f32 au retour redonne la même valeur.
        self.map_or(Val::Null, |x| Val::Real(x as f64))
    }
    fn from_val(v: Val) -> Self {
        match v {
            Val::Real(r) => Some(r as f32),
            Val::Int(i) => Some(i as f32),
            Val::Text(s) => s.trim().parse().ok(),
            Val::Null => None,
        }
    }
}

impl Cell for Option<bool> {
    fn to_val(&self) -> Val {
        self.map_or(Val::Null, |b| Val::Int(b as i64))
    }
    fn from_val(v: Val) -> Self {
        match v {
            Val::Text(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                t => t.parse::<i64>().ok().map(|i| i != 0),
            },
            v => v.as_i64().map(|i| i != 0),
        }
    }
}

/// Liste unique des colonnes « champ » : `nom_du_champ: Nature`.
/// (`id`, `pages` et `update_counter` sont gérés à part.)
macro_rules! book_columns {
    ($m:ident) => {
        $m! {
            file: Text,
            series: Text,
            number: Text,
            volume: Int,
            year: Int,
            title: Text,
            publisher: Text,
            page_count: Int,
            current_page: Int,
            last_page_read: Int,
            rating: Real,
            count: Int,
            writer: Text,
            penciller: Text,
            genre: Text,
            story_arc: Text,
            alternate_series: Text,
            added: Text,
            opened: Text,
            released: Text,
            open_count: Int,
            file_size: Int,
            file_modified_time: Text,
            file_creation_time: Text,
            alternate_number: Text,
            alternate_count: Int,
            series_group: Text,
            summary: Long,
            notes: Long,
            review: Long,
            month: Int,
            day: Int,
            inker: Text,
            colorist: Text,
            letterer: Text,
            cover_artist: Text,
            editor: Text,
            translator: Text,
            imprint: Text,
            web: Text,
            language_iso: Text,
            format: Text,
            age_rating: Text,
            black_and_white: Text,
            manga: Text,
            characters: Text,
            teams: Text,
            main_character_or_team: Text,
            locations: Text,
            tags: Text,
            community_rating: Real,
            book_age: Text,
            book_collection_status: Text,
            book_condition: Text,
            book_location: Text,
            book_notes: Long,
            book_owner: Text,
            book_store: Text,
            isbn: Text,
            book_price: Real,
            checked: Bool,
            comic_info_is_dirty: Bool,
            comic_book_is_dirty: Bool,
            enable_proposed: Bool,
            missing: Bool,
            series_complete: Text,
            scan_information: Long,
            preferred_front_cover: Int,
        }
    };
}

macro_rules! gen_columns {
    ($($f:ident: $k:ident,)*) => {
        /// Colonnes « champ », dans l'ordre de la macro.
        pub(crate) const FIELD_COLUMNS: &[(&str, Kind)] = &[$((stringify!($f), Kind::$k)),*];

        fn fields_to_vals(b: &LibraryBook, out: &mut Vec<Val>) {
            $(out.push(Cell::to_val(&b.$f));)*
        }

        fn fields_from_vals(b: &mut LibraryBook, vals: &mut impl Iterator<Item = Val>) {
            $(b.$f = Cell::from_val(vals.next().unwrap_or(Val::Null));)*
        }

        /// Ne sert qu'à la compilation : échoue si `LibraryBook` gagne un champ absent
        /// de `book_columns!`.
        #[allow(dead_code)]
        fn all_fields_listed(b: LibraryBook) {
            let LibraryBook { id: _, pages: _, proposed_cache: _, $($f: _,)* } = b;
        }
    };
}
book_columns!(gen_columns);

/// Toutes les colonnes de `comicrust_books`, dans l'ordre utilisé par
/// [`book_to_row`] / [`book_from_row`].
pub(crate) fn all_columns() -> Vec<&'static str> {
    let mut v = vec!["id", "update_counter", "pages"];
    v.extend(FIELD_COLUMNS.iter().map(|(n, _)| *n));
    v
}

/// Liste `` `a`, `b`, … `` pour une requête.
pub(crate) fn column_list() -> String {
    all_columns().iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
}

/// Pages en JSON : `[{"image":0,"type":"FrontCover"}, …]` ; aucune page → NULL.
fn pages_to_val(p: &PageList) -> Val {
    if p.page.is_empty() {
        return Val::Null;
    }
    let list: Vec<serde_json::Value> =
        p.page.iter().map(|pg| serde_json::json!({ "image": pg.image, "type": pg.kind })).collect();
    Val::Text(serde_json::Value::Array(list).to_string())
}

fn pages_from_val(v: Val) -> PageList {
    let Some(text) = v.into_text() else { return PageList::default() };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(&text) else {
        return PageList::default();
    };
    let page = items
        .iter()
        .map(|it| PageInfo {
            image: it.get("image").and_then(|i| i.as_i64()).and_then(|i| i32::try_from(i).ok()).unwrap_or(0),
            kind: it.get("type").and_then(|t| t.as_str()).map(str::to_string),
        })
        .collect();
    PageList { page }
}

/// Valeurs d'une ligne, dans l'ordre de [`all_columns`].
pub(crate) fn book_to_row(b: &LibraryBook, counter: i64, out: &mut Vec<Val>) {
    out.push(Val::Text(b.id.clone()));
    out.push(Val::Int(counter));
    out.push(pages_to_val(&b.pages));
    fields_to_vals(b, out);
}

/// Valeurs des colonnes « champ » de `b`, avec leur nom (ordre de `FIELD_COLUMNS`).
pub(crate) fn field_values(b: &LibraryBook) -> Vec<(&'static str, Val)> {
    let mut vals = Vec::with_capacity(FIELD_COLUMNS.len());
    fields_to_vals(b, &mut vals);
    FIELD_COLUMNS.iter().map(|(n, _)| *n).zip(vals).collect()
}

/// Valeur de la colonne `pages` de `b`.
pub(crate) fn pages_value(b: &LibraryBook) -> Val {
    pages_to_val(&b.pages)
}

/// Livre lu depuis une ligne (ordre de [`all_columns`]) ; renvoie aussi son `update_counter`.
pub(crate) fn book_from_row(row: Vec<Val>) -> (LibraryBook, i64) {
    let mut it = row.into_iter();
    let mut b = LibraryBook { id: it.next().and_then(Val::into_text).unwrap_or_default(), ..Default::default() };
    let counter = it.next().and_then(|v| v.as_i64()).unwrap_or(0);
    b.pages = pages_from_val(it.next().unwrap_or(Val::Null));
    fields_from_vals(&mut b, &mut it);
    (b, counter)
}

/// Longueur de l'index préfixe sur `file` en MySQL : 191 × 4 octets (utf8mb4) = 764,
/// sous la limite de 767 octets des anciens MySQL / MariaDB.
pub(crate) const MYSQL_FILE_INDEX_CHARS: usize = 191;

/// Colonnes de `comicrust_folders` en MySQL (schéma 2).
const MYSQL_FOLDERS_COLUMNS: &str = "`path_hash` CHAR(64) CHARACTER SET ascii NOT NULL PRIMARY KEY, \
     `path` TEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL";

/// Instructions de création des tables (si absentes).
pub(crate) fn create_statements(engine: Engine) -> Vec<String> {
    let fields: String = FIELD_COLUMNS.iter().map(|(n, k)| format!(",\n  `{n}` {}", k.sql_type(engine))).collect();
    match engine {
        Engine::Sqlite => vec![
            format!(
                "CREATE TABLE IF NOT EXISTS {BOOKS} (\n  `id` TEXT NOT NULL PRIMARY KEY COLLATE NOCASE,\n  \
                 `update_counter` INTEGER NOT NULL DEFAULT 0,\n  `pages` TEXT{fields}\n)"
            ),
            format!("CREATE INDEX IF NOT EXISTS {BOOKS}_file ON {BOOKS}(`file`)"),
            format!("CREATE INDEX IF NOT EXISTS {BOOKS}_counter ON {BOOKS}(`update_counter`)"),
            format!("CREATE TABLE IF NOT EXISTS {FOLDERS} (`path` TEXT NOT NULL PRIMARY KEY)"),
            format!("CREATE TABLE IF NOT EXISTS {META} (`key` TEXT NOT NULL PRIMARY KEY, `value` TEXT)"),
        ],
        // MySQL : pas d'index sur un TEXT sans longueur → index préfixe (non unique)
        // sur `file`, qui peut dépasser 255 caractères.
        //
        // Compatibilité avec les anciens serveurs (MySQL 5.5 / 5.6, MariaDB < 10.2 : clé
        // d'index InnoDB limitée à 767 octets, soit 191 caractères utf8mb4 de 4 octets) :
        // - index préfixe `file`(191) (764 octets) ;
        // - `comicrust_folders` : un chemin peut dépasser 191 caractères, la clé primaire
        //   est donc `path_hash` = SHA-256 du chemin en hexadécimal (64 octets ASCII),
        //   calculé par le serveur (`SHA2(?, 256)`) ; `path` est un `TEXT` en
        //   `utf8mb4_bin` (deux chemins qui ne diffèrent que par un accent restent distincts).
        Engine::MySql => vec![
            format!(
                "CREATE TABLE IF NOT EXISTS {BOOKS} (\n  `id` VARCHAR(64) NOT NULL PRIMARY KEY,\n  \
                 `update_counter` BIGINT NOT NULL DEFAULT 0,\n  `pages` MEDIUMTEXT{fields},\n  \
                 INDEX {BOOKS}_file (`file`({MYSQL_FILE_INDEX_CHARS})),\n  \
                 INDEX {BOOKS}_counter (`update_counter`)\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"
            ),
            format!(
                "CREATE TABLE IF NOT EXISTS {FOLDERS} ({MYSQL_FOLDERS_COLUMNS}) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"
            ),
            format!(
                "CREATE TABLE IF NOT EXISTS {META} (`key` VARCHAR(64) NOT NULL PRIMARY KEY, `value` TEXT) \
                 ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"
            ),
        ],
    }
}

fn existing_columns(conn: &mut Conn) -> Result<Vec<String>, Error> {
    let sql = match conn.engine() {
        Engine::Sqlite => format!("SELECT name FROM pragma_table_info('{BOOKS}')"),
        Engine::MySql => format!(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name = '{BOOKS}'"
        ),
    };
    Ok(conn.query(&sql, &[])?.into_iter().filter_map(|r| r.into_iter().next()?.into_text()).collect())
}

/// Crée les tables absentes, ajoute les colonnes manquantes (champ ajouté dans une
/// version plus récente de ComicRust) et applique les migrations.
pub(crate) fn ensure(conn: &mut Conn) -> Result<(), Error> {
    let engine = conn.engine();
    for sql in create_statements(engine) {
        conn.batch(&sql)?;
    }
    // Colonnes ajoutées depuis la création de la base : simple ALTER TABLE.
    let have: Vec<String> = existing_columns(conn)?.iter().map(|c| c.to_ascii_lowercase()).collect();
    for (name, kind) in FIELD_COLUMNS {
        if !have.iter().any(|c| c == name) {
            conn.batch(&format!("ALTER TABLE {BOOKS} ADD COLUMN `{name}` {}", kind.sql_type(engine)))?;
        }
    }
    let ins = engine.insert_ignore();
    for (key, value) in [("schema_version", SCHEMA_VERSION), ("update_counter", 0), ("delete_counter", 0)] {
        conn.exec(
            &format!("{ins} INTO {META} (`key`, `value`) VALUES (?, ?)"),
            &[Val::Text(key.into()), Val::Text(value.to_string())],
        )?;
    }
    let version = meta_int(conn, "schema_version")?;
    if version > SCHEMA_VERSION {
        return Err(Error::Database(format!(
            "base créée par une version plus récente de ComicRust (schéma {version}, celle-ci gère le {SCHEMA_VERSION})"
        )));
    }
    for v in version..SCHEMA_VERSION {
        migrate(conn, v)?;
        conn.exec(
            &format!("UPDATE {META} SET `value` = ? WHERE `key` = 'schema_version'"),
            &[Val::Text((v + 1).to_string())],
        )?;
    }
    Ok(())
}

/// Passage du schéma `from` au schéma `from + 1`.
fn migrate(conn: &mut Conn, from: i64) -> Result<(), Error> {
    match from {
        // 1 → 2 : MySQL, `comicrust_folders` recréée avec la clé `path_hash` en gardant
        // les chemins. SQLite : rien à faire (`path TEXT PRIMARY KEY` y est sans limite).
        // Attention : en MySQL, CREATE / DROP / RENAME valident la transaction en cours ;
        // chaque étape est donc faite pour pouvoir être rejouée après une interruption.
        1 => {
            if conn.engine() == Engine::Sqlite {
                return Ok(());
            }
            let cols = conn.query(
                "SELECT column_name FROM information_schema.columns \
                 WHERE table_schema = DATABASE() AND table_name = ?",
                &[Val::Text(FOLDERS.into())],
            )?;
            let has_hash = cols.into_iter().filter_map(|r| r.into_iter().next()?.into_text()).any(|c| c == "path_hash");
            if has_hash {
                return Ok(());
            }
            let new = format!("{FOLDERS}_v2");
            conn.batch(&format!("DROP TABLE IF EXISTS {new}"))?;
            conn.batch(&format!("CREATE TABLE {new} ({MYSQL_FOLDERS_COLUMNS}) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"))?;
            conn.batch(&format!(
                "INSERT IGNORE INTO {new} (`path_hash`, `path`) SELECT SHA2(`path`, 256), `path` FROM {FOLDERS}"
            ))?;
            conn.batch(&format!("DROP TABLE {FOLDERS}"))?;
            conn.batch(&format!("RENAME TABLE {new} TO {FOLDERS}"))?;
            Ok(())
        }
        _ => Err(Error::Database(format!("migration du schéma {from} inconnue"))),
    }
}

/// Valeur entière de `comicrust_meta` (0 si absente).
pub(crate) fn meta_int(conn: &mut Conn, key: &str) -> Result<i64, Error> {
    let rows = conn.query(&format!("SELECT `value` FROM {META} WHERE `key` = ?"), &[Val::Text(key.into())])?;
    Ok(rows.into_iter().next().and_then(|r| r.into_iter().next()).and_then(|v| v.as_i64()).unwrap_or(0))
}

/// Incrémente un compteur de `comicrust_meta` et renvoie la nouvelle valeur. À appeler
/// dans une transaction d'écriture : l'UPDATE verrouille la ligne jusqu'à la fin, deux
/// ComicRust ne peuvent donc pas obtenir la même valeur.
pub(crate) fn bump(conn: &mut Conn, key: &str) -> Result<i64, Error> {
    let cast = conn.engine().int_cast("`value`");
    conn.exec(&format!("UPDATE {META} SET `value` = {cast} + 1 WHERE `key` = ?"), &[Val::Text(key.into())])?;
    meta_int(conn, key)
}
