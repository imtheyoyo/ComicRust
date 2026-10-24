//! Mots de passe enregistrés dans les réglages (`data\comicrust.ron`) : **jamais en clair**.
//!
//! Sous Windows, ils sont chiffrés avec la protection de données de Windows (DPAPI,
//! `CryptProtectData`), liée au compte Windows de l'utilisateur. Seul ce compte, sur ce PC,
//! peut les relire. Dans le fichier, ils apparaissent sous la forme `dpapi:<hexadécimal>`.
//!
//! Conséquence : ComicRust copié sur un autre PC ou ouvert par un autre compte retrouve
//! tous ses réglages, sauf ce mot de passe, qu'il faut saisir à nouveau.
//!
//! Ailleurs que sous Windows, aucun coffre n'est disponible : le mot de passe n'est **pas
//! enregistré** et doit être saisi à chaque lancement.
//!
//! Les réglages d'une version précédente (mot de passe en clair) sont relus, puis réécrits
//! chiffrés au premier enregistrement.

use serde::{Deserialize, Deserializer, Serializer};

const PREFIX: &str = "dpapi:";

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Forme enregistrée d'un mot de passe (vide si impossible à protéger).
pub fn protect(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    match dpapi::protect(plain.as_bytes()) {
        Some(b) => format!("{PREFIX}{}", to_hex(&b)),
        None => String::new(),
    }
}

/// Mot de passe d'après sa forme enregistrée. Illisible (autre PC, autre compte Windows) :
/// vide, il sera redemandé. Ancienne forme en clair : acceptée telle quelle.
pub fn unprotect(stored: &str) -> String {
    match stored.strip_prefix(PREFIX) {
        Some(hex) => {
            from_hex(hex).and_then(|b| dpapi::unprotect(&b)).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default()
        }
        None => stored.to_string(),
    }
}

/// `#[serde(serialize_with = "crate::secret::serialize")]`
pub fn serialize<S: Serializer>(plain: &str, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&protect(plain))
}

/// `#[serde(deserialize_with = "crate::secret::deserialize")]`
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(unprotect(&String::deserialize(d)?))
}

#[cfg(windows)]
mod dpapi {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn run(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        // SAFETY : `input` pointe sur `data`, valide pendant l'appel ; `out` est alloué par
        // Windows (LocalAlloc) et libéré ci-dessous après copie.
        unsafe {
            let ok = if encrypt {
                CryptProtectData(
                    &input,
                    windows::core::w!("ComicRust"),
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            } else {
                CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            };
            ok.ok()?;
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
            Some(bytes)
        }
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

#[cfg(not(windows))]
mod dpapi {
    /// Pas de coffre : rien n'est enregistré.
    pub fn protect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }

    pub fn unprotect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_written_in_clear() {
        let stored = protect("T3st;pw");
        assert!(!stored.contains("T3st"), "{stored}");
        if cfg!(windows) {
            assert!(stored.starts_with(PREFIX));
            assert_eq!(unprotect(&stored), "T3st;pw", "même compte Windows : relu");
        } else {
            assert_eq!(stored, "", "sans coffre : pas enregistré");
        }
        // Réglages d'une version précédente : relus.
        assert_eq!(unprotect("ancien"), "ancien");
        // Chiffré ailleurs (autre PC) ou abîmé : redemandé.
        assert_eq!(unprotect("dpapi:zz"), "");
        assert_eq!(protect(""), "");
        assert_eq!(from_hex(&to_hex(&[0, 15, 255])), Some(vec![0, 15, 255]));
    }

    #[test]
    fn db_config_settings() {
        let cfg = crate::db::DbConfig {
            host: "nas".into(),
            port: 1236,
            user: "rke".into(),
            password: "secret-tres-long".into(),
            database: "base".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("secret-tres-long"), "{json}");
        let back: crate::db::DbConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.host, "nas");
        if cfg!(windows) {
            assert_eq!(back.password, "secret-tres-long");
        }
        let old: crate::db::DbConfig =
            serde_json::from_str(r#"{"host":"nas","port":1236,"user":"rke","password":"clair","database":"base"}"#)
                .unwrap();
        assert_eq!(old.password, "clair", "réglages v0.7.2 relus");
    }
}
