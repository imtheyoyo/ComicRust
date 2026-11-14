//! Thèmes de l'interface : des fichiers CSS à variables dans `data\themes\`.
//!
//! - Les thèmes livrés (Moderne clair, Moderne sombre, Classique) sont embarqués dans le binaire
//!   et **copiés au premier lancement** s'ils manquent : l'utilisateur peut les modifier ou les
//!   copier pour créer les siens, sans recompiler. Un fichier existant n'est jamais écrasé, sauf
//!   s'il est **identique à une ancienne version livrée** (mise à jour silencieuse, voir
//!   [`installer_themes_livres`]) : un thème modifié par l'utilisateur n'est jamais touché.
//! - Le nom affiché vient d'un commentaire en tête de fichier : `/* @nom: Mon thème */`
//!   (à défaut, le nom du fichier). `/* @contrat: 2 */` indique la version du contrat de variables.
//! - Le front charge un thème par son identifiant (nom du fichier sans `.css`) et injecte le CSS
//!   **assaini** par [`crate::themes_css::valider_css`].
//! - Contrat IPC : `docs/tauri/ipc-themes.md` ; contrat de variables : `docs/tauri/themes.md`.

use crate::edition::{commande_ouvrir_dossier, lancer_commande, CommandeDossier, Systeme};
use crate::themes_css::{valeur_variable, valider_css, variables_cr, TAILLE_MAX_CSS};
use serde::Serialize;
use std::fmt;
use std::path::Path;

/// Thèmes livrés : (identifiant, contenu).
pub const THEMES_LIVRES: &[(&str, &str)] = &[
    ("moderne-clair", include_str!("../themes/moderne-clair.css")),
    ("moderne-sombre", include_str!("../themes/moderne-sombre.css")),
    ("classique", include_str!("../themes/classique.css")),
];

/// Anciennes versions des thèmes livrés (contrat 1) : un fichier installé identique à l'une d'elles
/// est remplacé par la version courante au démarrage. À chaque nouvelle version d'un thème livré,
/// **ajouter ici le contenu de la version précédente** (fichiers dans `themes/anciennes/`).
pub const THEMES_ANCIENS: &[(&str, &str)] = &[
    ("moderne-clair", include_str!("../themes/anciennes/moderne-clair.v1.css")),
    ("moderne-sombre", include_str!("../themes/anciennes/moderne-sombre.v1.css")),
    ("classique", include_str!("../themes/anciennes/classique.v1.css")),
];

/// Version du contrat de variables des thèmes livrés (`/* @contrat: 2 */`).
pub const VERSION_CONTRAT: u32 = 2;

/// Thème utilisé quand rien n'est choisi ou que le thème choisi a disparu.
pub const THEME_DEFAUT: &str = "moderne-clair";

/// Une variable du contrat `--cr-*`.
#[derive(Debug, Clone, Copy)]
pub struct VariableContrat {
    /// Nom CSS, avec `--cr-`.
    pub nom: &'static str,
    /// Rôle, en français.
    pub role: &'static str,
    /// Obligatoire : un thème doit la définir. Sinon facultative, avec un repli.
    pub obligatoire: bool,
    /// Valeur de repli des facultatives (indicatif : la source de vérité est `frontend/src/app.css`).
    pub repli: &'static str,
}

const fn o(nom: &'static str, role: &'static str) -> VariableContrat {
    VariableContrat { nom, role, obligatoire: true, repli: "" }
}

const fn f(nom: &'static str, role: &'static str, repli: &'static str) -> VariableContrat {
    VariableContrat { nom, role, obligatoire: false, repli }
}

/// Contrat de variables entre les thèmes et le front (`docs/tauri/themes.md`). Toute variable
/// `--cr-*` utilisée par `frontend/src` doit figurer ici (garde-fou : test `themes_livres_coherents`).
pub const CONTRAT: &[VariableContrat] = &[
    o("--cr-fond", "Fond général de la fenêtre"),
    o("--cr-fond-panneau", "Panneaux, cartes, dialogues et menus"),
    o("--cr-fond-survol", "Survol d'une ligne ou d'un bouton"),
    o("--cr-texte", "Texte courant"),
    o("--cr-texte-discret", "Texte secondaire : légendes, aides, valeurs inactives"),
    o("--cr-bordure", "Contours des champs, séparateurs, cadres"),
    o("--cr-accent", "Couleur d'accent : sélection, boutons principaux, liens, progression"),
    o("--cr-accent-texte", "Texte posé sur l'accent"),
    o("--cr-erreur", "Erreurs et actions destructrices"),
    o("--cr-rayon", "Rayon des angles"),
    o("--cr-police", "Police de l'interface"),
    o("--cr-ombre", "Ombre douce des cartes et menus"),
    f("--cr-fond-lecteur", "Fond de la zone de lecture des pages", "var(--cr-fond)"),
    f("--cr-fond-entete", "Fond des en-têtes de tableau et de panneau", "var(--cr-fond-panneau)"),
    f(
        "--cr-fond-selection",
        "Fond d'un élément sélectionné",
        "color-mix(in srgb, var(--cr-accent) 18%, var(--cr-fond-panneau))",
    ),
    f("--cr-texte-inverse", "Texte posé sur une image ou un fond sombre (badges sur couvertures)", "#ffffff"),
    f("--cr-focus", "Anneau de focus clavier", "var(--cr-accent)"),
    f("--cr-accent-survol", "Accent au survol d'un bouton principal", "color-mix(in srgb, var(--cr-accent) 85%, #000)"),
    f("--cr-succes", "Succès : opération réussie, fichier présent", "#2e7d32"),
    f("--cr-avertissement", "Avertissement : attention, valeur à vérifier", "#b26a00"),
    f("--cr-etoile", "Étoiles de la note", "#f2b01e"),
    f(
        "--cr-rectangle-fond",
        "Fond du rectangle de sélection (glisser sur la grille)",
        "color-mix(in srgb, var(--cr-accent) 20%, transparent)",
    ),
    f("--cr-rectangle-bordure", "Contour du rectangle de sélection", "var(--cr-accent)"),
    f("--cr-lecture-piste", "Piste vide des barres de lecture et de progression", "var(--cr-bordure)"),
    f("--cr-ombre-forte", "Ombre marquée des fenêtres flottantes et du lecteur", "var(--cr-ombre)"),
];

