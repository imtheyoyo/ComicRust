//! Tests du lot L7 : diagnostic sans secret, bascule de base sans perte, « Copier les livres »,
//! panneau des listes mémorisé. Aucun MySQL réel (les identifiants du test sont fictifs).

use crate::biblio::*;
use crate::couvertures::cbz_de_test;
use crate::diagnostic::{self, EtatPdfium, SessionDiagnostic};
use crate::reglages::{self, Mode, PreferencesVue};
use comicrust_core::db::DbConfig;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::Duration;

const MDP: &str = "MOT-DE-PASSE-FACTICE-9f3a";
const UTILISATEUR: &str = "utilisateur_secret";
const SERVEUR: &str = "nas-secret.local";

fn connexion_factice() -> DbConfig {
    DbConfig {
        host: SERVEUR.into(),
        port: 3307,
        user: UTILISATEUR.into(),
        password: MDP.into(),
        database: "base_secrete".into(),
    }
}

fn info(serie: &str, numero: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><ComicInfo><Series>{serie}</Series><Number>{numero}</Number>\
         <Publisher>Marvel</Publisher><Year>2020</Year></ComicInfo>"
    )
}

/// Bibliothèque autonome avec 3 CBZ analysés (Thanos 1 et 2, Astérix 1).
fn autonome_avec_livres(dossier: &Path) -> Biblio {
    let bd = dossier.join("bd");
    std::fs::create_dir_all(&bd).unwrap();
    let c = [10, 20, 30, 255];
    cbz_de_test(&bd.join("Thanos 01.cbz"), c, 3, Some(&info("Thanos", "1")));
    cbz_de_test(&bd.join("Thanos 02.cbz"), c, 3, Some(&info("Thanos", "2")));
    cbz_de_test(&bd.join("Asterix 01.cbz"), c, 3, Some(&info("Astérix", "1")));
    let b = Biblio::nouvelle(dossier.join("data"));
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    b.ajouter_dossier(&bd.to_string_lossy()).unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    b.lancer_scan(Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    }))
    .unwrap();
    loop {
        let e = rx.recv_timeout(Duration::from_secs(60)).expect("l'analyse doit se terminer");
        if matches!(e, EvenementScan::Termine { .. } | EvenementScan::Erreur { .. }) {
            break;
        }
    }
    b
}

fn requete(recherche: Option<&str>) -> RequeteLivres {
    RequeteLivres {
        recherche: recherche.map(str::to_string),
        tri: "serie".into(),
        sens_tri: SensTri::Asc,
        groupe: None,
        debut: 0,
        nombre: 100,
        liste: None,
        serie: None,
    }
}

fn pdfium_fictif() -> EtatPdfium {
    EtatPdfium { charge: false, chemin: None, erreur: Some("PDFium indisponible : test".into()) }
}

// ───────────────────────────── Diagnostic sans secret ─────────────────────────────

#[test]
fn diagnostic_sans_secret() {
    let d = tempfile::tempdir().unwrap();
    let ini = d.path().join("ComicRack.ini");
    reglages::modifier(d.path(), |r| {
        r.mode = Some(Mode::Comicrack);
        r.chemin_ini = Some(ini.clone());
        r.store_mysql = Some(connexion_factice());
        r.comicrack_mysql = Some(connexion_factice());
    })
    .unwrap();
    let session = SessionDiagnostic { mode: Some(Mode::Comicrack), source: "mysql".into(), nb_livres: Some(42) };
    let infos = diagnostic::collecter(d.path(), &session, pdfium_fictif());
    let texte = diagnostic::texte_diagnostic(&infos);
    let json = serde_json::to_string(&infos).unwrap();
    let config = serde_json::to_string(&diagnostic::configuration(d.path()).unwrap()).unwrap();
    for sortie in [&texte, &json, &config] {
        for secret in [MDP, UTILISATEUR, SERVEUR, "base_secrete"] {
            assert!(!sortie.contains(secret), "« {secret} » ne doit pas apparaître : {sortie}");
        }
    }
    // Ce qui doit y figurer : version, système, mode, chemins, état de PDFium, présence de MySQL.
    assert!(texte.contains(env!("CARGO_PKG_VERSION")), "{texte}");
    assert!(texte.contains(std::env::consts::OS), "{texte}");
    assert!(texte.contains("lié à ComicRack") && texte.contains("Livres : 42"), "{texte}");
    assert!(texte.contains(&ini.display().to_string()), "{texte}");
    assert!(texte.contains(&d.path().display().to_string()), "{texte}");
    assert!(texte.contains("MySQL (paramètres non inclus)"), "{texte}");
    assert!(texte.contains("PDFium : non chargé — PDFium indisponible : test"), "{texte}");
    assert!(infos.store_mysql && infos.comicrack_mysql);
}

