//! Transfert de la bibliothèque **ComicRack** vers la base de ComicRust (v0.8.0).
//!
//! But : pouvoir se passer de ComicRack. La base de ComicRack (MySQL ou `ComicDb.xml`)
//! n'est **jamais modifiée** : elle est lue par l'appelant ([`crate::db::ComicDatabase::load`]
//! ou [`crate::library::Library::from_xml`]) et passée ici sous forme de
//! [`ComicRackSource`].
//!
//! Ce qui est transféré par [`Store::import_comicrack`] :
//!
//! 1. **Livres** — chaque fiche est enregistrée avec **son identifiant** ComicRack (les
//!    listes de ComicRack et les listes `.cbl` s'y réfèrent) et **tous** ses champs, par
//!    lots de [`IMPORT_BATCH`] livres, une transaction par lot (une annulation ou une
//!    erreur ne laisse que des lots complets) :
//!    - livre absent de la base : ajouté (`books_added`) ;
//!    - livre déjà présent **avec le même identifiant** (transfert relancé) : fiche
//!      remplacée par celle de ComicRack, sauf la **lecture**, fusionnée (voir
//!      ci-dessous) ; compté dans `books_updated` ;
//!    - livre présent sous un **autre identifiant** mais pour le **même fichier** (clé
//!      [`scanner::path_key`] : sans casse, `\` = `/`), typiquement ajouté par une analyse
//!      des dossiers de ComicRust : supprimé et remplacé par la fiche ComicRack (avec
//!      l'identifiant de ComicRack), sa lecture étant fusionnée de la même façon ; compté
//!      dans `books_replaced`. Un livre de la base dont l'identifiant existe aussi dans
//!      ComicRack n'est jamais supprimé de cette façon (doublons de fichier dans ComicRack) ;
//!    - les livres de la base absents de ComicRack ne sont pas touchés ;
//!    - livre de même identifiant dont le **fichier a changé dans ComicRust** depuis le
//!      transfert précédent (conversion CBR → CBZ, déplacement) : si le fichier de la base
//!      existe et que celui de ComicRack n'existe plus, les colonnes « fichier » de la base
//!      sont gardées (`file`, `file_size`, `file_modified_time`, `file_creation_time`,
//!      `page_count`, `pages`) ; compté dans `files_kept` (avertissement récapitulatif).
//!
//!    **Chemins d'un même partage écrits autrement** (`Z:\BD\…` et `\\SERVEURNAS\BD\…`,
//!    ou lien symbolique) : avant tout, la liste finale des dossiers surveillés est établie
//!    (ceux de la base, plus ceux de ComicRack avec [`ImportOptions::folders`]). Une fiche
//!    dont le fichier n'est sous aucun de ces dossiers (comparaison de [`scanner::path_key`])
//!    voit ses dossiers parents comparés **physiquement** aux dossiers surveillés (crate
//!    `same-file` : volume + index de fichier sous Windows, périphérique + inode ailleurs ;
//!    une fois par dossier, résultat mis en cache ; dossier inexistant = différent). Si un
//!    parent `A` est le dossier surveillé `W`, le chemin devient `W` + la suite relative à
//!    `A` (séparateur `\` gardé pour un chemin Windows) : l'analyse de ComicRust
//!    reconnaît alors le fichier, et le remplacement « même fichier » fonctionne. Seules
//!    les copies en mémoire sont modifiées (jamais ComicRack) ; compté dans
//!    `paths_rewritten`, un avertissement par correspondance `A → W`. Les fiches restées
//!    hors de tout dossier surveillé font l'objet d'un avertissement (elles ne seront pas
//!    suivies par l'analyse).
//!
//!    Chaque lot incrémente `update_counter` (et `delete_counter` s'il a supprimé des
//!    livres remplacés), comme toute écriture : un autre ComicRust connecté à la même base
//!    voit le transfert par son [`StoreWatcher`](super::StoreWatcher).
//!
//! 2. **Listes** — l'élément `<ComicLists>` de `ComicDb.xml` est enregistré **tel quel**
//!    dans la table `comicrust_lists`, **une ligne par liste** (à tous les niveaux) :
//!    `id`, `parent` (NULL au premier niveau), `position` (rang parmi les frères), `name`,
//!    `kind` (`xsi:type` de ComicRack : `ComicIdListItem`, `ComicSmartListItem`,
//!    `ComicListItemFolder`, `ComicLibraryListItem`…) et `xml` : l'élément `<Item>`
//!    d'origine, octet pour octet, **sauf** le contenu de `<Items>` des dossiers (leurs
//!    enfants sont des lignes à part ; il reste `<Items></Items>`). Ce découpage prépare la
//!    v0.9.0 (création, modification, suppression, copie d'une liste = une ligne, sans
//!    réécrire tout le bloc). [`Store::comic_lists`] reconstruit le document et le relit
//!    avec [`parse_comic_lists`] ; [`Store::comic_lists_xml`] rend le document lui-même.
//!
//!    Liste sans `Id` ou dont l'`Id` est déjà pris : identifiant **calculé** (UUID v5 de
//!    parent, position, nom et rang d'occurrence), le même à chaque transfert du même
//!    `ComicDb.xml` (avertissement) ; le XML garde l'original.
//!
//!    Transfert relancé : les listes de même identifiant sont remplacées ; les listes de
//!    la base absentes de ComicRack (créées dans ComicRust) sont gardées, placées après
//!    celles de ComicRack dans leur dossier (au premier niveau si leur dossier a disparu).
//!    Chaque transfert de listes incrémente `lists_counter` de `comicrust_meta`.
//!
//! 3. **Dossiers** — [`read_watch_folders`] lit `<WatchFolders>` ; avec
//!    [`ImportOptions::folders`], ils sont ajoutés aux dossiers de la base (sans doublon au
//!    sens de `path_key`, ni dossier physiquement identique à un dossier déjà présent :
//!    avertissement « dossier X déjà présent sous la forme Y »). Tous les dossiers sont repris, `Watch="false"` compris :
//!    ComicRack les analyse tous (`MainForm` : `ScanFilesOrFolders(WatchFolders.Folders…)`),
//!    `Watch` ne règle que la surveillance en direct.
//!
//! ## Fusion des lectures
//!
//! Entre la fiche de ComicRack et celle de la base (même identifiant, ou même fichier) :
//!
//! - `opened` : la date **la plus récente** des deux ;
//! - `current_page` : celle de la lecture la plus récente (le côté dont `opened` est
//!   retenu) ; à date égale ou inconnue des deux côtés : la plus grande ;
//! - `last_page_read` : la plus grande ; `open_count` : le plus grand.
//!
//! Une lecture faite dans ComicRust depuis le premier transfert est donc conservée ; une
//! lecture faite ensuite dans ComicRack aussi. Limite : « marquer comme non lu » dans
//! ComicRust (pages remises à zéro) est annulé par un nouveau transfert si ComicRack a la
//! lecture. Tous les autres champs (note, `checked`, `book_*`, pages…) viennent de ComicRack
//! (sauf les colonnes « fichier » gardées, voir plus haut).
//!
//! ## Ce qui est repris de chaque fiche, et ce qui ne l'est pas
//!
//! Repris : tous les champs de [`LibraryBook`], dont les éléments propres à la
//! bibliothèque `CustomValuesStore`, `CustomThumbnailKey`, `LastOpenedFromListId`,
//! `NewPages`, et pour chaque page tous les attributs de `ComicPageInfo` (`Image`, `Type`,
//! `Bookmark` — signets —, `ImageSize`, `ImageWidth`, `ImageHeight`, `Rotation`,
//! `PagePosition`, `Key`), en texte brut.
//!
//! Non repris : `ColorAdjustment` (réglages de couleur par livre, élément complexe que
//! ComicRust n'applique pas), les éléments de fiche que [`LibraryBook`] ne modélise pas
//! (ignorés à la lecture, par exemple `EnableDynamicUpdate`), et le reste de `ComicDb.xml`
//! hors livres, listes et dossiers.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use quick_xml::events::Event;
use quick_xml::Reader;
use same_file::Handle;

