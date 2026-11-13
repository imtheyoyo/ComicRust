//! Tests des scripts Python (`scripts.rs`) avec un vrai Python (`python3` du système, ignorés
//! s'il manque) : faux front qui répond aux questions / formulaires / listes de choix, arrêt
//! pendant une question, script d'exemple `nettoyage` sur une base SQLite temporaire (aperçu puis
//! application), refus en lecture seule (ComicDb.xml), réglages secrets jamais renvoyés, arrêt
//! forcé d'un script qui ignore l'annulation, fermeture, automatisme `NewBooks` après une analyse,
//! créneau interactif occupé pendant l'application et arrêt de l'application entre deux livres,
//! fermeture qui attend la fin d'une application au fil de l'eau d'un automatisme.

use crate::biblio::{Biblio, ChoixPremierDemarrage, EvenementScan};
use crate::edition::Edition;
use crate::lecteur::Lecteur;
use crate::reglages::Mode;
use crate::scripts::{
    masquer, ConfigScripts, EvenementScript, Scripts, Sortie, ATTENTE_FERMETURE, CLE_SECRETS, SECRET_MASQUE,
};
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{Hook, ScriptSettings};
use comicrust_core::store::{Store, StoreConfig, SQLITE_FILE_NAME};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn python() -> Option<PathBuf> {
    comicrust_core::scripting::find_python(Some(Path::new("/nulle-part"))).ok()
}

