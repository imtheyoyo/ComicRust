//! Tests de l'aperçu et de l'application des modifications des scripts.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use comicrust_core::db::{ComicDatabase, DbConfig};
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::ScriptChange;
use comicrust_core::store::{Store, StoreConfig};
use serde_json::{json, Value};

use super::*;
use crate::library::LibrarySource;
use crate::settings::LibraryMode;

const A: &str = "11111111-aaaa-4000-8000-000000000001";
const B: &str = "11111111-aaaa-4000-8000-000000000002";
const C: &str = "11111111-aaaa-4000-8000-000000000003";

fn book(id: &str, file: &Path, series: &str) -> LibraryBook {
    LibraryBook {
        id: id.into(),
        file: Some(file.to_string_lossy().into_owned()),
        series: Some(series.into()),
        number: Some("1".into()),
        year: Some(1990),
        ..Default::default()
    }
}

fn set(id: &str, field: &str, value: Value) -> ScriptChange {
    ScriptChange::SetField { book_id: id.into(), field: field.into(), value }
}

fn rename(id: &str, path: &str) -> ScriptChange {
    ScriptChange::Rename { book_id: id.into(), new_path: path.into() }
}

/// Dossier avec trois CBZ (a, b, c) et la bibliothèque correspondante.
fn fixture() -> (tempfile::TempDir, ComicDatabase) {
    let dir = tempfile::tempdir().unwrap();
    let mut books = Vec::new();
    for (id, name, series) in [(A, "a.cbz", "Thanos"), (B, "b.cbz", "Warlock"), (C, "c.cbr", "Silver Surfer")] {
        let f = dir.path().join(name);
        std::fs::write(&f, b"PK").unwrap();
        books.push(book(id, &f, series));
    }
    (dir, ComicDatabase::from_books(books))
}

#[test]
fn apercu_avant_apres() {
    let (dir, db) = fixture();
    let changes = vec![
        set(A, "Series", json!("Thanos (brouillon)")),
        set(A, "Series", json!("Thanos Quest")), // la dernière valeur l'emporte
        set(A, "Year", json!(1990)),             // inchangé : pas de ligne
        set(A, "CustomValue:source", json!("cv")),
        set(B, "Series", json!("Warlock")), // rien ne change pour B
        set(C, "Count", json!(-5)),         // valeur refusée
        ScriptChange::WriteInfo { book_id: C.into() },
        set("absent", "Series", json!("X")),
    ];
    let p = Preview::build(&changes, &db);
    assert_eq!(p.unchanged, 1, "B déjà dans l'état demandé");
    assert_eq!(p.missing, ["absent"]);
    assert_eq!(p.books.len(), 2);
    let a = &p.books[0];
    assert_eq!(a.id, A);
    let rows: Vec<(&str, &str, &str)> =
        a.fields.iter().map(|f| (f.label.as_str(), f.before.as_str(), f.after.as_str())).collect();
    assert_eq!(rows, [("Série", "Thanos", "Thanos Quest"), ("Valeur perso. « source »", "", "cv")]);
    assert!(a.checked && a.applicable());
    let c = &p.books[1];
    assert!(c.fields[0].error.is_some(), "Count négatif refusé");
    assert!(matches!(&c.write_info, Some(Some(m)) if m.contains("CBZ")), "CBR : pas d'infos");
    assert!(!c.checked && !c.applicable(), "rien d'applicable : décoché");
    assert_eq!(p.counts(), (2, 2, 0, 0, 2));
    drop(dir);
}

#[test]
fn apercu_des_renommages_et_conflits() {
    let (dir, db) = fixture();
    let d = dir.path();
    std::fs::write(d.join("pris.cbz"), b"x").unwrap();
    let changes = vec![
        rename(A, "Thanos 001.cbz"),                      // relatif : même dossier
        rename(B, &d.join("pris.cbz").to_string_lossy()), // destination existante
        rename(C, &d.join("sous").join("Thanos 001.cbz").to_string_lossy()),
        set(C, "Series", json!("Thanos")),
    ];
    let p = Preview::build(&changes, &db);
    let a = p.books[0].rename.as_ref().unwrap();
    assert_eq!(a.to, d.join("Thanos 001.cbz"));
    assert_eq!(rename_texts(&a.from, &a.to), ("a.cbz".to_string(), "Thanos 001.cbz".to_string()));
    assert!(a.problem.is_none());
    let b = p.books[1].rename.as_ref().unwrap();
    assert_eq!(b.problem.as_deref(), Some("la destination existe déjà"));
    assert!(!p.books[1].checked);
    let c = p.books[2].rename.as_ref().unwrap();
    assert!(c.problem.is_none(), "autre dossier : pas de conflit");
    assert_eq!(rename_texts(&c.from, &c.to).1, d.join("sous").join("Thanos 001.cbz").display().to_string());
    // Deux livres vers le même fichier : le second est signalé.
    let same = vec![rename(A, "x.cbz"), rename(B, "x.cbz"), rename(C, "a.cbz")];
    let p = Preview::build(&same, &db);
    assert!(p.books[0].rename.as_ref().unwrap().problem.is_none());
    assert!(p.books[1].rename.as_ref().unwrap().problem.as_deref().unwrap().contains("même destination"));
    assert!(p.books[2].rename.as_ref().unwrap().problem.is_some(), "a.cbz existe");
    // Fichier introuvable.
    std::fs::remove_file(d.join("a.cbz")).unwrap();
    let p = Preview::build(&[rename(A, "y.cbz")], &db);
    assert_eq!(p.books[0].rename.as_ref().unwrap().problem.as_deref(), Some("fichier introuvable"));
    // Même chemin : rien à faire.
    let p = Preview::build(&[rename(B, "b.cbz")], &db);
    assert_eq!((p.books.len(), p.unchanged), (0, 1));
}

