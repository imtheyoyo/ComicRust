//! Tests du module `store` : SQLite dans un dossier temporaire ; MySQL seulement si
//! `COMICRUST_TEST_MYSQL` est définie (même convention que `db::tests::mysql_server`).

use super::schema::META;
use super::*;
use crate::db::Applied;
use crate::library::{PageInfo, PageList};

/// Livre dont **tous** les champs sont remplis (littéral sans `..Default::default()` :
/// un champ ajouté à `LibraryBook` oblige à compléter ce test).
fn full_book(id: &str) -> LibraryBook {
    LibraryBook {
        id: id.into(),
        file: Some(r"Z:\BD\Thanos Rising 1-5 (Jason Aaron)(2013) — édition « spéciale » 🦸.cbz".into()),
        series: Some("COSMIC COLLECTION".into()),
        number: Some("224".into()),
        volume: Some(3),
        year: Some(2012),
        title: Some("L'ascension".into()),
        publisher: Some("Marvel".into()),
        page_count: Some(115),
        current_page: Some(12),
        last_page_read: Some(13),
        rating: Some(3.7),
        count: Some(5),
        writer: Some("Jason Aaron".into()),
        penciller: Some("Simone Bianchi".into()),
        genre: Some("Cosmique, Super-héros".into()),
        story_arc: Some("Quatrième époque : 2011-2016".into()),
        alternate_series: Some("Thanos".into()),
        added: Some("2019-10-01T12:58:00.1234567+02:00".into()),
        opened: Some("2026-10-20T21:00:00+02:00".into()),
        released: Some("2013-04-17".into()),
        open_count: Some(4),
        file_size: Some(48_173_972_000),
        file_modified_time: Some("2013-05-01T10:00:00Z".into()),
        file_creation_time: Some("2013-04-30T09:00:00Z".into()),
        alternate_number: Some("1".into()),
        alternate_count: Some(-1),
        series_group: Some("Cosmique".into()),
        summary: Some("Résumé\nsur plusieurs lignes ; avec \"guillemets\" et 'apostrophes'.".repeat(200)),
        notes: Some("notes".into()),
        review: Some("critique".into()),
        month: Some(5),
        day: Some(17),
        inker: Some("encreur".into()),
        colorist: Some("coloriste".into()),
        letterer: Some("lettreur".into()),
        cover_artist: Some("couverture".into()),
        editor: Some("éditeur".into()),
        translator: Some("traducteur".into()),
        imprint: Some("Marvel Now".into()),
        web: Some("https://example.org/a?b=c&d".into()),
        language_iso: Some("fr".into()),
        format: Some("TPB".into()),
        age_rating: Some("Teen".into()),
        black_and_white: Some("No".into()),
        manga: Some("No".into()),
        characters: Some("Thanos, Mort".into()),
        teams: Some("Avengers".into()),
        main_character_or_team: Some("Thanos".into()),
        locations: Some("Titan".into()),
        tags: Some("à lire".into()),
        community_rating: Some(4.25),
        book_age: Some("Modern".into()),
        book_collection_status: Some("Owned".into()),
        book_condition: Some("Mint".into()),
        book_location: Some("Étagère 2".into()),
        book_notes: Some("prêté à Océane".into()),
        book_owner: Some("Yoyo".into()),
        book_store: Some("BD Narbonne".into()),
        isbn: Some("978-0-7851-8454-0".into()),
        book_price: Some(19.99),
        checked: Some(false),
        comic_info_is_dirty: Some(true),
        comic_book_is_dirty: Some(false),
        enable_proposed: Some(true),
        proposed_cache: Default::default(),
        missing: Some(true),
        series_complete: Some("Yes".into()),
        scan_information: Some("scan HD".into()),
        preferred_front_cover: Some(1),
        pages: PageList {
            page: vec![
                PageInfo { image: 0, kind: Some("FrontCover".into()) },
                PageInfo { image: 1, kind: None },
                PageInfo { image: 7, kind: Some("FrontCover, InnerCover".into()) },
            ],
        },
    }
}

/// `LibraryBook` n'a pas `PartialEq` : comparaison par le `Debug` (tous les champs,
/// f32 exacts ; le cache `proposed_cache` reste non initialisé des deux côtés).
fn same(a: &LibraryBook, b: &LibraryBook) {
    assert_eq!(format!("{a:#?}"), format!("{b:#?}"));
}

fn simple(i: usize) -> LibraryBook {
    LibraryBook {
        id: format!("bbbbbbbb-0000-0000-0000-{i:012}"),
        file: Some(format!(r"Z:\BD\Série {i:05}.cbz")),
        series: Some("Série é".into()),
        number: Some(i.to_string()),
        page_count: Some(24),
        pages: PageList { page: vec![PageInfo { image: 0, kind: Some("FrontCover".into()) }] },
        ..Default::default()
    }
}

