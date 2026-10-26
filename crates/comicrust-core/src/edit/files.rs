//! Fichiers : déplacement / renommage d'un livre ([`rename_or_move`]), contrôle des noms
//! (règles de Windows, même sous Linux : la bibliothèque est partagée avec Windows) et
//! modèles de nom ([`expand_template`], [`path_from_template`]) pour ranger les fichiers.
//!
//! # Modèles de nom
//!
//! Syntaxe du format de nom d'export de ComicRack (`ExtendedStringFormater`, réglage
//! `ComicExportFileNameFormat`, par défaut `[{format} ][{series}][ {volume}][ #{number}][ ({year}[/{month}])]`) :
//!
//! - `{champ}` : valeur du champ (nom sans casse : `series`, `number`, `volume`, `year`,
//!   `month`, `day`, `title`, `count`, `format`, `publisher`, `imprint`, `writer`,
//!   `penciller`, `genre`, `storyarc`, `seriesgroup`, `alternateseries`,
//!   `alternatenumber`, `language`, `agerating`, `tags`, `filename` = nom actuel sans
//!   extension, et tout autre champ du catalogue par son nom) ; série, numéro, volume,
//!   année, nombre et format prennent la valeur proposée (déduite du nom du fichier) si
//!   la fiche est vide, comme ComicRack ;
//! - `{{clé}}` : valeur personnalisée ;
//! - `{champ:000}` : nombre complété par des zéros (`{number:000}` → `007`) ;
//! - `[ … ]` : partie optionnelle, omise si un champ qu'elle contient est vide (règle de
//!   ComicRack : une partie imbriquée réussie rend la partie englobante réussie) ;
//! - `\x` : caractère `x` tel quel ;
//! - `/` : séparateur de dossiers (seulement dans [`path_from_template`]).
//!
//! Écarts : les fonctions `$…>` de ComicRack ne sont pas prises en charge (texte
//! tel quel) ; `{volume}` et `{number}` donnent la valeur seule (ComicRack affiche
//! « V3 » et ajoute « (of 12) » au numéro).

use std::path::{Path, PathBuf};

use crate::db::{BookEdit, FileState};
use crate::library::LibraryBook;
use crate::Error;

use super::{custom_value, EditTarget, READ_ONLY_MESSAGE};

/// Modèle de nom par défaut : `Série v2 #12 (2024)`.
pub const DEFAULT_NAME_TEMPLATE: &str = "{series}[ v{volume}][ #{number}][ ({year})]";

/// Format de nom d'export par défaut de ComicRack (`ComicBook.DefaultComicExportFileNameFormat`).
pub const COMICRACK_EXPORT_TEMPLATE: &str = "[{format} ][{series}][ {volume}][ #{number}][ ({year}[/{month}])]";

/// Caractères interdits dans un nom de fichier Windows.
pub const FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Noms réservés de Windows (avec ou sans extension).
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "COM¹", "COM²",
    "COM³", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³", "CONIN$",
    "CONOUT$",
];

/// Longueur maximale d'un élément de chemin (unités UTF-16, NTFS).
pub const MAX_NAME_LEN: usize = 255;

/// Longueur maximale d'un chemin complet : `MAX_PATH` − 1 (ComicRack, programme .NET
/// Framework, ne gère pas les chemins longs).
pub const MAX_PATH_LEN: usize = 259;

fn invalid(msg: String) -> Error {
    Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg))
}

/// Nom de fichier ou de dossier valide sous Windows ? (Message en français sinon.)
pub fn check_file_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("nom vide".into());
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN_CHARS.contains(c) || (*c as u32) < 32) {
        let shown = if (c as u32) < 32 { format!("U+{:04X}", c as u32) } else { c.to_string() };
        return Err(format!("« {name} » : caractère interdit sous Windows ({shown})"));
    }
    if name == "." || name == ".." {
        return Err(format!("« {name} » : nom réservé"));
    }
    if name.ends_with(['.', ' ']) {
        return Err(format!("« {name} » : un nom ne peut pas finir par un point ou une espace sous Windows"));
    }
    let stem = name.split('.').next().unwrap_or("").trim_end();
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return Err(format!("« {name} » : nom réservé par Windows ({stem})"));
    }
    if name.encode_utf16().count() > MAX_NAME_LEN {
        return Err(format!("« {name} » : nom trop long (plus de {MAX_NAME_LEN} caractères)"));
    }
    Ok(())
}

