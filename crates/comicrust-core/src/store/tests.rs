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
    assert_eq!(serde_json::from_str::<StoreConfig>(&json).unwrap(), cfg);
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
        StoreConfig::Sqlite(p) => Conn::open_sqlite(p).unwrap(),
        StoreConfig::MySql(c) => Conn::open_mysql(c).unwrap(),
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
    s.set_folders(&[PathBuf::from("/srv/bd/é"), PathBuf::from("/srv/bd/e")]).unwrap();
    assert_eq!(s.folders().unwrap().len(), 2, "chemins distincts malgré l'accent");
    drop(s);

    let mut c = Conn::open_mysql(&db_cfg).unwrap();
    for t in [BOOKS, FOLDERS, META] {
        c.batch(&format!("DROP TABLE IF EXISTS {t}")).unwrap();
    }
    shared_base_scenario(&cfg);

    let mut bad = db_cfg;
    bad.password = "mauvais-mdp".into();
    let err = Store::open(&StoreConfig::MySql(bad)).err().unwrap().to_string();
    assert!(!err.contains("mauvais-mdp"), "le mot de passe ne doit pas apparaître : {err}");
}
