//! Portage de « Bedetheque Scraper 2 » (`scripts-exemples/exemples/bedetheque_scraper2.py`) :
//! exécution sur des livres factices contre un petit serveur HTTP local qui sert des pages
//! de bedetheque.com reconstituées (`tests/fixtures/bedetheque/`, structure attendue par les
//! expressions régulières de l'original). Aucun accès au vrai site.
//!
//! Interpréteur : `COMICRUST_PYTHON`, sinon `python3` / `python` du `PATH` ; sans Python, les
//! tests sont ignorés… sauf si `COMICRUST_EXIGER_PYTHON` est défini (CI).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{
    book_json, discover_with_warnings, ChooseSpec, FormAnswer, FormSpec, Hook, Question, QuestionKind, RunContext,
    RunOptions, RunResult, RunState, ScriptEntry, ScriptHost, ScriptRunner, ScriptSettings,
};
use serde_json::{json, Map, Value};

fn dossier_exemples() -> PathBuf {
    match std::env::var_os("COMICRUST_DOSSIER_EXEMPLES").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts-exemples"),
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bedetheque")
}

fn python() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("COMICRUST_PYTHON").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(p));
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
// Serveur local
// ---------------------------------------------------------------------------------------

/// Requête reçue : chemin avec la requête (tel quel) et en-têtes (noms en minuscules).
type Requete = (String, HashMap<String, String>);

/// Fichier servi pour un chemin (en minuscules, requête comprise).
fn page(chemin: &str) -> Option<(&'static str, &'static str)> {
    let html = "text/html; charset=utf-8";
    Some(match chemin {
        "/" => (html, "accueil.html"),
        "/bandes_dessinees_b.html" => (html, "lettre_b.html"),
        "/bandes_dessinees_s.html" => (html, "lettre_s.html"),
        "/bandes_dessinees_i.html" => (html, "lettre_i.html"),
        "/search/tout?rechtexte=spirou&rechwhere=7" => (html, "recherche_spirou.html"),
        "/search/tout?rechtexte=inconnue&rechwhere=7" => (html, "recherche_vide.html"),
        "/serie-59-bd-blacksad__10000.html" => (html, "serie_blacksad.html"),
        "/serie-1-bd-spirou-et-fantasio__10000.html" => (html, "serie_spirou.html"),
        "/bd-blacksad-tome-1-quelque-part-entre-les-ombres-21.html" | "/bd--21.html" => (html, "album_blacksad_1.html"),
        "/bd-blacksad-hs-l-integrale-555.html" => (html, "album_blacksad_hs.html"),
        "/bd-spirou-et-fantasio-tome-2-il-y-a-un-sorcier-a-champignac-1002.html" => (html, "album_spirou_2.html"),
        "/ajax/resume/album/21" => (html, "resume_21.html"),
        "/ajax/resume/album/555" => (html, "resume_555.html"),
        "/ajax/resume/album/1002" => (html, "resume_1002.html"),
        c if c.starts_with("/media/couvertures/") => ("image/png", "bd2.png"),
        _ => return None,
    })
}

struct Serveur {
    base: String,
    releve: Arc<Mutex<Vec<Requete>>>,
}

impl Serveur {
    fn chemins(&self) -> Vec<String> {
        self.releve.lock().unwrap().iter().map(|(c, _)| c.to_lowercase()).collect()
    }
}