/// Chemin valide sous Windows : chaque élément (hors préfixe de lecteur ou de partage)
/// et la longueur totale. Les éléments `.` et `..` sont refusés (ComicRack compare le
/// texte exact des chemins : `D:\BD\..\BD\x.cbz` serait un autre livre).
pub fn check_path(path: &Path) -> Result<(), String> {
    let text = path.to_str().ok_or_else(|| format!("{} : nom illisible (Unicode invalide)", path.display()))?;
    let parsed = parse_path(text, windows_style(text), true)?;
    for n in &parsed.parts {
        check_file_name(n)?;
    }
    let len = text.encode_utf16().count();
    if len > MAX_PATH_LEN {
        return Err(format!("chemin trop long ({len} caractères, {MAX_PATH_LEN} au plus pour ComicRack)"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Normalisation des chemins (renommages demandés par les scripts, fenêtre « Renommer »)
// ---------------------------------------------------------------------------------------

/// Racine d'un chemin, telle qu'écrite.
#[derive(Debug, Clone, PartialEq)]
enum Root {
    /// Lettre de lecteur (`C:`).
    Drive(String),
    /// Partage réseau `\\serveur\partage` : (serveur, partage).
    Unc(String, String),
    /// `/` (chemin Unix).
    Unix,
    /// Chemin relatif.
    Relative,
}

impl Root {
    fn is_windows(&self) -> bool {
        matches!(self, Root::Drive(_) | Root::Unc(..))
    }

    /// Même racine, sans casse pour une racine Windows ?
    fn same(&self, other: &Root) -> bool {
        match (self, other) {
            (Root::Drive(a), Root::Drive(b)) => a.eq_ignore_ascii_case(b),
            (Root::Unc(s1, p1), Root::Unc(s2, p2)) => same_text_nocase(s1, s2) && same_text_nocase(p1, p2),
            (a, b) => a == b,
        }
    }
}

/// Chemin découpé : racine et éléments (sans séparateurs).
#[derive(Debug, Clone, PartialEq)]
struct ParsedPath {
    root: Root,
    parts: Vec<String>,
}

impl ParsedPath {
    /// Texte du chemin : séparateur `\` pour une racine Windows, `/` sinon.
    fn text(&self) -> String {
        match &self.root {
            Root::Drive(d) => format!("{d}\\{}", self.parts.join("\\")),
            Root::Unc(server, share) => {
                let mut s = format!("\\\\{server}\\{share}");
                for p in &self.parts {
                    s.push('\\');
                    s.push_str(p);
                }
                s
            }
            Root::Unix => format!("/{}", self.parts.join("/")),
            Root::Relative => self.parts.join(if cfg!(windows) { "\\" } else { "/" }),
        }
    }
}

/// Mêmes textes, sans casse (noms de fichiers Windows).
fn same_text_nocase(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

/// Chemin de style Windows (lettre de lecteur ou partage `\\serveur`) ? Sous Windows,
/// tout chemin l'est.
fn windows_style(text: &str) -> bool {
    let b = text.as_bytes();
    cfg!(windows) || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':') || text.starts_with("\\\\")
}

/// Découpe `text`. `windows` : contexte Windows (un chemin commençant par un seul
/// séparateur n'a ni lecteur ni partage : refusé, sauf si `lenient`). `.` et `..` sont
/// toujours refusés.
fn parse_path(text: &str, windows: bool, lenient: bool) -> Result<ParsedPath, String> {
    let b = text.as_bytes();
    let starts_two_seps = text.chars().take(2).filter(|c| is_sep(*c)).count() == 2;
    if starts_two_seps && (text[2..].starts_with('?') || text[2..].starts_with('.')) && text[3..].starts_with(is_sep) {
        return Err(format!("« {text} » : préfixe « \\\\?\\ » ou « \\\\.\\ » non pris en charge"));
    }
    let (root, rest) = if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        if !text[2..].starts_with(is_sep) && text.len() > 2 {
            return Err(format!(
                "« {text} » : chemin relatif à un lecteur ; écrire le chemin complet ({}\\…)",
                &text[..2]
            ));
        }
        (Root::Drive(text[..2].to_string()), text.get(3..).unwrap_or(""))
    } else if starts_two_seps {
        let mut it = text[2..].splitn(3, is_sep);
        let server = it.next().unwrap_or("");
        let share = it.next().unwrap_or("");
        if server.is_empty() || share.is_empty() {
            return Err(format!("« {text} » : partage réseau incomplet (\\\\serveur\\partage\\…)"));
        }
        (Root::Unc(server.to_string(), share.to_string()), it.next().unwrap_or(""))
    } else if text.starts_with(is_sep) {
        if (windows || text.starts_with('\\')) && !lenient {
            return Err(format!(
                "« {text} » : chemin sans lecteur ni partage réseau ; écrire le chemin complet (D:\\BD\\… ou \\\\serveur\\partage\\…)"
            ));
        }
        (Root::Unix, &text[1..])
    } else {
        (Root::Relative, text)
    };
    let mut parts = Vec::new();
    for p in rest.split(is_sep).filter(|p| !p.is_empty()) {
        if p == "." || p == ".." {
            return Err(format!("« {text} » : les éléments « . » et « .. » sont interdits dans un chemin"));
        }
        parts.push(p.to_string());
    }
    Ok(ParsedPath { root, parts })
}

/// Chemin cible d'un renommage, **normalisé**, à écrire tel quel dans la fiche.
///
/// - `requested` : chemin complet, ou relatif au dossier actuel du fichier (`current`) ;
///   séparateurs `/` et `\` acceptés, doublés ou mélangés ;
/// - refusés : éléments `.` et `..`, chemin Windows sans lecteur ni partage (`\BD\…`),
///   préfixes `\\?\` ;
/// - résultat : séparateur `\` pour un chemin Windows (lecteur `C:` ou partage
///   `\\serveur\partage`), `/` sinon ;
/// - **style du chemin d'origine gardé** : ComicRack compare le texte exact des chemins,
///   donc une même racine (`\\SERVEURNAS\BD` demandé `\\serveurnas\bd`) et les mêmes
///   premiers dossiers sont écrits comme dans le chemin actuel du livre (sans casse pour
///   un chemin Windows) ; les nouveaux dossiers et le nom gardent la casse demandée.
pub fn normalize_rename(current: &str, requested: &str) -> Result<String, String> {
    let requested = requested.trim_end();
    if requested.trim().is_empty() {
        return Err("nouveau chemin vide".into());
    }
    let windows = windows_style(current) || windows_style(requested);
    let cur = parse_path(current, windows, true);
    let req = parse_path(requested, windows, false)?;
    let mut out = match req.root {
        Root::Relative => {
            let cur = cur.map_err(|e| format!("emplacement actuel du livre illisible : {e}"))?;
            if cur.root == Root::Relative {
                return Err(format!("emplacement actuel du livre relatif (« {current} »)"));
            }
            let mut parts = cur.parts.clone();
            parts.pop();
            parts.extend(req.parts);
            ParsedPath { root: cur.root, parts }
        }
        Root::Drive(d) => ParsedPath { root: Root::Drive(d.to_ascii_uppercase()), parts: req.parts },
        root => ParsedPath { root, parts: req.parts },
    };
    if out.parts.is_empty() {
        return Err(format!("« {requested} » : nom de fichier manquant"));
    }
    // Style du chemin d'origine : même racine et mêmes premiers dossiers écrits pareil.
    if let Ok(cur) = parse_path(current, windows, true) {
        if out.root.is_windows() && out.root.same(&cur.root) {
            out.root = cur.root.clone();
            let dirs = &cur.parts[..cur.parts.len().saturating_sub(1)];
            let n = out.parts.len() - 1;
            for (i, d) in dirs.iter().enumerate().take(n) {
                if !same_text_nocase(&out.parts[i], d) {
                    break;
                }
                out.parts[i] = d.clone();
            }
        }
    }
    Ok(out.text())
}

/// Rend un texte utilisable comme nom de fichier Windows : `:` → ` -`, `/ \ |` → `-`,
/// `"` → `'`, `< > ? *` et caractères de contrôle retirés, espaces multiples réduits,
/// points et espaces finaux retirés, nom réservé suffixé de `_`, longueur limitée à
/// 200 caractères (place pour l'extension). Peut rendre un texte vide.
pub fn sanitize_file_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ':' => out.push_str(" -"),
            '/' | '\\' | '|' => out.push('-'),
            '"' => out.push('\''),
            '<' | '>' | '?' | '*' => {}
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    let mut t = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.encode_utf16().count() > 200 {
        let mut n = 0;
        t = t
            .chars()
            .take_while(|c| {
                n += c.len_utf16();
                n <= 200
            })
            .collect();
    }
    let t = t.trim_end_matches(['.', ' ']).trim_start().to_string();
    let stem = t.split('.').next().unwrap_or("").trim_end();
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return format!("{t}_");
    }
    t
}

// ---------------------------------------------------------------------------------------
// Modèles de nom
// ---------------------------------------------------------------------------------------

/// Valeur d'un champ du modèle (texte vide si absent).
fn template_value(book: &LibraryBook, name: &str) -> String {
    if let Some(key) = name.strip_prefix('{').and_then(|n| n.strip_suffix('}')) {
        return custom_value(book.custom_values_store.as_deref(), key).unwrap_or_default();
    }
    let num = |v: i32| if v >= 0 { v.to_string() } else { String::new() };
    match name.trim().to_ascii_lowercase().as_str() {
        "series" => book.shadow_series(),
        "number" => {
            let n = book.shadow_number();
            if n == "-" {
                String::new()
            } else {
                n
            }
        }
        "volume" => num(book.shadow_volume()),
        "year" => num(book.shadow_year()),
        "count" => num(book.shadow_count()),
        "format" => book.shadow_format(),
        "language" => book.language_iso.clone().unwrap_or_default(),
        "filename" => book
            .file
            .as_deref()
            .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f))
            .map(|n| n.rsplit_once('.').map_or(n, |(s, _)| s).to_string())
            .unwrap_or_default(),
        other => super::field(other).map(|d| d.text(book)).unwrap_or_default(),
    }
}

