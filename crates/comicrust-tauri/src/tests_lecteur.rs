//! Tests du lecteur : CBZ généré ici, ordre / pages supprimées / rotation de la fiche,
//! avancement (autonome et lié à ComicRack), cache borné, jeton inconnu, délais.

use super::*;
use crate::biblio::{Biblio, ChoixPremierDemarrage};
use crate::reglages::Mode;
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::{LibraryBook, PageList};
use image::GenericImageView;
use std::io::Write;

/// CBZ de `n` pages PNG opaques de tailles différentes : image i = (100 + 10·i) × 300.
fn cbz_tailles(chemin: &Path, n: usize) {
    let mut w = zip::ZipWriter::new(std::fs::File::create(chemin).unwrap());
    for i in 0..n {
        w.start_file(format!("{:02}.png", i + 1), zip::write::SimpleFileOptions::default()).unwrap();
        let img = image::RgbaImage::from_pixel(100 + 10 * i as u32, 300, image::Rgba([20 * i as u8, 80, 160, 255]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        w.write_all(&png).unwrap();
    }
    w.finish().unwrap();
}

/// Sert une page et attend la réponse.
fn servir(l: &Lecteur, jeton: &str, index: usize, largeur: Option<u32>) -> ReponsePage {
    let (tx, rx) = mpsc::channel();
    l.servir(jeton, index, largeur, move |r| tx.send(r).unwrap());
    rx.recv_timeout(Duration::from_secs(30)).expect("réponse attendue")
}

/// Linux : attend la fin des lectures en cours (préchargement) puis vérifie qu'aucun descripteur
/// ne reste ouvert sur `f` (`/proc/self/fd`). Un fournisseur qui garderait le fichier ouvert ne
/// redescendrait jamais à zéro. Le cache doit aussi être stable (plus de préchargement).
#[cfg(target_os = "linux")]
pub(crate) fn attendre_aucun_descripteur(l: &Lecteur, f: &Path) {
    use comicrust_core::providers::on_demand::open_descriptors;
    let debut = Instant::now();
    let mut stable = 0;
    let mut avant = l.etat_cache();
    while stable < 5 {
        assert!(debut.elapsed() < Duration::from_secs(10), "descripteur resté ouvert sur {}", f.display());
        std::thread::sleep(Duration::from_millis(40));
        let maintenant = l.etat_cache();
        stable = if open_descriptors(f) == 0 && maintenant == avant { stable + 1 } else { 0 };
        avant = maintenant;
    }
    assert_eq!(open_descriptors(f), 0);
}

fn taille(r: &ReponsePage) -> (u32, u32) {
    assert_eq!(r.statut, 200, "{}", String::from_utf8_lossy(&r.octets));
    let img = image::load_from_memory(&r.octets).unwrap();
    (img.width(), img.height())
}

/// Bibliothèque autonome contenant un livre (fiche donnée) ; renvoie (biblio, lecteur).
fn autonome_avec(data: &Path, livre: LibraryBook) -> (Biblio, Lecteur) {
    let b = Biblio::nouvelle(data.to_path_buf());
    b.configurer(&ChoixPremierDemarrage { mode: Mode::Autonome, chemin_ini: None }).unwrap();
    let cfg = b.store_autonome().unwrap();
    Store::open_existing(&cfg).unwrap().upsert(&[livre]).unwrap();
    b.ouvrir().unwrap();
    let l = Lecteur::nouveau(data.to_path_buf(), b.clone());
    (b, l)
}

/// Fiche : image 3 en tête, image 1 supprimée, image 2 tournée de 90°.
fn fiche(id: &str, fichier: &Path) -> LibraryBook {
    let mut liste = pages::full_list(&[], 4);
    pages::move_to_start(&mut liste, &[3]);
    pages::set_type(&mut liste, &[1], "Deleted");
    pages::set_rotation(&mut liste, &[2], "Rotate90");
    LibraryBook {
        id: id.into(),
        file: Some(fichier.to_string_lossy().into()),
        series: Some("Thanos".into()),
        number: Some("1".into()),
        page_count: Some(4),
        pages: PageList { page: liste },
        ..Default::default()
    }
}

#[test]
fn uri_et_url_de_base() {
    assert_eq!(analyser_uri("/abc123/4", Some("l=1200")), Some(("abc123".into(), 4, Some(1200))));
    assert_eq!(analyser_uri("/abc123/0", None), Some(("abc123".into(), 0, None)));
    assert_eq!(analyser_uri("abc/2/", Some("x=1&l=99999")), Some(("abc".into(), 2, Some(16384))));
    assert_eq!(analyser_uri("/abc/2", Some("l=0")), Some(("abc".into(), 2, None)));
    assert_eq!(analyser_uri("/abc/2", Some("l=850.4")), Some(("abc".into(), 2, Some(850))));
    assert_eq!(analyser_uri("/abc", None), None);
    assert_eq!(analyser_uri("/abc/x", None), None);
    assert_eq!(analyser_uri("/a.b/1", None), None, "jeton alphanumérique seulement");
    assert_eq!(analyser_uri("/abc/1/2", None), None);
    let base = url_base("j1");
    if cfg!(windows) {
        assert_eq!(base, "http://cr-page.localhost/j1");
    } else {
        assert_eq!(base, "cr-page://localhost/j1");
    }
}

#[test]
fn cbz_hors_bibliotheque_pages_brutes_et_reduites() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("Hors biblio.cbz");
    cbz_tailles(&f, 3);
    let l = Lecteur::nouveau(d.path().join("data"), Biblio::nouvelle(d.path().join("data")));
    let o = l.ouvrir_fichier(&format!("\"{}\"", f.display())).unwrap();
    assert_eq!(o.id, None);
    assert_eq!(o.titre, "Hors biblio");
    assert_eq!(o.nb_pages, 3);
    assert_eq!(o.page_reprise, 0);
    assert_eq!(o.format, "eComic (ZIP)");
    assert_eq!(o.url_base, url_base(&o.jeton));
    assert_eq!(
        o.pages.iter().map(|p| (p.index, p.image, p.supprimee)).collect::<Vec<_>>(),
        [(0, 0, false), (1, 1, false), (2, 2, false)]
    );
    // Taille d'origine : PNG servi tel quel (aucun réencodage).
    let r = servir(&l, &o.jeton, 1, None);
    assert_eq!(r.mime, "image/png");
    assert_eq!(taille(&r), (110, 300));
    // Largeur maximale : réduite, JPEG (opaque).
    let r = servir(&l, &o.jeton, 2, Some(60));
    assert_eq!(r.mime, "image/jpeg");
    assert_eq!(taille(&r), (60, 150));
    // Plus large que l'image : servie telle quelle.
    assert_eq!(servir(&l, &o.jeton, 0, Some(500)).mime, "image/png");
    // Le fichier n'est pas gardé ouvert entre deux lectures (préchargement fini).
    #[cfg(target_os = "linux")]
    attendre_aucun_descripteur(&l, &f);
    // Index hors du livre, puis jeton inconnu après fermeture : 404.
    assert_eq!(servir(&l, &o.jeton, 3, None).statut, 404);
    assert!(l.url_page_base(&o.jeton).is_ok());
    l.fermer_livre(&o.jeton).unwrap();
    assert_eq!(servir(&l, &o.jeton, 0, None).statut, 404);
    assert!(l.url_page_base(&o.jeton).is_err());
    assert!(l.enregistrer_avancement(&o.jeton, 1, false).is_err());
    l.fermer_livre(&o.jeton).unwrap(); // deux fois : sans effet
    assert_eq!(l.etat_cache(), (0, 0), "pages du livre fermé retirées du cache");
    assert!(l.ouvrir_fichier(&d.path().join("absent.cbz").to_string_lossy()).is_err());
}

#[test]
fn jeton_inconnu_404() {
    let d = tempfile::tempdir().unwrap();
    let l = Lecteur::nouveau(d.path().to_path_buf(), Biblio::nouvelle(d.path().to_path_buf()));
    let r = servir(&l, "inconnu", 0, None);
    assert_eq!(r.statut, 404);
    assert!(String::from_utf8_lossy(&r.octets).contains("jeton inconnu"));
    assert!(l.ouvrir_livre("x").is_err(), "aucune bibliothèque ouverte");
}

#[test]
fn ordre_suppression_et_rotation_de_la_fiche() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("Thanos 01.cbz");
    cbz_tailles(&f, 4);
    let (_b, l) = autonome_avec(&d.path().join("data"), fiche("livre-1", &f));
    let o = l.ouvrir_livre("livre-1").unwrap();
    assert_eq!(o.id.as_deref(), Some("livre-1"));
    assert_eq!(o.titre, "Thanos #1");
    // Liste complète dans l'ordre de la fiche : image 3 d'abord, image 1 supprimée, image 2 à
    // tourner de 90° (rotation laissée au front).
    assert_eq!(
        o.pages.iter().map(|p| (p.index, p.image, p.rotation, p.supprimee)).collect::<Vec<_>>(),
        [(0, 3, 0, false), (1, 0, 0, false), (2, 1, 0, true), (3, 2, 90, false)]
    );
    assert_eq!(o.nb_pages, 4);
    assert_eq!(taille(&servir(&l, &o.jeton, 0, None)), (130, 300), "image 3");
    assert_eq!(taille(&servir(&l, &o.jeton, 1, None)), (100, 300), "image 0");
    assert_eq!(taille(&servir(&l, &o.jeton, 2, None)), (110, 300), "page supprimée : servie quand même");
    let r = servir(&l, &o.jeton, 3, None);
    assert_eq!(r.mime, "image/png", "non tournée : octets d'origine");
    assert_eq!(taille(&r), (120, 300), "image 2");
    // Même fichier ouvert « hors bibliothèque » : la fiche est retrouvée par son chemin.
    let o2 = l.ouvrir_fichier(&f.to_string_lossy()).unwrap();
    assert_eq!(o2.id.as_deref(), Some("livre-1"));
    assert_eq!(o2.pages.len(), 4);
}

