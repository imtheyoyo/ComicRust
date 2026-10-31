//! Tests des livres manquants : marquage (gardé quand la base est relue), filtre de la vue,
//! retrait, propositions, et opérations sur une base ComicRust SQLite.

use std::collections::HashMap;
use std::sync::Arc;

use comicrust_core::db::{Changes, ComicDatabase};
use comicrust_core::library::LibraryBook;
use comicrust_core::missing::{Candidate, MatchKind, SearchReport};

use super::*;
use crate::library::{GroupKey, SortKey, Status, ViewOptions};

fn book(id: &str, file: &str) -> LibraryBook {
    LibraryBook { id: id.into(), series: Some(format!("Série {id}")), file: Some(file.into()), ..Default::default() }
}

fn state(books: Vec<LibraryBook>) -> LibraryState {
    LibraryState { status: Status::Loaded(Arc::new(ComicDatabase::from_books(books))), ..Default::default() }
}

fn opts() -> ViewOptions {
    ViewOptions { sort: SortKey::Series, descending: false, group: GroupKey::None }
}

fn shown(s: &mut LibraryState) -> Vec<String> {
    let db = s.database().unwrap();
    s.view(&opts()).iter().map(|&i| db.books[i].id.clone()).collect()
}

#[test]
fn marks_survive_a_reload_and_feed_the_filter() {
    let mut cr = book("c", "/bd/c.cbz");
    cr.missing = Some(true); // <Missing> de ComicRack
    let mut s = state(vec![book("a", "/bd/a.cbz"), book("b", "/bd/b.cbz"), cr, book("sans", "")]);
    assert_eq!(s.missing_count(), 1);
    // Vérification : « a » manquant, « c » de nouveau présent.
    let files: HashMap<String, String> = [("a", "/bd/a.cbz"), ("b", "/bd/b.cbz"), ("c", "/bd/c.cbz")]
        .iter()
        .map(|(i, f)| (i.to_string(), f.to_string()))
        .collect();
    let report = comicrust_core::missing::CheckReport {
        missing: vec!["a".into()],
        present: vec!["b".into(), "c".into()],
        ..Default::default()
    };
    s.apply_check(&files, &report);
    assert_eq!(s.missing_count(), 1);
    let db = s.database().unwrap();
    assert_eq!((db.get("a").unwrap().missing, db.get("c").unwrap().missing), (Some(true), Some(false)));
    assert_eq!(db.get("b").unwrap().missing, None, "présent et jamais manquant : inchangé");

    // Filtre « Livres manquants ».
    s.missing_only = true;
    assert_eq!(shown(&mut s), ["a"]);
    s.missing_only = false;
    assert_eq!(shown(&mut s).len(), 4);

    // ComicRack réécrit la fiche de « a » (même fichier) : le marquage reste…
    s.apply_changes(Changes { updated: vec![book("a", "/bd/a.cbz")], ..Default::default() });
    assert_eq!(s.database().unwrap().get("a").unwrap().missing, Some(true));
    // … mais pas après une relocalisation (autre fichier).
    s.apply_changes(Changes { updated: vec![book("a", "/bd/rangé/a.cbz")], ..Default::default() });
    assert_eq!(s.database().unwrap().get("a").unwrap().missing, None);
    assert_eq!(s.missing_count(), 0);

    // Ouverture ratée (« Fichier introuvable ») : marqué.
    s.set_missing(&["B".into()], true);
    s.missing_only = true;
    assert_eq!(shown(&mut s), ["b"]);
    s.replace(ComicDatabase::from_books(vec![book("b", "/bd/b.cbz")]));
    assert_eq!(s.missing_count(), 1, "relecture complète (ComicDb.xml) : marquage gardé");
}

#[test]
fn removing_books_forgets_them_everywhere() {
    let mut s = state(vec![book("a", "/bd/a.cbz"), book("b", "/bd/b.cbz"), book("c", "/bd/c.cbz")]);
    assert_eq!(shown(&mut s).len(), 3);
    s.click_book("a", false, false);
    s.click_book("b", true, false);
    s.set_missing(&["a".into()], true);
    assert_eq!(s.remove_books(&["A".into(), "zzz".into()]), 1);
    assert_eq!(shown(&mut s), ["b", "c"]);
    assert_eq!(s.selection(), ["b"]);
    assert!(s.missing.overlay.is_empty());
    assert_eq!(s.missing_count(), 0);
}