fn serveur() -> Serveur {
    let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", ecoute.local_addr().unwrap());
    let releve = Arc::new(Mutex::new(Vec::new()));
    let (r2, b2) = (releve.clone(), base.clone());
    std::thread::spawn(move || {
        for flux in ecoute.incoming() {
            let Ok(mut flux) = flux else { continue };
            let mut lecteur = BufReader::new(flux.try_clone().unwrap());
            let mut ligne = String::new();
            if lecteur.read_line(&mut ligne).is_err() {
                continue;
            }
            let mut entetes = HashMap::new();
            loop {
                let mut h = String::new();
                if lecteur.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                    break;
                }
                if let Some((n, v)) = h.split_once(':') {
                    entetes.insert(n.trim().to_lowercase(), v.trim().to_string());
                }
            }
            let cible = ligne.split_whitespace().nth(1).unwrap_or("/").to_string();
            r2.lock().unwrap().push((cible.clone(), entetes));
            let (statut, type_, corps): (&str, &str, Vec<u8>) = match page(&cible.to_lowercase()) {
                Some((t, f)) => {
                    let chemin = if f == "bd2.png" {
                        dossier_exemples().join("exemples/bd2/bd2.png")
                    } else {
                        fixtures().join(f)
                    };
                    let mut octets = std::fs::read(chemin).unwrap();
                    if t.starts_with("text/") {
                        octets =
                            String::from_utf8(octets).unwrap().replace("https://www.bedetheque.com", &b2).into_bytes();
                    }
                    ("200 OK", t, octets)
                }
                None => ("404 Not Found", "text/html", b"<html><body>Page introuvable</body></html>".to_vec()),
            };
            let _ = write!(
                flux,
                "HTTP/1.1 {statut}\r\nContent-Type: {type_}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                corps.len()
            );
            let _ = flux.write_all(&corps);
            let _ = flux.flush();
            let mut reste = [0u8; 16];
            let _ = flux.set_read_timeout(Some(Duration::from_millis(10)));
            let _ = flux.read(&mut reste);
        }
    });
    Serveur { base, releve }
}

// ---------------------------------------------------------------------------------------
// Hôte de test
// ---------------------------------------------------------------------------------------

/// Réponse à une liste de choix : rang, sélection de départ (= délai expiré), ou annuler.
#[derive(Clone, Debug)]
enum Choix {
    Rang(usize),
    Delai,
    Annuler,
}

/// Réponse à un formulaire : bouton et valeurs à imposer (les autres gardent leur valeur
/// initiale), ou `None` = annuler.
type ReponseForm = Option<(String, Value)>;

#[derive(Default)]
struct Hote {
    livres: HashMap<String, Value>,
    reglages: ScriptSettings,
    questions: Mutex<Vec<Question>>,
    confirmations: Mutex<Vec<bool>>,
    messages: Mutex<Vec<String>>,
    choix: Mutex<Vec<Choix>>,
    listes: Mutex<Vec<ChooseSpec>>,
    reponses_form: Mutex<Vec<ReponseForm>>,
    formulaires: Mutex<Vec<FormSpec>>,
    enregistres: Mutex<Vec<ScriptSettings>>,
}

impl ScriptHost for Hote {
    fn books(&self, ids: &[String]) -> Vec<Value> {
        ids.iter().filter_map(|i| self.livres.get(i).cloned()).collect()
    }
    fn ask(&self, q: &Question) -> Option<Value> {
        self.questions.lock().unwrap().push(q.clone());
        if q.kind == QuestionKind::Confirm {
            let mut c = self.confirmations.lock().unwrap();
            if !c.is_empty() {
                return Some(Value::Bool(c.remove(0)));
            }
        }
        None
    }
    fn message(&self, text: &str) {
        self.messages.lock().unwrap().push(text.into());
    }
    fn form(&self, form: &FormSpec) -> Option<FormAnswer> {
        self.formulaires.lock().unwrap().push(form.clone());
        let mut r = self.reponses_form.lock().unwrap();
        let rep = if r.is_empty() { None } else { r.remove(0) };
        let (bouton, valeurs) = rep?;
        let mut v = form.initial_values();
        if let Value::Object(m) = valeurs {
            v.extend(m);
        }
        Some(FormAnswer { button: bouton, values: v })
    }
    fn choose(&self, choice: &ChooseSpec) -> Option<Vec<usize>> {
        self.listes.lock().unwrap().push(choice.clone());
        let mut c = self.choix.lock().unwrap();
        let rep = if c.is_empty() { Choix::Annuler } else { c.remove(0) };
        match rep {
            Choix::Rang(i) => Some(vec![i]),
            Choix::Delai => (!choice.default.is_empty()).then(|| choice.default.clone()),
            Choix::Annuler => None,
        }
    }
    fn load_settings(&self, _key: &str) -> ScriptSettings {
        self.reglages.clone()
    }
    fn save_settings(&self, _key: &str, s: &ScriptSettings) -> Result<(), String> {
        self.enregistres.lock().unwrap().push(s.clone());
        Ok(())
    }
}

