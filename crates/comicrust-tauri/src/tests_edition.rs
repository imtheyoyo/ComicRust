//! Tests de l'édition : bibliothèque autonome SQLite (CBZ générés ici), ComicInfo.xml écrit dans
//! le CBZ (champs touchés seulement), valeurs multiples, validation, suggestions, garde-fou des
//! opérations en masse, renommage (aperçu, collisions), conversion CBR / CB7 → CBZ (échantillons
//! de `recette/echantillons`), lecture seule (ComicDb.xml), et base MySQL de ComicRack si
//! `COMICRUST_TEST_MYSQL` est définie (base de test seulement).

use super::*;
use crate::biblio::{Biblio, ChoixPremierDemarrage, EvenementScan};
use crate::couvertures::cbz_de_test;
use crate::lecteur::Lecteur;
use crate::reglages::Mode;
use std::io::Read;
use std::sync::mpsc;

fn info(serie: &str, numero: &str, editeur: &str, extra: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><ComicInfo><Series>{serie}</Series><Number>{numero}</Number>\
         <Publisher>{editeur}</Publisher><Year>2020</Year><Writer>Jonathan Hickman</Writer>{extra}</ComicInfo>"
    )
}

/// Bibliothèque autonome analysée sur `racine`.
fn autonome(data: &Path, racine: &Path) -> (Biblio, Edition) {
    let b = Biblio::nouvelle(data.to_path_buf());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    b.ajouter_dossier(&racine.to_string_lossy()).unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    b.lancer_scan(Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    }))
    .unwrap();
    loop {
        match rx.recv_timeout(Duration::from_secs(60)).expect("analyse terminée") {
            EvenementScan::Termine { .. } => break,
            EvenementScan::Erreur { message } => panic!("{message}"),
            _ => {}
        }
    }
    let l = Lecteur::nouveau(data.to_path_buf(), b.clone());
    let e = Edition::nouvelle(b.clone(), l);
    (b, e)
}

/// Collecteur d'événements.
#[derive(Default)]
struct Evenements(Mutex<Vec<EvenementEdition>>);

impl Evenements {
    fn sortie(&self) -> impl Fn(EvenementEdition) + Send + Sync + '_ {
        move |e| self.0.lock().unwrap().push(e)
    }
    fn modifies(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                EvenementEdition::BibliothequeModifiee { ids } => Some(ids.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}

fn id_par_fichier(b: &Biblio, nom: &str) -> String {
    let base = b.base().unwrap();
    base.books.iter().find(|x| x.file.as_deref().is_some_and(|f| f.ends_with(nom))).expect(nom).id.clone()
}

fn comic_info(chemin: &Path) -> (String, Vec<String>) {
    let mut z = zip::ZipArchive::new(std::fs::File::open(chemin).unwrap()).unwrap();
    let noms: Vec<String> = z.file_names().map(str::to_string).collect();
    let mut t = String::new();
    z.by_name("ComicInfo.xml").unwrap().read_to_string(&mut t).unwrap();
    (t, noms)
}

fn champ<'a>(f: &'a FicheEdition, nom: &str) -> &'a ChampFiche {
    f.champs.iter().find(|c| c.nom == nom).unwrap_or_else(|| panic!("{nom}"))
}

fn modif(nom: &str, valeur: &str) -> ChampModifie {
    ChampModifie { nom: nom.into(), valeur: valeur.into() }
}

fn bd(racine: &Path) {
    let c = [180, 40, 40, 255];
    std::fs::create_dir_all(racine).unwrap();
    cbz_de_test(
        &racine.join("Thanos 01.cbz"),
        c,
        3,
        Some(&info("Thanos", "1", "Marvel", "<GTIN>978-1</GTIN><Notes>gardée</Notes>")),
    );
    cbz_de_test(&racine.join("Thanos 02.cbz"), c, 3, Some(&info("Thanos", "2", "Marvel", "")));
    cbz_de_test(&racine.join("Asterix 01.cbz"), c, 2, Some(&info("Astérix", "1", "Dargaud", "")));
}

#[test]
fn fiches_un_livre_plusieurs_livres_onglets_et_valeurs_multiples() {
    let d = tempfile::tempdir().unwrap();
    bd(&d.path().join("BD"));
    let (b, e) = autonome(&d.path().join("data"), &d.path().join("BD"));
    let t1 = id_par_fichier(&b, "Thanos 01.cbz");
    let t2 = id_par_fichier(&b, "Thanos 02.cbz");

    let f = e.lire_fiches(std::slice::from_ref(&t1)).unwrap();
    assert!(!f.multiple && f.lecture_seule.is_none() && f.ecrire_dans_fichier_possible);
    assert_eq!(champ(&f, "Series").valeur, "Thanos");
    assert_eq!(champ(&f, "Series").type_champ, TypeChamp::Texte);
    assert_eq!(champ(&f, "Writer").type_champ, TypeChamp::Liste);
    assert_eq!(champ(&f, "Checked").valeur, "Oui", "booléen absent : valeur par défaut");
    assert_eq!(champ(&f, "Manga").choix.as_ref().unwrap().len(), 4);
    assert_eq!((champ(&f, "Month").min, champ(&f, "Month").max), (Some(1), Some(12)));
    assert!(champ(&f, "Publisher").suggestions && !champ(&f, "Title").suggestions);
    assert!(f.champs.iter().all(|c| c.nom != "CustomValuesStore"));
    let noms: Vec<&str> = f.onglets.iter().map(|o| o.nom.as_str()).collect();
    assert_eq!(noms, ["Résumé", "Détails", "Intrigue et notes", "Catalogue", "Personnalisé"]);
    // Chaque champ du formulaire est dans exactement un onglet (hors valeurs perso.).
    let mut places: Vec<&str> = ONGLETS.iter().flat_map(|(_, c)| c.iter().copied()).collect();
    places.sort();
    let mut tous: Vec<&str> = champs_du_formulaire().map(|d| d.name).collect();
    tous.sort();
    assert_eq!(places, tous);

    let f = e.lire_fiches(&[t1.clone(), t2.clone(), "inconnu".into()]).unwrap();
    assert!(f.multiple);
    assert_eq!(f.ids.len(), 2, "livre inconnu ignoré");
    let n = champ(&f, "Number");
    assert!(n.multiple && n.valeur.is_empty());
    let s = champ(&f, "Series");
    assert!(!s.multiple && s.valeur == "Thanos");
    assert!(e.lire_fiches(&["inconnu".into()]).is_err());

    // JSON du contrat.
    let j = serde_json::to_value(&f).unwrap();
    assert!(j.get("ecrireDansFichierPossible").is_some() && j.get("seuilConfirmation").is_some());
    let c = &j["champs"][0];
    assert!(c.get("type").is_some() && c.get("dansComicInfo").is_some(), "{c}");
}