macro_rules! besoin_python {
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

/// Faux front : reçoit tous les événements (canal) et les garde (vérifications finales).
struct Front {
    rx: Receiver<EvenementScript>,
    tous: Arc<Mutex<Vec<EvenementScript>>>,
}

/// Événement du backend retenu (le fil qui l'émet attend) jusqu'à `liberer`.
struct Barriere {
    atteinte: Receiver<()>,
    liberer: mpsc::Sender<()>,
}

impl Barriere {
    fn attendre(&self) {
        self.atteinte.recv_timeout(Duration::from_secs(30)).expect("barrière atteinte");
    }
}

impl Front {
    /// Comme [`Front::nouveau`], mais le premier événement pour lequel `retenir` est vrai bloque
    /// le fil qui l'émet (application en cours…) jusqu'à `Barriere::liberer`.
    fn avec_barriere(retenir: impl Fn(&EvenementScript) -> bool + Send + Sync + 'static) -> (Sortie, Front, Barriere) {
        let (sortie, front) = Front::nouveau();
        let (tx_atteinte, atteinte) = mpsc::channel();
        let (liberer, rx_liberer) = mpsc::channel::<()>();
        let attente = Mutex::new(Some((tx_atteinte, rx_liberer)));
        let s2: Sortie = Arc::new(move |e: EvenementScript| {
            let bloque = if retenir(&e) { attente.lock().unwrap().take() } else { None };
            sortie(e);
            if let Some((tx, rx)) = bloque {
                let _ = tx.send(());
                let _ = rx.recv_timeout(Duration::from_secs(60));
            }
        });
        (s2, front, Barriere { atteinte, liberer })
    }

    fn nouveau() -> (Sortie, Front) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let tous: Arc<Mutex<Vec<EvenementScript>>> = Arc::default();
        let t2 = tous.clone();
        let sortie: Sortie = Arc::new(move |e: EvenementScript| {
            t2.lock().unwrap().push(e.clone());
            let _ = tx.lock().unwrap().send(e);
        });
        (sortie, Front { rx, tous })
    }

    /// Attend le premier événement pour lequel `f` rend quelque chose (30 s au plus).
    fn attendre<T>(&self, mut f: impl FnMut(&EvenementScript) -> Option<T>) -> T {
        let fin = Instant::now() + Duration::from_secs(30);
        loop {
            let reste = fin.saturating_duration_since(Instant::now());
            let e = self.rx.recv_timeout(reste).expect("événement attendu");
            if let Some(t) = f(&e) {
                return t;
            }
        }
    }

    fn fin(&self, run: &str) -> (String, Option<Map<String, Value>>) {
        self.attendre(|e| match e {
            EvenementScript::Fin { run_id, etat, champs, .. } if run_id == run => {
                Some((etat.to_string(), champs.clone()))
            }
            _ => None,
        })
    }

    fn journal(&self) -> String {
        self.tous
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                EvenementScript::Journal { texte, .. } => Some(texte.clone()),
                EvenementScript::Auto { ligne, .. } => Some(ligne.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn tout_en_json(&self) -> String {
        self.tous.lock().unwrap().iter().map(|e| e.pour_le_front().1.to_string()).collect::<Vec<_>>().join("\n")
    }
}

/// Bibliothèque autonome (SQLite) contenant `livres`.
fn biblio_autonome(data: &Path, livres: &[LibraryBook]) -> Biblio {
    let b = Biblio::nouvelle(data.to_path_buf());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    let cfg = StoreConfig::Sqlite(data.join(SQLITE_FILE_NAME));
    if !livres.is_empty() {
        Store::open_existing(&cfg).unwrap().upsert(livres).unwrap();
        b.installer(Mode::Autonome, ComicDatabase::from_books(livres.to_vec()), Some(cfg));
    }
    b
}

fn service(data: &Path, dossier: &Path, b: &Biblio, sortie: Sortie) -> (Scripts, Arc<Mutex<Vec<String>>>) {
    let lecteur = Lecteur::nouveau(data.to_path_buf(), b.clone());
    let edition = Edition::nouvelle(b.clone(), lecteur.clone());
    let config = ConfigScripts {
        data: data.to_path_buf(),
        dossier: dossier.to_path_buf(),
        dossier_exe: PathBuf::from("/nulle-part"),
        python: python(),
    };
    let ouvertes: Arc<Mutex<Vec<String>>> = Arc::default();
    let (o2, o3) = (ouvertes.clone(), ouvertes.clone());
    let s = Scripts::nouveau(config, b.clone(), edition, lecteur, sortie).avec_systeme(
        Arc::new(move |u| {
            o2.lock().unwrap().push(format!("url {u}"));
            true
        }),
        Arc::new(move |t| {
            o3.lock().unwrap().push(format!("copie {t}"));
            true
        }),
    );
    (s, ouvertes)
}

fn exemples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts-exemples")
}

fn livre(id: &str, fichier: &str) -> LibraryBook {
    LibraryBook { id: id.into(), file: Some(fichier.into()), ..Default::default() }
}

const SCRIPT_QUESTIONS: &str = "import comicrust\nfrom comicrust import ui\n\n\
#@Name Questions\n#@Hook Library\n#@Key questions\ndef q(books):\n    \
a = comicrust.ask('Nom ?', 'défaut')\n    print('réponse', a)\n    \
r = comicrust.form('Essai', [ui.text('nom', 'Nom', 'x')])\n    print('formulaire', r)\n    \
c = comicrust.choose('Album', ['Titre'], ['A', 'B'])\n    print('choix', c)\n    \
print('url', comicrust.open_url('https://exemple.fr/a'))\n    \
print('copie', comicrust.clipboard_set('texte'))\n    \
b = comicrust.confirm('Sûr ?', True)\n    print('confirmé', b)\n    \
import time\n    \
while not comicrust.cancelled():\n        time.sleep(0.05)\n";

#[test]
fn faux_front_questions_formulaire_choix_puis_arret() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let dossier = d.path().join("scripts");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("q.py"), SCRIPT_QUESTIONS).unwrap();
    let data = d.path().join("data");
    let b = biblio_autonome(&data, &[]);
    let (sortie, front) = Front::nouveau();
    let (s, ouvertes) = service(&data, &dossier, &b, sortie);

    let etat = s.lister();
    assert!(etat.python.chemin.is_some(), "{:?}", etat.python);
    assert!(!etat.auto, "automatismes désactivés par défaut");
    assert_eq!(etat.scripts.len(), 1);
    assert_eq!((etat.scripts[0].id.as_str(), etat.scripts[0].hooks.as_slice()), ("questions", &["Library"][..]));

    let run = s.lancer("questions", "Library", vec![]).unwrap();
    assert!(s.lancer("questions", "Library", vec![]).is_err(), "un script interactif à la fois");
    assert_eq!(s.lister().execution_en_cours.as_deref(), Some(run.as_str()));

    // Question texte.
    let (req, q) = front.attendre(|e| match e {
        EvenementScript::Question { run_id, req_id, genre: "ask", question, texte, .. } if *run_id == run => {
            assert_eq!(texte, "Nom ?");
            Some((*req_id, question.clone().unwrap()))
        }
        _ => None,
    });
    assert_eq!(q["type"], "texte");
    assert_eq!(q["defaut"], "défaut");
    assert!(s.repondre(&run, req + 100, json!("x")).is_err(), "question inconnue");
    s.repondre(&run, req, json!("Yoyo")).unwrap();
    assert!(s.repondre(&run, req, json!("encore")).is_err(), "déjà répondue");

    // Formulaire : objet des valeurs et clé réservée « $bouton ».
    let (req, spec) = front.attendre(|e| match e {
        EvenementScript::Formulaire { req_id, spec, .. } => Some((*req_id, spec.clone())),
        _ => None,
    });
    assert_eq!(spec["title"], "Essai");
    assert_eq!(spec["buttons"], json!(["OK", "Annuler"]));
    assert_eq!(spec["values"]["nom"], "x");
    assert_eq!(spec["controls"][0]["type"], "text");
    s.repondre(&run, req, json!({"nom": "Yoyo2", "$bouton": "OK"})).unwrap();

    // Liste de choix.
    let (req, spec) = front.attendre(|e| match e {
        EvenementScript::Choix { req_id, spec, .. } => Some((*req_id, spec.clone())),
        _ => None,
    });
    assert_eq!(spec["rows"], json!([["A"], ["B"]]));
    s.repondre(&run, req, json!(1)).unwrap();

    // Confirmation : l'utilisateur arrête le script au lieu de répondre.
    let (req, q) = front.attendre(|e| match e {
        EvenementScript::Question { req_id, question: Some(q), .. } if q["type"] == "confirmation" => {
            Some((*req_id, q.clone()))
        }
        _ => None,
    });
    assert_eq!(q["defaut"], true);
    assert_eq!(s.questions_en_attente(&run), 1);
    s.arreter(&run).unwrap();
    let (etat, _) = front.fin(&run);
    assert!(etat == "annule" || etat == "termine", "{etat}");
    assert!(s.repondre(&run, req, json!(false)).is_err(), "plus en attente après l'arrêt");
    let j = front.journal();
    assert!(j.contains("réponse Yoyo"), "{j}");
    assert!(j.contains("formulaire ('OK', {'nom': 'Yoyo2'})"), "{j}");
    assert!(j.contains("choix 1"), "{j}");
    assert!(j.contains("url True") && j.contains("copie True"), "{j}");
    assert_eq!(*ouvertes.lock().unwrap(), ["url https://exemple.fr/a", "copie texte"]);
    assert!(s.lister().execution_en_cours.is_none());
    // Plus rien à appliquer : aperçu vide.
    assert!(s.apercu(&run).unwrap().lignes.is_empty());
}

