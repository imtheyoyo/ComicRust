//! Tests des listes ComicRack (étape 5, `docs/tauri/ipc-listes.md`) :
//!
//! - arbre identique à ComicRack sur la fixture réelle anonymisée (47 listes,
//!   `fixtures/listes/listes_reference.txt` produite par ElementTree, indépendamment du Rust) ;
//! - écriture de `ComicDb.xml` : lecture seule sans l'option, refus ComicRack lancé (vérifié aussi
//!   juste avant le remplacement), sauvegarde d'origine + 10 dernières, seul `<ComicLists>`
//!   remplacé, listes non touchées et critères non modifiés gardés à l'octet près ;
//! - mode autonome (SQLite) : toutes les opérations, ordre de liste, séries entières, éditeur,
//!   persistance ; `.cbl` ;
//! - base de ComicRust MySQL (`COMICRUST_TEST_MYSQL`, base de test seulement) : deux postes,
//!   modification concurrente refusée sans rien écrire.

use crate::biblio::{Biblio, Groupe, RequeteLivres, SensTri};
use crate::listes::*;
use crate::listes_criteres::{CriteresListe, ModeCriteres, NoeudCritere};
use crate::reglages::{self, Mode, ReglagesEdition};
use comicrust_core::comic_lists::{load_comic_lists, ComicList, ListKind};
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use comicrust_core::store::{Store, StoreConfig};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/listes/ComicDb_anonyme.xml");
const REFERENCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/listes/listes_reference.txt");

const DOUBLONS: &str = "044097b6-e82c-4c8e-a395-ec8d16f4be06";
const NON_LUES: &str = "fb95f9a8-c856-477f-aff9-05e7444ab413";
const TEST_LISTE: &str = "1b7b2b42-df47-46b8-bc55-1a75decd23fe";
const GUIDE: &str = "726e57e4-d518-4b58-bac3-f5ef852dc600";
const DOSSIER_1: &str = "d3834683-f43f-4010-b469-48ff0ba128b2";

// ───────────────────────────── Outils ─────────────────────────────

fn livre(id: &str, serie: &str, numero: &str) -> LibraryBook {
    LibraryBook {
        id: id.into(),
        series: Some(serie.into()),
        number: Some(numero.into()),
        file: Some(format!(r"\\NAS\BD\_BD\Séries Terminées\{serie}\{serie} {numero}.cbz")),
        page_count: Some(40),
        last_page_read: Some(0),
        ..Default::default()
    }
}

/// Bibliothèque contenant tous les livres des listes de lecture de la fixture (+ quelques autres).
fn base_fixture() -> ComicDatabase {
    let listes = load_comic_lists(Path::new(FIXTURE)).unwrap();
    let mut ids = Vec::new();
    fn parcourir(l: &[ComicList], ids: &mut Vec<String>) {
        for x in l {
            match &x.kind {
                ListKind::Ids(v) => ids.extend(v.iter().cloned()),
                ListKind::Folder { children, .. } => parcourir(children, ids),
                _ => {}
            }
        }
    }
    parcourir(&listes, &mut ids);
    ids.sort();
    ids.dedup();
    let mut livres: Vec<LibraryBook> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| livre(id, &format!("Série {}", i / 10), &(i % 10 + 1).to_string()))
        .collect();
    livres.push(livre("ffffffff-0000-4000-8000-000000000001", "Hors liste", "1"));
    ComicDatabase::from_books(livres)
}

/// Copie de la fixture dans un dossier de test et bibliothèque liée à ce `ComicDb.xml`.
fn biblio_comicdb(dir: &Path) -> (Biblio, PathBuf) {
    let xml = dir.join("ComicDb.xml");
    std::fs::copy(FIXTURE, &xml).unwrap();
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    reglages::modifier(&data, |r| {
        r.mode = Some(Mode::Comicrack);
        r.chemin_ini = Some(xml.clone());
    })
    .unwrap();
    let b = Biblio::nouvelle(data);
    b.installer(Mode::Comicrack, base_fixture(), None);
    b.simuler_comicrack(Arc::new(|| Ok(false)));
    (b, xml)
}

fn ecriture(b: &Biblio, oui: bool) {
    reglages::enregistrer_reglages_edition(
        b.data(),
        ReglagesEdition { ecriture_comicrack: Some(oui), ..Default::default() },
    )
    .unwrap();
}

fn aplatir(l: &[NoeudListe], prof: usize, out: &mut Vec<(usize, NoeudListe)>) {
    for x in l {
        out.push((prof, x.clone()));
        if let Some(e) = &x.enfants {
            aplatir(e, prof + 1, out);
        }
    }
}

fn tous(a: &ArbreListes) -> Vec<(usize, NoeudListe)> {
    let mut v = Vec::new();
    aplatir(&a.listes, 0, &mut v);
    v
}

fn noeud(a: &ArbreListes, id: &str) -> NoeudListe {
    tous(a).into_iter().map(|(_, n)| n).find(|n| n.id.eq_ignore_ascii_case(id)).unwrap_or_else(|| panic!("{id}"))
}

fn nom_type(t: TypeListe) -> &'static str {
    match t {
        TypeListe::Bibliotheque => "bibliotheque",
        TypeListe::Dossier => "dossier",
        TypeListe::Lecture => "lecture",
        TypeListe::Intelligente => "intelligente",
    }
}

/// Élément `<Item>` d'identifiant `id` (texte d'origine, sans ses sous-listes).
fn element(texte: &str, id: &str) -> String {
    let debut = texte.find(&format!("Id=\"{id}\"")).unwrap();
    let debut = texte[..debut].rfind("<Item ").unwrap();
    let fin = texte[debut..].find("</Item>").map(|f| debut + f + "</Item>".len()).unwrap();
    texte[debut..fin].to_string()
}

fn bloc<'a>(texte: &'a str, balise: &str) -> &'a str {
    let a = texte.find(&format!("<{balise}>")).unwrap();
    let b = texte.find(&format!("</{balise}>")).unwrap() + balise.len() + 3;
    &texte[a..b]
}

fn sauvegardes(dossier: &Path) -> (Vec<String>, Vec<String>) {
    let mut origine = Vec::new();
    let mut autres = Vec::new();
    for e in std::fs::read_dir(dossier).unwrap() {
        let n = e.unwrap().file_name().to_string_lossy().into_owned();
        if n.starts_with("ComicDb.comicrust-origine-") && n.ends_with(".xml.bak") {
            origine.push(n);
        } else if n.starts_with("ComicDb.comicrust-") && n.ends_with(".xml.bak") {
            autres.push(n);
        }
    }
    (origine, autres)
}

fn requete(liste: Option<&str>, tri: &str, groupe: Option<Groupe>) -> RequeteLivres {
    RequeteLivres {
        recherche: None,
        tri: tri.into(),
        sens_tri: SensTri::Asc,
        groupe,
        debut: 0,
        nombre: 1000,
        liste: liste.map(str::to_string),
    }
}

fn ids_vue(b: &Biblio, r: &RequeteLivres) -> Vec<String> {
    b.lister(r).unwrap().livres.into_iter().map(|l| l.id).collect()
}

// ───────────────────────────── Fixture réelle ─────────────────────────────