impl Hote {
    fn avec(livres: Vec<LibraryBook>) -> Hote {
        Hote { livres: livres.iter().map(|b| (b.id.clone(), book_json(b))).collect(), ..Default::default() }
    }

    /// Réglages : adresse du serveur, journaux dans un dossier temporaire, pas de pause.
    fn reglages(mut self, srv: &Serveur, journaux: &Path, autres: Value) -> Hote {
        let mut m = Map::new();
        m.insert("url_base".into(), json!(srv.base));
        m.insert("dossier_journaux".into(), json!(journaux.to_string_lossy()));
        m.insert("TIMEOUTS".into(), json!("0"));
        if let Value::Object(a) = autres {
            m.extend(a);
        }
        self.reglages.values = m;
        self
    }

    fn choix(self, c: Vec<Choix>) -> Hote {
        *self.choix.lock().unwrap() = c;
        self
    }

    fn formulaires(self, r: Vec<ReponseForm>) -> Hote {
        *self.reponses_form.lock().unwrap() = r;
        self
    }

    fn confirmations(self, c: Vec<bool>) -> Hote {
        *self.confirmations.lock().unwrap() = c;
        self
    }
}

fn livre(id: &str, serie: &str, numero: &str) -> LibraryBook {
    LibraryBook {
        id: id.into(),
        file: Some(format!("/bd/{serie} {numero}.cbz")),
        series: (!serie.is_empty()).then(|| serie.into()),
        number: (!numero.is_empty()).then(|| numero.into()),
        ..Default::default()
    }
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
    assert!(r.rejected.is_empty(), "{cle} : refus {:?}\n{}", r.rejected, r.journal_text());
    r
}

fn ids(n: &[&str]) -> Vec<String> {
    n.iter().map(|s| s.to_string()).collect()
}

fn champs(r: &RunResult, id: &str) -> Map<String, Value> {
    r.changes_by_book().get(id).map(|c| c.fields.clone().into_iter().collect()).unwrap_or_default()
}

fn livres_ctx(hook: Hook, n: &[&str]) -> RunContext {
    RunContext::new(hook, ids(n))
}

// ---------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------

#[test]
fn decouverte_des_points_d_entree() {
    let (entrees, avertissements) = discover_with_warnings(&dossier_exemples());
    let miens: Vec<_> = avertissements.iter().filter(|a| a.file.ends_with("bedetheque_scraper2.py")).collect();
    assert!(miens.is_empty(), "{miens:?}");
    let mut vus: Vec<(String, Vec<Hook>, Option<String>)> = entrees
        .iter()
        .filter(|e| e.rel_path == "exemples/bedetheque_scraper2.py")
        .map(|e| {
            (e.id(), e.hooks.clone(), e.image.as_ref().map(|i| i.file_name().unwrap().to_string_lossy().into_owned()))
        })
        .collect();
    vus.sort();
    assert_eq!(
        vus,
        vec![
            ("Bedetheque2".into(), vec![Hook::Books, Hook::Editor], Some("bd2.png".into())),
            ("Bedetheque2Config".into(), vec![Hook::ConfigScript], Some("bd2.png".into())),
            ("ConfigureBD2".into(), vec![Hook::Library], Some("bd2.png".into())),
            ("QuickScrapeBD2".into(), vec![Hook::Books], Some("bd2q.png".into())),
        ]
    );
    for e in entrees.iter().filter(|e| e.rel_path == "exemples/bedetheque_scraper2.py") {
        assert!(e.image.as_ref().unwrap().is_file(), "{:?}", e.image);
        assert!(e.description.is_some());
    }
    // Fichiers de l'original livrés à côté du script.
    assert!(dossier_exemples().join("exemples/bd2/BDTranslations.Config").is_file());
}