#[test]
fn nettoyage_sur_base_sqlite_apercu_puis_application() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let mut a = livre("aaaaaaaa-0000-0000-0000-000000000001", "/bd/Blacksad/Blacksad T03 - Âme rouge (2005).cbz");
    a.series = Some("  Blacksad ".into());
    a.number = Some("003".into());
    let mut c = livre("aaaaaaaa-0000-0000-0000-000000000002", "/bd/propre.cbz");
    c.series = Some("Spirou et Fantasio".into());
    c.number = Some("1".into());
    let b = biblio_autonome(&data, &[a.clone(), c.clone()]);
    let (sortie, front) = Front::nouveau();
    let (s, _) = service(&data, &exemples(), &b, sortie);
    assert!(s
        .lister()
        .scripts
        .iter()
        .any(|i| i.id == "nettoyage" && i.reglages.as_deref() == Some("nettoyage-reglages")));

    let run = s.lancer("nettoyage", "Books", vec![a.id.clone(), c.id.clone()]).unwrap();
    let (etat, _) = front.fin(&run);
    assert_eq!(etat, "termine", "{}", front.journal());
    let ap = s.apercu(&run).unwrap();
    assert!(!ap.lecture_seule && ap.motif.is_none());
    assert_eq!(ap.inchanges, 0, "livre propre : rien demandé {ap:?}");
    let serie = ap.lignes.iter().find(|l| l.champ == "Series").expect("série corrigée");
    assert_eq!((serie.avant.as_str(), serie.apres.as_str(), serie.genre), ("  Blacksad ", "Blacksad", "fiche"));
    assert!(ap.lignes.iter().all(|l| l.livre_id == a.id));
    // Tout sauf le numéro.
    let cles: Vec<String> = ap.lignes.iter().filter(|l| l.champ != "Number").map(|l| l.cle.clone()).collect();
    let bilan = s.appliquer(&run, &cles).unwrap();
    assert_eq!((bilan.fiches, bilan.renommages, bilan.comicinfo), (1, 0, 0), "{bilan:?}");
    assert!(bilan.erreurs.is_empty() && !bilan.arrete, "{bilan:?}");
    let f = b.livre(&a.id).unwrap();
    assert_eq!(f.series.as_deref(), Some("Blacksad"));
    assert_eq!(f.number.as_deref(), Some("003"), "ligne non cochée : non appliquée");
    // Écrit dans la base (relue).
    let cfg = StoreConfig::Sqlite(data.join(SQLITE_FILE_NAME));
    let relue = Store::open_existing(&cfg).unwrap().get(std::slice::from_ref(&a.id)).unwrap();
    assert_eq!(relue[0].series.as_deref(), Some("Blacksad"));
    let tous = front.tous.lock().unwrap().clone();
    assert!(tous.iter().any(|e| matches!(e, EvenementScript::BibliothequeModifiee { ids } if ids.contains(&a.id))));
    assert!(tous.iter().any(|e| matches!(e, EvenementScript::Applique { run_id, .. } if *run_id == run)));
    assert!(s.appliquer(&run, &cles).is_err() || s.apercu(&run).unwrap().lignes.is_empty(), "appliqué une fois");
}

