//! Tests de la modification des listes (`lists_edit`) et de l'écriture dans `ComicDb.xml`.

use super::comicdb::{self, backups, replace_comic_lists, BACKUPS_KEPT};
use super::xml::ItemXml;
use super::*;
use crate::comic_lists::ListKind;
use crate::smart::{Limit, LimitKind, Matcher, Mode, Selection};

const LIB: &str = "4f632566-25d0-4f05-94c9-d1da14015030";
const F1: &str = "d3834683-f43f-4010-b469-48ff0ba128b2";
const L1: &str = "1b7b2b42-df47-46b8-bc55-1a75decd23fe";
const S1: &str = "36e9430d-e7bd-4660-90bb-611380d042b5";
const S2: &str = "a2215f7d-b795-48bc-9085-c1bf3e35dd05";
const F2: &str = "856ed100-bd86-439a-8db4-6f8a1e313ec7";
const B1: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const B2: &str = "aaaaaaaa-0000-0000-0000-000000000002";
const B3: &str = "aaaaaaaa-0000-0000-0000-000000000003";

/// Extrait d'un vrai `ComicDb.xml` de ComicRack CE (stockage MySQL : pas de livres), avec
/// réglages d'affichage, statistiques, dossier replié, élément inconnu (`<Futur>`) et
/// liste intelligente partant d'une autre (`BaseListId`).
const COMICDB: &str = "\u{feff}<?xml version=\"1.0\"?>
<ComicDatabase xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" Id=\"f30a051f-ef53-4ea8-bd9a-0f44a852cab3\">
  <Books />
  <ComicLists>
    <Item xsi:type=\"ComicLibraryListItem\" Id=\"4f632566-25d0-4f05-94c9-d1da14015030\" Name=\"Bibliothèque\">
      <BookCount>5319</BookCount>
      <NewBookCountDate>2026-10-21T20:53:19.9589731Z</NewBookCountDate>
      <Display>
        <View ItemViewMode=\"Thumbnail\" SortKey=\"1,100\" StackerId=\"1\">
          <ItemRowHeight>22</ItemRowHeight>
        </View>
      </Display>
    </Item>
    <Item xsi:type=\"ComicListItemFolder\" Id=\"d3834683-f43f-4010-b469-48ff0ba128b2\" Name=\"___1\">
      <BookCount>323</BookCount>
      <NewBookCount>1</NewBookCount>
      <NewBookCountDate>2026-10-21T20:53:20.1714959Z</NewBookCountDate>
      <UnreadBookCount>322</UnreadBookCount>
      <Display>
        <View ItemViewMode=\"Details\" />
      </Display>
      <Items>
        <Item xsi:type=\"ComicIdListItem\" Id=\"1b7b2b42-df47-46b8-bc55-1a75decd23fe\" Name=\"Test liste ComicRust\" QuickOpen=\"true\">
          <BookCount>2</BookCount>
          <NewBookCountDate>2026-10-21T20:53:16.4635313Z</NewBookCountDate>
          <Display>
            <View ItemViewMode=\"Tile\" />
          </Display>
          <BookIds>
            <guid>aaaaaaaa-0000-0000-0000-000000000001</guid>
            <guid>aaaaaaaa-0000-0000-0000-000000000002</guid>
          </BookIds>
        </Item>
        <Item xsi:type=\"ComicSmartListItem\" Id=\"36e9430d-e7bd-4660-90bb-611380d042b5\" Name=\"Séries complétes (Chemin)\">
          <NewBookCountDate>2026-10-21T20:53:20.1704936Z</NewBookCountDate>
          <CacheStorage>aaaaaaaa-0000-0000-0000-000000000003</CacheStorage>
          <Display>
            <View ItemViewMode=\"Details\" />
          </Display>
          <Matchers>
            <ComicBookMatcher xsi:type=\"ComicBookFullPathMatcher\" MatchOperator=\"1\">
              <MatchValue>.\\_BD\\</MatchValue>
            </ComicBookMatcher>
          </Matchers>
          <Futur>à garder</Futur>
        </Item>
      </Items>
    </Item>
    <Item xsi:type=\"ComicSmartListItem\" Id=\"a2215f7d-b795-48bc-9085-c1bf3e35dd05\" Name=\"Séries en cours\">
      <BookCount>12</BookCount>
      <NewBookCountDate>2026-10-21T21:01:04.3640846Z</NewBookCountDate>
      <Display>
        <View ItemViewMode=\"Details\" />
      </Display>
      <Matchers>
        <ComicBookMatcher xsi:type=\"ComicBookFullPathMatcher\" MatchOperator=\"1\">
          <MatchValue>Séries En cours</MatchValue>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type=\"SmartListSeriesAllCompleteMatcher\" MatchOperator=\"1\">
          <MatchValue />
        </ComicBookMatcher>
      </Matchers>
      <LimitValue>10</LimitValue>
      <BaseListId>1b7b2b42-df47-46b8-bc55-1a75decd23fe</BaseListId>
    </Item>
    <Item xsi:type=\"ComicListItemFolder\" Id=\"856ed100-bd86-439a-8db4-6f8a1e313ec7\" Name=\"Divers\" Collapsed=\"true\">
      <NewBookCountDate>2026-10-21T20:55:46.289247Z</NewBookCountDate>
      <Items>
        <Item xsi:type=\"ComicSmartListItem\" Id=\"fb183a92-af78-4238-aba0-4e77020fa9b4\" Name=\"Récemment lu\">
          <BookCount>2</BookCount>
          <Matchers>
            <ComicBookMatcher xsi:type=\"ComicBookOpenedMatcher\" MatchOperator=\"3\">
              <MatchValue>14</MatchValue>
            </ComicBookMatcher>
          </Matchers>
        </Item>
      </Items>
    </Item>
    <Item xsi:type=\"ComicListItemFolder\" Id=\"f612eb74-e8cd-4ec5-a44e-831e67e29d61\" Name=\"Listes temporaires\" Temporary=\"true\">
      <Items />
    </Item>
  </ComicLists>
  <WatchFolders>
    <WatchFolder Folder=\"\\\\NAS\\_BD\" />
  </WatchFolders>
