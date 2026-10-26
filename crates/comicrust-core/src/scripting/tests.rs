//! Tests de l'hôte de scripts (avec le `python3` du système ; ignorés s'il manque).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::*;
use crate::library::LibraryBook;

// ---------------------------------------------------------------------------------------
// Découverte
// ---------------------------------------------------------------------------------------

#[test]
fn en_tetes_plusieurs_fonctions() {
    let text = "\u{feff}# -*- coding: utf-8 -*-\nimport comicrust\n\n\
        #@Name Nettoyer les séries\n#@Hook Books, Library\n#@Description Première ligne\n\
        #@Description Deuxième ligne\n#@Key nettoyer\n#@Image icone.png\n\
        def nettoyer(books):\n    def interne(x):\n        pass\n\n\
        def outil():\n    pass\n\n\
        # @Name Réglages\n#   @Hook ConfigScript\ndef reglages():\n    pass\n";
    let (e, w) = parse_script(text, Path::new("/s/rangement/a.py"), "rangement/a.py");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].name, "Nettoyer les séries");
    assert_eq!(e[0].function, "nettoyer");
    assert_eq!(e[0].hooks, vec![Hook::Books, Hook::Library]);
    assert_eq!(e[0].description.as_deref(), Some("Première ligne\nDeuxième ligne"));
    assert_eq!(e[0].id(), "nettoyer");
    assert_eq!(e[0].image, Some(PathBuf::from("/s/rangement/icone.png")));
    assert_eq!(e[1].function, "reglages");
    assert_eq!(e[1].hooks, vec![Hook::ConfigScript]);
    assert_eq!(e[1].id(), "rangement/a.py:reglages");
    assert_eq!(e[0].settings_key(), e[1].settings_key());
}

#[test]
fn en_tetes_mal_formes() {
    let text = "#@Name Sans hook\ndef a():\n    pass\n\
        #@Hook Books\ndef b():\n    pass\n\
        #@Name Hook inconnu\n#@Hook Truc\ndef c():\n    pass\n\
        #@Name\n#@Hook Books\n#@Couleur bleu\ndef d():\n    pass\n\
        #@Name Asynchrone\n#@Hook Books\nasync def e():\n    pass\n\
        #@Name Orphelin\n#@Hook Books\n";
    let (e, w) = parse_script(text, Path::new("x.py"), "x.py");
    assert!(e.is_empty(), "{e:?}");
    let msgs: Vec<&str> = w.iter().map(|w| w.message.as_str()).collect();
    let has = |s: &str| msgs.iter().any(|m| m.contains(s));
    assert!(has("#@Hook manquant"), "{msgs:?}");
    assert!(has("#@Name manquant"), "{msgs:?}");
    assert!(has("hook inconnu « Truc »"), "{msgs:?}");
    assert!(has("aucun hook reconnu"), "{msgs:?}");
    assert!(has("sans valeur"), "{msgs:?}");
    assert!(has("en-tête inconnu « #@Couleur »"), "{msgs:?}");
    assert!(has("async"), "{msgs:?}");
    assert!(has("sans fonction"), "{msgs:?}");
}

/// `#@Enabled false` : point d'entrée désactivé par défaut (ignoré par `entries_for_hook`).
#[test]
fn en_tete_enabled() {
    let text = "#@Name A\n#@Hook NewBooks\n#@Enabled false\ndef a(books): pass\n\
        #@Name B\n#@Hook NewBooks\n#@Enabled True\ndef b(books): pass\n\
        #@Name C\n#@Hook NewBooks\ndef c(books): pass\n\
        #@Name D\n#@Hook NewBooks\n#@Enabled peut-être\ndef d(books): pass\n";
    let (e, w) = parse_script(text, Path::new("x.py"), "x.py");
    let states: Vec<(&str, bool)> = e.iter().map(|e| (e.function.as_str(), e.enabled_by_default)).collect();
    assert_eq!(states, [("a", false), ("b", true), ("c", true), ("d", true)]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].message.contains("#@Enabled"));
    let aucun = HashSet::new();
    let actifs: Vec<&str> = entries_for_hook(&e, Hook::NewBooks, &aucun).map(|e| e.function.as_str()).collect();
    assert_eq!(actifs, ["b", "c", "d"]);
}