fn temp_cfg() -> (tempfile::TempDir, StoreConfig) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = StoreConfig::Sqlite(dir.path().join("data").join(SQLITE_FILE_NAME));
    (dir, cfg)
}

#[test]
fn store_is_send() {
    fn send<T: Send>() {}
    send::<Store>();
    send::<StoreWatcher>();
}

#[test]
fn new_ids_look_like_comicrack() {
    let id = new_book_id();
    assert_eq!(id.len(), 36);
    assert_eq!(id, id.to_ascii_lowercase());
    assert_eq!(id.matches('-').count(), 4);
    assert_ne!(id, new_book_id());
}

#[test]
fn config_hides_password() {
    let cfg = StoreConfig::MySql(DbConfig {
        host: "nas".into(),
        port: 3306,
        user: "yo".into(),
        password: "SeCrEt".into(),
        database: "bd".into(),
    });
    assert!(!format!("{cfg:?}").contains("SeCrEt"));
    assert_eq!(cfg.describe(), "MySQL yo@nas:3306/bd");
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("SeCrEt"), "jamais en clair dans les réglages : {json}");
    if cfg!(windows) {
        assert_eq!(serde_json::from_str::<StoreConfig>(&json).unwrap(), cfg);
    }
}

#[test]
fn sqlite_create_and_full_round_trip() {
    let (_dir, cfg) = temp_cfg();
    assert_eq!(Store::test(&cfg).unwrap(), 0, "base créée vide (dossier data compris)");
    let mut s = Store::open(&cfg).unwrap();
    assert_eq!(s.counters().unwrap(), Counters::default());
    let full = full_book("30815f16-abb9-4c38-8c24-ecae11e25b1b");
    let empty = LibraryBook { id: "aaaaaaaa-0000-0000-0000-000000000000".into(), ..Default::default() };
    assert_eq!(s.upsert(&[full.clone(), empty.clone()]).unwrap(), 1);

    let db = s.load(None).unwrap();
    assert_eq!(db.len(), 2);
    same(db.get(&full.id).unwrap(), &full);
    same(db.get(&empty.id).unwrap(), &empty);
    assert_eq!(db.counters, Counters { update: 1, delete: 0 });
    assert_eq!(db.rows.get("30815f16-abb9-4c38-8c24-ecae11e25b1b"), Some(&1));
    // Identifiant sans casse, comme ComicRack.
    let got = s.get(&["30815F16-ABB9-4C38-8C24-ECAE11E25B1B".into()]).unwrap();
    same(&got[0], &full);
}

#[test]
fn sqlite_upsert_delete_progress_folders() {
    let (_dir, cfg) = temp_cfg();
    let mut s = Store::open(&cfg).unwrap();
    s.upsert(&[simple(1), simple(2), simple(3)]).unwrap();
    // Remplacement d'un livre existant : nouvelle fiche, nouveau compteur.
    let mut b2 = simple(2);
    b2.series = Some("Modifiée".into());
    b2.number = None;
    assert_eq!(s.upsert(&[b2.clone()]).unwrap(), 2);
    let db = s.load(None).unwrap();
    assert_eq!(db.len(), 3);
    same(db.get(&b2.id).unwrap(), &b2);
    assert_eq!(db.rows[&simple(2).id], 2);
    assert_eq!(db.rows[&simple(1).id], 1);

    assert!(s.upsert(&[LibraryBook::default()]).is_err(), "livre sans identifiant refusé");

    assert_eq!(s.delete(&[simple(1).id.to_uppercase(), "inconnu".into()]).unwrap(), 1);
    assert_eq!(s.counters().unwrap(), Counters { update: 2, delete: 1 });
    assert_eq!(s.count().unwrap(), 2);

    let c = s.set_progress(&simple(3).id, 10, 11, "2026-10-23T08:00:00+02:00").unwrap();
    assert_eq!(c, 3);
    s.set_progress(&simple(3).id, 12, 12, "2026-10-23T09:00:00+02:00").unwrap();
    let b3 = s.get(&[simple(3).id]).unwrap().remove(0);
    assert_eq!((b3.current_page, b3.last_page_read, b3.open_count), (Some(12), Some(12), Some(2)));
    assert_eq!(b3.opened.as_deref(), Some("2026-10-23T09:00:00+02:00"));
    assert_eq!(b3.series.as_deref(), Some("Série é"), "autres champs intacts");
    assert!(s.set_progress("inconnu", 1, 1, "x").is_err());
    assert_eq!(s.counters().unwrap().update, 4, "échec annulé : compteur inchangé");

    assert!(s.folders().unwrap().is_empty());
    let f = vec![PathBuf::from(r"Z:\BD\Marvel"), PathBuf::from("/srv/bd/é"), PathBuf::from("/srv/bd/e")];
    s.set_folders(&f).unwrap();
    assert_eq!(s.folders().unwrap(), vec![PathBuf::from("/srv/bd/e"), PathBuf::from("/srv/bd/é"), f[0].clone()]);
    s.set_folders(&f[..1]).unwrap();
    assert_eq!(s.folders().unwrap(), f[..1]);

    // Réouverture d'une base existante : rien n'est perdu ni recréé.
    drop(s);
    let mut s = Store::open(&cfg).unwrap();
    assert_eq!(s.count().unwrap(), 2);
    assert_eq!(s.counters().unwrap(), Counters { update: 4, delete: 1 });
    assert_eq!(s.folders().unwrap(), f[..1]);
}

