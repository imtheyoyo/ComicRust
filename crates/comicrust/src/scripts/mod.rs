//! Scripts Python côté interface (v0.10.0) : découverte du dossier `scripts\`, exécutions
//! lancées depuis les menus (fenêtre, questions, aperçu puis application), automatismes
//! (file d'attente, application au fil de l'eau) et réglages des scripts.
//!
//! - [`apply`] : aperçu des modifications, choix des livres, application et bilan ;
//! - [`host`] : `ScriptHost` de l'interface (canaux entre le fil du script et l'interface) ;
//! - [`auto`] : file d'attente des automatismes.
//!
//! Le dessin (menus, fenêtres) est dans `ui/scripts_ui.rs`.

pub mod apply;
pub mod auto;
pub mod host;

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use comicrust_core::db::ComicDatabase;
use comicrust_core::scripting::{
    self, discover_with_warnings, DiscoveryWarning, Hook, JournalKind, JournalLine, Question, RunContext, RunHandle,
    RunOptions, RunResult, RunState, ScriptEntry, ScriptRunner,
};
use eframe::egui;
use serde_json::Value;

use apply::{apply_plan, ApplyOutcome, ApplyTarget, Preview};
use auto::{AutoJob, AutoQueue};
use host::{HostEvent, Live, SharedSettings, UiHost};

use crate::library::watch::WrittenSink;

/// Nombre de lignes gardées dans le journal des automatismes.
const AUTO_JOURNAL_LINES: usize = 500;

/// Message quand Python manque (`detail` : chemin cherché).
pub fn python_missing(detail: &str) -> String {
    format!(
        "Python embarqué introuvable : dossier python\\ à côté de ComicRust.exe ({detail}).\n\nRéinstalle \
         ComicRust (le zip contient ce dossier) ou indique un interpréteur dans la variable d'environnement \
         COMICRUST_PYTHON."
    )
}

/// Interpréteur Python, ou message clair s'il manque.
pub fn python() -> Result<PathBuf, String> {
    scripting::find_python(None).map_err(|e| match e {
        scripting::ScriptError::PythonNotFound(p) => python_missing(&format!("cherché dans {p}")),
        other => other.to_string(),
    })
}

/// Dossier `scripts\` à côté de l'exécutable.
pub fn scripts_dir() -> PathBuf {
    crate::paths::exe_dir().join("scripts")
}

/// Question ou message en attente de l'utilisateur.
pub enum Pending {
    Ask {
        question: Question,
        reply: Sender<Option<Value>>,
        /// Saisie en cours (texte) ou choix (rang dans `question.choices`).
        text: String,
        choice: usize,
    },
    Message {
        text: String,
        reply: Sender<()>,
    },
}

impl Pending {
    fn ask(question: Question, reply: Sender<Option<Value>>) -> Pending {
        let text = match &question.default {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            v => v.to_string(),
        };
        let choice = question.choices.iter().position(|c| Some(c.as_str()) == question.default.as_str()).unwrap_or(0);
        Pending::Ask { question, reply, text, choice }
    }
}

/// Exécution lancée depuis un menu (fenêtre non bloquante).
pub struct ScriptRun {
    pub entry: ScriptEntry,
    pub hook: Hook,
    /// Livres du contexte.
    pub count: usize,
    pub started: Instant,
    handle: RunHandle,
    rx: Receiver<HostEvent>,
    stop: Arc<AtomicBool>,
    /// Bibliothèque au lancement (aperçu).
    pub db: Arc<ComicDatabase>,
    pub target: ApplyTarget,
    sink: Option<WrittenSink>,
    pub journal: Vec<JournalLine>,
    pub progress: Option<f32>,
    pub progress_text: String,
    pub pending: Option<Pending>,
    pub result: Option<RunResult>,
    pub preview: Option<Preview>,
    applying: Option<Receiver<ApplyOutcome>>,
    /// Bilan de l'application (texte à copier).
    pub report: Option<(String, Vec<String>)>,
    /// Modifications abandonnées par l'utilisateur.
    pub discarded: bool,
    pub stopping: bool,
}

impl ScriptRun {
    pub fn running(&self) -> bool {
        self.result.is_none()
    }

    pub fn applying(&self) -> bool {
        self.applying.is_some()
    }

