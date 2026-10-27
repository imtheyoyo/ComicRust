//! Script d'exemple `library_organizer.py` (portage de Library Organizer de ComicRack) :
//! moteur de modèles, règles, simulation, doublons, second passage, annulation, dossiers
//! vides, import d'un `losettingsx.dat` (fixture construite d'après `losettings.py`).
//!
//! Interpréteur : `COMICRUST_PYTHON`, sinon `python3` / `python` du `PATH` ; sans Python, les
//! tests sont ignorés (sauf si `COMICRUST_EXIGER_PYTHON` est défini).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{
    book_json, discover_with_warnings, FormAnswer, FormSpec, Hook, Question, RunContext, RunOptions, RunResult,
    RunState, ScriptChange, ScriptEntry, ScriptHost, ScriptRunner, ScriptSettings,
};
use serde_json::{json, Map, Value};

fn dossier_exemples() -> PathBuf {
    match std::env::var_os("COMICRUST_DOSSIER_EXEMPLES").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts-exemples"),
    }
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
                assert!(std::env::var_os("COMICRUST_EXIGER_PYTHON").is_none(), "Python exigé mais absent");
                eprintln!("Python absent : test ignoré");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------------------
// Hôte de test : formulaires répondus d'après leur titre
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct Hote {
    livres: HashMap<String, Value>,
    /// Réponses aux formulaires, dans l'ordre : (début du titre attendu, bouton, valeurs).
    formulaires: Mutex<Vec<(String, String, Value)>>,
    vus: Mutex<Vec<FormSpec>>,
    /// Réponses aux questions (`ask`, `confirm`), dans l'ordre.
    reponses: Mutex<Vec<Value>>,
    messages: Mutex<Vec<String>>,
    reglages: ScriptSettings,
    enregistres: Mutex<Vec<ScriptSettings>>,
}

impl ScriptHost for Hote {
    fn books(&self, ids: &[String]) -> Vec<Value> {
        ids.iter().filter_map(|i| self.livres.get(i).cloned()).collect()
    }
    fn ask(&self, _q: &Question) -> Option<Value> {
        let mut r = self.reponses.lock().unwrap();
        if r.is_empty() {
            None
        } else {
            Some(r.remove(0))
        }
    }
    fn message(&self, text: &str) {
        self.messages.lock().unwrap().push(text.into());
    }
    fn form(&self, form: &FormSpec) -> Option<FormAnswer> {
        self.vus.lock().unwrap().push(form.clone());
        let mut f = self.formulaires.lock().unwrap();
        if f.first().is_some_and(|(t, _, _)| form.title.starts_with(t.as_str())) {
            let (_, button, values) = f.remove(0);
            let values = values.as_object().cloned().unwrap_or_default();
            return Some(FormAnswer { button, values });
        }
        None
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
    fn avec(livres: &[LibraryBook], reglages: Value) -> Hote {
        let mut h =
            Hote { livres: livres.iter().map(|b| (b.id.clone(), book_json(b))).collect(), ..Default::default() };
        if let Value::Object(m) = reglages {
            h.reglages.values = m;
        }
        h
    }
    fn formulaires(self, f: Vec<(&str, &str, Value)>) -> Hote {
        *self.formulaires.lock().unwrap() = f.into_iter().map(|(t, b, v)| (t.into(), b.into(), v)).collect();
        self
    }
    fn reponses(self, r: Vec<Value>) -> Hote {
        *self.reponses.lock().unwrap() = r;
        self
    }
    fn derniers_reglages(&self) -> Map<String, Value> {
        self.enregistres.lock().unwrap().last().expect("réglages enregistrés").values.clone()
    }
}

fn entree(cle: &str) -> ScriptEntry {
    let (entrees, _) = discover_with_warnings(&dossier_exemples());
    entrees.into_iter().find(|e| e.key.as_deref() == Some(cle)).unwrap_or_else(|| panic!("entrée {cle} absente"))
}

fn lancer(py: &Path, cle: &str, hook: Hook, ids: &[&str], hote: Arc<Hote>) -> RunResult {
    let module = tempfile::tempdir().unwrap();
    let options = RunOptions {
        python: Some(py.to_path_buf()),
        module_dir: Some(module.path().to_path_buf()),
        scripts_dir: Some(dossier_exemples()),
        timeout: Some(Duration::from_secs(120)),
        ..Default::default()
    };
    let mut ctx = RunContext::new(hook, ids.iter().map(|s| s.to_string()).collect());
    ctx.interactive = true;
    let r = ScriptRunner::new(options).start(&entree(cle), ctx, hote).wait();
    assert_eq!(r.state, RunState::Finished, "{cle} : {:?}\n{}", r.failure, r.journal_text());
    assert!(r.rejected.is_empty(), "{cle} : refus {:?}", r.rejected);
    r
}

fn renommages(r: &RunResult) -> HashMap<String, PathBuf> {
    r.changes
        .iter()
        .filter_map(|c| match c {
            ScriptChange::Rename { book_id, new_path } => Some((book_id.clone(), PathBuf::from(new_path))),
            _ => None,
        })
        .collect()
}

/// Fichier factice (contenu « x ») ; rend son chemin.
fn fichier(p: &Path) -> String {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"x").unwrap();
    p.to_string_lossy().into_owned()
}

