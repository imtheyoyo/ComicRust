//! Tests de la surveillance : sondage et transitions en ligne / hors ligne avec une base SQLite
//! autonome (aucun serveur), refus d'écriture hors ligne, configuration MySQL saisie à la main
//! (serveur injoignable : aucun serveur nécessaire), et base MySQL réelle seulement si
//! `COMICRUST_TEST_MYSQL` est définie (base de test, jamais `comickrack_demo`).

use super::*;
use crate::biblio::{Biblio, ChoixPremierDemarrage, Groupe, RequeteLivres, SensTri};
use crate::edition::{ChampModifie, DemandeModification, Edition};
use crate::lecteur::Lecteur;
use crate::reglages::{self, Mode, ReglagesEdition};
use comicrust_core::db::{ComicDatabase, DbConfig};
use comicrust_core::library::LibraryBook;
use comicrust_core::store::{Store, StoreConfig, SQLITE_FILE_NAME};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Mot de passe factice : ne doit apparaître dans aucun message ni état.
const SECRET: &str = "secret-factice-a-ne-jamais-voir";

fn livre(id: &str, titre: &str) -> LibraryBook {
    LibraryBook {
        id: id.into(),
        title: Some(titre.into()),
        series: Some("Série".into()),
        file: Some(format!("Z:/BD/{id}.cbz")),
        ..Default::default()
    }
}

fn config(data: &Path) -> StoreConfig {
    StoreConfig::Sqlite(data.join(SQLITE_FILE_NAME))
}

/// Bibliothèque autonome ouverte, avec `livres` déjà en base ; `data` est le dossier portable.
fn autonome(data: &Path, livres: &[LibraryBook]) -> Biblio {
    let b = Biblio::nouvelle(data.to_path_buf());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    if !livres.is_empty() {
        Store::open_existing(&config(data)).unwrap().upsert(livres).unwrap();
    }
    b.ouvrir().unwrap();
    b
}

/// Autre « poste » : écrit dans la même base sans passer par la bibliothèque affichée.
fn ailleurs(data: &Path) -> Store {
    Store::open_existing(&config(data)).unwrap()
}

fn titres(b: &Biblio) -> Vec<String> {
    let req = RequeteLivres {
        recherche: None,
        tri: "titre".into(),
        sens_tri: SensTri::Asc,
        groupe: Some(Groupe::Aucun),
        debut: 0,
        nombre: 100,
        liste: None,
        serie: None,
    };
    b.lister(&req).unwrap().livres.into_iter().map(|l| l.titre).collect()
}

fn modifiee(ev: &[EvenementBase]) -> Option<(usize, usize, usize, Vec<String>)> {
    ev.iter().find_map(|e| match e {
        EvenementBase::Modifiee { ids, ajoutes, modifies, supprimes } => {
            Some((*ajoutes, *modifies, *supprimes, ids.clone()))
        }
        _ => None,
    })
}

