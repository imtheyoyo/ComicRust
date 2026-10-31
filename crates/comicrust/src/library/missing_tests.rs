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
            Candidate {
                id: "c".into(),
                path: "/rangé/c/x.cbz".into(),
                size: 1,
                kind: MatchKind::NameOtherSize,
                others: 0,
            },
            Candidate { id: "b".into(), path: "/autre/b.cbz".into(), size: 1, kind: MatchKind::NameAndSize, others: 2 },
        ],
        ..Default::default()
    };
    let p = s.proposals(&report);
    assert_eq!(p.len(), 4);
    assert!(p[0].checked && !p[1].checked, "par taille seule : à cocher soi-même");
    assert!(!p[2].checked, "même nom, taille différente : à cocher soi-même");
    assert!(!p[3].checked, "plusieurs fichiers de même nom : à cocher soi-même");
    // Fichier choisi déjà dans la bibliothèque (doublon) : décoché, signalé.
    let p = chosen_proposal(&s.database().unwrap(), "b", "/rangé/x.cbz".into(), 3).unwrap();
    assert!(!p.checked && p.duplicate.is_some());
    let p = chosen_proposal(&s.database().unwrap(), "a", "/ailleurs/29 - marvel team-up #55.CBZ".into(), 100).unwrap();
    assert_eq!((p.kind, p.checked), (MatchKind::NameAndSize, true));
    let known = s.known_files();
    assert!(known.contains(std::path::Path::new("/RANGÉ/x.cbz")) && !known.contains(std::path::Path::new("/y.cbz")));
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

    let seen_a: HashMap<String, String> = [("a".to_string(), a.file.clone().unwrap())].into();
    let done = wait(persist_marks(cfg.clone(), vec!["a".into()], vec![], seen_a, Some(sink.clone()), &ctx));
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
    let Ok(OpResult::Removed { ids, absent, error }) = done.result else { panic!() };
    assert_eq!((ids, absent, error), (vec!["b".to_string()], vec!["absent".to_string()], None));
    assert_eq!(deleted.pending(), ["b", "absent"], "signalé à la surveillance");
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
    conn.exec_drop("Insert into comics values (?, 1, ?)", (a, xml(a, r"\\NAS\Partage\BD\29 - Marvel Team-Up #55.cbz")))
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
    let Ok(OpResult::Removed { ids, absent, error }) = done.result else { panic!() };
    assert_eq!((ids, absent.len(), error), (vec![b.to_string()], 0, None));
    assert_eq!(deleted.pending(), [b.to_string()]);
    let (u, d): (i64, i64) = conn.query_first("SELECT update_counter, delete_counter FROM changes").unwrap().unwrap();
    assert_eq!((u, d), (6, 1));
    let n: i64 = conn.query_first("SELECT COUNT(*) FROM comics").unwrap().unwrap();
    assert_eq!(n, 1);
    assert!(kept.exists() && new_file.exists(), "aucun fichier supprimé");
}

/// Suppression MySQL partielle : les fiches supprimées sont rendues (et retirées de
/// l'affichage), avec l'erreur.
#[test]
fn partial_mysql_removal_keeps_the_deleted_books() {
    let r = comicrust_core::db::DeleteReport {
        deleted: vec!["a".into()],
        missing: vec!["z".into()],
        failed: vec![("b".into(), "verrou".into())],
        delete_counter: Some(3),
    };
    let OpResult::Removed { ids, absent, error } = removed_from_mysql(r) else { panic!() };
    assert_eq!((ids.as_slice(), absent.as_slice()), (["a".to_string()].as_slice(), ["z".to_string()].as_slice()));
    let e = error.unwrap();
    assert!(e.contains("1 fiche(s) supprimée(s), 1 en erreur") && e.contains("b : verrou"), "{e}");
    let mut s = state(vec![book("a", "/bd/a.cbz"), book("b", "/bd/b.cbz"), book("z", "/bd/z.cbz")]);
    s.remove_books(&[ids, absent].concat());
    assert_eq!(shown(&mut s), ["b"], "seule la fiche en erreur reste affichée");
    let ok = comicrust_core::db::DeleteReport { deleted: vec!["a".into()], ..Default::default() };
    assert!(matches!(removed_from_mysql(ok), OpResult::Removed { error: None, .. }));
}

