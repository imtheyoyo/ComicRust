//! Tests du module `edit` : validation, aperçu, écriture (SQLite ; MySQL seulement si
//! `COMICRUST_TEST_MYSQL` est définie), fichiers et modèles de nom.

use std::path::{Path, PathBuf};

use super::files::*;
use super::*;
use crate::store::{StoreConfig, SQLITE_FILE_NAME};

const A: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const B: &str = "aaaaaaaa-0000-4000-8000-000000000002";
const ABSENT: &str = "deadbeef-0000-0000-0000-000000000000";

fn norm(field: &str, v: impl Into<EditValue>) -> Result<Option<Norm>, String> {
    catalog::field(field).unwrap().normalize(Some(&v.into()))
}

fn text(s: &str) -> Result<Option<Norm>, String> {
    Ok(Some(Norm::Text(s.into())))
}

fn book() -> LibraryBook {
    LibraryBook {
        id: A.into(),
        file: Some(r"Z:\BD\Thanos Quest 01 (1990).cbz".into()),
        series: Some("Thanos Quest".into()),
        number: Some("1".into()),
        writer: Some("Jim Starlin".into()),
        current_page: Some(4),
        rating: Some(4.5),
        released: Some("1990-10-01T00:00:00".into()),
        custom_values_store: Some(",a=1".into()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------
// Catalogue et validation
// ---------------------------------------------------------------------------------------

#[test]
fn catalog_is_consistent() {
    let mut names = std::collections::HashSet::new();
    for f in FIELDS {
        assert!(names.insert(f.name.to_ascii_lowercase()), "nom en double : {}", f.name);
        assert!(
            crate::db::write::ELEMENTS.contains(&f.name) || f.name == "Checked",
            "{} n'est pas un élément de ComicRack",
            f.name
        );
        assert!(!READ_ONLY_FIELDS.iter().any(|(n, _)| *n == f.name), "{} à la fois modifiable et non", f.name);
        // Champs du ComicInfo.xml : exactement ceux relus depuis les fichiers.
        assert_eq!(
            f.in_comic_info,
            crate::scanner::COMIC_INFO_FIELDS.contains(&f.column) || f.name == "PreferredFrontCover",
            "{}",
            f.name
        );
        assert!(!f.label.is_empty() && !f.kind.label().is_empty());
    }
    assert_eq!(field("series").unwrap().name, "Series");
    assert_eq!(field("ReleasedTime").unwrap().name, "Released");
    assert_eq!(field("book_price").unwrap().name, "BookPrice");
    assert!(field("PageCount").is_none() && field("Id").is_none());
    assert_eq!(field("AgeRating").unwrap().suggestions()[0], "Unknown");
}

#[test]
fn validation_by_type() {
    // Texte : espaces retirés, vide = vider, retour à la ligne refusé.
    assert_eq!(norm("Series", "  Thanos  "), text("Thanos"));
    assert_eq!(norm("Series", "   "), Ok(None));
    assert!(norm("Title", "a\nb").unwrap_err().contains("retour à la ligne"));
    assert!(norm("Title", "a\u{1}b").unwrap_err().contains("U+0001"));
    assert_eq!(norm("Number", 12), text("12"));
    // Texte long : gardé tel quel.
    assert_eq!(norm("Summary", "  ligne 1\nligne 2 "), text("  ligne 1\nligne 2 "));
    // Listes : nettoyées, sans doublon (casse indifférente), « , » entre les valeurs.
    assert_eq!(norm("Writer", " Jim Starlin,Ron Lim , ,jim starlin"), text("Jim Starlin, Ron Lim"));
    assert_eq!(norm("Tags", vec!["a".to_string(), " b ".into(), "".into()]), text("a, b"));
    assert!(norm("Tags", vec!["a,b".to_string()]).is_err());
    assert_eq!(norm("Genre", ""), Ok(None));
    // Entiers : bornes, -1 (défaut) = vider, réel entier accepté.
    assert_eq!(norm("Volume", "1968"), Ok(Some(Norm::Int(1968))));
    assert_eq!(norm("Volume", -1), Ok(None));
    assert_eq!(norm("Volume", 3.0), Ok(Some(Norm::Int(3))));
    assert!(norm("Volume", 2.5).unwrap_err().contains("pas un entier"));
    assert!(norm("Month", 13).unwrap_err().contains("de 1 à 12"));
    assert!(norm("Year", "deux mille").unwrap_err().contains("pas un entier"));
    assert_eq!(norm("Year", ""), Ok(None));
    assert_eq!(norm("PreferredFrontCover", 0), Ok(None), "0 : valeur par défaut");
    // Notes : 0 à 5, virgule décimale, 0 = pas de note.
    assert_eq!(norm("Rating", "4,5"), Ok(Some(Norm::Real(4.5))));
    assert_eq!(norm("Rating", 0), Ok(None));
    assert!(norm("Rating", 7).unwrap_err().contains("de 0 à 5"));
    assert!(norm("CommunityRating", f64::NAN).is_err());
    // Prix : positif ou nul (0 = gratuit, gardé).
    assert_eq!(norm("BookPrice", "12,50"), Ok(Some(Norm::Real(12.5))));
    assert_eq!(norm("BookPrice", 0), Ok(Some(Norm::Real(0.0))));
    assert!(norm("BookPrice", -1).unwrap_err().contains("négatif"));
    // Oui / non / inconnu, manga.
    assert_eq!(norm("BlackAndWhite", "oui"), text("Yes"));
    assert_eq!(norm("SeriesComplete", false), text("No"));
    assert_eq!(norm("SeriesComplete", "Unknown"), Ok(None));
    assert!(norm("BlackAndWhite", "YesAndRightToLeft").is_err());
    assert_eq!(norm("Manga", "YesAndRightToLeft"), text("YesAndRightToLeft"));
    assert_eq!(norm("Manga", "droite à gauche"), text("YesAndRightToLeft"));
    assert!(norm("Manga", "peut-être").unwrap_err().contains("permis"));
    // Booléens (valeur par défaut vraie = vider).
    assert_eq!(norm("Checked", "non"), Ok(Some(Norm::Bool(false))));
    assert_eq!(norm("Checked", true), Ok(None));
    assert_eq!(norm("EnableProposed", 0), Ok(Some(Norm::Bool(false))));
    assert!(norm("Checked", "bof").is_err());
    // Dates.
    let d = |y, m, j| Ok(Some(Norm::Date(chrono::NaiveDate::from_ymd_opt(y, m, j).unwrap())));
    assert_eq!(norm("Released", "2024-05-17"), d(2024, 5, 17));
    assert_eq!(norm("Released", "17/05/2024"), d(2024, 5, 17));
    assert_eq!(norm("Released", "2024-05-17T10:00:00+02:00"), d(2024, 5, 17));
    assert_eq!(norm("Released", "2024-05-17T10:00:00"), d(2024, 5, 17));
    assert!(norm("Released", "31/02/2024").unwrap_err().contains("pas une date"));
    assert!(norm("Released", 2024).is_err());
    // Langue.
    assert_eq!(norm("LanguageISO", "FR"), text("fr"));
    assert_eq!(norm("LanguageISO", "pt_BR"), text("pt-BR"));
    assert!(norm("LanguageISO", "français").unwrap_err().contains("ISO"));
    // Valeurs personnalisées brutes : clés uniques, segments bien formés.
    assert_eq!(norm("CustomValuesStore", ",a=1,b=2"), text(",a=1,b=2"));
    assert!(norm("CustomValuesStore", "a=1,A=2").unwrap_err().contains("double"));
    assert!(norm("CustomValuesStore", "a=1,b").unwrap_err().contains("clé=valeur"));
}

#[test]
fn edit_errors_are_clear() {
    let e = validate(&FieldEdit::set(A, "Rating", 9)).unwrap_err();
    assert_eq!((e.book_id.as_str(), e.field.as_str()), (A, "Rating"));
    assert_eq!(e.to_string(), format!("livre {A}, Rating : « Note » : 9 hors limites (de 0 à 5)"));
    let e = validate(&FieldEdit::set(A, "PageCount", 3)).unwrap_err();
    assert!(e.message.contains("non modifiable"), "{e}");
    assert!(validate(&FieldEdit::set(A, "Couleur", "rouge")).unwrap_err().message.contains("inconnu"));
    assert!(validate(&FieldEdit::set(" ", "Series", "x")).is_err());
    assert!(validate(&FieldEdit::custom_value(A, "", Some("x"))).is_err());
    assert!(validate(&FieldEdit::clear(A, "series")).is_ok());
}

#[test]
fn custom_values_like_comicrack() {
    // `ValuesStore.ToString` : clés triées, virgule devant chaque paire, `=` `,` encodés.
    let s = set_custom_value(Some("b=2,a=1"), "c=d", Some("x,y"));
    assert_eq!(s.as_deref(), Some(",a=1,b=2,c&#61;d=x&#44;y"));
    assert_eq!(custom_value(s.as_deref(), "C=D").as_deref(), Some("x,y"));
    assert_eq!(set_custom_value(s.as_deref(), "B", Some("3")).as_deref(), Some(",a=1,B=3,c&#61;d=x&#44;y"));
    assert_eq!(set_custom_value(Some(",a=1"), "a", None), None);
    assert_eq!(set_custom_value(Some(",a=1"), "a", Some("")), None);
    assert_eq!(custom_values(Some(",a=1,mal,b=x=y,c=")), [("a".into(), "1".into()), ("c".into(), String::new())]);
}

#[test]
fn preview_without_writing() {
    let mut b = book();
    assert!(apply_to_book(&mut b, &FieldEdit::set(A, "Series", "Thanos Quest (VF)")).unwrap());
    assert_eq!(b.series.as_deref(), Some("Thanos Quest (VF)"));
    assert_eq!(b.comic_info_is_dirty, Some(true), "champ du ComicInfo : fichier à mettre à jour");

    let mut b = book();
    assert!(!apply_to_book(&mut b, &FieldEdit::set(A, "Writer", "Jim Starlin")).unwrap(), "même valeur");
    assert!(!apply_to_book(&mut b, &FieldEdit::set(A, "Released", "01/10/1990")).unwrap(), "même jour");
    assert!(!apply_to_book(&mut b, &FieldEdit::clear(A, "Volume")).unwrap(), "déjà vide");
    assert!(!apply_to_book(&mut b, &FieldEdit::custom_value(A, "A", Some("1"))).unwrap());
    assert_eq!(b.comic_info_is_dirty, None);
    // Champs de bibliothèque : pas de ComicInfoIsDirty.
    assert!(apply_to_book(&mut b, &FieldEdit::set(A, "Rating", 5)).unwrap());
    assert!(apply_to_book(&mut b, &FieldEdit::clear(A, "Released")).unwrap());
    assert!(apply_to_book(&mut b, &FieldEdit::custom_value(A, "b", Some("2"))).unwrap());
    assert!(apply_to_book(&mut b, &FieldEdit::set(A, "Checked", false)).unwrap());
    assert_eq!((b.rating, b.released.as_deref(), b.checked), (Some(5.0), None, Some(false)));
    assert_eq!(b.custom_values_store.as_deref(), Some(",a=1,b=2"));
    assert_eq!(b.comic_info_is_dirty, None);
    // Date : minuit, heure locale, format de .NET.
    assert!(apply_to_book(&mut b, &FieldEdit::set(A, "Released", "2024-05-17")).unwrap());
    let r = b.released.clone().unwrap();
    assert!(r.starts_with("2024-05-17T00:00:00") && chrono::DateTime::parse_from_rfc3339(&r).is_ok(), "{r}");
    // Valeur refusée : fiche intacte.
    let before = format!("{b:?}");
    assert!(apply_to_book(&mut b, &FieldEdit::set(A, "Month", 0)).is_err());
    assert_eq!(format!("{b:?}"), before);
    // Lecture des valeurs.
    let f = field("Rating").unwrap();
    assert_eq!((f.get(&b), f.text(&b)), (Some(EditValue::Real(5.0)), "5".to_string()));
    assert_eq!(field("Volume").unwrap().get(&b), None);
}

// ---------------------------------------------------------------------------------------
// Base ComicRust
// ---------------------------------------------------------------------------------------

fn temp_store() -> (tempfile::TempDir, StoreConfig) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = StoreConfig::Sqlite(dir.path().join("data").join(SQLITE_FILE_NAME));
    (dir, cfg)
}

/// Scénario commun SQLite / MySQL (tables `comicrust_*` vides au départ).
pub(crate) fn store_scenario(cfg: &StoreConfig) {
    let mut s = Store::open(cfg).unwrap();
    let mut b2 = book();
    b2.id = B.into();
    b2.missing = Some(true);
    let start = s.upsert(&[book(), b2]).unwrap();

    let edits = [
        FieldEdit::set(A, "Series", "Thanos Quest (VF)"),
        FieldEdit::set(A, "Tags", "cosmique,  thanos , Cosmique"),
        FieldEdit::set(A, "BookPrice", "12,5"),
        FieldEdit::custom_value(A, "b", Some("2")),
        FieldEdit::set(&B.to_uppercase(), "Rating", 2),
        FieldEdit::set(&B.to_uppercase(), "Released", "2024-05-17"),
        FieldEdit::set(ABSENT, "Series", "x"),
    ];
    let r = s.apply_edits(&edits).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.missing, [ABSENT]);
    assert_eq!(r.changed.len(), 2);
    assert_eq!(r.store_counter, Some(start + 1), "compteur incrémenté une fois");
    assert!(!r.is_complete() && r.summary().contains("2 livre(s) modifié(s)"), "{}", r.summary());
    let a = r.changed.iter().find(|b| b.id == A).unwrap();
    assert_eq!(a.series.as_deref(), Some("Thanos Quest (VF)"));
    assert_eq!(a.tags.as_deref(), Some("cosmique, thanos"));
    assert_eq!(a.book_price, Some(12.5));
    assert_eq!(a.custom_values_store.as_deref(), Some(",a=1,b=2"));
    assert_eq!((a.comic_info_is_dirty, a.current_page), (Some(true), Some(4)), "reste de la fiche gardé");
    let b = r.changed.iter().find(|b| b.id == B).unwrap();
    assert_eq!((b.rating, b.comic_info_is_dirty, b.missing), (Some(2.0), None, Some(true)));
    assert!(b.released.as_deref().unwrap().starts_with("2024-05-17"));

    // Relu depuis la base : identique au bilan.
    let db = s.load(None).unwrap();
    assert_eq!(format!("{:?}", db.get(A).unwrap()), format!("{a:?}"));
    assert_eq!(db.rows.get(A), Some(&(start + 1)));

    // Rien à changer : pas d'écriture, compteur inchangé.
    let r = s.apply_edits(&[FieldEdit::set(A, "Series", "Thanos Quest (VF)"), FieldEdit::set(B, "Rating", 2)]).unwrap();
    assert_eq!((r.unchanged.len(), r.changed.len(), r.store_counter), (2, 0, None));
    assert_eq!(s.counters().unwrap().update, start + 1);

    // Une valeur invalide : rien pour ce livre (ni les autres champs), l'autre est écrit.
    let r = s
        .apply_edits(&[
            FieldEdit::set(A, "Title", "Le gant"),
            FieldEdit::set(A, "Month", 14),
            FieldEdit::set(B, "Title", "Le cube"),
        ])
        .unwrap();
    assert_eq!(r.errors.len(), 1);
    assert_eq!((r.errors[0].book_id.as_str(), r.errors[0].field.as_str()), (A, "Month"));
    assert_eq!(r.changed.len(), 1);
    assert_eq!(r.changed[0].title.as_deref(), Some("Le cube"));
    assert_eq!(r.changed[0].comic_info_is_dirty, Some(true));
    assert_eq!(s.get(&[A.into()]).unwrap()[0].title, None);

    // Vider des champs.
    let r = s.apply_edits(&[FieldEdit::clear(A, "Tags"), FieldEdit::custom_value(A, "a", None)]).unwrap();
    assert_eq!(r.changed[0].tags, None);
    assert_eq!(r.changed[0].custom_values_store.as_deref(), Some(",b=2"));

    // Mode via `EditTarget`, et lecture seule.
    let r = apply_edits(EditTarget::Store(&mut s), &[FieldEdit::set(A, "Rating", 1)]).unwrap();
    assert_eq!(r.changed[0].rating, Some(1.0));
    let r =
        apply_edits(EditTarget::ReadOnly, &[FieldEdit::set(A, "Rating", 3), FieldEdit::set(B, "Rating", 9)]).unwrap();
    assert_eq!(r.errors.len(), 2);
    assert!(r.errors.iter().any(|e| e.message == READ_ONLY_MESSAGE));
    assert_eq!(s.get(&[A.into()]).unwrap()[0].rating, Some(1.0));
}

#[test]
fn sqlite_apply_edits() {
    let (_dir, cfg) = temp_store();
    store_scenario(&cfg);
}

// ---------------------------------------------------------------------------------------
// Fichiers
// ---------------------------------------------------------------------------------------

#[test]
fn windows_names() {
    for ok in ["Thanos #1 (1990).cbz", "a", "été 2024.cbr", ".cache", "CONSOLE.cbz", "COM10"] {
        assert!(check_file_name(ok).is_ok(), "{ok}");
    }
    for (bad, why) in [
        ("a:b.cbz", "interdit"),
        ("a?.cbz", "interdit"),
        ("a|b", "interdit"),
        ("a\tb", "U+0009"),
        ("fin.", "point"),
        ("fin ", "espace"),
        ("CON", "réservé"),
        ("nul.cbz", "réservé"),
        ("Com1.txt", "réservé"),
        ("", "vide"),
        ("..", "réservé"),
    ] {
        let e = check_file_name(bad).unwrap_err();
        assert!(e.contains(why), "{bad} : {e}");
    }
    assert!(check_file_name(&"é".repeat(256)).unwrap_err().contains("trop long"));
    assert!(check_path(Path::new(&format!("/bd/{}/x.cbz", "d".repeat(250)))).unwrap_err().contains("trop long"));
    assert!(check_path(Path::new("/bd/Série: 1/x.cbz")).is_err());
    assert!(check_path(Path::new("/bd/Série 1/x.cbz")).is_ok());

    assert_eq!(sanitize_file_name("Batman: Year One"), "Batman - Year One");
    assert_eq!(sanitize_file_name("AC/DC \"live\" <1>?*"), "AC-DC 'live' 1");
    assert_eq!(sanitize_file_name("  fin...  "), "fin");
    assert_eq!(sanitize_file_name("con"), "con_");
    assert_eq!(sanitize_file_name(&"x".repeat(300)).len(), 200);
}

#[test]
fn name_templates() {
    let mut b = book();
    b.volume = Some(2);
    b.year = Some(1990);
    b.month = Some(10);
    b.number = Some("7".into());
    assert_eq!(expand_template(DEFAULT_NAME_TEMPLATE, &b), "Thanos Quest v2 #7 (1990)");
    assert_eq!(expand_template(COMICRACK_EXPORT_TEMPLATE, &b), "Thanos Quest 2 #7 (1990/10)");
    assert_eq!(expand_template("{series} #{number:000}", &b), "Thanos Quest #007");
    assert_eq!(expand_template("{Series}[ - {title}]", &b), "Thanos Quest", "partie optionnelle omise");
    assert_eq!(expand_template("{{a}}-{{absent}}[{{absent}}]", &b), "1-");
    assert_eq!(expand_template(r"\[{year}\]", &b), "[1990]");
    assert_eq!(expand_template("{filename}|{writer}|{rating}", &b), "Thanos Quest 01 (1990)|Jim Starlin|4.5");
    // Règle de ComicRack : une partie imbriquée réussie rend la partie englobante réussie.
    b.year = None;
    b.enable_proposed = Some(false);
    assert_eq!(expand_template("[ ({year}[/{month}])]", &b), " (/10)");
    // Valeurs proposées (nom du fichier) quand la fiche est vide.
    let mut p = LibraryBook { id: A.into(), file: Some("/bd/Warlock 009 (1975).cbz".into()), ..Default::default() };
    assert_eq!(expand_template(DEFAULT_NAME_TEMPLATE, &p), "Warlock #9 (1975)");
    // Chemins : valeurs nettoyées, `/` du modèle = dossiers.
    p.series = Some("AC/DC: Live".into());
    p.publisher = Some("Marvel".into());
    assert_eq!(
        path_from_template("{publisher}/{series}/{series} #{number}", &p).unwrap(),
        "Marvel/AC-DC - Live/AC-DC - Live #9"
    );
    assert_eq!(path_from_template("[{imprint}/]{series}", &p).unwrap(), "AC-DC - Live");
    assert!(path_from_template("{title}", &p).unwrap_err().contains("vide"));
    let t = target_path(Path::new("/srv/bd"), "{publisher}/{series} #{number}", &p).unwrap();
    assert_eq!(t, PathBuf::from("/srv/bd/Marvel/AC-DC - Live #9.cbz"));
}

fn write_file(p: &Path, content: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

/// Scénario de déplacement commun (`dir` : dossier temporaire ; `target` : base où le
/// livre `id` existe, avec son fichier à `dir/BD/a.cbz`).
fn move_scenario(dir: &Path, id: &str, mv: &mut dyn FnMut(&LibraryBook, &Path) -> Result<MoveReport, Error>) {
    let src = dir.join("BD").join("a.cbz");
    let mut b = LibraryBook { id: id.into(), file: Some(src.to_string_lossy().into()), ..Default::default() };

    // Succès, avec création des dossiers.
    let dst = dir.join("Rangé").join("Marvel").join("Thanos #1.cbz");
    let r = mv(&b, &dst).unwrap();
    assert!(!src.exists() && dst.is_file());
    assert_eq!(r.book.file.as_deref(), Some(dst.to_string_lossy().as_ref()));
    assert_eq!(r.book.file_size, Some(5));
    assert!(r.book.file_modified_time.is_some());
    assert_eq!(r.moved.created_dirs, [dir.join("Rangé"), dir.join("Rangé").join("Marvel")]);
    b.file = r.book.file.clone();

    // Destination existante, nom invalide, source absente : refusés, rien ne bouge.
    let other = dir.join("BD").join("autre.cbz");
    write_file(&other, b"autre");
    let e = mv(&b, &other).unwrap_err().to_string();
    assert!(e.contains("existe déjà"), "{e}");
    let e = mv(&b, &dir.join("BD").join("Thanos: 1.cbz")).unwrap_err().to_string();
    assert!(e.contains("interdit"), "{e}");
    let e = mv(&b, &dir.join("BD").join("CON.cbz")).unwrap_err().to_string();
    assert!(e.contains("réservé"), "{e}");
    let ghost = LibraryBook { id: id.into(), file: Some(dir.join("absent.cbz").to_string_lossy().into()), ..b.clone() };
    let e = mv(&ghost, &dir.join("x.cbz")).unwrap_err().to_string();
    assert!(e.contains("introuvable"), "{e}");
    assert!(dst.is_file() && other.is_file());
    assert_eq!(std::fs::read(&other).unwrap(), b"autre");

    // La base échoue (livre absent) : fichier remis à sa place, dossiers créés supprimés.
    let unknown = LibraryBook { id: ABSENT.into(), ..b.clone() };
    let back = dir.join("Nouveau").join("b.cbz");
    let e = mv(&unknown, &back).unwrap_err().to_string();
    assert!(e.contains("remis à sa place"), "{e}");
    assert!(dst.is_file() && !back.exists() && !dir.join("Nouveau").exists());

    // Retour au point de départ (simple renommage).
    let r = mv(&b, &src).unwrap();
    assert!(src.is_file() && !dst.exists());
    assert!(r.moved.created_dirs.is_empty());
}

/// Déplacement avec la base ComicRust (tables `comicrust_*` vides au départ).
pub(crate) fn store_move_scenario(cfg: &StoreConfig) {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("BD").join("a.cbz"), b"12345");
    let mut s = Store::open(cfg).unwrap();
    let mut b = book();
    b.file = Some(dir.path().join("BD").join("a.cbz").to_string_lossy().into());
    b.missing = Some(true);
    s.upsert(&[b]).unwrap();
    move_scenario(dir.path(), A, &mut |b, p| rename_or_move(EditTarget::Store(&mut s), b, p));
    let b = s.get(&[A.into()]).unwrap().remove(0);
    assert_eq!(b.missing, None, "fichier présent : plus « manquant »");
    assert_eq!((b.series.as_deref(), b.current_page), (Some("Thanos Quest"), Some(4)), "fiche gardée");
    let e = rename_or_move(EditTarget::ReadOnly, &b, &dir.path().join("z.cbz")).unwrap_err().to_string();
    assert!(e.contains("lecture seule"), "{e}");
}

#[test]
fn sqlite_rename_or_move() {
    let (_dir, cfg) = temp_store();
    store_move_scenario(&cfg);
}

#[test]
fn copy_between_volumes() {
    let dir = tempfile::tempdir().unwrap();
    let (src, dst) = (dir.path().join("a.cbz"), dir.path().join("b.cbz"));
    let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    write_file(&src, &data);
    let mtime = std::fs::metadata(&src).unwrap().modified().unwrap();
    files::copy_then_delete(&src, &dst).unwrap();
    assert!(!src.exists());
    assert_eq!(std::fs::read(&dst).unwrap(), data);
    assert_eq!(std::fs::metadata(&dst).unwrap().modified().unwrap(), mtime, "date de modification gardée");
    // Destination existante : refus, original gardé.
    write_file(&src, b"x");
    assert!(files::copy_then_delete(&src, &dst).is_err());
    assert!(src.is_file());
    assert_eq!(std::fs::read(&dst).unwrap(), data);
    // Défaire un déplacement par copie.
    std::fs::remove_file(&src).unwrap();
    let m = MovedFile { from: src.clone(), to: dst.clone(), copied: true, created_dirs: vec![] };
    undo_move(&m).unwrap();
    assert!(src.is_file() && !dst.exists());
}

// ---------------------------------------------------------------------------------------
// Base MySQL de ComicRack
// ---------------------------------------------------------------------------------------

const CR_A: &str = r#"<ComicBook Id="aaaaaaaa-0000-4000-8000-000000000001" File="Z:\BD\Thanos Quest 01 (1990).cbz"><Series>Thanos Quest</Series><Number>1</Number><Writer>Jim Starlin</Writer><PageCount>11</PageCount><MyExtra>inconnu</MyExtra><Pages><Page Image="0" Type="FrontCover" /></Pages><Released>1990-10-01T00:00:00</Released><CurrentPage>4</CurrentPage><Rating>4.5</Rating><ColorAdjustment><Saturation>0.2</Saturation></ColorAdjustment><CustomValuesStore>,a=1</CustomValuesStore></ComicBook>"#;

#[test]
fn mysql_comicrack_edits() {
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test d'écriture ComicRack ignoré");
        return;
    };
    use mysql::prelude::Queryable;
    let _lock = crate::db::tests::mysql_lock();
    let cfg = DbConfig::from_connection_string(&url).unwrap();
    let mut conn = crate::db::connect(&cfg).unwrap();
    conn.query_drop("DROP TABLE IF EXISTS comics").unwrap();
    conn.query_drop("DROP TABLE IF EXISTS changes").unwrap();
    // Tables identiques à ComicStorageMySql.CreateTables.
    conn.query_drop("Create table changes (id varchar(40) not null primary key, update_counter bigint default 0, delete_counter bigint default 0)").unwrap();
    conn.query_drop(
        "Create table comics (id varchar(40) not null primary key, update_counter bigint default 0, data mediumtext)",
    )
    .unwrap();
    conn.exec_drop("Insert into changes values (?, 10, 0)", (crate::db::write::STORAGE_GUID,)).unwrap();
    let cr_b = CR_A.replace(A, B);
    for (id, data) in [(A, CR_A), (B, cr_b.as_str())] {
        conn.exec_drop("Insert into comics values (?, 5, ?)", (id, data)).unwrap();
    }
    let data = |conn: &mut mysql::Conn, id: &str| -> String {
        conn.exec_first("SELECT data FROM comics WHERE id = ?", (id,)).unwrap().unwrap()
    };

    let edits = [
        FieldEdit::set(A, "Series", "Thanos Quest (VF)"),
        FieldEdit::set(A, "Tags", "cosmique, thanos"),
        FieldEdit::set(A, "Checked", false),
        FieldEdit::set(A, "BookPrice", 12.5),
        FieldEdit::set(A, "Released", "01/10/1990"), // même jour : inchangé
        FieldEdit::custom_value(A, "b", Some("2")),
        FieldEdit::set(B, "Rating", 5),
        FieldEdit::set(B, "Released", "2024-05-17"),
        FieldEdit::set(ABSENT, "Rating", 1),
    ];
    let r = apply_edits(EditTarget::ComicRack(&cfg), &edits).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.missing, [ABSENT]);
    assert_eq!(r.changed.len(), 2);
    assert_eq!(r.comicrack.as_ref().unwrap().written.len(), 2);
    let xa = data(&mut conn, A);
    for part in [
        "<Series>Thanos Quest (VF)</Series>",
        "<Tags>cosmique, thanos</Tags>",
        r#"Checked="false""#,
        "<BookPrice>12.5</BookPrice>",
        "<ComicInfoIsDirty>true</ComicInfoIsDirty>",
        "<CustomValuesStore>,a=1,b=2</CustomValuesStore>",
        "<Released>1990-10-01T00:00:00</Released>",
        "<MyExtra>inconnu</MyExtra>",
        "<ColorAdjustment><Saturation>0.2</Saturation></ColorAdjustment>",
    ] {
        assert!(xa.contains(part), "{part} absent : {xa}");
    }
    let a = r.changed.iter().find(|b| b.id.eq_ignore_ascii_case(A)).unwrap();
    assert_eq!((a.checked, a.current_page, a.comic_info_is_dirty), (Some(false), Some(4), Some(true)));
    let xb = data(&mut conn, B);
    assert!(xb.contains("<Rating>5</Rating>") && !xb.contains("ComicInfoIsDirty"), "{xb}");
    assert!(xb.contains("<Released>2024-05-17T00:00:00"), "{xb}");
    let (counter,): (i64,) = conn.query_first("SELECT update_counter FROM changes").unwrap().unwrap();
    assert_eq!(counter, 12);

    // Déjà à jour ; valeur refusée ; vider un champ.
    let r = apply_edits(EditTarget::ComicRack(&cfg), &[FieldEdit::set(B, "Rating", "5")]).unwrap();
    assert_eq!((r.unchanged.as_slice(), r.changed.len()), ([B.to_string()].as_slice(), 0));
    let r = apply_edits(EditTarget::ComicRack(&cfg), &[FieldEdit::set(B, "Rating", 6), FieldEdit::clear(A, "Tags")])
        .unwrap();
    assert_eq!(r.errors.len(), 1);
    assert_eq!(r.changed.len(), 1);
    assert!(!data(&mut conn, A).contains("<Tags>"));
    assert!(data(&mut conn, B).contains("<Rating>5</Rating>"));

    // Déplacement de fichier : fiche ComicRack mise à jour ; échec → fichier remis.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("BD").join("a.cbz");
    write_file(&src, b"12345");
    let mut b = LibraryBook::from_xml(&data(&mut conn, A)).unwrap();
    b.file = Some(src.to_string_lossy().into());
    let dst = dir.path().join("Marvel").join("Thanos Quest #1.cbz");
    let r = rename_or_move(EditTarget::ComicRack(&cfg), &b, &dst).unwrap();
    assert_eq!(r.book.file.as_deref(), Some(dst.to_string_lossy().as_ref()));
    assert_eq!(r.book.file_size, Some(5));
    let xa = data(&mut conn, A);
    assert!(xa.contains("<FileSize>5</FileSize>") && xa.contains("<FileModifiedTime>"), "{xa}");
    assert!(xa.contains("<Series>Thanos Quest (VF)</Series>"));
    b.file = r.book.file.clone();
    let unknown = LibraryBook { id: ABSENT.into(), ..b.clone() };
    let e = rename_or_move(EditTarget::ComicRack(&cfg), &unknown, &src).unwrap_err().to_string();
    assert!(e.contains("absent de la base ComicRack") && e.contains("remis à sa place"), "{e}");
    assert!(dst.is_file() && !src.exists());
}