#[test]
fn lecture_seule_comicdb_xml_apercu_visible_application_refusee() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(
        &xml,
        "<ComicDatabase><Books><Book Id=\"11111111-0000-0000-0000-000000000001\" File=\"/bd/Saga 012 (2013).cbz\">\
         <Series>  Saga </Series><Number>012</Number></Book></Books></ComicDatabase>",
    )
    .unwrap();
    let data = d.path().join("data");
    let b = Biblio::nouvelle(data.clone());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(xml.to_string_lossy().into()) })
        .unwrap();
    let (sortie, front) = Front::nouveau();
    let (s, _) = service(&data, &exemples(), &b, sortie);
    let etat = s.lister();
    assert!(!etat.ecriture_autorisee && etat.motif_lecture_seule.is_some());
    let run = s.lancer("nettoyage", "Library", vec![]).unwrap();
    assert_eq!(front.fin(&run).0, "termine", "{}", front.journal());
    let ap = s.apercu(&run).unwrap();
    assert!(ap.lecture_seule, "{ap:?}");
    assert!(ap.motif.as_deref().unwrap().contains("ComicDb.xml"));
    assert!(!ap.lignes.is_empty(), "aperçu visible");
    let cles: Vec<String> = ap.lignes.iter().map(|l| l.cle.clone()).collect();
    let err = s.appliquer(&run, &cles).unwrap_err();
    assert!(err.contains("ComicDb.xml"), "{err}");
    assert!(std::fs::read_to_string(&xml).unwrap().contains("<Series>  Saga </Series>"), "rien d'écrit");
}