</ComicDatabase>";

fn editor() -> ListsEditor {
    let mut w = Vec::new();
    let e = ListsEditor::from_comicdb_xml(COMICDB, &mut w).unwrap();
    assert!(w.is_empty(), "{w:?}");
    e
}

fn xml_of(e: &ListsEditor, id: &str) -> String {
    e.row(id).unwrap().xml.clone()
}

fn find(lists: &[ComicList], id: &str) -> ComicList {
    ComicList::find(lists, id).unwrap_or_else(|| panic!("liste {id} absente")).clone()
}

fn v(kind: &str, op: i32, value: &str) -> Matcher {
    Matcher::value(kind, op, value, "", false, true)
}

/// Les lignes non modifiées sont identiques octet pour octet à `before`.
fn unchanged_except(before: &ListsEditor, after: &ListsEditor, changed: &[&str]) {
    for r in before.rows() {
        if changed.iter().any(|c| c.eq_ignore_ascii_case(&r.id)) {
            continue;
        }
        if let Some(a) = after.row(&r.id) {
            assert_eq!(a.xml, r.xml, "liste {} modifiée à tort", r.name);
        }
    }
}

#[test]
fn item_parse_render_is_identity() {
    for r in editor().rows() {
        let item = ItemXml::parse(&r.xml, 0).unwrap();
        assert_eq!(item.render(), r.xml);
    }
    // Élément vide, attributs entre apostrophes, commentaire : rendus tels quels.
    for x in [
        r#"<Item xsi:type="ComicIdListItem" Id="x" Name="n" />"#,
        "<Item xsi:type='ComicIdListItem' Id='x'  Name='a &amp; b'><!-- c --><BookIds /></Item>",
    ] {
        assert_eq!(ItemXml::parse(x, 0).unwrap().render(), x);
    }
}

#[test]
fn unmodified_roundtrip() {
    let e = editor();
    let doc = replace_comic_lists(COMICDB, &e.comic_lists_xml()).unwrap();
    assert_eq!(parse_comic_lists(&doc).unwrap(), parse_comic_lists(COMICDB).unwrap());
    assert!(doc.starts_with("\u{feff}<?xml"));
    assert!(doc.ends_with(
        "  <WatchFolders>\n    <WatchFolder Folder=\"\\\\NAS\\_BD\" />\n  </WatchFolders>\n</ComicDatabase>"
    ));
}

