//! Scripts Python (étape 7, `tauri-v0.9.0`). Contrat : `docs/tauri/ipc-scripts.md`.
//!
//! Le moteur est celui du cœur (`comicrust_core::scripting` : découverte, exécution dans un
//! processus Python séparé, protocole, formulaires vérifiés). Ce module reprend, **dupliqué**,
//! le comportement de la version egui (`crates/comicrust/src/scripts/` et `ui/scripts_ui.rs`) :
//! - découverte des scripts du dossier `scripts\` à côté de l'exécutable, scripts activés /
//!   désactivés, réglages de chaque script dans `data\comicrust-tauri.json` (section `scripts`,
//!   valeurs secrètes chiffrées, jamais renvoyées en clair ni journalisées) ; module
//!   `comicrust.py` écrit dans `data\python\` ;
//! - exécution lancée du menu : **un script interactif à la fois**, questions / formulaires /
//!   listes de choix posées au front par événements (le fil du script attend, jamais l'IPC),
//!   arrêt poli puis forcé (délai de grâce du cœur, 3 s), puis aperçu et application
//!   ([`apercu`]) ;
//! - automatismes (`Startup`, `BookOpened`, `BookClosed`, `NewBooks`) : désactivés par défaut,
//!   un à la fois par hook, aucune question, 300 s au plus, modifications appliquées au fil de
//!   l'eau, journal `scripts:auto` ;
//! - hook `Editor` (fenêtre Infos) : les champs proposés reviennent dans `script:fin.champs` ;
//! - fermeture de l'application : tout est annulé, attente de 8 s au plus, sans jamais couper
//!   une application en cours au milieu d'un livre.
//!
//! Aucune dépendance à Tauri : tout est testé ici (`tests_scripts.rs`) ; `app.rs` ne fait que
//! relier les commandes et les événements.

pub mod apercu;
pub mod formulaires;
pub mod hote;

use crate::biblio::Biblio;
use crate::edition::Edition;
use crate::lecteur::Lecteur;
use crate::reglages::{self, ReglagesScripts};
use apercu::{Apercu, ApercuCalcule, Applicateur, Bilan};
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{
    self, discover_with_warnings, AppliedRenames, DiscoveryWarning, Hook, RunContext, RunHandle, RunOptions, RunResult,
    RunState, ScriptEntry, ScriptRunner, ScriptSettings,
};
use hote::{Attentes, EnDirect, HoteTauri, Ouvreur, ReglagesScriptsHote};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Fermeture de l'application : délai maximum d'attente des scripts (comme la version egui).
pub const ATTENTE_FERMETURE: Duration = Duration::from_secs(8);
/// Valeur montrée à la place d'une valeur secrète définie (jamais la valeur elle-même).
pub const SECRET_MASQUE: &str = "••••";
/// Exécutions terminées gardées (aperçu, application).
const EXECUTIONS_GARDEES: usize = 8;
/// Taille maximale d'une image de menu (`#@Image`) renvoyée en URL `data:`.
const IMAGE_MENU_MAX: u64 = 512 * 1024;

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────────── Événements ─────────────────────────────

/// Événements transmis au front par `app.rs` (noms et charges du contrat).
#[derive(Debug, Clone, PartialEq)]
pub enum EvenementScript {
    Journal {
        run_id: String,
        genre: &'static str,
        texte: String,
    },
    Progression {
        run_id: String,
        fraction: Option<f32>,
        texte: Option<String>,
        detail: Option<String>,
        image: Option<String>,
    },
    Question {
        run_id: String,
        req_id: u64,
        genre: &'static str,
        titre: String,
        texte: String,
        question: Option<Value>,
    },
    Formulaire {
        run_id: String,
        req_id: u64,
        spec: Value,
    },
    Choix {
        run_id: String,
        req_id: u64,
        spec: Value,
    },
    Vignette {
        run_id: String,
        req_id: u64,
        rang: usize,
        image: String,
    },
    Fin {
        run_id: String,
        etat: &'static str,
        erreur: Option<String>,
        nb_modifs: usize,
        champs: Option<Map<String, Value>>,
    },
    Applique {
        run_id: String,
        bilan: Bilan,
    },
    Auto {
        script: String,
        hook: String,
        ligne: String,
    },
    /// Fiches modifiées par une application (même événement que l'édition).
    BibliothequeModifiee {
        ids: Vec<String>,
    },
}