/// L'arbre (ordre, imbrication, types, noms, identifiants, compte de ComicRack) est celui que
/// ComicRack a enregistré : 47 listes, comme la référence lue par ElementTree.
#[test]
fn arbre_identique_a_comicrack_sur_la_fixture() {
    let d = tempfile::tempdir().unwrap();
    let (b, xml) = biblio_comicdb(d.path());
    let a = b.arbre_listes().unwrap();
    assert_eq!(a.etat.source, SourceListes::Comicdb);
    assert_eq!(a.etat.chemin.as_deref(), Some(xml.display().to_string().as_str()));
    assert!(a.etat.erreur.is_none(), "{:?}", a.etat.erreur);
    let obtenu: Vec<String> = tous(&a)
        .iter()
        .map(|(p, n)| format!("{p}|{}|{}|{}|{}", nom_type(n.type_liste), n.nom, n.nb_comicrack, n.id))
        .collect();
    let attendu: Vec<String> = std::fs::read_to_string(REFERENCE).unwrap().lines().map(str::to_string).collect();
    assert_eq!(attendu.len(), 47);
    assert_eq!(obtenu, attendu);
    // Toutes les listes intelligentes sont calculées (aucun critère non pris en charge).
    for (_, n) in tous(&a) {
        assert!(n.nb_livres.is_some(), "{} : {:?}", n.nom, n.inconnu);
    }
    // Listes de lecture : même compte que ComicRack quand la bibliothèque contient leurs livres.
    assert_eq!(noeud(&a, TEST_LISTE).nb_livres, Some(9));
    assert_eq!(noeud(&a, GUIDE).nb_livres, Some(321));
    for (_, n) in tous(&a).into_iter().filter(|(_, n)| n.type_liste == TypeListe::Lecture) {
        assert_eq!(n.nb_livres, Some(n.nb_comicrack), "{}", n.nom);
    }
    let bibliotheque = &a.listes[0];
    assert_eq!(bibliotheque.type_liste, TypeListe::Bibliotheque);
    assert_eq!(bibliotheque.nb_livres, Some(b.base().unwrap().len()));
    assert!(!bibliotheque.peut_monter && bibliotheque.peut_descendre);
    // Bilan copiable : une ligne par liste, « ≠ » sur les différences.
    let bilan = b.bilan_listes().unwrap();
    assert_eq!(bilan.lines().filter(|l| l.contains(" — ComicRust ")).count(), 47, "{bilan}");
    assert!(bilan.contains("📄 Test liste ComicRust — ComicRust 9 / ComicRack 9\n"), "{bilan}");
    // Lecture seule par défaut (option d'écriture absente) : rien n'est modifiable.
    assert!(!a.etat.modifiable);
    assert_eq!(a.etat.lecture_seule.as_deref(), Some(LECTURE_SEULE));
    assert!(tous(&a).iter().all(|(_, n)| !n.accepte_livres));
}

/// Sélection d'une liste de lecture : livres dans l'ordre de la liste, rang « # ».
#[test]
fn lister_livres_d_une_liste_dans_son_ordre() {
    let d = tempfile::tempdir().unwrap();
    let (b, _) = biblio_comicdb(d.path());
    let listes = load_comic_lists(Path::new(FIXTURE)).unwrap();
    let ListKind::Ids(ordre) = &ComicList::find(&listes, GUIDE).unwrap().kind else { panic!() };
    let p = b.lister(&requete(Some(GUIDE), "ordre", None)).unwrap();
    assert_eq!(p.total, 321);
    let ids: Vec<String> = p.livres.iter().map(|l| l.id.to_ascii_lowercase()).collect();
    assert_eq!(&ids, ordre);
    assert_eq!(p.livres[0].rang, Some(1));
    assert_eq!(p.livres[320].rang, Some(321));
    // Sans liste : pas de rang ; liste inconnue : erreur ; tri par série dans une liste.
    assert!(b.lister(&requete(None, "ordre", None)).unwrap().livres[0].rang.is_none());
    assert!(b.lister(&requete(Some("inconnue"), "ordre", None)).unwrap_err().contains("liste introuvable"));
    let p = b.lister(&requete(Some(TEST_LISTE), "serie", None)).unwrap();
    assert_eq!(p.total, 9);
    // Dossier : réunion de ses sous-listes ; « Bibliothèque » : tout.
    let dossier = b.lister(&requete(Some(DOSSIER_1), "ordre", None)).unwrap().total;
    let a = b.arbre_listes().unwrap();
    assert_eq!(Some(dossier), noeud(&a, DOSSIER_1).nb_livres);
    assert_eq!(b.lister(&requete(Some(&a.listes[0].id), "serie", None)).unwrap().total, b.base().unwrap().len());
}

/// Écriture de `ComicDb.xml` : lecture seule, ComicRack lancé (aussi juste avant d'écrire),
/// sauvegardes, seul `<ComicLists>` remplacé, listes non touchées gardées à l'octet près.
#[test]
fn ecriture_comicdb_sure() {
    let d = tempfile::tempdir().unwrap();
    let (b, xml) = biblio_comicdb(d.path());
    let original = std::fs::read_to_string(&xml).unwrap();
    let demande = |nom: &str| DemandeCreation {
        type_liste: TypeNouvelle::Lecture,
        nom: Some(nom.into()),
        parent: None,
        position: None,
        livres: None,
        apres_liste: Some(TEST_LISTE.into()),
    };

    // 1. Sans l'option d'écriture : refus, fichier intact.
    assert_eq!(b.creer_liste(&demande("A")).unwrap_err(), LECTURE_SEULE);
    // 2. Option activée, ComicRack lancé : refus, commandes grisées, fichier intact.
    ecriture(&b, true);
    b.simuler_comicrack(Arc::new(|| Ok(true)));
    let e = b.etat_listes();
    assert!(!e.modifiable && e.comicrack_lance);
    assert!(e.lecture_seule.unwrap().starts_with("Ferme ComicRack"));
    assert!(b.creer_liste(&demande("A")).unwrap_err().starts_with("Ferme ComicRack"));
    // 3. Présence impossible à vérifier : refus par prudence.
    b.simuler_comicrack(Arc::new(|| Err("accès refusé".into())));
    let err = b.creer_liste(&demande("A")).unwrap_err();
    assert!(err.starts_with("Impossible de vérifier si ComicRack est lancé") && err.contains("accès refusé"), "{err}");
    // 4. ComicRack lancé pendant la préparation : refusé à la dernière vérification.
    let appels = Arc::new(AtomicUsize::new(0));
    let c = appels.clone();
    b.simuler_comicrack(Arc::new(move || Ok(c.fetch_add(1, Ordering::SeqCst) >= 2)));
    assert!(b.creer_liste(&demande("A")).unwrap_err().starts_with("Ferme ComicRack"));
    assert!(appels.load(Ordering::SeqCst) >= 3, "vérifié avant et juste avant le remplacement");
    assert_eq!(std::fs::read_to_string(&xml).unwrap(), original, "rien n'a été écrit");
    assert!(
        std::fs::read_dir(d.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains("tmp")),
        "aucun fichier temporaire laissé"
    );

    // 5. ComicRack fermé : création écrite, sauvegarde d'origine, reste du fichier identique.
    b.simuler_comicrack(Arc::new(|| Ok(false)));
    let r = b.creer_liste(&demande("Ma liste")).unwrap();
    let id = r.creee.clone().unwrap();
    assert_eq!(r.selection, Some(Some(id.clone())));
    assert_eq!(r.renommer.as_deref(), Some(id.as_str()));
    assert!(r.sauvegarde.as_deref().is_some_and(|s| s.starts_with("ComicDb.comicrust-")), "{:?}", r.sauvegarde);
    let apres = std::fs::read_to_string(&xml).unwrap();
    let (o, _) = sauvegardes(d.path());
    assert_eq!(o.len(), 1, "sauvegarde d'origine");
    assert_eq!(std::fs::read_to_string(d.path().join(&o[0])).unwrap(), original);
    let hors = |t: &str| {
        let a = t.find("<ComicLists>").unwrap();
        let z = t.find("</ComicLists>").unwrap();
        (t[..a].to_string(), t[z..].to_string())
    };
    assert_eq!(hors(&apres), hors(&original), "seul <ComicLists> est remplacé");
    assert!(apres.contains("Name=\"Ma liste\""));
    assert!(!apres.replace("\r\n", "").contains('\n'), "fins de ligne CRLF suivies");
    // Liste créée juste après « Test liste ComicRust », dans le même dossier.
    let a = b.arbre_listes().unwrap();
    let enfants: Vec<String> = noeud(&a, DOSSIER_1).enfants.unwrap().iter().map(|n| n.nom.clone()).collect();
    assert_eq!(enfants[..2], ["Test liste ComicRust".to_string(), "Ma liste".to_string()]);
    // Listes non touchées : identiques octet pour octet (hors dossier parent, dont les compteurs
    // mémorisés par ComicRack sont retirés).
    for l in ["4f632566-25d0-4f05-94c9-d1da14015030", DOUBLONS, GUIDE, NON_LUES, TEST_LISTE] {
        assert_eq!(element(&apres, l), element(&original, l), "{l}");
    }
    let tete = |t: &str| element(t, DOSSIER_1).split("<Items>").next().unwrap().to_string();
    assert!(tete(&original).contains("<BookCount>"));
    assert!(!tete(&apres).contains("<BookCount>"), "compte du dossier parent périmé, retiré");

    // 6. Fichier relu après une modification par ComicRack (version change, arbre à jour).
    let v = b.etat_listes().version;
    let mut t = std::fs::read_to_string(&xml).unwrap();
    t = t.replace("Name=\"Ma liste\"", "Name=\"Renommée dans ComicRack\"");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&xml, &t).unwrap();
    let e = b.etat_listes();
    assert_ne!(e.version, v);
    assert_eq!(noeud(&b.arbre_listes().unwrap(), &id).nom, "Renommée dans ComicRack");

    // 7. Douze enregistrements : 10 sauvegardes gardées + celle d'origine.
    for i in 0..12 {
        b.renommer_liste(&id, &format!("Nom {i}")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        if i == 3 {
            break; // les secondes de l'horodatage limitent la vitesse : 4 suffisent à vérifier le principe
        }
    }
    let (o, autres) = sauvegardes(d.path());
    assert_eq!(o.len(), 1, "l'origine n'est jamais supprimée");
    assert!(!autres.is_empty() && autres.len() <= 10, "{autres:?}");
    assert_eq!(noeud(&b.arbre_listes().unwrap(), &id).nom, "Nom 3");
}

