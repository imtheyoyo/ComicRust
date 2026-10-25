//! Tests de l'écriture dans la base ComicRack : modification des fiches (sans base) et
//! écriture MySQL (seulement si `COMICRUST_TEST_MYSQL` est définie, comme
//! `db::tests::mysql_server`).

use chrono::{DateTime, FixedOffset, TimeZone};
use mysql::prelude::Queryable;

use super::*;
use crate::db::tests::{mysql_lock, SAMPLE};

/// Fiche de la base de démo `comickrack_demo` (ordre non canonique, ancien format avec
/// déclaration XML et espace de noms).
const DEMO: &str = "<?xml version=\"1.0\"?>\n<ComicBook xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" Id=\"67ac7b06-e3b3-4b52-ad7e-4a599f9288fd\" File=\"/home/claude/comicrust/recette/echantillons/Thanos Quest 01 (1990).cbz\"><Series>Thanos Quest</Series><Number>1</Number><Title>Le gant</Title><Writer>Jim Starlin</Writer><PageCount>11</PageCount><Summary>Thanos part en quête des gemmes de l'infini pour séduire la Mort.</Summary><Characters>Thanos, La Mort, Le Collectionneur</Characters><PreferredFrontCover>0</PreferredFrontCover><Pages><Page Image=\"0\" Type=\"Other\" /><Page Image=\"1\" Type=\"FrontCover\" /></Pages><Publisher>Marvel</Publisher><Year>1990</Year><Month>10</Month><SeriesComplete>Yes</SeriesComplete><Penciller>Ron Lim</Penciller><StoryArc>La quête de Thanos</StoryArc><Rating>4.5</Rating><CurrentPage>4</CurrentPage><LastPageRead>4</LastPageRead></ComicBook>";
const DEMO_ID: &str = "67ac7b06-e3b3-4b52-ad7e-4a599f9288fd";

/// Fiche au format de ComicRack CE (`XmlUtility.ToString` : ni déclaration, ni espace
/// de noms, ni indentation), avec des éléments que ComicRust ne modélise pas.
const CE: &str = r#"<ComicBook Id="5f0c1a2b-0000-4000-8000-000000000001" File="Z:\BD\Batman &amp; Robin 01.cbr"><Series>Batman &amp; Robin</Series><Number>1</Number><Year>2009</Year><PageCount>3</PageCount><Tags>dc</Tags><MyExtra a="1">inconnu</MyExtra><Pages><Page Image="0" ImageWidth="1000" ImageHeight="1500" Type="FrontCover" /><Page Image="1" /><Page Image="2" /></Pages><Added>2020-01-05T10:00:00.5+01:00</Added><Opened>2024-03-01T20:15:42.1234567+01:00</Opened><OpenCount>3</OpenCount><CurrentPage>1</CurrentPage><LastPageRead>2</LastPageRead><Rating>4</Rating><ColorAdjustment><Saturation>0.2</Saturation></ColorAdjustment><ComicInfoIsDirty>true</ComicInfoIsDirty><FileSize>1234</FileSize><FileModifiedTime>2019-12-31T23:00:00+01:00</FileModifiedTime><CustomValuesStore>a=1</CustomValuesStore></ComicBook>"#;
const CE_ID: &str = "5f0c1a2b-0000-4000-8000-000000000001";

fn date(s: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(s).unwrap()
}

fn patch(xml: &str, edit: BookEdit) -> String {
    patch_comic_book_xml(xml, &edit.changes).unwrap()
}

fn edit() -> BookEdit {
    BookEdit::new("x")
}

#[test]
fn dates_like_dotnet() {
    let tz = FixedOffset::east_opt(2 * 3600).unwrap();
    let d = tz.with_ymd_and_hms(2026, 10, 24, 21, 5, 0).unwrap();
    assert_eq!(net_datetime(&d), "2026-10-24T21:05:00+02:00");
    let d = d + chrono::Duration::nanoseconds(123_456_700);
    assert_eq!(net_datetime(&d), "2026-10-24T21:05:00.1234567+02:00");
    assert_eq!(net_datetime(&date("2026-01-02T03:04:05.5-05:00")), "2026-01-02T03:04:05.5-05:00");
    // Précision de .NET : le tick (100 ns).
    assert_eq!(net_datetime(&date("2026-01-02T03:04:05.000000099Z")), "2026-01-02T03:04:05+00:00");
    let utc = |s: &str| date(s).with_timezone(&chrono::Utc);
    assert_eq!(net_datetime_utc(&utc("2026-10-24T21:05:00.25+02:00")), "2026-10-24T19:05:00.25Z");
    assert_eq!(net_datetime_utc(&utc("2026-01-02T03:04:05Z")), "2026-01-02T03:04:05Z");
}

