//! Écriture des listes dans le `ComicDb.xml` de ComicRack (mode « avec ComicRack »).
//!
//! ComicRack garde ses listes dans `ComicDb.xml` même quand ses livres sont dans MySQL
//! (`DatabaseManager.Save` : `ComicDatabase.Attach(…, withBooks: false).SaveXml`). Il lit
//! ce fichier **une seule fois**, à son démarrage, et le **réécrit entièrement** :
//! - à sa fermeture (`DatabaseManager.Dispose` → `Save`) ;
//! - en arrière-plan toutes les `DatabaseBackgroundSaving` secondes (600 par défaut,
//!   `ExtendedSettings`) si la base a changé (`SaveInBackground`, `IsDirty`).
//!
//! Une modification faite pendant que ComicRack tourne serait donc écrasée (et ComicRack
//! ne la verrait pas) : [`ComicDbLists::save`] **refuse d'écrire si ComicRack est lancé**
//! ([`comicrack_running`]), ou si cela ne peut pas être vérifié. La vérification est faite
//! au début **et juste avant le remplacement du fichier**.
//!
//! # Sécurité de l'écriture
//!
//! 1. Le fichier doit être **identique** à celui lu par [`ComicDbLists::read`] (taille et
//!    empreinte du contenu) : sinon refus (« modifié depuis la lecture »).
//! 2. Seul le contenu de `<ComicLists>…</ComicLists>` est remplacé, dans le texte ; tout le
//!    reste du fichier (en-tête, BOM, `<Books>`, `<WatchFolders>`, réglages…) est gardé
//!    octet pour octet.
//! 3. Le nouveau texte est relu avec [`parse_comic_lists`] et comparé aux listes attendues
//!    avant toute écriture : en cas d'écart ou d'erreur, rien n'est écrit.
//!    Contrôle aussi qu'aucun caractère interdit en XML 1.0 n'est écrit
//!    (`XmlSerializer` refuserait tout le fichier).
//! 4. Sauvegarde du fichier d'origine à côté de lui :
//!    `ComicDb.comicrust-AAAAMMJJ-HHMMSS.xml.bak` (les 10 plus récentes sont gardées), et
//!    la toute première sauvegarde faite par ComicRust, gardée à part et jamais supprimée :
//!    `ComicDb.comicrust-origine-AAAAMMJJ-HHMMSS.xml.bak`. Une sauvegarde impossible
//!    empêche l'écriture ; une ancienne sauvegarde impossible à supprimer est signalée
//!    ([`SaveReport::warnings`]).
//! 5. Nouvelle vérification que ComicRack n'est pas lancé, écriture dans un fichier
//!    temporaire du même dossier, puis remplacement atomique (`rename`).

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

use super::{assemble_lists, split_lists, ListRow};
use crate::comic_lists::{local, parse_comic_lists, xml_err};
use crate::Error;

/// Nombre de sauvegardes `*.comicrust-*.xml.bak` gardées.
pub const BACKUPS_KEPT: usize = 10;

/// Message affiché quand ComicRack est lancé.
pub const COMICRACK_RUNNING: &str =
    "Ferme ComicRack pour modifier ses listes : il réécrit ComicDb.xml en se fermant (et toutes les 10 minutes) \
     et effacerait les modifications.";

/// Début du message quand la présence de ComicRack ne peut pas être vérifiée.
pub const COMICRACK_UNKNOWN: &str = "Impossible de vérifier si ComicRack est lancé";

/// ComicRack est-il lancé sur ce poste (processus `ComicRack.exe`) ? `Err` : impossible
/// de le savoir (liste des processus illisible) ; l'appelant doit alors faire comme s'il
/// l'était. Toujours `Ok(false)` hors de Windows.
pub fn comicrack_running() -> Result<bool, String> {
    #[cfg(windows)]
    {
        windows_process_running("comicrack.exe")
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

/// Refus d'écrire si ComicRack est lancé, ou si on ne peut pas le savoir.
pub fn ensure_comicrack_closed(running: &dyn Fn() -> Result<bool, String>) -> Result<(), Error> {
    match running() {
        Ok(false) => Ok(()),
        Ok(true) => Err(Error::Conflict(COMICRACK_RUNNING.into())),
        Err(e) => {
            Err(Error::Conflict(format!("{COMICRACK_UNKNOWN} ({e}) : par prudence, ses listes ne sont pas modifiées.")))
        }
    }
}

#[cfg(windows)]
fn windows_process_running(exe: &str) -> Result<bool, String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    // SAFETY : instantané des processus, parcouru avec une structure dont `dwSize` est
    // renseigné, puis fermé.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| format!("liste des processus : {e}"))?;
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        // Premier processus illisible : liste inutilisable, rien n'est affirmé.
        if let Err(e) = Process32FirstW(snap, &mut entry) {
            let _ = CloseHandle(snap);
            return Err(format!("liste des processus illisible : {e}"));
        }
        let mut ok = true;
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case(exe) {
                found = true;
                break;
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        Ok(found)
    }
}

/// Empreinte d'un contenu (taille + hachage SipHash à clés fixes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    hash: u64,
}

