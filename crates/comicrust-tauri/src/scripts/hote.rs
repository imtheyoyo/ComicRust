//! `ScriptHost` de la version Tauri. Le script tourne dans son fil (et son processus Python) ;
//! tout ce qui doit être montré part en **événement** vers le front ; une question, un message,
//! un formulaire ou une liste de choix **bloque le fil du script** (jamais l'IPC) jusqu'à la
//! réponse (`script_repondre`) ou l'arrêt du script (valeur par défaut / `None`).

use super::apercu::{Applicateur, Plan};
use super::formulaires::{self, url_image};
use super::{EvenementScript, Sortie};
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{
    book_json, AppliedRenames, ChooseSpec, FormAnswer, FormSpec, JournalKind, JournalLine, ProgressDetails, Question,
    ScriptChange, ScriptHost, ScriptSettings,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn verrou<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ouvre une adresse http(s) déjà vérifiée ; vrai si c'est fait.
pub type Ouvreur = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Questions en attente d'une exécution, partagées entre le fil du script et les commandes IPC.
#[derive(Default)]
pub struct Attentes {
    prochain: AtomicU64,
    /// `reqId` → canal de la réponse.
    reponses: Mutex<HashMap<u64, Sender<Value>>>,
    /// Liste de choix ouverte (`reqId`) et lignes dont le front attend la vignette.
    choix_ouvert: Mutex<Option<u64>>,
    voulues: Mutex<Vec<usize>>,
}

impl Attentes {
    fn nouvelle(&self) -> (u64, mpsc::Receiver<Value>) {
        let id = self.prochain.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel();
        verrou(&self.reponses).insert(id, tx);
        (id, rx)
    }

    fn oublier(&self, id: u64) {
        verrou(&self.reponses).remove(&id);
    }

    /// Réponse du front ; `Err` si la question n'attend plus (déjà répondue, script arrêté).
    pub fn repondre(&self, id: u64, valeur: Value) -> Result<(), String> {
        let tx = verrou(&self.reponses).remove(&id).ok_or_else(|| format!("question {id} : plus en attente"))?;
        tx.send(valeur).map_err(|_| format!("question {id} : le script ne l'attend plus"))
    }

    /// Lignes de la liste de choix `id` dont le front attend la vignette.
    pub fn vignettes_voulues(&self, id: u64, rangs: &[usize]) -> Result<(), String> {
        if *verrou(&self.choix_ouvert) != Some(id) {
            return Err(format!("liste de choix {id} : plus ouverte"));
        }
        verrou(&self.voulues).extend_from_slice(rangs);
        Ok(())
    }

    /// Aucune question en attente (et canaux libérés : le script reçoit « pas de réponse »).
    pub fn vider(&self) {
        verrou(&self.reponses).clear();
    }

    pub fn en_attente(&self) -> usize {
        verrou(&self.reponses).len()
    }
}

/// Réglages des scripts partagés (clé = chemin relatif du fichier → forme enregistrée, valeurs
/// secrètes chiffrées) ; `enregistrer` les écrit dans `data\comicrust-tauri.json`.
pub trait ReglagesScriptsHote: Send + Sync {
    fn lire(&self, cle: &str) -> ScriptSettings;
    fn ecrire(&self, cle: &str, s: &ScriptSettings) -> Result<(), String>;
}

/// Application au fil de l'eau (automatismes).
pub struct EnDirect {
    pub applicateur: Applicateur,
    /// Journal `scripts:auto` (ligne).
    pub journal: Arc<dyn Fn(String) + Send + Sync>,
    /// Applications en cours (compteur du service, attendu par la fermeture de l'application).
    pub applications: Arc<AtomicUsize>,
    /// Fermeture de l'application demandée : plus rien n'est appliqué.
    pub fermeture: Arc<AtomicBool>,
}

/// Application comptée tant que la garde vit (décomptée même en cas d'erreur ou de panique).
pub struct Decompte<'a>(&'a AtomicUsize);

