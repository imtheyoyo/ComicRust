//! Réglages régionaux du poste, pour écrire des valeurs que ComicRack relit avec la
//! culture de l'utilisateur.
//!
//! ComicRack lit les valeurs numériques des critères de listes intelligentes avec
//! `float.TryParse(value, out …)` (`ComicBookNumericMatcher`), c'est-à-dire avec la
//! culture courante de Windows : « 4,5 » sur un poste français, « 4.5 » sur un poste
//! anglais. Écrite avec le mauvais séparateur, la valeur vaut -1 (ou 45 : en anglais, la
//! virgule est un séparateur de milliers).

/// Séparateur décimal de l'utilisateur : sous Windows, celui des réglages régionaux
/// (`GetLocaleInfoEx(LOCALE_NAME_USER_DEFAULT, LOCALE_SDECIMAL)`) ; ailleurs, d'après la
/// langue (`LC_ALL`, `LC_NUMERIC`, `LANG`) : la virgule en français, sinon le point.
pub fn decimal_separator() -> char {
    #[cfg(windows)]
    {
        windows_decimal_separator().unwrap_or('.')
    }
    #[cfg(not(windows))]
    {
        let lang = ["LC_ALL", "LC_NUMERIC", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.trim().is_empty());
        separator_for_language(lang.as_deref())
    }
}

/// Séparateur décimal d'après un nom de langue (`fr_FR.UTF-8`, `fr-FR`, `en_US`…) : la
/// virgule en français, sinon le point.
pub fn separator_for_language(lang: Option<&str>) -> char {
    match lang {
        Some(l) if l.trim().to_ascii_lowercase().starts_with("fr") => ',',
        _ => '.',
    }
}

#[cfg(windows)]
fn windows_decimal_separator() -> Option<char> {
    use windows::core::PCWSTR;
    use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SDECIMAL};
    let mut buf = [0u16; 8];
    // SAFETY : `PCWSTR::null()` vaut `LOCALE_NAME_USER_DEFAULT` ; le tampon est fourni
    // avec sa taille, la fonction renvoie le nombre de caractères écrits (0 : échec).
    let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_SDECIMAL, Some(&mut buf)) };
    if n <= 1 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len]).chars().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separator_by_language() {
        assert_eq!(separator_for_language(Some("fr_FR.UTF-8")), ',');
        assert_eq!(separator_for_language(Some("fr-BE")), ',');
        assert_eq!(separator_for_language(Some("en_US.UTF-8")), '.');
        assert_eq!(separator_for_language(None), '.');
        assert!(!decimal_separator().is_alphanumeric());
    }
}
