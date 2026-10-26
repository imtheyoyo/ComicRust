//! Mise à jour intégrée depuis les releases GitHub de ComicRust.
//!
//! 1. **Vérification** : liste des releases (`api.github.com`), la plus récente (pré-releases
//!    comprises, toutes les versions 0.x en sont) plus récente que la version en cours.
//! 2. **Téléchargement** du zip Windows et de son `.sha256` dans `data\maj\`, contrôle de
//!    l'empreinte.
//! 3. **Installation** : extraction, puis remplacement des fichiers à côté de l'exe. Windows
//!    interdit d'écraser un exe en cours d'exécution mais permet de le **renommer** :
//!    chaque fichier remplacé devient `nom.old`, supprimé au lancement suivant (seuls les
//!    `.old` créés par la mise à jour, listés dans `data\maj\anciens.txt`, le sont).
//!    Le dossier `data\` (réglages, caches) n'est jamais touché.
//!
//!    Dossier `scripts\` (scripts de l'utilisateur, exemples livrés dans `scripts\exemples\`) :
//!    - un fichier livré que l'utilisateur n'a pas modifié (empreinte notée dans
//!      `data\scripts-livres.txt`) est mis à jour normalement ;
//!    - un fichier existant **modifié** (ou inconnu) n'est jamais écrasé : la nouvelle
//!      version est posée à côté, `nom.py.nouveau` (extension non découverte comme script),
//!      et signalée dans le bilan de la mise à jour ;
//!    - un fichier livré puis supprimé par l'utilisateur n'est pas réinstallé.
//! 4. **Redémarrage** proposé à l'utilisateur.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use eframe::egui;
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const REPO: &str = "imtheyoyo/ComicRust";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Version comparée aux releases. Pour la recette, la variable d'environnement
/// `COMICRUST_SIMULER_VERSION` (ex. `0.2.0`) fait croire à une version plus ancienne,
/// ce qui permet d'essayer l'installation complète sans attendre la version suivante.
pub fn compared_version() -> String {
    std::env::var("COMICRUST_SIMULER_VERSION")
        .ok()
        .filter(|v| parse_version(v).is_some())
        .unwrap_or_else(|| CURRENT.to_string())
}
const OLD_SUFFIX: &str = ".old";
/// Nouvelle version d'un fichier de `scripts\` modifié par l'utilisateur, posée à côté
/// (`nom.py.nouveau` : pas découvert comme script, seuls les `.py` le sont).
pub const NEW_SUFFIX: &str = ".nouveau";
/// Dossier des scripts (protégé : voir l'en-tête du module).
const SCRIPTS_DIR: &str = "scripts";
/// `.old` créés par la mise à jour (chemins relatifs au dossier de l'application, un par
/// ligne), dans `data\maj\` : seuls ceux-là sont supprimés au lancement suivant.
const OLD_LIST: &str = "anciens.txt";
/// Fichiers de `scripts\` livrés par ComicRust (`<sha256>\t<chemin relatif>`), dans `data\`.
const SCRIPTS_MANIFEST: &str = "scripts-livres.txt";
/// Fichiers de `scripts\` relevés comme « livrés » au premier lancement sans liste
/// (paquet décompressé à la main, ou mis à jour par une version qui ne tenait pas la liste) :
/// les exemples et les fichiers d'accompagnement, jamais les scripts de l'utilisateur.
const SCRIPTS_DELIVERED: [&str; 3] = ["scripts/exemples/", "scripts/readme.md", "scripts/lib/lisezmoi.txt"];

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub page_url: String,
    /// Zip Windows x64 et son empreinte (absents si la release n'a pas de binaire Windows).
    pub zip: Option<(String, u64)>,
    pub sha256_url: Option<String>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// « v0.3.1 » → (0, 3, 1). Les suffixes (« -beta ») sont ignorés.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Choisit, dans la réponse JSON de l'API GitHub, la release la plus récente plus