impl<'a> Decompte<'a> {
    pub fn compter(n: &'a AtomicUsize) -> Self {
        n.fetch_add(1, Ordering::SeqCst);
        Decompte(n)
    }
}

impl Drop for Decompte<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct HoteTauri {
    pub run_id: String,
    pub nom: String,
    /// Bibliothèque au lancement (fiche surchargée par les champs de la fenêtre Infos pour le
    /// hook Editor).
    pub db: Arc<ComicDatabase>,
    /// Fiches relues après une application au fil de l'eau.
    pub fraiches: Mutex<HashMap<String, LibraryBook>>,
    pub sortie: Sortie,
    /// Arrêt demandé : une question en cours rend sa valeur par défaut, une application au fil
    /// de l'eau s'arrête entre deux livres.
    pub stop: Arc<AtomicBool>,
    pub attentes: Arc<Attentes>,
    pub reglages: Arc<dyn ReglagesScriptsHote>,
    pub renommages: Option<PathBuf>,
    pub ouvreur: Ouvreur,
    pub presse_papiers: Ouvreur,
    pub direct: Option<EnDirect>,
    /// Automatisme : le journal va à `scripts:auto` au lieu de `script:journal`.
    pub auto: Option<(String, String)>,
}

impl HoteTauri {
    fn emettre(&self, e: EvenementScript) {
        (self.sortie)(e);
    }

    fn livre(&self, id: &str) -> Option<LibraryBook> {
        if let Some(b) = verrou(&self.fraiches).get(&id.to_ascii_lowercase()) {
            return Some(b.clone());
        }
        self.db.get(id).cloned()
    }

    /// Attend la réponse du front ; `None` si le script est arrêté.
    fn attendre(&self, id: u64, rx: mpsc::Receiver<Value>) -> Option<Value> {
        let r = loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(v) => break Some(v),
                Err(RecvTimeoutError::Timeout) if !self.stop.load(Ordering::SeqCst) => {}
                Err(_) => break None,
            }
        };
        self.attentes.oublier(id);
        r
    }
}

impl ScriptHost for HoteTauri {
    fn books(&self, ids: &[String]) -> Vec<Value> {
        ids.iter().filter_map(|id| self.livre(id)).map(|b| book_json(&b)).collect()
    }

    fn library_ids(&self) -> Option<Vec<String>> {
        Some(self.db.books.iter().map(|b| b.id.clone()).collect())
    }

    fn applied_renames(&self, settings_key: &str) -> Value {
        self.renommages
            .as_deref()
            .and_then(|d| AppliedRenames::load(d, settings_key))
            .and_then(|b| serde_json::to_value(b).ok())
            .unwrap_or(Value::Null)
    }

    fn ask(&self, q: &Question) -> Option<Value> {
        let (id, rx) = self.attentes.nouvelle();
        self.emettre(EvenementScript::Question {
            run_id: self.run_id.clone(),
            req_id: id,
            genre: "ask",
            titre: self.nom.clone(),
            texte: q.text.clone(),
            question: Some(formulaires::question_json(q)),
        });
        self.attendre(id, rx).filter(|v| !v.is_null())
    }

    fn message(&self, text: &str) {
        let (id, rx) = self.attentes.nouvelle();
        self.emettre(EvenementScript::Question {
            run_id: self.run_id.clone(),
            req_id: id,
            genre: "message",
            titre: self.nom.clone(),
            texte: text.to_string(),
            question: None,
        });
        let _ = self.attendre(id, rx);
    }

