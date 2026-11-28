//! Emplacements portables. Comme la version egui, ComicRust-Tauri est **exclusivement
//! portable** : tout est dans `data\` à côté de l'exécutable, rien dans `%APPDATA%`, ni dans
//! `%LOCALAPPDATA%`, ni dans le registre. Le profil du WebView2 (cache, cookies, stockage local)
//! est redirigé vers `data\webview2`.

use std::path::{Path, PathBuf};

/// Dossier des thèmes CSS de l'utilisateur.
pub const THEMES_DIR: &str = "themes";
/// Profil WebView2.
pub const WEBVIEW_DIR: &str = "webview2";

/// Dossier de l'exécutable.
pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."))
}

/// `data\` à côté de l'exécutable, créé si besoin.
///
/// Échec franc si l'emplacement de l'exécutable est introuvable : jamais de repli sur un
/// chemin relatif (les données iraient dans le dossier courant, profil WebView2 compris).
pub fn data_dir() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dossier = exe
        .parent()
        .filter(|d| d.is_absolute())
        .ok_or_else(|| std::io::Error::other("dossier de l'exécutable introuvable"))?;
    let dir = dossier.join("data");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `data\themes`.
pub fn dossier_themes(data: &Path) -> PathBuf {
    data.join(THEMES_DIR)
}

/// `data\webview2`.
pub fn dossier_webview(data: &Path) -> PathBuf {
    data.join(WEBVIEW_DIR)
}

// ───────────────────────────── Instance unique ─────────────────────────────
//
// Une seule instance par dossier `data\` (exe normal et exe `-mcp` compris) : deux instances sur
// les mêmes fichiers perdaient des écritures (avancement, réglages). L'identifiant Tauri de
// `tauri.conf.json` n'est **pas** modifié (il pourrait servir à des chemins hors de `data\`) : la
// clé propre au dossier ([`cle_instance`]) ne sert qu'au mutex nommé et au canal nommé ci-dessous.
//
// - [`premiere_instance`] : mutex nommé `Local\<clé>-premiere`, gardé jusqu'à la fin du processus ;
// - [`ecouter`] : la première instance ouvre le canal nommé `\\.\pipe\<clé>-transmission` (lecture
//   seule, refus des clients distants) et reçoit les arguments des lancements suivants ;
// - [`transmettre`] : un lancement suivant y écrit son dossier courant et ses arguments
//   ([`encoder_transmission`]) puis s'arrête.
//
// Le greffon `tauri-plugin-single-instance` n'est plus utilisé : il construit ses noms de mutex et
// de fenêtre à partir de `config.identifier`, ce qui obligeait à changer l'identifiant de
// l'application au lancement.

/// Identifiant d'instance propre au dossier `data\` : deux copies portables dans des dossiers
/// différents restent indépendantes, mais deux lancements sur le même `data\` (exe normal et exe
/// `-mcp` compris) sont une seule instance. `identifiant` : celui de `tauri.conf.json`.
///
/// Le dossier est rendu canonique avant le hachage (lettre de lecteur, `..`, liens, chemin court
/// `8.3`, préfixe `\\?\` retiré) ; si c'est impossible, le texte du chemin est haché tel quel.
pub fn cle_instance(identifiant: &str, data: &Path) -> String {
    // FNV-1a 64 bits du chemin en minuscules (Windows ignore la casse) : court et stable.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in texte_canonique(data).to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{identifiant}.{h:016x}")
}

/// Chemin canonique en texte, sans le préfixe de chemin étendu de Windows (`\\?\C:\…` → `C:\…`,
/// `\\?\UNC\serveur\…` → `\\serveur\…`). Repli sur le texte du chemin si `canonicalize` échoue.
pub fn texte_canonique(chemin: &Path) -> String {
    let canonique = std::fs::canonicalize(chemin).unwrap_or_else(|_| chemin.to_path_buf());
    sans_prefixe_etendu(&canonique.to_string_lossy())
}

