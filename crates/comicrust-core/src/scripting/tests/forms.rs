//! Tests des formulaires et listes de choix (v0.10.1) : description, validation, et
//! dialogue réel avec `python3` (hôte de test).

use serde_json::{json, Map, Value};

use super::*;
use crate::scripting::forms::*;

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

/// Petit PNG (en-tête suffisant pour la vérification du format côté cœur).
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000IHDR";

fn b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn sample_form() -> Value {
    json!({
        "title": "Réglages",
        "controls": [
            {"type": "tabs", "tabs": [
                {"title": "Général", "controls": [
                    {"type": "text", "key": "nom", "label": "Nom", "default": "x", "required": true},
                    {"type": "checkbox", "key": "debug", "label": "Journal", "default": false},
                    {"type": "number", "key": "max", "label": "Max", "default": 50, "min": 1, "max": 100,
                     "enabled_if": "debug"},
                    {"type": "radio", "key": "langue", "choices": [["fr", "Français"], ["en", "Anglais"]]},
                ]},
                {"title": "Champs", "controls": [
                    {"type": "multichoice", "key": "champs", "choices": ["Series", "Title", "Writer"],
                     "default": ["Writer", "Series", "Inconnu"]},
                    {"type": "list_edit", "key": "exceptions", "default": ["A", "B"]},
                    {"type": "table", "key": "regles", "columns": [
                        "Champ", {"title": "Actif", "type": "checkbox"},
                        {"title": "Op", "type": "choice", "choices": ["est", "contient"]}],
                     "default": [["Series", 1, "contient"], ["Title"]]},
                    {"type": "group", "title": "Dossier", "enabled_if": "langue=fr", "controls": [
                        {"type": "folder", "key": "dossier", "required": true},
                        {"type": "choice", "key": "modele", "choices": ["a", "b"], "editable": true,
                         "default": "libre"},
                    ]},
                    {"type": "button", "key": "nouveau", "label": "Nouveau profil"},
                    {"type": "image", "data": b64(PNG), "caption": "couv"},
                ]},
            ]},
            {"type": "secret", "key": "cle", "label": "Clé"},
        ],
        "buttons": ["OK", "Annuler"],
        "values": {"nom": "Yoyo", "max": "70", "langue": "en", "cle": "fuite"},
    })
}

#[test]
fn formulaire_description_et_valeurs_initiales() {
    let f = FormSpec::parse(&obj(sample_form())).unwrap();
    assert_eq!(f.buttons, ["OK", "Annuler"]);
    assert_eq!(f.cancel, Some(1));
    let keys: Vec<&str> = f.value_controls().iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["nom", "debug", "max", "langue", "champs", "exceptions", "regles", "dossier", "modele", "cle"]);
    let v = f.initial_values();
    assert_eq!(v["nom"], "Yoyo");
    assert_eq!(v["max"], 70, "texte converti en entier");
    assert_eq!(v["langue"], "en");
    // Ordre des choix gardé, valeur inconnue ignorée.
    assert_eq!(v["champs"], json!(["Series", "Writer"]));
    assert_eq!(v["regles"], json!([["Series", true, "contient"], ["Title", false, "est"]]));
    assert_eq!(v["modele"], "libre", "liste éditable : texte libre gardé");
    // Saisie secrète : jamais pré-remplie, même si le script donne une valeur.
    assert_eq!(v["cle"], "");
    assert_eq!(f.images().len(), 1);
    // Identifiants uniques dans l'ordre de lecture.
    let mut ids = Vec::new();
    fn collect(cs: &[Control], ids: &mut Vec<usize>) {
        for c in cs {
            ids.push(c.id);
            for ch in c.children() {
                collect(std::slice::from_ref(ch), ids);
            }
        }
    }
    collect(&f.controls, &mut ids);
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n);
    assert_eq!(n, f.count);
}

