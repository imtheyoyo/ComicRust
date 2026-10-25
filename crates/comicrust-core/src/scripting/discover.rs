//! Découverte des scripts : lecture du **texte** des fichiers `.py` (rien n'est exécuté).
//!
//! Comme `PythonPluginInitializer` de ComicRack : les commentaires `#@Nom valeur` situés
//! au-dessus d'un `def` décrivent ce point d'entrée. Plusieurs fonctions par fichier.
//!
//! ```text
//! #@Name Nettoyer les séries
//! #@Hook Books
//! #@Description Retire les espaces en trop…
//! #@Key nettoyer-series
//! #@Image nettoyer.png
//! def nettoyer(books): ...
//! ```
//!
//! Différences voulues avec ComicRack : seul un `def` en début de ligne (fonction de
//! premier niveau) clôt un bloc d'en-têtes ; les en-têtes inconnus ou mal formés sont
//! signalés (avertissements) au lieu d'être ignorés en silence.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::Hook;

/// Taille maximale d'un fichier de script examiné.
const MAX_SCRIPT_BYTES: u64 = 4 * 1024 * 1024;
/// Profondeur maximale des sous-dossiers.
const MAX_DEPTH: usize = 8;

/// Un point d'entrée (une fonction) d'un script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEntry {
    /// Fichier `.py`.
    pub file: PathBuf,
    /// Chemin relatif au dossier des scripts, séparateurs `/` (ex. `rangement/series.py`).
    pub rel_path: String,
    /// Fonction Python appelée.
    pub function: String,
    /// `#@Name` : libellé dans les menus.
    pub name: String,
    /// `#@Hook` (plusieurs possibles, séparés par des virgules).
    pub hooks: Vec<Hook>,
    /// `#@Key` : identifiant stable choisi par l'auteur.
    pub key: Option<String>,
    /// `#@Description` (plusieurs lignes possibles, mises bout à bout).
    pub description: Option<String>,
    /// `#@Image` : image du menu, relative au dossier du script.
    pub image: Option<PathBuf>,
    /// Ligne (1 = première) du `def`.
    pub line: usize,
}

impl ScriptEntry {
    /// Identifiant stable du point d'entrée (pour le réglage « script désactivé ») :
    /// `#@Key` s'il existe, sinon `chemin/relatif.py:fonction`.
    pub fn id(&self) -> String {
        match &self.key {
            Some(k) => k.clone(),
            None => format!("{}:{}", self.rel_path, self.function),
        }
    }

    /// Clé des réglages du script : propre au **fichier** (la fonction `ConfigScript` et
    /// les fonctions du menu d'un même fichier partagent leurs réglages).
    pub fn settings_key(&self) -> String {
        self.rel_path.clone()
    }

    pub fn has_hook(&self, hook: Hook) -> bool {
        self.hooks.contains(&hook)
    }
}

/// Problème relevé pendant la découverte (affiché dans la fenêtre des scripts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryWarning {
    pub file: PathBuf,
    /// Ligne concernée (0 = fichier entier).
    pub line: usize,
    pub message: String,
}

/// Points d'entrée des scripts du dossier `dir` et de ses sous-dossiers (triés par
/// chemin puis par ligne). Un dossier absent donne une liste vide.
pub fn discover(dir: &Path) -> Vec<ScriptEntry> {
    discover_with_warnings(dir).0
}

/// Comme [`discover`], avec les avertissements (en-têtes mal formés, fichiers illisibles…).
///
/// Ignorés : dossiers cachés (`.xxx`), `__pycache__` et le dossier `lib` de premier niveau
/// (modules partagés importables par les scripts, sans points d'entrée).
pub fn discover_with_warnings(dir: &Path) -> (Vec<ScriptEntry>, Vec<DiscoveryWarning>) {
    let mut files = Vec::new();
    collect(dir, dir, 0, &mut files);
    files.sort();
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(dir)
            .unwrap_or(&file)
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        match std::fs::metadata(&file) {
            Ok(m) if m.len() > MAX_SCRIPT_BYTES => {
                warnings.push(DiscoveryWarning {
                    file: file.clone(),
                    line: 0,
                    message: "fichier trop gros, ignoré".into(),
                });
                continue;
            }
            Err(e) => {
                warnings.push(DiscoveryWarning { file: file.clone(), line: 0, message: format!("illisible : {e}") });
                continue;
            }
            _ => {}
        }
        let bytes = match std::fs::read(&file) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(DiscoveryWarning { file: file.clone(), line: 0, message: format!("illisible : {e}") });
                continue;
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(DiscoveryWarning {
                    file: file.clone(),
                    line: 0,
                    message: "texte non UTF-8 (les caractères invalides sont remplacés)".into(),
                });
                String::from_utf8_lossy(e.as_bytes()).into_owned()
            }
        };
        let (mut e, w) = parse_script(&text, &file, &rel);
        entries.append(&mut e);
        warnings.extend(w);
    }
    (entries, warnings)
}