#[test]
fn decouverte_dossier() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::create_dir_all(d.join("sous")).unwrap();
    std::fs::create_dir_all(d.join("lib")).unwrap();
    std::fs::create_dir_all(d.join("__pycache__")).unwrap();
    let s = "#@Name A\n#@Hook Books\ndef a(books): pass\n";
    std::fs::write(d.join("un.py"), s).unwrap();
    std::fs::write(d.join("sous").join("deux.py"), format!("\u{feff}{s}")).unwrap();
    std::fs::write(d.join("lib").join("outil.py"), s).unwrap();
    std::fs::write(d.join("__pycache__").join("x.py"), s).unwrap();
    std::fs::write(d.join("notes.txt"), s).unwrap();
    // Latin-1 : accepté avec avertissement.
    std::fs::write(d.join("latin.py"), b"#@Name Caf\xe9\n#@Hook Library\ndef c(): pass\n").unwrap();
    let (e, w) = discover_with_warnings(d);
    let rels: Vec<&str> = e.iter().map(|e| e.rel_path.as_str()).collect();
    assert_eq!(rels, vec!["latin.py", "sous/deux.py", "un.py"]);
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("UTF-8"));
    let disabled: HashSet<String> = ["un.py:a".to_string()].into();
    let books: Vec<_> = entries_for_hook(&e, Hook::Books, &disabled).map(|e| e.rel_path.clone()).collect();
    assert_eq!(books, vec!["sous/deux.py"]);
    assert!(discover(&d.join("absent")).is_empty());
}

// ---------------------------------------------------------------------------------------
// Exécution
// ---------------------------------------------------------------------------------------

fn python() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: &[&str] = if cfg!(windows) { &["python.exe", "python3.exe"] } else { &["python3"] };
    std::env::split_paths(&path).flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

macro_rules! need_python {
    () => {
        match python() {
            Some(p) => p,
            None => {
                eprintln!("python3 absent : test ignoré");
                return;
            }
        }
    };
}

#[derive(Default)]
struct TestHost {
    books: HashMap<String, Value>,
    answers: Mutex<Vec<Value>>,
    questions: Mutex<Vec<Question>>,
    messages: Mutex<Vec<String>>,
    progress: Mutex<Vec<(Option<f32>, String)>>,
    settings: Mutex<ScriptSettings>,
    saved: Mutex<Vec<(String, ScriptSettings)>>,
    live: Mutex<Vec<ScriptChange>>,
    book_requests: Mutex<usize>,
    /// Formulaires reçus et réponses à donner (dans l'ordre).
    forms: Mutex<Vec<FormSpec>>,
    form_answers: Mutex<Vec<Option<FormAnswer>>>,
    chooses: Mutex<Vec<ChooseSpec>>,
    choose_answers: Mutex<Vec<Option<Vec<usize>>>>,
    urls: Mutex<Vec<String>>,
    clipboard: Mutex<Vec<String>>,
    details: Mutex<Vec<ProgressDetails>>,
}

impl ScriptHost for TestHost {
    fn books(&self, ids: &[String]) -> Vec<Value> {
        *self.book_requests.lock().unwrap() += 1;
        ids.iter().filter_map(|i| self.books.get(i).cloned()).collect()
    }
    fn ask(&self, q: &Question) -> Option<Value> {
        self.questions.lock().unwrap().push(q.clone());
        let mut a = self.answers.lock().unwrap();
        (!a.is_empty()).then(|| a.remove(0))
    }
    fn message(&self, text: &str) {
        self.messages.lock().unwrap().push(text.into());
    }
    fn progress(&self, f: Option<f32>, text: &str) {
        self.progress.lock().unwrap().push((f, text.into()));
    }
    fn load_settings(&self, _key: &str) -> ScriptSettings {
        self.settings.lock().unwrap().clone()
    }
    fn save_settings(&self, key: &str, s: &ScriptSettings) -> Result<(), String> {
        self.saved.lock().unwrap().push((key.into(), s.clone()));
        Ok(())
    }
    fn apply_live(&self, changes: &[ScriptChange]) -> Result<(), String> {
        self.live.lock().unwrap().extend_from_slice(changes);
        Ok(())
    }
    fn progress_details(&self, d: &ProgressDetails) {
        self.details.lock().unwrap().push(d.clone());
    }
    fn form(&self, f: &FormSpec) -> Option<FormAnswer> {
        self.forms.lock().unwrap().push(f.clone());
        let mut a = self.form_answers.lock().unwrap();
        if a.is_empty() {
            None
        } else {
            a.remove(0)
        }
    }
    fn choose(&self, c: &ChooseSpec) -> Option<Vec<usize>> {
        self.chooses.lock().unwrap().push(c.clone());
        let mut a = self.choose_answers.lock().unwrap();
        if a.is_empty() {
            None
        } else {
            a.remove(0)
        }
    }
    fn open_url(&self, url: &str) -> bool {
        self.urls.lock().unwrap().push(url.into());
        true
    }
    fn clipboard_set(&self, text: &str) -> bool {
        self.clipboard.lock().unwrap().push(text.into());
        true
    }
}

