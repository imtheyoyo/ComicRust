//! Tests de « Marquer comme lu / non lu » : base autonome SQLite (écriture, relecture depuis la
//! base, avancement local aligné), livres déjà dans l'état demandé, pages inconnues, et ComicRack
//! en lecture seule (refus, rien d'écrit).

use crate::avancement::{Avancement, Memoire};
use crate::biblio::{Biblio, ChoixPremierDemarrage};
use crate::edition::Edition;
use crate::lecteur::Lecteur;
use crate::lecture_etat::{marquer_lecture, pages_cibles, LivreIgnore};
use crate::reglages::Mode;
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::store::Store;
use std::path::Path;

fn livre(id: &str, pages: Option<u32>, derniere: Option<u32>) -> LibraryBook {
    LibraryBook {
        id: id.into(),
        file: Some(format!("/BD/{id}.cbz")),
        series: Some("Thanos".into()),
        page_count: pages,
        last_page_read: derniere,
        current_page: derniere,
        ..Default::default()
    }
}

/// Bibliothèque autonome contenant `livres`.
fn autonome(data: &Path, livres: Vec<LibraryBook>) -> (Biblio, Edition, Lecteur) {
    let b = Biblio::nouvelle(data.to_path_buf());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    let cfg = b.store_autonome().unwrap();
    Store::open_existing(&cfg).unwrap().upsert(&livres).unwrap();
    b.ouvrir().unwrap();
    let l = Lecteur::nouveau(data.to_path_buf(), b.clone());
    let e = Edition::nouvelle(b.clone(), l.clone());
    (b, e, l)
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn pages_a_ecrire() {
    assert_eq!(pages_cibles(true, 20), (19, 19));
    assert_eq!(pages_cibles(true, 1), (0, 0));
    assert_eq!(pages_cibles(false, 20), (0, 0));
}

#[test]
fn marquer_lu_puis_non_lu_en_mode_autonome() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let (b, e, l) =
        autonome(&data, vec![livre("a", Some(20), None), livre("b", Some(10), Some(4)), livre("c", Some(8), Some(7))]);
    // Avancement local plus avancé que la base pour « b » : doit disparaître avec « non lu ».
    l.fixer_avancement(Path::new("/BD/b.cbz"), Some(Avancement { page: 8, page_max: 8, pages: 10, relecture: false }))
        .unwrap();

    let bilan = marquer_lecture(&b, &e, &l, &ids(&["a", "b", "c", "a", "inconnu"]), true).unwrap();
    assert_eq!(bilan.modifies.len(), 2, "{bilan:?}");
    assert_eq!(bilan.inchanges, 1, "« c » est déjà lu");
    assert!(bilan.ignores.is_empty());
    for (id, n) in [("a", 20), ("b", 10), ("c", 8)] {
        let f = b.livre(id).unwrap();
        assert_eq!((f.current_page, f.last_page_read), (Some(n - 1), Some(n - 1)), "{id} (bibliothèque affichée)");
        assert_eq!(f.read_percent(), Some(100));
        assert_eq!(f.open_count, None, "ce n'est pas une ouverture");
    }
    // Écrit dans la base (relue par une autre instance), compteur signalé à la surveillance.
    let cfg = b.store_autonome().unwrap();
    let relu = Store::open_existing(&cfg).unwrap().get(&ids(&["a"])).unwrap();
    assert_eq!(relu[0].last_page_read, Some(19));
    // Détail : lu à 100 %.
    let mut det = b.detail("a").unwrap();
    l.completer_detail(&mut det);
    assert!(det.lu);
    assert_eq!(Memoire::nouvelle(data.clone()).lire(Path::new("/BD/b.cbz")).unwrap().pourcentage(), Some(100));

    let bilan = marquer_lecture(&b, &e, &l, &ids(&["a", "b"]), false).unwrap();
    assert_eq!((bilan.modifies.len(), bilan.inchanges), (2, 0), "{bilan:?}");
    for id in ["a", "b"] {
        let f = b.livre(id).unwrap();
        assert_eq!((f.current_page, f.last_page_read, f.read_percent()), (Some(0), Some(0), Some(0)), "{id}");
    }
    assert_eq!(Store::open_existing(&cfg).unwrap().get(&ids(&["b"])).unwrap()[0].last_page_read, Some(0));
    // Avancement local oublié : le détail ne ressort pas « lu ».
    assert_eq!(Memoire::nouvelle(data).lire(Path::new("/BD/b.cbz")), None);
    let mut det = b.detail("b").unwrap();
    l.completer_detail(&mut det);
    assert!(!det.lu);
    // Déjà non lu : rien à faire.
    let bilan = marquer_lecture(&b, &e, &l, &ids(&["a"]), false).unwrap();
    assert_eq!((bilan.modifies.len(), bilan.inchanges), (0, 1));
}

#[test]
fn nombre_de_pages_inconnu_ignore_le_livre() {
    let d = tempfile::tempdir().unwrap();
    let (b, e, l) = autonome(&d.path().join("data"), vec![livre("a", None, None), livre("b", Some(5), None)]);
    let bilan = marquer_lecture(&b, &e, &l, &ids(&["a", "b"]), true).unwrap();
    assert_eq!(bilan.modifies, ids(&["b"]));
    assert_eq!(bilan.ignores, vec![LivreIgnore { id: "a".into(), raison: "nombre de pages inconnu".into() }]);
    assert_eq!(b.livre("a").unwrap().last_page_read, None);
}

#[test]
fn selection_sans_livre_connu_refusee() {
    let d = tempfile::tempdir().unwrap();
    let (b, e, l) = autonome(&d.path().join("data"), vec![livre("a", Some(5), None)]);
    assert!(marquer_lecture(&b, &e, &l, &ids(&["x"]), true).is_err());
    assert!(marquer_lecture(&b, &e, &l, &[], true).is_err());
}

#[test]
fn comicrack_en_lecture_seule_refuse_et_n_ecrit_rien() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    b.installer(Mode::Comicrack, ComicDatabase::from_books(vec![livre("a", Some(5), Some(2))]), None);
    let l = Lecteur::nouveau(data.clone(), b.clone());
    let e = Edition::nouvelle(b.clone(), l.clone());
    for lu in [true, false] {
        let err = marquer_lecture(&b, &e, &l, &ids(&["a"]), lu).unwrap_err();
        assert!(err.contains("Lecture seule"), "{err}");
    }
    let f = b.livre("a").unwrap();
    assert_eq!((f.current_page, f.last_page_read), (Some(2), Some(2)), "fiche intacte");
    assert!(!data.join("avancement.json").exists(), "avancement local intact");
}

#[test]
fn aucune_bibliotheque_refuse() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    let l = Lecteur::nouveau(data, b.clone());
    let e = Edition::nouvelle(b.clone(), l.clone());
    assert!(marquer_lecture(&b, &e, &l, &ids(&["a"]), true).is_err());
}