impl EvenementScript {
    /// Nom de l'événement Tauri et sa charge JSON (camelCase).
    pub fn pour_le_front(&self) -> (&'static str, Value) {
        let sans_nul = |mut v: Value| {
            if let Value::Object(o) = &mut v {
                o.retain(|_, x| !x.is_null());
            }
            v
        };
        match self {
            EvenementScript::Journal { run_id, genre, texte } => {
                ("script:journal", json!({"runId": run_id, "genre": genre, "texte": texte}))
            }
            EvenementScript::Progression { run_id, fraction, texte, detail, image } => (
                "script:progression",
                sans_nul(
                    json!({"runId": run_id, "fraction": fraction, "texte": texte, "detail": detail, "image": image}),
                ),
            ),
            EvenementScript::Question { run_id, req_id, genre, titre, texte, question } => (
                "script:question",
                sans_nul(json!({"runId": run_id, "reqId": req_id, "genre": genre, "titre": titre, "texte": texte,
                                "question": question})),
            ),
            EvenementScript::Formulaire { run_id, req_id, spec } => {
                ("script:formulaire", json!({"runId": run_id, "reqId": req_id, "spec": spec}))
            }
            EvenementScript::Choix { run_id, req_id, spec } => {
                ("script:choix", json!({"runId": run_id, "reqId": req_id, "spec": spec}))
            }
            EvenementScript::Vignette { run_id, req_id, rang, image } => {
                ("script:vignette", json!({"runId": run_id, "reqId": req_id, "rang": rang, "image": image}))
            }
            EvenementScript::Fin { run_id, etat, erreur, nb_modifs, champs } => (
                "script:fin",
                sans_nul(json!({"runId": run_id, "etat": etat, "erreur": erreur, "nbModifs": nb_modifs,
                                "champs": champs})),
            ),
            EvenementScript::Applique { run_id, bilan } => {
                ("script:applique", json!({"runId": run_id, "bilan": bilan}))
            }
            EvenementScript::Auto { script, hook, ligne } => {
                ("scripts:auto", json!({"script": script, "hook": hook, "ligne": ligne}))
            }
            EvenementScript::BibliothequeModifiee { ids } => ("bibliotheque-modifiee", json!({ "ids": ids })),
        }
    }
}

/// Sortie des événements (appelée depuis n'importe quel fil).
pub type Sortie = Arc<dyn Fn(EvenementScript) + Send + Sync>;

// ───────────────────────────── Contrat IPC ─────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct InfoPython {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chemin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erreur: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InfoScript {
    /// Identifiant du point d'entrée : `#@Key`, sinon `chemin/relatif.py:fonction`.
    pub id: String,
    pub nom: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auteur: Option<String>,
    pub hooks: Vec<&'static str>,
    /// Fichier, chemin relatif au dossier `scripts\` (séparateurs `/`).
    pub fichier: String,
    pub actif: bool,
    /// Image du menu (`#@Image`) en URL `data:`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Identifiant de la fonction `ConfigScript` du même fichier (bouton « Réglages »).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reglages: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatScripts {
    pub python: InfoPython,
    pub scripts: Vec<InfoScript>,
    pub avertissements: Vec<String>,
    pub auto: bool,
    pub ecriture_autorisee: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif_lecture_seule: Option<String>,
    /// Dossier des scripts (à ouvrir dans l'Explorateur).
    pub dossier: String,
    /// Exécution interactive en cours (lancée ou en application), s'il y en a une.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_en_cours: Option<String>,
}

// ───────────────────────────── File des automatismes ─────────────────────────────

/// Automatisme à lancer.
#[derive(Debug, Clone, PartialEq)]
pub struct TacheAuto {
    pub entree: ScriptEntry,
    pub hook: Hook,
    pub ids: Vec<String>,
}

/// File d'attente des automatismes (`AutoQueue` de la version egui) : **un seul à la fois par
/// hook**, les suivants attendent leur tour ; des hooks différents peuvent tourner ensemble.
#[derive(Debug, Default)]
pub struct FileAuto {
    attente: BTreeMap<Hook, VecDeque<TacheAuto>>,
    en_cours: BTreeSet<Hook>,
}

impl FileAuto {
    /// Au-delà, les nouveaux déclenchements d'un hook sont ignorés.
    pub const MAX_ATTENTE: usize = 50;

    pub fn ajouter(&mut self, t: TacheAuto) -> bool {
        let q = self.attente.entry(t.hook).or_default();
        if q.len() >= Self::MAX_ATTENTE {
            return false;
        }
        q.push_back(t);
        true
    }

    /// Tâches à lancer maintenant : la première de chaque hook libre, marquée « en cours ».
    pub fn pretes(&mut self) -> Vec<TacheAuto> {
        let mut out = Vec::new();
        for (hook, q) in &mut self.attente {
            if self.en_cours.contains(hook) {
                continue;
            }
            if let Some(t) = q.pop_front() {
                self.en_cours.insert(*hook);
                out.push(t);
            }
        }
        self.attente.retain(|_, q| !q.is_empty());
        out
    }

    pub fn terminee(&mut self, hook: Hook) {
        self.en_cours.remove(&hook);
    }

    pub fn vider(&mut self) {
        self.attente.clear();
    }

    pub fn en_attente(&self) -> usize {
        self.attente.values().map(VecDeque::len).sum()
    }
}

// ───────────────────────────── Réglages ─────────────────────────────

/// Réglages des scripts en mémoire, écrits dans `data\comicrust-tauri.json` à chaque changement.
struct Reglages {
    data: PathBuf,
    r: Mutex<ReglagesScripts>,
}

impl Reglages {
    fn enregistrer(&self, copie: ReglagesScripts) -> Result<(), String> {
        reglages::modifier(&self.data, |r| r.scripts = copie).map(drop)
    }

    fn modifier(&self, f: impl FnOnce(&mut ReglagesScripts)) -> Result<(), String> {
        let copie = {
            let mut g = verrou(&self.r);
            f(&mut g);
            g.clone()
        };
        self.enregistrer(copie)
    }
}

impl ReglagesScriptsHote for Reglages {
    fn lire(&self, cle: &str) -> ScriptSettings {
        verrou(&self.r).valeurs.get(cle).map(ScriptSettings::from_stored).unwrap_or_default()
    }

    fn ecrire(&self, cle: &str, s: &ScriptSettings) -> Result<(), String> {
        let stored = s.to_stored();
        self.modifier(|r| {
            r.valeurs.insert(cle.to_string(), stored);
        })
    }
}

