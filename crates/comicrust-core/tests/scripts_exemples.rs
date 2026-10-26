//! Scripts d'exemple livrés dans le zip Windows (`scripts-exemples/` → `scripts\` : les
//! exemples dans `scripts\exemples\`, `scripts\lib\` à côté) : découverte (aucun
//! avertissement) et exécution sur des livres factices.
//!
//! Interpréteur : `COMICRUST_PYTHON` (la CI Windows y met le Python « embeddable » du
//! paquet), sinon `python3` / `python` du `PATH`. Sans Python, les tests d'exécution sont
//! ignorés… sauf si `COMICRUST_EXIGER_PYTHON` est défini (CI) : ils échouent alors.
//!
//! `comic_vine.py` est essayé contre un petit serveur HTTP local qui rejoue des réponses
//! JSON écrites à la main (aucun accès à Comic Vine).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{
    book_json, discover_with_warnings, Hook, JournalKind, Question, QuestionKind, RunContext, RunOptions, RunResult,
    RunState, ScriptChange, ScriptEntry, ScriptHost, ScriptRunner, ScriptSettings,
};
use serde_json::{json, Map, Value};

/// Dossier des scripts (racine, celle de `scripts\` du paquet) : `COMICRUST_DOSSIER_EXEMPLES`
/// (la CI de release y met le dossier `scripts` du paquet), sinon `scripts-exemples/` du
/// dépôt (même disposition : `exemples/`, `lib/`).
fn dossier_exemples() -> PathBuf {
    match std::env::var_os("COMICRUST_DOSSIER_EXEMPLES").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts-exemples"),
    }
}

/// Interpréteur pour les tests (voir l'en-tête du fichier).
fn python() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("COMICRUST_PYTHON").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(p);
        assert!(p.is_file(), "COMICRUST_PYTHON désigne un fichier absent : {}", p.display());
        return Some(p);
    }
    let names: &[&str] = if cfg!(windows) { &["python.exe", "python3.exe"] } else { &["python3"] };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

