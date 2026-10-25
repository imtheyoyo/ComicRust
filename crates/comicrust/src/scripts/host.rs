//! `ScriptHost` de l'interface : le script tourne dans son propre fil (et son processus
//! Python), l'interface dans le sien. Tout ce qui doit être montré passe par un canal
//! ([`HostEvent`]) lu à chaque image ; une question bloque le fil du script jusqu'à la
//! réponse (ou l'arrêt du script : valeur par défaut).

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{book_json, JournalLine, Question, ScriptChange, ScriptHost, ScriptSettings};
use eframe::egui;
use serde_json::Value;

use super::apply::{apply_plan, ApplyOutcome, ApplyPlan, ApplyTarget};
use crate::library::watch::WrittenSink;

/// Réglages des scripts partagés entre les fils (clé → JSON de `ScriptSettings::to_stored`).
pub type SharedSettings = Arc<Mutex<BTreeMap<String, String>>>;

/// Relit les réglages enregistrés d'un script.
pub fn load_settings(stored: &BTreeMap<String, String>, key: &str) -> ScriptSettings {
    stored
        .get(key)
        .and_then(|t| serde_json::from_str::<Value>(t).ok())
        .map(|v| ScriptSettings::from_stored(&v))
        .unwrap_or_default()
}

/// Forme enregistrée des réglages d'un script (valeurs secrètes chiffrées).
pub fn store_settings(s: &ScriptSettings) -> String {
    s.to_stored().to_string()
}

/// Message du fil du script vers l'interface.
pub enum HostEvent {
    Log(JournalLine),
    Progress(Option<f32>, String),
    /// Question : la réponse (`None` = valeur par défaut) part par `reply`.
    Ask {
        question: Question,
        reply: Sender<Option<Value>>,
    },
    /// Message à lire ; le script attend « OK » (`reply`).
    Message {
        text: String,
        reply: Sender<()>,
    },
    /// Modifications appliquées au fil de l'eau (automatisme).
    Applied(Box<ApplyOutcome>),
    /// Modifications refusées : bibliothèque en lecture seule (automatisme).
    LiveRefused(String),
}

/// Application au fil de l'eau (automatismes).
pub struct Live {
    pub target: ApplyTarget,
    /// Surveillance de la bibliothèque, prévenue des écritures.
    pub sink: Option<WrittenSink>,
}

pub struct UiHost {
    /// Bibliothèque au lancement du script.
    db: Arc<ComicDatabase>,
    /// Fiches relues après une application au fil de l'eau (plus récentes que `db`).
    fresh: Mutex<HashMap<String, LibraryBook>>,
    tx: Sender<HostEvent>,
    ctx: egui::Context,
    /// Arrêt demandé par l'utilisateur : une question en cours rend la valeur par défaut.
    stop: Arc<AtomicBool>,
    stored: SharedSettings,
    settings_dirty: Arc<AtomicBool>,
    live: Option<Live>,
}

impl UiHost {
    pub fn new(
        db: Arc<ComicDatabase>,
        tx: Sender<HostEvent>,
        ctx: egui::Context,
        stop: Arc<AtomicBool>,
        stored: SharedSettings,
        settings_dirty: Arc<AtomicBool>,
        live: Option<Live>,
    ) -> UiHost {
        UiHost { db, fresh: Mutex::new(HashMap::new()), tx, ctx, stop, stored, settings_dirty, live }
    }

    fn send(&self, ev: HostEvent) {
        let _ = self.tx.send(ev);
        self.ctx.request_repaint();
    }

    fn book(&self, id: &str) -> Option<LibraryBook> {
        let key = id.to_ascii_lowercase();
        if let Some(b) = self.fresh.lock().unwrap().get(&key) {
            return Some(b.clone());
        }
        self.db.get(id).cloned()
    }

    /// Attend une réponse de l'interface ; `None` si le script est arrêté ou la fenêtre fermée.
    fn wait<T>(&self, rx: mpsc::Receiver<T>) -> Option<T> {
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(v) => return Some(v),
                Err(RecvTimeoutError::Timeout) if !self.stop.load(Ordering::SeqCst) => {}
                Err(_) => return None,
            }
        }
    }
}

impl ScriptHost for UiHost {
    fn books(&self, ids: &[String]) -> Vec<Value> {
        ids.iter().filter_map(|id| self.book(id)).map(|b| book_json(&b)).collect()
    }

    fn ask(&self, question: &Question) -> Option<Value> {
        let (reply, rx) = mpsc::channel();
        self.send(HostEvent::Ask { question: question.clone(), reply });
        self.wait(rx).flatten()
    }

    fn message(&self, text: &str) {
        let (reply, rx) = mpsc::channel();
        self.send(HostEvent::Message { text: text.to_string(), reply });
        let _ = self.wait(rx);
    }

    fn log(&self, line: &JournalLine) {
        self.send(HostEvent::Log(line.clone()));
    }