use super::conn::{Conn, Val};
use super::schema::{self, BOOKS, LISTS};
use super::{write_rows, Store};
use crate::comic_lists::{attr, local, parse_comic_lists, ComicList};
use crate::db::LoadProgress;
use crate::library::{Library, LibraryBook};
pub(crate) use crate::lists_edit::{assemble_lists, split_lists, ListRow};
use crate::scanner;
use crate::Error;

/// Nombre de livres par transaction lors d'un transfert.
pub const IMPORT_BATCH: usize = 1000;

/// Bibliothèque ComicRack à transférer (lue par l'appelant, jamais modifiée).
#[derive(Debug, Clone, Default)]
pub struct ComicRackSource {
    /// Fiches de ComicRack (`ComicDatabase::load(…).books` pour MySQL, ou
    /// `Library::from_xml(…).books.book` pour `ComicDb.xml`).
    pub books: Vec<LibraryBook>,
    /// Texte complet de `ComicDb.xml` (listes et dossiers surveillés ; toujours dans ce
    /// fichier, même quand ComicRack range ses livres dans MySQL). `None` : ni listes ni
    /// dossiers (un avertissement le signale si les options les demandaient).
    pub comicdb_xml: Option<String>,
}

impl ComicRackSource {
    /// Source tirée d'un `ComicDb.xml` contenant les livres (ComicRack sans MySQL).
    pub fn from_comicdb_xml(xml: String) -> Result<Self, Error> {
        let books = Library::from_xml(&xml)?.books.book;
        Ok(Self { books, comicdb_xml: Some(xml) })
    }
}

/// Ce qu'il faut transférer en plus des livres (par défaut : tout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    /// Reprendre les dossiers de ComicRack (`<WatchFolders>`).
    pub folders: bool,
    /// Reprendre les listes de ComicRack (`<ComicLists>`).
    pub lists: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { folders: true, lists: true }
    }
}

