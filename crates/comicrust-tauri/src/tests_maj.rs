//! Tests du module `maj` (copiés de la version egui et adaptés à la version Tauri).

use super::*;
use std::io::Write;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[test]
fn versions() {
    assert_eq!(parse_version("tauri-v0.3.1"), Some((0, 3, 1)));
    assert_eq!(parse_version("v0.3.1"), Some((0, 3, 1)));
    assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
    assert!(is_newer("tauri-v0.10.0", "tauri-v0.9.9"));
    assert!(is_newer("tauri-v0.10.0", "0.9.9"));
    assert!(is_newer("tauri-v0.8.0-beta", "0.7.3"));
    assert!(!is_newer("tauri-v0.7.3", "0.7.3"));
    assert!(!is_newer("nightly", "0.2.1"));
    assert!(meme_version("tauri-v0.8.0", "0.8.0") && !meme_version("tauri-v0.8.1", "tauri-v0.8.0"));
}

/// Réponse de l'API : releases egui (`vX.Y.Z`, plus récentes en numéro) et Tauri mêlées.
const JSON: &str = r####"[
  {"tag_name":"v0.10.7","name":"ComicRust v0.10.7","draft":false,"prerelease":true,"html_url":"h107",
   "assets":[{"name":"ComicRust-v0.10.7-windows-x64.zip","browser_download_url":"https://x/egui.zip","size":9},
             {"name":"ComicRust-v0.10.7-windows-x64.zip.sha256","browser_download_url":"https://x/egui.sha256","size":1}]},
  {"tag_name":"tauri-v0.9.0","name":"brouillon","draft":true,"html_url":"h9","assets":[]},
  {"tag_name":"tauri-v0.8.0","name":"ComicRust-Tauri tauri-v0.8.0","body":"### Nouveautés\n- mise à jour","draft":false,
   "prerelease":true,"html_url":"https://github.com/imtheyoyo/ComicRust/releases/tag/tauri-v0.8.0",
   "assets":[{"name":"ComicRust-v0.8.0-windows-x64.zip","browser_download_url":"https://x/mauvais.zip","size":1},
             {"name":"ComicRust-Tauri-tauri-v0.8.0-windows-x64.zip","browser_download_url":"https://x/t.zip","size":123},
             {"name":"ComicRust-Tauri-tauri-v0.8.0-windows-x64.zip.sha256","browser_download_url":"https://x/t.sha256","size":100},
             {"name":"tauri-v0.8.0.md","browser_download_url":"https://x/r.md","size":1}]},
  {"tag_name":"v0.9.9","draft":false,"html_url":"h99","assets":[]},
  {"tag_name":"tauri-v0.7.3","name":"","draft":false,"html_url":"h73","assets":[]}
]"####;

#[test]
fn choisit_la_release_tauri_et_son_zip() {
    let r = pick_release(JSON, "0.7.3").unwrap().unwrap();
    assert_eq!(r.tag, "tauri-v0.8.0");
    assert_eq!(r.zip, Some(("https://x/t.zip".into(), 123)));
    assert_eq!(r.sha256_url.as_deref(), Some("https://x/t.sha256"));
    assert!(r.notes.contains("Nouveautés"));
    let i = r.infos();
    assert!(i.a_paquet && i.taille == 123);
    // v0.10.7 (egui) ignorée : rien de plus récent que 0.8.0 ; brouillon tauri-v0.9.0 ignoré.
    assert_eq!(pick_release(JSON, "0.8.0").unwrap(), None);
    // Nom vide : le tag sert de nom ; sans paquet, pas d'installation possible.
    let r = pick_release(JSON, "0.7.0").unwrap().unwrap();
    assert_eq!(r.tag, "tauri-v0.8.0");
    let seule = r#"[{"tag_name":"tauri-v0.7.3","name":"","draft":false,"html_url":"h","assets":[]}]"#;
    let r = pick_release(seule, "0.7.0").unwrap().unwrap();
    assert_eq!(r.nom, "tauri-v0.7.3");
    assert!(!r.infos().a_paquet);
    assert!(pick_release("pas du json", "0.7.0").is_err());
}

#[test]
fn format_des_fichiers_sha256() {
    let h = "72e0451f6e416c76a589ee96b752d5dfb3d0f661258fa8296a987736efe006f8";
    let ligne = format!("{h}  ComicRust-Tauri-tauri-v0.8.0-windows-x64.zip\r\n");
    assert_eq!(parse_sha256_file(&ligne).as_deref(), Some(h));
    assert_eq!(parse_sha256_file(&h.to_uppercase()).as_deref(), Some(h));
    assert_eq!(parse_sha256_file("pas une empreinte"), None);
    assert_eq!(parse_sha256_file(""), None);
}