#[test]
fn create_lists_in_comicrack_format() {
    let mut e = editor();
    let before = e.clone();
    let folder = e.create(None, Some(1), "Nouveau dossier", NewList::Folder { combine: Combine::And }).unwrap();
    assert_eq!(uuid::Uuid::parse_str(&folder).unwrap().get_version_num(), 4);
    let reading = e
        .create(
            Some(&folder),
            None,
            "Lecture & co",
            NewList::Reading { books: vec![B2.to_uppercase(), B1.into(), B2.into()] },
        )
        .unwrap();
    let query = SmartQuery {
        mode: Mode::Or,
        matchers: vec![
            v("ComicBookWriterMatcher", 1, "Starlin <Jim>"),
            Matcher::group(
                Mode::Or,
                true,
                vec![
                    Matcher::value("ComicBookSeriesMatcher", 0, "Thanos", "", false, false),
                    Matcher::value("ComicBookYearMatcher", 3, "1990", "1995", true, true),
                ],
            ),
            v("ComicBookDuplicateMatcher", 0, ""),
        ],
        base_list: Some(L1.into()),
        not_in_base: true,
        limit: Some(Limit { kind: LimitKind::Gb, value: 2, selection: Selection::SortedBySeries, seed: 0 }),
        filtered: vec![B3.into()],
    };
    let smart = e.create(Some(&folder), Some(0), "Intelligente", NewList::Smart { query: query.clone() }).unwrap();

    // Format exact de XmlSerializer (indentation comprise, au 2e niveau).
    let expected = format!(
        r#"<Item xsi:type="ComicSmartListItem" Id="{smart}" Name="Intelligente" MatcherMode="Or">
        <Matchers>
          <ComicBookMatcher xsi:type="ComicBookWriterMatcher" MatchOperator="1">
            <MatchValue>Starlin &lt;Jim&gt;</MatchValue>
          </ComicBookMatcher>
          <ComicBookMatcher xsi:type="ComicBookGroupMatcher" Not="true" MatcherMode="Or">
            <Matchers>
              <ComicBookMatcher xsi:type="ComicBookSeriesMatcher" IgnoreCase="false">
                <MatchValue>Thanos</MatchValue>
              </ComicBookMatcher>
              <ComicBookMatcher xsi:type="ComicBookYearMatcher" Not="true" MatchOperator="3">
                <MatchValue>1990</MatchValue>
                <MatchValue2>1995</MatchValue2>
              </ComicBookMatcher>
            </Matchers>
          </ComicBookMatcher>
          <ComicBookMatcher xsi:type="ComicBookDuplicateMatcher">
            <MatchValue />
          </ComicBookMatcher>
        </Matchers>
        <Limit>true</Limit>
        <LimitType>GB</LimitType>
        <LimitValue>2</LimitValue>
        <LimitSelectionType>SortedBySeries</LimitSelectionType>
        <BaseListId>{L1}</BaseListId>
        <NotInBaseList>true</NotInBaseList>
        <FilteredIds>
          <guid>{B3}</guid>
        </FilteredIds>
      </Item>"#
    );
    assert_eq!(xml_of(&e, &smart), expected);
    assert_eq!(
        xml_of(&e, &reading),
        format!(
            "<Item xsi:type=\"ComicIdListItem\" Id=\"{reading}\" Name=\"Lecture &amp; co\">\n        <BookIds>\n          \
             <guid>{B2}</guid>\n          <guid>{B1}</guid>\n        </BookIds>\n      </Item>"
        )
    );
    assert_eq!(
        xml_of(&e, &folder),
        format!(
            "<Item xsi:type=\"ComicListItemFolder\" Id=\"{folder}\" Name=\"Nouveau dossier\" CombineMode=\"And\">\n      \
             <Items />\n    </Item>"
        )
    );

    // Relecture : modèle attendu, à la bonne place.
    let lists = e.lists().unwrap();
    assert_eq!(lists[1].id, folder);
    assert_eq!(lists[2].id, F1);
    let ListKind::Folder { combine, children } = &lists[1].kind else { panic!() };
    assert_eq!(*combine, Combine::And);
    assert_eq!(children.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), [smart.as_str(), reading.as_str()]);
    assert_eq!(children[1].kind, ListKind::Ids(vec![B2.into(), B1.into()]));
    assert_eq!(children[0].kind, ListKind::Smart { query: Some(query), cached: None });
    unchanged_except(&before, &e, &[]);

    // Pas d'attribut IgnoreCase sur un critère numérique.
    let year = Matcher::value("ComicBookYearMatcher", 0, "1990", "", false, false);
    assert_eq!(
        xml::matcher_xml(&year, "", "\n"),
        "<ComicBookMatcher xsi:type=\"ComicBookYearMatcher\">\n  <MatchValue>1990</MatchValue>\n</ComicBookMatcher>"
    );
    assert!(e.create(Some(L1), None, "x", NewList::Folder { combine: Combine::Or }).is_err(), "parent pas dossier");
    assert!(e.create(None, None, "  ", NewList::Folder { combine: Combine::Or }).is_err(), "nom vide");
}

#[test]
fn rename_and_description_touch_only_their_part() {
    let mut e = editor();
    let before = e.clone();
    e.rename(L1, "Thanos & Warlock \"intégrale\"").unwrap();
    e.set_description(L1, "Ordre <conseillé>").unwrap();
    let x = xml_of(&e, L1);
    let old = xml_of(&before, L1);
    assert!(x.starts_with(&format!(
        "<Item xsi:type=\"ComicIdListItem\" Id=\"{L1}\" Name=\"Thanos &amp; Warlock &quot;intégrale&quot;\" QuickOpen=\"true\">"
    )));
    // Description à sa place (après les statistiques, avant Display), le reste identique.
    assert_eq!(
        x.split_once('>').unwrap().1,
        old.split_once('>')
            .unwrap()
            .1
            .replace("<Display>", "<Description>Ordre &lt;conseillé&gt;</Description>\n          <Display>")
    );
    let l = find(&e.lists().unwrap(), L1);
    assert_eq!(l.name, "Thanos & Warlock \"intégrale\"");
    assert_eq!(l.description, "Ordre <conseillé>");
    assert_eq!(e.row(L1).unwrap().name, l.name);
    unchanged_except(&before, &e, &[L1]);
    e.set_description(L1, "").unwrap();
    assert!(!xml_of(&e, L1).contains("Description"));
}

#[test]
fn delete_reports_broken_base_lists() {
    let mut e = editor();
    assert_eq!(e.base_list_dependents(F1), [(S2.to_string(), "Séries en cours".to_string())]);
    let r = e.delete(F1).unwrap();
    assert_eq!(r.removed, [F1, L1, S1]);
    assert_eq!(r.broken_base, [(S2.to_string(), "Séries en cours".to_string())]);
    let lists = e.lists().unwrap();
    assert_eq!(lists.len(), 4);
    assert!(ComicList::find(&lists, L1).is_none());
    assert_eq!(e.rows().iter().filter(|r| r.parent.is_none()).map(|r| r.position).max(), Some(3));
    // Résultats mémorisés de la liste devenue « orpheline » retirés.
    assert!(!xml_of(&e, S2).contains("<BookCount>"));
    assert!(e.delete(LIB).is_err());
    assert!(e.delete("inconnue").is_err());
}