/// Éditeur : critères non modifiés gardés à l'octet près, un seul critère réécrit.
#[test]
fn criteres_non_modifies_gardes_a_l_octet() {
    let d = tempfile::tempdir().unwrap();
    let (b, xml) = biblio_comicdb(d.path());
    ecriture(&b, true);
    let original = std::fs::read_to_string(&xml).unwrap();
    // Lecture : origine de chaque critère, consultation possible en lecture seule.
    let ed = b.lire_criteres(Some(NON_LUES)).unwrap();
    assert!(ed.lecture_seule.is_none());
    assert_eq!(ed.criteres.nom, "Séries complétes (Non Lues)");
    assert!(ed.listes_depart.iter().all(|l| !l.id.eq_ignore_ascii_case(NON_LUES)), "pas elle-même");
    // Renommer seulement : `<Matchers>` identique octet pour octet.
    let mut c = ed.criteres.clone();
    c.nom = "Complètes non lues".into();
    let r = b.enregistrer_criteres(Some(NON_LUES), &c, None, None, None).unwrap();
    assert!(r.info.contains("Complètes non lues"), "{}", r.info);
    let apres = std::fs::read_to_string(&xml).unwrap();
    assert_eq!(bloc(&element(&apres, NON_LUES), "Matchers"), bloc(&element(&original, NON_LUES), "Matchers"));
    // Un critère modifié : les autres critères gardent leur texte d'origine.
    let ed = b.lire_criteres(Some(NON_LUES)).unwrap();
    let mut c = ed.criteres.clone();
    let mut modifie = false;
    fn changer(n: &mut [NoeudCritere], fait: &mut bool) {
        for x in n {
            match x {
                NoeudCritere::Critere(c) if !*fait && c.valeur == "95" => {
                    c.valeur = "90".into();
                    *fait = true;
                }
                NoeudCritere::Groupe(g) => changer(&mut g.noeuds, fait),
                _ => {}
            }
        }
    }
    changer(&mut c.noeuds, &mut modifie);
    assert!(modifie, "critère « 95 » trouvé : {:?}", c.noeuds);
    let ap = b.apercu_criteres(Some(NON_LUES), &c).unwrap();
    assert!(ap.erreurs.is_empty() && ap.nb_livres.is_some(), "{ap:?}");
    b.enregistrer_criteres(Some(NON_LUES), &c, None, None, None).unwrap();
    let apres2 = std::fs::read_to_string(&xml).unwrap();
    let m1 = bloc(&element(&apres, NON_LUES), "Matchers").to_string();
    let m2 = bloc(&element(&apres2, NON_LUES), "Matchers").to_string();
    assert_ne!(m1, m2);
    assert!(m2.contains("<MatchValue>90</MatchValue>") && !m2.contains("<MatchValue>95</MatchValue>"), "{m2}");
    assert!(m2.contains("<MatchValue>Séries Terminées</MatchValue>"));
    // Les autres listes intelligentes n'ont pas bougé.
    assert_eq!(element(&apres2, DOUBLONS), element(&original, DOUBLONS));
    // Saisie invalide : erreurs, rien d'écrit.
    let mut c = ed.criteres.clone();
    c.limite = Some(crate::listes_criteres::LimiteCriteres {
        valeur: "abc".into(),
        unite: crate::listes_criteres::UniteLimite::Livres,
        selection: crate::listes_criteres::SelectionLimite::Hasard,
    });
    let ap = b.apercu_criteres(Some(NON_LUES), &c).unwrap();
    assert_eq!(ap.erreurs, ["Limite : « abc » n'est pas un nombre entier positif"]);
    assert!(b.enregistrer_criteres(Some(NON_LUES), &c, None, None, None).is_err());
    assert_eq!(std::fs::read_to_string(&xml).unwrap(), apres2);
}

// ───────────────────────────── Mode autonome ─────────────────────────────