/// Un thème disponible dans `data\themes`, tel que le voit le front (contrat IPC `ThemeInfo`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInfo {
    /// Nom de fichier sans `.css`.
    pub id: String,
    /// Nom affiché.
    pub nom: String,
    /// `true` si l'identifiant est celui d'un thème livré.
    pub livre: bool,
    /// Couleurs d'aperçu lues dans le `:root` (`None` si l'une des trois manque).
    pub couleurs: Option<CouleursTheme>,
    /// Problèmes non bloquants (variables manquantes, lignes supprimées…).
    pub avertissements: Vec<String>,
}

/// Couleurs d'aperçu d'un thème : valeurs CSS telles qu'écrites.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CouleursTheme {
    pub fond: String,
    pub accent: String,
    pub texte: String,
}

/// CSS assaini d'un thème et ses avertissements (contrat IPC `ThemeCharge`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeCharge {
    pub css: String,
    pub avertissements: Vec<String>,
}

/// Erreurs de lecture ou d'écriture d'un thème.
#[derive(Debug)]
pub enum ErreurTheme {
    /// Identifiant avec séparateur, point ou caractère inattendu (évite de sortir de `themes\`),
    /// ou nom réservé de Windows.
    IdInvalide(String),
    /// Fichier absent.
    Introuvable(String),
    /// Fichier trop gros.
    TropGros(String),
    /// CSS refusé par le filtrage (motifs).
    Refuse(String),
    /// Suppression d'un thème livré.
    LivreProtege,
    /// Erreur d'entrée/sortie.
    Io(std::io::Error),
}

impl fmt::Display for ErreurTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdInvalide(id) => write!(f, "identifiant de thème invalide : « {id} »"),
            Self::Introuvable(id) => write!(f, "thème introuvable : « {id} »"),
            Self::TropGros(id) => write!(f, "fichier de thème trop gros : « {id} »"),
            Self::Refuse(motif) => write!(f, "thème refusé : {motif}"),
            Self::LivreProtege => write!(f, "un thème livré ne se supprime pas : utilisez Réinitialiser"),
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

