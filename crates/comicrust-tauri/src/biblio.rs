//! Bibliothèque : ouverture (autonome ou liée à ComicRack, en lecture seule), liste filtrée /
//! triée / regroupée, détail d'un livre, dossiers surveillés, analyse (scan), couvertures.
//!
//! Aucune dépendance à Tauri : tout est testé ici (SQLite autonome, scan d'un dossier de CBZ).
//! Les commandes de `app.rs` ne font qu'appeler ces fonctions. Logique reprise de la version
//! egui (`library/mod.rs`, `library/scan.rs`, assistant `setup_window.rs`), dupliquée (voir le
//! plan : on ne touche pas à `crates/comicrust`).

use crate::connexion::{self, ChoixMysql, SourceBase, TypeStore};
use crate::couvertures;
use crate::reglages::{self, Mode, Reglages};
use crate::tri::{self, ClefTri, Regroupement};
use comicrust_core::db::{ComicDatabase, DbConfig};
use comicrust_core::library::LibraryBook;
use comicrust_core::scanner::{path_key, ScanProgress};
use comicrust_core::store::{new_book_id, Store, StoreConfig, SQLITE_FILE_NAME};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Fenêtre maximale d'un `lister_livres`.
pub const NOMBRE_MAX: usize = 5000;
/// Période d'émission de `scan-progression`.
const PERIODE_PROGRESSION: Duration = Duration::from_millis(200);

// ───────────────────────────── Types de l'IPC ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatBiblio {
    pub mode: Mode,
    pub nb_livres: usize,
    pub premier_demarrage: bool,
    /// D'où viennent les livres : SQLite, MySQL (base de ComicRust ou de ComicRack) ou `ComicDb.xml`.
    pub source: SourceBase,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoixPremierDemarrage {
    pub mode: Mode,
    /// Mode ComicRack : `ComicRack.ini` (MySQL) ou `ComicDb.xml` ; absent = recherche automatique.
    pub chemin_ini: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SensTri {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Groupe {
    #[default]
    Aucun,
    Serie,
    Editeur,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequeteLivres {
    pub recherche: Option<String>,
    pub tri: String,
    pub sens_tri: SensTri,
    pub groupe: Option<Groupe>,
    pub debut: usize,
    pub nombre: usize,
    /// Liste affichée (`docs/tauri/ipc-listes.md`) ; absente : toute la bibliothèque.
    #[serde(default)]
    pub liste: Option<String>,
    /// Pile ouverte (vue empilée par série) : seuls les livres de cette série sont listés. Titre tel
    /// que renvoyé par `lister_piles` (`(sans série)` pour les livres sans série).
    #[serde(default)]
    pub serie: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivreResume {
    pub id: String,
    pub titre: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serie: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numero: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editeur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annee: Option<i32>,
    pub pages: u32,
    pub lu: bool,
    pub url_couverture: String,
    /// Avec une liste : rang du livre dans la liste (1 = premier), colonne « # ».
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rang: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLivres {
    pub total: usize,
    pub livres: Vec<LivreResume>,
}

/// Une pile de la vue empilée par série (`lister_piles`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pile {
    /// Titre de la pile : série affichée (`ShadowSeries`), `(sans série)` si vide.
    pub serie: String,
    pub nb_livres: usize,
    /// Livres lus à 100 % (barre de lecture de la pile).
    pub nb_lus: usize,
    /// Identifiants des livres de la pile, dans l'ordre de la vue (glisser une pile vers une liste).
    pub ids: Vec<String>,
    /// Couverture du premier livre de la pile.
    pub url_couverture: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagePiles {
    /// Nombre de livres de la vue (somme des piles).
    pub total_livres: usize,
    pub piles: Vec<Pile>,
}

/// Relecture en cours d'un livre déjà lu (voir [`LivreDetail::relecture`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relecture {
    /// Page courante de la relecture, à partir de 1.
    pub page: u32,
    /// Nombre de pages du livre.
    pub total: u32,
    /// Avancement de la relecture, 1 à 99 (arrondi).
    pub pourcentage: u32,
}

/// Fiche complète d'un livre (champs absents = non renseignés).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivreDetail {
    pub id: String,
    pub titre: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serie: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numero: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annee: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mois: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jour: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editeur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    pub pages: u32,
    pub lu: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progression: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_courante: Option<u32>,
    /// Page la plus loin lue, à partir de 1 (affichage « page n / total »), d'après la source
    /// de `progression`. Absente : jamais ouvert.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_lue: Option<u32>,
    /// D'où vient `progression` : `"comicrack"` (fiche / base) ou `"comicrust"` (lecture faite dans
    /// ComicRust, `avancement.json`, quand elle est plus avancée). Absente : jamais ouvert.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_lecture: Option<String>,
    /// **Relecture en cours** d'un livre déjà lu (lecture ComicRust, `avancement.json`) : le
    /// livre reste `lu` (fiche non dégradée), le panneau affiche aussi la position de la
    /// relecture. Absente : pas de relecture en cours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relecture: Option<Relecture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenariste: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dessinateur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encreur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coloriste: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lettreur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artiste_couverture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redacteur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traducteur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arc_narratif: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personnages: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lieux: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etiquettes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critique: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub langue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classement_age: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noir_et_blanc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manga: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_communaute: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isbn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taille_fichier: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_ajout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derniere_lecture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fichier: Option<String>,
    pub fichier_present: bool,
    pub manquant: bool,
    pub url_couverture: String,
}

/// Événements de l'analyse, transmis au front par `app.rs`.
#[derive(Debug, Clone, PartialEq)]
pub enum EvenementScan {
    Progression { fait: usize, total: usize },
    Termine { ajoutes: usize, mis_a_jour: usize, manquants: usize },
    Erreur { message: String },
}

impl EvenementScan {
    /// Nom de l'événement Tauri et sa charge JSON (camelCase), selon le contrat IPC.
    pub fn pour_le_front(&self) -> (&'static str, serde_json::Value) {
        use serde_json::json;
        match self {
            EvenementScan::Progression { fait, total } => ("scan-progression", json!({"fait": fait, "total": total})),
            EvenementScan::Termine { ajoutes, mis_a_jour, manquants } => {
                ("scan-termine", json!({"ajoutes": ajoutes, "misAJour": mis_a_jour, "manquants": manquants}))
            }
            EvenementScan::Erreur { message } => ("scan-erreur", json!({"message": message})),
        }
    }
}

// ───────────────────────────── État ─────────────────────────────

/// Clé du cache de vue : recherche normalisée, tri, sens, groupe, génération de la base, liste
/// affichée et version des listes.
type CleVue = (String, ClefTri, bool, Regroupement, u64, Option<String>, u64);

/// Vue calculée : livres affichés (indices dans la base) et rang de chaque livre dans la liste
/// affichée (`u32::MAX` : hors de la liste).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Vue {
    pub indices: Vec<usize>,
    pub rangs: Vec<u32>,
}

impl Vue {
    /// Rang d'un livre (indice dans la base) dans la liste affichée.
    pub fn rang(&self, i: usize) -> Option<usize> {
        self.rangs.get(i).copied().filter(|r| *r != u32::MAX).map(|r| r as usize)
    }
}

#[derive(Default)]
struct Interne {
    mode: Option<Mode>,
    base: Option<Arc<ComicDatabase>>,
    /// Mode autonome : la base SQLite.
    store: Option<StoreConfig>,
    /// Augmente à chaque remplacement de la base (invalide la vue).
    generation: u64,
    /// Augmente à chaque changement du contenu des fiches (base remplacée, fiches modifiées,
    /// lecture notée) : invalide le contenu calculé des listes intelligentes.
    contenu: u64,
    vue: Option<(CleVue, Arc<Vue>)>,
    scan_en_cours: bool,
    /// Une opération d'édition (fiches, fichiers, renommage) est en cours : l'analyse et le
    /// changement de bibliothèque attendent sa fin (sinon un fichier renommé pendant l'analyse
    /// serait ajouté comme un nouveau livre, et la bibliothèque rechargée écraserait les fiches
    /// reportées par l'édition).
    edition_en_cours: bool,
    /// Mode lié à ComicRack par MySQL : paramètres de connexion (en mémoire seulement, relus
    /// dans `ComicRack.ini` à chaque ouverture ; jamais écrits dans les réglages).
    comicrack: Option<DbConfig>,
    /// Livres ajoutés par la dernière analyse, pas encore lus par les automatismes `NewBooks`
    /// des scripts ([`Biblio::prendre_ajoutes`]).
    ajoutes: Vec<String>,
}

/// D'où vient la bibliothèque ouverte (pour l'édition : où écrire).
#[derive(Debug, Clone, PartialEq)]
pub enum SourceBiblio {
    /// Base de ComicRust.
    Autonome(StoreConfig),
    /// Base MySQL de ComicRack.
    ComicrackMysql(DbConfig),
    /// `ComicDb.xml` de ComicRack.
    ComicrackXml,
    Aucune,
}

/// Bibliothèque ouverte, partageable entre les commandes et les fils (clonage bon marché).
#[derive(Clone)]
pub struct Biblio {
    data: PathBuf,
    interne: Arc<Mutex<Interne>>,
    pool: Arc<couvertures::Pool>,
    /// Cache négatif : identifiants dont la couverture a échoué, avec l'heure de l'échec.
    negatif: Arc<Mutex<HashMap<String, Instant>>>,
    /// Révision de la couverture des livres modifiés (ajoutée à `urlCouverture` : la WebView
    /// garde une heure l'image d'une même URL).
    revisions: Arc<Mutex<HashMap<String, u32>>>,
    /// Listes ComicRack (`listes.rs`) : listes lues, contenu calculé, opération en cours.
    pub(crate) listes: Arc<Mutex<crate::listes::CacheListes>>,
    /// État de la connexion à la base et écritures à signaler à la surveillance.
    pub(crate) connexion: Arc<crate::surveillance::Connexion>,
}

/// Durée pendant laquelle un échec de couverture n'est pas retenté.
const DUREE_ECHEC: Duration = Duration::from_secs(180);
/// Au-delà, `fichierPresent` est donné faux (partage réseau qui ne répond plus).
const DELAI_FICHIER: Duration = Duration::from_secs(2);

/// Remet `scan_en_cours` à faux quand elle disparaît, quelle que soit la sortie (erreur,
/// panique du fil).
struct GardeScan(Arc<Mutex<Interne>>);

impl Drop for GardeScan {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).scan_en_cours = false;
    }
}

