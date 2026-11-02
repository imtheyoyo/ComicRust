//! Tests de la bibliothèque : SQLite autonome, scan d'un dossier de CBZ généré ici, mode
//! ComicRack (ComicDb.xml), contrat JSON. Aucun MySQL réel (le mot de passe du test est fictif).

use crate::biblio::*;
use crate::couvertures::cbz_de_test;
use crate::reglages::{self, Mode};
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::Duration;

fn info(serie: &str, numero: &str, editeur: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><ComicInfo><Series>{serie}</Series><Number>{numero}</Number>\
         <Publisher>{editeur}</Publisher><Year>2020</Year><Writer>Jonathan Hickman</Writer></ComicInfo>"
    )
}

/// Dossier de BD : 4 CBZ, dont un dans un sous-dossier.
fn creer_bd(racine: &Path) {
    std::fs::create_dir_all(racine.join("Thanos").join("Extra")).unwrap();
    let c = [180, 40, 40, 255];
    cbz_de_test(&racine.join("Thanos").join("Thanos 02.cbz"), c, 3, Some(&info("Thanos", "2", "Marvel")));
    cbz_de_test(&racine.join("Thanos").join("Thanos 10.cbz"), c, 5, Some(&info("Thanos", "10", "Marvel")));
    cbz_de_test(&racine.join("Thanos").join("Extra").join("Hors-serie.cbz"), c, 2, None);
    cbz_de_test(&racine.join("Asterix 01.cbz"), c, 4, Some(&info("Astérix", "1", "Dargaud")));
}

fn requete(tri: &str, sens: SensTri, groupe: Option<Groupe>, recherche: Option<&str>) -> RequeteLivres {
    RequeteLivres {
        recherche: recherche.map(str::to_string),
        tri: tri.into(),
        sens_tri: sens,
        groupe,
        debut: 0,
        nombre: 100,
    }
}

/// Lance un scan et attend `Termine` (ou `Erreur`) ; renvoie tous les événements reçus.
fn scanner(b: &Biblio) -> Vec<EvenementScan> {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    b.lancer_scan(Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    }))
    .unwrap();
    let mut tous = Vec::new();
    loop {
        let e = rx.recv_timeout(Duration::from_secs(60)).expect("l'analyse doit se terminer");
        let fin = matches!(e, EvenementScan::Termine { .. } | EvenementScan::Erreur { .. });
        tous.push(e);
        if fin {
            return tous;
        }
    }
}

fn autonome(data: &Path) -> Biblio {
    let b = Biblio::nouvelle(data.to_path_buf());
    let etat = b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    assert_eq!(etat, EtatBiblio { mode: Mode::Autonome, nb_livres: 0, premier_demarrage: false });
    b
}

#[test]
fn premier_demarrage_puis_choix_autonome_persistant() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    assert_eq!(b.ouvrir().unwrap(), EtatBiblio { mode: Mode::Autonome, nb_livres: 0, premier_demarrage: true });
    assert!(b.lister(&requete("serie", SensTri::Asc, None, None)).is_err(), "rien d'ouvert");
    assert!(!d.path().join("comicrust.db").exists(), "rien n'est créé avant le choix");

    autonome(d.path());
    assert!(d.path().join("comicrust.db").is_file());
    assert_eq!(reglages::lire(d.path()).unwrap().mode, Some(Mode::Autonome));
    // Redémarrage : le choix est retenu.
    let b2 = Biblio::nouvelle(d.path().to_path_buf());
    assert_eq!(b2.ouvrir().unwrap(), EtatBiblio { mode: Mode::Autonome, nb_livres: 0, premier_demarrage: false });
    // Base supprimée : erreur claire, pas de base vide recréée en silence.
    std::fs::remove_file(d.path().join("comicrust.db")).unwrap();
    let e = Biblio::nouvelle(d.path().to_path_buf()).ouvrir().unwrap_err();
    assert!(e.contains("introuvable"), "{e}");
}