fn etats(ev: &[EvenementBase]) -> Vec<EtatBase> {
    ev.iter()
        .filter_map(|e| match e {
            EvenementBase::Etat(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// Écarte la base (dossier renommé : une base SQLite disparue est une erreur, comme un serveur
/// arrêté), et la remet.
fn ecarter(data: &Path) -> PathBuf {
    let ecarte = data.with_extension("ecarte");
    std::fs::rename(data, &ecarte).unwrap();
    ecarte
}

fn remettre(ecarte: &Path, data: &Path) {
    std::fs::rename(ecarte, data).unwrap();
}

#[test]
fn premier_sondage_donne_letat_en_ligne_sans_modification() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha")]);
    let mut s = Sondeur::nouveau(b.clone());
    let ev = s.sonder();
    let e = etats(&ev);
    assert_eq!(e.len(), 1, "{ev:?}");
    assert!(e[0].en_ligne && e[0].surveillee && e[0].source == SourceBase::Comicrust && e[0].essais == 0);
    assert!(modifiee(&ev).is_none());
    // Rien n'a changé : plus aucun événement (pas de rafale).
    assert!(s.sonder().is_empty());
}

#[test]
fn modifications_exterieures_appliquees_et_signalees() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha"), livre("b", "Bravo")]);
    let mut s = Sondeur::nouveau(b.clone());
    s.sonder();
    assert_eq!(titres(&b), ["Alpha", "Bravo"]);

    // Un autre poste modifie « Alpha », ajoute « Charlie ».
    ailleurs(&data).upsert(&[livre("a", "Alpha 2"), livre("c", "Charlie")]).unwrap();
    let ev = s.sonder();
    let (ajoutes, modifies, supprimes, mut ids) = modifiee(&ev).expect("modification signalée");
    ids.sort();
    assert_eq!((ajoutes, modifies, supprimes), (1, 1, 0));
    assert_eq!(ids, ["a", "c"]);
    // La vue affichée est à jour (cache de vue invalidé).
    assert_eq!(titres(&b), ["Alpha 2", "Bravo", "Charlie"]);
    assert!(s.sonder().is_empty());

    // Suppression faite ailleurs.
    ailleurs(&data).delete(&["b".to_string()]).unwrap();
    let ev = s.sonder();
    let (a, m, sup, _) = modifiee(&ev).expect("suppression signalée");
    assert_eq!((a, m, sup), (0, 0, 1));
    assert_eq!(titres(&b), ["Alpha 2", "Charlie"]);
}

#[test]
fn les_ecritures_de_comicrust_ne_sont_pas_renotifiees() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha"), livre("b", "Bravo")]);
    let mut s = Sondeur::nouveau(b.clone());
    s.sonder();

    // Écriture signalée (comme le lecteur ou l'édition) : pas de modification extérieure.
    let compteur = ailleurs(&data).set_progress("a", 3, 3, "2026-01-01T10:00:00").unwrap();
    b.signaler_ecriture_autonome(["a".to_string()], Some(compteur));
    assert!(modifiee(&s.sonder()).is_none(), "écriture propre renvoyée comme extérieure");

    // Témoin : la même écriture non signalée serait relue une fois.
    ailleurs(&data).set_progress("b", 1, 1, "2026-01-01T10:00:00").unwrap();
    let (_, modifies, _, ids) = modifiee(&s.sonder()).expect("écriture non signalée");
    assert_eq!((modifies, ids), (1, vec!["b".to_string()]));
}

#[test]
fn suppression_de_comicrust_signalee_avant_nest_pas_renotifiee() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha"), livre("b", "Bravo")]);
    let mut s = Sondeur::nouveau(b.clone());
    s.sonder();
    b.connexion().supprimant(["B".to_string()], || {
        ailleurs(&data).delete(&["b".to_string()]).unwrap();
    });
    // Vérification complète immédiate : la disparition de « b » est la nôtre.
    assert!(modifiee(&s.sonder()).is_none());
}

#[test]
fn hors_ligne_puis_reprise_automatique() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha")]);
    // Pas de sondage avant la panne : sous Linux une connexion SQLite déjà ouverte survit au
    // renommage du dossier (le surveillant ouvre sa connexion au premier sondage).
    let mut s = Sondeur::nouveau(b.clone());

    // Base injoignable : un seul événement de bascule, la cause est donnée, puis les essais
    // continuent sans émettre de nouveau.
    let ecarte = ecarter(&data);
    let ev = s.sonder();
    let e = etats(&ev);
    assert_eq!(e.len(), 1, "{ev:?}");
    assert!(!e[0].en_ligne && e[0].message.is_some() && e[0].essais == 1, "{:?}", e[0]);
    assert!(b.motif_hors_ligne().is_some());
    assert!(s.sonder().is_empty());
    assert_eq!(b.etat_base().essais, 2);
    assert!(!b.etat_base().en_ligne);
    // La bibliothèque affichée reste lisible hors ligne.
    assert_eq!(titres(&b), ["Alpha"]);

    // Retour de la base avec une modification faite entre-temps : état en ligne d'abord, puis
    // la modification est appliquée (sans rechargement complet).
    remettre(&ecarte, &data);
    ailleurs(&data).upsert(&[livre("z", "Zéro")]).unwrap();
    let ev = s.sonder();
    assert!(matches!(&ev[0], EvenementBase::Etat(e) if e.en_ligne && e.essais == 0), "{ev:?}");
    assert_eq!(modifiee(&ev).map(|m| m.0), Some(1));
    assert!(b.motif_hors_ligne().is_none());
    assert_eq!(titres(&b), ["Alpha", "Zéro"]);
}