#[test]
fn copy_gets_new_ids_and_follows_internal_base_lists() {
    let mut e = editor();
    // S1 part de L1 (interne au dossier copié), S2 aussi (externe).
    let q = SmartQuery {
        matchers: vec![v("ComicBookSeriesMatcher", 1, "x")],
        base_list: Some(L1.into()),
        ..Default::default()
    };
    e.set_query(S1, &q).unwrap();
    let before = e.clone();
    let copy = e.copy(F1).unwrap();
    let lists = e.lists().unwrap();
    assert_eq!(lists[2].id, copy, "juste après l'original");
    assert_eq!(lists[2].name, "Copie de ___1");
    let ListKind::Folder { children, .. } = &lists[2].kind else { panic!() };
    assert_eq!(children.len(), 2);
    assert!(children.iter().all(|c| c.id != L1 && c.id != S1));
    assert_eq!(children[0].name, "Test liste ComicRust");
    assert_eq!(children[0].kind, ListKind::Ids(vec![B1.into(), B2.into()]));
    let ListKind::Smart { query: Some(cq), cached } = &children[1].kind else { panic!() };
    assert_eq!(cq.base_list.as_deref(), Some(children[0].id.as_str()), "BaseListId suit la copie");
    assert_eq!(*cached, None);
    // Réglages d'affichage et éléments inconnus copiés.
    let cx = xml_of(&e, &children[1].id);
    assert!(cx.contains("<Futur>à garder</Futur>") && cx.contains("<View ItemViewMode=\"Details\" />"));
    // Compteurs mémorisés par ComicRack retirés de la copie (et de ses sous-listes).
    for id in [copy.as_str(), children[0].id.as_str(), children[1].id.as_str()] {
        let x = xml_of(&e, id);
        assert!(STALE.iter().all(|n| !x.contains(&format!("<{n}>"))), "{x}");
    }
    assert!(xml_of(&e, L1).contains("<BookCount>2</BookCount>"), "original intact");
    // L'original et la liste externe ne bougent pas.
    unchanged_except(&before, &e, &[]);
    assert_eq!(find(&lists, S2).kind, find(&before.lists().unwrap(), S2).kind);
    assert!(e.copy(LIB).is_err());
}

#[test]
fn move_keeps_tree_consistent() {
    let mut e = editor();
    assert!(e.move_to(F1, Some(F1), None).is_err(), "dans lui-même");
    e.move_to(F2, Some(F1), Some(0)).unwrap();
    assert!(e.move_to(F1, Some(F2), None).is_err(), "dans un descendant");
    let lists = e.lists().unwrap();
    let ListKind::Folder { children, .. } = &lists[1].kind else { panic!() };
    assert_eq!(children.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), [F2, L1, S1]);
    assert_eq!(lists.len(), 4);
    // Déplacé tout en haut, puis à la fin du premier niveau.
    e.move_to(L1, None, Some(0)).unwrap();
    assert_eq!(e.lists().unwrap()[0].id, L1);
    e.move_to(L1, None, None).unwrap();
    let lists = e.lists().unwrap();
    assert_eq!(lists.last().unwrap().id, L1);
    let mut pos: Vec<i64> = e.rows().iter().filter(|r| r.parent.is_none()).map(|r| r.position).collect();
    pos.sort();
    assert_eq!(pos, (0..pos.len() as i64).collect::<Vec<_>>());
    // Le dossier d'origine a perdu un enfant : ses statistiques sont retirées.
    assert!(!xml_of(&e, F1).contains("<BookCount>"));
}

#[test]
fn reading_list_books() {
    let mut e = editor();
    let before = e.clone();
    assert_eq!(e.add_books(L1, &[B2.into(), B3.to_uppercase(), B3.into()], Some(1)).unwrap(), 1);
    assert_eq!(e.books(L1).unwrap(), [B1, B3, B2]);
    e.move_books(L1, &[B1.into()], 2).unwrap();
    assert_eq!(e.books(L1).unwrap(), [B3, B2, B1]);
    e.move_books(L1, &[B2.into(), B1.into()], 0).unwrap();
    assert_eq!(e.books(L1).unwrap(), [B2, B1, B3]);
    assert_eq!(e.remove_books(L1, &[B1.into(), "absent".into()]).unwrap(), 1);
    assert_eq!(find(&e.lists().unwrap(), L1).kind, ListKind::Ids(vec![B2.into(), B3.into()]));
    let x = xml_of(&e, L1);
    assert!(x.contains("<View ItemViewMode=\"Tile\" />") && x.contains("QuickOpen=\"true\""));
    assert!(!x.contains("<BookCount>"), "nombre mémorisé retiré");
    // Le dossier parent et la liste intelligente qui part de L1 aussi.
    assert!(!xml_of(&e, F1).contains("<BookCount>") && !xml_of(&e, F1).contains("<UnreadBookCount>"));
    assert!(!xml_of(&e, S2).contains("<BookCount>"));
    assert!(xml_of(&e, F1).contains("<NewBookCountDate>"), "date gardée");
    unchanged_except(&before, &e, &[L1, F1, S2]);
    assert!(e.add_books(S1, &[B1.into()], None).is_err(), "pas une liste de lecture");
}

#[test]
fn smart_list_converted_to_reading_list() {
    let mut e = editor();
    e.convert_to_reading_list(S2, vec![B3.into(), B1.into()]).unwrap();
    let x = xml_of(&e, S2);
    assert!(x.starts_with(&format!("<Item xsi:type=\"ComicIdListItem\" Id=\"{S2}\" Name=\"Séries en cours\">")));
    assert!(x.contains("<NewBookCountDate>") && x.contains("<Display>"));
    for gone in ["Matchers", "LimitValue", "BaseListId", "BookCount>"] {
        assert!(!x.contains(gone), "{gone} : {x}");
    }
    assert_eq!(e.row(S2).unwrap().kind, KIND_READING);
    assert_eq!(find(&e.lists().unwrap(), S2).kind, ListKind::Ids(vec![B3.into(), B1.into()]));
    assert!(e.convert_to_reading_list(L1, vec![]).is_err());
}

