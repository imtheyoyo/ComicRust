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
use comicrust_core::scripting::forms::ControlKind;
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
    /// Ordre de la bibliothèque (`library_ids`).
    ordre: Vec<String>,
    /// Bilan des renommages appliqués rendu à `applied_renames` (`Null` : aucun).
    bilan: Value,
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
    fn library_ids(&self) -> Option<Vec<String>> {
        Some(self.ordre.clone())
    }
    fn applied_renames(&self, settings_key: &str) -> Value {
        assert_eq!(settings_key, "exemples/library_organizer.py");
        self.bilan.clone()
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
        let mut h = Hote {
            livres: livres.iter().map(|b| (b.id.clone(), book_json(b))).collect(),
            ordre: livres.iter().map(|b| b.id.clone()).collect(),
            ..Default::default()
        };
        if let Value::Object(m) = reglages {
            h.reglages.values = m;
        }
        h
    }
    fn formulaires(self, f: Vec<(&str, &str, Value)>) -> Hote {
        *self.formulaires.lock().unwrap() = f.into_iter().map(|(t, b, v)| (t.into(), b.into(), v)).collect();
        self
    }
    /// Bilan des renommages appliqués par ComicRust (`entree` : point d'entrée appliqué).
    fn bilan(mut self, entree: &str, renommages: &[(&str, &Path, &Path)]) -> Hote {
        self.bilan = json!({
            "script": "exemples/library_organizer.py", "entry": entree, "function": "f",
            "date": "2026-10-31T10:20:30+01:00",
            "renames": renommages.iter().map(|(id, de, vers)| json!({
                "book": id, "from": de.to_string_lossy(), "to": vers.to_string_lossy()})).collect::<Vec<_>>(),
        });
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
    // Rangement demandé (profils) : l'annulation ne s'en sert que pour les profils.
    let demande: Value =
        serde_json::from_str(&std::fs::read_to_string(data.join("rangement_demande.json")).unwrap()).unwrap();
    let entrees = demande["entrees"].as_array().unwrap();
    assert_eq!(entrees.len(), 3);
    assert!(entrees.contains(&json!({"profil": "P", "nouveau": ren["a"].to_string_lossy(),
                                      "ancien": entree_dir.join("a.cbz").to_string_lossy()})));
    assert!(!data.join("annulation.dat").exists());
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

    // Annulation du rangement d'après le bilan des renommages appliqués : retour aux
    // anciens chemins (demandé, pas fait).
    let appliques: Vec<(&str, &Path, &Path)> =
        livres.iter().map(|l| (l.id.as_str(), Path::new(l.file.as_deref().unwrap()), ren[&l.id].as_path())).collect();
    let hote4 = Arc::new(
        Hote::avec(&deplaces, reglages(&data, json!({"Name": "P"}))).bilan("library-organizer-quick", &appliques),
    );
    let r4 = lancer(&py, "library-organizer-annuler", Hook::Library, &["a", "b", "c"], hote4);
    let ren4 = renommages(&r4);
    assert_eq!(ren4["a"], entree_dir.join("a.cbz"));
    assert_eq!(ren4["c"], entree_dir.join("c.cbz"));
    assert_eq!(r4.return_value["succes"], json!(3));
}

