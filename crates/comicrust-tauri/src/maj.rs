//! Mise à jour intégrée de ComicRust-Tauri depuis les releases GitHub `tauri-v*`.
//!
//! Noyau **dupliqué** de la version egui (`crates/comicrust/src/update/mod.rs`, qui reste
//! inchangé), adapté à la version Tauri. Contrat IPC : `docs/tauri/ipc-maj.md`.
//!
//! 1. **Vérification** : liste des releases de `imtheyoyo/ComicRust` (API GitHub, `per_page=50`).
//!    Seules les releases dont le tag commence par `tauri-v` sont retenues : les releases de la
//!    version egui (`vX.Y.Z`) cohabitent dans le même dépôt (sinon `v0.10.7` paraîtrait plus
//!    récente que `0.7.3`). Brouillons ignorés, pré-releases acceptées (toutes les 0.x en sont).
//! 2. **Téléchargement** de `ComicRust-Tauri-{tag}-windows-x64.zip` (fichier `.part` dans
//!    `data\maj\`, renommé à la fin) et de son `.zip.sha256`, contrôle de l'empreinte.
//! 3. **Installation** : extraction (chemins relatifs uniquement, `../` refusé), le paquet doit
//!    contenir `ComicRust-Tauri.exe`. Windows interdit d'écraser un exe en cours d'exécution mais
//!    permet de le **renommer** : chaque fichier remplacé (les deux exe, `pdfium.dll`, les `.md`,
//!    `licences\`) devient `nom.old`, noté dans `data\maj\anciens.txt` et supprimé au lancement
//!    suivant (seuls ceux-là). Le dossier `data\` (réglages, thèmes, profil WebView2) n'est
//!    jamais touché.
//! 4. **Redémarrage** : le même exe est relancé avec les mêmes arguments (`--mcp=47803`…) et
//!    `--attendre-pid <pid>` : la nouvelle instance attend (10 s au plus) la fin de l'ancienne,
//!    qui libère le profil WebView2 et le port MCP.
//!
//! Variables d'environnement (recette, tests) :
//! - `COMICRUST_SIMULER_VERSION` (ex. `0.7.0`) : version comparée aux releases ;
//! - `COMICRUST_MAJ_URL` : remplace l'adresse de l'API GitHub (faux serveur de test).
//!
//! Le noyau (versions, choix de la release, empreintes, extraction, remplacement, nettoyage,
//! arguments de relance) compile sans la fonction `app` ; la partie réseau et l'état partagé
//! ([`Maj`]) demandent `ureq` (fonction `app`, et les tests).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Dépôt GitHub (releases egui et Tauri mêlées).
pub const REPO: &str = "imtheyoyo/ComicRust";
/// Préfixe des tags des releases de la version Tauri.
pub const PREFIXE_TAG: &str = "tauri-v";
/// Exécutable obligatoire dans le paquet.
pub const EXE_PRINCIPAL: &str = "ComicRust-Tauri.exe";
/// Version en cours (celle de `comicrust-tauri`).
pub const VERSION_ACTUELLE: &str = env!("CARGO_PKG_VERSION");
/// Option interne passée à la nouvelle instance après une mise à jour.
pub const ARG_ATTENDRE_PID: &str = "--attendre-pid";
/// Attente maximale de la fin de l'ancienne instance.
pub const DELAI_ATTENTE_PID: Duration = Duration::from_secs(10);
/// Sous-dossier de `data\` : téléchargements, extraction, liste des `.old`.
pub const DOSSIER_MAJ: &str = "maj";
/// Suffixe des fichiers remplacés.
const SUFFIXE_OLD: &str = ".old";
/// `.old` créés par la mise à jour (chemins relatifs au dossier de l'application, un par
/// ligne), dans `data\maj\` : seuls ceux-là sont supprimés au lancement suivant.
const LISTE_OLD: &str = "anciens.txt";

/// Adresse de l'API (`COMICRUST_MAJ_URL` pour un faux serveur de test).
pub fn url_api() -> String {
    std::env::var("COMICRUST_MAJ_URL")
        .ok()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| format!("https://api.github.com/repos/{REPO}/releases?per_page=50"))
}

/// Version comparée aux releases. Pour la recette, `COMICRUST_SIMULER_VERSION` (ex. `0.7.0`)
/// fait croire à une version plus ancienne, pour essayer l'installation complète.
pub fn version_comparee() -> String {
    std::env::var("COMICRUST_SIMULER_VERSION")
        .ok()
        .filter(|v| parse_version(v).is_some())
        .unwrap_or_else(|| VERSION_ACTUELLE.to_string())
}

/// L'installation n'est possible que sous Windows, dans la version livrée (front embarqué,
/// compilation optimisée) : jamais en développement, où l'exe est celui de `target\`.
pub fn installable() -> bool {
    cfg!(windows) && cfg!(feature = "custom-protocol") && !cfg!(debug_assertions)
}

