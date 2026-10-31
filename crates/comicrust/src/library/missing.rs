//! Livres manquants (v0.10.2) : fiche dans la bibliothèque, fichier introuvable.
//!
//! - **Repérage** : base ComicRust, colonne `missing` (analyse des dossiers) ; ComicRack,
//!   élément `<Missing>` de la fiche (tenu par ComicRack quand il relit le fichier), complété
//!   par une **vérification en arrière-plan** après le chargement
//!   (`comicrust_core::missing::check_files` : un fil, par dossier, jamais sur le fil de
//!   l'interface ; NAS injoignable ou disparition massive : rien de marqué, et c'est dit).
//!   Le résultat de cette vérification est gardé en mémoire (`overlay`), jamais écrit dans
//!   la base de ComicRack (ComicRack recalcule `Missing` lui-même ; son `CopyFrom` ne le
//!   recopie pas) ; il cède dès que la fiche change dans la base (`Missing` ou compteur de
//!   la ligne : l'information de la base est alors la plus récente). Une ouverture qui
//!   échoue sur « Fichier introuvable » marque aussi le livre (et, base ComicRust,
//!   enregistre `missing`). Base ComicRust : `missing` n'est écrit que si la fiche a
//!   encore le chemin vérifié (`Store::apply_scan_seen`) — une fiche déplacée pendant une
//!   longue vérification n'est jamais marquée manquante.
//! - **Retrouver le fichier** : fiche mise à jour comme pour un déplacement trouvé par
//!   l'analyse (chemin, taille, dates ; lecture, note et infos gardées) : base ComicRust
//!   (`Store::apply_scan`, colonnes ciblées) ou ComicRack en écriture (`BookEdit::file` +
//!   `Missing` retiré) ; ComicRack en lecture seule ou `ComicDb.xml` : refus.
//! - **Retirer de la bibliothèque** : la **fiche seulement, jamais le fichier**. Base
//!   ComicRust : `Store::delete` ; ComicRack MySQL en écriture : `db::delete_books`
//!   (comme `ComicStorageBaseSql.Delete`). Les suppressions sont signalées à la
//!   surveillance sous un verrou qu'elle prend aussi pour interroger la base
//!   (`watch::DeletedSink`), pour qu'elle ne les prenne pas pour des suppressions
//!   extérieures. Une suppression MySQL partielle rend les fiches réellement supprimées
//!   (retirées de l'affichage) **et** l'erreur.
//!
//! Listes de lecture : comme ComicRack (`ComicListLibraryBrowser.RemoveBookHandler` appelle
//! seulement `library.Remove(book)`), elles ne sont pas réécrites : le livre n'y apparaît
//! plus (identifiant absent de la bibliothèque), et ComicRack purge l'identifiant au calcul
//! suivant de la liste (`ComicIdListItem.OnGetBooks`). ComicRack peut en plus mettre le
//! chemin sur une « liste noire » (option « Ne pas réajouter les fichiers supprimés »,
//! désactivée par défaut) : pas reproduit, un fichier encore présent dans un dossier
//! surveillé réapparaît donc à l'analyse suivante (sous une nouvelle fiche).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use comicrust_core::db::{BookEdit, ComicDatabase, DbConfig, WriteReport};
use comicrust_core::library::LibraryBook;
use comicrust_core::missing::{
    name_key, CheckItem, CheckReport, KnownFiles, MatchKind, Progress, SearchReport, Wanted,
};
use comicrust_core::scanner::path_key;
use comicrust_core::store::{Store, StoreConfig};
use eframe::egui;

use super::watch::{DeletedSink, ListsSource, WrittenSink};
use super::LibraryState;

/// Le livre est-il manquant ? (Comme `ComicBookIsMissingMatcher` : livre avec fichier.)
pub fn is_missing(b: &LibraryBook) -> bool {
    b.missing == Some(true) && b.file.as_deref().is_some_and(|f| !f.is_empty())
}