#[test]
fn cases_a_cocher_et_plan() {
    let (dir, db) = fixture();
    let changes = vec![
        set(A, "Series", json!("Thanos Quest")),
        ScriptChange::WriteInfo { book_id: A.into() },
        set(B, "Series", json!("Adam Warlock")),
        rename(B, "Adam Warlock 01.cbz"),
        set(C, "Series", json!("Surfer")),
        set(C, "Count", json!(-1)), // -1 : vide le champ (valeur par défaut), accepté
        set(C, "Month", json!(13)), // refusé, le reste du livre s'applique
    ];
    let mut p = Preview::build(&changes, &db);
    assert_eq!(p.checked(), 3);
    p.books[1].checked = false;
    let plan = p.plan(&db);
    let fields: Vec<(&str, &str)> = plan.edits.iter().map(|e| (e.book_id.as_str(), e.field.as_str())).collect();
    assert_eq!(fields, [(A, "Series"), (C, "Series")], "B décoché ; Month refusé non envoyé");
    assert!(plan.renames.is_empty());
    assert_eq!(plan.write_info, [A]);
    p.set_all(false);
    assert!(p.plan(&db).is_empty());
    p.set_all(true);
    let plan = p.plan(&db);
    assert_eq!(plan.renames.len(), 1);
    assert_eq!(plan.renames[0].1, dir.path().join("Adam Warlock 01.cbz"));
}

#[test]
fn conversion_des_modifications_au_fil_de_l_eau() {
    let (dir, db) = fixture();
    let changes = vec![
        set(A, "Year", json!(2001)),
        set(A, "Rating", json!(4.5)),
        set(A, "Summary", Value::Null),
        rename(A, "premier.cbz"),
        rename(A, "second.cbz"),
        ScriptChange::WriteInfo { book_id: A.into() },
        ScriptChange::WriteInfo { book_id: A.into() },
        rename("inconnu", "z.cbz"),
    ];
    let plan = ApplyPlan::from_changes(&changes, |id| db.get(id).cloned());
    use comicrust_core::edit::EditValue;
    let values: Vec<Option<EditValue>> = plan.edits.iter().map(|e| e.value.clone()).collect();
    assert_eq!(values, [Some(EditValue::Int(2001)), Some(EditValue::Real(4.5)), None]);
    assert_eq!(plan.renames.len(), 1, "la dernière demande l'emporte, livre inconnu ignoré");
    assert_eq!(plan.renames[0].1, dir.path().join("second.cbz"));
    assert_eq!(plan.write_info, [A]);
}

#[test]
fn lecture_seule_refusee() {
    let mysql = DbConfig::default();
    let src = LibrarySource::ComicRackMySql(mysql.clone());
    assert_eq!(ApplyTarget::of(LibraryMode::ComicRackMySql, true, Some(&src)), ApplyTarget::ComicRack(mysql));
    let ro = ApplyTarget::of(LibraryMode::ComicRackMySql, false, Some(&src));
    assert!(ro.read_only().unwrap().starts_with("Lecture seule : active l'écriture"));
    let xml = LibrarySource::ComicRackXml("ComicDb.xml".into());
    assert!(ApplyTarget::of(LibraryMode::ComicRackXml, true, Some(&xml)).read_only().unwrap().contains("ComicDb.xml"));
    assert!(ApplyTarget::of(LibraryMode::Standalone, false, None).read_only().is_some());
    let store = StoreConfig::Sqlite("x.db".into());
    let st = LibrarySource::Standalone(store.clone());
    assert_eq!(ApplyTarget::of(LibraryMode::Standalone, false, Some(&st)), ApplyTarget::Store(store));

    // Rien n'est touché : ni fiche ni fichier.
    let (dir, db) = fixture();
    let plan = ApplyPlan {
        edits: vec![comicrust_core::scripting::field_edit(A, "Series", &json!("X"))],
        renames: vec![(db.get(A).unwrap().clone(), dir.path().join("x.cbz"))],
        write_info: vec![A.into()],
    };
    let out = apply_plan(&ro, &plan, &AtomicBool::new(false));
    assert!(out.fatal.as_deref().unwrap().starts_with("Lecture seule"));
    assert!(out.edit.is_none() && out.moves.is_empty() && out.write_info.is_empty());
    assert!(dir.path().join("a.cbz").is_file() && !dir.path().join("x.cbz").exists());
    assert!(!out.is_complete());
}

