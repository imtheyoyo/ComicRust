//! Modification des listes de ComicRack (v0.9.0 « gestion complète des listes »).
//!
//! # Principe
//!
//! Les listes sont manipulées sous forme de lignes [`ListRow`] (une par liste, à tous les
//! niveaux : identifiant, parent, rang, nom, type et XML de l'élément `<Item>` sans les
//! enfants d'un dossier), les mêmes que la table `comicrust_lists` de la base de ComicRust
//! ([`crate::store`]) et que le découpage du `<ComicLists>` de `ComicDb.xml`
//! ([`split_lists`], [`assemble_lists`]). [`ListsEditor`] applique les opérations sur ces
//! lignes ; l'appelant enregistre ensuite le résultat :
//!
//! - base de ComicRust : [`crate::store::Store::save_list_rows`] (refus si un autre poste a
//!   modifié les listes entre-temps) ;
//! - `ComicDb.xml` de ComicRack : [`ComicDbLists::save`] (voir [`comicdb`] : refus si
//!   ComicRack est lancé, sauvegarde, remplacement atomique).
//!
//! # Ce qui est conservé
//!
//! Chaque opération ne réécrit que ce qu'elle change, **dans l'élément concerné** :
//! attribut `Name`, élément `<Description>`, `<BookIds>`, `<Matchers>` et réglages de la
//! liste intelligente, attribut `CombineMode`… Tout le reste de l'élément — réglages
//! d'affichage (`<Display>`), favori, `QuickOpen`, `NewBookCountDate`, éléments inconnus de
//! ComicRust — est gardé octet pour octet, et une liste non modifiée reste identique octet
//! pour octet. Les éléments ajoutés suivent l'ordre et les règles de `XmlSerializer`
//! (voir le module `lists_edit::xml`) : ComicRack relit sans perte ce que ComicRust écrit.
//!
//! Quand le **contenu** d'une liste change (livres, critères, mode d'un dossier, sous-liste
//! ajoutée, retirée ou déplacée), les résultats mémorisés par ComicRack deviennent faux :
//! `<CacheStorage>`, `<BookCount>`, `<NewBookCount>` et `<UnreadBookCount>` sont retirés de
//! cette liste, de ses dossiers parents et des listes intelligentes qui en dépendent (par
//! `BaseListId`, de proche en proche). ComicRack les recalcule à son prochain démarrage.
//!
//! # Règles reprises de ComicRack (vérifiées dans la référence C#)
//!
//! - Nouvelle liste : identifiant GUID v4 (`IdComponent` : `Guid.NewGuid()`), insérée à
//!   la position demandée (ComicRack : juste après la liste sélectionnée).
//! - La liste « Bibliothèque » (`ComicLibraryListItem`) ne peut être ni supprimée ni copiée
//!   (`RemoveListOrFolder`, `CopyList` : seulement les `ShareableComicListItem`).
//! - Suppression d'un dossier : ses sous-listes avec lui. ComicRack ne vérifie pas les
//!   listes intelligentes qui partaient de la liste supprimée (`BaseListId` devient
//!   introuvable : elles partent alors de toute la bibliothèque) ; ComicRust les signale.
//! - Copie (`Clone()` de ComicRack) : nouveaux identifiants pour la liste et ses enfants ;
//!   ComicRust nomme la copie « Copie de X » et fait suivre les `BaseListId` internes à la
//!   copie.
//! - Déplacement : impossible dans soi-même ou un descendant (`RecursionTest`).
//! - Liste intelligente → liste de lecture : ComicRack sait le faire
//!   (`new ComicIdListItem(ComicListItem)` : nom, description et affichage repris, livres
//!   = contenu de la liste) ; ComicRust garde en plus l'identifiant, pour que les listes
//!   qui en partaient continuent de fonctionner.
//! - Liste de lecture : un livre n'y figure qu'une fois (`OnGetBooks` retire les doublons).
//! - Liste intelligente : une liste ne peut pas partir d'elle-même, ni d'une liste qui
//!   dépend d'elle (`GetBaseList` / `RecursionTest`).