/// Deux ComicRust sur la même base : le second voit ajouts, modifications et suppressions.
fn shared_base_scenario(cfg: &StoreConfig) {
    let mut a = Store::open(cfg).unwrap();
    a.upsert(&(0..10).map(simple).collect::<Vec<_>>()).unwrap();

    let mut b = Store::open(cfg).unwrap();
    let mut db = b.load(None).unwrap();
    assert_eq!(db.len(), 10);
    let mut w = StoreWatcher::new(cfg.clone(), &db);
    assert!(w.poll().unwrap().is_none(), "rien n'a changé");

    // « A » modifie, ajoute, lit et supprime.
    let mut b4 = simple(4);
    b4.title = Some("Titre".into());
    a.upsert(&[b4, simple(20)]).unwrap();
    a.set_progress(&simple(5).id, 3, 3, "2026-10-23").unwrap();
    a.delete(&[simple(6).id]).unwrap();
    let ch = w.poll().unwrap().expect("modifications vues");
    assert_eq!(db.apply(ch), Applied { added: 1, modified: 2, removed: 1 });
    assert_eq!(db.get(&simple(4).id).unwrap().title.as_deref(), Some("Titre"));
    assert_eq!(db.get(&simple(5).id).unwrap().open_count, Some(1));
    assert!(db.get(&simple(6).id).is_none());
    assert!(w.poll().unwrap().is_none());
    assert!(w.full_check().unwrap().is_none(), "vérification complète : rien de plus");

    // Écritures de « B » lui-même : signalées, donc pas renvoyées.
    let c = b.upsert(&[simple(7)]).unwrap();
    w.note_written([simple(7).id.as_str()], c);
    b.delete(&[simple(8).id]).unwrap();
    w.note_deleted([simple(8).id.as_str()]);
    assert!(w.poll().unwrap().is_none(), "nos propres écritures ne reviennent pas");

    // Écriture « discrète » (compteur de ligne changé sans le compteur global) :
    // seule la vérification complète la voit.
    let mut conn = match cfg {
        StoreConfig::MySql(c) => Conn::open_mysql(c).unwrap(),
        _ => Conn::open_sqlite(&cfg.sqlite_path().unwrap().unwrap()).unwrap(),
    };
    conn.exec(
        &format!("UPDATE {BOOKS} SET `series` = 'Discrète', `update_counter` = 1000 WHERE `id` = ?"),
        &[Val::Text(simple(9).id)],
    )
    .unwrap();
    assert!(w.poll().unwrap().is_none());
    let ch = w.full_check().unwrap().expect("rattrapé par la vérification complète");
    db.apply(ch);
    assert_eq!(db.get(&simple(9).id).unwrap().series.as_deref(), Some("Discrète"));
}

#[test]
fn sqlite_watcher_two_instances() {
    let (_dir, cfg) = temp_cfg();
    shared_base_scenario(&cfg);
}

#[test]
fn sqlite_5000_books() {
    let (_dir, cfg) = temp_cfg();
    let mut s = Store::open(&cfg).unwrap();
    let books: Vec<LibraryBook> = (0..5000)
        .map(|i| {
            let mut b = full_book(&new_book_id());
            b.summary = Some("Un résumé de taille raisonnable pour une BD.".into());
            b.number = Some(i.to_string());
            b
        })
        .collect();
    let t = Instant::now();
    s.upsert(&books).unwrap();
    let write = t.elapsed();
    let t = Instant::now();
    let progress = LoadProgress::default();
    let db = s.load(Some(&progress)).unwrap();
    let read = t.elapsed();
    eprintln!("5 000 livres SQLite : écriture {write:?}, lecture {read:?}");
    assert_eq!(db.len(), 5000);
    assert_eq!(progress.get(), (5000, 5000));
    same(db.get(&books[4321].id).unwrap(), &books[4321]);
    // Large marge pour une CI lente (en local : bien moins d'une seconde en release).
    assert!(write.as_secs() < 20, "écriture trop lente : {write:?}");
    assert!(read.as_secs() < 20, "lecture trop lente : {read:?}");
}