#[test]
fn dossiers_surveilles() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    std::fs::create_dir_all(&bd).unwrap();
    let b = autonome(d.path());
    assert!(b.dossiers().unwrap().is_empty());
    assert!(b.lancer_scan(Arc::new(|_| {})).unwrap_err().contains("aucun dossier"));
    assert!(b.ajouter_dossier(&d.path().join("absent").to_string_lossy()).unwrap_err().contains("introuvable"));
    assert!(b.ajouter_dossier("  ").is_err());
    let chemin = bd.to_string_lossy().into_owned();
    // Guillemets et espaces autour (chemin collé) tolérés.
    assert_eq!(b.ajouter_dossier(&format!(" \"{chemin}\" ")).unwrap(), vec![chemin.clone()]);
    // Doublon (avec séparateur final) ignoré.
    assert_eq!(b.ajouter_dossier(&format!("{chemin}/")).unwrap().len(), 1);
    assert_eq!(b.dossiers().unwrap(), vec![chemin.clone()]);
    // Retrait : persistant d'un lancement à l'autre.
    let b2 = Biblio::nouvelle(d.path().to_path_buf());
    b2.ouvrir().unwrap();
    assert_eq!(b2.dossiers().unwrap(), vec![chemin.clone()]);
    assert!(b2.retirer_dossier(&chemin).unwrap().is_empty());
    assert!(b2.retirer_dossier(&chemin).unwrap().is_empty(), "retirer deux fois ne casse rien");
}

#[test]
fn scan_liste_detail_couverture_et_manquants() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    creer_bd(&bd);
    let b = autonome(&d.path().join("data"));
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();

    // Premier scan : 4 ajouts, progression émise au moins une fois (final).
    let evts = scanner(&b);
    assert!(evts.iter().any(|e| matches!(e, EvenementScan::Progression { fait: 4, total: 4 })), "{evts:?}");
    assert_eq!(*evts.last().unwrap(), EvenementScan::Termine { ajoutes: 4, mis_a_jour: 0, manquants: 0 });

    // Liste triée par série : ordre naturel des numéros (2 avant 10), « sans série » en tête
    // (série vide < « astérix »).
    let p = b.lister(&requete("serie", SensTri::Asc, None, None)).unwrap();
    assert_eq!(p.total, 4);
    let numeros: Vec<_> = p.livres.iter().map(|l| (l.serie.clone(), l.numero.clone())).collect();
    assert_eq!(
        numeros,
        [
            (None, None),
            (Some("Astérix".into()), Some("1".into())),
            (Some("Thanos".into()), Some("2".into())),
            (Some("Thanos".into()), Some("10".into())),
        ]
    );
    assert_eq!(p.livres[2].pages, 3);
    assert_eq!(p.livres[2].editeur.as_deref(), Some("Marvel"));
    assert_eq!(p.livres[2].annee, Some(2020));
    assert!(!p.livres[2].lu);
    assert!(p.livres[2].url_couverture.contains(&p.livres[2].id), "{}", p.livres[2].url_couverture);
    assert_eq!(p.livres[0].titre, "Hors-serie.cbz", "sans titre ni série : nom du fichier");

    // Sens inverse, recherche, groupe par éditeur, pagination.
    let desc = b.lister(&requete("serie", SensTri::Desc, None, None)).unwrap();
    assert_eq!(desc.livres[0].numero.as_deref(), Some("10"));
    let rech = b.lister(&requete("serie", SensTri::Asc, None, Some("  MARVEL  hickman "))).unwrap();
    assert_eq!(rech.total, 2);
    assert_eq!(b.lister(&requete("serie", SensTri::Asc, None, Some("inexistant"))).unwrap().total, 0);
    let groupe = b.lister(&requete("pages", SensTri::Asc, Some(Groupe::Editeur), None)).unwrap();
    let eds: Vec<_> = groupe.livres.iter().map(|l| l.editeur.clone()).collect();
    assert_eq!(eds, [None, Some("Dargaud".into()), Some("Marvel".into()), Some("Marvel".into())]);
    assert_eq!(groupe.livres[2].pages, 3, "tri par pages dans le groupe");
    let mut fen = requete("serie", SensTri::Asc, None, None);
    (fen.debut, fen.nombre) = (1, 2);
    let f = b.lister(&fen).unwrap();
    assert_eq!((f.total, f.livres.len(), f.livres[0].id.clone()), (4, 2, p.livres[1].id.clone()));
    (fen.debut, fen.nombre) = (99, 10);
    assert!(b.lister(&fen).unwrap().livres.is_empty());
    assert!(b.lister(&requete("couleur", SensTri::Asc, None, None)).is_err(), "tri inconnu");

    // Détail.
    let id = p.livres[3].id.clone();
    let det = b.detail(&id).unwrap();
    assert_eq!((det.serie.as_deref(), det.numero.as_deref(), det.pages), (Some("Thanos"), Some("10"), 5));
    assert_eq!(det.scenariste.as_deref(), Some("Jonathan Hickman"));
    assert!(det.fichier_present && !det.manquant);
    assert!(det.fichier.as_deref().unwrap().ends_with("Thanos 10.cbz"));
    assert!(b.detail("inconnu").is_err());

    // Couverture : JPEG miniature, mise en cache dans data/couvertures.
    let jpeg = b.couverture(&id).unwrap();
    let img = image::load_from_memory(&jpeg).unwrap();
    assert_eq!(img.height(), 300);
    let nb_cache = std::fs::read_dir(b.dossier_couvertures()).unwrap().count();
    assert_eq!(nb_cache, 1);

    // Un fichier disparaît, un autre arrive : manquant + ajout (le scan peut être relancé,
    // l'indicateur « en cours » est retombé).
    std::fs::remove_file(bd.join("Thanos").join("Thanos 10.cbz")).unwrap();
    cbz_de_test(&bd.join("Nouveau.cbz"), [0, 0, 0, 255], 1, None);
    let evts = scanner(&b);
    assert_eq!(*evts.last().unwrap(), EvenementScan::Termine { ajoutes: 1, mis_a_jour: 0, manquants: 1 });
    let det = b.detail(&id).unwrap();
    assert!(det.manquant && !det.fichier_present);
    assert_eq!(b.lister(&requete("serie", SensTri::Asc, None, None)).unwrap().total, 5, "manquants conservés");
    // La couverture du livre manquant vient toujours du cache.
    assert_eq!(b.couverture(&id).unwrap(), jpeg);

    // Les livres sont dans la base : un nouveau lancement les retrouve.
    let b2 = Biblio::nouvelle(d.path().join("data"));
    assert_eq!(b2.ouvrir().unwrap().nb_livres, 5);
}