/// Recherche par la liste de la lettre (Blacksad), puis par la recherche du site avec choix
/// de la série (Spirou) ; fiche complète.
#[test]
fn recherche_choix_et_fiche_complete() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let hote = Arc::new(
        Hote::avec(vec![livre("b1", "Blacksad", "01"), livre("s2", "Spirou", "2")])
            .reglages(&srv, journaux.path(), json!({}))
            .choix(vec![Choix::Rang(0)]),
    );
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["b1", "s2"]), hote.clone());
    assert_eq!(r.return_value, json!({"traites": 2, "ignores": 0}), "{}", r.journal_text());

    let b = champs(&r, "b1");
    let web = format!("{}/BD-Blacksad-Tome-1-Quelque-part-entre-les-ombres-21.html", srv.base);
    assert_eq!(b.get("Series"), None, "série déjà « Blacksad » : {b:?}");
    assert_eq!(b["Number"], json!("1"));
    assert_eq!(b["Title"], json!("Quelque Part Entre Les Ombres"));
    assert_eq!(b["Writer"], json!("Juan Díaz Canales"));
    assert_eq!(b["Penciller"], json!("Juanjo Guarnido"));
    assert_eq!(b["Colorist"], json!("Juanjo Guarnido"));
    assert_eq!(b["CoverArtist"], json!("Juanjo Guarnido"));
    assert_eq!(b["BlackAndWhite"], json!("No"));
    assert!(!b.contains_key("Letterer"), "<Indéterminé> refusé par défaut : {b:?}");
    assert_eq!(b["Month"], json!(11));
    assert_eq!(b["Year"], json!(2000));
    assert_eq!(b["Publisher"], json!("Dargaud"));
    assert_eq!(b["Imprint"], json!("Noir"));
    assert_eq!(b["Format"], json!("Format normal"));
    assert_eq!(b["ISBN"], json!("2-205-04969-8"));
    assert_eq!(b["CommunityRating"], json!(4.6));
    assert_eq!(b["Genre"], json!("Polar"));
    assert_eq!(b["SeriesComplete"], json!("No"));
    assert_eq!(b["LanguageISO"], json!("fr"));
    assert_eq!(b["EnableProposed"], json!(false));
    assert!(!b.contains_key("Count"), "série en cours : pas de nombre d'albums");
    assert_eq!(b["Web"], json!(web));
    assert_eq!(
        b["Summary"],
        json!(
            "Dans une Amérique des années 50 peuplée d'animaux, le détective John Blacksad enquête.\n\n\
             >Quelque Part Entre Les Ombres< \nJohn Blacksad, détective privé, enquête sur le meurtre de \
             l'actrice Natalia Wilford.\n\nInfo édition: Premier tirage avec ex-libris."
        )
    );
    let notes = b["Notes"].as_str().unwrap();
    assert!(notes.starts_with("Bedetheque.com - ") && notes.ends_with("\nBD2 scraper v6.08"), "{notes}");

    // Spirou : pas dans la liste « S » sous ce nom → recherche → choix de la série.
    let s = champs(&r, "s2");
    assert_eq!(s["Series"], json!("Spirou Et Fantasio"));
    assert_eq!(s["Title"], json!("Il Y A Un Sorcier À Champignac"));
    assert_eq!(s["Writer"], json!("André Franquin, Jidéhem"));
    assert!(!s.contains_key("Colorist"), "<Collectif> refusé par défaut : {s:?}");
    assert_eq!(s["SeriesComplete"], json!("Yes"));
    assert_eq!(s["Count"], json!(3));
    assert_eq!(s["Month"], json!(1));
    assert_eq!(s["Year"], json!(1951));
    assert!(!s.contains_key("ISBN"));
    assert_eq!(s["Genre"], json!("Humour"));
    // « Tout sur la série … : » retiré du résumé de la série.
    assert_eq!(
        s["Summary"],
        json!(
            "Les aventures du groom Spirou et de son ami Fantasio.\n\n>Il Y A Un Sorcier À Champignac< \n\
             Le comte de Champignac et ses champignons : Spirou & Fantasio enquêtent."
        )
    );

    // Une seule fenêtre de choix (la série de Spirou), fermeture automatique après 30 s
    // sans série présélectionnée, avec les couvertures trouvées dans les résultats.
    let listes = hote.listes.lock().unwrap();
    assert_eq!(listes.len(), 1);
    assert_eq!(listes[0].title, "Choisissez une série pour Spirou");
    assert_eq!(listes[0].rows.len(), 3);
    assert_eq!(listes[0].rows[0][1], "Spirou et Fantasio");
    assert_eq!(listes[0].timeout, Some(30.0));
    assert!(listes[0].default.is_empty());
    assert_eq!(listes[0].images.iter().filter(|i| i.is_some()).count(), 2);

    // Requêtes : mêmes en-têtes que le fetcher de l'original, accueil avant la recherche.
    let chemins = srv.chemins();
    let i_accueil = chemins.iter().position(|c| c == "/").expect("accueil");
    let i_recherche = chemins.iter().position(|c| c.starts_with("/search/tout")).expect("recherche");
    assert!(i_accueil < i_recherche, "{chemins:?}");
    for (c, h) in srv.releve.lock().unwrap().iter() {
        assert!(h.get("user-agent").is_some_and(|a| a.contains("Chrome/124")), "{c} : {h:?}");
    }
    // Bilan final et journaux de l'original.
    assert!(hote.messages.lock().unwrap().last().unwrap().contains("Traités: 2"));
    let rapport = std::fs::read_to_string(journaux.path().join("BD2_Rename_Log.txt")).unwrap();
    assert!(rapport.contains("** Traité **"), "{rapport}");
    assert!(journaux.path().join("BD2_Debug_Log.txt").is_file());
}