/// Annulation : seuls les renommages **appliqués** (bilan de ComicRust) sont annulés, avec
/// le profil du rangement demandé (« | » dans son nom compris) ; rien sans bilan ; une
/// annulation appliquée n'est pas rejouée.
#[test]
fn annulation_des_renommages_appliques() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let entree_dir = tmp.path().join("entree");
    let sortie = tmp.path().join("sortie");
    let data = tmp.path().join("data");
    let livres = [
        batman("a", &fichier(&entree_dir.join("a.cbz")), "1", 2011, 9),
        batman("b", &fichier(&entree_dir.join("b.cbz")), "2", 2011, 10),
    ];
    let profil = json!({"Name": "Mes|BD", "BaseFolder": sortie.to_string_lossy(), "Mode": "Move",
                        "FolderTemplate": "{<series>}", "FileTemplate": "{<series>} {<number>}"});
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil.clone())));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &["a", "b"], hote);
    let ren = renommages(&r);
    assert_eq!(ren.len(), 2);

    // Aperçu abandonné (aucun bilan) : rien à annuler, même si le rangement a été demandé.
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil.clone())));
    let r = lancer(&py, "library-organizer-annuler", Hook::Library, &["a", "b"], hote.clone());
    assert!(r.changes.is_empty(), "{:?}", r.changes);
    assert!(hote.messages.lock().unwrap()[0].contains("Rien à annuler"));

    // Seul « a » a réellement été déplacé (b refusé à l'application).
    let mut a = livres[0].clone();
    std::fs::create_dir_all(ren["a"].parent().unwrap()).unwrap();
    std::fs::rename(a.file.as_ref().unwrap(), &ren["a"]).unwrap();
    a.file = Some(ren["a"].to_string_lossy().into_owned());
    let bib = [a, livres[1].clone()];
    let hote = Arc::new(
        Hote::avec(&bib, reglages(&data, profil.clone()))
            .bilan("library-organizer-quick", &[("a", &entree_dir.join("a.cbz"), &ren["a"])]),
    );
    let r = lancer(&py, "library-organizer-annuler", Hook::Library, &["a", "b"], hote);
    let ren2 = renommages(&r);
    assert_eq!(ren2.len(), 1, "{ren2:?}");
    assert_eq!(ren2["a"], entree_dir.join("a.cbz"));
    assert_eq!(r.return_value["succes"], json!(1));
    assert!(r.journal_text().contains("[Mes|BD] Déplacement demandé"), "{}", r.journal_text());

    // L'annulation a été appliquée (bilan de « annuler ») : pas rejouée.
    let hote = Arc::new(
        Hote::avec(&livres, reglages(&data, profil))
            .bilan("library-organizer-annuler", &[("a", &ren["a"], &entree_dir.join("a.cbz"))]),
    );
    let r = lancer(&py, "library-organizer-annuler", Hook::Library, &["a", "b"], hote.clone());
    assert!(r.changes.is_empty());
    assert_eq!(r.return_value, json!({"deja_annule": true}));
    assert!(hote.messages.lock().unwrap()[0].contains("déjà été annulé"));
}

/// Valeurs calculées sur la série : toute la bibliothèque compte, pas seulement les livres
/// sélectionnés (GetLibraryBooks de l'original).
#[test]
fn valeurs_calculees_sur_toute_la_bibliotheque() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let sortie = tmp.path().join("sortie");
    let data = tmp.path().join("data");
    let mut b1 = livre("b1", &fichier(&tmp.path().join("in").join("b1.cbz")));
    let mut b2 = livre("b2", &fichier(&tmp.path().join("in").join("b2.cbz")));
    for (b, n, an) in [(&mut b1, "1", 2000), (&mut b2, "2", 1999)] {
        b.series = Some("Blacksad".into());
        b.publisher = Some("Dargaud".into());
        b.number = Some(n.into());
        b.year = Some(an);
    }
    let profil = json!({"Name": "P", "BaseFolder": sortie.to_string_lossy(), "Mode": "Move",
                        "FolderTemplate": "{<series>}{ (<startyear>)}",
                        "FileTemplate": "{<series>} {<number>} sur {<lastissuenumber>}"});
    let livres = [b1, b2];
    let attendu = sortie.join("Blacksad (1999)").join("Blacksad 1 sur 2.cbz");
    // Les deux sélectionnés.
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil.clone())));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &["b1", "b2"], hote);
    assert_eq!(renommages(&r)["b1"], attendu);
    // Blacksad 1 seul sélectionné, Blacksad 2 seulement dans la bibliothèque : même résultat.
    let hote = Arc::new(Hote::avec(&livres, reglages(&data, profil)));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &["b1"], hote);
    let ren = renommages(&r);
    assert_eq!(ren.len(), 1);
    assert_eq!(ren["b1"], attendu, "{}", r.journal_text());
}