#[test]
fn mode_comicrack_avec_comicdb_xml_en_lecture_seule() {
    let d = tempfile::tempdir().unwrap();
    let xml = d.path().join("ComicDb.xml");
    let b = Biblio::nouvelle(d.path().join("data"));
    let choix = |c: Option<String>| ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: c };
    // Fichier absent / vide : refus, rien d'enregistré.
    assert!(b.configurer(&choix(Some(xml.to_string_lossy().into()))).unwrap_err().contains("introuvable"));
    std::fs::write(&xml, "<ComicDatabase><Books></Books></ComicDatabase>").unwrap();
    assert!(b.configurer(&choix(Some(xml.to_string_lossy().into()))).unwrap_err().contains("aucun livre"));
    assert_eq!(reglages::lire(&d.path().join("data")).unwrap().mode, None);

    std::fs::write(
        &xml,
        "\u{feff}<ComicDatabase><Books>\
         <Book Id=\"11111111-0000-0000-0000-000000000001\" File=\"Z:\\BD\\Thanos 01.cbz\"><Series>Thanos</Series>\
         <Number>1</Number><PageCount>20</PageCount><LastPageRead>19</LastPageRead></Book>\
         <Book Id=\"11111111-0000-0000-0000-000000000002\" File=\"Z:\\BD\\Thanos 02.cbz\"><Series>Thanos</Series>\
         <Number>2</Number><PageCount>20</PageCount></Book>\
         </Books></ComicDatabase>",
    )
    .unwrap();
    let etat = b.configurer(&choix(Some(format!("\"{}\"", xml.display())))).unwrap();
    assert_eq!(etat, EtatBiblio { mode: Mode::Comicrack, nb_livres: 2, premier_demarrage: false });
    assert!(!d.path().join("data").join("comicrust.db").exists(), "aucune base ComicRust créée en mode lié");
    let p = b.lister(&requete("serie", SensTri::Asc, None, None)).unwrap();
    assert_eq!((p.total, p.livres[0].lu, p.livres[1].lu), (2, true, false));
    assert!(!b.detail(&p.livres[0].id).unwrap().fichier_present, "Z:\\BD n'existe pas ici");
    // Mode lié : pas de dossiers ni de scan.
    assert!(b.dossiers().unwrap().is_empty());
    assert!(b.ajouter_dossier(&d.path().to_string_lossy()).is_err());
    assert!(b.lancer_scan(Arc::new(|_| {})).unwrap_err().contains("mode autonome"));
    // Redémarrage.
    let b2 = Biblio::nouvelle(d.path().join("data"));
    assert_eq!(b2.ouvrir().unwrap().mode, Mode::Comicrack);
}