#[test]
fn empreinte_d_un_fichier() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("f");
    std::fs::write(&p, b"abc").unwrap();
    assert_eq!(sha256_of(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

/// Zip d'une « version » : fichiers (chemin, contenu).
fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default();
    for (name, data) in files {
        w.start_file(*name, o).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// Paquet Tauri complet (comme `release-tauri.yml`), avec des entrées à refuser.
fn paquet(version: &str) -> Vec<u8> {
    zip_of(&[
        ("ComicRust-Tauri.exe", format!("exe {version}").as_bytes()),
        ("ComicRust-Tauri-mcp.exe", format!("mcp {version}").as_bytes()),
        ("pdfium.dll", b"dll"),
        ("README.md", b"lisez-moi"),
        ("MCP-tauri.md", b"mcp"),
        ("RECETTE.md", b"recette"),
        ("licences/PDFium-LICENSE.txt", b"lic"),
        ("data/comicrust-tauri.json", b"ne doit pas ecraser"),
        ("data/webview2/Default/Cookies", b"ne doit pas ecraser"),
        ("../evasion.txt", b"refuse"),
    ])
}

/// Dossier d'une installation existante (ancienne version, réglages, profil WebView2).
fn installation(racine: &Path) -> PathBuf {
    let app = racine.join("ComicRust-Tauri");
    for (f, contenu) in [
        ("ComicRust-Tauri.exe", &b"exe ancien"[..]),
        ("ComicRust-Tauri-mcp.exe", b"mcp ancien"),
        ("pdfium.dll", b"dll ancienne"),
        ("data/comicrust-tauri.json", b"{\"mode\":\"autonome\"}"),
        ("data/webview2/Default/Cookies", b"profil"),
        ("data/themes/perso.css", b"theme"),
    ] {
        let p = app.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contenu).unwrap();
    }
    app
}

fn data_intact(app: &Path) {
    assert_eq!(std::fs::read(app.join("data/comicrust-tauri.json")).unwrap(), b"{\"mode\":\"autonome\"}");
    assert_eq!(std::fs::read(app.join("data/webview2/Default/Cookies")).unwrap(), b"profil");
    assert_eq!(std::fs::read(app.join("data/themes/perso.css")).unwrap(), b"theme");
}

#[test]
fn extraction_et_remplacement_gardent_data_et_renomment_en_old() {
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    std::fs::write(app.join("mes-notes.old"), b"a l'utilisateur").unwrap();
    let zip_path = tmp.path().join("maj.zip");
    std::fs::write(&zip_path, paquet("2")).unwrap();

    let extrait = tmp.path().join("extrait");
    let files = extract(&zip_path, &extrait).unwrap();
    assert!(!files.iter().any(|f| f.to_string_lossy().contains("evasion")));
    assert!(!tmp.path().join("evasion.txt").exists());
    assert!(contient_exe_principal(&files));

    let bilan = apply(&extrait, &files, &app).unwrap();
    // 7 fichiers hors data\ ; les deux exe et la dll existaient : 3 `.old`.
    assert_eq!(bilan, Bilan { remplaces: 7, restes_old: 3, ..Default::default() });
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri.exe")).unwrap(), b"exe 2");
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri.exe.old")).unwrap(), b"exe ancien");
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri-mcp.exe")).unwrap(), b"mcp 2");
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri-mcp.exe.old")).unwrap(), b"mcp ancien");
    assert_eq!(std::fs::read(app.join("pdfium.dll.old")).unwrap(), b"dll ancienne");
    assert!(app.join("licences/PDFium-LICENSE.txt").exists() && app.join("RECETTE.md").exists());
    data_intact(&app);

    // Deuxième mise à jour avant redémarrage : l'ancien `.old` (peut-être verrouillé) est gardé,
    // le nouveau prend un nom libre.
    let bilan = apply(&extrait, &files, &app).unwrap();
    assert_eq!(bilan.restes_old, 7);
    assert!(app.join("ComicRust-Tauri.exe.1.old").exists());

    // Lancement suivant : les `.old` de la mise à jour disparaissent, pas celui de l'utilisateur.
    cleanup(&app);
    assert!(!app.join("ComicRust-Tauri.exe.old").exists() && !app.join("ComicRust-Tauri.exe.1.old").exists());
    assert!(!app.join("pdfium.dll.old").exists() && !app.join("README.md.old").exists());
    assert_eq!(std::fs::read(app.join("mes-notes.old")).unwrap(), b"a l'utilisateur");
    assert!(app.join("ComicRust-Tauri.exe").exists());
    assert!(!app.join("data").join(DOSSIER_MAJ).exists());
    data_intact(&app);
}

/// Un fichier qui ne peut pas être écrit : erreur claire, l'ancien reprend sa place, les `.old`
/// déjà créés restent notés.
#[test]
fn echec_d_ecriture_remet_l_ancien_fichier() {
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    let extrait = tmp.path().join("extrait");
    std::fs::create_dir_all(&extrait).unwrap();
    std::fs::write(extrait.join("pdfium.dll"), b"dll 2").unwrap();
    // `ComicRust-Tauri.exe` annoncé mais absent de l'extraction : la copie échoue.
    let files = [PathBuf::from("pdfium.dll"), PathBuf::from("ComicRust-Tauri.exe")];
    let e = apply(&extrait, &files, &app).unwrap_err();
    assert!(e.contains("ComicRust-Tauri.exe"), "{e}");
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri.exe")).unwrap(), b"exe ancien");
    assert!(!app.join("ComicRust-Tauri.exe.old").exists());
    let liste = std::fs::read_to_string(app.join("data/maj/anciens.txt")).unwrap();
    assert_eq!(liste.trim(), "pdfium.dll.old");
}

#[test]
fn nettoyage_ne_supprime_que_les_old_listes() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("app");
    for f in ["ComicRust-Tauri.exe.old", "licences/x.txt.old", "a-moi.old", "data/r.old", "data/maj/x.part"] {
        let p = app.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }
    std::fs::write(tmp.path().join("dehors.old"), b"x").unwrap();
    std::fs::write(
        app.join("data/maj/anciens.txt"),
        "ComicRust-Tauri.exe.old\nlicences/x.txt.old\n../dehors.old\ndata/r.old\npas-un-old.txt\n\n",
    )
    .unwrap();
    cleanup(&app);
    assert!(!app.join("ComicRust-Tauri.exe.old").exists() && !app.join("licences/x.txt.old").exists());
    assert!(app.join("a-moi.old").exists() && app.join("data/r.old").exists());
    assert!(tmp.path().join("dehors.old").exists());
    assert!(!app.join("data/maj").exists());
    // Sans liste : rien d'autre que `data\maj`.
    cleanup(&app);
    assert!(app.join("a-moi.old").exists());
}

#[test]
fn arguments_de_relance() {
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(arguments_relance(a(&["--mcp=47803"]), 42), a(&["--mcp=47803", "--attendre-pid", "42"]));
    // Un ancien `--attendre-pid` (deux formes) est remplacé, pas cumulé.
    let r = arguments_relance(a(&["--attendre-pid", "7", "--mcp=47803", "--attendre-pid=8"]), 42);
    assert_eq!(r, a(&["--mcp=47803", "--attendre-pid", "42"]));
    // Côté nouvelle instance : retiré, le reste conservé dans l'ordre.
    let (pid, reste) = retirer_attendre_pid(a(&["--mcp=47803", "--attendre-pid", "42", "--mcp-token=t"]));
    assert_eq!(pid, Some(42));
    assert_eq!(reste, a(&["--mcp=47803", "--mcp-token=t"]));
    assert_eq!(retirer_attendre_pid(a(&["--attendre-pid", "x"])), (None, vec![]));
    assert_eq!(retirer_attendre_pid(a(&[])), (None, vec![]));
    // L'exe renommé en `.old` n'est jamais relancé.
    assert_eq!(exe_a_relancer(Path::new("C:/CR/ComicRust-Tauri.exe.old")), Path::new("C:/CR/ComicRust-Tauri.exe"));
    assert_eq!(exe_a_relancer(Path::new("C:/CR/ComicRust-Tauri.exe")), Path::new("C:/CR/ComicRust-Tauri.exe"));
    // Processus déjà terminé (ou inconnu) : pas d'attente.
    let debut = Instant::now();
    assert!(attendre_fin_processus(u32::MAX - 1, Duration::from_secs(5)));
    assert!(debut.elapsed() < Duration::from_secs(2));
}

#[test]
fn demande_de_reglage() {
    use serde_json::json;
    let d = analyser_demande_reglage(&json!({})).unwrap();
    assert_eq!(d, DemandeReglage::default());
    let d = analyser_demande_reglage(&json!({"verifierAuDemarrage": false})).unwrap();
    assert_eq!(d.verifier_au_demarrage, Some(false));
    assert_eq!(d.version_ignoree, None);
    let d = analyser_demande_reglage(&json!({"versionIgnoree": "tauri-v0.8.0"})).unwrap();
    assert_eq!(d.version_ignoree, Some(Some("tauri-v0.8.0".into())));
    let d = analyser_demande_reglage(&json!({"versionIgnoree": null})).unwrap();
    assert_eq!(d.version_ignoree, Some(None));
    assert!(analyser_demande_reglage(&json!({"versionIgnoree": 3})).is_err());
    assert!(analyser_demande_reglage(&json!({"verifierAuDemarrage": "oui"})).is_err());
}

#[test]
fn contrat_json_du_statut() {
    let s = StatutMaj {
        etat: EtatStatut::Installee,
        manuel: true,
        release: pick_release(JSON, "0.7.3").unwrap().map(|r| r.infos()),
        bilan: Some(Bilan { remplaces: 7, restes_old: 3, ..Default::default() }),
        ..Default::default()
    };
    let (nom, v) = EvenementMaj::Statut(s).pour_le_front();
    assert_eq!(nom, "maj://statut");
    assert_eq!(v["etat"], "installee");
    assert_eq!(v["release"]["urlPage"], "https://github.com/imtheyoyo/ComicRust/releases/tag/tauri-v0.8.0");
    assert_eq!(v["release"]["aPaquet"], true);
    assert_eq!(v["bilan"]["restesOld"], 3);
    assert!(v.get("erreur").is_none());
    let (_, v) = EvenementMaj::Statut(StatutMaj::simple(EtatStatut::AJour, true)).pour_le_front();
    assert_eq!(v, serde_json::json!({"etat": "aJour", "manuel": true}));
    let (nom, v) = EvenementMaj::Progression { recu: 5, total: 10 }.pour_le_front();
    assert_eq!((nom, v), ("maj://progression", serde_json::json!({"recu": 5, "total": 10})));
}

// ─────────────────────────── Faux serveur HTTP local ───────────────────────────

struct Reponse {
    statut: u16,
    entetes: Vec<(&'static str, String)>,
    corps: Vec<u8>,
}

fn ok(corps: impl Into<Vec<u8>>) -> Reponse {
    Reponse { statut: 200, entetes: vec![], corps: corps.into() }
}

/// Petit serveur HTTP (une connexion à la fois, `Connection: close`) : `routes(chemin, port)`.
/// Renvoie le port ; le fil vit jusqu'à la fin des tests.
fn serveur(routes: impl Fn(&str, u16) -> Option<Reponse> + Send + 'static) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for flux in l.incoming() {
            let Ok(mut flux) = flux else { continue };
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match flux.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => req.extend_from_slice(&buf[..n]),
                }
            }
            let texte = String::from_utf8_lossy(&req);
            let chemin = texte.split_whitespace().nth(1).unwrap_or("/").to_string();
            let r =
                routes(&chemin, port).unwrap_or(Reponse { statut: 404, entetes: vec![], corps: b"absent".to_vec() });
            let mut tete =
                format!("HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n", r.statut, r.corps.len());
            for (k, v) in &r.entetes {
                tete.push_str(&format!("{k}: {v}\r\n"));
            }
            tete.push_str("\r\n");
            let _ = flux.write_all(tete.as_bytes());
            let _ = flux.write_all(&r.corps);
        }
    });
    port
}