mod forms;

fn library(n: usize) -> TestHost {
    let mut host = TestHost::default();
    for i in 0..n {
        let b = LibraryBook {
            id: format!("id-{i}"),
            file: Some(format!("/bd/Serie {}/Livre {i}.cbz", i % 10)),
            series: Some(format!("  Série   {}  ", i % 10)),
            number: Some((i + 1).to_string()),
            year: Some(2000 + (i % 20) as i32),
            language_iso: Some("fr".into()),
            ..Default::default()
        };
        host.books.insert(b.id.clone(), book_json(&b));
    }
    host
}

struct Fixture {
    dir: tempfile::TempDir,
    python: PathBuf,
}

impl Fixture {
    fn new(python: PathBuf) -> Fixture {
        Fixture { dir: tempfile::tempdir().unwrap(), python }
    }

    fn options(&self) -> RunOptions {
        RunOptions {
            python: Some(self.python.clone()),
            module_dir: Some(self.dir.path().join("module")),
            scripts_dir: Some(self.dir.path().join("scripts")),
            grace: Duration::from_millis(2500),
            ..Default::default()
        }
    }

    /// Écrit le script et renvoie son point d'entrée `fonction`.
    fn script(&self, code: &str, function: &str, hook: Hook) -> ScriptEntry {
        let scripts = self.dir.path().join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        let file = scripts.join("essai.py");
        std::fs::write(&file, code).unwrap();
        let (e, w) = parse_script(code, &file, "essai.py");
        if let Some(e) = e.into_iter().find(|e| e.function == function) {
            return e;
        }
        assert!(w.is_empty(), "{w:?}");
        ScriptEntry {
            file,
            rel_path: "essai.py".into(),
            function: function.into(),
            name: function.into(),
            hooks: vec![hook],
            key: None,
            description: None,
            image: None,
            line: 1,
            enabled_by_default: true,
        }
    }

    fn run(&self, code: &str, function: &str, ctx: RunContext, host: Arc<TestHost>) -> RunResult {
        self.run_with(code, function, ctx, host, self.options())
    }

    fn run_with(&self, code: &str, f: &str, ctx: RunContext, host: Arc<TestHost>, o: RunOptions) -> RunResult {
        let entry = self.script(code, f, ctx.hook);
        ScriptRunner::new(o).start(&entry, ctx, host).wait()
    }
}

fn ids(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("id-{i}")).collect()
}

#[test]
fn lecture_et_modifications_en_attente() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(library(5));
    let code = r#"
import comicrust

#@Name Nettoyer
#@Hook Books
def nettoyer(books):
    print("début", len(books))
    import sys, os
    sys.stdout.write("écrit sur stdout\n")
    os.write(1, b"ecrit sur le descripteur 1\n")
    for i, b in enumerate(books):
        comicrust.progress(i / len(books), b.series)
        b.series = " ".join(b.series.split())
        assert b["Series"] == b.series
        assert b.language_iso == "fr" and b["LanguageISO"] == "fr"
        assert b.title is None
        b["Title"] = "T" + b.number
        b.set("Year", b.year + 1)
        b.writer = ["Moebius", "Jodorowsky"]
    first = books[0]
    comicrust.rename_file(first, "nouveau.cbz")
    comicrust.write_info(first)
    comicrust.log("fini", 42)
    print(comicrust.format_name("{series}/{series} #{number:03} ({year}) {absent}", first))
    return {"vu": len(books)}
"#;
    let r = fx.run(code, "nettoyer", RunContext::new(Hook::Books, ids(5)), host.clone());
    assert_eq!(r.state, RunState::Finished, "{:?}\n{}", r.failure, r.journal_text());
    assert_eq!(r.return_value, json!({"vu": 5}));
    assert_eq!(r.rejected, vec![]);
    let journal = r.journal_text();
    assert!(journal.contains("début 5"), "{journal}");
    assert!(journal.contains("écrit sur stdout"), "{journal}");
    assert!(journal.contains("ecrit sur le descripteur 1"), "{journal}");
    assert!(journal.contains("fini 42"), "{journal}");
    assert!(journal.contains("Série 0/Série 0 #001 (2001)"), "{journal}");
    let by = r.changes_by_book();
    assert_eq!(by.len(), 5);
    let b0 = &by["id-0"];
    assert_eq!(b0.fields["Series"], json!("Série 0"));
    assert_eq!(b0.fields["Title"], json!("T1"));
    assert_eq!(b0.fields["Year"], json!(2001));
    assert_eq!(b0.fields["Writer"], json!("Moebius, Jodorowsky"));
    assert_eq!(b0.rename.as_deref(), Some("nouveau.cbz"));
    assert!(b0.write_info);
    assert!(!by["id-1"].write_info);
    assert!(!r.applied_live);
    assert!(host.live.lock().unwrap().is_empty());
    assert_eq!(host.progress.lock().unwrap().len(), 5);
}

