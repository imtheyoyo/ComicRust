//! Enregistrement des listes modifiées dans ComicRust (v0.9.0).
//!
//! Lecture : [`Store::list_rows`] rend les lignes de `comicrust_lists` **et** la valeur de
//! `lists_counter` au même instant (transaction de lecture). Les lignes sont modifiées
//! avec [`crate::lists_edit::ListsEditor`], puis [`Store::save_list_rows`] les enregistre
//! en une transaction, **seulement si `lists_counter` n'a pas changé** depuis la lecture
//! (un autre poste connecté à la même base MySQL, ou un transfert depuis ComicRack, a pu
//! modifier les listes entre-temps) : sinon erreur [`Error::Conflict`] et rien n'est écrit.
//!
//! Le contrôle est fait en incrémentant d'abord `lists_counter` dans la transaction
//! (l'`UPDATE` verrouille la ligne jusqu'au `COMMIT` : deux enregistrements simultanés
//! sont faits l'un après l'autre) puis en vérifiant que la nouvelle valeur vaut l'ancienne
//! + 1. Seules les lignes différentes sont écrites (supprimées, modifiées, ajoutées).

use std::collections::HashMap;

use super::conn::{Conn, Val};
use super::import::read_list_rows;
use super::schema::{self, LISTS};
use super::Store;
use crate::lists_edit::ListRow;
use crate::Error;

/// Listes de la base et compteur `lists_counter` lus ensemble.
#[derive(Debug, Clone, PartialEq)]
pub struct ListsSnapshot {
    pub rows: Vec<ListRow>,
    /// À repasser à [`Store::save_list_rows`].
    pub counter: i64,
}

impl Store {
    /// Listes de la base (lignes) et `lists_counter`, lus dans une même transaction.
    pub fn list_rows(&mut self) -> Result<ListsSnapshot, Error> {
        self.conn.read_tx(|conn| {
            let rows = read_list_rows(conn)?;
            let counter = schema::meta_int(conn, "lists_counter")?;
            Ok(ListsSnapshot { rows, counter })
        })
    }

    /// Enregistre `rows` (toutes les listes) si `lists_counter` vaut toujours
    /// `expected_counter` ; renvoie la nouvelle valeur du compteur.
    pub fn save_list_rows(&mut self, rows: &[ListRow], expected_counter: i64) -> Result<i64, Error> {
        // Contrôle final : aucun caractère interdit en XML 1.0 (ces listes peuvent être
        // rendues à ComicRack).
        for r in rows {
            crate::lists_edit::xml::check_xml_document(&r.xml, &format!("Liste « {} »", r.name))?;
        }
        self.conn.write_tx(|conn| {
            let counter = schema::bump(conn, "lists_counter")?;
            if counter != expected_counter + 1 {
                return Err(Error::Conflict(
                    "Les listes ont été modifiées ailleurs (autre poste ou transfert depuis ComicRack) depuis leur \
                     lecture : rien n'a été enregistré. Recharge les listes puis recommence."
                        .into(),
                ));
            }
            write_diff(conn, rows)?;
            Ok(counter)
        })
    }
}

fn write_diff(conn: &mut Conn, rows: &[ListRow]) -> Result<(), Error> {
    let current: HashMap<String, ListRow> =
        read_list_rows(conn)?.into_iter().map(|r| (r.id.to_ascii_lowercase(), r)).collect();
    let wanted: HashMap<String, &ListRow> = rows.iter().map(|r| (r.id.to_ascii_lowercase(), r)).collect();
    if wanted.len() != rows.len() {
        return Err(Error::Database("listes non enregistrées : identifiant de liste en double".into()));
    }
    for (id, r) in &current {
        if !wanted.contains_key(id) {
            conn.exec(&format!("DELETE FROM {LISTS} WHERE `id` = ?"), &[Val::Text(r.id.clone())])?;
        }
    }
    for r in rows {
        let params = |first: Vec<Val>| -> Vec<Val> {
            let mut v = first;
            v.extend([
                r.parent.clone().map_or(Val::Null, Val::Text),
                Val::Int(r.position),
                Val::Text(r.name.clone()),
                Val::Text(r.kind.clone()),
                Val::Text(r.xml.clone()),
            ]);
            v
        };
        match current.get(&r.id.to_ascii_lowercase()) {
            Some(old) if old == r => {}
            Some(old) => {
                let mut p = params(Vec::new());
                p.push(Val::Text(old.id.clone()));
                conn.exec(
                    &format!(
                        "UPDATE {LISTS} SET `parent` = ?, `position` = ?, `name` = ?, `kind` = ?, `xml` = ? WHERE `id` = ?"
                    ),
                    &p,
                )?;
            }
            None => {
                conn.exec(
                    &format!(
                        "INSERT INTO {LISTS} (`id`, `parent`, `position`, `name`, `kind`, `xml`) VALUES (?,?,?,?,?,?)"
                    ),
                    &params(vec![Val::Text(r.id.clone())]),
                )?;
            }
        }
    }
    Ok(())
}
