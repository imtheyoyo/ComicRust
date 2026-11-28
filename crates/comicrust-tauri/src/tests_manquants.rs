//! Tests des livres manquants : dossiers temporaires, base SQLite autonome (analyse de CBZ générés
//! ici) et `ComicDb.xml` en lecture seule. Aucun MySQL réel.

use super::*;
use crate::biblio::{ChoixPremierDemarrage, EvenementScan, Groupe, RequeteLivres, SensTri};
use crate::couvertures::cbz_de_test;
use crate::reglages::Mode;
use crate::surveillance::{Sondeur, MOTIF_HORS_LIGNE};
use comicrust_core::store::StoreConfig;
use std::sync::mpsc;
use std::time::Duration;

/// Racine d'un volume qui n'existe pas (NAS débranché).
const VOLUME_ABSENT: &str = "/mnt/nas-absent-lot-l3";

fn info(serie: &str, numero: &str) -> String {
    format!("<?xml version=\"1.0\"?><ComicInfo><Series>{serie}</Series><Number>{numero}</Number></ComicInfo>")
}

/// Trois CBZ de tailles différentes : `Thanos 01`, `Thanos 02` et `Extra/Hors-serie`.
fn creer_bd(racine: &Path) {
    std::fs::create_dir_all(racine.join("Thanos")).unwrap();
    std::fs::create_dir_all(racine.join("Extra")).unwrap();
    let c = [180, 40, 40, 255];
    cbz_de_test(&racine.join("Thanos").join("Thanos 01.cbz"), c, 2, Some(&info("Thanos", "1")));
    cbz_de_test(&racine.join("Thanos").join("Thanos 02.cbz"), c, 3, Some(&info("Thanos", "2")));
    cbz_de_test(&racine.join("Extra").join("Hors-serie.cbz"), c, 4, None);
}

fn scanner(b: &Biblio) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    b.lancer_scan(Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    }))
    .unwrap();
    loop {
        match rx.recv_timeout(Duration::from_secs(60)).expect("l'analyse doit se terminer") {
            EvenementScan::Termine { .. } => return,
            EvenementScan::Erreur { message } => panic!("analyse : {message}"),
            _ => {}
        }
    }
}

struct Banc {
    /// Garde le dossier temporaire vivant.
    _d: tempfile::TempDir,
    data: PathBuf,
    bd: PathBuf,
    biblio: Biblio,
    service: Manquants,
    evenements: Arc<Mutex<Vec<EvenementManquants>>>,
}

fn service(biblio: &Biblio) -> (Manquants, Arc<Mutex<Vec<EvenementManquants>>>) {
    let evenements: Arc<Mutex<Vec<EvenementManquants>>> = Arc::default();
    let e = evenements.clone();
    (Manquants::nouveau(biblio.clone(), Arc::new(move |ev| e.lock().unwrap().push(ev))), evenements)
}

/// Bibliothèque autonome avec les trois CBZ analysés.
fn banc() -> Banc {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let bd = d.path().join("bd");
    creer_bd(&bd);
    let biblio = Biblio::nouvelle(data.clone());
    biblio.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    biblio.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    scanner(&biblio);
    let (service, evenements) = service(&biblio);
    Banc { _d: d, data, bd, biblio, service, evenements }
}

fn requete(liste: Option<&str>) -> RequeteLivres {
    RequeteLivres {
        recherche: None,
        tri: "fichier".into(),
        sens_tri: SensTri::Asc,
        groupe: Some(Groupe::Aucun),
        debut: 0,
        nombre: 100,
        liste: liste.map(str::to_string),
        serie: None,
    }
}

impl Banc {
    fn id_de(&self, nom: &str) -> String {
        let base = self.biblio.base().unwrap();
        base.books.iter().find(|b| b.file.as_deref().is_some_and(|f| f.ends_with(nom))).unwrap().id.clone()
    }

    fn fichier(&self, nom: &str) -> PathBuf {
        let base = self.biblio.base().unwrap();
        PathBuf::from(base.books.iter().find_map(|b| b.file.clone().filter(|f| f.ends_with(nom))).unwrap())
    }