#[test]
fn ordre_du_comicinfo_sans_fiche() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("ci.cbz");
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&f).unwrap());
        w.start_file("ComicInfo.xml", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(
            br#"<ComicInfo><Manga>YesAndRightToLeft</Manga><Pages><Page Image="1" Type="FrontCover" ImageWidth="110" ImageHeight="300"/><Page Image="0" Type="Deleted"/><Page Image="2" DoublePage="true"/></Pages></ComicInfo>"#,
        )
        .unwrap();
        for i in 0..3u32 {
            w.start_file(format!("{i}.png"), zip::write::SimpleFileOptions::default()).unwrap();
            let mut png = Vec::new();
            image::RgbaImage::from_pixel(100 + 10 * i, 300, image::Rgba([1, 2, 3, 255]))
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            w.write_all(&png).unwrap();
        }
        w.finish().unwrap();
    }
    let l = Lecteur::nouveau(d.path().join("data"), Biblio::nouvelle(d.path().join("data")));
    let o = l.ouvrir_fichier(&f.to_string_lossy()).unwrap();
    assert!(o.manga);
    assert_eq!(o.pages.iter().map(|p| (p.image, p.supprimee)).collect::<Vec<_>>(), [(1, false), (0, true), (2, false)]);
    assert_eq!(o.pages[0].type_page.as_deref(), Some("FrontCover"));
    assert_eq!((o.pages[0].largeur, o.pages[0].hauteur, o.pages[0].double), (Some(110), Some(300), false));
    assert_eq!((o.pages[2].largeur, o.pages[2].double), (None, true));
    assert_eq!(o.pages[2].type_page, None);
}