/// Scénario des écritures ciblées (SQLite et MySQL) : une lecture enregistrée par
/// `set_progress` **après** la copie qui a servi à construire le rapport est conservée.
fn targeted_writes_scenario(cfg: &StoreConfig) {
    let mut s = Store::open(cfg).unwrap();
    let mut a = simple(1);
    a.rating = Some(4.0);
    a.book_notes = Some("prêté".into());
    a.checked = Some(true);
    a.added = Some("2020-01-01T00:00:00+01:00".into());
    let mut b = simple(2);
    b.comic_info_is_dirty = Some(true);
    b.series = Some("Corrigée à la main".into());
    let m = simple(3);
    let c = simple(4);
    let mut d = simple(5);
    d.missing = Some(true);
    s.upsert(&[a.clone(), b.clone(), m.clone(), c.clone(), d.clone()]).unwrap();

    // Copie en mémoire « ancienne », puis lecture enregistrée par un autre chemin.
    let old = s.load(None).unwrap();
    s.set_progress(&a.id, 5, 6, "2026-10-24T10:00:00+02:00").unwrap();
    s.set_progress(&m.id, 2, 2, "2026-10-24T11:00:00+02:00").unwrap();

    // Rapport construit à partir de la copie ancienne.
    let mut a2 = old.get(&a.id).unwrap().clone();
    a2.series = Some("Relue".into());
    a2.page_count = Some(30);
    a2.file_size = Some(999);
    a2.pages = PageList::default();
    a2.current_page = None;
    a2.rating = None;
    a2.book_notes = None;
    let mut b2 = old.get(&b.id).unwrap().clone();
    b2.series = Some("Du fichier".into());
    b2.page_count = Some(7);
    b2.comic_info_is_dirty = None;
    let mut m2 = old.get(&m.id).unwrap().clone();
    m2.file = Some(r"Z:\BD\Rangé\Série 00003.cbz".into());
    m2.series = Some("ne doit pas être écrite".into());
    let mut n1 = simple(10);
    n1.id = "nouveau-1".into();
    let mut n2 = simple(11);
    n2.id = "nouveau-2".into();
    // Même fichier que `a`, autre casse et autres séparateurs (autre poste).
    n2.file = Some(a.file.clone().unwrap().to_uppercase().replace('\\', "/"));
    let mut n3 = n1.clone();
    n3.id = "nouveau-3".into();
    let report = ScanReport {
        added: vec![n1.clone(), n2, n3],
        updated: vec![a2, b2, m2, simple(99)],
        moved: vec![m.id.clone()],
        missing: vec![c.id.clone()],
        back: vec![d.id.clone()],
        ..Default::default()
    };
    let w = s.apply_scan(&report).unwrap();
    assert_eq!(w.skipped, ["nouveau-2", "nouveau-3"]);
    let mut ids: Vec<&str> = w.books.iter().map(|b| b.id.as_str()).collect();
    ids.sort();
    let mut want = vec![a.id.as_str(), b.id.as_str(), m.id.as_str(), c.id.as_str(), d.id.as_str(), "nouveau-1"];
    want.sort();
    assert_eq!(ids, want, "fiches écrites, relues ; livre inconnu ignoré");
    assert_eq!(s.counters().unwrap().update, w.counter);

    let get = |s: &mut Store, id: &str| s.get(&[id.to_string()]).unwrap().remove(0);
    let ga = get(&mut s, &a.id);
    assert_eq!((ga.series.as_deref(), ga.page_count, ga.file_size), (Some("Relue"), Some(30), Some(999)));
    assert!(ga.pages.page.is_empty());
    assert_eq!((ga.current_page, ga.last_page_read, ga.open_count), (Some(5), Some(6), Some(1)), "lecture gardée");
    assert_eq!(ga.opened.as_deref(), Some("2026-10-24T10:00:00+02:00"));
    assert_eq!((ga.rating, ga.book_notes.as_deref(), ga.checked), (Some(4.0), Some("prêté"), Some(true)));
    assert_eq!(ga.added, a.added);
    same(w.books.iter().find(|x| x.id == a.id).unwrap(), &ga);
    let gb = get(&mut s, &b.id);
    assert_eq!(gb.series.as_deref(), Some("Corrigée à la main"), "fiche modifiée non écrite : ComicInfo gardé");
    assert_eq!((gb.page_count, gb.comic_info_is_dirty), (Some(7), Some(true)));
    let gm = get(&mut s, &m.id);
    assert_eq!(gm.file.as_deref(), Some(r"Z:\BD\Rangé\Série 00003.cbz"));
    assert_eq!(gm.series.as_deref(), Some("Série é"), "déplacé : seul le fichier change");
    assert_eq!((gm.current_page, gm.open_count), (Some(2), Some(1)));
    assert_eq!(get(&mut s, &c.id).missing, Some(true));
    assert_eq!(get(&mut s, &d.id).missing, None);
    assert_eq!(s.count().unwrap(), 6);

    // Rapport rejoué (deux postes) : rien n'est ajouté en double, les manquants déjà
    // marqués ne sont pas réécrits.
    let again = ScanReport { added: vec![n1], missing: vec![c.id.clone()], ..Default::default() };
    let w2 = s.apply_scan(&again).unwrap();
    assert!(w2.books.is_empty() && w2.skipped == ["nouveau-1"], "{w2:?}");
    assert_eq!(s.count().unwrap(), 6);

    // ComicInfo écrit dans le fichier : seul l'indicateur change.
    let before = get(&mut s, &b.id);
    let cnt = s.set_comic_info_written(&[b.id.to_uppercase()]).unwrap();
    let mut after = get(&mut s, &b.id);
    assert_eq!(after.comic_info_is_dirty, None);
    after.comic_info_is_dirty = before.comic_info_is_dirty;
    same(&after, &before);
    assert_eq!(s.load(None).unwrap().rows[&b.id], cnt);

    // Conversion en CBZ : fichier et ComicInfo depuis la fiche neuve, le reste gardé.
    s.set_progress(&a.id, 8, 9, "2026-10-24T12:00:00+02:00").unwrap();
    let mut fresh = simple(1);
    fresh.id = "quelconque".into();
    fresh.file = Some(r"Z:\BD\Série 00001 (converti).cbz".into());
    fresh.series = Some("Convertie".into());
    fresh.page_count = Some(31);
    s.replace_file(&a.id, &fresh).unwrap();
    let ga = get(&mut s, &a.id);
    assert_eq!((ga.file, ga.series.as_deref(), ga.page_count), (fresh.file.clone(), Some("Convertie"), Some(31)));
    assert_eq!((ga.current_page, ga.last_page_read, ga.open_count), (Some(8), Some(9), Some(2)));
    assert_eq!((ga.rating, ga.book_notes.as_deref()), (Some(4.0), Some("prêté")));
    assert!(s.replace_file("inconnu", &fresh).is_err());
}