#[test]
fn formulaire_validation_et_conditions() {
    let f = FormSpec::parse(&obj(sample_form())).unwrap();
    let mut v = f.initial_values();
    // `max` désactivé (debug décoché) : hors bornes mais pas vérifié ; groupe désactivé
    // (langue = en) : dossier obligatoire vide non signalé.
    v.insert("max".into(), json!(500));
    v.insert("nom".into(), json!("  "));
    let e = f.validate(&v);
    assert_eq!(e.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["nom"], "{e:?}");
    v.insert("debug".into(), json!(true));
    v.insert("langue".into(), json!("fr"));
    v.insert("exceptions".into(), json!(["A", "a"]));
    let e = f.validate(&v);
    let keys: Vec<&str> = e.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["nom", "max", "exceptions", "dossier"], "{e:?}");
    assert!(e[1].message.contains("au plus 100"), "{e:?}");
    let enabled = f.enabled_keys(&v);
    assert!(enabled.contains("dossier") && enabled.contains("max"));

    assert!(Condition::parse("!debug").unwrap().eval(&Map::new()));
    assert!(Condition::parse("langue != en").unwrap().eval(&obj(json!({"langue": "fr"}))));
    assert!(Condition::parse("n=3").unwrap().eval(&obj(json!({"n": 3}))));
    assert!(Condition::parse(" = x").is_err());
}

#[test]
fn formulaire_reponse_remise_en_forme_et_boutons() {
    let f = FormSpec::parse(&obj(sample_form())).unwrap();
    let answer = obj(json!({
        "nom": 12, "debug": "oui", "max": 7.6, "langue": "de", "champs": ["Title", "Title"],
        "regles": [["A", "x", "zzz"]], "inconnue": 1, "cle": "secret",
    }));
    let s = f.sanitize(&answer);
    assert_eq!(s["nom"], "12");
    assert_eq!(s["debug"], true);
    assert_eq!(s["max"], 8);
    assert_eq!(s["langue"], "en", "choix inconnu : valeur initiale");
    assert_eq!(s["champs"], json!(["Title"]));
    assert_eq!(s["regles"], json!([["A", true, "est"]]));
    assert_eq!(s["cle"], "secret");
    assert!(!s.contains_key("inconnue"));
    assert_eq!(f.check_button("OK"), Ok(true));
    assert_eq!(f.check_button("Annuler"), Ok(false));
    assert_eq!(f.check_button("nouveau"), Ok(true));
    assert!(f.check_button("nom").is_err());
    assert!(f.check_button("Supprimer").is_err());
    assert!(f.button_validates("OK") && !f.button_validates("nouveau"));
}

#[test]
fn formulaire_descriptions_refusees() {
    let bad = |v: Value, attendu: &str| {
        let e = FormSpec::parse(&obj(v)).unwrap_err();
        assert!(e.contains(attendu), "{e} (attendu : {attendu})");
    };
    bad(json!({"controls": [{"type": "truc", "key": "a"}]}), "type de contrôle inconnu « truc »");
    bad(json!({"controls": [{"type": "text"}]}), "clé manquante");
    bad(json!({"controls": [{"type": "text", "key": "a"}, {"type": "checkbox", "key": "a"}]}), "deux fois");
    bad(json!({"controls": [{"type": "text", "key": "a", "enabled_if": "b"}]}), "clé inconnue « b »");
    bad(json!({"controls": [{"type": "radio", "key": "a"}]}), "aucun choix");
    bad(json!({"controls": [{"type": "number", "key": "a", "min": 5, "max": 1}]}), "supérieur au maximum");
    bad(json!({"controls": [{"type": "image", "data": "AAAA"}]}), "format d'image non reconnu");
    bad(json!({"controls": [], "buttons": ["OK", "OK"]}), "en double");
    bad(json!({"controls": [], "buttons": ["OK"], "cancel": "Non"}), "absent");
    let big = b64(&[PNG, &vec![0u8; MAX_IMAGE_BYTES]].concat());
    bad(json!({"controls": [{"type": "image", "data": big}]}), "image trop grosse");
    // Imbrication trop profonde.
    let mut deep = json!({"type": "label", "text": "x"});
    for _ in 0..=MAX_DEPTH {
        deep = json!({"type": "group", "controls": [deep]});
    }
    bad(json!({"controls": [deep]}), "trop profondément");
    // Sans boutons : OK / Annuler.
    let f = FormSpec::parse(&obj(json!({"controls": []}))).unwrap();
    assert_eq!(f.buttons, ["OK", "Annuler"]);
    let f = FormSpec::parse(&obj(json!({"controls": [], "buttons": ["Fermer"]}))).unwrap();
    assert_eq!(f.cancel, None, "un seul bouton : pas d'annulation");
}

