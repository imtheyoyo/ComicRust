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
use std::time::{Duration, Instant};

use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::forms::{text_hint, ChooseLayout, Control, ControlKind};
use comicrust_core::scripting::{
    book_json, discover_with_warnings, ChooseSpec, FormAnswer, FormSpec, Hook, LazyImage, Question, QuestionKind,
    RunContext, RunOptions, RunResult, RunState, ScriptEntry, ScriptHost, ScriptRunner, ScriptSettings,
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
fn page(chemin: &str) -> Option<(&'static str, String)> {
    let html = "text/html; charset=utf-8";
    // Images : couvertures factices `images/couv_1.png` à `couv_6.png` selon le numéro de
    // l'album ; Couv_802 n'existe qu'en grand (repli sur la grande image testé).
    if let Some(nom) = chemin.strip_prefix("/media/couvertures/").or_else(|| chemin.strip_prefix("/cache/thb_couv/")) {
        if chemin == "/cache/thb_couv/couv_802.jpg" {
            return None;
        }
        // Couverture « .jpg » servie en WebP (comme le fait le site selon l'en-tête Accept).
        if nom == "couv_950.jpg" {
            return Some(("image/jpeg", "images/couv_webp.webp".into()));
        }
        let n: u64 = nom.trim_start_matches("couv_").trim_end_matches(".jpg").parse().unwrap_or(1);
        return Some(("image/png", format!("images/couv_{}.png", n % 6 + 1)));
    }
    let f = match chemin {
        "/" => "accueil.html",
        "/bandes_dessinees_b.html" => "lettre_b.html",
        "/bandes_dessinees_s.html" => "lettre_s.html",
        "/bandes_dessinees_i.html" => "lettre_i.html",
        "/search/tout?rechtexte=spirou&rechwhere=7" => "recherche_spirou.html",
        "/search/tout?rechtexte=inconnue&rechwhere=7" => "recherche_vide.html",
        "/serie-59-bd-blacksad__10000.html" | "/serie-500-bd-blacksad__10000.html" => "serie_blacksad.html",
        "/serie-1-bd-spirou-et-fantasio__10000.html" => "serie_spirou.html",
        "/bd-blacksad-tome-1-quelque-part-entre-les-ombres-21.html"
        | "/bd--21.html"
        | "/bd-blacksad-tome-1-quelque-part-entre-les-ombres-15161.html" => "album_blacksad_1.html",
        "/bd-blacksad-hs-l-integrale-555.html" => "album_blacksad_hs.html",
        "/bd-spirou-et-fantasio-tome-2-il-y-a-un-sorcier-a-champignac-1002.html" => "album_spirou_2.html",
        "/ajax/resume/album/21" | "/ajax/resume/album/15161" => "resume_21.html",
        "/ajax/resume/album/555" => "resume_555.html",
        "/ajax/resume/album/1002" => "resume_1002.html",
        "/bandes_dessinees_t.html" => "lettre_t.html",
        "/search/tout?rechtexte=thorgal%20monde&rechwhere=7" | "/search/tout?rechtexte=thorgal+monde&rechwhere=7" => {
            "recherche_thorgal.html"
        }
        "/serie-80-bd-thorgal__10000.html" => "serie_thorgal.html",
        "/serie-81-bd-monde-de-thorgal-le-kriss-de-valnor__10000.html" => "serie_kriss.html",
        "/bandes_dessinees_l.html" => "lettre_l.html",
        "/search/tout?rechtexte=lanfeust&rechwhere=7" => "recherche_lanfeust.html",
        "/serie-100-bd-lanfeust-de-troy__10000.html" => "serie_lanfeust.html",
        "/serie-101-bd-lanfeust-des-etoiles__10000.html" => "serie_lanfeust_etoiles.html",
        "/bd-lanfeust-de-troy-tome-3-castel-or-903.html" => "album_lanfeust_3.html",
        // Page d'album dont la structure a changé (détection d'un changement du site).
        "/bd-blacksad-tome-1-nouvelle-maquette-777.html" => "album_nouvelle_maquette.html",
        _ => return None,
    };
    Some((html, f.to_string()))
}

struct Serveur {
    base: String,
    releve: Arc<Mutex<Vec<Requete>>>,
    /// Instant de chaque requête (même ordre que `releve`).
    instants: Arc<Mutex<Vec<Instant>>>,
}

impl Serveur {
    fn chemins(&self) -> Vec<String> {
        self.releve.lock().unwrap().iter().map(|(c, _)| c.to_lowercase()).collect()
    }

    /// Instants des requêtes dont le chemin (en minuscules) commence par `debut`.
    fn instants(&self, debut: &str) -> Vec<Instant> {
        let chemins = self.chemins();
        let t = self.instants.lock().unwrap();
        chemins.iter().zip(t.iter()).filter(|(c, _)| c.starts_with(debut)).map(|(_, t)| *t).collect()
    }
}

/// Comportement du serveur pour les pages HTML (blocage du script par le site).
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    /// Toujours « 403 Forbidden », même avec de nouveaux cookies.
    Refus403,
    /// Page de protection Cloudflare (« Just a moment… ») en 403.
    AntiRobot,
    /// Captcha servi en 200 à la place de la page.
    Captcha,
}

fn serveur() -> Serveur {
    serveur_mode(Mode::Normal)
}