// ─────────────────────────────── Releases ───────────────────────────────

/// Release retenue (adresses complètes, gardées côté Rust).
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub tag: String,
    pub nom: String,
    /// Notes de version (markdown brut).
    pub notes: String,
    pub url_page: String,
    /// Zip Windows x64 : adresse et taille (absent si la release n'a pas de paquet).
    pub zip: Option<(String, u64)>,
    pub sha256_url: Option<String>,
}

impl Release {
    /// Vue envoyée au front (`release` de `StatutMaj`).
    pub fn infos(&self) -> InfosRelease {
        InfosRelease {
            tag: self.tag.clone(),
            nom: self.nom.clone(),
            notes: self.notes.clone(),
            url_page: self.url_page.clone(),
            taille: self.zip.as_ref().map_or(0, |(_, t)| *t),
            a_paquet: self.zip.is_some() && self.sha256_url.is_some(),
        }
    }
}

/// Nom du paquet Windows d'une release (`ComicRust-Tauri-tauri-v0.8.0-windows-x64.zip`).
pub fn nom_paquet(tag: &str) -> String {
    format!("ComicRust-Tauri-{tag}-windows-x64.zip")
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
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// « tauri-v0.3.1 », « v0.3.1 » ou « 0.3.1 » → (0, 3, 1). Les suffixes (« -beta ») sont ignorés.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let s = s.strip_prefix("tauri-").unwrap_or(s).trim_start_matches(['v', 'V']);
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

/// Même version (tag ou numéro), pour la version ignorée par l'utilisateur.
pub fn meme_version(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

/// Choisit, dans la réponse JSON de l'API GitHub, la release **Tauri** (`tauri-v*`) la plus
/// récente plus récente que `current` (brouillons ignorés, pré-releases acceptées).
pub fn pick_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let list: Vec<GhRelease> = serde_json::from_str(json).map_err(|e| format!("réponse GitHub illisible : {e}"))?;
    let best = list
        .into_iter()
        .filter(|r| !r.draft && r.tag_name.starts_with(PREFIXE_TAG) && is_newer(&r.tag_name, current))
        .max_by_key(|r| parse_version(&r.tag_name));
    Ok(best.map(|r| {
        let zip_name = nom_paquet(&r.tag_name);
        let zip = r.assets.iter().find(|a| a.name == zip_name).map(|a| (a.browser_download_url.clone(), a.size));
        let sha256_url =
            r.assets.iter().find(|a| a.name == format!("{zip_name}.sha256")).map(|a| a.browser_download_url.clone());
        Release {
            nom: r.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            tag: r.tag_name,
            notes: r.body.unwrap_or_default(),
            url_page: r.html_url,
            zip,
            sha256_url,
        }
    }))
}

// ─────────────────────────────── Empreintes ───────────────────────────────

/// Empreinte attendue dans un fichier `.sha256` (« <hex>  <nom> »).
pub fn parse_sha256_file(text: &str) -> Option<String> {
    let h = text.trim_start_matches('\u{feff}').split_whitespace().next()?.to_ascii_lowercase();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

/// SHA-256 d'un fichier, en hexadécimal minuscule.
pub fn sha256_of(path: &Path) -> std::io::Result<String> {
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

// ─────────────────────────────── Installation ───────────────────────────────

/// Extrait le zip dans `dest` (vidé d'abord) ; chemins relatifs uniquement (une entrée
/// `../x` ou absolue est ignorée). Renvoie les fichiers extraits (relatifs à `dest`).
pub fn extract(zip_path: &Path, dest: &Path) -> Result<Vec<PathBuf>, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("zip invalide : {e}"))?;
    let _ = std::fs::remove_dir_all(dest);
    let mut files = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { continue };
        if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            continue;
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| format!("{} : {e}", out.display()))?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        files.push(rel);
    }
    Ok(files)
}

/// Vrai si le paquet extrait contient l'exe principal à sa racine.
pub fn contient_exe_principal(files: &[PathBuf]) -> bool {
    files.iter().any(|f| f.components().count() == 1 && f.to_string_lossy().eq_ignore_ascii_case(EXE_PRINCIPAL))
}

/// Bilan d'une installation (`bilan` de `StatutMaj`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bilan {
    /// Fichiers installés (remplacés ou nouveaux).
    pub remplaces: usize,
    /// Anciens fichiers renommés en `.old`, supprimés au prochain démarrage.
    pub restes_old: usize,
}

fn dans_dossier(rel: &Path, dir: &str) -> bool {
    rel.components().next().is_some_and(|c| c.as_os_str().eq_ignore_ascii_case(dir))
}