#[test]
fn proposals_from_search_and_chosen_file() {
    let mut a = book("a", r"\\NAS\BD\29 - Marvel Team-Up #55.cbz");
    a.file_size = Some(100);
    let s = state(vec![a, book("b", "/bd/b.cbz"), book("c", "/rangé/x.cbz")]);
    let w = s.wanted(&["a".into(), "absent".into()]);
    assert_eq!((w.len(), w[0].size), (1, Some(100)));
    let report = SearchReport {
        found: vec![
            Candidate {
                id: "a".into(),
                path: "/rangé/29 - Marvel Team-Up #55.cbz".into(),
                size: 100,
                kind: MatchKind::NameAndSize,
                others: 0,
            },
            Candidate { id: "b".into(), path: "/rangé/b2.cbz".into(), size: 1, kind: MatchKind::Size, others: 0 },
        ],
        ..Default::default()
    };
    let p = s.proposals(&report);
    assert_eq!(p.len(), 2);
    assert!(p[0].checked && !p[1].checked, "par taille seule : à cocher soi-même");
    // Fichier choisi déjà dans la bibliothèque (doublon) : décoché, signalé.
    let p = s.chosen_proposal("b", "/rangé/x.cbz".into(), 3).unwrap();
    assert!(!p.checked && p.duplicate.is_some());
    let p = s.chosen_proposal("a", "/ailleurs/29 - marvel team-up #55.CBZ".into(), 100).unwrap();
    assert_eq!((p.kind, p.checked), (MatchKind::NameAndSize, true));
    assert!(s.files_index().contains_key("/rangé/x.cbz"));
}

/// Base ComicRust (SQLite) : marquage enregistré, relocalisation (lecture gardée) et retrait
/// (fiche seulement : le fichier reste sur le disque).
#[test]
fn standalone_operations() {
    use comicrust_core::store::{Store, StoreConfig};
    let dir = tempfile::tempdir().unwrap();
    let cfg = StoreConfig::Sqlite(dir.path().join("comicrust.db"));
    let new_file = dir.path().join("Rangés/a.cbz");
    let kept = dir.path().join("b.cbz");
    for f in [&new_file, &kept] {
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, b"BD").unwrap();
    }
    let mut a = book("a", &dir.path().join("a.cbz").to_string_lossy());
    a.current_page = Some(3);
    a.rating = Some(5.0);
    let b = book("b", &kept.to_string_lossy());
    Store::open(&cfg).unwrap().upsert(&[a.clone(), b]).unwrap();
    let ctx = eframe::egui::Context::default();
    let wait = |rx: std::sync::mpsc::Receiver<OpDone>| rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap();
    let sink: crate::library::watch::WrittenSink = Default::default();

    let done = wait(persist_marks(cfg.clone(), vec!["a".into()], vec![], Some(sink.clone()), &ctx));
    assert_eq!(done.kind, OpKind::Mark);
    let Ok(OpResult::Store { books, counter }) = done.result else { panic!() };
    assert_eq!((books.len(), books[0].missing), (1, Some(true)));
    assert_eq!(sink.lock().unwrap().as_slice(), [("a".to_string(), counter)], "signalé à la surveillance");

    let done = wait(relocate(Target::Store(cfg.clone()), vec![(a, new_file.clone())], None, &ctx));
    let Ok(OpResult::Store { books, .. }) = done.result else { panic!("{:?}", done.result) };
    assert_eq!(books[0].file.as_deref(), Some(new_file.to_string_lossy().as_ref()));
    assert_eq!((books[0].missing, books[0].current_page, books[0].rating), (None, Some(3), Some(5.0)));

    let deleted: crate::library::watch::DeletedSink = Default::default();
    let done = wait(remove(Target::Store(cfg.clone()), vec!["b".into(), "absent".into()], Some(deleted.clone()), &ctx));
    let Ok(OpResult::Removed { ids, absent }) = done.result else { panic!() };
    assert_eq!((ids, absent), (vec!["b".to_string()], vec!["absent".to_string()]));
    assert_eq!(deleted.lock().unwrap().len(), 2, "signalé avant la suppression");
    assert!(kept.exists() && new_file.exists(), "aucun fichier supprimé");
    assert_eq!(Store::open_existing(&cfg).unwrap().count().unwrap(), 1);
}