/// Fenêtre « Retirer » : bouton inactif pendant la vérification ; confirmation exigée si
/// un volume ne répond pas, une disparition massive est bloquée, ou beaucoup de livres.
#[test]
fn removal_needs_a_finished_and_safe_check() {
    use comicrust_core::missing::{CheckReport, Skipped};
    let sure = CheckReport { missing: vec!["a".into()], ..Default::default() };
    assert!(!remove_allowed(None, 1, true), "vérification en cours");
    assert!(remove_allowed(Some(&sure), 1, false));
    assert!(!remove_allowed(Some(&sure), REMOVE_CONFIRM_ABOVE + 1, false), "beaucoup de livres");
    assert!(remove_allowed(Some(&sure), REMOVE_CONFIRM_ABOVE + 1, true));
    let nas = CheckReport {
        unreachable: vec![Skipped { root: r"\\NAS\BD".into(), books: 3, gone: 0 }],
        unknown: vec!["a".into(), "b".into(), "c".into()],
        ..Default::default()
    };
    assert!(remove_needs_confirmation(3, &nas));
    assert!(!remove_allowed(Some(&nas), 3, false), "NAS injoignable : confirmation obligatoire");
    assert!(remove_allowed(Some(&nas), 3, true));
    let massive = CheckReport {
        blocked: vec![Skipped { root: "/srv/bd".into(), books: 40, gone: 30 }],
        unknown: vec!["x".into(); 30],
        ..Default::default()
    };
    assert!(!remove_allowed(Some(&massive), 30, false));
    let unreadable = CheckReport { unknown: vec!["x".into()], ..Default::default() };
    assert!(!remove_allowed(Some(&unreadable), 1, false));
    // Fichiers encore présents (livre non manquant) : confirmation exigée, même pour un seul.
    let present = CheckReport { present: vec!["a".into()], ..Default::default() };
    assert!(remove_needs_confirmation(1, &present));
    assert!(!remove_allowed(Some(&present), 1, false));
    assert!(remove_allowed(Some(&present), 1, true));
    let some = CheckReport { missing: vec!["a".into()], present: vec!["b".into()], ..Default::default() };
    assert!(!remove_allowed(Some(&some), 2, false), "un des deux existe encore");
}

/// Un seul calcul du fichier réel (CBZ converti) pour la vérification et le retrait.
#[test]
fn check_items_use_the_converted_file() {
    let mut a = book("a", "/bd/a.cbr");
    a.missing = Some(true);
    let books = [a, book("b", "/bd/b.cbz"), book("vide", "")];
    let converted: HashMap<std::path::PathBuf, std::path::PathBuf> =
        [("/bd/a.cbr".into(), "/bd/a.cbz".into())].into_iter().collect();
    let items = check_items(&books, &converted);
    assert_eq!(items.len(), 2, "sans fichier : pas vérifié");
    assert_eq!((items[0].file.as_str(), items[0].item.path.to_str()), ("/bd/a.cbr", Some("/bd/a.cbz")));
    assert!(items[0].item.was_missing && !items[1].item.was_missing);
    assert_eq!(items[1].item.path.to_str(), Some("/bd/b.cbz"));
}

/// La surveillance ne peut pas interroger la base pendant une suppression : elle attend la
/// fin, puis relève les identifiants signalés avant d'interroger.
#[test]
fn watcher_never_polls_during_a_removal() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let sink = crate::library::watch::DeletedSink::default();
    let deleting = Arc::new(AtomicBool::new(false));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (s2, d2) = (sink.clone(), deleting.clone());
    let remover = std::thread::spawn(move || {
        s2.deleting(["A".to_string()], || {
            d2.store(true, Ordering::SeqCst);
            started_tx.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(150));
            d2.store(false, Ordering::SeqCst);
        })
    });
    started_rx.recv().unwrap();
    let (noted, during) = sink.noted_then(|ids| (ids, deleting.load(Ordering::SeqCst)));
    remover.join().unwrap();
    assert_eq!(noted, ["a"], "suppression signalée avant l'interrogation");
    assert!(!during, "interrogation jamais pendant la suppression");
    assert!(sink.pending().is_empty());
}