#[test]
fn modifier_ecrit_la_base_et_le_comicinfo_des_champs_touches_seulement() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let t1 = id_par_fichier(&b, "Thanos 01.cbz");
    let t2 = id_par_fichier(&b, "Thanos 02.cbz");
    let ev = Evenements::default();

    // Plusieurs livres : seul le champ envoyé est écrit (les numéros, différents, restent).
    let dm = DemandeModification {
        ids: vec![t1.clone(), t2.clone()],
        champs: vec![modif("writer", "Jim Starlin, Ron Lim"), modif("Rating", "4,5")],
        pages: None,
        ecrire_dans_fichier: true,
        confirme: false,
    };
    let bilan = e.appliquer_modifications(&dm, &ev.sortie()).unwrap();
    assert!(bilan.complet, "{}", bilan.rapport);
    assert_eq!(bilan.livres.len(), 2);
    assert!(bilan.livres.iter().all(|l| l.statut == Statut::Ok), "{:?}", bilan.livres);
    let mut m = ev.modifies();
    m.sort();
    let mut attendu = vec![t1.clone(), t2.clone()];
    attendu.sort();
    m.dedup();
    assert_eq!(m, attendu, "bibliotheque-modifiee");
    let d1 = b.detail(&t1).unwrap();
    let d2 = b.detail(&t2).unwrap();
    assert_eq!(d1.scenariste.as_deref(), Some("Jim Starlin, Ron Lim"));
    assert_eq!((d1.numero.as_deref(), d2.numero.as_deref()), (Some("1"), Some("2")));
    assert_eq!(d1.note, Some(4.5));
    // Relu depuis la base (redémarrage) : écrit, et les infos sont « écrites dans le fichier ».
    let b2 = Biblio::nouvelle(d.path().join("data"));
    b2.ouvrir().unwrap();
    let f = b2.livre(&t1).unwrap();
    assert_eq!(f.writer.as_deref(), Some("Jim Starlin, Ron Lim"));
    assert_ne!(f.comic_info_is_dirty, Some(true), "fichier écrit : plus « à mettre à jour »");

    // ComicInfo.xml du CBZ : champs modifiés, les autres gardés (élément inconnu compris).
    let (xml, noms) = comic_info(&racine.join("Thanos 01.cbz"));
    assert!(xml.contains("<Writer>Jim Starlin, Ron Lim</Writer>"), "{xml}");
    assert!(xml.contains("<Series>Thanos</Series>") && xml.contains("<Number>1</Number>"), "{xml}");
    assert!(xml.contains("<GTIN>978-1</GTIN>") && xml.contains("<Notes>gardée</Notes>"), "{xml}");
    assert!(!xml.contains("<Rating>"), "la note est propre à la bibliothèque : {xml}");
    assert_eq!(noms.iter().filter(|n| n.ends_with(".png")).count(), 3, "pages recopiées");
    let reste: Vec<_> = std::fs::read_dir(&racine)
        .unwrap()
        .filter_map(|x| x.ok())
        .filter(|x| x.file_name().to_string_lossy().contains(".tmp"))
        .collect();
    assert!(reste.is_empty(), "aucun fichier temporaire laissé");
    assert!(reglages::lire_reglages_edition(&d.path().join("data")).unwrap().ecrire_dans_fichier, "case mémorisée");

    // Même demande : rien à écrire, ni fiche ni fichier.
    let avant = std::fs::metadata(racine.join("Thanos 01.cbz")).unwrap().modified().unwrap();
    let bilan = e.appliquer_modifications(&dm, &ev.sortie()).unwrap();
    assert!(bilan.livres.iter().all(|l| l.statut == Statut::Inchange), "{}", bilan.rapport);
    assert_eq!(std::fs::metadata(racine.join("Thanos 01.cbz")).unwrap().modified().unwrap(), avant);

    // Vider un champ ; écrire seulement les fichiers ; un format qui ne reçoit pas les infos.
    let dm = DemandeModification {
        ids: vec![t1.clone()],
        champs: vec![modif("Rating", "")],
        pages: None,
        ecrire_dans_fichier: false,
        confirme: false,
    };
    e.appliquer_modifications(&dm, &ev.sortie()).unwrap();
    assert_eq!(b.detail(&t1).unwrap().note, None);
    let r = e.ecrire_infos_fichiers(&[t1.clone(), t2.clone()], false, &ev.sortie()).unwrap();
    assert!(r.livres.iter().all(|l| l.statut == Statut::Inchange), "{}", r.rapport);
}

#[test]
fn valeurs_refusees_et_valeurs_perso() {
    let d = tempfile::tempdir().unwrap();
    bd(&d.path().join("BD"));
    let (b, e) = autonome(&d.path().join("data"), &d.path().join("BD"));
    let t1 = id_par_fichier(&b, "Thanos 01.cbz");
    let erreurs = verifier_champs(&[
        modif("Year", "deux mille"),
        modif("Month", "13"),
        modif("Rating", "3,5"),
        modif("Released", "31/02/2024"),
        modif("Inconnu", "x"),
        modif("PageCount", "3"),
    ]);
    let noms: Vec<&str> = erreurs.iter().map(|x| x.nom.as_str()).collect();
    assert_eq!(noms, ["Year", "Month", "Released", "Inconnu", "PageCount"]);
    assert!(erreurs[1].message.starts_with("13 hors limites"), "libellé retiré : {}", erreurs[1].message);
    assert!(verifier_champs(&[modif("Released", "01/02/2024"), modif("LanguageISO", "FR")]).is_empty());
    let dm = DemandeModification {
        ids: vec![t1.clone()],
        champs: vec![modif("Series", "Autre"), modif("Year", "deux mille")],
        pages: None,
        ecrire_dans_fichier: false,
        confirme: false,
    };
    let err = e.appliquer_modifications(&dm, &|_| {}).unwrap_err();
    assert!(err.contains("Année"), "{err}");
    assert_eq!(b.detail(&t1).unwrap().serie.as_deref(), Some("Thanos"), "rien n'est écrit");

    // Valeurs personnalisées : ajout, écriture, onglet « Personnalisé ».
    assert!(nouvelle_valeur_perso("a=b", &[]).is_err());
    assert!(nouvelle_valeur_perso(" ", &[]).is_err());
    assert!(nouvelle_valeur_perso("source", &["CustomValue:Source".into()]).is_err());
    let c = nouvelle_valeur_perso("Lu en", &[]).unwrap();
    assert_eq!((c.nom.as_str(), c.libelle.as_str()), ("CustomValue:Lu en", "Lu en"));
    let dm = DemandeModification {
        ids: vec![t1.clone()],
        champs: vec![modif(&c.nom, "2025"), modif("Released", "17/05/2024")],
        pages: None,
        ecrire_dans_fichier: false,
        confirme: false,
    };
    e.appliquer_modifications(&dm, &|_| {}).unwrap();
    let f = e.lire_fiches(std::slice::from_ref(&t1)).unwrap();
    assert_eq!(champ(&f, "CustomValue:Lu en").valeur, "2025");
    assert_eq!(champ(&f, "Released").valeur, "17/05/2024");
    assert_eq!(f.onglets.last().unwrap().champs, ["CustomValue:Lu en"]);
}