/// `{nom}` ou `{nom:000}`.
fn format_value(spec: &str, get: &dyn Fn(&str) -> String) -> String {
    // `{{clé}}` : la clé peut contenir `:`.
    let (name, fmt) = if spec.starts_with('{') {
        (spec, None)
    } else {
        spec.split_once(':').map_or((spec, None), |(n, f)| (n, Some(f)))
    };
    let v = get(name);
    match fmt {
        Some(f) if !v.is_empty() && !f.is_empty() && f.chars().all(|c| c == '0') => {
            match v.trim().replace(',', ".").parse::<f64>() {
                Ok(x) if x.is_finite() => {
                    // Arrondi comme .NET (au plus loin de zéro).
                    let r = x.abs().round() as u64;
                    let sign = if x < 0.0 && r != 0 { "-" } else { "" };
                    format!("{sign}{r:0width$}", width = f.len())
                }
                _ => v,
            }
        }
        _ => v,
    }
}

/// Contenu entre `open` et la fermeture correspondante (imbrications comprises), à
/// partir de `chars[*i] == open` ; `*i` pointe ensuite sur la fermeture.
fn part(chars: &[char], i: &mut usize, open: char, close: char) -> String {
    let mut depth = 0;
    let start = *i + 1;
    while *i < chars.len() {
        let c = chars[*i];
        if c == '\\' {
            *i += 2;
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return chars[start..*i].iter().collect();
            }
        }
        *i += 1;
    }
    chars[start.min(chars.len())..].iter().collect()
}