/// « Ne remplacer que les champs vides » (ajout du portage) et choix de l'édition d'après le
/// numéro alternatif du livre.
#[test]
fn champs_vides_seulement_et_numero_alternatif() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let mut b = livre("b1", "Blacksad", "1");
    b.alternate_number = Some("a2005".into());
    b.title = Some("Mon titre".into());
    b.writer = Some("Quelqu'un".into());
    b.publisher = Some("Autre éditeur".into());
    let hote = Arc::new(Hote::avec(vec![b]).reglages(&srv, journaux.path(), json!({"ChampsVidesSeulement": true})));
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["b1"]), hote.clone());
    let c = champs(&r, "b1");
    for gardé in ["Title", "Writer", "Publisher", "Series", "Number", "AlternateNumber"] {
        assert!(!c.contains_key(gardé), "{gardé} déjà rempli : {c:?}");
    }
    assert_eq!(c["Penciller"], json!("Juanjo Guarnido"));
    // Édition « a2005 » choisie d'office (même numéro alternatif).
    assert_eq!(c["ISBN"], json!("978-2-205-05000-1"));
    assert_eq!(c["Month"], json!(9));
    assert_eq!(c["Year"], json!(2005));
    assert!(c["Web"].as_str().unwrap().ends_with("-21.html#12345"), "{c:?}");
    // Résumé : titre existant (« Mon titre ») et pas d'info édition (« &nbsp; »).
    assert!(c["Summary"].as_str().unwrap().contains(">Mon titre< \nJohn Blacksad"), "{c:?}");
    assert!(!c["Summary"].as_str().unwrap().contains("Info édition"));
    assert!(hote.listes.lock().unwrap().is_empty());
}

