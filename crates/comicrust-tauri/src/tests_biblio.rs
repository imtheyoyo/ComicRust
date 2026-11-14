//! Tests de la bibliothèque : SQLite autonome, scan d'un dossier de CBZ généré ici, mode
//! ComicRack (ComicDb.xml), contrat JSON. Aucun MySQL réel (le mot de passe du test est fictif).

use crate::biblio::*;
use crate::connexion::SourceBase;
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
        liste: None,
        serie: None,
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

/// Vue empilée par série : une pile par série dans l'ordre de la vue, comptes de livres et de lus,
/// recherche prise en compte, pile ouverte (`serie`) = les seuls livres de la série.
#[test]
fn piles_par_serie_et_pile_ouverte() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    creer_bd(&bd);
    let b = autonome(&d.path().join("data"));
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    scanner(&b);

    let req = requete("serie", SensTri::Asc, None, None);
    let piles = b.lister_piles(&req).unwrap();
    assert_eq!(piles.total_livres, 4);
    let noms: Vec<_> = piles.piles.iter().map(|p| (p.serie.as_str(), p.nb_livres, p.nb_lus)).collect();
    assert_eq!(noms, [("Astérix", 1, 0), ("Hors-serie", 1, 0), ("Thanos", 2, 0)]);
    let thanos = &piles.piles[2];
    assert_eq!(thanos.ids.len(), 2);
    assert!(thanos.url_couverture.contains(&thanos.ids[0]), "couverture du premier livre de la pile");

    // Tri décroissant : l'ordre des piles suit celui de la vue (première apparition de la série).
    let desc = b.lister_piles(&requete("serie", SensTri::Desc, None, None)).unwrap();
    assert_eq!(desc.piles[0].serie, "Thanos");

    // Un livre lu à 100 % (Thanos n° 2, 3 pages : dernière page lue = 2) est compté.
    let p = b.lister(&req).unwrap();
    let thanos_2 =
        p.livres.iter().find(|l| l.serie.as_deref() == Some("Thanos") && l.numero.as_deref() == Some("2")).unwrap();
    b.noter_lecture(&thanos_2.id, 2, 2, "2026-01-01T00:00:00");
    let piles = b.lister_piles(&req).unwrap();
    assert_eq!(piles.piles[2].nb_lus, 1);
    assert_eq!(piles.piles[2].nb_livres, 2);

    // Recherche : seules les piles des livres retenus.
    let r = b.lister_piles(&requete("serie", SensTri::Asc, None, Some("astérix"))).unwrap();
    assert_eq!(r.piles.len(), 1);
    assert_eq!(r.piles[0].serie, "Astérix");
    assert_eq!(r.total_livres, 1);

    // Pile ouverte : `lister` ne rend que ses livres ; `lister_piles` ignore `serie`.
    let ouverte = RequeteLivres { serie: Some("Thanos".into()), ..req.clone() };
    let l = b.lister(&ouverte).unwrap();
    assert_eq!(l.total, 2);
    assert!(l.livres.iter().all(|x| x.serie.as_deref() == Some("Thanos")));
    assert_eq!(b.lister_piles(&ouverte).unwrap().piles.len(), 3);
    assert_eq!(b.lister(&RequeteLivres { serie: Some("Inconnue".into()), ..req.clone() }).unwrap().total, 0);
    // Le résultat sans `serie` n'est pas pollué par la requête filtrée (cache de vue).
    assert_eq!(b.lister(&req).unwrap().total, 4);
}

fn autonome(data: &Path) -> Biblio {
    let b = Biblio::nouvelle(data.to_path_buf());
    let etat = b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    assert_eq!(
        etat,
        EtatBiblio {
            mode: Mode::Autonome,
            nb_livres: 0,
            premier_demarrage: false,
            source: SourceBase::Sqlite,
            base_absente: None
        }
    );
    b
}

#[test]
fn configurer_conserve_les_preferences_de_vue() {
    let d = tempfile::tempdir().unwrap();
    let p = reglages::PreferencesVue { tri: Some("titre".into()), groupe: Some("serie".into()), ..Default::default() };
    reglages::enregistrer_preferences_vue(d.path(), p.clone()).unwrap();
    autonome(d.path());
    let r = reglages::lire(d.path()).unwrap();
    assert_eq!(r.mode, Some(Mode::Autonome));
    assert_eq!(r.vue, p);
}