#[test]
fn exception_python_lisible() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let code = "def aide(x):\n    return 1 / x\n\n#@Name E\n#@Hook Library\ndef f(books):\n    aide(0)\n";
    let r = fx.run(code, "f", RunContext::new(Hook::Library, vec![]), Arc::new(TestHost::default()));
    assert_eq!(r.state, RunState::Failed);
    let f = r.failure.clone().unwrap();
    assert_eq!(f.kind, "ZeroDivisionError");
    assert!(f.traceback.contains("essai.py\", line 2"), "{}", f.traceback);
    assert!(f.traceback.contains("aide(0)"), "{}", f.traceback);
    assert!(!f.traceback.contains("comicrust.py"), "{}", f.traceback);
    assert!(r.journal_text().contains("ZeroDivisionError"));

    // Erreur de syntaxe, fonction absente, sys.exit(0).
    let r = fx.run("def f(:\n", "f", RunContext::new(Hook::Library, vec![]), Arc::new(TestHost::default()));
    assert_eq!(r.failure.unwrap().kind, "SyntaxError");
    let r = fx.run("x = 1\n", "f", RunContext::new(Hook::Library, vec![]), Arc::new(TestHost::default()));
    assert!(r.failure.unwrap().message.contains("introuvable"));
    let r = fx.run(
        "import sys\ndef f():\n    sys.exit(0)\n",
        "f",
        RunContext::new(Hook::Startup, vec![]),
        Arc::new(TestHost::default()),
    );
    assert_eq!(r.state, RunState::Finished, "{:?}", r.failure);
}

#[test]
fn champ_non_modifiable() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(library(1));
    let r = fx.run("def f(books):\n    books[0].file = 'x'\n", "f", RunContext::new(Hook::Books, ids(1)), host.clone());
    assert_eq!(r.failure.unwrap().kind, "AttributeError");
    let r = fx.run("def f(books):\n    books[0].inconnu = 'x'\n", "f", RunContext::new(Hook::Books, ids(1)), host);
    assert!(r.failure.unwrap().message.contains("champ inconnu"));
}

#[test]
fn livre_hors_contexte_refuse() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(library(10));
    let code = r#"
import comicrust
def f(books):
    comicrust.rename_file("id-7", "vol.cbz")
    comicrust.write_info("id-8")
    comicrust.rename_file(books[0], "ok.cbz")
    comicrust._queue({"op": "set", "book": "id-0", "field": "PageCount", "value": 3})
    comicrust._queue({"op": "set", "book": "id-0", "field": "Series", "value": [1]})
    comicrust._queue({"op": "efface", "book": "id-0"})
    assert comicrust.flush() == 1
    try:
        comicrust._session().ch.request("books", ids=["id-9"])
    except comicrust.ScriptError as e:
        print("refus :", e)
"#;
    let r = fx.run(code, "f", RunContext::new(Hook::Books, ids(2)), host);
    assert_eq!(r.state, RunState::Finished, "{:?}\n{}", r.failure, r.journal_text());
    assert_eq!(r.changes, vec![ScriptChange::Rename { book_id: "id-0".into(), new_path: "ok.cbz".into() }]);
    assert_eq!(r.rejected.len(), 5);
    assert!(r.rejected[0].reason.contains("hors du contexte"));
    assert!(r.rejected[2].reason.contains("non modifiable"));
    assert!(r.journal_text().contains("refus : livre id-9 hors du contexte"), "{}", r.journal_text());
}