/// Remet `edition_en_cours` à faux quand elle disparaît (voir [`Biblio::prendre_edition`]).
pub struct GardeEdition(Arc<Mutex<Interne>>);

impl Drop for GardeEdition {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).edition_en_cours = false;
    }
}

impl Biblio {
    /// Bibliothèque non ouverte pour un dossier `data` (voir [`Biblio::ouvrir`]).
    pub fn nouvelle(data: PathBuf) -> Self {
        Biblio {
            data,
            interne: Arc::default(),
            pool: Arc::new(couvertures::Pool::nouveau(couvertures::FILS)),
            negatif: Arc::default(),
            revisions: Arc::default(),
            listes: Arc::default(),
            connexion: Arc::default(),
        }
    }

    /// Dossier `data\` portable.
    pub fn data(&self) -> &Path {
        &self.data
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Fils de travail des couvertures (le protocole `cr-couv` y met ses tâches).
    pub fn pool(&self) -> &couvertures::Pool {
        &self.pool
    }

    /// Dossier `data\couvertures`.
    pub fn dossier_couvertures(&self) -> PathBuf {
        couvertures::dossier(&self.data)
    }

    // ───────────── Ouverture ─────────────

    /// Ouvre la bibliothèque choisie au premier démarrage ; sans choix, renvoie l'état
    /// « premier démarrage » sans rien ouvrir. Rappelable pour recharger.
    pub fn ouvrir(&self) -> Result<EtatBiblio, String> {
        self.refuser_pendant_analyse()?;
        let r = reglages::lire(&self.data)?;
        let Some(mode) = r.mode else {
            return Ok(EtatBiblio {
                mode: Mode::Autonome,
                nb_livres: 0,
                premier_demarrage: true,
                source: SourceBase::Sqlite,
            });
        };
        let (base, store, cr) = charger(&self.data, &r)?;
        let nb = base.len();
        let source = source_de(&store, &cr);
        self.installer(mode, base, store);
        self.verrou().comicrack = cr;
        Ok(EtatBiblio { mode, nb_livres: nb, premier_demarrage: false, source })
    }

    /// Enregistre le choix du premier démarrage et ouvre la bibliothèque. Rien n'est écrit dans
    /// ComicRack (lecture seule) ni copié depuis lui.
    pub fn configurer(&self, choix: &ChoixPremierDemarrage) -> Result<EtatBiblio, String> {
        self.refuser_pendant_analyse()?;
        let r = match choix.mode {
            Mode::Autonome => Reglages { mode: Some(Mode::Autonome), ..Default::default() },
            Mode::Comicrack => {
                let saisi = choix.chemin_ini.as_deref().map(nettoyer_chemin).filter(|s| !s.is_empty());
                let chemin = match saisi {
                    Some(c) => PathBuf::from(c),
                    None => chercher_ini().ok_or(
                        "ComicRack.ini introuvable aux emplacements habituels : indique son chemin \
                         (à côté de ComicRack.exe) ou celui de ComicDb.xml",
                    )?,
                };
                if !chemin.is_file() {
                    return Err(format!("fichier introuvable : {}", chemin.display()));
                }
                Reglages { mode: Some(Mode::Comicrack), chemin_ini: Some(chemin), ..Default::default() }
            }
        };
        self.appliquer_choix(r)
    }

    /// Premier démarrage avec MySQL : base de ComicRust en MySQL (mode autonome) ou base MySQL de
    /// ComicRack saisie à la main (mode lié, sans `ComicRack.ini`). Le mot de passe saisi est
    /// enregistré chiffré ; un choix qui échoue (serveur injoignable…) n'est pas enregistré.
    pub fn configurer_mysql(&self, choix: &ChoixMysql) -> Result<EtatBiblio, String> {
        self.refuser_pendant_analyse()?;
        let actuels = reglages::lire(&self.data)?;
        let r = match choix.mode {
            Mode::Autonome => match choix.store.unwrap_or(TypeStore::Sqlite) {
                TypeStore::Sqlite => Reglages { mode: Some(Mode::Autonome), ..Default::default() },
                TypeStore::Mysql => {
                    let p = choix.mysql.as_ref().ok_or("paramètres MySQL manquants")?;
                    let cfg = p.resoudre(actuels.store_mysql.as_ref())?;
                    Reglages { mode: Some(Mode::Autonome), store_mysql: Some(cfg), ..Default::default() }
                }
            },
            Mode::Comicrack => {
                let p = choix.mysql.as_ref().ok_or("paramètres MySQL manquants")?;
                let cfg = p.resoudre(actuels.comicrack_mysql.as_ref())?;
                Reglages { mode: Some(Mode::Comicrack), comicrack_mysql: Some(cfg), ..Default::default() }
            }
        };
        self.appliquer_choix(r)
    }

    /// Ouvre la bibliothèque décrite par `r` puis, seulement si elle s'ouvre, enregistre le choix
    /// (mode, chemin, connexions MySQL) en gardant les préférences d'affichage.
    fn appliquer_choix(&self, r: Reglages) -> Result<EtatBiblio, String> {
        let mode = r.mode.ok_or("aucun mode choisi")?;
        if mode == Mode::Autonome {
            // Crée la base (schéma) : seul endroit où un fichier absent est créé.
            let cfg = config_autonome(&self.data, &r);
            Store::open(&cfg).map_err(|e| erreur_store(&cfg, e.to_string()))?;
        }
        // Charge d'abord : un choix qui échoue (MySQL injoignable…) n'est pas enregistré.
        let (base, store, cr) = charger(&self.data, &r)?;
        if mode == Mode::Comicrack && base.is_empty() {
            return Err("cette bibliothèque ComicRack ne contient aucun livre \
                        (ComicRack utilise sans doute une base MySQL : indique son ComicRack.ini \
                        ou les paramètres de sa base)"
                .into());
        }
        // Seul le choix de bibliothèque change : les préférences d'affichage sont conservées.
        reglages::modifier(&self.data, |actuels| {
            // Changer de base ComicRack ne reporte jamais « Lecture et écriture » : le
            // consentement donné pour une base ne vaut pas pour une autre.
            if actuels.mode != r.mode
                || actuels.chemin_ini != r.chemin_ini
                || actuels.comicrack_mysql != r.comicrack_mysql
            {
                actuels.edition.ecriture_comicrack = None;
            }
            actuels.mode = r.mode;
            actuels.chemin_ini = r.chemin_ini.clone();
            actuels.store_mysql = r.store_mysql.clone();
            actuels.comicrack_mysql = r.comicrack_mysql.clone();
        })?;
        let nb = base.len();
        let source = source_de(&store, &cr);
        self.installer(mode, base, store);
        self.verrou().comicrack = cr;
        Ok(EtatBiblio { mode, nb_livres: nb, premier_demarrage: false, source })
    }

    fn refuser_pendant_analyse(&self) -> Result<(), String> {
        let g = self.verrou();
        if g.scan_en_cours {
            return Err("analyse en cours : attends sa fin avant de changer de bibliothèque".into());
        }
        if g.edition_en_cours {
            return Err("opération d'édition en cours : attends sa fin avant de changer de bibliothèque".into());
        }
        Ok(())
    }

    pub(crate) fn installer(&self, mode: Mode, base: ComicDatabase, store: Option<StoreConfig>) {
        let mut g = self.verrou();
        g.mode = Some(mode);
        g.base = Some(Arc::new(base));
        g.store = store;
        g.comicrack = None;
        g.generation += 1;
        g.contenu += 1;
        g.vue = None;
        drop(g);
        self.negatif.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Base ouverte et état de son contenu (génération, révision des fiches), pour le cache du
    /// contenu des listes.
    pub(crate) fn base_et_contenu(&self) -> Option<(Arc<ComicDatabase>, (u64, u64))> {
        let g = self.verrou();
        Some((g.base.clone()?, (g.generation, g.contenu)))
    }

    pub(crate) fn base(&self) -> Result<Arc<ComicDatabase>, String> {
        self.verrou().base.clone().ok_or_else(|| "aucune bibliothèque ouverte".to_string())
    }

    // ───────────── Livres ─────────────

    /// Une fenêtre de la liste filtrée, triée et regroupée.
    pub fn lister(&self, req: &RequeteLivres) -> Result<PageLivres, String> {
        let (base, vue) = self.vue(req)?;
        let total = vue.indices.len();
        let debut = req.debut.min(total);
        let fin = debut.saturating_add(req.nombre.min(NOMBRE_MAX)).min(total);
        let revs = self.revisions.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let avec_rang = req.liste.is_some();
        let livres = vue.indices[debut..fin]
            .iter()
            .map(|&i| {
                let mut r = resume(&base.books[i]);
                r.url_couverture = avec_revision(r.url_couverture, revs.get(&r.id.to_ascii_lowercase()));
                if avec_rang {
                    r.rang = vue.rang(i).map(|n| n + 1);
                }
                r
            })
            .collect();
        Ok(PageLivres { total, livres })
    }

    /// Piles de la vue décrite par la requête (recherche, liste, tri, groupe ; `debut`, `nombre` et
    /// `serie` sont ignorés) : une pile par série, dans l'ordre d'apparition de la série dans la vue,
    /// comme `stacks()` de la version egui.
    pub fn lister_piles(&self, req: &RequeteLivres) -> Result<PagePiles, String> {
        let (base, vue) = self.vue_complete(req)?;
        let revs = self.revisions.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut position: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut piles: Vec<Pile> = Vec::new();
        for &i in &vue.indices {
            let b = &base.books[i];
            let titre = titre_pile(b);
            let k = *position.entry(titre.clone()).or_insert_with(|| {
                let url = avec_revision(url_couverture(&b.id), revs.get(&b.id.to_ascii_lowercase()));
                piles.push(Pile { serie: titre, nb_livres: 0, nb_lus: 0, ids: Vec::new(), url_couverture: url });
                piles.len() - 1
            });
            let p = &mut piles[k];
            p.nb_livres += 1;
            p.nb_lus += usize::from(b.read_percent() == Some(100));
            p.ids.push(b.id.clone());
        }
        Ok(PagePiles { total_livres: vue.indices.len(), piles })
    }

    /// Vue décrite par la requête, restreinte à la pile ouverte (`serie`) s'il y en a une.
    pub(crate) fn vue(&self, req: &RequeteLivres) -> Result<(Arc<ComicDatabase>, Arc<Vue>), String> {
        let (base, vue) = self.vue_complete(req)?;
        let Some(serie) = req.serie.as_deref() else { return Ok((base, vue)) };
        let indices = vue.indices.iter().copied().filter(|&i| titre_pile(&base.books[i]) == serie).collect();
        Ok((base, Arc::new(Vue { indices, rangs: vue.rangs.clone() })))
    }

    /// Vue complète décrite par la requête (liste, recherche, tri, groupe), en cache.
    fn vue_complete(&self, req: &RequeteLivres) -> Result<(Arc<ComicDatabase>, Arc<Vue>), String> {
        let clef = ClefTri::depuis_texte(&req.tri)?;
        let desc = req.sens_tri == SensTri::Desc;
        let groupe = regroupement(req.groupe);
        let recherche = req.recherche.as_deref().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
        let recherche = recherche.to_lowercase();
        let liste = req.liste.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(str::to_ascii_lowercase);
        // Livres de la liste (dans l'ordre de la liste) et version des listes, avant le verrou de
        // la bibliothèque (le calcul des listes intelligentes le prend lui-même).
        let (membres, version) = match &liste {
            Some(id) => {
                let (m, v) = self.membres_liste(id)?;
                (Some(m), v)
            }
            None => (None, 0),
        };
        let (base, vue, generation) = {
            let g = self.verrou();
            let base = g.base.clone().ok_or("aucune bibliothèque ouverte")?;
            let cle: CleVue = (recherche.clone(), clef, desc, groupe, g.generation, liste.clone(), version);
            let vue = match &g.vue {
                Some((c, v)) if *c == cle => Some(v.clone()),
                _ => None,
            };
            (base, vue, g.generation)
        };
        if let Some(v) = vue {
            return Ok((base, v));
        }
        // Calcul hors du verrou : un tri de 5 000 livres ne bloque pas les couvertures.
        let n = base.books.len();
        let ordre: Vec<usize> = match &membres {
            None | Some(comicrust_core::comic_lists::Members::All) => (0..n).collect(),
            Some(comicrust_core::comic_lists::Members::Ids(ids)) => {
                ids.iter().filter_map(|id| base.index_of(id)).collect()
            }
            Some(comicrust_core::comic_lists::Members::Unknown(_)) => Vec::new(),
        };
        let mut rangs = vec![u32::MAX; n];
        for (r, &i) in ordre.iter().enumerate() {
            if rangs[i] == u32::MAX {
                rangs[i] = r as u32;
            }
        }
        let mut dans = vec![false; n];
        for &i in &ordre {
            dans[i] = true;
        }
        let mut indices: Vec<usize> = tri::filtrer(&base.books, &recherche).into_iter().filter(|&i| dans[i]).collect();
        let rang = |i: usize| rangs.get(i).copied().filter(|r| *r != u32::MAX).map(|r| r as usize);
        tri::trier(&mut indices, &base.books, clef, desc, groupe, &rang);
        let vue = Arc::new(Vue { indices, rangs });
        let mut g = self.verrou();
        // Enregistrée seulement si la base n'a pas été remplacée entre-temps.
        if g.generation == generation && Arc::ptr_eq(g.base.as_ref().unwrap_or(&base), &base) {
            g.vue = Some(((recherche, clef, desc, groupe, generation, liste, version), vue.clone()));
        }
        Ok((base, vue))
    }

    /// Fiche complète d'un livre.
    pub fn detail(&self, id: &str) -> Result<LivreDetail, String> {
        let base = self.base()?;
        let b = base.get(id).ok_or_else(|| format!("livre introuvable : {id}"))?;
        let mut d = detail(b);
        d.url_couverture = self.url_couverture(&d.id);
        // Le disque (souvent un partage réseau) peut ne plus répondre : délai maximal.
        if let Some(f) = d.fichier.clone() {
            d.fichier_present = fichier_existe(PathBuf::from(f), DELAI_FICHIER);
        }
        Ok(d)
    }

    /// Ce qu'il faut pour produire la couverture : (dossier du cache, id, fichier, image).
    fn plan_couverture(&self, id: &str) -> Result<(PathBuf, String, Option<PathBuf>, usize), String> {
        let base = self.base()?;
        let b = base.get(id).ok_or_else(|| format!("livre introuvable : {id}"))?;
        Ok((self.dossier_couvertures(), b.id.clone(), b.file.as_deref().map(PathBuf::from), b.front_cover_image()))
    }

    /// Couverture d'un livre (JPEG), avec délai maximal ; bloquant.
    pub fn couverture(&self, id: &str) -> Result<Vec<u8>, String> {
        let (cache, id, fichier, image) = self.plan_couverture(id)?;
        couvertures::couverture_avec_delai(cache, id, fichier, image, couvertures::DELAI_PRODUCTION)
    }

    /// Sert une couverture au protocole `cr-couv`. Le cache disque et le cache négatif sont
    /// consultés tout de suite, dans le fil appelant (réponse immédiate, sans file d'attente) ;
    /// seuls les échecs de cache passent par le pool (tâches récentes d'abord, périmées
    /// rejetées). `repondre` reçoit les octets JPEG, ou `None` (404).
    pub fn servir(&self, id: &str, repondre: impl FnOnce(Option<Vec<u8>>) + Send + 'static) {
        let Ok((cache, id, fichier, image)) = self.plan_couverture(id) else {
            return repondre(None);
        };
        if self.echec_recent(&id) {
            return repondre(None);
        }
        if let Some(octets) = couvertures::depuis_le_cache(&cache, &id, image) {
            return repondre(Some(octets));
        }
        let negatif = self.negatif.clone();
        self.pool.executer(move |perime| {
            if perime {
                return repondre(None); // abandonnée depuis longtemps ; pas d'échec mémorisé
            }
            match couvertures::couverture_avec_delai(cache, id.clone(), fichier, image, couvertures::DELAI_PRODUCTION) {
                Ok(octets) => repondre(Some(octets)),
                Err(_) => {
                    negatif.lock().unwrap_or_else(|e| e.into_inner()).insert(id, Instant::now());
                    repondre(None)
                }
            }
        });
    }

    fn echec_recent(&self, id: &str) -> bool {
        let mut n = self.negatif.lock().unwrap_or_else(|e| e.into_inner());
        match n.get(id) {
            Some(t) if t.elapsed() < DUREE_ECHEC => true,
            Some(_) => {
                n.remove(id);
                false
            }
            None => false,
        }
    }

    // ───────────── Dossiers surveillés ─────────────

    fn config_store(&self) -> Result<StoreConfig, String> {
        let g = self.verrou();
        match (g.mode, &g.store) {
            (Some(Mode::Autonome), Some(c)) => Ok(c.clone()),
            (Some(Mode::Comicrack), _) => {
                Err("les dossiers sont gérés par ComicRack (mode lié, lecture seule)".to_string())
            }
            _ => Err("aucune bibliothèque autonome ouverte".to_string()),
        }
    }

    /// Dossiers surveillés (vide en mode ComicRack).
    pub fn dossiers(&self) -> Result<Vec<String>, String> {
        let cfg = match self.config_store() {
            Ok(c) => c,
            Err(_) if self.verrou().mode == Some(Mode::Comicrack) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        lire_dossiers(&cfg)
    }

    /// Ajoute un dossier surveillé (doublon ignoré) ; renvoie la liste à jour.
    pub fn ajouter_dossier(&self, chemin: &str) -> Result<Vec<String>, String> {
        let cfg = self.config_store()?;
        let chemin = nettoyer_chemin(chemin);
        if chemin.is_empty() {
            return Err("chemin de dossier vide".into());
        }
        if !Path::new(&chemin).is_dir() {
            return Err(format!("dossier introuvable : {chemin}"));
        }
        let mut store = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
        let mut liste = store.folders().map_err(|e| e.to_string())?;
        let cle = path_key(&chemin);
        if !liste.iter().any(|d| path_key(&d.to_string_lossy()) == cle) {
            liste.push(PathBuf::from(&chemin));
            store.set_folders(&liste).map_err(|e| e.to_string())?;
        }
        lire_dossiers(&cfg)
    }

    /// Retire un dossier surveillé ; renvoie la liste à jour.
    pub fn retirer_dossier(&self, chemin: &str) -> Result<Vec<String>, String> {
        let cfg = self.config_store()?;
        let cle = path_key(&nettoyer_chemin(chemin));
        let mut store = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
        let liste: Vec<PathBuf> = store
            .folders()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|d| path_key(&d.to_string_lossy()) != cle)
            .collect();
        store.set_folders(&liste).map_err(|e| e.to_string())?;
        lire_dossiers(&cfg)
    }

    // ───────────── Analyse ─────────────

    /// Lance l'analyse des dossiers dans un fil et rend la main. `sortie` reçoit la progression
    /// (environ 5 fois par seconde) puis `Termine` ou `Erreur`. La bibliothèque affichée est
    /// rechargée avant `Termine`.
    pub fn lancer_scan(&self, sortie: Arc<dyn Fn(EvenementScan) + Send + Sync>) -> Result<(), String> {
        let (cfg, base, generation) = {
            let mut g = self.verrou();
            let cfg = match (g.mode, &g.store) {
                (Some(Mode::Autonome), Some(c)) => c.clone(),
                (Some(Mode::Comicrack), _) => {
                    return Err("l'analyse des dossiers n'existe qu'en mode autonome \
                                (en mode lié, ComicRack gère sa bibliothèque)"
                        .into())
                }
                _ => return Err("aucune bibliothèque autonome ouverte".into()),
            };
            if g.scan_en_cours {
                return Err("une analyse est déjà en cours".into());
            }
            if g.edition_en_cours {
                return Err("une opération d'édition est en cours : attends sa fin pour analyser les dossiers".into());
            }
            let base = g.base.clone().ok_or("aucune bibliothèque ouverte")?;
            g.scan_en_cours = true;
            (cfg, base, g.generation)
        };
        // À partir d'ici, toute sortie (erreur, panique du fil) remet l'indicateur à faux.
        let garde = GardeScan(self.interne.clone());
        // Dossiers lus avant de rendre la main : « aucun dossier » est une erreur de la commande.
        let dossiers = match Store::open_existing(&cfg).and_then(|mut s| s.folders()) {
            Ok(d) if !d.is_empty() => d,
            Ok(_) => {
                return Err("aucun dossier à analyser : ajoute d'abord un dossier de BD".into());
            }
            Err(e) => return Err(e.to_string()),
        };
        let me = self.clone();
        let lancement = std::thread::Builder::new().name("comicrust-analyse".into()).spawn(move || {
            let resultat = me.analyser(&cfg, &dossiers, &base, generation, &*sortie);
            // Libéré avant l'événement final : le front peut relancer aussitôt.
            drop(garde);
            sortie(match resultat {
                Ok(e) => e,
                Err(message) => EvenementScan::Erreur { message },
            });
        });
        if let Err(e) = lancement {
            return Err(format!("fil d'analyse impossible : {e}"));
        }
        Ok(())
    }

    /// Corps de l'analyse (dans son fil) : parcours + enregistrement, puis rechargement.
    fn analyser(
        &self,
        cfg: &StoreConfig,
        dossiers: &[PathBuf],
        base: &Arc<ComicDatabase>,
        generation: u64,
        sortie: &(dyn Fn(EvenementScan) + Send + Sync),
    ) -> Result<EvenementScan, String> {
        let progression = ScanProgress::default();
        let annule = AtomicBool::new(false);
        let rapport = std::thread::scope(|s| {
            let travail = s.spawn(|| -> Result<_, String> {
                let mut store = Store::open_existing(cfg).map_err(|e| e.to_string())?;
                // Enregistré au fil de l'eau (lots de 200) ; seules les colonnes venant des
                // fichiers sont écrites (voir `Store::scan_into`).
                let (rapport, _) = store
                    .scan_into(dossiers, &base.books, &progression, &annule, new_book_id)
                    .map_err(|e| e.to_string())?;
                Ok(rapport)
            });
            while !travail.is_finished() {
                let (fait, total) = progression.get();
                sortie(EvenementScan::Progression { fait, total });
                std::thread::sleep(PERIODE_PROGRESSION);
            }
            travail.join().unwrap_or_else(|_| Err("l'analyse a planté".to_string()))
        })?;
        let (fait, total) = progression.get();
        sortie(EvenementScan::Progression { fait, total });
        // Rechargement complet : plus sûr que d'appliquer les changements un par un.
        let neuve = Store::open_existing(cfg).and_then(|mut s| s.load(None)).map_err(|e| e.to_string())?;
        {
            let mut g = self.verrou();
            // Remplacée seulement si la bibliothèque affichée est toujours celle qui a été
            // analysée (jamais d'écrasement après un changement de mode).
            if g.generation == generation && g.base.as_ref().is_some_and(|b| Arc::ptr_eq(b, base)) {
                g.base = Some(Arc::new(neuve));
                g.generation += 1;
                g.contenu += 1;
                g.vue = None;
            }
            g.ajoutes = rapport.added.iter().map(|b| b.id.clone()).collect();
        }
        self.negatif.lock().unwrap_or_else(|e| e.into_inner()).clear();
        Ok(EvenementScan::Termine {
            ajoutes: rapport.added.len(),
            mis_a_jour: rapport.updated.len(),
            manquants: rapport.missing.len(),
        })
    }
}

// ───────────────────────────── Pour le lecteur et l'édition ─────────────────────────────

impl Biblio {
    /// URL de la couverture d'un livre, révision comprise.
    pub fn url_couverture(&self, id: &str) -> String {
        let rev = self.revisions.lock().unwrap_or_else(|e| e.into_inner()).get(&id.to_ascii_lowercase()).copied();
        avec_revision(url_couverture(id), rev.as_ref())
    }

    /// Source de la bibliothèque ouverte.
    pub fn source(&self) -> SourceBiblio {
        let g = self.verrou();
        match (g.mode, &g.store, &g.comicrack) {
            (Some(Mode::Autonome), Some(c), _) => SourceBiblio::Autonome(c.clone()),
            (Some(Mode::Comicrack), _, Some(c)) => SourceBiblio::ComicrackMysql(c.clone()),
            (Some(Mode::Comicrack), _, None) if g.base.is_some() => SourceBiblio::ComicrackXml,
            _ => SourceBiblio::Aucune,
        }
    }

    /// Mode lié MySQL : paramètres de connexion (tests).
    #[cfg(test)]
    pub(crate) fn installer_comicrack_mysql(&self, cfg: DbConfig) {
        self.verrou().comicrack = Some(cfg);
    }

    /// Base MySQL de ComicRack où écrire, si l'option « Lecture et écriture » est choisie
    /// (`comicrack_write_config` de la version egui).
    pub fn config_comicrack_ecriture(&self) -> Option<DbConfig> {
        let SourceBiblio::ComicrackMysql(cfg) = self.source() else { return None };
        let ecriture = reglages::lire(&self.data).ok()?.edition.ecriture_comicrack.unwrap_or(false);
        ecriture.then_some(cfg)
    }

    /// Livres ajoutés par la dernière analyse (vidé à la lecture) : automatismes `NewBooks`.
    pub fn prendre_ajoutes(&self) -> Vec<String> {
        std::mem::take(&mut self.verrou().ajoutes)
    }

    /// Une analyse est-elle en cours ?
    pub fn analyse_en_cours(&self) -> bool {
        self.verrou().scan_en_cours
    }

    /// Réserve la bibliothèque pour une opération d'édition : refusé pendant une analyse ou une
    /// autre opération ; l'analyse est refusée tant que la garde existe. Contrôle et réservation
    /// sous le même verrou (pas de course avec `lancer_scan`).
    pub fn prendre_edition(&self) -> Result<GardeEdition, String> {
        let mut g = self.verrou();
        if g.scan_en_cours {
            return Err("analyse des dossiers en cours : attends sa fin".into());
        }
        if g.edition_en_cours {
            return Err("une opération est déjà en cours".into());
        }
        g.edition_en_cours = true;
        Ok(GardeEdition(self.interne.clone()))
    }

    /// Reporte dans la bibliothèque affichée des fiches relues après écriture (la plus récente
    /// l'emporte) ; avec `couvertures`, leurs couvertures sont oubliées (cache disque, cache
    /// négatif, révision de l'URL). Renvoie les identifiants mis à jour.
    pub fn remplacer_fiches(&self, fiches: &[LibraryBook], couvertures: bool) -> Vec<String> {
        let mut faits = Vec::new();
        {
            let mut g = self.verrou();
            if let Some(base) = g.base.as_mut() {
                let base = Arc::make_mut(base);
                for f in fiches {
                    if let Some(b) = base.get_mut(&f.id) {
                        *b = f.clone();
                        faits.push(f.id.clone());
                    }
                }
            }
            if !faits.is_empty() {
                g.contenu += 1;
                g.vue = None;
            }
        }
        for f in fiches.iter().filter(|f| couvertures && faits.contains(&f.id)) {
            self.oublier_couverture(&f.id, f.front_cover_image());
        }
        faits
    }

    /// Couverture d'un livre à refaire (fichier réécrit, converti, déplacé).
    pub fn oublier_couverture(&self, id: &str, image: usize) {
        let cache = self.dossier_couvertures();
        for i in [0, image] {
            let _ = std::fs::remove_file(cache.join(couvertures::nom_cache(id, i)));
        }
        self.negatif.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        *self.revisions.lock().unwrap_or_else(|e| e.into_inner()).entry(id.to_ascii_lowercase()).or_insert(0) += 1;
    }
    /// Fiche d'un livre (copie), si une bibliothèque est ouverte.
    pub fn livre(&self, id: &str) -> Option<LibraryBook> {
        self.verrou().base.as_ref()?.get(id).cloned()
    }

    /// Fiche du livre dont le fichier est `chemin` (casse indifférente, comme la version egui).
    pub fn livre_par_chemin(&self, chemin: &Path) -> Option<LibraryBook> {
        let base = self.verrou().base.clone()?;
        let p = chemin.to_string_lossy();
        base.books
            .iter()
            .find(|b| b.file.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&p) || Path::new(f) == chemin))
            .cloned()
    }