    /// Arrêt demandé : la question en cours rend sa valeur par défaut.
    pub fn stop(&mut self) {
        self.stopping = true;
        self.stop.store(true, Ordering::SeqCst);
        self.handle.cancel();
        self.pending = None; // `reply` libéré : le fil du script reçoit « pas de réponse »
    }

    /// Événements du script ; renvoie le bilan d'une application terminée.
    pub fn poll(&mut self) -> Option<ApplyOutcome> {
        for ev in self.rx.try_iter().collect::<Vec<_>>() {
            match ev {
                HostEvent::Log(l) => self.journal.push(l),
                HostEvent::Progress(f, t) => {
                    self.progress = f;
                    self.progress_text = t;
                }
                HostEvent::Ask { question, reply } => {
                    if self.stopping {
                        drop(reply);
                    } else {
                        self.pending = Some(Pending::ask(question, reply));
                    }
                }
                HostEvent::Message { text, reply } => {
                    if !self.stopping {
                        self.pending = Some(Pending::Message { text, reply });
                    }
                }
                HostEvent::Applied(_) | HostEvent::LiveRefused(_) => {}
            }
        }
        if self.result.is_none() {
            if let Some(r) = self.handle.poll() {
                let r = r.clone();
                self.pending = None;
                // Journal complet (lignes d'erreur finales comprises).
                self.journal = r.journal.clone();
                // Menu terminé normalement : aperçu tout de suite. Arrêté : proposé.
                if r.state == RunState::Finished && !r.changes.is_empty() {
                    self.preview = Some(Preview::build(&r.changes, &self.db));
                }
                self.result = Some(r);
            }
        }
        let rx = self.applying.as_ref()?;
        let outcome = rx.try_recv().ok()?;
        self.applying = None;
        Some(outcome)
    }

    /// Aperçu de ce qui a été demandé avant un arrêt ou une erreur.
    pub fn show_partial_preview(&mut self) {
        if let Some(r) = &self.result {
            self.preview = Some(Preview::build(&r.changes, &self.db));
        }
    }

    /// Lance l'application des livres cochés (fil d'arrière-plan). `db` : bibliothèque
    /// actuelle (fiches à jour pour les renommages).
    pub fn start_apply(&mut self, db: &ComicDatabase, ctx: &egui::Context) {
        let Some(preview) = &self.preview else { return };
        let plan = preview.plan(db);
        let target = self.target.clone();
        let sink = self.sink.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("comicrust-script-application".into())
            .spawn(move || {
                let outcome = apply_plan(&target, &plan);
                if let Some(sink) = &sink {
                    sink.lock().unwrap().extend(outcome.written());
                }
                let _ = tx.send(outcome);
                ctx.request_repaint();
            })
            .expect("fil d'application des modifications");
        self.applying = Some(rx);
    }

    /// Journal en texte (à copier).
    pub fn journal_text(&self) -> String {
        let mut out = format!(
            "ComicRust v{} — script « {} » ({}, {}:{})\n",
            env!("CARGO_PKG_VERSION"),
            self.entry.name,
            self.hook.name(),
            self.entry.rel_path,
            self.entry.function
        );
        for l in &self.journal {
            let p = match l.kind {
                JournalKind::Output => "",
                JournalKind::Info => "[info] ",
                JournalKind::Warning => "[attention] ",
                JournalKind::Error => "[erreur] ",
            };
            out.push_str(&format!("{p}{}\n", l.text));
        }
        if let Some((summary, problems)) = &self.report {
            out.push_str(&format!("\nBilan : {summary}\n"));
            for p in problems {
                out.push_str(&format!("- {p}\n"));
            }
        }
        out
    }
}

/// Automatisme en cours.
pub struct AutoRun {
    pub job: AutoJob,
    handle: RunHandle,
    rx: Receiver<HostEvent>,
}

/// Fin d'un automatisme (pour l'application).
pub enum AutoEvent {
    Applied(String, ApplyOutcome),
    Refused(String, String),
    Finished(AutoJob, RunResult),
}