/// récente que `current` (les brouillons sont ignorés).
pub fn pick_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let list: Vec<GhRelease> = serde_json::from_str(json).map_err(|e| format!("réponse GitHub illisible : {e}"))?;
    let best = list
        .into_iter()
        .filter(|r| !r.draft && is_newer(&r.tag_name, current))
        .max_by_key(|r| parse_version(&r.tag_name));
    Ok(best.map(|r| {
        let zip_name = format!("ComicRust-{}-windows-x64.zip", r.tag_name);
        let zip = r.assets.iter().find(|a| a.name == zip_name).map(|a| (a.browser_download_url.clone(), a.size));
        let sha256_url =
            r.assets.iter().find(|a| a.name == format!("{zip_name}.sha256")).map(|a| a.browser_download_url.clone());
        Release {
            name: r.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            tag: r.tag_name,
            notes: r.body.unwrap_or_default(),
            page_url: r.html_url,
            zip,
            sha256_url,
        }
    }))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .user_agent(concat!("ComicRust/", env!("CARGO_PKG_VERSION")))
        // Certificats du système (magasin Windows) : fonctionne derrière un antivirus
        // ou un proxy d'entreprise qui inspecte le HTTPS.
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
        // Redirections suivies à la main (voir `get`).
        .max_redirects(0)
        .build()
        .into()
}

/// GET qui suit les redirections sans lire le corps des réponses 302 : GitHub renvoie
/// ses fichiers de release par une 302 vers release-assets.githubusercontent.com, et
/// certains proxys tronquent ce corps vide, ce qui fait échouer la lecture.
fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    let agent = agent();
    let mut url = url.to_string();
    for _ in 0..6 {
        let resp = agent.get(&url).call().map_err(|e| e.to_string())?;
        if !resp.status().is_redirection() {
            return Ok(resp);
        }
        url =
            resp.headers().get("location").and_then(|v| v.to_str().ok()).ok_or("redirection sans adresse")?.to_string();
    }
    Err("trop de redirections".into())
}

pub fn fetch_latest() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=15");
    let json = get(&url)
        .map_err(|e| format!("GitHub injoignable : {e}"))?
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    pick_release(&json, &compared_version())
}

/// Empreinte attendue dans un fichier `.sha256` (« <hex>  <nom> »).
pub fn parse_sha256_file(text: &str) -> Option<String> {
    let h = text.split_whitespace().next()?.to_ascii_lowercase();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn download(url: &str, dest: &Path, done: &AtomicU64) -> Result<(), String> {
    let resp = get(url).map_err(|e| format!("téléchargement impossible : {e}"))?;
    let mut reader = resp.into_body().into_reader();
    let tmp = dest.with_extension("part");
    let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("téléchargement interrompu : {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done.fetch_add(n as u64, Ordering::Relaxed);
    }
    drop(out);
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Extrait le zip dans `dest` (chemins relatifs uniquement).
pub fn extract(zip_path: &Path, dest: &Path) -> Result<Vec<PathBuf>, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("zip invalide : {e}"))?;
    let _ = std::fs::remove_dir_all(dest);
    let mut files = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        files.push(rel);
    }
    Ok(files)
}

/// Bilan d'une installation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InstallReport {
    /// Fichiers installés ou déjà à jour.
    pub installed: usize,
    /// Fichiers de `scripts\` modifiés par l'utilisateur, gardés : nouvelle version posée à
    /// côté (`…{NEW_SUFFIX}`), chemins relatifs au dossier de l'application.
    pub set_aside: Vec<PathBuf>,
    /// Fichiers livrés supprimés par l'utilisateur : pas réinstallés.
    pub not_reinstalled: Vec<PathBuf>,
}

/// Chemin relatif → clé de comparaison (`/`, minuscules : Windows ignore la casse).
fn rel_key(rel: &Path) -> String {
    rel.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect::<Vec<_>>().join("/")
}

fn in_dir(rel: &Path, dir: &str) -> bool {
    rel.components().next().is_some_and(|c| c.as_os_str().eq_ignore_ascii_case(dir))
}

/// Liste des fichiers de `scripts\` livrés : clé → (empreinte, chemin relatif tel qu'écrit).
type Manifest = std::collections::BTreeMap<String, (String, String)>;