    /// Livres tels que la base de ComicRust les relit.
    fn en_base(&self) -> Vec<LibraryBook> {
        let cfg = StoreConfig::Sqlite(self.data.join(comicrust_core::store::SQLITE_FILE_NAME));
        Store::open_existing(&cfg).unwrap().load(None).unwrap().books
    }

    fn manquant_en_base(&self, id: &str) -> Option<bool> {
        self.en_base().into_iter().find(|b| b.id == id).unwrap().missing
    }

    fn total(&self, liste: Option<&str>) -> usize {
        self.biblio.lister(&requete(liste)).unwrap().total
    }
}

// ───────────────────────────── Examen d'un fichier ─────────────────────────────

#[test]
fn sonde_present_introuvable_et_volume_absent() {
    let d = tempfile::tempdir().unwrap();
    let cbz = d.path().join("a.cbz");
    std::fs::write(&cbz, b"BD").unwrap();
    let f = cbz.to_string_lossy().into_owned();
    assert_eq!(sonder_fichier(&f), Sonde::Present);
    // ComicRack réécrit le fichier : `.tmp` présent, le livre n'est pas manquant.
    std::fs::rename(&cbz, d.path().join("a.cbz.tmp")).unwrap();
    assert_eq!(sonder_fichier(&f), Sonde::Present);
    std::fs::remove_file(d.path().join("a.cbz.tmp")).unwrap();
    // Fichier déplacé, volume joignable : introuvable.
    assert_eq!(sonder_fichier(&f), Sonde::Introuvable);
    // Volume absent : rien à conclure.
    assert_eq!(
        sonder_fichier(&format!("{VOLUME_ABSENT}/BD/Thanos 01.cbz")),
        Sonde::Injoignable(VOLUME_ABSENT.to_string())
    );
}

// ───────────────────────────── Vérification ─────────────────────────────

#[test]
fn fichier_deplace_marque_manquant_et_enregistre_dans_la_base() {
    let b = banc();
    let bilan = b.service.verifier().unwrap();
    assert_eq!((bilan.manquants, bilan.presents, bilan.concluant), (0, 3, true), "{bilan:?}");
    assert_eq!(b.service.etat().nb_manquants, 0);
    assert_eq!(b.total(Some(ID_LISTE_MANQUANTS)), 0);

    // Un fichier déplacé hors du dossier surveillé.
    let id = b.id_de("Thanos 02.cbz");
    let ancien = b.fichier("Thanos 02.cbz");
    std::fs::create_dir_all(b.bd.parent().unwrap().join("ailleurs")).unwrap();
    std::fs::rename(&ancien, b.bd.parent().unwrap().join("ailleurs").join("Thanos 02.cbz")).unwrap();

    let bilan = b.service.verifier().unwrap();
    assert_eq!((bilan.manquants, bilan.presents, bilan.concluant), (1, 2, true), "{bilan:?}");
    assert_eq!(bilan.avertissement, None);
    assert_eq!(b.service.etat().nb_manquants, 1);
    assert_eq!(b.service.etat().derniere.as_ref().map(|d| d.manquants), Some(1));

    // Badge dans la grille et le tableau, liste virtuelle, détail cohérent.
    let tous = b.biblio.lister(&requete(None)).unwrap();
    assert_eq!(tous.total, 3);
    assert_eq!(tous.livres.iter().filter(|l| l.manquant).map(|l| l.id.as_str()).collect::<Vec<_>>(), [id.as_str()]);
    let manquants = b.biblio.lister(&requete(Some(ID_LISTE_MANQUANTS))).unwrap();
    assert_eq!(manquants.total, 1);
    assert_eq!(manquants.livres[0].id, id);
    assert!(b.biblio.detail(&id).unwrap().manquant);
    // Enregistré dans la base de ComicRust (comme l'analyse des dossiers).
    assert_eq!(b.manquant_en_base(&id), Some(true));
    // La vue a été invalidée : un événement a été émis pour rafraîchir l'affichage.
    assert!(b
        .evenements
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, EvenementManquants::Modifiee { ids, .. } if ids == std::slice::from_ref(&id))));

    // Le fichier revient : le livre n'est plus manquant, partout.
    std::fs::rename(b.bd.parent().unwrap().join("ailleurs").join("Thanos 02.cbz"), &ancien).unwrap();
    let bilan = b.service.verifier().unwrap();
    assert_eq!((bilan.manquants, bilan.presents), (0, 3));
    assert_eq!(b.service.etat().nb_manquants, 0);
    assert!(!b.biblio.detail(&id).unwrap().manquant);
    assert_ne!(b.manquant_en_base(&id), Some(true));
}

