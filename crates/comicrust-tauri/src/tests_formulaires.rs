//! Formulaires **réels** des scripts d'exemple (tauri-v0.9.1) : chaque fenêtre de réglages ou de
//! saisie est produite par le vrai script (Python 3 du système, test ignoré s'il manque), passée
//! par le cœur (`FormSpec`) puis par `formulaire_json` comme pour le front. On vérifie que :
//! - tous les types de contrôles rencontrés sont connus du front (`ControlesScript.svelte`) ;
//! - le JSON envoyé au front est relu à l'identique par le cœur ;
//! - la liste « Données » de « Configurer BD2 » (anomalie de la tauri-v0.9.0) est bien transmise,
//!   avec tous ses champs ;
//! - les réglages de « Ranger les fichiers » (trois questions, comprises aussi par la version egui)
//!   expliquent les champs utilisables et montrent l'aperçu du nom sur le premier livre.
//!
//! `COMICRUST_DUMP_FORMULAIRES=<dossier>` : écrit aussi chaque formulaire en JSON (pour le rendu
//! simulé avec Playwright et `simuleScripts.ts`).

use crate::scripts::formulaires::formulaire_json;
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{
    discover_with_warnings, find_python, FormAnswer, FormSpec, Hook, Question, RunContext, RunOptions, RunResult,
    ScriptHost, ScriptRunner, ScriptSettings,
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Types de contrôles rendus par le front (`ControlesScript.svelte`) : doit couvrir tous ceux
/// de `comicrust_core::scripting::forms`.
const TYPES_FRONT: &[&str] = &[
    "text",
    "textarea",
    "secret",
    "checkbox",
    "number",
    "choice",
    "radio",
    "multichoice",
    "list_edit",
    "table",
    "folder",
    "file",
    "label",
    "separator",
    "image",
    "group",
    "columns",
    "tabs",
    "button",
];

/// Hôte de test : garde chaque formulaire demandé et l'annule (le script s'arrête) ; garde les
/// questions (réponse par défaut) et les réglages enregistrés ; `bibliotheque` : les livres
/// forment aussi toute la bibliothèque (`comicrust.library_books()`).
struct Hote {
    livres: Vec<Value>,
    formulaires: Mutex<Vec<Value>>,
    questions: Mutex<Vec<Question>>,
    enregistres: Mutex<Vec<ScriptSettings>>,
    bibliotheque: bool,
}

impl ScriptHost for Hote {
    fn books(&self, ids: &[String]) -> Vec<Value> {
        self.livres.iter().filter(|l| ids.iter().any(|i| l["Id"] == *i)).cloned().collect()
    }
    fn load_settings(&self, _key: &str) -> ScriptSettings {
        ScriptSettings::default()
    }
    fn save_settings(&self, _key: &str, s: &ScriptSettings) -> Result<(), String> {
        self.enregistres.lock().unwrap().push(s.clone());
        Ok(())
    }
    fn ask(&self, q: &Question) -> Option<Value> {
        self.questions.lock().unwrap().push(q.clone());
        None
    }
    fn library_ids(&self) -> Option<Vec<String>> {
        self.bibliotheque.then(|| self.livres.iter().filter_map(|l| l["Id"].as_str().map(str::to_string)).collect())
    }
    fn form(&self, f: &FormSpec) -> Option<FormAnswer> {
        self.formulaires.lock().unwrap().push(formulaire_json(f));
        None
    }
}

fn exemples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts-exemples")
}

/// Lance l'entrée `cle` (interactive) avec l'hôte de test.
fn lancer(py: &Path, cle: &str, hook: Hook, livres: Vec<LibraryBook>, bibliotheque: bool) -> (RunResult, Arc<Hote>) {
    let (entrees, _) = discover_with_warnings(&exemples());
    let entree = entrees.into_iter().find(|e| e.key.as_deref() == Some(cle)).expect("entrée présente");
    let module = tempfile::tempdir().unwrap();
    let options = RunOptions {
        python: Some(py.to_path_buf()),
        module_dir: Some(module.path().to_path_buf()),
        scripts_dir: Some(exemples()),
        timeout: Some(Duration::from_secs(120)),
        ..Default::default()
    };
    let ids = if bibliotheque { Vec::new() } else { livres.iter().map(|l| l.id.clone()).collect() };
    let hote = Arc::new(Hote {
        livres: livres.iter().map(comicrust_core::scripting::book_json).collect(),
        formulaires: Mutex::new(Vec::new()),
        questions: Mutex::new(Vec::new()),
        enregistres: Mutex::new(Vec::new()),
        bibliotheque,
    });
    let mut ctx = RunContext::new(hook, ids);
    ctx.interactive = true;
    let r = ScriptRunner::new(options).start(&entree, ctx, hote.clone()).wait();
    (r, hote)
}

/// Premier formulaire ouvert par l'entrée `cle` (annulé aussitôt).
fn formulaire(py: &Path, cle: &str, hook: Hook, livres: Vec<LibraryBook>) -> Value {
    let (r, hote) = lancer(py, cle, hook, livres, false);
    let f = hote.formulaires.lock().unwrap().first().cloned();
    f.unwrap_or_else(|| panic!("{cle} : aucun formulaire ({:?})\n{}", r.failure, r.journal_text()))
}