fn livre(id: &str, file: &str) -> LibraryBook {
    LibraryBook { id: id.into(), file: Some(file.into()), ..Default::default() }
}

fn batman(id: &str, file: &str, numero: &str, annee: i32, mois: i32) -> LibraryBook {
    let mut b = livre(id, file);
    b.series = Some("Batman".into());
    b.number = Some(numero.into());
    b.volume = Some(2011);
    b.year = Some(annee);
    b.month = Some(mois);
    b.publisher = Some("DC Comics".into());
    b
}

/// Réglages : un profil (champs de `Profile`, noms de l'original) et le dossier du journal.
fn reglages(data: &Path, profil: Value) -> Value {
    let nom = profil["Name"].as_str().unwrap_or("P").to_string();
    json!({"profils": {nom.clone(): profil}, "derniers": [nom], "dossier_journal": data.to_string_lossy()})
}

// ---------------------------------------------------------------------------------------
// Découverte
// ---------------------------------------------------------------------------------------

#[test]
fn decouverte() {
    let (entrees, avertissements) = discover_with_warnings(&dossier_exemples());
    assert!(
        avertissements.iter().all(|a| !a.file.to_string_lossy().contains("library_organizer")),
        "{avertissements:?}"
    );
    let mut lo: Vec<(String, Vec<Hook>)> = entrees
        .iter()
        .filter(|e| e.rel_path.starts_with("exemples/library_organizer"))
        .map(|e| (e.id(), e.hooks.clone()))
        .collect();
    lo.sort();
    assert_eq!(
        lo,
        vec![
            ("library-organizer".into(), vec![Hook::Books]),
            ("library-organizer-annuler".into(), vec![Hook::Library]),
            ("library-organizer-bibliotheque".into(), vec![Hook::Library]),
            ("library-organizer-configurer".into(), vec![Hook::Library]),
            ("library-organizer-dossiers-vides".into(), vec![Hook::Library]),
            ("library-organizer-quick".into(), vec![Hook::Books]),
            ("library-organizer-reglages".into(), vec![Hook::ConfigScript]),
        ]
    );
    // Le moteur (module sans en-tête) n'est pas proposé comme script ; la licence est livrée.
    assert!(entrees.iter().all(|e| e.rel_path != "exemples/library_organizer_moteur.py"));
    assert!(dossier_exemples().join("LICENCE-library-organizer.txt").is_file());
}

// ---------------------------------------------------------------------------------------
// Modèles (syntaxe de l'original), simulation, second passage
// ---------------------------------------------------------------------------------------

