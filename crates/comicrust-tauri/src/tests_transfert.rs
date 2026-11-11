//! Tests du transfert ComicRack → ComicRust (`transfert.rs`, `docs/tauri/ipc-transfert.md`) :
//!
//! - SQLite + `ComicDb.xml` de test (fixture réelle anonymisée : 47 listes, livres fabriqués ici) :
//!   5 323 livres, 47 listes, 0 doublon, idempotence, arrêt, doublon évité, lecture seule de la
//!   source, bascule en autonome ;
//! - le mot de passe MySQL ne sort jamais (aperçu, erreurs, JSON) ;
//! - MySQL réel de ComicRack (`COMICRUST_TEST_MYSQL`, base de test seulement) : ignoré sans la
//!   variable.

use crate::biblio::Biblio;
use crate::reglages::{self, Mode};
use crate::transfert::*;
use comicrust_core::db::DbConfig;
use comicrust_core::library::LibraryBook;
use comicrust_core::store::{Store, StoreConfig};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/listes/ComicDb_anonyme.xml");
/// Livres de la bibliothèque de référence (critère de recette).
const NB_LIVRES: usize = 5323;

/// Mot de passe factice : sert à vérifier qu'il ne ressort jamais (ce n'est aucun vrai mot de passe).
const MDP_FACTICE: &str = "mdp-factice-pour-test";

/// Les tests qui créent les tables `comics` / `changes` dans la base MySQL de test ne tournent pas
/// en même temps (édition et transfert).
pub(crate) fn verrou_mysql() -> MutexGuard<'static, ()> {
    static V: Mutex<()> = Mutex::new(());
    V.lock().unwrap_or_else(|e| e.into_inner())
}

/// `ComicDb.xml` de la fixture (47 listes) dont `<Books />` est remplacé par `n` livres : ceux que
/// citent les listes d'abord, puis des livres fabriqués.
fn comicdb_avec_livres(n: usize) -> String {
    let xml = std::fs::read_to_string(FIXTURE).unwrap();
    let mut ids: Vec<String> = Vec::new();
    let mut vus = HashSet::new();
    for morceau in xml.split("<guid>").skip(1) {
        let id = morceau.split("</guid>").next().unwrap().to_string();
        if vus.insert(id.clone()) {
            ids.push(id);
        }
    }
    let mut i = 0;
    while ids.len() < n {
        let id = format!("{i:08x}-0000-4000-8000-000000000000");
        if vus.insert(id.clone()) {
            ids.push(id);
        }
        i += 1;
    }
    let mut livres = String::from("<Books>\n");
    for (k, id) in ids.iter().enumerate() {
        livres.push_str(&format!(
            "<Book Id=\"{id}\" File=\"Z:\\BD\\Série {}\\Livre {k}.cbz\"><Series>Série {}</Series>\
             <Number>{}</Number><PageCount>20</PageCount></Book>\n",
            k / 7,
            k / 7,
            k % 7 + 1
        ));
    }
    livres.push_str("</Books>");
    assert!(xml.contains("<Books />"));
    xml.replacen("<Books />", &livres, 1)
}

fn demande(source: SourceTransfert, comicdb: Option<&Path>) -> DemandeTransfert {
    DemandeTransfert {
        source,
        comicdb: comicdb.map(|p| p.display().to_string()),
        listes: true,
        dossiers: true,
        basculer: false,
    }
}

/// Dossier de test : `ComicDb.xml` de `n` livres, `data\` vide.
fn cadre(n: usize) -> (tempfile::TempDir, PathBuf, Transfert, Biblio) {
    let d = tempfile::tempdir().unwrap();
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(&xml, comicdb_avec_livres(n)).unwrap();
    let data = d.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let b = Biblio::nouvelle(data);
    (d, xml, Transfert::nouveau(b.clone()), b)
}

fn sans_sortie(_: EvenementTransfert) {}

fn base_autonome(b: &Biblio) -> StoreConfig {
    StoreConfig::Sqlite(b.data().join("comicrust.db"))
}

