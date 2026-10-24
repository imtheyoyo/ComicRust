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
//!    - les livres de la base absents de ComicRack ne sont pas touchés.
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
//!    Transfert relancé : les listes de même identifiant sont remplacées ; les listes de
//!    la base absentes de ComicRack (créées dans ComicRust) sont gardées, placées après
//!    celles de ComicRack dans leur dossier (au premier niveau si leur dossier a disparu).
//!    Chaque transfert de listes incrémente `lists_counter` de `comicrust_meta`.
//!
//! 3. **Dossiers** — [`read_watch_folders`] lit `<WatchFolders>` ; avec
//!    [`ImportOptions::folders`], ils sont ajoutés aux dossiers de la base (sans doublon au
//!    sens de `path_key`). Tous les dossiers sont repris, `Watch="false"` compris :
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
//! lecture. Tous les autres champs (note, `checked`, `book_*`, pages…) viennent de ComicRack.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use quick_xml::events::Event;
use quick_xml::Reader;

use super::conn::{Conn, Val};
use super::schema::{self, BOOKS, LISTS};
use super::{write_rows, Store};
use crate::comic_lists::{attr, local, parse_comic_lists, xml_err, ComicList};
use crate::db::LoadProgress;
use crate::library::{Library, LibraryBook};
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

/// Une ligne de `comicrust_lists`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListRow {
    pub id: String,
    pub parent: Option<String>,
    pub position: i64,
    pub name: String,
    pub kind: String,
    pub xml: String,
}

/// Découpe `<ComicLists>` d'un `ComicDb.xml` en lignes (une par `<Item>`, tous niveaux).
/// Identifiant absent ou en double : remplacé par un nouveau GUID **pour la ligne**
/// seulement (le XML garde l'original), avec un avertissement.
pub(crate) fn split_lists(comicdb_xml: &str, warnings: &mut Vec<String>) -> Result<Vec<ListRow>, Error> {
    let text = comicdb_xml.trim_start_matches('\u{feff}');
    let mut r = Reader::from_str(text);
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    let mut depth = 0;
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 && local(e.name().as_ref()) == b"ComicLists" {
                    let mut sp = Splitter { text, rows: &mut rows, seen: &mut seen, warnings };
                    sp.items(&mut r, None)?;
                    return Ok(rows);
                }
                if depth == 2 {
                    let e = e.into_owned();
                    r.read_to_end(e.name()).map_err(xml_err)?;
                    depth -= 1;
                }
            }
            Event::End(_) => depth -= 1,
            Event::Eof => return Ok(rows),
            _ => {}
        }
    }
}

struct Splitter<'a, 'w> {
    text: &'a str,
    rows: &'a mut Vec<ListRow>,
    seen: &'a mut HashSet<String>,
    warnings: &'w mut Vec<String>,
}

impl Splitter<'_, '_> {
    /// Lit les `<Item>` d'un conteneur jusqu'à sa balise fermante ; renvoie la position
    /// (octets) du début de cette balise fermante.
    fn items(&mut self, r: &mut Reader<&[u8]>, parent: Option<&str>) -> Result<usize, Error> {
        let mut position = 0i64;
        loop {
            let before = r.buffer_position() as usize;
            match r.read_event().map_err(xml_err)? {
                Event::Start(e) if local(e.name().as_ref()) == b"Item" => {
                    let e = e.into_owned();
                    self.item(r, &e, before, false, parent, position)?;
                    position += 1;
                }
                Event::Empty(e) if local(e.name().as_ref()) == b"Item" => {
                    let e = e.into_owned();
                    self.item(r, &e, before, true, parent, position)?;
                    position += 1;
                }
                Event::Start(e) => {
                    let e = e.into_owned();
                    r.read_to_end(e.name()).map_err(xml_err)?;
                }
                Event::End(_) => return Ok(before),
                Event::Eof => return Err(xml_err("fin de fichier inattendue")),
                _ => {}
            }
        }
    }

    fn item(
        &mut self,
        r: &mut Reader<&[u8]>,
        e: &quick_xml::events::BytesStart,
        start: usize,
        empty: bool,
        parent: Option<&str>,
        position: i64,
    ) -> Result<(), Error> {
        let name = attr(e, b"Name").unwrap_or_default();
        let kind = attr(e, b"xsi:type").unwrap_or_default();
        let mut id = attr(e, b"Id").unwrap_or_default().trim().to_ascii_lowercase();
        if id.is_empty() || !self.seen.insert(id.clone()) {
            let new = super::new_book_id();
            self.warnings.push(format!(
                "liste « {name} » : identifiant {} — enregistrée sous {new}",
                if id.is_empty() { "absent".to_string() } else { format!("{id} en double") }
            ));
            self.seen.insert(new.clone());
            id = new;
        }
        let index = self.rows.len();
        self.rows.push(ListRow {
            id: id.clone(),
            parent: parent.map(str::to_string),
            position,
            name,
            kind,
            xml: String::new(),
        });
        let mut hole: Option<(usize, usize)> = None;
        let end = if empty {
            r.buffer_position() as usize
        } else {
            loop {
                match r.read_event().map_err(xml_err)? {
                    Event::Start(c) if local(c.name().as_ref()) == b"Items" && hole.is_none() => {
                        let inner_start = r.buffer_position() as usize;
                        let inner_end = self.items(r, Some(&id))?;
                        hole = Some((inner_start, inner_end));
                    }
                    Event::Start(c) => {
                        let c = c.into_owned();
                        r.read_to_end(c.name()).map_err(xml_err)?;
                    }
                    Event::End(_) => break r.buffer_position() as usize,
                    Event::Eof => return Err(xml_err("fin de fichier inattendue")),
                    _ => {}
                }
            }
        };
        let xml = match hole {
            Some((a, b)) => format!("{}{}", &self.text[start..a], &self.text[b..end]),
            None => self.text[start..end].to_string(),
        };
        self.rows[index].xml = xml;
        Ok(())
    }
}