    /// Mode de la bibliothèque ouverte.
    pub fn mode(&self) -> Option<Mode> {
        self.verrou().mode
    }

    /// Base SQLite de ComicRust, en mode autonome seulement.
    pub fn store_autonome(&self) -> Option<StoreConfig> {
        let g = self.verrou();
        match (g.mode, &g.store) {
            (Some(Mode::Autonome), Some(c)) => Some(c.clone()),
            _ => None,
        }
    }

    /// Reporte dans la bibliothèque affichée une lecture écrite dans la base (page courante,
    /// dernière page lue, date d'ouverture), sans la recharger. Pendant une analyse, rien n'est
    /// changé : la base rechargée à la fin de l'analyse contient déjà la lecture.
    pub fn noter_lecture(&self, id: &str, courante: u32, derniere: u32, ouvert: &str) {
        let mut g = self.verrou();
        if g.scan_en_cours {
            return;
        }
        let Some(base) = g.base.as_mut() else { return };
        if let Some(b) = Arc::make_mut(base).get_mut(id) {
            b.current_page = Some(courante);
            b.last_page_read = Some(derniere);
            b.opened = Some(ouvert.to_string());
            // Les tris « lecture » / « avancement » changent : vue recalculée.
            g.contenu += 1;
            g.vue = None;
        }
    }
}