/// `ExtendedStringFormater.Format` : rend le texte et « réussi » (aucun champ vide).
fn format_part(format: &str, get: &dyn Fn(&str) -> String) -> (String, bool) {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut ok = true;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '[' => {
                let inner = part(&chars, &mut i, '[', ']');
                let (text, ok2) = format_part(&inner, get);
                if ok2 {
                    out.push_str(&text);
                }
                ok |= ok2;
            }
            '{' => {
                let spec = part(&chars, &mut i, '{', '}');
                let v = format_value(&spec, get);
                ok &= !v.is_empty();
                out.push_str(&v);
            }
            '\\' => {
                if let Some(c) = chars.get(i + 1) {
                    out.push(*c);
                }
                i += 1;
            }
            c => out.push(c),
        }
        i += 1;
    }
    (out, ok)
}

/// Texte du modèle pour `book`, sans nettoyage (voir l'en-tête du module).
pub fn expand_template(template: &str, book: &LibraryBook) -> String {
    format_part(template, &|n| template_value(book, n)).0
}

/// Chemin relatif (séparateur `/`) d'après le modèle, **sans extension** : chaque valeur
/// est nettoyée ([`sanitize_file_name`] ; un `/` dans une série ne crée pas de dossier),
/// les `/` du modèle séparent les dossiers, les éléments vides sont retirés. Erreur si
/// le résultat est vide ou invalide.
pub fn path_from_template(template: &str, book: &LibraryBook) -> Result<String, String> {
    // Les valeurs nettoyées ne contiennent plus de `/` : seuls ceux du modèle restent.
    let raw = format_part(template, &|n| sanitize_file_name(&template_value(book, n))).0;
    let parts: Vec<String> =
        raw.split(['/', '\\']).map(sanitize_file_name).filter(|p| !p.is_empty() && p != "." && p != "..").collect();
    if parts.is_empty() {
        return Err(format!("le modèle « {template} » donne un nom vide pour ce livre"));
    }
    for p in &parts {
        check_file_name(p)?;
    }
    Ok(parts.join("/"))
}