/// `x.old`, ou `x.1.old`, `x.2.old`… : jamais un fichier existant (qui peut être à
/// l'utilisateur, ou une ancienne copie encore verrouillée).
fn free_old_name(dst: &Path) -> PathBuf {
    let base = dst.display().to_string();
    let first = PathBuf::from(format!("{base}{SUFFIXE_OLD}"));
    if !first.exists() {
        return first;
    }
    (1..).map(|i| PathBuf::from(format!("{base}.{i}{SUFFIXE_OLD}"))).find(|p| !p.exists()).expect("nom libre")
}

/// Copie les fichiers extraits dans `app_dir`, en renommant d'abord ceux qui existent
/// (`x.exe` → `x.exe.old`, noté dans `data\maj\anciens.txt`). Rien n'est écrit sous `data\`.
/// Un fichier verrouillé arrête l'installation avec une erreur en français ; les `.old` déjà
/// créés restent notés (supprimés au prochain démarrage).
pub fn apply(extracted: &Path, files: &[PathBuf], app_dir: &Path) -> Result<Bilan, String> {
    let mut bilan = Bilan::default();
    let mut olds: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<(), String> {
        for rel in files {
            if dans_dossier(rel, "data") {
                continue;
            }
            let src = extracted.join(rel);
            let dst = app_dir.join(rel);
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p).map_err(|e| format!("{} : {e}", p.display()))?;
            }
            let mut ancien = None;
            if dst.exists() {
                let old = free_old_name(&dst);
                std::fs::rename(&dst, &old).map_err(|e| {
                    format!(
                        "impossible de remplacer {} : le fichier est peut-être utilisé par un autre programme \
                         (antivirus, autre instance de ComicRust…). Fermez-le puis réessayez. Détail : {e}",
                        rel.display()
                    )
                })?;
                olds.push(old.strip_prefix(app_dir).map(Path::to_path_buf).unwrap_or_else(|_| old.clone()));
                ancien = Some(old);
            }
            if let Err(e) = std::fs::copy(&src, &dst) {
                // L'ancien fichier reprend sa place : jamais d'exe manquant.
                if let Some(old) = ancien {
                    let _ = std::fs::remove_file(&dst);
                    if std::fs::rename(&old, &dst).is_ok() {
                        olds.pop();
                    }
                }
                return Err(format!("impossible d'écrire {} (dossier en lecture seule ?) : {e}", rel.display()));
            }
            bilan.remplaces += 1;
        }
        Ok(())
    })();
    // Notés même après une erreur : les `.old` déjà créés seront supprimés au lancement suivant.
    record_olds(app_dir, &olds);
    bilan.restes_old = olds.len();
    result.map(|()| bilan)
}

fn old_list_path(app_dir: &Path) -> PathBuf {
    app_dir.join("data").join(DOSSIER_MAJ).join(LISTE_OLD)
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

/// Nettoyage au démarrage (avant la création de la fenêtre) : supprime les `.old` listés
/// dans `data\maj\anciens.txt` (ceux encore verrouillés restent listés pour le lancement
/// suivant) et les téléchargements. Jamais un `.old` de l'utilisateur ; sans liste, seuls
/// les restes de téléchargement (`data\maj\`) sont supprimés.
pub fn cleanup(app_dir: &Path) {
    let list = old_list_path(app_dir);
    let mut left: Vec<PathBuf> = Vec::new();
    if let Ok(text) = std::fs::read_to_string(&list) {
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let rel = PathBuf::from(line);
            // Uniquement des `.old` relatifs, sans remontée ni `data\` (liste écrite par nous).
            let safe = rel.is_relative()
                && rel.components().all(|c| matches!(c, std::path::Component::Normal(_)))
                && !dans_dossier(&rel, "data")
                && line.ends_with(SUFFIXE_OLD);
            if !safe {
                continue;
            }
            let p = app_dir.join(&rel);
            if p.exists() && std::fs::remove_file(&p).is_err() {
                left.push(rel);
            }
        }
    }
    let _ = std::fs::remove_dir_all(app_dir.join("data").join(DOSSIER_MAJ));
    record_olds(app_dir, &left);
}

// ─────────────────────────────── Relance ───────────────────────────────

/// Retire `--attendre-pid <pid>` (ou `--attendre-pid=<pid>`) des arguments ; renvoie le pid
/// trouvé (le dernier) et les autres arguments, dans leur ordre.
pub fn retirer_attendre_pid(args: impl IntoIterator<Item = String>) -> (Option<u32>, Vec<String>) {
    let mut pid = None;
    let mut reste = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == ARG_ATTENDRE_PID {
            if let Some(v) = it.next() {
                pid = v.trim().parse().ok().or(pid);
            }
        } else if let Some(v) = a.strip_prefix(&format!("{ARG_ATTENDRE_PID}=")) {
            pid = v.trim().parse().ok().or(pid);
        } else {
            reste.push(a);
        }
    }
    (pid, reste)
}