impl Stamp {
    fn of(bytes: &[u8]) -> Self {
        // `DefaultHasher::new()` : même résultat pour toutes les instances du processus.
        let mut h = std::hash::DefaultHasher::new();
        bytes.hash(&mut h);
        Stamp { len: bytes.len() as u64, hash: h.finish() }
    }
}

/// Position du contenu de `<ComicLists>` dans le texte : `(début, fin)` du contenu, ou
/// élément vide `<ComicLists />` (`(début, fin)` de l'élément entier, `true`).
fn comic_lists_span(text: &str) -> Result<(usize, usize, bool), Error> {
    // Marque d'ordre des octets (UTF-8 avec BOM, comme l'écrit ComicRack) : sautée.
    let bom = if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let (a, b, empty) = span_in(&text[bom..])?;
    Ok((a + bom, b + bom, empty))
}

fn span_in(text: &str) -> Result<(usize, usize, bool), Error> {
    let mut r = Reader::from_str(text);
    let mut depth = 0;
    loop {
        let before = r.buffer_position() as usize;
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 && local(e.name().as_ref()) == b"ComicLists" {
                    let start = r.buffer_position() as usize;
                    // Fin du contenu : début de la balise fermante.
                    let mut d = 0;
                    loop {
                        let pos = r.buffer_position() as usize;
                        match r.read_event().map_err(xml_err)? {
                            Event::Start(_) => d += 1,
                            Event::End(_) if d == 0 => return Ok((start, pos, false)),
                            Event::End(_) => d -= 1,
                            Event::Eof => return Err(xml_err("<ComicLists> non fermé")),
                            _ => {}
                        }
                    }
                }
                if depth == 2 {
                    let e = e.into_owned();
                    r.read_to_end(e.name()).map_err(xml_err)?;
                    depth -= 1;
                }
            }
            Event::Empty(e) if depth == 1 && local(e.name().as_ref()) == b"ComicLists" => {
                return Ok((before, r.buffer_position() as usize, true));
            }
            Event::End(_) => depth -= 1,
            Event::Eof => return Err(xml_err("élément <ComicLists> absent")),
            _ => {}
        }
    }
}

/// Remplace le contenu de `<ComicLists>` de `text` (un `ComicDb.xml` complet) par `inner` ;
/// tout le reste est gardé octet pour octet.
pub fn replace_comic_lists(text: &str, inner: &str) -> Result<String, Error> {
    let (a, b, empty) = comic_lists_span(text)?;
    Ok(if empty {
        format!("{}<ComicLists>{inner}</ComicLists>{}", &text[..a], &text[b..])
    } else {
        format!("{}{inner}{}", &text[..a], &text[b..])
    })
}

/// Listes d'un `ComicDb.xml`, lues pour être modifiées puis réenregistrées.
#[derive(Debug, Clone)]
pub struct ComicDbLists {
    pub path: PathBuf,
    /// Listes lues (point de départ de [`super::ListsEditor`]).
    pub rows: Vec<ListRow>,
    /// Avertissements du découpage (identifiant absent ou en double).
    pub warnings: Vec<String>,
    stamp: Stamp,
}

/// Résultat d'un enregistrement.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveReport {
    /// Sauvegarde du fichier d'origine.
    pub backup: PathBuf,
    /// Problèmes sans conséquence sur l'écriture (ancienne sauvegarde impossible à
    /// supprimer…), à montrer.
    pub warnings: Vec<String>,
}

impl ComicDbLists {
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = std::fs::read(path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Database(format!("{} n'est pas en UTF-8", path.display())))?;
        let mut warnings = Vec::new();
        let rows = split_lists(text, &mut warnings)?;
        Ok(ComicDbLists { path: path.to_path_buf(), rows, warnings, stamp: Stamp::of(&bytes) })
    }

    /// Enregistre `rows` dans le fichier (voir le module) ; refuse si ComicRack est lancé.
    pub fn save(&self, rows: &[ListRow]) -> Result<SaveReport, Error> {
        self.save_checked(rows, comicrack_running)
    }

    /// Comme [`ComicDbLists::save`], avec la détection de ComicRack fournie (tests).
    pub fn save_checked(
        &self,
        rows: &[ListRow],
        running: impl Fn() -> Result<bool, String>,
    ) -> Result<SaveReport, Error> {
        ensure_comicrack_closed(&running)?;
        let bytes = std::fs::read(&self.path)?;
        if Stamp::of(&bytes) != self.stamp {
            return Err(Error::Conflict(format!(
                "{} a été modifié depuis sa lecture (par ComicRack ?) : recharge les listes puis recommence",
                self.path.display()
            )));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Database(format!("{} n'est pas en UTF-8", self.path.display())))?;
        let inner = assemble_lists(rows);
        let new_text = replace_comic_lists(text, &inner)?;
        // Relecture avant de toucher au fichier.
        let expected = parse_comic_lists(&super::ListsEditor::new(rows.to_vec()).document())?;
        let reread = parse_comic_lists(&new_text)
            .map_err(|e| Error::Database(format!("listes non enregistrées, résultat illisible : {e}")))?;
        if reread != expected {
            return Err(Error::Database(
                "listes non enregistrées : la relecture ne donne pas les listes attendues".into(),
            ));
        }
        super::xml::check_xml_document(&new_text, "ComicDb.xml")?;
        let (backup, warnings) = backup(&self.path, &bytes)?;
        // ComicRack a pu être lancé pendant la préparation : dernière vérification.
        write_atomic(&self.path, new_text.as_bytes(), &|| ensure_comicrack_closed(&running))?;
        Ok(SaveReport { backup, warnings })
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "ComicDb".into())
}