/// Choix interdit (« Ignorer et passer à la suite ») : série ambiguë et album ambigu ignorés
/// sans question ; choix automatique : fenêtre fermée après le délai sur le premier album ;
/// choix de l'édition demandé (option décochée).
#[test]
fn choix_ignorer_automatique_et_edition() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let hote = Arc::new(Hote::avec(vec![livre("s2", "Spirou", "2"), livre("hs", "Blacksad", "HS")]).reglages(
        &srv,
        journaux.path(),
        json!({"CBStop": "0"}),
    ));
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["s2", "hs"]), hote.clone());
    assert_eq!(r.return_value, json!({"traites": 0, "ignores": 2}), "{}", r.journal_text());
    assert!(hote.listes.lock().unwrap().is_empty());
    assert!(champs(&r, "s2").is_empty());
    // Genre… déjà lus sur la page de la série avant l'abandon (comme l'original).
    assert!(!champs(&r, "hs").contains_key("Title"));

    // Choix automatique : délai de 12 s, premier album présélectionné → hors-série 555.
    let hote = Arc::new(
        Hote::avec(vec![livre("hs", "Blacksad", "HS")])
            .reglages(&srv, journaux.path(), json!({"TIMEPOPUP": "12"}))
            .choix(vec![Choix::Delai]),
    );
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["hs"]), hote.clone());
    let c = champs(&r, "hs");
    assert_eq!(c["Title"], json!("L'Intégrale"));
    assert_eq!(c["Year"], json!(2009));
    assert!(!c.contains_key("Month"), "année seule : {c:?}");
    assert!(!c.contains_key("Number"), "« HS » inchangé");
    let listes = hote.listes.lock().unwrap();
    assert_eq!(listes.len(), 1);
    assert_eq!(listes[0].title, "Choisissez un album pour HS");
    assert_eq!(listes[0].rows.len(), 2);
    assert_eq!(listes[0].timeout, Some(12.0));
    assert_eq!(listes[0].default, vec![0]);
    assert_eq!(listes[0].images.iter().filter(|i| i.is_some()).count(), 2);
    drop(listes);

    // Choix de l'édition (« Choix automatique de l'édition » décoché), sans délai.
    let hote = Arc::new(
        Hote::avec(vec![livre("b1", "Blacksad", "1")])
            .reglages(&srv, journaux.path(), json!({"PopUpEditionForm": true, "CBStop": "1", "TITLEIT": false}))
            .choix(vec![Choix::Rang(1)]),
    );
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["b1"]), hote.clone());
    let c = champs(&r, "b1");
    assert_eq!(c["ISBN"], json!("978-2-205-05000-1"));
    assert_eq!(c["AlternateNumber"], json!("a2005"));
    assert_eq!(c["Title"], json!("Quelque part entre les ombres"), "sans « Pascal Case »");
    let listes = hote.listes.lock().unwrap();
    assert_eq!(listes[0].title, "Choisissez une édition pour 1");
    assert_eq!(listes[0].timeout, None);
    assert_eq!(listes[0].rows[1][2], "a2005");
}

/// Hook `Editor` (fenêtre « Infos du livre ») : même traitement, sur le livre affiché.
#[test]
fn hook_editor() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let hote = Arc::new(Hote::avec(vec![livre("b1", "blacksad", "1")]).reglages(&srv, journaux.path(), json!({})));
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Editor, &["b1"]), hote.clone());
    let c = champs(&r, "b1");
    assert_eq!(c["Series"], json!("Blacksad"));
    assert_eq!(c["Title"], json!("Quelque Part Entre Les Ombres"));
    assert!(!r.applied_live);
}