#[test]
fn recette_5323_livres_47_listes_zero_doublon_idempotent() {
    let (_d, xml, t, b) = cadre(NB_LIVRES);
    let avant = std::fs::read(&xml).unwrap();
    let dem = demande(SourceTransfert::Comicdb, Some(&xml));
    let progressions = Mutex::new(Vec::<String>::new());
    let sortie = |e: EvenementTransfert| {
        let EvenementTransfert::Progression { phase, .. } = e;
        progressions.lock().unwrap().push(phase);
    };
    let r = t.lancer(&dem, &sortie).unwrap();
    assert_eq!((r.livres_source, r.ajoutes, r.remplaces, r.mis_a_jour, r.ecartes), (NB_LIVRES, NB_LIVRES, 0, 0, 0));
    assert_eq!(r.listes, 47, "{}", r.resume);
    assert_eq!(r.nb_livres_destination, Some(NB_LIVRES));
    assert_eq!(r.doublons_restants, Some(0));
    assert!(!r.interrompu && r.etat.is_none() && !r.bascule);
    assert!(r.resume.starts_with("5323 livres de ComicRack : 5323 ajoutés"), "{}", r.resume);
    // Au moins un événement de progression (phase renseignée).
    assert!(progressions.lock().unwrap().iter().any(|p| !p.is_empty()));

    // Idempotent : relancer ne duplique rien.
    let r2 = t.lancer(&dem, &sans_sortie).unwrap();
    assert_eq!((r2.ajoutes, r2.remplaces, r2.mis_a_jour), (0, 0, NB_LIVRES));
    assert_eq!(r2.doublons_evites, NB_LIVRES);
    assert_eq!(r2.nb_livres_destination, Some(NB_LIVRES));
    assert_eq!(r2.doublons_restants, Some(0));
    assert_eq!(r2.listes, 47, "listes remplacées, pas ajoutées");
    let base = Store::open(&base_autonome(&b)).unwrap().load(None).unwrap();
    assert_eq!(base.len(), NB_LIVRES);

    // La source n'a pas été touchée (lecture seule).
    assert_eq!(std::fs::read(&xml).unwrap(), avant, "ComicDb.xml inchangé");
}

#[test]
fn doublon_evite_quand_le_fichier_est_deja_dans_la_base() {
    let (_d, xml, t, b) = cadre(60);
    // Analyse antérieure : même fichier que le livre 0 de ComicRack, autre identifiant.
    let mut s = Store::open(&base_autonome(&b)).unwrap();
    let existant = LibraryBook {
        id: "11111111-2222-4333-8444-555555555555".into(),
        file: Some("Z:\\BD\\Série 0\\Livre 0.cbz".into()),
        ..Default::default()
    };
    s.upsert(&[existant]).unwrap();
    drop(s);
    let r = t.lancer(&demande(SourceTransfert::Comicdb, Some(&xml)), &sans_sortie).unwrap();
    assert_eq!(r.remplaces, 1, "{}", r.resume);
    assert_eq!(r.doublons_evites, 1);
    assert_eq!(r.doublons_restants, Some(0));
}

#[test]
fn arret_demande_garde_les_lots_et_ne_bascule_pas() {
    let (_d, xml, t, b) = cadre(200);
    let dest = base_autonome(&b);
    t.annuler();
    let dem = DemandeTransfert { basculer: true, ..demande(SourceTransfert::Comicdb, Some(&xml)) };
    // `executer` ne remet pas l'arrêt à zéro (c'est `lancer` qui le fait au départ).
    let r = t.executer(None, Some(xml.clone()), &dest, &dem, &sans_sortie).unwrap();
    assert!(r.interrompu, "{}", r.resume);
    assert_eq!(r.listes, 0, "ni listes ni dossiers quand le transfert est interrompu");
    assert!(r.resume.contains("Interrompu"));
    // Un nouveau lancement repart sans arrêt, termine et ne duplique rien.
    let r = t.lancer(&demande(SourceTransfert::Comicdb, Some(&xml)), &sans_sortie).unwrap();
    assert!(!r.interrompu);
    assert_eq!(r.nb_livres_destination, Some(200.max(r.livres_source)));
    assert_eq!(r.doublons_restants, Some(0));
}

