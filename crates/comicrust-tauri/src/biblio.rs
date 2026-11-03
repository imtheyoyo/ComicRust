//! Bibliothèque : ouverture (autonome ou liée à ComicRack, en lecture seule), liste filtrée /
//! triée / regroupée, détail d'un livre, dossiers surveillés, analyse (scan), couvertures.
//!
//! Aucune dépendance à Tauri : tout est testé ici (SQLite autonome, scan d'un dossier de CBZ).
//! Les commandes de `app.rs` ne font qu'appeler ces fonctions. Logique reprise de la version
//! egui (`library/mod.rs`, `library/scan.rs`, assistant `setup_window.rs`), dupliquée (voir le
//! plan : on ne touche pas à `crates/comicrust`).

use crate::couvertures;
use crate::reglages::{self, Mode, PreferencesVue, Reglages};
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLivres {
    pub total: usize,
    pub livres: Vec<LivreResume>,
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

/// Clé du cache de vue : recherche normalisée, tri, sens, groupe, génération de la base.
type CleVue = (String, ClefTri, bool, Regroupement, u64);

#[derive(Default)]
struct Interne {
    mode: Option<Mode>,
    base: Option<Arc<ComicDatabase>>,
    /// Mode autonome : la base SQLite.
    store: Option<StoreConfig>,
    /// Augmente à chaque remplacement de la base (invalide la vue).
    generation: u64,
    vue: Option<(CleVue, Arc<Vec<usize>>)>,
    scan_en_cours: bool,
}

/// Bibliothèque ouverte, partageable entre les commandes et les fils (clonage bon marché).
#[derive(Clone)]
pub struct Biblio {
    data: PathBuf,
    interne: Arc<Mutex<Interne>>,
    pool: Arc<couvertures::Pool>,
    /// Cache négatif : identifiants dont la couverture a échoué, avec l'heure de l'échec.
    negatif: Arc<Mutex<HashMap<String, Instant>>>,
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

impl Biblio {
    /// Bibliothèque non ouverte pour un dossier `data` (voir [`Biblio::ouvrir`]).
    pub fn nouvelle(data: PathBuf) -> Self {
        Biblio {
            data,
            interne: Arc::default(),
            pool: Arc::new(couvertures::Pool::nouveau(couvertures::FILS)),
            negatif: Arc::default(),
        }
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
            return Ok(EtatBiblio { mode: Mode::Autonome, nb_livres: 0, premier_demarrage: true });
        };
        let (base, store) = charger(&self.data, &r)?;
        let nb = base.len();
        self.installer(mode, base, store);
        Ok(EtatBiblio { mode, nb_livres: nb, premier_demarrage: false })
    }

    /// Enregistre le choix du premier démarrage et ouvre la bibliothèque. Rien n'est écrit dans
    /// ComicRack (lecture seule) ni copié depuis lui.
    pub fn configurer(&self, choix: &ChoixPremierDemarrage) -> Result<EtatBiblio, String> {
        self.refuser_pendant_analyse()?;
        let r = match choix.mode {
            Mode::Autonome => {
                // Crée la base (schéma) : seul endroit où un fichier absent est créé.
                let cfg = config_autonome(&self.data);
                Store::open(&cfg).map_err(|e| e.to_string())?;
                Reglages { mode: Some(Mode::Autonome), chemin_ini: None, vue: PreferencesVue::default() }
            }
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
                Reglages { mode: Some(Mode::Comicrack), chemin_ini: Some(chemin), vue: PreferencesVue::default() }
            }
        };
        // Charge d'abord : un choix qui échoue (MySQL injoignable…) n'est pas enregistré.
        let (base, store) = charger(&self.data, &r)?;
        if r.mode == Some(Mode::Comicrack) && base.is_empty() {
            return Err("cette bibliothèque ComicRack ne contient aucun livre \
                        (ComicRack utilise sans doute une base MySQL : indique son ComicRack.ini)"
                .into());
        }
        // Seuls le mode et le chemin changent : les préférences d'affichage sont conservées.
        reglages::modifier(&self.data, |actuels| {
            actuels.mode = r.mode;
            actuels.chemin_ini = r.chemin_ini.clone();
        })?;
        let nb = base.len();
        self.installer(choix.mode, base, store);
        Ok(EtatBiblio { mode: choix.mode, nb_livres: nb, premier_demarrage: false })
    }

    fn refuser_pendant_analyse(&self) -> Result<(), String> {
        if self.verrou().scan_en_cours {
            return Err("analyse en cours : attends sa fin avant de changer de bibliothèque".into());
        }
        Ok(())
    }

    pub(crate) fn installer(&self, mode: Mode, base: ComicDatabase, store: Option<StoreConfig>) {
        let mut g = self.verrou();
        g.mode = Some(mode);
        g.base = Some(Arc::new(base));
        g.store = store;
        g.generation += 1;
        g.vue = None;
        drop(g);
        self.negatif.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    fn base(&self) -> Result<Arc<ComicDatabase>, String> {
        self.verrou().base.clone().ok_or_else(|| "aucune bibliothèque ouverte".to_string())
    }

    // ───────────── Livres ─────────────

    /// Une fenêtre de la liste filtrée, triée et regroupée.
    pub fn lister(&self, req: &RequeteLivres) -> Result<PageLivres, String> {
        let clef = ClefTri::depuis_texte(&req.tri)?;
        let desc = req.sens_tri == SensTri::Desc;
        let groupe = match req.groupe.unwrap_or_default() {
            Groupe::Aucun => Regroupement::Aucun,
            Groupe::Serie => Regroupement::Serie,
            Groupe::Editeur => Regroupement::Editeur,
        };
        let recherche = req.recherche.as_deref().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
        let recherche = recherche.to_lowercase();
        let (base, vue) = {
            let g = self.verrou();
            let base = g.base.clone().ok_or("aucune bibliothèque ouverte")?;
            let cle: CleVue = (recherche.clone(), clef, desc, groupe, g.generation);
            let vue = match &g.vue {
                Some((c, v)) if *c == cle => Some(v.clone()),
                _ => None,
            };
            (base, vue)
        };
        let vue = match vue {
            Some(v) => v,
            None => {
                // Calcul hors du verrou : un tri de 5 000 livres ne bloque pas les couvertures.
                let mut v = tri::filtrer(&base.books, &recherche);
                tri::trier(&mut v, &base.books, clef, desc, groupe);
                let v = Arc::new(v);
                let mut g = self.verrou();
                // Enregistré seulement si la base n'a pas été remplacée entre-temps.
                if Arc::ptr_eq(g.base.as_ref().unwrap_or(&base), &base) {
                    let generation = g.generation;
                    g.vue = Some(((recherche, clef, desc, groupe, generation), v.clone()));
                }
                v
            }
        };
        let total = vue.len();
        let debut = req.debut.min(total);
        let fin = debut.saturating_add(req.nombre.min(NOMBRE_MAX)).min(total);
        let livres = vue[debut..fin].iter().map(|&i| resume(&base.books[i])).collect();
        Ok(PageLivres { total, livres })
    }

    /// Fiche complète d'un livre.
    pub fn detail(&self, id: &str) -> Result<LivreDetail, String> {
        let base = self.base()?;
        let b = base.get(id).ok_or_else(|| format!("livre introuvable : {id}"))?;
        let mut d = detail(b);
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
                g.vue = None;
            }
        }
        self.negatif.lock().unwrap_or_else(|e| e.into_inner()).clear();
        Ok(EvenementScan::Termine {
            ajoutes: rapport.added.len(),
            mis_a_jour: rapport.updated.len(),
            manquants: rapport.missing.len(),
        })
    }
}