#[test]
fn reglages_secrets_jamais_renvoyes() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let dossier = d.path().join("scripts");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(
        dossier.join("sec.py"),
        "import comicrust\n\n#@Name Clé\n#@Hook ConfigScript\n#@Key cle\ndef cfg():\n    \
         s = comicrust.settings()\n    s['api'] = 'CLE-TRES-SECRETE'\n    s.mark_secret('api')\n    \
         s['dossier'] = 'D:/BD'\n    s.save()\n    print('enregistré')\n",
    )
    .unwrap();
    let data = d.path().join("data");
    let b = biblio_autonome(&data, &[]);
    let (sortie, front) = Front::nouveau();
    let (s, _) = service(&data, &dossier, &b, sortie);
    let run = s.lancer("cle", "ConfigScript", vec![]).unwrap();
    assert_eq!(front.fin(&run).0, "termine", "{}", front.journal());
    let r = s.reglages_lire("cle").unwrap();
    assert_eq!(r["dossier"], "D:/BD");
    // Windows : « •••• » (chiffré par DPAPI) ; ailleurs, aucun coffre : rien n'est gardé.
    assert!(r["api"] == SECRET_MASQUE || r["api"] == "", "{r:?}");
    // Écriture depuis le front : le masque laisse la valeur secrète inchangée.
    let mut v = Map::new();
    v.insert("api".into(), json!(SECRET_MASQUE));
    v.insert("dossier".into(), json!("E:/BD"));
    let r2 = s.reglages_ecrire("cle", &v).unwrap();
    assert_eq!(r2["dossier"], "E:/BD");
    assert_ne!(r2["api"], "••••••••");
    let fichier = std::fs::read_to_string(data.join(crate::reglages::FICHIER_REGLAGES)).unwrap();
    assert!(!fichier.contains("CLE-TRES-SECRETE"), "{fichier}");
    assert!(fichier.contains("$secret") && fichier.contains("E:/BD"), "{fichier}");
    assert!(!front.tout_en_json().contains("CLE-TRES-SECRETE"), "jamais dans les événements");
    // Masquage (indépendant du coffre du système).
    let mut st = ScriptSettings::default();
    st.values.insert("api".into(), json!("abc"));
    st.values.insert("vide".into(), json!(""));
    st.values.insert("n".into(), json!(3));
    st.secret.extend(["api".to_string(), "vide".to_string()]);
    let m = masquer(&st);
    assert_eq!((m["api"].as_str(), m["vide"].as_str(), m["n"].as_i64()), (Some(SECRET_MASQUE), Some(""), Some(3)));
    assert_eq!(m[CLE_SECRETS], json!(["api", "vide"]), "clés secrètes listées pour le front");
    // Le front renvoie la liste réservée avec ses valeurs : elle n'est jamais enregistrée.
    let mut v = r2.clone();
    v.insert("dossier".into(), json!("F:/BD"));
    let r3 = s.reglages_ecrire("cle", &v).unwrap();
    assert_eq!(r3["dossier"], "F:/BD");
    let fichier = std::fs::read_to_string(data.join(crate::reglages::FICHIER_REGLAGES)).unwrap();
    assert!(!fichier.contains(CLE_SECRETS), "{fichier}");
}

const SCRIPT_TETU: &str = "import comicrust, time\n\n#@Name Têtu\n#@Hook Library\n#@Key tetu\ndef t(books):\n    \
print('départ', flush=True)\n    \
while True:\n        try:\n            time.sleep(0.05)\n        except KeyboardInterrupt:\n            pass\n";