#[test]
fn smart_query_replaced_and_caches_cleared() {
    let mut e = editor();
    let before = e.clone();
    let q = SmartQuery {
        matchers: vec![v("ComicBookTagsMatcher", 6, "pull")],
        base_list: None,
        limit: None,
        ..Default::default()
    };
    e.set_query(S1, &q).unwrap();
    let x = xml_of(&e, S1);
    assert!(!x.contains("CacheStorage"));
    assert!(x.contains("<Futur>à garder</Futur>"), "élément inconnu gardé");
    assert!(x.contains(
        "<Matchers>\n            <ComicBookMatcher xsi:type=\"ComicBookTagsMatcher\" MatchOperator=\"6\">\n              \
         <MatchValue>pull</MatchValue>\n            </ComicBookMatcher>\n          </Matchers>\n          <Futur>"
    ));
    assert_eq!(find(&e.lists().unwrap(), S1).kind, ListKind::Smart { query: Some(q), cached: None });
    unchanged_except(&before, &e, &[S1, F1]);

    // Limite désactivée : les réglages de limite existants restent (comme ComicRack).
    let mut q2 = match find(&e.lists().unwrap(), S2).kind {
        ListKind::Smart { query: Some(q), .. } => q,
        k => panic!("{k:?}"),
    };
    assert_eq!(q2.base_list.as_deref(), Some(L1));
    q2.mode = Mode::Or;
    e.set_query(S2, &q2).unwrap();
    let x = xml_of(&e, S2);
    assert!(x.contains("Name=\"Séries en cours\" MatcherMode=\"Or\">"));
    assert!(x.contains("<LimitValue>10</LimitValue>") && !x.contains("<Limit>"));
    assert!(x.contains("<MatchValue />"));
    q2.base_list = None;
    q2.limit = Some(Limit { kind: LimitKind::Count, value: 25, selection: Selection::Random, seed: 0 });
    e.set_query(S2, &q2).unwrap();
    let x = xml_of(&e, S2);
    assert!(x.contains("<Limit>true</Limit>") && !x.contains("LimitValue") && !x.contains("BaseListId"), "{x}");

    // Une liste ne part pas d'elle-même ni d'une liste qui dépend d'elle.
    let self_base = SmartQuery { base_list: Some(S1.into()), ..Default::default() };
    assert!(e.set_query(S1, &self_base).is_err());
    let folder_base = SmartQuery { base_list: Some(F1.into()), ..Default::default() };
    assert!(e.set_query(S1, &folder_base).is_err(), "son dossier dépend d'elle");
    assert!(e.set_query(S2, &folder_base).is_ok());
    assert!(e.depends_on(S2, L1));
    assert!(e.set_query(L1, &q2).is_err(), "pas une liste intelligente");
}

#[test]
fn folder_combine_mode() {
    let mut e = editor();
    e.set_combine(F2, Combine::Empty).unwrap();
    assert!(xml_of(&e, F2).starts_with(&format!(
        "<Item xsi:type=\"ComicListItemFolder\" Id=\"{F2}\" Name=\"Divers\" Collapsed=\"true\" CombineMode=\"Empty\">"
    )));
    e.set_combine(F2, Combine::Or).unwrap();
    assert!(!xml_of(&e, F2).contains("CombineMode"), "valeur par défaut omise");
    assert!(matches!(find(&e.lists().unwrap(), F2).kind, ListKind::Folder { combine: Combine::Or, .. }));
    assert!(e.set_combine(L1, Combine::And).is_err());
}

#[test]
fn comicdb_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ComicDb.xml");
    std::fs::write(&path, COMICDB).unwrap();
    let db = ComicDbLists::read(&path).unwrap();
    let mut e = ListsEditor::new(db.rows.clone());
    e.rename(L1, "Renommée").unwrap();

    let err = db.save_checked(e.rows(), || Ok(true)).unwrap_err();
    assert!(err.to_string().contains("Ferme ComicRack"), "{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), COMICDB, "rien écrit");

    let report = db.save_checked(e.rows(), || Ok(false)).unwrap();
    assert_eq!(std::fs::read_to_string(&report.backup).unwrap(), COMICDB);
    let name = report.backup.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("ComicDb.comicrust-") && name.ends_with(".xml.bak"), "{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let (head, _) = COMICDB.split_once("<ComicLists>").unwrap();
    let (_, tail) = COMICDB.split_once("</ComicLists>").unwrap();
    assert!(text.starts_with(&format!("{head}<ComicLists>")) && text.ends_with(&format!("</ComicLists>{tail}")));
    assert_eq!(find(&parse_comic_lists(&text).unwrap(), L1).name, "Renommée");
    assert_eq!(parse_comic_lists(&text).unwrap(), e.lists().unwrap());

    // Fichier changé depuis la lecture : refus.
    let err = db.save_checked(e.rows(), || Ok(false)).unwrap_err();
    assert!(matches!(err, Error::Conflict(_)) && err.to_string().contains("modifié depuis sa lecture"), "{err}");

    // Les 10 sauvegardes les plus récentes sont gardées.
    for n in 0..12 {
        std::fs::write(dir.path().join(format!("ComicDb.comicrust-2020010{}-0000{n:02}.xml.bak", n % 3)), "x").unwrap();
    }
    let db = ComicDbLists::read(&path).unwrap();
    let report = db.save_checked(db.rows.as_slice(), || Ok(false)).unwrap();
    let kept = backups(dir.path(), "ComicDb");
    assert_eq!(kept.len(), BACKUPS_KEPT);
    assert!(kept.contains(&report.backup));
    assert!(std::fs::read_dir(dir.path()).unwrap().flatten().all(|f| !f.file_name().to_string_lossy().contains("tmp")));
}