/// Bibliothèque autonome (SQLite) avec 9 livres : Thanos 1-3, Warlock 1-3, Silver Surfer 1-3.
fn biblio_autonome(dir: &Path) -> Biblio {
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let b = Biblio::nouvelle(data.clone());
    b.configurer(&crate::biblio::ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    let mut s = Store::open_existing(&StoreConfig::Sqlite(data.join("comicrust.db"))).unwrap();
    let mut v = Vec::new();
    for (k, serie) in ["Thanos", "Warlock", "Silver Surfer"].iter().enumerate() {
        for n in 1..=3 {
            let mut l = livre(&format!("{k}{n}000000-0000-4000-8000-000000000000"), serie, &n.to_string());
            l.year = Some(1990 + k as i32);
            v.push(l);
        }
    }
    s.upsert(&v).unwrap();
    b.ouvrir().unwrap();
    b
}

fn id_livre(k: usize, n: usize) -> String {
    format!("{k}{n}000000-0000-4000-8000-000000000000")
}

#[test]
fn autonome_toutes_les_operations() {
    let d = tempfile::tempdir().unwrap();
    let b = biblio_autonome(d.path());
    let a = b.arbre_listes().unwrap();
    assert_eq!(a.etat.source, SourceListes::Comicrust);
    assert!(a.etat.modifiable && a.listes.is_empty());

    // Dossier, puis liste de lecture dedans avec 4 livres (Thanos 1, Warlock 2, Thanos 2, Surfer 1).
    let r = b
        .creer_liste(&DemandeCreation {
            type_liste: TypeNouvelle::Dossier,
            nom: None,
            parent: None,
            position: None,
            livres: None,
            apres_liste: None,
        })
        .unwrap();
    let dossier = r.creee.unwrap();
    assert_eq!(noeud(&r.arbre, &dossier).nom, "Nouveau dossier");
    let livres = vec![id_livre(0, 1), id_livre(1, 2), id_livre(0, 2), id_livre(2, 1)];
    let r = b
        .creer_liste(&DemandeCreation {
            type_liste: TypeNouvelle::Lecture,
            nom: Some("Lecture".into()),
            parent: Some(dossier.clone()),
            position: None,
            livres: Some(livres.clone()),
            apres_liste: None,
        })
        .unwrap();
    let liste = r.creee.unwrap();
    assert!(r.renommer.is_none() && r.info.contains("créée avec 4 livres"), "{}", r.info);
    let n = noeud(&r.arbre, &liste);
    assert_eq!((n.nb_livres, n.accepte_livres), (Some(4), true));
    assert_eq!(noeud(&r.arbre, &dossier).nb_livres, Some(4), "dossier : réunion");
    let ordre = requete(Some(&liste), "ordre", None);
    assert_eq!(ids_vue(&b, &ordre), livres);

    // Ajout (déjà présent ignoré), retrait, décalage.
    let r = b.ajouter_livres_liste(&liste, vec![id_livre(0, 3), id_livre(0, 1)], None).unwrap();
    assert_eq!(r.info, "1 livre ajouté à « Lecture » (1 déjà dans la liste)");
    let r = b.retirer_livres_liste(&liste, vec![id_livre(2, 1)]).unwrap();
    assert_eq!(r.info, "1 livre retiré de « Lecture »");
    assert_eq!(b.base().unwrap().len(), 9, "le livre reste dans la bibliothèque");
    b.decaler_livres_liste(&liste, vec![id_livre(0, 2)], true).unwrap();
    assert_eq!(ids_vue(&b, &ordre), [id_livre(0, 1), id_livre(0, 2), id_livre(1, 2), id_livre(0, 3)]);
    let r = b.decaler_livres_liste(&liste, vec![id_livre(0, 1)], true).unwrap();
    assert!(r.info.is_empty(), "déjà en tête : rien d'écrit");

    // Glisser dans la vue : Thanos 3 déposé en tête.
    b.deplacer_livres_liste(&liste, vec![id_livre(0, 3)], &ordre, 0).unwrap();
    assert_eq!(ids_vue(&b, &ordre), [id_livre(0, 3), id_livre(0, 1), id_livre(0, 2), id_livre(1, 2)]);
    // Vue non triée dans l'ordre de la liste : refus.
    let err = b.deplacer_livres_liste(&liste, vec![id_livre(0, 3)], &requete(Some(&liste), "serie", None), 2);
    assert_eq!(err.unwrap_err(), TRIER_POUR_REORDONNER);

    // Séries entières (v0.10.1) : vue groupée par série, Warlock en tête puis tout en bas.
    let groupee = requete(Some(&liste), "ordre", Some(Groupe::Serie));
    let g = b.groupes_vue(&groupee).unwrap();
    assert_eq!(g.iter().map(|x| (x.titre.as_str(), x.nombre)).collect::<Vec<_>>(), [("Thanos", 3), ("Warlock", 1)]);
    b.deplacer_groupes_liste(&liste, &groupee, &["Warlock".into()], 0).unwrap();
    assert_eq!(ids_vue(&b, &groupee), [id_livre(1, 2), id_livre(0, 3), id_livre(0, 1), id_livre(0, 2)]);
    b.decaler_groupe_liste(&liste, &groupee, "Warlock", false).unwrap();
    assert_eq!(ids_vue(&b, &ordre), [id_livre(0, 3), id_livre(0, 1), id_livre(0, 2), id_livre(1, 2)]);
    let r = b.decaler_groupe_liste(&liste, &groupee, "Warlock", false).unwrap();
    assert!(r.info.is_empty(), "déjà en dernier");
    assert_eq!(ids_vue(&b, &ordre).len(), 4, "aucun livre perdu");

    // Dépôt de livres : liste de lecture acceptée, sauf celle affichée ; dossier refusé.
    assert_eq!(b.depot_livres_possible(&liste, None).unwrap(), None);
    assert_eq!(
        b.depot_livres_possible(&liste, Some(&liste)).unwrap().unwrap(),
        "Les livres sont déjà dans cette liste"
    );
    assert!(b.depot_livres_possible(&dossier, None).unwrap().unwrap().starts_with("Dossier"));

    // Liste intelligente : « Série contient Thanos » OU groupe (Année est 1991), aperçu = contenu.
    let mut c: CriteresListe = b.lire_criteres(None).unwrap().criteres;
    assert_eq!(c.nom, "Nouvelle liste intelligente");
    c.nom = "Intelligente".into();
    c.mode = ModeCriteres::AuMoinsUn;
    let NoeudCritere::Critere(premier) = &mut c.noeuds[0] else { panic!() };
    assert_eq!((premier.type_critere.as_str(), premier.op), ("ComicBookSeriesMatcher", 1));
    premier.valeur = "Thanos".into();
    let mut annee = crate::listes_criteres::nouveau_critere("ComicBookYearMatcher", None).unwrap();
    annee.valeur = "1991".into();
    c.noeuds.push(NoeudCritere::Groupe(crate::listes_criteres::GroupeCriteres {
        non: false,
        mode: ModeCriteres::Tous,
        noeuds: vec![NoeudCritere::Critere(annee)],
        origine: None,
    }));
    let ap = b.apercu_criteres(None, &c).unwrap();
    assert_eq!(ap.nb_livres, Some(6), "{ap:?}");
    let r = b.enregistrer_criteres(None, &c, None, None, Some(&dossier)).unwrap();
    let intel = r.creee.unwrap();
    assert_eq!(noeud(&r.arbre, &intel).nb_livres, Some(6));
    assert_eq!(r.arbre.listes.iter().map(|l| l.id.clone()).collect::<Vec<_>>(), [dossier.clone(), intel.clone()]);
    // Liste qui part de « Lecture » : dépendance signalée à la suppression.
    let mut c2 = b.lire_criteres(None).unwrap().criteres;
    c2.nom = "Depuis Lecture".into();
    c2.depart = Some(liste.clone());
    let NoeudCritere::Critere(x) = &mut c2.noeuds[0] else { panic!() };
    x.valeur = "Warlock".into();
    let depuis = b.enregistrer_criteres(None, &c2, None, None, None).unwrap().creee.unwrap();
    assert_eq!(noeud(&b.arbre_listes().unwrap(), &depuis).nb_livres, Some(1));
    // Boucle refusée : « Lecture » ne peut pas devenir… (c'est une liste de lecture) ; « Depuis
    // Lecture » n'est pas proposée comme départ d'elle-même.
    let ed = b.lire_criteres(Some(&depuis)).unwrap();
    assert!(ed.listes_depart.iter().all(|l| l.id != depuis.to_ascii_lowercase()));

    // Renommer, modifier (description, combinaison), dupliquer, déplacer, monter.
    b.renommer_liste(&liste, "Thanos et Cie").unwrap();
    let r = b.modifier_liste(&dossier, None, Some("Mes lectures".into()), Some(Combinaison::Vide)).unwrap();
    let nd = noeud(&r.arbre, &dossier);
    assert_eq!(
        (nd.description.as_deref(), nd.combinaison, nd.nb_livres),
        (Some("Mes lectures"), Some(Combinaison::Vide), Some(0))
    );
    assert!(b.modifier_liste(&dossier, Some("Nouveau dossier".into()), None, None).unwrap().info.is_empty());
    let r = b.dupliquer_liste(&liste).unwrap();
    let copie = r.creee.unwrap();
    assert_eq!(noeud(&r.arbre, &copie).nom, "Copie de Thanos et Cie");
    assert_eq!(noeud(&r.arbre, &copie).nb_livres, Some(4));
    let dest = b.destinations_liste(&copie).unwrap();
    assert_eq!(dest[0].id, None);
    assert!(dest.iter().any(|x| x.actuel && x.id.as_deref() == Some(dossier.as_str())));
    assert!(b.destinations_liste(&dossier).unwrap().iter().all(|x| x.id.as_deref() != Some(dossier.as_str())));
    let r = b.deplacer_liste(&copie, None, Some(0)).unwrap();
    assert_eq!(r.arbre.listes[0].id, copie);
    assert!(b.place_depot_liste(&copie, &dossier, PlaceDepot::Dans));
    assert!(!b.place_depot_liste(&dossier, &liste, PlaceDepot::Apres), "dans sa propre branche");
    assert!(!b.place_depot_liste(&copie, &intel, PlaceDepot::Dans), "pas un dossier");
    assert!(b.deposer_liste(&dossier, &liste, PlaceDepot::Avant).is_err());
    let r = b.deposer_liste(&copie, &liste, PlaceDepot::Apres).unwrap();
    let enfants: Vec<String> = noeud(&r.arbre, &dossier).enfants.unwrap().into_iter().map(|n| n.id).collect();
    assert_eq!(enfants, [liste.clone(), copie.clone()]);
    let r = b.decaler_liste(&copie, true).unwrap();
    let enfants: Vec<String> = noeud(&r.arbre, &dossier).enfants.unwrap().into_iter().map(|n| n.id).collect();
    assert_eq!(enfants, [copie.clone(), liste.clone()]);

    // Conversion : la liste intelligente devient une liste de lecture de ses 6 livres.
    let ic = b.infos_conversion(&intel).unwrap();
    assert_eq!((ic.nom.as_str(), ic.nb_livres), ("Intelligente", 6));
    let r = b.convertir_en_lecture(&intel).unwrap();
    assert_eq!(noeud(&r.arbre, &intel).type_liste, TypeListe::Lecture);
    assert_eq!(noeud(&r.arbre, &intel).nb_livres, Some(6));

    // Suppression : sous-listes et dépendances annoncées ; liste affichée → sélection effacée.
    let info = b.infos_suppression(&dossier).unwrap();
    assert_eq!((info.sous_listes, info.dependantes.clone()), (2, vec!["Depuis Lecture".to_string()]));
    let r = b.supprimer_liste(&dossier, Some(&liste)).unwrap();
    assert_eq!(r.selection, Some(None));
    assert!(r.info.contains("avec 2 sous-listes") && r.info.contains("« Depuis Lecture »"), "{}", r.info);

    // Persistance : une autre instance relit les mêmes listes.
    let b2 = Biblio::nouvelle(b.data().to_path_buf());
    b2.ouvrir().unwrap();
    let noms = |a: &ArbreListes| tous(a).into_iter().map(|(p, n)| (p, n.nom, n.nb_livres)).collect::<Vec<_>>();
    assert_eq!(noms(&b2.arbre_listes().unwrap()), noms(&b.arbre_listes().unwrap()));
    assert_eq!(noms(&b2.arbre_listes().unwrap()).len(), 2);
}

/// Une seule opération à la fois ; erreurs du cœur montrées telles quelles.
#[test]
fn une_operation_a_la_fois_et_erreurs_lisibles() {
    let d = tempfile::tempdir().unwrap();
    let b = biblio_autonome(d.path());
    let b2 = b.clone();
    let r = b.executer("Opération longue", move |_| {
        let e = b2
            .creer_liste(&DemandeCreation {
                type_liste: TypeNouvelle::Dossier,
                nom: None,
                parent: None,
                position: None,
                livres: None,
                apres_liste: None,
            })
            .unwrap_err();
        assert_eq!(e, DEJA_EN_COURS);
        assert_eq!(b2.etat_listes().en_cours.as_deref(), Some("Opération longue"));
        assert!(!b2.etat_listes().modifiable);
        Ok(Issue::default())
    });
    assert_eq!(r.unwrap().info, "✔ Opération longue");
    assert!(b.etat_listes().en_cours.is_none());
    let id = b
        .creer_liste(&DemandeCreation {
            type_liste: TypeNouvelle::Lecture,
            nom: Some("X".into()),
            parent: None,
            position: None,
            livres: None,
            apres_liste: None,
        })
        .unwrap()
        .creee
        .unwrap();
    assert!(b.renommer_liste(&id, "  ").unwrap_err().contains("vide"));
    assert!(b.dupliquer_liste("inconnue").unwrap_err().contains("introuvable"));
    // Identifiant de livre qui n'est pas un GUID : refusé (ComicRack ne pourrait plus lire ses
    // listes), rien d'écrit.
    let err = b.ajouter_livres_liste(&id, vec!["x".into()], None).unwrap_err();
    assert!(err.contains("GUID attendu"), "{err}");
    assert_eq!(noeud(&b.arbre_listes().unwrap(), &id).nb_livres, Some(0));
}

// ───────────────────────────── Revue de l'étape 5 ─────────────────────────────

/// `ComicDb.xml` : un identifiant de livre invalide n'est jamais écrit (XmlSerializer lit des
/// `Guid` : un seul mauvais et ComicRack ne charge plus le fichier) ; GUID en majuscules
/// accepté et écrit en minuscules.
#[test]
fn comicdb_identifiants_de_livres_controles() {
    let d = tempfile::tempdir().unwrap();
    let (b, xml) = biblio_comicdb(d.path());
    ecriture(&b, true);
    let original = std::fs::read(&xml).unwrap();
    // Identifiant vide : ignoré, rien n'est réécrit (pas même une sauvegarde).
    let r = b.ajouter_livres_liste(TEST_LISTE, vec!["  ".into()], None).unwrap();
    assert!(r.info.starts_with("0 livre ajouté") && r.sauvegarde.is_none(), "{r:?}");
    assert_eq!(std::fs::read(&xml).unwrap(), original);
    assert_eq!(sauvegardes(d.path()), (vec![], vec![]));
    for mauvais in ["pas-un-guid", "{11111111-2222-4333-8444-555555555555}", "11111111-2222-4333-8444-55555555555g"] {
        let err = b.ajouter_livres_liste(TEST_LISTE, vec![mauvais.into()], None).unwrap_err();
        assert!(err.contains("GUID attendu"), "{mauvais} : {err}");
        let err = b
            .creer_liste(&DemandeCreation {
                type_liste: TypeNouvelle::Lecture,
                nom: Some("Mauvaise".into()),
                parent: None,
                position: None,
                livres: Some(vec!["ffffffff-0000-4000-8000-000000000001".into(), mauvais.into()]),
                apres_liste: None,
            })
            .unwrap_err();
        assert!(err.contains("GUID attendu"), "{mauvais} : {err}");
    }
    assert_eq!(std::fs::read(&xml).unwrap(), original, "rien n'a été écrit");
    let r = b.ajouter_livres_liste(TEST_LISTE, vec!["FFFFFFFF-0000-4000-8000-000000000001".into()], None).unwrap();
    assert!(r.info.starts_with("1 livre ajouté"), "{}", r.info);
    let t = std::fs::read_to_string(&xml).unwrap();
    assert!(element(&t, TEST_LISTE).contains("<guid>ffffffff-0000-4000-8000-000000000001</guid>"));
    // Le reste du fichier est relu à l'identique par le cœur (aucune liste perdue).
    assert_eq!(load_comic_lists(&xml).unwrap().len(), load_comic_lists(Path::new(FIXTURE)).unwrap().len());
}

/// `ComicDb.xml` écrit par ComicRack (UTF-8 avec BOM, CRLF) : BOM et fins de ligne gardés, seul
/// `<ComicLists>` change.
#[test]
fn comicdb_avec_bom_garde_a_l_octet() {
    let d = tempfile::tempdir().unwrap();
    let (b, xml) = biblio_comicdb(d.path());
    ecriture(&b, true);
    let mut octets = "\u{feff}".as_bytes().to_vec();
    octets.extend(std::fs::read(FIXTURE).unwrap());
    std::fs::write(&xml, &octets).unwrap();
    assert!(b.etat_listes().erreur.is_none());
    b.renommer_liste(TEST_LISTE, "Renommée").unwrap();
    let apres = std::fs::read(&xml).unwrap();
    assert!(apres.starts_with("\u{feff}<?xml".as_bytes()), "BOM gardé");
    let (avant, apres) = (String::from_utf8(octets).unwrap(), String::from_utf8(apres).unwrap());
    let hors = |t: &str| {
        let a = t.find("<ComicLists>").unwrap();
        let z = t.find("</ComicLists>").unwrap();
        (t[..a].to_string(), t[z..].to_string())
    };
    assert_eq!(hors(&apres), hors(&avant));
    assert!(!apres.replace("\r\n", "").contains('\n'), "CRLF");
    assert!(element(&apres, TEST_LISTE).contains("Name=\"Renommée\""));
    assert_eq!(element(&apres, GUIDE), element(&avant, GUIDE));
}

/// Éditeur : un critère non modifiable (absent du catalogue) n'est recopié que si son origine
/// désigne toujours le même critère (liste modifiée dans ComicRack entre-temps : refus, plutôt
/// que dupliquer un autre critère).
#[test]
fn critere_non_modifiable_et_origine_changee() {
    use crate::listes_criteres::{vers_requete, Critere};
    use comicrust_core::smart::{Matcher, SmartQuery};
    let inconnu = Matcher::value("ComicBookMachinInconnuMatcher", 0, "x", "", false, true);
    let serie = Matcher::value("ComicBookSeriesMatcher", 1, "Thanos", "", false, true);
    let critere = Critere {
        non: false,
        type_critere: "ComicBookMachinInconnuMatcher".into(),
        op: 0,
        valeur: "x".into(),
        valeur2: String::new(),
        ignorer_casse: true,
        origine: Some("0".into()),
        modifiable: false,
        libelle: None,
    };
    let saisie = CriteresListe {
        nom: "L".into(),
        description: String::new(),
        mode: ModeCriteres::Tous,
        noeuds: vec![NoeudCritere::Critere(critere)],
        depart: None,
        pas_dans_depart: false,
        limite: None,
        retires: 0,
        reinitialiser_retires: None,
    };
    let q = |m: Vec<Matcher>| SmartQuery { matchers: m, ..Default::default() };
    let ok = vers_requete(&saisie, Some(&q(vec![inconnu.clone(), serie.clone()]))).unwrap();
    assert_eq!(ok.matchers, vec![inconnu.clone()]);
    // ComicRack a supprimé le critère inconnu : « 0 » désigne maintenant « Série ».
    let err = vers_requete(&saisie, Some(&q(vec![serie]))).unwrap_err();
    assert!(err[0].contains("a changé depuis l'ouverture"), "{err:?}");
    assert!(vers_requete(&saisie, Some(&q(vec![]))).is_err());
}

/// `.cbl` hostiles : entités (« milliard de rires »), fichier énorme, imbrication profonde,
/// dossier au lieu d'un fichier : refus lisible, sans tout charger ni planter ; rapport borné.
#[test]
fn cbl_hostiles() {
    let d = tempfile::tempdir().unwrap();
    let b = biblio_autonome(d.path());
    let ecrire = |nom: &str, t: &str| {
        let p = d.path().join(nom);
        std::fs::write(&p, t).unwrap();
        p.display().to_string()
    };
    let rires = ecrire(
        "rires.cbl",
        "<?xml version=\"1.0\"?>\n<!DOCTYPE l [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\">\
         <!ENTITY c \"&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;\"><!ENTITY e \"&c;&c;&c;&c;&c;&c;&c;&c;&c;&c;\">]>\n\
         <ReadingList><Name>&e;</Name><Books><Book Series=\"&e;\" Number=\"1\" /></Books></ReadingList>",
    );
    let t = std::time::Instant::now();
    let r = b.analyser_cbl(&rires);
    assert!(t.elapsed() < std::time::Duration::from_secs(2));
    if let Ok(r) = r {
        assert!(r.nom.len() < 100 && r.texte.len() < 10_000, "entités non développées");
    }
    let externe = ecrire(
        "externe.cbl",
        "<?xml version=\"1.0\"?><!DOCTYPE l [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><ReadingList><Name>&x;</Name>\
         <Books /></ReadingList>",
    );
    if let Ok(r) = b.analyser_cbl(&externe) {
        assert!(!r.nom.contains("root"), "entité externe jamais lue");
    }
    // Fichier énorme (creux) : refusé d'après sa taille, sans lecture.
    let gros = d.path().join("gros.cbl");
    std::fs::File::create(&gros).unwrap().set_len(crate::listes_cbl::TAILLE_MAX_CBL + 1).unwrap();
    let err = b.analyser_cbl(&gros.display().to_string()).unwrap_err();
    assert!(err.contains("trop gros"), "{err}");
    assert!(b.importer_cbl(&gros.display().to_string(), None, None, None, None).is_err());
    // Imbrication très profonde : pas de débordement de pile.
    let profond = ecrire(
        "profond.cbl",
        &format!("<ReadingList><Books>{}{}</Books></ReadingList>", "<x>".repeat(200_000), "</x>".repeat(200_000)),
    );
    let _ = b.analyser_cbl(&profond);
    assert!(b.analyser_cbl(&d.path().display().to_string()).unwrap_err().contains("pas un fichier"));
    // 1 500 entrées introuvables : comptées, 1 000 détaillées.
    let livres: String = (0..1500).map(|i| format!("<Book Series=\"Absente\" Number=\"{i}\" />")).collect();
    let beaucoup = ecrire("beaucoup.cbl", &format!("<ReadingList><Name>B</Name><Books>{livres}</Books></ReadingList>"));
    let r = b.analyser_cbl(&beaucoup).unwrap();
    assert_eq!((r.total, r.trouves, r.manquants.len()), (1500, 0, crate::listes_cbl::MANQUANTS_MAX));
    assert!(
        r.texte.contains("Manquants (1500)") && r.texte.ends_with("… et 500 autres manquants"),
        "{}",
        &r.texte[..80]
    );
    assert!(b.arbre_listes().unwrap().listes.is_empty(), "rien n'a été créé");
}

/// Performance : 5 000 livres et les 47 listes de la fixture (arbre, liste de lecture de 321
/// livres, vue groupée, état) restent rapides, même en mode test (non optimisé).
#[test]
fn performance_5000_livres_47_listes() {
    let d = tempfile::tempdir().unwrap();
    let (b, _) = biblio_comicdb(d.path());
    let mut livres = base_fixture().books;
    for i in livres.len()..5000 {
        livres.push(livre(
            &format!("{i:08x}-0000-4000-8000-000000000000"),
            &format!("Série {}", i / 7),
            &(i % 7 + 1).to_string(),
        ));
    }
    b.installer(Mode::Comicrack, ComicDatabase::from_books(livres), None);
    let t = std::time::Instant::now();
    let a = b.arbre_listes().unwrap();
    let arbre = t.elapsed();
    assert_eq!(tous(&a).len(), 47);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        b.etat_listes();
        b.arbre_listes().unwrap();
    }
    let repete = t.elapsed();
    let t = std::time::Instant::now();
    assert_eq!(b.lister(&requete(Some(GUIDE), "ordre", Some(Groupe::Serie))).unwrap().total, 321);
    b.groupes_vue(&requete(Some(GUIDE), "ordre", Some(Groupe::Serie))).unwrap();
    for (_, n) in tous(&a) {
        b.lister(&requete(Some(&n.id), "serie", None)).unwrap();
    }
    let vues = t.elapsed();
    eprintln!("arbre {arbre:?}, 20 × (état + arbre) {repete:?}, 49 vues {vues:?}");
    assert!(arbre < std::time::Duration::from_secs(5), "{arbre:?}");
    assert!(repete < std::time::Duration::from_secs(5), "cache des comptes : {repete:?}");
    assert!(vues < std::time::Duration::from_secs(10), "{vues:?}");
}