#[test]
fn nothing_changes_byte_for_byte() {
    for xml in [DEMO, CE, SAMPLE] {
        assert_eq!(patch_comic_book_xml(xml, &[]).unwrap(), xml);
        LibraryBook::from_xml(xml).unwrap();
    }
    // Valeurs déjà en place : fiche identique.
    assert_eq!(patch(DEMO, edit().set("CurrentPage", Value::Int(4)).set("LastPageRead", Value::Max(2))), DEMO);
    assert_eq!(patch(DEMO, edit().rating(4.5).set("SeriesComplete", Value::Text("Yes".into()))), DEMO);
    assert_eq!(patch(CE, edit().remove("Missing").comic_info_dirty(true)), CE, "élément absent supprimé : rien");
}

#[test]
fn replace_value() {
    let out = patch(SAMPLE, edit().set("CurrentPage", Value::Int(30)));
    assert_eq!(out, SAMPLE.replace("<CurrentPage>12</CurrentPage>", "<CurrentPage>30</CurrentPage>"));
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!((b.current_page, b.last_page_read), (Some(30), Some(12)));
}

#[test]
fn reading_progress_and_opening() {
    let at = date("2026-10-24T21:05:00.1234567+02:00");
    // Fiche indentée : insertion à sa place, avec la même indentation.
    let out = patch(SAMPLE, edit().progress(20).opened(at));
    let expected = SAMPLE
        .replace("<CurrentPage>12</CurrentPage>", "<CurrentPage>20</CurrentPage>")
        .replace("<LastPageRead>12</LastPageRead>", "<LastPageRead>20</LastPageRead>")
        .replace(
            "  <FileSize>",
            "  <Opened>2026-10-24T21:05:00.1234567+02:00</Opened>\n  <OpenCount>1</OpenCount>\n  <FileSize>",
        );
    assert_eq!(out, expected);
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!((b.current_page, b.last_page_read, b.open_count), (Some(20), Some(20), Some(1)));
    assert_eq!(b.opened.as_deref(), Some("2026-10-24T21:05:00.1234567+02:00"));

    // Retour en arrière : LastPageRead garde le maximum ; OpenCount s'ajoute à l'existant.
    let out = patch(CE, edit().progress(0).opened(at));
    let expected = CE
        .replace("<CurrentPage>1</CurrentPage>", "")
        .replace("<OpenCount>3</OpenCount>", "<OpenCount>4</OpenCount>")
        .replace("2024-03-01T20:15:42.1234567+01:00", "2026-10-24T21:05:00.1234567+02:00");
    assert_eq!(out, expected, "CurrentPage = 0 : valeur par défaut, élément supprimé");

    // Fiche de la démo (ordre non canonique) : insertion devant le premier élément qui
    // suit dans l'ordre de XmlSerializer (ici SeriesComplete).
    let out = patch(DEMO, edit().opened(at));
    assert_eq!(
        out,
        DEMO.replace(
            "<SeriesComplete>",
            "<Opened>2026-10-24T21:05:00.1234567+02:00</Opened><OpenCount>1</OpenCount><SeriesComplete>"
        )
    );
}