#[path = "biblio_surveillance.rs"]
mod surveillance_biblio;

// ───────────────────────────── Fonctions utilitaires ─────────────────────────────

/// Groupe de l'IPC → regroupement du tri.
pub(crate) fn regroupement(g: Option<Groupe>) -> Regroupement {
    match g.unwrap_or_default() {
        Groupe::Aucun => Regroupement::Aucun,
        Groupe::Serie => Regroupement::Serie,
        Groupe::Editeur => Regroupement::Editeur,
    }
}

/// Base de ComicRust choisie : MySQL si une connexion est enregistrée (`storeMysql`), sinon SQLite.
fn config_autonome(data: &Path, r: &Reglages) -> StoreConfig {
    if let Some(c) = &r.store_mysql {
        return StoreConfig::MySql(c.clone());
    }
    // Chemin explicite (et non `PortableSqlite`, calculé d'après l'exécutable) : même fichier
    // en production (`data\comicrust.db`), mais testable dans un dossier temporaire.
    StoreConfig::Sqlite(data.join(SQLITE_FILE_NAME))
}

/// Message d'échec d'ouverture de la base de ComicRust : sans mot de passe, avec un indice s'il
/// manque.
fn erreur_store(cfg: &StoreConfig, message: String) -> String {
    match cfg {
        StoreConfig::MySql(c) => connexion::avec_indice_mot_de_passe(message, c),
        _ => message,
    }
}