// ---------------------------------------------------------------------------------------
// Normalisation des chemins de renommage
// ---------------------------------------------------------------------------------------

#[test]
fn normalisation_chemins_windows() {
    let cur = r"D:\BD\Batman\Batman 001.cbz";
    let n = |req: &str| normalize_rename(cur, req);
    // Simple nom : même dossier, séparateur de Windows.
    assert_eq!(n("Batman 1.cbz").unwrap(), r"D:\BD\Batman\Batman 1.cbz");
    // Séparateurs mélangés et doublés.
    assert_eq!(n("D:/BD//Batman\\Annuel/x.cbz").unwrap(), r"D:\BD\Batman\Annuel\x.cbz");
    // Relatif avec sous-dossier.
    assert_eq!(n("Annuel/x.cbz").unwrap(), r"D:\BD\Batman\Annuel\x.cbz");
    // Casse de la racine et des dossiers existants gardée (ComicRack compare le texte exact) ;
    // le nouveau dossier et le nom gardent la casse demandée.
    assert_eq!(n(r"d:\bd\BATMAN\Nouveau\X.cbz").unwrap(), r"D:\BD\Batman\Nouveau\X.cbz");
    // Autre lecteur : lettre en majuscule, rien d'autre de changé.
    assert_eq!(n(r"e:\bd\x.cbz").unwrap(), r"E:\bd\x.cbz");
    // Refusés.
    for (bad, why) in [
        (r"D:\BD\..\BD\x.cbz", "« .. »"),
        (r"..\x.cbz", "« .. »"),
        (r".\x.cbz", "« . »"),
        (r"\BD\x.cbz", "sans lecteur"),
        ("/BD/x.cbz", "sans lecteur"),
        (r"D:x.cbz", "relatif à un lecteur"),
        (r"\\?\D:\BD\x.cbz", "non pris en charge"),
        (r"\\serveur", "incomplet"),
        ("  ", "vide"),
        (r"D:\", "manquant"),
    ] {
        let e = n(bad).unwrap_err();
        assert!(e.contains(why), "{bad} : {e}");
    }
}

#[test]
fn normalisation_chemins_unc() {
    let cur = r"\\SERVEURNAS\BD\Marvel\Thanos\Thanos 01.cbz";
    let n = |req: &str| normalize_rename(cur, req);
    // Même partage écrit autrement : le préfixe du livre est gardé exactement.
    assert_eq!(
        n(r"\\serveurnas\bd\marvel\Thanos Quest\01.cbz").unwrap(),
        r"\\SERVEURNAS\BD\Marvel\Thanos Quest\01.cbz"
    );
    assert_eq!(n("//SERVEURNAS/BD/Marvel/x.cbz").unwrap(), r"\\SERVEURNAS\BD\Marvel\x.cbz");
    assert_eq!(n("Thanos 1.cbz").unwrap(), r"\\SERVEURNAS\BD\Marvel\Thanos\Thanos 1.cbz");
    // Autre partage : écrit tel que demandé (séparateurs normalisés).
    assert_eq!(n(r"\\autre\Partage/x.cbz").unwrap(), r"\\autre\Partage\x.cbz");
    // Vers un lecteur depuis un partage.
    assert_eq!(n(r"z:/BD/x.cbz").unwrap(), r"Z:\BD\x.cbz");
    assert!(n(r"\\SERVEURNAS\BD\Marvel\..\x.cbz").unwrap_err().contains(".."));
    assert!(n(r"\BD\x.cbz").unwrap_err().contains("sans lecteur"));
    // Lettre de lecteur d'origine en minuscule : gardée telle quelle.
    assert_eq!(normalize_rename(r"d:\BD\a.cbz", r"D:\bd\b.cbz").unwrap(), r"d:\BD\b.cbz");
}

#[cfg(not(windows))]
#[test]
fn normalisation_chemins_unix() {
    let cur = "/srv/bd/Batman/Batman 001.cbz";
    let n = |req: &str| normalize_rename(cur, req);
    assert_eq!(n("Batman 1.cbz").unwrap(), "/srv/bd/Batman/Batman 1.cbz");
    assert_eq!(n("/srv//bd/Autre/x.cbz").unwrap(), "/srv/bd/Autre/x.cbz");
    assert_eq!(n("Annuel\\x.cbz").unwrap(), "/srv/bd/Batman/Annuel/x.cbz", "\\ = séparateur");
    // Casse respectée (Unix : noms sensibles à la casse).
    assert_eq!(n("/srv/BD/x.cbz").unwrap(), "/srv/BD/x.cbz");
    assert!(n("/srv/bd/../x.cbz").unwrap_err().contains(".."));
    assert!(n("\\srv\\x.cbz").unwrap_err().contains("sans lecteur"));
    // check_path refuse aussi `..` et `.`.
    assert!(check_path(Path::new("/srv/bd/../x.cbz")).is_err());
    assert!(check_path(Path::new("/srv/./x.cbz")).is_err());
}

/// La destination apparue entre le contrôle et le renommage n'est jamais remplacée.
#[test]
fn renommage_sans_remplacement() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.cbz"), dir.path().join("b.cbz"));
    write_file(&a, b"source");
    write_file(&b, b"autre livre");
    let e = rename_no_replace(&a, &b).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists, "{e}");
    assert_eq!(std::fs::read(&b).unwrap(), b"autre livre");
    assert_eq!(std::fs::read(&a).unwrap(), b"source");
    let c = dir.path().join("c.cbz");
    rename_no_replace(&a, &c).unwrap();
    assert!(!a.exists());
    assert_eq!(std::fs::read(&c).unwrap(), b"source");
}

/// `rename_or_move` écrit dans la fiche le chemin normalisé.
#[test]
fn renommage_normalise_dans_la_fiche() {
    let (_d, cfg) = temp_store();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("BD").join("a.cbz");
    write_file(&src, b"12345");
    let mut s = Store::open(&cfg).unwrap();
    let mut b = book();
    b.file = Some(src.to_string_lossy().into());
    s.upsert(std::slice::from_ref(&b)).unwrap();
    let sep = std::path::MAIN_SEPARATOR;
    let r = rename_or_move(EditTarget::Store(&mut s), &b, Path::new("Série//b.cbz")).unwrap();
    let expected = format!("{}{sep}BD{sep}Série{sep}b.cbz", dir.path().display());
    assert_eq!(r.book.file.as_deref(), Some(expected.as_str()));
    assert!(Path::new(&expected).is_file());
    let e = rename_or_move(EditTarget::Store(&mut s), &r.book, Path::new("../c.cbz")).unwrap_err().to_string();
    assert!(e.contains(".."), "{e}");
}