fn sha_hex(octets: &[u8]) -> String {
    Sha256::digest(octets).iter().map(|b| format!("{b:02x}")).collect()
}

/// Serveur d'une release `tag` (egui plus récente et brouillon mêlés) ; le zip passe par une
/// 302 comme sur GitHub. `empreinte` : contenu du `.sha256`.
fn serveur_release(tag: &'static str, zip: Vec<u8>, empreinte: String) -> u16 {
    serveur(move |chemin, port| {
        let base = format!("http://127.0.0.1:{port}");
        let nom = nom_paquet(tag);
        match chemin {
            "/releases" => Some(ok(serde_json::json!([
                {"tag_name": "v100.0.0", "draft": false, "html_url": "egui", "assets": []},
                {"tag_name": "tauri-v99.9.0", "draft": true, "html_url": "brouillon", "assets": []},
                {"tag_name": tag, "name": "", "body": "notes", "draft": false, "prerelease": true,
                 "html_url": format!("{base}/page"),
                 "assets": [
                    {"name": nom, "browser_download_url": format!("{base}/telecharger/zip"), "size": zip.len()},
                    {"name": format!("{nom}.sha256"), "browser_download_url": format!("{base}/fichiers/sha"), "size": 90}
                 ]}
            ])
            .to_string())),
            "/telecharger/zip" => {
                Some(Reponse { statut: 302, entetes: vec![("Location", "/fichiers/zip".into())], corps: vec![] })
            }
            "/fichiers/zip" => Some(ok(zip.clone())),
            "/fichiers/sha" => Some(ok(empreinte.clone())),
            _ => None,
        }
    })
}