#[test]
fn sqlite_targeted_writes_keep_reading_progress() {
    let (_dir, cfg) = temp_cfg();
    targeted_writes_scenario(&cfg);
}

/// CBZ minimal d'une page.
fn tiny_cbz(path: &Path) {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut w = ::zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    w.start_file("001.jpg", ::zip::write::SimpleFileOptions::default()).unwrap();
    w.write_all(b"x").unwrap();
    w.finish().unwrap();
}

#[test]
fn scan_into_saves_added_books_in_batches() {
    let (dir, cfg) = temp_cfg();
    let root = dir.path().join("BD");
    let n = 2 * scanner::ADD_BATCH + 50;
    for i in 0..n {
        tiny_cbz(&root.join(format!("Livre {i:04}.cbz")));
    }
    let mut s = Store::open(&cfg).unwrap();
    s.upsert(&[simple(1)]).unwrap();
    s.set_progress(&simple(1).id, 3, 3, "2026-10-24T08:00:00+02:00").unwrap();
    let folders = [root.clone()];
    let p = ScanProgress::default();
    let (report, writes) = s.scan_into(&folders, &[], &p, &AtomicBool::new(false), new_book_id).unwrap();
    assert_eq!(report.added.len(), n);
    // 3 lots (200, 200, 50) puis l'écriture finale (rien de plus à écrire).
    let sizes: Vec<usize> = writes.iter().map(|w| w.books.len()).collect();
    assert_eq!(sizes, [scanner::ADD_BATCH, scanner::ADD_BATCH, 50, 0]);
    assert!(writes.iter().all(|w| w.skipped.is_empty()));
    assert!(writes.windows(2).all(|w| w[0].counter < w[1].counter));
    assert_eq!(s.count().unwrap(), n + 1);

    // Analyse suivante : tout est connu, la lecture du livre déjà présent est intacte.
    let known = s.load(None).unwrap().books;
    std::fs::remove_file(root.join("Livre 0000.cbz")).unwrap();
    let (report, writes) = s.scan_into(&folders, &known, &p, &AtomicBool::new(false), new_book_id).unwrap();
    assert!(report.added.is_empty() && report.missing.len() == 1, "{}", report.summary());
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].books.len(), 1);
    assert_eq!(writes[0].books[0].missing, Some(true));
    assert_eq!(s.get(&[simple(1).id]).unwrap()[0].current_page, Some(3));
}

