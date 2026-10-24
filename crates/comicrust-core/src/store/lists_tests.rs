//! Tests de l'enregistrement des listes modifiées (`Store::list_rows`,
//! `Store::save_list_rows`) : SQLite dans un dossier temporaire, MySQL depuis
//! `tests::mysql_store`.

use super::tests::temp_cfg;
use super::*;
use crate::comic_lists::{Combine, ListKind};
use crate::lists_edit::{ListsEditor, NewList};

const XML: &str = r#"<?xml version="1.0"?>
<ComicDatabase xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <ComicLists>
    <Item xsi:type="ComicLibraryListItem" Id="11111111-0000-0000-0000-00000000000a" Name="Bibliothèque" />
    <Item xsi:type="ComicListItemFolder" Id="11111111-0000-0000-0000-0000000000f1" Name="Dossier">
      <Display><View ItemViewMode="Tile" /></Display>
      <Items>
        <Item xsi:type="ComicIdListItem" Id="11111111-0000-0000-0000-0000000000b1" Name="Thanos">
          <BookIds>
            <guid>aaaaaaaa-0000-0000-0000-000000000001</guid>
          </BookIds>
        </Item>
      </Items>
    </Item>
  </ComicLists>
</ComicDatabase>"#;

const F1: &str = "11111111-0000-0000-0000-0000000000f1";
const L1: &str = "11111111-0000-0000-0000-0000000000b1";

/// Scénario commun (SQLite et MySQL) : deux connexions à la même base.
pub(super) fn lists_scenario(cfg: &StoreConfig) {
    let mut a = Store::open(cfg).unwrap();
    let mut b = Store::open(cfg).unwrap();
    let src = ComicRackSource::from_comicdb_xml(XML.to_string()).unwrap();
    a.import_comicrack(&src, &ImportOptions { folders: false, lists: true }, None, None).unwrap();

    let snap = a.list_rows().unwrap();
    assert_eq!(snap.rows.len(), 3);
    assert_eq!(snap.counter, a.lists_counter().unwrap());
    let mut e = ListsEditor::new(snap.rows.clone());
    let new = e.create(Some(F1), Some(0), "Nouvelle", NewList::Reading { books: vec![] }).unwrap();
    e.rename(L1, "Thanos (ordre)").unwrap();
    e.set_combine(F1, Combine::And).unwrap();
    let counter = a.save_list_rows(e.rows(), snap.counter).unwrap();
    assert_eq!(counter, snap.counter + 1);
    assert_eq!(b.lists_counter().unwrap(), counter, "l'autre poste voit le changement");

    // Relu par l'autre connexion : identique à ce qui a été enregistré.
    let lists = b.comic_lists().unwrap();
    assert_eq!(lists, e.lists().unwrap());
    let ListKind::Folder { combine, children } = &lists[1].kind else { panic!("{lists:?}") };
    assert_eq!(*combine, Combine::And);
    assert_eq!(children[0].id, new);
    assert_eq!(children[1].name, "Thanos (ordre)");
    let mut rows = b.list_rows().unwrap().rows;
    let mut want = e.rows().to_vec();
    rows.sort_by(|x, y| x.id.cmp(&y.id));
    want.sort_by(|x, y| x.id.cmp(&y.id));
    assert_eq!(rows, want, "lignes enregistrées octet pour octet");

    // Conflit : `b` lit, `a` enregistre, `b` est refusé et rien n'est écrit.
    let snap_b = b.list_rows().unwrap();
    let mut eb = ListsEditor::new(snap_b.rows.clone());
    eb.delete(F1).unwrap();
    let snap_a = a.list_rows().unwrap();
    let mut ea = ListsEditor::new(snap_a.rows);
    ea.set_description(L1, "par a").unwrap();
    a.save_list_rows(ea.rows(), snap_a.counter).unwrap();
    let err = b.save_list_rows(eb.rows(), snap_b.counter).unwrap_err();
    assert!(matches!(err, Error::Conflict(_)), "{err}");
    assert!(err.to_string().contains("modifiées ailleurs"), "{err}");
    let after = b.list_rows().unwrap();
    assert_eq!(after.counter, snap_a.counter + 1, "compteur inchangé par l'échec");
    assert_eq!(after.rows.len(), 4, "suppression de b non appliquée");
    let l = crate::comic_lists::ComicList::find(&b.comic_lists().unwrap(), L1).unwrap().clone();
    assert_eq!(l.description, "par a");

    // Suppression enregistrée.
    let mut e = ListsEditor::new(after.rows);
    e.delete(F1).unwrap();
    b.save_list_rows(e.rows(), after.counter).unwrap();
    assert_eq!(a.comic_lists().unwrap().len(), 1);
}

#[test]
fn sqlite_lists_saved() {
    let (_dir, cfg) = temp_cfg();
    lists_scenario(&cfg);
}

/// Partie MySQL, appelée par `tests::mysql_store`.
pub(super) fn mysql_lists(db_cfg: &DbConfig) {
    super::tests::drop_tables(db_cfg);
    lists_scenario(&StoreConfig::MySql(db_cfg.clone()));
    super::tests::drop_tables(db_cfg);
}
