//! Polices de l'interface : celles d'egui, complétées pour que chaque icône s'affiche.
//!
//! Depuis egui 0.37, les polices d'emoji monochromes (Noto Emoji, emoji-icon-font) ne sont
//! plus chargées par défaut : les symboles de l'interface (📜 ⚙ 🔒 ⏹ ★ ☰ …) étaient alors
//! cherchés dans les polices du système, et Windows ne propose que des emoji en couleur
//! (Segoe UI Emoji) qu'egui affiche sous forme de carrés. On réactive donc ces polices
//! (fonction `monochrome_emoji_fonts` d'egui, déclarée dans `Cargo.toml`) et on ajoute Hack
//! (déjà embarquée pour la police à chasse fixe) en dernier repli de la police
//! proportionnelle : flèches (← → ↑ ↓), filets (─ │ ┌), petits triangles (▸ ▾), ● …
//!
//! Les polices embarquées passent avant celles du système (qui restent un repli, utile
//! pour les titres en japonais, coréen, etc.) : l'affichage est le même sur toutes les
//! machines.
//!
//! Sous Windows, **Segoe UI Symbol** (`seguisym.ttf`, monochrome) est ajoutée juste après
//! les polices embarquées, avant la recherche automatique dans les polices du système : elle
//! couvre les symboles que des scripts ou des fiches pourraient afficher en plus de ceux de
//! l'interface (flèches, pictogrammes, formes géométriques…). **Segoe UI Emoji**
//! (`seguiemj.ttf`) n'est volontairement pas chargée : ses emoji sont des calques en couleur
//! (COLR) qu'egui ne sait pas peindre — c'est elle que la recherche automatique choisissait
//! en v0.10.1, d'où les carrés. Le test `tous_les_caracteres_de_l_interface_ont_un_glyphe` vérifie que chaque
//! caractère non ASCII des sources de l'interface et des scripts d'exemple y a un glyphe.

use eframe::egui::{self, FontDefinitions, FontFamily};

/// Nom de la police à chasse fixe d'egui (Hack), reprise en repli de la proportionnelle.
const HACK: &str = "Hack";

/// Police de symboles de Windows, ajoutée en repli quand elle est présente.
const SEGOE_SYMBOL: &str = "Segoe UI Symbol";

/// Polices utilisées par l'application (avec les polices de symboles du système trouvées).
pub fn definitions() -> FontDefinitions {
    definitions_with(system_symbol_fonts())
}

/// Polices embarquées, complétées par `extra` (nom, contenu du fichier) en dernier repli des
/// deux familles, dans l'ordre donné.
pub fn definitions_with(extra: Vec<(String, Vec<u8>)>) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        if !prop.iter().any(|n| n == HACK) {
            prop.push(HACK.to_owned());
        }
    }
    for (name, data) in extra {
        // Fichier illisible ou qui n'est pas une police : ignoré (egui paniquerait au
        // premier texte).
        if !looks_like_font(&data) {
            continue;
        }
        fonts.font_data.insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(data)));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                if !list.contains(&name) {
                    list.push(name.clone());
                }
            }
        }
    }
    fonts
}

