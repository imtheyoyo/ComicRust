//! Tests du lot « Scan et dossiers » : arrêt de l'analyse, bilan copiable, analyse automatique du
//! démarrage (options par défaut, jamais en lecture seule ni hors ligne), option de suivi de
//! ComicRack. SQLite autonome, dossier de CBZ générés ici ; aucun serveur.

use crate::biblio::*;
use crate::couvertures::cbz_de_test;
use crate::reglages::{self, Mode, ReglagesAnalyse};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const DELAI: Duration = Duration::from_secs(60);

fn autonome(data: &Path) -> Biblio {
    let b = Biblio::nouvelle(data.to_path_buf());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    b
}

/// `n` petits CBZ dans `racine`, plus un faux CBZ illisible (octets quelconques).
fn creer_bd(racine: &Path, n: usize, avec_illisible: bool) {
    std::fs::create_dir_all(racine).unwrap();
    for i in 0..n {
        cbz_de_test(&racine.join(format!("Livre {i:03}.cbz")), [10, 20, 30, 255], 2, None);
    }
    if avec_illisible {
        std::fs::write(racine.join("Cassé.cbz"), b"ceci n'est pas une archive").unwrap();
    }
}

type Sortie = Arc<dyn Fn(EvenementScan) + Send + Sync>;

/// Reçoit les événements d'un rappel de scan.
fn canal() -> (Sortie, mpsc::Receiver<EvenementScan>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    (
        Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
        rx,
    )
}

/// Attend la fin (`Termine` ou `Erreur`) ; renvoie tous les événements.
fn attendre_fin(rx: &mpsc::Receiver<EvenementScan>) -> Vec<EvenementScan> {
    let mut tous = Vec::new();
    loop {
        let e = rx.recv_timeout(DELAI).expect("l'analyse doit se terminer");
        let fin = matches!(e, EvenementScan::Termine { .. } | EvenementScan::Erreur { .. });
        tous.push(e);
        if fin {
            return tous;
        }
    }
}

fn bilan(evts: &[EvenementScan]) -> BilanScan {
    evts.iter()
        .find_map(|e| if let EvenementScan::Bilan(b) = e { Some(b.clone()) } else { None })
        .expect("un bilan est émis")
}

#[test]
fn arret_demande_avant_le_parcours_rien_n_est_ajoute() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    creer_bd(&bd, 6, false);
    let b = autonome(&d.path().join("data"));
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    // Drapeau levé d'avance : l'analyse s'arrête dès le début (déterministe).
    let (sortie, rx) = canal();
    b.lancer_scan_avec(sortie, Arc::new(AtomicBool::new(true))).unwrap();
    let evts = attendre_fin(&rx);
    let bil = bilan(&evts);
    assert!(bil.arretee, "{bil:?}");
    assert!(bil.resume.contains("interrompu"), "{}", bil.resume);
    assert_eq!(*evts.last().unwrap(), EvenementScan::Termine { ajoutes: 0, mis_a_jour: 0, manquants: 0 });
    assert_eq!(b.ouvrir().unwrap().nb_livres, 0);
    // Le bilan précède `Termine`.
    let pos_bilan = evts.iter().position(|e| matches!(e, EvenementScan::Bilan(_))).unwrap();
    assert_eq!(pos_bilan, evts.len() - 2);
    // Libéré : on peut relancer, et l'analyse complète trouve tout.
    let (sortie, rx) = canal();
    b.lancer_scan(sortie).unwrap();
    let evts = attendre_fin(&rx);
    assert!(!bilan(&evts).arretee);
    assert_eq!(*evts.last().unwrap(), EvenementScan::Termine { ajoutes: 6, mis_a_jour: 0, manquants: 0 });
}