fn serveur_mode(mode: Mode) -> Serveur {
    let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", ecoute.local_addr().unwrap());
    let releve = Arc::new(Mutex::new(Vec::new()));
    let instants = Arc::new(Mutex::new(Vec::new()));
    let (r2, b2, i2) = (releve.clone(), base.clone(), instants.clone());
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
            // Images : refusées sans Referer du site (comme un serveur qui protège ses
            // images des liens extérieurs).
            let image = cible.to_lowercase().starts_with("/media/") || cible.to_lowercase().starts_with("/cache/");
            let referer_ok = entetes.get("referer").is_some_and(|r| r.starts_with(&b2));
            r2.lock().unwrap().push((cible.clone(), entetes));
            i2.lock().unwrap().push(Instant::now());
            let (statut, type_, corps): (&str, &str, Vec<u8>) = match page(&cible.to_lowercase()) {
                _ if !image && mode == Mode::Refus403 => {
                    ("403 Forbidden", "text/html", b"<html><body>Forbidden</body></html>".to_vec())
                }
                _ if !image && mode == Mode::AntiRobot => (
                    "403 Forbidden",
                    "text/html",
                    b"<html><head><title>Just a moment...</title></head><body><div id=\"cf-chl-widget\"></div>\
                      <script src=\"/cdn-cgi/challenge-platform/h/b/orchestrate/chl_page/v1\"></script></body></html>"
                        .to_vec(),
                ),
                _ if !image && mode == Mode::Captcha => (
                    "200 OK",
                    "text/html; charset=utf-8",
                    b"<html><head><title>Attention Required! | Cloudflare</title></head><body>\
                      <div class=\"g-recaptcha\"></div></body></html>"
                        .to_vec(),
                ),
                _ if image && !referer_ok => ("403 Forbidden", "text/html", b"<html>Interdit</html>".to_vec()),
                Some((t, f)) => {
                    let chemin = fixtures().join(f);
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
    Serveur { base, releve, instants }
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
    /// Vignettes reçues après l'ouverture de chaque liste : (ligne, avec image).
    images_recues: Mutex<Vec<Vec<(usize, bool)>>>,
    /// Lignes « survolées » par liste (rendues par `choose_wants`), dans l'ordre des listes.
    survols: Mutex<Vec<Vec<usize>>>,
    survol_courant: Mutex<Vec<usize>>,
    reponses_form: Mutex<Vec<ReponseForm>>,
    formulaires: Mutex<Vec<FormSpec>>,
    enregistres: Mutex<Vec<ScriptSettings>>,
    /// Textes copiés dans le presse-papiers.
    presse_papiers: Mutex<Vec<String>>,
    /// Adresses ouvertes dans le navigateur.
    urls: Mutex<Vec<String>>,
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
        self.images_recues.lock().unwrap().push(Vec::new());
        // Vignettes chargées après coup : l'« utilisateur » attend celles qui viennent d'office
        // et celles des lignes qu'il survole, comme s'il regardait la fenêtre.
        if !choice.lazy.is_empty() {
            let survol = {
                let mut s = self.survols.lock().unwrap();
                if s.is_empty() {
                    Vec::new()
                } else {
                    s.remove(0)
                }
            };
            let mut attendues: Vec<usize> =
                (0..choice.rows.len()).filter(|&i| choice.lazy[i] == LazyImage::Auto).collect();
            attendues.extend(survol.iter().copied());
            *self.survol_courant.lock().unwrap() = survol;
            let fin = Instant::now() + Duration::from_secs(30);
            loop {
                let recues: Vec<usize> =
                    self.images_recues.lock().unwrap().last().unwrap().iter().map(|r| r.0).collect();
                if attendues.iter().all(|i| recues.contains(i)) || Instant::now() > fin {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let mut c = self.choix.lock().unwrap();
        let rep = if c.is_empty() { Choix::Annuler } else { c.remove(0) };
        match rep {
            Choix::Rang(i) => Some(vec![i]),
            Choix::Delai => (!choice.default.is_empty()).then(|| choice.default.clone()),
            Choix::Annuler => None,
        }
    }
    fn choose_image(&self, index: usize, image: Option<Arc<Vec<u8>>>) {
        if let Some(l) = self.images_recues.lock().unwrap().last_mut() {
            l.push((index, image.is_some()));
        }
    }
    fn choose_wants(&self) -> Vec<usize> {
        std::mem::take(&mut *self.survol_courant.lock().unwrap())
    }
    fn load_settings(&self, _key: &str) -> ScriptSettings {
        self.reglages.clone()
    }
    fn save_settings(&self, _key: &str, s: &ScriptSettings) -> Result<(), String> {
        self.enregistres.lock().unwrap().push(s.clone());
        Ok(())
    }
    fn clipboard_set(&self, text: &str) -> bool {
        self.presse_papiers.lock().unwrap().push(text.into());
        true
    }
    fn open_url(&self, url: &str) -> bool {
        self.urls.lock().unwrap().push(url.into());
        true
    }
}

impl Hote {
    /// Lignes de la liste `n` dont une image est arrivée après l'ouverture (dans l'ordre
    /// d'arrivée).
    fn avec_image(&self, n: usize) -> Vec<usize> {
        self.images_recues.lock().unwrap()[n].iter().filter(|r| r.1).map(|r| r.0).collect()
    }

    fn survols(self, s: Vec<Vec<usize>>) -> Hote {
        *self.survols.lock().unwrap() = s;
        self
    }

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

/// Lance le script avec un dossier `data` neuf (module dans `data\python`, cache des
/// couvertures dans `data\bedetheque\cache`) : chaque lancement part d'un cache vide.
fn lancer(py: &Path, cle: &str, ctx: RunContext, hote: Arc<Hote>) -> RunResult {
    let data = tempfile::tempdir().unwrap();
    lancer_dans(py, cle, ctx, hote, &module_dans(data.path()))
}

/// Dossier du module `comicrust` dans `racine\data\python` ; le cache des couvertures est
/// alors `racine\data\bedetheque\cache` (voir [`cache_dans`]).
fn module_dans(racine: &Path) -> PathBuf {
    racine.join("data").join("python")
}

fn cache_dans(racine: &Path) -> PathBuf {
    racine.join("data").join("bedetheque").join("cache")
}

/// Comme [`lancer`], avec le dossier du module `comicrust` (`data\python` dans ComicRust).
fn lancer_dans(py: &Path, cle: &str, ctx: RunContext, hote: Arc<Hote>, module: &Path) -> RunResult {
    let options = RunOptions {
        python: Some(py.to_path_buf()),
        module_dir: Some(module.to_path_buf()),
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
            ("TesterBD2".into(), vec![Hook::Library], Some("bd2.png".into())),
        ]
    );
    for e in entrees.iter().filter(|e| e.rel_path == "exemples/bedetheque_scraper2.py") {
        assert!(e.image.as_ref().unwrap().is_file(), "{:?}", e.image);
        assert!(e.description.is_some());
    }
    // Fichiers de l'original livrés à côté du script.
    assert!(dossier_exemples().join("exemples/bd2/BDTranslations.Config").is_file());
    assert!(dossier_exemples().join("exemples/bd2/motifs.json").is_file());
    let nom = entrees.iter().find(|e| e.key.as_deref() == Some("TesterBD2")).map(|e| e.name.clone());
    assert_eq!(nom.as_deref(), Some("Bedetheque Scraper 2 – tester le site"));
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
    assert_eq!(listes[0].columns, ["Série", "Éditeur / pays", "Résultat", "Lien"]);
    assert_eq!(listes[0].rows[0], ["Spirou et Fantasio", "Dupuis", "001", "serie-1-BD-Spirou-et-Fantasio.html"]);
    assert_eq!(listes[0].timeout, None, "pas de choix automatique (v0.10.3)");
    assert!(listes[0].default.is_empty());
    assert_eq!(listes[0].layout, ChooseLayout::Cards);
    assert_eq!(listes[0].thumb_size, 150.0);
    // Fenêtre ouverte tout de suite, couvertures envoyées ensuite : deux dans les résultats ;
    // la page de la troisième série (pour sa couverture) n'existe pas : rien, et aucun
    // message d'erreur.
    assert_eq!(listes[0].lazy, [LazyImage::Auto; 3]);
    assert!(listes[0].images.iter().all(Option::is_none));
    assert_eq!(hote.avec_image(0), [0, 1]);

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
    assert!(!hote.messages.lock().unwrap().iter().any(|m| m.contains("changé de structure")), "pages normales");
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
    assert_eq!(listes[0].timeout, None, "pas de choix automatique (v0.10.3)");
    assert_eq!(listes[0].default, vec![0]);
    assert_eq!(hote.avec_image(0), [0, 1], "couvertures envoyées après l'ouverture");
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
    assert_eq!(listes[0].columns, ["Titre", "N°", "Dépôt légal", "Éditeur", "ISBN", "Identifiant"]);
    assert_eq!(listes[0].rows[0], ["Quelque part entre les ombres", "1", "11/2000", "Dargaud", "2-205-04969-8", "21"]);
    assert_eq!(listes[0].rows[1][1], "1 a2005");
    assert_eq!(hote.avec_image(0), [0, 1]);
}

/// Fenêtres de choix avec couvertures (v0.10.2) : résultats de recherche sans image
/// (drapeaux seulement, comme le site) → page des premières séries lue pour sa couverture
/// pendant que la fenêtre est ouverte (premier album en « data-src », ou og:image), sans
/// relire ensuite la page de la série choisie ; miniature du site (`cache/thb_couv`) plutôt que
/// la couverture en grand, repli sur la grande si la miniature manque ; réglages
/// « VignettesSeries » et « TailleVignettes ».
#[test]
fn couvertures_des_fenetres_de_choix() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let hote = Arc::new(
        Hote::avec(vec![livre("t", "Thorgal Monde", "99")])
            .reglages(&srv, journaux.path(), json!({"TailleVignettes": 180}))
            .choix(vec![Choix::Rang(0), Choix::Annuler]),
    );
    let r = lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["t"]), hote.clone(), &module_dans(data.path()));
    assert_eq!(r.return_value, json!({"traites": 0, "ignores": 1}), "{}", r.journal_text());
    let listes = hote.listes.lock().unwrap();
    assert_eq!(listes.len(), 2, "série, puis album (numéro 99 absent : toute la série)");
    let series = &listes[0];
    assert_eq!(series.title, "Choisissez une série pour Thorgal Monde");
    assert_eq!(series.rows[0], ["Thorgal", "Le Lombard · Belgique", "001", "serie-80-BD-Thorgal.html"]);
    assert_eq!(series.rows[2][1], "France");
    assert_eq!(series.thumb_size, 180.0);
    assert_eq!(hote.avec_image(0), [0, 1]);
    let albums = &listes[1];
    assert_eq!(albums.title, "Choisissez un album pour 99");
    assert_eq!(albums.columns, ["Titre", "N°", "Parution", "Éditeur", "Lien"]);
    assert_eq!(
        albums.rows[0],
        ["La Magicienne trahie", "1", "", "", "BD-Thorgal-Tome-1-La-Magicienne-trahie-801.html"]
    );
    assert_eq!(albums.lazy, [LazyImage::Auto; 2]);
    assert_eq!(hote.avec_image(1), [0, 1]);
    drop(listes);
    let chemins = srv.chemins();
    let n = |c: &str| chemins.iter().filter(|x| x.as_str() == c).count();
    assert_eq!(n("/serie-80-bd-thorgal__10000.html"), 1, "page gardée : {chemins:?}");
    assert_eq!(n("/serie-81-bd-monde-de-thorgal-le-kriss-de-valnor__10000.html"), 1);
    assert_eq!(n("/serie-82-bd-mondes-de-thorgal-les-louve__10000.html"), 1);
    assert!(!chemins.iter().any(|c| c.contains("/flags/") || c.contains("blank.gif")), "{chemins:?}");
    // Miniature du site plutôt que la couverture en grand ; la grande si la miniature manque.
    assert_eq!(n("/cache/thb_couv/couv_801.jpg"), 1);
    assert_eq!(n("/media/couvertures/couv_801.jpg"), 0);
    assert_eq!(n("/cache/thb_couv/couv_802.jpg"), 1);
    assert_eq!(n("/media/couvertures/couv_802.jpg"), 1, "repli sur la grande");
    assert_eq!(n("/cache/thb_couv/couv_811.jpg"), 1, "og:image (grande) de la série 81 → miniature");
    assert_eq!(n("/media/couvertures/couv_811.jpg"), 0);
    assert!(hote.messages.lock().unwrap().iter().all(|m| !m.contains("404")), "404 sans message");
    // Deux requêtes au plus par couverture (miniature, puis grande), jamais une troisième.
    let images: Vec<&String> =
        chemins.iter().filter(|c| c.starts_with("/media/") || c.starts_with("/cache/")).collect();
    for c in &images {
        let nom = c.rsplit('/').next().unwrap();
        assert!(images.iter().filter(|x| x.ends_with(nom)).count() <= 2, "{nom} : {images:?}");
    }
    // Absence (404) gardée sur le disque : au passage suivant, la miniature absente n'est
    // pas redemandée, la grande vient du cache.
    let avant = srv.chemins().len();
    let hote = Arc::new(
        Hote::avec(vec![livre("t", "Thorgal Monde", "99")])
            .reglages(&srv, journaux.path(), json!({}))
            .choix(vec![Choix::Rang(0), Choix::Annuler]),
    );
    lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["t"]), hote.clone(), &module_dans(data.path()));
    assert_eq!(hote.avec_image(1), [0, 1], "couvertures des albums, depuis le cache");
    let apres: Vec<String> = srv.chemins()[avant..].to_vec();
    assert!(!apres.iter().any(|c| c.contains("couv_802")), "absence gardée : {apres:?}");
    let index = std::fs::read_to_string(cache_dans(data.path()).join("couvertures.json")).unwrap();
    assert!(index.contains("absente:") && index.to_lowercase().contains("thb_couv/couv_802.jpg"), "{index}");

    // Option décochée : aucune page de série lue avant le choix.
    let srv = serveur();
    let hote = Arc::new(
        Hote::avec(vec![livre("t", "Thorgal Monde", "99")])
            .reglages(&srv, journaux.path(), json!({"VignettesSeries": false}))
            .choix(vec![Choix::Annuler]),
    );
    lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["t"]), hote.clone());
    let listes = hote.listes.lock().unwrap();
    assert!(listes[0].images.is_empty() && listes[0].lazy.is_empty(), "aucune couverture");
    assert_eq!(listes[0].layout, ChooseLayout::Cards);
    assert_eq!(listes[0].thumb_size, 150.0);
    assert!(!srv.chemins().iter().any(|c| c.starts_with("/serie-")), "{:?}", srv.chemins());
}