    fn progress(&self, fraction: Option<f32>, text: &str) {
        self.send(HostEvent::Progress(fraction, text.to_string()));
    }

    fn load_settings(&self, key: &str) -> ScriptSettings {
        load_settings(&self.stored.lock().unwrap(), key)
    }

    fn save_settings(&self, key: &str, settings: &ScriptSettings) -> Result<(), String> {
        self.stored.lock().unwrap().insert(key.to_string(), store_settings(settings));
        self.settings_dirty.store(true, Ordering::SeqCst);
        self.ctx.request_repaint();
        Ok(())
    }

    fn apply_live(&self, changes: &[ScriptChange]) -> Result<(), String> {
        let Some(live) = &self.live else { return Err("application immédiate non prévue".into()) };
        if let Some(m) = live.target.read_only() {
            self.send(HostEvent::LiveRefused(m.to_string()));
            return Err(m.to_string());
        }
        let plan = ApplyPlan::from_changes(changes, |id| self.book(id));
        let outcome = apply_plan(&live.target, &plan);
        let written = outcome.written();
        if let Some(sink) = &live.sink {
            sink.lock().unwrap().extend(written);
        }
        {
            let mut fresh = self.fresh.lock().unwrap();
            for b in outcome.fresh_books() {
                fresh.insert(b.id.to_ascii_lowercase(), b);
            }
        }
        let result = if outcome.is_complete() {
            Ok(())
        } else {
            let caption = |id: &str| self.book(id).map(|b| b.caption()).unwrap_or_else(|| id.to_string());
            Err(outcome.problems(caption).join(" ; "))
        };
        self.send(HostEvent::Applied(Box::new(outcome)));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use comicrust_core::scripting::{parse_script, Hook, RunContext, RunOptions, RunState, ScriptRunner};

    #[test]
    fn reglages_chiffres_aller_retour() {
        let mut s = ScriptSettings::default();
        s.values.insert("dossier".into(), Value::String("D:\\BD".into()));
        s.values.insert("n".into(), Value::from(3));
        let mut stored = BTreeMap::new();
        stored.insert("a.py".to_string(), store_settings(&s));
        let back = load_settings(&stored, "a.py");
        assert_eq!(back.values, s.values);
        assert_eq!(load_settings(&stored, "autre.py"), ScriptSettings::default());
    }

    /// Script réel (python3 du système) : question répondue par « l'interface », puis
    /// question interrompue par « Arrêter » (valeur par défaut rendue au script).
    #[test]
    fn questions_par_canal_et_arret_pendant_une_question() {
        if scripting_python().is_none() {
            eprintln!("python3 absent : test ignoré");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let code = "import comicrust\n\n#@Name Q\n#@Hook Library\ndef q(books):\n    \
                    a = comicrust.ask('Nom ?', 'défaut')\n    print('réponse', a)\n    \
                    b = comicrust.confirm('Sûr ?', True)\n    print('confirmé', b)\n    \
                    import time\n    \
                    while not comicrust.cancelled():\n        time.sleep(0.05)\n";
        let file = dir.path().join("q.py");
        std::fs::write(&file, code).unwrap();
        let entry = parse_script(code, &file, "q.py").0.remove(0);
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let host = UiHost::new(
            Arc::new(ComicDatabase::default()),
            tx,
            egui::Context::default(),
            stop.clone(),
            Default::default(),
            Default::default(),
            None,
        );
        let opts = RunOptions { module_dir: Some(dir.path().join("module")), ..Default::default() };
        let mut ctx = RunContext::new(Hook::Library, vec![]);
        ctx.interactive = true;
        let handle = ScriptRunner::new(opts).start(&entry, ctx, Arc::new(host));
        let next_question = || loop {
            match rx.recv_timeout(Duration::from_secs(20)).expect("question attendue") {
                HostEvent::Ask { question, reply } => return (question, reply),
                _ => continue,
            }
        };
        let (q, reply) = next_question();
        assert_eq!(q.text, "Nom ?");
        reply.send(Some(Value::String("Yoyo".into()))).unwrap();
        // Seconde question : l'utilisateur arrête le script au lieu de répondre.
        let (q, reply) = next_question();
        assert_eq!(q.text, "Sûr ?");
        stop.store(true, Ordering::SeqCst);
        handle.cancel();
        drop(reply);
        let r = handle.wait();
        // Le script sort de lui-même de sa boucle (`cancelled()`) : fin normale ou arrêt.
        assert!(matches!(r.state, RunState::Cancelled | RunState::Finished), "{:?}", r.state);
        let text = r.journal_text();
        assert!(text.contains("réponse Yoyo"), "{text}");
        assert!(text.contains("confirmé True"), "valeur par défaut après l'arrêt : {text}");
    }

    fn scripting_python() -> Option<std::path::PathBuf> {
        comicrust_core::scripting::find_python(Some(std::path::Path::new("/nulle-part"))).ok()
    }
}
