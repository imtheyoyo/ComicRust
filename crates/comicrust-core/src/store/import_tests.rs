//! Tests du transfert ComicRack → ComicRust (`store::import`) : SQLite dans un dossier
//! temporaire ; MySQL depuis `tests::mysql_store` (une seule fonction de test touche la
//! base MySQL de test, pour ne pas que deux tests parallèles s'y marchent dessus).

use std::sync::atomic::AtomicBool;

use super::import::{assemble_lists, split_lists};
use super::schema::{LISTS, META};
use super::tests::{full_book, same, simple, temp_cfg};
use super::*;
use crate::comic_lists::{parse_comic_lists, ListKind};
use crate::library::Library;

const B1: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const B2: &str = "aaaaaaaa-0000-0000-0000-000000000002";
const B3: &str = "aaaaaaaa-0000-0000-0000-000000000003";

/// Petit `ComicDb.xml` au format de `XmlSerializer` (ComicRack CE) : livres avec lecture,
/// note, pages ; listes (bibliothèque, dossier avec listes de lecture, liste intelligente
/// avec critères et cache, dossier vide) ; dossiers surveillés.
const COMICDB: &str = r#"<?xml version="1.0"?>
<ComicDatabase xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Id="9a7e0f6c-1111-2222-3333-444455556666">
  <Books>
    <Book Id="aaaaaaaa-0000-0000-0000-000000000001" File="Z:\BD\Thanos\Thanos Quest 01.cbz">
      <Series>Thanos Quest</Series>
      <Number>1</Number>
      <Year>1990</Year>
      <Writer>Jim Starlin</Writer>
      <PageCount>48</PageCount>
      <CurrentPage>10</CurrentPage>
      <LastPageRead>10</LastPageRead>
      <Rating>4.5</Rating>
      <Added>2019-10-01T12:58:00.1234567+02:00</Added>
      <Opened>2026-01-10T20:00:00+01:00</Opened>
      <OpenCount>2</OpenCount>
      <Pages><Page Image="0" Type="FrontCover" /><Page Image="1" /><Page Image="47" Type="BackCover" /></Pages>
    </Book>
    <Book Id="aaaaaaaa-0000-0000-0000-000000000002" File="Z:\BD\Thanos\Thanos Quest 02.cbz" Checked="false">
      <Series>Thanos Quest</Series>
      <Number>2</Number>
      <PageCount>48</PageCount>
      <CurrentPage>20</CurrentPage>
      <LastPageRead>20</LastPageRead>
      <Opened>2026-05-01T10:00:00+02:00</Opened>
      <OpenCount>1</OpenCount>
      <BookNotes>prêté à Océane &amp; Johan</BookNotes>
    </Book>
    <Book Id="aaaaaaaa-0000-0000-0000-000000000003" File="Z:/BD/Warlock/Warlock 09.cbr">
      <Series>Warlock</Series>
      <Number>9</Number>
      <PageCount>32</PageCount>
      <BookOwner>Yoyo</BookOwner>
    </Book>
    <Book Id="" File="Z:\BD\sans id.cbz" />
  </Books>
  <ComicLists>
    <Item xsi:type="ComicLibraryListItem" Id="11111111-0000-0000-0000-00000000000a" Name="Bibliothèque" />
    <Item xsi:type="ComicListItemFolder" Id="11111111-0000-0000-0000-0000000000f1" Name="Lectures &amp; arcs" CombineMode="Or">
      <Items>
        <Item xsi:type="ComicIdListItem" Id="11111111-0000-0000-0000-0000000000b1" Name="Thanos">
          <Description>Ordre de lecture</Description>
          <BookCount>2</BookCount>
          <BookIds>
            <guid>aaaaaaaa-0000-0000-0000-000000000002</guid>
            <guid>aaaaaaaa-0000-0000-0000-000000000001</guid>
          </BookIds>
        </Item>
        <Item xsi:type="ComicListItemFolder" Id="11111111-0000-0000-0000-0000000000f2" Name="Sous-dossier" CombineMode="And">
          <Items>
            <Item xsi:type="ComicIdListItem" Id="11111111-0000-0000-0000-0000000000b2" Name="Warlock"><BookIds><guid>aaaaaaaa-0000-0000-0000-000000000003</guid></BookIds></Item>
          </Items>
        </Item>
      </Items>
    </Item>
    <Item xsi:type="ComicSmartListItem" Id="11111111-0000-0000-0000-0000000000c1" Name="Starlin non lus" MatcherMode="And">
      <Matchers>
        <ComicBookMatcher xsi:type="ComicBookWriterMatcher" MatchOperator="1"><MatchValue>Starlin</MatchValue></ComicBookMatcher>
        <ComicBookMatcher xsi:type="ComicBookGroupMatcher" MatcherMode="Or">
          <Matchers>
            <ComicBookMatcher xsi:type="ComicBookReadPercentageMatcher" MatchOperator="1"><MatchValue>0</MatchValue></ComicBookMatcher>
          </Matchers>
        </ComicBookMatcher>
      </Matchers>
      <Limit>true</Limit>
      <LimitValue>10</LimitValue>
      <CacheStorage>aaaaaaaa-0000-0000-0000-000000000002,aaaaaaaa-0000-0000-0000-000000000001</CacheStorage>
    </Item>
    <Item xsi:type="ComicListItemFolder" Id="11111111-0000-0000-0000-0000000000f3" Name="Vide" />
  </ComicLists>
  <WatchFolders>
    <WatchFolder Folder="Z:\BD" Watch="true" />
    <WatchFolder Folder="\\NAS\Comics &amp; BD" />
  </WatchFolders>
