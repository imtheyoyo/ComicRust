//! Recherche des BD dans des dossiers et mise à jour de la bibliothèque autonome
//! (équivalent des « dossiers surveillés » de ComicRack : `ComicScanner`,
//! `ComicBookFactory.Create`, `ComicBook.RefreshInfoFromFile`).
//!
//! Deux entrées :
//! - [`book_from_file`] : fiche d'un fichier (ComicInfo.xml, nombre de pages, taille, dates) ;
//! - [`scan`] : parcours incrémental de dossiers comparé aux livres déjà connus, qui
//!   produit un [`ScanReport`] (ajoutés, mis à jour, déplacés, manquants, revenus, erreurs).
//!   Le module n'écrit rien : c'est l'appelant (le `store`) qui applique le rapport.
//!
//! ## Ce qui est lu dans un fichier
//!
//! Seulement la liste des entrées et le `ComicInfo.xml`, jamais les images :
//! - ZIP/CBZ : répertoire central + l'entrée ComicInfo.xml ;
//! - RAR/CBR : liste des entrées, puis une passe qui saute tout jusqu'au ComicInfo.xml
//!   (dans une archive solide, UnRAR doit décompresser ce qui précède, mais rien n'est
//!   gardé en mémoire ; on s'arrête dès que le ComicInfo.xml est lu) ;
//! - 7z/CB7 : en-tête + le seul bloc qui contient le ComicInfo.xml ;
//! - TAR : index des entrées + ComicInfo.xml (lecture directe) ;
//! - PDF / DjVu : nombre de pages (pas de ComicInfo.xml) ;
//! - WebComic (`.cbw`) : le fichier XML seul (bloc `<Info>`), sans rien télécharger ;
//!   le nombre de pages reste inconnu.
//!
//! ## Champs d'une fiche
//!
//! Viennent **du fichier** (remplacés à chaque relecture) : tous les champs ComicInfo
//! ([`COMIC_INFO_FIELDS`] : série, numéro, volume, titre, auteurs, résumé, notes, genre,
//! tags, langue, format, âge, N&B, manga, personnages, équipes, lieux, note de la
//! communauté…), la liste des pages et leur type (`Pages`), `PageCount` (nombre réel de
//! pages du fichier, pas la valeur écrite dans le ComicInfo.xml), `FileSize`,
//! `FileModifiedTime`, `FileCreationTime`.
//!
//! Sont **propres à la bibliothèque** (gardés lors d'une relecture) : `Id`, `File`
//! (sauf déplacement), `Added`, `Opened`, `OpenCount`, `CurrentPage`, `LastPageRead`
//! (ramenées sous le nouveau nombre de pages comme `ComicBook.SetInfo`), `Rating`,
//! `Checked`, `Released`, `SeriesComplete`, `PreferredFrontCover`, `EnableProposed`,
//! `ComicInfoIsDirty`, `ComicBookIsDirty`, `ISBN` et tous les `Book*` (âge, état,
//! emplacement, notes, propriétaire, magasin, prix, statut de collection).
//!
//! Comme ComicRack (`RefreshInfoFromFile` ne relit pas les infos si `ComicInfoIsDirty`
//! ou `ComicBookIsDirty`), une fiche modifiée dans la bibliothèque mais pas encore
//! écrite dans le fichier garde ses champs ComicInfo et ses pages : seuls taille, dates
//! et nombre de pages sont rafraîchis.
//!
//! Écart volontaire : sans « forcer », ComicRack ne remplit que les champs vides
//! (`SetInfo(ci, onlyUpdateEmpty: true)`). Ici les champs ComicInfo sont **remplacés**
//! par ceux du fichier quand celui-ci a changé, pour que les modifications faites par un
//! autre outil (ComicTagger, ComicRack…) soient prises en compte.
//!
//! ## Dates
//!
//! Écrites en ISO 8601 avec fuseau, à la seconde, en heure locale
//! (`2026-10-23T14:05:09+02:00`), ce que lisent `smart.rs` (10 premiers caractères) et le
//! tri de l'interface (ordre du texte). Pour savoir si un fichier a changé, la date
//! enregistrée est relue (`+02:00`, `Z`, fractions de seconde à la ComicRack, ou sans
//! fuseau = UTC) et comparée à la seconde près avec celle du fichier.
//!
//! ## Chemins
//!
//! `File` est le chemin tel que trouvé (dossier choisi + sous-dossiers). Pour comparer, les
//! chemins sont normalisés ([`path_key`]) : `\` → `/`, minuscules, sans `/` final —
//! un partage Windows (`\\SERVEURNAS\BD`) ne distingue pas la casse.

use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use rayon::prelude::*;

pub use crate::index::ScanProgress;
use crate::library::{LibraryBook, PageInfo, PageList};
use crate::providers::{self, index_entries, is_book_path, BookFormat, PageProvider, TarProvider, WebComic};
use crate::{ComicInfo, Error};

/// Nombre de fils de lecture : les fichiers sont sur un NAS, plus de lectures simultanées
/// n'accélèrent rien et gênent les autres utilisateurs du partage.
pub const SCAN_THREADS: usize = 4;

/// Sous-dossier où la conversion en CBZ de ComicRust range les originaux (ignoré).
pub const ORIGINALS_DIR: &str = "_originaux";

/// Taille maximale lue pour un ComicInfo.xml (protection contre une entrée aberrante).
const MAX_INFO_SIZE: u64 = 8 * 1024 * 1024;

/// Bilan d'un [`scan`], à appliquer à la bibliothèque.
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Nouveaux livres (fiche complète, identifiant donné par `new_id`).
    pub added: Vec<LibraryBook>,
    /// Livres connus dont le fichier a changé (relu) ou a été déplacé : fiche complète à
    /// enregistrer à la place de l'ancienne (même `id`).
    pub updated: Vec<LibraryBook>,
    /// Identifiants des livres de `updated` qui ont été retrouvés sous un autre chemin
    /// (même nom de fichier et même taille, ancien fichier absent — comme ComicRack).
    pub moved: Vec<String>,
    /// Identifiants des livres dont le fichier a disparu (`Missing` à mettre à vrai).
    pub missing: Vec<String>,
    /// Identifiants des livres signalés manquants dont le fichier est revenu au même
    /// chemin (`Missing` à remettre à faux ; s'il a aussi changé, il est dans `updated`).
    pub back: Vec<String>,
    /// Fichiers illisibles ou dossiers inaccessibles, avec le message d'erreur.
    pub errors: Vec<(PathBuf, String)>,
    /// Nombre de fichiers de BD trouvés.
    pub files_seen: usize,
    /// Le scan a été interrompu par `cancel`.
    pub cancelled: bool,
}