#[test]
fn annulation_boucle_infinie() {
    let py = need_python!();
    let fx = Fixture::new(py);
    // Boucle Python pure : interrompue par KeyboardInterrupt.
    let entry = fx.script("def f():\n    while True:\n        pass\n", "f", Hook::Library);
    let h = ScriptRunner::new(fx.options()).start(
        &entry,
        RunContext::new(Hook::Library, vec![]),
        Arc::new(TestHost::default()),
    );
    std::thread::sleep(Duration::from_millis(600));
    assert!(!h.is_finished());
    let t = Instant::now();
    h.cancel();
    let r = h.wait();
    assert_eq!(r.state, RunState::Cancelled, "{:?}", r.failure);
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());

    // Appel bloquant (sleep) : arrêt forcé après le délai de grâce.
    let entry = fx.script("import time\ndef f():\n    time.sleep(120)\n", "f", Hook::Library);
    let h = ScriptRunner::new(fx.options()).start(
        &entry,
        RunContext::new(Hook::Library, vec![]),
        Arc::new(TestHost::default()),
    );
    std::thread::sleep(Duration::from_millis(600));
    let t = Instant::now();
    h.cancel();
    let r = h.wait();
    assert_eq!(r.state, RunState::Cancelled);
    assert!(t.elapsed() < Duration::from_secs(6), "{:?}", t.elapsed());

    // Annulation coopérative : le script teste cancelled() et garde ses modifications.
    let host = Arc::new(library(3));
    let code = "import comicrust, time\ndef f(books):\n    books[0].title = 'a'\n    while not comicrust.cancelled():\n        time.sleep(0.01)\n";
    let entry = fx.script(code, "f", Hook::Books);
    let h = ScriptRunner::new(fx.options()).start(&entry, RunContext::new(Hook::Books, ids(3)), host);
    std::thread::sleep(Duration::from_millis(600));
    h.cancel();
    let r = h.wait();
    assert_eq!(r.state, RunState::Finished, "{:?}", r.failure);
    assert_eq!(r.changes.len(), 1);
}

#[test]
fn delai_depasse() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let mut o = fx.options();
    o.timeout = Some(Duration::from_millis(700));
    let r = fx.run_with(
        "def f():\n    while True:\n        pass\n",
        "f",
        RunContext::new(Hook::Startup, vec![]),
        Arc::new(TestHost::default()),
        o,
    );
    assert_eq!(r.state, RunState::TimedOut);
    assert!(r.failure.unwrap().message.contains("délai"));
}

#[test]
fn questions_et_messages() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let code = r#"
import comicrust
def f(books):
    a = comicrust.ask("Clé ?", "défaut")
    b = comicrust.ask_choice("Quel ?", ["un", "deux"], "un")
    c = comicrust.confirm("Sûr ?", False)
    d = comicrust.ask("Encore ?", "rien")
    comicrust.message("Bonjour")
    return [a, b, c, d, comicrust.interactive(), comicrust.hook()]
"#;
    let host = Arc::new(TestHost::default());
    // « trois » n'est pas un choix proposé : valeur par défaut.
    *host.answers.lock().unwrap() = vec![json!("ma clé"), json!("trois"), json!(true)];
    let r = fx.run(code, "f", RunContext::new(Hook::Books, vec![]), host.clone());
    assert_eq!(r.return_value, json!(["ma clé", "un", true, "rien", true, "Books"]), "{:?}", r.failure);
    let q = host.questions.lock().unwrap();
    assert_eq!(q.len(), 4);
    assert_eq!(q[1].kind, QuestionKind::Choice);
    assert_eq!(q[1].choices, vec!["un", "deux"]);
    assert_eq!(*host.messages.lock().unwrap(), vec!["Bonjour"]);

    // Automatisme : pas de question, valeurs par défaut, message au journal.
    let host = Arc::new(TestHost::default());
    *host.answers.lock().unwrap() = vec![json!("x")];
    let r = fx.run(code, "f", RunContext::new(Hook::NewBooks, vec![]), host.clone());
    assert_eq!(r.return_value, json!(["défaut", "un", false, "rien", false, "NewBooks"]));
    assert!(host.questions.lock().unwrap().is_empty());
    assert!(r.journal_text().contains("Bonjour"));
}

#[test]
fn reglages_du_script() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(TestHost::default());
    host.settings.lock().unwrap().values.insert("dossier".into(), json!("/bd"));
    let code = r#"
import comicrust
#@Name Réglages
#@Hook ConfigScript
def config():
    s = comicrust.settings()
    assert s["dossier"] == "/bd"
    s["api_key"] = "abc123"
    s.mark_secret("api_key")