/// Compte rendu d'un transfert.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    /// Livres absents de la base, ajoutés.
    pub books_added: usize,
    /// Livres déjà présents sous un autre identifiant (même fichier, ajoutés par une
    /// analyse) : remplacés par la fiche ComicRack.
    pub books_replaced: usize,
    /// Livres déjà présents avec le même identifiant (transfert relancé) : mis à jour.
    pub books_updated: usize,
    /// Fiches ComicRack écartées (sans identifiant, ou identifiant en double).
    pub books_skipped: usize,
    /// Chemins de fichiers de ComicRack réécrits sous un dossier surveillé qui est
    /// physiquement le même dossier (`Z:\BD\…` → `\\SERVEURNAS\BD\…`, voir le module).
    pub paths_rewritten: usize,
    /// Livres de même identifiant dont le fichier de la base est gardé : converti ou
    /// déplacé dans ComicRust depuis le transfert précédent (celui de ComicRack n'existe
    /// plus, celui de la base existe).
    pub files_kept: usize,
    /// Nombre de listes enregistrées (tous niveaux : dossiers et leurs listes compris).
    pub lists: usize,
    /// Dossiers **ajoutés** à ceux de la base.
    pub folders: Vec<PathBuf>,
    /// Problèmes non bloquants (fiches écartées, `ComicDb.xml` absent ou illisible…).
    pub warnings: Vec<String>,
    /// Transfert interrompu par `cancel` : les lots déjà écrits restent en base ; les
    /// listes et dossiers ne sont pas transférés.
    pub cancelled: bool,
}

impl ImportReport {
    /// Livres écrits (ajoutés + remplacés + mis à jour).
    pub fn books_written(&self) -> usize {
        self.books_added + self.books_replaced + self.books_updated
    }
}

/// Dossiers de ComicRack : `<ComicDatabase><WatchFolders><WatchFolder Folder="…"/>`, dans
/// l'ordre du fichier, doublons exacts écartés. Un XML illisible donne ce qui a pu être lu.
pub fn read_watch_folders(comicdb_xml: &str) -> Vec<PathBuf> {
    let mut r = Reader::from_str(comicdb_xml.trim_start_matches('\u{feff}'));
    let mut out: Vec<PathBuf> = Vec::new();
    let mut depth = 0;
    let mut inside = false;
    loop {
        let ev = match r.read_event() {
            Ok(ev) => ev,
            Err(_) => return out,
        };
        match ev {
            Event::Start(e) => {
                depth += 1;
                let name = local(e.name().as_ref()).to_vec();
                if depth == 2 && name == b"WatchFolders" {
                    inside = true;
                } else if depth == 3 && inside && name == b"WatchFolder" {
                    push_folder(&mut out, attr(&e, b"Folder"));
                } else if depth == 2 {
                    // <Books> peut être énorme : sauté d'un bloc.
                    let e = e.into_owned();
                    if r.read_to_end(e.name()).is_err() {
                        return out;
                    }
                    depth -= 1;
                }
            }
            Event::Empty(e) if depth == 2 && inside && local(e.name().as_ref()) == b"WatchFolder" => {
                push_folder(&mut out, attr(&e, b"Folder"));
            }
            Event::End(_) => {
                if depth == 2 {
                    inside = false;
                }
                depth -= 1;
            }
            Event::Eof => return out,
            _ => {}
        }
    }
}

fn push_folder(out: &mut Vec<PathBuf>, folder: Option<String>) {
    if let Some(f) = folder.filter(|f| !f.trim().is_empty()) {
        let p = PathBuf::from(f);
        if !out.contains(&p) {
            out.push(p);
        }
    }
}

/// Séparateur de chemin (Windows ou Unix).
fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

/// Dossier parent d'un chemin, **sur le texte** (le chemin peut être un chemin Windows
/// lu sous un autre système) : `Z:\BD\x.cbz` → `Z:\BD` → `Z:\` → aucun ;
/// `/srv/bd` → `/srv` → `/` → aucun ; `\\NAS\BD\x` → `\\NAS\BD` → `\\NAS` → `\\` → aucun.
pub(crate) fn parent_dir(p: &str) -> Option<String> {
    let t = p.trim_end_matches(is_sep);
    let i = t.rfind(is_sep)?;
    let head = &t[..i];
    // Racine (`/`, `Z:\`, `\\`) : on garde le séparateur.
    let parent = if head.is_empty() || head.ends_with(':') || head.chars().all(is_sep) { &t[..=i] } else { head };
    (parent != p).then(|| parent.to_string())
}

/// `rest` (chemin relatif, tiré de `orig`) placé sous le dossier `dir` ; séparateur
/// ajouté : `\` si le chemin d'origine est un chemin Windows, `/` sinon.
pub(crate) fn join_under(dir: &str, rest: &str, orig: &str) -> String {
    let rest = rest.trim_start_matches(is_sep);
    let base = dir.trim_end_matches(is_sep);
    if rest.is_empty() {
        return dir.to_string();
    }
    if base.is_empty() {
        // `dir` est la racine `/` (ou `\`).
        return format!("{dir}{rest}");
    }
    let sep = if orig.contains('\\') { '\\' } else { '/' };
    format!("{base}{sep}{rest}")
}