impl ScanReport {
    /// Rien à appliquer à la bibliothèque.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.missing.is_empty() && self.back.is_empty()
    }

    /// Résumé lisible (barre d'état, diagnostic).
    pub fn summary(&self) -> String {
        format!(
            "{} fichiers : {} ajoutés, {} mis à jour ({} déplacés), {} manquants, {} retrouvés, {} erreurs{}",
            self.files_seen,
            self.added.len(),
            self.updated.len(),
            self.moved.len(),
            self.missing.len(),
            self.back.len(),
            self.errors.len(),
            if self.cancelled { " — interrompu" } else { "" }
        )
    }
}

/// Clé de comparaison d'un chemin : insensible à la casse et au séparateur.
pub fn path_key(path: &str) -> String {
    let mut k = path.replace('\\', "/").to_lowercase();
    while k.len() > 1 && k.ends_with('/') {
        k.pop();
    }
    k
}

/// `key` est-il dans le dossier de clé `dir` (ou est-ce `dir` lui-même) ?
fn is_under(key: &str, dir: &str) -> bool {
    key.strip_prefix(dir).is_some_and(|rest| rest.is_empty() || rest.starts_with('/') || dir.ends_with('/'))
}

/// Date ISO 8601 locale avec fuseau, à la seconde (format des champs de date des fiches).
pub fn iso_time(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// Maintenant, au format de [`iso_time`] (champ `Added`).
pub fn now_iso() -> String {
    iso_time(SystemTime::now())
}

/// Secondes depuis 1970 d'une date ISO enregistrée (avec fuseau, `Z`, ou sans = UTC).
fn parse_iso_secs(text: &str) -> Option<i64> {
    let t = text.trim();
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(t) {
        return Some(d.timestamp());
    }
    chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|d| d.and_utc().timestamp())
}

fn system_secs(t: SystemTime) -> i64 {
    chrono::DateTime::<chrono::Utc>::from(t).timestamp()
}

/// Ce qu'on lit d'un fichier de BD.
struct FileMeta {
    page_count: Option<u32>,
    info: Option<ComicInfo>,
    /// ComicInfo.xml présent mais illisible (le livre reste utilisable).
    info_error: Option<String>,
}

fn info_from_bytes(bytes: Result<Vec<u8>, Error>) -> (Option<ComicInfo>, Option<String>) {
    match bytes.and_then(|b| providers::parse_comic_info(&b)) {
        Ok(ci) => (Some(ci), None),
        Err(e) => (None, Some(format!("ComicInfo.xml illisible : {e}"))),
    }
}

fn read_zip(path: &Path) -> Result<FileMeta, Error> {
    let mut zip = ::zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let (pages, info_entry) = index_entries(zip.file_names().map(str::to_string));
    let (info, info_error) = match info_entry {
        None => (None, None),
        Some(name) => info_from_bytes((|| {
            let mut buf = Vec::new();
            zip.by_name(&name)?.take(MAX_INFO_SIZE).read_to_end(&mut buf)?;
            Ok(buf)
        })()),
    };
    Ok(FileMeta { page_count: Some(pages.len() as u32), info, info_error })
}

fn rar_err(e: unrar_ng::error::UnrarError) -> Error {
    Error::Archive(format!("RAR : {e}"))
}

fn read_rar(path: &Path) -> Result<FileMeta, Error> {
    use unrar_ng::Archive;
    let entry_name = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let mut names = Vec::new();
    for header in Archive::new(path).open_for_listing().map_err(rar_err)? {
        let header = header.map_err(rar_err)?;
        if header.is_file() {
            names.push(entry_name(&header.filename));
        }
    }
    let (pages, info_entry) = index_entries(names);
    let (info, info_error) = match info_entry {
        None => (None, None),
        Some(wanted) => info_from_bytes((|| {
            let mut archive = Archive::new(path).open_for_processing().map_err(rar_err)?;
            while let Some(header) = archive.read_header().map_err(rar_err)? {
                if header.entry().is_file() && entry_name(&header.entry().filename) == wanted {
                    return Ok(header.read().map_err(rar_err)?.0);
                }
                archive = header.skip().map_err(rar_err)?;
            }
            Err(Error::Archive(format!("entrée absente : {wanted}")))
        })()),
    };
    Ok(FileMeta { page_count: Some(pages.len() as u32), info, info_error })
}

fn read_7z(path: &Path) -> Result<FileMeta, Error> {
    use sevenz_rust2::{ArchiveReader, Password};
    let err = |e: sevenz_rust2::Error| Error::Archive(format!("7z : {e}"));
    let mut reader = ArchiveReader::open(path, Password::empty()).map_err(err)?;
    reader.set_thread_count(1);
    // Nom normalisé → nom brut (clé de `read_file`).
    let raw: HashMap<String, String> = reader
        .archive()
        .files
        .iter()
        .filter(|e| e.has_stream && !e.is_directory)
        .map(|e| (e.name.replace('\\', "/"), e.name.clone()))
        .collect();
    let (pages, info_entry) = index_entries(raw.keys().cloned());
    let (info, info_error) = match info_entry {
        None => (None, None),
        Some(name) => info_from_bytes(reader.read_file(&raw[&name]).map_err(err)),
    };
    Ok(FileMeta { page_count: Some(pages.len() as u32), info, info_error })
}

/// Lit le minimum d'un fichier (voir l'en-tête du module).
fn read_meta(path: &Path) -> Result<FileMeta, Error> {
    let format = BookFormat::detect(path).ok_or_else(|| {
        Error::Unsupported(path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "?".into()))
    })?;
    match format {
        BookFormat::Zip => read_zip(path),
        BookFormat::Rar => read_rar(path),
        BookFormat::SevenZip => read_7z(path),
        BookFormat::WebComic => {
            let wc = WebComic::from_xml(&std::fs::read_to_string(path)?)?;
            Ok(FileMeta { page_count: None, info: wc.info, info_error: None })
        }
        BookFormat::Tar => {
            let mut book = TarProvider::open(path)?;
            let (info, info_error) = match book.comic_info() {
                Ok(i) => (i, None),
                Err(e) => (None, Some(format!("ComicInfo.xml illisible : {e}"))),
            };
            Ok(FileMeta { page_count: Some(book.pages().len() as u32), info, info_error })
        }
        BookFormat::Pdf | BookFormat::Djvu | BookFormat::Folder => {
            let mut book = providers::open_book(path)?;
            let info = book.comic_info().ok().flatten();
            Ok(FileMeta { page_count: Some(book.pages().len() as u32), info, info_error: None })
        }
    }
}