#[test]
fn liste_de_choix_description() {
    let c = ChooseSpec::parse(&obj(json!({
        "title": "Album", "columns": ["Titre"], "rows": [["A", 1990], "B", ["C"]],
        "images": [b64(PNG), null, null], "default": 7,
    })))
    .unwrap();
    assert_eq!(c.columns, ["Titre", ""]);
    assert_eq!(c.rows[0], ["A", "1990"]);
    assert_eq!(c.rows[1], ["B", ""]);
    assert!(c.images[0].is_some() && c.images[1].is_none());
    assert!(c.default.is_empty(), "rang hors liste ignoré");
    assert!(!c.filter);
    assert_eq!(c.sanitize(&[1]), Some(vec![1]));
    assert_eq!(c.sanitize(&[0, 1]), None, "choix simple : une seule ligne");
    assert_eq!(c.sanitize(&[9]), None);
    let m = ChooseSpec::parse(&obj(json!({"rows": ["a", "b", "c"], "multi": true, "default": [2, 0]}))).unwrap();
    assert_eq!(m.default, [2, 0]);
    assert_eq!(m.sanitize(&[2, 0, 2, 7]), Some(vec![0, 2]));
    assert_eq!(m.sanitize(&[]), Some(vec![]));

    let e = ChooseSpec::parse(&obj(json!({"rows": ["a"], "images": [null, null]}))).unwrap_err();
    assert!(e.contains("2 images pour 1 lignes"), "{e}");
    let big = b64(&[PNG, &vec![0u8; MAX_IMAGE_BYTES]].concat());
    let e = ChooseSpec::parse(&obj(json!({"rows": ["a"], "images": [big]}))).unwrap_err();
    assert!(e.contains("trop grosse"), "{e}");
    let e = ChooseSpec::parse(&obj(json!({"rows": ["a"], "images": ["!!!"]}))).unwrap_err();
    assert!(e.contains("base64"), "{e}");
}

#[test]
fn base64_et_adresses() {
    assert_eq!(decode_base64(&b64(b"ComicRust!")).unwrap(), b"ComicRust!");
    assert_eq!(decode_base64("Q29t\naWM=").unwrap(), b"Comic");
    assert!(check_url("https://www.bedetheque.com/serie-1.html").is_ok());
    assert!(check_url("http://exemple.fr").is_ok());
    for bad in ["file:///etc/passwd", "javascript:alert(1)", "ftp://x.fr", "https://", "pas une adresse"] {
        assert!(check_url(bad).is_err(), "{bad}");
    }
}

// ---------------------------------------------------------------------------------------
// Avec python3
// ---------------------------------------------------------------------------------------

const SCRIPT: &str = r#"
import comicrust
from comicrust import ui

PNG = b"\x89PNG\r\n\x1a\n0000IHDR"

def formulaire(books):
    controls = [
        ui.tabs({
            "Général": [
                ui.text("nom", "Nom", "défaut", required=True, inserts=[("{series}", "Série")]),
                ui.checkbox("debug", "Journal"),
                ui.number("max", "Max", 5, min=1, max=10, enabled_if="debug"),
                ui.secret("cle", "Clé d'API", default="ANCIENNE"),
            ],
            "Divers": [
                ui.choice("langue", "Langue", [("fr", "Français"), ("en", "Anglais")], default="fr"),
                ui.multichoice("champs", "Champs", ["Series", "Title"], default=["Title"]),
                ui.list_edit("exc", "Exceptions", ["A"]),
                ui.table("regles", "Règles", [ui.column("Champ"), ui.column("Actif", "checkbox")], [["S", True]]),
                ui.group("Cadre", [ui.folder("dossier", "Dossier"), ui.file("f", "Fichier", filters=[("Images", ["jpg"])])]),
                ui.columns([ui.image(PNG, "couv")], [ui.label("texte", style="bold"), ui.separator()]),
                ui.button("nouveau", "Nouveau"),
            ],
        }),
    ]
    out = []
    for _ in range(4):
        r = comicrust.form("Réglages", controls, values={"nom": "Yoyo", "cle": "jamais envoyée"})
        out.append(r)
    return out

