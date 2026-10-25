//! Modifications de fiches dans la base ComicRust (voir [`crate::edit`]).

use std::collections::HashMap;

use super::conn::{Conn, Engine, Val};
use super::schema::{self, BOOKS};
use super::{update_columns, Store, MOVE_COLUMNS};
use crate::edit::{self, EditReport};
use crate::library::LibraryBook;
use crate::Error;

/// Fiches de `ids` (identifiants en minuscules → fiche), verrouillées en MySQL jusqu'à
/// la fin de la transaction (`FOR UPDATE` ; SQLite : la transaction `IMMEDIATE` suffit).
fn read_for_update(conn: &mut Conn, ids: &[String]) -> Result<HashMap<String, LibraryBook>, Error> {
    let lock = if conn.engine() == Engine::MySql { " FOR UPDATE" } else { "" };
    let mut out = HashMap::new();
    for chunk in ids.chunks(200) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!("SELECT {} FROM {BOOKS} WHERE `id` IN ({marks}){lock}", schema::column_list());
        let params: Vec<Val> = chunk.iter().map(|i| Val::Text(i.clone())).collect();
        conn.query_each(&sql, &params, |row| {
            let b = schema::book_from_row(row).0;
            out.insert(b.id.to_ascii_lowercase(), b);
            Ok(())
        })?;
    }
    Ok(out)
}

impl Store {
    /// Écrit des modifications de fiches ([`edit::FieldEdit`]) en **une** transaction :
    /// les fiches sont relues (verrouillées), les modifications appliquées, puis seules
    /// les colonnes qui changent sont écrites (`comic_info_is_dirty` à vrai si un champ du
    /// ComicInfo.xml change). Le compteur global n'est incrémenté que si un livre change ;
    /// sa valeur ([`EditReport::store_counter`]) est donnée aux lignes écrites. Les fiches
    /// modifiées sont rendues relues après écriture.
    pub fn apply_edits(&mut self, edits: &[edit::FieldEdit]) -> Result<EditReport, Error> {
        let (groups, errors) = edit::prepare(edits);
        let mut report = EditReport { errors, ..Default::default() };
        if groups.is_empty() {
            return Ok(report);
        }
        let ids: Vec<String> = groups.iter().map(|g| g.id.clone()).collect();
        let (changed, counter) = self.conn.write_tx(|conn| {
            let current = read_for_update(conn, &ids)?;
            let mut writes: Vec<(String, Vec<(&'static str, Val)>)> = Vec::new();
            for g in &groups {
                let Some(before) = current.get(&g.id) else {
                    report.missing.push(g.id.clone());
                    continue;
                };
                let mut after = before.clone();
                for v in &g.edits {
                    edit::apply_valid(&mut after, v);
                }
                let old = schema::field_values(before);
                let diff: Vec<(&'static str, Val)> = schema::field_values(&after)
                    .into_iter()
                    .zip(old)
                    .filter(|(n, o)| n.1 != o.1)
                    .map(|(n, _)| n)
                    .collect();
                if diff.is_empty() {
                    report.unchanged.push(g.id.clone());
                } else {
                    writes.push((before.id.clone(), diff));
                }
            }
            if writes.is_empty() {
                return Ok((Vec::new(), None));
            }
            let counter = schema::bump(conn, "update_counter")?;
            for (id, cols) in &writes {
                let set: Vec<String> = cols.iter().map(|(n, _)| format!("`{n}` = ?")).collect();
                let mut params: Vec<Val> = cols.iter().map(|(_, v)| v.clone()).collect();
                params.push(Val::Int(counter));
                params.push(Val::Text(id.clone()));
                conn.exec(
                    &format!("UPDATE {BOOKS} SET {}, `update_counter` = ? WHERE `id` = ?", set.join(", ")),
                    &params,
                )?;
            }
            let ids: Vec<String> = writes.into_iter().map(|(id, _)| id).collect();
            let fresh = read_for_update(conn, &ids)?;
            let books = ids.iter().filter_map(|i| fresh.get(&i.to_ascii_lowercase()).cloned()).collect();
            Ok((books, Some(counter)))
        })?;
        report.changed = changed;
        report.store_counter = counter;
        Ok(report)
    }

    /// Nouvel emplacement du fichier d'un livre (déplacé, contenu identique) : seules les
    /// colonnes `file`, `file_size`, `file_modified_time`, `file_creation_time` sont prises
    /// dans `moved` ; `missing` à NULL. Rend le compteur et la fiche relue ; erreur si le
    /// livre est absent.
    pub(crate) fn set_file_location(&mut self, moved: &LibraryBook) -> Result<(i64, LibraryBook), Error> {
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "update_counter")?;
            if update_columns(conn, moved, MOVE_COLUMNS, false, counter, "")? == 0 {
                return Err(Error::Database(format!("livre {} absent de la base", moved.id)));
            }
            let mut fresh = read_for_update(conn, std::slice::from_ref(&moved.id))?;
            let book = fresh
                .remove(&moved.id.to_ascii_lowercase())
                .ok_or_else(|| Error::Database(format!("livre {} absent de la base", moved.id)))?;
            Ok((counter, book))
        })
    }
}

#[cfg(test)]
mod tests {
    /// Chaque champ du catalogue a sa colonne dans la base ComicRust.
    #[test]
    fn catalog_columns_exist() {
        for f in crate::edit::FIELDS {
            assert!(super::schema::FIELD_COLUMNS.iter().any(|(c, _)| *c == f.column), "colonne {} absente", f.column);
        }
    }
}