</ComicDatabase>"#;

fn source() -> ComicRackSource {
    ComicRackSource::from_comicdb_xml(COMICDB.to_string()).unwrap()
}

fn one(s: &mut Store, id: &str) -> LibraryBook {
    s.get(&[id.to_string()]).unwrap().pop().unwrap_or_else(|| panic!("livre {id} absent"))
}

#[test]
fn watch_folders_are_read() {
    assert_eq!(read_watch_folders(COMICDB), [PathBuf::from(r"Z:\BD"), PathBuf::from(r"\\NAS\Comics & BD")]);
    assert!(read_watch_folders("<ComicDatabase><Books/></ComicDatabase>").is_empty());
    assert!(read_watch_folders("pas du XML <<<").is_empty());
}

#[test]
fn lists_split_and_reassembled() {
    let mut warnings = Vec::new();
    let rows = split_lists(COMICDB, &mut warnings).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(rows.len(), 7);
    let f2 = rows.iter().find(|r| r.id.ends_with("f2")).unwrap();
    assert_eq!(f2.parent.as_deref(), Some("11111111-0000-0000-0000-0000000000f1"));
    assert_eq!(f2.position, 1);
    assert_eq!(f2.kind, "ComicListItemFolder");
    assert_eq!(f2.name, "Sous-dossier");
    assert!(f2.xml.contains("<Items></Items>"), "enfants retirés : {}", f2.xml);
    assert!(!f2.xml.contains("Warlock"));
    let f1 = rows.iter().find(|r| r.id.ends_with("f1")).unwrap();
    assert_eq!(f1.name, "Lectures & arcs");
    // Ordre inversé : la position fait foi.
    let mut shuffled = rows.clone();
    shuffled.reverse();
    let doc = format!(
        "<ComicDatabase xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><ComicLists>{}</ComicLists></ComicDatabase>",
        assemble_lists(&shuffled)
    );
    assert_eq!(parse_comic_lists(&doc).unwrap(), parse_comic_lists(COMICDB).unwrap());
}

#[test]
fn children_inserted_in_every_folder_form() {
    let kid = r#"<Item xsi:type="ComicIdListItem" Id="k" Name="k"><BookIds><guid>x</guid></BookIds></Item>"#;
    for folder in [
        r#"<Item xsi:type="ComicListItemFolder" Id="f" Name="f"><Items></Items></Item>"#,
        r#"<Item xsi:type="ComicListItemFolder" Id="f" Name="f"><Items /></Item>"#,
        r#"<Item xsi:type="ComicListItemFolder" Id="f" Name="f"><Description>d</Description></Item>"#,
        r#"<Item xsi:type="ComicListItemFolder" Id="f" Name="f" />"#,
    ] {
        let rows = vec![
            import::ListRow {
                id: "f".into(),
                parent: None,
                position: 0,
                name: "f".into(),
                kind: String::new(),
                xml: folder.into(),
            },
            import::ListRow {
                id: "k".into(),
                parent: Some("F".into()),
                position: 0,
                name: "k".into(),
                kind: String::new(),
                xml: kid.into(),
            },
        ];
        let doc = format!(
            "<ComicDatabase xmlns:xsi=\"x\"><ComicLists>{}</ComicLists></ComicDatabase>",
            assemble_lists(&rows)
        );
        let lists = parse_comic_lists(&doc).unwrap_or_else(|e| panic!("{e}\n{doc}"));
        let ListKind::Folder { children, .. } = &lists[0].kind else { panic!("{doc}") };
        assert_eq!(children.len(), 1, "{doc}");
        assert_eq!(children[0].kind, ListKind::Ids(vec!["x".into()]));
    }
}

