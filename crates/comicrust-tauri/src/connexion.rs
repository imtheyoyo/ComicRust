//! Connexion MySQL (étape 6, lot A) : formulaire de connexion, bouton « Tester », choix de la base
//! de ComicRust (SQLite ou MySQL) et source ComicRack saisie à la main (sans `ComicRack.ini`).
//!
//! Contrat IPC : `docs/tauri/ipc-bibliotheque.md` (section « Connexion MySQL »). Logique portée de
//! `MySqlForm` / `SetupWindow` de la version egui (`crates/comicrust/src/ui/setup_window.rs`).
//!
//! **Le mot de passe ne sort jamais** : le front l'envoie (champ masqué) mais ne le relit jamais,
//! seulement « défini oui / non » ([`ParametresMysqlPublic::mot_de_passe_defini`]). Il est écrit
//! chiffré par `comicrust_core::secret` dans `data\comicrust-tauri.json`, absent des messages
//! d'erreur ([`masquer`]) et du `Debug` de [`ParametresMysql`].

use crate::reglages::{self, Mode};
use comicrust_core::db::{self, DbConfig};
use comicrust_core::store::{Store, StoreConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Port MySQL par défaut.
pub const PORT_DEFAUT: u16 = 3306;

fn port_defaut() -> u16 {
    PORT_DEFAUT
}

/// Paramètres saisis dans le formulaire (envoyés par le front).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParametresMysql {
    pub serveur: String,
    #[serde(default = "port_defaut")]
    pub port: u16,
    pub utilisateur: String,
    /// `None` : garder le mot de passe déjà enregistré pour cette connexion (le champ masqué est
    /// laissé vide à la réédition). `Some("")` : mot de passe vide.
    #[serde(default)]
    pub mot_de_passe: Option<String>,
    pub base: String,
}

// Jamais de mot de passe dans les journaux.
impl std::fmt::Debug for ParametresMysql {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParametresMysql")
            .field("serveur", &self.serveur)
            .field("port", &self.port)
            .field("utilisateur", &self.utilisateur)
            .field("mot_de_passe", &self.mot_de_passe.as_ref().map(|m| if m.is_empty() { "" } else { "***" }))
            .field("base", &self.base)
            .finish()
    }
}

impl ParametresMysql {
    /// Connexion complète. `memorise` : connexion déjà enregistrée, dont le mot de passe est repris
    /// quand le formulaire n'en fournit pas.
    pub fn resoudre(&self, memorise: Option<&DbConfig>) -> Result<DbConfig, String> {
        let cfg = DbConfig {
            host: self.serveur.trim().to_string(),
            port: self.port,
            user: self.utilisateur.trim().to_string(),
            password: match &self.mot_de_passe {
                Some(m) => m.clone(),
                None => memorise.map(|c| c.password.clone()).unwrap_or_default(),
            },
            database: self.base.trim().to_string(),
        };
        if !cfg.is_complete() {
            return Err("serveur, utilisateur et base sont nécessaires".into());
        }
        if cfg.port == 0 {
            return Err("port invalide".into());
        }
        Ok(cfg)
    }
}

/// Connexion telle que le front la relit : **sans mot de passe**.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParametresMysqlPublic {
    pub serveur: String,
    pub port: u16,
    pub utilisateur: String,
    /// Un mot de passe est enregistré (lisible sur ce PC, par ce compte Windows).
    pub mot_de_passe_defini: bool,
    pub base: String,
}

impl From<&DbConfig> for ParametresMysqlPublic {
    fn from(c: &DbConfig) -> Self {
        Self {
            serveur: c.host.clone(),
            port: c.port,
            utilisateur: c.user.clone(),
            mot_de_passe_defini: !c.password.is_empty(),
            base: c.database.clone(),
        }
    }
}

/// Où est la base de ComicRust en mode autonome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeStore {
    /// `data\comicrust.db`.
    Sqlite,
    /// Base MySQL / MariaDB (doit exister ; les tables y sont créées).
    Mysql,
}

/// D'où viennent les livres affichés (champ `source` de `EtatBiblio`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceBase {
    Sqlite,
    Mysql,
    /// `ComicDb.xml` de ComicRack.
    Xml,
}

/// Choix du premier démarrage avec MySQL (commande `configurer_mysql`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoixMysql {
    pub mode: Mode,
    /// Mode autonome : `sqlite` (défaut) ou `mysql` (alors `mysql` est obligatoire).
    #[serde(default)]
    pub store: Option<TypeStore>,
    /// Mode autonome + `store: 'mysql'` : base de ComicRust. Mode ComicRack : base MySQL de
    /// ComicRack saisie à la main (sans `ComicRack.ini`).
    #[serde(default)]
    pub mysql: Option<ParametresMysql>,
}

