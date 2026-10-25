//! Tests de l'écriture dans ComicRack côté interface : fiches construites depuis l'état
//! du lecteur, d'une conversion ou d'une écriture d'infos, ménage des réglages locaux.

use super::*;
use crate::settings::ReadProgress;
use comicrust_core::db::patch_comic_book_xml;

/// Fiche au format de ComicRack CE (comme dans les tests du cœur).
const CE: &str = r#"<ComicBook Id="5f0c1a2b-0000-4000-8000-000000000001" File="Z:\BD\Batman 01.cbr"><Series>Batman</Series><Number>1</Number><PageCount>20</PageCount><Opened>2024-03-01T20:15:42.1234567+01:00</Opened><OpenCount>3</OpenCount><CurrentPage>1</CurrentPage><LastPageRead>12</LastPageRead><NewPages>2</NewPages><ComicInfoIsDirty>true</ComicInfoIsDirty><FileSize>1234</FileSize><FileCreationTime>2019-12-31T22:00:00Z</FileCreationTime></ComicBook>"#;
const ID: &str = "5f0c1a2b-0000-4000-8000-000000000001";

fn apply(xml: &str, e: &BookEdit) -> LibraryBook {
    LibraryBook::from_xml(&patch_comic_book_xml(xml, &e.changes).unwrap()).unwrap()
}

fn at(s: &str) -> chrono::DateTime<chrono::FixedOffset> {
    chrono::DateTime::parse_from_rfc3339(s).unwrap()
}

#[test]
fn reading_like_comicrack() {
    let book = LibraryBook::from_xml(CE).unwrap();
    let r = Reading { current: 5, shown_max: 6, pages: 20, opened_at: at("2026-10-24T21:05:00+02:00") };
    let e = reading_edit(&book, &r);
    let out = patch_comic_book_xml(CE, &e.changes).unwrap();
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!(b.current_page, Some(5));
    assert_eq!(b.last_page_read, Some(12), "LastPageRead n'est jamais reculé");
    assert_eq!(b.open_count, Some(4), "une ouverture de plus");
    assert_eq!(b.opened.as_deref(), Some("2026-10-24T21:05:00+02:00"), "heure d'ouverture");
    assert!(!out.contains("NewPages"), "NewPages remis à 0 (valeur par défaut : élément retiré)");
    assert!(out.contains("<PageCount>20</PageCount>"), "PageCount connu : inchangé");
    assert!(out.contains("<ComicInfoIsDirty>true</ComicInfoIsDirty>"), "le reste de la fiche est gardé");

    // Plus loin que ComicRack, en double page : la seconde page compte ; PageCount manquant écrit.
    let bare = r#"<ComicBook Id="x" File="/bd/a.cbz"><Series>A</Series></ComicBook>"#;
    let book = LibraryBook::from_xml(bare).unwrap();
    let r = Reading { current: 14, shown_max: 15, pages: 32, opened_at: at("2026-10-24T21:05:00+02:00") };
    let b = apply(bare, &reading_edit(&book, &r));
    assert_eq!((b.current_page, b.last_page_read, b.open_count, b.page_count), (Some(14), Some(15), Some(1), Some(32)));
}

#[test]
fn converted_file() {
    let dir = tempfile::tempdir().unwrap();
    let cbz = dir.path().join("Batman 01.cbz");
    std::fs::write(&cbz, vec![0u8; 4321]).unwrap();
    let done = Done::Converted { cbz: cbz.clone(), pages: 22, moved: None };
    let e = file_edit(ID, &done, &dir.path().join("Batman 01.cbr")).unwrap();
    let out = patch_comic_book_xml(CE, &e.changes).unwrap();
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!(b.file.as_deref(), Some(cbz.to_string_lossy().as_ref()));
    assert_eq!((b.file_size, b.page_count), (Some(4321), Some(22)));
    let modified = b.file_modified_time.unwrap();
    assert!(modified.ends_with('Z'), "date du fichier en UTC comme ComicRack : {modified}");
    assert_eq!(b.current_page, Some(1), "lecture gardée");
    assert!(file_edit(ID, &done, &cbz).is_ok());
    std::fs::remove_file(&cbz).unwrap();
    assert!(file_edit(ID, &done, &cbz).is_err(), "CBZ disparu : rien d'écrit");
}

#[test]
fn info_written_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("Batman 01.cbz");
    std::fs::write(&f, vec![0u8; 999]).unwrap();
    // Déjà à jour : seul l'indicateur « à mettre à jour » change.
    let e = file_edit(ID, &Done::Info { written: false, counter: 1 }, &f).unwrap();
    assert_eq!(e.changes.len(), 1);
    let b = apply(CE, &e);
    assert_eq!((b.comic_info_is_dirty, b.file_size), (None, Some(1234)), "ComicInfoIsDirty=false (défaut)");
    // Fichier réécrit : taille et date relues, comme RefreshFileProperties.
    let e = file_edit(ID, &Done::Info { written: true, counter: 1 }, &f).unwrap();
    let out = patch_comic_book_xml(CE, &e.changes).unwrap();
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!((b.comic_info_is_dirty, b.file_size), (None, Some(999)));
    assert!(b.file_modified_time.is_some());
    assert!(out.contains("<FileCreationTime>"), "date de création jamais supprimée");
}

