//! Thèmes de l'interface : des fichiers CSS à variables dans `data\themes\`.
//!
//! - Les thèmes livrés (Moderne clair, Moderne sombre, Classique) sont embarqués dans le binaire
//!   et **copiés au premier lancement** s'ils manquent : l'utilisateur peut les modifier ou les
//!   copier pour créer les siens, sans recompiler. Un fichier existant n'est jamais écrasé.
//! - Le nom affiché vient d'un commentaire en tête de fichier : `/* @nom: Mon thème */`
//!   (à défaut, le nom du fichier).
//! - Le front charge un thème par son identifiant (nom du fichier sans `.css`) et injecte le CSS.

use serde::Serialize;
use std::fmt;
use std::path::Path;

/// Thèmes livrés : (identifiant, contenu).
pub const THEMES_LIVRES: &[(&str, &str)] = &[
    ("moderne-clair", include_str!("../themes/moderne-clair.css")),
    ("moderne-sombre", include_str!("../themes/moderne-sombre.css")),
    ("classique", include_str!("../themes/classique.css")),
];

/// Thème utilisé quand rien n'est choisi ou que le thème choisi a disparu.
pub const THEME_DEFAUT: &str = "moderne-clair";

/// Taille maximale d'un fichier de thème (garde-fou contre un fichier énorme).
const TAILLE_MAX: u64 = 1024 * 1024;

/// Un thème disponible dans `data\themes`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Theme {
    /// Nom de fichier sans `.css`.
    pub id: String,
    /// Nom affiché.
    pub nom: String,
    /// `true` si l'identifiant est celui d'un thème livré.
    pub livre: bool,
}

/// Erreurs de lecture d'un thème.
#[derive(Debug)]
pub enum ErreurTheme {
    /// Identifiant avec séparateur, point ou caractère inattendu (évite de sortir de `themes\`).
    IdInvalide(String),
    /// Fichier absent.
    Introuvable(String),
    /// Fichier trop gros.
    TropGros(String),
    /// Erreur d'entrée/sortie.
    Io(std::io::Error),
}

impl fmt::Display for ErreurTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdInvalide(id) => write!(f, "identifiant de thème invalide : « {id} »"),
            Self::Introuvable(id) => write!(f, "thème introuvable : « {id} »"),
            Self::TropGros(id) => write!(f, "fichier de thème trop gros : « {id} »"),
            Self::Io(e) => write!(f, "E/S : {e}"),
        }
    }
}

impl std::error::Error for ErreurTheme {}