/// Connexions MySQL enregistrées, sans mot de passe (commande `lire_connexions_mysql`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnexionsMysql {
    /// Base de ComicRust choisie (SQLite tant que rien d'autre n'est enregistré).
    pub store: TypeStore,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_mysql: Option<ParametresMysqlPublic>,
    /// Connexion ComicRack saisie à la main.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comicrack_mysql: Option<ParametresMysqlPublic>,
}

/// Ce que le bouton « Tester » vérifie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CibleTest {
    /// Base de ComicRust : accès au serveur et à la base ; **le schéma de ComicRust y est créé**
    /// s'il manque (comme « Continuer »), la base doit donc être une base à lui.
    Autonome,
    /// Base MySQL de ComicRack : accès et table `comics` ; rien n'est écrit.
    Comicrack,
}

/// Résultat d'un test réussi.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatTest {
    /// Nombre de livres de la base.
    pub nb_livres: usize,
    /// « utilisateur@serveur:port/base », sans mot de passe.
    pub description: String,
}

/// Retire le mot de passe d'un message d'erreur (au cas où une couche basse le répéterait).
pub fn masquer(message: &str, cfg: &DbConfig) -> String {
    if cfg.password.is_empty() {
        message.to_string()
    } else {
        message.replace(&cfg.password, "***")
    }
}

/// Teste la connexion saisie. `data` sert à reprendre le mot de passe déjà enregistré quand le
/// formulaire n'en fournit pas.
pub fn tester(data: &Path, cible: CibleTest, p: &ParametresMysql) -> Result<ResultatTest, String> {
    let memorise = reglages::lire(data)?;
    let memorise = match cible {
        CibleTest::Autonome => memorise.store_mysql,
        CibleTest::Comicrack => memorise.comicrack_mysql,
    };
    let cfg = p.resoudre(memorise.as_ref())?;
    let n = match cible {
        CibleTest::Autonome => Store::test(&StoreConfig::MySql(cfg.clone())),
        CibleTest::Comicrack => db::test_connection(&cfg),
    }
    .map_err(|e| masquer(&e.to_string(), &cfg))?;
    Ok(ResultatTest { nb_livres: n, description: cfg.describe() })
}

/// Connexions enregistrées (sans mot de passe).
pub fn lire_connexions(data: &Path) -> Result<ConnexionsMysql, String> {
    let r = reglages::lire(data)?;
    Ok(ConnexionsMysql {
        store: if r.store_mysql.is_some() { TypeStore::Mysql } else { TypeStore::Sqlite },
        store_mysql: r.store_mysql.as_ref().map(Into::into),
        comicrack_mysql: r.comicrack_mysql.as_ref().map(Into::into),
    })
}