#[test]
fn arret_force_puis_fermeture() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let dossier = d.path().join("scripts");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("t.py"), SCRIPT_TETU).unwrap();
    let data = d.path().join("data");
    let b = biblio_autonome(&data, &[]);
    let (sortie, front) = Front::nouveau();
    let (s, _) = service(&data, &dossier, &b, sortie);
    let attendre_depart = |run: &str| {
        front.attendre(|e| match e {
            EvenementScript::Journal { run_id, texte, .. } if run_id == run && texte.contains("départ") => Some(()),
            _ => None,
        })
    };

    let run = s.lancer("tetu", "Library", vec![]).unwrap();
    attendre_depart(&run);
    let t0 = Instant::now();
    s.arreter(&run).unwrap();
    let (etat, _) = front.fin(&run);
    let duree = t0.elapsed();
    assert!(etat == "annule" || etat == "erreur", "{etat}");
    assert!(duree >= Duration::from_secs(2), "arrêt forcé après le délai de grâce : {duree:?}");
    assert!(duree < Duration::from_secs(10), "{duree:?}");

    // Fermeture de l'application pendant un script : tout est arrêté en moins de 8 s.
    let run = s.lancer("tetu", "Library", vec![]).unwrap();
    attendre_depart(&run);
    let t0 = Instant::now();
    assert!(s.fermer(ATTENTE_FERMETURE));
    assert!(t0.elapsed() < ATTENTE_FERMETURE, "{:?}", t0.elapsed());
    assert!(s.lancer("tetu", "Library", vec![]).is_err(), "plus de script après la fermeture");
}

#[test]
fn automatisme_nouveaux_livres_apres_une_analyse() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let racine = d.path().join("BD");
    std::fs::create_dir_all(&racine).unwrap();
    crate::couvertures::cbz_de_test(&racine.join("Thanos Quest 01 (1990).cbz"), [10, 20, 30, 255], 2, None);
    let data = d.path().join("data");
    let b = biblio_autonome(&data, &[]);
    let (sortie, front) = Front::nouveau();
    let (s, _) = service(&data, &exemples(), &b, sortie);
    // Désactivé par défaut (automatismes et script) : rien ne part.
    s.declencher(Hook::NewBooks, vec!["x".into()]);
    assert!(s.lister().scripts.iter().any(|i| i.id == "nouveaux-livres" && !i.actif));
    s.regler_auto(true).unwrap();
    s.activer("nouveaux-livres", true).unwrap();
    assert!(s.lister().auto);

    b.ajouter_dossier(&racine.to_string_lossy()).unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    b.lancer_scan(Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    }))
    .unwrap();
    loop {
        match rx.recv_timeout(Duration::from_secs(60)).expect("analyse terminée") {
            EvenementScan::Termine { ajoutes, .. } => {
                assert_eq!(ajoutes, 1);
                break;
            }
            EvenementScan::Erreur { message } => panic!("{message}"),
            _ => {}
        }
    }
    let ajoutes = b.prendre_ajoutes();
    assert_eq!(ajoutes.len(), 1);
    assert!(b.prendre_ajoutes().is_empty(), "lus une seule fois");
    s.declencher(Hook::NewBooks, ajoutes.clone());
    front.attendre(|e| match e {
        EvenementScript::Auto { ligne, hook, .. } if hook == "NewBooks" && ligne.contains(") terminé") => Some(()),
        EvenementScript::Auto { ligne, .. } if ligne.contains("en erreur") || ligne.contains("non lancé") => {
            panic!("{ligne}")
        }
        _ => None,
    });
    let f = b.livre(&ajoutes[0]).unwrap();
    assert_eq!(f.tags.as_deref(), Some("à lire"), "{}", front.journal());
    assert_eq!(f.series.as_deref(), Some("Thanos Quest"));
    let j = front.journal();
    assert!(j.contains("modifications appliquées"), "{j}");
    // Aucune question posée par un automatisme, et pas de `script:fin`.
    let tous = front.tous.lock().unwrap().clone();
    assert!(!tous.iter().any(|e| matches!(e, EvenementScript::Question { .. } | EvenementScript::Fin { .. })));
    // Écrit dans la base.
    let cfg = StoreConfig::Sqlite(data.join(SQLITE_FILE_NAME));
    let relue = Store::open_existing(&cfg).unwrap().get(&ajoutes).unwrap();
    assert_eq!(relue[0].tags.as_deref(), Some("à lire"));
}