#[test]
fn changer_de_base_retire_lecture_et_ecriture_comicrack() {
    let d = tempfile::tempdir().unwrap();
    reglages::modifier(d.path(), |r| r.edition.ecriture_comicrack = Some(true)).unwrap();
    assert_eq!(reglages::lire(d.path()).unwrap().edition.ecriture_comicrack, Some(true));
    // Le choix d'une base (ici le passage en autonome) ne reporte pas le consentement d'écriture.
    autonome(d.path());
    assert_eq!(reglages::lire(d.path()).unwrap().edition.ecriture_comicrack, None);
}

#[test]
fn premier_demarrage_puis_choix_autonome_persistant() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    assert_eq!(
        b.ouvrir().unwrap(),
        EtatBiblio {
            mode: Mode::Autonome,
            nb_livres: 0,
            premier_demarrage: true,
            source: SourceBase::Sqlite,
            base_absente: None
        }
    );
    assert!(b.lister(&requete("serie", SensTri::Asc, None, None)).is_err(), "rien d'ouvert");
    assert!(!d.path().join("comicrust.db").exists(), "rien n'est créé avant le choix");

    autonome(d.path());
    assert!(d.path().join("comicrust.db").is_file());
    assert_eq!(reglages::lire(d.path()).unwrap().mode, Some(Mode::Autonome));
    // Redémarrage : le choix est retenu.
    let b2 = Biblio::nouvelle(d.path().to_path_buf());
    assert_eq!(
        b2.ouvrir().unwrap(),
        EtatBiblio {
            mode: Mode::Autonome,
            nb_livres: 0,
            premier_demarrage: false,
            source: SourceBase::Sqlite,
            base_absente: None
        }
    );
    // Base supprimée : l'assistant est proposé (avec le chemin attendu), pas de base vide
    // recréée en silence.
    std::fs::remove_file(d.path().join("comicrust.db")).unwrap();
    let e = Biblio::nouvelle(d.path().to_path_buf()).ouvrir().unwrap();
    assert!(e.premier_demarrage);
    assert_eq!(e.base_absente.as_deref(), Some(d.path().join("comicrust.db").display().to_string().as_str()));
    assert!(!d.path().join("comicrust.db").exists(), "rien n'est créé avant le choix");
}

