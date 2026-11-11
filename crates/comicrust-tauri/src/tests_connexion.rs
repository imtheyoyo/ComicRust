//! Tests de la connexion MySQL (étape 6, lot A) : réglages chiffrés, lecture tolérante, choix du
//! premier démarrage, ouverture au redémarrage. Aucun mot de passe écrit dans le code : la valeur
//! fictive est produite à l'exécution. Le vrai MySQL n'est utilisé que si `COMICRUST_TEST_MYSQL`
//! est définie (base de test, jamais `comickrack_demo`) ; sinon le test est ignoré.

use crate::biblio::Biblio;
use crate::connexion::{ChoixMysql, ParametresMysql, SourceBase, TypeStore};
use crate::reglages::{self, Mode, PreferencesVue, Reglages};
use comicrust_core::db::DbConfig;

/// Valeur fictive propre à chaque exécution.
fn mdp_fictif() -> String {
    format!("mdp-test-{}", std::process::id())
}

fn cfg(mdp: &str) -> DbConfig {
    DbConfig { host: "srv".into(), port: 3307, user: "u".into(), password: mdp.into(), database: "bd".into() }
}

/// Serveur qui refuse tout de suite (port 1 de la boucle locale).
fn injoignable(mdp: &str) -> ParametresMysql {
    ParametresMysql {
        serveur: "127.0.0.1".into(),
        port: 1,
        utilisateur: "u".into(),
        mot_de_passe: Some(mdp.into()),
        base: "bd".into(),
    }
}

// ───────────── Réglages ─────────────

#[test]
fn mot_de_passe_mysql_jamais_en_clair_dans_le_fichier() {
    let d = tempfile::tempdir().unwrap();
    let m = mdp_fictif();
    reglages::modifier(d.path(), |r| {
        r.mode = Some(Mode::Autonome);
        r.store_mysql = Some(cfg(&m));
        r.comicrack_mysql = Some(cfg(&m));
    })
    .unwrap();
    let texte = std::fs::read_to_string(reglages::chemin_reglages(d.path())).unwrap();
    assert!(!texte.contains(&m), "{texte}");
    assert!(texte.contains("storeMysql") && texte.contains("comicrackMysql"), "{texte}");
    let r = reglages::lire(d.path()).unwrap();
    let relu = r.store_mysql.clone().unwrap();
    assert_eq!((relu.host.as_str(), relu.port, relu.user.as_str(), relu.database.as_str()), ("srv", 3307, "u", "bd"));
    // Windows : chiffré (`dpapi:`) puis relu ; ailleurs, pas de coffre : non gardé (jamais en clair).
    if cfg!(windows) {
        assert!(texte.contains("dpapi:"), "{texte}");
        assert_eq!(relu.password, m);
    } else {
        assert!(relu.password.is_empty());
    }
    assert!(!format!("{r:?}").contains(&m), "le Debug des réglages masque le mot de passe");
}

#[test]
fn anciens_fichiers_et_lecture_tolerante() {
    let d = tempfile::tempdir().unwrap();
    let ecrire = |t: &str| std::fs::write(reglages::chemin_reglages(d.path()), t).unwrap();
    // Ancien fichier sans les champs MySQL.
    ecrire(r#"{"mode":"autonome","vue":{"tri":"titre"}}"#);
    let r = reglages::lire(d.path()).unwrap();
    assert!(r.store_mysql.is_none() && r.comicrack_mysql.is_none() && r.mode == Some(Mode::Autonome));
    // Entrées invalides : ignorées ou corrigées, le reste du fichier reste lisible.
    ecrire(
        r#"{"mode":"comicrack","storeMysql":"oui",
            "comicrackMysql":{"host":"h","user":"u","database":"b","port":99999,"password":7}}"#,
    );
    let r = reglages::lire(d.path()).unwrap();
    assert_eq!(r.mode, Some(Mode::Comicrack));
    assert!(r.store_mysql.is_none());
    let c = r.comicrack_mysql.unwrap();
    assert_eq!((c.port, c.password.as_str()), (3306, ""), "port hors bornes = défaut, mot de passe invalide = vide");
    // Connexion incomplète (pas de base) : ignorée.
    ecrire(r#"{"storeMysql":{"host":"h","user":"u"}}"#);
    assert!(reglages::lire(d.path()).unwrap().store_mysql.is_none());
    // Aucune connexion enregistrée : rien d'écrit.
    reglages::ecrire(d.path(), &Reglages::default()).unwrap();
    let texte = std::fs::read_to_string(reglages::chemin_reglages(d.path())).unwrap();
    assert!(!texte.contains("Mysql"), "{texte}");
}

#[test]
fn preferences_et_connexions_se_conservent_mutuellement() {
    let d = tempfile::tempdir().unwrap();
    reglages::modifier(d.path(), |r| r.store_mysql = Some(cfg("x"))).unwrap();
    let m = PreferencesVue { tri: Some("titre".into()), ..Default::default() };
    reglages::enregistrer_preferences_vue(d.path(), m).unwrap();
    let r = reglages::lire(d.path()).unwrap();
    assert!(r.store_mysql.is_some());
    assert_eq!(r.vue.tri.as_deref(), Some("titre"));
}

// ───────────── Premier démarrage et ouverture ─────────────

#[test]
fn mysql_injoignable_autonome_et_comicrack_n_enregistrent_rien() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    let m = mdp_fictif();
    for choix in [
        ChoixMysql { mode: Mode::Autonome, store: Some(TypeStore::Mysql), mysql: Some(injoignable(&m)) },
        ChoixMysql { mode: Mode::Comicrack, store: None, mysql: Some(injoignable(&m)) },
    ] {
        let e = b.configurer_mysql(&choix).unwrap_err();
        assert!(e.contains("impossible") && !e.contains(&m), "{e}");
        assert!(!reglages::chemin_reglages(d.path()).exists(), "un choix qui échoue n'est pas enregistré");
        assert!(b.ouvrir().unwrap().premier_demarrage);
    }
}