def choix(books):
    rows = [["Album %d" % i, 2000 + i] for i in range(3)]
    a = comicrust.choose("Album", ["Titre", "Année"], rows, images=[PNG, None, PNG], default=0)
    b = comicrust.choose("Albums", ["Titre"], ["x", "y", "z"], multi=True)
    c = comicrust.choose("Rien", ["Titre"], ["x"])
    try:
        comicrust.choose("Trop", ["T"], ["x"], images=[b"\x89PNG\r\n\x1a\n" + b"0" * (3 * 1024 * 1024)])
        d = "accepté"
    except ValueError as e:
        d = "refusé : %s" % e
    # Sans passer par la vérification du module : refusé par ComicRust.
    try:
        import base64
        big = base64.b64encode(b"\x89PNG\r\n\x1a\n" + b"0" * (3 * 1024 * 1024)).decode()
        comicrust._session().ch.request("choose", title="x", columns=["a"], rows=[["a"]], images=[big])
        e = "accepté"
    except comicrust.ScriptError as x:
        e = "refusé : %s" % x
    return [a, b, c, d, e]

def divers(books):
    out = [comicrust.open_url("https://www.bedetheque.com/")]
    try:
        comicrust.open_url("file:///etc/passwd")
        out.append("accepté")
    except ValueError:
        out.append("refusé")
    try:
        comicrust._session().ch.request("open_url", url="file:///etc/passwd")
        out.append("accepté")
    except comicrust.ScriptError:
        out.append("refusé")
    out.append(comicrust.clipboard_set("copié"))
    comicrust.progress(0.5, "Album", detail="Recherche…", image=PNG)
    comicrust.progress(0.6, "Album")
    comicrust.progress(0.7, "Album", image=b"")
    try:
        comicrust._session().ch.request("form", title="x", controls=[{"type": "truc", "key": "a"}])
        out.append("accepté")
    except comicrust.ScriptError as e:
        out.append(str(e))
    out.append(comicrust.form("x", [ui.text("a")]))
    out.append(comicrust.choose("x", ["a"], ["a"]))
    return out
"#;

#[test]
fn formulaire_avec_python() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(TestHost::default());
    {
        let ok = |values: Value| FormAnswer { button: "OK".into(), values: obj(values) };
        let mut a = host.form_answers.lock().unwrap();
        // 1. OK : valeurs remises en forme ; clé vide = valeur actuelle gardée.
        a.push(Some(ok(json!({"nom": "Moi", "max": 99, "cle": "", "langue": "en", "regles": [["T", 0]]}))));
        // 2. Bouton du formulaire : clé rendue ; nouvelle clé saisie.
        a.push(Some(FormAnswer { button: "nouveau".into(), values: obj(json!({"cle": "NOUVELLE"})) }));
        // 3. Annuler : None.
        a.push(Some(FormAnswer { button: "Annuler".into(), values: Map::new() }));
        // 4. Fenêtre fermée (ou script arrêté) : None.
        a.push(None);
    }
    let r = fx.run(SCRIPT, "formulaire", RunContext::new(Hook::Books, vec![]), host.clone());
    assert_eq!(r.state, RunState::Finished, "{}", r.journal_text());
    let out = r.return_value.as_array().unwrap();
    assert_eq!(out[0][0], "OK");
    let v = &out[0][1];
    assert_eq!(v["nom"], "Moi");
    assert_eq!(v["max"], 99, "remis en forme, la validation est celle de l'interface");
    assert_eq!(v["cle"], "ANCIENNE", "secret vide : valeur actuelle");
    assert_eq!(v["langue"], "en");
    assert_eq!(v["champs"], json!(["Title"]));
    assert_eq!(v["exc"], json!(["A"]));
    assert_eq!(v["regles"], json!([["T", false]]));
    assert_eq!(v["dossier"], "");
    assert_eq!(out[1][0], "nouveau");
    assert_eq!(out[1][1]["cle"], "NOUVELLE");
    assert_eq!(out[1][1]["nom"], "Yoyo", "valeurs initiales du script");
    assert!(out[2].is_null() && out[3].is_null());
    let forms = host.forms.lock().unwrap();
    assert_eq!(forms.len(), 4);
    let f = &forms[0];
    assert_eq!(f.title, "Réglages");
    // La valeur secrète n'est jamais envoyée à l'interface.
    assert!(!f.values.contains_key("cle"));
    let cle = f.control("cle").unwrap();
    assert!(matches!(&cle.kind, ControlKind::Text { secret: true, default, .. } if default.is_empty()));
    assert_eq!(f.images().len(), 1);
    assert!(
        matches!(&f.control("nom").unwrap().kind, ControlKind::Text { inserts, .. } if inserts[0].label == "Série")
    );
}

