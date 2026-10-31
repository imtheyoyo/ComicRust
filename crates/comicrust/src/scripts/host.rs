//! `ScriptHost` de l'interface : le script tourne dans son propre fil (et son processus
//! Python), l'interface dans le sien. Tout ce qui doit être montré passe par un canal
//! ([`HostEvent`]) lu à chaque image ; une question bloque le fil du script jusqu'à la
//! réponse (ou l'arrêt du script : valeur par défaut).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{
    book_json, AppliedRename, AppliedRenames, ChooseSpec, FormAnswer, FormSpec, JournalLine, ProgressDetails, Question,
    ScriptChange, ScriptHost, ScriptSettings,
};
use eframe::egui::{self, ColorImage};
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

/// Enregistre le bilan des renommages **réussis** d'une application (fenêtre d'aperçu →
/// « Appliquer ») dans `dir`, pour `comicrust.applied_renames()` : seulement si au moins
/// un fichier a été renommé (une application sans renommage ne fait pas oublier le
/// rangement précédent). Rend le fichier écrit.
pub fn record_applied_renames(
    dir: &Path,
    entry: &comicrust_core::scripting::ScriptEntry,
    outcome: &ApplyOutcome,
) -> std::io::Result<Option<PathBuf>> {
    let renames: Vec<AppliedRename> = outcome
        .moves
        .iter()
        .map(|m| AppliedRename {
            book: m.book.id.clone(),
            from: m.moved.from.to_string_lossy().into_owned(),
            to: m.moved.to.to_string_lossy().into_owned(),
        })
        .collect();
    if renames.is_empty() {
        return Ok(None);
    }
    AppliedRenames::new(entry, renames).save(dir).map(Some)
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
    /// Formulaire (images déjà décodées, par identifiant de contrôle) ; `None` = annulé.
    Form {
        spec: Box<FormSpec>,
        images: HashMap<usize, ColorImage>,
        reply: Sender<Option<FormAnswer>>,
    },
    /// Liste de choix (vignettes déjà décodées) ; `None` = annulé.
    Choose {
        spec: Box<ChooseSpec>,
        images: Vec<Option<ColorImage>>,
        reply: Sender<Option<Vec<usize>>>,
    },
    /// Sous-texte et image de la progression (`None` : inchangés ; image `Some(None)` :
    /// retirée).
    ProgressDetails {
        detail: Option<String>,
        image: Option<Option<ColorImage>>,
    },
    /// Adresse http(s) vérifiée à ouvrir dans le navigateur.
    OpenUrl(String),
    /// Texte à copier dans le presse-papiers.
    Clipboard(String),
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
    /// Arrêt demandé (utilisateur, fermeture de ComicRust) : une question en cours rend la
    /// valeur par défaut ; une application au fil de l'eau s'arrête entre deux livres.
    stop: Arc<AtomicBool>,
    stored: SharedSettings,
    settings_dirty: Arc<AtomicBool>,
    live: Option<Live>,
    /// Dossier des bilans de renommages appliqués (`data\renommages\`), relus par
    /// `comicrust.applied_renames()` ; `None` : aucun bilan.
    renames_dir: Option<PathBuf>,
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
        UiHost { db, fresh: Mutex::new(HashMap::new()), tx, ctx, stop, stored, settings_dirty, live, renames_dir: None }
    }

    /// Dossier où relire les bilans de renommages appliqués ([`AppliedRenames`]).
    pub fn with_renames_dir(mut self, dir: PathBuf) -> UiHost {
        self.renames_dir = Some(dir);
        self
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

    /// Toute la bibliothèque au lancement du script (lecture seule pour le script : le
    /// cœur ne laisse modifier que les livres du contexte).
    fn library_ids(&self) -> Option<Vec<String>> {
        Some(self.db.books.iter().map(|b| b.id.clone()).collect())
    }

    fn applied_renames(&self, settings_key: &str) -> Value {
        self.renames_dir
            .as_deref()
            .and_then(|d| AppliedRenames::load(d, settings_key))
            .and_then(|b| serde_json::to_value(b).ok())
            .unwrap_or(Value::Null)
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

    fn progress_details(&self, d: &ProgressDetails) {
        // Décodage ici, sur le fil du script : l'interface ne se fige pas.
        let image = d
            .image
            .as_ref()
            .map(|img| img.as_ref().and_then(|b| super::forms::decode_image(b, super::forms::PROGRESS_IMAGE_SIDE)));
        self.send(HostEvent::ProgressDetails { detail: d.detail.clone(), image });
    }

    fn form(&self, spec: &FormSpec) -> Option<FormAnswer> {
        let images = spec
            .images()
            .into_iter()
            .filter_map(|(id, b)| super::forms::decode_image(&b, super::forms::FORM_IMAGE_SIDE).map(|i| (id, i)))
            .collect();
        let (reply, rx) = mpsc::channel();
        self.send(HostEvent::Form { spec: Box::new(spec.clone()), images, reply });
        self.wait(rx).flatten()
    }

    fn choose(&self, spec: &ChooseSpec) -> Option<Vec<usize>> {
        let images = spec
            .images
            .iter()
            .map(|b| b.as_ref().and_then(|b| super::forms::decode_image(b, super::forms::CHOOSE_IMAGE_SIDE)))
            .collect();
        let (reply, rx) = mpsc::channel();
        self.send(HostEvent::Choose { spec: Box::new(spec.clone()), images, reply });
        self.wait(rx).flatten()
    }

    fn open_url(&self, url: &str) -> bool {
        self.send(HostEvent::OpenUrl(url.to_string()));
        true
    }

    fn clipboard_set(&self, text: &str) -> bool {
        self.send(HostEvent::Clipboard(text.to_string()));
        true
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
        let outcome = apply_plan(&live.target, &plan, &self.stop);
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

    /// Formulaire et liste de choix par le canal de l'interface (images décodées sur le fil
    /// du script), puis arrêt pendant un formulaire : le script reçoit None.
    #[test]
    fn formulaire_choix_et_arret_pendant_un_formulaire() {
        if scripting_python().is_none() {
            eprintln!("python3 absent : test ignoré");
            return;
        }
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(600, 900, image::Rgba([0, 90, 200, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("couv.png"), &png).unwrap();
        let code = "import comicrust\nfrom comicrust import ui\n\n#@Name F\n#@Hook Library\ndef f(books):\n    \
                    r = comicrust.form('Essai', [ui.text('nom', 'Nom', 'x'), ui.image('couv.png')])\n    \
                    print('formulaire', r)\n    \
                    c = comicrust.choose('Album', ['Titre'], ['A', 'B'], images=['couv.png', None])\n    \
                    print('choix', c)\n    \
                    r = comicrust.form('Second', [ui.checkbox('c', 'Case')])\n    \
                    print('second', r)\n    \
                    import time\n    \
                    while not comicrust.cancelled():\n        time.sleep(0.05)\n";
        let file = dir.path().join("f.py");
        std::fs::write(&file, code).unwrap();
        let entry = parse_script(code, &file, "f.py").0.remove(0);
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
        let handle = ScriptRunner::new(opts).start(&entry, RunContext::new(Hook::Library, vec![]), Arc::new(host));
        let next = || loop {
            match rx.recv_timeout(Duration::from_secs(20)).expect("fenêtre attendue") {
                ev @ (HostEvent::Form { .. } | HostEvent::Choose { .. }) => return ev,
                _ => continue,
            }
        };
        let HostEvent::Form { spec, images, reply } = next() else { panic!("formulaire attendu") };
        assert_eq!(spec.title, "Essai");
        let img = images.values().next().expect("image décodée");
        assert_eq!(img.size, [600, 900], "image de formulaire gardée jusqu'à 1024 px");
        let mut st = super::super::forms::FormState::new(*spec, images);
        st.fields.insert("nom".into(), super::super::forms::Field::Text("Yoyo".into()));
        reply.send(st.submit("OK")).unwrap();
        let HostEvent::Choose { spec, images, reply } = next() else { panic!("liste de choix attendue") };
        assert_eq!(spec.rows.len(), 2);
        assert_eq!(images[0].as_ref().unwrap().size, [320, 480], "vignette réduite");
        assert!(images[1].is_none());
        reply.send(Some(vec![1])).unwrap();
        // Second formulaire : l'utilisateur arrête le script au lieu de répondre.
        let HostEvent::Form { reply, .. } = next() else { panic!("formulaire attendu") };
        stop.store(true, Ordering::SeqCst);
        handle.cancel();
        drop(reply);
        let r = handle.wait();
        assert!(matches!(r.state, RunState::Cancelled | RunState::Finished), "{:?}", r.state);
        let text = r.journal_text();
        assert!(text.contains("formulaire ('OK', {'nom': 'Yoyo'})"), "{text}");
        assert!(text.contains("choix 1"), "{text}");
        assert!(text.contains("second None"), "{text}");
    }

    /// Bilan des renommages appliqués : écrit seulement si un fichier a été renommé, relu
    /// par `applied_renames` (clé du fichier de script) ; `library_ids` rend toute la base.
    #[test]
    fn bilan_des_renommages_appliques_et_bibliotheque() {
        use comicrust_core::edit::files::MovedFile;
        use comicrust_core::edit::MoveReport;
        let dir = tempfile::tempdir().unwrap();
        let code = "#@Name LO\n#@Hook Books\n#@Key lo\ndef ranger(books):\n    pass\n";
        let entry = parse_script(code, Path::new("/s/lo.py"), "exemples/lo.py").0.remove(0);
        // Rien de renommé (tout refusé) : pas de bilan, le précédent reste valable.
        let mut outcome = ApplyOutcome::default();
        outcome.rename_errors.push(("b".into(), "destination occupée".into()));
        assert!(record_applied_renames(dir.path(), &entry, &outcome).unwrap().is_none());
        let book = LibraryBook { id: "a".into(), file: Some("/BD/a.cbz".into()), ..Default::default() };
        outcome.moves.push(MoveReport {
            moved: MovedFile { from: "/in/a.cbz".into(), to: "/BD/a.cbz".into(), copied: false, created_dirs: vec![] },
            book: book.clone(),
            store_counter: None,
            comicrack: None,
        });
        let f = record_applied_renames(dir.path(), &entry, &outcome).unwrap().expect("bilan écrit");
        assert!(f.starts_with(dir.path()));

        let mut db = ComicDatabase::default();
        db.books.push(book);
        let host = UiHost::new(
            Arc::new(db),
            mpsc::channel().0,
            egui::Context::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            None,
        );
        assert_eq!(host.applied_renames("exemples/lo.py"), Value::Null, "dossier non donné : aucun bilan");
        assert_eq!(host.library_ids(), Some(vec!["a".to_string()]));
        let host = host.with_renames_dir(dir.path().to_path_buf());
        let v = host.applied_renames("exemples/lo.py");
        assert_eq!(v["entry"], "lo");
        assert_eq!(v["renames"], serde_json::json!([{"book": "a", "from": "/in/a.cbz", "to": "/BD/a.cbz"}]));
        assert_eq!(host.applied_renames("exemples/autre.py"), Value::Null);
    }

    fn scripting_python() -> Option<std::path::PathBuf> {
        comicrust_core::scripting::find_python(Some(std::path::Path::new("/nulle-part"))).ok()
    }
}