#[test]
fn hook_editeur_rend_les_champs_sans_rien_enregistrer() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let dossier = d.path().join("scripts");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(
        dossier.join("ed.py"),
        "import comicrust\n\n#@Name Éditeur\n#@Hook Editor\n#@Key ed\ndef e(books):\n    \
         for b in books:\n        b.Title = 'Titre de ' + (b.Series or '?')\n",
    )
    .unwrap();
    let data = d.path().join("data");
    let mut l = livre("bbbbbbbb-0000-0000-0000-000000000001", "/bd/x.cbz");
    l.series = Some("Ancienne".into());
    let b = biblio_autonome(&data, std::slice::from_ref(&l));
    let (sortie, front) = Front::nouveau();
    let (s, _) = service(&data, &dossier, &b, sortie);
    let mut champs = Map::new();
    champs.insert("Series".into(), json!("Saisie non enregistrée"));
    let run = s.hook_editeur("ed", &l.id, &champs).unwrap();
    let (etat, champs) = front.fin(&run);
    assert_eq!(etat, "termine", "{}", front.journal());
    assert_eq!(champs.unwrap()["Title"], "Titre de Saisie non enregistrée");
    assert_eq!(b.livre(&l.id).unwrap().title, None, "rien d'enregistré");
}

const SCRIPT_COMICINFO: &str = "import comicrust\n\n#@Name Infos\n#@Hook Books\n#@Key ci\ndef ci(books):\n    \
for b in books:\n        comicrust.write_info(b)\n";