#[test]
fn volume_absent_ne_marque_rien() {
    let b = banc();
    let id = b.id_de("Thanos 01.cbz");
    // Le NAS est débranché : la fiche pointe vers un volume qui ne répond pas.
    let mut fiche = b.biblio.livre(&id).unwrap();
    let chemin = format!("{VOLUME_ABSENT}/BD/Thanos/Thanos 01.cbz");
    fiche.file = Some(chemin.clone());
    b.biblio.remplacer_fiches(&[fiche], false);

    let bilan = b.service.verifier().unwrap();
    assert_eq!(bilan.manquants, 0, "{bilan:?}");
    assert_eq!(bilan.injoignables, [VolumeSignale { racine: VOLUME_ABSENT.into(), livres: 1 }]);
    assert!(!bilan.concluant);
    assert!(bilan.resume.contains(VOLUME_ABSENT), "{}", bilan.resume);
    assert_eq!(b.service.etat().nb_manquants, 0);
    assert_eq!(b.manquant_en_base(&id), None);

    // Ouverture : volume injoignable, le livre n'est pas marqué manquant.
    let s = b.service.sonder_livre(&id).unwrap();
    assert_eq!(s.etat, EtatFichier::VolumeInjoignable);
    assert_eq!(s.racine.as_deref(), Some(VOLUME_ABSENT));
    assert_eq!(s.fichier, chemin);
    assert!(!b.biblio.detail(&id).unwrap().manquant);
    assert_eq!(b.manquant_en_base(&id), None);
    // Le retrait exige une confirmation explicite : le volume ne répond pas.
    let prep = b.service.preparer_retrait(std::slice::from_ref(&id)).unwrap();
    assert!(prep.confirmation_requise && !prep.bilan.concluant);
    assert!(b.service.retirer(std::slice::from_ref(&id), false).unwrap_err().contains("confirmation requise"));
    assert_eq!(b.total(None), 3, "rien n'a été retiré");
}

#[test]
fn ouverture_d_un_livre_manquant_le_marque_et_present_le_remet() {
    let b = banc();
    let id = b.id_de("Hors-serie.cbz");
    let fichier = b.fichier("Hors-serie.cbz");
    let s = b.service.sonder_livre(&id).unwrap();
    assert_eq!((s.etat, s.racine), (EtatFichier::Present, None));

    let cache = fichier.with_extension("cache");
    std::fs::rename(&fichier, &cache).unwrap();
    let s = b.service.sonder_livre(&id).unwrap();
    assert_eq!(s.etat, EtatFichier::Introuvable);
    assert_eq!(s.titre, b.biblio.livre(&id).unwrap().caption());
    assert!(b.biblio.detail(&id).unwrap().manquant);
    assert_eq!(b.manquant_en_base(&id), Some(true));
    assert_eq!(b.total(Some(ID_LISTE_MANQUANTS)), 1);

    std::fs::rename(&cache, &fichier).unwrap();
    assert_eq!(b.service.sonder_livre(&id).unwrap().etat, EtatFichier::Present);
    assert!(!b.biblio.detail(&id).unwrap().manquant);
    assert_ne!(b.manquant_en_base(&id), Some(true));
    assert_eq!(b.total(Some(ID_LISTE_MANQUANTS)), 0);
}