/// Tout l'état des scripts.
pub struct ScriptsState {
    pub dir: PathBuf,
    pub entries: Vec<ScriptEntry>,
    pub warnings: Vec<DiscoveryWarning>,
    /// Dossier impossible à créer / lire.
    pub dir_error: Option<String>,
    pub stored: SharedSettings,
    pub settings_dirty: Arc<AtomicBool>,
    pub run: Option<ScriptRun>,
    pub show_manager: bool,
    pub queue: AutoQueue,
    pub autos: Vec<AutoRun>,
    pub auto_journal: VecDeque<String>,
    /// Automatismes `Startup` déjà lancés pour cette session.
    pub startup_done: bool,
    /// Messages déjà signalés une fois pendant la session (lecture seule, Python absent…).
    pub warned: HashSet<String>,
    /// Livres dont les infos sont à écrire dans le fichier dès que le travail « Fichiers »
    /// en cours sera fini (`true` : montrer la fenêtre du travail).
    pub pending_write_info: Vec<(String, bool)>,
}

impl ScriptsState {
    pub fn new(stored: std::collections::BTreeMap<String, String>) -> ScriptsState {
        let mut s = ScriptsState {
            dir: scripts_dir(),
            entries: Vec::new(),
            warnings: Vec::new(),
            dir_error: None,
            stored: Arc::new(std::sync::Mutex::new(stored)),
            settings_dirty: Arc::new(AtomicBool::new(false)),
            run: None,
            show_manager: false,
            queue: AutoQueue::default(),
            autos: Vec::new(),
            auto_journal: VecDeque::new(),
            startup_done: false,
            warned: HashSet::new(),
            pending_write_info: Vec::new(),
        };
        s.reload();
        s
    }

    /// (Re)lit le dossier des scripts (créé s'il manque).
    pub fn reload(&mut self) {
        self.dir_error = std::fs::create_dir_all(&self.dir)
            .err()
            .map(|e| format!("Dossier des scripts impossible à créer ({}) : {e}", self.dir.display()));
        let (entries, warnings) = discover_with_warnings(&self.dir);
        self.entries = entries;
        self.warnings = warnings;
    }