#[test]
fn modeles_par_defaut_et_second_passage() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let entree_dir = tmp.path().join("entree");
    let sortie = tmp.path().join("sortie");
    let data = tmp.path().join("data");
    let mut a = batman("a", &fichier(&entree_dir.join("a.cbz")), "7", 2012, 3);
    a.count = Some(12);
    a.format = Some("Limited Series".into());
    let b = batman("b", &fichier(&entree_dir.join("b.cbz")), "1", 2011, 9);
    let c = batman("c", &fichier(&entree_dir.join("c.cbz")), "52", 2016, -1);
    // Modèles par défaut de l'original (losettings.load_profiles).
    let profil = json!({
        "Name": "P", "BaseFolder": sortie.to_string_lossy(), "Mode": "Move",
        "FolderTemplate": "{<publisher>}\\{<imprint>}\\{<series>}{ (<startyear>{ <format>})}",
        "FileTemplate": "{<series>}{ Vol.<volume>}{ #<number2>}{ (of <count2>)}{ ({<month>, }<year>)}",
    });
    let livres = [a.clone(), b.clone(), c.clone()];
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil.clone())));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &["a", "b", "c"], hote.clone());
    let ren = renommages(&r);
    assert_eq!(ren.len(), 3, "{ren:?}\n{}", r.journal_text());
    // Imprint vide : niveau retiré ; année de début calculée sur la série (2011) ; mois en
    // texte ; {…} imbriqués : « (March, 2012) », et « (2016) » sans mois.
    assert_eq!(
        ren["a"],
        sortie
            .join("DC Comics")
            .join("Batman (2011 Limited Series)")
            .join("Batman Vol.2011 #07 (of 12) (March, 2012).cbz")
    );
    assert_eq!(
        ren["b"],
        sortie.join("DC Comics").join("Batman (2011)").join("Batman Vol.2011 #01 (September, 2011).cbz")
    );
    assert_eq!(ren["c"], sortie.join("DC Comics").join("Batman (2011)").join("Batman Vol.2011 #52 (2016).cbz"));
    // Rien n'a bougé ; journal et fichier d'annulation écrits dans le dossier choisi.
    assert!(entree_dir.join("a.cbz").is_file());
    assert!(r.return_value["journal"].as_str().unwrap().starts_with(&*data.to_string_lossy()));
    let undo = std::fs::read_to_string(data.join("annulation.dat")).unwrap();
    assert_eq!(undo.lines().count(), 3);
    assert!(undo.contains(&format!("P|{}|{}", ren["a"].display(), entree_dir.join("a.cbz").display())));
    // Dossier « entree » vide après application : listé pour la suppression.
    assert_eq!(r.return_value["dossiers_vides"], json!([entree_dir.to_string_lossy()]));
    assert_eq!(r.return_value["profils"]["P"]["succes"], json!(3));

    // Second passage, une fois les fichiers déplacés : rien à faire.
    let mut deplaces = Vec::new();
    for l in &livres {
        let cible = &ren[&l.id];
        std::fs::create_dir_all(cible.parent().unwrap()).unwrap();
        std::fs::rename(l.file.as_ref().unwrap(), cible).unwrap();
        let mut n = l.clone();
        n.file = Some(cible.to_string_lossy().into_owned());
        deplaces.push(n);
    }
    let hote2 = Arc::new(Hote::avec(&deplaces, reglages(&data, profil)));
    let r2 = lancer(&py, "library-organizer-quick", Hook::Books, &["a", "b", "c"], hote2);
    assert!(renommages(&r2).is_empty(), "{:?}", r2.changes);
    assert!(r2.journal_text().contains("déjà à l'emplacement calculé"));

    // Suppression des dossiers vides (après confirmation) : seulement s'ils sont vides.
    let hote3 = Arc::new(Hote::avec(&deplaces, reglages(&data, json!({"Name": "P"}))).reponses(vec![json!(true)]));
    let r3 = lancer(&py, "library-organizer-dossiers-vides", Hook::Library, &[], hote3);
    assert_eq!(r3.return_value["supprimes"], json!([entree_dir.to_string_lossy()]));
    assert!(!entree_dir.exists());
    assert!(sortie.join("DC Comics").is_dir());

    // Annulation du rangement : retour aux anciens chemins (demandé, pas fait).
    let hote4 = Arc::new(Hote::avec(&deplaces, reglages(&data, json!({"Name": "P"}))));
    let r4 = lancer(&py, "library-organizer-annuler", Hook::Library, &["a", "b", "c"], hote4);
    let ren4 = renommages(&r4);
    assert_eq!(ren4["a"], entree_dir.join("a.cbz"));
    assert_eq!(ren4["c"], entree_dir.join("c.cbz"));
    assert_eq!(r4.return_value["succes"], json!(3));
}