#[test]
fn avancement_autonome_ecrit_dans_la_base_une_fois_par_seance() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let f = d.path().join("Thanos 01.cbz");
    cbz_tailles(&f, 4);
    let (b, l) = autonome_avec(&data, fiche("livre-1", &f));
    let o = l.ouvrir_livre("livre-1").unwrap();
    assert_eq!(o.page_reprise, 0);
    l.enregistrer_avancement(&o.jeton, 1, false).unwrap();
    // Mémoire locale tout de suite (numéros de page du livre) ; base pas encore.
    assert_eq!(crate::avancement::Memoire::nouvelle(data.clone()).lire(&f).map(|a| (a.page, a.page_max)), Some((1, 1)));
    let cfg = b.store_autonome().unwrap();
    let relire = || Store::open_existing(&cfg).unwrap().load(None).unwrap().get("livre-1").cloned().unwrap();
    assert_eq!(relire().current_page.unwrap_or(0), 0);
    l.enregistrer_avancement(&o.jeton, 99, false).unwrap(); // borné à la dernière page
    l.enregistrer_avancement(&o.jeton, 1, false).unwrap();
    l.fermer_livre(&o.jeton).unwrap();
    // Page courante 1 ; plus loin 3 (99 borné à la dernière page).
    let livre = relire();
    assert_eq!((livre.current_page, livre.last_page_read), (Some(1), Some(3)));
    assert!(livre.opened.is_some());
    assert_eq!(livre.read_percent(), Some(100));
    // Bibliothèque affichée à jour sans rechargement.
    let det = b.detail("livre-1").unwrap();
    assert_eq!(det.page_courante, Some(3));
    assert!(det.derniere_lecture.is_some());
    // Réouverture : reprise au rang 1.
    let o = l.ouvrir_livre("livre-1").unwrap();
    assert_eq!(o.page_reprise, 1);
    assert_eq!(o.page_reprise_comicrack, None, "base et mémoire locale d'accord");
    // « Terminé » : écrit tout de suite, une seule fois.
    l.enregistrer_avancement(&o.jeton, 0, true).unwrap();
    assert_eq!(relire().current_page, Some(0));
    l.enregistrer_avancement(&o.jeton, 2, false).unwrap();
    l.fermer_livre(&o.jeton).unwrap();
    assert_eq!(relire().current_page, Some(0), "déjà enregistrée pour cette séance");
    assert_eq!(relire().last_page_read, Some(3));
}