#[test]
fn arreter_scan_pendant_l_analyse_et_sans_analyse() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    creer_bd(&bd, 40, false);
    let b = autonome(&d.path().join("data"));
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    assert!(!b.arreter_scan(), "rien à arrêter sans analyse");

    // Le rappel bloque sur le premier événement : l'analyse reste « en cours ».
    let (vu_tx, vu_rx) = mpsc::channel();
    let (porte_tx, porte_rx) = mpsc::channel::<()>();
    let (fin_tx, fin_rx) = mpsc::channel();
    let (vu_tx, porte_rx, fin_tx) = (Mutex::new(vu_tx), Mutex::new(porte_rx), Mutex::new(fin_tx));
    let premier = AtomicBool::new(true);
    b.lancer_scan(Arc::new(move |e| match e {
        EvenementScan::Progression { .. } => {
            if premier.swap(false, Ordering::SeqCst) {
                let _ = vu_tx.lock().unwrap().send(());
                let _ = porte_rx.lock().unwrap().recv_timeout(Duration::from_secs(30));
            }
        }
        autre => {
            let _ = fin_tx.lock().unwrap().send(autre);
        }
    }))
    .unwrap();
    vu_rx.recv_timeout(DELAI).unwrap();
    assert!(b.arreter_scan(), "une analyse est en cours");
    porte_tx.send(()).unwrap();
    // L'analyse (arrêtée ou déjà finie) se termine proprement ; le bilan est cohérent avec la base.
    let mut bil = None;
    let ajoutes = loop {
        match fin_rx.recv_timeout(DELAI).expect("fin de l'analyse") {
            EvenementScan::Bilan(x) => bil = Some(x),
            EvenementScan::Termine { ajoutes, .. } => break ajoutes,
            EvenementScan::Erreur { message } => panic!("{message}"),
            EvenementScan::Progression { .. } => {}
        }
    };
    let bil = bil.expect("bilan émis avant Termine");
    assert_eq!(b.ouvrir().unwrap().nb_livres, ajoutes, "les livres lus avant l'arrêt sont enregistrés");
    assert!(bil.arretee || ajoutes == 40, "{bil:?}");
    assert!(!b.analyse_en_cours());
    assert!(!b.arreter_scan(), "le drapeau est libéré avec l'analyse");
}

#[test]
fn bilan_avec_erreurs_et_texte_copiable() {
    let d = tempfile::tempdir().unwrap();
    let bd = d.path().join("bd");
    creer_bd(&bd, 3, true);
    let b = autonome(&d.path().join("data"));
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    let (sortie, rx) = canal();
    b.lancer_scan(sortie).unwrap();
    let evts = attendre_fin(&rx);
    let bil = bilan(&evts);
    assert_eq!(bil.dossiers, 1);
    assert_eq!(bil.fichiers, 4);
    assert!(!bil.arretee);
    assert_eq!(bil.erreurs_total, bil.erreurs.len());
    assert!(!bil.erreurs.is_empty(), "le fichier illisible est signalé : {bil:?}");
    assert!(bil.erreurs.iter().any(|e| e.chemin.ends_with("Cassé.cbz") && !e.message.is_empty()), "{bil:?}");
    let t = &bil.texte;
    assert!(t.starts_with("ComicRust v"), "{t}");
    assert!(t.contains("analyse des dossiers") && t.contains("1 dossier(s) analysé(s)."), "{t}");
    assert!(t.contains("- ") && t.contains("Cassé.cbz"), "{t}");
    // Contrat JSON du front.
    let (nom, v) = EvenementScan::Bilan(bil.clone()).pour_le_front();
    assert_eq!(nom, "scan-bilan");
    for cle in ["resume", "dossiers", "fichiers", "arretee", "erreurs", "erreursTotal", "texte"] {
        assert!(v.get(cle).is_some(), "clé {cle} absente : {v}");
    }
    assert!(v["erreurs"][0]["chemin"].is_string() && v["erreurs"][0]["message"].is_string());
}

#[test]
fn bilan_tronque_au_dela_du_maximum() {
    let mut r = comicrust_core::scanner::ScanReport::default();
    for i in 0..(MAX_ERREURS_BILAN + 7) {
        r.errors.push((format!("/bd/{i}.cbz").into(), "illisible".into()));
    }
    let b = BilanScan::depuis_rapport(&r, 2);
    assert_eq!((b.erreurs.len(), b.erreurs_total), (MAX_ERREURS_BILAN, MAX_ERREURS_BILAN + 7));
    assert!(b.texte.contains("7 autre(s) erreur(s) non détaillée(s)"), "{}", b.texte);
}

#[test]
fn options_par_defaut_et_analyse_automatique() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let bd = d.path().join("bd");
    creer_bd(&bd, 3, false);
    let b = autonome(&data);
    // Options : tout activé par défaut (comme egui).
    let r = reglages::lire_reglages_analyse(&data).unwrap();
    assert!(r.au_demarrage && r.suivre_comicrack);
    assert!(b.suivre_comicrack());
    // Sans dossier : pas d'analyse (pas d'erreur).
    assert_eq!(b.motif_pas_d_analyse_auto(), Some("aucun dossier à analyser"));
    let (sortie, _rx) = canal();
    assert_eq!(b.lancer_scan_auto(sortie), Ok(false));
    // Avec un dossier : l'analyse a lieu.
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    assert_eq!(b.motif_pas_d_analyse_auto(), None);
    let (sortie, rx) = canal();
    assert_eq!(b.lancer_scan_auto(sortie), Ok(true));
    let evts = attendre_fin(&rx);
    assert!(matches!(evts.last(), Some(EvenementScan::Termine { ajoutes: 3, .. })), "{evts:?}");
}