#[test]
fn jetons_de_l_original() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let sortie = tmp.path().join("sortie");
    let data = tmp.path().join("data");
    let mut a = livre("a", &fichier(&tmp.path().join("in").join("x.cbz")));
    a.series = Some("The Walking Dead".into());
    a.number = Some("3".into());
    a.title = Some("Safety: Behind Bars".into());
    a.manga = Some("No".into());
    a.page_count = Some(20);
    a.last_page_read = Some(19);
    a.added = Some("2013-02-01T10:20:30".into());
    a.custom_values_store = Some(",edition=Deluxe".into());
    a.writer = Some("Kirkman".into());
    let mut b = livre("b", &fichier(&tmp.path().join("in").join("Saga 12 (2013).cbr")));
    b.manga = Some("YesAndRightToLeft".into());
    let profil = json!({
        "Name": "Jetons", "BaseFolder": sortie.to_string_lossy(), "Mode": "Move",
        // Première lettre sans article, valeur personnalisée, champ multiple à valeur unique.
        "FolderTemplate": "{<first(Series)>}\\{<series>}{ - <writer(, )(issue)>}",
        // Conditionnel, inversion, Oui/Non, pourcentage lu, date .NET, valeur de
        // substitution pour un champ vide, compteur.
        "FileTemplate": "{<series>}{ #<number3>}{ - <title>}{ [<manga(Manga)>]}{ (<Custom(edition)>)}\
    { <?series(The Walking Dead)>TWD}{ <!volume>sans volume}{ <read(lu)(=)(100)>}{ <AddedDate(yyyy-MM)>}{ <counter(5)(5)(2)>}",
        "EmptyData": {"ShadowTitle": "Sans titre"},
    });
    let hote = Arc::new(Hote::avec(&[a, b], reglages(&data, profil)));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &["a", "b"], hote);
    let ren = renommages(&r);
    // « : » → « - » (caractères interdits par défaut de l'original).
    assert_eq!(
        ren["a"],
        sortie
            .join("W")
            .join("The Walking Dead - Kirkman")
            .join("The Walking Dead #003 - Safety - Behind Bars (Deluxe) TWD sans volume lu 2013-02 05.cbz"),
        "{}",
        r.journal_text()
    );
    // Valeurs proposées depuis le nom du fichier ; titre vide → texte de substitution (sans
    // préfixe) ; Manga « de droite à gauche » = Oui ; compteur suivant.
    assert_eq!(ren["b"], sortie.join("S").join("Saga").join("Saga #012Sans titre [Manga] sans volume 10.cbr"));
}

// ---------------------------------------------------------------------------------------
// Règles, simulation, doublons
// ---------------------------------------------------------------------------------------