/// Fichier choisi déjà en bibliothèque sous un autre chemin (lien symbolique ≈ `Z:\` ↔
/// `\\SERVEUR\…`) : signalé comme doublon, décoché.
#[cfg(unix)]
#[test]
fn chosen_file_already_in_library_under_an_alias() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("réel/Série/Tome 01.cbz");
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::write(&real, b"BD").unwrap();
    std::os::unix::fs::symlink(dir.path().join("réel"), dir.path().join("alias")).unwrap();
    let alias = dir.path().join("alias/Série/Tome 01.cbz");
    let s = state(vec![book("a", "/ancien/Tome 01.cbz"), book("b", &alias.to_string_lossy())]);
    let p = chosen_proposal(&s.database().unwrap(), "a", real.clone(), 2).unwrap();
    assert!(p.duplicate.is_some() && !p.checked, "{p:?}");
    let other = dir.path().join("réel/Série/Tome 02.cbz");
    std::fs::write(&other, b"BD").unwrap();
    assert!(chosen_proposal(&s.database().unwrap(), "a", other, 2).unwrap().duplicate.is_none());
}

/// Base ComicRust : une fiche déplacée (script, autre poste) **pendant** une vérification
/// longue n'est pas marquée manquante à son nouveau chemin ; une relocalisation ne touche
/// pas une fiche dont le chemin a changé depuis l'ouverture de la fenêtre (v0.10.2).
#[test]
fn marks_are_written_only_for_the_checked_path() {
    use comicrust_core::store::{Store, StoreConfig};
    let dir = tempfile::tempdir().unwrap();
    let cfg = StoreConfig::Sqlite(dir.path().join("comicrust.db"));
    let old = dir.path().join("Séries_Temp/a.cbz").to_string_lossy().into_owned();
    let rangee = dir.path().join("Séries/a.cbz");
    std::fs::create_dir_all(rangee.parent().unwrap()).unwrap();
    std::fs::write(&rangee, b"BD").unwrap();
    let a = book("a", &old);
    let mut b = book("b", &dir.path().join("b.cbz").to_string_lossy());
    b.missing = Some(true);
    Store::open(&cfg).unwrap().upsert(&[a.clone(), b.clone()]).unwrap();
    let ctx = eframe::egui::Context::default();
    let wait = |rx: std::sync::mpsc::Receiver<OpDone>| rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap();

    // La vérification a vu « a » à son ancien chemin (absent) ; pendant ce temps, un script
    // range le fichier et met la fiche à jour.
    let seen: HashMap<String, String> =
        [("a".to_string(), old.clone()), ("b".to_string(), b.file.clone().unwrap())].into();
    let mut moved = a.clone();
    moved.file = Some(rangee.to_string_lossy().into_owned());
    Store::open_existing(&cfg).unwrap().upsert(&[moved.clone()]).unwrap();
    let done = wait(persist_marks(cfg.clone(), vec!["a".into()], vec!["b".into()], seen.clone(), None, &ctx));
    let Ok(OpResult::Store { books, .. }) = done.result else { panic!("{:?}", done.result) };
    assert_eq!(books.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["b"], "seul « b » écrit");
    let mut st = Store::open_existing(&cfg).unwrap();
    let ga = st.get(&["a".to_string()]).unwrap().remove(0);
    assert_eq!((ga.file.as_deref(), ga.missing), (moved.file.as_deref(), None), "fiche déplacée : pas manquante");
    assert_eq!(st.get(&["b".to_string()]).unwrap()[0].missing, None, "revenu : effacé");
    // Livre absent des chemins vérifiés : jamais écrit (pas d'écriture sans condition).
    let done = wait(persist_marks(cfg.clone(), vec!["a".into()], vec![], HashMap::new(), None, &ctx));
    let Ok(OpResult::Store { books, .. }) = done.result else { panic!() };
    assert!(books.is_empty());
    assert_eq!(st.get(&["a".to_string()]).unwrap()[0].missing, None);
    // Sans déplacement : marqué.
    let seen: HashMap<String, String> = [("a".to_string(), moved.file.clone().unwrap())].into();
    let done = wait(persist_marks(cfg.clone(), vec!["a".into()], vec![], seen, None, &ctx));
    assert!(matches!(done.result, Ok(OpResult::Store { ref books, .. }) if books.len() == 1));
    assert_eq!(st.get(&["a".to_string()]).unwrap()[0].missing, Some(true));

    // Relocalisation proposée d'après l'ancienne fiche (chemin `old`) : la fiche a changé
    // depuis, rien n'est écrit.
    let done = wait(relocate(Target::Store(cfg.clone()), vec![(a, rangee.clone())], None, &ctx));
    let Ok(OpResult::Store { books, .. }) = done.result else { panic!("{:?}", done.result) };
    assert!(books.is_empty(), "fiche modifiée entre-temps : pas réécrite");
}