/// Champs copiés tels quels du ComicInfo.xml vers la fiche (mêmes noms des deux côtés).
macro_rules! comic_info_fields {
    ($m:ident) => {
        $m!(
            title,
            series,
            number,
            count,
            volume,
            alternate_series,
            alternate_number,
            alternate_count,
            story_arc,
            series_group,
            summary,
            notes,
            review,
            year,
            month,
            day,
            writer,
            penciller,
            inker,
            colorist,
            letterer,
            cover_artist,
            editor,
            translator,
            publisher,
            imprint,
            genre,
            tags,
            web,
            language_iso,
            format,
            age_rating,
            black_and_white,
            manga,
            characters,
            teams,
            main_character_or_team,
            locations,
            scan_information,
            community_rating
        )
    };
}

macro_rules! names {
    ($($f:ident),*) => { &[$(stringify!($f)),*] };
}

/// Noms (Rust) des champs de [`LibraryBook`] qui viennent du ComicInfo.xml.
pub const COMIC_INFO_FIELDS: &[&str] = comic_info_fields!(names);

/// Remplace les champs ComicInfo et les pages de `b` par ceux de `ci` (vidés si `None`).
fn apply_comic_info(b: &mut LibraryBook, ci: Option<&ComicInfo>) {
    macro_rules! copy {
        ($($f:ident),*) => { $( b.$f = ci.and_then(|c| c.$f.clone()); )* };
    }
    comic_info_fields!(copy);
    b.pages = PageList {
        page: ci
            .and_then(|c| c.pages.as_ref())
            .map(|p| {
                p.page
                    .iter()
                    .map(|p| PageInfo {
                        image: p.image as i32,
                        // Comme dans ComicDB.xml, « Story » (valeur par défaut) n'est pas écrit.
                        kind: (p.page_type != crate::comic_info::PageType::Story).then(|| format!("{:?}", p.page_type)),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
}

/// Enlève les infos de pages au-delà du nombre réel de pages (`TrimExcessPageInfo`).
fn trim_pages(b: &mut LibraryBook) {
    if let Some(n) = b.page_count.filter(|n| *n > 0) {
        b.pages.page.retain(|p| p.image >= 0 && (p.image as u32) < n);
    }
}

fn set_file_props(b: &mut LibraryBook, meta: &Metadata) {
    b.file_size = Some(meta.len());
    b.file_modified_time = meta.modified().ok().map(iso_time);
    b.file_creation_time = meta.created().ok().map(iso_time);
}

fn build_book(path: &Path, id: String, meta: &Metadata) -> Result<(LibraryBook, Option<String>), Error> {
    let fm = read_meta(path)?;
    let mut b = LibraryBook { id, file: Some(path.to_string_lossy().into_owned()), ..Default::default() };
    apply_comic_info(&mut b, fm.info.as_ref());
    b.page_count = fm.page_count;
    trim_pages(&mut b);
    set_file_props(&mut b, meta);
    b.added = Some(now_iso());
    Ok((b, fm.info_error))
}

/// Fiche d'un fichier de BD : champs du ComicInfo.xml (et types de pages), nombre réel de
/// pages, chemin tel quel, taille, dates du fichier, date d'ajout (maintenant).
///
/// Erreur si le fichier ne peut pas être ouvert (archive corrompue, format inconnu,
/// PDFium absent…). Un ComicInfo.xml illisible dans une archive saine n'est pas une
/// erreur : la fiche est rendue sans ces informations (comme ComicRack, dont le lecteur
/// XML renvoie `null`) ; [`scan`] le signale dans `errors`.
pub fn book_from_file(path: &Path, id: String) -> Result<LibraryBook, Error> {
    let meta = std::fs::metadata(path)?;
    Ok(build_book(path, id, &meta)?.0)
}

/// Fiche relue `fresh` fusionnée avec la fiche connue `existing` : champs du fichier pris
/// dans `fresh`, champs propres à la bibliothèque gardés de `existing` (voir l'en-tête).
pub fn refresh_book(existing: &LibraryBook, fresh: LibraryBook) -> LibraryBook {
    let dirty = existing.comic_info_is_dirty == Some(true) || existing.comic_book_is_dirty == Some(true);
    let mut out = if dirty {
        // Modifications pas encore écrites dans le fichier : on garde la fiche, on ne
        // rafraîchit que ce qui décrit le fichier.
        let mut b = existing.clone();
        b.file = fresh.file;
        b.file_size = fresh.file_size;
        b.file_modified_time = fresh.file_modified_time;
        b.file_creation_time = fresh.file_creation_time;
        b.page_count = fresh.page_count;
        trim_pages(&mut b);
        b
    } else {
        let mut b = fresh;
        let e = existing;
        macro_rules! keep {
            ($($f:ident),*) => { $( b.$f = e.$f.clone(); )* };
        }
        keep!(
            id,
            added,
            opened,
            open_count,
            current_page,
            last_page_read,
            rating,
            checked,
            released,
            series_complete,
            preferred_front_cover,
            enable_proposed,
            comic_info_is_dirty,
            comic_book_is_dirty,
            isbn,
            book_age,
            book_collection_status,
            book_condition,
            book_location,
            book_notes,
            book_owner,
            book_store,
            book_price
        );
        b
    };
    out.missing = existing.missing.map(|_| false);
    out.proposed_cache = Default::default();
    // `ComicBook.SetInfo` : page courante et dernière page lue restent dans le livre.
    if let Some(n) = out.page_count.filter(|n| *n > 0) {
        out.current_page = out.current_page.map(|p| p.min(n - 1));
        out.last_page_read = out.last_page_read.map(|p| p.min(n - 1));
    }
    out
}

/// Fichier de BD trouvé pendant le parcours.
struct Found {
    path: PathBuf,
    key: String,
    meta: Metadata,
}

/// Nom de fichier à ignorer : temporaires (`*.tmp`, dont `*.comicrust.tmp` et les
/// `*.cbz.tmp` de ComicRack) et ressources macOS `._xxx` (option « MacCompatibleScanning »
/// de ComicRack, toujours active ici : ce ne sont jamais des BD).
fn is_ignored_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".tmp") || lower.starts_with("._")
}

/// Résultat du parcours d'un ensemble de dossiers.
#[derive(Default)]
struct Walk {
    found: Vec<Found>,
    /// Clés des dossiers illisibles (racines comprises) : leurs livres ne sont pas déclarés
    /// manquants (NAS débranché, droits…), comme le `DriveChecker` de ComicRack.
    unreadable: Vec<String>,
    /// Clés des fichiers en cours de réécriture par ComicRack (`x.cbz` + `x.cbz.tmp`).
    busy: HashSet<String>,
    errors: Vec<(PathBuf, String)>,
}

fn walk(roots: &[PathBuf], progress: &ScanProgress, cancel: &AtomicBool) -> Option<Walk> {
    let mut w = Walk::default();
    let mut seen = HashSet::new();
    for root in roots {
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let rd = match std::fs::read_dir(&dir) {
                Ok(rd) => rd,
                Err(e) => {
                    w.unreadable.push(path_key(&dir.to_string_lossy()));
                    w.errors.push((dir, format!("dossier inaccessible : {e}")));
                    continue;
                }
            };
            for entry in rd {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        // Liste incomplète : prudence pour les manquants de ce dossier.
                        w.unreadable.push(path_key(&dir.to_string_lossy()));
                        w.errors.push((dir.clone(), format!("lecture du dossier : {e}")));
                        break;
                    }
                };
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_dir() {
                    if !name.eq_ignore_ascii_case(ORIGINALS_DIR) {
                        stack.push(path);
                    }
                    continue;
                }
                if is_ignored_name(&name) || !is_book_path(&path) {
                    continue;
                }
                // Lien symbolique : on suit vers le fichier (pas vers les dossiers, pour
                // éviter les boucles). Sous Windows, `entry.metadata()` ne coûte rien.
                let meta = if ft.is_symlink() { std::fs::metadata(&path) } else { entry.metadata() };
                let meta = match meta {
                    Ok(m) if m.is_file() => m,
                    Ok(_) => continue,
                    Err(e) => {
                        w.errors.push((path, format!("fichier inaccessible : {e}")));
                        continue;
                    }
                };
                let key = path_key(&path.to_string_lossy());
                if providers::is_being_rewritten(&path) {
                    w.busy.insert(key);
                    continue;
                }
                // Dossiers choisis imbriqués ou en double : un seul exemplaire.
                if seen.insert(key.clone()) {
                    progress.total.fetch_add(1, Ordering::Relaxed);
                    w.found.push(Found { path, key, meta });
                }
            }
        }
    }
    w.found.sort_by(|a, b| a.key.cmp(&b.key));
    Some(w)
}