#[test]
fn regles_simulation_doublons() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let entree_dir = tmp.path().join("entree");
    let sortie = tmp.path().join("sortie");
    let data = tmp.path().join("data");
    let mut livres = Vec::new();
    for (id, serie, editeur, annee) in [
        ("a", "Batman", "DC", 1989),
        ("b", "X-Men", "Marvel", 1991),
        ("c", "Spawn", "Image", 1992),
        ("d", "Batman", "DC", 2005),
    ] {
        let mut l = livre(id, &fichier(&entree_dir.join(format!("{id}.cbz"))));
        l.series = Some(serie.into());
        l.publisher = Some(editeur.into());
        l.year = Some(annee);
        l.number = Some("1".into());
        livres.push(l);
    }
    // Exclu par dossier.
    let mut e = livre("e", &fichier(&tmp.path().join("garder").join("e.cbz")));
    e.series = Some("Batman".into());
    livres.push(e);
    // Déplacer seulement : éditeur DC, ou (Marvel et année < 1990) — b ne correspond pas.
    let regles = json!([
        {"type": "regle", "champ": "Publisher", "operateur": "is", "valeur": "DC"},
        {"type": "groupe", "operateur": "All", "regles": [
            {"type": "regle", "champ": "Publisher", "operateur": "is", "valeur": "Marvel"},
            {"type": "regle", "champ": "Year", "operateur": "less than", "valeur": "1990"}]},
    ]);
    let profil = |mode: &str| {
        json!({"Name": "R", "BaseFolder": sortie.to_string_lossy(), "Mode": mode, "UseFolder": true,
               "FolderTemplate": "{<publisher>}", "FileTemplate": "{<series>}{ #<number>}",
               "ExcludeMode": "Only", "ExcludeOperator": "Any", "ExcludeRules": regles,
               "ExcludeFolders": [tmp.path().join("garder").to_string_lossy()]})
    };
    let ids = ["a", "b", "c", "d", "e"];

    // Simulation : aucune demande de renommage, rapport complet.
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil("Simulate"))));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &ids, hote.clone());
    assert!(r.changes.is_empty(), "{:?}", r.changes);
    let journal = std::fs::read_to_string(r.return_value["journal"].as_str().unwrap()).unwrap();
    assert!(journal.contains("Déplacé (simulé)"), "{journal}");
    assert!(journal.contains("exclu par les règles"));
    assert!(journal.contains("dossier exclu"));
    // a et d donnent le même nom : le second est un doublon (fenêtre ; sans réponse = ne
    // pas déplacer).
    assert_eq!(r.return_value["profils"]["R"]["succes"], json!(1), "{journal}");
    assert!(hote.vus.lock().unwrap().iter().any(|f| f.title == "Ce fichier existe déjà"));
    assert!(!std::fs::read_dir(&data).unwrap().any(|f| f.unwrap().file_name() == "annulation.dat"));

    // Déplacer, avec un fichier déjà présent : « garder les deux » (pour tous) → « (1) », « (2) ».
    let occupe = sortie.join("DC").join("Batman #1.cbz");
    fichier(&occupe);
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil("Move"))).formulaires(vec![(
        "Ce fichier existe déjà",
        "Déplacer, mais garder les deux",
        json!({"tous": true}),
    )]));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &ids, hote.clone());
    let ren = renommages(&r);
    assert_eq!(ren.len(), 2, "{ren:?}\n{}", r.journal_text());
    let mut cibles: Vec<PathBuf> = ren.values().cloned().collect();
    cibles.sort();
    assert_eq!(cibles, vec![sortie.join("DC").join("Batman #1 (1).cbz"), sortie.join("DC").join("Batman #1 (2).cbz")]);
    assert_eq!(hote.vus.lock().unwrap().iter().filter(|f| f.title == "Ce fichier existe déjà").count(), 1);
    assert_eq!(std::fs::read(&occupe).unwrap(), b"x", "jamais écrasé");

    // « Ne pas déplacer » : aucun renommage.
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil("Move"))).formulaires(vec![
        ("Ce fichier existe déjà", "Ne pas déplacer", json!({})),
        ("Ce fichier existe déjà", "Ne pas déplacer", json!({})),
    ]));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &ids, hote);
    assert!(renommages(&r).is_empty());
    assert_eq!(r.return_value["profils"]["R"]["echecs"], json!(0));

    // Mode Copier : impossible dans ComicRust → simulation signalée.
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil("Copy"))));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &ids, hote);
    assert!(r.changes.is_empty());
    assert!(r.journal_text().contains("mode Copier"));
}