#[test]
fn fiche_deplacee_pendant_la_verification_n_est_jamais_marquee() {
    let b = banc();
    let id = b.id_de("Thanos 01.cbz");
    let vu = b.fichier("Thanos 01.cbz").to_string_lossy().into_owned();
    // Une marque pour un chemin qui n'est plus celui de la fiche : ignorée.
    let modifies = b.biblio.marquer_manquants(&[(id.clone(), format!("{vu}.ancien"), true)]);
    assert!(modifies.is_empty());
    assert_eq!(b.service.compter(), 0);
    let modifies = b.biblio.marquer_manquants(&[(id.clone(), vu, true)]);
    assert_eq!(modifies, [id]);
    assert_eq!(b.service.compter(), 1);
}

// ───────────────────────────── Retrouver le fichier ─────────────────────────────

#[test]
fn fichier_deplace_retrouve_par_nom_dans_un_dossier() {
    let b = banc();
    let id = b.id_de("Thanos 02.cbz");
    let ancien = b.fichier("Thanos 02.cbz");
    let nouveau_dossier = b.bd.parent().unwrap().join("range").join("sous-dossier");
    std::fs::create_dir_all(&nouveau_dossier).unwrap();
    let nouveau = nouveau_dossier.join("Thanos 02.cbz");
    std::fs::rename(&ancien, &nouveau).unwrap();
    b.service.verifier().unwrap();
    assert_eq!(b.service.compter(), 1);

    let r = b
        .service
        .chercher(std::slice::from_ref(&id), &format!("\"{}\"", b.bd.parent().unwrap().join("range").display()))
        .unwrap();
    assert_eq!(r.propositions.len(), 1, "{r:?}");
    let p = &r.propositions[0];
    assert_eq!((p.id.as_str(), p.coche, p.sure, p.autres), (id.as_str(), true, true, 0));
    assert_eq!(PathBuf::from(&p.nouveau), nouveau);
    assert_eq!(p.ancien, ancien.to_string_lossy());
    assert_eq!(p.correspondance, "même nom et même taille");
    assert_eq!(r.non_retrouves, 0);
    assert!(r.fichiers_examines >= 1 && r.info.is_none() && !r.interrompu);

    let bilan = b
        .service
        .appliquer_relocalisations(&[DemandeRelocalisation {
            id: id.clone(),
            ancien: p.ancien.clone(),
            nouveau: p.nouveau.clone(),
        }])
        .unwrap();
    assert_eq!(bilan.nb, 1, "{bilan:?}");
    let fiche = b.biblio.livre(&id).unwrap();
    assert_eq!(fiche.file.as_deref(), Some(nouveau.to_string_lossy().as_ref()));
    assert_ne!(fiche.missing, Some(true));
    assert_eq!(b.service.compter(), 0);
    assert_eq!(b.total(Some(ID_LISTE_MANQUANTS)), 0);
    // La base de ComicRust a le nouveau chemin et plus de marque.
    let en_base = b.en_base().into_iter().find(|x| x.id == id).unwrap();
    assert_eq!(en_base.file.as_deref(), Some(nouveau.to_string_lossy().as_ref()));
    assert_ne!(en_base.missing, Some(true));
    // Les fichiers ne sont pas touchés.
    assert!(nouveau.is_file());
}

#[test]
fn recherche_sans_resultat_et_livres_non_manquants_ignores() {
    let b = banc();
    let vide = b.bd.parent().unwrap().join("vide");
    std::fs::create_dir_all(&vide).unwrap();
    let id = b.id_de("Thanos 01.cbz");
    let r = b.service.chercher(std::slice::from_ref(&id), &vide.to_string_lossy()).unwrap();
    assert!(r.propositions.is_empty());
    assert!(r.info.as_deref().unwrap().starts_with("Aucun fichier retrouvé dans"), "{:?}", r.info);
    // Plusieurs livres dont aucun n'est manquant : rien à chercher.
    let tous: Vec<String> = b.biblio.base().unwrap().books.iter().map(|x| x.id.clone()).collect();
    let r = b.service.chercher(&tous, &vide.to_string_lossy()).unwrap();
    assert_eq!(r.info.as_deref(), Some("Aucun livre manquant à rechercher."));
    // Dossier inexistant : erreur claire.
    let e = b.service.chercher(&[id], &format!("{}/inexistant", vide.display())).unwrap_err();
    assert!(e.starts_with("dossier introuvable"), "{e}");
}