/// D'où viennent les livres, d'après ce que `charger` a ouvert.
fn source_de(store: &Option<StoreConfig>, cr: &Option<DbConfig>) -> SourceBase {
    match (store, cr) {
        (Some(StoreConfig::MySql(_)), _) | (_, Some(_)) => SourceBase::Mysql,
        (Some(_), None) => SourceBase::Sqlite,
        (None, None) => SourceBase::Xml,
    }
}

/// `is_file` avec délai maximal (thread détaché si le disque ne répond pas).
pub(crate) fn fichier_existe(chemin: PathBuf, delai: Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let lance = std::thread::Builder::new().name("comicrust-fichier".into()).spawn(move || {
        let _ = tx.send(chemin.is_file());
    });
    lance.is_ok() && rx.recv_timeout(delai).unwrap_or(false)
}

fn lire_dossiers(cfg: &StoreConfig) -> Result<Vec<String>, String> {
    let mut store = Store::open_existing(cfg).map_err(|e| e.to_string())?;
    Ok(store.folders().map_err(|e| e.to_string())?.iter().map(|d| d.to_string_lossy().into_owned()).collect())
}

/// Chemin tapé ou collé : espaces et guillemets autour retirés.
fn nettoyer_chemin(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

/// URL avec la révision de la couverture (`?v=n`), si elle a changé.
fn avec_revision(url: String, rev: Option<&u32>) -> String {
    match rev {
        Some(n) if *n > 0 => format!("{url}?v={n}"),
        _ => url,
    }
}

/// Bibliothèque chargée : livres, base ComicRust (autonome), connexion MySQL (ComicRack).
type Chargee = (ComicDatabase, Option<StoreConfig>, Option<DbConfig>);

/// Charge la bibliothèque décrite par les réglages (dans le fil de l'appelant).
fn charger(data: &Path, r: &Reglages) -> Result<Chargee, String> {
    match r.mode {
        None => Err("aucun mode choisi".into()),
        Some(Mode::Autonome) => {
            let cfg = config_autonome(data, r);
            let base = Store::open_existing(&cfg)
                .and_then(|mut s| s.load(None))
                .map_err(|e| erreur_store(&cfg, e.to_string()))?;
            Ok((base, Some(cfg), None))
        }
        Some(Mode::Comicrack) => {
            let Some(chemin) = r.chemin_ini.as_deref() else {
                // Saisie à la main (sans ComicRack.ini) : connexion enregistrée dans les réglages.
                let cfg = r.comicrack_mysql.clone().ok_or("ComicRack.ini non indiqué")?;
                let base = ComicDatabase::load(&cfg, None)
                    .map_err(|e| connexion::avec_indice_mot_de_passe(e.to_string(), &cfg))?;
                return Ok((base, None, Some(cfg)));
            };
            let xml = chemin.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("xml"));
            if xml {
                return Ok((charger_comicdb_xml(chemin)?, None, None));
            }
            let cfg = DbConfig::from_comicrack_ini(chemin).map_err(|e| e.to_string())?;
            let base = ComicDatabase::load(&cfg, None).map_err(|e| {
                if cfg.password.is_empty() {
                    format!("{e}\n\nAucun mot de passe dans ce ComicRack.ini : la base ne peut pas être ouverte.")
                } else {
                    e.to_string()
                }
            })?;
            Ok((base, None, Some(cfg)))
        }
    }
}