// ───────────────────────────── .cbl ─────────────────────────────

#[test]
fn import_et_export_cbl() {
    let d = tempfile::tempdir().unwrap();
    let b = biblio_autonome(d.path());
    let cbl = d.path().join("Thanos.cbl");
    std::fs::write(
        &cbl,
        format!(
            "\u{feff}<?xml version=\"1.0\"?>\r\n<ReadingList xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\">\r\n  \
             <Name>Saga</Name>\r\n  <Books>\r\n    <Book Series=\"Warlock\" Number=\"2\" Volume=\"1972\" Year=\"1991\">\r\n      \
             <Id>{}</Id>\r\n    </Book>\r\n    <Book Series=\"THANOS\" Number=\"01\" />\r\n    \
             <Book Series=\"Thanos\" Number=\"1\" />\r\n    <Book Series=\"Infinity Gauntlet\" Number=\"2\" Year=\"1991\" />\r\n  \
             </Books>\r\n</ReadingList>",
            id_livre(1, 2)
        ),
    )
    .unwrap();
    let rap = b.analyser_cbl(&cbl.display().to_string()).unwrap();
    assert_eq!((rap.nom.as_str(), rap.total, rap.trouves), ("Saga", 4, 3));
    assert_eq!((rap.par_identifiant, rap.par_serie, rap.doublons), (1, 2, 1));
    assert_eq!(rap.manquants.len(), 1);
    assert_eq!((rap.manquants[0].rang, rap.manquants[0].serie.as_str()), (4, "Infinity Gauntlet"));
    assert!(rap.texte.starts_with("Liste « Saga » : 3 / 4 numéros trouvés"), "{}", rap.texte);
    assert!(b.arbre_listes().unwrap().listes.is_empty(), "l'analyse ne crée rien");
    let imp = b.importer_cbl(&format!("\"{}\"", cbl.display()), None, None, None, None).unwrap();
    let id = imp.resultat.creee.clone().unwrap();
    assert_eq!(noeud(&imp.resultat.arbre, &id).nom, "Saga");
    assert_eq!(ids_vue(&b, &requete(Some(&id), "ordre", None)), [id_livre(1, 2), id_livre(0, 1)]);
    // Export : ordre de la liste, identifiants, extension ajoutée ; relu par le cœur.
    let ex = b.exporter_cbl(&id, &d.path().join("sortie").display().to_string()).unwrap();
    assert!(ex.chemin.ends_with("sortie.cbl") && ex.livres == 2);
    let relu = comicrust_core::ReadingList::from_xml(&std::fs::read_to_string(&ex.chemin).unwrap()).unwrap();
    assert_eq!(relu.name, "Saga");
    assert_eq!(relu.books.book[0].series, "Warlock");
    assert_eq!(relu.books.book[0].id.as_deref(), Some(id_livre(1, 2).as_str()));
    assert_eq!(relu.books.book[1].year, Some(1990));
    assert!(b.analyser_cbl("/n/existe/pas.cbl").is_err());
}