#[test]
fn removal_and_defaults() {
    // Suppression avec l'indentation qui précède.
    let out = patch(SAMPLE, edit().remove("CurrentPage"));
    assert_eq!(out, SAMPLE.replace("\n  <CurrentPage>12</CurrentPage>", ""));
    // Valeurs par défaut (`[DefaultValue]`) : éléments supprimés.
    let out = patch(CE, edit().comic_info_dirty(false).rating(0.0).set("FileSize", Value::Int(-1)));
    let expected = CE
        .replace("<ComicInfoIsDirty>true</ComicInfoIsDirty>", "")
        .replace("<Rating>4</Rating>", "")
        .replace("<FileSize>1234</FileSize>", "");
    assert_eq!(out, expected);
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!((b.comic_info_is_dirty, b.rating, b.file_size), (None, None, None));
    // Chaîne vide : supprimée (défaut "") ; sauf défaut null : `<X />`.
    let out = patch(CE, edit().set("Series", Value::Text(String::new())));
    assert_eq!(out, CE.replace("<Series>Batman &amp; Robin</Series>", ""));
    let out = patch(CE, edit().set("CustomThumbnailKey", Value::Text(String::new())));
    assert_eq!(out, CE.replace("</FileModifiedTime>", "</FileModifiedTime><CustomThumbnailKey />"));
    // Booléens, réels, énumérations, GUID.
    let out = patch(
        CE,
        edit()
            .rating(7.0)
            .set("EnableProposed", Value::Bool(false))
            .set("BookPrice", Value::Float(12.5))
            .set("Manga", Value::Text("YesAndRightToLeft".into()))
            .set("LastOpenedFromListId", Value::Text("8E9B3C1A-0000-4000-8000-0000000000AA".into())),
    );
    assert!(out.contains("<Rating>5</Rating>"), "note bornée à 5 comme ComicRack");
    assert!(out.contains("<EnableProposed>false</EnableProposed>"));
    assert!(out.contains("<BookPrice>12.5</BookPrice>"));
    assert!(out.contains("<Manga>YesAndRightToLeft</Manga><Tags>"));
    assert!(out.contains(
        "<EnableProposed>false</EnableProposed><LastOpenedFromListId>8e9b3c1a-0000-4000-8000-0000000000aa</LastOpenedFromListId><ComicInfoIsDirty>"
    ));
}