/// Contrôles de tout niveau (onglets, groupes, colonnes compris).
fn tous(controles: &Value, sortie: &mut Vec<Value>) {
    for c in controles.as_array().into_iter().flatten() {
        sortie.push(c.clone());
        tous(&c["controls"], sortie);
        for col in c["columns"].as_array().into_iter().flatten() {
            tous(col, sortie);
        }
        for t in c["tabs"].as_array().into_iter().flatten() {
            tous(&t["controls"], sortie);
        }
    }
}

#[test]
fn formulaires_reels_des_scripts_d_exemple() {
    let Ok(py) = find_python(Some(Path::new("/nulle-part"))) else {
        eprintln!("python3 absent : test ignoré");
        return;
    };
    let livre = LibraryBook {
        id: "a".into(),
        file: Some("/bd/Blacksad - T01 - Quelque part entre les ombres (2000).cbz".into()),
        series: Some("Blacksad".into()),
        number: Some("1".into()),
        ..Default::default()
    };
    let cas: Vec<(&str, &str, Value)> = vec![
        ("configurer-bd2", "Bedetheque2Config", formulaire(&py, "Bedetheque2Config", Hook::ConfigScript, vec![])),
        ("quickscrape-bd2", "QuickScrapeBD2", formulaire(&py, "QuickScrapeBD2", Hook::Books, vec![livre])),
        (
            "library-organizer",
            "library-organizer-reglages",
            formulaire(&py, "library-organizer-reglages", Hook::ConfigScript, vec![]),
        ),
    ];
    let dump = std::env::var_os("COMICRUST_DUMP_FORMULAIRES").map(PathBuf::from);
    for (nom, cle, f) in &cas {
        let mut liste = Vec::new();
        tous(&f["controls"], &mut liste);
        assert!(liste.len() > 3, "{cle} : formulaire trop court");
        for c in &liste {
            let t = c["type"].as_str().unwrap_or_default();
            assert!(TYPES_FRONT.contains(&t), "{cle} : type « {t} » inconnu du front");
        }
        // Relu à l'identique par le cœur (le front renvoie ce format).
        let relu = FormSpec::parse(f.as_object().unwrap()).unwrap_or_else(|e| panic!("{cle} : {e}"));
        assert_eq!(formulaire_json(&relu)["controls"], f["controls"], "{cle}");
        if let Some(d) = &dump {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join(format!("{nom}.json")), serde_json::to_string_pretty(f).unwrap()).unwrap();
        }
    }

    // « Configurer BD2 », onglet « Données » : la liste à cocher des champs, avec ses libellés,
    // ses valeurs cochées et une hauteur, dans une colonne d'un onglet.
    let bd2 = &cas[0].2;
    let onglets = bd2["controls"][0]["tabs"].as_array().expect("onglets");
    assert_eq!(onglets.len(), 3);
    let mut donnees = Vec::new();
    tous(&onglets[1]["controls"], &mut donnees);
    let liste = donnees.iter().find(|c| c["key"] == "Donnees").expect("liste « Donnees »");
    assert_eq!(liste["type"], "multichoice");
    let choix = liste["choices"].as_array().unwrap();
    assert!(choix.len() >= 20, "{} champs", choix.len());
    assert!(choix.iter().all(|c| c["label"].as_str().is_some_and(|l| !l.is_empty())));
    let coches = bd2["values"]["Donnees"].as_array().expect("valeurs cochées");
    assert!(!coches.is_empty());
    let _: &Map<String, Value> = bd2.as_object().unwrap();
}

#[test]
fn reglages_ranger_les_fichiers_aide_et_apercu() {
    let Ok(py) = find_python(Some(Path::new("/nulle-part"))) else {
        eprintln!("python3 absent : test ignoré");
        return;
    };
    let livre = LibraryBook {
        id: "a".into(),
        file: Some("/bd/vrac/asterix.cbz".into()),
        series: Some("Astérix".into()),
        number: Some("7".into()),
        volume: Some(2),
        publisher: Some("Dargaud".into()),
        ..Default::default()
    };
    let (r, hote) = lancer(&py, "ranger-fichiers-reglages", Hook::ConfigScript, vec![livre], true);
    assert!(r.failure.is_none(), "{:?}\n{}", r.failure, r.journal_text());
    let q = hote.questions.lock().unwrap();
    assert_eq!(q.len(), 3, "trois questions, comme avant (version egui, tests du cœur)");
    // Aide : jetons, crochets facultatifs, exemple calculé par le script.
    for morceau in
        ["{series}", "{volume}", "{number}", "{year}", "{title}", "{publisher}", "[ v{volume}]", "{number:000}"]
    {
        assert!(q[0].text.contains(morceau), "aide sans {morceau} :\n{}", q[0].text);
    }
    assert!(q[0].text.contains("→ Blacksad v1 #1 (2000).cbz"), "{}", q[0].text);
    // Aperçu sur le premier livre de la bibliothèque, avec le modèle par défaut.
    assert!(q[1].text.contains("« asterix.cbz »") && q[1].text.contains("→ Astérix v2 #7.cbz"), "{}", q[1].text);
    assert!(q[2].text.contains(r"Dargaud\Astérix\Astérix v2 #7.cbz"), "{}", q[2].text);
    let s = hote.enregistres.lock().unwrap();
    assert_eq!(s[0].values["modele"], "{series}[ v{volume}][ #{number}][ ({year})]");
}