/// Livres d'un `ComicDb.xml` de ComicRack (bibliothèque sans MySQL).
fn charger_comicdb_xml(chemin: &Path) -> Result<ComicDatabase, String> {
    let texte = std::fs::read_to_string(chemin).map_err(|e| format!("{} : {e}", chemin.display()))?;
    let lib = comicrust_core::Library::from_xml(texte.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{} illisible : {e}", chemin.display()))?;
    Ok(ComicDatabase::from_books(lib.books.book))
}

/// Emplacements habituels de `ComicRack.ini` (à côté de `ComicRack.exe`).
pub(crate) fn chercher_ini() -> Option<PathBuf> {
    let mut dossiers: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(d) = std::env::var_os(var) {
            dossiers.push(PathBuf::from(&d).join("ComicRack Community Edition"));
            dossiers.push(PathBuf::from(&d).join("ComicRack"));
        }
    }
    dossiers.into_iter().map(|d| d.join("ComicRack.ini")).find(|p| p.is_file())
}

fn non_vide(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn url_couverture(id: &str) -> String {
    let mut enc = String::with_capacity(id.len());
    for o in id.bytes() {
        if o.is_ascii_alphanumeric() || matches!(o, b'-' | b'_' | b'.' | b'~') {
            enc.push(o as char);
        } else {
            enc.push_str(&format!("%{o:02X}"));
        }
    }
    // WebView2 (Windows) sert les protocoles personnalisés sous http://<protocole>.localhost ;
    // les autres moteurs sous <protocole>://localhost (voir la doc Tauri, convertFileSrc).
    if cfg!(windows) {
        format!("http://{PROTOCOLE_COUVERTURES}.localhost/{enc}")
    } else {
        format!("{PROTOCOLE_COUVERTURES}://localhost/{enc}")
    }
}

/// Nom du protocole de ressources des couvertures.
pub const PROTOCOLE_COUVERTURES: &str = "cr-couv";

/// Identifiant d'un livre d'après l'URI d'une requête `cr-couv` : le chemin (`/<id>`), à défaut
/// l'hôte (`cr-couv://<id>`), décodé (`%XX`).
pub fn id_depuis_uri(chemin: &str, hote: Option<&str>) -> String {
    let brut = chemin.trim_matches('/');
    let brut = if brut.is_empty() { hote.unwrap_or("") } else { brut };
    let octets = brut.as_bytes();
    let mut sortie = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' && i + 2 < octets.len() {
            let hex = std::str::from_utf8(&octets[i + 1..i + 3]).ok();
            if let Some(h) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                sortie.push(h);
                i += 3;
                continue;
            }
        }
        sortie.push(octets[i]);
        i += 1;
    }
    String::from_utf8_lossy(&sortie).into_owned()
}