#[test]
fn fichier_choisi_a_la_main_nom_taille_et_doublon() {
    let b = banc();
    let id = b.id_de("Thanos 02.cbz");
    let ancien = b.fichier("Thanos 02.cbz");
    // Fichier renommé : même taille, nom différent (à vérifier, jamais coché d'office).
    let renomme = ancien.with_file_name("Thanos 2 (renommé).cbz");
    std::fs::rename(&ancien, &renomme).unwrap();
    let p = b.service.proposer_fichier(&id, &format!("  \"{}\"  ", renomme.display())).unwrap();
    assert_eq!(p.correspondance, "même taille, nom différent (à vérifier)");
    assert!(!p.sure && !p.coche && p.doublon.is_none());
    assert_eq!(PathBuf::from(&p.nouveau), renomme);

    // Même nom, même taille que la fiche : sûr, coché.
    std::fs::rename(&renomme, &ancien).unwrap();
    let p = b.service.proposer_fichier(&id, &ancien.to_string_lossy()).unwrap();
    assert!(p.sure && p.coche, "{p:?}");

    // Fichier déjà dans la bibliothèque (autre livre) : doublon signalé, décoché.
    let autre = b.fichier("Thanos 01.cbz");
    let p = b.service.proposer_fichier(&id, &autre.to_string_lossy()).unwrap();
    assert!(p.doublon.is_some() && !p.coche, "{p:?}");

    // Chemins invalides.
    assert!(b.service.proposer_fichier(&id, "").unwrap_err().contains("vide"));
    assert!(b
        .service
        .proposer_fichier(&id, &format!("{}/rien.cbz", b.bd.display()))
        .unwrap_err()
        .starts_with("fichier introuvable"));
    assert!(b
        .service
        .proposer_fichier(&id, &b.bd.to_string_lossy())
        .unwrap_err()
        .starts_with("ce n'est pas un fichier"));
    assert!(b.service.proposer_fichier("inconnu", &ancien.to_string_lossy()).unwrap_err().contains("introuvable"));
}

#[test]
fn relocalisation_ignore_une_fiche_modifiee_entre_temps() {
    let b = banc();
    let id = b.id_de("Thanos 02.cbz");
    let nouveau = b.fichier("Thanos 01.cbz");
    let bilan = b
        .service
        .appliquer_relocalisations(&[DemandeRelocalisation {
            id,
            ancien: "D:/autre/chemin.cbz".into(),
            nouveau: nouveau.to_string_lossy().into_owned(),
        }])
        .unwrap();
    assert_eq!(bilan.nb, 0);
    assert!(bilan.info.contains("ignorée"), "{}", bilan.info);
}

// ───────────────────────────── Retirer de la bibliothèque ─────────────────────────────

#[test]
fn retirer_un_livre_manquant_ne_touche_jamais_les_fichiers() {
    let b = banc();
    let id = b.id_de("Hors-serie.cbz");
    let autre = b.fichier("Thanos 01.cbz");
    std::fs::remove_file(b.fichier("Hors-serie.cbz")).unwrap();
    b.service.verifier().unwrap();

    let prep = b.service.preparer_retrait(std::slice::from_ref(&id)).unwrap();
    assert_eq!(prep.ids, std::slice::from_ref(&id));
    assert_eq!((prep.autres, prep.legendes.len()), (0, 1));
    assert!(!prep.confirmation_requise, "fichier bien disparu, volume sûr, un seul livre");
    assert!(!prep.comicrack);

    let bilan = b.service.retirer(std::slice::from_ref(&id), false).unwrap();
    assert_eq!(bilan.nb, 1, "{bilan:?}");
    assert!(bilan.info.contains("fichiers non touchés"));
    assert!(b.biblio.livre(&id).is_none());
    assert_eq!(b.total(None), 2);
    assert_eq!(b.service.compter(), 0);
    assert_eq!(b.total(Some(ID_LISTE_MANQUANTS)), 0);
    assert!(b.en_base().iter().all(|x| x.id != id), "supprimé de la base");
    assert!(autre.is_file(), "les autres fichiers ne sont pas touchés");
    assert!(b.evenements.lock().unwrap().iter().any(|e| matches!(e, EvenementManquants::Modifiee { retires: 1, .. })));
}