/// Fenêtres de choix (v0.10.2) : la fenêtre s'ouvre tout de suite, les couvertures arrivent
/// ensuite. Adresses sans schéma (« //www… »), relatives, en `data-src` ; « .jpg » servi en
/// WebP ; Referer exigé par le serveur d'images (403 sinon) ; titre, numéro, dépôt légal et
/// éditeur des albums lus dans la page de la série ; album sans couverture dans cette page :
/// page de l'album lue seulement au survol ; au moins 1 s entre deux pages de séries ;
/// cache disque (`cache\` du dossier des journaux) : rien n'est retéléchargé ensuite.
#[test]
fn couvertures_progressives_cache_et_pauses() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let module = module_dans(data.path());
    let hote = Arc::new(
        Hote::avec(vec![livre("l", "Lanfeust", "9")])
            .reglages(&srv, journaux.path(), json!({}))
            .choix(vec![Choix::Rang(0), Choix::Annuler]),
    );
    let r = lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["l"]), hote.clone(), &module);
    assert_eq!(r.return_value, json!({"traites": 0, "ignores": 1}), "{}", r.journal_text());
    let listes = hote.listes.lock().unwrap();
    assert_eq!(listes.len(), 2, "série puis album");
    assert_eq!(listes[0].rows[1][0], "Lanfeust des Étoiles");
    assert_eq!(listes[0].lazy, [LazyImage::Auto; 2]);
    assert_eq!(hote.avec_image(0), [0, 1], "couverture de chaque série (la 2e en WebP)");
    let albums = &listes[1];
    assert_eq!(albums.columns, ["Titre", "N°", "Parution", "Éditeur", "Lien"]);
    assert_eq!(albums.rows[0][..4], ["L'ivoire du Magohamoth", "1", "05/1994", "Soleil Productions"]);
    assert_eq!(albums.rows[1][..4], ["Thanos l'incongru", "2", "02/1995", "Soleil Productions"]);
    assert_eq!(albums.rows[2][..4], ["Castel Or", "3", "", ""]);
    assert_eq!(albums.lazy, [LazyImage::Auto, LazyImage::Auto, LazyImage::OnDemand]);
    assert_eq!(hote.avec_image(1), [0, 1]);
    drop(listes);
    let chemins = srv.chemins();
    let n = |c: &str| chemins.iter().filter(|x| x.as_str() == c).count();
    assert_eq!(n("/serie-100-bd-lanfeust-de-troy__10000.html"), 1, "page gardée pour la suite : {chemins:?}");
    assert_eq!(n("/serie-101-bd-lanfeust-des-etoiles__10000.html"), 1);
    assert_eq!(n("/bd-lanfeust-de-troy-tome-3-castel-or-903.html"), 0, "album 3 : pas survolé, pas lu");
    assert_eq!(n("/cache/thb_couv/couv_901.jpg"), 1, "une seule fois pour les deux fenêtres");
    assert_eq!(n("/cache/thb_couv/couv_902.jpg"), 1, "adresse relative");
    assert_eq!(n("/cache/thb_couv/couv_950.jpg"), 1, "adresse sans schéma");
    assert!(!chemins.iter().any(|c| c.starts_with("/media/")), "miniatures seulement : {chemins:?}");
    // Images demandées comme un navigateur : Referer du site, pas d'AVIF.
    for (c, h) in srv.releve.lock().unwrap().iter() {
        if c.starts_with("/media/") || c.starts_with("/cache/") {
            assert!(h.get("referer").is_some_and(|r| r.starts_with(&srv.base)), "{c} : {h:?}");
            let accept = h.get("accept").cloned().unwrap_or_default();
            assert!(accept.contains("image/webp") && !accept.contains("avif"), "{c} : {accept}");
        }
    }
    // Pauses : au moins 1 s entre deux pages de séries lues pour leurs couvertures.
    let t = srv.instants("/serie-10");
    assert_eq!(t.len(), 2);
    assert!(t[1].duration_since(t[0]) >= Duration::from_millis(950), "{:?}", t[1].duration_since(t[0]));
    // … et au moins 0,4 s entre deux images téléchargées.
    let t = srv.instants("/cache/thb_couv/");
    assert_eq!(t.len(), 3);
    for w in t.windows(2) {
        assert!(w[1].duration_since(w[0]) >= Duration::from_millis(380), "{:?}", w[1].duration_since(w[0]));
    }
    // Cache disque borné, toujours dans data\bedetheque\cache (jamais dans le dossier des
    // journaux, même réglé ailleurs).
    let cache = cache_dans(data.path());
    assert!(cache.join("couvertures.json").is_file());
    assert!(!journaux.path().join("cache").exists());
    let n_img = std::fs::read_dir(&cache)
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "img"));
    assert_eq!(n_img.count(), 3, "901 (aussi couverture de la série 100), 902, 950");

    // Second passage (même cache) : aucune page de série relue pour sa couverture, aucune
    // image retéléchargée ; l'album 3, survolé, est lu (une fois) pour sa couverture.
    let avant = srv.chemins().len();
    let hote = Arc::new(
        Hote::avec(vec![livre("l", "Lanfeust", "9")])
            .reglages(&srv, journaux.path(), json!({}))
            .choix(vec![Choix::Rang(0), Choix::Annuler])
            .survols(vec![vec![], vec![2]]),
    );
    lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["l"]), hote.clone(), &module);
    let apres: Vec<String> = srv.chemins()[avant..].to_vec();
    assert_eq!(hote.avec_image(0), [0, 1], "depuis le cache");
    assert!(hote.avec_image(1).contains(&2), "couverture de l'album survolé");
    let n = |c: &str| apres.iter().filter(|x| x.as_str() == c).count();
    assert_eq!(n("/serie-101-bd-lanfeust-des-etoiles__10000.html"), 0, "{apres:?}");
    assert_eq!(n("/serie-100-bd-lanfeust-de-troy__10000.html"), 1, "page de la série choisie");
    assert_eq!(n("/bd-lanfeust-de-troy-tome-3-castel-or-903.html"), 1);
    let images: Vec<&String> = apres.iter().filter(|c| c.starts_with("/media/") || c.starts_with("/cache/")).collect();
    assert_eq!(images, ["/cache/thb_couv/couv_903.jpg"], "seule l'image nouvelle (miniature)");
}