fn collect(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            let skip = name.starts_with('.')
                || name == "__pycache__"
                || (depth == 0 && name.eq_ignore_ascii_case("lib") && dir == root);
            if !skip {
                collect(root, &path, depth + 1, out);
            }
        } else if ft.is_file() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("py")) {
            out.push(path);
        }
    }
}

/// En-têtes accumulés au-dessus d'un `def`.
#[derive(Default)]
struct Pending {
    first_line: usize,
    name: Option<String>,
    hooks: Vec<Hook>,
    hook_given: bool,
    key: Option<String>,
    description: Vec<String>,
    image: Option<String>,
}

/// Analyse le texte d'un script (BOM UTF-8 accepté). `rel_path` : chemin relatif affiché.
pub fn parse_script(text: &str, file: &Path, rel_path: &str) -> (Vec<ScriptEntry>, Vec<DiscoveryWarning>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    let mut pending: Option<Pending> = None;
    let warn = |warnings: &mut Vec<DiscoveryWarning>, line: usize, message: String| {
        warnings.push(DiscoveryWarning { file: file.to_path_buf(), line, message });
    };
    let base_dir = file.parent().map(Path::to_path_buf).unwrap_or_default();

    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let trimmed = raw.trim();
        // En-tête : `#@Nom valeur` (espaces tolérés entre `#` et `@`).
        if let Some(rest) = trimmed.strip_prefix('#') {
            let rest = rest.trim_start();
            if let Some(attr) = rest.strip_prefix('@') {
                let (name, value) = match attr.find(char::is_whitespace) {
                    Some(p) => (&attr[..p], attr[p..].trim()),
                    None => (attr, ""),
                };
                let p = pending.get_or_insert_with(|| Pending { first_line: n, ..Default::default() });
                if value.is_empty() {
                    warn(&mut warnings, n, format!("en-tête « #@{name} » sans valeur"));
                    continue;
                }
                match name.to_ascii_lowercase().as_str() {
                    "name" => p.name = Some(value.to_string()),
                    "hook" => {
                        p.hook_given = true;
                        for h in value.split(',').map(str::trim).filter(|h| !h.is_empty()) {
                            match Hook::parse(h) {
                                Some(hook) if !p.hooks.contains(&hook) => p.hooks.push(hook),
                                Some(_) => {}
                                None => warn(&mut warnings, n, format!("hook inconnu « {h} »")),
                            }
                        }
                    }
                    "key" => p.key = Some(value.to_string()),
                    "description" => p.description.push(value.to_string()),
                    "image" => p.image = Some(value.to_string()),
                    // Attributs de ComicRack sans objet ici : ignorés sans bruit.
                    "pcount" | "enabled" | "editor" => {}
                    _ => warn(&mut warnings, n, format!("en-tête inconnu « #@{name} »")),
                }
                continue;
            }
            continue;
        }
        // `def` de premier niveau (pas d'indentation) ; `async def` refusé.
        let def = raw.strip_prefix("def ").or_else(|| raw.strip_prefix("def\t"));
        let async_def = raw.starts_with("async def ");
        if def.is_none() && !async_def {
            continue;
        }
        let Some(p) = pending.take() else { continue };
        if async_def {
            warn(&mut warnings, n, "fonction « async » non prise en charge".into());
            continue;
        }
        let fname: String =
            def.unwrap_or("").trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if fname.is_empty() || fname.starts_with(|c: char| c.is_ascii_digit()) {
            warn(&mut warnings, n, "nom de fonction illisible".into());
            continue;
        }
        let Some(name) = p.name else {
            warn(&mut warnings, n, format!("fonction « {fname} » : en-tête #@Name manquant"));
            continue;
        };
        if p.hooks.is_empty() {
            let why = if p.hook_given { "aucun hook reconnu" } else { "en-tête #@Hook manquant" };
            warn(&mut warnings, n, format!("fonction « {fname} » : {why}"));
            continue;
        }
        entries.push(ScriptEntry {
            file: file.to_path_buf(),
            rel_path: rel_path.to_string(),
            function: fname,
            name,
            hooks: p.hooks,
            key: p.key,
            description: (!p.description.is_empty()).then(|| p.description.join("\n")),
            image: p.image.map(|i| base_dir.join(i)),
            line: n,
        });
    }
    if let Some(p) = pending {
        warn(&mut warnings, p.first_line, "en-têtes sans fonction (« def ») à la suite".into());
    }
    (entries, warnings)
}

/// Points d'entrée d'un hook, sans ceux que l'utilisateur a désactivés (identifiants
/// [`ScriptEntry::id`]).
pub fn entries_for_hook<'a>(
    entries: &'a [ScriptEntry],
    hook: Hook,
    disabled: &'a HashSet<String>,
) -> impl Iterator<Item = &'a ScriptEntry> + 'a {
    entries.iter().filter(move |e| e.has_hook(hook) && !disabled.contains(&e.id()))
}