#[test]
fn suggestions_depuis_la_bibliotheque() {
    let d = tempfile::tempdir().unwrap();
    bd(&d.path().join("BD"));
    let (b, e) = autonome(&d.path().join("data"), &d.path().join("BD"));
    let t2 = id_par_fichier(&b, "Thanos 02.cbz");
    let dm = DemandeModification {
        ids: vec![t2],
        champs: vec![modif("Writer", "Jim Starlin, Ron Lim")],
        pages: None,
        ecrire_dans_fichier: false,
        confirme: false,
    };
    e.appliquer_modifications(&dm, &|_| {}).unwrap();
    let p = e.proposer_valeurs("Writer", "Jim Starlin, ro", false, None).unwrap();
    assert_eq!(p.iter().map(|x| x.valeur.as_str()).collect::<Vec<_>>(), ["Ron Lim"]);
    assert_eq!(p[0].texte, "Jim Starlin, Ron Lim");
    let p = e.proposer_valeurs("Writer", "li", false, None).unwrap();
    assert_eq!(p.iter().map(|x| x.valeur.as_str()).collect::<Vec<_>>(), ["Jim Starlin", "Ron Lim"], "contient");
    let p = e.proposer_valeurs("Publisher", "", true, None).unwrap();
    assert_eq!(p.iter().map(|x| x.valeur.as_str()).collect::<Vec<_>>(), ["Dargaud", "Marvel"]);
    assert!(e.proposer_valeurs("Publisher", "Marvel", false, None).unwrap().is_empty(), "déjà complet");
    let age = e.proposer_valeurs("AgeRating", "te", false, None).unwrap();
    assert!(age.iter().any(|x| x.valeur == "Teen"));
    assert!(e.proposer_valeurs("Title", "T", true, None).unwrap().is_empty());
    assert_eq!(accepter("Mar", false, "Marvel"), "Marvel");
    assert_eq!(jeton_courant("a, b", true), "b");
}

#[test]
fn garde_fou_au_dela_du_seuil() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    let cfg = comicrust_core::store::StoreConfig::Sqlite(data.join(comicrust_core::store::SQLITE_FILE_NAME));
    let livres: Vec<LibraryBook> = (0..=SEUIL_CONFIRMATION)
        .map(|i| LibraryBook {
            id: format!("00000000-0000-0000-0000-{i:012}"),
            file: Some(format!("{}/BD/Livre {i}.cbz", d.path().display())),
            series: Some("Série".into()),
            ..Default::default()
        })
        .collect();
    let mut st = Store::open_existing(&cfg).unwrap();
    st.upsert(&livres).unwrap();
    b.installer(Mode::Autonome, ComicDatabase::from_books(livres.clone()), Some(cfg));
    let e = Edition::nouvelle(b.clone(), Lecteur::nouveau(data.clone(), b.clone()));
    let ids: Vec<String> = livres.iter().map(|l| l.id.clone()).collect();
    let dm = DemandeModification {
        ids: ids.clone(),
        champs: vec![modif("Series", "Autre")],
        pages: None,
        ecrire_dans_fichier: false,
        confirme: false,
    };
    let r = e.appliquer_modifications(&dm, &|_| {}).unwrap();
    let c = r.confirmation_requise.as_ref().expect("confirmation");
    assert_eq!((c.nb_livres, c.seuil), (SEUIL_CONFIRMATION + 1, SEUIL_CONFIRMATION));
    assert!(r.livres.is_empty());
    assert_eq!(b.livre(&ids[0]).unwrap().series.as_deref(), Some("Série"), "rien n'est fait sans confirmation");
    assert!(serde_json::to_value(&r).unwrap()["confirmationRequise"]["nbLivres"].is_number());
    assert!(e.ecrire_infos_fichiers(&ids, false, &|_| {}).unwrap().confirmation_requise.is_some());
    // Confirmé : appliqué à tous.
    let ev = Evenements::default();
    let r = e.appliquer_modifications(&DemandeModification { confirme: true, ..dm }, &ev.sortie()).unwrap();
    assert!(r.confirmation_requise.is_none() && r.complet, "{}", r.resume);
    assert_eq!(b.livre(&ids[SEUIL_CONFIRMATION]).unwrap().series.as_deref(), Some("Autre"));
    assert!(ev.0.lock().unwrap().iter().any(|x| matches!(x, EvenementEdition::Progression { .. })));
    // Une seule opération à la fois.
    let _occupe = e.occuper().unwrap();
    assert!(e.lire_fiches(&ids[..1]).is_ok(), "la lecture reste permise");
    assert!(e.ecrire_infos_fichiers(&ids[..1], false, &|_| {}).unwrap_err().contains("déjà en cours"));
}

#[test]
fn renommage_apercu_collisions_puis_application() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let c = [10, 10, 10, 255];
    // Doublon de Thanos #2 (même destination) et Astérix déjà au bon nom.
    cbz_de_test(&racine.join("Copie thanos.cbz"), c, 1, Some(&info("Thanos", "2", "Marvel", "")));
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let t1 = id_par_fichier(&b, "Thanos 01.cbz");
    let t2 = id_par_fichier(&b, "Thanos 02.cbz");
    let copie = id_par_fichier(&b, "Copie thanos.cbz");
    let ast = id_par_fichier(&b, "Asterix 01.cbz");
    let ids = vec![t1.clone(), t2.clone(), copie.clone(), ast.clone()];
    let cible = d.path().join("Rangé");
    let dem = |apercu| DemandeRenommage {
        ids: ids.clone(),
        modele: "{publisher}/{series} #{number:000}".into(),
        dossier_cible: Some(cible.to_string_lossy().into()),
        apercu,
        confirme: false,
    };
    // Une destination déjà prise sur le disque.
    std::fs::create_dir_all(cible.join("Dargaud")).unwrap();
    std::fs::write(cible.join("Dargaud").join("Astérix #001.cbz"), b"occupe").unwrap();
    let r = e.renommer_deplacer(&dem(true), &|_| {}).unwrap();
    assert!(r.bilan.is_none());
    let l = |id: &str| r.lignes.iter().find(|x| x.id == *id).unwrap().clone();
    assert_eq!(l(&t1).vers, cible.join("Marvel").join("Thanos #001.cbz").display().to_string());
    assert!(l(&t1).probleme.is_none() && l(&t2).probleme.is_none());
    assert!(l(&copie).probleme.as_deref().unwrap().starts_with("même destination"), "{:?}", l(&copie));
    assert_eq!(l(&ast).probleme.as_deref(), Some("la destination existe déjà"));
    assert_eq!(r.a_renommer, 2);
    assert!(racine.join("Thanos 01.cbz").exists(), "aperçu : rien n'est déplacé");

    let ev = Evenements::default();
    let r = e.renommer_deplacer(&dem(false), &ev.sortie()).unwrap();
    let bilan = r.bilan.unwrap();
    assert!(!bilan.complet, "deux lignes refusées");
    assert!(!racine.join("Thanos 01.cbz").exists() && cible.join("Marvel").join("Thanos #001.cbz").is_file());
    assert!(racine.join("Copie thanos.cbz").exists(), "conflit : pas déplacé");
    assert_eq!(
        std::fs::read(cible.join("Dargaud").join("Astérix #001.cbz")).unwrap(),
        b"occupe",
        "jamais d'écrasement"
    );
    let f = b.livre(&t1).unwrap();
    assert!(f.file.as_deref().unwrap().ends_with("Thanos #001.cbz"));
    assert!(ev.modifies().contains(&t1));
    // Fiche relue après redémarrage.
    let b2 = Biblio::nouvelle(d.path().join("data"));
    b2.ouvrir().unwrap();
    assert!(b2.livre(&t2).unwrap().file.as_deref().unwrap().ends_with("Thanos #002.cbz"));
    // Relancé : déjà en place.
    let r = e.renommer_deplacer(&DemandeRenommage { ids: vec![t1.clone()], ..dem(true) }, &|_| {}).unwrap();
    assert!(r.lignes[0].inchange && r.a_renommer == 0);
    // Sans dossier cible : simple renommage dans le dossier actuel ; nom vide refusé.
    let r = e
        .renommer_deplacer(
            &DemandeRenommage {
                ids: vec![copie.clone()],
                modele: "{title}".into(),
                dossier_cible: None,
                apercu: true,
                confirme: false,
            },
            &|_| {},
        )
        .unwrap();
    assert!(r.lignes[0].probleme.as_deref().unwrap().contains("nom vide"), "{:?}", r.lignes[0]);
    let mut dm = dem(true);
    dm.modele = " ".into();
    assert!(e.renommer_deplacer(&dm, &|_| {}).is_err());
    assert_eq!(modeles_de_nom()[0].modele, files::DEFAULT_NAME_TEMPLATE);
    let reg = reglages::lire_reglages_edition(&d.path().join("data")).unwrap();
    assert_eq!(reg.modele_nom.as_deref(), Some("{publisher}/{series} #{number:000}"));
}