#[test]
fn mode_comicrack_lecture_seule_memoire_locale() {
    let d = tempfile::tempdir().unwrap();
    let data = d.path().join("data");
    let f = d.path().join("Thanos 01.cbz");
    cbz_tailles(&f, 4);
    let mut livre = fiche("cr-1", &f);
    livre.current_page = Some(2); // ComicRack s'est arrêté sur la page supprimée → reprise à la 3
    let b = Biblio::nouvelle(data.clone());
    b.installer(Mode::Comicrack, ComicDatabase::from_books(vec![livre]), None);
    let l = Lecteur::nouveau(data.clone(), b.clone());
    let o = l.ouvrir_livre("cr-1").unwrap();
    assert_eq!((o.page_reprise, o.page_reprise_comicrack), (3, None), "page de ComicRack");
    l.enregistrer_avancement(&o.jeton, 1, false).unwrap();
    l.fermer_livre(&o.jeton).unwrap();
    // Rien d'écrit dans ComicRack : la fiche affichée est inchangée.
    assert_eq!(b.livre("cr-1").unwrap().current_page, Some(2));
    assert!(b.livre("cr-1").unwrap().opened.is_none());
    // Mémoire locale : page de ComicRust, différente de celle de ComicRack → les deux.
    let o = l.ouvrir_livre("cr-1").unwrap();
    assert_eq!((o.page_reprise, o.page_reprise_comicrack), (1, Some(3)));
    l.fermer_livre(&o.jeton).unwrap();
}