/// Copie autonome sans `data\comicrust.db` (tauri-v0.9.1) : assistant au lieu du seul message,
/// base créée seulement sur choix explicite ; jamais de base vide à la place d'une base MySQL
/// (même à la connexion illisible) ni par-dessus ce qui occupe déjà l'emplacement.
#[test]
fn copie_autonome_sans_base_propose_l_assistant() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("comicrust.db");
    reglages::modifier(d.path(), |r| {
        r.mode = Some(Mode::Autonome);
        r.vue.tri = Some("titre".into());
    })
    .unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    let e = b.ouvrir().unwrap();
    assert_eq!(
        e,
        EtatBiblio {
            mode: Mode::Autonome,
            nb_livres: 0,
            premier_demarrage: true,
            source: SourceBase::Sqlite,
            base_absente: Some(db.display().to_string()),
        }
    );
    let json = serde_json::to_value(&e).unwrap();
    assert_eq!(json["baseAbsente"], db.display().to_string());
    assert!(!db.exists(), "ouvrir ne crée rien");
    assert!(b.lister(&requete("serie", SensTri::Asc, None, None)).is_err(), "rien d'ouvert");
    // « Créer une nouvelle base » = choix autonome de l'assistant : base créée, préférences gardées.
    let e = b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    assert!(!e.premier_demarrage && e.base_absente.is_none());
    assert!(db.is_file());
    assert_eq!(reglages::lire(d.path()).unwrap().vue.tri.as_deref(), Some("titre"));

    // Un dossier (ou autre chose) à la place du fichier : erreur, rien de proposé ni d'écrasé.
    std::fs::remove_file(&db).unwrap();
    std::fs::create_dir(&db).unwrap();
    let err = Biblio::nouvelle(d.path().to_path_buf()).ouvrir().unwrap_err();
    assert!(err.contains("introuvable"), "{err}");
    assert!(db.is_dir());
    std::fs::remove_dir(&db).unwrap();

    // Base MySQL enregistrée mais illisible (connexion incomplète, ignorée à la lecture) : pas
    // d'assistant SQLite, l'erreur reste affichée ; aucun fichier créé.
    let p = d.path().join(reglages::FICHIER_REGLAGES);
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    v["storeMysql"] = serde_json::json!({"host": "nas", "user": ""});
    std::fs::write(&p, v.to_string()).unwrap();
    assert!(reglages::lire(d.path()).unwrap().store_mysql.is_none());
    assert!(Biblio::nouvelle(d.path().to_path_buf()).ouvrir().is_err());
    assert!(!db.exists());
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
            (Some("Astérix".into()), Some("1".into())),
            // Sans fiche : série déduite du nom de fichier (ShadowSeries de ComicRack).
            (Some("Hors-serie".into()), None),
            (Some("Thanos".into()), Some("2".into())),
            (Some("Thanos".into()), Some("10".into())),
        ]
    );
    assert_eq!(p.livres[2].pages, 3);
    assert_eq!(p.livres[2].editeur.as_deref(), Some("Marvel"));
    assert_eq!(p.livres[2].annee, Some(2020));
    assert!(!p.livres[2].lu);
    assert!(p.livres[2].url_couverture.contains(&p.livres[2].id), "{}", p.livres[2].url_couverture);
    assert_eq!(p.livres[1].titre, "Hors-serie.cbz", "sans titre ni série : nom du fichier");

    // Sens inverse, recherche, groupe par éditeur, pagination.
    let desc = b.lister(&requete("serie", SensTri::Desc, None, None)).unwrap();
    assert_eq!(desc.livres[0].numero.as_deref(), Some("10"));
    // Recherche rapide de ComicRack : tous les mots dans **un même** champ, casse ignorée.
    let rech = b.lister(&requete("serie", SensTri::Asc, None, Some("  MARVEL  "))).unwrap();
    assert_eq!(rech.total, 2);
    let rech = b.lister(&requete("serie", SensTri::Asc, None, Some("jonathan  HICKMAN"))).unwrap();
    assert_eq!(rech.total, 3);
    let rech = b.lister(&requete("serie", SensTri::Asc, None, Some("marvel hickman"))).unwrap();
    assert_eq!(rech.total, 0, "éditeur et scénariste sont deux champs distincts");
    let rech = b.lister(&requete("serie", SensTri::Asc, None, Some("thanos"))).unwrap();
    assert_eq!(rech.total, 3, "le hors-série est trouvé par le chemin de son fichier");
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
    assert_eq!(img.height(), 360);
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
    assert_eq!(
        etat,
        EtatBiblio {
            mode: Mode::Comicrack,
            nb_livres: 2,
            premier_demarrage: false,
            source: SourceBase::Xml,
            base_absente: None
        }
    );
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
    r.recherche = Some("série 12".into());
    let t = std::time::Instant::now();
    // « série » et « 12 » dans la série : 12, 112, 120…129 (13 livres chacune), 212 et 312 (12).
    assert_eq!(b.lister(&r).unwrap().total, 12 * 13 + 2 * 12);
    assert!(t.elapsed() < Duration::from_secs(1), "recherche : {:?}", t.elapsed());
    r.recherche = Some("série 12 éditeur".into());
    assert_eq!(b.lister(&r).unwrap().total, 0, "mots répartis sur deux champs");
}

#[test]
fn contrat_json_en_camel_case() {
    let etat = EtatBiblio {
        mode: Mode::Comicrack,
        nb_livres: 3,
        premier_demarrage: false,
        source: SourceBase::Xml,
        base_absente: None,
    };
    assert_eq!(
        serde_json::to_string(&etat).unwrap(),
        r#"{"mode":"comicrack","nbLivres":3,"premierDemarrage":false,"source":"xml"}"#
    );
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

#[test]
fn changement_de_bibliotheque_refuse_pendant_une_analyse() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    creer_bd(&bd);
    let b = autonome(&d.path().join("data"));
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    // Le rappel bloque sur le premier événement : l'analyse reste « en cours » tant qu'on ne
    // la libère pas.
    let (vu_tx, vu_rx) = mpsc::channel();
    let (porte_tx, porte_rx) = mpsc::channel::<()>();
    let (fin_tx, fin_rx) = mpsc::channel();
    let vu_tx = std::sync::Mutex::new(vu_tx);
    let porte_rx = std::sync::Mutex::new(porte_rx);
    let fin_tx = std::sync::Mutex::new(fin_tx);
    let premier = std::sync::atomic::AtomicBool::new(true);
    b.lancer_scan(Arc::new(move |e| match e {
        EvenementScan::Termine { .. } | EvenementScan::Erreur { .. } => {
            let _ = fin_tx.lock().unwrap().send(e);
        }
        _ => {
            if premier.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let _ = vu_tx.lock().unwrap().send(());
                let _ = porte_rx.lock().unwrap().recv_timeout(Duration::from_secs(30));
            }
        }
    }))
    .unwrap();
    vu_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let e = b.ouvrir().unwrap_err();
    assert!(e.contains("analyse en cours"), "{e}");
    let choix = ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None };
    assert!(b.configurer(&choix).unwrap_err().contains("analyse en cours"));
    assert!(b.lancer_scan(Arc::new(|_| {})).unwrap_err().contains("déjà en cours"));
    porte_tx.send(()).unwrap();
    let fin = fin_rx.recv_timeout(Duration::from_secs(60)).unwrap();
    assert!(matches!(fin, EvenementScan::Termine { ajoutes: 4, .. }), "{fin:?}");
    // Libéré : on peut de nouveau ouvrir.
    assert_eq!(b.ouvrir().unwrap().nb_livres, 4);
}