/// Complète un message d'échec d'ouverture quand aucun mot de passe n'est disponible.
pub fn avec_indice_mot_de_passe(message: String, cfg: &DbConfig) -> String {
    let message = masquer(&message, cfg);
    if cfg.password.is_empty() {
        format!(
            "{message}\n\nAucun mot de passe n'est enregistré pour cette connexion (réglages copiés \
             depuis un autre PC ou un autre compte Windows ?) : saisis-le à nouveau."
        )
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(mdp: Option<&str>) -> ParametresMysql {
        ParametresMysql {
            serveur: " srv ".into(),
            port: 3307,
            utilisateur: "u".into(),
            mot_de_passe: mdp.map(str::to_string),
            base: " b ".into(),
        }
    }

    /// Valeur fictive propre à chaque exécution (aucun mot de passe écrit dans le code).
    fn marqueur() -> String {
        format!("mdp-test-{}", std::process::id())
    }

    #[test]
    fn resoudre_nettoie_et_reprend_le_mot_de_passe_memorise() {
        let m = marqueur();
        let c = p(Some(&m)).resoudre(None).unwrap();
        assert_eq!((c.host.as_str(), c.port, c.user.as_str(), c.database.as_str()), ("srv", 3307, "u", "b"));
        assert_eq!(c.password, m);
        // Champ laissé vide à la réédition : le mot de passe enregistré est repris.
        let memo = DbConfig { password: m.clone(), ..c.clone() };
        assert_eq!(p(None).resoudre(Some(&memo)).unwrap().password, m);
        // Mot de passe explicitement vide : respecté.
        assert_eq!(p(Some("")).resoudre(Some(&memo)).unwrap().password, "");
        assert_eq!(p(None).resoudre(None).unwrap().password, "");
    }

    #[test]
    fn resoudre_refuse_les_champs_manquants() {
        let mut x = p(None);
        x.serveur = "  ".into();
        assert!(x.resoudre(None).unwrap_err().contains("nécessaires"));
        let mut x = p(None);
        x.port = 0;
        assert_eq!(x.resoudre(None).unwrap_err(), "port invalide");
    }

    #[test]
    fn jamais_de_mot_de_passe_dans_debug_ni_dans_la_vue_publique() {
        let m = marqueur();
        let x = p(Some(&m));
        assert!(!format!("{x:?}").contains(&m));
        let cfg = x.resoudre(None).unwrap();
        let vue = ParametresMysqlPublic::from(&cfg);
        assert!(vue.mot_de_passe_defini);
        let json = serde_json::to_string(&vue).unwrap();
        assert!(!json.contains(&m) && json.contains("\"motDePasseDefini\":true"), "{json}");
        assert!(!json.contains("motDePasse\""), "pas de champ mot de passe : {json}");
        assert!(!ParametresMysqlPublic::from(&DbConfig { password: String::new(), ..cfg }).mot_de_passe_defini);
    }

    #[test]
    fn masquer_remplace_le_mot_de_passe() {
        let m = marqueur();
        let cfg = DbConfig { password: m.clone(), ..Default::default() };
        let msg = masquer(&format!("accès refusé avec {m} !"), &cfg);
        assert!(!msg.contains(&m) && msg.contains("***"), "{msg}");
        assert_eq!(masquer("inchangé", &DbConfig::default()), "inchangé");
        let hint = avec_indice_mot_de_passe("échec".into(), &DbConfig::default());
        assert!(hint.contains("saisis-le à nouveau"));
        assert_eq!(avec_indice_mot_de_passe("échec".into(), &cfg), "échec");
    }

    #[test]
    fn desserialisation_du_front() {
        let c: ChoixMysql = serde_json::from_str(
            r#"{"mode":"autonome","store":"mysql","mysql":{"serveur":"h","utilisateur":"u","base":"b"}}"#,
        )
        .unwrap();
        assert_eq!(c.store, Some(TypeStore::Mysql));
        let m = c.mysql.unwrap();
        assert_eq!((m.port, m.mot_de_passe), (PORT_DEFAUT, None));
        let c: ChoixMysql = serde_json::from_str(r#"{"mode":"comicrack"}"#).unwrap();
        assert!(c.store.is_none() && c.mysql.is_none());
    }

    #[test]
    fn test_sur_serveur_injoignable_sans_fuite_du_mot_de_passe() {
        let d = tempfile::tempdir().unwrap();
        let m = marqueur();
        for cible in [CibleTest::Autonome, CibleTest::Comicrack] {
            // Port 1 sur la boucle locale : refus immédiat.
            let x = ParametresMysql {
                serveur: "127.0.0.1".into(),
                port: 1,
                utilisateur: "u".into(),
                mot_de_passe: Some(m.clone()),
                base: "b".into(),
            };
            let e = tester(d.path(), cible, &x).unwrap_err();
            assert!(e.contains("impossible") && !e.contains(&m), "{e}");
        }
        assert!(tester(d.path(), CibleTest::Comicrack, &p(None).tap(|x| x.base.clear())).is_err());
    }

    trait Tap: Sized {
        fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
            f(&mut self);
            self
        }
    }
    impl Tap for ParametresMysql {}

    #[test]
    fn connexions_lues_sans_mot_de_passe() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            lire_connexions(d.path()).unwrap(),
            ConnexionsMysql { store: TypeStore::Sqlite, store_mysql: None, comicrack_mysql: None }
        );
        let m = marqueur();
        let cfg = p(Some(&m)).resoudre(None).unwrap();
        reglages::modifier(d.path(), |r| r.store_mysql = Some(cfg.clone())).unwrap();
        let c = lire_connexions(d.path()).unwrap();
        assert_eq!(c.store, TypeStore::Mysql);
        assert!(c.comicrack_mysql.is_none());
        let v = c.store_mysql.unwrap();
        // Windows : mot de passe chiffré relu ; ailleurs, pas de coffre : non gardé.
        assert_eq!(v.mot_de_passe_defini, cfg!(windows));
        assert!(!serde_json::to_string(&lire_connexions(d.path()).unwrap()).unwrap().contains(&m));
    }

    /// Vrai serveur : seulement si `COMICRUST_TEST_MYSQL` est définie (chaîne de connexion d'une
    /// base de test, jamais la base de démonstration). Ignoré sinon ou si injoignable.
    #[test]
    fn test_reel_comicrack_si_variable_definie() {
        let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
            eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL ignoré");
            return;
        };
        let cfg = DbConfig::from_connection_string(&url).unwrap();
        assert_ne!(cfg.database, "comickrack_demo", "jamais la base de démonstration");
        let d = tempfile::tempdir().unwrap();
        let x = ParametresMysql {
            serveur: cfg.host.clone(),
            port: cfg.port,
            utilisateur: cfg.user.clone(),
            mot_de_passe: Some(cfg.password.clone()),
            base: cfg.database.clone(),
        };
        // Base de ComicRust : le schéma est créé dans la base de test.
        match tester(d.path(), CibleTest::Autonome, &x) {
            Ok(r) => {
                assert!(r.description.contains(&cfg.database));
                assert!(cfg.password.is_empty() || !r.description.contains(&cfg.password));
            }
            Err(e) => eprintln!("MySQL de test injoignable ou refusé : {}", masquer(&e, &cfg)),
        }
    }
}