/// Où modifier les fiches (relocalisation, suppression).
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// Base de ComicRust.
    Store(StoreConfig),
    /// Base MySQL de ComicRack, en écriture.
    ComicRack(DbConfig),
}

/// Vérification en arrière-plan de l'existence des fichiers.
pub struct CheckJob {
    pub progress: Arc<Progress>,
    pub cancel: Arc<AtomicBool>,
    /// Chemin de la fiche de chaque livre vérifié (le résultat ne vaut que pour lui).
    files: HashMap<String, String>,
    rx: Receiver<CheckReport>,
}

impl CheckJob {
    /// `items` : livres à vérifier (voir [`check_items`]) ; `folders` : où lire les dossiers
    /// surveillés (garde-fou « disparition massive »), lus dans le fil.
    pub fn start(items: Vec<CheckEntry>, folders: Option<ListsSource>, ctx: &egui::Context) -> Self {
        let progress = Arc::new(Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let files = items.iter().map(|e| (e.item.id.to_ascii_lowercase(), e.file.clone())).collect();
        let check: Vec<CheckItem> = items.into_iter().map(|e| e.item).collect();
        let (tx, rx) = mpsc::channel();
        let (p, c, ctx) = (progress.clone(), cancel.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("comicrust-manquants".into())
            .spawn(move || {
                let folders = folders.map(|f| f.watch_folders()).unwrap_or_default();
                let _ = tx.send(comicrust_core::missing::check_files(&check, &folders, &p, &c));
                ctx.request_repaint();
            })
            .expect("fil de vérification des fichiers");
        Self { progress, cancel, files, rx }
    }

    pub fn poll(&self) -> Option<CheckReport> {
        self.rx.try_recv().ok()
    }
}

/// Vérification terminée (voir [`LibraryState::poll_check`]).
pub struct CheckDone {
    pub report: CheckReport,
    /// Chemin de la fiche de chaque livre vérifié (identifiant en minuscules) : à passer à
    /// [`persist_marks`].
    pub files: HashMap<String, String>,
}

/// Livre à vérifier : chemin de la fiche (le résultat ne vaut que pour lui) et élément de
/// la vérification (fichier réel).
#[derive(Debug, Clone)]
pub struct CheckEntry {
    pub file: String,
    pub item: CheckItem,
}

/// Livres à vérifier : ceux qui ont un fichier, avec leur fichier réel (CBZ converti par
/// ComicRust si l'original a été rangé : `converted`, réglages). Seule façon de calculer le
/// fichier réel, pour la vérification comme pour la fenêtre « Retirer de la bibliothèque ».
pub fn check_items<'a>(
    books: impl IntoIterator<Item = &'a LibraryBook>,
    converted: &HashMap<PathBuf, PathBuf>,
) -> Vec<CheckEntry> {
    books
        .into_iter()
        .filter_map(|b| {
            let f = b.file.clone().filter(|f| !f.is_empty())?;
            let path = comicrust_core::missing::real_file(&f, converted);
            Some(CheckEntry {
                item: CheckItem { id: b.id.clone(), path, was_missing: b.missing == Some(true) },
                file: f,
            })
        })
        .collect()
}

/// Recherche des fichiers manquants dans un dossier.
pub struct SearchJob {
    pub progress: Arc<Progress>,
    pub cancel: Arc<AtomicBool>,
    pub dir: PathBuf,
    rx: Receiver<SearchReport>,
}

impl SearchJob {
    pub fn start(dir: PathBuf, wanted: Vec<Wanted>, known: KnownFiles, ctx: &egui::Context) -> Self {
        let progress = Arc::new(Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c, d, ctx) = (progress.clone(), cancel.clone(), dir.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("comicrust-recherche".into())
            .spawn(move || {
                let _ = tx.send(comicrust_core::missing::search_folder(&d, &wanted, &known, &p, &c));
                ctx.request_repaint();
            })
            .expect("fil de recherche des fichiers");
        Self { progress, cancel, dir, rx }
    }

    pub fn poll(&self) -> Option<SearchReport> {
        self.rx.try_recv().ok()
    }
}

/// Fichier proposé pour un livre (fenêtre de confirmation).
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub id: String,
    pub caption: String,
    /// Chemin actuel de la fiche.
    pub old: String,
    pub new: PathBuf,
    pub kind: MatchKind,
    /// Autres fichiers de même nom trouvés.
    pub others: usize,
    /// Livre de la bibliothèque qui a déjà ce fichier (doublon après une analyse).
    pub duplicate: Option<String>,
    pub checked: bool,
}

/// Résultat d'une opération sur les fiches, faite dans un fil.
#[derive(Debug)]
pub enum OpResult {
    /// Base ComicRust : fiches réécrites (relues), et leur compteur.
    Store { books: Vec<LibraryBook>, counter: i64 },
    /// Base ComicRack : bilan d'écriture.
    ComicRack(WriteReport),
    /// Fiches supprimées ; `absent` : déjà absentes de la base ; `error` : suppression
    /// partielle (les fiches de `ids` sont bien supprimées, les autres non).
    Removed { ids: Vec<String>, absent: Vec<String>, error: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Relocate,
    Remove,
    /// Enregistrement de `missing` (base ComicRust).
    Mark,
}

pub struct OpDone {
    pub kind: OpKind,
    pub result: Result<OpResult, String>,
}

/// Résultat mémorisé d'une vérification ou d'une ouverture ratée.
#[derive(Debug, Clone, PartialEq)]
struct Mark {
    /// Chemin de la fiche vérifié (le résultat ne vaut que pour lui).
    file: String,
    missing: bool,
    /// `missing` de la fiche **dans la base** quand le résultat a été mémorisé.
    base: Option<bool>,
    /// Compteur de la ligne (`update_counter`) à ce moment-là, s'il est connu.
    counter: Option<i64>,
}

impl Mark {
    /// Valeur affichée : manquant, ou présent (`Some(false)` seulement si la base disait
    /// manquant ; sinon la valeur de la base).
    fn shown(&self) -> Option<bool> {
        match (self.missing, self.base) {
            (true, _) => Some(true),
            (false, Some(true)) => Some(false),
            (false, other) => other,
        }
    }
}

/// État des livres manquants (gardé dans [`LibraryState`]).
#[derive(Default)]
pub struct MissingState {
    /// Résultats de la vérification et des ouvertures ratées, par identifiant
    /// (minuscules). Réappliqués quand la base est relue, tant que la fiche n'y a pas
    /// changé (voir [`LibraryState::reapply_missing`]).
    overlay: HashMap<String, Mark>,
    pub check: Option<CheckJob>,
    /// Bilan de la dernière vérification.
    pub last_check: Option<CheckReport>,
    pub search: Option<SearchJob>,
    ops: Vec<Receiver<OpDone>>,
}

impl MissingState {
    /// Opération en cours (relocalisation, suppression) ?
    pub fn busy(&self) -> bool {
        !self.ops.is_empty()
    }