#[test]
fn mysql_injoignable_n_enregistre_rien() {
    let d = tempfile::tempdir().unwrap();
    let ini = d.path().join("ComicRack.ini");
    // Port 1 : connexion refusée aussitôt. Mot de passe fictif.
    std::fs::write(&ini, "DataSource=mysql:Server=127.0.0.1; Port=1; Uid=test; Pwd=fictif; Database=bd;\r\n").unwrap();
    let b = Biblio::nouvelle(d.path().join("data"));
    let e = b
        .configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(ini.to_string_lossy().into()) })
        .unwrap_err();
    assert!(!e.is_empty());
    assert!(!e.contains("fictif"), "le mot de passe n'apparaît jamais dans l'erreur : {e}");
    assert_eq!(reglages::lire(&d.path().join("data")).unwrap().mode, None);
    // ini sans ligne MySQL
    std::fs::write(&ini, "Langue=fr\r\n").unwrap();
    assert!(b
        .configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(ini.to_string_lossy().into()) })
        .is_err());
}

#[test]
fn cinq_mille_livres_restent_fluides() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    let livres: Vec<LibraryBook> = (0..5000)
        .map(|i| LibraryBook {
            id: format!("00000000-0000-0000-0000-{i:012}"),
            series: Some(format!("Série {}", i % 400)),
            number: Some((i / 400 + 1).to_string()),
            publisher: Some(format!("Éditeur {}", i % 17)),
            page_count: Some(20 + (i % 50) as u32),
            file: Some(format!("Z:\\BD\\{}\\{i}.cbz", i % 400)),
            ..Default::default()
        })
        .collect();
    b.installer(Mode::Autonome, ComicDatabase::from_books(livres), None);
    let t = std::time::Instant::now();
    let mut r = requete("serie", SensTri::Asc, Some(Groupe::Serie), None);
    r.nombre = 60;
    let p = b.lister(&r).unwrap();
    assert_eq!((p.total, p.livres.len()), (5000, 60));
    let premier = t.elapsed();
    // Défilement : la vue triée est en cache, chaque fenêtre est quasi instantanée.
    let t = std::time::Instant::now();
    for i in 0..80 {
        r.debut = i * 60;
        assert_eq!(b.lister(&r).unwrap().livres.len(), 60);
    }
    let defilement = t.elapsed();
    assert!(
        premier < Duration::from_secs(2) && defilement < Duration::from_millis(500),
        "{premier:?} / {defilement:?}"
    );
    r.recherche = Some("série 12 éditeur".into());
    assert!(b.lister(&r).unwrap().total > 0);
}