// ---------------------------------------------------------------------------------------
// Import des réglages de ComicRack (losettingsx.dat) et configuration
// ---------------------------------------------------------------------------------------

/// Fichier de profils au format de l'original (losettings.save_profiles) : deux profils,
/// règles avec groupe imbriqué, attribut ancien « Text » d'une règle 1.7, mois et
/// caractères interdits partiels (complétés par les valeurs par défaut).
const LOSETTINGS: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>
<Profiles LastUsed=\"Comics,Manga\">
  <Profile Name=\"Comics\" Version=\"2.1\">
    <FolderTemplate>{&lt;publisher&gt;}\\{&lt;series&gt;}</FolderTemplate>
    <BaseFolder>D:\\BD</BaseFolder>
    <FileTemplate>{&lt;series&gt;}{ #&lt;number3&gt;}</FileTemplate>
    <EmptyFolder>Inconnu</EmptyFolder>
    <EmptyData><Item Name=\"Publisher\" Value=\"Sans éditeur\" /></EmptyData>
    <Postfix><Item Name=\"Year\" Value=\")\" /></Postfix>
    <Prefix><Item Name=\"Number\" Value=\"#\" /><Item Name=\"Year\" Value=\"(\" /></Prefix>
    <Seperator><Item Name=\"Writer\" Value=\", \" /></Seperator>
    <IllegalCharacters><Item Name=\":\" Value=\" -\" /></IllegalCharacters>
    <Months><Item Name=\"1\" Value=\"Janvier\" /></Months>
    <TextBox><Item Name=\"Manga\" Value=\"Manga\" /></TextBox>
    <UseFolder>true</UseFolder>
    <UseFileName>true</UseFileName>
    <ExcludeFolders><Item>D:\\BD\\A trier</Item></ExcludeFolders>
    <DontAskWhenMultiOne>false</DontAskWhenMultiOne>
    <ExcludeOperator>All</ExcludeOperator>
    <RemoveEmptyFolder>false</RemoveEmptyFolder>
    <ExcludedEmptyFolder />
    <MoveFileless>false</MoveFileless>
    <FilelessFormat>.png</FilelessFormat>
    <ExcludeMode>Only</ExcludeMode>
    <FailEmptyValues>true</FailEmptyValues>
    <MoveFailed>true</MoveFailed>
    <FailedFolder>D:\\BD\\Echecs</FailedFolder>
    <FailedFields><Item>ShadowSeries</Item></FailedFields>
    <Mode>Simulate</Mode>
    <CopyMode>false</CopyMode>
    <AutoSpaceFields>false</AutoSpaceFields>
    <ReplaceMultipleSpaces>true</ReplaceMultipleSpaces>
    <CopyReadPercentage>true</CopyReadPercentage>
    <ExcludeRules Operator=\"All\" ExcludeMode=\"Only\">
      <ExcludeRule Field=\"Publisher\" Operator=\"is not\" Value=\"Marvel\" />
      <ExcludeGroup Operator=\"Any\">
        <ExcludeRule Field=\"Year\" Operator=\"greater than\" Value=\"2000\" />
        <ExcludeGroup Operator=\"All\">
          <ExcludeRule Field=\"Manga\" Operator=\"is\" Text=\"Yes (Right to Left)\" />
        </ExcludeGroup>
      </ExcludeGroup>
    </ExcludeRules>
  </Profile>
  <Profile Name=\"Manga\" Version=\"2.1\">
    <FolderTemplate>{&lt;series&gt;}</FolderTemplate>
    <BaseFolder>E:\\Mangas</BaseFolder>
    <FileTemplate>{&lt;series&gt;}</FileTemplate>
    <Mode>Move</Mode>
    <ExcludeRules Operator=\"Any\" ExcludeMode=\"Do not\" />
  </Profile>
</Profiles>
";

#[test]
fn import_des_reglages_comicrack() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let dat = tmp.path().join("losettingsx.dat");
    std::fs::write(&dat, LOSETTINGS).unwrap();
    let export = tmp.path().join("export.lop");
    let hote = Arc::new(
        Hote::avec(&[], json!({"dossier_journal": tmp.path().join("data").to_string_lossy()})).formulaires(vec![
            ("Library Organizer", "importer_cr", json!({})),
            ("Importer les réglages", "Importer", json!({"fichier": dat.to_string_lossy(), "remplacer": true})),
            // Profil importé affiché : « Comics » (premier des derniers utilisés).
            ("Library Organizer", "exporter_tout", json!({})),
            ("Exporter", "Exporter", json!({"fichier": export.to_string_lossy()})),
            ("Library Organizer", "OK", json!({})),
        ]),
    );
    lancer(&py, "library-organizer-reglages", Hook::ConfigScript, &[], hote.clone());
    let s = hote.derniers_reglages();
    let profils = s["profils"].as_object().unwrap();
    let mut noms: Vec<&String> = profils.keys().collect();
    noms.sort();
    assert_eq!(noms, ["Comics", "Manga"], "le profil par défaut intact est remplacé");
    assert_eq!(s["derniers"], json!(["Comics", "Manga"]));
    let c = &profils["Comics"];
    assert_eq!(c["FolderTemplate"], json!("{<publisher>}\\{<series>}"));
    assert_eq!(c["BaseFolder"], json!("D:\\BD"));
    assert_eq!(c["Mode"], json!("Simulate"));
    assert_eq!(c["FilelessFormat"], json!(".png"));
    assert_eq!(c["DontAskWhenMultiOne"], json!(false));
    assert_eq!(c["FailedFields"], json!(["ShadowSeries"]));
    assert_eq!(c["EmptyData"], json!({"Publisher": "Sans éditeur"}));
    assert_eq!(c["Prefix"]["Number"], json!("#"));
    assert_eq!(c["Months"]["1"], json!("Janvier"));
    assert_eq!(c["Months"]["2"], json!("February"), "mois complétés par les valeurs par défaut");
    assert_eq!(c["IllegalCharacters"][":"], json!(" -"));
    assert_eq!(c["IllegalCharacters"]["?"], json!(""), "caractères par défaut gardés");
    assert_eq!(c["ExcludeMode"], json!("Only"));
    assert_eq!(c["ExcludeOperator"], json!("All"));
    assert_eq!(
        c["ExcludeRules"],
        json!([
            {"type": "regle", "champ": "Publisher", "operateur": "is not", "valeur": "Marvel"},
            {"type": "groupe", "operateur": "Any", "regles": [
                {"type": "regle", "champ": "Year", "operateur": "greater than", "valeur": "2000"},
                {"type": "groupe", "operateur": "All", "regles": [
                    {"type": "regle", "champ": "Manga", "operateur": "is", "valeur": "Yes (Right to Left)"}]}]}
        ])
    );
    assert_eq!(profils["Manga"]["ExcludeMode"], json!("Do not"));
    // La fenêtre de configuration a été rouverte sur le profil importé, onglets de l'original.
    let vus = hote.vus.lock().unwrap();
    let conf: Vec<&FormSpec> = vus.iter().filter(|f| f.title.starts_with("Library Organizer")).collect();
    assert_eq!(conf.len(), 3);
    assert_eq!(conf[1].values["profil"], json!("Comics"));
    assert_eq!(conf[1].values["modele_dossier"], json!("{<publisher>}\\{<series>}"));
    // Règles passées par les tableaux du formulaire sans perte (groupe imbriqué « 1.1 »).
    assert_eq!(conf[1].values["groupes"], json!([["1", "Any"], ["1.1", "All"]]));
    // Export au format de l'original, relisible par ComicRack.
    let x = std::fs::read_to_string(&export).unwrap();
    assert!(x.contains("<Profiles LastUsed=\"Comics,Manga\">"), "{x}");
    assert!(x.contains("<ExcludeRules Operator=\"All\" ExcludeMode=\"Only\">"));
    assert!(x.contains("<ExcludeRule Field=\"Manga\" Operator=\"is\" Value=\"Yes (Right to Left)\" />"));
    assert!(x.contains("<UseFolder>true</UseFolder>"));
}