fn manifest_path(app_dir: &Path) -> PathBuf {
    app_dir.join("data").join(SCRIPTS_MANIFEST)
}

fn read_manifest(app_dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(manifest_path(app_dir)).ok()?;
    let mut m = Manifest::new();
    for line in text.lines() {
        if let Some((hash, rel)) = line.split_once('\t') {
            m.insert(rel_key(Path::new(rel)), (hash.to_string(), rel.to_string()));
        }
    }
    Some(m)
}

fn write_manifest(app_dir: &Path, m: &Manifest) -> Result<(), String> {
    let path = manifest_path(app_dir);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let text: String = m.values().map(|(h, rel)| format!("{h}\t{rel}\n")).collect();
    std::fs::write(&path, text).map_err(|e| format!("{} : {e}", path.display()))
}

/// Premier lancement sans liste des scripts livrés : relève les fichiers livrés présents
/// (voir [`SCRIPTS_DELIVERED`]) comme non modifiés.
pub fn ensure_scripts_manifest(app_dir: &Path) {
    if manifest_path(app_dir).exists() {
        return;
    }
    let mut files = Vec::new();
    collect_files(&app_dir.join(SCRIPTS_DIR), &mut files, 8);
    let mut m = Manifest::new();
    for f in files {
        let Ok(rel) = f.strip_prefix(app_dir) else { continue };
        let key = rel_key(rel);
        let delivered = SCRIPTS_DELIVERED.iter().any(|d| if d.ends_with('/') { key.starts_with(d) } else { key == *d });
        let ours = key.ends_with(NEW_SUFFIX) || key.ends_with(OLD_SUFFIX) || key.contains("/__pycache__/");
        if delivered && !ours {
            if let Ok(h) = sha256_of(&f) {
                m.insert(
                    key,
                    (h, rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")),
                );
            }
        }
    }
    let _ = write_manifest(app_dir, &m);
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        match e.file_type() {
            Ok(t) if t.is_dir() && depth > 0 => collect_files(&e.path(), out, depth - 1),
            Ok(t) if t.is_file() => out.push(e.path()),
            _ => {}
        }
    }
}

/// `x.old`, ou `x.1.old`, `x.2.old`… : jamais un fichier existant (qui peut être à
/// l'utilisateur).
fn free_old_name(dst: &Path) -> PathBuf {
    let base = dst.display().to_string();
    let first = PathBuf::from(format!("{base}{OLD_SUFFIX}"));
    if !first.exists() {
        return first;
    }
    (1..).map(|i| PathBuf::from(format!("{base}.{i}{OLD_SUFFIX}"))).find(|p| !p.exists()).expect("nom libre")
}