/// Page de série introuvable (404) et série inconnue : livres ignorés, message d'erreur.
#[test]
fn page_introuvable() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let hote = Arc::new(Hote::avec(vec![livre("x", "Bouncer", "1"), livre("y", "Inconnue", "3")]).reglages(
        &srv,
        journaux.path(),
        json!({}),
    ));
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["x", "y"]), hote.clone());
    assert_eq!(r.return_value, json!({"traites": 0, "ignores": 2}), "{}", r.journal_text());
    assert!(r.changes.is_empty(), "{:?}", r.changes);
    let messages = hote.messages.lock().unwrap();
    assert!(messages[0].contains("Erreur HTTP") && messages[0].contains("404"), "{messages:?}");
    assert!(messages.last().unwrap().contains("Ignorés: 2"), "{messages:?}");
    assert!(srv.chemins().iter().any(|c| c == "/search/tout?rechtexte=inconnue&rechwhere=7"));
}

/// QuickScrape : lien d'un album, puis lien (relatif) d'une série avec choix de l'album ;
/// rescrape depuis le lien Web raccourci.
#[test]
fn scrape_rapide_et_rescrape() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let lien_album = format!("{}/BD-Blacksad-Tome-1-Quelque-part-entre-les-ombres-21.html", srv.base);
    let hote = Arc::new(
        Hote::avec(vec![livre("a", "", ""), livre("b", "", "")])
            .reglages(&srv, journaux.path(), json!({}))
            .formulaires(vec![
                Some(("OK".into(), json!({"lien": lien_album}))),
                Some(("OK".into(), json!({"lien": "serie-1-BD-Spirou-et-Fantasio.html"}))),
            ])
            .choix(vec![Choix::Rang(1)]),
    );
    let r = lancer(&py, "QuickScrapeBD2", livres_ctx(Hook::Books, &["a", "b"]), hote.clone());
    assert_eq!(r.return_value, json!({"traites": 2, "ignores": 0}), "{}", r.journal_text());
    let a = champs(&r, "a");
    assert_eq!(a["Series"], json!("Blacksad"));
    assert_eq!(a["Number"], json!("1"));
    assert_eq!(a["LanguageISO"], json!("fr"), "lien direct : français par défaut");
    assert_eq!(a["Web"], json!(lien_album));
    let b = champs(&r, "b");
    assert_eq!(b["Series"], json!("Spirou Et Fantasio"));
    assert_eq!(b["Number"], json!("2"));
    assert!(!b.contains_key("Count"), "lien direct : pas de nombre d'albums");
    let formulaires = hote.formulaires.lock().unwrap();
    assert_eq!(formulaires.len(), 2);
    assert_eq!(formulaires[0].title, "Scrape directement un lien");
    let listes = hote.listes.lock().unwrap();
    assert_eq!(listes[0].rows.len(), 3, "toute la série est proposée");
    drop((formulaires, listes));

    // Rescrape : lien Web raccourci (www.bedetheque.com/BD--21.html), après confirmation ;
    // le lien Web n'est pas réécrit.
    let mut c = livre("c", "Blacksad", "1");
    c.web = Some("www.bedetheque.com/BD--21.html".into());
    let sans_lien = livre("d", "Blacksad", "2");
    let hote = Arc::new(
        Hote::avec(vec![c, sans_lien])
            .reglages(&srv, journaux.path(), json!({"CBRescrape": true}))
            .confirmations(vec![true]),
    );
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["c", "d"]), hote.clone());
    assert_eq!(r.return_value, json!({"traites": 1, "ignores": 1}), "{}", r.journal_text());
    let c = champs(&r, "c");
    assert_eq!(c["Title"], json!("Quelque Part Entre Les Ombres"));
    assert!(!c.contains_key("Web"));
    assert!(champs(&r, "d").is_empty());
    assert!(hote.questions.lock().unwrap()[0].text.contains("rescrape"));
}