/// Scénario complet (SQLite et MySQL).
pub(super) fn import_scenario(cfg: &StoreConfig) {
    let mut s = Store::open(cfg).unwrap();
    // Avant le transfert : un livre ajouté par une analyse de ComicRust (autre id, même
    // fichier que B3 à la casse et aux séparateurs près) avec une lecture, et un livre
    // propre à ComicRust.
    let mut scanned = simple(1);
    scanned.id = "cccccccc-0000-0000-0000-000000000001".into();
    scanned.file = Some(r"z:\bd\WARLOCK\warlock 09.cbr".into());
    scanned.current_page = Some(5);
    scanned.last_page_read = Some(5);
    scanned.opened = Some("2026-09-01T18:00:00+02:00".into());
    scanned.open_count = Some(1);
    let own = simple(2);
    s.upsert(&[scanned.clone(), own.clone()]).unwrap();
    s.set_folders(&[PathBuf::from("z:/bd/")]).unwrap();

    let src = source();
    let progress = LoadProgress::default();
    let r = s.import_comicrack(&src, &ImportOptions::default(), Some(&progress), None).unwrap();
    assert_eq!((r.books_added, r.books_replaced, r.books_updated, r.books_skipped), (2, 1, 0, 1), "{r:?}");
    assert_eq!(r.books_written(), 3);
    assert_eq!(r.lists, 7);
    assert_eq!(r.warnings.len(), 1, "{r:?}");
    assert!(!r.cancelled);
    assert_eq!(progress.get(), (3, 3));
    // `Z:\BD` existe déjà (« z:/bd/ ») : seul le dossier du NAS est ajouté.
    assert_eq!(r.folders, [PathBuf::from(r"\\NAS\Comics & BD")]);
    assert_eq!(s.folders().unwrap().len(), 2);

    // Livres : tous les champs de ComicRack, avec leur identifiant.
    assert_eq!(s.count().unwrap(), 4);
    assert!(s.get(&[scanned.id.clone()]).unwrap().is_empty(), "livre de l'analyse remplacé");
    same(&one(&mut s, &own.id), &own);
    let lib = Library::from_xml(COMICDB).unwrap();
    same(&one(&mut s, B1), &lib.books.book[0]);
    same(&one(&mut s, B2), &lib.books.book[1]);
    assert_eq!(one(&mut s, B2).checked, Some(false));
    assert_eq!(one(&mut s, B1).pages.page.len(), 3);
    // B3 : fiche ComicRack + lecture de l'analyse (plus récente).
    let b3 = one(&mut s, B3);
    assert_eq!(b3.book_owner.as_deref(), Some("Yoyo"));
    assert_eq!(b3.file.as_deref(), Some("Z:/BD/Warlock/Warlock 09.cbr"));
    assert_eq!((b3.current_page, b3.last_page_read, b3.open_count), (Some(5), Some(5), Some(1)));
    assert_eq!(b3.opened.as_deref(), Some("2026-09-01T18:00:00+02:00"));

    // Listes : identiques à celles du ComicDb.xml d'origine.
    let expected = parse_comic_lists(COMICDB).unwrap();
    assert_eq!(s.comic_lists().unwrap(), expected);

    // Lectures faites dans ComicRust après le transfert ; une liste créée dans ComicRust.
    let seen = s.load(None).unwrap();
    let mut watcher = StoreWatcher::new(cfg.clone(), &seen);
    s.set_progress(B1, 30, 30, "2026-10-20T21:00:00+02:00").unwrap();
    // B2 : lecture plus ancienne que celle de ComicRack mais plus avancée.
    let mut b2 = one(&mut s, B2);
    b2.current_page = Some(40);
    b2.last_page_read = Some(40);
    b2.opened = Some("2025-01-01T10:00:00Z".into());
    s.upsert(&[b2]).unwrap();
    s.conn
        .exec(
            &format!("INSERT INTO {LISTS} (`id`, `parent`, `position`, `name`, `kind`, `xml`) VALUES (?,?,?,?,?,?)"),
            &[
                Val::Text("22222222-0000-0000-0000-000000000001".into()),
                Val::Text("11111111-0000-0000-0000-0000000000f1".into()),
                Val::Int(0),
                Val::Text("Perso".into()),
                Val::Text("ComicIdListItem".into()),
                Val::Text(r#"<Item xsi:type="ComicIdListItem" Id="22222222-0000-0000-0000-000000000001" Name="Perso"><BookIds /></Item>"#.into()),
            ],
        )
        .unwrap();
    watcher.poll().unwrap();
    let lists_before = schema::meta_int(&mut s.conn, "lists_counter").unwrap();

    // Second transfert.
    let r = s.import_comicrack(&src, &ImportOptions::default(), None, None).unwrap();
    assert_eq!((r.books_added, r.books_replaced, r.books_updated), (0, 0, 3), "{r:?}");
    assert!(r.folders.is_empty());
    assert_eq!(s.count().unwrap(), 4);
    let b1 = one(&mut s, B1);
    assert_eq!((b1.current_page, b1.last_page_read, b1.open_count), (Some(30), Some(30), Some(3)));
    assert_eq!(b1.opened.as_deref(), Some("2026-10-20T21:00:00+02:00"));
    assert_eq!(b1.rating, Some(4.5));
    let b2 = one(&mut s, B2);
    assert_eq!((b2.current_page, b2.last_page_read), (Some(20), Some(40)), "page de la lecture la plus récente");
    assert_eq!(b2.opened.as_deref(), Some("2026-05-01T10:00:00+02:00"));
    assert_eq!(b2.book_notes.as_deref(), Some("prêté à Océane & Johan"));
    assert_eq!(one(&mut s, B3).current_page, Some(5), "lecture de l'analyse toujours gardée");
    // Liste propre à ComicRust gardée, après celles de ComicRack dans son dossier.
    let lists = s.comic_lists().unwrap();
    let ListKind::Folder { children, .. } = &lists[1].kind else { panic!("{lists:?}") };
    assert_eq!(children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Thanos", "Sous-dossier", "Perso"]);
    assert_eq!(lists.len(), expected.len());
    assert_eq!(schema::meta_int(&mut s.conn, "lists_counter").unwrap(), lists_before + 1);
    // Un autre ComicRust voit les fiches réécrites.
    let ch = watcher.poll().unwrap().expect("modifications vues");
    assert_eq!(ch.updated.len(), 3);

    // Annulation avant le premier lot : rien n'est écrit.
    let counters = s.counters().unwrap();
    let cancel = AtomicBool::new(true);
    let r = s.import_comicrack(&src, &ImportOptions::default(), None, Some(&cancel)).unwrap();
    assert!(r.cancelled);
    assert_eq!(r.books_written(), 0);
    assert_eq!(s.counters().unwrap(), counters);

    // Sans ComicDb.xml ni options : livres seulement.
    let only_books = ComicRackSource { books: src.books.clone(), comicdb_xml: None };
    let r = s.import_comicrack(&only_books, &ImportOptions { folders: false, lists: false }, None, None).unwrap();
    assert_eq!(r.books_updated, 3);
    assert!(r.warnings.iter().all(|w| !w.contains("ComicDb.xml")), "{r:?}");
}

#[test]
fn sqlite_import_scenario() {
    let (_dir, cfg) = temp_cfg();
    import_scenario(&cfg);
}

/// `ComicDb.xml` de `n` livres complets, avec une liste de lecture de tous les livres.
fn big_comicdb(n: usize) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\"?>\n<ComicDatabase xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n<Books>\n",
    );
    for i in 0..n {
        let mut b = full_book(&format!("eeeeeeee-0000-0000-0000-{i:012}"));
        b.file = Some(format!(r"Z:\BD\Grosse bibliothèque\Série {}\Tome {i:05}.cbz", i / 50));
        b.summary = Some("Un résumé de taille raisonnable pour une BD.".into());
        xml.push_str(&quick_xml::se::to_string_with_root("Book", &b).unwrap());
        xml.push('\n');
    }
    xml.push_str("</Books>\n<ComicLists>\n<Item xsi:type=\"ComicIdListItem\" Id=\"big\" Name=\"Tout\"><BookIds>");
    for i in 0..n {
        xml.push_str(&format!("<guid>eeeeeeee-0000-0000-0000-{i:012}</guid>"));
    }
    xml.push_str("</BookIds></Item>\n</ComicLists>\n</ComicDatabase>");
    xml
}