#[test]
fn insertion_follows_xmlserializer_order() {
    let out = patch(
        CE,
        edit()
            .set("Released", Value::Date(date("2009-06-01T00:00:00+02:00")))
            .set("Missing", Value::Bool(true))
            .set("NewPages", Value::Int(2))
            .set("Genre", Value::Text("Super-héros".into()))
            .set("Teams", Value::Text("Batman Inc.".into()))
            .set("Title", Value::Text("Batman Reborn".into()))
            .set("Checked", Value::Bool(false))
            .set("IsDynamicSource", Value::Bool(true)),
    );
    let expected = CE
        .replace(r#"" File="#, r#"" Checked="false" File="#)
        .replace(r#".cbr">"#, r#".cbr" IsDynamicSource="true">"#)
        .replace("<Series>", "<Title>Batman Reborn</Title><Series>")
        .replace("<PageCount>", "<Genre>Super-héros</Genre><PageCount>")
        .replace("<Tags>", "<Teams>Batman Inc.</Teams><Tags>")
        .replace("<Opened>", "<Released>2009-06-01T00:00:00+02:00</Released><Opened>")
        .replace("<FileModifiedTime>", "<Missing>true</Missing><FileModifiedTime>")
        .replace("<CustomValuesStore>", "<NewPages>2</NewPages><CustomValuesStore>");
    assert_eq!(out, expected);
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!((b.checked, b.missing, b.new_pages), (Some(false), Some(true), Some(2)));
    assert_eq!(b.title.as_deref(), Some("Batman Reborn"));

    // Éléments inconnus (`UnparsedElements`) : entre Tags et Pages.
    let xml = r#"<ComicBook Id="a"><Series>S</Series><MyExtra>1</MyExtra><Added>2020-01-01T00:00:00+01:00</Added></ComicBook>"#;
    let out = patch(xml, edit().set("Tags", Value::Text("t".into())).set("Pages", Value::Pages(vec![])));
    assert_eq!(
        out,
        r#"<ComicBook Id="a"><Series>S</Series><Tags>t</Tags><MyExtra>1</MyExtra><Pages /><Added>2020-01-01T00:00:00+01:00</Added></ComicBook>"#
    );
    // Fiche sans enfant, et racine vide `<ComicBook … />`.
    let out = patch(r#"<ComicBook Id="a"></ComicBook>"#, edit().progress(3));
    assert_eq!(out, r#"<ComicBook Id="a"><CurrentPage>3</CurrentPage><LastPageRead>3</LastPageRead></ComicBook>"#);
    let out = patch(r#"<ComicBook Id="a" File="f" />"#, edit().progress(3));
    assert_eq!(
        out,
        r#"<ComicBook Id="a" File="f"><CurrentPage>3</CurrentPage><LastPageRead>3</LastPageRead></ComicBook>"#
    );
    // Attribut ajouté à la fin, remplacé, supprimé.
    let out = patch(r#"<ComicBook Id="a"/>"#, edit().set("File", Value::Text("f.cbz".into())));
    assert_eq!(out, r#"<ComicBook Id="a" File="f.cbz"/>"#);
    let out =
        patch(r#"<ComicBook Id="a" File='g.cbr' Checked="false"/>"#, edit().set("FilePath", Value::Text("f".into())));
    assert_eq!(out, r#"<ComicBook Id="a" File="f" Checked="false"/>"#);
    let out = patch(r#"<ComicBook Id="a" Checked="false" File="f"/>"#, edit().set("Checked", Value::Bool(true)));
    assert_eq!(out, r#"<ComicBook Id="a" File="f"/>"#);
}

#[test]
fn special_characters_are_escaped() {
    let out = patch(
        CE,
        edit()
            .set("Series", Value::Text("Tom & Jerry <1> \"l'été\"".into()))
            .set("File", Value::Text(r#"Z:\BD\"A" & <B>.cbz"#.into()))
            .set("Summary", Value::Text("Ligne 1\nLigne 2 ✓ 🦸".into())),
    );
    assert!(out.contains("<Series>Tom &amp; Jerry &lt;1&gt; \"l'été\"</Series>"));
    assert!(out.contains(r#"File="Z:\BD\&quot;A&quot; &amp; &lt;B&gt;.cbz""#));
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!(b.series.as_deref(), Some("Tom & Jerry <1> \"l'été\""));
    assert_eq!(b.file.as_deref(), Some(r#"Z:\BD\"A" & <B>.cbz"#));
    assert_eq!(b.summary.as_deref(), Some("Ligne 1\nLigne 2 ✓ 🦸"));
    // Seuls ces éléments ont changé.
    let back = out
        .replace("Tom &amp; Jerry &lt;1&gt; \"l'été\"", "Batman &amp; Robin")
        .replace(r#"Z:\BD\&quot;A&quot; &amp; &lt;B&gt;.cbz"#, r#"Z:\BD\Batman &amp; Robin 01.cbr"#)
        .replace("<Summary>Ligne 1\nLigne 2 ✓ 🦸</Summary>", "");
    assert_eq!(back, CE);
}

fn page(image: i32, kind: Option<&str>) -> PageInfo {
    PageInfo { image, kind: kind.map(str::to_string), ..Default::default() }
}

#[test]
fn pages_replaced_whole() {
    let mut cover = page(0, Some("FrontCover"));
    cover.image_width = Some("1988".into());
    cover.image_height = Some("3056".into());
    cover.image_size = Some("0".into()); // défaut : omis
    let mut marked = page(1, Some("Story"));
    marked.bookmark = Some(r#"Chapitre "1" <&>"#.into());
    marked.rotation = Some("Rotate90".into());
    marked.page_position = Some("Default".into());
    let mut last = page(2, Some("BackCover, Other"));
    last.key = Some("k1".into());
    let pages = vec![cover, marked, last];
    let rendered = r#"<Pages><Page Image="0" ImageWidth="1988" ImageHeight="3056" Type="FrontCover" /><Page Image="1" Bookmark="Chapitre &quot;1&quot; &lt;&amp;&gt;" Rotation="Rotate90" /><Page Image="2" Key="k1" Type="BackCover, Other" /></Pages>"#;
    let out = patch(DEMO, edit().set("Pages", Value::Pages(pages.clone())));
    let old = r#"<Pages><Page Image="0" Type="Other" /><Page Image="1" Type="FrontCover" /></Pages>"#;
    assert_eq!(out, DEMO.replace(old, rendered));
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!(b.pages.page.len(), 3);
    assert_eq!(b.pages.page[1].bookmark.as_deref(), Some(r#"Chapitre "1" <&>"#));
    assert_eq!(b.pages.page[1].kind, None, "Story : défaut omis");
    assert_eq!(b.pages.page[0].image_width.as_deref(), Some("1988"));
    assert_eq!(b.front_cover_image(), 0);

    // Fiche indentée : les <Page> le sont d'un cran de plus.
    let out = patch(SAMPLE, edit().set("Pages", Value::Pages(pages[..2].to_vec())));
    assert!(out.contains(
        "  <Pages>\n    <Page Image=\"0\" ImageWidth=\"1988\" ImageHeight=\"3056\" Type=\"FrontCover\" />\n    <Page Image=\"1\" Bookmark="
    ));
    assert!(out.contains("Rotation=\"Rotate90\" />\n  </Pages>\n</ComicBook>"));
    // Liste vide : `<Pages />` (collection vide de XmlSerializer) ; suppression possible.
    assert_eq!(patch(DEMO, edit().set("Pages", Value::Pages(vec![]))), DEMO.replace(old, "<Pages />"));
    assert_eq!(patch(DEMO, edit().remove("Pages")), DEMO.replace(old, ""));
}

#[test]
fn file_after_conversion() {
    let f = FileState {
        path: "/bd/Thanos Quest 01 (1990).cbz".into(),
        size: 48_173_972,
        modified: Some(date("2026-10-24T21:05:00.25+02:00")),
        created: None,
        page_count: Some(11),
        pages: None,
    };
    let out = patch(DEMO, edit().file(&f).comic_info_dirty(false));
    let expected = DEMO
        .replace(
            "/home/claude/comicrust/recette/echantillons/Thanos Quest 01 (1990).cbz",
            "/bd/Thanos Quest 01 (1990).cbz",
        )
        .replace(
            "</LastPageRead>",
            "</LastPageRead><FileSize>48173972</FileSize><FileModifiedTime>2026-10-24T19:05:00.25Z</FileModifiedTime>",
        );
    assert_eq!(out, expected);
    let b = LibraryBook::from_xml(&out).unwrap();
    assert_eq!(b.file_size, Some(48_173_972));
    assert_eq!(b.file_modified_time.as_deref(), Some("2026-10-24T19:05:00.25Z"), "UTC, comme ComicRack");
}

#[test]
fn refused_changes() {
    let err = |c: Change| patch_comic_book_xml(CE, &[c]).unwrap_err().to_string();
    assert!(err(Change::set("Serie", Value::Text("x".into()))).contains("inconnu"));
    assert!(err(Change::set("Id", Value::Text("x".into()))).contains("identifiant"));
    assert!(err(Change::set("ColorAdjustment", Value::Text("x".into()))).contains("pas modifiable"));
    assert!(err(Change::set("Rating", Value::Text("5".into()))).contains("texte"));
    assert!(err(Change::set("Opened", Value::Text("hier".into()))).contains("texte"));
    assert!(err(Change::set("Series", Value::Text("a\u{1}b".into()))).contains("U+0001"));
    assert!(err(Change::set("SeriesComplete", Value::Text("Peut-être".into()))).contains("inconnue"));
    assert!(err(Change::set("Rating", Value::Float(f32::NAN))).contains("invalide"));
    assert!(err(Change::set("CurrentPage", Value::Int(1 << 40))).contains("hors limites"));
    assert!(err(Change::set("LastOpenedFromListId", Value::Text("pas un guid".into()))).contains("GUID"));
    let mut p = page(0, None);
    p.rotation = Some("Rotate45".into());
    assert!(err(Change::set("Pages", Value::Pages(vec![p]))).contains("Rotation"));
    assert!(err(Change::set("Pages", Value::Pages(vec![page(0, Some("Couverture"))]))).contains("type inconnu"));
    // Fiches illisibles.
    for bad in ["<ComicBook Id=\"a\"><Series>x</ComicBook>", "<Book Id=\"a\"/>", "pas du xml", ""] {
        assert!(patch_comic_book_xml(bad, &[Change::set("CurrentPage", Value::Int(1))]).is_err(), "{bad}");
    }
    // Valeur entière actuelle illisible : Add refusé.
    let xml = r#"<ComicBook Id="a"><OpenCount>beaucoup</OpenCount></ComicBook>"#;
    assert!(patch_comic_book_xml(xml, &[Change::set("OpenCount", Value::Add(1))]).is_err());
}

// ---------------------------------------------------------------------------------------
// MySQL
// ---------------------------------------------------------------------------------------

fn counters(conn: &mut Conn) -> (i64, i64) {
    conn.query_first("SELECT update_counter, delete_counter FROM changes").unwrap().unwrap()
}

fn row(conn: &mut Conn, id: &str) -> (i64, String) {
    conn.exec_first("SELECT update_counter, data FROM comics WHERE id = ?", (id,)).unwrap().unwrap()
}

/// Écriture d'un livre exactement comme `ComicStorageBaseSql.Write` (hors transaction).
fn comicrack_write(conn: &mut Conn, id: &str, data: &str) {
    let n: i64 = conn.query_first("Select update_counter from changes").unwrap().unwrap();
    conn.exec_drop("Update comics set data=?, update_counter=? where id=?", (data, n + 1, id)).unwrap();
    let n: i64 = conn.query_first("Select update_counter from changes").unwrap().unwrap();
    conn.exec_drop("Update changes set update_counter=? where id=?", (n + 1, STORAGE_GUID)).unwrap();
}

#[test]
fn mysql_write() {
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test d'écriture MySQL ignoré");
        return;
    };
    let _lock = mysql_lock();
    let cfg = DbConfig::from_connection_string(&url).unwrap();
    let mut conn = connect(&cfg).unwrap();
    conn.query_drop("DROP TABLE IF EXISTS comics").unwrap();
    conn.query_drop("DROP TABLE IF EXISTS changes").unwrap();
    // Tables identiques à ComicStorageMySql.CreateTables.
    conn.query_drop("Create table changes (id varchar(40) not null primary key, update_counter bigint default 0, delete_counter bigint default 0)").unwrap();
    conn.query_drop(
        "Create table comics (id varchar(40) not null primary key, update_counter bigint default 0, data mediumtext)",
    )
    .unwrap();
    conn.exec_drop("Insert into changes values (?, 10, 0)", (STORAGE_GUID,)).unwrap();
    for (id, counter, data) in
        [(DEMO_ID, 5, DEMO), (CE_ID, 10, CE), ("30815f16-abb9-4c38-8c24-ecae11e25b1b", 7, SAMPLE)]
    {
        conn.exec_drop("Insert into comics values (?, ?, ?)", (id, counter, data)).unwrap();
    }

    // Deux « instances » : ComicRust lui-même et un autre lecteur de la base.
    let mut mine = ComicDatabase::load(&cfg, None).unwrap();
    let mut w_mine = Watcher::new(cfg.clone(), &mine);
    let mut other = ComicDatabase::load(&cfg, None).unwrap();
    let mut w_other = Watcher::new(cfg.clone(), &other);

    // Rien n'est écrit sans appel explicite.
    assert_eq!(counters(&mut conn), (10, 0));
    assert!(update_books(&cfg, &[]).unwrap().written.is_empty());

    // Lecture : page, date d'ouverture, compteur d'ouvertures ; livre absent signalé.
    let at = date("2026-10-24T21:05:00.1234567+02:00");
    let reading = BookEdit::new(&DEMO_ID.to_uppercase()).progress(7).opened(at);
    let absent = "deadbeef-0000-0000-0000-000000000000";
    let report = update_books(&cfg, &[reading.clone(), BookEdit::new(absent).progress(1)]).unwrap();
    assert_eq!(report.missing, [absent]);
    assert!(report.conflicts.is_empty() && report.failed.is_empty() && report.unchanged.is_empty());
    assert!(!report.is_complete());
    assert!(report.summary().contains("1 livre(s) mis à jour"), "{}", report.summary());
    let w = &report.written[0];
    assert_eq!((w.id.as_str(), w.record.counter, w.retries), (DEMO_ID, 11, 0));
    assert_eq!(w.record.data, patch_comic_book_xml(DEMO, &reading.changes).unwrap());
    assert_eq!((w.book.current_page, w.book.last_page_read, w.book.open_count), (Some(7), Some(7), Some(1)));
    assert_eq!(counters(&mut conn), (11, 0), "compteur global incrémenté comme ComicRack");
    assert_eq!(row(&mut conn, DEMO_ID), (11, w.record.data.clone()));
    let n: i64 = conn.query_first("SELECT COUNT(*) FROM changes").unwrap().unwrap();
    assert_eq!(n, 1, "pas de ligne propre à ComicRust dans changes");

    // L'autre instance voit la modification ; ComicRust ne la reprend pas pour une
    // modification extérieure.
    let ch = w_other.poll().unwrap().expect("modification vue par l'autre instance");
    assert_eq!(ch.updated.len(), 1);
    other.apply(ch);
    assert_eq!(other.get(DEMO_ID).unwrap().current_page, Some(7));
    w_mine.note_report(&report);
    assert!(w_mine.poll().unwrap().is_none(), "notre propre écriture n'est pas une modification extérieure");
    assert_eq!(mine.apply_written(&report), 1);
    assert_eq!(mine.get(DEMO_ID).unwrap().open_count, Some(1));
    assert!(w_mine.full_check().unwrap().is_none());
    let fresh = ComicDatabase::load(&cfg, None).unwrap();
    assert_eq!(fresh.get(DEMO_ID).unwrap().last_page_read, Some(7));
    assert_eq!(fresh.counters.update, 11);

    // Déjà à jour : rien d'écrit, compteur inchangé.
    let report = update_books(&cfg, &[BookEdit::new(DEMO_ID).progress(7)]).unwrap();
    assert_eq!((report.unchanged.as_slice(), report.written.len()), ([DEMO_ID.to_string()].as_slice(), 0));
    assert_eq!(counters(&mut conn), (11, 0));

    // Valeur refusée : erreur pour ce livre seulement, rien d'écrit.
    let report = update_books(
        &cfg,
        &[BookEdit::new(CE_ID).set("Rating", Value::Text("5".into())), BookEdit::new(DEMO_ID).rating(3.0)],
    )
    .unwrap();
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, CE_ID);
    assert_eq!(report.written.len(), 1);
    assert_eq!(counters(&mut conn), (12, 0));
    assert!(w_other.poll().unwrap().is_some());

    // ComicRack écrit le livre entre notre lecture et notre écriture : nouvel essai, et
    // les deux modifications sont gardées.
    let mut other_conn = connect(&cfg).unwrap();
    let by_comicrack = CE.replace("<Series>Batman &amp; Robin</Series>", "<Series>Batman &amp; Robin (CR)</Series>");
    let mut hook = |id: &str, attempt: usize| {
        if attempt == 0 {
            comicrack_write(&mut other_conn, id, &by_comicrack);
        }
    };
    let report = update_books_with(&cfg, &[BookEdit::new(CE_ID).rating(5.0)], &mut hook).unwrap();
    let w = &report.written[0];
    assert_eq!((w.retries, w.record.counter), (1, 14), "ComicRack a pris 13, nous 14");
    assert_eq!(w.record.data, by_comicrack.replace("<Rating>4</Rating>", "<Rating>5</Rating>"));
    assert_eq!(counters(&mut conn), (14, 0));
    let ch = w_other.poll().unwrap().unwrap();
    other.apply(ch);
    assert_eq!(other.get(CE_ID).unwrap().series.as_deref(), Some("Batman & Robin (CR)"));
    assert_eq!(other.get(CE_ID).unwrap().rating, Some(5.0));

    // Écritures concurrentes à chaque essai : conflit signalé, notre compteur annulé.
    let mut n = 0;
    let mut hook = |id: &str, _attempt: usize| {
        n += 1;
        let data = by_comicrack.replace("(CR)", &format!("(CR {n})"));
        comicrack_write(&mut other_conn, id, &data);
    };
    let report = update_books_with(&cfg, &[BookEdit::new(CE_ID).rating(2.0)], &mut hook).unwrap();
    assert_eq!(report.conflicts, [CE_ID]);
    assert!(report.written.is_empty());
    assert_eq!(counters(&mut conn), (14 + MAX_ATTEMPTS as i64, 0), "seules les écritures de « ComicRack » comptent");
    let (_, data) = row(&mut conn, CE_ID);
    assert!(data.contains(&format!("(CR {MAX_ATTEMPTS})")) && data.contains("<Rating>4</Rating>"));

    // Sans ligne dans `changes` : créée comme le fait WriteCounter.
    conn.query_drop("DELETE FROM changes").unwrap();
    let report = update_books(&cfg, &[BookEdit::new("30815f16-abb9-4c38-8c24-ecae11e25b1b").progress(13)]).unwrap();
    assert_eq!(report.written[0].record.counter, 1);
    assert_eq!(counters(&mut conn), (1, 0));
    let id: String = conn.query_first("SELECT id FROM changes").unwrap().unwrap();
    assert_eq!(id, STORAGE_GUID);

    // Livre supprimé par ComicRack entre la lecture et l'écriture : absent.
    let mut hook = |id: &str, _: usize| {
        other_conn.exec_drop("Delete from comics where id=?", (id,)).unwrap();
    };
    let report = update_books_with(&cfg, &[BookEdit::new(DEMO_ID).progress(9)], &mut hook).unwrap();
    assert_eq!(report.missing, [DEMO_ID]);
    assert_eq!(counters(&mut conn), (1, 0));
}