/// Arguments de la nouvelle instance : ceux de l'instance en cours (sans un ancien
/// `--attendre-pid`), puis `--attendre-pid <pid>`.
pub fn arguments_relance(args: impl IntoIterator<Item = String>, pid: u32) -> Vec<String> {
    let (_, mut reste) = retirer_attendre_pid(args);
    reste.push(ARG_ATTENDRE_PID.to_string());
    reste.push(pid.to_string());
    reste
}

/// Exe à relancer : `exe`, sans un `.old` final (après le renommage, certains systèmes
/// rendent le nouveau nom de l'exe en cours).
pub fn exe_a_relancer(exe: &Path) -> PathBuf {
    let s = exe.display().to_string();
    match s.strip_suffix(SUFFIXE_OLD) {
        Some(base) if !base.is_empty() => PathBuf::from(base),
        _ => exe.to_path_buf(),
    }
}

/// Relance `exe` avec les arguments `args` (sans le nom du programme) et `--attendre-pid`.
pub fn relancer(exe: &Path, args: impl IntoIterator<Item = String>) -> Result<(), String> {
    std::process::Command::new(exe_a_relancer(exe))
        .args(arguments_relance(args, std::process::id()))
        .spawn()
        .map(drop)
        .map_err(|e| format!("impossible de relancer ComicRust : {e}"))
}

/// À appeler tout au début : si la ligne de commande contient `--attendre-pid <pid>`, attend
/// (au plus [`DELAI_ATTENTE_PID`]) la fin de ce processus. Renvoie le pid attendu.
pub fn traiter_attendre_pid() -> Option<u32> {
    let (pid, _) = retirer_attendre_pid(std::env::args().skip(1));
    let pid = pid.filter(|p| *p != std::process::id())?;
    attendre_fin_processus(pid, DELAI_ATTENTE_PID);
    Some(pid)
}

/// Attend la fin du processus `pid` (au plus `delai`) ; vrai s'il est terminé (ou inconnu).
pub fn attendre_fin_processus(pid: u32, delai: Duration) -> bool {
    imp_attente::attendre(pid, delai)
}

#[cfg(all(windows, feature = "app"))]
mod imp_attente {
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};

    pub fn attendre(pid: u32, delai: Duration) -> bool {
        // SAFETY : appels Win32 sur un handle ouvert ici et refermé avant de sortir.
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if h.is_null() {
                // Processus déjà terminé (ou inaccessible) : rien à attendre.
                return true;
            }
            let ms = u32::try_from(delai.as_millis()).unwrap_or(u32::MAX);
            let r = WaitForSingleObject(h, ms);
            CloseHandle(h);
            r != WAIT_TIMEOUT
        }
    }
}

#[cfg(not(all(windows, feature = "app")))]
mod imp_attente {
    use std::time::{Duration, Instant};

    /// Hors Windows : sondage de `/proc/<pid>` (Linux) ; ailleurs, rien à attendre.
    pub fn attendre(pid: u32, delai: Duration) -> bool {
        let actif = || cfg!(target_os = "linux") && std::path::Path::new(&format!("/proc/{pid}")).exists();
        let debut = Instant::now();
        while actif() {
            if debut.elapsed() >= delai {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }
}

// ─────────────────────────────── Pages des releases ───────────────────────────────

/// Seules adresses que `maj_ouvrir_page` accepte (page de release, liens des notes de version).
pub const PREFIXE_PAGE_AUTORISEE: &str = "https://github.com/imtheyoyo/ComicRust/";

/// Vérifie qu'une adresse peut être ouverte dans le navigateur : `https://github.com/imtheyoyo/ComicRust/…`
/// uniquement (préfixe exact, sensible à la casse), sans espace, caractère de contrôle ni caractère
/// interprétable par un interpréteur de commandes. Renvoie l'adresse nettoyée (espaces de bord retirés).
pub fn verifier_url_page(url: &str) -> Result<String, String> {
    let url = url.trim();
    let refus = || format!("adresse refusée : seules les pages {PREFIXE_PAGE_AUTORISEE}… peuvent être ouvertes");
    if !url.starts_with(PREFIXE_PAGE_AUTORISEE) || url.len() > 2048 {
        return Err(refus());
    }
    let interdit = |c: char| {
        c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '\\' | '^' | '`' | '|' | '{' | '}')
    };
    if url.chars().any(interdit) {
        return Err(refus());
    }
    Ok(url.to_string())
}

/// Ouvre une page autorisée (voir [`verifier_url_page`]) dans le navigateur par défaut. Sous
/// Windows, l'adresse est passée telle quelle à `explorer.exe` (aucun interpréteur de commandes,
/// donc rien à échapper) ; ailleurs, `xdg-open`.
pub fn ouvrir_page(url: &str) -> Result<(), String> {
    let url = verifier_url_page(url)?;
    let programme = if cfg!(windows) { "explorer" } else { "xdg-open" };
    std::process::Command::new(programme)
        .arg(&url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("impossible d'ouvrir le navigateur : {e}"))
}

// ─────────────────────────────── Réglages et contrat IPC ───────────────────────────────

/// Changement demandé par `maj_regler` : champ absent = inchangé ; `versionIgnoree: null`
/// efface la version ignorée.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DemandeReglage {
    pub verifier_au_demarrage: Option<bool>,
    pub version_ignoree: Option<Option<String>>,
}