/// Dossiers de ComicRack à ajouter à ceux de la base (`existing`) : sans doublon au sens
/// de `path_key`, ni dossier **physiquement identique** à un dossier déjà présent ou déjà
/// retenu (`Z:\BD` et `\\SERVEURNAS\BD` : avertissement « déjà présent sous la forme… »).
fn plan_folders(existing: &[PathBuf], from_cr: Vec<PathBuf>, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut keys: HashSet<String> = existing.iter().map(|f| scanner::path_key(&f.to_string_lossy())).collect();
    // Identités ouvertes au premier besoin (accès disque, éventuellement réseau).
    let mut known: Vec<(PathBuf, Option<Option<Handle>>)> = existing.iter().map(|f| (f.clone(), None)).collect();
    let mut added = Vec::new();
    for f in from_cr {
        if !keys.insert(scanner::path_key(&f.to_string_lossy())) {
            continue;
        }
        let me = Handle::from_path(&f).ok();
        let twin = me.as_ref().and_then(|me| {
            known.iter_mut().find_map(|(k, h)| {
                let h = h.get_or_insert_with(|| Handle::from_path(&*k).ok());
                (h.as_ref() == Some(me)).then(|| k.clone())
            })
        });
        if let Some(twin) = twin {
            warnings.push(format!("dossier {} déjà présent sous la forme {}", f.display(), twin.display()));
            continue;
        }
        known.push((f.clone(), Some(me)));
        added.push(f);
    }
    added
}

/// Un dossier surveillé, pour [`rewrite_paths`].
struct Watched {
    text: String,
    key: String,
}

/// Recherche, pour un dossier (texte), de l'ancêtre (lui compris) physiquement identique
/// à un dossier surveillé ; résultats mis en cache par dossier (texte) pour n'interroger
/// le disque qu'une fois par dossier (5 000 livres sur un NAS).
struct Resolver<'a> {
    watched: &'a [Watched],
    /// Identités des dossiers surveillés (ouvertes au premier besoin ; `None` : inaccessible).
    handles: Option<Vec<Option<Handle>>>,
    /// Dossier → (ancêtre identique à un dossier surveillé, rang de ce dossier).
    cache: HashMap<String, Option<(String, usize)>>,
}

impl Resolver<'_> {
    /// Rang du dossier surveillé physiquement identique à `dir` (inexistant : aucun).
    fn same_as_watched(&mut self, dir: &str) -> Option<usize> {
        let watched = self.watched;
        let handles =
            self.handles.get_or_insert_with(|| watched.iter().map(|w| Handle::from_path(&w.text).ok()).collect());
        if handles.iter().all(Option::is_none) {
            return None;
        }
        let h = Handle::from_path(dir).ok()?;
        handles.iter().position(|w| w.as_ref() == Some(&h))
    }

    fn resolve(&mut self, dir: &str) -> Option<(String, usize)> {
        let mut chain: Vec<String> = Vec::new();
        let mut cur = Some(dir.to_string());
        let mut found = None;
        while let Some(d) = cur {
            if let Some(r) = self.cache.get(&d) {
                found = r.clone();
                break;
            }
            if let Some(i) = self.same_as_watched(&d) {
                found = Some((d.clone(), i));
                chain.push(d);
                break;
            }
            cur = parent_dir(&d);
            chain.push(d);
        }
        for d in chain {
            self.cache.insert(d, found.clone());
        }
        found
    }
}