/// Cache disque plein (800 images) : la place est faite **avant** d'écrire, la borne n'est
/// jamais dépassée ; les images les plus anciennes partent, les nouvelles restent.
#[test]
fn cache_plein_jamais_depasse() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let cache = cache_dans(data.path());
    std::fs::create_dir_all(&cache).unwrap();
    let vieux = std::time::SystemTime::now() - Duration::from_secs(3600);
    for i in 0..800 {
        let f = cache.join(format!("{i:040}.img"));
        std::fs::write(&f, b"x").unwrap();
        std::fs::File::options().write(true).open(&f).unwrap().set_modified(vieux).unwrap();
    }
    let hote = Arc::new(
        Hote::avec(vec![livre("l", "Lanfeust", "9")])
            .reglages(&srv, journaux.path(), json!({}))
            .choix(vec![Choix::Rang(0), Choix::Annuler]),
    );
    lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["l"]), hote.clone(), &module_dans(data.path()));
    assert_eq!(hote.avec_image(0), [0, 1]);
    let images: Vec<PathBuf> = std::fs::read_dir(&cache)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "img"))
        .collect();
    assert_eq!(images.len(), 800, "borne respectée");
    let neuves = images.iter().filter(|p| std::fs::metadata(p).unwrap().len() > 1).count();
    assert_eq!(neuves, 3, "les 3 images téléchargées sont gardées");
}

/// Réglages réels relevés à la recette de la v0.10.1 (fixture `App_EN.Config`, réduite) :
/// étiquettes vides écrites sur deux lignes → vides (l'original les lit vides), plugin en
/// anglais → fenêtres en anglais et « Edition info: » dans le résumé (comme l'original),
/// tailles des journaux gardées à l'octet près.
#[test]
fn import_reglages_reels_anglais() {
    let py = need_python!();
    let journaux = tempfile::tempdir().unwrap();
    let mut hote = Hote::default().formulaires(vec![
        Some(("importer".into(), json!({}))),
        Some(("OK".into(), json!({"fichier": fixtures().join("App_EN.Config").to_string_lossy()}))),
        Some(("OK".into(), json!({}))),
    ]);
    hote.reglages.values.insert("dossier_journaux".into(), json!(journaux.path().to_string_lossy()));
    let hote = Arc::new(hote);
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    lancer(&py, "Bedetheque2Config", ctx, hote.clone());
    let s = hote.enregistres.lock().unwrap().last().expect("réglages enregistrés").values.clone();
    assert_eq!(s["TBTags"], json!(""), "« <TBTags>⏎  </TBTags> » lu vide");
    assert_eq!(s["LANGENFR"], json!("EN"));
    assert_eq!(s["DBGLOGMAX"], json!(20000), "taille gardée (pas 20971)");
    assert_eq!(s["RENLOGMAX"], json!(20000));
    assert_eq!(s["SUBPATT"], json!(" - - "));
    assert_eq!(s["TIMEOUT"], json!("70"));
    assert_eq!(s["CBStop"], json!("2"));
    let f = hote.formulaires.lock().unwrap();
    assert_eq!(f[2].buttons, vec!["OK", "Cancel"]);
    assert_eq!(f[2].label_width, Some(360.0));

    // Réglages enregistrés par la v0.10.1 (étiquettes « ⏎   ») : les étiquettes du livre
    // ne sont pas effacées ; résumé avec « Edition info: » (plugin en anglais).
    let srv = serveur();
    let mut b = livre("b1", "Blacksad", "1");
    b.tags = Some("À lire".into());
    let hote = Arc::new(Hote::avec(vec![b]).reglages(
        &srv,
        journaux.path(),
        json!({"LANGENFR": "EN", "TBTags": "\n  ", "TITLEIT": false}),
    ));
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["b1"]), hote.clone());
    let c = champs(&r, "b1");
    assert!(!c.contains_key("Tags"), "étiquettes gardées : {c:?}");
    assert!(c["Summary"].as_str().unwrap().ends_with("Edition info: Premier tirage avec ex-libris."), "{c:?}");
    assert!(hote.messages.lock().unwrap().last().unwrap().contains("Processed: 1"), "{:?}", hote.messages);
}

/// `App.Config` réel complet de l'utilisateur (fixture `App_reel.Config`, recette de la
/// v0.10.1 ; aucun chemin personnel) : les 50 réglages de l'original sont tous relus, avec
/// leur valeur (booléens 0/1, textes gardés tels quels, tailles des journaux à l'octet).
#[test]
fn import_reglages_reels_complets() {
    let py = need_python!();
    let journaux = tempfile::tempdir().unwrap();
    let mut hote = Hote::default().formulaires(vec![
        Some(("importer".into(), json!({}))),
        Some(("OK".into(), json!({"fichier": fixtures().join("App_reel.Config").to_string_lossy()}))),
        Some(("OK".into(), json!({}))),
    ]);
    hote.reglages.values.insert("dossier_journaux".into(), json!(journaux.path().to_string_lossy()));
    let hote = Arc::new(hote);
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    lancer(&py, "Bedetheque2Config", ctx, hote.clone());
    let s = hote.enregistres.lock().unwrap().last().expect("réglages enregistrés").values.clone();
    let attendu: Value = serde_json::from_str(
        r#"{
           "SHOWRENLOG": false, "SHOWDBGLOG": false, "DBGONOFF": false, "DBGLOGMAX": 20000, "RENLOGMAX": 20000,
           "LANGENFR": "EN", "TBTags": "", "CBCover": true, "CBStatus": true, "CBGenre": true, "CBNotes": true,
           "CBWeb": true, "ShortWebLink": false, "CBCount": true, "CBSynopsys": true, "CBImprint": true,
           "CBLetterer": true, "CBInker": true, "CBPrinted": true, "CBRating": true, "CBISBN": true,
           "CBLanguage": true, "CBEditor": true, "CBFormat": true, "CBColorist": true, "CBPenciller": true,
           "CBWriter": true, "CBSeries": true, "CBTitle": true, "CBDefault": false, "CBRescrape": false,
           "ARTICLES": "Le,La,Les,Die,Der,Das,Het", "SUBPATT": " - - ", "COUNTOF": true, "CBCouverture": true,
           "COUNTFINIE": true, "TITLEIT": true, "TIMEOUT": "70", "TIMEOUTS": "7", "TIMEPOPUP": "30",
           "FORMATARTICLES": true, "PopUpEditionForm": false, "PadNumber": "0", "SerieResumeEverywhere": true,
           "AlwaysChooseSerie": false, "ONESHOTFORMAT": false, "AcceptGenericArtists": false, "CBStop": "2"}"#,
    )
    .unwrap();
    for (cle, v) in attendu.as_object().unwrap() {
        assert_eq!(s.get(cle), Some(v), "{cle}");
    }
    // Réglages propres au portage : valeurs par défaut (absents de l'original).
    assert_eq!(s["TailleVignettes"], json!(150));
    assert_eq!(s["VignettesSeries"], json!(true));
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