/// Retire le préfixe de chemin étendu de Windows (`\\?\`, `\\?\UNC\`).
pub fn sans_prefixe_etendu(texte: &str) -> String {
    if let Some(reste) = texte.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{reste}")
    } else {
        texte.strip_prefix(r"\\?\").unwrap_or(texte).to_string()
    }
}

/// Garde de la première instance (tenue jusqu'à la fin du processus).
pub struct GardeInstance {
    #[cfg(all(windows, feature = "app"))]
    _poignee: isize,
}

/// Vrai si ce processus est la **première** instance sur ce `data\` (mutex nommé `cle`, gardé tant
/// que le processus vit). Une deuxième instance ne doit rien nettoyer (`maj::cleanup` supprimerait le
/// téléchargement en cours de l'autre) : elle transmet son fichier ([`transmettre`]) et s'arrête.
/// Hors Windows : toujours vrai (développement).
pub fn premiere_instance(cle: &str) -> (bool, GardeInstance) {
    imp_instance::prendre(cle)
}

/// Taille maximale d'une transmission (dossier courant et arguments) : au-delà, elle est tronquée
/// et refusée par [`decoder_transmission`].
pub const TAILLE_MAX_TRANSMISSION: usize = 64 * 1024;

/// Délai pendant lequel un lancement suivant essaie de joindre la première instance (qui peut
/// être encore en train de démarrer).
pub const DELAI_TRANSMISSION: std::time::Duration = std::time::Duration::from_secs(10);

/// Message d'un lancement suivant : dossier courant puis arguments (programme compris), séparés
/// par un caractère nul (impossible dans un chemin ou un argument Windows), en UTF-8.
pub fn encoder_transmission(dossier_courant: &str, args: &[String]) -> Vec<u8> {
    let mut texte = dossier_courant.replace('\0', "");
    for a in args {
        texte.push('\0');
        texte.push_str(&a.replace('\0', ""));
    }
    texte.into_bytes()
}

/// Inverse de [`encoder_transmission`] : `(dossier courant, arguments)`. `None` si le message est
/// vide, trop long ou n'est pas de l'UTF-8.
pub fn decoder_transmission(octets: &[u8]) -> Option<(String, Vec<String>)> {
    if octets.is_empty() || octets.len() > TAILLE_MAX_TRANSMISSION {
        return None;
    }
    let texte = std::str::from_utf8(octets).ok()?;
    let mut parties = texte.split('\0').map(str::to_string);
    let dossier = parties.next()?;
    Some((dossier, parties.collect()))
}

/// Rappel de la première instance pour chaque transmission reçue (octets bruts).
pub type RappelTransmission = Box<dyn Fn(Vec<u8>) + Send + 'static>;

/// Première instance : reçoit, dans un fil à part, les transmissions des lancements suivants.
/// Si le canal existe déjà (ancienne instance pas encore fermée après une mise à jour), nouvel
/// essai toutes les demi-secondes pendant deux minutes. Hors Windows : sans effet.
pub fn ecouter(cle: &str, rappel: RappelTransmission) {
    imp_instance::ecouter(cle, rappel)
}

/// Lancement suivant : transmet `message` à la première instance (nouveaux essais pendant
/// [`DELAI_TRANSMISSION`]). Hors Windows : toujours une erreur (pas d'instance unique).
pub fn transmettre(cle: &str, message: &[u8]) -> Result<(), String> {
    imp_instance::transmettre(cle, message)
}