    fn log(&self, line: &JournalLine) {
        if let Some((script, hook)) = &self.auto {
            if line.kind != JournalKind::Output || !line.text.is_empty() {
                self.emettre(EvenementScript::Auto {
                    script: script.clone(),
                    hook: hook.clone(),
                    ligne: format!("[{script}] {}", line.text),
                });
            }
            return;
        }
        let (genre, texte) = match line.kind {
            JournalKind::Output => ("sortie", line.text.clone()),
            JournalKind::Info => ("info", line.text.clone()),
            JournalKind::Warning => ("info", format!("[attention] {}", line.text)),
            JournalKind::Error => ("erreur", line.text.clone()),
        };
        self.emettre(EvenementScript::Journal { run_id: self.run_id.clone(), genre, texte });
    }

    fn progress(&self, fraction: Option<f32>, text: &str) {
        if self.auto.is_none() {
            self.emettre(EvenementScript::Progression {
                run_id: self.run_id.clone(),
                fraction,
                texte: Some(text.to_string()),
                detail: None,
                image: None,
            });
        }
    }

    fn progress_details(&self, d: &ProgressDetails) {
        if self.auto.is_some() {
            return;
        }
        // Image retirée : chaîne vide ; inchangée : absente.
        let image = d.image.as_ref().map(|i| i.as_deref().and_then(|b| url_image(b)).unwrap_or_default());
        self.emettre(EvenementScript::Progression {
            run_id: self.run_id.clone(),
            fraction: None,
            texte: None,
            detail: d.detail.clone(),
            image,
        });
    }

    fn form(&self, spec: &FormSpec) -> Option<FormAnswer> {
        let (id, rx) = self.attentes.nouvelle();
        self.emettre(EvenementScript::Formulaire {
            run_id: self.run_id.clone(),
            req_id: id,
            spec: formulaires::formulaire_json(spec),
        });
        let v = self.attendre(id, rx)?;
        formulaires::reponse_formulaire(spec, &v)
    }

    fn choose(&self, spec: &ChooseSpec) -> Option<Vec<usize>> {
        let (id, rx) = self.attentes.nouvelle();
        verrou(&self.attentes.voulues).clear();
        *verrou(&self.attentes.choix_ouvert) = Some(id);
        self.emettre(EvenementScript::Choix {
            run_id: self.run_id.clone(),
            req_id: id,
            spec: formulaires::choix_json_spec(spec),
        });
        let v = self.attendre(id, rx);
        *verrou(&self.attentes.choix_ouvert) = None;
        formulaires::reponse_choix(&v?)
    }

    fn choose_image(&self, index: usize, image: Option<Arc<Vec<u8>>>) {
        let Some(id) = *verrou(&self.attentes.choix_ouvert) else { return };
        let image = image.as_deref().and_then(|b| url_image(b)).unwrap_or_default();
        self.emettre(EvenementScript::Vignette { run_id: self.run_id.clone(), req_id: id, rang: index, image });
    }

    fn choose_wants(&self) -> Vec<usize> {
        std::mem::take(&mut *verrou(&self.attentes.voulues))
    }

    fn open_url(&self, url: &str) -> bool {
        // Vérifiée par le cœur (`check_url`) ; revérifiée ici par prudence.
        match comicrust_core::scripting::forms::check_url(url) {
            Ok(u) => (self.ouvreur)(&u),
            Err(_) => false,
        }
    }

    fn clipboard_set(&self, text: &str) -> bool {
        (self.presse_papiers)(text)
    }

    fn load_settings(&self, key: &str) -> ScriptSettings {
        self.reglages.lire(key)
    }

    fn save_settings(&self, key: &str, settings: &ScriptSettings) -> Result<(), String> {
        self.reglages.ecrire(key, settings)
    }