/// Dossiers vides : jamais au-dessus du dossier de base du profil, ni le dossier de base
/// lui-même, ni une racine ; chemins normalisés, sans doublon.
#[test]
fn dossiers_vides_bornes_au_dossier_de_base() {
    let py = need_python!();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("BD");
    let ancien = base.join("A trier").join("Vrac");
    let a = batman("a", &fichier(&ancien.join("a.cbz")), "1", 2011, 9);
    // Dossier de base donné avec une barre finale.
    let profil = json!({"Name": "P", "BaseFolder": format!("{}/", base.to_string_lossy()), "Mode": "Move",
                        "RemoveEmptyFolder": true,
                        "FolderTemplate": "{<series>}", "FileTemplate": "{<series>} {<number>}"});
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    // Liste existante : doublon avec barre finale et racine (ajoutée à la main).
    std::fs::write(data.join("dossiers_vides.txt"), format!("{}/\n/\n", ancien.to_string_lossy())).unwrap();
    let hote = Arc::new(Hote::avec(&[a], reglages(&data, profil)));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &["a"], hote);
    assert_eq!(renommages(&r)["a"], base.join("Batman").join("Batman 1.cbz"));
    // « Vrac » puis « A trier » (devenu vide), mais pas « BD » ni au-dessus.
    assert_eq!(
        r.return_value["dossiers_vides"],
        json!([ancien.to_string_lossy(), base.join("A trier").to_string_lossy()])
    );
    let liste = std::fs::read_to_string(data.join("dossiers_vides.txt")).unwrap();
    assert_eq!(
        liste,
        format!("{}\n{}\n", ancien.to_string_lossy(), base.join("A trier").to_string_lossy()),
        "normalisée, sans doublon ni racine"
    );
    // Hors du dossier de base : seul le dossier du livre, sans remonter.
    let ailleurs = tmp.path().join("ailleurs").join("sous");
    let b = batman("b", &fichier(&ailleurs.join("b.cbz")), "2", 2011, 9);
    let profil = json!({"Name": "P", "BaseFolder": base.to_string_lossy(), "Mode": "Move", "RemoveEmptyFolder": true,
                        "FolderTemplate": "{<series>}", "FileTemplate": "{<series>} {<number>}"});
    let hote = Arc::new(Hote::avec(&[b], reglages(&data, profil)));
    let r = lancer(&py, "library-organizer-quick", Hook::Books, &["b"], hote);
    assert_eq!(r.return_value["dossiers_vides"], json!([ailleurs.to_string_lossy()]));
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
    // Menu « Insérer ▾ » par catégorie de jetons (groupes de l'interface, avec filtre).
    let groupes = |cle: &str| -> Vec<String> {
        let ControlKind::Text { inserts, .. } = &vus[0].control(cle).unwrap().kind else { panic!("{cle}") };
        inserts.iter().map(|g| g.title.clone()).collect()
    };
    assert_eq!(
        groupes("modele_fichier"),
        ["Texte", "Nombres", "Oui/Non", "Valeurs multiples", "Calculées", "Conditions"]
    );
    assert_eq!(groupes("modele_dossier")[0], "Dossiers");
    let ControlKind::Text { inserts, .. } = &vus[0].control("modele_fichier").unwrap().kind else { panic!() };
    let calc = inserts.iter().find(|g| g.title == "Calculées").unwrap();
    assert!(
        calc.items.iter().any(|i| i.label == "Année de début" && i.value.as_str().unwrap().contains("<startyear>")),
        "{calc:?}"
    );
    // Aperçu affiché dans les onglets Fichiers / Dossiers.
    let texte = format!("{:?}", vus[1].controls);
    assert!(texte.contains("Batman #007.cbz"), "{texte}");
    // `{:?}` double les `\` sous Windows : chercher le chemin sous sa forme échappée.
    let dossier = format!("{:?}", base.join("Batman").to_string_lossy());
    assert!(texte.contains(dossier.trim_matches('"')), "{texte}");
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