#[test]
fn retirer_un_livre_dont_le_fichier_existe_exige_la_confirmation() {
    let b = banc();
    let id = b.id_de("Thanos 01.cbz");
    let fichier = b.fichier("Thanos 01.cbz");
    let prep = b.service.preparer_retrait(std::slice::from_ref(&id)).unwrap();
    assert!(prep.confirmation_requise);
    assert_eq!(prep.bilan.presents, 1);
    let e = b.service.retirer(std::slice::from_ref(&id), false).unwrap_err();
    assert!(e.contains("confirmation requise"), "{e}");
    assert!(b.biblio.livre(&id).is_some(), "refusé : rien n'est retiré");
    let bilan = b.service.retirer(std::slice::from_ref(&id), true).unwrap();
    assert_eq!(bilan.nb, 1);
    assert!(b.biblio.livre(&id).is_none());
    assert!(fichier.is_file(), "le fichier n'est jamais supprimé");
}

#[test]
fn regles_de_confirmation_du_retrait() {
    let vide = CheckReport::default();
    assert!(!retrait_exige_confirmation(1, &vide));
    assert!(retrait_exige_confirmation(SEUIL_CONFIRMATION_RETRAIT + 1, &vide), "plus de 100 livres");
    assert!(!retrait_exige_confirmation(SEUIL_CONFIRMATION_RETRAIT, &vide));
    let present = CheckReport { present: vec!["a".into()], ..Default::default() };
    assert!(retrait_exige_confirmation(1, &present));
    let annule = CheckReport { cancelled: true, ..Default::default() };
    assert!(retrait_exige_confirmation(1, &annule));
    // Jamais avant la fin de la vérification.
    assert!(!retrait_permis(None, 1, true));
    assert!(!retrait_permis(Some(&present), 1, false));
    assert!(retrait_permis(Some(&present), 1, true));
    assert!(retrait_permis(Some(&vide), 1, false));
}

#[test]
fn retirer_plusieurs_livres_sans_doublon_ni_inconnu() {
    let b = banc();
    let a = b.id_de("Thanos 01.cbz");
    let c = b.id_de("Thanos 02.cbz");
    std::fs::remove_file(b.fichier("Thanos 01.cbz")).unwrap();
    std::fs::remove_file(b.fichier("Thanos 02.cbz")).unwrap();
    let ids = vec![a.clone(), c.clone(), a.to_uppercase(), "inconnu".to_string()];
    let prep = b.service.preparer_retrait(&ids).unwrap();
    assert_eq!(prep.ids.len(), 2, "sans doublon (casse comprise) ni livre inconnu");
    let bilan = b.service.retirer(&ids, false).unwrap();
    assert_eq!(bilan.nb, 2);
    assert_eq!(b.total(None), 1);
    assert!(b.service.retirer(&["inconnu".to_string()], true).unwrap_err().contains("aucun livre connu"));
}

// ───────────────────────────── Lecture seule et hors ligne ─────────────────────────────