/// Chemin cible complet : `dir` + [`path_from_template`] + extension actuelle du fichier.
pub fn target_path(dir: &Path, template: &str, book: &LibraryBook) -> Result<PathBuf, String> {
    let rel = path_from_template(template, book)?;
    let ext = book
        .file
        .as_deref()
        .and_then(|f| f.rsplit(['/', '\\']).next())
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_string());
    let mut p = dir.to_path_buf();
    for part in rel.split('/') {
        p.push(part);
    }
    if let Some(e) = ext {
        let name = format!("{}.{e}", p.file_name().and_then(|n| n.to_str()).unwrap_or(""));
        p.set_file_name(name);
    }
    check_path(&p)?;
    Ok(p)
}

// ---------------------------------------------------------------------------------------
// Déplacement
// ---------------------------------------------------------------------------------------

/// Déplacement fait sur le disque (pour le défaire).
#[derive(Debug, Clone)]
pub struct MovedFile {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Autre volume : copie vérifiée puis suppression de l'original.
    pub copied: bool,
    /// Dossiers créés pour la destination (du plus haut au plus bas).
    pub created_dirs: Vec<PathBuf>,
}

/// Erreur `rename` « autre volume » (EXDEV sous Unix, ERROR_NOT_SAME_DEVICE sous Windows).
fn is_cross_device(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::CrossesDevices {
        return true;
    }
    match e.raw_os_error() {
        Some(code) if cfg!(windows) => code == 17,
        Some(code) => code == 18,
        None => false,
    }
}

/// Mêmes octets ?
fn same_content(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let (mut fa, mut fb) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    if fa.metadata()?.len() != fb.metadata()?.len() {
        return Ok(false);
    }
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let n = fa.read(&mut ba)?;
        if n == 0 {
            return Ok(fb.read(&mut bb[..1])? == 0);
        }
        fb.read_exact(&mut bb[..n])?;
        if ba[..n] != bb[..n] {
            return Ok(false);
        }
    }
}