#[test]
fn comicdb_without_lists_element() {
    let doc = "<?xml version=\"1.0\"?>\n<ComicDatabase xmlns:xsi=\"x\">\n  <Books />\n  <ComicLists />\n  <Foo />\n</ComicDatabase>";
    let out = replace_comic_lists(doc, "<Item xsi:type=\"ComicIdListItem\" Id=\"a\" Name=\"a\" />").unwrap();
    assert_eq!(
        out,
        "<?xml version=\"1.0\"?>\n<ComicDatabase xmlns:xsi=\"x\">\n  <Books />\n  <ComicLists><Item xsi:type=\"ComicIdListItem\" \
         Id=\"a\" Name=\"a\" /></ComicLists>\n  <Foo />\n</ComicDatabase>"
    );
    assert!(replace_comic_lists("<ComicDatabase><Books /></ComicDatabase>", "").is_err());
}

/// `COMICRUST_TEST_COMICDB` = chemin d'un vrai `ComicDb.xml` : chaque liste est rendue à
/// l'identique, et une modification ne touche que la liste concernée.
#[test]
fn real_comicdb_roundtrip() {
    let Ok(path) = std::env::var("COMICRUST_TEST_COMICDB") else { return };
    let text = std::fs::read_to_string(&path).unwrap();
    let mut w = Vec::new();
    let e = ListsEditor::from_comicdb_xml(&text, &mut w).unwrap();
    for r in e.rows() {
        assert_eq!(ItemXml::parse(&r.xml, 0).unwrap().render(), r.xml);
    }
    let doc = replace_comic_lists(&text, &e.comic_lists_xml()).unwrap();
    assert_eq!(parse_comic_lists(&doc).unwrap(), parse_comic_lists(&text).unwrap());
    for r in e.rows().iter().filter(|r| r.kind == KIND_SMART) {
        let ListKind::Smart { query: Some(q), .. } = find(&e.lists().unwrap(), &r.id).kind else { panic!() };
        let mut e2 = e.clone();
        e2.set_query(&r.id, &q).unwrap();
        let ListKind::Smart { query: Some(q2), .. } = find(&e2.lists().unwrap(), &r.id).kind else { panic!() };
        assert_eq!(q, q2, "{}", r.name);
        // Critères réécrits au format de ComicRack : identiques à l'original.
        let old = ItemXml::parse(&r.xml, 0).unwrap();
        let new = ItemXml::parse(&e2.row(&r.id).unwrap().xml, 0).unwrap();
        assert_eq!(old.child_raw("Matchers"), new.child_raw("Matchers"), "{}", r.name);
    }
    eprintln!("{} listes relues et réécrites sans écart", e.rows().len());
}

/// `ComicDb.xml` écrit sous Windows (fins de ligne `\r\n`) : les éléments ajoutés suivent.
#[test]
fn windows_line_endings_are_followed() {
    let crlf = COMICDB.replace('\n', "\r\n");
    let mut w = Vec::new();
    let mut e = ListsEditor::from_comicdb_xml(&crlf, &mut w).unwrap();
    let q = SmartQuery { matchers: vec![v("ComicBookSeriesMatcher", 0, "x")], ..Default::default() };
    let id = e.create(Some(F2), None, "n", NewList::Smart { query: q.clone() }).unwrap();
    e.set_query(S1, &q).unwrap();
    e.add_books(L1, &[B3.into()], None).unwrap();
    e.set_description(F2, "d").unwrap();
    for x in [xml_of(&e, &id), xml_of(&e, S1), xml_of(&e, L1), xml_of(&e, F2)] {
        assert!(x.contains("\r\n"));
        assert!(!x.replace("\r\n", "").contains('\n'), "{x:?}");
    }
}

// ---------------------------------------------------------------------------
// Relecture v0.9.0 : caractères interdits, critères gardés, ComicRack, sauvegardes
// ---------------------------------------------------------------------------

#[test]
fn forbidden_xml_characters_are_never_written() {
    let mut e = editor();
    e.rename(L1, "A\u{1}B\u{B}C\u{FFFF}").unwrap();
    e.set_description(L1, "x\u{0}y\u{C}z\tt").unwrap();
    let q = SmartQuery { matchers: vec![v("ComicBookSeriesMatcher", 1, "Tha\u{1F}nos\u{8}")], ..Default::default() };
    e.set_query(S1, &q).unwrap();
    let id = e.create(None, None, "\u{2}Nouvelle", NewList::Folder { combine: Combine::Or }).unwrap();
    assert_eq!(e.row(&id).unwrap().name, "Nouvelle");
    assert_eq!(e.row(L1).unwrap().name, "AB C");
    let doc = e.document();
    xml::check_xml_document(&doc, "test").unwrap();
    assert!(!doc.contains("&#x1") && !doc.contains("&#x0;") && !doc.contains("&#x8;"), "aucune référence interdite");
    assert!(xml_of(&e, L1).contains("Name=\"AB C\"") && xml_of(&e, L1).contains("<Description>xy z\tt</Description>"));
    assert!(xml_of(&e, S1).contains("<MatchValue>Thanos</MatchValue>"));
    // Échappement direct : rien d'interdit ne sort.
    assert_eq!(xml::escape_attr("a\u{1}\u{B}b\n"), "a b&#xA;");
    assert_eq!(xml::escape_text("a\u{1F}\u{C}b\r"), "a b&#xD;");
    // Contrôle final : caractère brut ou référence interdite refusés, références permises acceptées.
    assert!(xml::check_xml_document("<a>\u{1}</a>", "x").is_err());
    assert!(xml::check_xml_document("<a>&#x1;</a>", "x").is_err());
    assert!(xml::check_xml_document("<a>&#11;</a>", "x").is_err());
    assert!(xml::check_xml_document("<a>&#xFFFE;</a>", "x").is_err());
    assert!(xml::check_xml_document("<a>&#xD800;</a>", "x").is_err(), "demi-codet");
    assert!(xml::check_xml_document("\u{feff}<a b=\"&#xA;&#9;&#xD;\">&amp;&#x1F600;</a>", "x").is_ok());
}