    /// Points d'entrée actifs d'un hook.
    pub fn enabled<'a>(
        &'a self,
        hook: Hook,
        disabled: &'a std::collections::BTreeSet<String>,
    ) -> impl Iterator<Item = &'a ScriptEntry> + 'a {
        self.entries.iter().filter(move |e| e.has_hook(hook) && !disabled.contains(&e.id()))
    }

    /// Point d'entrée d'après son identifiant.
    pub fn entry(&self, id: &str) -> Option<&ScriptEntry> {
        self.entries.iter().find(|e| e.id() == id)
    }

    /// Fonction `ConfigScript` du même fichier que `entry` (réglages partagés).
    pub fn config_of(&self, entry: &ScriptEntry) -> Option<&ScriptEntry> {
        self.entries.iter().find(|e| e.has_hook(Hook::ConfigScript) && e.settings_key() == entry.settings_key())
    }

    pub fn busy(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.running() || r.applying())
    }

    fn options(&self, hook: Hook, python: PathBuf) -> RunOptions {
        RunOptions { python: Some(python), scripts_dir: Some(self.dir.clone()), ..RunOptions::for_hook(hook) }
    }

    /// Lance un script depuis un menu (interactif, aperçu avant application).
    #[allow(clippy::too_many_arguments)]
    pub fn start_run(
        &mut self,
        entry: ScriptEntry,
        hook: Hook,
        book_ids: Vec<String>,
        db: Arc<ComicDatabase>,
        target: ApplyTarget,
        sink: Option<WrittenSink>,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let python = python()?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let host = UiHost::new(
            db.clone(),
            tx,
            ctx.clone(),
            stop.clone(),
            self.stored.clone(),
            self.settings_dirty.clone(),
            None,
        );
        let mut context = RunContext::new(hook, book_ids);
        context.interactive = true;
        context.live = false;
        let count = context.book_ids.len();
        let handle = ScriptRunner::new(self.options(hook, python)).start(&entry, context, Arc::new(host));
        self.run = Some(ScriptRun {
            entry,
            hook,
            count,
            started: Instant::now(),
            handle,
            rx,
            stop,
            db,
            target,
            sink,
            journal: Vec::new(),
            progress: None,
            progress_text: String::new(),
            pending: None,
            result: None,
            preview: None,
            applying: None,
            report: None,
            discarded: false,
            stopping: false,
        });
        Ok(())
    }

    /// Ajoute une ligne au journal des automatismes.
    pub fn auto_log(&mut self, line: String) {
        let stamp = chrono::Local::now().format("%H:%M:%S");
        self.auto_journal.push_back(format!("{stamp} {line}"));
        while self.auto_journal.len() > AUTO_JOURNAL_LINES {
            self.auto_journal.pop_front();
        }
    }

    /// Met en file les automatismes actifs de `hook`.
    pub fn trigger(&mut self, hook: Hook, book_ids: Vec<String>, disabled: &std::collections::BTreeSet<String>) {
        let jobs: Vec<AutoJob> = self
            .enabled(hook, disabled)
            .map(|e| AutoJob { entry: e.clone(), hook, book_ids: book_ids.clone() })
            .collect();
        for job in jobs {
            let name = job.entry.name.clone();
            if !self.queue.push(job) {
                self.auto_log(format!("« {name} » ({}) ignoré : trop d'automatismes en attente", hook.name()));
            }
        }
    }

    /// Lance les automatismes prêts (`allowed` faux : transfert en cours, ils attendent).
    pub fn start_autos(
        &mut self,
        allowed: bool,
        db: Option<Arc<ComicDatabase>>,
        target: &ApplyTarget,
        sink: Option<WrittenSink>,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let Some(db) = db else { return Ok(()) };
        let jobs = self.queue.start_ready(allowed);
        if jobs.is_empty() {
            return Ok(());
        }
        let python = match python() {
            Ok(p) => p,
            Err(e) => {
                for j in &jobs {
                    self.queue.finished(j.hook);
                }
                self.queue.clear_waiting();
                return Err(e);
            }
        };
        for job in jobs {
            let (tx, rx) = mpsc::channel();
            let live = Live { target: target.clone(), sink: sink.clone() };
            let host = UiHost::new(
                db.clone(),
                tx,
                ctx.clone(),
                Arc::new(AtomicBool::new(false)),
                self.stored.clone(),
                self.settings_dirty.clone(),
                Some(live),
            );
            let context = RunContext::new(job.hook, job.book_ids.clone());
            self.auto_log(format!(
                "« {} » ({}) lancé{}",
                job.entry.name,
                job.hook.name(),
                if job.book_ids.is_empty() { String::new() } else { format!(" sur {} livre(s)", job.book_ids.len()) }
            ));
            let handle =
                ScriptRunner::new(self.options(job.hook, python.clone())).start(&job.entry, context, Arc::new(host));
            self.autos.push(AutoRun { job, handle, rx });
        }
        Ok(())
    }

    /// Événements et fins des automatismes.
    pub fn poll_autos(&mut self) -> Vec<AutoEvent> {
        let mut out = Vec::new();
        let mut lines = Vec::new();
        let mut i = 0;
        while i < self.autos.len() {
            let a = &mut self.autos[i];
            let name = a.job.entry.name.clone();
            for ev in a.rx.try_iter().collect::<Vec<_>>() {
                match ev {
                    HostEvent::Log(l) => {
                        if l.kind != JournalKind::Output || !l.text.is_empty() {
                            lines.push(format!("[{name}] {}", l.text));
                        }
                    }
                    HostEvent::Applied(o) => out.push(AutoEvent::Applied(name.clone(), *o)),
                    HostEvent::LiveRefused(m) => out.push(AutoEvent::Refused(name.clone(), m)),
                    // Automatisme : jamais interactif (le runner ne pose pas de question).
                    HostEvent::Ask { .. } | HostEvent::Message { .. } | HostEvent::Progress(..) => {}
                }
            }
            if let Some(r) = a.handle.poll() {
                let r = r.clone();
                let a = self.autos.remove(i);
                self.queue.finished(a.job.hook);
                let state = match r.state {
                    RunState::Finished => "terminé".to_string(),
                    RunState::Cancelled => "arrêté".to_string(),
                    RunState::TimedOut => "arrêté (délai dépassé)".to_string(),
                    RunState::Failed => {
                        format!("en erreur : {}", r.failure.as_ref().map(|f| f.to_string()).unwrap_or_default())
                    }
                };
                lines.push(format!("« {} » ({}) {state}", a.job.entry.name, a.job.hook.name()));
                out.push(AutoEvent::Finished(a.job, r));
                continue;
            }
            i += 1;
        }
        for l in lines {
            self.auto_log(l);
        }
        out
    }

    /// Arrête les automatismes en cours et oublie ceux en attente.
    pub fn stop_autos(&mut self) {
        self.queue.clear_waiting();
        for a in &self.autos {
            a.handle.cancel();
        }
    }
}