/// Un résultat « manquant » mémorisé cède quand la fiche change dans la base : ComicRack
/// qui efface `<Missing>` (fichier revenu), ou ligne réécrite (compteur) ; il reste tant
/// que la fiche est relue sans changement.
#[test]
fn a_newer_database_state_wins_over_a_remembered_mark() {
    let mut cr = book("c", "/bd/c.cbz");
    cr.missing = Some(true);
    let mut s = state(vec![book("a", "/bd/a.cbz"), cr.clone()]);
    if let Status::Loaded(db) = &mut s.status {
        Arc::make_mut(db).rows.extend([("a".to_string(), 1), ("c".to_string(), 1)]);
    }
    s.set_missing(&["a".into(), "c".into()], true);
    assert_eq!(s.missing_count(), 2);
    // Relecture sans changement (même compteur) : marquage gardé.
    let same = |id: &str, n: i64, b: LibraryBook| Changes {
        updated: vec![b],
        rows: vec![(id.into(), n)],
        ..Default::default()
    };
    s.apply_changes(same("a", 1, book("a", "/bd/a.cbz")));
    assert_eq!(s.database().unwrap().get("a").unwrap().missing, Some(true));
    // ComicRack efface <Missing> de « c » (fichier revenu) : plus manquant.
    let mut c2 = cr.clone();
    c2.missing = None;
    s.apply_changes(same("c", 2, c2.clone()));
    assert_eq!(s.database().unwrap().get("c").unwrap().missing, None);
    // … même sans compteur (ComicDb.xml relu) : `Missing` a changé.
    let mut s2 = state(vec![cr.clone()]);
    s2.set_missing(&["c".into()], true);
    s2.replace(ComicDatabase::from_books(vec![c2]));
    assert_eq!(s2.missing_count(), 0, "base plus récente");
    // Fiche « a » réécrite par ComicRack (compteur changé) : la base a raison.
    s.apply_changes(same("a", 7, book("a", "/bd/a.cbz")));
    assert_eq!(s.missing_count(), 0);
    assert!(s.missing.overlay.is_empty(), "résultats périmés oubliés");
    // Présent d'après la vérification, manquant d'après la base : « présent » tant que la
    // base ne change pas, puis la base.
    let mut s3 = state(vec![cr.clone()]);
    s3.set_missing(&["c".into()], false);
    assert_eq!(s3.database().unwrap().get("c").unwrap().missing, Some(false));
    s3.replace(ComicDatabase::from_books(vec![cr.clone()]));
    assert_eq!(s3.database().unwrap().get("c").unwrap().missing, Some(false), "relu sans changement");
}

/// Suppr / retrait : seulement les livres sélectionnés **affichés** (un livre sélectionné
/// puis masqué par la recherche ou le filtre n'est jamais visé).
#[test]
fn destructive_actions_target_only_shown_books() {
    let mut s = state(vec![book("a", "/bd/a.cbz"), book("b", "/bd/b.cbz")]);
    assert_eq!(shown(&mut s).len(), 2);
    s.click_book("a", false, false);
    assert_eq!(s.visible_selection(), ["a"]);
    s.set_missing(&["b".into()], true);
    s.missing_only = true;
    assert_eq!(shown(&mut s), ["b"]);
    assert_eq!(s.selection(), ["a"], "repli sur le livre courant (fiche, infos)");
    assert!(s.visible_selection().is_empty(), "« a » est masqué : rien à retirer");
    s.click_book("b", false, false);
    assert_eq!(s.visible_selection(), ["b"]);
}

/// « Retrouver le fichier… » : taille et doublons examinés dans un fil.
#[test]
fn chosen_file_is_examined_in_a_thread() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("Tome 01.cbz");
    std::fs::write(&f, b"12345").unwrap();
    let mut a = book("a", "/ancien/Tome 01.cbz");
    a.file_size = Some(5);
    let s = state(vec![a]);
    let ctx = eframe::egui::Context::default();
    let rx = check_chosen(s.database().unwrap(), "a".into(), f.clone(), &ctx);
    let p = rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap().unwrap();
    assert_eq!((p.kind, p.checked, p.new), (MatchKind::NameAndSize, true, f));
    let rx = check_chosen(s.database().unwrap(), "absent".into(), dir.path().join("x.cbz"), &ctx);
    assert!(rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap().is_none());
}