/// Réécrit (copies en mémoire seulement) les chemins des fiches qui ne sont sous aucun
/// dossier surveillé mais dont un dossier parent est physiquement un dossier surveillé
/// (voir le module). Renvoie le nombre de chemins réécrits ; avertissements ajoutés.
fn rewrite_paths(books: &mut [Cow<'_, LibraryBook>], watched: &[PathBuf], warnings: &mut Vec<String>) -> usize {
    let watched: Vec<Watched> = watched
        .iter()
        .map(|w| {
            let text = w.to_string_lossy().into_owned();
            Watched { key: scanner::path_key(&text), text }
        })
        .collect();
    let mut resolver = Resolver { watched: &watched, handles: None, cache: HashMap::new() };
    // (ancêtre, dossier surveillé) → nombre de chemins réécrits ; ordre stable.
    let mut rewrites: BTreeMap<(String, usize), usize> = BTreeMap::new();
    let mut outside = 0usize;
    let mut example: Option<String> = None;
    for b in books.iter_mut() {
        let Some(file) = b.file.as_deref() else { continue };
        let key = scanner::path_key(file);
        if watched.iter().any(|w| scanner::is_under(&key, &w.key)) {
            continue;
        }
        let file = file.to_string();
        let found = if watched.is_empty() { None } else { parent_dir(&file).and_then(|d| resolver.resolve(&d)) };
        match found {
            Some((a, i)) => {
                let new = join_under(&watched[i].text, &file[a.len()..], &file);
                b.to_mut().file = Some(new);
                *rewrites.entry((a, i)).or_default() += 1;
            }
            None => {
                outside += 1;
                example.get_or_insert(file);
            }
        }
    }
    let mut total = 0;
    for ((a, i), n) in rewrites {
        let w = &watched[i].text;
        warnings.push(format!("{n} chemins de ComicRack réécrits sous le dossier surveillé {w} ({a} → {w})"));
        total += n;
    }
    if let Some(ex) = example {
        warnings.push(format!(
            "{outside} livres de ComicRack hors de tout dossier surveillé (ex. {ex}) : ils ne seront pas suivis par l'analyse"
        ));
    }
    total
}

/// Lecture (`current_page`, `last_page_read`, `opened`, `open_count`) d'une fiche en base.
#[derive(Debug, Clone, Default, PartialEq)]
struct Reading {
    current_page: Option<u32>,
    last_page_read: Option<u32>,
    opened: Option<String>,
    open_count: Option<u32>,
}

/// Colonnes lues pour la fusion des lectures (après `id` et `file`).
const READING_SELECT: &str = "`id`, `file`, `current_page`, `last_page_read`, `opened`, `open_count`";

fn reading_from_row(row: Vec<Val>) -> (String, Option<String>, Reading) {
    let mut it = row.into_iter();
    let id = it.next().and_then(Val::into_text).unwrap_or_default();
    let file = it.next().and_then(Val::into_text);
    let mut num = || it.next().and_then(|v| v.as_i64()).and_then(|i| u32::try_from(i).ok());
    let current_page = num();
    let last_page_read = num();
    let opened = it.next().and_then(Val::into_text);
    let open_count = it.next().and_then(|v| v.as_i64()).and_then(|i| u32::try_from(i).ok());
    (id, file, Reading { current_page, last_page_read, opened, open_count })
}

/// Date de `opened` comparable : ISO 8601 avec fuseau (ramenée en UTC), sans fuseau, ou
/// date seule. `None` si absente ou illisible.
fn parse_date(s: Option<&str>) -> Option<chrono::NaiveDateTime> {
    let s = s?.trim();
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.naive_utc());
    }
    if let Ok(d) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(d);
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0))
}

/// Fusionne dans `book` (fiche ComicRack) la lecture `other` de la base, selon les règles
/// du module (date la plus récente, pages les plus avancées).
fn merge_reading(book: &mut LibraryBook, other: &Reading) {
    use std::cmp::Ordering as O;
    let mine = parse_date(book.opened.as_deref());
    let theirs = parse_date(other.opened.as_deref());
    let order = match (mine, theirs) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => O::Greater,
        (None, Some(_)) => O::Less,
        // Aucune date lisible : garder un texte présent plutôt que rien.
        (None, None) => match (&book.opened, &other.opened) {
            (None, Some(_)) => O::Less,
            _ => O::Equal,
        },
    };
    match order {
        O::Less => {
            book.opened = other.opened.clone();
            book.current_page = other.current_page;
        }
        O::Equal => book.current_page = book.current_page.max(other.current_page),
        O::Greater => {}
    }
    book.last_page_read = book.last_page_read.max(other.last_page_read);
    book.open_count = book.open_count.max(other.open_count);
}

/// Requête `SELECT {READING_SELECT} … WHERE id IN (…)` par paquets de 200.
fn readings_by_id(conn: &mut Conn, ids: &[String]) -> Result<Vec<(String, Option<String>, Reading)>, Error> {
    let mut out = Vec::new();
    for chunk in ids.chunks(200) {
        let marks = vec!["?"; chunk.len()].join(",");
        let params: Vec<Val> = chunk.iter().map(|i| Val::Text(i.clone())).collect();
        conn.query_each(&format!("SELECT {READING_SELECT} FROM {BOOKS} WHERE `id` IN ({marks})"), &params, |row| {
            out.push(reading_from_row(row));
            Ok(())
        })?;
    }
    Ok(out)
}

/// Résultat d'un lot de livres.
#[derive(Default)]
struct BatchCounts {
    added: usize,
    replaced: usize,
    updated: usize,
    /// Livres de même identifiant dont le fichier de la base est gardé.
    kept: usize,
    /// Identifiants (minuscules) supprimés car remplacés.
    deleted: Vec<String>,
}