#[test]
fn comicdb_xml_en_lecture_seule_rien_n_est_ecrit() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    std::fs::create_dir_all(&racine).unwrap();
    let fichier = racine.join("Thanos 01.cbz");
    cbz_de_test(&fichier, [10, 20, 30, 255], 2, None);
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(
        &xml,
        format!(
            "<ComicDatabase><Books><Book Id=\"11111111-0000-0000-0000-000000000001\" File=\"{}\">\
             <Series>Thanos</Series><Number>1</Number></Book></Books></ComicDatabase>",
            fichier.display()
        ),
    )
    .unwrap();
    let avant = std::fs::read(&xml).unwrap();
    let biblio = Biblio::nouvelle(d.path().join("data"));
    biblio
        .configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(xml.to_string_lossy().into()) })
        .unwrap();
    let (service, _) = service(&biblio);
    let id = "11111111-0000-0000-0000-000000000001".to_string();

    // La vérification et la marque en mémoire restent possibles : elles n'écrivent rien.
    std::fs::remove_file(&fichier).unwrap();
    let bilan = service.verifier().unwrap();
    assert_eq!((bilan.manquants, bilan.avertissement), (1, None));
    assert_eq!(service.etat().nb_manquants, 1);
    assert_eq!(service.etat().raison_lecture_seule.as_deref(), Some(LECTURE_SEULE_XML));

    // Retrouver, chercher, retirer : refusés avec la raison, avant toute écriture.
    let nouveau = d.path().join("ailleurs.cbz");
    cbz_de_test(&nouveau, [10, 20, 30, 255], 2, None);
    assert_eq!(service.proposer_fichier(&id, &nouveau.to_string_lossy()).unwrap_err(), LECTURE_SEULE_XML);
    assert_eq!(
        service.chercher(std::slice::from_ref(&id), &d.path().to_string_lossy()).unwrap_err(),
        LECTURE_SEULE_XML
    );
    let demande = DemandeRelocalisation {
        id: id.clone(),
        ancien: fichier.to_string_lossy().into_owned(),
        nouveau: nouveau.to_string_lossy().into_owned(),
    };
    assert_eq!(service.appliquer_relocalisations(&[demande]).unwrap_err(), LECTURE_SEULE_XML);
    assert_eq!(service.preparer_retrait(std::slice::from_ref(&id)).unwrap_err(), LECTURE_SEULE_XML);
    assert_eq!(service.retirer(std::slice::from_ref(&id), true).unwrap_err(), LECTURE_SEULE_XML);
    assert!(biblio.livre(&id).is_some());
    assert_eq!(std::fs::read(&xml).unwrap(), avant, "ComicDb.xml n'est jamais réécrit");
}

#[test]
fn mysql_comicrack_sans_ecriture_est_en_lecture_seule() {
    let d = tempfile::tempdir().unwrap();
    let biblio = Biblio::nouvelle(d.path().join("data"));
    biblio.installer(Mode::Comicrack, ComicDatabase::from_books(vec![]), None);
    biblio.installer_comicrack_mysql(DbConfig {
        host: "serveur-factice.invalid".into(),
        port: 3306,
        user: "u".into(),
        password: "mot-de-passe-factice".into(),
        database: "b".into(),
    });
    let (service, _) = service(&biblio);
    assert_eq!(service.raison_lecture_seule().as_deref(), Some(LECTURE_SEULE_MYSQL));
    assert_eq!(service.retirer(&["x".to_string()], true).unwrap_err(), LECTURE_SEULE_MYSQL);
    // Entrées grisées des actions sur les fichiers : texte qui parle des fichiers des livres,
    // le même que le refus du backend (`Cible::fichiers_modifiables`).
    let e = service.etat();
    assert_eq!(e.raison_lecture_seule.as_deref(), Some(LECTURE_SEULE_MYSQL));
    assert_eq!(e.raison_fichiers.as_deref(), Some(crate::edition::LECTURE_SEULE_FICHIERS));
    assert!(crate::edition::LECTURE_SEULE_FICHIERS.contains("ni ses fiches ni les fichiers des livres"));
    assert_eq!(
        crate::manquants::cible_ecriture(&biblio).fichiers_modifiables().unwrap_err(),
        crate::edition::LECTURE_SEULE_FICHIERS
    );
    let j = serde_json::to_value(&e).unwrap();
    assert_eq!(j["raisonFichiers"], crate::edition::LECTURE_SEULE_FICHIERS);
}