#[test]
fn diagnostic_sans_bibliotheque_ni_reglages_lisibles() {
    let d = tempfile::tempdir().unwrap();
    // Réglages illisibles : le diagnostic reste possible (c'est justement quand il sert).
    std::fs::write(reglages::chemin_reglages(d.path()), "{ pas du json").unwrap();
    let infos = diagnostic::collecter(d.path(), &SessionDiagnostic::default(), pdfium_fictif());
    let texte = diagnostic::texte_diagnostic(&infos);
    assert!(texte.contains("aucune bibliothèque ouverte"), "{texte}");
    assert!(texte.contains("SQLite") && texte.contains("absente"), "{texte}");
    assert!(diagnostic::configuration(d.path()).is_err(), "la description de la bibliothèque, elle, signale le fichier illisible");
}

#[test]
fn chemin_de_pdfium_cherche_dans_l_ordre() {
    let d = tempfile::tempdir().unwrap();
    let (a, b) = (d.path().join("a"), d.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    assert_eq!(diagnostic::chemin_pdfium_dans(&[a.clone(), b.clone()]), None);
    let lib = diagnostic::nom_bibliotheque_pdfium();
    std::fs::write(b.join(lib), b"x").unwrap();
    assert_eq!(diagnostic::chemin_pdfium_dans(&[a.clone(), b.clone()]), Some(b.join(lib)));
    std::fs::write(a.join(lib), b"x").unwrap();
    assert_eq!(diagnostic::chemin_pdfium_dans(&[a.clone(), b]), Some(a.join(lib)));
    // L'état réel est cohérent : « chargé » ou une raison, jamais les deux.
    let e = diagnostic::etat_pdfium();
    assert_eq!(e.charge, e.erreur.is_none());
}

// ───────────────────────────── Bascule de base ─────────────────────────────

fn empreinte(chemin: &Path) -> (u64, Vec<u8>) {
    let octets = std::fs::read(chemin).unwrap();
    (octets.len() as u64, octets)
}

/// Passer d'une bibliothèque autonome à une bibliothèque liée puis revenir : la base SQLite n'est
/// jamais supprimée ni modifiée, les préférences d'affichage sont gardées, le consentement
/// d'écriture ComicRack n'est jamais reporté, et un choix qui échoue ne change rien.
#[test]
fn bascule_de_base_sans_perte_ni_suppression() {
    let d = tempfile::tempdir().unwrap();
    let b = autonome_avec_livres(d.path());
    let data = d.path().join("data");
    let base = data.join("comicrust.db");
    assert_eq!(b.lister(&requete(None)).unwrap().total, 3);
    reglages::enregistrer_preferences_vue(
        &data,
        PreferencesVue { theme: Some("moderne-sombre".into()), listes_visibles: Some(false), ..Default::default() },
    )
    .unwrap();
    reglages::modifier(&data, |r| r.edition.ecriture_comicrack = Some(true)).unwrap();
    let avant = empreinte(&base);

    let c = diagnostic::configuration(&data).unwrap();
    assert_eq!(c.mode, Some(Mode::Autonome));
    assert!(c.base_sqlite_presente && c.base_sqlite_taille == Some(avant.0) && !c.store_mysql);

    // Choix qui échoue (fichier introuvable) : rien n'est enregistré, la bibliothèque courante reste servie.
    let manquant = d.path().join("absent").join("ComicDb.xml");
    let e = b
        .configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(manquant.display().to_string()) })
        .unwrap_err();
    assert!(e.contains("introuvable"), "{e}");
    assert_eq!(reglages::lire(&data).unwrap().mode, Some(Mode::Autonome));
    assert_eq!(b.mode(), Some(Mode::Autonome));
    assert_eq!(b.lister(&requete(None)).unwrap().total, 3);

    // Bascule vers ComicRack (ComicDb.xml).
    let xml = d.path().join("ComicDb.xml");
    std::fs::write(
        &xml,
        "<ComicDatabase><Books><Book Id=\"11111111-0000-0000-0000-000000000001\" File=\"Z:\\BD\\Hulk 01.cbz\">\
         <Series>Hulk</Series><Number>1</Number><PageCount>20</PageCount></Book></Books></ComicDatabase>",
    )
    .unwrap();
    let etat = b
        .configurer(&ChoixPremierDemarrage { mode: Mode::Comicrack, chemin_ini: Some(xml.display().to_string()) })
        .unwrap();
    assert_eq!((etat.mode, etat.nb_livres), (Mode::Comicrack, 1));
    assert_eq!(b.lister(&requete(None)).unwrap().total, 1, "la vue montre la nouvelle bibliothèque");
    assert!(base.is_file(), "la base de ComicRust n'est jamais supprimée");
    assert_eq!(empreinte(&base), avant, "…ni modifiée");
    let r = reglages::lire(&data).unwrap();
    assert_eq!(r.vue.theme.as_deref(), Some("moderne-sombre"));
    assert_eq!(r.vue.listes_visibles, Some(false), "les préférences d'affichage sont gardées");
    assert_eq!(r.edition.ecriture_comicrack, None, "le consentement d'écriture ComicRack n'est pas reporté");
    let c = diagnostic::configuration(&data).unwrap();
    assert_eq!(c.mode, Some(Mode::Comicrack));
    assert_eq!(c.chemin_ini, Some(xml.display().to_string()));
    assert!(c.base_sqlite_presente);

    // Retour à l'autonome : les 3 livres sont retrouvés (base intacte).
    let etat = b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    assert_eq!((etat.mode, etat.nb_livres), (Mode::Autonome, 3));
    assert_eq!(empreinte(&base), avant);
    assert_eq!(b.lister(&requete(None)).unwrap().total, 3);
    assert_eq!(reglages::lire(&data).unwrap().vue.theme.as_deref(), Some("moderne-sombre"));
}