/// Copie `from` vers `to` (qui ne doit pas exister), vérifie le contenu, garde la date de
/// modification, puis supprime `from`. En cas d'échec, la copie est supprimée.
pub(crate) fn copy_then_delete(from: &Path, to: &Path) -> Result<(), Error> {
    let mut created = false;
    let res = (|| -> Result<(), Error> {
        {
            let mut src = std::fs::File::open(from)?;
            let mut dst = std::fs::OpenOptions::new().write(true).create_new(true).open(to)?;
            created = true;
            std::io::copy(&mut src, &mut dst)?;
            dst.sync_all()?;
            if let Ok(t) = src.metadata().and_then(|m| m.modified()) {
                let _ = dst.set_modified(t);
            }
        }
        if !same_content(from, to)? {
            return Err(Error::Io(std::io::Error::other(format!(
                "copie de {} vers {} différente de l'original",
                from.display(),
                to.display()
            ))));
        }
        std::fs::remove_file(from)?;
        Ok(())
    })();
    // Copie ratée : on ne supprime que le fichier créé ici (jamais une destination qui
    // existait déjà), et seulement si l'original est toujours là.
    if res.is_err() && created && from.exists() {
        let _ = std::fs::remove_file(to);
    }
    res
}

/// Renomme `from` en `to` **sans jamais remplacer** un fichier `to` qui existerait
/// (créé entre le contrôle de [`move_file`] et le renommage) : erreur `AlreadyExists`.
///
/// - Windows : `MoveFileExW` sans `MOVEFILE_REPLACE_EXISTING` (ni copie entre volumes :
///   `ERROR_NOT_SAME_DEVICE`, traité par l'appelant) ;
/// - Linux : `renameat2(RENAME_NOREPLACE)` ; si le système de fichiers ne le permet pas
///   (`EINVAL`, `ENOSYS`), lien dur puis suppression de l'original (le lien échoue si la
///   destination existe) ;
/// - autres Unix : lien dur puis suppression ;
/// - en dernier recours (liens durs impossibles : FAT, certains partages réseau), contrôle
///   d'existence puis `rename` : il reste alors un court instant où une destination créée
///   par un autre programme pourrait être remplacée.
pub fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVE_FILE_FLAGS};
        let wide = |p: &Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
        let (f, t) = (wide(from), wide(to));
        // SAFETY : chaînes UTF-16 terminées par 0, vivantes pendant l'appel.
        let r = unsafe { MoveFileExW(PCWSTR(f.as_ptr()), PCWSTR(t.as_ptr()), MOVE_FILE_FLAGS(0)) };
        r.map_err(|e| {
            let hr = e.code().0 as u32;
            // HRESULT_FROM_WIN32 : code Win32 dans les 16 bits bas.
            if hr & 0xFFFF_0000 == 0x8007_0000 {
                std::io::Error::from_raw_os_error((hr & 0xFFFF) as i32)
            } else {
                std::io::Error::other(e.message())
            }
        })
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStrExt;
            let c = |p: &Path| std::ffi::CString::new(p.as_os_str().as_bytes());
            let (Ok(f), Ok(t)) = (c(from), c(to)) else {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "chemin avec un octet nul"));
            };
            // SAFETY : chaînes C valides, terminées par 0.
            let r = unsafe {
                libc::renameat2(libc::AT_FDCWD, f.as_ptr(), libc::AT_FDCWD, t.as_ptr(), libc::RENAME_NOREPLACE)
            };
            if r == 0 {
                return Ok(());
            }
            let e = std::io::Error::last_os_error();
            match e.raw_os_error() {
                Some(libc::EINVAL) | Some(libc::ENOSYS) | Some(libc::EOPNOTSUPP) => {}
                _ => return Err(e),
            }
        }
        match std::fs::hard_link(from, to) {
            Ok(()) => {
                if let Err(e) = std::fs::remove_file(from) {
                    let _ = std::fs::remove_file(to);
                    return Err(e);
                }
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
            Err(e) if is_cross_device(&e) => Err(e),
            Err(_) => {
                if to.symlink_metadata().is_ok() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        format!("la destination existe déjà : {}", to.display()),
                    ));
                }
                std::fs::rename(from, to)
            }
        }
    }
}