/// Configuration : aperçu, nouveau profil, contrôle des réglages (dossier de base
/// obligatoire, comme check_profile_settings de l'original).
#[test]
fn configuration_et_apercu() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let a = batman("a", &fichier(&tmp.path().join("in").join("a.cbz")), "7", 2012, 3);
    let base = tmp.path().join("BD");
    let reglages_profil = json!({
        "dossier_base": base.to_string_lossy(), "modele_dossier": "{<series>}",
        "modele_fichier": "{<series>}{ #<number3>}"});
    let hote = Arc::new(
        Hote::avec(&[a], json!({"dossier_journal": tmp.path().join("data").to_string_lossy()}))
            .formulaires(vec![
                ("Library Organizer", "apercu", reglages_profil.clone()),
                ("Library Organizer", "nouveau", json!({})),
                // Nouveau profil vide : OK refusé (dossier de base, structure de fichier).
                ("Library Organizer", "OK", json!({})),
                ("Library Organizer", "OK", reglages_profil),
            ])
            .reponses(vec![json!("Séries")]),
    );
    lancer(&py, "library-organizer-configurer", Hook::Library, &["a"], hote.clone());
    let vus = hote.vus.lock().unwrap();
    assert_eq!(vus.len(), 4, "{:?}", vus.iter().map(|f| &f.title).collect::<Vec<_>>());
    // Aperçu affiché dans les onglets Fichiers / Dossiers.
    let texte = format!("{:?}", vus[1].controls);
    assert!(texte.contains("Batman #007.cbz"), "{texte}");
    assert!(texte.contains(&*base.join("Batman").to_string_lossy()), "{texte}");
    assert_eq!(vus[2].values["profil"], json!("Séries"));
    assert!(vus[3].message.contains("dossier de base"), "{}", vus[3].message);
    let s = hote.derniers_reglages();
    assert_eq!(s["profils"]["Séries"]["FileTemplate"], json!("{<series>}{ #<number3>}"));
    assert_eq!(s["profils"]["Défaut"]["FolderTemplate"], json!("{<series>}"));
}

/// Point d'entrée principal : configuration (OK) puis rangement ; Annuler = rien.
#[test]
fn library_organizer_principal() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let a = batman("a", &fichier(&tmp.path().join("in").join("a.cbz")), "7", 2012, 3);
    let base = tmp.path().join("BD");
    let valeurs = json!({"dossier_base": base.to_string_lossy(), "modele_dossier": "{<publisher>}",
                         "modele_fichier": "{<series>}{ #<number3>}"});
    let data = json!({"dossier_journal": tmp.path().join("data").to_string_lossy()});
    let hote = Arc::new(Hote::avec(std::slice::from_ref(&a), data.clone()).formulaires(vec![(
        "Library Organizer",
        "OK",
        valeurs,
    )]));
    let r = lancer(&py, "library-organizer", Hook::Books, &["a"], hote);
    assert_eq!(renommages(&r)["a"], base.join("DC Comics").join("Batman #007.cbz"));
    // Fenêtre fermée : aucun changement, rien d'enregistré.
    let hote = Arc::new(Hote::avec(&[a], data));
    let r = lancer(&py, "library-organizer", Hook::Books, &["a"], hote.clone());
    assert!(r.changes.is_empty());
    assert!(hote.enregistres.lock().unwrap().is_empty());
}