/// Insère `children` (XML des enfants, déjà assemblé) dans le `<Item>` `xml` d'un
/// dossier : dans son `<Items>` direct (vide ou `<Items/>`), sinon dans un `<Items>`
/// ajouté avant la fermeture de l'élément.
fn insert_children(xml: &str, children: &str) -> String {
    let mut r = Reader::from_str(xml);
    let mut depth = 0;
    let mut last_end = None;
    loop {
        let before = r.buffer_position() as usize;
        let Ok(ev) = r.read_event() else { break };
        match ev {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 && local(e.name().as_ref()) == b"Items" {
                    let at = r.buffer_position() as usize;
                    return format!("{}{children}{}", &xml[..at], &xml[at..]);
                }
            }
            Event::Empty(e) => {
                if depth == 1 && local(e.name().as_ref()) == b"Items" {
                    let after = r.buffer_position() as usize;
                    return format!("{}<Items>{children}</Items>{}", &xml[..before], &xml[after..]);
                }
                if depth == 0 {
                    // `<Item … />` : élément vide transformé en élément avec enfants.
                    let after = r.buffer_position() as usize;
                    let tag = xml[before..after].trim_end_matches('>').trim_end_matches('/').trim_end();
                    let qname = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    return format!("{tag}><Items>{children}</Items></{qname}>{}", &xml[after..]);
                }
            }
            Event::End(_) => {
                if depth == 1 {
                    last_end = Some(before);
                }
                depth -= 1;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    match last_end {
        Some(at) => format!("{}<Items>{children}</Items>{}", &xml[..at], &xml[at..]),
        None => xml.to_string(),
    }
}

/// Reconstruit le contenu de `<ComicLists>` à partir des lignes (ordre `position`, puis
/// identifiant ; parent introuvable = premier niveau).
pub(crate) fn assemble_lists(rows: &[ListRow]) -> String {
    let ids: HashSet<String> = rows.iter().map(|r| r.id.to_ascii_lowercase()).collect();
    let mut children: HashMap<Option<String>, Vec<&ListRow>> = HashMap::new();
    for r in rows {
        let parent = r.parent.as_ref().map(|p| p.to_ascii_lowercase()).filter(|p| ids.contains(p));
        children.entry(parent).or_default().push(r);
    }
    for v in children.values_mut() {
        v.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
    }
    fn render(
        row: &ListRow,
        children: &HashMap<Option<String>, Vec<&ListRow>>,
        visiting: &mut HashSet<String>,
    ) -> String {
        let id = row.id.to_ascii_lowercase();
        let kids = match children.get(&Some(id.clone())) {
            Some(k) if visiting.insert(id.clone()) => {
                let s: String = k.iter().map(|c| render(c, children, visiting)).collect();
                visiting.remove(&id);
                s
            }
            _ => String::new(),
        };
        if kids.is_empty() {
            row.xml.clone()
        } else {
            insert_children(&row.xml, &kids)
        }
    }
    let mut visiting = HashSet::new();
    children.get(&None).map(|top| top.iter().map(|r| render(r, &children, &mut visiting)).collect()).unwrap_or_default()
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
        let mut books: Vec<&LibraryBook> = Vec::with_capacity(src.books.len());
        for b in &src.books {
            if b.id.trim().is_empty() {
                report.books_skipped += 1;
                report.warnings.push(format!("fiche sans identifiant écartée : {}", b.caption()));
            } else if !cr_ids.insert(b.id.to_ascii_lowercase()) {
                report.books_skipped += 1;
                report.warnings.push(format!("identifiant {} en double, fiche écartée : {}", b.id, b.caption()));
            } else {
                books.push(b);
            }
        }
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
        if cancelled() {
            report.cancelled = true;
            return Ok(report);
        }

        let xml = src.comicdb_xml.as_deref();
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
        if let (true, Some(xml)) = (opts.folders, xml) {
            let mut current = self.folders()?;
            let mut keys: HashSet<String> = current.iter().map(|f| scanner::path_key(&f.to_string_lossy())).collect();
            for f in read_watch_folders(xml) {
                if keys.insert(scanner::path_key(&f.to_string_lossy())) {
                    report.folders.push(f);
                }
            }
            if !report.folders.is_empty() {
                current.extend(report.folders.iter().cloned());
                self.set_folders(&current)?;
            }
        }
        Ok(report)
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

fn read_list_rows(conn: &mut Conn) -> Result<Vec<ListRow>, Error> {
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
    let same_id: HashMap<String, Reading> =
        readings_by_id(conn, &ids)?.into_iter().map(|(id, _, r)| (id.to_ascii_lowercase(), r)).collect();

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