#[test]
fn comicdb_save_refuses_forbidden_characters() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ComicDb.xml");
    std::fs::write(&path, COMICDB).unwrap();
    let db = ComicDbLists::read(&path).unwrap();
    // Ligne fabriquée à la main (contournant l'éditeur) avec une référence interdite.
    let mut rows = db.rows.clone();
    let i = rows.iter().position(|r| r.id == L1).unwrap();
    rows[i].xml = rows[i].xml.replace("Name=\"Test liste ComicRust\"", "Name=\"Test&#x1;\"");
    rows[i].name = "Test\u{1}".into();
    let err = db.save_checked(&rows, || Ok(false)).unwrap_err();
    assert!(err.to_string().contains("caractère interdit"), "{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), COMICDB, "rien écrit");
    assert!(backups(dir.path(), "ComicDb").is_empty(), "pas même une sauvegarde");
}

/// Liste intelligente avec des membres de critères que ComicRust ne modélise pas.
const RICH: &str = "\u{feff}<?xml version=\"1.0\"?>
<ComicDatabase xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">
  <ComicLists>
    <Item xsi:type=\"ComicSmartListItem\" Id=\"36e9430d-e7bd-4660-90bb-611380d042b5\" Name=\"Riche\">
      <Matchers>
        <ComicBookMatcher xsi:type=\"ComicBookSeriesMatcher\" Name=\"Ma série\" MatchOperator=\"1\">
          <MatchValue>Thanos</MatchValue>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type=\"ComicBookPluginMatcher\" Name=\"Module\" PluginKey=\"cle-123\" MatchOperator=\"2\">
          <MatchValue>a</MatchValue>
          <MatchValue2>b</MatchValue2>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type=\"ComicBookAllPropertiesMatcher\" MatchOperator=\"1\">
          <MatchValue>Starlin</MatchValue>
          <Option>Writer</Option>
        </ComicBookMatcher>
        <ComicBookMatcher xsi:type=\"ComicBookGroupMatcher\" MatcherMode=\"Or\" Collapsed=\"true\">
          <Matchers>
            <ComicBookMatcher xsi:type=\"ComicBookRatingMatcher\" MatchOperator=\"1\">
              <MatchValue>4,5</MatchValue>
            </ComicBookMatcher>
          </Matchers>
        </ComicBookMatcher>
      </Matchers>
    </Item>
  </ComicLists>
</ComicDatabase>";

fn rich_query(e: &ListsEditor) -> SmartQuery {
    match find(&e.lists().unwrap(), S1).kind {
        ListKind::Smart { query: Some(q), .. } => q,
        k => panic!("{k:?}"),
    }
}

#[test]
fn unknown_matcher_members_survive_rewrites() {
    let mut w = Vec::new();
    let mut e = ListsEditor::from_comicdb_xml(RICH, &mut w).unwrap();
    let original = xml_of(&e, S1);
    let q = rich_query(&e);
    assert_eq!(q.matchers[2].all_properties_option(), "Writer");

    // Mêmes critères : élément identique octet pour octet.
    e.set_query(S1, &q).unwrap();
    assert_eq!(xml_of(&e, S1), original);

    // Un critère voisin modifié : les autres recopiés tels quels, le modifié garde `Name`.
    let mut q2 = q.clone();
    if let Matcher::Value { value, .. } = &mut q2.matchers[0] {
        *value = "Warlock".into();
    }
    if let Matcher::Group { mode, .. } = &mut q2.matchers[3] {
        *mode = Mode::And;
    }
    e.set_query(S1, &q2).unwrap();
    let x = xml_of(&e, S1);
    assert!(
        x.contains(
            "<ComicBookMatcher xsi:type=\"ComicBookSeriesMatcher\" Name=\"Ma série\" MatchOperator=\"1\">\n          \
             <MatchValue>Warlock</MatchValue>"
        ),
        "{x}"
    );
    for kept in [
        "PluginKey=\"cle-123\"",
        "<MatchValue2>b</MatchValue2>",
        "<Option>Writer</Option>",
        "<MatchValue>4,5</MatchValue>",
    ] {
        assert!(x.contains(kept), "{kept} perdu :\n{x}");
    }
    assert!(x.contains("<ComicBookMatcher xsi:type=\"ComicBookGroupMatcher\" Collapsed=\"true\">"), "{x}");
    let plugin = original.find("<ComicBookMatcher xsi:type=\"ComicBookPluginMatcher\"").unwrap();
    let end = original[plugin..].find("</ComicBookMatcher>").unwrap() + plugin + "</ComicBookMatcher>".len();
    assert!(x.contains(&original[plugin..end]), "critère non modifié recopié octet pour octet");
    // Relu : même sens, membres inconnus toujours là.
    let back = rich_query(&e);
    assert_eq!(back, q2);
    assert_eq!(back.matchers[2].all_properties_option(), "Writer");

    // Type changé : seul `Name` est gardé.
    let mut q3 = back.clone();
    if let Matcher::Value { kind, .. } = &mut q3.matchers[1] {
        *kind = "ComicBookTitleMatcher".into();
    }
    e.set_query(S1, &q3).unwrap();
    let x = xml_of(&e, S1);
    assert!(
        x.contains("xsi:type=\"ComicBookTitleMatcher\" Name=\"Module\" MatchOperator=\"2\"")
            && !x.contains("PluginKey")
    );
}