#[test]
fn base_injoignable_refuse_les_ecritures_et_ne_marque_pas_la_base() {
    let b = banc();
    let id = b.id_de("Hors-serie.cbz");
    std::fs::remove_file(b.fichier("Hors-serie.cbz")).unwrap();
    // Base écartée : une base SQLite disparue est une erreur, comme un serveur arrêté.
    let mut s = Sondeur::nouveau(b.biblio.clone());
    let ecarte = b.data.with_extension("ecarte");
    std::fs::rename(&b.data, &ecarte).unwrap();
    s.sonder();
    assert!(b.biblio.motif_hors_ligne().is_some());
    assert_eq!(b.service.raison_lecture_seule().as_deref(), Some(MOTIF_HORS_LIGNE));

    // Les écritures sont refusées, avec le message de l'édition.
    assert_eq!(b.service.preparer_retrait(std::slice::from_ref(&id)).unwrap_err(), MOTIF_HORS_LIGNE);
    assert_eq!(b.service.retirer(std::slice::from_ref(&id), true).unwrap_err(), MOTIF_HORS_LIGNE);
    assert_eq!(b.service.proposer_fichier(&id, &b.bd.to_string_lossy()).unwrap_err(), MOTIF_HORS_LIGNE);
    // La marque en mémoire reste possible (consultation) ; la base n'est pas écrite : dit.
    let bilan = b.service.verifier().unwrap();
    assert_eq!(bilan.manquants, 1);
    assert!(bilan.avertissement.as_deref().unwrap().contains("Base injoignable"), "{bilan:?}");
    assert_eq!(b.service.compter(), 1);

    // Retour de la base : elle n'a jamais reçu la marque.
    std::fs::rename(&ecarte, &b.data).unwrap();
    assert_ne!(b.manquant_en_base(&id), Some(true));
}

#[test]
fn verification_refusee_pendant_une_analyse_et_arret_sans_effet_sans_travail() {
    let b = banc();
    b.service.arreter(); // rien en cours : sans effet
    assert_eq!(b.service.etat().verification, None);
    assert_eq!(b.service.etat().recherche, None);
    // Deux vérifications à la fois : la seconde est refusée (emplacement pris puis libéré).
    let (occupation, _, _) = b.service.occuper(Genre::Verification, None).unwrap();
    assert!(b.service.verifier().unwrap_err().contains("déjà en cours"));
    assert!(b.service.etat().verification.is_some());
    drop(occupation);
    assert!(b.service.verifier().is_ok());
}

#[test]
fn contrat_json_de_l_etat_et_des_resumes() {
    let b = banc();
    let etat = serde_json::to_value(b.service.etat()).unwrap();
    assert_eq!(etat["nbManquants"], 0);
    assert!(etat.get("verification").is_none() && etat.get("raisonLectureSeule").is_none());
    let l = serde_json::to_value(&b.biblio.lister(&requete(None)).unwrap().livres[0]).unwrap();
    assert!(l.get("manquant").is_none(), "absent quand faux");
    let (nom, charge) = EvenementManquants::Modifiee { ids: vec!["a".into()], retires: 2 }.pour_le_front();
    assert_eq!(nom, "bibliotheque-modifiee");
    assert_eq!(charge["origine"], "manquants");
    assert_eq!(charge["supprimes"], 2);
}

/// Relecture tauri-v1.0.2 : écriture MySQL de ComicRack partielle — les fiches écrites sont
/// rendues avec l'erreur (avant, l'erreur seule : fiches écrites jamais reportées à l'affichage).
#[test]
fn ecriture_comicrack_partielle_rend_les_fiches_ecrites() {
    use comicrust_core::db::{Record, WriteReport, Written};
    let livre = LibraryBook { id: "a".into(), file: Some("D:/BD/a.cbz".into()), ..Default::default() };
    let rapport = WriteReport {
        written: vec![Written {
            id: "a".into(),
            record: Record { data: String::new(), counter: 2 },
            book: livre.clone(),
            retries: 0,
        }],
        failed: vec![("b".into(), "valeur refusée".into())],
        ..Default::default()
    };
    let (ecrites, erreur) = fiches_et_erreur(&rapport);
    assert_eq!(
        ecrites.iter().map(|b| (b.id.as_str(), b.file.as_deref())).collect::<Vec<_>>(),
        [("a", livre.file.as_deref())]
    );
    assert_eq!(erreur.as_deref(), Some("b : valeur refusée"));
    let (_, aucune) = fiches_et_erreur(&WriteReport::default());
    assert!(aucune.is_none());
}