pub mod comicdb;
mod rows;
#[cfg(test)]
mod tests;
pub(crate) mod xml;

use std::collections::HashSet;

pub use comicdb::{comicrack_running, ensure_comicrack_closed, ComicDbLists, COMICRACK_UNKNOWN};
pub use rows::{assemble_lists, split_lists, ListRow};
pub use xml::xml_safe;

use crate::comic_lists::{parse_comic_lists, Combine, ComicList};
use crate::smart::SmartQuery;
use crate::Error;
use xml::{guid_list, ItemXml};

pub const KIND_LIBRARY: &str = "ComicLibraryListItem";
pub const KIND_FOLDER: &str = "ComicListItemFolder";
pub const KIND_READING: &str = "ComicIdListItem";
pub const KIND_SMART: &str = "ComicSmartListItem";

/// Éléments mémorisés par ComicRack, faux dès que le contenu d'une liste change.
const STALE: [&str; 4] = ["CacheStorage", "BookCount", "NewBookCount", "UnreadBookCount"];

/// Type d'une nouvelle liste.
#[derive(Debug, Clone, PartialEq)]
pub enum NewList {
    Folder {
        combine: Combine,
    },
    /// Liste de lecture, avec ses livres (identifiants, dans l'ordre).
    Reading {
        books: Vec<String>,
    },
    Smart {
        query: SmartQuery,
    },
}

/// Résultat d'une suppression.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeleteReport {
    /// Listes supprimées (la liste et ses sous-listes).
    pub removed: Vec<String>,
    /// Listes intelligentes restantes qui partaient d'une liste supprimée
    /// (identifiant, nom) : elles partent désormais de toute la bibliothèque.
    pub broken_base: Vec<(String, String)>,
}

fn err(msg: impl Into<String>) -> Error {
    Error::Database(msg.into())
}

fn combine_name(c: Combine) -> Option<&'static str> {
    match c {
        Combine::Or => None,
        Combine::And => Some("And"),
        Combine::Empty => Some("Empty"),
    }
}

/// Listes en cours de modification.
#[derive(Debug, Clone, Default)]
pub struct ListsEditor {
    rows: Vec<ListRow>,
}

impl ListsEditor {
    pub fn new(rows: Vec<ListRow>) -> Self {
        ListsEditor { rows }
    }

    /// Listes d'un `ComicDb.xml` (texte complet) ; les avertissements du découpage
    /// (identifiant absent ou en double) sont ajoutés à `warnings`.
    pub fn from_comicdb_xml(xml: &str, warnings: &mut Vec<String>) -> Result<Self, Error> {
        Ok(Self::new(split_lists(xml, warnings)?))
    }

    pub fn rows(&self) -> &[ListRow] {
        &self.rows
    }

    pub fn into_rows(self) -> Vec<ListRow> {
        self.rows
    }

    /// Contenu de `<ComicLists>` reconstruit.
    pub fn comic_lists_xml(&self) -> String {
        assemble_lists(&self.rows)
    }