#[test]
fn servir_cache_echecs_memorises_et_perimes() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    creer_bd(&bd);
    let b = autonome(&d.path().join("data"));
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    scanner(&b);
    let ids: Vec<String> =
        b.lister(&requete("serie", SensTri::Asc, None, None)).unwrap().livres.into_iter().map(|l| l.id).collect();
    let servir = |id: &str| {
        let (tx, rx) = mpsc::channel();
        b.servir(id, move |o| tx.send(o).unwrap());
        rx.recv_timeout(Duration::from_secs(30)).unwrap()
    };
    // Défaut de cache : produite par le pool, puis servie depuis le disque.
    let jpeg = servir(&ids[1]).expect("couverture produite");
    assert_eq!(&jpeg[..2], [0xFF, 0xD8]);
    assert_eq!(servir(&ids[1]), Some(jpeg));
    // Livre inconnu : 404 sans rien produire.
    assert_eq!(servir("inconnu"), None);
    // Fichier disparu avant toute couverture : 404, puis échec mémorisé (le fichier qui
    // revient n'est pas relu tout de suite).
    let fichier = b.detail(&ids[2]).unwrap().fichier.unwrap();
    let sauve = d.path().join("sauvegarde.cbz");
    std::fs::rename(&fichier, &sauve).unwrap();
    assert_eq!(servir(&ids[2]), None);
    std::fs::rename(&sauve, &fichier).unwrap();
    assert_eq!(servir(&ids[2]), None, "échec mémorisé quelques minutes");
}

#[test]
fn detail_avec_fichier_present_et_delai() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    let f = d.path().join("a.cbz");
    std::fs::write(&f, b"x").unwrap();
    let livre = LibraryBook { id: "a".into(), file: Some(f.to_string_lossy().into()), ..Default::default() };
    b.installer(Mode::Autonome, ComicDatabase::from_books(vec![livre]), None);
    assert!(b.detail("a").unwrap().fichier_present);
    std::fs::remove_file(&f).unwrap();
    assert!(!b.detail("a").unwrap().fichier_present);
}

// --- Progression de lecture du panneau Détail (lot Q) ---

fn livre_lu(last: u32, total: u32, note: Option<f32>) -> LibraryBook {
    LibraryBook {
        id: "L1".into(),
        file: Some("Z:/BD/Thanos 01.cbz".into()),
        page_count: Some(total),
        last_page_read: Some(last),
        rating: note,
        ..Default::default()
    }
}

#[test]
fn detail_progression_depuis_la_fiche() {
    let d = detail_pour_test(&livre_lu(9, 20, Some(3.5)));
    assert_eq!((d.progression, d.page_lue, d.lu), (Some(50), Some(10), false));
    assert_eq!(d.source_lecture.as_deref(), Some("comicrack"));
    assert_eq!(d.note, Some(3.5));
    // Jamais ouvert : ni page lue ni source.
    let d = detail_pour_test(&livre_lu(0, 20, None));
    assert_eq!((d.page_lue, d.source_lecture.clone(), d.note), (None, None, None));
    // Lu en entier.
    let d = detail_pour_test(&livre_lu(19, 20, None));
    assert_eq!((d.progression, d.page_lue, d.lu), (Some(100), Some(20), true));
}

#[test]
fn lecture_comicrust_prime_si_plus_avancee() {
    use crate::avancement::Avancement;
    let mut d = detail_pour_test(&livre_lu(4, 20, None)); // ComicRack : 25 %
                                                          // Moins avancée dans ComicRust : la fiche garde la main.
    fusionner_lecture(&mut d, Some(Avancement { page: 2, page_max: 2, pages: 20, relecture: false }));
    assert_eq!((d.progression, d.source_lecture.as_deref()), (Some(25), Some("comicrack")));
    // Plus avancée : elle prime (page_max 11 -> page 12 / 20 = 60 %).
    fusionner_lecture(&mut d, Some(Avancement { page: 11, page_max: 11, pages: 20, relecture: false }));
    assert_eq!((d.progression, d.page_lue, d.lu), (Some(60), Some(12), false));
    assert_eq!(d.source_lecture.as_deref(), Some("comicrust"));
    // Lu jusqu'au bout dans ComicRust, fiche jamais ouverte.
    let mut d = detail_pour_test(&livre_lu(0, 20, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 19, page_max: 19, pages: 20, relecture: false }));
    assert_eq!((d.progression, d.page_lue, d.lu), (Some(100), Some(20), true));
    // Aucune lecture locale : inchangé.
    let mut d = detail_pour_test(&livre_lu(0, 20, None));
    fusionner_lecture(&mut d, None);
    assert_eq!((d.progression, d.source_lecture), (Some(0), None));
}