#[test]
fn mysql_choix_incomplet_refuse() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    let sans = |mode, store| ChoixMysql { mode, store, mysql: None };
    assert_eq!(b.configurer_mysql(&sans(Mode::Comicrack, None)).unwrap_err(), "paramètres MySQL manquants");
    let e = b.configurer_mysql(&sans(Mode::Autonome, Some(TypeStore::Mysql))).unwrap_err();
    assert_eq!(e, "paramètres MySQL manquants");
    let mut p = injoignable("x");
    p.base = " ".into();
    let e = b.configurer_mysql(&ChoixMysql { mode: Mode::Comicrack, store: None, mysql: Some(p) }).unwrap_err();
    assert!(e.contains("nécessaires"), "{e}");
    assert!(!reglages::chemin_reglages(d.path()).exists());
}

/// Autonome sans MySQL : SQLite par défaut, et le choix MySQL précédent est effacé des réglages.
#[test]
fn mysql_autonome_sqlite_par_defaut_et_source_exposee() {
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    reglages::modifier(d.path(), |r| r.store_mysql = Some(cfg(&mdp_fictif()))).unwrap();
    let etat = b.configurer_mysql(&ChoixMysql { mode: Mode::Autonome, store: None, mysql: None }).unwrap();
    assert_eq!(etat.source, SourceBase::Sqlite);
    let r = reglages::lire(d.path()).unwrap();
    assert!(r.store_mysql.is_none() && r.mode == Some(Mode::Autonome));
    assert!(d.path().join("comicrust.db").is_file());
    assert_eq!(b.ouvrir().unwrap().source, SourceBase::Sqlite);
}

/// Au redémarrage, `ouvrir` lit le choix enregistré : MySQL pour la base de ComicRust comme pour la
/// source ComicRack saisie ; l'échec ne montre pas le mot de passe et dit s'il manque.
#[test]
fn ouvrir_lit_le_choix_mysql_enregistre() {
    let m = mdp_fictif();
    for (mode, autonome) in [(Mode::Autonome, true), (Mode::Comicrack, false)] {
        let d = tempfile::tempdir().unwrap();
        let c = injoignable(&m).resoudre(None).unwrap();
        reglages::modifier(d.path(), |r| {
            r.mode = Some(mode);
            if autonome {
                r.store_mysql = Some(c.clone());
            } else {
                r.comicrack_mysql = Some(c.clone());
            }
        })
        .unwrap();
        let e = Biblio::nouvelle(d.path().to_path_buf()).ouvrir().unwrap_err();
        assert!(e.contains("127.0.0.1:1") && !e.contains(&m), "{e}");
        // Hors Windows, le mot de passe n'est pas gardé : l'erreur le dit.
        assert_eq!(e.contains("saisis-le à nouveau"), !cfg!(windows), "{e}");
        let texte = std::fs::read_to_string(reglages::chemin_reglages(d.path())).unwrap();
        assert!(!texte.contains(&m), "mot de passe jamais en clair : {texte}");
    }
}

/// Vrai MySQL (si `COMICRUST_TEST_MYSQL` est définie) : base de ComicRust en MySQL, les tables de
/// ComicRust y sont créées.
#[test]
fn mysql_reel_base_de_comicrust_si_variable_definie() {
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL ignoré");
        return;
    };
    let c = DbConfig::from_connection_string(&url).unwrap();
    assert_ne!(c.database, "comickrack_demo", "jamais la base de démonstration");
    let p = ParametresMysql {
        serveur: c.host.clone(),
        port: c.port,
        utilisateur: c.user.clone(),
        mot_de_passe: Some(c.password.clone()),
        base: c.database.clone(),
    };
    let d = tempfile::tempdir().unwrap();
    let b = Biblio::nouvelle(d.path().to_path_buf());
    let choix = ChoixMysql { mode: Mode::Autonome, store: Some(TypeStore::Mysql), mysql: Some(p) };
    let etat = match b.configurer_mysql(&choix) {
        Ok(e) => e,
        Err(e) => return eprintln!("MySQL de test injoignable : test ignoré ({})", crate::connexion::masquer(&e, &c)),
    };
    assert_eq!((etat.mode, etat.source), (Mode::Autonome, SourceBase::Mysql));
    assert!(matches!(b.store_autonome(), Some(comicrust_core::store::StoreConfig::MySql(_))));
    let texte = std::fs::read_to_string(reglages::chemin_reglages(d.path())).unwrap();
    assert!(c.password.is_empty() || !texte.contains(&c.password), "mot de passe jamais en clair");
    // Redémarrage : possible seulement là où le mot de passe est gardé (DPAPI).
    if cfg!(windows) {
        assert_eq!(Biblio::nouvelle(d.path().to_path_buf()).ouvrir().unwrap().source, SourceBase::Mysql);
    }
}