"#;
    let r = fx.run(code, "config", RunContext::new(Hook::ConfigScript, vec![]), host.clone());
    assert_eq!(r.state, RunState::Finished, "{:?}", r.failure);
    let saved = host.saved.lock().unwrap();
    assert_eq!(saved.len(), 1, "enregistrement automatique en fin de script");
    let (key, s) = &saved[0];
    assert_eq!(key, "essai.py");
    assert_eq!(s.values["api_key"], json!("abc123"));
    assert!(s.secret.contains("api_key"));
    // Forme enregistrée : jamais le secret en clair.
    let stored = s.to_stored();
    assert!(!stored.to_string().contains("abc123"), "{stored}");
    assert_eq!(stored["dossier"], json!("/bd"));
    let back = ScriptSettings::from_stored(&stored);
    assert!(back.secret.contains("api_key"));
    if cfg!(windows) {
        assert_eq!(back.values["api_key"], json!("abc123"));
    }
}

#[test]
fn automatisme_au_fil_de_l_eau() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(library(3));
    let code = "#@Name Ouvert\n#@Hook BookOpened\ndef ouvert(book):\n    book.notes = 'lu ' + book.id\n";
    let r = fx.run(code, "ouvert", RunContext::new(Hook::BookOpened, vec!["id-2".into()]), host.clone());
    assert_eq!(r.state, RunState::Finished, "{:?}", r.failure);
    assert!(r.applied_live);
    let live = host.live.lock().unwrap();
    assert_eq!(
        *live,
        vec![ScriptChange::SetField { book_id: "id-2".into(), field: "Notes".into(), value: json!("lu id-2") }]
    );
    assert_eq!(r.changes, *live);
}

#[test]
fn grosse_bibliotheque() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(library(5_000));
    let code = r#"
import comicrust
def f(books):
    n = 0
    for b in books:
        b.series = b.series.strip()
        n += 1
    assert books[4999].id == "id-4999"
    return n
"#;
    let t = Instant::now();
    let r = fx.run(code, "f", RunContext::new(Hook::Library, ids(5_000)), host.clone());
    let elapsed = t.elapsed();
    assert_eq!(r.state, RunState::Finished, "{:?}", r.failure);
    assert_eq!(r.return_value, json!(5_000));
    assert_eq!(r.changes.len(), 5_000);
    assert_eq!(*host.book_requests.lock().unwrap(), 10, "lecture par lots de 500");
    assert!(elapsed < Duration::from_secs(30), "{elapsed:?}");
    eprintln!("5 000 livres : {elapsed:?}");
}

#[test]
fn message_trop_gros_et_python_absent() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let mut o = fx.options();
    o.max_message_bytes = 10_000;
    let r = fx.run_with(
        "import comicrust\ndef f():\n    comicrust.log('x' * 20000)\n",
        "f",
        RunContext::new(Hook::Startup, vec![]),
        Arc::new(TestHost::default()),
        o,
    );
    assert_eq!(r.state, RunState::Failed);
    assert!(r.failure.unwrap().message.contains("trop gros"));

    let mut o = fx.options();
    o.python = Some(fx.dir.path().join("pas-de-python"));
    let r =
        fx.run_with("def f(): pass\n", "f", RunContext::new(Hook::Startup, vec![]), Arc::new(TestHost::default()), o);
    assert_eq!(r.state, RunState::Failed);
    assert!(r.failure.unwrap().message.contains("impossible de lancer Python"));
}

#[test]
fn bibliotheques_partagees_lib() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let lib = fx.dir.path().join("scripts").join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("outils_bd.py"), "def double(x):\n    return 2 * x\n").unwrap();
    let r = fx.run(
        "import outils_bd\ndef f():\n    return outils_bd.double(21)\n",
        "f",
        RunContext::new(Hook::Startup, vec![]),
        Arc::new(TestHost::default()),
    );
    assert_eq!(r.return_value, json!(42), "{:?}", r.failure);
}

#[test]
fn fiche_json() {
    let b = LibraryBook { id: "x".into(), file: Some("/a.cbz".into()), series: Some("S".into()), ..Default::default() };
    let v = book_json(&b);
    assert_eq!(v["Id"], json!("x"));
    assert_eq!(v["File"], json!("/a.cbz"));
    assert_eq!(v["Series"], json!("S"));
    assert!(v.get("Pages").is_none());
    // Les noms de champs des scripts sont ceux de la fiche JSON.
    let mut full = LibraryBook { id: "x".into(), ..Default::default() };
    full.series = Some(String::new());
    full.language_iso = Some(String::new());
    full.book_price = Some(1.0);
    let v = book_json(&full);
    for f in ["Series", "LanguageISO", "BookPrice"] {
        assert!(EDITABLE_FIELDS.contains(&f) && v.get(f).is_some(), "{f}");
    }
}