#[test]
fn application_dans_la_base_comicrust() {
    let (dir, db) = fixture();
    let cfg = StoreConfig::Sqlite(dir.path().join("data").join("comicrust.db"));
    Store::open(&cfg).unwrap().upsert(&db.books).unwrap();
    let changes = vec![
        set(A, "Series", json!("Thanos Quest")),
        rename(A, "Thanos Quest 01.cbz"),
        ScriptChange::WriteInfo { book_id: A.into() },
        set(B, "Series", json!("Warlock")), // inchangé
        set(C, "Year", json!(1991)),
    ];
    let preview = Preview::build(&changes, &db);
    let plan = preview.plan(&db);
    let out = apply_plan(&ApplyTarget::Store(cfg.clone()), &plan, &AtomicBool::new(false));
    assert!(out.is_complete(), "{:?}", out.problems(|s| s.to_string()));
    assert_eq!(out.modified(), 2);
    assert_eq!(out.moves.len(), 1);
    let new = dir.path().join("Thanos Quest 01.cbz");
    assert!(new.is_file() && !dir.path().join("a.cbz").exists());
    assert_eq!(out.write_info, [A]);
    // Fiches relues : la plus récente (après renommage) garde la série modifiée.
    let fresh = out.fresh_books();
    let a = fresh.iter().find(|b| b.id.eq_ignore_ascii_case(A)).unwrap();
    assert_eq!(a.series.as_deref(), Some("Thanos Quest"));
    assert_eq!(PathBuf::from(a.file.as_deref().unwrap()), new);
    // Écritures à signaler à la surveillance (compteurs de la base).
    let written = out.written();
    assert!(written.iter().any(|(id, _)| id == &A.to_lowercase()));
    assert!(written.iter().any(|(id, _)| id == &C.to_lowercase()));
    // Gardé après relecture de la base.
    let reloaded = Store::open_existing(&cfg).unwrap().load(None).unwrap();
    assert_eq!(reloaded.get(A).unwrap().series.as_deref(), Some("Thanos Quest"));
    assert_eq!(reloaded.get(C).unwrap().year, Some(1991));
    assert!(out.summary().starts_with("2 livre(s) modifié(s)"), "{}", out.summary());
}

/// Chemins demandés par un script : normalisés dans l'aperçu (c'est ce texte qui sera
/// écrit dans la fiche) ; `..`, `.` et chemins Windows sans lecteur refusés.
#[test]
fn apercu_chemins_normalises() {
    let (dir, db) = fixture();
    let d = dir.path();
    let sep = std::path::MAIN_SEPARATOR;
    let p = Preview::build(&[rename(A, "sous//Thanos 001.cbz"), rename(B, "../b.cbz"), rename(C, r"\BD\c.cbr")], &db);
    let a = p.books[0].rename.as_ref().unwrap();
    assert!(a.problem.is_none(), "{:?}", a.problem);
    assert_eq!(a.to.to_string_lossy(), format!("{}{sep}sous{sep}Thanos 001.cbz", d.display()));
    let b = p.books[1].rename.as_ref().unwrap();
    assert!(b.problem.as_deref().unwrap().contains(".."), "{b:?}");
    assert!(!p.books[1].checked);
    let c = p.books[2].rename.as_ref().unwrap();
    assert!(c.problem.as_deref().unwrap().contains("sans lecteur"), "{c:?}");
    // Automatisme : même normalisation.
    let plan = ApplyPlan::from_changes(&[rename(A, "x//y.cbz")], |id| db.get(id).cloned());
    assert_eq!(plan.renames[0].1.to_string_lossy(), format!("{}{sep}x{sep}y.cbz", d.display()));
    // Chemins de style Windows (calcul seul) : séparateurs, casse de la racine d'origine.
    assert_eq!(
        resolve_rename(Path::new(r"\\NAS\BD\a.cbz"), "//nas/bd/Sous/b.cbz").unwrap(),
        PathBuf::from(r"\\NAS\BD\Sous\b.cbz")
    );
}

/// Arrêt demandé (fermeture) : vérifié entre deux livres ; les renommages restants ne sont
/// pas faits, ceux faits ont leur fiche à jour.
#[test]
fn arret_entre_deux_livres() {
    let (dir, db) = fixture();
    let cfg = StoreConfig::Sqlite(dir.path().join("data").join("comicrust.db"));
    Store::open(&cfg).unwrap().upsert(&db.books).unwrap();
    let plan =
        ApplyPlan::from_changes(&[set(A, "Series", json!("X")), rename(A, "a2.cbz"), rename(B, "b2.cbz")], |id| {
            db.get(id).cloned()
        });
    let out = apply_plan(&ApplyTarget::Store(cfg.clone()), &plan, &AtomicBool::new(true));
    assert_eq!(out.interrupted, 2);
    assert!(out.moves.is_empty() && !out.is_complete());
    assert!(out.edit.as_ref().is_some_and(|r| r.changed.len() == 1), "fiches écrites d'un seul tenant");
    assert!(dir.path().join("a.cbz").is_file() && dir.path().join("b.cbz").is_file());
    assert!(out.summary().contains("non fait"), "{}", out.summary());
}