/// 5 000 livres : transfert initial puis relancé (fusion des lectures). Renvoie les durées.
pub(super) fn import_5000(cfg: &StoreConfig) -> (std::time::Duration, std::time::Duration) {
    let src = ComicRackSource::from_comicdb_xml(big_comicdb(5000)).unwrap();
    assert_eq!(src.books.len(), 5000);
    let mut s = Store::open(cfg).unwrap();
    let t = Instant::now();
    let r = s.import_comicrack(&src, &ImportOptions::default(), None, None).unwrap();
    let first = t.elapsed();
    assert_eq!(r.books_added, 5000, "{:?}", r.warnings);
    let t = Instant::now();
    let r = s.import_comicrack(&src, &ImportOptions::default(), None, None).unwrap();
    let second = t.elapsed();
    assert_eq!(r.books_updated, 5000);
    assert_eq!(s.count().unwrap(), 5000);
    same(&one(&mut s, &src.books[4321].id), &src.books[4321]);
    let lists = s.comic_lists().unwrap();
    let ListKind::Ids(ids) = &lists[0].kind else { panic!() };
    assert_eq!(ids.len(), 5000);
    (first, second)
}

#[test]
fn sqlite_import_5000_books() {
    let (_dir, cfg) = temp_cfg();
    let (first, second) = import_5000(&cfg);
    eprintln!("transfert de 5 000 livres SQLite : {first:?}, relancé : {second:?}");
    // Large marge pour une CI lente (en local : quelques secondes au plus en debug).
    assert!(first.as_secs() < 20 && second.as_secs() < 20, "trop lent : {first:?} / {second:?}");
}