#[test]
fn reessayer_sonde_tout_de_suite_et_renvoie_letat() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha")]);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    // Période très longue : seul « Réessayer » sonde.
    let surv = Surveillance::demarrer(
        b.clone(),
        Duration::from_secs(3600),
        Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    );
    let ecarte = ecarter(&data);
    let etat = surv.reessayer();
    assert!(!etat.en_ligne && etat.message.is_some());
    assert!(matches!(rx.try_recv(), Ok(EvenementBase::Etat(e)) if !e.en_ligne));
    remettre(&ecarte, &data);
    let etat = surv.reessayer();
    assert!(etat.en_ligne && etat.message.is_none() && etat.essais == 0);
    assert!(matches!(rx.try_recv(), Ok(EvenementBase::Etat(e)) if e.en_ligne));
}

#[test]
fn le_fil_de_sondage_emet_les_evenements() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha")]);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let _surv = Surveillance::demarrer(
        b.clone(),
        Duration::from_millis(50),
        Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    );
    assert!(matches!(rx.recv_timeout(Duration::from_secs(10)), Ok(EvenementBase::Etat(e)) if e.en_ligne));
    ailleurs(&data).upsert(&[livre("n", "Nouveau")]).unwrap();
    let e = rx.recv_timeout(Duration::from_secs(10)).expect("modification extérieure");
    let (nom, charge) = e.pour_le_front();
    assert_eq!(nom, "bibliotheque-modifiee");
    assert_eq!(charge["origine"], "externe");
    assert_eq!(charge["ajoutes"], 1);
    assert_eq!(charge["ids"], serde_json::json!(["n"]));
}

#[test]
fn pas_de_sondage_pendant_une_edition_ni_apres_rechargement() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha")]);
    let mut s = Sondeur::nouveau(b.clone());
    s.sonder();
    ailleurs(&data).upsert(&[livre("n", "Nouveau")]).unwrap();
    {
        let _garde = b.prendre_edition().unwrap();
        assert!(s.sonder().is_empty(), "pas de sondage pendant une opération d'édition");
    }
    // Bibliothèque rechargée : le surveillant est refait d'après la nouvelle base, qui contient
    // déjà « Nouveau » : rien à signaler.
    b.ouvrir().unwrap();
    assert!(modifiee(&s.sonder()).is_none());
    assert_eq!(titres(&b), ["Alpha", "Nouveau"]);
}

fn demande(id: &str) -> DemandeModification {
    DemandeModification {
        ids: vec![id.into()],
        champs: vec![ChampModifie { nom: "Series".into(), valeur: "Nouvelle série".into() }],
        pages: None,
        ecrire_dans_fichier: false,
        confirme: false,
    }
}

#[test]
fn ecritures_refusees_hors_ligne_sans_perte_puis_reprises() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = autonome(&data, &[livre("a", "Alpha")]);
    let e = Edition::nouvelle(b.clone(), Lecteur::nouveau(data.clone(), b.clone()));
    let mut s = Sondeur::nouveau(b.clone());
    assert!(e.lire_fiches(&["a".into()]).unwrap().lecture_seule.is_none());

    let ecarte = ecarter(&data);
    s.sonder();
    // Fiche affichée en lecture seule, avec la raison lisible ; modification refusée, rien tenté.
    let f = e.lire_fiches(&["a".into()]).unwrap();
    assert_eq!(f.lecture_seule.as_deref(), Some(MOTIF_HORS_LIGNE));
    assert_eq!(e.appliquer_modifications(&demande("a"), &|_| {}).unwrap_err(), MOTIF_HORS_LIGNE);
    assert_eq!(e.ecrire_infos_fichiers(&["a".into()], false, &|_| {}).unwrap_err(), MOTIF_HORS_LIGNE);
    assert_eq!(e.convertir_cbr_cbz(&["a".into()], None, false, &|_| {}).unwrap_err(), MOTIF_HORS_LIGNE);
    // Les listes de ComicRust ne sont plus modifiables.
    let l = b.etat_listes();
    assert!(!l.modifiable && l.lecture_seule.as_deref() == Some(MOTIF_HORS_LIGNE), "{l:?}");

    // Reprise : l'écriture repasse, la fiche porte la nouvelle valeur.
    remettre(&ecarte, &data);
    s.sonder();
    assert!(e.lire_fiches(&["a".into()]).unwrap().lecture_seule.is_none());
    let bilan = e.appliquer_modifications(&demande("a"), &|_| {}).unwrap();
    assert!(bilan.complet, "{}", bilan.rapport);
    assert_eq!(b.livre("a").unwrap().series.as_deref(), Some("Nouvelle série"));
    // L'écriture signalée n'est pas renvoyée comme modification extérieure.
    s.sonder();
    assert!(modifiee(&s.sonder()).is_none());
}