#[test]
fn portable_sqlite_follows_the_executable() {
    let cfg = StoreConfig::portable_sqlite().unwrap();
    assert_eq!(cfg, StoreConfig::PortableSqlite);
    let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let path = cfg.sqlite_path().unwrap().unwrap();
    assert_eq!(path, exe_dir.join("data").join(SQLITE_FILE_NAME));
    assert!(cfg.describe().starts_with("SQLite portable "), "{}", cfg.describe());
    if !path.exists() {
        let err = Store::open_existing(&cfg).err().expect("base absente refusée").to_string();
        assert!(err.contains("base ComicRust introuvable : "), "{err}");
        assert!(!path.exists(), "rien n'a été créé");
    }
    // Ancienne configuration (chemin fixe) toujours lisible ; la nouvelle s'enregistre.
    let old: StoreConfig = serde_json::from_str(r#"{"Sqlite":"C:\\ComicRust\\data\\comicrust.db"}"#).unwrap();
    assert_eq!(old, StoreConfig::Sqlite(PathBuf::from(r"C:\ComicRust\data\comicrust.db")));
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("SeCrEt"), "jamais en clair dans les réglages : {json}");
    if cfg!(windows) {
        assert_eq!(serde_json::from_str::<StoreConfig>(&json).unwrap(), cfg);
    }
}

#[test]
fn sqlite_schema_1_is_migrated() {
    let (_dir, cfg) = temp_cfg();
    let mut s = Store::open(&cfg).unwrap();
    s.set_folders(&[PathBuf::from("/srv/bd")]).unwrap();
    drop(s);
    let path = cfg.sqlite_path().unwrap().unwrap();
    Conn::open_sqlite(&path)
        .unwrap()
        .batch(&format!("UPDATE {META} SET `value` = '1' WHERE `key` = 'schema_version'"))
        .unwrap();
    let mut s = Store::open_existing(&cfg).unwrap();
    assert_eq!(schema::meta_int(&mut s.conn, "schema_version").unwrap(), SCHEMA_VERSION);
    assert_eq!(s.folders().unwrap(), [PathBuf::from("/srv/bd")]);
}

#[test]
fn open_existing_refuses_a_missing_sqlite_file() {
    let (dir, cfg) = temp_cfg();
    let err = Store::open_existing(&cfg).err().expect("fichier absent refusé").to_string();
    assert!(err.contains("base ComicRust introuvable : "), "{err}");
    assert!(err.contains(SQLITE_FILE_NAME), "{err}");
    assert!(!dir.path().join("data").exists(), "rien n'a été créé");
    // Créée par l'assistant (`open`), elle s'ouvre ensuite normalement.
    Store::open(&cfg).unwrap().upsert(&[simple(1)]).unwrap();
    assert_eq!(Store::open_existing(&cfg).unwrap().count().unwrap(), 1);
}

#[test]
fn mysql_statements_fit_old_servers() {
    use super::conn::Engine;
    let sql = schema::create_statements(Engine::MySql).join("\n");
    assert!(sql.contains("`file`(191)"), "{sql}");
    assert!(!sql.contains("VARCHAR(700)"));
    let folders = schema::create_statements(Engine::MySql).into_iter().find(|s| s.contains(FOLDERS)).unwrap();
    assert!(folders.contains("`path_hash` CHAR(64) CHARACTER SET ascii NOT NULL PRIMARY KEY"), "{folders}");
    assert!(folders.contains("`path` TEXT"), "{folders}");
}

#[test]
fn replace_batches_stay_under_the_packet_limit() {
    let small: Vec<Vec<Val>> = (0..450).map(|_| vec![Val::Text("x".repeat(100)), Val::Int(1)]).collect();
    let b = statement_batches(&small, 200, 1024 * 1024);
    assert_eq!(b, [0..200, 200..400, 400..450]);
    // Fiches à gros résumé : découpées par taille, une fiche énorme seule.
    let big = |n: usize| vec![Val::Text("r".repeat(n)), Val::Null];
    let rows = vec![big(400_000), big(400_000), big(400_000), big(3_000_000), big(10)];
    let b = statement_batches(&rows, 200, 1024 * 1024);
    assert_eq!(b, [0..2, 2..3, 3..4, 4..5]);
    for r in &b {
        let bytes: usize = rows[r.clone()].iter().map(|row| row_bytes(row)).sum();
        assert!(r.len() == 1 || bytes <= 1024 * 1024);
    }
    assert!(statement_batches(&[], 200, 10).is_empty());
}