/// Sans réglage `dossier_journaux` : journaux de l'original dans `data\bedetheque\` (le
/// module est dans `data\python\`), jamais dans le dossier des scripts.
#[test]
fn journaux_dans_data_bedetheque() {
    let py = need_python!();
    let srv = serveur();
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let hote = Arc::new(Hote::avec(vec![livre("b1", "blacksad", "1")]).reglages(
        &srv,
        tmp.path(),
        json!({"dossier_journaux": ""}),
    ));
    let r = lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["b1"]), hote, &data.join("python"));
    assert_eq!(r.return_value, json!({"traites": 1, "ignores": 0}), "{}", r.journal_text());
    let rapport = std::fs::read_to_string(data.join("bedetheque").join("BD2_Rename_Log.txt")).unwrap();
    assert!(rapport.contains("** Traité **"), "{rapport}");
    assert!(!dossier_exemples().join("exemples").join("BD2_Rename_Log.txt").exists());
    assert!(!tmp.path().join("BD2_Rename_Log.txt").exists());
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
    // Recherche sans résultat (« 0 série trouvée ») : pas un changement du site.
    assert!(!messages.iter().any(|m| m.contains("changé de structure")), "{messages:?}");
    assert!(srv.chemins().iter().any(|c| c == "/search/tout?rechtexte=inconnue&rechwhere=7"));
}

/// QuickScrape : aperçu puis lien d'un album (page lue une seule fois), puis lien (relatif)
/// d'une série avec choix de l'album ; rescrape depuis le lien Web raccourci.
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
                // « Aperçu du lien » : titre et couverture de la page, puis OK.
                Some(("apercu".into(), json!({"lien": lien_album}))),
                Some(("OK".into(), json!({}))),
                // « Chercher sur bedetheque.com » : la fenêtre revient avec le lien saisi.
                Some(("chercher".into(), json!({"lien": "serie-1-BD-Spirou-et-Fantasio.html", "vides": true}))),
                Some(("OK".into(), json!({}))),
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
    assert_eq!(formulaires.len(), 4);
    assert_eq!(formulaires[3].values["lien"], json!("serie-1-BD-Spirou-et-Fantasio.html"), "lien gardé");
    assert_eq!(formulaires[3].values["vides"], json!(true), "option gardée");
    // Aperçu : couverture (miniature) à côté du titre de la page ; page de l'album lue une
    // seule fois (gardée pour le scrape).
    assert_eq!(formulaires[1].values["lien"], json!(lien_album));
    let ControlKind::Group { controls, .. } = &formulaires[1].controls[1].kind else { panic!("groupe du lien") };
    let ControlKind::Columns { columns, widths } = &controls[2].kind else { panic!("aperçu : {controls:?}") };
    assert_eq!(*widths, [Some(120.0), None]);
    assert!(matches!(columns[0][0].kind, ControlKind::Image { .. }));
    let ControlKind::Label { text, .. } = &columns[1][0].kind else { panic!("titre") };
    assert_eq!(text, "Blacksad -1- Quelque part entre les ombres");
    let chemins = srv.chemins();
    let n = |c: &str| chemins.iter().filter(|x| x.as_str() == c).count();
    assert_eq!(n("/bd-blacksad-tome-1-quelque-part-entre-les-ombres-21.html"), 1, "{chemins:?}");
    assert_eq!(n("/cache/thb_couv/couv_21.jpg"), 1, "aperçu en miniature");
    assert_eq!(formulaires[0].title, "Scrape directement un lien");
    // Champ du lien : bouton « Coller » et aperçu du lien reconnu (expressions du script
    // évaluées par ComicRust).
    let ControlKind::Text { paste, hints, hint_none, .. } = &formulaires[0].control("lien").unwrap().kind else {
        panic!("champ du lien")
    };
    assert!(*paste);
    let apercu = |t: &str| text_hint(hints, hint_none, t).map(|(ok, h)| (ok, h.to_string()));
    assert_eq!(apercu(&lien_album), Some((true, "Lien d'un album : ses infos seront lues".into())));
    assert!(apercu("www.bedetheque.com/BD--21.html#reed").unwrap().1.starts_with("Lien d'un album"));
    assert!(apercu(" serie-1-BD-Spirou-et-Fantasio.html ").unwrap().1.starts_with("Lien d'une série"));
    assert_eq!(apercu("https://www.bedetheque.com/revue-Spirou.html").unwrap().1, "Lien d'une revue");
    assert!(!apercu("https://example.com/").unwrap().0);
    assert_eq!(apercu("  "), None);
    // Mise en page : livre concerné, lien (avec « Chercher sur bedetheque.com »), options
    // pour ce passage, aide.
    let groupe = |c: &Control| match &c.kind {
        ControlKind::Group { title, controls } => (title.clone(), controls.iter().map(|c| c.key.clone()).collect()),
        _ => (String::new(), Vec::<String>::new()),
    };
    let groupes: Vec<(String, Vec<String>)> = formulaires[0].controls.iter().map(groupe).collect();
    assert_eq!(groupes[0].0, "Livre 1 sur 2");
    assert_eq!(groupes[1].1, ["lien", ""], "champ, puis boutons « Aperçu » et « Chercher » côte à côte");
    assert!(groupes[2].0.starts_with("Liens acceptés"));
    assert_eq!(groupes[3].0, "Options pour ce passage");
    assert_eq!(groupes[3].1, ["vides", "edition"]);
    assert_eq!(formulaires[0].buttons, ["OK", "Annuler"]);
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
    assert!(v0.get("FermetureAuto").is_none(), "option retirée (v0.10.3)");
    assert_eq!(v0["Donnees"].as_array().unwrap().len(), 22, "tout coché par défaut");
    // Rouvert après l'import : valeurs de l'App.Config, textes en anglais.
    let v2 = &formulaires[2].values;
    assert_eq!(formulaires[2].buttons, vec!["OK", "Cancel"]);
    assert_eq!(v2["LANGENFR"], json!("EN"));
    assert_eq!(v2["TIMEOUTS"], json!(3));
    assert_eq!(v2["TIMEPOPUP"], json!(12));
    assert_eq!(v2["ChoixUtilisateur"], json!("1"));
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