impl From<std::io::Error> for ErreurTheme {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Un identifiant valide : lettres, chiffres, `-` et `_` seulement (pas de `/`, `\`, `.`, `:`).
pub fn id_valide(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Copie les thèmes livrés manquants dans `dossier` (créé au besoin). N'écrase jamais.
pub fn installer_themes_livres(dossier: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dossier)?;
    for (id, css) in THEMES_LIVRES {
        let chemin = dossier.join(format!("{id}.css"));
        if !chemin.exists() {
            std::fs::write(chemin, css)?;
        }
    }
    Ok(())
}

/// Nom affiché d'après le commentaire `/* @nom: … */` des premières lignes.
fn nom_depuis_css(css: &str) -> Option<String> {
    css.lines().take(10).find_map(|l| {
        let (_, reste) = l.split_once("@nom:")?;
        let nom = reste.split("*/").next()?.trim();
        (!nom.is_empty()).then(|| nom.to_string())
    })
}

/// Liste les thèmes de `dossier` (fichiers `.css` à identifiant valide), triés par nom affiché.
pub fn lister_themes(dossier: &Path) -> Result<Vec<Theme>, ErreurTheme> {
    let mut themes = Vec::new();
    for entree in std::fs::read_dir(dossier)? {
        let chemin = entree?.path();
        if chemin.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("css")) != Some(true) {
            continue;
        }
        let Some(id) = chemin.file_stem().and_then(|s| s.to_str()) else { continue };
        if !id_valide(id) || !chemin.is_file() {
            continue;
        }
        // Un fichier illisible reste listé sous son identifiant.
        let nom =
            std::fs::read_to_string(&chemin).ok().and_then(|c| nom_depuis_css(&c)).unwrap_or_else(|| id.to_string());
        themes.push(Theme { id: id.to_string(), nom, livre: THEMES_LIVRES.iter().any(|(l, _)| *l == id) });
    }
    themes.sort_by(|a, b| a.nom.to_lowercase().cmp(&b.nom.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    Ok(themes)
}

/// Contenu CSS du thème `id`.
pub fn charger_theme(dossier: &Path, id: &str) -> Result<String, ErreurTheme> {
    if !id_valide(id) {
        return Err(ErreurTheme::IdInvalide(id.to_string()));
    }
    let chemin = dossier.join(format!("{id}.css"));
    let meta = std::fs::metadata(&chemin).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ErreurTheme::Introuvable(id.to_string()),
        _ => ErreurTheme::Io(e),
    })?;
    if !meta.is_file() {
        return Err(ErreurTheme::Introuvable(id.to_string()));
    }
    if meta.len() > TAILLE_MAX {
        return Err(ErreurTheme::TropGros(id.to_string()));
    }
    Ok(std::fs::read_to_string(chemin)?)
}

/// Variables CSS que tout thème doit définir (contrat avec le front, voir `docs/tauri/themes.md`).
pub const VARIABLES_REQUISES: &[&str] = &[
    "--cr-fond",
    "--cr-fond-panneau",
    "--cr-fond-survol",
    "--cr-texte",
    "--cr-texte-discret",
    "--cr-bordure",
    "--cr-accent",
    "--cr-accent-texte",
    "--cr-erreur",
    "--cr-rayon",
    "--cr-police",
    "--cr-ombre",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiants() {
        assert!(id_valide("moderne-clair"));
        assert!(id_valide("Mon_thème2"));
        for mauvais in ["", "..", "a/b", "a\\b", "x.css", "C:", "../secret"] {
            assert!(!id_valide(mauvais), "{mauvais}");
        }
    }

    #[test]
    fn installation_sans_ecrasement_et_liste() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("themes");
        installer_themes_livres(&dir).unwrap();
        // Un thème modifié par l'utilisateur survit à une réinstallation.
        std::fs::write(dir.join("classique.css"), "/* @nom: Mon classique */ :root{}").unwrap();
        std::fs::write(dir.join("perso.css"), ":root{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "pas un thème").unwrap();
        std::fs::write(dir.join("a b.css"), "id invalide").unwrap();
        installer_themes_livres(&dir).unwrap();

        let themes = lister_themes(&dir).unwrap();
        let ids: Vec<_> = themes.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 4, "{ids:?}");
        assert!(ids.contains(&"perso") && ids.contains(&"moderne-sombre"));
        let classique = themes.iter().find(|t| t.id == "classique").unwrap();
        assert_eq!(classique.nom, "Mon classique");
        assert!(classique.livre);
        assert!(!themes.iter().find(|t| t.id == "perso").unwrap().livre);
        assert_eq!(charger_theme(&dir, "classique").unwrap(), "/* @nom: Mon classique */ :root{}");
    }

    #[test]
    fn chargement_refuse_les_chemins_et_les_absents() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(charger_theme(tmp.path(), "../x"), Err(ErreurTheme::IdInvalide(_))));
        assert!(matches!(charger_theme(tmp.path(), "absent"), Err(ErreurTheme::Introuvable(_))));
    }

    #[test]
    fn themes_livres_coherents() {
        for (id, css) in THEMES_LIVRES {
            assert!(id_valide(id));
            assert!(nom_depuis_css(css).is_some(), "{id} sans @nom");
            // Toutes les variables utilisées par le front sont définies par chaque thème.
            for var in VARIABLES_REQUISES {
                assert!(css.contains(&format!("{var}:")), "{id} : variable {var} absente");
            }
        }
        assert!(THEMES_LIVRES.iter().any(|(id, _)| *id == THEME_DEFAUT));
    }
}