/// Configuration MySQL « saisie à la main » (pas issue d'un `ComicRack.ini`) vers un serveur
/// qui ne répond pas : port fermé de la machine locale.
fn config_injoignable() -> DbConfig {
    DbConfig {
        host: "127.0.0.1".into(),
        port: 1,
        user: "cr".into(),
        password: SECRET.into(),
        database: "cr_test".into(),
    }
}

/// Bibliothèque liée à ComicRack par MySQL (sans serveur : base déjà « chargée »), écriture activée.
fn comicrack_injoignable(data: &Path, cfg: DbConfig) -> Biblio {
    let b = Biblio::nouvelle(data.to_path_buf());
    b.installer(Mode::Comicrack, ComicDatabase::from_books(vec![livre("a", "Alpha")]), None);
    b.installer_comicrack_mysql(cfg);
    reglages::enregistrer_reglages_edition(
        data,
        ReglagesEdition { ecriture_comicrack: Some(true), ..Default::default() },
    )
    .unwrap();
    b
}

#[test]
fn comicrack_saisie_a_la_main_injoignable_echec_propre_sans_mot_de_passe() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = comicrack_injoignable(&data, config_injoignable());
    let e = Edition::nouvelle(b.clone(), Lecteur::nouveau(data.clone(), b.clone()));

    // Avant le premier sondage : l'écriture est tentée et échoue proprement, en français.
    let err = e.appliquer_modifications(&demande("a"), &|_| {}).unwrap_err();
    assert!(err.contains("base ComicRack inaccessible"), "{err}");
    assert!(!err.contains(SECRET), "mot de passe dans l'erreur : {err}");
    assert_eq!(b.livre("a").unwrap().series.as_deref(), Some("Série"), "fiche affichée intacte");

    // Le sondage bascule hors ligne ; les écritures sont ensuite refusées sans rien tenter.
    let mut s = Sondeur::nouveau(b.clone());
    let ev = s.sonder();
    let etat = etats(&ev).remove(0);
    assert_eq!(etat.source, SourceBase::ComicrackMysql);
    assert!(!etat.en_ligne);
    let json = serde_json::to_string(&etat).unwrap();
    assert!(!json.contains(SECRET), "mot de passe dans l'état : {json}");
    assert!(!format!("{ev:?}").contains(SECRET));
    assert_eq!(e.appliquer_modifications(&demande("a"), &|_| {}).unwrap_err(), MOTIF_HORS_LIGNE);
    assert_eq!(e.lire_fiches(&["a".into()]).unwrap().lecture_seule.as_deref(), Some(MOTIF_HORS_LIGNE));
}

#[test]
fn comicrack_saisie_a_la_main_incomplete_refusee() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = comicrack_injoignable(&data, DbConfig { password: SECRET.into(), ..Default::default() });
    let e = Edition::nouvelle(b.clone(), Lecteur::nouveau(data.clone(), b.clone()));
    assert_eq!(
        e.appliquer_modifications(&demande("a"), &|_| {}).unwrap_err(),
        crate::surveillance::CONFIG_MYSQL_INCOMPLETE
    );
    assert_eq!(
        e.lire_fiches(&["a".into()]).unwrap().lecture_seule.as_deref(),
        Some(crate::surveillance::CONFIG_MYSQL_INCOMPLETE)
    );
}