#[cfg(all(windows, feature = "app"))]
mod imp_instance {
    use super::{GardeInstance, RappelTransmission, DELAI_TRANSMISSION, TAILLE_MAX_TRANSMISSION};
    use std::io::{Read, Write};
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{
        GetLastError, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

    fn large(texte: &str) -> Vec<u16> {
        texte.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn nom_canal(cle: &str) -> String {
        format!(r"\\.\pipe\{cle}-transmission")
    }

    pub fn prendre(cle: &str) -> (bool, GardeInstance) {
        let nom = large(&format!("Local\\{cle}-premiere"));
        // SAFETY : nom terminé par un zéro, vivant pendant l'appel ; la poignée est volontairement
        // gardée jusqu'à la fin du processus (le système la libère alors).
        let (h, existe) = unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, nom.as_ptr());
            (h, GetLastError() == ERROR_ALREADY_EXISTS)
        };
        (h.is_null() || !existe, GardeInstance { _poignee: h as isize })
    }

    pub fn ecouter(cle: &str, rappel: RappelTransmission) {
        let nom = large(&nom_canal(cle));
        let lancement = std::thread::Builder::new().name("instance-unique".into()).spawn(move || {
            let mut premiere = true;
            let debut = Instant::now();
            loop {
                // Première ouverture : FILE_FLAG_FIRST_PIPE_INSTANCE refuse un canal déjà créé par
                // un autre processus (ancienne instance encore ouverte, ou usurpation).
                let ouverture = PIPE_ACCESS_INBOUND | if premiere { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
                // SAFETY : nom terminé par un zéro et vivant pendant l'appel ; sécurité par défaut.
                let h = unsafe {
                    CreateNamedPipeW(
                        nom.as_ptr(),
                        ouverture,
                        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                        PIPE_UNLIMITED_INSTANCES,
                        0,
                        TAILLE_MAX_TRANSMISSION as u32,
                        0,
                        std::ptr::null(),
                    )
                };
                if h == INVALID_HANDLE_VALUE {
                    if premiere && debut.elapsed() < Duration::from_secs(120) {
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    eprintln!("ComicRust : canal de l'instance unique indisponible (fichiers des autres lancements non reçus)");
                    return;
                }
                premiere = false;
                // SAFETY : `h` est une poignée de canal valide.
                let connecte =
                    unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) != 0 || GetLastError() == ERROR_PIPE_CONNECTED };
                // SAFETY : `h` est valide et n'appartient qu'à ce fil : le `File` la ferme.
                let fichier = unsafe { std::fs::File::from_raw_handle(h as _) };
                if connecte {
                    let mut octets = Vec::new();
                    // Le client écrit puis ferme : fin de lecture (canal rompu = fin de fichier).
                    if fichier.take(TAILLE_MAX_TRANSMISSION as u64 + 1).read_to_end(&mut octets).is_ok() {
                        rappel(octets);
                    }
                }
            }
        });
        if let Err(e) = lancement {
            eprintln!("ComicRust : fil de l'instance unique non démarré ({e})");
        }
    }