/// Titre de la pile d'un livre : sa série affichée, `(sans série)` si elle est vide.
fn titre_pile(b: &LibraryBook) -> String {
    crate::tri::Regroupement::Serie.titre(b)
}

fn non_vide_txt(s: &str) -> Option<String> {
    Some(s.trim().to_string()).filter(|s| !s.is_empty())
}

fn resume(b: &LibraryBook) -> LivreResume {
    LivreResume {
        id: b.id.clone(),
        titre: non_vide(&b.title).unwrap_or_else(|| b.caption()),
        // Valeurs affichées : fiche, sinon déduites du nom de fichier (ShadowSeries / ShadowNumber).
        serie: non_vide_txt(&b.shadow_series()),
        numero: non_vide_txt(&b.shadow_number()).filter(|n| n != "-"),
        editeur: non_vide(&b.publisher),
        annee: b.year.filter(|y| *y > 0),
        pages: b.page_count.unwrap_or(0),
        lu: b.read_percent() == Some(100),
        url_couverture: url_couverture(&b.id),
        rang: None,
    }
}

/// Fait primer la lecture faite dans ComicRust (`avancement.json`) quand elle est plus avancée que
/// celle de la fiche, comme la version egui (`read_percent` : le plus avancé des deux). `lu` suit.
///
/// Relecture (C-06, tauri-v0.7.3) : si le livre est lu (fiche ou ComicRust) et que ComicRust a noté
/// une position avant la dernière page **pendant une séance ouverte sur le livre déjà lu**
/// ([`Avancement::relecture`](crate::avancement::Avancement::relecture)), le livre reste lu et
/// `relecture` donne cette position. Rien n'est écrit (ni fiche ComicRack, ni base).
pub fn fusionner_lecture(d: &mut LivreDetail, local: Option<crate::avancement::Avancement>) {
    let Some(a) = local else { return };
    if let Some(pct) = a.pourcentage() {
        if pct > d.progression.unwrap_or(0) {
            d.progression = Some(pct);
            d.lu = pct == 100;
            d.page_lue = Some(((a.page_max + 1) as u32).min(d.pages.max(a.pages as u32).max(1)));
            d.source_lecture = Some("comicrust".to_string());
        }
    }
    d.relecture = None;
    if d.lu && a.relecture {
        let total = d.pages.max(a.pages as u32);
        let page = a.page as u32 + 1;
        if page < total {
            let pourcentage = ((page * 100 + total / 2) / total).clamp(1, 99);
            d.relecture = Some(Relecture { page, total, pourcentage });
        }
    }
}

