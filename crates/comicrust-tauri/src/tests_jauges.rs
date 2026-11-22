//! Tests des jauges des listes (`jauges.rs`, contrat `docs/tauri/ipc-jauges.md`) : définition de
//! « nouveau » et « non lu », listes de lecture, intelligentes, dossiers, listes vides, contenu
//! inconnu, livres manquants (comptés comme dans `nbLivres`), identifiants absents, cache.

use crate::biblio::{Biblio, ChoixPremierDemarrage};
use crate::jauges::{calculer, etat_livre, Jauge, JaugesListes, JOURS_NOUVEAU};
use crate::listes::{DemandeCreation, TypeNouvelle};
use crate::reglages::Mode;
use chrono::NaiveDate;
use comicrust_core::comic_lists::{resolve_all, Combine, ComicList, ListKind};
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::smart::{Matcher, SmartQuery};
use comicrust_core::store::{Store, StoreConfig};
use std::path::Path;
use std::sync::Arc;

fn aujourdhui() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 22).unwrap()
}

/// Date d'ajout « il y a `n` jours ».
fn il_y_a(n: i64) -> Option<String> {
    Some(format!("{}T12:00:00+01:00", aujourdhui() - chrono::Duration::days(n)))
}

fn livre(id: &str, serie: &str, ajoute: Option<String>, derniere: Option<u32>) -> LibraryBook {
    LibraryBook {
        id: id.into(),
        file: Some(format!("/BD/{id}.cbz")),
        series: Some(serie.into()),
        page_count: Some(20),
        added: ajoute,
        last_page_read: derniere,
        current_page: derniere,
        ..Default::default()
    }
}

fn liste(id: &str, kind: ListKind) -> ComicList {
    ComicList { id: id.into(), name: id.into(), description: String::new(), kind, comicrack_count: 0 }
}

fn ids(v: &[&str]) -> ListKind {
    ListKind::Ids(v.iter().map(|s| s.to_lowercase()).collect())
}

fn j(nouveaux: usize, non_lus: usize, total: usize) -> Jauge {
    Jauge { nouveaux, non_lus, total }
}

/// Six livres : a (nouveau), b (nouveau), c (ancien non lu), d (en cours, récent), e (lu), f (manquant,
/// récent, non lu).
fn base() -> ComicDatabase {
    let mut f = livre("f", "Warlock", il_y_a(1), None);
    f.missing = Some(true);
    ComicDatabase::from_books(vec![
        livre("a", "Thanos", il_y_a(0), None),
        livre("b", "Thanos", il_y_a(JOURS_NOUVEAU), Some(0)),
        livre("c", "Thanos", il_y_a(JOURS_NOUVEAU + 1), None),
        livre("d", "Warlock", il_y_a(2), Some(5)),
        livre("e", "Warlock", il_y_a(3), Some(19)),
        f,
    ])
}

fn jauges(listes: &[ComicList], base: &ComicDatabase) -> JaugesListes {
    calculer(base, &resolve_all(listes, base), aujourdhui())
}

#[test]
fn definition_de_nouveau_et_non_lu() {
    let seuil = aujourdhui() - chrono::Duration::days(JOURS_NOUVEAU);
    let e = |b: &LibraryBook| etat_livre(b, seuil);
    // Récent, jamais ouvert : nouveau et non lu.
    assert_eq!(e(&livre("x", "S", il_y_a(0), None)), (true, true));
    // Le dernier jour compte, le lendemain non.
    assert_eq!(e(&livre("x", "S", il_y_a(JOURS_NOUVEAU), None)), (true, true));
    assert_eq!(e(&livre("x", "S", il_y_a(JOURS_NOUVEAU + 1), None)), (false, true));
    // Sans date d'ajout (ou « jamais » de ComicRack) : jamais nouveau.
    assert_eq!(e(&livre("x", "S", None, None)), (false, true));
    assert_eq!(e(&livre("x", "S", Some("0001-01-01T00:00:00".into()), None)), (false, true));
    // Lecture commencée : plus nouveau, encore non lu ; lu : ni l'un ni l'autre.
    assert_eq!(e(&livre("x", "S", il_y_a(0), Some(3))), (false, true));
    assert_eq!(e(&livre("x", "S", il_y_a(0), Some(19))), (false, false));
    // Ouvert (date d'ouverture ou compteur) : plus nouveau.
    let mut ouvert = livre("x", "S", il_y_a(0), None);
    ouvert.opened = Some("2026-11-21T08:00:00+01:00".into());
    assert_eq!(e(&ouvert), (false, true));
    let mut compte = livre("x", "S", il_y_a(0), None);
    compte.open_count = Some(2);
    assert_eq!(e(&compte), (false, true));
    // Nombre de pages inconnu : pas de pourcentage, donc non lu.
    let mut inconnu = livre("x", "S", il_y_a(0), None);
    inconnu.page_count = None;
    assert_eq!(e(&inconnu), (true, true));
}