/// Réglages : formulaire à trois onglets, import de l'App.Config de ComicRack, enregistrement.
#[test]
fn reglages_et_import_comicrack() {
    let py = need_python!();
    let journaux = tempfile::tempdir().unwrap();
    let app_config = fixtures().join("App.Config");
    let mut hote = Hote::default().formulaires(vec![
        // 1. Réglages : clic sur « Importer les réglages de ComicRack… ».
        Some(("importer".into(), json!({}))),
        // 2. Choix du fichier.
        Some(("OK".into(), json!({"fichier": app_config.to_string_lossy()}))),
        // 3. Réglages rouverts avec les valeurs importées : OK.
        Some(("OK".into(), json!({}))),
    ]);
    hote.reglages.values.insert("dossier_journaux".into(), json!(journaux.path().to_string_lossy()));
    let hote = Arc::new(hote);
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    let r = lancer(&py, "Bedetheque2Config", ctx, hote.clone());
    assert!(r.changes.is_empty());

    let formulaires = hote.formulaires.lock().unwrap();
    assert_eq!(formulaires.len(), 3);
    assert_eq!(formulaires[0].title, "Bedetheque Scraper 2");
    assert_eq!(formulaires[0].buttons, vec!["OK", "Annuler"]);
    let v0 = &formulaires[0].values;
    assert_eq!(v0["LANGENFR"], json!("FR"));
    assert_eq!(v0["ChoixUtilisateur"], json!("1"));
    assert_eq!(v0["FermetureAuto"], json!(true));
    assert_eq!(v0["Donnees"].as_array().unwrap().len(), 22, "tout coché par défaut");
    // Rouvert après l'import : valeurs de l'App.Config, textes en anglais.
    let v2 = &formulaires[2].values;
    assert_eq!(formulaires[2].buttons, vec!["OK", "Cancel"]);
    assert_eq!(v2["LANGENFR"], json!("EN"));
    assert_eq!(v2["TIMEOUTS"], json!(3));
    assert_eq!(v2["TIMEPOPUP"], json!(12));
    assert_eq!(v2["ChoixUtilisateur"], json!("1"));
    assert_eq!(v2["FermetureAuto"], json!(false));
    assert_eq!(v2["ChoixAutoEdition"], json!(false));
    assert_eq!(v2["ResumePremierTome"], json!(true));
    assert_eq!(v2["DBGLOGMAX"], json!(2.0));
    let donnees: Vec<&str> = v2["Donnees"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
    for absent in ["Cover", "Notes", "Letterer", "Rating", "Format"] {
        assert!(!donnees.contains(&absent), "{absent} : {donnees:?}");
    }
    assert!(donnees.contains(&"Web") && donnees.contains(&"Writer"));
    drop(formulaires);

    let s = hote.enregistres.lock().unwrap();
    let s = &s.last().expect("réglages enregistrés").values;
    assert_eq!(s["LANGENFR"], json!("EN"));
    assert_eq!(s["TBTags"], json!("BD, Bedetheque"));
    assert_eq!(s["ARTICLES"], json!("Le,La,Les,L',The,Un"));
    assert_eq!(s["SUBPATT"], json!(" - - "));
    assert_eq!(s["CBStop"], json!("1"));
    assert_eq!(s["TIMEOUT"], json!("50"));
    assert_eq!(s["PadNumber"], json!("2"));
    assert_eq!(s["CBCover"], json!(false));
    assert_eq!(s["CBWeb"], json!(true));
    assert_eq!(s["ShortWebLink"], json!(true), "CBWeb = 2 (ancienne version) : lien raccourci");
    assert_eq!(s["ONESHOTFORMAT"], json!(true));
    assert_eq!(s["CBFormat"], json!(false), "« One Shot » remplace le format");
    assert_eq!(s["PopUpEditionForm"], json!(true));
    assert_eq!(s["SerieResumeEverywhere"], json!(false));
    assert_eq!(s["AcceptGenericArtists"], json!(true));
    assert_eq!(s["use_cloudscraper"], json!(true));
    assert_eq!(s["TITLEIT"], json!(false));
    assert_eq!(s["RENLOGMAX"], json!(1048576));
    assert_eq!(s["DBGLOGMAX"], json!(2097152));
    // Réglage du portage gardé.
    assert_eq!(s["ChampsVidesSeulement"], json!(false));
}