impl Store {
    /// Transfère la bibliothèque ComicRack `src` dans la base (voir la documentation du
    /// module [`import`](self) pour les règles exactes). `progress` : `total` = nombre de
    /// fiches, `done` avancé à chaque lot. `cancel` : vérifié avant chaque lot.
    ///
    /// Erreur seulement si la base est inaccessible ou si une écriture échoue (les lots
    /// déjà écrits restent) ; un `ComicDb.xml` absent ou illisible n'empêche pas le
    /// transfert des livres (avertissement).
    ///
    /// Après l'appel, l'interface recharge la bibliothèque ([`Store::load`]) — ou laisse
    /// son [`StoreWatcher`](super::StoreWatcher) ramener les fiches écrites — ainsi que
    /// les listes ([`Store::comic_lists`]) et les dossiers ([`Store::folders`]).
    pub fn import_comicrack(
        &mut self,
        src: &ComicRackSource,
        opts: &ImportOptions,
        progress: Option<&LoadProgress>,
        cancel: Option<&AtomicBool>,
    ) -> Result<ImportReport, Error> {
        let mut report = ImportReport::default();
        let cancelled = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));

        // Fiches valides, sans doublon d'identifiant (la première est gardée).
        let mut cr_ids: HashSet<String> = HashSet::new();
        let mut books: Vec<Cow<'_, LibraryBook>> = Vec::with_capacity(src.books.len());
        for b in &src.books {
            if b.id.trim().is_empty() {
                report.books_skipped += 1;
                report.warnings.push(format!("fiche sans identifiant écartée : {}", b.caption()));
            } else if !cr_ids.insert(b.id.to_ascii_lowercase()) {
                report.books_skipped += 1;
                report.warnings.push(format!("identifiant {} en double, fiche écartée : {}", b.id, b.caption()));
            } else {
                books.push(Cow::Borrowed(b));
            }
        }

        // Dossiers surveillés après le transfert (ceux de la base, plus ceux de ComicRack
        // si demandé ; enregistrés à la fin), puis chemins de ComicRack ramenés sous ces
        // dossiers quand ils y sont physiquement (`Z:\BD` = `\\SERVEURNAS\BD`), avant la
        // recherche des fichiers déjà en base : remplacement et fusion en tiennent compte.
        let xml = src.comicdb_xml.as_deref();
        let existing = self.folders()?;
        let new_folders = match (opts.folders, xml) {
            (true, Some(xml)) => plan_folders(&existing, read_watch_folders(xml), &mut report.warnings),
            _ => Vec::new(),
        };
        let watched: Vec<PathBuf> = existing.iter().chain(&new_folders).cloned().collect();
        report.paths_rewritten = rewrite_paths(&mut books, &watched, &mut report.warnings);
        let books: Vec<&LibraryBook> = books.iter().map(|b| b.as_ref()).collect();

        if let Some(p) = progress {
            p.total.store(books.len(), Ordering::Relaxed);
            p.done.store(0, Ordering::Relaxed);
        }

        // Fichiers déjà en base sous un identifiant inconnu de ComicRack : candidats au
        // remplacement (vérifiés à nouveau dans la transaction de chaque lot).
        let mut by_path: HashMap<String, Vec<String>> = HashMap::new();
        self.conn.query_each(&format!("SELECT `id`, `file` FROM {BOOKS} WHERE `file` IS NOT NULL"), &[], |row| {
            let mut it = row.into_iter();
            if let (Some(id), Some(file)) = (it.next().and_then(Val::into_text), it.next().and_then(Val::into_text)) {
                if !cr_ids.contains(&id.to_ascii_lowercase()) {
                    by_path.entry(scanner::path_key(&file)).or_default().push(id);
                }
            }
            Ok(())
        })?;

        for chunk in books.chunks(IMPORT_BATCH) {
            if cancelled() {
                report.cancelled = true;
                return Ok(report);
            }
            let counts = self.conn.write_tx(|conn| import_batch(conn, chunk, &by_path))?;
            report.books_added += counts.added;
            report.books_replaced += counts.replaced;
            report.books_updated += counts.updated;
            report.files_kept += counts.kept;
            if !counts.deleted.is_empty() {
                let gone: HashSet<&String> = counts.deleted.iter().collect();
                for v in by_path.values_mut() {
                    v.retain(|id| !gone.contains(&id.to_ascii_lowercase()));
                }
            }
            if let Some(p) = progress {
                p.done.fetch_add(chunk.len(), Ordering::Relaxed);
            }
        }
        if report.files_kept > 0 {
            report.warnings.push(format!(
                "{} livres gardent leur fichier de ComicRust (converti ou déplacé depuis le transfert précédent : \
                 le fichier indiqué par ComicRack n'existe plus)",
                report.files_kept
            ));
        }
        if cancelled() {
            report.cancelled = true;
            return Ok(report);
        }

        if xml.is_none() && (opts.lists || opts.folders) {
            report.warnings.push("ComicDb.xml absent : ni listes ni dossiers transférés".into());
        }
        if let (true, Some(xml)) = (opts.lists, xml) {
            match split_lists(xml, &mut report.warnings) {
                Ok(rows) => {
                    report.lists = rows.len();
                    self.conn.write_tx(|conn| save_lists(conn, &rows))?;
                }
                Err(e) => report.warnings.push(format!("listes non transférées : {e}")),
            }
        }
        if !new_folders.is_empty() {
            self.set_folders(&watched)?;
            report.folders = new_folders;
        }
        Ok(report)
    }

    /// Compteur des modifications de listes (`lists_counter`) : change à chaque transfert
    /// (et, à partir de la v0.9.0, à chaque modification de liste). Sert à la surveillance.
    pub fn lists_counter(&mut self) -> Result<i64, Error> {
        schema::meta_int(&mut self.conn, "lists_counter")
    }

    /// Listes enregistrées dans la base (transférées de ComicRack ou, à partir de la
    /// v0.9.0, créées dans ComicRust), relues comme celles d'un `ComicDb.xml`.
    pub fn comic_lists(&mut self) -> Result<Vec<ComicList>, Error> {
        match self.comic_lists_xml()? {
            Some(doc) => parse_comic_lists(&doc),
            None => Ok(Vec::new()),
        }
    }

    /// Document `<ComicDatabase><ComicLists>…</ComicLists></ComicDatabase>` reconstruit à
    /// partir de `comicrust_lists` (`None` si la table est vide), au format de ComicRack.
    pub fn comic_lists_xml(&mut self) -> Result<Option<String>, Error> {
        let rows = read_list_rows(&mut self.conn)?;
        if rows.is_empty() {
            return Ok(None);
        }
        Ok(Some(format!(
            "<?xml version=\"1.0\"?>\n<ComicDatabase xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" \
             xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n<ComicLists>{}</ComicLists>\n</ComicDatabase>",
            assemble_lists(&rows)
        )))
    }
}