/// Copie les fichiers extraits dans `app_dir`, en renommant d'abord ceux qui existent
/// (`x.exe` → `x.exe.old`, noté dans `data\maj\anciens.txt`). Le dossier `data` n'est
/// jamais modifié ; `scripts\` est protégé (voir l'en-tête du module).
pub fn apply(extracted: &Path, files: &[PathBuf], app_dir: &Path) -> Result<InstallReport, String> {
    let mut report = InstallReport::default();
    let mut manifest = read_manifest(app_dir).unwrap_or_default();
    let mut olds: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<(), String> {
        for rel in files {
            if in_dir(rel, "data") {
                continue;
            }
            let src = extracted.join(rel);
            let dst = app_dir.join(rel);
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let scripts = in_dir(rel, SCRIPTS_DIR);
            let key = rel_key(rel);
            let rel_text = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            let new_hash = if scripts { Some(sha256_of(&src).map_err(|e| e.to_string())?) } else { None };
            if dst.exists() {
                if let Some(new_hash) = &new_hash {
                    let current = sha256_of(&dst).map_err(|e| format!("{} : {e}", dst.display()))?;
                    if &current == new_hash {
                        manifest.insert(key, (current, rel_text));
                        report.installed += 1;
                        continue;
                    }
                    let unmodified = manifest.get(&key).is_some_and(|(h, _)| *h == current);
                    if !unmodified {
                        // Modifié par l'utilisateur (ou fichier inconnu) : gardé, nouvelle
                        // version à côté.
                        let aside = PathBuf::from(format!("{}{NEW_SUFFIX}", dst.display()));
                        std::fs::copy(&src, &aside).map_err(|e| format!("{} : {e}", aside.display()))?;
                        report.set_aside.push(PathBuf::from(format!("{}{NEW_SUFFIX}", rel.display())));
                        continue;
                    }
                }
                let old = if scripts {
                    free_old_name(&dst)
                } else {
                    let mut old = PathBuf::from(format!("{}{OLD_SUFFIX}", dst.display()));
                    if old.exists() && std::fs::remove_file(&old).is_err() {
                        // Ancienne copie encore verrouillée : nom unique.
                        old = PathBuf::from(format!("{}.{}{OLD_SUFFIX}", dst.display(), std::process::id()));
                    }
                    old
                };
                std::fs::rename(&dst, &old).map_err(|e| format!("{} : {e}", dst.display()))?;
                olds.push(old.strip_prefix(app_dir).map(Path::to_path_buf).unwrap_or(old));
            } else if scripts && manifest.contains_key(&key) {
                // Livré par une version précédente puis supprimé par l'utilisateur.
                report.not_reinstalled.push(rel.clone());
                continue;
            }
            std::fs::copy(&src, &dst).map_err(|e| format!("{} : {e}", dst.display()))?;
            if let Some(h) = new_hash {
                manifest.insert(key, (h, rel_text));
            }
            report.installed += 1;
        }
        Ok(())
    })();
    // Notés même après une erreur : les `.old` déjà créés seront supprimés au lancement
    // suivant, la liste des scripts livrés reste juste.
    let _ = write_manifest(app_dir, &manifest);
    record_olds(app_dir, &olds);
    result.map(|()| report)
}

fn old_list_path(app_dir: &Path) -> PathBuf {
    app_dir.join("data").join("maj").join(OLD_LIST)
}

/// Ajoute des `.old` (relatifs à `app_dir`) à la liste de ceux à supprimer.
fn record_olds(app_dir: &Path, olds: &[PathBuf]) {
    if olds.is_empty() {
        return;
    }
    let path = old_list_path(app_dir);
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for o in olds {
        text.push_str(&o.to_string_lossy());
        text.push('\n');
    }
    let _ = std::fs::write(&path, text);
}

/// Supprime les `*.old` laissés par une mise à jour précédente, et les téléchargements ;
/// relève les scripts livrés au premier lancement (voir [`ensure_scripts_manifest`]).
pub fn cleanup_previous_install() {
    let dir = crate::paths::exe_dir();
    cleanup(&dir);
    ensure_scripts_manifest(&dir);
}

/// Nettoyage de `app_dir` après une mise à jour : seuls les `.old` listés dans
/// `data\maj\anciens.txt` sont supprimés (ceux encore verrouillés restent listés pour le
/// lancement suivant), jamais un `.old` de l'utilisateur. Sans liste (mise à jour faite par
/// une version ≤ 0.9.1, qui ne la tenait pas) : `.old` hors de `data\` et de `scripts\`.
fn cleanup(app_dir: &Path) {
    let list = old_list_path(app_dir);
    let mut left: Vec<PathBuf> = Vec::new();
    match std::fs::read_to_string(&list) {
        Ok(text) => {
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let rel = PathBuf::from(line.trim());
                // Uniquement des `.old` relatifs, sans remontée (liste écrite par nous).
                let safe = rel.is_relative()
                    && rel.components().all(|c| matches!(c, std::path::Component::Normal(_)))
                    && line.trim().ends_with(OLD_SUFFIX);
                if !safe {
                    continue;
                }
                let p = app_dir.join(&rel);
                if p.exists() && std::fs::remove_file(&p).is_err() {
                    left.push(rel);
                }
            }
        }
        Err(_) => legacy_cleanup_dir(app_dir, 2, true),
    }
    let _ = std::fs::remove_dir_all(app_dir.join("data").join("maj"));
    record_olds(app_dir, &left);
}