#[test]
fn comicrack_detection_checked_twice_and_unknown_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ComicDb.xml");
    std::fs::write(&path, COMICDB).unwrap();
    let db = ComicDbLists::read(&path).unwrap();
    let mut e = ListsEditor::new(db.rows.clone());
    e.rename(L1, "Renommée").unwrap();

    // Détection impossible : comme si ComicRack était lancé.
    let err = db.save_checked(e.rows(), || Err("accès refusé".into())).unwrap_err();
    assert!(err.to_string().starts_with(COMICRACK_UNKNOWN) && err.to_string().contains("accès refusé"), "{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), COMICDB);

    // ComicRack lancé pendant la préparation : vu juste avant le remplacement.
    let calls = std::cell::Cell::new(0);
    let err = db
        .save_checked(e.rows(), || {
            calls.set(calls.get() + 1);
            Ok(calls.get() > 1)
        })
        .unwrap_err();
    assert_eq!(calls.get(), 2, "détection au début et juste avant le remplacement");
    assert!(err.to_string().contains("Ferme ComicRack"), "{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), COMICDB, "fichier intact");
    assert!(std::fs::read_dir(dir.path()).unwrap().flatten().all(|f| !f.file_name().to_string_lossy().contains("tmp")));
    let calls = std::cell::Cell::new(0);
    db.save_checked(e.rows(), || {
        calls.set(calls.get() + 1);
        Ok(false)
    })
    .unwrap();
    assert_eq!(calls.get(), 2);
}

#[test]
fn origin_backup_is_kept_forever() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ComicDb.xml");
    std::fs::write(&path, COMICDB).unwrap();
    // Première écriture : sauvegarde d'origine + sauvegarde ordinaire.
    let db = ComicDbLists::read(&path).unwrap();
    let mut e = ListsEditor::new(db.rows.clone());
    e.rename(L1, "Un").unwrap();
    let report = db.save_checked(e.rows(), || Ok(false)).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let origin = comicdb::origin_backup(dir.path(), "ComicDb").expect("sauvegarde d'origine");
    let name = origin.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("ComicDb.comicrust-origine-") && name.ends_with(".xml.bak"), "{name}");
    assert_eq!(std::fs::read_to_string(&origin).unwrap(), COMICDB);
    assert_eq!(backups(dir.path(), "ComicDb"), std::slice::from_ref(&report.backup));
    // Beaucoup d'écritures : la rotation ne touche jamais l'origine.
    for n in 0..(BACKUPS_KEPT + 3) {
        let db = ComicDbLists::read(&path).unwrap();
        let mut e = ListsEditor::new(db.rows.clone());
        e.rename(L1, &format!("N{n}")).unwrap();
        db.save_checked(e.rows(), || Ok(false)).unwrap();
    }
    assert_eq!(backups(dir.path(), "ComicDb").len(), BACKUPS_KEPT);
    assert_eq!(comicdb::origin_backup(dir.path(), "ComicDb"), Some(origin.clone()));
    assert_eq!(std::fs::read_to_string(&origin).unwrap(), COMICDB, "toujours le fichier d'avant ComicRust");
}

#[test]
fn oldest_previous_backup_becomes_origin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ComicDb.xml");
    std::fs::write(&path, COMICDB).unwrap();
    std::fs::write(dir.path().join("ComicDb.comicrust-20200101-000000.xml.bak"), "ancienne").unwrap();
    std::fs::write(dir.path().join("ComicDb.comicrust-20210101-000000.xml.bak"), "moins ancienne").unwrap();
    let db = ComicDbLists::read(&path).unwrap();
    db.save_checked(db.rows.as_slice(), || Ok(false)).unwrap();
    let origin = comicdb::origin_backup(dir.path(), "ComicDb").unwrap();
    assert_eq!(origin.file_name().unwrap(), "ComicDb.comicrust-origine-20200101-000000.xml.bak");
    assert_eq!(std::fs::read_to_string(&origin).unwrap(), "ancienne");
    assert_eq!(backups(dir.path(), "ComicDb").len(), 2);
}

#[cfg(unix)]
#[test]
fn backup_failure_prevents_writing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ComicDb.xml");
    std::fs::write(&path, COMICDB).unwrap();
    let db = ComicDbLists::read(&path).unwrap();
    let mut e = ListsEditor::new(db.rows.clone());
    e.rename(L1, "Un").unwrap();
    // Dossier en lecture seule : sauvegarde impossible (sauf pour root, qui passe outre).
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = std::fs::write(dir.path().join("essai"), "x").is_ok();
    let res = db.save_checked(e.rows(), || Ok(false));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    if probe {
        eprintln!("droits ignorés (root) : test de sauvegarde impossible sauté");
        return;
    }
    let err = res.unwrap_err();
    assert!(err.to_string().contains("sauvegarde"), "{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), COMICDB, "rien écrit");
}