/// En-tête d'un fichier TrueType / OpenType (ou d'une collection).
fn looks_like_font(data: &[u8]) -> bool {
    matches!(data.get(..4), Some([0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf"))
}

/// Polices de symboles du système à ajouter : Segoe UI Symbol sous Windows (dossier
/// `%WINDIR%\Fonts`), rien ailleurs (les polices embarquées couvrent l'interface).
fn system_symbol_fonts() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    if cfg!(windows) {
        let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
        let path = std::path::Path::new(&windir).join("Fonts").join("seguisym.ttf");
        if let Ok(data) = std::fs::read(&path) {
            if looks_like_font(&data) {
                out.push((SEGOE_SYMBOL.to_owned(), data));
            }
        }
    }
    out
}

/// Installe les polices dans le contexte egui (une fois, au démarrage).
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::epaint::text::{Fonts, TextOptions};
    use eframe::egui::FontId;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    /// Les polices attendues sont bien là (si egui change ses noms, le test le signale).
    #[test]
    fn polices_d_emoji_chargees() {
        let defs = definitions();
        for nom in ["Ubuntu-Light", HACK, "egui-icons", "NotoEmoji-Regular", "emoji-icon-font"] {
            assert!(defs.font_data.contains_key(nom), "police {nom} absente (fonction monochrome_emoji_fonts ?)");
        }
        assert!(defs.families[&FontFamily::Proportional].iter().any(|n| n == HACK));
    }

    /// Les polices de symboles du système passent après toutes les polices embarquées (dans
    /// les deux familles) ; un fichier qui n'est pas une police est ignoré.
    #[test]
    fn polices_du_systeme_en_dernier_repli() {
        let hack = epaint_default_fonts_hack();
        let defs =
            definitions_with(vec![("Symboles".to_owned(), hack), ("Abîmée".to_owned(), b"pas une police".to_vec())]);
        for famille in [FontFamily::Proportional, FontFamily::Monospace] {
            let liste = &defs.families[&famille];
            assert_eq!(liste.last().map(String::as_str), Some("Symboles"), "{famille:?} : {liste:?}");
            assert!(liste.iter().position(|n| n == HACK) < liste.iter().position(|n| n == "Symboles"));
        }
        assert!(!defs.font_data.contains_key("Abîmée"));
        // Sans police du système : les polices embarquées seules.
        assert!(!definitions_with(Vec::new()).font_data.contains_key(SEGOE_SYMBOL));
    }

    /// Octets de la police Hack d'egui (police valide pour les essais).
    fn epaint_default_fonts_hack() -> Vec<u8> {
        let d = FontDefinitions::default();
        let data: &[u8] = (*d.font_data[HACK].font).as_ref();
        data.to_vec()
    }

    /// Fichiers à examiner : sources de l'interface et scripts d'exemple (textes qu'ils affichent).
    fn fichiers(dossier: &Path, extensions: &[&str], sortie: &mut Vec<PathBuf>) {
        let Ok(entrees) = std::fs::read_dir(dossier) else { return };
        for e in entrees.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "__pycache__") {
                    fichiers(&p, extensions, sortie);
                }
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| extensions.contains(&x)) {
                sortie.push(p);
            }
        }
    }

    /// Caractères non ASCII hors lignes de commentaire, avec un fichier où ils apparaissent.
    fn caracteres_utilises() -> BTreeMap<char, String> {
        let racine = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut liste = Vec::new();
        fichiers(&racine.join("src"), &["rs"], &mut liste);
        // Messages du cœur affichés par l'interface (erreurs, bilans, module Python des scripts).
        fichiers(&racine.join("../comicrust-core/src"), &["rs", "py"], &mut liste);
        let scripts = racine.join("../../scripts-exemples");
        fichiers(&scripts, &["py", "md", "txt", "Config", "xml", "ini"], &mut liste);
        assert!(liste.len() > 20, "sources introuvables depuis {}", racine.display());
        let mut vus = BTreeMap::new();
        // Tests du cœur (données d'essai, jamais affichées) exclus.
        let essai = |f: &Path| {
            f.components().any(|c| c.as_os_str() == "tests")
                || f.file_stem().and_then(|x| x.to_str()).is_some_and(|x| x == "tests" || x.ends_with("_tests"))
        };
        for f in liste.into_iter().filter(|f| f.starts_with(racine.join("src")) || !essai(f)) {
            let texte = std::fs::read_to_string(&f).unwrap_or_default();
            // Les commentaires ne sont jamais affichés : `//` en Rust, `#` en Python.
            let commentaire = match f.extension().and_then(|x| x.to_str()) {
                Some("rs") => Some("//"),
                Some("py") => Some("#"),
                _ => None,
            };
            for (n, ligne) in texte.lines().enumerate() {
                // `#@Name`, `#@Description`… des scripts sont affichés (menus) : pas des commentaires.
                let l = ligne.trim_start();
                if commentaire.is_some_and(|c| l.starts_with(c)) && !l.starts_with("#@") {
                    continue;
                }
                for c in ligne.chars().filter(|c| !c.is_ascii() && *c != '\u{feff}') {
                    vus.entry(c).or_insert_with(|| format!("{}:{}", f.display(), n + 1));
                }
            }
        }
        vus
    }

    /// Chaque caractère affiché par l'interface a un glyphe dans les polices embarquées,
    /// sans compter sur celles du système (absentes ou en couleur sous Windows).
    #[test]
    fn tous_les_caracteres_de_l_interface_ont_un_glyphe() {
        let mut fonts = Fonts::new(TextOptions::default(), definitions());
        let car = caracteres_utilises();
        assert!(car.contains_key(&'é') && car.contains_key(&'⚙'), "inventaire incomplet");
        let mut manquants = Vec::new();
        for (c, ou) in &car {
            for famille in [FontFamily::Proportional, FontFamily::Monospace] {
                if !fonts.has_glyph(&FontId::new(14.0, famille.clone()), *c) {
                    manquants.push(format!("U+{:04X} « {c} » ({famille:?}) — {ou}", *c as u32));
                }
            }
        }
        assert!(
            manquants.is_empty(),
            "caractères sans glyphe (carrés à l'écran) : remplacer par un symbole couvert\n{}",
            manquants.join("\n")
        );
    }

    /// Les sélecteurs de variante (U+FE0E/U+FE0F) et le liant U+200D ne servent qu'aux emoji
    /// en couleur : egui les afficherait comme des carrés.
    #[test]
    fn pas_de_selecteur_de_variante() {
        for (c, ou) in caracteres_utilises() {
            assert!(!matches!(c, '\u{fe0e}' | '\u{fe0f}' | '\u{200d}'), "U+{:04X} dans {ou}", c as u32);
        }
    }
}