    pub fn push_op(&mut self, rx: Receiver<OpDone>) {
        self.ops.push(rx);
    }

    /// Opérations terminées.
    pub fn poll_ops(&mut self) -> Vec<OpDone> {
        let mut out = Vec::new();
        self.ops.retain(|rx| match rx.try_recv() {
            Ok(d) => {
                out.push(d);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        out
    }
}

impl LibraryState {
    /// Nombre de livres manquants de la bibliothèque.
    pub fn missing_count(&self) -> usize {
        self.database().map_or(0, |db| db.books.iter().filter(|b| is_missing(b)).count())
    }

    /// Applique `overlay` à la base en mémoire (après une relecture de la base). Renvoie
    /// `true` si un livre a changé.
    ///
    /// Un résultat mémorisé est oublié dès que la fiche a changé dans la base depuis
    /// (`Missing` différent de celui d'alors, ou compteur de la ligne différent) : la base
    /// est alors l'information la plus récente — par exemple ComicRack qui efface
    /// `<Missing>` parce que le fichier est revenu. Fiche relocalisée entre-temps
    /// (autre chemin) : résultat périmé, ignoré.
    pub(super) fn reapply_missing(&mut self) -> bool {
        if self.missing.overlay.is_empty() {
            return false;
        }
        let Some(db) = self.database() else { return false };
        let mut todo: Vec<(usize, Option<bool>)> = Vec::new();
        let mut stale: Vec<String> = Vec::new();
        for (id, m) in &self.missing.overlay {
            let Some(i) = db.index_of(id) else { continue };
            let b = &db.books[i];
            if b.file.as_deref() != Some(m.file.as_str()) {
                continue; // fiche relocalisée entre-temps : résultat périmé
            }
            let want = m.shown();
            if b.missing == want {
                continue;
            }
            if b.missing == m.base && db.rows.get(id).copied() == m.counter {
                todo.push((i, want)); // fiche relue sans changement
            } else {
                stale.push(id.clone()); // fiche modifiée dans la base depuis : elle a raison
            }
        }
        for id in &stale {
            self.missing.overlay.remove(id);
        }
        if todo.is_empty() {
            return false;
        }
        if let super::Status::Loaded(db) = &mut self.status {
            let db = Arc::make_mut(db);
            for (i, m) in todo {
                db.books[i].missing = m;
            }
        }
        true
    }

    /// Mémorise un résultat pour la fiche `key` (identifiant en minuscules) au chemin
    /// `file`, avec l'état de la base à ce moment-là.
    fn remember(&mut self, db: &ComicDatabase, key: String, file: String, missing: bool) {
        let Some(b) = db.get(&key) else { return };
        // La fiche affiche peut-être déjà un résultat précédent : l'état de la base est
        // alors celui de ce résultat.
        let base = match self.missing.overlay.get(&key) {
            Some(old) if old.file == file && b.missing == old.shown() => old.base,
            _ => b.missing,
        };
        let counter = db.rows.get(&key).copied();
        self.missing.overlay.insert(key, Mark { file, missing, base, counter });
    }

    /// Marque des livres manquants (ou présents) en mémoire, pour leur fichier actuel.
    #[cfg(test)]
    pub fn set_missing(&mut self, ids: &[String], missing: bool) {
        let Some(db) = self.database() else { return };
        let files: Vec<(String, String)> =
            ids.iter().filter_map(|id| Some((id.clone(), db.get(id)?.file.clone()?))).collect();
        self.set_missing_files(&files, missing);
    }

    /// Marque des livres manquants (ou présents) en mémoire, seulement si leur fiche a
    /// encore le chemin vérifié (`files` : identifiant, chemin de la fiche vérifié).
    pub fn set_missing_files(&mut self, files: &[(String, String)], missing: bool) {
        let Some(db) = self.database() else { return };
        for (id, f) in files {
            self.remember(&db, id.to_ascii_lowercase(), f.clone(), missing);
        }
        if self.reapply_missing() {
            self.generation += 1;
            self.view_key = None;
        }
    }

    /// Résultat d'une vérification : livres marqués manquants ou présents (seulement ceux
    /// dont la fiche a encore le chemin vérifié).
    pub fn apply_check(&mut self, files: &HashMap<String, String>, report: &CheckReport) {
        let Some(db) = self.database() else { return };
        for (ids, missing) in [(&report.missing, true), (&report.present, false)] {
            for id in ids {
                let key = id.to_ascii_lowercase();
                if let Some(f) = files.get(&key) {
                    self.remember(&db, key, f.clone(), missing);
                }
            }
        }
        if self.reapply_missing() {
            self.generation += 1;
            self.view_key = None;
        }
    }

    /// Fiches retirées de la bibliothèque par ComicRust (déjà supprimées de la base).
    pub fn remove_books(&mut self, ids: &[String]) -> usize {
        let super::Status::Loaded(db) = &mut self.status else { return 0 };
        let n = Arc::make_mut(db).remove(ids);
        let gone: HashSet<String> = ids.iter().map(|i| i.to_ascii_lowercase()).collect();
        for id in &gone {
            self.missing.overlay.remove(id);
        }
        self.selected_books.retain(|b| !gone.contains(&b.to_ascii_lowercase()));
        if self.selected_book.as_ref().is_some_and(|b| gone.contains(&b.to_ascii_lowercase())) {
            self.selected_book = None;
        }
        if self.cursor.as_ref().is_some_and(|b| gone.contains(&b.to_ascii_lowercase())) {
            self.cursor = None;
        }
        self.generation += 1;
        self.view_key = None;
        n
    }

    /// Lance la vérification des fichiers ; `converted` : CBZ convertis (voir
    /// [`check_items`]) ; `folders` : où lire les dossiers surveillés. Sans effet si une
    /// vérification tourne déjà.
    pub fn start_check(
        &mut self,
        converted: &HashMap<PathBuf, PathBuf>,
        folders: Option<ListsSource>,
        ctx: &egui::Context,
    ) {
        if self.missing.check.is_some() {
            return;
        }
        let Some(db) = self.database() else { return };
        let items = check_items(&db.books, converted);
        self.missing.check = Some(CheckJob::start(items, folders, ctx));
    }

    /// Vérification terminée : appliquée ; renvoie le bilan et les chemins vérifiés.
    pub fn poll_check(&mut self) -> Option<CheckDone> {
        let job = self.missing.check.as_ref()?;
        let report = job.poll()?;
        let job = self.missing.check.take()?;
        if !report.cancelled {
            self.apply_check(&job.files, &report);
        }
        self.missing.last_check = Some(report.clone());
        Some(CheckDone { report, files: job.files })
    }

    /// Livres à retrouver (fichier et taille de la fiche).
    pub fn wanted(&self, ids: &[String]) -> Vec<Wanted> {
        let Some(db) = self.database() else { return Vec::new() };
        ids.iter()
            .filter_map(|id| db.get(id))
            .filter_map(|b| Some(Wanted { id: b.id.clone(), file: b.file.clone()?, size: b.file_size }))
            .collect()
    }

    /// Fichiers déjà dans la bibliothèque, jamais proposés par une recherche (même sous un
    /// autre chemin du même partage).
    pub fn known_files(&self) -> KnownFiles {
        let Some(db) = self.database() else { return KnownFiles::new([]) };
        KnownFiles::new(db.books.iter().filter_map(|b| b.file.as_deref()))
    }

    /// Propositions de la fenêtre de confirmation, d'après le résultat d'une recherche.
    pub fn proposals(&self, report: &SearchReport) -> Vec<Proposal> {
        let Some(db) = self.database() else { return Vec::new() };
        report
            .found
            .iter()
            .filter_map(|c| {
                let b = db.get(&c.id)?;
                Some(Proposal {
                    id: b.id.clone(),
                    caption: b.caption(),
                    old: b.file.clone().unwrap_or_default(),
                    new: c.path.clone(),
                    kind: c.kind,
                    others: c.others,
                    duplicate: None,
                    checked: c.sure(),
                })
            })
            .collect()
    }
}

/// Proposition pour un fichier choisi à la main. Lit le disque (même fichier physique
/// sous un autre chemin, pour les fiches de même nom de fichier : jusqu'à des centaines
/// sur un NAS) : jamais sur le fil de l'interface (voir [`check_chosen`]).
pub fn chosen_proposal(db: &ComicDatabase, id: &str, new: PathBuf, size: u64) -> Option<Proposal> {
    let b = db.get(id)?;
    let old = b.file.clone().unwrap_or_default();
    let text = new.to_string_lossy();
    let (key, name) = (path_key(&text), name_key(&text));
    // Même chemin, ou même fichier physique sous un autre chemin (`Z:\` ↔ `\\SERVEUR\…`,
    // lien symbolique) : seulement pour les fiches de même nom de fichier.
    let duplicate = db
        .books
        .iter()
        .find(|o| {
            o.id != b.id
                && o.file.as_deref().is_some_and(|f| {
                    path_key(f) == key
                        || (name_key(f) == name
                            && comicrust_core::missing::same_physical_file(std::path::Path::new(f), &new))
                })
        })
        .map(|o| o.caption());
    let kind = comicrust_core::missing::chosen_kind(&old, b.file_size, &new, size);
    Some(Proposal {
        id: b.id.clone(),
        caption: b.caption(),
        old,
        new,
        kind,
        others: 0,
        checked: duplicate.is_none(),
        duplicate,
    })
}

/// Examine dans un fil le fichier choisi pour le livre `id` (taille, doublon) ; rend la
/// proposition (`None` : livre disparu de la bibliothèque entre-temps).
pub fn check_chosen(
    db: Arc<ComicDatabase>,
    id: String,
    new: PathBuf,
    ctx: &egui::Context,
) -> Receiver<Option<Proposal>> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("comicrust-fichier-choisi".into())
        .spawn(move || {
            let size = std::fs::metadata(&new).map(|m| m.len()).unwrap_or(0);
            let _ = tx.send(chosen_proposal(&db, &id, new, size));
            ctx.request_repaint();
        })
        .expect("fil d'examen du fichier choisi");
    rx
}

/// Lance une relocalisation (fiches mises à jour dans un fil).
pub fn relocate(
    target: Target,
    books: Vec<(LibraryBook, PathBuf)>,
    written: Option<WrittenSink>,
    ctx: &egui::Context,
) -> Receiver<OpDone> {
    spawn(ctx, OpKind::Relocate, move || match target {
        Target::Store(cfg) => {
            let report = comicrust_core::missing::relocation_report(&books).map_err(|e| e.to_string())?;
            // Seulement les fiches qui ont encore le chemin affiché dans la fenêtre.
            let seen: HashMap<String, String> =
                books.iter().filter_map(|(b, _)| Some((b.id.to_ascii_lowercase(), b.file.clone()?))).collect();
            let mut st = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
            let w = st.apply_scan_seen(&report, Some(&seen)).map_err(|e| e.to_string())?;
            if let Some(s) = &written {
                s.lock().unwrap().extend(w.books.iter().map(|b| (b.id.to_ascii_lowercase(), w.counter)));
            }
            Ok(OpResult::Store { books: w.books, counter: w.counter })
        }
        Target::ComicRack(cfg) => {
            let mut edits = Vec::new();
            for (b, path) in &books {
                let f = super::comicrack_write::file_state(&path.to_string_lossy(), path, None)
                    .map_err(|e| format!("{} : {e}", path.display()))?;
                edits.push(BookEdit::new(&b.id).file(&f).remove("Missing"));
            }
            let report = comicrust_core::db::update_books(&cfg, &edits).map_err(|e| e.to_string())?;
            if let Some(s) = &written {
                s.lock().unwrap().extend(report.written.iter().map(|w| (w.id.clone(), w.record.counter)));
            }
            Ok(OpResult::ComicRack(report))
        }
    })
}

/// Au-delà de ce nombre de livres retirés d'un coup, la case de confirmation est exigée
/// même si la vérification des fichiers est sûre.
pub const REMOVE_CONFIRM_ABOVE: usize = 100;

/// Fenêtre « Retirer de la bibliothèque » : la case « je confirme » est-elle exigée ? Oui si
/// un volume est injoignable, une disparition massive bloquée, des livres non vérifiés, des
/// fichiers encore présents (fiches de livres non manquants), ou plus de
/// [`REMOVE_CONFIRM_ABOVE`] livres.
pub fn remove_needs_confirmation(n: usize, check: &CheckReport) -> bool {
    !check.is_conclusive() || !check.present.is_empty() || n > REMOVE_CONFIRM_ABOVE
}

/// Bouton « Retirer » actif ? Jamais avant la fin de la vérification ; ensuite, seulement si
/// la confirmation exigée est cochée.
pub fn remove_allowed(check: Option<&CheckReport>, n: usize, confirmed: bool) -> bool {
    check.is_some_and(|c| confirmed || !remove_needs_confirmation(n, c))
}

/// Bilan d'une suppression dans la base MySQL de ComicRack : les fiches réellement
/// supprimées sont toujours rendues (pour les retirer de l'affichage), avec l'erreur si
/// d'autres ne l'ont pas été.
pub fn removed_from_mysql(r: comicrust_core::db::DeleteReport) -> OpResult {
    let error = r.failed.first().map(|(id, e)| {
        format!("{} fiche(s) supprimée(s), {} en erreur (ex. {id} : {e})", r.deleted.len(), r.failed.len())
    });
    OpResult::Removed { ids: r.deleted, absent: r.missing, error }
}

/// Lance le retrait de fiches de la bibliothèque (jamais les fichiers). `deleted` : la
/// surveillance, prévenue pendant la suppression (voir [`DeletedSink`]).
pub fn remove(target: Target, ids: Vec<String>, deleted: Option<DeletedSink>, ctx: &egui::Context) -> Receiver<OpDone> {
    spawn(ctx, OpKind::Remove, move || match &deleted {
        Some(s) => s.deleting(ids.clone(), || remove_now(target, ids)),
        None => remove_now(target, ids),
    })
}

fn remove_now(target: Target, ids: Vec<String>) -> Result<OpResult, String> {
    {
        match target {
            Target::Store(cfg) => {
                let mut st = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
                let present: HashSet<String> =
                    st.get(&ids).map_err(|e| e.to_string())?.into_iter().map(|b| b.id.to_ascii_lowercase()).collect();
                st.delete(&ids).map_err(|e| e.to_string())?;
                let (ids, absent) = ids.into_iter().partition(|i| present.contains(&i.to_ascii_lowercase()));
                Ok(OpResult::Removed { ids, absent, error: None })
            }
            Target::ComicRack(cfg) => {
                let r = comicrust_core::db::delete_books(&cfg, &ids).map_err(|e| e.to_string())?;
                Ok(removed_from_mysql(r))
            }
        }
    }
}

/// Base ComicRust : enregistre `missing` (vrai pour `missing`, effacé pour `back`), pour
/// les seules fiches qui ont encore le chemin vérifié (`seen` : identifiant en minuscules →
/// chemin de la fiche au moment de la vérification ; un livre absent de `seen` n'est pas
/// écrit). Une fiche relocalisée ou déplacée pendant la vérification (script, autre poste)
/// n'est donc jamais marquée manquante à son nouveau chemin.
pub fn persist_marks(
    cfg: StoreConfig,
    missing: Vec<String>,
    back: Vec<String>,
    seen: HashMap<String, String>,
    written: Option<WrittenSink>,
    ctx: &egui::Context,
) -> Receiver<OpDone> {
    spawn(ctx, OpKind::Mark, move || {
        let known = |ids: Vec<String>| -> Vec<String> {
            ids.into_iter().filter(|id| seen.contains_key(&id.to_ascii_lowercase())).collect()
        };
        let report =
            comicrust_core::scanner::ScanReport { missing: known(missing), back: known(back), ..Default::default() };
        let mut st = Store::open_existing(&cfg).map_err(|e| e.to_string())?;
        let w = st.apply_scan_seen(&report, Some(&seen)).map_err(|e| e.to_string())?;
        if let Some(s) = &written {
            s.lock().unwrap().extend(w.books.iter().map(|b| (b.id.to_ascii_lowercase(), w.counter)));
        }
        Ok(OpResult::Store { books: w.books, counter: w.counter })
    })
}

fn spawn(
    ctx: &egui::Context,
    kind: OpKind,
    f: impl FnOnce() -> Result<OpResult, String> + Send + 'static,
) -> Receiver<OpDone> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("comicrust-fiches".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
                .unwrap_or_else(|_| Err("plantage pendant la mise à jour des fiches".into()));
            let _ = tx.send(OpDone { kind, result });
            ctx.request_repaint();
        })
        .expect("fil de mise à jour des fiches");
    rx
}

#[cfg(test)]
#[path = "missing_tests.rs"]
mod tests;