#[test]
fn options_listes_et_dossiers() {
    let (_d, xml, t, _b) = cadre(30);
    let dem = DemandeTransfert { listes: false, dossiers: false, ..demande(SourceTransfert::Comicdb, Some(&xml)) };
    let r = t.lancer(&dem, &sans_sortie).unwrap();
    assert_eq!((r.listes, r.dossiers), (0, 0));
    assert!(r.ajoutes >= 30, "{}", r.resume);
}

#[test]
fn erreurs_lisibles() {
    let (d, xml, t, _b) = cadre(30);
    // Source ComicDb.xml sans livres.
    let vide = d.path().join("vide.xml");
    std::fs::write(&vide, std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    let e = t.lancer(&demande(SourceTransfert::Comicdb, Some(&vide)), &sans_sortie).unwrap_err();
    assert!(e.contains("ne contient aucun livre") && e.contains("MySQL"), "{e}");
    // Fichier absent.
    let e = t.lancer(&demande(SourceTransfert::Comicdb, Some(&d.path().join("absent.xml"))), &sans_sortie).unwrap_err();
    assert!(e.contains("absent.xml"), "{e}");
    // Aucun chemin.
    let e = t.lancer(&demande(SourceTransfert::Comicdb, None), &sans_sortie).unwrap_err();
    assert!(e.contains("ComicDb.xml"), "{e}");
    // MySQL demandé sans configuration.
    let e = t.lancer(&demande(SourceTransfert::Mysql, Some(&xml)), &sans_sortie).unwrap_err();
    assert!(e.contains("aucune base MySQL"), "{e}");
}

#[test]
fn une_seule_operation_a_la_fois() {
    let (_d, xml, t, b) = cadre(30);
    let _garde = b.prendre_edition().unwrap();
    let e = t.lancer(&demande(SourceTransfert::Comicdb, Some(&xml)), &sans_sortie).unwrap_err();
    assert!(e.contains("déjà en cours"), "{e}");
}

#[test]
fn bascule_en_autonome_apres_le_transfert() {
    let (_d, xml, t, b) = cadre(NB_LIVRES);
    // Bibliothèque affichée : ComicDb.xml de ComicRack (mode lié).
    reglages::modifier(b.data(), |r| {
        r.mode = Some(Mode::Comicrack);
        r.chemin_ini = Some(xml.clone());
    })
    .unwrap();
    b.ouvrir().unwrap();
    let a = t.apercu();
    assert_eq!(a.mode, Mode::Comicrack);
    assert_eq!(a.comicdb, xml.display().to_string());
    assert!(a.comicdb_present && a.mysql.is_none() && a.source_par_defaut == SourceTransfert::Comicdb);
    assert!(!a.destination_existe && a.nb_livres_destination.is_none() && a.bloque.is_none());

    let dem = DemandeTransfert { basculer: true, ..demande(SourceTransfert::Comicdb, None) };
    let r = t.lancer(&dem, &sans_sortie).unwrap();
    assert!(r.bascule, "{}", r.resume);
    let etat = r.etat.expect("bibliothèque rouverte");
    assert_eq!((etat.mode, etat.nb_livres), (Mode::Autonome, NB_LIVRES));
    assert_eq!(reglages::lire(b.data()).unwrap().mode, Some(Mode::Autonome));
    assert_eq!(b.mode(), Some(Mode::Autonome));
    assert_eq!(t.apercu().nb_livres_destination, Some(NB_LIVRES));
    // Les listes transférées sont celles de la base autonome.
    assert!(!b.arbre_listes().unwrap().listes.is_empty());
}

// ───────────────────────────── Mot de passe ─────────────────────────────

fn ini_mysql(dossier: &Path, mdp: &str, port: u16) -> PathBuf {
    let ini = dossier.join("ComicRack.ini");
    std::fs::write(
        &ini,
        format!("DataSource=mysql:Server=127.0.0.1;Port={port};Database=cr_test;Uid=cr_user;Pwd={mdp};\r\n"),
    )
    .unwrap();
    ini
}

#[test]
fn mot_de_passe_jamais_renvoye() {
    let (d, xml, t, b) = cadre(30);
    // Port fermé : la connexion échoue tout de suite.
    let ini = ini_mysql(d.path(), MDP_FACTICE, 1);
    reglages::modifier(b.data(), |r| {
        r.mode = Some(Mode::Autonome);
        r.chemin_ini = Some(ini.clone());
    })
    .unwrap();
    let a = t.apercu();
    assert_eq!(a.mysql.as_deref(), Some("cr_user@127.0.0.1:1/cr_test"));
    assert_eq!(a.source_par_defaut, SourceTransfert::Mysql);
    let json = serde_json::to_string(&a).unwrap();
    assert!(!json.contains(MDP_FACTICE), "{json}");

    let e = t.lancer(&demande(SourceTransfert::Mysql, Some(&xml)), &sans_sortie).unwrap_err();
    assert!(e.contains("127.0.0.1"), "erreur de connexion lisible : {e}");
    assert!(!e.contains(MDP_FACTICE), "{e}");

    // Les fichiers de réglages ne contiennent pas le mot de passe non plus.
    for f in std::fs::read_dir(b.data()).unwrap() {
        let p = f.unwrap().path();
        if p.is_file() && p.extension().is_none_or(|e| e != "db") {
            assert!(!std::fs::read_to_string(&p).unwrap_or_default().contains(MDP_FACTICE), "{}", p.display());
        }
    }
}

#[test]
fn masquer_retire_le_mot_de_passe() {
    let cfg = DbConfig { password: MDP_FACTICE.into(), ..Default::default() };
    let m = masquer(&format!("accès refusé (mot de passe {MDP_FACTICE}) ; encore {MDP_FACTICE}"), &cfg);
    assert!(!m.contains(MDP_FACTICE) && m.contains("***"), "{m}");
    // Sans mot de passe : texte inchangé.
    assert_eq!(masquer("rien", &DbConfig::default()), "rien");
}

// ───────────────────────────── MySQL réel ─────────────────────────────

/// Base MySQL de ComicRack (`COMICRUST_TEST_MYSQL`, base de test) : les livres sont lus, rien n'est
/// écrit dans ComicRack (fiches et compteurs inchangés), relancer ne duplique pas.
#[test]
fn mysql_comicrack_lecture_seule() {
    use mysql::prelude::Queryable;
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL du transfert ignoré");
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
    let _v = verrou_mysql();
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
    for k in 0..3 {
        let id = format!("5f0c1a2b-0000-4000-8000-00000000000{k}");
        let fiche = format!(
            "<ComicBook Id=\"{id}\" File=\"Z:\\BD\\Thanos {k}.cbz\"><Series>Thanos</Series><Number>{k}</Number></ComicBook>"
        );
        conn.exec_drop("Insert into comics values (?, 5, ?)", (id, fiche)).unwrap();
    }
    let instantane = |c: &mut mysql::Conn| -> Vec<(String, i64, String)> {
        c.query("Select id, update_counter, data from comics order by id").unwrap()
    };
    let avant = instantane(&mut conn);
    let (compteur,): (i64,) = conn.query_first("Select update_counter from changes").unwrap().unwrap();

    let d = tempfile::tempdir().unwrap();
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(&xml, comicdb_avec_livres(1)).unwrap(); // listes seulement : livres de MySQL
    let data = d.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let t = Transfert::nouveau(Biblio::nouvelle(data.clone()));
    let dest = StoreConfig::Sqlite(data.join("comicrust.db"));
    let dem = demande(SourceTransfert::Mysql, Some(&xml));
    let r = t.executer(Some(cfg.clone()), Some(xml), &dest, &dem, &sans_sortie).unwrap();
    assert_eq!((r.livres_source, r.ajoutes), (3, 3), "{}", r.resume);
    assert_eq!(r.doublons_restants, Some(0));
    let r = t.executer(Some(cfg), None, &dest, &DemandeTransfert { listes: false, ..dem }, &sans_sortie).unwrap();
    assert_eq!((r.ajoutes, r.mis_a_jour, r.nb_livres_destination), (0, 3, Some(3)));
    assert_eq!(instantane(&mut conn), avant, "ComicRack intact");
    let (apres,): (i64,) = conn.query_first("Select update_counter from changes").unwrap().unwrap();
    assert_eq!(apres, compteur);
    conn.query_drop("DROP TABLE IF EXISTS comics").unwrap();
    conn.query_drop("DROP TABLE IF EXISTS changes").unwrap();
}