/// Lit les éléments enfants de `<configuration>` d'un App.Config : (nom, texte).
fn elements_app_config(chemin: &Path) -> Vec<(String, String)> {
    use quick_xml::events::Event;
    let texte = std::fs::read_to_string(chemin).unwrap();
    let mut lecteur = quick_xml::Reader::from_str(&texte);
    let (mut out, mut pile, mut courant) = (Vec::new(), 0usize, None::<(String, String)>);
    loop {
        match lecteur.read_event().unwrap() {
            Event::Start(e) => {
                pile += 1;
                if pile == 2 {
                    courant = Some((String::from_utf8_lossy(e.name().as_ref()).into_owned(), String::new()));
                }
            }
            Event::Empty(e) if pile == 1 => {
                out.push((String::from_utf8_lossy(e.name().as_ref()).into_owned(), String::new()));
            }
            Event::Text(t) => {
                if let Some((_, s)) = courant.as_mut() {
                    s.push_str(&t.unescape().unwrap());
                }
            }
            Event::End(_) => {
                if pile == 2 {
                    out.push(courant.take().unwrap());
                }
                pile -= 1;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

/// `App.Config` **réel** de l'utilisateur, hors du dépôt (données personnelles) : test lancé
/// seulement si `COMICRUST_TEST_BD2_APPCONFIG` donne son chemin. Chaque réglage du fichier
/// doit être repris (import, formulaire rouvert, OK, enregistrement) avec la valeur que lui
/// donne LoadSetting de l'original : « 0 »/« 1 » → booléen, tailles des journaux en
/// entiers, textes tels quels (contenu fait seulement d'espaces → vide, comme XmlDocument),
/// CBWeb = 2 → lien raccourci, « One Shot » → format décoché. Aucun réglage inconnu.
#[test]
fn import_app_config_reel_hors_depot() {
    let Some(chemin) = std::env::var_os("COMICRUST_TEST_BD2_APPCONFIG").filter(|v| !v.is_empty()) else {
        eprintln!("COMICRUST_TEST_BD2_APPCONFIG absent : test ignoré");
        return;
    };
    let chemin = PathBuf::from(chemin);
    let py = need_python!();
    let journaux = tempfile::tempdir().unwrap();
    let mut hote = Hote::default().formulaires(vec![
        Some(("importer".into(), json!({}))),
        Some(("OK".into(), json!({"fichier": chemin.to_string_lossy()}))),
        Some(("OK".into(), json!({}))),
    ]);
    hote.reglages.values.insert("dossier_journaux".into(), json!(journaux.path().to_string_lossy()));
    let hote = Arc::new(hote);
    let mut ctx = RunContext::new(Hook::ConfigScript, vec![]);
    ctx.interactive = true;
    let r = lancer(&py, "Bedetheque2Config", ctx, hote.clone());
    assert!(hote.messages.lock().unwrap().is_empty(), "{:?}\n{}", hote.messages, r.journal_text());
    let s = hote.enregistres.lock().unwrap().last().expect("réglages enregistrés").values.clone();

    const TEXTES: [&str; 9] =
        ["LANGENFR", "TBTags", "ARTICLES", "SUBPATT", "TIMEOUT", "TIMEOUTS", "TIMEPOPUP", "PadNumber", "CBStop"];
    let elements = elements_app_config(&chemin);
    assert!(!elements.is_empty(), "aucun réglage dans {chemin:?}");
    let (mut attendu, mut ancien_lien) = (Map::new(), false);
    for (cle, brut) in &elements {
        let brut = if brut.trim().is_empty() { "" } else { brut.as_str() };
        let v = if TEXTES.contains(&cle.as_str()) {
            let t = if ["SUBPATT", "ARTICLES", "TBTags"].contains(&cle.as_str()) { brut } else { brut.trim() };
            json!(t)
        } else if cle == "DBGLOGMAX" || cle == "RENLOGMAX" {
            json!(brut.trim().parse::<i64>().unwrap_or_else(|_| panic!("{cle} = {brut:?}")))
        } else if cle == "CBWeb" && brut.trim() == "2" {
            ancien_lien = true;
            json!(true)
        } else {
            match brut.trim().to_lowercase().as_str() {
                "1" | "true" => json!(true),
                "0" | "false" | "" => json!(false),
                autre => panic!("{cle} : valeur inattendue {autre:?}"),
            }
        };
        attendu.insert(cle.clone(), v);
    }
    if ancien_lien {
        attendu.insert("ShortWebLink".into(), json!(true));
    }
    if attendu.get("ONESHOTFORMAT") == Some(&json!(true)) {
        attendu.insert("CBFormat".into(), json!(false));
    }
    let mut ecarts = Vec::new();
    for (cle, v) in &attendu {
        if s.get(cle) != Some(v) {
            ecarts.push(format!("{cle} : fichier {v}, repris {:?}", s.get(cle)));
        }
    }
    assert!(ecarts.is_empty(), "réglages mal repris :\n{}", ecarts.join("\n"));
    eprintln!("{} réglages de l'App.Config repris à l'identique", attendu.len());
}

// ---------------------------------------------------------------------------------------
// Changements du site : détection, motifs dans bd2/motifs.json, test du site
// ---------------------------------------------------------------------------------------

/// Page d'album « cassée » (structure changée) : message une seule fois par exécution,
/// détail (adresse, éléments manquants) dans le journal de débogage, rien d'appliqué.
#[test]
fn detection_page_album_cassee() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let lien = format!("{}/BD-Blacksad-Tome-1-Nouvelle-maquette-777.html", srv.base);
    let (mut a, mut b) = (livre("a", "Blacksad", "1"), livre("b", "Blacksad", "1"));
    a.web = Some(lien.clone());
    b.web = Some(lien.clone());
    let hote = Arc::new(
        Hote::avec(vec![a, b]).reglages(&srv, journaux.path(), json!({"CBRescrape": true})).confirmations(vec![true]),
    );
    let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["a", "b"]), hote.clone());
    assert_eq!(r.return_value, json!({"traites": 0, "ignores": 2}), "{}", r.journal_text());
    assert!(champs(&r, "a").is_empty() && champs(&r, "b").is_empty(), "{:?}", r.changes);
    let messages = hote.messages.lock().unwrap();
    let alertes: Vec<&String> = messages.iter().filter(|m| m.contains("changé de structure")).collect();
    assert_eq!(alertes.len(), 1, "une fois par exécution : {messages:?}");
    assert!(
        alertes[0].starts_with(
            "La page de Bedetheque semble avoir changé de structure (album : titre, auteurs, éditeur non trouvés) : \
             la recherche peut être incomplète. Signale-le avec l'album essayé."
        ),
        "{}",
        alertes[0]
    );
    assert!(alertes[0].contains("Album essayé : [Blacksad] 1") && alertes[0].contains(&lien), "{}", alertes[0]);
    let debug = std::fs::read_to_string(journaux.path().join("BD2_Debug_Log.txt")).unwrap();
    let lignes: Vec<&str> = debug.lines().filter(|l| l.starts_with("STRUCTURE : ")).collect();
    assert_eq!(lignes.len(), 2, "chaque page notée : {debug}");
    assert!(lignes[0].contains(&lien) && lignes[0].contains("non trouvés : titre, auteurs, éditeur"), "{}", lignes[0]);
    assert!(lignes[0].contains("motifs 1.0 du 2026-11-01"), "{}", lignes[0]);
    assert!(r.journal_text().contains("page album inattendue"), "{}", r.journal_text());
}

/// `bd2/motifs.json` livré = motifs intégrés au script ; fichiers illisibles ou d'une autre
/// version ignorés (motifs intégrés gardés).
#[test]
fn motifs_livres_identiques_aux_integres() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let module = module_dans(tmp.path());
    std::fs::create_dir_all(&module).unwrap();
    std::fs::write(module.join("comicrust.py"), comicrust_core::scripting::PYTHON_MODULE).unwrap();
    let utilisateur = tmp.path().join("data").join("bedetheque");
    std::fs::create_dir_all(&utilisateur).unwrap();
    let code = r#"
import json, os, sys
sys.path[:0] = [sys.argv[1], sys.argv[2]]
import bedetheque_scraper2 as b
data, err = b.lire_fichier_motifs(b.FICHIER_MOTIFS)
assert err is None, err
fichier = {k: (v["motif"], v["options"], bool(v.get("gabarit"))) for k, v in data["motifs"].items()}
assert fichier == b.MOTIFS_INTEGRES, sorted(set(fichier.items()) ^ set(b.MOTIFS_INTEGRES.items()))
assert data["version"] == b.VERSION_MOTIFS_INTEGRES and data["reference"] == b.REFERENCE_INTEGREE
info = b.charger_motifs()
assert info["avertissements"] == [] and info["corriges"] == [], info
assert info["sources"][1:] == ["bd2/motifs.json"], info["sources"]
assert b.fichier_motifs_utilisateur() == os.path.join(sys.argv[3], "motifs.json")
# Fichier de l'utilisateur d'une autre version, puis illisible : ignoré.
for contenu, raison in (('{"version": "2.0", "motifs": {"ALBUM_ISBN": {"motif": "x"}}}', "incompatible"),
                        ("{pas du json", "illisible")):
    open(os.path.join(sys.argv[3], "motifs.json"), "w", encoding="utf-8").write(contenu)
    info = b.charger_motifs()
    assert len(info["avertissements"]) == 1 and raison in info["avertissements"][0], info
    assert b.ALBUM_ISBN.pattern == b.MOTIFS_INTEGRES["ALBUM_ISBN"][0]
print("ok")
"#;
    let sortie = std::process::Command::new(&py)
        .args(["-I", "-c", code])
        .arg(&module)
        .arg(dossier_exemples().join("exemples"))
        .arg(&utilisateur)
        .output()
        .unwrap();
    let texte = String::from_utf8_lossy(&sortie.stdout).into_owned() + &String::from_utf8_lossy(&sortie.stderr);
    assert!(sortie.status.success() && texte.trim() == "ok", "{texte}");
}