#[test]
fn liste_de_choix_avec_python() {
    let py = need_python!();
    let fx = Fixture::new(py);
    let host = Arc::new(TestHost::default());
    host.choose_answers.lock().unwrap().extend([Some(vec![2]), Some(vec![2, 0, 9]), None]);
    let r = fx.run(SCRIPT, "choix", RunContext::new(Hook::Books, vec![]), host.clone());
    assert_eq!(r.state, RunState::Finished, "{}", r.journal_text());
    let out = r.return_value.as_array().unwrap();
    assert_eq!(out[0], 2);
    assert_eq!(out[1], json!([0, 2]));
    assert!(out[2].is_null());
    assert!(out[3].as_str().unwrap().starts_with("refusé : image de la ligne 0 : image trop grosse"), "{}", out[3]);
    assert!(out[4].as_str().unwrap().contains("trop grosse"), "{}", out[4]);
    let c = host.chooses.lock().unwrap();
    assert_eq!(c.len(), 3, "les images trop grosses n'atteignent pas l'interface");
    assert_eq!(c[0].rows[1], ["Album 1", "2001"]);
    assert_eq!(c[0].images.iter().filter(|i| i.is_some()).count(), 2);
    assert_eq!(c[0].images[0].as_deref().map(Vec::as_slice), Some(&b"\x89PNG\r\n\x1a\n0000IHDR"[..]));
    assert_eq!(c[0].default, [0]);
    assert!(c[1].multi);
}

#[test]
fn navigateur_presse_papiers_progression_et_sans_interface() {
    let py = need_python!();
    let fx = Fixture::new(py.clone());
    let host = Arc::new(TestHost::default());
    let r = fx.run(SCRIPT, "divers", RunContext::new(Hook::Books, vec![]), host.clone());
    assert_eq!(r.state, RunState::Finished, "{}", r.journal_text());
    let out = r.return_value.as_array().unwrap();
    assert_eq!(out[0], true);
    assert_eq!(out[1], "refusé");
    assert_eq!(out[2], "refusé");
    assert_eq!(out[3], true);
    assert!(out[4].as_str().unwrap().contains("type de contrôle inconnu « truc »"), "{}", out[4]);
    // Interface de test sans réponse : None.
    assert!(out[5].is_null() && out[6].is_null());
    assert_eq!(*host.urls.lock().unwrap(), ["https://www.bedetheque.com/"]);
    assert_eq!(*host.clipboard.lock().unwrap(), ["copié"]);
    let d = host.details.lock().unwrap();
    assert_eq!(d.len(), 2, "progression sans détail : rien de plus ({d:?})");
    assert_eq!(d[0].detail.as_deref(), Some("Recherche…"));
    assert!(matches!(&d[0].image, Some(Some(_))));
    assert_eq!(d[1].image, Some(None), "image retirée");

    // Automatisme : rien n'est montré, None / False.
    let fx = Fixture::new(py);
    let host = Arc::new(TestHost::default());
    host.form_answers.lock().unwrap().push(Some(FormAnswer { button: "OK".into(), values: Map::new() }));
    let ctx = RunContext::new(Hook::NewBooks, vec![]);
    assert!(!ctx.interactive);
    let r = fx.run(SCRIPT, "divers", ctx, host.clone());
    assert_eq!(r.state, RunState::Finished, "{}", r.journal_text());
    let out = r.return_value.as_array().unwrap();
    assert_eq!(out[0], false);
    assert_eq!(out[3], false);
    assert!(out[5].is_null() && out[6].is_null());
    assert!(host.forms.lock().unwrap().is_empty() && host.chooses.lock().unwrap().is_empty());
    assert!(host.urls.lock().unwrap().is_empty() && host.clipboard.lock().unwrap().is_empty());
    assert!(r.journal_text().contains("script sans interface"), "{}", r.journal_text());
}