/// La date de modification connue diffère-t-elle de celle du fichier ? Une date connue
/// absente ou illisible compte comme différente ; une date de fichier indisponible
/// (système qui ne la donne pas) laisse la taille seule décider.
fn modified_differs(known: Option<&str>, meta: &Metadata) -> bool {
    let Some(actual) = meta.modified().ok().map(system_secs) else { return false };
    known.and_then(parse_iso_secs) != Some(actual)
}

fn file_name_key(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_lowercase()
}

enum Job<'a> {
    New(&'a Found),
    Refresh(&'a LibraryBook, &'a Found),
}

enum Outcome {
    Added(LibraryBook),
    Updated(LibraryBook),
}

/// Parcourt `folders` (récursivement) et compare les fichiers trouvés aux livres
/// `known` de la bibliothèque. Ne modifie rien : voir [`ScanReport`].
///
/// - Fichiers retenus : extensions de [`providers::BOOK_EXTENSIONS`] ; ignorés : `*.tmp`,
///   `._*`, le sous-dossier [`ORIGINALS_DIR`], les dossiers d'images, les fichiers que
///   ComicRack est en train de réécrire.
/// - Nouveau fichier → `added` ; sauf s'il porte le même nom et la même taille qu'un livre
///   connu dont le fichier n'existe plus : la fiche est alors déplacée (`updated` +
///   `moved`), sans relire le fichier (`ComicScanner.OnProcessScannedFile`).
/// - Fichier connu dont la taille ou la date de modification a changé → relu, `updated`
///   (fusion par [`refresh_book`]).
/// - Livre connu situé dans un des dossiers scannés dont le fichier n'est plus là →
///   `missing` (s'il ne l'était pas déjà). Pas pour un dossier inaccessible.
/// - Livre manquant dont le fichier est revenu → `back`.
/// - Fichier illisible → `errors`, et il **n'est pas ajouté** : ComicRack non plus (son
///   scanner avale l'exception levée par l'ouverture de l'archive et passe au suivant) ;
///   il sera retenté au scan suivant. Un livre connu illisible garde sa fiche. Un
///   ComicInfo.xml illisible dans une archive saine est signalé, mais le livre est ajouté.
///
/// Lecture sur [`SCAN_THREADS`] fils. `progress.total` = fichiers trouvés (augmente
/// pendant le parcours), `progress.done` = fichiers traités. `cancel` arrête au plus
/// tôt : interrompu pendant le parcours, le rapport est vide ; pendant la lecture, il
/// contient ce qui a été lu, et les manquants (le parcours étant complet).
/// `new_id` donne l'identifiant de chaque nouveau livre (appelé depuis les fils de lecture).
pub fn scan(
    folders: &[PathBuf],
    known: &[LibraryBook],
    progress: &ScanProgress,
    cancel: &AtomicBool,
    new_id: impl Fn() -> String + Sync,
) -> ScanReport {
    progress.total.store(0, Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    let mut report = ScanReport::default();

    let Some(walk) = walk(folders, progress, cancel) else {
        report.cancelled = true;
        return report;
    };
    report.files_seen = walk.found.len();
    report.errors = walk.errors;

    let roots: Vec<String> = folders.iter().map(|f| path_key(&f.to_string_lossy())).collect();
    let in_scope =
        |key: &str| roots.iter().any(|r| is_under(key, r)) && !walk.unreadable.iter().any(|d| is_under(key, d));

    // Livres connus par chemin (le premier en cas de doublon).
    let mut by_key: HashMap<String, &LibraryBook> = HashMap::new();
    for b in known {
        if let Some(f) = b.file.as_deref() {
            by_key.entry(path_key(f)).or_insert(b);
        }
    }

    // 1. Fichiers déjà connus.
    let mut jobs = Vec::new();
    let mut new_files = Vec::new();
    let mut seen_ids: HashSet<&str> = HashSet::new();
    for f in &walk.found {
        match by_key.get(f.key.as_str()) {
            Some(b) => {
                seen_ids.insert(b.id.as_str());
                if b.missing == Some(true) {
                    report.back.push(b.id.clone());
                }
                if b.file_size != Some(f.meta.len()) || modified_differs(b.file_modified_time.as_deref(), &f.meta) {
                    jobs.push(Job::Refresh(b, f));
                } else {
                    progress.done.fetch_add(1, Ordering::Relaxed);
                }
            }
            None => new_files.push(f),
        }
    }

    // 2. Déplacements : livre connu non retrouvé, même nom de fichier et même taille.
    let mut gone: HashMap<(String, u64), Vec<&LibraryBook>> = HashMap::new();
    for b in known {
        if let (Some(file), Some(size)) = (b.file.as_deref(), b.file_size) {
            let key = path_key(file);
            if !seen_ids.contains(b.id.as_str()) && !walk.busy.contains(&key) {
                gone.entry((file_name_key(file), size)).or_default().push(b);
            }
        }
    }
    let mut claimed: HashSet<&str> = HashSet::new();
    for f in new_files {
        let name = file_name_key(&f.path.to_string_lossy());
        let old = gone.get(&(name, f.meta.len())).and_then(|cands| {
            cands.iter().copied().find(|b| {
                let file = b.file.as_deref().unwrap_or_default();
                // Ancien fichier absent : certain s'il est dans un dossier scanné lisible,
                // sinon on vérifie (seulement pour ces rares candidats).
                !claimed.contains(b.id.as_str()) && (in_scope(&path_key(file)) || !Path::new(file).exists())
            })
        });
        match old {
            Some(b) => {
                claimed.insert(b.id.as_str());
                let mut m = b.clone();
                m.file = Some(f.path.to_string_lossy().into_owned());
                set_file_props(&mut m, &f.meta);
                m.missing = b.missing.map(|_| false);
                m.proposed_cache = Default::default();
                report.moved.push(m.id.clone());
                report.updated.push(m);
                progress.done.fetch_add(1, Ordering::Relaxed);
            }
            None => jobs.push(Job::New(f)),
        }
    }

    // 3. Manquants : connus, dans un dossier scanné lisible, pas retrouvés ni déplacés.
    for b in known {
        let Some(file) = b.file.as_deref() else { continue };
        let key = path_key(file);
        if b.missing != Some(true)
            && !seen_ids.contains(b.id.as_str())
            && !claimed.contains(b.id.as_str())
            && !walk.busy.contains(&key)
            && in_scope(&key)
            // Original renommé en `.tmp` par ComicRack le temps de l'enregistrer.
            && !providers::is_being_rewritten(Path::new(file))
        {
            report.missing.push(b.id.clone());
        }
    }

    // 4. Lecture des fichiers nouveaux ou modifiés, en parallèle limité.
    for r in run_jobs(&jobs, progress, cancel, &new_id) {
        match r {
            None => report.cancelled = true,
            Some(Err(e)) => report.errors.push(e),
            Some(Ok((outcome, warning))) => {
                report.errors.extend(warning);
                match outcome {
                    Outcome::Added(b) => report.added.push(b),
                    Outcome::Updated(b) => report.updated.push(b),
                }
            }
        }
    }
    report.cancelled |= cancel.load(Ordering::Relaxed);
    report
}

/// Résultat d'une lecture : `None` = non faite (annulation), sinon la fiche et un
/// éventuel avertissement (ComicInfo.xml illisible), ou l'erreur.
type JobResult = Option<Result<(Outcome, Option<(PathBuf, String)>), (PathBuf, String)>>;

fn run_jobs(
    jobs: &[Job],
    progress: &ScanProgress,
    cancel: &AtomicBool,
    new_id: &(impl Fn() -> String + Sync),
) -> Vec<JobResult> {
    let work = || -> Vec<JobResult> {
        jobs.par_iter()
            .map(|job| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let (f, id) = match job {
                    Job::New(f) => (*f, new_id()),
                    Job::Refresh(b, f) => (*f, b.id.clone()),
                };
                let res = build_book(&f.path, id, &f.meta);
                progress.done.fetch_add(1, Ordering::Relaxed);
                Some(match res {
                    Err(e) => Err((f.path.clone(), e.to_string())),
                    Ok((book, warning)) => {
                        let outcome = match job {
                            Job::New(_) => Outcome::Added(book),
                            Job::Refresh(old, _) => Outcome::Updated(refresh_book(old, book)),
                        };
                        Ok((outcome, warning.map(|w| (f.path.clone(), w))))
                    }
                })
            })
            .collect()
    };
    match rayon::ThreadPoolBuilder::new()
        .num_threads(SCAN_THREADS)
        .thread_name(|i| format!("comicrust-scan-{i}"))
        .build()
    {
        Ok(pool) => pool.install(work),
        // Création du pool impossible (ressources) : pool global de rayon.
        Err(_) => work(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::providers::testutil::{page_number, png, INFO, NAMES};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    /// Même CBZ que `providers::zip::tests::write_cbz` (3 pages, série « Thanos » n° 1).
    fn write_cbz(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut w = ::zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opt = ::zip::write::SimpleFileOptions::default();
        for n in NAMES {
            w.start_file(n, opt).unwrap();
            w.write_all(&png(page_number(n))).unwrap();
        }
        w.start_file("ComicInfo.xml", opt).unwrap();
        w.write_all(INFO.as_bytes()).unwrap();
        w.finish().unwrap();
    }

    /// CBZ de `pages` pages, avec un ComicInfo.xml facultatif.
    fn cbz(path: &Path, pages: usize, info: Option<&str>) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        let mut w = ::zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opt = ::zip::write::SimpleFileOptions::default();
        for i in 0..pages {
            w.start_file(format!("{i:03}.jpg"), opt).unwrap();
            w.write_all(b"x").unwrap();
        }
        if let Some(xml) = info {
            w.start_file("ComicInfo.xml", opt).unwrap();
            w.write_all(xml.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    const RICH: &str = r#"<ComicInfo><Series>Infinity Gauntlet</Series><Number>2</Number><Volume>1991</Volume>
<Title>Le Gant</Title><Writer>Jim Starlin</Writer><Year>1991</Year><Month>8</Month><Manga>No</Manga>
<Characters>Thanos, Adam Warlock</Characters><CommunityRating>4.5</CommunityRating><PageCount>99</PageCount>
<Pages><Page Image="0" Type="FrontCover"/><Page Image="1"/><Page Image="2" Type="Advertisement"/><Page Image="7" Type="BackCover"/></Pages>
</ComicInfo>"#;

    fn ids() -> impl Fn() -> String + Sync {
        let n = AtomicUsize::new(0);
        move || format!("id-{}", n.fetch_add(1, Ordering::Relaxed))
    }

    fn run(folders: &[PathBuf], known: &[LibraryBook]) -> ScanReport {
        scan(folders, known, &ScanProgress::default(), &AtomicBool::new(false), ids())
    }

    fn by_name<'a>(books: &'a [LibraryBook], name: &str) -> &'a LibraryBook {
        books.iter().find(|b| b.file.as_deref().unwrap().ends_with(name)).unwrap_or_else(|| panic!("{name}"))
    }

    /// Recule la date de modification d'un fichier (pour simuler un changement).
    fn touch_back(path: &Path, secs: u64) {
        let f = std::fs::File::options().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - std::time::Duration::from_secs(secs)).unwrap();
    }

    #[test]
    fn book_from_cbz_fills_comic_info_and_file_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("IG 02.cbz");
        cbz(&path, 3, Some(RICH));
        let b = book_from_file(&path, "X".into()).unwrap();
        assert_eq!(b.id, "X");
        assert_eq!(b.file.as_deref(), Some(path.to_string_lossy().as_ref()));
        assert_eq!(b.series.as_deref(), Some("Infinity Gauntlet"));
        assert_eq!((b.number.as_deref(), b.volume, b.year, b.month), (Some("2"), Some(1991), Some(1991), Some(8)));
        assert_eq!(b.writer.as_deref(), Some("Jim Starlin"));
        assert_eq!(b.manga.as_deref(), Some("No"));
        assert_eq!(b.community_rating, Some(4.5));
        // Nombre réel de pages, pas le PageCount du ComicInfo.xml ; page 7 inexistante enlevée.
        assert_eq!(b.page_count, Some(3));
        let kinds: Vec<(i32, Option<&str>)> = b.pages.page.iter().map(|p| (p.image, p.kind.as_deref())).collect();
        assert_eq!(kinds, [(0, Some("FrontCover")), (1, None), (2, Some("Advertisement"))]);
        assert_eq!(b.file_size, Some(std::fs::metadata(&path).unwrap().len()));
        for d in [&b.added, &b.file_modified_time, &b.file_creation_time] {
            let d = d.as_deref().unwrap();
            assert!(chrono::DateTime::parse_from_rfc3339(d).is_ok(), "{d}");
        }
        let added = parse_iso_secs(b.added.as_deref().unwrap()).unwrap();
        assert!((chrono::Utc::now().timestamp() - added).abs() < 60);
        assert!(b.current_page.is_none() && b.opened.is_none() && b.missing.is_none());
    }

    #[test]
    fn dates_are_compared_whatever_their_spelling() {
        assert_eq!(parse_iso_secs("2019-10-01T12:58:00.1234567+02:00"), Some(1569927480));
        assert_eq!(parse_iso_secs("2019-10-01T10:58:00Z"), Some(1569927480));
        assert_eq!(parse_iso_secs("2019-10-01T10:58:00"), Some(1569927480));
        assert_eq!(parse_iso_secs("n'importe quoi"), None);
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1569927480);
        assert_eq!(parse_iso_secs(&iso_time(t)), Some(1569927480));
    }

    #[test]
    fn path_keys_ignore_case_and_separators() {
        assert_eq!(path_key(r"\\SERVEURNAS\BD\Thanos\IG 01.CBZ"), "//serveurnas/bd/thanos/ig 01.cbz");
        assert_eq!(path_key("//serveurnas/bd/"), "//serveurnas/bd");
        assert!(is_under("//serveurnas/bd/x.cbz", &path_key(r"\\SERVEURNAS\BD\")));
        assert!(!is_under("//serveurnas/bd2/x.cbz", "//serveurnas/bd"));
        assert!(is_under("c:/bd/x.cbz", &path_key(r"C:\")));
    }

    /// Dossier de test : CBZ avec et sans ComicInfo, CBR (RAR4, RAR5 solide), PDF, et des
    /// fichiers à ignorer.
    fn library_folder(root: &Path) {
        write_cbz(&root.join("Thanos 01.cbz"));
        cbz(&root.join("Sous dossier/Warlock 09.cbz"), 2, None);
        cbz(&root.join("Sous dossier/IG 02.cbz"), 3, Some(RICH));
        std::fs::copy(fixture("rar4.cbr"), root.join("Sous dossier/rar4.cbr")).unwrap();
        std::fs::copy(fixture("solid_rar5.cbr"), root.join("solid.CBR")).unwrap();
        std::fs::copy(fixture("sample.pdf"), root.join("sample.pdf")).unwrap();
        // À ignorer : temporaires, originaux de conversion, ressources macOS, autres fichiers.
        cbz(&root.join("Thanos 02.cbz.tmp"), 1, None);
        cbz(&root.join("Thanos 03.cbz.comicrust.tmp"), 1, None);
        cbz(&root.join("_originaux/Thanos 01.cbr"), 1, None);
        cbz(&root.join("Sous dossier/_Originaux/Vieux.cbz"), 1, None);
        cbz(&root.join("._Thanos 01.cbz"), 1, None);
        std::fs::write(root.join("notes.txt"), "x").unwrap();
        // Dossier d'images : pas ajouté en v0.7.0.
        std::fs::create_dir_all(root.join("Images")).unwrap();
        std::fs::write(root.join("Images/001.png"), png(1)).unwrap();
    }

    #[test]
    fn incremental_scan() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("BD");
        library_folder(&root);
        let folders = [root.clone()];

        // 1. Premier scan : tout est ajouté.
        let progress = ScanProgress::default();
        let r = scan(&folders, &[], &progress, &AtomicBool::new(false), ids());
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.files_seen, 6);
        assert_eq!(progress.get(), (6, 6));
        assert_eq!(r.added.len(), 6, "{:?}", r.added.iter().map(|b| &b.file).collect::<Vec<_>>());
        assert!(!r.cancelled && r.updated.is_empty() && r.missing.is_empty() && r.back.is_empty());
        let mut known = r.added;
        let ids: HashSet<&str> = known.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids.len(), 6);
        let thanos = by_name(&known, "Thanos 01.cbz");
        assert_eq!((thanos.series.as_deref(), thanos.page_count), (Some("Thanos"), Some(3)));
        let warlock = by_name(&known, "Warlock 09.cbz");
        assert_eq!((warlock.series.as_deref(), warlock.page_count), (None, Some(2)));
        for cbr in ["rar4.cbr", "solid.CBR"] {
            let b = by_name(&known, cbr);
            assert_eq!((b.series.as_deref(), b.number.as_deref(), b.page_count), (Some("Thanos"), Some("1"), Some(3)));
        }
        let pdf = by_name(&known, "sample.pdf");
        assert!(pdf.page_count.unwrap() > 0 && pdf.series.is_none() && pdf.file_size.is_some());
        assert_eq!(by_name(&known, "IG 02.cbz").pages.page.len(), 3);

        // 2. Second scan sans changement : rien.
        let r = run(&folders, &known);
        assert!(r.is_empty() && r.errors.is_empty(), "{}", r.summary());
        assert_eq!(r.files_seen, 6);

        // 3. Fichier modifié : relu, champs de lecture gardés.
        let ig = known.iter_mut().find(|b| b.file.as_deref().unwrap().ends_with("IG 02.cbz")).unwrap();
        ig.current_page = Some(2);
        ig.last_page_read = Some(2);
        ig.open_count = Some(4);
        ig.opened = Some("2026-10-01T20:00:00+02:00".into());
        ig.rating = Some(4.0);
        ig.book_owner = Some("Yoyo".into());
        ig.checked = Some(false);
        let ig_id = ig.id.clone();
        let ig_added = ig.added.clone();
        let ig_path = PathBuf::from(ig.file.clone().unwrap());
        cbz(&ig_path, 2, Some("<ComicInfo><Series>Infinity Gauntlet</Series><Number>3</Number></ComicInfo>"));
        let r = run(&folders, &known);
        assert_eq!(r.updated.len(), 1, "{}", r.summary());
        assert!(r.added.is_empty() && r.missing.is_empty() && r.moved.is_empty());
        let u = &r.updated[0];
        assert_eq!((u.id.as_str(), &u.added), (ig_id.as_str(), &ig_added));
        assert_eq!((u.number.as_deref(), u.writer.as_deref(), u.year), (Some("3"), None, None));
        assert_eq!(u.page_count, Some(2));
        assert!(u.pages.page.is_empty());
        // Page courante ramenée dans le livre (2 pages : 0 et 1).
        assert_eq!((u.current_page, u.last_page_read), (Some(1), Some(1)));
        assert_eq!((u.open_count, u.rating, u.checked), (Some(4), Some(4.0), Some(false)));
        assert_eq!((u.opened.as_deref(), u.book_owner.as_deref()), (Some("2026-10-01T20:00:00+02:00"), Some("Yoyo")));
        assert_eq!(u.file_size, Some(std::fs::metadata(&ig_path).unwrap().len()));
        let i = known.iter().position(|b| b.id == ig_id).unwrap();
        known[i] = r.updated.into_iter().next().unwrap();
        assert!(run(&folders, &known).is_empty());

        // Même taille, date seule changée : relu aussi.
        touch_back(&ig_path, 3600);
        assert_eq!(run(&folders, &known).updated.len(), 1);
        known[i].file_modified_time = Some(iso_time(std::fs::metadata(&ig_path).unwrap().modified().unwrap()));

        // Fiche modifiée dans la bibliothèque, pas encore écrite : infos gardées.
        known[i].comic_info_is_dirty = Some(true);
        known[i].series = Some("Corrigée".into());
        cbz(&ig_path, 3, Some(RICH));
        let r = run(&folders, &known);
        assert_eq!(r.updated[0].series.as_deref(), Some("Corrigée"));
        assert_eq!(r.updated[0].page_count, Some(3));
        known[i] = r.updated.into_iter().next().unwrap();

        // 4. Fichier supprimé : manquant (une seule fois).
        let warlock_path = PathBuf::from(by_name(&known, "Warlock 09.cbz").file.clone().unwrap());
        let warlock_id = by_name(&known, "Warlock 09.cbz").id.clone();
        let saved = std::fs::read(&warlock_path).unwrap();
        std::fs::remove_file(&warlock_path).unwrap();
        let r = run(&folders, &known);
        assert_eq!(r.missing, std::slice::from_ref(&warlock_id));
        assert!(r.added.is_empty() && r.updated.is_empty() && r.back.is_empty());
        let w = known.iter_mut().find(|b| b.id == warlock_id).unwrap();
        w.missing = Some(true);
        assert!(run(&folders, &known).is_empty());

        // 5. Remis : revenu.
        std::fs::write(&warlock_path, &saved).unwrap();
        let mtime = std::fs::metadata(&warlock_path).unwrap().modified().unwrap();
        known.iter_mut().find(|b| b.id == warlock_id).unwrap().file_modified_time = Some(iso_time(mtime));
        let r = run(&folders, &known);
        assert_eq!(r.back, std::slice::from_ref(&warlock_id));
        assert!(r.updated.is_empty() && r.missing.is_empty() && r.added.is_empty());
        known.iter_mut().find(|b| b.id == warlock_id).unwrap().missing = Some(false);

        // 6. Déplacé dans un autre sous-dossier : même fiche, nouveau chemin.
        let moved_to = root.join("Rangement/Warlock 09.cbz");
        std::fs::create_dir_all(moved_to.parent().unwrap()).unwrap();
        std::fs::rename(&warlock_path, &moved_to).unwrap();
        let r = run(&folders, &known);
        assert_eq!(r.moved, std::slice::from_ref(&warlock_id));
        assert_eq!(r.updated.len(), 1);
        assert_eq!(r.updated[0].file.as_deref(), Some(moved_to.to_string_lossy().as_ref()));
        assert!(r.added.is_empty() && r.missing.is_empty(), "{}", r.summary());

        // Chemins connus écrits autrement (casse, séparateurs) : rien de nouveau.
        let respelled: Vec<LibraryBook> = known
            .iter()
            .map(|b| {
                let mut b = b.clone();
                b.file = b.file.map(|f| f.to_uppercase().replace('/', "\\"));
                b
            })
            .collect();
        let r = run(&folders, &respelled);
        assert!(r.added.is_empty() && r.missing.is_empty(), "{}", r.summary());
    }

    #[test]
    fn books_outside_the_scanned_folders_and_unreachable_folders_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("A");
        cbz(&a.join("Un.cbz"), 1, None);
        let r = run(std::slice::from_ref(&a), &[]);
        let mut known = r.added;
        known.push(LibraryBook { id: "ailleurs".into(), file: Some(r"\\AUTRE\BD\x.cbz".into()), ..Default::default() });
        // Dossier B absent (NAS débranché) : ses livres ne sont pas manquants.
        let b = dir.path().join("B");
        known.push(LibraryBook {
            id: "nas".into(),
            file: Some(b.join("y.cbz").to_string_lossy().into_owned()),
            ..Default::default()
        });
        let r = run(&[a.clone(), b.clone()], &known);
        assert!(r.missing.is_empty(), "{:?}", r.missing);
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.errors[0].0, b);
        assert!(r.errors[0].1.starts_with("dossier inaccessible"));
    }

    #[test]
    fn file_being_rewritten_by_comicrack_is_neither_added_nor_missing() {
        let dir = tempfile::tempdir().unwrap();
        cbz(&dir.path().join("Un.cbz"), 1, None);
        let known = run(&[dir.path().to_path_buf()], &[]).added;
        // ComicRack renomme l'original en .tmp le temps d'écrire le nouveau.
        std::fs::rename(dir.path().join("Un.cbz"), dir.path().join("Un.cbz.tmp")).unwrap();
        let r = run(&[dir.path().to_path_buf()], &known);
        assert!(r.is_empty(), "{}", r.summary());
        cbz(&dir.path().join("Deux.cbz"), 1, None);
        cbz(&dir.path().join("Deux.cbz.tmp"), 1, None);
        let r = run(&[dir.path().to_path_buf()], &known);
        assert!(r.is_empty(), "{}", r.summary());
    }

    #[test]
    fn unreadable_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        cbz(&root.join("Bon.cbz"), 2, None);
        // Signature ZIP mais archive tronquée.
        std::fs::write(root.join("Corrompu.cbz"), b"PK\x03\x04 ceci n'est pas une archive").unwrap();
        // Ni signature ni contenu.
        std::fs::write(root.join("Vide.cbr"), b"").unwrap();
        // Archive saine, ComicInfo.xml invalide : ajouté quand même, avec avertissement.
        cbz(&root.join("Info cassée.cbz"), 2, Some("<ComicInfo><Series>Oups</Series><Year>abc</Year></ComicInfo>"));
        let r = run(&[root.to_path_buf()], &[]);
        let mut added: Vec<String> = r
            .added
            .iter()
            .map(|b| Path::new(b.file.as_deref().unwrap()).file_name().unwrap().to_string_lossy().into())
            .collect();
        added.sort();
        assert_eq!(added, ["Bon.cbz", "Info cassée.cbz"]);
        let mut errs: Vec<String> =
            r.errors.iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        errs.sort();
        assert_eq!(errs, ["Corrompu.cbz", "Info cassée.cbz", "Vide.cbr"], "{:?}", r.errors);
        assert!(r.errors.iter().any(|(_, m)| m.starts_with("ComicInfo.xml illisible")));
        assert!(by_name(&r.added, "Info cassée.cbz").series.is_none());
        assert!(book_from_file(&root.join("Corrompu.cbz"), "x".into()).is_err());
        // Un livre connu devenu illisible garde sa fiche (pas de mise à jour).
        let known = r.added;
        std::fs::write(root.join("Bon.cbz"), b"PK\x03\x04 abime").unwrap();
        let r = run(&[root.to_path_buf()], &known);
        assert!(r.updated.is_empty() && r.missing.is_empty());
        assert!(r.errors.iter().any(|(p, _)| p.ends_with("Bon.cbz")));
    }

    #[test]
    fn seven_zip_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        for i in 0..4 {
            std::fs::write(src.join(format!("{i:02}.png")), png(i)).unwrap();
        }
        std::fs::write(src.join("ComicInfo.xml"), INFO).unwrap();
        let out = dir.path().join("BD");
        std::fs::create_dir_all(&out).unwrap();
        sevenz_rust2::compress_to_path(&src, out.join("t.cb7")).unwrap();
        let r = run(&[out], &[]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!((r.added[0].series.as_deref(), r.added[0].page_count), (Some("Thanos"), Some(4)));
    }

    #[test]
    fn cancel_keeps_a_consistent_partial_report() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..40 {
            cbz(&dir.path().join(format!("Livre {i:02}.cbz")), 1, None);
        }
        let folders = [dir.path().to_path_buf()];
        // Annulé avant de commencer : rapport vide.
        let cancel = AtomicBool::new(true);
        let r = scan(&folders, &[], &ScanProgress::default(), &cancel, ids());
        assert!(r.cancelled && r.is_empty() && r.files_seen == 0);
        // Annulé pendant la lecture (au 5e nouvel identifiant).
        let cancel = AtomicBool::new(false);
        let n = AtomicUsize::new(0);
        let progress = ScanProgress::default();
        let r = scan(&folders, &[], &progress, &cancel, || {
            if n.fetch_add(1, Ordering::Relaxed) == 4 {
                cancel.store(true, Ordering::Relaxed);
            }
            format!("id-{}", n.load(Ordering::Relaxed))
        });
        assert!(r.cancelled);
        assert_eq!(r.files_seen, 40);
        assert!(r.added.len() >= 5 && r.added.len() < 40, "{}", r.added.len());
        assert_eq!(progress.get().0, r.added.len());
        // Ce qui a été lu est complet ; le scan suivant ajoute le reste, sans doublon.
        let known = r.added;
        let r = run(&folders, &known);
        assert_eq!(known.len() + r.added.len(), 40);
        assert!(r.missing.is_empty() && r.updated.is_empty());
    }

    /// Mesure : `cargo test -p comicrust-core --release scanner::tests::five_thousand -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn five_thousand_files() {
        let dir = tempfile::tempdir().unwrap();
        let info = |i: usize| format!("<ComicInfo><Series>Série {}</Series><Number>{i}</Number></ComicInfo>", i / 50);
        let t = std::time::Instant::now();
        for i in 0..5000 {
            cbz(&dir.path().join(format!("{:02}/Livre {i:04}.cbz", i / 100)), 20, Some(&info(i)));
        }
        eprintln!("création : {:?}", t.elapsed());
        let folders = [dir.path().to_path_buf()];
        let t = std::time::Instant::now();
        let r = run(&folders, &[]);
        eprintln!("premier scan : {:?} — {}", t.elapsed(), r.summary());
        assert_eq!(r.added.len(), 5000);
        let t = std::time::Instant::now();
        let again = run(&folders, &r.added);
        eprintln!("second scan (rien de changé) : {:?} — {}", t.elapsed(), again.summary());
        assert!(again.is_empty());
    }
}