/// Déplace / renomme `from` en `to` sur le disque, avec les contrôles de
/// [`rename_or_move`] (source existante, destination libre et valide), en créant les
/// dossiers manquants.
pub fn move_file(from: &Path, to: &Path) -> Result<MovedFile, Error> {
    if !from.is_file() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("fichier introuvable : {}", from.display()),
        )));
    }
    check_path(to).map_err(invalid)?;
    // Simple changement de casse sous Windows : la « destination » est le fichier lui-même.
    let same = same_file::is_same_file(from, to).unwrap_or(false);
    if same && from == to {
        return Err(invalid(format!("{} : nouvel emplacement identique à l'actuel", to.display())));
    }
    if !same && (to.exists() || to.symlink_metadata().is_ok()) {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("la destination existe déjà : {}", to.display()),
        )));
    }
    let mut created_dirs = Vec::new();
    if let Some(parent) = to.parent().filter(|p| !p.as_os_str().is_empty()) {
        let mut missing = Vec::new();
        let mut d = parent;
        while !d.exists() {
            missing.push(d.to_path_buf());
            match d.parent() {
                Some(p) if !p.as_os_str().is_empty() => d = p,
                _ => break,
            }
        }
        std::fs::create_dir_all(parent)?;
        missing.reverse();
        created_dirs = missing;
    }
    let undo_dirs = |dirs: &[PathBuf]| {
        for d in dirs.iter().rev() {
            let _ = std::fs::remove_dir(d);
        }
    };
    // Simple changement de casse : la destination est le fichier lui-même.
    let renamed = if same { std::fs::rename(from, to) } else { rename_no_replace(from, to) };
    match renamed {
        Ok(()) => Ok(MovedFile { from: from.into(), to: to.into(), copied: false, created_dirs }),
        Err(e) if is_cross_device(&e) => match copy_then_delete(from, to) {
            Ok(()) => Ok(MovedFile { from: from.into(), to: to.into(), copied: true, created_dirs }),
            Err(e) => {
                undo_dirs(&created_dirs);
                Err(e)
            }
        },
        Err(e) => {
            undo_dirs(&created_dirs);
            Err(e.into())
        }
    }
}

/// Défait un déplacement : remet le fichier à sa place et supprime les dossiers créés
/// (s'ils sont vides).
pub fn undo_move(m: &MovedFile) -> Result<(), Error> {
    let same = same_file::is_same_file(&m.to, &m.from).unwrap_or(false);
    let res = if m.copied {
        copy_then_delete(&m.to, &m.from)
    } else if same {
        std::fs::rename(&m.to, &m.from).map_err(Error::from)
    } else {
        rename_no_replace(&m.to, &m.from).map_err(Error::from)
    };
    if res.is_ok() {
        for d in m.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(d);
        }
    }
    res
}

/// Bilan de [`rename_or_move`].
#[derive(Debug, Clone)]
pub struct MoveReport {
    pub moved: MovedFile,
    /// Fiche relue après la mise à jour.
    pub book: LibraryBook,
    /// Base ComicRust : compteur de la ligne écrite.
    pub store_counter: Option<i64>,
    /// Base ComicRack : bilan brut (`Watcher::note_report`, `ComicDatabase::apply_written`).
    pub comicrack: Option<crate::db::WriteReport>,
}

/// Fiche `book` avec son fichier à `path` (taille et dates relues sur le disque).
fn moved_book(book: &LibraryBook, path: &Path) -> Result<(LibraryBook, FileState), Error> {
    let meta = std::fs::metadata(path)?;
    let local = |t: std::io::Result<std::time::SystemTime>| {
        t.ok().map(|t| chrono::DateTime::<chrono::Local>::from(t).fixed_offset())
    };
    let state = FileState {
        path: path.to_string_lossy().into_owned(),
        size: meta.len(),
        modified: local(meta.modified()),
        created: local(meta.created()),
        page_count: None,
        pages: None,
    };
    let mut b = book.clone();
    b.file = Some(state.path.clone());
    b.file_size = Some(meta.len());
    b.file_modified_time = meta.modified().ok().map(crate::scanner::iso_time);
    b.file_creation_time = meta.created().ok().map(crate::scanner::iso_time);
    Ok((b, state))
}