#[test]
fn racine_et_bibliotheque() {
    let b = base();
    let r = jauges(&[liste("lib", ListKind::Library)], &b);
    // Nouveaux : a, b (7 jours) et f (manquant : compté comme les autres) ; non lus : tous sauf e.
    assert_eq!(r.racine, j(3, 5, 6));
    assert_eq!(r.listes["lib"], r.racine);
}

#[test]
fn liste_de_lecture_et_identifiants_absents() {
    let b = base();
    // « Z » n'est pas dans la base : ignoré, comme dans `nbLivres`. Casse de l'identifiant libre.
    let r = jauges(&[liste("L", ids(&["A", "c", "d", "e", "z"]))], &b);
    assert_eq!(r.listes["L"], j(1, 3, 4));
}

#[test]
fn liste_vide_et_dossier_vide() {
    let b = base();
    let l = [liste("vide", ids(&[])), liste("dossier", ListKind::Folder { combine: Combine::Or, children: vec![] })];
    let r = jauges(&l, &b);
    assert_eq!(r.listes["vide"], j(0, 0, 0));
    assert_eq!(r.listes["dossier"], j(0, 0, 0));
}

#[test]
fn livres_manquants_comptes_comme_le_total_affiche() {
    let b = base();
    let r = jauges(&[liste("L", ids(&["f", "e"]))], &b);
    // f : manquant mais présent dans la base (comme `nbLivres`), récent, jamais ouvert.
    assert_eq!(r.listes["L"], j(1, 1, 2));
}

#[test]
fn liste_intelligente_et_dossier() {
    let b = base();
    let q = SmartQuery {
        matchers: vec![Matcher::value("ComicBookSeriesMatcher", 0, "Thanos", "", false, true)],
        ..Default::default()
    };
    let intelligente = liste("S", ListKind::Smart { query: Some(q), cached: None });
    let dossier = liste(
        "D",
        ListKind::Folder { combine: Combine::Or, children: vec![intelligente.clone(), liste("L", ids(&["d", "e"]))] },
    );
    let r = jauges(&[dossier, liste("I", ids(&["a", "d"]))], &b);
    assert_eq!(r.listes["S"], j(2, 3, 3), "Thanos : a, b nouveaux ; a, b, c non lus");
    assert_eq!(r.listes["L"], j(0, 1, 2));
    assert_eq!(r.listes["D"], j(2, 4, 5), "réunion Thanos + d, e");
    assert_eq!(r.listes["I"], j(1, 2, 2));
}

#[test]
fn contenu_inconnu_sans_jauge() {
    let b = base();
    // Liste intelligente sans critères connus ni résultat mémorisé : contenu inconnu, pas de jauge
    // (le front n'affiche rien plutôt qu'un faux zéro).
    let r = jauges(&[liste("X", ListKind::Smart { query: None, cached: None })], &b);
    assert!(!r.listes.contains_key("X"));
    // Résultat mémorisé par ComicRack : utilisé.
    let r = jauges(&[liste("X", ListKind::Smart { query: None, cached: Some(vec!["a".into(), "e".into()]) })], &b);
    assert_eq!(r.listes["X"], j(1, 1, 2));
}

#[test]
fn bibliotheque_vide() {
    let b = ComicDatabase::from_books(vec![]);
    let r = jauges(&[liste("lib", ListKind::Library), liste("L", ids(&["a"]))], &b);
    assert_eq!(r.racine, j(0, 0, 0));
    assert_eq!(r.listes["lib"], j(0, 0, 0));
    assert_eq!(r.listes["L"], j(0, 0, 0));
}