    fn apply_live(&self, changes: &[ScriptChange]) -> Result<(), String> {
        let Some(direct) = &self.direct else { return Err("application immédiate non prévue".into()) };
        // Comptée **avant** de regarder la fermeture : soit la fermeture voit cette application
        // (et attend la fin du livre en cours), soit l'application voit la fermeture (et ne fait rien).
        let _compte = Decompte::compter(&direct.applications);
        if direct.fermeture.load(Ordering::SeqCst) {
            return Err("ComicRust se ferme : modifications non appliquées".into());
        }
        let plan = Plan::depuis_changements(changes, |id| self.livre(id));
        let r = direct.applicateur.appliquer(&plan, &self.stop, &|_| {});
        {
            let mut f = verrou(&self.fraiches);
            for b in &r.fraiches {
                f.insert(b.id.to_ascii_lowercase(), b.clone());
            }
        }
        if !r.modifies.is_empty() {
            self.emettre(EvenementScript::BibliothequeModifiee { ids: r.modifies.clone() });
        }
        let caption = |id: &str| self.livre(id).map(|b| b.caption()).unwrap_or_else(|| id.to_string());
        let bilan = r.bilan(caption);
        (direct.journal)(format!("[{}] modifications appliquées : {}", self.nom, bilan.resume));
        if r.complet() {
            Ok(())
        } else {
            Err(bilan.erreurs.join(" ; "))
        }
    }
}

/// `CREATE_NO_WINDOW` (Windows) : aucune fenêtre console ne clignote au lancement.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Commande système lancée sans fenêtre console sous Windows (`clip.exe`, `explorer.exe`).
fn commande_discrete(programme: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(programme);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Ouvre une adresse dans le navigateur par défaut (même mécanisme que `maj::ouvrir_page`,
/// adresse vérifiée par `check_url` du cœur : http/https avec un hôte).
pub fn ouvrir_url(url: &str) -> bool {
    let Ok(url) = comicrust_core::scripting::forms::check_url(url) else { return false };
    let programme = if cfg!(windows) { "explorer" } else { "xdg-open" };
    commande_discrete(programme)
        .arg(&url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Copie un texte dans le presse-papiers du système (Windows : `clip.exe`, UTF-16 avec BOM ;
/// ailleurs : `wl-copy` ou `xclip`).
pub fn copier_texte(texte: &str) -> bool {
    use std::io::Write;
    let essais: &[(&str, &[&str])] =
        if cfg!(windows) { &[("clip", &[])] } else { &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])] };
    for (programme, args) in essais {
        let Ok(mut enfant) = commande_discrete(programme)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            continue;
        };
        let octets: Vec<u8> = if cfg!(windows) {
            let mut o = vec![0xFF, 0xFE];
            o.extend(texte.encode_utf16().flat_map(u16::to_le_bytes));
            o
        } else {
            texte.as_bytes().to_vec()
        };
        let ecrit = enfant.stdin.take().is_some_and(|mut s| s.write_all(&octets).is_ok());
        if ecrit && enfant.wait().is_ok_and(|s| s.success()) {
            return true;
        }
    }
    false
}

/// Champs proposés par un script `Editor` pour le livre `id` (nom → valeur affichée dans la
/// fenêtre Infos), et problèmes (renommage ignoré, autre livre…).
pub fn champs_editeur(book: &LibraryBook, changes: &[ScriptChange]) -> (serde_json::Map<String, Value>, Vec<String>) {
    let mut champs = serde_json::Map::new();
    let mut problemes = Vec::new();
    let mut apres = book.clone();
    for c in changes {
        match c {
            ScriptChange::SetField { book_id, field, value } if book_id.eq_ignore_ascii_case(&book.id) => {
                let e = comicrust_core::scripting::field_edit(&book.id, field, value);
                match comicrust_core::edit::apply_to_book(&mut apres, &e) {
                    Ok(_) => {
                        champs.insert(field.clone(), json!(crate::edition::affichage_champ(field, &apres)));
                    }
                    Err(e) => problemes.push(format!("{field} : {}", e.message)),
                }
            }
            ScriptChange::WriteInfo { .. } => {
                champs.insert("$ecrireDansFichier".into(), json!(true));
            }
            ScriptChange::Rename { .. } => problemes.push("renommage du fichier ignoré dans cette fenêtre".into()),
            ScriptChange::SetField { .. } => problemes.push("modification d'un autre livre ignorée".into()),
        }
    }
    (champs, problemes)
}