#[test]
fn conversion_cbr_et_cb7_en_cbz() {
    let echantillons = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recette/echantillons");
    let cbr = echantillons.join("Silver Surfer 04 (1987).cbr");
    let cb7 = echantillons.join("Warlock 09 (1976).cb7");
    if !cbr.is_file() || !cb7.is_file() {
        eprintln!("échantillons absents : test de conversion ignoré");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    std::fs::copy(&cbr, racine.join("Silver Surfer 04 (1987).cbr")).unwrap();
    std::fs::copy(&cb7, racine.join("Warlock 09 (1976).cb7")).unwrap();
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let ss = id_par_fichier(&b, "Silver Surfer 04 (1987).cbr");
    let wl = id_par_fichier(&b, "Warlock 09 (1976).cb7");
    let t1 = id_par_fichier(&b, "Thanos 01.cbz");
    // Le CBR reçoit d'abord une série dans la base : elle va dans le ComicInfo.xml du CBZ.
    let dm = DemandeModification {
        ids: vec![ss.clone()],
        champs: vec![modif("Series", "Silver Surfer"), modif("Number", "4")],
        pages: None,
        ecrire_dans_fichier: true,
        confirme: false,
    };
    let r = e.appliquer_modifications(&dm, &|_| {}).unwrap();
    assert_eq!(r.livres[0].statut, Statut::Ok, "fiche modifiée ; le CBR n'est simplement pas écrit");

    let ev = Evenements::default();
    let r = e.convertir_cbr_cbz(&[ss.clone(), wl.clone(), t1.clone()], Some(true), false, &ev.sortie()).unwrap();
    assert!(r.complet, "{}", r.rapport);
    let st = |id: &str| r.livres.iter().find(|l| l.id == *id).unwrap().clone();
    assert_eq!(st(&t1).statut, Statut::Ignore);
    assert_eq!(st(&ss).statut, Statut::Ok);
    let cbz = racine.join("Silver Surfer 04 (1987).cbz");
    assert!(cbz.is_file() && !racine.join("Silver Surfer 04 (1987).cbr").exists());
    assert!(racine.join(DOSSIER_ORIGINAUX).join("Silver Surfer 04 (1987).cbr").is_file(), "original rangé");
    assert!(racine.join("Warlock 09 (1976).cbz").is_file());
    let (xml, _) = comic_info(&cbz);
    assert!(xml.contains("<Series>Silver Surfer</Series>"), "{xml}");
    // La fiche pointe sur le CBZ (base et bibliothèque affichée).
    assert!(b.livre(&ss).unwrap().file.as_deref().unwrap().ends_with(".cbz"));
    assert!(b.livre(&ss).unwrap().page_count.unwrap_or(0) > 0);
    assert!(ev.modifies().contains(&ss));
    assert!(reglages::lire_reglages_edition(&d.path().join("data")).unwrap().ranger_original);
    // Déjà converti : ignoré.
    let r = e.convertir_cbr_cbz(std::slice::from_ref(&ss), None, false, &|_| {}).unwrap();
    assert_eq!(r.livres[0].statut, Statut::Ignore);
}

#[test]
fn comicdb_xml_lecture_seule_pour_les_fiches() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let fichier = racine.join("Thanos 01.cbz");
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(
        &xml,
        format!(
            "<ComicDatabase><Books><Book Id=\"11111111-0000-0000-0000-000000000001\" File=\"{}\">\
             <Series>Thanos</Series><Number>1</Number><Writer>Jim Starlin</Writer></Book></Books></ComicDatabase>",
            fichier.display()
        ),
    )
    .unwrap();
    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(xml.to_string_lossy().into()) })
        .unwrap();
    let e = Edition::nouvelle(b.clone(), Lecteur::nouveau(data.clone(), b.clone()));
    let id = "11111111-0000-0000-0000-000000000001".to_string();
    // Même avec l'option d'écriture : ComicDb.xml reste en lecture seule pour les fiches.
    reglages::enregistrer_reglages_edition(
        &data,
        ReglagesEdition { ecriture_comicrack: Some(true), ..Default::default() },
    )
    .unwrap();
    let f = e.lire_fiches(std::slice::from_ref(&id)).unwrap();
    assert_eq!(f.lecture_seule.as_deref(), Some(LECTURE_SEULE_XML));
    let dm = DemandeModification {
        ids: vec![id.clone()],
        champs: vec![modif("Series", "X")],
        pages: None,
        ecrire_dans_fichier: false,
        confirme: false,
    };
    assert_eq!(e.appliquer_modifications(&dm, &|_| {}).unwrap_err(), LECTURE_SEULE_XML);
    let rn = DemandeRenommage {
        ids: vec![id.clone()],
        modele: "{series}".into(),
        dossier_cible: None,
        apercu: true,
        confirme: false,
    };
    assert!(e.renommer_deplacer(&rn, &|_| {}).is_err());
    assert!(e
        .convertir_cbr_cbz(std::slice::from_ref(&id), Some(true), false, &|_| {})
        .unwrap_err()
        .contains("lecture seule"));
    // Les fichiers restent modifiables (fiche du ComicDb.xml écrite dans le CBZ).
    let r = e.ecrire_infos_fichiers(std::slice::from_ref(&id), false, &|_| {}).unwrap();
    assert_eq!(r.livres[0].statut, Statut::Ok, "{}", r.rapport);
    let (x, _) = comic_info(&fichier);
    assert!(x.contains("<Writer>Jim Starlin</Writer>"), "{x}");
    assert!(std::fs::read_to_string(&xml).unwrap().contains("<Series>Thanos</Series>"), "ComicDb.xml intact");
}

#[test]
fn evenements_pour_le_front() {
    let (n, j) =
        EvenementEdition::Progression { operation: Operation::EcritureInfos, fait: 1, total: 3, livre: "A".into() }
            .pour_le_front();
    assert_eq!(n, "edition-progression");
    assert_eq!(j, serde_json::json!({"operation": "ecritureInfos", "fait": 1, "total": 3, "livre": "A"}));
    let (n, j) = EvenementEdition::BibliothequeModifiee { ids: vec!["x".into()] }.pour_le_front();
    assert_eq!((n, j), ("bibliotheque-modifiee", serde_json::json!({"ids": ["x"]})));
    let s = serde_json::to_value(Statut::Inchange).unwrap();
    assert_eq!(s, "inchange");
}

// ───────────────────────────── MySQL de ComicRack (base de test) ─────────────────────────────