#[test]
fn une_seule_liste_de_champs() {
    // Ce qu'un script peut demander est exactement ce que `edit` sait appliquer.
    let edit_names: Vec<&str> = crate::edit::FIELDS.iter().map(|f| f.name).collect();
    assert_eq!(*EDITABLE_FIELDS, edit_names);
    for f in EDITABLE_FIELDS.iter() {
        assert!(crate::edit::field(f).is_some(), "{f}");
        assert!(!READ_ONLY_FIELDS.contains(f), "{f} à la fois modifiable et en lecture seule");
    }
    for f in ["Id", "File", "PageCount", "CurrentPage", "Opened", "Missing"] {
        assert!(READ_ONLY_FIELDS.contains(&f), "{f}");
    }
    // Tous les champs d'une fiche JSON sont connus du module Python (lecture `book.x`).
    let xml = r#"<Book Id="x" File="/a.cbz"><Series>S</Series><PageCount>3</PageCount><Checked>false</Checked>
        <CustomValuesStore>,a=1</CustomValuesStore><LastOpenedFromListId>l</LastOpenedFromListId></Book>"#;
    let b = LibraryBook::from_xml(xml).unwrap();
    for k in book_json(&b).as_object().unwrap().keys() {
        assert!(EDITABLE_FIELDS.contains(&k.as_str()) || READ_ONLY_FIELDS.contains(&k.as_str()), "{k}");
    }
}

#[test]
fn conversion_en_modification_de_fiche() {
    use crate::edit::EditValue;
    let e = field_edit("b", "Year", &json!(2024));
    assert_eq!((e.book_id.as_str(), e.field.as_str(), e.value), ("b", "Year", Some(EditValue::Int(2024))));
    assert_eq!(field_edit("b", "Rating", &json!(4.5)).value, Some(EditValue::Real(4.5)));
    assert_eq!(field_edit("b", "Manga", &json!(true)).value, Some(EditValue::Bool(true)));
    assert_eq!(field_edit("b", "Series", &json!("X")).value, Some(EditValue::Text("X".into())));
    assert_eq!(field_edit("b", "Series", &Value::Null).value, None);
    let mut book = LibraryBook { id: "b".into(), ..Default::default() };
    assert!(crate::edit::apply_to_book(&mut book, &field_edit("b", "CustomValue:cv", &json!("1"))).unwrap());
    assert_eq!(crate::edit::custom_value(book.custom_values_store.as_deref(), "cv").as_deref(), Some("1"));
}

#[test]
fn valeurs_personnalisees_et_valeurs_refusees() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let mut host = TestHost::default();
    let b =
        LibraryBook { id: "id-0".into(), custom_values_store: Some(",Source=cv,Vu=oui".into()), ..Default::default() };
    host.books.insert(b.id.clone(), book_json(&b));
    let code = r#"
import comicrust

#@Name Valeurs
#@Hook Books
def valeurs(books):
    b = books[0]
    assert b.custom_value("source") == "cv", b.custom_value("source")
    assert b.custom_value("absente", "d") == "d"
    b.set_custom_value("Note", "bien")
    assert b.custom_value("note") == "bien"
    b.set_custom_value("Vu", None)
    b.year = "pas une année"
    b.checked = False
    comicrust.flush()