/// Noms de fichiers réservés par Windows (avec ou sans extension) : refusés comme identifiants.
fn nom_reserve_windows(id: &str) -> bool {
    let bas = id.to_ascii_lowercase();
    matches!(bas.as_str(), "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|p| bas.strip_prefix(p).is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9'])))
}

/// Un identifiant valide : lettres, chiffres, `-` et `_` seulement (pas de `/`, `\`, `.`, `:`),
/// et pas un nom réservé de Windows (`con`, `prn`, `aux`, `nul`, `com1`-`com9`, `lpt1`-`lpt9`).
pub fn id_valide(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !nom_reserve_windows(id)
}

fn est_livre(id: &str) -> bool {
    THEMES_LIVRES.iter().any(|(l, _)| l.eq_ignore_ascii_case(id))
}

/// Texte comparable : fins de ligne unifiées, espaces de fin de ligne et BOM ignorés.
fn normaliser(s: &str) -> String {
    let s = s.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let lignes: Vec<&str> = s.lines().map(str::trim_end).collect();
    lignes.join("\n").trim_end().to_string()
}

/// Premier nom libre `<base>`, `<base>2`, `<base>3`… dans `dossier`, pour `<id>.css.avant`.
fn chemin_avant(dossier: &Path, id: &str) -> std::path::PathBuf {
    let premier = dossier.join(format!("{id}.css.avant"));
    if !premier.exists() {
        return premier;
    }
    (2..).map(|n| dossier.join(format!("{id}.css.avant{n}"))).find(|p| !p.exists()).unwrap_or(premier)
}

/// Copie les thèmes livrés manquants dans `dossier` (créé au besoin) et met à jour ceux qui sont
/// **identiques à une ancienne version livrée** ([`THEMES_ANCIENS`], fins de ligne et espaces de fin
/// ignorés). Un fichier modifié par l'utilisateur n'est jamais écrasé. Renvoie les identifiants des
/// thèmes remplacés par une version plus récente.
pub fn installer_themes_livres(dossier: &Path) -> std::io::Result<Vec<String>> {
    std::fs::create_dir_all(dossier)?;
    let mut mis_a_jour = Vec::new();
    for (id, css) in THEMES_LIVRES {
        let chemin = dossier.join(format!("{id}.css"));
        if !chemin.exists() {
            std::fs::write(chemin, css)?;
            continue;
        }
        let Ok(existant) = std::fs::read_to_string(&chemin) else { continue };
        let n = normaliser(&existant);
        if n == normaliser(css) {
            continue;
        }
        if THEMES_ANCIENS.iter().any(|(i, ancien)| i == id && normaliser(ancien) == n) {
            std::fs::write(chemin, css)?;
            mis_a_jour.push((*id).to_string());
        }
    }
    Ok(mis_a_jour)
}

/// Recopie les trois thèmes livrés. Un fichier installé **différent** est d'abord renommé
/// `<id>.css.avant` (jamais écrasé : `.avant2`… si besoin). Renvoie les identifiants réellement remis
/// à neuf (fichier absent ou différent).
pub fn reinitialiser_themes_livres(dossier: &Path) -> Result<Vec<String>, ErreurTheme> {
    std::fs::create_dir_all(dossier)?;
    let mut remis = Vec::new();
    for (id, css) in THEMES_LIVRES {
        let chemin = dossier.join(format!("{id}.css"));
        if chemin.is_file() {
            let identique = std::fs::read_to_string(&chemin).is_ok_and(|e| normaliser(&e) == normaliser(css));
            if identique {
                continue;
            }
            std::fs::rename(&chemin, chemin_avant(dossier, id))?;
        }
        std::fs::write(&chemin, css)?;
        remis.push((*id).to_string());
    }
    Ok(remis)
}

/// Nom affiché d'après le commentaire `/* @nom: … */` des premières lignes.
fn nom_depuis_css(css: &str) -> Option<String> {
    css.lines().take(10).find_map(|l| {
        let (_, reste) = l.split_once("@nom:")?;
        let nom = reste.split("*/").next()?.trim();
        (!nom.is_empty()).then(|| nom.to_string())
    })
}

/// Couleurs d'aperçu d'après les variables `--cr-*` du CSS.
fn couleurs_depuis(css: &str) -> Option<CouleursTheme> {
    let v = variables_cr(css);
    let lire = |nom: &str| valeur_variable(&v, nom).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Some(CouleursTheme { fond: lire("--cr-fond")?, accent: lire("--cr-accent")?, texte: lire("--cr-texte")? })
}

/// Contenu brut (non assaini) du thème `id`.
fn lire_brut(dossier: &Path, id: &str) -> Result<String, ErreurTheme> {
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
    if meta.len() > TAILLE_MAX_CSS as u64 {
        return Err(ErreurTheme::TropGros(id.to_string()));
    }
    Ok(std::fs::read_to_string(chemin)?)
}

/// `ThemeInfo` du thème `id` (un fichier illisible ou refusé reste décrit, avec un avertissement).
fn info_theme(dossier: &Path, id: &str) -> ThemeInfo {
    let mut info = ThemeInfo {
        id: id.to_string(),
        nom: id.to_string(),
        livre: est_livre(id),
        couleurs: None,
        avertissements: Vec::new(),
    };
    match lire_brut(dossier, id) {
        Err(e) => info.avertissements.push(e.to_string()),
        Ok(brut) => {
            if let Some(nom) = nom_depuis_css(&brut) {
                info.nom = nom;
            }
            let rapport = valider_css(&brut);
            if rapport.erreurs.is_empty() {
                info.couleurs = couleurs_depuis(&rapport.css_assaini);
                info.avertissements = rapport.avertissements;
            } else {
                info.avertissements.push(ErreurTheme::Refuse(rapport.erreurs.join(" ; ")).to_string());
            }
        }
    }
    info
}

/// Liste les thèmes de `dossier` (fichiers `.css` à identifiant valide), triés par nom affiché.
pub fn lister_themes(dossier: &Path) -> Result<Vec<ThemeInfo>, ErreurTheme> {
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
        themes.push(info_theme(dossier, id));
    }
    themes.sort_by(|a, b| a.nom.to_lowercase().cmp(&b.nom.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    Ok(themes)
}

/// CSS assaini du thème `id` et ses avertissements.
pub fn charger_theme(dossier: &Path, id: &str) -> Result<ThemeCharge, ErreurTheme> {
    let brut = lire_brut(dossier, id)?;
    let rapport = valider_css(&brut);
    if !rapport.erreurs.is_empty() {
        return Err(ErreurTheme::Refuse(rapport.erreurs.join(" ; ")));
    }
    Ok(ThemeCharge { css: rapport.css_assaini, avertissements: rapport.avertissements })
}

/// Identifiant dérivé d'un nom : minuscules, accents retirés, tout le reste en `-`.
fn identifiant_depuis_nom(nom: &str) -> String {
    let mut id = String::new();
    for c in nom.chars().flat_map(char::to_lowercase) {
        let t: &str = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => "a",
            'ç' => "c",
            'è' | 'é' | 'ê' | 'ë' => "e",
            'ì' | 'í' | 'î' | 'ï' => "i",
            'ñ' => "n",
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => "o",
            'ù' | 'ú' | 'û' | 'ü' => "u",
            'ý' | 'ÿ' => "y",
            'œ' => "oe",
            'æ' => "ae",
            c if c.is_ascii_alphanumeric() => {
                id.push(c);
                continue;
            }
            _ => "-",
        };
        if t == "-" && (id.ends_with('-') || id.is_empty()) {
            continue;
        }
        id.push_str(t);
    }
    let id: String = id.trim_matches('-').chars().take(48).collect();
    let id = id.trim_matches('-').to_string();
    if id.is_empty() {
        "mon-theme".to_string()
    } else {
        id
    }
}

/// Premier identifiant libre : `base`, `base-2`, `base-3`… (sans tenir compte de la casse, comme
/// le système de fichiers de Windows).
fn identifiant_libre(dossier: &Path, base: &str) -> String {
    let pris: Vec<String> = std::fs::read_dir(dossier)
        .map(|d| {
            d.flatten().filter_map(|e| e.path().file_stem().and_then(|s| s.to_str()).map(str::to_lowercase)).collect()
        })
        .unwrap_or_default();
    let libre = |c: &str| id_valide(c) && !est_livre(c) && !pris.contains(&c.to_lowercase());
    if libre(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|c| libre(c)).unwrap_or_else(|| base.to_string())
}

/// Nom affichable sans risque dans un commentaire CSS : pas de contrôle, pas de `/*` ni `*/`.
fn nom_sur(nom: &str) -> String {
    let n: String = nom.chars().filter(|c| !c.is_control()).collect();
    // `*` et `/` sont ôtés un à un : retirer `*/` puis `/*` pouvait en recréer (`**//`).
    n.chars()
        .filter(|c| !matches!(c, '*' | '/'))
        .collect::<String>()
        .trim()
        .chars()
        .take(80)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Importe un thème : valide et assainit `css`, choisit un identifiant libre dérivé de `nom`
/// (jamais d'écrasement), écrit `/* @nom: … */` en tête s'il manque. Renvoie le thème créé.
pub fn importer_theme(dossier: &Path, nom: &str, css: &str) -> Result<ThemeInfo, ErreurTheme> {
    let rapport = valider_css(css);
    if !rapport.erreurs.is_empty() {
        return Err(ErreurTheme::Refuse(rapport.erreurs.join(" ; ")));
    }
    if !crate::themes_css::a_regle_root(&rapport.css_assaini) {
        return Err(ErreurTheme::Refuse(
            "aucune règle :root : les variables --cr-* doivent être définies dans `:root { … }`".to_string(),
        ));
    }
    std::fs::create_dir_all(dossier)?;
    let nom = nom_sur(nom);
    let base = identifiant_depuis_nom(&nom);
    let mut contenu = rapport.css_assaini;
    if nom_depuis_css(&contenu).is_none() {
        let affiche = if nom.is_empty() { "Mon thème" } else { nom.as_str() };
        contenu = format!("/* @nom: {affiche} */\n{contenu}");
    }
    // `create_new` : même en cas de course, un fichier existant n'est jamais écrasé.
    let mut id = identifiant_libre(dossier, &base);
    loop {
        let chemin = dossier.join(format!("{id}.css"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&chemin) {
            Ok(mut fichier) => {
                std::io::Write::write_all(&mut fichier, contenu.as_bytes())?;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => id = identifiant_libre(dossier, &base),
            Err(e) => return Err(e.into()),
        }
    }
    // Les avertissements du filtrage (lignes supprimées…) ne sont plus visibles dans le fichier écrit.
    let mut info = info_theme(dossier, &id);
    info.avertissements = rapport.avertissements;
    Ok(info)
}

/// En-tête de commentaire des exports.
const ENTETE_EXPORT: &str = "/* ComicRust : thème";

/// CSS assaini du thème `id`, précédé d'un commentaire d'en-tête.
pub fn exporter_theme(dossier: &Path, id: &str) -> Result<String, ErreurTheme> {
    let charge = charger_theme(dossier, id)?;
    if charge.css.trim_start().starts_with(ENTETE_EXPORT) {
        return Ok(charge.css);
    }
    let nom = nom_depuis_css(&charge.css).unwrap_or_else(|| id.to_string());
    Ok(format!(
        "{ENTETE_EXPORT} « {nom} » (identifiant : {id}).\n   Pour l'utiliser : Réglages, Thèmes, Importer ; ou copier ce fichier dans data\\themes. */\n{}",
        charge.css
    ))
}

/// Supprime le thème `id`. Refusé pour un thème livré.
pub fn supprimer_theme(dossier: &Path, id: &str) -> Result<(), ErreurTheme> {
    if !id_valide(id) {
        return Err(ErreurTheme::IdInvalide(id.to_string()));
    }
    if est_livre(id) {
        return Err(ErreurTheme::LivreProtege);
    }
    std::fs::remove_file(dossier.join(format!("{id}.css"))).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ErreurTheme::Introuvable(id.to_string()),
        _ => ErreurTheme::Io(e),
    })
}

/// Commande qui ouvre `dossier` dans l'explorateur (rien n'est lancé).
pub fn commande_reveler_dossier(dossier: &Path, systeme: Systeme) -> CommandeDossier {
    // `commande_ouvrir_dossier` ouvre le **parent** du chemin donné : on lui passe un fichier fictif.
    commande_ouvrir_dossier(&dossier.join("themes.css"), false, systeme)
}

/// Crée `dossier` au besoin et l'ouvre dans l'explorateur de fichiers.
pub fn reveler_dossier(dossier: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dossier).map_err(|e| format!("E/S : {e}"))?;
    lancer_commande(&commande_reveler_dossier(dossier, Systeme::courant()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valeur d'une variable dans un thème livré.
    fn variable(css: &str, nom: &str) -> String {
        let v = variables_cr(css);
        valeur_variable(&v, nom).unwrap_or_else(|| panic!("{nom} absente")).to_string()
    }

    /// Couleur `#rgb` ou `#rrggbb` en composantes 0-255.
    fn hex(s: &str) -> [f64; 3] {
        let h = s.trim().strip_prefix('#').unwrap_or_else(|| panic!("pas une couleur hexadécimale : {s}"));
        let h = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect::<String>() } else { h.to_string() };
        assert_eq!(h.len(), 6, "{s}");
        let c = |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap());
        [c(0), c(2), c(4)]
    }

    /// Luminance relative WCAG.
    fn luminance(rgb: [f64; 3]) -> f64 {
        let l = |v: f64| {
            let v = v / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * l(rgb[0]) + 0.7152 * l(rgb[1]) + 0.0722 * l(rgb[2])
    }

    /// Rapport de contraste WCAG entre deux couleurs hexadécimales.
    fn contraste(a: &str, b: &str) -> f64 {
        let (la, lb) = (luminance(hex(a)), luminance(hex(b)));
        let (clair, sombre) = if la > lb { (la, lb) } else { (lb, la) };
        (clair + 0.05) / (sombre + 0.05)
    }

    #[test]
    fn identifiants() {
        assert!(id_valide("moderne-clair"));
        assert!(id_valide("Mon_theme2") && !id_valide("Mon_thème2"));
        for mauvais in ["", "..", "a/b", "a\\b", "x.css", "C:", "../secret"] {
            assert!(!id_valide(mauvais), "{mauvais}");
        }
    }

    #[test]
    fn identifiants_reserves_windows_refuses() {
        for reserve in ["con", "CON", "Prn", "aux", "NUL", "com1", "COM9", "lpt1", "Lpt9"] {
            assert!(!id_valide(reserve), "{reserve}");
        }
        for ok in ["console", "com0", "com10", "lpt", "com", "auxiliaire", "nul-2", "con_x", "mon-com1"] {
            assert!(id_valide(ok), "{ok}");
        }
    }

    #[test]
    fn contrat_coherent() {
        assert_eq!(CONTRAT.iter().filter(|v| v.obligatoire).count(), 12);
        assert_eq!(CONTRAT.iter().filter(|v| !v.obligatoire).count(), 13);
        for (i, v) in CONTRAT.iter().enumerate() {
            assert!(v.nom.starts_with("--cr-"), "{}", v.nom);
            assert!(!v.role.is_empty(), "{}", v.nom);
            assert_eq!(v.obligatoire, v.repli.is_empty(), "{} : repli ssi facultative", v.nom);
            assert!(CONTRAT[i + 1..].iter().all(|w| w.nom != v.nom), "doublon {}", v.nom);
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
        std::fs::write(dir.join("con.css"), "nom réservé Windows").unwrap();
        installer_themes_livres(&dir).unwrap();

        let themes = lister_themes(&dir).unwrap();
        let ids: Vec<_> = themes.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 4, "{ids:?}");
        assert!(ids.contains(&"perso") && ids.contains(&"moderne-sombre"));
        let classique = themes.iter().find(|t| t.id == "classique").unwrap();
        assert_eq!(classique.nom, "Mon classique");
        assert!(classique.livre);
        assert!(!themes.iter().find(|t| t.id == "perso").unwrap().livre);
        assert_eq!(charger_theme(&dir, "classique").unwrap().css, "/* @nom: Mon classique */ :root{}");
    }

    #[test]
    fn liste_avec_couleurs_et_avertissements() {
        let tmp = tempfile::tempdir().unwrap();
        installer_themes_livres(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("incomplet.css"), "/* @nom: Incomplet */ :root { --cr-fond: #000; }").unwrap();
        std::fs::write(tmp.path().join("dangereux.css"), "@import 'http://x/y.css';\n:root{--cr-fond:#fff}").unwrap();
        std::fs::write(tmp.path().join("refuse.css"), ".a{b:\\75rl(x)}").unwrap();
        let themes = lister_themes(tmp.path()).unwrap();

        let clair = themes.iter().find(|t| t.id == "moderne-clair").unwrap();
        let c = clair.couleurs.as_ref().unwrap();
        assert_eq!((c.fond.as_str(), c.texte.as_str()), ("#f5f6f8", "#1d2330"));
        assert!(clair.avertissements.is_empty(), "{:?}", clair.avertissements);

        let inc = themes.iter().find(|t| t.id == "incomplet").unwrap();
        assert!(inc.couleurs.is_none());
        assert!(inc.avertissements.iter().any(|a| a.contains("--cr-texte")));

        let danger = themes.iter().find(|t| t.id == "dangereux").unwrap();
        assert!(danger.avertissements.iter().any(|a| a.contains("@import")));

        let refuse = themes.iter().find(|t| t.id == "refuse").unwrap();
        assert!(refuse.couleurs.is_none());
        assert!(refuse.avertissements.iter().any(|a| a.contains("thème refusé")), "{:?}", refuse.avertissements);
    }

    #[test]
    fn json_en_camel_case() {
        let info = ThemeInfo {
            id: "a".into(),
            nom: "A".into(),
            livre: false,
            couleurs: Some(CouleursTheme { fond: "#fff".into(), accent: "#00f".into(), texte: "#000".into() }),
            avertissements: vec![],
        };
        let v = serde_json::to_value(&info).unwrap();
        assert_eq!(v["couleurs"]["fond"], "#fff");
        assert!(v["avertissements"].is_array() && v["livre"] == false);
        let c = serde_json::to_value(ThemeCharge { css: "x".into(), avertissements: vec![] }).unwrap();
        assert_eq!(c["css"], "x");
        assert!(c["avertissements"].is_array());
    }

    #[test]
    fn chargement_refuse_les_chemins_et_les_absents() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(charger_theme(tmp.path(), "../x"), Err(ErreurTheme::IdInvalide(_))));
        assert!(matches!(charger_theme(tmp.path(), "con"), Err(ErreurTheme::IdInvalide(_))));
        assert!(matches!(charger_theme(tmp.path(), "absent"), Err(ErreurTheme::Introuvable(_))));
    }

    #[test]
    fn chargement_assainit_et_refuse() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.css"), ":root { --cr-fond: #fff; background: url(http://x/y.png); }")
            .unwrap();
        let c = charger_theme(tmp.path(), "a").unwrap();
        assert!(!c.css.contains("url("));
        assert!(c.avertissements.iter().any(|a| a.contains("url()")));
        std::fs::write(tmp.path().join("b.css"), "a{b:c}\0").unwrap();
        assert!(matches!(charger_theme(tmp.path(), "b"), Err(ErreurTheme::Refuse(_))));
        std::fs::write(tmp.path().join("c.css"), "x".repeat(TAILLE_MAX_CSS + 1)).unwrap();
        assert!(matches!(charger_theme(tmp.path(), "c"), Err(ErreurTheme::TropGros(_))));
    }

    #[test]
    fn import_id_libre_sans_ecrasement() {
        let tmp = tempfile::tempdir().unwrap();
        let css = ":root { --cr-fond: #fff; --cr-accent: #00f; --cr-texte: #000; }";
        let a = importer_theme(tmp.path(), "Mon thème", css).unwrap();
        assert_eq!((a.id.as_str(), a.nom.as_str(), a.livre), ("mon-theme", "Mon thème", false));
        assert_eq!(a.couleurs.as_ref().unwrap().accent, "#00f");
        let b = importer_theme(tmp.path(), "Mon thème", css).unwrap();
        assert_eq!(b.id, "mon-theme-2");
        let c = importer_theme(tmp.path(), "MON THEME", css).unwrap();
        assert_eq!(c.id, "mon-theme-3");
        // Nom vide ou sans lettre : mon-theme (déjà pris, donc suffixé).
        assert_eq!(importer_theme(tmp.path(), "  ***  ", css).unwrap().id, "mon-theme-4");
        // Nom d'un thème livré : son identifiant n'est jamais repris.
        let l = importer_theme(tmp.path(), "Classique", css).unwrap();
        assert_eq!(l.id, "classique-2");
        assert!(!l.livre);
        let l2 = importer_theme(tmp.path(), "Classique", css).unwrap();
        assert_eq!(l2.id, "classique-3");
        // Nom réservé Windows.
        assert_eq!(importer_theme(tmp.path(), "CON", css).unwrap().id, "con-2");
        // Le premier fichier est intact.
        let contenu = std::fs::read_to_string(tmp.path().join("mon-theme.css")).unwrap();
        assert!(contenu.starts_with("/* @nom: Mon thème */\n"));
    }

    #[test]
    fn import_conserve_le_nom_du_fichier_et_ecarte_les_injections() {
        let tmp = tempfile::tempdir().unwrap();
        let i = importer_theme(tmp.path(), "x", "/* @nom: Fourni */\n:root { --cr-fond: #fff; }").unwrap();
        assert_eq!(i.nom, "Fourni");
        let contenu = std::fs::read_to_string(tmp.path().join("x.css")).unwrap();
        assert_eq!(contenu.matches("@nom:").count(), 1);
        // Un nom avec `*/` ne peut pas sortir du commentaire.
        let i = importer_theme(tmp.path(), "evil */ a{b:c} /*", ":root{}").unwrap();
        let contenu = std::fs::read_to_string(tmp.path().join(format!("{}.css", i.id))).unwrap();
        assert!(contenu.starts_with("/* @nom: evil  a{b:c} */\n"), "{contenu}");
        assert!(crate::themes_css::valider_css(&contenu).erreurs.is_empty());
    }

    #[test]
    fn import_refuse_et_assainit() {
        let tmp = tempfile::tempdir().unwrap();
        let r = importer_theme(tmp.path(), "Mauvais", ".a{b:\\75rl(http://x)}");
        assert!(matches!(r, Err(ErreurTheme::Refuse(_))));
        assert!(std::fs::read_dir(tmp.path()).unwrap().next().is_none(), "rien d'écrit");
        let i = importer_theme(tmp.path(), "Net", "@import 'x.css';\n:root { --cr-fond: #fff; }").unwrap();
        assert!(i.avertissements.iter().any(|a| a.contains("@import")));
        let contenu = std::fs::read_to_string(tmp.path().join("net.css")).unwrap();
        assert!(!contenu.contains("@import"));
        // Importer dans un dossier qui n'existe pas encore : créé.
        let neuf = tmp.path().join("sous").join("themes");
        assert!(importer_theme(&neuf, "A", ":root{}").is_ok());
    }

    #[test]
    fn identifiants_depuis_nom() {
        assert_eq!(identifiant_depuis_nom("Mon thème"), "mon-theme");
        assert_eq!(identifiant_depuis_nom("  Été  2026 !! "), "ete-2026");
        assert_eq!(identifiant_depuis_nom("Cœur d'Ève"), "coeur-d-eve");
        assert_eq!(identifiant_depuis_nom("../../etc/passwd"), "etc-passwd");
        assert_eq!(identifiant_depuis_nom("日本"), "mon-theme");
        assert!(identifiant_depuis_nom(&"a".repeat(200)).len() <= 48);
    }

    #[test]
    fn export_avec_entete_sans_doublon() {
        let tmp = tempfile::tempdir().unwrap();
        installer_themes_livres(tmp.path()).unwrap();
        let e = exporter_theme(tmp.path(), "classique").unwrap();
        assert!(e.starts_with(ENTETE_EXPORT), "{e}");
        assert!(e.contains("« Classique »") && e.contains("--cr-fond:"));
        assert!(!crate::themes_css::valider_css(&e).css_assaini.is_empty());
        // Ré-import puis ré-export : un seul en-tête.
        let i = importer_theme(tmp.path(), "Copie", &e).unwrap();
        let e2 = exporter_theme(tmp.path(), &i.id).unwrap();
        assert_eq!(e2.matches(ENTETE_EXPORT).count(), 1);
        assert_eq!(i.nom, "Classique");
        assert!(matches!(exporter_theme(tmp.path(), "nul"), Err(ErreurTheme::IdInvalide(_))));
    }

    #[test]
    fn suppression() {
        let tmp = tempfile::tempdir().unwrap();
        installer_themes_livres(tmp.path()).unwrap();
        for (id, _) in THEMES_LIVRES {
            assert!(matches!(supprimer_theme(tmp.path(), id), Err(ErreurTheme::LivreProtege)), "{id}");
            assert!(tmp.path().join(format!("{id}.css")).exists());
        }
        let i = importer_theme(tmp.path(), "Jetable", ":root{}").unwrap();
        supprimer_theme(tmp.path(), &i.id).unwrap();
        assert!(!tmp.path().join("jetable.css").exists());
        assert!(matches!(supprimer_theme(tmp.path(), "jetable"), Err(ErreurTheme::Introuvable(_))));
        assert!(matches!(supprimer_theme(tmp.path(), "../x"), Err(ErreurTheme::IdInvalide(_))));
    }

    #[test]
    fn reinitialisation_sauvegarde_l_ancien_fichier() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        assert_eq!(reinitialiser_themes_livres(d).unwrap().len(), 3, "dossier vide : tout recopié");
        assert!(reinitialiser_themes_livres(d).unwrap().is_empty(), "déjà à jour");
        std::fs::write(d.join("classique.css"), "/* @nom: Ma version */ :root{}").unwrap();
        let remis = reinitialiser_themes_livres(d).unwrap();
        assert_eq!(remis, vec!["classique".to_string()]);
        assert_eq!(std::fs::read_to_string(d.join("classique.css.avant")).unwrap(), "/* @nom: Ma version */ :root{}");
        assert_eq!(std::fs::read_to_string(d.join("classique.css")).unwrap(), THEMES_LIVRES[2].1);
        // Deuxième sauvegarde : l'ancienne `.avant` n'est pas perdue.
        std::fs::write(d.join("classique.css"), "seconde version").unwrap();
        reinitialiser_themes_livres(d).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("classique.css.avant")).unwrap(), "/* @nom: Ma version */ :root{}");
        assert_eq!(std::fs::read_to_string(d.join("classique.css.avant2")).unwrap(), "seconde version");
        // Les fichiers `.avant` ne sont pas des thèmes.
        assert_eq!(lister_themes(d).unwrap().len(), 3);
        // Un livré supprimé revient sans sauvegarde.
        std::fs::remove_file(d.join("moderne-sombre.css")).unwrap();
        assert_eq!(reinitialiser_themes_livres(d).unwrap(), vec!["moderne-sombre".to_string()]);
        assert!(!d.join("moderne-sombre.css.avant").exists());
    }

    #[test]
    fn mise_a_jour_automatique_des_livres_inchanges() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        // Installation d'anciennes versions (une avec fins de ligne Windows et espace final).
        std::fs::write(d.join("moderne-clair.css"), THEMES_ANCIENS[0].1).unwrap();
        std::fs::write(d.join("moderne-sombre.css"), THEMES_ANCIENS[1].1.replace('\n', "\r\n") + "  \r\n").unwrap();
        // Classique modifié par l'utilisateur à partir de l'ancienne version : jamais touché.
        let modifie = THEMES_ANCIENS[2].1.replace("#ece9d8", "#ffeecc");
        std::fs::write(d.join("classique.css"), &modifie).unwrap();
        let maj = installer_themes_livres(d).unwrap();
        assert_eq!(maj, vec!["moderne-clair".to_string(), "moderne-sombre".to_string()]);
        assert_eq!(std::fs::read_to_string(d.join("moderne-clair.css")).unwrap(), THEMES_LIVRES[0].1);
        assert_eq!(std::fs::read_to_string(d.join("moderne-sombre.css")).unwrap(), THEMES_LIVRES[1].1);
        assert_eq!(std::fs::read_to_string(d.join("classique.css")).unwrap(), modifie);
        // Deuxième passage : plus rien à faire.
        assert!(installer_themes_livres(d).unwrap().is_empty());
    }

    #[test]
    fn les_anciennes_versions_sont_bien_anciennes() {
        for ((id, ancien), (id2, nouveau)) in THEMES_ANCIENS.iter().zip(THEMES_LIVRES) {
            assert_eq!(id, id2);
            assert_ne!(normaliser(ancien), normaliser(nouveau), "{id} : l'ancienne version doit différer");
            assert!(!ancien.contains("@contrat"), "{id}");
            assert!(nouveau.contains(&format!("@contrat: {VERSION_CONTRAT}")), "{id}");
        }
    }

    #[test]
    fn commande_reveler_ouvre_le_dossier_des_themes() {
        let dossier = Path::new("data").join("themes");
        let c = commande_reveler_dossier(&dossier, Systeme::Autre);
        assert_eq!(c.programme, "xdg-open");
        assert_eq!(c.arguments, vec![dossier.to_string_lossy().into_owned()]);
        let w = commande_reveler_dossier(Path::new("C:\\app\\data\\themes"), Systeme::Windows);
        assert_eq!(w.programme, "explorer.exe");
        assert_eq!(w.arguments, vec!["\"C:\\app\\data\\themes\"".to_string()]);
        assert!(w.brut);
    }

    #[test]
    fn themes_livres_coherents() {
        for (id, css) in THEMES_LIVRES {
            assert!(id_valide(id));
            assert!(nom_depuis_css(css).is_some(), "{id} sans @nom");
            assert!(css.contains(&format!("/* @contrat: {VERSION_CONTRAT} */")), "{id} sans @contrat");
            // Aucun avertissement ni erreur du filtrage : un thème livré est propre.
            let r = valider_css(css);
            assert!(r.erreurs.is_empty(), "{id} : {:?}", r.erreurs);
            assert!(r.avertissements.is_empty(), "{id} : {:?}", r.avertissements);
            // Sous Windows, git peut livrer les fichiers en CRLF : le filtrage les normalise en LF.
            assert_eq!(r.css_assaini, css.replace("\r\n", "\n"), "{id} : le filtrage ne doit rien changer");
            // Toutes les variables du contrat (obligatoires et facultatives) et color-scheme.
            for v in CONTRAT {
                assert!(css.contains(&format!("{}:", v.nom)), "{id} : variable {} absente", v.nom);
            }
            assert!(css.contains("color-scheme:"), "{id} : color-scheme absent");
            // Un commentaire par rôle : au moins autant de commentaires que de variables.
            assert!(css.matches("/*").count() >= CONTRAT.len(), "{id} : variables non commentées");
        }
        assert!(THEMES_LIVRES.iter().any(|(id, _)| *id == THEME_DEFAUT));
        assert_eq!(THEMES_LIVRES.len(), THEMES_ANCIENS.len());
    }

    #[test]
    fn variables_utilisees_par_le_front_figurent_au_contrat() {
        let racine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frontend/src");
        if !racine.is_dir() {
            return; // dépôt sans front (paquet isolé)
        }
        let mut utilisees = std::collections::BTreeSet::new();
        let mut pile = vec![racine];
        while let Some(d) = pile.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    pile.push(p);
                    continue;
                }
                if !matches!(p.extension().and_then(|x| x.to_str()), Some("svelte" | "ts" | "js" | "css" | "html")) {
                    continue;
                }
                let Ok(texte) = std::fs::read_to_string(&p) else { continue };
                for (i, _) in texte.match_indices("--cr-") {
                    let nom: String = texte[i..]
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                        .collect();
                    // `--cr-${…}` construit dynamiquement : rien à vérifier.
                    if !nom.ends_with('-') {
                        utilisees.insert((nom, p.display().to_string()));
                    }
                }
            }
        }
        for (nom, fichier) in &utilisees {
            assert!(
                CONTRAT.iter().any(|v| v.nom == nom),
                "{nom} (utilisée dans {fichier}) ne figure pas dans CONTRAT : l'ajouter à themes.rs et à docs/tauri/themes.md"
            );
        }
    }

    #[test]
    fn contrastes_wcag_des_themes_livres() {
        for (id, css) in THEMES_LIVRES {
            let v = |nom: &str| variable(css, nom);
            let (fond, panneau, survol) = (v("--cr-fond"), v("--cr-fond-panneau"), v("--cr-fond-survol"));
            let (texte, discret) = (v("--cr-texte"), v("--cr-texte-discret"));
            let (accent, accent_texte, bordure) = (v("--cr-accent"), v("--cr-accent-texte"), v("--cr-bordure"));
            for (nom, premier, second, min) in [
                ("texte/fond", &texte, &fond, 4.5),
                ("texte/fond-panneau", &texte, &panneau, 4.5),
                ("texte/fond-survol", &texte, &survol, 4.5),
                ("texte-discret/fond", &discret, &fond, 4.5),
                ("texte-discret/fond-panneau", &discret, &panneau, 4.5),
                ("accent-texte/accent", &accent_texte, &accent, 4.5),
                ("bordure/fond", &bordure, &fond, 3.0),
            ] {
                let c = contraste(premier, second);
                assert!(c >= min, "{id} : contraste {nom} = {c:.2}:1, minimum {min}:1");
            }
            // Messages d'erreur, de succès et d'avertissement lisibles sur le fond et le panneau.
            for nom in ["--cr-erreur", "--cr-succes", "--cr-avertissement"] {
                for support in [&fond, &panneau] {
                    let c = contraste(&v(nom), support);
                    assert!(c >= 4.5, "{id} : contraste {nom}/fond = {c:.2}:1, minimum 4.5:1");
                }
            }
        }
    }

    #[test]
    fn utilitaires_de_contraste() {
        assert!((contraste("#000000", "#ffffff") - 21.0).abs() < 0.01);
        assert!((contraste("#fff", "#fff") - 1.0).abs() < 1e-9);
    }
}