/// Fiche de ComicRack CE avec des éléments que ComicRust ne modélise pas.
/// Revue de l'étape 3 : « (valeurs multiples) » n'est jamais écrit tel quel dans une fiche.
#[test]
fn valeurs_multiples_jamais_ecrites_litteralement() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let ids = vec![id_par_fichier(&b, "Thanos 01.cbz"), id_par_fichier(&b, "Asterix 01.cbz")];
    let err = verifier_champs(&[modif("Series", VALEURS_MULTIPLES), modif("Writer", " (valeurs multiples) ")]);
    assert_eq!(err.len(), 2, "{err:?}");
    let dm = DemandeModification {
        ids: ids.clone(),
        champs: vec![modif("Series", VALEURS_MULTIPLES)],
        pages: None,
        ecrire_dans_fichier: true,
        confirme: false,
    };
    assert!(e.appliquer_modifications(&dm, &|_| {}).unwrap_err().contains("rien n'est enregistré"));
    assert_eq!(b.livre(&ids[0]).unwrap().series.as_deref(), Some("Thanos"));
    assert_eq!(b.livre(&ids[1]).unwrap().series.as_deref(), Some("Astérix"));
    // Plusieurs livres : un seul champ envoyé, les autres (valeurs différentes) restent intacts.
    let dm = DemandeModification { champs: vec![modif("Genre", "BD")], ..dm };
    assert!(e.appliquer_modifications(&dm, &|_| {}).unwrap().complet);
    assert_eq!(b.livre(&ids[0]).unwrap().series.as_deref(), Some("Thanos"));
    assert_eq!(b.livre(&ids[1]).unwrap().series.as_deref(), Some("Astérix"));
    let (xml, _) = comic_info(&racine.join("Asterix 01.cbz"));
    assert!(xml.contains("<Series>Astérix</Series>") && xml.contains("<Genre>BD</Genre>"), "{xml}");
}

/// Revue de l'étape 3 : l'analyse des dossiers et une opération d'édition s'excluent (un fichier
/// renommé pendant l'analyse serait ajouté comme nouveau livre).
#[test]
fn analyse_et_edition_s_excluent() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let garde = e.occuper().unwrap();
    let err = b.lancer_scan(Arc::new(|_| {})).unwrap_err();
    assert!(err.contains("édition est en cours"), "{err}");
    assert!(b.prendre_edition().err().unwrap().contains("déjà en cours"));
    let err = b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap_err();
    assert!(err.contains("édition en cours"), "{err}");
    drop(garde);
    // Analyse en cours : l'édition est refusée.
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    b.lancer_scan(Arc::new(move |ev| {
        let _ = tx.lock().unwrap().send(ev);
    }))
    .unwrap();
    let pendant = e.occuper().err();
    loop {
        if let EvenementScan::Termine { .. } | EvenementScan::Erreur { .. } =
            rx.recv_timeout(Duration::from_secs(60)).unwrap()
        {
            break;
        }
    }
    // L'analyse peut finir avant `occuper` : seul le message, s'il y en a un, est vérifié.
    if let Some(m) = pendant {
        assert!(m.contains("analyse"), "{m}");
    }
    assert!(e.occuper().is_ok(), "analyse finie : l'édition reprend");
}

/// Revue de l'étape 3 : arrêt demandé avant le premier livre : rien n'est écrit, chaque livre
/// est noté « non fait » dans le bilan.
#[test]
fn arret_du_lot_entre_deux_livres() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let ids = vec![id_par_fichier(&b, "Thanos 01.cbz"), id_par_fichier(&b, "Thanos 02.cbz")];
    let avant = std::fs::read(racine.join("Thanos 02.cbz")).unwrap();
    let e2 = e.clone();
    let sortie = move |ev: EvenementEdition| {
        if matches!(ev, EvenementEdition::Progression { .. }) {
            e2.annuler();
        }
    };
    let dm = DemandeModification {
        ids: ids.clone(),
        champs: vec![modif("Genre", "Cosmique")],
        pages: None,
        ecrire_dans_fichier: true,
        confirme: false,
    };
    let r = e.appliquer_modifications(&dm, &sortie).unwrap();
    assert!(r.interrompu && !r.complet, "{}", r.rapport);
    assert_eq!(std::fs::read(racine.join("Thanos 02.cbz")).unwrap(), avant, "fichier non réécrit");
    assert!(r.livres.iter().all(|l| l.message.as_deref().unwrap_or("").contains("arrêt demandé")), "{}", r.rapport);
    // Lot suivant : l'arrêt précédent est oublié.
    let r = e.ecrire_infos_fichiers(&ids, false, &|_| {}).unwrap();
    assert!(r.complet && !r.interrompu, "{}", r.rapport);
}