/// Fiche détaillée d'un livre, sans accès disque (tests).
#[cfg(test)]
pub fn detail_pour_test(b: &LibraryBook) -> LivreDetail {
    detail(b)
}

fn detail(b: &LibraryBook) -> LivreDetail {
    let fichier = non_vide(&b.file);
    LivreDetail {
        id: b.id.clone(),
        titre: non_vide(&b.title).unwrap_or_else(|| b.caption()),
        serie: non_vide(&b.series),
        numero: non_vide(&b.number),
        volume: b.volume.filter(|v| *v >= 0),
        annee: b.year.filter(|y| *y > 0),
        mois: b.month.filter(|m| *m > 0),
        jour: b.day.filter(|j| *j > 0),
        editeur: non_vide(&b.publisher),
        collection: non_vide(&b.imprint),
        pages: b.page_count.unwrap_or(0),
        lu: b.read_percent() == Some(100),
        progression: b.read_percent(),
        page_courante: b.last_page_read,
        page_lue: b.last_page_read.filter(|p| *p > 0).map(|p| p + 1),
        source_lecture: b.read_percent().filter(|p| *p > 0).map(|_| "comicrack".to_string()),
        relecture: None,
        scenariste: non_vide(&b.writer),
        dessinateur: non_vide(&b.penciller),
        encreur: non_vide(&b.inker),
        coloriste: non_vide(&b.colorist),
        lettreur: non_vide(&b.letterer),
        artiste_couverture: non_vide(&b.cover_artist),
        redacteur: non_vide(&b.editor),
        traducteur: non_vide(&b.translator),
        genre: non_vide(&b.genre),
        arc_narratif: non_vide(&b.story_arc),
        personnages: non_vide(&b.characters),
        equipes: non_vide(&b.teams),
        lieux: non_vide(&b.locations),
        etiquettes: non_vide(&b.tags),
        resume: non_vide(&b.summary),
        notes: non_vide(&b.notes),
        critique: non_vide(&b.review),
        format: non_vide(&b.format),
        langue: non_vide(&b.language_iso),
        classement_age: non_vide(&b.age_rating),
        noir_et_blanc: non_vide(&b.black_and_white),
        manga: non_vide(&b.manga),
        note: b.rating.filter(|r| *r > 0.0),
        note_communaute: b.community_rating.filter(|r| *r > 0.0),
        isbn: non_vide(&b.isbn),
        web: non_vide(&b.web),
        taille_fichier: b.file_size,
        date_ajout: non_vide(&b.added).filter(|d| !d.starts_with("0001-01-01")),
        derniere_lecture: non_vide(&b.opened).filter(|d| !d.starts_with("0001-01-01")),
        fichier_present: false, // calculé par `Biblio::detail` (avec délai)
        fichier,
        manquant: b.missing.unwrap_or(false),
        url_couverture: url_couverture(&b.id),
    }
}