macro_rules! need_python {
    () => {
        match python() {
            Some(p) => p,
            None => {
                assert!(
                    std::env::var_os("COMICRUST_EXIGER_PYTHON").is_none(),
                    "COMICRUST_EXIGER_PYTHON défini mais aucun Python trouvé"
                );
                eprintln!("Python absent : test ignoré");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------------------
// Hôte de test
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct Hote {
    livres: HashMap<String, Value>,
    /// Réponses aux questions, dans l'ordre. Pour un choix, un texte commençant par `~`
    /// désigne le premier choix qui contient la suite.
    reponses: Mutex<Vec<Value>>,
    questions: Mutex<Vec<Question>>,
    messages: Mutex<Vec<String>>,
    reglages: ScriptSettings,
    enregistres: Mutex<Vec<ScriptSettings>>,
    direct: Mutex<Vec<ScriptChange>>,
}

impl ScriptHost for Hote {
    fn books(&self, ids: &[String]) -> Vec<Value> {
        ids.iter().filter_map(|i| self.livres.get(i).cloned()).collect()
    }
    fn ask(&self, q: &Question) -> Option<Value> {
        self.questions.lock().unwrap().push(q.clone());
        let mut r = self.reponses.lock().unwrap();
        if r.is_empty() {
            return None;
        }
        let rep = r.remove(0);
        match (&rep, q.kind) {
            (Value::String(s), QuestionKind::Choice) if s.starts_with('~') => {
                q.choices.iter().find(|c| c.contains(&s[1..])).map(|c| Value::String(c.clone()))
            }
            _ => Some(rep),
        }
    }
    fn message(&self, text: &str) {
        self.messages.lock().unwrap().push(text.into());
    }
    fn load_settings(&self, _key: &str) -> ScriptSettings {
        self.reglages.clone()
    }
    fn save_settings(&self, _key: &str, s: &ScriptSettings) -> Result<(), String> {
        self.enregistres.lock().unwrap().push(s.clone());
        Ok(())
    }
    fn apply_live(&self, changes: &[ScriptChange]) -> Result<(), String> {
        self.direct.lock().unwrap().extend_from_slice(changes);
        Ok(())
    }
}

impl Hote {
    fn avec(livres: Vec<LibraryBook>) -> Hote {
        Hote { livres: livres.iter().map(|b| (b.id.clone(), book_json(b))).collect(), ..Default::default() }
    }

    fn reglages(mut self, valeurs: Value) -> Hote {
        if let Value::Object(m) = valeurs {
            self.reglages.values = m;
        }
        self
    }

    fn reponses(self, r: Vec<Value>) -> Hote {
        *self.reponses.lock().unwrap() = r;
        self
    }
}

fn livre(id: &str, file: &str) -> LibraryBook {
    LibraryBook { id: id.into(), file: Some(file.into()), ..Default::default() }
}

fn entree(cle: &str) -> ScriptEntry {
    let (entrees, _) = discover_with_warnings(&dossier_exemples());
    entrees.into_iter().find(|e| e.key.as_deref() == Some(cle)).unwrap_or_else(|| panic!("entrée {cle} absente"))
}

fn lancer(py: &Path, cle: &str, ctx: RunContext, hote: Arc<Hote>) -> RunResult {
    let module = tempfile::tempdir().unwrap();
    let options = RunOptions {
        python: Some(py.to_path_buf()),
        module_dir: Some(module.path().to_path_buf()),
        scripts_dir: Some(dossier_exemples()),
        timeout: Some(Duration::from_secs(120)),
        ..Default::default()
    };
    let r = ScriptRunner::new(options).start(&entree(cle), ctx, hote).wait();
    assert_eq!(r.state, RunState::Finished, "{cle} : {:?}\n{}", r.failure, r.journal_text());
    assert!(r.rejected.is_empty(), "{cle} : refus {:?}", r.rejected);
    r
}

fn ids(n: &[&str]) -> Vec<String> {
    n.iter().map(|s| s.to_string()).collect()
}

/// Champs modifiés d'un livre (nom → valeur).
fn champs(r: &RunResult, id: &str) -> Map<String, Value> {
    r.changes_by_book().get(id).map(|c| c.fields.clone().into_iter().collect()).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------
// Découverte
// ---------------------------------------------------------------------------------------

#[test]
fn decouverte_sans_avertissement() {
    let (entrees, avertissements) = discover_with_warnings(&dossier_exemples());
    assert!(avertissements.is_empty(), "{avertissements:?}");
    let mut vus: Vec<(String, String, Vec<Hook>)> =
        entrees.iter().map(|e| (e.id(), e.rel_path.clone(), e.hooks.clone())).collect();
    vus.sort();
    let attendu = vec![
        ("comic-vine".into(), "exemples/comic_vine.py".into(), vec![Hook::Books]),
        ("comic-vine-reglages".into(), "exemples/comic_vine.py".into(), vec![Hook::ConfigScript]),
        ("nettoyage".into(), "exemples/nettoyage.py".into(), vec![Hook::Books, Hook::Library]),
        ("nettoyage-reglages".into(), "exemples/nettoyage.py".into(), vec![Hook::ConfigScript]),
        ("nouveaux-livres".into(), "exemples/automatisme_nouveaux.py".into(), vec![Hook::NewBooks]),
        ("nouveaux-livres-reglages".into(), "exemples/automatisme_nouveaux.py".into(), vec![Hook::ConfigScript]),
        ("ranger-fichiers".into(), "exemples/ranger_fichiers.py".into(), vec![Hook::Books]),
        ("ranger-fichiers-reglages".into(), "exemples/ranger_fichiers.py".into(), vec![Hook::ConfigScript]),
    ];
    assert_eq!(vus, attendu);
    assert!(entrees.iter().all(|e| e.description.is_some()), "chaque entrée a une description");
    // Le dossier lib est livré (avec son LISEZMOI) mais n'est pas parcouru.
    assert!(dossier_exemples().join("lib").join("LISEZMOI.txt").is_file());
    // Exemples dans `exemples/` (la mise à jour ne touche pas les scripts de l'utilisateur,
    // à la racine) ; seul l'automatisme est désactivé par défaut (`#@Enabled false`).
    assert!(entrees.iter().all(|e| e.rel_path.starts_with("exemples/")));
    let desactives: Vec<String> = entrees.iter().filter(|e| !e.enabled_by_default).map(|e| e.id()).collect();
    assert_eq!(desactives, vec!["nouveaux-livres"]);
}

// ---------------------------------------------------------------------------------------
// nettoyage.py
// ---------------------------------------------------------------------------------------

#[test]
fn nettoyage() {
    let py = need_python!();
    let mut a = livre("a", "/bd/Blacksad/Blacksad T03 - Âme rouge (2005).cbz");
    a.series = Some("  Blacksad ".into());
    a.number = Some("003".into());
    a.writer = Some("Juan Díaz Canales ;Juanjo Guarnido,  juan díaz canales".into());
    a.summary = Some("Ligne 1  \n\n\n\nLigne 2 ".into());
    a.language_iso = Some(" FR".into());
    let mut b = livre("b", "/bd/Saga/Saga 012 (2013) (Digital).cbz");
    b.number = Some("#12".into());
    b.year = Some(-1);
    b.language_iso = Some("pt-br".into());
    let mut c = livre("c", "/bd/LES TUNIQUES BLEUES - 45.cbz");
    c.series = Some("LES TUNIQUES BLEUES".into());
    c.number = Some("1.5".into());
    c.title = Some("Déjà là".into());
    c.language_iso = Some("pt-BR".into()); // casse BCP 47 gardée
    let mut d = livre("d", "/bd/propre.cbz");
    d.series = Some("Spirou et Fantasio".into());
    d.number = Some("½".into());
    let livres = vec![a, b, c, d];

    // Réglages par défaut : pas de changement de casse.
    let hote = Arc::new(Hote::avec(livres.clone()));
    let r = lancer(&py, "nettoyage", RunContext::new(Hook::Books, ids(&["a", "b", "c", "d"])), hote);
    let ca = champs(&r, "a");
    assert_eq!(ca["Series"], json!("Blacksad"));
    assert_eq!(ca["Number"], json!("3"));
    assert_eq!(ca["Writer"], json!("Juan Díaz Canales, Juanjo Guarnido"));
    assert_eq!(ca["Summary"], json!("Ligne 1\n\nLigne 2"));
    assert_eq!(ca["LanguageISO"], json!("fr"));
    assert_eq!(ca["Title"], json!("Âme rouge"));
    assert_eq!(ca["Year"], json!(2005));
    let cb = champs(&r, "b");
    assert_eq!(cb["Series"], json!("Saga"));
    assert_eq!(cb["Number"], json!("12"));
    assert_eq!(cb["Year"], json!(2013));
    assert_eq!(cb["LanguageISO"], json!("pt-BR"));
    assert!(!cb.contains_key("Title"), "{cb:?}");
    let cc = champs(&r, "c");
    assert!(cc.is_empty(), "série et titre déjà remplis, casse non demandée : {cc:?}");
    assert!(champs(&r, "d").is_empty());
    assert!(r.changes.iter().all(|c| matches!(c, ScriptChange::SetField { .. })), "aucun renommage");
    assert_eq!(r.return_value, json!({"examines": 4, "a_corriger": 2}));

    // Avec la casse des séries.
    let hote = Arc::new(Hote::avec(livres).reglages(json!({"casse_series": true})));
    let r = lancer(&py, "nettoyage", RunContext::new(Hook::Library, ids(&["c", "d"])), hote);
    assert_eq!(champs(&r, "c")["Series"], json!("Les Tuniques Bleues"));
    assert!(champs(&r, "d").is_empty(), "casse mixte gardée");
}

#[test]
fn nettoyage_reglages() {
    let py = need_python!();
    let hote = Arc::new(Hote::default().reponses(vec![json!(false), json!(true), json!(true)]));
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    lancer(&py, "nettoyage-reglages", ctx, hote.clone());
    let s = hote.enregistres.lock().unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(Value::Object(s[0].values.clone()), json!({"numero": false, "depuis_nom": true, "casse_series": true}));
}

// ---------------------------------------------------------------------------------------
// ranger_fichiers.py
// ---------------------------------------------------------------------------------------

#[test]
fn ranger_fichiers() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let entree_dir = tmp.path().join("entree");
    let sortie = tmp.path().join("sortie");
    std::fs::create_dir_all(&entree_dir).unwrap();
    let f = |n: &str| {
        let p = entree_dir.join(n);
        std::fs::write(&p, b"x").unwrap();
        p.to_string_lossy().into_owned()
    };
    let mut a = livre("a", &f("a.cbz"));
    a.series = Some("Saga".into());
    a.number = Some("1".into());
    a.year = Some(2012);
    a.publisher = Some("Image".into());
    let mut b = a.clone();
    b.id = "b".into();
    b.file = Some(f("b.cbz"));
    let mut c = livre("c", &f("c.cbr"));
    c.series = Some("AC/DC: Live".into());
    c.number = Some("3".into());
    c.volume = Some(2);
    let d = livre("d", &f("Thanos Quest 01 (1990).cbr"));
    let e = livre("e", &f("divers.cbz"));
    // Fichier déjà présent à la destination de « c » : jamais écrasé.
    let occupe = sortie.join("Éditeur inconnu").join("AC-DC - Live").join("AC-DC - Live v2 #003.cbr");
    std::fs::create_dir_all(occupe.parent().unwrap()).unwrap();
    std::fs::write(&occupe, b"autre").unwrap();

    let hote = Arc::new(Hote::avec(vec![a, b, c, d, e]).reglages(json!({
        "modele": "{series}[ v{volume}][ #{number:000}][ ({year})]",
        "racine": sortie.to_string_lossy(),
        "sous_dossiers": "éditeur/série",
    })));
    let r = lancer(&py, "ranger-fichiers", RunContext::new(Hook::Books, ids(&["a", "b", "c", "d", "e"])), hote);
    let renommages: HashMap<String, PathBuf> = r
        .changes
        .iter()
        .filter_map(|c| match c {
            ScriptChange::Rename { book_id, new_path } => Some((book_id.clone(), PathBuf::from(new_path))),
            _ => None,
        })
        .collect();
    assert_eq!(renommages.len(), 4, "{renommages:?}\n{}", r.journal_text());
    assert_eq!(renommages["a"], sortie.join("Image").join("Saga").join("Saga #001 (2012).cbz"));
    assert_eq!(renommages["b"], sortie.join("Image").join("Saga").join("Saga #001 (2012) (2).cbz"));
    assert_eq!(renommages["c"], occupe.with_file_name("AC-DC - Live v2 #003 (2).cbr"));
    assert_eq!(
        renommages["d"],
        sortie.join("Éditeur inconnu").join("Thanos Quest").join("Thanos Quest #001 (1990).cbr")
    );
    assert!(!renommages.contains_key("e"));
    assert!(r.changes.iter().all(|c| matches!(c, ScriptChange::Rename { .. })), "aucun champ modifié");
    assert_eq!(r.return_value, json!({"a_renommer": 4, "inchanges": 0, "ignores": 1}));
    assert!(r.journal.iter().any(|l| l.kind == JournalKind::Warning && l.text.contains("Conflit")));
    // Rien n'a bougé sur le disque (le script ne fait que demander).
    assert!(entree_dir.join("a.cbz").is_file());
    assert_eq!(std::fs::read(&occupe).unwrap(), b"autre");

    // Modèle par défaut, dossier actuel : un fichier déjà bien nommé ne bouge pas.
    let bien = entree_dir.join("Saga v2 #7 (2015).cbz");
    std::fs::write(&bien, b"x").unwrap();
    let mut g = livre("g", &bien.to_string_lossy());
    g.series = Some("Saga".into());
    g.volume = Some(2);
    g.number = Some("7".into());
    g.year = Some(2015);
    let hote = Arc::new(Hote::avec(vec![g]));
    let r = lancer(&py, "ranger-fichiers", RunContext::new(Hook::Books, ids(&["g"])), hote);
    assert!(r.changes.is_empty(), "{:?}", r.changes);
    assert_eq!(r.return_value, json!({"a_renommer": 0, "inchanges": 1, "ignores": 0}));
}

/// Sans dossier racine, avec sous-dossier par série : un second passage ne renomme rien
/// (pas de `Batman/Batman/…`) ; chemins Windows normalisés (`\\`, lecteur, partage).
#[test]
fn ranger_fichiers_second_passage() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let bd = tmp.path().join("BD");
    std::fs::create_dir_all(&bd).unwrap();
    let source = bd.join("batman 1.cbz");
    std::fs::write(&source, b"x").unwrap();
    let batman = |id: &str, file: &str| {
        let mut b = livre(id, file);
        b.series = Some("Batman".into());
        b.number = Some("1".into());
        b.publisher = Some("DC".into());
        b
    };
    let reglages = |sous: &str| json!({"racine": "", "sous_dossiers": sous});
    let premier = |sous: &str, file: &str| {
        let hote = Arc::new(Hote::avec(vec![batman("a", file)]).reglages(reglages(sous)));
        let r = lancer(&py, "ranger-fichiers", RunContext::new(Hook::Books, ids(&["a"])), hote);
        r.changes
            .iter()
            .find_map(|c| match c {
                ScriptChange::Rename { new_path, .. } => Some(new_path.clone()),
                _ => None,
            })
            .unwrap_or_default()
    };
    // Premier passage : rangé dans BD/Batman/.
    let cible = premier("série", &source.to_string_lossy());
    assert_eq!(PathBuf::from(&cible), bd.join("Batman").join("Batman #1.cbz"));
    // Second passage (fichier déplacé) : rien à faire.
    std::fs::create_dir_all(bd.join("Batman")).unwrap();
    std::fs::rename(&source, &cible).unwrap();
    assert_eq!(premier("série", &cible), "", "second passage sans renommage");
    // Éditeur/série : BD/Batman → BD/DC/Batman, puis stable.
    let cible2 = premier("éditeur/série", &cible);
    assert_eq!(PathBuf::from(&cible2), bd.join("DC").join("Batman").join("Batman #1.cbz"));
    assert_eq!(premier("éditeur/série", &cible2), "");
    // Chemins Windows (fichiers absents : seul le calcul est vérifié).
    assert_eq!(premier("série", r"D:\BD\Batman\Batman #1.cbz"), "");
    assert_eq!(premier("série", r"d:\bd\BATMAN\Batman #1.cbz"), "", "casse ignorée sous Windows");
    assert_eq!(premier("série", "D:/BD//batman 1.cbz"), r"D:\BD\Batman\Batman #1.cbz");
    // Partage réseau : pas sous Windows (le script cherche si la destination existe, ce
    // qui interrogerait le réseau).
    if !cfg!(windows) {
        assert_eq!(premier("série", r"\\NAS\BD\x.cbz"), r"\\NAS\BD\Batman\Batman #1.cbz");
        assert_eq!(premier("éditeur/série", r"\\NAS\BD\DC\x.cbz"), r"\\NAS\BD\DC\Batman\Batman #1.cbz");
    }
}

#[test]
fn ranger_fichiers_reglages() {
    let py = need_python!();
    let hote =
        Arc::new(Hote::default().reponses(vec![json!("{series} #{number:000}"), json!("D:\\BD"), json!("~série")]));
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    lancer(&py, "ranger-fichiers-reglages", ctx, hote.clone());
    let s = hote.enregistres.lock().unwrap();
    assert_eq!(
        Value::Object(s[0].values.clone()),
        json!({"modele": "{series} #{number:000}", "racine": "D:\\BD", "sous_dossiers": "série"})
    );
}

// ---------------------------------------------------------------------------------------
// comic_vine.py (serveur local factice)
// ---------------------------------------------------------------------------------------

/// Réponse de l'API Comic Vine.
fn cv(results: Value) -> Value {
    let n = results.as_array().map_or(1, Vec::len);
    json!({"error": "OK", "limit": 100, "offset": 0, "number_of_page_results": n,
           "number_of_total_results": n, "status_code": 1, "results": results, "version": "1.0"})
}

fn reponse_comic_vine(chemin: &str, q: &HashMap<String, String>) -> Value {
    let volume = |id: u64, nom: &str, annee: &str, editeur: &str, n: u64| {
        json!({"id": id, "name": nom, "start_year": annee, "count_of_issues": n,
               "publisher": {"id": 1, "name": editeur},
               "site_detail_url": format!("https://comicvine.gamespot.com/x/4050-{id}/")})
    };
    let numero = |id: u64, num: &str, nom: Value, date: &str| {
        json!({"id": id, "issue_number": num, "name": nom, "cover_date": date, "store_date": null,
               "volume": {"id": 0}})
    };
    match chemin {
        "/api/volumes/" => match q.get("filter").map(String::as_str) {
            Some("name:Saga") => cv(json!([
                volume(1001, "Saga of the Swamp Thing", "1982", "DC Comics", 45),
                volume(43461, "Saga", "2012", "Image", 54),
            ])),
            Some("name:Batman") => cv(json!([
                volume(796, "Batman", "1940", "DC Comics", 716),
                volume(42721, "Batman", "2011", "DC Comics", 52),
            ])),
            _ => cv(json!([])),
        },
        "/api/search/" => cv(json!([])),
        "/api/issues/" => match q.get("filter").map(String::as_str) {
            Some("volume:43461,issue_number:1") => cv(json!([numero(325486, "1", json!("Chapter One"), "2012-03-31")])),
            Some("volume:43461,issue_number:2") => cv(json!([numero(330000, "2", json!(null), "2012-04-30")])),
            Some("volume:42721,issue_number:1") => cv(json!([numero(290000, "1", json!("Knightmares"), "2011-11-01")])),
            _ => cv(json!([])),
        },
        "/api/issue/4000-325486/" => cv(json!({
            "id": 325486, "issue_number": "1", "name": "Chapter One", "cover_date": "2012-03-31",
            "deck": null,
            "description": "<p>Une <em>guerre</em> sans fin.</p><figure><img src=\"x.jpg\"></figure>\
                            <h4>Éditions</h4><table><tr><td>Vol. 1</td></tr></table><p>Fin &amp; suite.</p>",
            "person_credits": [
                {"id": 1, "name": "Brian K. Vaughan", "role": "writer"},
                {"id": 2, "name": "Fiona Staples", "role": "artist, cover"},
                {"id": 3, "name": "Fonografiks", "role": "letterer"},
                {"id": 4, "name": "Eric Stephenson", "role": "editor"},
            ],
            "character_credits": [{"id": 5, "name": "Alana"}, {"id": 6, "name": "Marko"}],
            "team_credits": [], "location_credits": [{"id": 7, "name": "Cleave"}],
            "story_arc_credits": [],
            "site_detail_url": "https://comicvine.gamespot.com/saga-1-chapter-one/4000-325486/",
            "volume": {"id": 43461, "name": "Saga"}
        })),
        "/api/issue/4000-330000/" => cv(json!({
            "id": 330000, "issue_number": "2", "name": null, "cover_date": "2012-04-30",
            "description": null, "deck": "La suite.", "person_credits": [],
            "site_detail_url": "https://comicvine.gamespot.com/saga-2/4000-330000/"
        })),
        "/api/issue/4000-290000/" => cv(json!({
            "id": 290000, "issue_number": "1", "name": "Knightmares", "cover_date": "2011-11-01",
            "description": "<p>Gotham.</p>",
            "person_credits": [{"id": 8, "name": "Scott Snyder", "role": "writer"},
                               {"id": 9, "name": "Greg Capullo", "role": "penciler"},
                               {"id": 10, "name": "FCO Plascencia", "role": "colorist"}],
            "site_detail_url": "https://comicvine.gamespot.com/batman-1/4000-290000/"
        })),
        _ => json!({"error": "Object Not Found", "status_code": 101, "results": []}),
    }
}

/// Requête reçue : chemin, paramètres, User-Agent.
type Requete = (String, HashMap<String, String>, String);

/// Petit serveur HTTP/1.0 local ; renvoie l'URL de base de l'API et le relevé des requêtes.
fn serveur_comic_vine() -> (String, Arc<Mutex<Vec<Requete>>>) {
    let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
    let adresse = ecoute.local_addr().unwrap();
    let releve = Arc::new(Mutex::new(Vec::new()));
    let r2 = releve.clone();
    std::thread::spawn(move || {
        for flux in ecoute.incoming() {
            let Ok(mut flux) = flux else { continue };
            let mut lecteur = BufReader::new(flux.try_clone().unwrap());
            let mut ligne = String::new();
            if lecteur.read_line(&mut ligne).is_err() {
                continue;
            }
            let mut agent = String::new();
            loop {
                let mut h = String::new();
                if lecteur.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                    break;
                }
                if let Some((n, v)) = h.split_once(':') {
                    if n.eq_ignore_ascii_case("user-agent") {
                        agent = v.trim().to_string();
                    }
                }
            }
            let cible = ligne.split_whitespace().nth(1).unwrap_or("/").to_string();
            let url = url::Url::parse(&format!("http://local{cible}")).unwrap();
            let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let corps = reponse_comic_vine(url.path(), &q).to_string();
            r2.lock().unwrap().push((url.path().to_string(), q, agent));
            let _ = write!(
                flux,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{corps}",
                corps.len()
            );
        }
    });
    (format!("http://{adresse}/api/"), releve)
}

#[test]
fn comic_vine() {
    let py = need_python!();
    let (url, releve) = serveur_comic_vine();
    let mut saga1 = livre("s1", "/bd/Saga/Saga 001.cbz");
    saga1.series = Some("Saga".into());
    saga1.number = Some("001".into());
    saga1.year = Some(2012);
    saga1.title = Some("Chapitre un".into()); // déjà rempli : gardé (mode « compléter »)
    let mut saga2 = saga1.clone();
    saga2.id = "s2".into();
    saga2.number = Some("2".into());
    saga2.title = None;
    let mut batman = livre("b1", "/bd/Batman 01.cbz");
    batman.series = Some("Batman".into());
    batman.number = Some("1".into());
    let mut inconnu = livre("x", "/bd/Inconnue 3.cbz");
    inconnu.series = Some("Inconnue".into());
    inconnu.number = Some("3".into());

    let hote = Arc::new(
        Hote::avec(vec![saga1, saga2, batman, inconnu])
            .reglages(json!({"api_key": "CLE-SECRETE", "url_base": url, "delai": 0, "ecrire_fichier": true}))
            // « Batman » : deux séries de même nom, sans année → question.
            .reponses(vec![json!("~(2011)")]),
    );
    let r = lancer(&py, "comic-vine", RunContext::new(Hook::Books, ids(&["s1", "s2", "b1", "x"])), hote.clone());
    assert_eq!(r.return_value, json!({"trouves": 3, "ignores": 1, "requetes": 10}), "{}", r.journal_text());

    let s1 = champs(&r, "s1");
    assert!(!s1.contains_key("Title"), "titre existant gardé : {s1:?}");
    assert_eq!(s1["Summary"], json!("Une guerre sans fin.\n\nÉditions\n\nFin & suite."));
    assert_eq!(s1["Writer"], json!("Brian K. Vaughan"));
    assert_eq!(s1["Penciller"], json!("Fiona Staples"));
    assert_eq!(s1["Inker"], json!("Fiona Staples"));
    assert_eq!(s1["CoverArtist"], json!("Fiona Staples"));
    assert_eq!(s1["Letterer"], json!("Fonografiks"));
    assert_eq!(s1["Editor"], json!("Eric Stephenson"));
    assert_eq!(s1["Characters"], json!("Alana, Marko"));
    assert_eq!(s1["Locations"], json!("Cleave"));
    assert_eq!(s1["Publisher"], json!("Image"));
    assert_eq!(s1["Count"], json!(54));
    assert_eq!(s1["Month"], json!(3));
    assert!(!s1.contains_key("Year"), "année déjà remplie");
    assert_eq!(s1["Web"], json!("https://comicvine.gamespot.com/saga-1-chapter-one/4000-325486/"));
    let s2 = champs(&r, "s2");
    assert_eq!(s2["Summary"], json!("La suite."));
    assert!(!s2.contains_key("Title"));
    let b1 = champs(&r, "b1");
    assert_eq!(b1["Title"], json!("Knightmares"));
    assert_eq!(b1["Year"], json!(2011));
    assert_eq!(b1["Penciller"], json!("Greg Capullo"));
    assert_eq!(b1["Colorist"], json!("FCO Plascencia"));
    assert_eq!(b1["Count"], json!(52));
    assert!(champs(&r, "x").is_empty());
    let ecrits: Vec<&str> = r
        .changes
        .iter()
        .filter_map(|c| match c {
            ScriptChange::WriteInfo { book_id } => Some(book_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ecrits, vec!["s1", "s2", "b1"]);

    // Une seule question (Batman), avec « ignorer » en dernier choix.
    let questions = hote.questions.lock().unwrap();
    assert_eq!(questions.len(), 1, "{questions:?}");
    assert_eq!(questions[0].choices.len(), 3);
    assert_eq!(questions[0].choices[2], "(ignorer ce livre)");

    // Requêtes : clé, format, User-Agent ; la série Saga n'est cherchée qu'une fois.
    let releve = releve.lock().unwrap();
    let chemins: Vec<&str> = releve.iter().map(|(p, _, _)| p.as_str()).collect();
    assert_eq!(chemins.iter().filter(|p| **p == "/api/volumes/").count(), 3, "{chemins:?}");
    assert_eq!(chemins.iter().filter(|p| **p == "/api/search/").count(), 1);
    for (p, q, agent) in releve.iter() {
        assert_eq!(q.get("api_key").map(String::as_str), Some("CLE-SECRETE"), "{p}");
        assert_eq!(q.get("format").map(String::as_str), Some("json"), "{p}");
        assert!(q.contains_key("field_list"), "{p}");
        assert!(agent.starts_with("ComicRust/"), "{agent}");
    }
    // La clé n'apparaît jamais dans le journal.
    assert!(!r.journal_text().contains("CLE-SECRETE"));
}

#[test]
fn comic_vine_sans_cle() {
    let py = need_python!();
    let mut b = livre("a", "/bd/Saga 001.cbz");
    b.series = Some("Saga".into());
    let hote = Arc::new(Hote::avec(vec![b]));
    let r = lancer(&py, "comic-vine", RunContext::new(Hook::Books, ids(&["a"])), hote.clone());
    assert!(r.changes.is_empty());
    assert!(hote.messages.lock().unwrap()[0].contains("clé d'API"));
}

#[test]
fn comic_vine_reglages_cle_secrete() {
    let py = need_python!();
    let hote = Arc::new(Hote::default().reponses(vec![json!("  MA-CLE "), json!(false), json!(true)]));
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    lancer(&py, "comic-vine-reglages", ctx, hote.clone());
    let s = hote.enregistres.lock().unwrap();
    assert_eq!(s[0].values["api_key"], json!("MA-CLE"));
    assert_eq!(s[0].values["ecrire_fichier"], json!(true));
    assert!(s[0].secret.contains("api_key"));
    // Saisie masquée, sans valeur par défaut affichée.
    let q = hote.questions.lock().unwrap();
    assert!(q[0].secret && q[0].default == json!(""), "{:?}", q[0]);
    drop((q, s));

    // Clé déjà enregistrée : jamais envoyée à l'interface ; réponse vide = clé gardée.
    let hote = Arc::new(Hote::default().reglages(json!({"api_key": "ANCIENNE-CLE"})).reponses(vec![
        json!(""),
        json!(false),
        json!(false),
    ]));
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    lancer(&py, "comic-vine-reglages", ctx, hote.clone());
    let q = hote.questions.lock().unwrap();
    assert!(q[0].secret && q[0].default == json!("") && !q[0].text.contains("ANCIENNE"), "{:?}", q[0]);
    assert_eq!(hote.enregistres.lock().unwrap()[0].values["api_key"], json!("ANCIENNE-CLE"));
}

/// Erreur quelconque (ici une URL refusée par urllib, dont le message contient l'URL
/// complète) : message clair, sans la clé.
#[test]
fn comic_vine_erreur_sans_cle() {
    let py = need_python!();
    let mut b = livre("a", "/bd/Saga 001.cbz");
    b.series = Some("Saga".into());
    b.number = Some("1".into());
    let hote = Arc::new(
        Hote::avec(vec![b]).reglages(json!({"api_key": "CLE/SECRETE+1", "url_base": "sans-schema/", "delai": 0})),
    );
    let r = lancer(&py, "comic-vine", RunContext::new(Hook::Books, ids(&["a"])), hote.clone());
    let messages = hote.messages.lock().unwrap();
    assert!(messages[0].contains("Comic Vine"), "{messages:?}");
    for t in messages.iter().chain(std::iter::once(&r.journal_text())) {
        assert!(!t.contains("SECRETE"), "clé visible : {t}");
    }
}

// ---------------------------------------------------------------------------------------
// automatisme_nouveaux.py
// ---------------------------------------------------------------------------------------

#[test]
fn automatisme_nouveaux() {
    let py = need_python!();
    let a = livre("a", "/bd/Thanos Quest 01 (1990).cbr");
    let mut b = livre("b", "/bd/Saga 003.cbz");
    b.series = Some("Saga".into());
    b.tags = Some("À LIRE, favori".into());

    // Désactivé par défaut par son en-tête (`#@Enabled false`) : ComicRust ne le lance pas
    // tant que l'utilisateur ne l'a pas coché ; lancé, il fait son travail.
    assert!(!entree("nouveaux-livres").enabled_by_default);
    let hote = Arc::new(Hote::avec(vec![a, b]));
    let r = lancer(&py, "nouveaux-livres", RunContext::new(Hook::NewBooks, ids(&["a", "b"])), hote.clone());
    assert!(r.applied_live);
    assert_eq!(hote.direct.lock().unwrap().len(), r.changes.len());
    let ca = champs(&r, "a");
    assert_eq!(ca["Tags"], json!("à lire"));
    assert_eq!(ca["Series"], json!("Thanos Quest"));
    assert_eq!(ca["Number"], json!("1"));
    assert_eq!(ca["Year"], json!(1990));
    let cb = champs(&r, "b");
    assert!(!cb.contains_key("Tags"), "étiquette déjà présente (casse ignorée) : {cb:?}");
    assert_eq!(cb["Number"], json!("3"));
}