fn installer_depuis(port: u16, app: &Path) -> Result<Bilan, String> {
    let rel = chercher(&format!("http://127.0.0.1:{port}/releases"), "0.7.3").unwrap().unwrap();
    installer_release(&rel, app, &app.join("data/maj"), &mut |_| {})
}

/// Empreinte fausse : rien n'est modifié.
#[test]
fn sha256_faux_ne_modifie_rien() {
    let zip = paquet("2");
    let port = serveur_release("tauri-v99.0.0", zip, format!("{}  x.zip", sha_hex(b"autre chose")));
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    let e = installer_depuis(port, &app).unwrap_err();
    assert!(e.contains("SHA-256") && e.contains("rien n'a été modifié"), "{e}");
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri.exe")).unwrap(), b"exe ancien");
    assert!(!app.join("ComicRust-Tauri.exe.old").exists() && !app.join("data/maj/anciens.txt").exists());
    data_intact(&app);
}

/// Paquet sans `ComicRust-Tauri.exe` (ex. un zip de la version egui) : refusé, rien de modifié.
#[test]
fn paquet_sans_exe_tauri_refuse() {
    let zip = zip_of(&[("ComicRust.exe", b"egui"), ("pdfium.dll", b"dll 2"), ("sous/ComicRust-Tauri.exe", b"x")]);
    let empreinte = format!("{}  x.zip\n", sha_hex(&zip));
    let port = serveur_release("tauri-v99.0.0", zip, empreinte);
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    let e = installer_depuis(port, &app).unwrap_err();
    assert!(e.contains("ComicRust-Tauri.exe absent"), "{e}");
    assert_eq!(std::fs::read(app.join("pdfium.dll")).unwrap(), b"dll ancienne");
    assert!(!app.join("ComicRust.exe").exists());
    assert!(!app.join("data/maj/tauri-v99.0.0").exists());
}