pub(super) fn read_list_rows(conn: &mut Conn) -> Result<Vec<ListRow>, Error> {
    let mut rows = Vec::new();
    conn.query_each(&format!("SELECT `id`, `parent`, `position`, `name`, `kind`, `xml` FROM {LISTS}"), &[], |r| {
        let mut it = r.into_iter();
        rows.push(ListRow {
            id: it.next().and_then(Val::into_text).unwrap_or_default(),
            parent: it.next().and_then(Val::into_text),
            position: it.next().and_then(|v| v.as_i64()).unwrap_or(0),
            name: it.next().and_then(Val::into_text).unwrap_or_default(),
            kind: it.next().and_then(Val::into_text).unwrap_or_default(),
            xml: it.next().and_then(Val::into_text).unwrap_or_default(),
        });
        Ok(())
    })?;
    Ok(rows)
}

/// Enregistre les listes de ComicRack (dans une transaction) : remplace les lignes de
/// même identifiant, garde les autres après elles (voir le module).
fn save_lists(conn: &mut Conn, rows: &[ListRow]) -> Result<(), Error> {
    let imported: HashSet<String> = rows.iter().map(|r| r.id.to_ascii_lowercase()).collect();
    let kept: Vec<ListRow> =
        read_list_rows(conn)?.into_iter().filter(|r| !imported.contains(&r.id.to_ascii_lowercase())).collect();
    let ids: Vec<String> = imported.iter().cloned().collect();
    for chunk in ids.chunks(200) {
        let marks = vec!["?"; chunk.len()].join(",");
        let params: Vec<Val> = chunk.iter().map(|i| Val::Text(i.clone())).collect();
        conn.exec(&format!("DELETE FROM {LISTS} WHERE `id` IN ({marks})"), &params)?;
    }
    // Une ligne par requête : une liste peut peser plusieurs centaines de Ko.
    let insert =
        format!("INSERT INTO {LISTS} (`id`, `parent`, `position`, `name`, `kind`, `xml`) VALUES (?,?,?,?,?,?)");
    for r in rows {
        conn.exec(
            &insert,
            &[
                Val::Text(r.id.clone()),
                r.parent.clone().map_or(Val::Null, Val::Text),
                Val::Int(r.position),
                Val::Text(r.name.clone()),
                Val::Text(r.kind.clone()),
                Val::Text(r.xml.clone()),
            ],
        )?;
    }
    // Listes propres à ComicRust : après celles de ComicRack, dans leur ordre.
    let all: HashSet<String> = imported.iter().cloned().chain(kept.iter().map(|r| r.id.to_ascii_lowercase())).collect();
    let mut next: HashMap<Option<String>, i64> = HashMap::new();
    for r in rows {
        let e = next.entry(r.parent.as_ref().map(|p| p.to_ascii_lowercase())).or_insert(0);
        *e = (*e).max(r.position + 1);
    }
    let mut kept = kept;
    kept.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
    for r in &kept {
        let parent = r.parent.as_ref().map(|p| p.to_ascii_lowercase()).filter(|p| all.contains(p));
        let pos = next.entry(parent.clone()).or_insert(0);
        let position = *pos;
        *pos += 1;
        if parent.as_deref() != r.parent.as_deref().map(str::to_ascii_lowercase).as_deref() || position != r.position {
            conn.exec(
                &format!("UPDATE {LISTS} SET `parent` = ?, `position` = ? WHERE `id` = ?"),
                &[parent.map_or(Val::Null, Val::Text), Val::Int(position), Val::Text(r.id.clone())],
            )?;
        }
    }
    schema::bump(conn, "lists_counter")?;
    Ok(())
}