// ───────────────────────────── Pour le lecteur ─────────────────────────────

impl Biblio {
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
            g.vue = None;
        }
    }
}

// ───────────────────────────── Fonctions utilitaires ─────────────────────────────

fn config_autonome(data: &Path) -> StoreConfig {
    // Chemin explicite (et non `PortableSqlite`, calculé d'après l'exécutable) : même fichier
    // en production (`data\comicrust.db`), mais testable dans un dossier temporaire.
    StoreConfig::Sqlite(data.join(SQLITE_FILE_NAME))
}

/// `is_file` avec délai maximal (thread détaché si le disque ne répond pas).
fn fichier_existe(chemin: PathBuf, delai: Duration) -> bool {
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

/// Charge la bibliothèque décrite par les réglages (dans le fil de l'appelant).
fn charger(data: &Path, r: &Reglages) -> Result<(ComicDatabase, Option<StoreConfig>), String> {
    match r.mode {
        None => Err("aucun mode choisi".into()),
        Some(Mode::Autonome) => {
            let cfg = config_autonome(data);
            let base = Store::open_existing(&cfg).and_then(|mut s| s.load(None)).map_err(|e| e.to_string())?;
            Ok((base, Some(cfg)))
        }
        Some(Mode::Comicrack) => {
            let chemin = r.chemin_ini.as_deref().ok_or("ComicRack.ini non indiqué")?;
            let xml = chemin.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("xml"));
            let base = if xml {
                charger_comicdb_xml(chemin)?
            } else {
                let cfg = DbConfig::from_comicrack_ini(chemin).map_err(|e| e.to_string())?;
                ComicDatabase::load(&cfg, None).map_err(|e| {
                    if cfg.password.is_empty() {
                        format!("{e}\n\nAucun mot de passe dans ce ComicRack.ini : la base ne peut pas être ouverte.")
                    } else {
                        e.to_string()
                    }
                })?
            };
            Ok((base, None))
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
fn chercher_ini() -> Option<PathBuf> {
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

fn resume(b: &LibraryBook) -> LivreResume {
    LivreResume {
        id: b.id.clone(),
        titre: non_vide(&b.title).unwrap_or_else(|| b.caption()),
        serie: non_vide(&b.series),
        numero: non_vide(&b.number),
        editeur: non_vide(&b.publisher),
        annee: b.year.filter(|y| *y > 0),
        pages: b.page_count.unwrap_or(0),
        lu: b.read_percent() == Some(100),
        url_couverture: url_couverture(&b.id),
    }
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