/// Lecture seule (mode lié sans option) : l'export reste possible, l'import est refusé.
#[test]
fn cbl_en_lecture_seule() {
    let d = tempfile::tempdir().unwrap();
    let (b, xml) = biblio_comicdb(d.path());
    let avant = std::fs::read(&xml).unwrap();
    let ex = b.exporter_cbl(GUIDE, &d.path().join("guide.cbl").display().to_string()).unwrap();
    assert_eq!(ex.livres, 321);
    let err = b.importer_cbl(&ex.chemin, Some("Copie".into()), None, None, None).unwrap_err();
    assert_eq!(err, LECTURE_SEULE);
    assert_eq!(std::fs::read(&xml).unwrap(), avant);
    // Relecture du .cbl exporté : tous retrouvés par identifiant.
    let rap = b.analyser_cbl(&ex.chemin).unwrap();
    assert_eq!((rap.trouves, rap.par_identifiant), (321, 321));
}

// ───────────────────────────── MySQL ─────────────────────────────

/// Base de ComicRust MySQL (`COMICRUST_TEST_MYSQL`) : deux postes ; une modification faite par
/// l'autre poste pendant une opération fait refuser celle-ci, sans rien écrire.
#[test]
fn mysql_deux_postes() {
    let Ok(url) = std::env::var("COMICRUST_TEST_MYSQL") else {
        eprintln!("COMICRUST_TEST_MYSQL absent : test MySQL des listes ignoré");
        return;
    };
    let db = comicrust_core::db::DbConfig::from_connection_string(&url).unwrap();
    assert_ne!(db.database, "comickrack_demo", "jamais la base de démonstration");
    let cfg = StoreConfig::MySql(db);
    let Ok(mut s) = Store::open(&cfg) else {
        eprintln!("MySQL de test injoignable : test ignoré");
        return;
    };
    let snap = s.list_rows().unwrap();
    s.save_list_rows(&[], snap.counter).unwrap();
    let d = tempfile::tempdir().unwrap();
    let poste = |nom: &str| {
        let data = d.path().join(nom);
        std::fs::create_dir_all(&data).unwrap();
        let b = Biblio::nouvelle(data);
        b.installer(
            Mode::Autonome,
            ComicDatabase::from_books(vec![livre(&id_livre(0, 1), "Thanos", "1")]),
            Some(cfg.clone()),
        );
        b
    };
    let (a, b) = (poste("a"), poste("b"));
    let nouvelle = |nom: &str| DemandeCreation {
        type_liste: TypeNouvelle::Lecture,
        nom: Some(nom.into()),
        parent: None,
        position: None,
        livres: Some(vec![id_livre(0, 1)]),
        apres_liste: None,
    };
    let id = a.creer_liste(&nouvelle("Poste A")).unwrap().creee.unwrap();
    // L'autre poste voit la liste.
    assert_eq!(noeud(&b.arbre_listes().unwrap(), &id).nb_livres, Some(1));
    // Modification concurrente : refus, rien d'écrit par A.
    let b2 = b.clone();
    let err = a
        .executer("Renommage", move |ed| {
            ed.rename(&id, "Par A")?;
            b2.creer_liste(&nouvelle("Poste B")).unwrap();
            Ok(Issue::default())
        })
        .unwrap_err();
    assert!(err.contains("modifiées ailleurs"), "{err}");
    let noms: Vec<String> = tous(&b.arbre_listes().unwrap()).into_iter().map(|(_, n)| n.nom).collect();
    assert_eq!(noms, ["Poste A", "Poste B"], "renommage de A non écrit");
    // A recommence : il relit et réussit.
    let noms_a: Vec<String> = tous(&a.arbre_listes().unwrap()).into_iter().map(|(_, n)| n.nom).collect();
    assert_eq!(noms_a, ["Poste A", "Poste B"]);
    let snap = s.list_rows().unwrap();
    s.save_list_rows(&[], snap.counter).unwrap();
}