#[test]
fn missing_columns_are_added() {
    let (_dir, cfg) = temp_cfg();
    let StoreConfig::Sqlite(path) = &cfg else { unreachable!() };
    // Base « ancienne » à laquelle il manque une colonne.
    {
        let mut c = Conn::open_sqlite(path).unwrap();
        c.batch(&format!(
            "CREATE TABLE {BOOKS} (`id` TEXT PRIMARY KEY, `update_counter` INTEGER, `pages` TEXT, `file` TEXT)"
        ))
        .unwrap();
        c.batch(&format!("INSERT INTO {BOOKS} (`id`, `file`) VALUES ('x', 'a.cbz')")).unwrap();
    }
    let mut s = Store::open(&cfg).unwrap();
    let db = s.load(None).unwrap();
    assert_eq!(db.get("x").unwrap().file.as_deref(), Some("a.cbz"));
    s.upsert(&[full_book("y")]).unwrap();
    same(&s.get(&["y".into()]).unwrap()[0], &full_book("y"));
}

#[test]
fn newer_schema_is_refused() {
    let (_dir, cfg) = temp_cfg();
    drop(Store::open(&cfg).unwrap());
    let StoreConfig::Sqlite(path) = &cfg else { unreachable!() };
    let mut c = Conn::open_sqlite(path).unwrap();
    c.batch(&format!("UPDATE {META} SET `value` = '99' WHERE `key` = 'schema_version'")).unwrap();
    let err = Store::open(&cfg).err().unwrap().to_string();
    assert!(err.contains("plus récente"), "{err}");
}

fn drop_tables(db_cfg: &DbConfig) {
    let mut c = Conn::open_mysql(db_cfg).unwrap();
    for t in [BOOKS, FOLDERS, META] {
        c.batch(&format!("DROP TABLE IF EXISTS {t}")).unwrap();
    }
}