/// Actif ? Le choix de l'utilisateur prime ; sans choix, l'en-tête `#@Enabled`.
fn est_actif(r: &ReglagesScripts, e: &ScriptEntry) -> bool {
    let id = e.id();
    if r.desactives.contains(&id) {
        false
    } else if r.actives.contains(&id) {
        true
    } else {
        e.enabled_by_default
    }
}

// ───────────────────────────── Exécutions ─────────────────────────────

/// Une exécution (menu, Editor ou automatisme).
struct Execution {
    run_id: String,
    entree: ScriptEntry,
    hook: Hook,
    stop: Arc<AtomicBool>,
    attentes: Arc<Attentes>,
    handle: Mutex<Option<RunHandle>>,
    resultat: Mutex<Option<RunResult>>,
    apercu: Mutex<Option<ApercuCalcule>>,
    /// Arrêt de l'application (entre deux livres).
    stop_application: Arc<AtomicBool>,
    en_application: AtomicBool,
    /// Hook Editor : fiche affichée dans la fenêtre Infos (champs non enregistrés compris).
    editeur: Option<LibraryBook>,
}

impl Execution {
    fn terminee(&self) -> bool {
        verrou(&self.resultat).is_some()
    }

    fn occupee(&self) -> bool {
        !self.terminee() || self.en_application.load(Ordering::SeqCst)
    }