/// Ancien nettoyage (sans liste) : `.old` du dossier de l'application et de ses
/// sous-dossiers, sauf `data\` et `scripts\` (où l'utilisateur peut avoir les siens).
fn legacy_cleanup_dir(dir: &Path, depth: usize, top: bool) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            let protected = top
                && p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("data") || n.eq_ignore_ascii_case(SCRIPTS_DIR));
            if depth > 0 && !protected {
                legacy_cleanup_dir(&p, depth - 1, false);
            }
        } else if p.to_string_lossy().ends_with(OLD_SUFFIX) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Télécharge, vérifie et installe une release. `done` compte les octets reçus.
pub fn install(release: &Release, app_dir: &Path, work: &Path, done: &AtomicU64) -> Result<InstallReport, String> {
    let (zip_url, _) = release.zip.as_ref().ok_or("cette version n'a pas de paquet Windows")?;
    let sha_url = release.sha256_url.as_ref().ok_or("empreinte SHA-256 absente de la release")?;
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let expected = get(sha_url)
        .map_err(|e| format!("empreinte introuvable : {e}"))?
        .into_body()
        .read_to_string()
        .ok()
        .and_then(|t| parse_sha256_file(&t))
        .ok_or("empreinte SHA-256 illisible")?;
    let zip_path = work.join(format!("ComicRust-{}.zip", release.tag));
    download(zip_url, &zip_path, done)?;
    let actual = sha256_of(&zip_path).map_err(|e| e.to_string())?;
    if actual != expected {
        let _ = std::fs::remove_file(&zip_path);
        return Err("le fichier téléchargé est corrompu (empreinte SHA-256 différente) : rien n'a été modifié".into());
    }
    let extracted = work.join(&release.tag);
    let files = extract(&zip_path, &extracted)?;
    if !files.iter().any(|f| f.to_string_lossy().eq_ignore_ascii_case("ComicRust.exe")) {
        return Err("paquet inattendu (ComicRust.exe absent) : rien n'a été modifié".into());
    }
    let res = apply(&extracted, &files, app_dir);
    // Le zip et sa copie extraite ne servent plus (la liste des `.old`, dans le même
    // dossier, sert au lancement suivant).
    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_dir_all(&extracted);
    res
}

/// Relance ComicRust (après installation).
pub fn restart() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).spawn().map(drop).map_err(|e| e.to_string())
}

// ----------------------------------------------------------------------------
// État pour l'interface

pub enum Status {
    Idle,
    Checking { rx: Receiver<Result<Option<Release>, String>>, manual: bool },
    UpToDate,
    Available(Release),
    Installing { rx: Receiver<Result<InstallReport, String>>, release: Release, done: Arc<AtomicU64> },
    Installed(Release, InstallReport),
    Failed(String),
}

pub struct UpdateState {
    pub status: Status,
    /// Fenêtre de détail ouverte.
    pub window: bool,
    /// Bandeau fermé (« Plus tard ») pour cette session.
    pub banner_closed: bool,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self { status: Status::Idle, window: false, banner_closed: false }
    }
}