#[test]
fn comicrack_lecture_seule_hors_ligne_garde_les_fichiers_modifiables() {
    // Option « Lecture et écriture » absente : rien n'est écrit dans la base, hors ligne ou non ;
    // le message reste celui de la lecture seule (pas de faux « base injoignable »).
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let b = comicrack_injoignable(&data, config_injoignable());
    reglages::enregistrer_reglages_edition(
        &data,
        ReglagesEdition { ecriture_comicrack: Some(false), ..Default::default() },
    )
    .unwrap();
    let e = Edition::nouvelle(b.clone(), Lecteur::nouveau(data.clone(), b.clone()));
    Sondeur::nouveau(b.clone()).sonder();
    assert!(b.motif_hors_ligne().is_some());
    let f = e.lire_fiches(&["a".into()]).unwrap();
    assert_eq!(f.lecture_seule.as_deref(), Some(crate::edition::LECTURE_SEULE_COMICRACK));
}

#[test]
fn masquer_secret_remplace_le_mot_de_passe() {
    assert_eq!(masquer_secret("accès refusé avec abc123 sur hôte", "abc123"), "accès refusé avec *** sur hôte");
    assert_eq!(masquer_secret("rien", ""), "rien");
}

#[test]
fn etat_sans_surveillance_pour_comicdb_xml() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().join("data"));
    // Aucune bibliothèque : état « aucune », sans événement de modification.
    let mut s = Sondeur::nouveau(b.clone());
    let etat = etats(&s.sonder()).remove(0);
    assert_eq!(etat.source, SourceBase::Aucune);
    assert!(etat.en_ligne && !etat.surveillee);
    // `ComicDb.xml` : jamais surveillé par ce sondage, jamais hors ligne.
    b.installer(Mode::Comicrack, ComicDatabase::from_books(vec![livre("a", "Alpha")]), None);
    let etat = etats(&s.sonder()).remove(0);
    assert_eq!(etat.source, SourceBase::ComicrackXml);
    assert!(etat.en_ligne && !etat.surveillee);
}

// ───────────────────────────── MySQL réel (optionnel) ─────────────────────────────

fn fiche_ce(id: &str, titre: &str) -> String {
    format!("<ComicBook Id=\"{id}\" File=\"Z:/BD/{id}.cbz\"><Series>S</Series><Title>{titre}</Title></ComicBook>")
}

#[test]
fn mysql_comicrack_sondage_et_hors_ligne() {
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
    let id = "5f0c1a2b-0000-4000-8000-0000000000bb";
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
    conn.exec_drop("Insert into comics values (?, 5, ?)", (id, fiche_ce(id, "Avant"))).unwrap();

    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().join("data"));
    b.installer(Mode::Comicrack, ComicDatabase::load(&cfg, None).unwrap(), None);
    b.installer_comicrack_mysql(cfg.clone());
    let mut s = Sondeur::nouveau(b.clone());
    let etat = etats(&s.sonder()).remove(0);
    assert!(etat.en_ligne && etat.surveillee && etat.source == SourceBase::ComicrackMysql);

    // Modification faite dans ComicRack : appliquée à la bibliothèque affichée.
    conn.exec_drop("Update comics set data = ?, update_counter = 6 where id = ?", (fiche_ce(id, "Après"), id)).unwrap();
    conn.exec_drop("Update changes set update_counter = 11", ()).unwrap();
    let ev = s.sonder();
    assert_eq!(modifiee(&ev).map(|m| (m.0, m.1)), Some((0, 1)), "{ev:?}");
    assert_eq!(b.livre(id).unwrap().title.as_deref(), Some("Après"));
    assert!(s.sonder().is_empty());

    // Serveur injoignable (port fermé, même identifiants) : hors ligne, sans le mot de passe.
    let b2 = Biblio::nouvelle(d.path().join("data2"));
    b2.installer(Mode::Comicrack, ComicDatabase::from_books(vec![]), None);
    b2.installer_comicrack_mysql(DbConfig { port: 1, ..cfg.clone() });
    let etat = etats(&Sondeur::nouveau(b2).sonder()).remove(0);
    assert!(!etat.en_ligne);
    if !cfg.password.is_empty() {
        assert!(!serde_json::to_string(&etat).unwrap().contains(&cfg.password));
    }
}