/// Mode lié MySQL : les listes sont dans le `ComicDb.xml` trouvé d'après `ComicRack.ini`
/// (`DatabasePath`), en lecture seule sans l'option d'écriture.
#[test]
fn mode_mysql_listes_dans_comicdb() {
    let d = tempfile::tempdir().unwrap();
    let base = d.path().join("Base");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::copy(FIXTURE, base.join("ComicDb.xml")).unwrap();
    let ini = d.path().join("ComicRack.ini");
    std::fs::write(&ini, format!("DatabasePath={}\r\n", base.display())).unwrap();
    let data = d.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    reglages::modifier(&data, |r| {
        r.mode = Some(Mode::Comicrack);
        r.chemin_ini = Some(ini.clone());
    })
    .unwrap();
    let b = Biblio::nouvelle(data);
    b.installer(Mode::Comicrack, base_fixture(), None);
    b.installer_comicrack_mysql(comicrust_core::db::DbConfig::default());
    b.simuler_comicrack(Arc::new(|| Ok(false)));
    let a = b.arbre_listes().unwrap();
    assert_eq!(a.etat.chemin.as_deref(), Some(base.join("ComicDb.xml").display().to_string().as_str()));
    assert_eq!(tous(&a).len(), 47);
    assert_eq!(a.etat.lecture_seule.as_deref(), Some(LECTURE_SEULE));
    // ComicDb.xml introuvable : erreur lisible, aucune liste.
    std::fs::remove_file(base.join("ComicDb.xml")).unwrap();
    let a = b.arbre_listes().unwrap();
    assert!(a.listes.is_empty() && a.etat.erreur.as_deref().is_some_and(|e| e.contains("introuvable")), "{:?}", a.etat);
    assert_eq!(a.etat.source, SourceListes::Aucune);
}