/// Dossier `data` avec un fichier de motifs de l'utilisateur (`data\bedetheque\motifs.json`).
fn data_avec_motifs(motifs: Value) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let dossier = tmp.path().join("data").join("bedetheque");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("motifs.json"), serde_json::to_string_pretty(&motifs).unwrap()).unwrap();
    tmp
}

/// Motifs de l'utilisateur prioritaires (éditeur cherché sous un autre libellé : plus
/// trouvé), motif invalide → valeur de secours pour ce motif seulement (ISBN trouvé).
#[test]
fn surcharge_utilisateur_et_motif_invalide() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let tmp = data_avec_motifs(json!({
        "version": "1.3",
        "date": "2026-12-01",
        "motifs": {
            "ALBUM_EDITEUR": {"motif": r"<dt>Maison d'édition</dt>\s+<dd>(.*?)</dd>", "options": "IMD"},
            "ALBUM_ISBN": {"motif": "(<dt>EAN", "options": "ID"},
            "ALBUM_INCONNU": {"motif": "x"}
        }
    }));
    let hote = Arc::new(Hote::avec(vec![livre("b1", "Blacksad", "1")]).reglages(&srv, journaux.path(), json!({})));
    let module = module_dans(tmp.path());
    let r = lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["b1"]), hote, &module);
    let b = champs(&r, "b1");
    assert!(!b.contains_key("Publisher"), "motif de l'utilisateur prioritaire : {b:?}");
    assert_eq!(b["ISBN"], json!("2-205-04969-8"), "motif invalide : secours");
    assert_eq!(b["Writer"], json!("Juan Díaz Canales"));
    let journal = r.journal_text();
    assert!(journal.contains("motif « ALBUM_ISBN » invalide"), "{journal}");
    assert!(journal.contains("motif inconnu « ALBUM_INCONNU »"), "{journal}");
    let debug = std::fs::read_to_string(journaux.path().join("BD2_Debug_Log.txt")).unwrap();
    assert!(debug.contains("Motifs : version 1.3 du 2026-12-01"), "{debug}");
}

fn lancer_test_site(py: &Path, hote: Arc<Hote>, module: &Path) -> RunResult {
    let mut ctx = RunContext::new(Hook::Library, vec![]);
    ctx.interactive = true;
    lancer_dans(py, "TesterBD2", ctx, hote, module)
}

/// Test du site : tout est trouvé sur l'album de référence et sa série ; rapport copié ;
/// pause du script (TIMEOUTS) avant la page de la série.
#[test]
fn test_du_site_ok() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let hote = Arc::new(
        Hote::default()
            .reglages(&srv, journaux.path(), json!({"TIMEOUTS": "1"}))
            .formulaires(vec![Some(("Copier le rapport".into(), json!({}))), Some(("Fermer".into(), json!({})))]),
    );
    let r = lancer_test_site(&py, hote.clone(), &module_dans(data.path()));
    assert_eq!(
        r.return_value,
        json!({"trouves": true, "manquants": [], "pages_non_lues": 0, "motifs": "1.0"}),
        "{}",
        r.journal_text()
    );
    let formulaires = hote.formulaires.lock().unwrap();
    assert_eq!(formulaires.len(), 2, "rouvert après la copie");
    assert_eq!(formulaires[0].title, "Tester Bedetheque");
    assert_eq!(formulaires[0].buttons, ["Copier le rapport", "Fermer"]);
    assert_eq!(formulaires[1].message, "Rapport copié dans le presse-papiers.");
    let rapport = formulaires[0].initial_values()["rapport"].as_str().unwrap().to_string();
    for ligne in [
        "Motifs : version 1.0 du 2026-11-01 (motifs intégrés au script + bd2/motifs.json)",
        "✔ Titre : Quelque part entre les ombres",
        "✔ Série : Blacksad",
        "✔ Numéro : 1",
        "✔ Scénariste : Juan Díaz Canales",
        "✔ Dessinateur : Juanjo Guarnido",
        "✔ Éditeur : Dargaud",
        "✔ Dépôt légal : 11/2000",
        "✔ ISBN : 2-205-04969-8",
        "✔ Résumé de l'album : John Blacksad",
        "✔ Couverture : ",
        "✔ Liste des albums : 5 albums (1er : Quelque part entre les ombres)",
        "✔ Résumé de la série : Dans une Amérique",
        "Conclusion : Tout est trouvé : les motifs sont à jour.",
    ] {
        assert!(rapport.contains(ligne), "« {ligne} » absent :\n{rapport}");
    }
    assert!(!rapport.contains('✖'), "{rapport}");
    assert_eq!(*hote.presse_papiers.lock().unwrap(), [rapport]);
    // Pages lues : album, résumé, puis la série après la pause entre deux albums.
    let chemins = srv.chemins();
    assert_eq!(
        chemins,
        [
            "/bd-blacksad-tome-1-quelque-part-entre-les-ombres-15161.html",
            "/ajax/resume/album/15161",
            "/serie-500-bd-blacksad__10000.html"
        ]
    );
    let t = srv.instants.lock().unwrap();
    assert!(t[2] - t[1] >= Duration::from_millis(950), "pause TIMEOUTS : {:?}", t[2] - t[1]);
}

/// Test du site : album de référence dont la structure a changé (adresse prise dans le
/// fichier de l'utilisateur) et éditeur cherché sous un autre libellé : champs ✖ et
/// conclusion « le site a changé ».
#[test]
fn test_du_site_ko() {
    let py = need_python!();
    let srv = serveur();
    let journaux = tempfile::tempdir().unwrap();
    let tmp = data_avec_motifs(json!({
        "version": "1.1",
        "motifs": {"SERIE_GENRE": {"motif": "class=\"bdt-genre-v2\">([^<]+)<", "options": "I"}},
        "reference": {"album": "BD-Blacksad-Tome-1-Nouvelle-maquette-777.html"}
    }));
    let hote = Arc::new(Hote::default().reglages(&srv, journaux.path(), json!({})).formulaires(vec![]));
    let r = lancer_test_site(&py, hote.clone(), &module_dans(tmp.path()));
    let v = &r.return_value;
    assert_eq!(v["trouves"], json!(false), "{}", r.journal_text());
    assert_eq!(v["motifs"], json!("1.1"));
    let manquants: Vec<&str> = v["manquants"].as_array().unwrap().iter().map(|m| m.as_str().unwrap()).collect();
    for champ in ["Titre", "Scénariste", "Dessinateur", "Éditeur", "ISBN", "Couverture", "Genre"] {
        assert!(manquants.contains(&champ), "{champ} : {manquants:?}");
    }
    assert!(!manquants.contains(&"Liste des albums") && !manquants.contains(&"Série"), "{manquants:?}");
    let formulaires = hote.formulaires.lock().unwrap();
    let rapport = formulaires[0].initial_values()["rapport"].as_str().unwrap().to_string();
    assert!(rapport.contains("✖ Titre : non trouvé") && rapport.contains("✔ Nom de la série : Blacksad"), "{rapport}");
    assert!(rapport.contains("Motifs corrigés par les fichiers : SERIE_GENRE"), "{rapport}");
    assert!(
        rapport
            .contains(&format!("Conclusion : {} champ(s) ne sont plus trouvés : le site a changé.", manquants.len())),
        "{rapport}"
    );
    assert!(srv.chemins().contains(&"/bd-blacksad-tome-1-nouvelle-maquette-777.html".to_string()));
}