fn attendre_statut(maj: &Maj, etats: &[EtatStatut]) -> StatutMaj {
    let debut = Instant::now();
    loop {
        let s = maj.statut();
        if etats.contains(&s.etat) {
            return s;
        }
        assert!(debut.elapsed() < Duration::from_secs(30), "statut attendu {etats:?}, trouvé {s:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

type Journal = Arc<Mutex<Vec<EvenementMaj>>>;

fn sortie() -> (Journal, SortieMaj) {
    let j: Journal = Arc::default();
    let j2 = j.clone();
    (j, Arc::new(move |e| j2.lock().unwrap().push(e)))
}

/// Bout en bout avec le faux serveur (`COMICRUST_MAJ_URL`) : vérification manuelle, téléchargement
/// suivi d'une 302, SHA-256, installation, progression, bilan ; `data\` intact.
#[test]
fn bout_en_bout_avec_un_serveur_local() {
    let zip = paquet("99");
    let empreinte = format!("{}  {}\r\n", sha_hex(&zip), nom_paquet("tauri-v99.0.0"));
    let port = serveur_release("tauri-v99.0.0", zip.clone(), empreinte);
    // Seul ce test touche à ces variables (lues à la création de `Maj`).
    std::env::set_var("COMICRUST_MAJ_URL", format!("http://127.0.0.1:{port}/releases"));
    std::env::set_var("COMICRUST_SIMULER_VERSION", "0.0.1");
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    let (journal, sortie) = sortie();
    let maj = Maj::nouveau(app.join("data"), app.clone(), true, sortie);
    std::env::remove_var("COMICRUST_MAJ_URL");
    std::env::remove_var("COMICRUST_SIMULER_VERSION");

    let etat = maj.etat();
    assert_eq!(etat.version_comparee, "0.0.1");
    assert!(etat.verifier_au_demarrage && etat.installable && etat.version_ignoree.is_none());
    assert!(maj.installer("tauri-v99.0.0").is_err(), "installation sans vérification préalable");

    maj.verifier(true);
    let s = attendre_statut(&maj, &[EtatStatut::Disponible, EtatStatut::Echec]);
    assert_eq!(s.etat, EtatStatut::Disponible, "{s:?}");
    let rel = s.release.unwrap();
    assert_eq!((rel.tag.as_str(), rel.nom.as_str(), rel.taille), ("tauri-v99.0.0", "tauri-v99.0.0", zip.len() as u64));
    assert!(rel.a_paquet && s.manuel);

    maj.installer("tauri-v99.0.0").unwrap();
    let s = attendre_statut(&maj, &[EtatStatut::Installee, EtatStatut::Echec]);
    assert_eq!(s.etat, EtatStatut::Installee, "{s:?}");
    assert_eq!(s.bilan, Some(Bilan { remplaces: 7, restes_old: 3, ..Default::default() }));
    assert!(maj.installer("tauri-v99.0.0").is_err(), "déjà installée");
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri.exe")).unwrap(), b"exe 99");
    assert_eq!(std::fs::read(app.join("ComicRust-Tauri-mcp.exe.old")).unwrap(), b"mcp ancien");
    data_intact(&app);
    // Téléchargements effacés, liste des `.old` gardée pour le prochain démarrage.
    let restes: Vec<_> = std::fs::read_dir(app.join("data/maj")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(restes, ["anciens.txt"]);

    let j = journal.lock().unwrap();
    let etats: Vec<EtatStatut> = j
        .iter()
        .filter_map(|e| match e {
            EvenementMaj::Statut(s) => Some(s.etat),
            _ => None,
        })
        .collect();
    assert_eq!(
        etats,
        [EtatStatut::Verification, EtatStatut::Disponible, EtatStatut::Installation, EtatStatut::Installee]
    );
    let prog: Vec<(u64, u64)> = j
        .iter()
        .filter_map(|e| match e {
            EvenementMaj::Progression { recu, total } => Some((*recu, *total)),
            _ => None,
        })
        .collect();
    assert_eq!(prog.last(), Some(&(zip.len() as u64, zip.len() as u64)));
}

/// Vérification automatique : la version ignorée n'est pas proposée (silence), une vérification
/// manuelle la montre ; « à jour » et les erreurs automatiques restent silencieux.
#[test]
fn verification_automatique_et_version_ignoree() {
    let zip = paquet("99");
    let port = serveur_release("tauri-v99.0.0", zip.clone(), format!("{}  x.zip", sha_hex(&zip)));
    let url = format!("http://127.0.0.1:{port}/releases");
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    let data = app.join("data");
    let (journal, s) = sortie();
    let maj = Maj::nouveau(data.clone(), app.clone(), false, s).avec_url(url.clone());

    let d = DemandeReglage { version_ignoree: Some(Some("tauri-v99.0.0".into())), ..Default::default() };
    let etat = maj.regler(d).unwrap();
    assert_eq!(etat.version_ignoree.as_deref(), Some("tauri-v99.0.0"));
    assert!(!etat.installable);
    // Réglage écrit dans la section `maj` sans perdre le reste du fichier.
    let texte = std::fs::read_to_string(data.join("comicrust-tauri.json")).unwrap();
    assert!(texte.contains("\"versionIgnoree\": \"tauri-v99.0.0\"") && texte.contains("autonome"), "{texte}");

    maj.verifier_au_demarrage();
    std::thread::sleep(Duration::from_millis(50));
    attendre_statut(&maj, &[EtatStatut::Inactif]);
    assert!(journal.lock().unwrap().is_empty(), "vérification automatique silencieuse");

    // Manuelle : toujours montrée ; installation refusée hors Windows / en dev.
    maj.verifier(true);
    let s = attendre_statut(&maj, &[EtatStatut::Disponible]);
    assert!(s.manuel);
    assert!(maj.installer("tauri-v99.0.0").unwrap_err().contains("indisponible"));

    // Version ignorée effacée : l'automatique la propose.
    let maj2 = Maj::nouveau(data.clone(), app.clone(), false, sortie().1).avec_url(url);
    maj2.regler(DemandeReglage { version_ignoree: Some(None), ..Default::default() }).unwrap();
    maj2.verifier(false);
    let s = attendre_statut(&maj2, &[EtatStatut::Disponible, EtatStatut::Inactif]);
    assert_eq!(s.etat, EtatStatut::Disponible);
    assert!(!s.manuel);

    // Vérification au démarrage désactivée : rien n'est lancé.
    let (j3, s3) = sortie();
    let maj3 = Maj::nouveau(data.clone(), app.clone(), false, s3).avec_url("http://127.0.0.1:1/x");
    maj3.regler(DemandeReglage { verifier_au_demarrage: Some(false), ..Default::default() }).unwrap();
    assert!(!maj3.etat().verifier_au_demarrage);
    maj3.verifier_au_demarrage();
    assert_eq!(maj3.statut().etat, EtatStatut::Inactif);
    // Erreur réseau automatique : silencieuse ; manuelle : `echec` avec le message.
    maj3.verifier(false);
    attendre_statut(&maj3, &[EtatStatut::Inactif]);
    std::thread::sleep(Duration::from_millis(100));
    attendre_statut(&maj3, &[EtatStatut::Inactif]);
    assert!(j3.lock().unwrap().is_empty());
    maj3.verifier(true);
    let s = attendre_statut(&maj3, &[EtatStatut::Echec]);
    assert!(s.erreur.unwrap().contains("injoignable"));
}

/// « À jour » : vérification manuelle → `aJour`.
#[test]
fn verification_manuelle_a_jour() {
    let port = serveur(|chemin, _| (chemin == "/releases").then(|| ok(r#"[{"tag_name":"v99.0.0","html_url":"h"}]"#)));
    let tmp = tempfile::tempdir().unwrap();
    let (_, s) = sortie();
    let maj = Maj::nouveau(tmp.path().join("data"), tmp.path().into(), false, s)
        .avec_url(format!("http://127.0.0.1:{port}/releases"));
    maj.verifier(true);
    assert_eq!(attendre_statut(&maj, &[EtatStatut::AJour, EtatStatut::Echec]).etat, EtatStatut::AJour);
}

/// Bout en bout contre GitHub (réseau) : la dernière release `tauri-v*` vue depuis une
/// « 0.0.1 », téléchargée, vérifiée et installée dans un dossier temporaire.
/// `cargo test -p comicrust-tauri -- --ignored maj_reelle`
#[test]
#[ignore]
fn maj_reelle_depuis_github() {
    let rel = chercher(&format!("https://api.github.com/repos/{REPO}/releases?per_page=50"), "0.0.1")
        .unwrap()
        .expect("une release tauri-v* plus récente que 0.0.1");
    assert!(rel.tag.starts_with(PREFIXE_TAG));
    let dir = tempfile::tempdir().unwrap();
    let app = installation(dir.path());
    let mut recu = 0;
    let bilan = installer_release(&rel, &app, &app.join("data/maj"), &mut |n| recu = n).unwrap();
    assert!(std::fs::metadata(app.join(EXE_PRINCIPAL)).unwrap().len() > 1_000_000);
    assert!(app.join("pdfium.dll").exists() && app.join("ComicRust-Tauri-mcp.exe").exists());
    data_intact(&app);
    eprintln!("{} : {bilan:?}, {recu} octets téléchargés", rel.tag);
}

/// Une demande manuelle reçue pendant la vérification automatique s'y rattache : une seule requête,
/// résultat montré (`manuel: true`).
#[test]
fn verification_manuelle_rattachee_a_l_automatique() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = n.clone();
    let port = serveur(move |chemin, _| {
        (chemin == "/releases").then(|| {
            n2.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(300));
            ok(r#"[{"tag_name":"v99.0.0","html_url":"h"}]"#)
        })
    });
    let tmp = tempfile::tempdir().unwrap();
    let (journal, s) = sortie();
    let maj = Maj::nouveau(tmp.path().join("data"), tmp.path().into(), false, s)
        .avec_url(format!("http://127.0.0.1:{port}/releases"));
    maj.verifier(false);
    maj.verifier(true);
    maj.verifier(true);
    let s = attendre_statut(&maj, &[EtatStatut::AJour, EtatStatut::Echec, EtatStatut::Inactif]);
    assert_eq!((s.etat, s.manuel), (EtatStatut::AJour, true));
    assert_eq!(n.load(Ordering::SeqCst), 1);
    let etats: Vec<_> = journal
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            EvenementMaj::Statut(s) => Some((s.etat, s.manuel)),
            _ => None,
        })
        .collect();
    assert_eq!(etats, [(EtatStatut::Verification, true), (EtatStatut::AJour, true)]);
}

#[test]
fn filtre_des_pages_ouvertes() {
    let ok = [
        "https://github.com/imtheyoyo/ComicRust/releases/tag/tauri-v0.8.0",
        "  https://github.com/imtheyoyo/ComicRust/issues/12#issuecomment-1?a=1&b=%20  ",
        "https://github.com/imtheyoyo/ComicRust/",
    ];
    for u in ok {
        assert_eq!(verifier_url_page(u).unwrap(), u.trim());
    }
    let refuses = [
        "http://github.com/imtheyoyo/ComicRust/releases",
        "https://github.com/imtheyoyo/ComicRust",
        "https://github.com/imtheyoyo/ComicRustX/x",
        "https://github.com/imtheyoyo/comicrust/releases",
        "https://github.com.evil.example/imtheyoyo/ComicRust/",
        "https://github.com/autre/ComicRust/",
        "file:///C:/Windows/system32/calc.exe",
        "https://github.com/imtheyoyo/ComicRust/\" & calc",
        "https://github.com/imtheyoyo/ComicRust/a b",
        "https://github.com/imtheyoyo/ComicRust/a\nb",
        "https://github.com/imtheyoyo/ComicRust/a|calc",
        "https://github.com/imtheyoyo/ComicRust/a^b",
        "javascript:alert(1)",
        "",
    ];
    for u in refuses {
        let e = verifier_url_page(u).unwrap_err();
        assert!(e.contains("refusée"), "{u} : {e}");
        assert!(ouvrir_page(u).is_err(), "{u}");
    }
    assert!(verifier_url_page(&format!("{PREFIXE_PAGE_AUTORISEE}{}", "a".repeat(3000))).is_err());
}

#[test]
fn tags_tauri_stricts_et_finale_avant_rc() {
    assert!(tag_tauri_valide("tauri-v0.8.0"));
    assert!(tag_tauri_valide("tauri-v1.10.2-rc1"));
    for t in ["tauri-vV1.0.0", "tauri-v1", "tauri-v1.0", "v0.10.7", "tauri-v1.0.x", "tauri-v.1.0", "Tauri-v1.0.0"] {
        assert!(!tag_tauri_valide(t), "{t}");
    }
    // Tags malformés jamais retenus, même « plus récents ».
    let malformes = r#"[{"tag_name":"tauri-vV9.0.0","name":"","draft":false,"html_url":"h","assets":[]},
        {"tag_name":"tauri-v9","name":"","draft":false,"html_url":"h","assets":[]}]"#;
    assert_eq!(pick_release(malformes, "0.7.0").unwrap(), None);
    // À version égale, la finale l'emporte sur la rc, quel que soit l'ordre de la liste GitHub.
    for json in [
        r#"[{"tag_name":"tauri-v0.9.0-rc1","name":"","draft":false,"html_url":"h","assets":[]},
            {"tag_name":"tauri-v0.9.0","name":"","draft":false,"html_url":"h","assets":[]}]"#,
        r#"[{"tag_name":"tauri-v0.9.0","name":"","draft":false,"html_url":"h","assets":[]},
            {"tag_name":"tauri-v0.9.0-rc1","name":"","draft":false,"html_url":"h","assets":[]}]"#,
    ] {
        assert_eq!(pick_release(json, "0.8.0").unwrap().unwrap().tag, "tauri-v0.9.0");
    }
}

#[test]
fn virgule_refusee_pour_explorer() {
    // explorer.exe découpe ses options sur les virgules (/root,…, /select,…).
    assert!(verifier_url_page("https://github.com/imtheyoyo/ComicRust/x,/root,C:").is_err());
}

// ─────────────────────────── Scripts de l'utilisateur ───────────────────────────

/// Extrait le zip `files` (version `n`) puis l'applique à `app`.
fn installer_zip(tmp: &Path, app: &Path, n: u32, files: &[(&str, &[u8])]) -> Bilan {
    let zip_path = tmp.join(format!("v{n}.zip"));
    std::fs::write(&zip_path, zip_of(files)).unwrap();
    let extrait = tmp.join(format!("extrait{n}"));
    let liste = extract(&zip_path, &extrait).unwrap();
    apply(&extrait, &liste, app).unwrap()
}

fn cles_manifeste(app: &Path) -> Vec<String> {
    lire_manifeste(app).unwrap().into_keys().collect()
}

/// Scripts de l'utilisateur (comme la version egui) : un exemple modifié n'est jamais écrasé
/// (nouvelle version à côté, `.nouveau`, comptée dans le bilan), un exemple non modifié est mis
/// à jour, un exemple supprimé ne revient pas, un nouvel exemple est ajouté, les `.old` de
/// l'utilisateur ne sont jamais supprimés ; `python\` est remplacé normalement ; `data\` intact.
#[test]
fn mise_a_jour_respecte_les_scripts_de_l_utilisateur() {
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    let ex = |n: &str| app.join("scripts").join("exemples").join(n);
    // Paquet v1 décompressé à la main (scripts et Python embarqué).
    let v1: &[(&str, &[u8])] = &[
        ("scripts/exemples/a.py", b"a 1"),
        ("scripts/exemples/b.py", b"b 1"),
        ("scripts/exemples/c.py", b"c 1"),
        ("scripts/exemples/bd2/motifs.json", b"motifs 1"),
        ("scripts/README.md", b"lisez 1"),
        ("scripts/lib/LISEZMOI.txt", b"lib 1"),
        ("python/python.exe", b"python 1"),
    ];
    for (name, data) in v1 {
        let p = app.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }
    // Script, module et `.old` de l'utilisateur.
    std::fs::write(app.join("scripts/mon_script.py"), b"perso").unwrap();
    std::fs::write(app.join("scripts/lib/mon_module.py"), b"module perso").unwrap();
    std::fs::write(app.join("scripts/mon_script.py.old"), b"sauvegarde perso").unwrap();
    std::fs::write(ex("c.py.old"), b"ma sauvegarde de c").unwrap();

    // Premier lancement : manifeste créé à partir des fichiers livrés présents (ni les scripts
    // de l'utilisateur, ni ses `.old`) ; un second appel ne le change pas.
    assurer_manifeste_scripts(&app);
    assert_eq!(
        cles_manifeste(&app),
        [
            "scripts/exemples/a.py",
            "scripts/exemples/b.py",
            "scripts/exemples/bd2/motifs.json",
            "scripts/exemples/c.py",
            "scripts/lib/lisezmoi.txt",
            "scripts/readme.md",
        ]
    );
    let texte = std::fs::read_to_string(app.join("data").join(MANIFESTE_SCRIPTS)).unwrap();
    assert!(texte.contains(&format!("{}\tscripts/README.md\n", sha_hex(b"lisez 1"))), "{texte}");
    // L'utilisateur modifie a.py, supprime b.py.
    std::fs::write(ex("a.py"), b"a 1 modifie").unwrap();
    std::fs::remove_file(ex("b.py")).unwrap();
    assurer_manifeste_scripts(&app);
    assert_eq!(std::fs::read_to_string(app.join("data").join(MANIFESTE_SCRIPTS)).unwrap(), texte);

    let bilan = installer_zip(
        tmp.path(),
        &app,
        2,
        &[
            ("ComicRust-Tauri.exe", b"exe 2"),
            ("python/python.exe", b"python 2"),
            ("scripts/exemples/a.py", b"a 2"),
            ("scripts/exemples/b.py", b"b 2"),
            ("scripts/exemples/c.py", b"c 2"),
            ("scripts/exemples/d.py", b"d 2"),
            ("scripts/exemples/bd2/motifs.json", b"motifs 1"),
            ("scripts/mon_script.py", b"homonyme livre"),
            ("scripts/README.md", b"lisez 1"),
        ],
    );
    assert_eq!(std::fs::read(ex("a.py")).unwrap(), b"a 1 modifie", "modifié : jamais écrasé");
    assert_eq!(std::fs::read(ex("a.py.nouveau")).unwrap(), b"a 2");
    assert!(!ex("b.py").exists(), "supprimé par l'utilisateur : pas réinstallé");
    assert_eq!(std::fs::read(ex("c.py")).unwrap(), b"c 2", "non modifié : mis à jour");
    assert_eq!(std::fs::read(ex("d.py")).unwrap(), b"d 2", "nouvel exemple installé");
    assert_eq!(std::fs::read(app.join("scripts/mon_script.py")).unwrap(), b"perso");
    assert_eq!(std::fs::read(app.join("scripts/mon_script.py.nouveau")).unwrap(), b"homonyme livre");
    assert_eq!(std::fs::read(app.join("scripts/lib/mon_module.py")).unwrap(), b"module perso");
    // Python embarqué : remplacé comme le reste (`.old`).
    assert_eq!(std::fs::read(app.join("python/python.exe")).unwrap(), b"python 2");
    assert_eq!(std::fs::read(app.join("python/python.exe.old")).unwrap(), b"python 1");
    // L'ancien c.py est mis de côté sans toucher au `.old` de l'utilisateur.
    assert_eq!(std::fs::read(ex("c.py.old")).unwrap(), b"ma sauvegarde de c");
    assert_eq!(std::fs::read(ex("c.py.1.old")).unwrap(), b"c 1");
    // Bilan : exe, python, c, d, motifs (identique), README (identique) ; 3 `.old`.
    assert_eq!(
        bilan,
        Bilan {
            remplaces: 6,
            restes_old: 3,
            scripts_nouveaux: 2,
            chemins_nouveaux: vec!["scripts/exemples/a.py.nouveau".into(), "scripts/mon_script.py.nouveau".into()],
            scripts_non_reinstalles: vec!["scripts/exemples/b.py".into()],
        }
    );
    let statut = StatutMaj { bilan: Some(bilan), ..StatutMaj::simple(EtatStatut::Installee, true) };
    let (_, v) = EvenementMaj::Statut(statut).pour_le_front();
    assert_eq!(v["bilan"]["scriptsNouveaux"], 2);
    assert_eq!(v["bilan"]["cheminsNouveaux"][1], "scripts/mon_script.py.nouveau");
    assert_eq!(v["bilan"]["scriptsNonReinstalles"][0], "scripts/exemples/b.py");
    // Manifeste : d.py ajouté, c.py à sa nouvelle empreinte, b.py gardé (jamais réinstallé),
    // a.py à l'empreinte livrée d'origine, script de l'utilisateur absent.
    let m = lire_manifeste(&app).unwrap();
    assert_eq!(m["scripts/exemples/d.py"].0, sha_hex(b"d 2"));
    assert_eq!(m["scripts/exemples/c.py"].0, sha_hex(b"c 2"));
    assert_eq!(m["scripts/exemples/b.py"].0, sha_hex(b"b 1"));
    assert_eq!(m["scripts/exemples/a.py"].0, sha_hex(b"a 1"));
    assert!(!m.contains_key("scripts/mon_script.py"));
    data_intact(&app);

    // Lancement suivant : seuls les `.old` de la mise à jour sont supprimés.
    cleanup(&app);
    assert!(!app.join("ComicRust-Tauri.exe.old").exists() && !app.join("python/python.exe.old").exists());
    assert!(!ex("c.py.1.old").exists());
    assert_eq!(std::fs::read(ex("c.py.old")).unwrap(), b"ma sauvegarde de c");
    assert_eq!(std::fs::read(app.join("scripts/mon_script.py.old")).unwrap(), b"sauvegarde perso");
    assert!(app.join("data").join(MANIFESTE_SCRIPTS).exists(), "manifeste conservé par le nettoyage");
    // Les `.nouveau` ne sont pas des scripts.
    let trouves = comicrust_core::scripting::discover(&app.join("scripts"));
    assert!(trouves.iter().all(|s| !s.rel_path.ends_with(SUFFIXE_NOUVEAU)), "{trouves:?}");

    // Mise à jour suivante : c.py (non modifié depuis) suit ; a.py reste, son `.nouveau` est
    // remplacé ; b.py ne revient toujours pas.
    let bilan = installer_zip(
        tmp.path(),
        &app,
        3,
        &[("scripts/exemples/c.py", b"c 3"), ("scripts/exemples/a.py", b"a 3"), ("scripts/exemples/b.py", b"b 3")],
    );
    assert_eq!(std::fs::read(ex("c.py")).unwrap(), b"c 3");
    assert_eq!(std::fs::read(ex("a.py")).unwrap(), b"a 1 modifie");
    assert_eq!(std::fs::read(ex("a.py.nouveau")).unwrap(), b"a 3");
    assert!(!ex("b.py").exists());
    assert_eq!((bilan.remplaces, bilan.scripts_nouveaux), (1, 1));
    // L'utilisateur reprend la version livrée de a.py : de nouveau suivi par les mises à jour.
    std::fs::write(ex("a.py"), b"a 3").unwrap();
    installer_zip(tmp.path(), &app, 4, &[("scripts/exemples/a.py", b"a 3")]);
    installer_zip(tmp.path(), &app, 5, &[("scripts/exemples/a.py", b"a 5")]);
    assert_eq!(std::fs::read(ex("a.py")).unwrap(), b"a 5");
    data_intact(&app);
}

/// Sans manifeste au moment d'installer, il est relevé d'abord : un exemple livré non modifié
/// est mis à jour (pas de `.nouveau`). Une installation sans `scripts\` (ancienne version Tauri)
/// reçoit tous les scripts.
#[test]
fn manifeste_releve_avant_installation_et_premiere_livraison() {
    let tmp = tempfile::tempdir().unwrap();
    let app = installation(tmp.path());
    let b = installer_zip(tmp.path(), &app, 1, &[("scripts/exemples/a.py", b"a 1"), ("scripts/README.md", b"l 1")]);
    assert_eq!((b.remplaces, b.scripts_nouveaux, b.restes_old), (2, 0, 0));
    assert_eq!(cles_manifeste(&app), ["scripts/exemples/a.py", "scripts/readme.md"]);

    // Paquet décompressé à la main, sans manifeste.
    let app2 = tmp.path().join("app2");
    std::fs::create_dir_all(app2.join("scripts/exemples")).unwrap();
    std::fs::write(app2.join("scripts/exemples/a.py"), b"a 1").unwrap();
    std::fs::write(app2.join("scripts/perso.py"), b"perso").unwrap();
    let b = installer_zip(tmp.path(), &app2, 2, &[("scripts/exemples/a.py", b"a 2"), ("scripts/perso.py", b"livre")]);
    assert_eq!(std::fs::read(app2.join("scripts/exemples/a.py")).unwrap(), b"a 2");
    assert_eq!(std::fs::read(app2.join("scripts/perso.py")).unwrap(), b"perso");
    assert_eq!(b.chemins_nouveaux, ["scripts/perso.py.nouveau"]);
}