fn db() -> ComicDatabase {
    let book = |id: &str, file: &str, current: u32, last: u32| LibraryBook {
        id: id.into(),
        series: Some(id.to_uppercase()),
        file: Some(file.into()),
        current_page: Some(current),
        last_page_read: Some(last),
        page_count: Some(20),
        ..Default::default()
    };
    ComicDatabase::from_books(vec![
        book("a", "Z:\\BD\\A.cbz", 4, 4),
        book("b", "Z:\\BD\\B.cbz", 2, 9),
        book("c", "Z:\\BD\\C.cbz", 0, 10),
    ])
}

#[test]
fn local_progress_sorted_out() {
    let mut s = Settings::default();
    let p = |x: &str| PathBuf::from(x);
    let progress = |max_page| ReadProgress { max_page, pages: 20 };
    // a : ComicRack a déjà cette lecture → oubliée.
    s.last_pages.insert(p("Z:\\BD\\A.cbz"), 4);
    s.read_progress.insert(p("Z:\\BD\\A.cbz"), progress(4));
    // b : plus avancée dans ComicRust → à reporter (chemin en casse différente).
    s.last_pages.insert(p("z:\\bd\\b.cbz"), 15);
    s.read_progress.insert(p("z:\\bd\\b.cbz"), progress(15));
    // c : relu depuis le début dans ComicRust, moins loin que ComicRack → gardé, pas proposé.
    s.last_pages.insert(p("Z:\\BD\\C.cbz"), 3);
    s.read_progress.insert(p("Z:\\BD\\C.cbz"), progress(3));
    // Hors bibliothèque : intouché.
    s.last_pages.insert(p("/tmp/x.cbz"), 7);
    let pending = sort_local_progress(&mut s, &db());
    assert_eq!(
        pending,
        [Pending {
            id: "b".into(),
            caption: db().get("b").unwrap().caption(),
            path: p("z:\\bd\\b.cbz"),
            current: 15,
            max_page: 15,
            pages: 20
        }]
    );
    assert!(!s.last_pages.contains_key(&p("Z:\\BD\\A.cbz")) && !s.read_progress.contains_key(&p("Z:\\BD\\A.cbz")));
    assert!(s.last_pages.contains_key(&p("Z:\\BD\\C.cbz")) && s.last_pages.contains_key(&p("/tmp/x.cbz")));

    // Report : rien n'est reculé, puis la lecture locale est oubliée.
    let db = db();
    let xml = r#"<ComicBook Id="b" File="Z:\BD\B.cbz"><PageCount>20</PageCount><OpenCount>2</OpenCount><CurrentPage>2</CurrentPage><LastPageRead>9</LastPageRead></ComicBook>"#;
    let b = apply(xml, &pending[0].edit(db.get("b").unwrap()));
    assert_eq!((b.current_page, b.last_page_read, b.open_count), (Some(15), Some(15), Some(2)));
    assert!(forget_if_synced(&mut s, &pending[0].path, &b));
    assert!(!s.read_progress.contains_key(&pending[0].path));
    // Lecture continuée entre-temps (page plus loin) : gardée.
    s.read_progress.insert(p("Z:\\BD\\C.cbz"), progress(18));
    assert!(!forget_if_synced(&mut s, &p("Z:\\BD\\C.cbz"), db.get("c").unwrap()));
}

#[test]
fn problems_message() {
    let mut r = WriteReport::default();
    let captions = HashMap::from([("a".to_string(), "Batman #1".to_string())]);
    assert!(problems(&r, &captions, "Lecture", "gardée").is_none());
    r.conflicts.push("a".into());
    r.missing.push("b".into());
    r.failed.push(("c".into(), "erreur SQL".into()));
    let m = problems(&r, &captions, "Lecture", "Elle reste gardée dans ComicRust.").unwrap();
    assert!(m.contains("Batman #1 : fiche modifiée en même temps"), "{m}");
    assert!(m.contains("b : livre absent de la base"), "{m}");
    assert!(m.contains("c : erreur SQL") && m.ends_with("Elle reste gardée dans ComicRust."), "{m}");
}

#[test]
fn unreachable_database_reported_and_close_not_blocked() {
    let mut w = ComicRackWrites::default();
    let cfg =
        DbConfig { host: "127.0.0.1".into(), port: 1, user: "x".into(), database: "x".into(), ..Default::default() };
    w.submit(Job {
        cfg,
        edits: vec![BookEdit::new("a").set("CurrentPage", Value::Int(1))],
        kind: WriteKind::Reading { path: "/bd/a.cbz".into() },
        captions: HashMap::new(),
        sink: None,
    });
    w.waiter().join().unwrap();
    assert_eq!(*w.busy.0.lock().unwrap(), 0);
    let done = w.poll();
    assert_eq!(done.len(), 1);
    assert!(done[0].result.is_err());
}