/// « ⟳ Relire » = `ouvrir()` : recharge la même bibliothèque ; refusé pendant une analyse ou une
/// édition (comme un changement de bibliothèque).
#[test]
fn relire_recharge_la_meme_bibliotheque() {
    let d = tempfile::tempdir().unwrap();
    let b = autonome_avec_livres(d.path());
    let etat = b.ouvrir().unwrap();
    assert_eq!((etat.mode, etat.nb_livres, etat.premier_demarrage), (Mode::Autonome, 3, false));
    assert_eq!(b.lister(&requete(None)).unwrap().total, 3);
    let _garde = b.prendre_edition().unwrap();
    assert!(b.ouvrir().unwrap_err().contains("édition"), "pas de rechargement pendant une édition");
}

// ───────────────────────────── Copier les livres ─────────────────────────────

#[test]
fn rapport_des_livres_affiches() {
    let d = tempfile::tempdir().unwrap();
    let b = autonome_avec_livres(d.path());
    let t = diagnostic::rapport_livres(&b, &requete(None), "").unwrap();
    let lignes: Vec<_> = t.lines().collect();
    assert_eq!(lignes[0], format!("Bibliothèque — 3 livres (ComicRust v{})", env!("CARGO_PKG_VERSION")));
    assert_eq!(lignes[1], "");
    // Tri par série : Astérix, puis Thanos 1 et 2 ; une ligne « série #numéro (vVolume, année) — fichier ».
    assert_eq!(lignes.len(), 5, "{t}");
    assert!(lignes[2].starts_with("Astérix #1 (v") && lignes[2].contains(", 2020) — "), "{t}");
    assert!(lignes[2].ends_with("Asterix 01.cbz"), "{t}");
    assert!(lignes[3].starts_with("Thanos #1 ") && lignes[3].ends_with("Thanos 01.cbz"), "{t}");
    assert!(lignes[4].starts_with("Thanos #2 ") && lignes[4].ends_with("Thanos 02.cbz"), "{t}");
    // La recherche, le nom de la liste et la pile ouverte sont pris en compte.
    let t = diagnostic::rapport_livres(&b, &requete(Some("thanos")), "Ma liste").unwrap();
    assert!(t.starts_with("Ma liste — 2 livres"), "{t}");
    assert!(!t.contains("Astérix"), "{t}");
    let pile = RequeteLivres { serie: Some("Thanos".into()), ..requete(None) };
    assert!(diagnostic::rapport_livres(&b, &pile, "x").unwrap().starts_with("x — 2 livres"));
    // Rien d'ouvert : erreur claire.
    let vide = Biblio::nouvelle(d.path().join("autre"));
    assert!(diagnostic::rapport_livres(&vide, &requete(None), "").is_err());
}

// ───────────────────────────── Panneau des listes ─────────────────────────────

#[test]
fn panneau_listes_memorise_et_tolerant() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(reglages::lire_preferences_vue(d.path()).unwrap().listes_visibles, None, "affiché par défaut");
    let p = |v| PreferencesVue { listes_visibles: Some(v), ..Default::default() };
    assert_eq!(reglages::enregistrer_preferences_vue(d.path(), p(false)).unwrap().listes_visibles, Some(false));
    assert_eq!(reglages::lire_preferences_vue(d.path()).unwrap().listes_visibles, Some(false));
    let texte = std::fs::read_to_string(reglages::chemin_reglages(d.path())).unwrap();
    assert!(texte.contains("\"listesVisibles\": false"), "{texte}");
    // Autre préférence enregistrée : la valeur est gardée ; valeur absurde dans le fichier : ignorée.
    reglages::enregistrer_preferences_vue(d.path(), PreferencesVue { zoom: Some(150), ..Default::default() }).unwrap();
    assert_eq!(reglages::lire_preferences_vue(d.path()).unwrap().listes_visibles, Some(false));
    std::fs::write(reglages::chemin_reglages(d.path()), r#"{"mode":"autonome","vue":{"listesVisibles":"non","zoom":120}}"#).unwrap();
    let v = reglages::lire_preferences_vue(d.path()).unwrap();
    assert_eq!((v.listes_visibles, v.zoom), (None, Some(120)));
    assert_eq!(reglages::enregistrer_preferences_vue(d.path(), p(true)).unwrap().listes_visibles, Some(true));
}