/// Relecture (recette tauri-v0.7.2, C-06) : livre lu, relu jusqu'à la page 7 / 59.
#[test]
fn fusion_relecture_d_un_livre_lu() {
    use crate::avancement::Avancement;
    // Fiche ComicRack lue à 100 %, relecture notée dans ComicRust (page 6 = 7e page).
    let mut d = detail_pour_test(&livre_lu(58, 59, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 6, page_max: 6, pages: 59, relecture: true }));
    // Le livre reste lu, la fiche n'est pas dégradée…
    assert_eq!((d.lu, d.progression, d.page_lue), (true, Some(100), Some(59)));
    assert_eq!(d.source_lecture.as_deref(), Some("comicrack"));
    // … et la relecture est donnée à part (7 / 59 = 11,9 % -> 12 %).
    assert_eq!(d.relecture, Some(Relecture { page: 7, total: 59, pourcentage: 12 }));
    // Lu dans ComicRust (fin vue, fiche jamais ouverte), puis relu.
    let mut d = detail_pour_test(&livre_lu(0, 20, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 0, page_max: 19, pages: 20, relecture: true }));
    assert_eq!((d.lu, d.source_lecture.as_deref()), (true, Some("comicrust")));
    assert_eq!(d.relecture, Some(Relecture { page: 1, total: 20, pourcentage: 5 }));
    // Même position sans l'indicateur (fin vue puis retour en arrière dans la même séance) :
    // pas de relecture.
    let mut d = detail_pour_test(&livre_lu(58, 59, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 6, page_max: 58, pages: 59, relecture: false }));
    assert_eq!((d.lu, d.relecture), (true, None));
    // Relecture menée jusqu'à la dernière page : rien en cours.
    let mut d = detail_pour_test(&livre_lu(58, 59, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 58, page_max: 58, pages: 59, relecture: true }));
    assert_eq!(d.relecture, None);
    // Le JSON porte le champ seulement s'il y a une relecture.
    let mut d = detail_pour_test(&livre_lu(58, 59, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 6, page_max: 6, pages: 59, relecture: true }));
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["relecture"], serde_json::json!({ "page": 7, "total": 59, "pourcentage": 12 }));
    assert!(serde_json::to_value(detail_pour_test(&livre_lu(58, 59, None))).unwrap().get("relecture").is_none());
}

/// Lecture ComicRust plus avancée que la fiche, sans relecture ; livre non lu marqué
/// « relecture » (incohérent) : ignoré.
#[test]
fn fusion_lecture_plus_avancee_sans_relecture() {
    use crate::avancement::Avancement;
    let mut d = detail_pour_test(&livre_lu(4, 20, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 9, page_max: 9, pages: 20, relecture: true }));
    assert_eq!((d.progression, d.page_lue, d.lu), (Some(50), Some(10), false));
    assert_eq!(d.source_lecture.as_deref(), Some("comicrust"));
    assert_eq!(d.relecture, None);
}

/// Fiche seule (aucune lecture ComicRust) et livre de 0 page : rien d'inventé.
#[test]
fn fusion_fiche_seule_et_zero_page() {
    use crate::avancement::Avancement;
    let mut d = detail_pour_test(&livre_lu(58, 59, None));
    fusionner_lecture(&mut d, None);
    assert_eq!((d.lu, d.progression, d.relecture), (true, Some(100), None));
    assert_eq!(d.source_lecture.as_deref(), Some("comicrack"));
    // 0 page partout : pas de pourcentage, pas de relecture, pas de division par zéro.
    let mut d = detail_pour_test(&livre_lu(0, 0, None));
    fusionner_lecture(&mut d, Some(Avancement { page: 0, page_max: 0, pages: 0, relecture: true }));
    assert_eq!((d.lu, d.relecture, d.source_lecture.clone()), (false, None, None));
    // Fiche lue mais 0 page connue nulle part : pas de relecture affichable.
    let mut d = detail_pour_test(&livre_lu(0, 0, None));
    d.lu = true;
    fusionner_lecture(&mut d, Some(Avancement { page: 3, page_max: 3, pages: 0, relecture: true }));
    assert_eq!(d.relecture, None);
}