#[test]
fn application_occupe_le_creneau_et_s_arrete_entre_deux_livres() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let dossier = d.path().join("scripts");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("ci.py"), SCRIPT_COMICINFO).unwrap();
    let bd = d.path().join("BD");
    std::fs::create_dir_all(&bd).unwrap();
    let livres: Vec<LibraryBook> = (1..=3)
        .map(|i| {
            let f = bd.join(format!("Album {i}.cbz"));
            crate::couvertures::cbz_de_test(&f, [10, 20, 30, 255], 1, None);
            let mut l = livre(&format!("cccccccc-0000-0000-0000-00000000000{i}"), &f.to_string_lossy());
            l.series = Some(format!("Série {i}"));
            l
        })
        .collect();
    let ids: Vec<String> = livres.iter().map(|l| l.id.clone()).collect();
    let data = d.path().join("data");
    let b = biblio_autonome(&data, &livres);
    // L'application est retenue à son premier avancement de l'écriture des ComicInfo.xml.
    let (sortie, front, barriere) = Front::avec_barriere(
        |e| matches!(e, EvenementScript::Progression { texte: Some(t), .. } if t.starts_with("ComicInfo.xml")),
    );
    let (s, _) = service(&data, &dossier, &b, sortie);
    let run = s.lancer("ci", "Books", ids.clone()).unwrap();
    assert_eq!(front.fin(&run).0, "termine", "{}", front.journal());
    let ap = s.apercu(&run).unwrap();
    assert_eq!(ap.lignes.len(), 3, "{ap:?}");
    assert!(ap.lignes.iter().all(|l| l.genre == "comicinfo" && l.conflit.is_none()), "{ap:?}");
    assert!(s.lister().execution_en_cours.is_none(), "script fini : créneau libre");
    let cles: Vec<String> = ap.lignes.iter().map(|l| l.cle.clone()).collect();

    let (s2, run2, cles2) = (s.clone(), run.clone(), cles.clone());
    let application = std::thread::spawn(move || s2.appliquer(&run2, &cles2));
    barriere.attendre();
    // Pendant l'application : créneau occupé, lancement et seconde application refusés.
    assert_eq!(s.lister().execution_en_cours.as_deref(), Some(run.as_str()), "executionEnCours pendant l'application");
    let refus = s.lancer("ci", "Books", ids.clone()).unwrap_err();
    assert!(refus.contains("déjà en cours"), "{refus}");
    assert!(s.appliquer(&run, &cles).is_err(), "une seule application à la fois");
    // Arrêt : entre deux livres (l'écriture des ComicInfo.xml est annulée).
    s.arreter(&run).unwrap();
    barriere.liberer.send(()).unwrap();
    let bilan = application.join().unwrap().unwrap();
    assert!(bilan.arrete, "{bilan:?}");
    assert_eq!(bilan.comicinfo, 0, "aucun livre écrit après l'arrêt : {bilan:?}");
    assert!(front.journal().contains("l'application s'arrête après le livre en cours"), "{}", front.journal());
    let tous = front.tous.lock().unwrap().clone();
    assert!(tous
        .iter()
        .any(|e| matches!(e, EvenementScript::Applique { run_id, bilan } if *run_id == run && bilan.arrete)));
    // Bilan rendu : créneau libéré, un nouveau script peut partir.
    assert!(s.lister().execution_en_cours.is_none());
    let run3 = s.lancer("ci", "Books", ids).unwrap();
    assert_eq!(front.fin(&run3).0, "termine");
}

const SCRIPT_AUTO_DIRECT: &str = "import comicrust, time\n\n#@Name Direct\n#@Hook NewBooks\n#@Key direct\n\
def d(books):\n    for b in books:\n        b.Tags = 'vu'\n    comicrust.flush()\n    \
while not comicrust.cancelled():\n        time.sleep(0.05)\n";

#[test]
fn fermeture_attend_l_application_au_fil_de_l_eau_d_un_automatisme() {
    let _py = besoin_python!();
    let d = tempfile::tempdir().unwrap();
    let dossier = d.path().join("scripts");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("direct.py"), SCRIPT_AUTO_DIRECT).unwrap();
    let data = d.path().join("data");
    let l = livre("dddddddd-0000-0000-0000-000000000001", "/bd/x.cbz");
    let b = biblio_autonome(&data, std::slice::from_ref(&l));
    // L'application au fil de l'eau est retenue juste après avoir écrit la fiche.
    let (sortie, front, barriere) = Front::avec_barriere(|e| matches!(e, EvenementScript::BibliothequeModifiee { .. }));
    let (s, _) = service(&data, &dossier, &b, sortie);
    s.regler_auto(true).unwrap();
    s.declencher(Hook::NewBooks, vec![l.id.clone()]);
    barriere.attendre();
    assert_eq!(s.automatismes_en_cours(), (1, 1));

    let s2 = s.clone();
    let fermeture = std::thread::spawn(move || s2.fermer(Duration::from_millis(200)));
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!fermeture.is_finished(), "la fermeture attend la fin de l'application en cours");
    barriere.liberer.send(()).unwrap();
    fermeture.join().unwrap();
    // Le script, arrêté par la fermeture, finit peu après ; rien ne reste « en cours ».
    let fin = Instant::now() + Duration::from_secs(15);
    while s.automatismes_en_cours() != (0, 0) {
        assert!(Instant::now() < fin, "automatisme toujours en cours : {}", front.journal());
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(b.livre(&l.id).unwrap().tags.as_deref(), Some("vu"), "{}", front.journal());
}