    /// Document minimal `<ComicDatabase><ComicLists>…` (relisible par [`parse_comic_lists`]).
    pub fn document(&self) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<ComicDatabase xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" \
             xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n<ComicLists>{}</ComicLists>\n</ComicDatabase>",
            self.comic_lists_xml()
        )
    }

    /// Listes relues (comme celles d'un `ComicDb.xml`).
    pub fn lists(&self) -> Result<Vec<ComicList>, Error> {
        parse_comic_lists(&self.document())
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    fn index(&self, id: &str) -> Result<usize, Error> {
        let id = id.trim();
        self.rows
            .iter()
            .position(|r| r.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| err(format!("liste {id} introuvable")))
    }

    pub fn row(&self, id: &str) -> Option<&ListRow> {
        self.index(id).ok().map(|i| &self.rows[i])
    }

    /// Parent effectif (un parent introuvable compte comme le premier niveau, comme dans
    /// [`assemble_lists`]).
    fn parent_of(&self, i: usize) -> Option<String> {
        let p = self.rows[i].parent.as_deref()?.to_ascii_lowercase();
        self.rows.iter().any(|r| r.id.eq_ignore_ascii_case(&p)).then_some(p)
    }

    /// Indices des listes d'un dossier (`None` : premier niveau), dans l'ordre.
    fn siblings(&self, parent: Option<&str>) -> Vec<usize> {
        let parent = parent.map(str::to_ascii_lowercase);
        let mut v: Vec<usize> = (0..self.rows.len()).filter(|&i| self.parent_of(i) == parent).collect();
        v.sort_by(|&a, &b| {
            self.rows[a].position.cmp(&self.rows[b].position).then_with(|| self.rows[a].id.cmp(&self.rows[b].id))
        });
        v
    }

    /// Renumérote les rangs d'un dossier (0, 1, 2…), dans l'ordre donné.
    fn renumber(&mut self, order: &[usize]) {
        for (n, &i) in order.iter().enumerate() {
            self.rows[i].position = n as i64;
        }
    }

    fn depth(&self, i: usize) -> usize {
        let mut d = 0;
        let mut cur = self.parent_of(i);
        while let Some(p) = cur {
            d += 1;
            if d > 64 {
                break;
            }
            cur = self.index(&p).ok().and_then(|j| self.parent_of(j));
        }
        d
    }

    /// Identifiants des dossiers parents (du plus proche au plus lointain).
    fn ancestors(&self, i: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = self.parent_of(i);
        while let Some(p) = cur {
            if out.contains(&p) {
                break;
            }
            cur = self.index(&p).ok().and_then(|j| self.parent_of(j));
            out.push(p);
        }
        out
    }

    /// La liste et toutes ses sous-listes (identifiants en minuscules).
    fn subtree(&self, id: &str) -> Vec<String> {
        let mut out = vec![id.to_ascii_lowercase()];
        let mut k = 0;
        while k < out.len() {
            let cur = out[k].clone();
            for i in self.siblings(Some(&cur)) {
                let c = self.rows[i].id.to_ascii_lowercase();
                if !out.contains(&c) {
                    out.push(c);
                }
            }
            k += 1;
        }
        out
    }

    /// Fin de ligne des listes existantes (`\r\n` pour un `ComicDb.xml` écrit sous Windows).
    fn newline(&self) -> &'static str {
        self.rows.iter().map(|r| xml::newline_of(&r.xml)).find(|n| *n == "\r\n").unwrap_or("\n")
    }

    fn item(&self, i: usize) -> Result<ItemXml, Error> {
        ItemXml::parse(&self.rows[i].xml, self.depth(i))
    }

    /// Écrit l'élément modifié (et aligne son attribut `Id` sur l'identifiant de la ligne).
    fn store(&mut self, i: usize, mut item: ItemXml) {
        let id = self.rows[i].id.clone();
        if item.attr("Id").is_none_or(|v| !v.eq_ignore_ascii_case(&id)) {
            item.set_attr("Id", Some(&id));
        }
        self.rows[i].xml = item.render();
    }

    fn base_of(&self, i: usize) -> Option<String> {
        if self.rows[i].kind != KIND_SMART {
            return None;
        }
        let item = self.item(i).ok()?;
        let b = item.child_text("BaseListId")?.trim().to_ascii_lowercase();
        (!b.is_empty() && b != "00000000-0000-0000-0000-000000000000").then_some(b)
    }

    fn expect_kind(&self, i: usize, kind: &str, what: &str) -> Result<(), Error> {
        if self.rows[i].kind == kind {
            Ok(())
        } else {
            Err(err(format!("« {} » n'est pas {what}", self.rows[i].name)))
        }
    }

    /// Le calcul de la liste `a` a-t-il besoin de la liste `b` (`a` = `b`, sous-liste d'un
    /// dossier, liste de départ d'une liste intelligente, de proche en proche) ?
    pub fn depends_on(&self, a: &str, b: &str) -> bool {
        let b = b.to_ascii_lowercase();
        let mut todo = vec![a.to_ascii_lowercase()];
        let mut seen = HashSet::new();
        while let Some(cur) = todo.pop() {
            if cur == b {
                return true;
            }
            if !seen.insert(cur.clone()) {
                continue;
            }
            let Ok(i) = self.index(&cur) else { continue };
            for c in self.siblings(Some(&cur)) {
                todo.push(self.rows[c].id.to_ascii_lowercase());
            }
            if let Some(base) = self.base_of(i) {
                todo.push(base);
            }
        }
        false
    }

    /// Listes intelligentes (identifiant, nom) qui partent de la liste `id` ou d'une de
    /// ses sous-listes, hors de cette branche : à signaler avant de la supprimer.
    pub fn base_list_dependents(&self, id: &str) -> Vec<(String, String)> {
        let branch: HashSet<String> = self.subtree(id).into_iter().collect();
        (0..self.rows.len())
            .filter(|&i| !branch.contains(&self.rows[i].id.to_ascii_lowercase()))
            .filter(|&i| self.base_of(i).is_some_and(|b| branch.contains(&b)))
            .map(|i| (self.rows[i].id.clone(), self.rows[i].name.clone()))
            .collect()
    }

    /// Retire les résultats mémorisés par ComicRack de la liste `id`, de ses dossiers
    /// parents et des listes intelligentes qui en dépendent (de proche en proche).
    fn invalidate(&mut self, id: &str) {
        let mut todo = vec![id.to_ascii_lowercase()];
        let mut done: HashSet<String> = HashSet::new();
        while let Some(cur) = todo.pop() {
            if !done.insert(cur.clone()) {
                continue;
            }
            let Ok(i) = self.index(&cur) else { continue };
            todo.extend(self.ancestors(i));
            for j in 0..self.rows.len() {
                if self.base_of(j).as_deref() == Some(cur.as_str()) {
                    todo.push(self.rows[j].id.to_ascii_lowercase());
                }
            }
        }
        for cur in done {
            let Ok(i) = self.index(&cur) else { continue };
            let Ok(mut item) = self.item(i) else { continue };
            if STALE.iter().any(|n| item.has_child(n)) {
                for n in STALE {
                    item.set_child(n, None);
                }
                self.rows[i].xml = item.render();
            }
        }
    }

    fn check_folder(&self, parent: Option<&str>) -> Result<Option<String>, Error> {
        match parent.map(str::trim).filter(|p| !p.is_empty()) {
            None => Ok(None),
            Some(p) => {
                let i = self.index(p)?;
                self.expect_kind(i, KIND_FOLDER, "un dossier")?;
                Ok(Some(self.rows[i].id.to_ascii_lowercase()))
            }
        }
    }

    /// Insère la ligne `i` dans `parent` à la position `position` (fin si `None` ou au-delà).
    fn place(&mut self, i: usize, parent: Option<String>, position: Option<usize>) {
        let mut order: Vec<usize> = self.siblings(parent.as_deref()).into_iter().filter(|&j| j != i).collect();
        let at = position.unwrap_or(order.len()).min(order.len());
        order.insert(at, i);
        self.rows[i].parent = parent;
        self.renumber(&order);
    }

    // -----------------------------------------------------------------------
    // Opérations
    // -----------------------------------------------------------------------

    /// Crée une liste dans le dossier `parent` (`None` : premier niveau) à la position
    /// `position` (fin si `None`) ; renvoie son identifiant (GUID v4).
    pub fn create(
        &mut self,
        parent: Option<&str>,
        position: Option<usize>,
        name: &str,
        kind: NewList,
    ) -> Result<String, Error> {
        let name = clean_name(name)?;
        let parent = self.check_folder(parent)?;
        let id = uuid::Uuid::new_v4().to_string();
        let depth = parent.as_deref().and_then(|p| self.index(p).ok()).map(|p| self.depth(p) + 1).unwrap_or(0);
        let type_name = match &kind {
            NewList::Folder { .. } => KIND_FOLDER,
            NewList::Reading { .. } => KIND_READING,
            NewList::Smart { .. } => KIND_SMART,
        };
        let nl = self.newline();
        let mut item = ItemXml::new(type_name, &id, &name, depth, nl);
        match kind {
            NewList::Folder { combine } => {
                item.set_attr("CombineMode", combine_name(combine));
                item.set_child("Items", Some("<Items />".into()));
            }
            NewList::Reading { books } => {
                let ind = item.child_indent();
                item.set_child("BookIds", Some(guid_list("BookIds", &dedup(books), &ind, nl)));
            }
            NewList::Smart { query } => {
                if let Some(b) = &query.base_list {
                    self.index(b)?;
                }
                item.set_query(&query);
            }
        }
        self.rows.push(ListRow {
            id: id.clone(),
            parent: parent.clone(),
            position: 0,
            name,
            kind: type_name.to_string(),
            xml: item.render(),
        });
        let i = self.rows.len() - 1;
        self.place(i, parent, position);
        self.invalidate(&id);
        Ok(id)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), Error> {
        let name = clean_name(name)?;
        let i = self.index(id)?;
        let mut item = self.item(i)?;
        item.set_attr("Name", Some(&name));
        self.store(i, item);
        self.rows[i].name = name;
        Ok(())
    }

    /// Change la description (vide : retirée, comme ComicRack).
    pub fn set_description(&mut self, id: &str, description: &str) -> Result<(), Error> {
        let i = self.index(id)?;
        let mut item = self.item(i)?;
        let description = xml::xml_safe(description);
        item.set_text_child("Description", (!description.is_empty()).then_some(&*description));
        self.store(i, item);
        Ok(())
    }

    /// Supprime la liste et ses sous-listes.
    pub fn delete(&mut self, id: &str) -> Result<DeleteReport, Error> {
        let i = self.index(id)?;
        if self.rows[i].kind == KIND_LIBRARY {
            return Err(err("la liste « Bibliothèque » ne peut pas être supprimée"));
        }
        let parent = self.parent_of(i);
        let broken_base = self.base_list_dependents(id);
        let removed = self.subtree(id);
        let gone: HashSet<&String> = removed.iter().collect();
        self.rows.retain(|r| !gone.contains(&r.id.to_ascii_lowercase()));
        let order = self.siblings(parent.as_deref());
        self.renumber(&order);
        if let Some(p) = &parent {
            self.invalidate(p);
        }
        for (b, _) in &broken_base {
            self.invalidate(b);
        }
        Ok(DeleteReport { removed, broken_base })
    }

    /// Copie la liste (et ses sous-listes) juste après elle : « Copie de X », nouveaux
    /// identifiants ; renvoie l'identifiant de la copie.
    pub fn copy(&mut self, id: &str) -> Result<String, Error> {
        let i = self.index(id)?;
        if self.rows[i].kind == KIND_LIBRARY {
            return Err(err("la liste « Bibliothèque » ne peut pas être copiée"));
        }
        let branch = self.subtree(id);
        let map: Vec<(String, String)> = branch.iter().map(|b| (b.clone(), uuid::Uuid::new_v4().to_string())).collect();
        let new_id = |old: &str| map.iter().find(|(o, _)| o.eq_ignore_ascii_case(old)).map(|(_, n)| n.clone());
        let mut copies = Vec::new();
        for (old, new) in &map {
            let j = self.index(old)?;
            let src = &self.rows[j];
            let mut item = self.item(j)?;
            item.set_attr("Id", Some(new));
            let mut name = src.name.clone();
            if j == i {
                name = format!("Copie de {name}");
                item.set_attr("Name", Some(&name));
            }
            // ComicRack ne copie pas le résultat mémorisé (`ComicSmartListItem(…)`) ; les
            // compteurs mémorisés (`BookCount`…) sont retirés aussi : ComicRack les recalcule.
            for n in STALE {
                item.set_child(n, None);
            }
            if let Some(b) = item.child_text("BaseListId").and_then(|b| new_id(b.trim())) {
                item.set_text_child("BaseListId", Some(&b));
            }
            copies.push(ListRow {
                id: new.clone(),
                parent: if j == i { self.parent_of(i) } else { src.parent.as_deref().and_then(new_id) },
                position: src.position,
                name,
                kind: src.kind.clone(),
                xml: item.render(),
            });
        }
        let parent = self.parent_of(i);
        let at = self.siblings(parent.as_deref()).iter().position(|&j| j == i).map(|p| p + 1);
        let top = copies[0].id.clone();
        self.rows.extend(copies);
        let k = self.index(&top)?;
        self.place(k, parent, at);
        self.invalidate(&top);
        Ok(top)
    }

    /// Déplace la liste dans le dossier `parent` (`None` : premier niveau), à la position
    /// `position` parmi ses nouvelles voisines (fin si `None`).
    pub fn move_to(&mut self, id: &str, parent: Option<&str>, position: Option<usize>) -> Result<(), Error> {
        let i = self.index(id)?;
        let parent = self.check_folder(parent)?;
        if let Some(p) = &parent {
            if self.subtree(id).contains(p) {
                return Err(err(format!(
                    "impossible de déplacer « {} » dans lui-même ou dans un de ses sous-dossiers",
                    self.rows[i].name
                )));
            }
        }
        let old = self.parent_of(i);
        self.place(i, parent.clone(), position);
        if old != parent {
            let order = self.siblings(old.as_deref());
            self.renumber(&order);
            if let Some(o) = &old {
                self.invalidate(o);
            }
            self.invalidate(id);
        }
        Ok(())
    }

    /// Livres d'une liste de lecture, dans l'ordre.
    pub fn books(&self, id: &str) -> Result<Vec<String>, Error> {
        let i = self.index(id)?;
        self.expect_kind(i, KIND_READING, "une liste de lecture")?;
        Ok(self.item(i)?.child_texts("BookIds"))
    }

    /// Remplace les livres d'une liste de lecture (doublons retirés).
    pub fn set_books(&mut self, id: &str, books: Vec<String>) -> Result<(), Error> {
        let i = self.index(id)?;
        self.expect_kind(i, KIND_READING, "une liste de lecture")?;
        let mut item = self.item(i)?;
        let (ind, nl) = (item.child_indent(), item.newline());
        item.set_child("BookIds", Some(guid_list("BookIds", &dedup(books), &ind, nl)));
        self.store(i, item);
        self.invalidate(id);
        Ok(())
    }

    /// Ajoute des livres à une liste de lecture, à la fin (`None`) ou à la position
    /// donnée ; les livres déjà présents sont ignorés. Renvoie le nombre ajouté.
    pub fn add_books(&mut self, id: &str, books: &[String], at: Option<usize>) -> Result<usize, Error> {
        let mut cur = self.books(id)?;
        let have: HashSet<String> = cur.iter().cloned().collect();
        let new: Vec<String> = dedup(books.to_vec()).into_iter().filter(|b| !have.contains(b)).collect();
        let n = new.len();
        if n > 0 {
            let at = at.unwrap_or(cur.len()).min(cur.len());
            cur.splice(at..at, new);
            self.set_books(id, cur)?;
        }
        Ok(n)
    }

    /// Retire des livres d'une liste de lecture ; renvoie le nombre retiré.
    pub fn remove_books(&mut self, id: &str, books: &[String]) -> Result<usize, Error> {
        let cur = self.books(id)?;
        let gone: HashSet<String> = books.iter().map(|b| b.trim().to_ascii_lowercase()).collect();
        let kept: Vec<String> = cur.iter().filter(|b| !gone.contains(*b)).cloned().collect();
        let n = cur.len() - kept.len();
        if n > 0 {
            self.set_books(id, kept)?;
        }
        Ok(n)
    }

    /// Réordonne : déplace `books` (dans leur ordre actuel) à la position `to` de la liste
    /// obtenue sans eux.
    pub fn move_books(&mut self, id: &str, books: &[String], to: usize) -> Result<(), Error> {
        let cur = self.books(id)?;
        let moving: HashSet<String> = books.iter().map(|b| b.trim().to_ascii_lowercase()).collect();
        let (moved, mut rest): (Vec<String>, Vec<String>) = cur.iter().cloned().partition(|b| moving.contains(b));
        let to = to.min(rest.len());
        rest.splice(to..to, moved);
        if rest != cur {
            self.set_books(id, rest)?;
        }
        Ok(())
    }

    /// Transforme une liste intelligente en liste de lecture contenant `books` (son
    /// contenu, calculé par l'appelant) ; identifiant, nom, description et affichage gardés.
    pub fn convert_to_reading_list(&mut self, id: &str, books: Vec<String>) -> Result<(), Error> {
        let i = self.index(id)?;
        self.expect_kind(i, KIND_SMART, "une liste intelligente")?;
        let mut item = self.item(i)?;
        item.retype(
            KIND_READING,
            &["MatcherMode"],
            &[
                "Matchers",
                "Limit",
                "LimitType",
                "LimitValue",
                "LimitSelectionType",
                "LimitRandomSeed",
                "BaseListId",
                "NotInBaseList",
                "FilteredIds",
                "ShowFiltered",
            ],
        );
        let (ind, nl) = (item.child_indent(), item.newline());
        item.set_child("BookIds", Some(guid_list("BookIds", &dedup(books), &ind, nl)));
        self.store(i, item);
        self.rows[i].kind = KIND_READING.into();
        self.invalidate(id);
        Ok(())
    }

    /// Remplace les critères d'une liste intelligente.
    pub fn set_query(&mut self, id: &str, query: &SmartQuery) -> Result<(), Error> {
        let i = self.index(id)?;
        self.expect_kind(i, KIND_SMART, "une liste intelligente")?;
        if let Some(b) = query.base_list.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            let j = self.index(b)?;
            if self.depends_on(b, id) {
                return Err(err(format!(
                    "« {} » ne peut pas partir de « {} » : cette liste dépend d'elle",
                    self.rows[i].name, self.rows[j].name
                )));
            }
        }
        let mut item = self.item(i)?;
        item.set_query(query);
        self.store(i, item);
        self.invalidate(id);
        Ok(())
    }

    /// Mode de combinaison des sous-listes d'un dossier.
    pub fn set_combine(&mut self, id: &str, combine: Combine) -> Result<(), Error> {
        let i = self.index(id)?;
        self.expect_kind(i, KIND_FOLDER, "un dossier")?;
        let mut item = self.item(i)?;
        item.set_attr("CombineMode", combine_name(combine));
        self.store(i, item);
        self.invalidate(id);
        Ok(())
    }
}

/// Nom de liste : sans espaces autour ni caractère interdit en XML 1.0.
fn clean_name(name: &str) -> Result<String, Error> {
    let safe = xml::xml_safe(name);
    let n = safe.trim();
    if n.is_empty() {
        Err(err("le nom de la liste ne peut pas être vide"))
    } else {
        Ok(n.to_string())
    }
}

/// Identifiants en minuscules, sans doublon ni vide, ordre gardé.
fn dedup(ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.into_iter().map(|b| b.trim().to_ascii_lowercase()).filter(|b| !b.is_empty() && seen.insert(b.clone())).collect()
}