/// Revue de l'étape 3 : l'original converti n'est rangé dans « _originaux » que si la fiche
/// pointe bien sur le CBZ (sinon la fiche pointerait sur un fichier disparu).
#[test]
fn conversion_original_garde_si_fiche_non_suivie() {
    let cbr = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recette/echantillons/Silver Surfer 04 (1987).cbr");
    if !cbr.is_file() {
        eprintln!("échantillon absent : test ignoré");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let original = racine.join("Silver Surfer 04 (1987).cbr");
    std::fs::copy(&cbr, &original).unwrap();
    let data = d.path().join("data");
    let (b, e) = autonome(&data, &racine);
    let ss = id_par_fichier(&b, "Silver Surfer 04 (1987).cbr");
    // La fiche disparaît de la base (retirée ailleurs) : la mise à jour échouera.
    let cfg = comicrust_core::store::StoreConfig::Sqlite(data.join(comicrust_core::store::SQLITE_FILE_NAME));
    Store::open_existing(&cfg).unwrap().delete(std::slice::from_ref(&ss)).unwrap();
    let r = e.convertir_cbr_cbz(std::slice::from_ref(&ss), Some(true), false, &|_| {}).unwrap();
    assert_eq!(r.livres[0].statut, Statut::Erreur, "{}", r.rapport);
    assert!(original.is_file(), "original resté à sa place : {}", r.rapport);
    assert!(!racine.join(DOSSIER_ORIGINAUX).exists());
    assert!(racine.join("Silver Surfer 04 (1987).cbz").is_file(), "le CBZ, lui, est créé");
}

/// « Ouvrir le dossier du fichier » : commandes par système, livre sans fichier, dossier absent.
#[test]
fn ouvrir_le_dossier_du_fichier() {
    let w = commande_ouvrir_dossier(Path::new(r"C:\BD\Marvel\Thanos 01.cbz"), true, Systeme::Windows);
    assert_eq!(w.programme, "explorer.exe");
    assert_eq!(w.arguments, vec![r#"/select,"C:\BD\Marvel\Thanos 01.cbz""#.to_string()]);
    assert!(w.brut, "arguments passés tels quels à l'Explorateur");
    let unc = commande_ouvrir_dossier(Path::new("//nas/BD/Thanos 01.cbz"), true, Systeme::Windows);
    assert_eq!(unc.arguments, vec![r#"/select,"\\nas\BD\Thanos 01.cbz""#.to_string()]);
    let l = commande_ouvrir_dossier(Path::new("/srv/BD/Thanos 01.cbz"), true, Systeme::Autre);
    assert_eq!((l.programme.as_str(), l.arguments.clone(), l.brut), ("xdg-open", vec!["/srv/BD".to_string()], false));
    let m = commande_ouvrir_dossier(Path::new("/srv/BD/Thanos 01.cbz"), true, Systeme::MacOs);
    assert_eq!(m.arguments, vec!["-R".to_string(), "/srv/BD/Thanos 01.cbz".to_string()]);
    let absent = commande_ouvrir_dossier(Path::new("/srv/BD/Thanos 01.cbz"), false, Systeme::MacOs);
    assert_eq!(absent.arguments, vec!["/srv/BD".to_string()]);

    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let id = id_par_fichier(&b, "Thanos 01.cbz");
    let c = e.commande_dossier_fichier(&id).unwrap();
    assert_eq!(c, commande_ouvrir_dossier(&racine.join("Thanos 01.cbz"), true, Systeme::courant()));
    assert!(e.commande_dossier_fichier("inconnu").unwrap_err().contains("inconnu"));
    // Fichier disparu, dossier présent : le dossier est ouvert ; dossier disparu : erreur lisible.
    std::fs::remove_file(racine.join("Thanos 01.cbz")).unwrap();
    let c = e.commande_dossier_fichier(&id).unwrap();
    assert_eq!(c, commande_ouvrir_dossier(&racine.join("Thanos 01.cbz"), false, Systeme::courant()));
    std::fs::remove_dir_all(&racine).unwrap();
    assert!(e.commande_dossier_fichier(&id).unwrap_err().contains("dossier introuvable"));
}

/// Pages renvoyées par le front (liste complète, nouvel ordre) à partir d'une liste lue.
fn modifiees(l: &[pages_livre::PageLivre]) -> Vec<PageModifiee> {
    l.iter().map(|p| serde_json::from_value(serde_json::to_value(p).unwrap()).unwrap()).collect()
}

/// Pages d'un CBZ de test réordonnées, supprimée, typée et tournée par les opérations de l'onglet.
fn pages_modifiees(e: &Edition, id: &str) -> (pages_livre::PagesLivre, Vec<pages_livre::PageLivre>) {
    use crate::pages_livre::{transformer, OperationPages as Op};
    let p = e.lire_pages(id, None).unwrap();
    let l = transformer(&p.pages, &Op::Deplacer { images: vec![2], position: 0 }).unwrap();
    let l = transformer(&l, &Op::BasculerSupprimee { images: vec![1] }).unwrap();
    let l = transformer(&l, &Op::Type { images: vec![0], valeur: "Advertisement".into() }).unwrap();
    let l = transformer(&l, &Op::Rotation { images: vec![2], valeur: 90 }).unwrap();
    (p, l)
}

#[test]
fn onglet_pages_lecture_et_vignettes() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let t1 = id_par_fichier(&b, "Thanos 01.cbz");
    let p = e.lire_pages(&t1, None).unwrap();
    assert_eq!(p.nb_images, 3);
    assert!(p.modifiable && p.lecture_seule.is_none() && p.ecrire_dans_fichier_possible);
    assert_eq!(
        p.pages.iter().map(|x| (x.index, x.image, x.nom.as_str())).collect::<Vec<_>>(),
        [(0, 0, "01.png"), (1, 1, "02.png"), (2, 2, "03.png")]
    );
    assert_eq!(p.pages[0].type_page, "FrontCover");
    assert_eq!((p.types.len(), p.rotations.len(), p.positions.len()), (11, 4, 3));
    assert_eq!(p.types[10].valeur, "Deleted");
    assert_eq!(p.pages[2].url_vignette, format!("{}/2?l=192", p.url_base));
    // Vignette servie par cr-page (index = image du fichier), réduite à 192 px.
    let (tx, rx) = mpsc::channel();
    e.lecteur.servir(&p.jeton, 2, Some(192), move |r| tx.send(r).unwrap());
    let r = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert_eq!(r.statut, 200);
    assert_eq!(image::load_from_memory(&r.octets).unwrap().width(), 192);
    // Contrat JSON.
    let j = serde_json::to_value(&p).unwrap();
    for k in ["jeton", "urlBase", "largeurVignette", "nbImages", "pages", "modifiable", "ecrireDansFichierPossible"] {
        assert!(j.get(k).is_some(), "{k}");
    }
    // Nouvelle séance (après « Appliquer ») : l'ancienne est fermée.
    let p2 = e.lire_pages(&t1, Some(&p.jeton)).unwrap();
    assert!(e.lecteur.url_page_base(&p.jeton).is_err() && e.lecteur.url_page_base(&p2.jeton).is_ok());
    e.fermer_pages(&p2.jeton);
    assert!(e.lecteur.url_page_base(&p2.jeton).is_err());
    assert!(e.lire_pages("inconnu", None).is_err());
}

#[test]
fn onglet_pages_enregistre_base_puis_fichier_seulement_si_demande() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let (b, e) = autonome(&d.path().join("data"), &racine);
    let t1 = id_par_fichier(&b, "Thanos 01.cbz");
    let fichier = racine.join("Thanos 01.cbz");
    let avant = std::fs::read(&fichier).unwrap();
    let (p, l) = pages_modifiees(&e, &t1);
    assert_eq!(l.iter().map(|x| x.image).collect::<Vec<_>>(), [2, 0, 1]);

    // 1. Sans « écrire dans le fichier » : base seulement, fichier intact, fiche « à écrire ».
    let ev = Evenements::default();
    let dm = DemandeModification {
        ids: vec![t1.clone()],
        champs: vec![],
        pages: Some(modifiees(&l)),
        ecrire_dans_fichier: false,
        confirme: false,
    };
    let r = e.appliquer_modifications(&dm, &ev.sortie()).unwrap();
    assert!(r.complet && r.livres[0].statut == Statut::Ok, "{}", r.rapport);
    assert_eq!(std::fs::read(&fichier).unwrap(), avant, "fichier non écrit sans la case");
    let f = b.livre(&t1).unwrap();
    let ordre: Vec<(i32, Option<&str>, Option<&str>)> =
        f.pages.page.iter().map(|x| (x.image, x.kind.as_deref(), x.rotation.as_deref())).collect();
    assert_eq!(ordre, [(2, None, Some("Rotate90")), (0, Some("Advertisement"), None), (1, Some("Deleted"), None)]);
    assert_eq!(f.comic_info_is_dirty, Some(true), "infos pas encore écrites dans le fichier");
    assert!(ev.modifies().contains(&t1));
    // Relue depuis la base (autonome : colonne JSON des pages).
    let cfg = b.store_autonome().unwrap();
    let relue = Store::open_existing(&cfg).unwrap().get(std::slice::from_ref(&t1)).unwrap();
    assert_eq!(relue[0].pages.page, f.pages.page);
    // Le lecteur lit dans le nouvel ordre, page supprimée masquée.
    let o = e.lecteur.ouvrir_livre(&t1).unwrap();
    assert_eq!(
        o.pages.iter().map(|x| (x.image, x.supprimee, x.rotation)).collect::<Vec<_>>(),
        [(2, false, 90), (0, false, 0), (1, true, 0)]
    );
    e.lecteur.fermer_livre(&o.jeton).unwrap();
    // Liste renvoyée identique : rien à écrire.
    let p2 = e.lire_pages(&t1, Some(&p.jeton)).unwrap();
    let dm_meme = DemandeModification { pages: Some(modifiees(&p2.pages)), ..dm.clone() };
    let r = e.appliquer_modifications(&dm_meme, &|_| {}).unwrap();
    assert!(r.livres.iter().all(|x| x.statut != Statut::Ok), "{}", r.rapport);

    // 2. Avec la case : ComicInfo.xml <Pages> écrit (séance de vignettes ouverte, servie pendant).
    let (tx, rx) = mpsc::channel();
    e.lecteur.servir(&p2.jeton, 0, Some(192), move |r| tx.send(r).unwrap());
    assert_eq!(rx.recv_timeout(Duration::from_secs(20)).unwrap().statut, 200);
    let l2 = crate::pages_livre::transformer(
        &p2.pages,
        &crate::pages_livre::OperationPages::Favori { image: 0, texte: Some("Combat".into()) },
    )
    .unwrap();
    let dm2 = DemandeModification {
        champs: vec![modif("Series", "Thanos Quest")],
        pages: Some(modifiees(&l2)),
        ecrire_dans_fichier: true,
        ..dm.clone()
    };
    let r = e.appliquer_modifications(&dm2, &|_| {}).unwrap();
    assert!(r.complet, "{}", r.rapport);
    let (ci, noms) = comic_info(&fichier);
    assert!(ci.contains("<Series>Thanos Quest</Series>"), "{ci}");
    let i2 = ci.find("Image=\"2\"").expect("page 2");
    let i0 = ci.find("Image=\"0\"").expect("page 0");
    let i1 = ci.find("Image=\"1\"").expect("page 1");
    assert!(i2 < i0 && i0 < i1, "ordre de lecture dans <Pages> : {ci}");
    assert!(
        ci.contains("Type=\"Deleted\"") && ci.contains("Rotation=\"Rotate90\"") && ci.contains("Bookmark=\"Combat\""),
        "{ci}"
    );
    assert_eq!(noms.iter().filter(|n| n.ends_with(".png")).count(), 3, "images recopiées");
    assert_ne!(b.livre(&t1).unwrap().comic_info_is_dirty, Some(true), "infos écrites");
    // Après l'écriture, la séance de vignettes relit le fichier réécrit ; aucun descripteur ouvert.
    let (tx, rx) = mpsc::channel();
    e.lecteur.servir(&p2.jeton, 1, Some(192), move |r| tx.send(r).unwrap());
    assert_eq!(rx.recv_timeout(Duration::from_secs(20)).unwrap().statut, 200);
    #[cfg(target_os = "linux")]
    crate::lecteur::tests::attendre_aucun_descripteur(&e.lecteur, &fichier);

    // Revue étape 4 : favori retiré (Ctrl+Maj+D) puis la liste renvoyée telle quelle (JSON) :
    // le favori doit disparaître de la base et du fichier (un champ absent voulait dire « gardé »).
    let p3 = e.lire_pages(&t1, Some(&p2.jeton)).unwrap();
    assert_eq!(p3.pages.iter().find(|x| x.image == 0).unwrap().favori.as_deref(), Some("Combat"));
    let l3 = crate::pages_livre::transformer(
        &p3.pages,
        &crate::pages_livre::OperationPages::Favori { image: 0, texte: None },
    )
    .unwrap();
    let r = e
        .appliquer_modifications(
            &DemandeModification { pages: Some(modifiees(&l3)), ecrire_dans_fichier: true, ..dm.clone() },
            &|_| {},
        )
        .unwrap();
    assert!(r.complet && r.livres[0].statut == Statut::Ok, "{}", r.rapport);
    assert!(b.livre(&t1).unwrap().pages.page.iter().all(|x| x.bookmark.is_none()), "favori retiré de la base");
    assert!(!comic_info(&fichier).0.contains("Bookmark="), "favori retiré du fichier");
    e.fermer_pages(&p3.jeton);

    // Refus : liste incomplète, plusieurs livres ; rien d'écrit.
    let t2 = id_par_fichier(&b, "Thanos 02.cbz");
    let mut incomplete = modifiees(&l2);
    incomplete.pop();
    assert!(e
        .appliquer_modifications(&DemandeModification { pages: Some(incomplete), ..dm.clone() }, &|_| {})
        .is_err());
    let plusieurs = DemandeModification { ids: vec![t1.clone(), t2], pages: Some(modifiees(&l2)), ..dm.clone() };
    assert!(e.appliquer_modifications(&plusieurs, &|_| {}).unwrap_err().contains("un seul livre"));
}

#[test]
fn onglet_pages_lecture_seule_comicdb() {
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let fichier = racine.join("Thanos 01.cbz");
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(
        &xml,
        format!(
            "<ComicDatabase><Books><Book Id=\"ro\" File=\"{}\"><Series>Thanos</Series></Book></Books></ComicDatabase>",
            fichier.display()
        ),
    )
    .unwrap();
    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(xml.to_string_lossy().into()) })
        .unwrap();
    let e = Edition::nouvelle(b.clone(), Lecteur::nouveau(data, b.clone()));
    let p = e.lire_pages("ro", None).unwrap();
    assert!(!p.modifiable && p.lecture_seule.is_some());
    let dm = DemandeModification {
        ids: vec!["ro".into()],
        champs: vec![],
        pages: Some(modifiees(&p.pages)),
        ecrire_dans_fichier: true,
        confirme: false,
    };
    assert!(e.appliquer_modifications(&dm, &|_| {}).is_err());
}

fn fiche_ce(id: &str, fichier: &Path) -> String {
    format!(
        "<ComicBook Id=\"{id}\" File=\"{}\"><Series>Thanos</Series><Number>1</Number><Year>1990</Year>\
         <MyExtra a=\"1\">inconnu</MyExtra><Added>2020-01-05T10:00:00.5+01:00</Added><OpenCount>2</OpenCount>\
         <ColorAdjustment><Saturation>0.2</Saturation></ColorAdjustment></ComicBook>",
        fichier.display()
    )
}

#[test]
fn mysql_comicrack_lecture_et_ecriture() {
    use mysql::prelude::Queryable;
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL ignoré");
        return;
    };
    let cfg = DbConfig::from_connection_string(&url).unwrap();
    assert_ne!(cfg.database, "comickrack_demo", "jamais la base de démonstration");
    let opts = mysql::OptsBuilder::new()
        .ip_or_hostname(Some(cfg.host.clone()))
        .tcp_port(cfg.port)
        .user(Some(cfg.user.clone()))
        .pass(Some(cfg.password.clone()))
        .db_name(Some(cfg.database.clone()));
    let Ok(mut conn) = mysql::Conn::new(opts) else {
        eprintln!("MySQL de test injoignable : test ignoré");
        return;
    };
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    bd(&racine);
    let fichier = racine.join("Thanos 01.cbz");
    let id = "5f0c1a2b-0000-4000-8000-0000000000aa";
    conn.query_drop("DROP TABLE IF EXISTS comics").unwrap();
    conn.query_drop("DROP TABLE IF EXISTS changes").unwrap();
    conn.query_drop(
        "Create table changes (id varchar(40) not null primary key, update_counter bigint default 0, delete_counter bigint default 0)",
    )
    .unwrap();
    conn.query_drop(
        "Create table comics (id varchar(40) not null primary key, update_counter bigint default 0, data mediumtext)",
    )
    .unwrap();
    conn.exec_drop("Insert into changes values (?, 10, 0)", (comicrust_core::db::write::STORAGE_GUID,)).unwrap();
    conn.exec_drop("Insert into comics values (?, 5, ?)", (id, fiche_ce(id, &fichier))).unwrap();
    let data_de = |c: &mut mysql::Conn| -> (String, i64) {
        c.exec_first::<(String, i64), _, _>("Select data, update_counter from comics where id = ?", (id,))
            .unwrap()
            .unwrap()
    };

    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    b.installer(Mode::Comicrack, ComicDatabase::load(&cfg, None).unwrap(), None);
    b.installer_comicrack_mysql(cfg.clone());
    let lecteur = Lecteur::nouveau(data.clone(), b.clone());
    let e = Edition::nouvelle(b.clone(), lecteur.clone());

    // Défaut : lecture seule ; les fichiers restent modifiables, rien dans ComicRack.
    let f = e.lire_fiches(&[id.to_string()]).unwrap();
    assert_eq!(f.lecture_seule.as_deref(), Some(LECTURE_SEULE_COMICRACK));
    let dm = DemandeModification {
        ids: vec![id.to_string()],
        champs: vec![modif("Series", "Thanos Quest"), modif("Writer", "Jim Starlin")],
        pages: None,
        ecrire_dans_fichier: true,
        confirme: false,
    };
    assert!(e.appliquer_modifications(&dm, &|_| {}).is_err());
    let r = e.ecrire_infos_fichiers(&[id.to_string()], false, &|_| {}).unwrap();
    assert_eq!(r.livres[0].statut, Statut::Ok, "{}", r.rapport);
    assert_eq!(data_de(&mut conn).1, 5, "lecture seule : base ComicRack intacte");
    // Lecture en lecture seule : rien dans ComicRack.
    let o = lecteur.ouvrir_livre(id).unwrap();
    lecteur.enregistrer_avancement(&o.jeton, 2, false).unwrap();
    lecteur.fermer_livre(&o.jeton).unwrap();
    assert_eq!(data_de(&mut conn).1, 5);

    // « Lecture et écriture » : seuls les éléments touchés sont réécrits, compteurs avancés.
    reglages::enregistrer_reglages_edition(
        &data,
        ReglagesEdition { ecriture_comicrack: Some(true), ..Default::default() },
    )
    .unwrap();
    assert!(e.lire_fiches(&[id.to_string()]).unwrap().lecture_seule.is_none());
    let ev = Evenements::default();
    let r = e.appliquer_modifications(&dm, &ev.sortie()).unwrap();
    assert!(r.complet, "{}", r.rapport);
    let (xml, compteur) = data_de(&mut conn);
    assert!(compteur > 10, "compteur global de ComicRack attribué : {compteur}");
    assert!(xml.contains("<Series>Thanos Quest</Series>") && xml.contains("<Writer>Jim Starlin</Writer>"), "{xml}");
    assert!(
        xml.contains("<MyExtra a=\"1\">inconnu</MyExtra>") && xml.contains("<Saturation>0.2</Saturation>"),
        "{xml}"
    );
    assert!(!xml.contains("<ComicInfoIsDirty>true"), "fichier écrit : plus « à mettre à jour » : {xml}");
    assert!(xml.contains("<FileSize>"), "taille du fichier réécrit : {xml}");
    let (ci, _) = comic_info(&fichier);
    assert!(ci.contains("<Series>Thanos Quest</Series>"), "{ci}");
    assert_eq!(b.livre(id).unwrap().series.as_deref(), Some("Thanos Quest"), "bibliothèque affichée à jour");
    assert!(ev.modifies().iter().any(|x| x.eq_ignore_ascii_case(id)));
    let (changes,): (i64,) = conn.query_first("Select update_counter from changes").unwrap().unwrap();
    assert!(changes >= compteur);

    // Fiche modifiée dans ComicRack depuis le chargement : la fiche est relue au moment d'écrire
    // (condition optimiste sur le compteur relu), la modification passe.
    conn.exec_drop("Update comics set update_counter = update_counter + 100 where id = ?", (id,)).unwrap();
    let dm2 = DemandeModification { champs: vec![modif("Year", "1991")], ecrire_dans_fichier: false, ..dm.clone() };
    assert!(e.appliquer_modifications(&dm2, &|_| {}).unwrap().complet);
    assert!(data_de(&mut conn).0.contains("<Year>1991</Year>"));

    // Lecture écrite dans ComicRack à la fermeture du livre (comme la version egui).
    let o = lecteur.ouvrir_livre(id).unwrap();
    lecteur.enregistrer_avancement(&o.jeton, 2, false).unwrap();
    lecteur.fermer_livre(&o.jeton).unwrap();
    let (xml, _) = data_de(&mut conn);
    assert!(xml.contains("<CurrentPage>2</CurrentPage>") && xml.contains("<LastPageRead>2</LastPageRead>"), "{xml}");
    assert!(xml.contains("<OpenCount>3</OpenCount>") && xml.contains("<Opened>"), "{xml}");
    assert_eq!(b.livre(id).unwrap().current_page, Some(2));

    // Onglet « Pages » en écriture : élément <Pages> de la fiche ComicRack (Value::Pages), puis
    // ComicInfo.xml du fichier.
    let (p, l) = pages_modifiees(&e, id);
    let dmp =
        DemandeModification { champs: vec![], pages: Some(modifiees(&l)), ecrire_dans_fichier: false, ..dm.clone() };
    let r = e.appliquer_modifications(&dmp, &|_| {}).unwrap();
    assert!(r.complet && r.livres[0].statut == Statut::Ok, "{}", r.rapport);
    let (xml, _) = data_de(&mut conn);
    let pages_xml = &xml[xml.find("<Pages>").expect("<Pages> écrit")..];
    assert!(pages_xml.find("Image=\"2\"").unwrap() < pages_xml.find("Image=\"0\"").unwrap(), "{xml}");
    assert!(pages_xml.contains("Deleted") && pages_xml.contains("Rotate90"), "{xml}");
    assert!(xml.contains("<ComicInfoIsDirty>true"), "pages pas encore dans le fichier : {xml}");
    assert!(!comic_info(&fichier).0.contains("Deleted"), "fichier non écrit sans la case");
    let dmf = DemandeModification { pages: None, ecrire_dans_fichier: true, ..dmp.clone() };
    assert!(e.appliquer_modifications(&dmf, &|_| {}).unwrap().complet);
    let (ci, _) = comic_info(&fichier);
    assert!(ci.contains("Type=\"Deleted\"") && ci.contains("Rotation=\"Rotate90\""), "{ci}");
    assert!(!data_de(&mut conn).0.contains("<ComicInfoIsDirty>true"));
    e.fermer_pages(&p.jeton);

    // Renommage : la fiche ComicRack suit le fichier.
    let rn = DemandeRenommage {
        ids: vec![id.to_string()],
        modele: "{series} #{number:00}".into(),
        dossier_cible: None,
        apercu: false,
        confirme: false,
    };
    let r = e.renommer_deplacer(&rn, &|_| {}).unwrap();
    assert!(r.bilan.as_ref().unwrap().complet, "{}", r.bilan.unwrap().rapport);
    assert!(racine.join("Thanos Quest #01.cbz").is_file());
    assert!(data_de(&mut conn).0.contains("Thanos Quest #01.cbz"));
    conn.query_drop("DROP TABLE IF EXISTS comics").unwrap();
    conn.query_drop("DROP TABLE IF EXISTS changes").unwrap();
}