#[test]
fn cache_borne() {
    let page = |n: usize| Arc::new(PageServie { mime: "image/jpeg", octets: vec![0; n] });
    let cle = |i: usize| ("j".to_string(), i, None);
    let mut c = CachePages::nouveau(250, 3);
    c.inserer(cle(0), page(100));
    c.inserer(cle(1), page(100));
    assert!(c.prendre(&cle(0)).is_some()); // 0 devient la plus récente
    c.inserer(cle(2), page(100)); // 300 > 250 : la moins récente (1) part
    assert!(!c.contient(&cle(1)) && c.contient(&cle(0)) && c.contient(&cle(2)));
    assert_eq!(c.octets(), 200);
    c.inserer(cle(3), page(10));
    c.inserer(cle(4), page(10)); // 4 entrées > 3
    assert_eq!(c.len(), 3);
    c.inserer(cle(5), page(1000)); // seule au-delà du plafond : gardée seule
    assert_eq!((c.len(), c.octets()), (1, 1000));
    c.inserer(cle(5), page(20)); // remplacée
    assert_eq!((c.len(), c.octets()), (1, 20));
    c.inserer(("autre".into(), 0, None), page(5));
    c.purger("j");
    assert_eq!((c.len(), c.octets()), (1, 5));

    // Dans le lecteur : 8 pages servies, préchargement compris, jamais plus de 3 en cache.
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("long.cbz");
    cbz_tailles(&f, 8);
    let b = Biblio::nouvelle(d.path().to_path_buf());
    let l = Lecteur::avec_reglages(d.path().to_path_buf(), b, CachePages::nouveau(usize::MAX, 3), DELAI_PAGE);
    let o = l.ouvrir_fichier(&f.to_string_lossy()).unwrap();
    for i in 0..8 {
        assert_eq!(servir(&l, &o.jeton, i, None).statut, 200);
        assert!(l.etat_cache().1 <= 3);
    }
}

#[test]
fn prechargement_des_pages_suivantes() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("livre.cbz");
    cbz_tailles(&f, 6);
    let l = Lecteur::nouveau(d.path().to_path_buf(), Biblio::nouvelle(d.path().to_path_buf()));
    let o = l.ouvrir_fichier(&f.to_string_lossy()).unwrap();
    // Miniature (bandeau) : aucun préchargement.
    servir(&l, &o.jeton, 3, Some(256));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(l.etat_cache().1, 1, "une miniature ne précharge rien");
    servir(&l, &o.jeton, 1, Some(512));
    // Pages 2, 3, 4 et 0 préparées en arrière-plan (même largeur).
    let attendues = [2usize, 3, 4, 0];
    let debut = Instant::now();
    loop {
        let c = l.cache.lock().unwrap();
        if attendues.iter().all(|&i| c.contient(&(o.jeton.clone(), i, Some(512)))) {
            assert!(!c.contient(&(o.jeton.clone(), 5, Some(512))), "au-delà de ±N : pas préchargée");
            break;
        }
        drop(c);
        assert!(debut.elapsed() < Duration::from_secs(20), "préchargement attendu");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn veilleur_repond_504_apres_le_delai() {
    let attentes: Arc<Attentes> = Arc::default();
    lancer_veilleur(Arc::downgrade(&attentes));
    let (tx, rx) = mpsc::channel();
    let r = Reponse::nouvelle(move |p| tx.send(p.statut).unwrap());
    attentes.lock().unwrap().push((Instant::now() + Duration::from_millis(50), r.clone()));
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(504));
    // Une seule réponse : le fil du livre qui finit plus tard ne répond plus.
    assert!(!r.repondre(ReponsePage::erreur(200, "")));
}

#[test]
fn livre_reecrit_pendant_la_lecture_rouvert() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("livre.cbz");
    cbz_tailles(&f, 3);
    let l = Lecteur::nouveau(d.path().to_path_buf(), Biblio::nouvelle(d.path().to_path_buf()));
    let o = l.ouvrir_fichier(&f.to_string_lossy()).unwrap();
    assert_eq!(servir(&l, &o.jeton, 0, Some(1000)).statut, 200);
    // Infos écrites dans le CBZ (comme ComicRack) : le fichier change.
    std::thread::sleep(Duration::from_millis(1100)); // date de modification différente
    comicrust_core::writer::write_comic_info(&f, r#"<ComicBook Id="x"><Series>S</Series></ComicBook>"#).unwrap();
    assert_eq!(taille(&servir(&l, &o.jeton, 2, Some(999))), (120, 300));
}