/// Lit les arguments de `maj_regler` depuis le JSON de l'appel (pour distinguer un champ
/// absent d'un `null`, ce que la désérialisation des arguments Tauri ne permet pas).
pub fn analyser_demande_reglage(v: &Value) -> Result<DemandeReglage, String> {
    let mut d = DemandeReglage::default();
    match v.get("verifierAuDemarrage") {
        None | Some(Value::Null) => {}
        Some(Value::Bool(b)) => d.verifier_au_demarrage = Some(*b),
        Some(_) => return Err("verifierAuDemarrage : booléen attendu".into()),
    }
    match v.get("versionIgnoree") {
        None => {}
        Some(Value::Null) => d.version_ignoree = Some(None),
        Some(Value::String(s)) if s.trim().is_empty() => d.version_ignoree = Some(None),
        Some(Value::String(s)) if s.len() <= 100 => d.version_ignoree = Some(Some(s.trim().to_string())),
        Some(_) => return Err("versionIgnoree : texte (tag de version) ou null attendu".into()),
    }
    Ok(d)
}

/// Statut de la mise à jour (`StatutMaj.etat`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EtatStatut {
    #[default]
    Inactif,
    Verification,
    AJour,
    Disponible,
    Installation,
    Installee,
    Echec,
}

/// Release vue par le front.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfosRelease {
    pub tag: String,
    pub nom: String,
    pub notes: String,
    pub url_page: String,
    /// Taille du zip en octets (0 sans paquet).
    pub taille: u64,
    /// Zip Windows et son `.sha256` présents : installation possible.
    pub a_paquet: bool,
}

/// `StatutMaj` du contrat (événement `maj://statut`, champ `statut` de `EtatMaj`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatutMaj {
    pub etat: EtatStatut,
    pub manuel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<InfosRelease>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erreur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bilan: Option<Bilan>,
}

impl StatutMaj {
    /// Statut sans release, erreur ni bilan.
    pub fn simple(etat: EtatStatut, manuel: bool) -> Self {
        Self { etat, manuel, ..Default::default() }
    }
}

/// `EtatMaj` du contrat (réponse de `maj_etat` et `maj_regler`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatMaj {
    pub version: String,
    pub version_comparee: String,
    pub verifier_au_demarrage: bool,
    pub version_ignoree: Option<String>,
    pub statut: StatutMaj,
    pub installable: bool,
}

/// Événements envoyés au front.
#[derive(Debug, Clone, PartialEq)]
pub enum EvenementMaj {
    Statut(StatutMaj),
    Progression { recu: u64, total: u64 },
}

impl EvenementMaj {
    /// Nom de l'événement Tauri et sa charge JSON.
    pub fn pour_le_front(&self) -> (&'static str, Value) {
        match self {
            EvenementMaj::Statut(s) => ("maj://statut", serde_json::json!(s)),
            EvenementMaj::Progression { recu, total } => {
                ("maj://progression", serde_json::json!({ "recu": recu, "total": total }))
            }
        }
    }
}

// ─────────────────────────────── Réseau (fonction `app`, tests) ───────────────────────────────

#[cfg(any(test, feature = "app"))]
pub use reseau::*;

#[cfg(any(test, feature = "app"))]
mod reseau {
    use super::*;
    use crate::reglages;
    use std::io::Write;
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::time::Instant;

    /// Intervalle minimal entre deux événements de progression (≤ 10 par seconde).
    const INTERVALLE_PROGRESSION: Duration = Duration::from_millis(100);

    fn est_local(url: &str) -> bool {
        let reste = url.split_once("://").map_or(url, |(_, r)| r);
        let hote = reste.split(['/', '?']).next().unwrap_or("");
        let hote = hote.rsplit_once(':').map_or(hote, |(h, _)| h);
        matches!(hote, "127.0.0.1" | "localhost" | "[::1]")
    }

    fn agent(url: &str) -> ureq::Agent {
        let mut cfg = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .user_agent(concat!("ComicRust-Tauri/", env!("CARGO_PKG_VERSION")))
            // Certificats du système (magasin Windows) : fonctionne derrière un antivirus
            // ou un proxy d'entreprise qui inspecte le HTTPS.
            .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
            // Redirections suivies à la main (voir `get`).
            .max_redirects(0);
        if est_local(url) {
            // Faux serveur de test sur la machine : jamais par un proxy.
            cfg = cfg.proxy(None);
        }
        cfg.build().into()
    }