#[test]
fn sqlite_schema_2_is_migrated() {
    let (_dir, cfg) = temp_cfg();
    let mut s = Store::open(&cfg).unwrap();
    s.upsert(&[simple(1)]).unwrap();
    drop(s);
    // Base v2 : ni table des listes ni `lists_counter`.
    let path = cfg.sqlite_path().unwrap().unwrap();
    let mut c = Conn::open_sqlite(&path).unwrap();
    c.batch(&format!("DROP TABLE {LISTS}")).unwrap();
    c.batch(&format!("DELETE FROM {META} WHERE `key` = 'lists_counter'")).unwrap();
    c.batch(&format!("UPDATE {META} SET `value` = '2' WHERE `key` = 'schema_version'")).unwrap();
    drop(c);
    let mut s = Store::open_existing(&cfg).unwrap();
    assert_eq!(schema::meta_int(&mut s.conn, "schema_version").unwrap(), 3);
    assert_eq!(SCHEMA_VERSION, 3);
    assert!(s.comic_lists().unwrap().is_empty());
    assert_eq!(s.count().unwrap(), 1);
    let r = s.import_comicrack(&source(), &ImportOptions::default(), None, None).unwrap();
    assert_eq!(r.lists, 7);
    assert_eq!(schema::meta_int(&mut s.conn, "lists_counter").unwrap(), 1);
}

/// Partie MySQL, appelée par `tests::mysql_store` (tables recréées à chaque étape).
pub(super) fn mysql_import(db_cfg: &DbConfig) {
    let cfg = StoreConfig::MySql(db_cfg.clone());
    super::tests::drop_tables(db_cfg);
    import_scenario(&cfg);
    super::tests::drop_tables(db_cfg);
    let (first, second) = import_5000(&cfg);
    eprintln!("transfert de 5 000 livres MySQL : {first:?}, relancé : {second:?}");
    // Migration 2 → 3.
    let mut c = Conn::open_mysql(db_cfg).unwrap();
    c.batch(&format!("DROP TABLE {LISTS}")).unwrap();
    c.batch(&format!("UPDATE {META} SET `value` = '2' WHERE `key` = 'schema_version'")).unwrap();
    drop(c);
    let mut s = Store::open_existing(&cfg).unwrap();
    assert_eq!(schema::meta_int(&mut s.conn, "schema_version").unwrap(), 3);
    assert!(s.comic_lists().unwrap().is_empty());
    assert_eq!(s.count().unwrap(), 5000);
    drop(s);
    super::tests::drop_tables(db_cfg);
}