#[test]
fn image_seule_ouvre_son_dossier() {
    let d = tempfile::tempdir().unwrap();
    for (i, n) in ["p1.png", "p2.png", "p10.png"].iter().enumerate() {
        image::RgbaImage::from_pixel(50 + i as u32, 80, image::Rgba([0, 0, 0, 255])).save(d.path().join(n)).unwrap();
    }
    let l = Lecteur::nouveau(d.path().join("data"), Biblio::nouvelle(d.path().join("data")));
    let o = l.ouvrir_fichier(&d.path().join("p2.png").to_string_lossy()).unwrap();
    assert_eq!(o.format, "Dossier d'images");
    assert_eq!(o.nb_pages, 3);
    assert_eq!(o.page_reprise, 1, "p2 après p1, avant p10 (ordre naturel)");
    assert_eq!(taille(&servir(&l, &o.jeton, 2, None)), (52, 80));
    // Le dossier lui-même.
    let o = l.ouvrir_fichier(&d.path().to_string_lossy()).unwrap();
    assert_eq!((o.nb_pages, o.page_reprise), (3, 0));
}

#[test]
fn trop_de_livres_ouverts_le_plus_ancien_ferme() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("l.cbz");
    cbz_tailles(&f, 2);
    let l = Lecteur::nouveau(d.path().join("data"), Biblio::nouvelle(d.path().join("data")));
    let jetons: Vec<String> =
        (0..=SESSIONS_MAX).map(|_| l.ouvrir_fichier(&f.to_string_lossy()).unwrap().jeton).collect();
    assert!(l.url_page_base(&jetons[0]).is_err(), "le plus ancien est fermé");
    assert!(jetons[1..].iter().all(|j| l.url_page_base(j).is_ok()));
    l.fermer_tout();
    assert!(jetons.iter().all(|j| l.url_page_base(j).is_err()));
}

#[test]
fn contrat_json_en_camel_case() {
    let o = LivreOuvert {
        jeton: "j".into(),
        id: None,
        titre: "T".into(),
        fichier: "f".into(),
        format: "eComic (ZIP)".into(),
        nb_pages: 1,
        page_reprise: 0,
        page_reprise_comicrack: None,
        manga: false,
        url_base: "u".into(),
        pages: vec![PageInfo {
            index: 0,
            image: 0,
            largeur: None,
            hauteur: None,
            double: false,
            type_page: Some("FrontCover".into()),
            supprimee: false,
            rotation: 0,
        }],
    };
    let v = serde_json::to_value(&o).unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "jeton": "j", "titre": "T", "fichier": "f", "format": "eComic (ZIP)", "nbPages": 1,
            "pageReprise": 0, "manga": false, "urlBase": "u",
            "pages": [{"index": 0, "image": 0, "double": false, "type": "FrontCover",
                       "supprimee": false, "rotation": 0}]
        })
    );
}

#[test]
fn produire_page_tourne_et_reduit() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("l.cbz");
    cbz_tailles(&f, 3);
    let mut livre = comicrust_core::open_book(&f).unwrap();
    let p = produire_page(livre.as_mut(), 2, 90, None).unwrap();
    assert_eq!(p.mime, "image/jpeg");
    assert_eq!(image::load_from_memory(&p.octets).unwrap().dimensions(), (300, 120));
    let p = produire_page(livre.as_mut(), 2, 0, Some(60)).unwrap();
    assert_eq!(image::load_from_memory(&p.octets).unwrap().dimensions(), (60, 150));
    // Transparence gardée : PNG.
    let img = image::RgbaImage::from_pixel(40, 20, image::Rgba([0, 0, 0, 0]));
    assert_eq!(encoder(img, Some(20)).unwrap().mime, "image/png");
    assert!(produire_page(livre.as_mut(), 9, 0, None).is_err());
}

/// L'état géré par Tauri (`app.rs`, compilé seulement sous Windows en CI) doit être partageable.
#[test]
fn etat_tauri_partageable() {
    fn partageable<T: Send + Sync + Clone + 'static>() {}
    partageable::<Lecteur>();
    partageable::<Biblio>();
}