    /// Adresse absolue d'une redirection (`Location` relative acceptée).
    fn suivre(base: &str, location: &str) -> String {
        if location.contains("://") {
            return location.to_string();
        }
        let (schema, reste) = base.split_once("://").unwrap_or(("https", base));
        let hote = reste.split('/').next().unwrap_or("");
        if location.starts_with('/') {
            format!("{schema}://{hote}{location}")
        } else {
            let dossier = base.rsplit_once('/').map_or(base, |(d, _)| d);
            format!("{dossier}/{location}")
        }
    }

    /// GET qui suit les redirections sans lire le corps des réponses 302 : GitHub renvoie
    /// ses fichiers de release par une 302 vers release-assets.githubusercontent.com, et
    /// certains proxys tronquent ce corps vide, ce qui fait échouer la lecture.
    fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
        let mut url = url.to_string();
        for _ in 0..6 {
            let resp = agent(&url).get(&url).call().map_err(|e| e.to_string())?;
            if !resp.status().is_redirection() {
                return Ok(resp);
            }
            let location =
                resp.headers().get("location").and_then(|v| v.to_str().ok()).ok_or("redirection sans adresse")?;
            url = suivre(&url, location);
        }
        Err("trop de redirections".into())
    }

    fn lire_texte(url: &str) -> Result<String, String> {
        get(url)?.into_body().read_to_string().map_err(|e| e.to_string())
    }

    /// Interroge l'API (`url_api`) et choisit la release Tauri plus récente que `current`.
    pub fn chercher(url_api: &str, current: &str) -> Result<Option<Release>, String> {
        let json = lire_texte(url_api).map_err(|e| format!("GitHub injoignable : {e}"))?;
        pick_release(&json, current)
    }

    /// Télécharge `url` dans `dest` (via `dest.part`, renommé à la fin) ; `progression`
    /// reçoit le total d'octets reçus après chaque bloc.
    fn download(url: &str, dest: &Path, progression: &mut dyn FnMut(u64)) -> Result<(), String> {
        let resp = get(url).map_err(|e| format!("téléchargement impossible : {e}"))?;
        let mut reader = resp.into_body().into_reader();
        let tmp = dest.with_extension("part");
        let mut out = std::fs::File::create(&tmp).map_err(|e| format!("{} : {e}", tmp.display()))?;
        let mut buf = vec![0u8; 1 << 16];
        let mut recu = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("téléchargement interrompu : {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            recu += n as u64;
            progression(recu);
        }
        drop(out);
        std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
    }

    /// Télécharge, vérifie et installe une release dans `app_dir` (travail dans `work`,
    /// normalement `data\maj`). `progression` reçoit les octets reçus du zip.
    pub fn installer_release(
        release: &Release,
        app_dir: &Path,
        work: &Path,
        progression: &mut dyn FnMut(u64),
    ) -> Result<Bilan, String> {
        let (zip_url, _) = release.zip.as_ref().ok_or("cette version n'a pas de paquet Windows")?;
        let sha_url = release.sha256_url.as_ref().ok_or("empreinte SHA-256 absente de la release")?;
        std::fs::create_dir_all(work).map_err(|e| format!("{} : {e}", work.display()))?;
        let expected = lire_texte(sha_url)
            .map_err(|e| format!("empreinte introuvable : {e}"))
            .and_then(|t| parse_sha256_file(&t).ok_or_else(|| "empreinte SHA-256 illisible".to_string()))?;
        let zip_path = work.join(format!("ComicRust-Tauri-{}.zip", release.tag));
        download(zip_url, &zip_path, progression)?;
        let actual = sha256_of(&zip_path).map_err(|e| e.to_string())?;
        if actual != expected {
            let _ = std::fs::remove_file(&zip_path);
            return Err(
                "le fichier téléchargé est corrompu (empreinte SHA-256 différente) : rien n'a été modifié".into()
            );
        }
        let extracted = work.join(&release.tag);
        let nettoyer = || {
            let _ = std::fs::remove_file(&zip_path);
            let _ = std::fs::remove_dir_all(&extracted);
        };
        let files = match extract(&zip_path, &extracted) {
            Ok(f) => f,
            Err(e) => {
                nettoyer();
                return Err(format!("{e} : rien n'a été modifié"));
            }
        };
        if !contient_exe_principal(&files) {
            nettoyer();
            return Err(format!("paquet inattendu ({EXE_PRINCIPAL} absent) : rien n'a été modifié"));
        }
        let res = apply(&extracted, &files, app_dir);
        // Le zip et sa copie extraite ne servent plus (la liste des `.old`, dans le même
        // dossier, sert au lancement suivant).
        nettoyer();
        res
    }

    // ─────────────────────────── État partagé (commandes IPC) ───────────────────────────

    /// Sortie des événements (vers `AppHandle::emit` dans l'application).
    pub type SortieMaj = Arc<dyn Fn(EvenementMaj) + Send + Sync>;

    struct Interne {
        statut: StatutMaj,
        /// Release de la dernière vérification (adresses complètes).
        release: Option<Release>,
    }

    /// État de la mise à jour, partagé par les commandes IPC (géré par Tauri).
    #[derive(Clone)]
    pub struct Maj {
        interne: Arc<Mutex<Interne>>,
        data: PathBuf,
        app_dir: PathBuf,
        url_api: String,
        version_comparee: String,
        installable: bool,
        /// Exe lancé (relevé avant toute installation, qui le renomme en `.old`).
        exe: Option<PathBuf>,
        sortie: SortieMaj,
    }

    impl Maj {
        /// `data` : dossier `data\` ; `app_dir` : dossier de l'exe ; `installable` : voir
        /// [`installable`](super::installable).
        pub fn nouveau(data: PathBuf, app_dir: PathBuf, installable: bool, sortie: SortieMaj) -> Self {
            Self {
                interne: Arc::new(Mutex::new(Interne { statut: StatutMaj::default(), release: None })),
                data,
                app_dir,
                url_api: url_api(),
                version_comparee: version_comparee(),
                installable,
                exe: std::env::current_exe().ok(),
                sortie,
            }
        }

        /// Remplace l'adresse de l'API (tests).
        pub fn avec_url(mut self, url: impl Into<String>) -> Self {
            self.url_api = url.into();
            self
        }

        fn verrou(&self) -> MutexGuard<'_, Interne> {
            self.interne.lock().unwrap_or_else(|e| e.into_inner())
        }

        fn reglages(&self) -> reglages::ReglagesMaj {
            // Lecture tolérante : un fichier illisible donne les valeurs par défaut.
            reglages::lire(&self.data).map(|r| r.maj).unwrap_or_default()
        }

        /// Change le statut ; l'émet si `emettre`.
        fn poser(&self, statut: StatutMaj, emettre: bool) {
            self.verrou().statut = statut.clone();
            if emettre {
                (self.sortie)(EvenementMaj::Statut(statut));
            }
        }

        /// Statut courant.
        pub fn statut(&self) -> StatutMaj {
            self.verrou().statut.clone()
        }

        /// `maj_etat`.
        pub fn etat(&self) -> EtatMaj {
            let r = self.reglages();
            EtatMaj {
                version: VERSION_ACTUELLE.to_string(),
                version_comparee: self.version_comparee.clone(),
                verifier_au_demarrage: r.verifier_au_demarrage.unwrap_or(true),
                version_ignoree: r.version_ignoree,
                statut: self.statut(),
                installable: self.installable,
            }
        }

        /// `maj_regler` : enregistre les champs fournis ; renvoie l'état à jour.
        pub fn regler(&self, d: DemandeReglage) -> Result<EtatMaj, String> {
            if d.verifier_au_demarrage.is_some() || d.version_ignoree.is_some() {
                reglages::modifier(&self.data, |r| {
                    if let Some(v) = d.verifier_au_demarrage {
                        r.maj.verifier_au_demarrage = Some(v);
                    }
                    if let Some(v) = d.version_ignoree {
                        r.maj.version_ignoree = v;
                    }
                })?;
            }
            Ok(self.etat())
        }

        /// Vérification automatique au démarrage, si le réglage est actif (vrai par défaut).
        pub fn verifier_au_demarrage(&self) {
            if self.reglages().verifier_au_demarrage.unwrap_or(true) {
                self.verifier(false);
            }
        }

        /// `maj_verifier` : lance la vérification en arrière-plan et rend la main. Sans effet
        /// pendant une vérification ou une installation. Automatique (`manuel` faux) : seul
        /// `disponible` (version non ignorée) est émis ; « à jour » et les erreurs restent
        /// silencieux (statut `inactif`).
        pub fn verifier(&self, manuel: bool) {
            {
                let mut g = self.verrou();
                let etat = g.statut.etat;
                match etat {
                    EtatStatut::Installation => return,
                    EtatStatut::Verification => {
                        // Une seule vérification à la fois : une demande manuelle reçue pendant une
                        // vérification automatique s'y rattache (son résultat sera montré).
                        if manuel && !g.statut.manuel {
                            g.statut.manuel = true;
                            drop(g);
                            (self.sortie)(EvenementMaj::Statut(StatutMaj::simple(EtatStatut::Verification, true)));
                        }
                        return;
                    }
                    _ => g.statut = StatutMaj::simple(EtatStatut::Verification, manuel),
                }
            }
            if manuel {
                (self.sortie)(EvenementMaj::Statut(StatutMaj::simple(EtatStatut::Verification, true)));
            }
            let moi = self.clone();
            std::thread::spawn(move || {
                let resultat = chercher(&moi.url_api, &moi.version_comparee);
                // `manuel` relu : une demande manuelle a pu se rattacher entre-temps.
                let manuel = {
                    let g = moi.verrou();
                    g.statut.etat == EtatStatut::Verification && g.statut.manuel
                };
                moi.fin_verification(resultat, manuel);
            });
        }

        fn fin_verification(&self, resultat: Result<Option<Release>, String>, manuel: bool) {
            match resultat {
                Ok(Some(rel)) => {
                    let ignoree = self.reglages().version_ignoree.is_some_and(|v| meme_version(&v, &rel.tag));
                    let infos = rel.infos();
                    self.verrou().release = Some(rel);
                    if manuel || !ignoree {
                        let statut = StatutMaj {
                            etat: EtatStatut::Disponible,
                            manuel,
                            release: Some(infos),
                            ..Default::default()
                        };
                        self.poser(statut, true);
                    } else {
                        self.poser(StatutMaj::default(), false);
                    }
                }
                Ok(None) if manuel => self.poser(StatutMaj::simple(EtatStatut::AJour, true), true),
                Err(e) if manuel => {
                    self.poser(StatutMaj { erreur: Some(e), ..StatutMaj::simple(EtatStatut::Echec, true) }, true)
                }
                _ => self.poser(StatutMaj::default(), false),
            }
        }

        /// `maj_installer` : télécharge, vérifie et installe la release `tag` (celle de la
        /// dernière vérification) en arrière-plan ; progression et résultat par événements.
        pub fn installer(&self, tag: &str) -> Result<(), String> {
            if !self.installable {
                return Err("installation automatique indisponible ici (Windows et version livrée uniquement) : \
                            télécharger la nouvelle version depuis la page de la release"
                    .into());
            }
            let release = {
                let mut g = self.verrou();
                if matches!(g.statut.etat, EtatStatut::Verification | EtatStatut::Installation) {
                    return Err("une vérification ou une installation est déjà en cours".into());
                }
                if g.statut.etat == EtatStatut::Installee {
                    return Err("mise à jour déjà installée : redémarrer ComicRust".into());
                }
                let rel = g
                    .release
                    .clone()
                    .filter(|r| r.tag == tag)
                    .ok_or_else(|| format!("version {tag} inconnue : relancer la vérification"))?;
                if rel.zip.is_none() || rel.sha256_url.is_none() {
                    return Err("cette version n'a pas de paquet Windows (ou pas d'empreinte SHA-256)".into());
                }
                g.statut = StatutMaj {
                    etat: EtatStatut::Installation,
                    manuel: true,
                    release: Some(rel.infos()),
                    ..Default::default()
                };
                rel
            };
            (self.sortie)(EvenementMaj::Statut(self.statut()));
            let moi = self.clone();
            std::thread::spawn(move || {
                let total = release.zip.as_ref().map_or(0, |(_, t)| *t);
                let mut dernier: Option<Instant> = None;
                let mut recu_final = 0u64;
                let mut progression = |recu: u64| {
                    recu_final = recu;
                    if dernier.is_none_or(|t| t.elapsed() >= INTERVALLE_PROGRESSION) {
                        dernier = Some(Instant::now());
                        (moi.sortie)(EvenementMaj::Progression { recu, total: total.max(recu) });
                    }
                };
                let work = moi.app_dir.join("data").join(DOSSIER_MAJ);
                let resultat = installer_release(&release, &moi.app_dir, &work, &mut progression);
                // Dernier point de progression toujours envoyé (100 %).
                if recu_final > 0 {
                    (moi.sortie)(EvenementMaj::Progression { recu: recu_final, total: total.max(recu_final) });
                }
                let infos = Some(release.infos());
                let statut = match resultat {
                    Ok(bilan) => StatutMaj {
                        etat: EtatStatut::Installee,
                        manuel: true,
                        release: infos,
                        bilan: Some(bilan),
                        ..Default::default()
                    },
                    Err(e) => StatutMaj {
                        etat: EtatStatut::Echec,
                        manuel: true,
                        release: infos,
                        erreur: Some(e),
                        ..Default::default()
                    },
                };
                moi.poser(statut, true);
            });
            Ok(())
        }

        /// `maj_redemarrer` : relance l'exe (mêmes arguments, `--attendre-pid`). L'appelant
        /// ferme ensuite l'application.
        pub fn redemarrer(&self) -> Result<(), String> {
            let exe = match &self.exe {
                Some(e) => e.clone(),
                None => std::env::current_exe().map_err(|e| e.to_string())?,
            };
            relancer(&exe, std::env::args().skip(1))
        }
    }
}

#[cfg(test)]
#[path = "tests_maj.rs"]
mod tests;