/// Écrit un lot de fiches ComicRack (dans une transaction), avec fusion des lectures et
/// remplacement des livres de même fichier.
fn import_batch(
    conn: &mut Conn,
    chunk: &[&LibraryBook],
    by_path: &HashMap<String, Vec<String>>,
) -> Result<BatchCounts, Error> {
    let counter = schema::bump(conn, "update_counter")?;
    let mut counts = BatchCounts::default();

    let ids: Vec<String> = chunk.iter().map(|b| b.id.clone()).collect();
    let mut same_id: HashMap<String, Reading> = HashMap::new();
    // Même identifiant, fichier différent : converti (CBR → CBZ) ou déplacé dans ComicRust
    // depuis le transfert précédent si le fichier de la base existe et pas celui de
    // ComicRack. On garde alors les colonnes « fichier » de la base (sinon le livre
    // deviendrait manquant et le nouveau fichier reviendrait comme un autre livre).
    let cr_files: HashMap<String, Option<&str>> =
        chunk.iter().map(|b| (b.id.to_ascii_lowercase(), b.file.as_deref())).collect();
    let mut keep_ids: Vec<String> = Vec::new();
    for (id, file, r) in readings_by_id(conn, &ids)? {
        let id = id.to_ascii_lowercase();
        if let (Some(db_file), Some(Some(cr_file))) = (file.as_deref(), cr_files.get(&id)) {
            if scanner::path_key(db_file) != scanner::path_key(cr_file)
                && Path::new(db_file).exists()
                && !Path::new(cr_file).exists()
            {
                keep_ids.push(id.clone());
            }
        }
        same_id.insert(id, r);
    }
    let mut kept_files: HashMap<String, LibraryBook> = HashMap::new();
    for part in keep_ids.chunks(200) {
        let marks = vec!["?"; part.len()].join(",");
        let params: Vec<Val> = part.iter().map(|i| Val::Text(i.clone())).collect();
        let sql = format!("SELECT {} FROM {BOOKS} WHERE `id` IN ({marks})", schema::column_list());
        conn.query_each(&sql, &params, |row| {
            let b = schema::book_from_row(row).0;
            kept_files.insert(b.id.to_ascii_lowercase(), b);
            Ok(())
        })?;
    }

    // Candidats « même fichier, autre identifiant », relus dans la transaction.
    let mut candidates: Vec<String> = Vec::new();
    for b in chunk {
        if let Some(v) = b.file.as_deref().and_then(|f| by_path.get(&scanner::path_key(f))) {
            candidates.extend(v.iter().cloned());
        }
    }
    candidates.sort();
    candidates.dedup();
    let mut others: HashMap<String, Vec<(String, Reading)>> = HashMap::new();
    for (id, file, r) in readings_by_id(conn, &candidates)? {
        if let Some(f) = file {
            others.entry(scanner::path_key(&f)).or_default().push((id, r));
        }
    }

    let mut out: Vec<LibraryBook> = Vec::with_capacity(chunk.len());
    let mut deleted: Vec<String> = Vec::new();
    for b in chunk {
        let mut book = (*b).clone();
        if let Some(r) = same_id.get(&b.id.to_ascii_lowercase()) {
            merge_reading(&mut book, r);
            counts.updated += 1;
        }
        if let Some(db) = kept_files.get(&b.id.to_ascii_lowercase()) {
            book.file = db.file.clone();
            book.file_size = db.file_size;
            book.file_modified_time = db.file_modified_time.clone();
            book.file_creation_time = db.file_creation_time.clone();
            book.page_count = db.page_count;
            book.pages = db.pages.clone();
            counts.kept += 1;
        }
        // Chaque livre remplacé ne sert qu'une fois (doublons de fichier dans ComicRack).
        if let Some(list) = b.file.as_deref().and_then(|f| others.get_mut(&scanner::path_key(f))) {
            let replaced = !list.is_empty();
            for (id, r) in list.drain(..) {
                merge_reading(&mut book, &r);
                deleted.push(id);
            }
            if replaced && !same_id.contains_key(&b.id.to_ascii_lowercase()) {
                counts.replaced += 1;
            }
        }
        out.push(book);
    }
    // Ni même identifiant ni même fichier : ajouté.
    counts.added = chunk.len() - counts.updated - counts.replaced;

    if !deleted.is_empty() {
        for part in deleted.chunks(200) {
            let marks = vec!["?"; part.len()].join(",");
            let params: Vec<Val> = part.iter().map(|i| Val::Text(i.clone())).collect();
            conn.exec(&format!("DELETE FROM {BOOKS} WHERE `id` IN ({marks})"), &params)?;
        }
        schema::bump(conn, "delete_counter")?;
    }
    let refs: Vec<&LibraryBook> = out.iter().collect();
    write_rows(conn, &refs, counter, "REPLACE")?;
    counts.deleted = deleted.into_iter().map(|i| i.to_ascii_lowercase()).collect();
    Ok(counts)
}