/// Base MySQL de ComicRack en écriture (si `COMICRUST_TEST_MYSQL` est définie) :
/// relocalisation (`File`, taille, `Missing` retiré, lecture gardée) et retrait comme
/// `ComicStorageBaseSql.Delete` (fichier jamais supprimé).
#[test]
fn comicrack_mysql_operations() {
    use mysql::prelude::Queryable;
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL ignoré");
        return;
    };
    let cfg = comicrust_core::db::DbConfig::from_connection_string(&url).unwrap();
    let opts = mysql::OptsBuilder::new()
        .ip_or_hostname(Some(cfg.host.clone()))
        .tcp_port(cfg.port)
        .user(Some(cfg.user.clone()))
        .pass(Some(cfg.password.clone()))
        .db_name(Some(cfg.database.clone()))
        .prefer_socket(false);
    let mut conn = mysql::Conn::new(opts).unwrap();
    conn.query_drop("DROP TABLE IF EXISTS comics").unwrap();
    conn.query_drop("DROP TABLE IF EXISTS changes").unwrap();
    conn.query_drop("Create table changes (id varchar(40) not null primary key, update_counter bigint default 0, delete_counter bigint default 0)").unwrap();
    conn.query_drop(
        "Create table comics (id varchar(40) not null primary key, update_counter bigint default 0, data mediumtext)",
    )
    .unwrap();
    conn.query_drop("Insert into changes values ('00000000-0000-0000-0000-000000000000', 5, 0)").unwrap();
    let a = "aaaaaaaa-0000-4000-8000-000000000001";
    let b = "bbbbbbbb-0000-4000-8000-000000000002";
    let xml = |id: &str, file: &str| {
        format!(
            r#"<ComicBook Id="{id}" File="{file}"><Series>Marvel Team-Up</Series><Number>55</Number><CurrentPage>6</CurrentPage><Rating>4</Rating><FileSize>100</FileSize><Missing>true</Missing></ComicBook>"#
        )
    };
    conn.exec_drop(
        "Insert into comics values (?, 1, ?)",
        (a, xml(a, r"\\SERVEURNAS\BD\Séries_Temp\29 - Marvel Team-Up #55.cbz")),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let kept = dir.path().join("b.cbz");
    let new_file = dir.path().join("Rangés/29 - Marvel Team-Up #55.cbz");
    std::fs::create_dir_all(new_file.parent().unwrap()).unwrap();
    std::fs::write(&new_file, b"nouveau").unwrap();
    std::fs::write(&kept, b"BD").unwrap();
    conn.exec_drop("Insert into comics values (?, 2, ?)", (b, xml(b, &kept.to_string_lossy()))).unwrap();
    let db = comicrust_core::db::ComicDatabase::load(&cfg, None).unwrap();
    let ctx = eframe::egui::Context::default();
    let wait = |rx: std::sync::mpsc::Receiver<OpDone>| rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap();

    let sink: crate::library::watch::WrittenSink = Default::default();
    let book_a = db.get(a).unwrap().clone();
    let done =
        wait(relocate(Target::ComicRack(cfg.clone()), vec![(book_a, new_file.clone())], Some(sink.clone()), &ctx));
    let Ok(OpResult::ComicRack(report)) = done.result else { panic!("{:?}", done.result) };
    assert_eq!(report.written.len(), 1, "{}", report.summary());
    let w = &report.written[0].book;
    assert_eq!(w.file.as_deref(), Some(new_file.to_string_lossy().as_ref()));
    assert_eq!((w.missing, w.file_size, w.current_page, w.rating), (None, Some(7), Some(6), Some(4.0)));
    assert_eq!(sink.lock().unwrap().len(), 1);

    let deleted: crate::library::watch::DeletedSink = Default::default();
    let done = wait(remove(Target::ComicRack(cfg.clone()), vec![b.to_uppercase()], Some(deleted.clone()), &ctx));
    let Ok(OpResult::Removed { ids, absent }) = done.result else { panic!() };
    assert_eq!((ids, absent.len()), (vec![b.to_string()], 0));
    assert_eq!(deleted.lock().unwrap().as_slice(), [b.to_string()]);
    let (u, d): (i64, i64) = conn.query_first("SELECT update_counter, delete_counter FROM changes").unwrap().unwrap();
    assert_eq!((u, d), (6, 1));
    let n: i64 = conn.query_first("SELECT COUNT(*) FROM comics").unwrap().unwrap();
    assert_eq!(n, 1);
    assert!(kept.exists() && new_file.exists(), "aucun fichier supprimé");
}