/// Déplace / renomme le fichier du livre (`book.file`) vers `new_path`, puis met à jour
/// la fiche (fichier, taille, dates) dans la base de `target`.
///
/// - Refusé si la bibliothèque est en lecture seule, si la source n'existe pas, si la
///   destination existe, ou si le nouveau chemin est invalide sous Windows (caractères
///   `<>:"/\|?*`, noms réservés `CON`, `PRN`, `COM1`…, point ou espace final, plus de
///   255 caractères par élément ou [`MAX_PATH_LEN`] au total).
/// - Le nouveau chemin est d'abord normalisé ([`normalize_rename`]) : c'est ce texte qui
///   est écrit dans la fiche (`.`/`..` et chemins Windows sans lecteur refusés).
/// - Même volume : `rename` sans remplacement ([`rename_no_replace`]) ; autre volume : copie vérifiée (octet par octet) puis
///   suppression de l'original. Les dossiers manquants sont créés.
/// - Si la mise à jour de la base échoue, le fichier est **remis à sa place** (et les
///   dossiers créés supprimés) ; l'erreur rendue le dit.
pub fn rename_or_move(target: EditTarget<'_>, book: &LibraryBook, new_path: &Path) -> Result<MoveReport, Error> {
    if matches!(target, EditTarget::ReadOnly) {
        return Err(Error::Conflict(READ_ONLY_MESSAGE.into()));
    }
    let from = PathBuf::from(
        book.file
            .as_deref()
            .filter(|f| !f.is_empty())
            .ok_or_else(|| invalid(format!("livre {} sans fichier", book.id)))?,
    );
    // Chemin normalisé (séparateurs, style du chemin d'origine) : c'est lui qui est écrit
    // dans la fiche.
    let normalized = normalize_rename(&from.to_string_lossy(), &new_path.to_string_lossy()).map_err(invalid)?;
    let new_path = Path::new(&normalized);
    let moved = move_file(&from, new_path)?;
    let undo_after = |e: Error, moved: &MovedFile| -> Error {
        match undo_move(moved) {
            Ok(()) => Error::Database(format!("fiche non mise à jour, fichier remis à sa place : {e}")),
            Err(u) => Error::Database(format!(
                "fiche non mise à jour ({e}) et fichier impossible à remettre à sa place ({u}) : il est maintenant à {}",
                moved.to.display()
            )),
        }
    };
    let (new_book, state) = match moved_book(book, new_path) {
        Ok(x) => x,
        Err(e) => return Err(undo_after(e, &moved)),
    };
    match target {
        EditTarget::Store(store) => match store.set_file_location(&new_book) {
            Ok((counter, book)) => Ok(MoveReport { moved, book, store_counter: Some(counter), comicrack: None }),
            Err(e) => Err(undo_after(e, &moved)),
        },
        EditTarget::ComicRack(cfg) => {
            let edit = BookEdit::new(&book.id).file(&state);
            match crate::db::update_books(cfg, &[edit]) {
                Ok(w) if w.written.len() == 1 => {
                    let book = w.written[0].book.clone();
                    Ok(MoveReport { moved, book, store_counter: None, comicrack: Some(w) })
                }
                Ok(w) => {
                    let why = if !w.missing.is_empty() {
                        format!("livre {} absent de la base ComicRack", book.id)
                    } else if !w.unchanged.is_empty() {
                        "fiche ComicRack déjà à cet emplacement".into()
                    } else if !w.failed.is_empty() {
                        w.failed.iter().map(|(_, m)| m.as_str()).collect::<Vec<_>>().join(" ; ")
                    } else {
                        "fiche modifiée en même temps dans ComicRack".into()
                    };
                    Err(undo_after(Error::Database(why), &moved))
                }
                Err(e) => Err(undo_after(e, &moved)),
            }
        }
        EditTarget::ReadOnly => unreachable!("refusé plus haut"),
    }
}