// ---------------------------------------------------------------------------------------
// Site qui bloque le script : proposition d'installer cloudscraper
// ---------------------------------------------------------------------------------------

/// Formulaires « Site bloqué » reçus.
fn formulaires_blocage(hote: &Hote) -> Vec<FormSpec> {
    hote.formulaires.lock().unwrap().iter().filter(|f| f.title.contains("Site bloqué")).cloned().collect()
}

/// 403 répétés malgré le renouvellement des cookies : une seule proposition (deux livres),
/// boutons « Copier la commande » et « Télécharger Python… », plus aucune page demandée
/// après le blocage ; l'option cochée sans module : message « indisponible » une fois.
#[test]
fn site_bloque_403_propose_cloudscraper() {
    let py = need_python!();
    let srv = serveur_mode(Mode::Refus403);
    let journaux = tempfile::tempdir().unwrap();
    let rep = |b: &str| Some((b.to_string(), json!({})));
    let hote = Arc::new(
        Hote::avec(vec![livre("x", "Blacksad", "1"), livre("y", "Spirou et Fantasio", "2")])
            .reglages(&srv, journaux.path(), json!({"use_cloudscraper": true}))
            .formulaires(vec![rep("Copier la commande"), rep("Télécharger Python…"), rep("Fermer")]),
    );
    let data = tempfile::tempdir().unwrap();
    let r =
        lancer_dans(&py, "Bedetheque2", livres_ctx(Hook::Books, &["x", "y"]), hote.clone(), &module_dans(data.path()));
    assert!(r.changes.is_empty(), "{:?}", r.changes);
    let journal = r.journal_text();
    assert_eq!(journal.matches("cloudscraper indisponible").count(), 1, "{journal}");
    let f = formulaires_blocage(&hote);
    assert_eq!(f.len(), 3, "une proposition, rouverte après chaque bouton : {journal}");
    assert!(f.iter().all(|x| x.buttons == ["Copier la commande", "Télécharger Python…", "Fermer"]));
    let lib = data.path().join("scripts").join("lib");
    let attendu = format!("py -3.13 -m pip install cloudscraper --target \"{}\"", lib.display());
    assert_eq!(*hote.presse_papiers.lock().unwrap(), [attendu]);
    assert_eq!(*hote.urls.lock().unwrap(), ["https://www.python.org/downloads/"]);
    let texte = format!("{:?}", f[0].controls);
    assert!(texte.contains("cloudscraper") && texte.contains("Configurer BD2") && texte.contains("3.13"), "{texte}");
    // Pas de message d'erreur par page : le blocage est expliqué une fois.
    assert!(!hote.messages.lock().unwrap().iter().any(|m| m.contains("403")), "{:?}", hote.messages.lock().unwrap());
    // Après le blocage, plus aucune page demandée (3 refus, chacun avec un second essai).
    let pages: Vec<String> =
        srv.chemins().into_iter().filter(|c| !c.starts_with("/media/") && !c.starts_with("/cache/")).collect();
    assert!(pages.len() <= 8, "{pages:?}");
}

/// Page anti-robot (« Just a moment… », en 403) ou captcha servi en 200 : blocage reconnu
/// dès la première page, proposition une fois ; sans interface (formulaire refusé) le
/// journal garde l'explication.
#[test]
fn page_anti_robot_reconnue() {
    let py = need_python!();
    for mode in [Mode::AntiRobot, Mode::Captcha] {
        let srv = serveur_mode(mode);
        let journaux = tempfile::tempdir().unwrap();
        let hote = Arc::new(Hote::avec(vec![livre("x", "Blacksad", "1"), livre("y", "Blacksad", "2")]).reglages(
            &srv,
            journaux.path(),
            json!({}),
        ));
        let r = lancer(&py, "Bedetheque2", livres_ctx(Hook::Books, &["x", "y"]), hote.clone());
        assert!(r.changes.is_empty());
        assert_eq!(formulaires_blocage(&hote).len(), 1, "{}", r.journal_text());
        let journal = r.journal_text();
        assert!(journal.contains("protection anti-robot") && journal.contains("pip install cloudscraper"), "{journal}");
        assert!(!journal.contains("cloudscraper indisponible"), "option non cochée : {journal}");
        let pages = srv.chemins().into_iter().filter(|c| c.starts_with("/search") || c.starts_with("/bandes")).count();
        assert!(pages <= 1, "{:?}", srv.chemins());
    }
}

/// cloudscraper disponible (module factice dans `scripts\lib`) mais le site refuse
/// toujours : message « réessayer plus tard », pas de proposition d'installation.
#[test]
fn site_bloque_meme_avec_cloudscraper() {
    let py = need_python!();
    let srv = serveur_mode(Mode::Refus403);
    let journaux = tempfile::tempdir().unwrap();
    // Copie des exemples avec un faux module cloudscraper (requêtes par urllib).
    let exemples = tempfile::tempdir().unwrap();
    let dest = exemples.path().join("exemples");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::create_dir_all(exemples.path().join("lib")).unwrap();
    fn copier(de: &Path, vers: &Path) {
        std::fs::create_dir_all(vers).unwrap();
        for e in std::fs::read_dir(de).unwrap() {
            let e = e.unwrap();
            if e.file_name() == "__pycache__" {
                continue;
            }
            if e.path().is_dir() {
                copier(&e.path(), &vers.join(e.file_name()));
            } else {
                std::fs::copy(e.path(), vers.join(e.file_name())).unwrap();
            }
        }
    }
    copier(&dossier_exemples().join("exemples"), &dest);
    std::fs::write(
        exemples.path().join("lib").join("cloudscraper.py"),
        r#"import urllib.request, urllib.error
class _Rep:
    def __init__(self, code, octets):
        self.status_code, self.content = code, octets
        self.text = octets.decode("utf-8", "replace")
    def raise_for_status(self):
        if self.status_code >= 400:
            e = Exception("HTTP %d" % self.status_code)
            e.response = self
            raise e
class _Scraper:
    def __init__(self):
        self.headers = {}
    def get(self, url, headers=None, timeout=None):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=headers or {}), timeout=timeout) as r:
                return _Rep(r.status, r.read())
        except urllib.error.HTTPError as e:
            return _Rep(e.code, e.read())
def create_scraper(**kw):
    return _Scraper()
"#,
    )
    .unwrap();
    let hote = Arc::new(Hote::avec(vec![livre("x", "Blacksad", "1"), livre("y", "Blacksad", "2")]).reglages(
        &srv,
        journaux.path(),
        json!({"use_cloudscraper": true}),
    ));
    let (entrees, _) = discover_with_warnings(exemples.path());
    let entree = entrees.into_iter().find(|e| e.key.as_deref() == Some("Bedetheque2")).unwrap();
    let data = tempfile::tempdir().unwrap();
    let options = RunOptions {
        python: Some(py.clone()),
        module_dir: Some(module_dans(data.path())),
        scripts_dir: Some(exemples.path().to_path_buf()),
        timeout: Some(Duration::from_secs(120)),
        ..Default::default()
    };
    let r = ScriptRunner::new(options).start(&entree, livres_ctx(Hook::Books, &["x", "y"]), hote.clone()).wait();
    assert_eq!(r.state, RunState::Finished, "{:?}\n{}", r.failure, r.journal_text());
    let journal = r.journal_text();
    assert!(!journal.contains("cloudscraper indisponible"), "{journal}");
    assert!(formulaires_blocage(&hote).is_empty(), "pas de proposition d'installation");
    let messages = hote.messages.lock().unwrap();
    let bloque: Vec<&String> = messages.iter().filter(|m| m.contains("même avec cloudscraper")).collect();
    assert_eq!(bloque.len(), 1, "{messages:?}");
    assert!(bloque[0].contains("réessayez plus tard"), "{}", bloque[0]);
}