#[test]
fn analyse_automatique_desactivee_hors_ligne_ou_en_lecture_seule() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let bd = d.path().join("bd");
    creer_bd(&bd, 2, false);
    let b = autonome(&data);
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();

    // Désactivée dans les réglages.
    reglages::enregistrer_reglages_analyse(&data, ReglagesAnalyse { au_demarrage: Some(false), ..Default::default() })
        .unwrap();
    assert_eq!(b.motif_pas_d_analyse_auto(), Some("désactivée dans les réglages"));
    let (sortie, rx) = canal();
    assert_eq!(b.lancer_scan_auto(sortie), Ok(false));
    assert!(rx.try_recv().is_err(), "aucun événement : rien n'a été lancé");
    assert_eq!(b.ouvrir().unwrap().nb_livres, 0);
    reglages::enregistrer_reglages_analyse(&data, ReglagesAnalyse { au_demarrage: Some(true), ..Default::default() })
        .unwrap();

    // Hors ligne : jamais d'analyse automatique.
    b.connexion().passer_hors_ligne("base injoignable".into());
    assert_eq!(b.motif_pas_d_analyse_auto(), Some("base hors ligne"));
    let (sortie, _rx) = canal();
    assert_eq!(b.lancer_scan_auto(sortie), Ok(false));
    b.connexion().revenir_en_ligne();
    assert_eq!(b.motif_pas_d_analyse_auto(), None);

    // Mode lié à ComicRack (lecture seule) : jamais.
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(
        &xml,
        "<ComicDatabase><Books><Book Id=\"11111111-0000-0000-0000-000000000001\" File=\"Z:\\BD\\A.cbz\">\
         <Series>A</Series></Book></Books></ComicDatabase>",
    )
    .unwrap();
    let lie = Biblio::nouvelle(d.path().join("data2"));
    lie.configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(xml.to_string_lossy().into()) })
        .unwrap();
    assert_eq!(lie.motif_pas_d_analyse_auto(), Some("bibliothèque non autonome (lecture seule)"));
    let (sortie, _rx) = canal();
    assert_eq!(lie.lancer_scan_auto(sortie), Ok(false));
}

#[test]
fn option_suivre_comicrack_dans_la_surveillance() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data);
    assert!(b.suivre_comicrack());
    // Base de ComicRust : toujours surveillée, quelle que soit l'option.
    assert!(b.etat_base().surveillee);
    reglages::enregistrer_reglages_analyse(&data, ReglagesAnalyse { suivre_comicrack: Some(false), ..Default::default() })
        .unwrap();
    assert!(!b.suivre_comicrack());
    assert!(b.etat_base().surveillee, "l'option ne concerne pas la base de ComicRust");
    reglages::enregistrer_reglages_analyse(&data, ReglagesAnalyse { suivre_comicrack: Some(true), ..Default::default() })
        .unwrap();
    assert!(b.suivre_comicrack());
}

/// Mode lié par MySQL (serveur injoignable) : avec l'option désactivée, le sondage ne touche pas
/// la base (pas de passage hors ligne) ; réactivée, il la sonde et constate l'échec.
#[test]
fn sondage_comicrack_selon_l_option_de_suivi() {
    use crate::surveillance::{EvenementBase, Sondeur};
    use comicrust_core::db::{ComicDatabase, DbConfig};
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    b.installer(Mode::Comicrack, ComicDatabase::from_books(vec![]), None);
    b.installer_comicrack_mysql(DbConfig {
        host: "127.0.0.1".into(),
        port: 1,
        user: "cr".into(),
        password: "x".into(),
        database: "cr_test".into(),
    });
    let suivre = |v: bool| {
        reglages::enregistrer_reglages_analyse(
            &data,
            ReglagesAnalyse { suivre_comicrack: Some(v), ..Default::default() },
        )
        .unwrap();
    };
    suivre(false);
    assert!(!b.etat_base().surveillee, "ComicRack non suivi : base non surveillée");
    let mut s = Sondeur::nouveau(b.clone());
    let evts = s.sonder();
    assert!(
        evts.iter().all(|e| matches!(e, EvenementBase::Etat(x) if x.en_ligne && !x.surveillee)),
        "aucune connexion tentée : {evts:?}"
    );
    assert!(b.etat_base().en_ligne);
    // Option réactivée : la base est sondée, le serveur ne répond pas.
    suivre(true);
    assert!(b.etat_base().surveillee);
    s.sonder();
    assert!(!b.etat_base().en_ligne, "serveur injoignable constaté");
    // Désactivée de nouveau : le sondage ne retente plus et l'état revient en ligne.
    suivre(false);
    s.sonder();
    assert!(b.etat_base().en_ligne);
}