"#;
    let r = fx.run(code, "valeurs", RunContext::new(Hook::Books, ids(1)), Arc::new(host));
    assert_eq!(r.state, RunState::Finished, "{:?}\n{}", r.failure, r.journal_text());
    let fields: Vec<&str> = r
        .changes
        .iter()
        .filter_map(|c| match c {
            ScriptChange::SetField { field, .. } => Some(field.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(fields, ["CustomValue:Note", "CustomValue:Vu", "Checked"]);
    assert_eq!(r.rejected.len(), 1, "{:?}", r.rejected);
    assert!(r.rejected[0].reason.contains("Année"), "{}", r.rejected[0].reason);
}

// ---------------------------------------------------------------------------------------
// Fin de ComicRust, valeurs refusées, saisie secrète, dossier du module
// ---------------------------------------------------------------------------------------

/// ComicRust disparaît (stdin du script fermé, sans « cancel ») : le module interrompt le
/// script puis, s'il ne s'arrête pas (KeyboardInterrupt avalé), sort de lui-même : pas de
/// processus Python orphelin.
#[test]
fn fermeture_de_stdin_arrete_python() {
    use std::io::{BufRead, BufReader, Write};
    let py = need_python!();
    let fx = Fixture::new(py.clone());
    let module = fx.dir.path().join("module");
    super::runner::ensure_module(&module).unwrap();
    for (code, max) in [
        // Boucle ordinaire : interrompue tout de suite.
        ("import time\ndef f():\n    while True:\n        time.sleep(0.05)\n", Duration::from_secs(4)),
        // Script qui avale KeyboardInterrupt : arrêt forcé après EXIT_DELAY (5 s).
        (
            "import time\ndef f():\n    while True:\n        try:\n            time.sleep(0.05)\n        except BaseException:\n            pass\n",
            Duration::from_secs(10),
        ),
    ] {
        let entry = fx.script(code, "f", Hook::Library);
        let mut child = std::process::Command::new(&py)
            .args(["-I", "-X", "utf8", "-u", "-B", "-c", super::runner::BOOT])
            .arg(&module)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut hello = String::new();
        out.read_line(&mut hello).unwrap();
        assert!(hello.contains("hello"), "{hello}");
        let mut stdin = child.stdin.take().unwrap();
        let run = json!({"t": "run", "protocol": PROTOCOL_VERSION, "script": entry.file.display().to_string(),
                         "function": "f", "hook": "Library", "book_ids": [], "interactive": false, "live": false,
                         "args": null, "settings": {}, "fields": {}, "lib_dirs": []});
        writeln!(stdin, "{run}").unwrap();
        stdin.flush().unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(child.try_wait().unwrap().is_none(), "le script tourne");
        let t = Instant::now();
        drop(stdin); // ComicRust fermé
        let status = loop {
            if let Some(st) = child.try_wait().unwrap() {
                break Some(st);
            }
            if t.elapsed() > max {
                let _ = child.kill();
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(status.is_some(), "Python toujours vivant {:?} après la fermeture de stdin", t.elapsed());
    }
}

/// NaN / infini : seule la modification fautive est refusée (ValueError claire, que le
/// script peut intercepter) ; les autres modifications restent.
#[test]
fn nan_refuse_seule_modification() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let code = r#"
import comicrust
def f(books):
    b = books[0]
    b.series = "Bonne"
    try:
        b.rating = float("nan")
    except ValueError as e:
        comicrust.log("refus : %s" % e)
    b.number = "2"
    b.year = float("inf")
"#;
    let r = fx.run(code, "f", RunContext::new(Hook::Books, ids(1)), Arc::new(library(1)));
    assert_eq!(r.state, RunState::Failed);
    let f = r.failure.clone().unwrap();
    assert_eq!(f.kind, "ValueError");
    assert!(f.message.contains("non fini") && f.message.contains("Year"), "{f:?}");
    assert!(r.journal_text().contains("refus : ") && r.journal_text().contains("Rating"), "{}", r.journal_text());
    // Les modifications valides demandées avant l'erreur sont gardées (aperçu partiel).
    let fields: Vec<&str> = r
        .changes
        .iter()
        .filter_map(|c| match c {
            ScriptChange::SetField { field, .. } => Some(field.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(fields, ["Series", "Number"]);
    // Progression NaN : ignorée sans erreur.
    let r = fx.run(
        "import comicrust\ndef f():\n    comicrust.progress(float('nan'), 'x')\n    return 1\n",
        "f",
        RunContext::new(Hook::Library, vec![]),
        Arc::new(TestHost::default()),
    );
    assert_eq!(r.state, RunState::Finished, "{:?}", r.failure);
}

/// `ask(..., secret=True)` : question marquée secrète, valeur actuelle jamais envoyée ;
/// réponse vide = valeur actuelle.
#[test]
fn question_secrete() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let code = "import comicrust\ndef f():\n    return [comicrust.ask('Clé ?', 'ACTUELLE', secret=True), comicrust.ask('Clé ?', 'ACTUELLE', secret=True)]\n";
    let host = Arc::new(TestHost::default());
    *host.answers.lock().unwrap() = vec![json!(""), json!("NOUVELLE")];
    let r = fx.run(code, "f", RunContext::new(Hook::Books, vec![]), host.clone());
    assert_eq!(r.return_value, json!(["ACTUELLE", "NOUVELLE"]), "{:?}", r.failure);
    let q = host.questions.lock().unwrap();
    assert!(q.iter().all(|q| q.secret && q.default == json!("")), "{q:?}");
}

/// Module `comicrust.py` : par défaut dans `data/python/` à côté de l'exécutable, jamais
/// dans le dossier temporaire du système.
#[test]
fn dossier_du_module_portable() {
    let d = default_module_dir();
    let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    assert_eq!(d, exe_dir.join("data").join("python"));
    assert!(!d.starts_with(std::env::temp_dir()) || exe_dir.starts_with(std::env::temp_dir()));
    assert!(RunOptions::default().module_dir.is_none());
}