/// Test d'intégration contre un vrai serveur : `COMICRUST_TEST_MYSQL` = chaîne de
/// connexion d'une base de test. Supprime et recrée uniquement les tables `comicrust_*`.
#[test]
fn mysql_store() {
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL du store ignoré");
        return;
    };
    let db_cfg = DbConfig::from_connection_string(&url).unwrap();
    let cfg = StoreConfig::MySql(db_cfg.clone());
    {
        let mut c = Conn::open_mysql(&db_cfg).unwrap();
        for t in [BOOKS, FOLDERS, META] {
            c.batch(&format!("DROP TABLE IF EXISTS {t}")).unwrap();
        }
    }
    assert_eq!(Store::test(&cfg).unwrap(), 0);
    let mut s = Store::open(&cfg).unwrap();
    // Aller-retour complet, chemin de plus de 255 caractères compris.
    let mut full = full_book("30815f16-abb9-4c38-8c24-ecae11e25b1b");
    full.file = Some(format!(r"Z:\BD\{}\é 🦸.cbz", "dossier très long ".repeat(30)));
    s.upsert(&[full.clone()]).unwrap();
    let db = s.load(None).unwrap();
    same(db.get(&full.id).unwrap(), &full);

    // 1 000 livres (REPLACE multiple).
    let books: Vec<LibraryBook> = (0..1000).map(simple).collect();
    let t = Instant::now();
    s.upsert(&books).unwrap();
    eprintln!("1 000 livres MySQL : {:?}", t.elapsed());
    assert_eq!(s.count().unwrap(), 1001);
    s.delete(&books.iter().skip(10).map(|b| b.id.clone()).collect::<Vec<_>>()).unwrap();
    assert_eq!(s.count().unwrap(), 11);
    s.set_folders(&[PathBuf::from("/srv/bd/é"), PathBuf::from("/srv/bd/e"), PathBuf::from("/srv/bd/e")]).unwrap();
    assert_eq!(s.folders().unwrap().len(), 2, "chemins distincts malgré l'accent, doublon écarté");
    // Chemin de dossier plus long que n'importe quelle clé d'index d'un ancien MySQL.
    let long = PathBuf::from(format!(r"\\SERVEURNAS\BD\{}", "sous-dossier très long é\\".repeat(40)));
    s.set_folders(std::slice::from_ref(&long)).unwrap();
    assert_eq!(s.folders().unwrap(), [long]);
    // Fiches à gros résumé (≈ 6 Mio en tout) : plusieurs requêtes, chacune < 4 Mio.
    let heavy: Vec<LibraryBook> = (0..12)
        .map(|i| {
            let mut b = simple(5000 + i);
            b.summary = Some("é".repeat(250_000));
            b
        })
        .collect();
    s.upsert(&heavy).unwrap();
    same(&s.get(&[heavy[11].id.clone()]).unwrap()[0], &heavy[11]);
    s.delete(&heavy.iter().map(|b| b.id.clone()).collect::<Vec<_>>()).unwrap();
    drop(s);

    let mut c = Conn::open_mysql(&db_cfg).unwrap();
    for t in [BOOKS, FOLDERS, META] {
        c.batch(&format!("DROP TABLE IF EXISTS {t}")).unwrap();
    }
    shared_base_scenario(&cfg);

    // Écritures ciblées.
    drop_tables(&db_cfg);
    targeted_writes_scenario(&cfg);

    // Migration du schéma 1 (clé `VARCHAR(700)` de la v0.7.0) vers le schéma 2.
    drop_tables(&db_cfg);
    {
        let mut c = Conn::open_mysql(&db_cfg).unwrap();
        c.batch(&format!(
            "CREATE TABLE {FOLDERS} (`path` VARCHAR(700) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin \
             NOT NULL PRIMARY KEY) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"
        ))
        .unwrap();
        c.batch(&format!("CREATE TABLE {META} (`key` VARCHAR(64) NOT NULL PRIMARY KEY, `value` TEXT) ENGINE=InnoDB"))
            .unwrap();
        c.batch(&format!("INSERT INTO {META} VALUES ('schema_version', '1'), ('update_counter', '7')")).unwrap();
        c.batch(&format!("INSERT INTO {FOLDERS} VALUES ('/srv/bd/é'), ('/srv/bd/e')")).unwrap();
    }
    let mut s = Store::open_existing(&cfg).unwrap();
    assert_eq!(s.folders().unwrap(), [PathBuf::from("/srv/bd/e"), PathBuf::from("/srv/bd/é")]);
    assert_eq!(schema::meta_int(&mut s.conn, "schema_version").unwrap(), SCHEMA_VERSION);
    assert_eq!(s.counters().unwrap().update, 7);
    let hashes = s.conn.query(&format!("SELECT `path_hash` FROM {FOLDERS} ORDER BY `path`"), &[]).unwrap();
    assert_eq!(hashes.len(), 2);
    let h = hashes[0][0].clone().into_text().unwrap();
    assert!(h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()), "{h}");
    // SHA-256 de « /srv/bd/e » (`printf /srv/bd/e | sha256sum`).
    assert_eq!(h, "dfede2b30f0d9d7339d5d089231c46dd08cab1c8454189da84149a038e4308a7");
    s.set_folders(&[PathBuf::from("/srv/bd/e")]).unwrap();
    assert_eq!(s.folders().unwrap(), [PathBuf::from("/srv/bd/e")]);
    drop(s);

    // Anciens serveurs (clé d'index ≤ 767 octets) : simulés par ROW_FORMAT=COMPACT, qui
    // garde cette limite sur les serveurs récents. Tables renommées pour ne rien toucher.
    {
        let mut c = Conn::open_mysql(&db_cfg).unwrap();
        let old_key = "CREATE TABLE zz_cr_old (`path` VARCHAR(700) CHARACTER SET utf8mb4 NOT NULL PRIMARY KEY) \
                       ROW_FORMAT=COMPACT";
        assert!(c.batch(old_key).is_err(), "la simulation reproduit bien la limite");
        // Réglage par défaut de MySQL 5.5 / 5.6 et MariaDB < 10.2 : sinon la taille de
        // ligne théorique (préfixes de 768 octets des ~70 colonnes TEXT) est refusée.
        // (MySQL 8 refuse ce réglage sans privilège particulier : simulation sautée.)
        let relaxed = c.batch("SET SESSION innodb_strict_mode = OFF").is_ok();
        for sql in schema::create_statements(super::conn::Engine::MySql).into_iter().filter(|_| relaxed) {
            let sql = sql.replace("comicrust_", "zz_cr_") + " ROW_FORMAT=COMPACT";
            c.batch(&sql).unwrap_or_else(|e| panic!("{e}\n{sql}"));
        }
        for t in ["zz_cr_books", "zz_cr_folders", "zz_cr_meta"] {
            c.batch(&format!("DROP TABLE IF EXISTS {t}")).unwrap();
        }
    }

    let mut bad = db_cfg;
    bad.password = "mauvais-mdp".into();
    let err = Store::open(&StoreConfig::MySql(bad)).err().unwrap().to_string();
    assert!(!err.contains("mauvais-mdp"), "le mot de passe ne doit pas apparaître : {err}");
}