// ───────── Série / numéro déduits du nom de fichier (ShadowSeries) ─────────

/// Livre sans série ni numéro en fiche : seul le nom de fichier les porte.
fn livre_sans_fiche(id: &str, fichier: &str) -> LibraryBook {
    LibraryBook { id: id.into(), file: Some(fichier.into()), ..Default::default() }
}

#[test]
fn cbl_rattache_par_serie_deduite_du_nom_de_fichier() {
    use comicrust_core::cbl::{CblBook, CblBooks, ReadingList};
    let mut db = ComicDatabase::default();
    db.books.push(livre_sans_fiche("a0000000-0000-0000-0000-000000000001", r"Z:\BD\Silver Surfer #3 ok.pdf"));
    db.books.push(livre_sans_fiche("a0000000-0000-0000-0000-000000000002", r"Z:\BD\Warlock #9.cbz"));
    let entree = |s: &str, n: &str| CblBook {
        series: s.into(),
        number: n.into(),
        volume: None,
        year: None,
        format: None,
        id: None,
    };
    let liste = ReadingList {
        name: "T".into(),
        books: CblBooks { book: vec![entree("Silver Surfer", "3"), entree("Warlock", "9"), entree("Inconnu", "1")] },
    };
    let (ids, rapport) = crate::listes_cbl::rattacher(&liste, "T", &db);
    assert_eq!(rapport.trouves, 2, "{}", rapport.texte);
    assert_eq!(ids.len(), 2);
    assert_eq!(rapport.manquants.len(), 1);
}

#[test]
fn affichage_et_regroupement_utilisent_la_serie_deduite() {
    let b = livre_sans_fiche(
        "a0000000-0000-0000-0000-000000000003",
        r"Z:\BD\COSMIC COLLECTION #009 2012_05 - Thanos (X)(2013).cbz",
    );
    assert_eq!(b.shadow_series(), "COSMIC COLLECTION");
    assert_eq!(crate::tri::Regroupement::Serie.titre(&b), "COSMIC COLLECTION");
    // Une série en fiche reste prioritaire.
    let mut c = b.clone();
    c.series = Some("Ma série".into());
    assert_eq!(crate::tri::Regroupement::Serie.titre(&c), "Ma série");
}

/// Noms réels de l'échantillon `Thanos.cbl` (recette S-02) : numéro sur deux chiffres, année
/// entre parenthèses, livres sans fiche.
#[test]
fn cbl_thanos_noms_reels_des_echantillons() {
    use comicrust_core::cbl::ReadingList;
    let mut db = ComicDatabase::default();
    for (k, f) in [
        r"Z:\E\Silver Surfer 03.pdf",
        r"Z:\E\Silver Surfer 04 (1987).cbz",
        r"Z:\E\Warlock 09 (1976).cbz",
        r"Z:\E\Thanos Quest 01 (1990).cbz",
        r"Z:\E\Infinity Gauntlet 02 (1991).cbt",
    ]
    .iter()
    .enumerate()
    {
        db.books.push(livre_sans_fiche(&format!("b0000000-0000-0000-0000-00000000000{k}"), f));
    }
    let texte = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../recette/echantillons/Thanos.cbl")).unwrap();
    let liste = ReadingList::from_xml(&texte).unwrap();
    let (_, rapport) = crate::listes_cbl::rattacher(&liste, "Thanos", &db);
    assert_eq!(rapport.trouves, 5, "{}", rapport.texte);
}

/// Recette E-01 : un critère sans marqueur `genre` (tel que renvoyé par `nouveau_critere`) est
/// accepté ; un groupe sans marqueur est reconnu à ses `noeuds` ; le marqueur reste écrit en sortie.
#[test]
fn noeud_sans_genre_accepte() {
    use crate::listes_criteres::{CriteresListe, NoeudCritere};
    let c = crate::listes_criteres::nouveau_critere("ComicBookPageCountMatcher", None).unwrap();
    let mut critere = serde_json::to_value(&c).unwrap();
    assert!(critere.get("genre").is_none());
    critere["valeur"] = "10".into();
    let json = serde_json::json!({
        "nom": "x", "mode": "tous",
        "noeuds": [critere, { "mode": "auMoinsUn", "noeuds": [{ "genre": "critere", "type": "ComicBookSeriesMatcher", "op": 1, "valeur": "a" }] }]
    });
    let l: CriteresListe = serde_json::from_value(json).unwrap();
    assert!(matches!(l.noeuds[0], NoeudCritere::Critere(_)));
    assert!(matches!(l.noeuds[1], NoeudCritere::Groupe(_)));
    let sortie = serde_json::to_value(&l).unwrap();
    assert_eq!(sortie["noeuds"][0]["genre"], "critere");
    assert_eq!(sortie["noeuds"][1]["genre"], "groupe");
    assert!(serde_json::from_value::<NoeudCritere>(serde_json::json!({ "genre": "autre" })).is_err());
}