/// Recette tauri-v0.4.0 (os error 5 sous Windows) : livre affiché dans le lecteur **et** dans
/// l'onglet Pages ; avant l'écriture, le fichier est libéré (aucun descripteur, lectures
/// suspendues), écrit, puis les séances le rechargent.
#[test]
fn fichier_libere_pendant_l_ecriture_puis_recharge() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("Libere.cbz");
    cbz_tailles(&f, 4);
    let l = Lecteur::nouveau(d.path().join("data"), Biblio::nouvelle(d.path().join("data")));
    let o = l.ouvrir_fichier(&f.to_string_lossy()).unwrap();
    let v = l.ouvrir_vignettes(&f).unwrap();
    assert_eq!(v.noms.len(), 4);
    assert_eq!(v.url_base, url_base(&v.jeton));
    for i in 0..4 {
        assert_eq!(servir(&l, &v.jeton, i, Some(192)).statut, 200, "vignette {i}");
    }
    assert_eq!(servir(&l, &o.jeton, 1, Some(1000)).statut, 200);
    #[cfg(target_os = "linux")]
    attendre_aucun_descripteur(&l, &f);

    let garde = l.liberer(std::slice::from_ref(&f));
    assert!(crate::liberation::en_ecriture(&f));
    assert_eq!(l.etat_cache().1, 0, "pages du fichier oubliées");
    #[cfg(target_os = "linux")]
    assert_eq!(comicrust_core::providers::on_demand::open_descriptors(&f), 0);
    // Une page demandée pendant l'écriture attend (le fichier n'est pas ouvert).
    let (tx, rx) = mpsc::channel();
    l.servir(&o.jeton, 3, Some(1000), move |r| tx.send(r).unwrap());
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "lecture suspendue pendant l'écriture");
    #[cfg(target_os = "linux")]
    assert_eq!(comicrust_core::providers::on_demand::open_descriptors(&f), 0);
    let ecrit =
        comicrust_core::writer::write_comic_info(&f, r#"<ComicBook Id="x"><Series>Libéré</Series></ComicBook>"#)
            .unwrap();
    assert_eq!(ecrit, comicrust_core::writer::Outcome::Written);
    drop(garde);
    // Écriture finie : la page attendue arrive, lue dans le fichier réécrit.
    let r = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert_eq!(taille(&r), (130, 300));
    assert_eq!(taille(&servir(&l, &v.jeton, 2, Some(192))).0, 120, "plus étroite que 192 : taille d'origine");
    assert!(!crate::liberation::en_ecriture(&f));
    #[cfg(target_os = "linux")]
    attendre_aucun_descripteur(&l, &f);
    // La séance de vignettes n'écrit aucun avancement.
    l.fermer_livre(&v.jeton).unwrap();
    l.fermer_livre(&o.jeton).unwrap();
}

/// Revue étape 4 : une lecture commencée juste après la réservation du fichier (préchargement
/// ou page demandée entre `reserver` et l'envoi de `Liberer`) ne doit pas faire attendre
/// l'écriture jusqu'à `DELAI_LIBERATION` : le fil accuse réception pendant qu'il attend.
#[test]
fn lecture_en_attente_n_interbloque_pas_la_liberation() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("Course.cbz");
    cbz_tailles(&f, 4);
    let l = Lecteur::nouveau(d.path().join("data"), Biblio::nouvelle(d.path().join("data")));
    let o = l.ouvrir_fichier(&f.to_string_lossy()).unwrap();
    // Réservation seule (comme au tout début de `liberer`), puis une page demandée : le fil
    // se met à attendre la fin de l'écriture.
    let reservation = crate::liberation::reserver([f.as_path()]);
    let (tx, rx) = mpsc::channel();
    l.servir(&o.jeton, 2, Some(1000), move |r| tx.send(r).unwrap());
    std::thread::sleep(Duration::from_millis(200));
    let debut = Instant::now();
    let garde = l.liberer(std::slice::from_ref(&f));
    assert!(debut.elapsed() < Duration::from_secs(3), "libération bloquée {:?}", debut.elapsed());
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "lecture toujours suspendue");
    drop(reservation);
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "suspendue tant que la garde vit");
    drop(garde);
    let r = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert_eq!(taille(&r), (120, 300));
    l.fermer_livre(&o.jeton).unwrap();
}