    pub fn transmettre(cle: &str, message: &[u8]) -> Result<(), String> {
        let nom = nom_canal(cle);
        let debut = Instant::now();
        loop {
            match std::fs::OpenOptions::new().write(true).open(&nom) {
                Ok(mut f) => {
                    // La première instance pourra passer au premier plan (Windows ne le permet
                    // qu'au processus qui a reçu l'action de l'utilisateur, celui-ci).
                    let mut pid = 0u32;
                    // SAFETY : poignée valide du canal ouvert ; `pid` vit pendant l'appel.
                    unsafe {
                        if GetNamedPipeServerProcessId(f.as_raw_handle() as _, &mut pid) != 0 && pid != 0 {
                            AllowSetForegroundWindow(pid);
                        }
                    }
                    return f.write_all(message).and_then(|()| f.flush()).map_err(|e| e.to_string());
                }
                Err(e) if debut.elapsed() >= DELAI_TRANSMISSION => return Err(e.to_string()),
                // Canal pas encore créé (instance en train de démarrer) ou occupé : nouvel essai.
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

#[cfg(not(all(windows, feature = "app")))]
mod imp_instance {
    use super::{GardeInstance, RappelTransmission};

    pub fn prendre(_cle: &str) -> (bool, GardeInstance) {
        (true, GardeInstance {})
    }

    pub fn ecouter(_cle: &str, _rappel: RappelTransmission) {}

    pub fn transmettre(_cle: &str, _message: &[u8]) -> Result<(), String> {
        Err("instance unique disponible sous Windows seulement".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sous_dossiers_dans_data() {
        let d = Path::new("data");
        assert_eq!(dossier_themes(d), Path::new("data").join("themes"));
        assert_eq!(dossier_webview(d), Path::new("data").join("webview2"));
    }

    #[test]
    fn cle_d_instance_par_dossier_data() {
        let a = cle_instance("fr.imtheyoyo.comicrust", Path::new("D:/BD/ComicRust/data"));
        assert_eq!(a, cle_instance("fr.imtheyoyo.comicrust", Path::new("d:/bd/comicrust/DATA")), "casse ignorée");
        assert_ne!(a, cle_instance("fr.imtheyoyo.comicrust", Path::new("E:/ComicRust/data")));
        assert!(a.starts_with("fr.imtheyoyo.comicrust.") && a.len() == "fr.imtheyoyo.comicrust.".len() + 16);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'), "nom de mutex sûr : {a}");
        assert!(premiere_instance(&a).0, "hors Windows : toujours la première");
    }

    /// Clé calculée sur le dossier canonique : `..`, lien symbolique et préfixe `\\?\` sans effet ;
    /// repli sur le texte si le dossier n'existe pas.
    #[test]
    fn cle_d_instance_sur_le_dossier_canonique() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("ComicRust").join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(tmp.path().join("autre")).unwrap();
        let detour = tmp.path().join("autre").join("..").join("ComicRust").join("data");
        let id = "fr.imtheyoyo.comicrust";
        assert_eq!(cle_instance(id, &data), cle_instance(id, &detour), "« .. » sans effet");
        #[cfg(unix)]
        {
            let lien = tmp.path().join("lien");
            std::os::unix::fs::symlink(tmp.path().join("ComicRust"), &lien).unwrap();
            assert_eq!(cle_instance(id, &data), cle_instance(id, &lien.join("data")), "lien suivi");
        }
        // Dossier absent : le texte du chemin (stable), différent d'un autre dossier.
        let absent = tmp.path().join("absent").join("data");
        assert_eq!(cle_instance(id, &absent), cle_instance(id, &absent));
        assert_ne!(cle_instance(id, &absent), cle_instance(id, &data));
        assert_eq!(sans_prefixe_etendu(r"\\?\C:\BD\data"), r"C:\BD\data");
        assert_eq!(sans_prefixe_etendu(r"\\?\UNC\nas\bd\data"), r"\\nas\bd\data");
        assert_eq!(sans_prefixe_etendu(r"D:\BD\data"), r"D:\BD\data");
    }

    /// Message transmis par un lancement suivant : aller-retour, arguments avec espaces, `|`,
    /// accents ; message vide, trop long ou non UTF-8 refusé.
    #[test]
    fn transmission_d_un_lancement_suivant() {
        let args = vec!["C:\\ComicRust\\ComicRust-Tauri.exe".to_string(), "D:\\BD\\Thanos | Été 01.cbz".to_string()];
        let m = encoder_transmission("C:\\Users\\yo", &args);
        assert_eq!(decoder_transmission(&m), Some(("C:\\Users\\yo".to_string(), args)));
        assert_eq!(decoder_transmission(&encoder_transmission("C:\\", &[])), Some(("C:\\".to_string(), vec![])));
        assert_eq!(decoder_transmission(b""), None);
        assert_eq!(decoder_transmission(&[0xff, 0xfe]), None);
        assert_eq!(decoder_transmission(&vec![b'a'; TAILLE_MAX_TRANSMISSION + 1]), None);
        assert!(transmettre("x", b"y").is_err(), "hors Windows : pas d'instance unique");
    }
}