/// Préfixe (en minuscules) de la première sauvegarde, jamais supprimée.
const ORIGIN: &str = "origine-";

/// Copie `bytes` (contenu de `path`) dans `<nom>.comicrust-AAAAMMJJ-HHMMSS.xml.bak` et
/// ne garde que les [`BACKUPS_KEPT`] plus récentes ; la première sauvegarde de ComicRust
/// est gardée à part ([`origin_backup`]). Renvoie la sauvegarde et les avertissements.
fn backup(path: &Path, bytes: &[u8]) -> Result<(PathBuf, Vec<String>), Error> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = file_stem(path);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut warnings = Vec::new();
    let write = |target: &Path| -> Result<(), Error> {
        std::fs::write(target, bytes).map_err(|e| {
            Error::Database(format!(
                "sauvegarde de {} impossible ({}) : {e} — rien n'a été modifié",
                path.display(),
                target.display()
            ))
        })
    };
    if origin_backup(dir, &stem).is_none() {
        // Première sauvegarde de ComicRust : la plus ancienne existante (versions
        // précédentes) devient l'origine, sinon une copie du fichier actuel.
        match backups(dir, &stem).first() {
            Some(oldest) => {
                let name = oldest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let rest = name.get(format!("{stem}.comicrust-").len()..).unwrap_or("").to_string();
                let target = dir.join(format!("{stem}.comicrust-{ORIGIN}{rest}"));
                if let Err(e) = std::fs::rename(oldest, &target) {
                    warnings.push(format!("sauvegarde d'origine non créée ({}) : {e}", target.display()));
                }
            }
            None => write(&dir.join(format!("{stem}.comicrust-{ORIGIN}{stamp}.xml.bak")))?,
        }
    }
    let mut target = dir.join(format!("{stem}.comicrust-{stamp}.xml.bak"));
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("{stem}.comicrust-{stamp}-{n}.xml.bak"));
        n += 1;
    }
    write(&target)?;
    warnings.extend(prune_backups(dir, &stem, BACKUPS_KEPT));
    Ok((target, warnings))
}

fn backup_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Sauvegardes de `stem` dans `dir` (hors sauvegarde d'origine), de la plus ancienne à la
/// plus récente.
pub(crate) fn backups(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let prefix = format!("{stem}.comicrust-").to_lowercase();
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = backup_name(p);
                    n.strip_prefix(&prefix).is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
                        && n.ends_with(".xml.bak")
                })
                .collect()
        })
        .unwrap_or_default();
    // Le nom porte la date (AAAAMMJJ-HHMMSS) : l'ordre alphabétique est chronologique.
    v.sort_by_key(|p| backup_name(p));
    v
}

/// Sauvegarde d'origine (la plus ancienne s'il y en a plusieurs).
pub(crate) fn origin_backup(dir: &Path, stem: &str) -> Option<PathBuf> {
    let prefix = format!("{stem}.comicrust-{ORIGIN}").to_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = backup_name(p);
            n.starts_with(&prefix) && n.ends_with(".xml.bak")
        })
        .min_by_key(|p| backup_name(p))
}

/// Supprime les sauvegardes au-delà des `keep` plus récentes ; renvoie les échecs.
fn prune_backups(dir: &Path, stem: &str, keep: usize) -> Vec<String> {
    let all = backups(dir, stem);
    let mut warnings = Vec::new();
    if all.len() > keep {
        for p in &all[..all.len() - keep] {
            if let Err(e) = std::fs::remove_file(p) {
                warnings.push(format!("ancienne sauvegarde non supprimée ({}) : {e}", p.display()));
            }
        }
    }
    warnings
}

/// Écrit dans un fichier temporaire du même dossier puis remplace `path` (`rename` :
/// atomique sur un même volume, `MoveFileEx` avec remplacement sous Windows).
/// `before_replace` est appelé juste avant le remplacement : une erreur l'annule.
fn write_atomic(path: &Path, bytes: &[u8], before_replace: &dyn Fn() -> Result<(), Error>) -> Result<(), Error> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp = dir.join(format!(".{}.comicrust-tmp", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
    let res = (|| -> Result<(), Error> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        before_replace()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}