#[test]
fn cinq_mille_livres_cent_listes() {
    let livres: Vec<LibraryBook> = (0..5000)
        .map(|i| livre(&format!("{i:05}"), "Série", il_y_a((i % 20) as i64), (i % 3 == 0).then_some(19)))
        .collect();
    let b = ComicDatabase::from_books(livres);
    let listes: Vec<ComicList> = (0..100)
        .map(|k| {
            let membres: Vec<String> = (0..5000).filter(|i| i % 100 == k).map(|i| format!("{i:05}")).collect();
            liste(&format!("L{k}"), ListKind::Ids(membres))
        })
        .collect();
    let r = jauges(&listes, &b);
    assert_eq!(r.racine.total, 5000);
    assert_eq!(r.listes.values().map(|x| x.total).sum::<usize>(), 5000);
    assert_eq!(r.listes.values().map(|x| x.non_lus).sum::<usize>(), r.racine.non_lus);
    assert_eq!(r.listes.values().map(|x| x.nouveaux).sum::<usize>(), r.racine.nouveaux);
}

#[test]
fn serialisation_camel_case() {
    let r = JaugesListes { racine: j(1, 2, 3), listes: [("L".to_string(), j(0, 1, 1))].into() };
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["racine"], serde_json::json!({"nouveaux": 1, "nonLus": 2, "total": 3}));
    assert_eq!(v["listes"]["L"]["nonLus"], 1);
}

// ───────────────────────────── Avec une vraie bibliothèque (cache) ─────────────────────────────

fn biblio_autonome(dir: &Path) -> Biblio {
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let b = Biblio::nouvelle(data.clone());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    let mut s = Store::open_existing(&StoreConfig::Sqlite(data.join("comicrust.db"))).unwrap();
    let aujourd = chrono::Local::now().date_naive();
    let ajoute = Some(format!("{aujourd}T08:00:00+01:00"));
    let v: Vec<LibraryBook> = ["a", "b", "c"]
        .iter()
        .map(|i| livre(&format!("{i}0000000-0000-4000-8000-000000000000"), "Thanos", ajoute.clone(), None))
        .collect();
    s.upsert(&v).unwrap();
    b.ouvrir().unwrap();
    b
}

fn id(c: &str) -> String {
    format!("{c}0000000-0000-4000-8000-000000000000")
}

#[test]
fn biblio_cache_invalide_par_lecture_et_listes() {
    let d = tempfile::tempdir().unwrap();
    let b = biblio_autonome(d.path());
    let j1 = b.jauges_listes().unwrap();
    assert_eq!(j1.racine, j(3, 3, 3));
    assert!(j1.listes.is_empty());
    assert!(Arc::ptr_eq(&j1, &b.jauges_listes().unwrap()), "même résultat tant que rien ne change");

    // Une liste de lecture est créée : les jauges la contiennent (version des listes changée).
    let r = b
        .creer_liste(&DemandeCreation {
            type_liste: TypeNouvelle::Lecture,
            nom: Some("Lecture".into()),
            parent: None,
            position: None,
            livres: Some(vec![id("a"), id("b")]),
            apres_liste: None,
        })
        .unwrap();
    let lid = r.creee.unwrap();
    let j2 = b.jauges_listes().unwrap();
    assert_eq!(j2.listes[&lid], j(2, 2, 2));

    // Un livre est lu en entier : fiche remplacée (comme après une lecture ou « Marquer comme lu »).
    let mut lu = b.base().unwrap().get(&id("a")).unwrap().clone();
    lu.last_page_read = Some(19);
    lu.current_page = Some(19);
    assert_eq!(b.remplacer_fiches(&[lu], false).len(), 1);
    let j3 = b.jauges_listes().unwrap();
    assert!(!Arc::ptr_eq(&j2, &j3));
    assert_eq!(j3.racine, j(2, 2, 3));
    assert_eq!(j3.listes[&lid], j(1, 1, 2));
}

#[test]
fn biblio_non_ouverte_refusee() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    assert!(b.jauges_listes().is_err());
}