#[test]
fn contrat_json_en_camel_case() {
    let etat = EtatBiblio { mode: Mode::Comicrack, nb_livres: 3, premier_demarrage: false };
    assert_eq!(serde_json::to_string(&etat).unwrap(), r#"{"mode":"comicrack","nbLivres":3,"premierDemarrage":false}"#);
    let choix: ChoixPremierDemarrage =
        serde_json::from_str(r#"{"mode":"comicrack","cheminIni":"C:/CR/ComicRack.ini"}"#).unwrap();
    assert_eq!((choix.mode, choix.chemin_ini.as_deref()), (Mode::Comicrack, Some("C:/CR/ComicRack.ini")));
    let choix: ChoixPremierDemarrage = serde_json::from_str(r#"{"mode":"autonome"}"#).unwrap();
    assert!(choix.chemin_ini.is_none());
    let req: RequeteLivres =
        serde_json::from_str(r#"{"tri":"titre","sensTri":"desc","groupe":"serie","debut":10,"nombre":50}"#).unwrap();
    assert_eq!((req.sens_tri, req.groupe, req.debut, req.nombre), (SensTri::Desc, Some(Groupe::Serie), 10, 50));
    assert!(req.recherche.is_none());
    let livre = LibraryBook { id: "a".into(), series: Some("S".into()), ..Default::default() };
    let json = serde_json::to_value(&crate::biblio::PageLivres { total: 1, livres: vec![] }).unwrap();
    assert_eq!(json, serde_json::json!({"total": 1, "livres": []}));
    // Champs vides absents (TypeScript : `?:`), noms en camelCase.
    let b = Biblio::nouvelle(std::env::temp_dir());
    b.installer(Mode::Autonome, ComicDatabase::from_books(vec![livre]), None);
    let l = b.lister(&requete("serie", SensTri::Asc, None, None)).unwrap();
    let v = serde_json::to_value(&l.livres[0]).unwrap();
    let cles: std::collections::BTreeSet<_> = v.as_object().unwrap().keys().cloned().collect();
    assert_eq!(cles.into_iter().collect::<Vec<_>>(), ["id", "lu", "pages", "serie", "titre", "urlCouverture"]);
    let det = serde_json::to_value(b.detail("a").unwrap()).unwrap();
    assert_eq!(det["fichierPresent"], false);
    assert_eq!(det["manquant"], false);
}

#[test]
fn evenements_du_scan_pour_le_front() {
    let (n, v) = EvenementScan::Progression { fait: 3, total: 10 }.pour_le_front();
    assert_eq!((n, v.to_string().as_str()), ("scan-progression", r#"{"fait":3,"total":10}"#));
    let (n, v) = EvenementScan::Termine { ajoutes: 1, mis_a_jour: 2, manquants: 3 }.pour_le_front();
    assert_eq!((n, v.to_string().as_str()), ("scan-termine", r#"{"ajoutes":1,"manquants":3,"misAJour":2}"#));
    let (n, v) = EvenementScan::Erreur { message: "x".into() }.pour_le_front();
    assert_eq!((n, v["message"].as_str()), ("scan-erreur", Some("x")));
}

#[test]
fn urls_de_couverture() {
    assert_eq!(id_depuis_uri("/abc-123", None), "abc-123");
    assert_eq!(id_depuis_uri("", Some("abc-123")), "abc-123", "cr-couv://<id> : l'id est l'hôte");
    assert_eq!(id_depuis_uri("/a%2Fb%20c", None), "a/b c");
    assert_eq!(id_depuis_uri("/%zz%4", None), "%zz%4", "pourcentage invalide laissé tel quel");
    let b = Biblio::nouvelle(std::env::temp_dir());
    let livre = LibraryBook { id: "a b/é".into(), ..Default::default() };
    b.installer(Mode::Autonome, ComicDatabase::from_books(vec![livre]), None);
    let url = b.lister(&requete("serie", SensTri::Asc, None, None)).unwrap().livres[0].url_couverture.clone();
    assert!(url.ends_with("/a%20b%2F%C3%A9"), "{url}");
    assert!(url.starts_with(if cfg!(windows) { "http://cr-couv.localhost/" } else { "cr-couv://localhost/" }));
    // Aller-retour avec ce que le protocole reçoit comme chemin.
    let chemin = url.rsplit_once("localhost").unwrap().1;
    assert_eq!(id_depuis_uri(chemin, None), "a b/é");
}