    fn arreter(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = verrou(&self.handle).as_ref() {
            h.cancel();
        }
        self.attentes.vider();
        self.stop_application.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct Etat {
    entrees: Vec<ScriptEntry>,
    avertissements: Vec<DiscoveryWarning>,
    erreur_dossier: Option<String>,
    execs: Vec<Arc<Execution>>,
    interactif: Option<Arc<Execution>>,
    file: FileAuto,
    autos: Vec<Arc<Execution>>,
    startup_fait: bool,
}

/// Paramètres du service (chemins, interpréteur forcé pour les tests).
pub struct ConfigScripts {
    pub data: PathBuf,
    /// Dossier des scripts (`scripts\` à côté de l'exécutable).
    pub dossier: PathBuf,
    /// Dossier de l'exécutable (Python embarqué `python\`).
    pub dossier_exe: PathBuf,
    /// Interpréteur imposé (`None` : `find_python`).
    pub python: Option<PathBuf>,
}

impl ConfigScripts {
    /// Emplacements de l'application portable.
    pub fn portable(data: PathBuf) -> ConfigScripts {
        let exe = crate::portable::exe_dir();
        ConfigScripts { data, dossier: exe.join("scripts"), dossier_exe: exe, python: None }
    }
}

struct Interne {
    config: ConfigScripts,
    applicateur: Applicateur,
    sortie: Sortie,
    ouvreur: Ouvreur,
    presse_papiers: Ouvreur,
    reglages: Arc<Reglages>,
    etat: Mutex<Etat>,
    fermeture: AtomicBool,
    applications: AtomicUsize,
    compteur: AtomicU64,
}

/// Service des scripts, partageable entre les commandes et les fils (clonage bon marché).
#[derive(Clone)]
pub struct Scripts(Arc<Interne>);

impl Scripts {
    pub fn nouveau(config: ConfigScripts, biblio: Biblio, edition: Edition, lecteur: Lecteur, sortie: Sortie) -> Self {
        // Lecture tolérante : un fichier de réglages illisible donne des réglages vides (il
        // n'est réécrit qu'à un changement, et alors l'erreur de lecture est signalée).
        let r = reglages::lire(&config.data).map(|r| r.scripts).unwrap_or_default();
        let reglages = Arc::new(Reglages { data: config.data.clone(), r: Mutex::new(r) });
        let s = Scripts(Arc::new(Interne {
            config,
            applicateur: Applicateur { biblio, edition, lecteur },
            sortie,
            ouvreur: Arc::new(hote::ouvrir_url),
            presse_papiers: Arc::new(hote::copier_texte),
            reglages,
            etat: Mutex::default(),
            fermeture: AtomicBool::new(false),
            applications: AtomicUsize::new(0),
            compteur: AtomicU64::new(0),
        }));
        s.recharger_dossier();
        s
    }

    /// Remplace l'ouverture d'adresses et le presse-papiers (tests).
    pub fn avec_systeme(self, ouvreur: Ouvreur, presse_papiers: Ouvreur) -> Self {
        let Ok(mut i) = Arc::try_unwrap(self.0) else { panic!("Scripts déjà partagé") };
        i.ouvreur = ouvreur;
        i.presse_papiers = presse_papiers;
        Scripts(Arc::new(i))
    }

    fn etat(&self) -> MutexGuard<'_, Etat> {
        verrou(&self.0.etat)
    }

    fn emettre(&self, e: EvenementScript) {
        (self.0.sortie)(e);
    }

    fn systeme(&self, run_id: &str, texte: String) {
        self.emettre(EvenementScript::Journal { run_id: run_id.to_string(), genre: "systeme", texte });
    }

    fn journal_auto(&self, script: &str, hook: Hook, ligne: String) {
        let stamp = chrono::Local::now().format("%H:%M:%S");
        self.emettre(EvenementScript::Auto {
            script: script.to_string(),
            hook: hook.name().to_string(),
            ligne: format!("{stamp} {ligne}"),
        });
    }

    fn python(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.0.config.python {
            return Ok(p.clone());
        }
        scripting::find_python(Some(&self.0.config.dossier_exe)).map_err(|e| match e {
            scripting::ScriptError::PythonNotFound(p) => format!(
                "Python embarqué introuvable : dossier python\\ à côté de ComicRust-Tauri.exe (cherché dans {p}). \
                 Réinstalle ComicRust (le zip contient ce dossier) ou indique un interpréteur dans la variable \
                 d'environnement COMICRUST_PYTHON."
            ),
            autre => autre.to_string(),
        })
    }

    fn options(&self, hook: Hook, python: PathBuf) -> RunOptions {
        RunOptions {
            python: Some(python),
            scripts_dir: Some(self.0.config.dossier.clone()),
            // `data\python\` : rien n'est écrit hors du dossier portable.
            module_dir: Some(self.0.config.data.join("python")),
            ..RunOptions::for_hook(hook)
        }
    }

    fn dossier_renommages(&self) -> PathBuf {
        self.0.config.data.join("renommages")
    }

    fn motif_lecture_seule(&self) -> Option<String> {
        apercu::motif_lecture_seule(&self.0.applicateur.edition, &self.0.applicateur.biblio)
    }

    fn nouvel_id(&self) -> String {
        format!("s{}", self.0.compteur.fetch_add(1, Ordering::SeqCst) + 1)
    }

    // ───────────── Découverte et état ─────────────

    fn recharger_dossier(&self) {
        let dossier = &self.0.config.dossier;
        let erreur = std::fs::create_dir_all(dossier)
            .err()
            .map(|e| format!("Dossier des scripts impossible à créer ({}) : {e}", dossier.display()));
        let (entrees, avertissements) = discover_with_warnings(dossier);
        let mut g = self.etat();
        g.entrees = entrees;
        g.avertissements = avertissements;
        g.erreur_dossier = erreur;
    }

    /// État des scripts (`scripts_lister`).
    pub fn lister(&self) -> EtatScripts {
        let python = match self.python() {
            Ok(p) => InfoPython { chemin: Some(p.display().to_string()), erreur: None },
            Err(e) => InfoPython { chemin: None, erreur: Some(e) },
        };
        let r = verrou(&self.0.reglages.r).clone();
        let motif = self.motif_lecture_seule();
        let g = self.etat();
        let dossier = &self.0.config.dossier;
        let scripts = g
            .entrees
            .iter()
            .map(|e| InfoScript {
                id: e.id(),
                nom: e.name.clone(),
                description: e.description.clone(),
                auteur: None,
                hooks: e.hooks.iter().map(|h| h.name()).collect(),
                fichier: e.rel_path.clone(),
                actif: est_actif(&r, e),
                image: e.image.as_deref().and_then(image_menu),
                reglages: g
                    .entrees
                    .iter()
                    .find(|c| c.has_hook(Hook::ConfigScript) && c.settings_key() == e.settings_key())
                    .map(ScriptEntry::id),
            })
            .collect();
        let mut avertissements: Vec<String> = g.erreur_dossier.iter().cloned().collect();
        avertissements.extend(g.avertissements.iter().map(|w| {
            let f = w.file.strip_prefix(dossier).unwrap_or(&w.file).display().to_string();
            if w.line > 0 {
                format!("{f}:{} : {}", w.line, w.message)
            } else {
                format!("{f} : {}", w.message)
            }
        }));
        EtatScripts {
            python,
            scripts,
            avertissements,
            auto: r.auto.unwrap_or(false),
            ecriture_autorisee: motif.is_none(),
            motif_lecture_seule: motif,
            dossier: dossier.display().to_string(),
            execution_en_cours: g.interactif.as_ref().filter(|x| x.occupee()).map(|x| x.run_id.clone()),
        }
    }

    /// Relit le dossier des scripts (`scripts_recharger`).
    pub fn recharger(&self) -> EtatScripts {
        self.recharger_dossier();
        self.lister()
    }

    /// Active / désactive un point d'entrée (`scripts_activer`).
    pub fn activer(&self, id: &str, actif: bool) -> Result<EtatScripts, String> {
        let entree = self.entree(id)?;
        let id = entree.id();
        self.0.reglages.modifier(|r| {
            if actif {
                r.desactives.remove(&id);
                if !entree.enabled_by_default {
                    r.actives.insert(id.clone());
                }
            } else {
                r.actives.remove(&id);
                r.desactives.insert(id.clone());
            }
        })?;
        Ok(self.lister())
    }

    /// Automatismes activés ou non (`scripts_auto`) ; coupés : ceux en cours sont arrêtés.
    pub fn regler_auto(&self, actif: bool) -> Result<EtatScripts, String> {
        self.0.reglages.modifier(|r| r.auto = Some(actif))?;
        if !actif {
            let mut g = self.etat();
            g.file.vider();
            for a in &g.autos {
                a.arreter();
            }
        }
        Ok(self.lister())
    }

    fn entree(&self, id: &str) -> Result<ScriptEntry, String> {
        self.etat()
            .entrees
            .iter()
            .find(|e| e.id() == id)
            .cloned()
            .ok_or_else(|| format!("script inconnu : {id} (recharge la liste des scripts)"))
    }

    fn execution(&self, run_id: &str) -> Result<Arc<Execution>, String> {
        let g = self.etat();
        g.execs
            .iter()
            .chain(g.autos.iter())
            .find(|x| x.run_id == run_id)
            .cloned()
            .ok_or_else(|| format!("exécution inconnue : {run_id}"))
    }

    // ───────────── Réglages des scripts ─────────────

    /// Réglages enregistrés du script `id` ; valeur secrète définie : [`SECRET_MASQUE`].
    pub fn reglages_lire(&self, id: &str) -> Result<Map<String, Value>, String> {
        let entree = self.entree(id)?;
        Ok(masquer(&self.0.reglages.lire(&entree.settings_key())))
    }

    /// Fusionne `valeurs` dans les réglages du script `id` (une valeur secrète laissée à
    /// [`SECRET_MASQUE`] reste inchangée) ; renvoie les réglages masqués.
    pub fn reglages_ecrire(&self, id: &str, valeurs: &Map<String, Value>) -> Result<Map<String, Value>, String> {
        let cle = self.entree(id)?.settings_key();
        let mut s = self.0.reglages.lire(&cle);
        for (k, v) in valeurs {
            if s.secret.contains(k) && v.as_str() == Some(SECRET_MASQUE) {
                continue;
            }
            s.values.insert(k.clone(), v.clone());
        }
        self.0.reglages.ecrire(&cle, &s)?;
        Ok(masquer(&s))
    }

    // ───────────── Exécution interactive ─────────────

    /// Refus si un script interactif tourne (ou applique ses modifications).
    fn verifier_libre(&self) -> Result<(), String> {
        if self.0.fermeture.load(Ordering::SeqCst) {
            return Err("ComicRust se ferme : scripts indisponibles".into());
        }
        if self.etat().interactif.as_ref().is_some_and(|x| x.occupee()) {
            return Err(
                "Un script est déjà en cours : attends sa fin (ou arrête-le) avant d'en lancer un autre.".into()
            );
        }
        Ok(())
    }

    /// Lance le script `id` depuis un menu (`script_lancer`) : `Books` (sélection), `Library`
    /// (toute la bibliothèque si `ids` est vide) ou `ConfigScript`. Rend l'identifiant de
    /// l'exécution.
    pub fn lancer(&self, id: &str, hook: &str, ids: Vec<String>) -> Result<String, String> {
        let hook = Hook::parse(hook).ok_or_else(|| format!("hook inconnu : {hook}"))?;
        if !matches!(hook, Hook::Books | Hook::Library | Hook::ConfigScript) {
            return Err(format!("le hook {} ne se lance pas depuis un menu", hook.name()));
        }
        let entree = self.entree(id)?;
        if !entree.has_hook(hook) {
            return Err(format!("« {} » n'a pas le hook {}", entree.name, hook.name()));
        }
        if hook != Hook::ConfigScript && !est_actif(&verrou(&self.0.reglages.r), &entree) {
            return Err(format!("« {} » est désactivé (Gérer les scripts)", entree.name));
        }
        self.verifier_libre()?;
        let db = match self.0.applicateur.biblio.base() {
            Ok(db) => db,
            Err(_) if hook == Hook::ConfigScript => Arc::new(ComicDatabase::default()),
            Err(e) => return Err(e),
        };
        let ids =
            if hook == Hook::Library && ids.is_empty() { db.books.iter().map(|b| b.id.clone()).collect() } else { ids };
        if hook == Hook::Books && ids.is_empty() {
            return Err("aucun livre sélectionné".into());
        }
        self.demarrer_interactif(entree, hook, ids, db, None)
    }

    /// Hook Editor de la fenêtre Infos (`script_hook_editeur`) : le script voit la fiche avec
    /// les champs saisis (`champs`, nom ComicRack → texte) ; les champs proposés reviennent dans
    /// `script:fin.champs` (rien n'est enregistré).
    pub fn hook_editeur(&self, id: &str, livre_id: &str, champs: &Map<String, Value>) -> Result<String, String> {
        let entree = self.entree(id)?;
        if !entree.has_hook(Hook::Editor) {
            return Err(format!("« {} » n'a pas le hook Editor", entree.name));
        }
        self.verifier_libre()?;
        let db = self.0.applicateur.biblio.base()?;
        let mut livre = db.get(livre_id).cloned().ok_or_else(|| format!("livre introuvable : {livre_id}"))?;
        for (nom, v) in champs {
            let v = match v {
                Value::String(s) if s.trim().is_empty() => Value::Null,
                autre => autre.clone(),
            };
            // Valeur refusée : la fiche garde l'ancienne (la fenêtre la signalera à l'enregistrement).
            let e = scripting::field_edit(&livre.id, nom, &v);
            let _ = comicrust_core::edit::apply_to_book(&mut livre, &e);
        }
        let ids = vec![livre.id.clone()];
        self.demarrer_interactif(entree, Hook::Editor, ids, db, Some(livre))
    }

    fn hote(&self, x: &Execution, db: Arc<ComicDatabase>, direct: Option<EnDirect>) -> HoteTauri {
        let mut fraiches = HashMap::new();
        if let Some(l) = &x.editeur {
            fraiches.insert(l.id.to_ascii_lowercase(), l.clone());
        }
        HoteTauri {
            run_id: x.run_id.clone(),
            nom: x.entree.name.clone(),
            db,
            fraiches: Mutex::new(fraiches),
            sortie: self.0.sortie.clone(),
            stop: x.stop.clone(),
            attentes: x.attentes.clone(),
            reglages: self.0.reglages.clone(),
            renommages: Some(self.dossier_renommages()),
            ouvreur: self.0.ouvreur.clone(),
            presse_papiers: self.0.presse_papiers.clone(),
            auto: direct.is_some().then(|| (x.entree.name.clone(), x.hook.name().to_string())),
            direct,
        }
    }

    fn nouvelle_execution(&self, entree: ScriptEntry, hook: Hook, editeur: Option<LibraryBook>) -> Arc<Execution> {
        Arc::new(Execution {
            run_id: self.nouvel_id(),
            entree,
            hook,
            stop: Arc::default(),
            attentes: Arc::default(),
            handle: Mutex::new(None),
            resultat: Mutex::new(None),
            apercu: Mutex::new(None),
            stop_application: Arc::default(),
            en_application: AtomicBool::new(false),
            editeur,
        })
    }

    fn demarrer_interactif(
        &self,
        entree: ScriptEntry,
        hook: Hook,
        ids: Vec<String>,
        db: Arc<ComicDatabase>,
        editeur: Option<LibraryBook>,
    ) -> Result<String, String> {
        let python = self.python()?;
        let x = self.nouvelle_execution(entree, hook, editeur);
        {
            // Réservé sous le verrou : deux lancements simultanés ne passent pas tous les deux.
            let mut g = self.etat();
            if g.interactif.as_ref().is_some_and(|y| y.occupee()) {
                return Err("Un script est déjà en cours : attends sa fin avant d'en lancer un autre.".into());
            }
            g.interactif = Some(x.clone());
            g.execs.push(x.clone());
            let mut trop = g.execs.len().saturating_sub(EXECUTIONS_GARDEES);
            g.execs.retain(|y| {
                if trop > 0 && !y.occupee() {
                    trop -= 1;
                    false
                } else {
                    true
                }
            });
        }
        let mut contexte = RunContext::new(hook, ids);
        contexte.interactive = true;
        contexte.live = false;
        let n = contexte.book_ids.len();
        self.systeme(
            &x.run_id,
            format!(
                "ComicRust-Tauri v{} — « {} » ({}, {} livre(s))",
                env!("CARGO_PKG_VERSION"),
                x.entree.name,
                hook.name(),
                n
            ),
        );
        let hote = Arc::new(self.hote(&x, db, None));
        let handle = ScriptRunner::new(self.options(hook, python)).start(&x.entree, contexte, hote);
        *verrou(&x.handle) = Some(handle);
        let me = self.clone();
        let x2 = x.clone();
        std::thread::Builder::new()
            .name("comicrust-script-attente".into())
            .spawn(move || me.attendre_fin(&x2))
            .map_err(|e| format!("fil d'attente du script impossible : {e}"))?;
        Ok(x.run_id.clone())
    }

    /// Attend la fin du script (fil dédié), calcule l'aperçu, puis émet `script:fin`.
    fn attendre_fin(&self, x: &Execution) -> RunResult {
        let r = loop {
            if let Some(r) = verrou(&x.handle).as_mut().and_then(|h| h.poll().cloned()) {
                break r;
            }
            std::thread::sleep(Duration::from_millis(40));
        };
        x.attentes.vider();
        if self.etat().autos.iter().any(|y| std::ptr::eq(y.as_ref(), x)) {
            // Automatisme : bilan dans `scripts:auto` (fil de l'automatisme), pas de `script:fin`.
            *verrou(&x.resultat) = Some(r.clone());
            return r;
        }
        let (etat, erreur) = match r.state {
            RunState::Finished => ("termine", None),
            RunState::Cancelled => ("annule", None),
            RunState::Failed => ("erreur", Some(r.failure.as_ref().map(|f| f.to_string()).unwrap_or_default())),
            RunState::TimedOut => ("erreur", Some("délai maximum dépassé".to_string())),
        };
        let mut champs = None;
        if let Some(livre) = &x.editeur {
            let (c, problemes) = hote::champs_editeur(livre, &r.changes);
            for p in problemes {
                self.systeme(&x.run_id, p);
            }
            champs = Some(c);
        } else if !r.changes.is_empty() {
            // Fiches actuelles (une modification faite pendant l'exécution est prise en compte).
            let db = self.0.applicateur.biblio.base().unwrap_or_default();
            *verrou(&x.apercu) = Some(ApercuCalcule::construire(&r.changes, &db));
        }
        if !r.rejected.is_empty() {
            self.systeme(&x.run_id, format!("{} modification(s) refusée(s) par ComicRust", r.rejected.len()));
        }
        let nb_modifs = r.changes.len();
        *verrou(&x.resultat) = Some(r.clone());
        {
            let mut g = self.etat();
            if g.interactif.as_ref().is_some_and(|y| std::ptr::eq(y.as_ref(), x) && !y.occupee()) {
                g.interactif = None;
            }
        }
        self.systeme(&x.run_id, format!("Script {etat} ({nb_modifs} modification(s) demandée(s))"));
        self.emettre(EvenementScript::Fin { run_id: x.run_id.clone(), etat, erreur, nb_modifs, champs });
        r
    }

    /// Réponse du front à une question (`script_repondre`).
    pub fn repondre(&self, run_id: &str, req_id: u64, valeur: Value) -> Result<(), String> {
        self.execution(run_id)?.attentes.repondre(req_id, valeur)
    }

    /// Vignettes attendues d'une liste de choix (`script_vignettes_voulues`).
    pub fn vignettes_voulues(&self, run_id: &str, req_id: u64, rangs: &[usize]) -> Result<(), String> {
        self.execution(run_id)?.attentes.vignettes_voulues(req_id, rangs)
    }

    /// Arrêt (`script_arreter`) : annulation polie, arrêt forcé du processus après 3 s (délai de
    /// grâce du cœur) ; une application en cours s'arrête entre deux livres.
    pub fn arreter(&self, run_id: &str) -> Result<(), String> {
        let x = self.execution(run_id)?;
        if x.occupee() {
            self.systeme(run_id, "Arrêt demandé…".into());
        }
        x.arreter();
        Ok(())
    }

    /// Aperçu des modifications d'une exécution terminée (`script_apercu`).
    pub fn apercu(&self, run_id: &str) -> Result<Apercu, String> {
        let x = self.execution(run_id)?;
        if !x.terminee() {
            return Err("script en cours : l'aperçu vient à sa fin".into());
        }
        let motif = self.motif_lecture_seule();
        let a = verrou(&x.apercu).as_ref().map(|a| a.pour_le_front(motif.clone()));
        Ok(a.unwrap_or_else(|| Apercu { lecture_seule: motif.is_some(), motif, ..Default::default() }))
    }

    /// Applique les lignes `cles` de l'aperçu (`script_appliquer`) ; refusé en lecture seule.
    pub fn appliquer(&self, run_id: &str, cles: &[String]) -> Result<Bilan, String> {
        let x = self.execution(run_id)?;
        if !x.terminee() {
            return Err("script en cours : attends sa fin avant d'appliquer".into());
        }
        if self.0.fermeture.load(Ordering::SeqCst) {
            return Err("ComicRust se ferme : modifications non appliquées".into());
        }
        if let Some(m) = self.motif_lecture_seule() {
            return Err(m);
        }
        if x.en_application.swap(true, Ordering::SeqCst) {
            return Err("application déjà en cours".into());
        }
        self.0.applications.fetch_add(1, Ordering::SeqCst);
        let fin = Fin(self, &x);
        let plan = {
            let a = verrou(&x.apercu);
            let Some(a) = a.as_ref() else { return Err("rien à appliquer".into()) };
            let base = self.0.applicateur.biblio.base()?;
            a.plan(cles, &base)
        };
        x.stop_application.store(false, Ordering::SeqCst);
        let run_id = x.run_id.clone();
        let progression = |t: String| {
            self.emettre(EvenementScript::Progression {
                run_id: run_id.clone(),
                fraction: None,
                texte: Some(t),
                detail: None,
                image: None,
            })
        };
        let r = self.0.applicateur.appliquer(&plan, &x.stop_application, &progression);
        let biblio = &self.0.applicateur.biblio;
        let bilan = r.bilan(|id| biblio.livre(id).map(|b| b.caption()).unwrap_or_else(|| id.to_string()));
        // Bilan des renommages faits, relu par le script (`comicrust.applied_renames()`).
        let faits = r.renommages_faits();
        if !faits.is_empty() {
            match AppliedRenames::new(&x.entree, faits).save(&self.dossier_renommages()) {
                Ok(f) => self.systeme(&run_id, format!("Bilan des renommages appliqués : {}", f.display())),
                Err(e) => self.systeme(&run_id, format!("Bilan des renommages appliqués non enregistré : {e}")),
            }
        }
        if !r.modifies.is_empty() {
            self.emettre(EvenementScript::BibliothequeModifiee { ids: r.modifies.clone() });
        }
        *verrou(&x.apercu) = None;
        self.systeme(&run_id, format!("Bilan : {}", bilan.resume));
        drop(fin);
        self.emettre(EvenementScript::Applique { run_id, bilan: bilan.clone() });
        Ok(bilan)
    }

    // ───────────── Automatismes ─────────────

    /// Bibliothèque ouverte : automatismes `Startup` (une fois par session).
    pub fn bibliotheque_ouverte(&self) {
        let premiere = !std::mem::replace(&mut self.etat().startup_fait, true);
        if premiere {
            self.declencher(Hook::Startup, Vec::new());
        }
    }

    /// Met en file les automatismes actifs de `hook` (si les automatismes sont activés) et lance
    /// ceux qui peuvent partir.
    pub fn declencher(&self, hook: Hook, ids: Vec<String>) {
        if self.0.fermeture.load(Ordering::SeqCst) || !hook.is_automatic() {
            return;
        }
        let r = verrou(&self.0.reglages.r).clone();
        if !r.auto.unwrap_or(false) {
            return;
        }
        let mut ignores = Vec::new();
        {
            let mut g = self.etat();
            let taches: Vec<TacheAuto> = g
                .entrees
                .iter()
                .filter(|e| e.has_hook(hook) && est_actif(&r, e))
                .map(|e| TacheAuto { entree: e.clone(), hook, ids: ids.clone() })
                .collect();
            for t in taches {
                let nom = t.entree.name.clone();
                if !g.file.ajouter(t) {
                    ignores.push(nom);
                }
            }
        }
        for nom in ignores {
            self.journal_auto(
                &nom,
                hook,
                format!("« {nom} » ({}) ignoré : trop d'automatismes en attente", hook.name()),
            );
        }
        self.lancer_autos();
    }

    /// Lance les automatismes prêts.
    fn lancer_autos(&self) {
        if self.0.fermeture.load(Ordering::SeqCst) {
            return;
        }
        let taches = self.etat().file.pretes();
        if taches.is_empty() {
            return;
        }
        let refus = match (self.motif_lecture_seule(), self.0.applicateur.biblio.base()) {
            (Some(m), _) => Some(format!("non lancé : lecture seule ({m})")),
            (None, Err(e)) => Some(format!("non lancé : {e}")),
            (None, Ok(_)) => self.python().err().map(|e| format!("non lancé : {e}")),
        };
        if let Some(m) = refus {
            let mut g = self.etat();
            for t in &taches {
                g.file.terminee(t.hook);
            }
            g.file.vider();
            drop(g);
            for t in &taches {
                self.journal_auto(&t.entree.name, t.hook, format!("« {} » ({}) {m}", t.entree.name, t.hook.name()));
            }
            return;
        }
        let (Ok(db), Ok(python)) = (self.0.applicateur.biblio.base(), self.python()) else { return };
        for t in taches {
            let x = self.nouvelle_execution(t.entree.clone(), t.hook, None);
            self.etat().autos.push(x.clone());
            let me = self.clone();
            let journal: Arc<dyn Fn(String) + Send + Sync> = {
                let (nom, hook) = (t.entree.name.clone(), t.hook);
                Arc::new(move |l| me.journal_auto(&nom, hook, l))
            };
            let direct = EnDirect { applicateur: self.0.applicateur.clone(), journal };
            let hote = Arc::new(self.hote(&x, db.clone(), Some(direct)));
            let contexte = RunContext::new(t.hook, t.ids.clone());
            let n = if t.ids.is_empty() { String::new() } else { format!(" sur {} livre(s)", t.ids.len()) };
            self.journal_auto(&t.entree.name, t.hook, format!("« {} » ({}) lancé{n}", t.entree.name, t.hook.name()));
            let handle = ScriptRunner::new(self.options(t.hook, python.clone())).start(&t.entree, contexte, hote);
            *verrou(&x.handle) = Some(handle);
            let me = self.clone();
            let lance = std::thread::Builder::new().name("comicrust-automatisme".into()).spawn(move || {
                let r = me.attendre_fin(&x);
                let etat = match r.state {
                    RunState::Finished => "terminé".to_string(),
                    RunState::Cancelled => "arrêté".to_string(),
                    RunState::TimedOut => "arrêté (délai dépassé)".to_string(),
                    RunState::Failed => {
                        format!("en erreur : {}", r.failure.as_ref().map(|f| f.to_string()).unwrap_or_default())
                    }
                };
                me.journal_auto(&x.entree.name, x.hook, format!("« {} » ({}) {etat}", x.entree.name, x.hook.name()));
                {
                    let mut g = me.etat();
                    g.file.terminee(x.hook);
                    g.autos.retain(|y| !Arc::ptr_eq(y, &x));
                }
                me.lancer_autos();
            });
            if lance.is_err() {
                self.etat().file.terminee(t.hook);
            }
        }
    }

    // ───────────── Fermeture ─────────────

    /// Fermeture de l'application : annule tout (scripts, automatismes, application entre deux
    /// livres) et attend au plus `delai` ; une application en cours n'est jamais coupée au milieu
    /// d'un livre (attente jusqu'à la fin du livre en cours). Vrai si tout est arrêté.
    pub fn fermer(&self, delai: Duration) -> bool {
        self.0.fermeture.store(true, Ordering::SeqCst);
        let toutes: Vec<Arc<Execution>> = {
            let mut g = self.etat();
            g.file.vider();
            g.execs.iter().chain(g.autos.iter()).cloned().collect()
        };
        for x in &toutes {
            x.arreter();
        }
        if self.0.applications.load(Ordering::SeqCst) > 0 {
            self.0.applicateur.edition.annuler();
        }
        let debut = Instant::now();
        loop {
            let scripts_finis = toutes.iter().all(|x| x.terminee()) && self.etat().autos.is_empty();
            let applications = self.0.applications.load(Ordering::SeqCst);
            if scripts_finis && applications == 0 {
                return true;
            }
            if applications == 0 && debut.elapsed() >= delai {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Questions en attente d'une exécution (tests).
    pub fn questions_en_attente(&self, run_id: &str) -> usize {
        self.execution(run_id).map_or(0, |x| x.attentes.en_attente())
    }
}

/// Fin d'une application (même en cas d'erreur) : compteur et indicateurs remis à zéro.
struct Fin<'a>(&'a Scripts, &'a Arc<Execution>);

impl Drop for Fin<'_> {
    fn drop(&mut self) {
        self.1.en_application.store(false, Ordering::SeqCst);
        (self.0).0.applications.fetch_sub(1, Ordering::SeqCst);
        let mut g = self.0.etat();
        if g.interactif.as_ref().is_some_and(|y| Arc::ptr_eq(y, self.1)) && !self.1.occupee() {
            g.interactif = None;
        }
    }
}

/// Réglages pour le front : valeurs secrètes remplacées par [`SECRET_MASQUE`] (vide si non
/// définie).
pub fn masquer(s: &ScriptSettings) -> Map<String, Value> {
    s.values
        .iter()
        .map(|(k, v)| {
            let v = if s.secret.contains(k) {
                let defini = match v {
                    Value::String(t) => !t.is_empty(),
                    Value::Null => false,
                    _ => true,
                };
                Value::String(if defini { SECRET_MASQUE.into() } else { String::new() })
            } else {
                v.clone()
            };
            (k.clone(), v)
        })
        .collect()
}

/// Image de menu en URL `data:` (fichier absent, trop gros ou format inconnu : `None`).
fn image_menu(p: &Path) -> Option<String> {
    let meta = std::fs::metadata(p).ok()?;
    if !meta.is_file() || meta.len() > IMAGE_MENU_MAX {
        return None;
    }
    formulaires::url_image(&std::fs::read(p).ok()?)
}