impl UpdateState {
    /// Lance une vérification ; `manual` : afficher aussi « à jour » ou l'erreur.
    pub fn check(&mut self, ctx: &egui::Context, manual: bool) {
        if matches!(self.status, Status::Checking { .. } | Status::Installing { .. }) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_latest());
            ctx.request_repaint();
        });
        self.status = Status::Checking { rx, manual };
        if manual {
            self.window = true;
        }
    }

    pub fn start_install(&mut self, release: Release, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let done = Arc::new(AtomicU64::new(0));
        let d = done.clone();
        let ctx = ctx.clone();
        let rel = release.clone();
        std::thread::spawn(move || {
            let app_dir = crate::paths::exe_dir();
            let work = app_dir.join("data").join("maj");
            let _ = tx.send(install(&rel, &app_dir, &work, &d));
            ctx.request_repaint();
        });
        self.status = Status::Installing { rx, release, done };
        self.window = true;
    }

    pub fn poll(&mut self) {
        let next = match &self.status {
            Status::Checking { rx, manual } => match rx.try_recv() {
                Ok(Ok(Some(r))) => Some(Status::Available(r)),
                Ok(Ok(None)) => Some(if *manual { Status::UpToDate } else { Status::Idle }),
                Ok(Err(e)) => Some(if *manual { Status::Failed(e) } else { Status::Idle }),
                Err(_) => None,
            },
            Status::Installing { rx, release, .. } => match rx.try_recv() {
                Ok(Ok(report)) => Some(Status::Installed(release.clone(), report)),
                Ok(Err(e)) => Some(Status::Failed(e)),
                Err(_) => None,
            },
            _ => None,
        };
        if let Some(s) = next {
            self.status = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.3.1"), Some((0, 3, 1)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert!(is_newer("v0.3.0", "0.2.1"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("v0.2.1", "0.2.1"));
        assert!(!is_newer("nightly", "0.2.1"));
    }

    const JSON: &str = r#"[
      {"tag_name":"v0.4.0","name":"brouillon","draft":true,"html_url":"h4","assets":[]},
      {"tag_name":"v0.3.1","name":"ComicRust v0.3.1","body":"Correctifs","draft":false,"prerelease":true,
       "html_url":"https://github.com/imtheyoyo/ComicRust/releases/tag/v0.3.1",
       "assets":[{"name":"ComicRust-v0.3.1-windows-x64.zip","browser_download_url":"https://x/z.zip","size":123},
                 {"name":"ComicRust-v0.3.1-windows-x64.zip.sha256","browser_download_url":"https://x/z.sha256","size":100},
                 {"name":"v0.3.1.md","browser_download_url":"https://x/r.md","size":1}]},
      {"tag_name":"v0.3.0","name":"","draft":false,"html_url":"h3","assets":[]},
      {"tag_name":"v0.2.1","draft":false,"html_url":"h2","assets":[]}
    ]"#;

    #[test]
    fn picks_newest_published_release() {
        let r = pick_release(JSON, "0.3.0").unwrap().unwrap();
        assert_eq!(r.tag, "v0.3.1");
        assert_eq!(r.zip, Some(("https://x/z.zip".into(), 123)));
        assert_eq!(r.sha256_url.as_deref(), Some("https://x/z.sha256"));
        assert_eq!(pick_release(JSON, "0.3.1").unwrap(), None);
        assert_eq!(pick_release(JSON, "0.2.1").unwrap().unwrap().tag, "v0.3.1");
    }

    #[test]
    fn sha256_file_format() {
        let h = "72e0451f6e416c76a589ee96b752d5dfb3d0f661258fa8296a987736efe006f8";
        assert_eq!(parse_sha256_file(&format!("{h}  ComicRust-v0.2.1-windows-x64.zip\r\n")).as_deref(), Some(h));
        assert_eq!(parse_sha256_file("pas une empreinte"), None);
    }

    #[test]
    fn extract_and_apply_keep_data_and_rename_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("ComicRust");
        std::fs::create_dir_all(app.join("data")).unwrap();
        std::fs::write(app.join("ComicRust.exe"), b"ancienne version").unwrap();
        std::fs::write(app.join("data").join("comicrust.ron"), b"reglages").unwrap();

        let zip_path = tmp.path().join("maj.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            for (name, data) in [
                ("ComicRust.exe", &b"nouvelle version"[..]),
                ("pdfium.dll", b"dll"),
                ("licences/UnRAR-license.txt", b"lic"),
                ("data/comicrust.ron", b"ne doit pas ecraser"),
                ("../evasion.txt", b"refuse"),
            ] {
                w.start_file(name, o).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        let extracted = tmp.path().join("extrait");
        let files = extract(&zip_path, &extracted).unwrap();
        assert!(!files.iter().any(|f| f.to_string_lossy().contains("evasion")));
        let n = apply(&extracted, &files, &app).unwrap().installed;
        assert_eq!(n, 3);
        assert_eq!(std::fs::read(app.join("ComicRust.exe")).unwrap(), b"nouvelle version");
        assert_eq!(std::fs::read(app.join("ComicRust.exe.old")).unwrap(), b"ancienne version");
        assert_eq!(std::fs::read(app.join("data").join("comicrust.ron")).unwrap(), b"reglages");
        assert!(app.join("licences").join("UnRAR-license.txt").exists());
        assert!(!tmp.path().join("evasion.txt").exists());

        // Deuxième mise à jour : l'ancien .old est remplacé.
        let n = apply(&extracted, &files, &app).unwrap().installed;
        assert_eq!(n, 3);
        cleanup(&app);
        assert!(!app.join("ComicRust.exe.old").exists());
        assert!(app.join("ComicRust.exe").exists());
    }

    /// Zip d'une « version » : fichiers (chemin, contenu).
    fn zip_of(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            w.start_file(*name, o).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    fn install_zip(tmp: &Path, app: &Path, n: u32, files: &[(&str, &[u8])]) -> InstallReport {
        let zip_path = tmp.join(format!("v{n}.zip"));
        zip_of(&zip_path, files);
        let extracted = tmp.join(format!("extrait{n}"));
        let list = extract(&zip_path, &extracted).unwrap();
        apply(&extracted, &list, app).unwrap()
    }

    /// Scripts de l'utilisateur : un exemple modifié n'est jamais écrasé (nouvelle version à
    /// côté, `.nouveau`), un exemple non modifié est mis à jour, un exemple supprimé ne
    /// revient pas, les `.old` de l'utilisateur ne sont jamais supprimés.
    #[test]
    fn mise_a_jour_respecte_les_scripts_de_l_utilisateur() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("ComicRust");
        let ex = |n: &str| app.join("scripts").join("exemples").join(n);
        // Paquet décompressé à la main : v1.
        let v1: &[(&str, &[u8])] = &[
            ("ComicRust.exe", b"exe 1"),
            ("scripts/exemples/a.py", b"a 1"),
            ("scripts/exemples/b.py", b"b 1"),
            ("scripts/exemples/c.py", b"c 1"),
            ("scripts/README.md", b"lisez 1"),
        ];
        for (name, data) in v1 {
            let p = app.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, data).unwrap();
        }
        // Script et `.old` de l'utilisateur.
        std::fs::write(app.join("scripts").join("mon_script.py"), b"perso").unwrap();
        std::fs::write(app.join("scripts").join("mon_script.py.old"), b"sauvegarde perso").unwrap();
        std::fs::write(ex("c.py.old"), b"ma sauvegarde de c").unwrap();
        // Premier lancement : relevé des fichiers livrés (pas le script de l'utilisateur).
        ensure_scripts_manifest(&app);
        let m = read_manifest(&app).unwrap();
        assert!(m.contains_key("scripts/exemples/a.py") && m.contains_key("scripts/readme.md"));
        assert!(!m.contains_key("scripts/mon_script.py"));

        // L'utilisateur modifie a.py, supprime b.py.
        std::fs::write(ex("a.py"), b"a 1 modifie").unwrap();
        std::fs::remove_file(ex("b.py")).unwrap();
        let r = install_zip(
            tmp.path(),
            &app,
            2,
            &[
                ("ComicRust.exe", b"exe 2"),
                ("scripts/exemples/a.py", b"a 2"),
                ("scripts/exemples/b.py", b"b 2"),
                ("scripts/exemples/c.py", b"c 2"),
                ("scripts/exemples/d.py", b"d 2"),
                ("scripts/mon_script.py", b"homonyme livre"),
                ("scripts/README.md", b"lisez 1"),
            ],
        );
        assert_eq!(std::fs::read(ex("a.py")).unwrap(), b"a 1 modifie", "modifié : jamais écrasé");
        assert_eq!(std::fs::read(ex("a.py.nouveau")).unwrap(), b"a 2");
        assert!(!ex("b.py").exists(), "supprimé par l'utilisateur : pas réinstallé");
        assert_eq!(std::fs::read(ex("c.py")).unwrap(), b"c 2", "non modifié : mis à jour");
        assert_eq!(std::fs::read(ex("d.py")).unwrap(), b"d 2", "nouvel exemple installé");
        assert_eq!(std::fs::read(app.join("scripts").join("mon_script.py")).unwrap(), b"perso");
        assert_eq!(std::fs::read(app.join("scripts").join("mon_script.py.nouveau")).unwrap(), b"homonyme livre");
        let mut aside: Vec<String> = r.set_aside.iter().map(|p| rel_key(p)).collect();
        aside.sort();
        assert_eq!(aside, ["scripts/exemples/a.py.nouveau", "scripts/mon_script.py.nouveau"]);
        assert_eq!(r.not_reinstalled, [PathBuf::from("scripts/exemples/b.py")]);
        // L'ancien c.py est mis de côté sans toucher au `.old` de l'utilisateur.
        assert_eq!(std::fs::read(ex("c.py.old")).unwrap(), b"ma sauvegarde de c");
        assert_eq!(std::fs::read(ex("c.py.1.old")).unwrap(), b"c 1");

        // Lancement suivant : seuls les `.old` de la mise à jour sont supprimés.
        cleanup(&app);
        assert!(!app.join("ComicRust.exe.old").exists());
        assert!(!ex("c.py.1.old").exists());
        assert_eq!(std::fs::read(ex("c.py.old")).unwrap(), b"ma sauvegarde de c");
        assert_eq!(std::fs::read(app.join("scripts").join("mon_script.py.old")).unwrap(), b"sauvegarde perso");
        assert!(!app.join("data").join("maj").exists());
        // Les `.nouveau` ne sont pas des scripts.
        let found = comicrust_core::scripting::discover(&app.join("scripts"));
        assert!(found.is_empty(), "{found:?}");

        // Mise à jour suivante : c.py (mis à jour, non modifié depuis) suit ; a.py reste.
        install_zip(tmp.path(), &app, 3, &[("scripts/exemples/c.py", b"c 3"), ("scripts/exemples/a.py", b"a 3")]);
        assert_eq!(std::fs::read(ex("c.py")).unwrap(), b"c 3");
        assert_eq!(std::fs::read(ex("a.py")).unwrap(), b"a 1 modifie");
        assert_eq!(std::fs::read(ex("a.py.nouveau")).unwrap(), b"a 3");
    }

    /// Sans liste (mise à jour faite par une ancienne version) : `.old` hors de `scripts\`
    /// et de `data\` seulement.
    #[test]
    fn nettoyage_sans_liste() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path();
        for f in ["ComicRust.exe.old", "licences/x.txt.old", "scripts/a.py.old", "data/r.old"] {
            let p = app.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        cleanup(app);
        assert!(!app.join("ComicRust.exe.old").exists() && !app.join("licences/x.txt.old").exists());
        assert!(app.join("scripts/a.py.old").exists() && app.join("data/r.old").exists());
    }

    #[test]
    fn checksum_of_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_of(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Bout en bout contre GitHub (réseau) : la release v0.2.1 vue depuis une « v0.2.0 »,
    /// téléchargée, vérifiée et installée dans un dossier temporaire.
    /// `cargo test -p comicrust -- --ignored maj_reelle`
    #[test]
    #[ignore]
    fn maj_reelle_depuis_github() {
        let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=15");
        let json = get(&url).unwrap().into_body().read_to_string().unwrap();
        let rel = pick_release(&json, "0.2.0").unwrap().expect("une release plus récente que 0.2.0");
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        std::fs::create_dir_all(app.join("data")).unwrap();
        std::fs::write(app.join("ComicRust.exe"), b"ancienne").unwrap();
        std::fs::write(app.join("data/comicrust.ron"), b"reglages").unwrap();
        let done = AtomicU64::new(0);
        let n = install(&rel, &app, &dir.path().join("maj"), &done).unwrap().installed;
        assert!(n >= 3, "{n} fichiers installés");
        assert!(std::fs::metadata(app.join("ComicRust.exe")).unwrap().len() > 1_000_000);
        assert!(app.join("pdfium.dll").exists());
        assert_eq!(std::fs::read(app.join("data/comicrust.ron")).unwrap(), b"reglages");
        eprintln!("{} : {n} fichiers, {} octets téléchargés", rel.tag, done.load(std::sync::atomic::Ordering::Relaxed));
    }
}
