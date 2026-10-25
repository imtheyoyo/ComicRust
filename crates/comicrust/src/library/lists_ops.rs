//! Modification des listes depuis l'interface (v0.9.0).
//!
//! Une opération ([`ListOp`]) est une fonction appliquée au [`ListsEditor`] du cœur. Elle
//! est faite **dans un fil** (jamais sur celui de l'interface) :
//!
//! 1. lecture des listes au moment même : base de ComicRust ([`Store::list_rows`]) ou
//!    `ComicDb.xml` de ComicRack ([`ComicDbLists::read`]) ;
//! 2. application de l'opération ;
//! 3. enregistrement ([`Store::save_list_rows`] : refus si un autre poste a modifié les
//!    listes entre-temps ; [`ComicDbLists::save`] : refus si ComicRack est lancé ou si le
//!    fichier a changé, sauvegarde `.bak`) ;
//! 4. listes relues renvoyées à l'interface, qui les affiche aussitôt (sans attendre la
//!    surveillance, qui les relira une fois et les trouvera identiques).
//!
//! Une seule opération à la fois ([`ListsOps::busy`]).
//!
//! Le module contient aussi les calculs sur l'arbre des listes affiché (sous-listes,
//! listes de départ possibles sans boucle, positions de déplacement), testés à part.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use comicrust_core::comic_lists::{ComicList, ListKind};
use comicrust_core::lists_edit::{ComicDbLists, ListsEditor};
use comicrust_core::store::{Store, StoreConfig};
use comicrust_core::Error;
use eframe::egui;

/// Intervalle minimal entre deux recherches du processus ComicRack.
const PROBE_INTERVAL: Duration = Duration::from_secs(3);

/// Où sont enregistrées les listes.
#[derive(Debug, Clone, PartialEq)]
pub enum ListsTarget {
    /// Base de ComicRust (mode autonome).
    Store(StoreConfig),
    /// `ComicDb.xml` de ComicRack (mode « avec ComicRack », ComicRack fermé).
    ComicDb(PathBuf),
}

/// Ce que l'interface fait après une opération réussie.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// Liste à afficher (`Some(None)` : toute la bibliothèque).
    pub select: Option<Option<String>>,
    /// Liste à renommer aussitôt (liste ou dossier qui vient d'être créé).
    pub rename: Option<String>,
    /// Bilan à montrer sous le panneau des listes.
    pub info: Option<String>,
    /// Identifiant de la liste créée.
    pub created: Option<String>,
}

impl Outcome {
    pub fn info(msg: impl Into<String>) -> Self {
        Outcome { info: Some(msg.into()), ..Default::default() }
    }
}

/// Qui a demandé l'opération (pour savoir à qui rendre le résultat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpTag {
    Panel,
    Editor,
}

pub type OpFn = Box<dyn FnOnce(&mut ListsEditor) -> Result<Outcome, Error> + Send>;

/// Opération sur les listes.
pub struct ListOp {
    /// Libellé (« Suppression de X »), affiché pendant l'enregistrement.
    pub what: String,
    pub run: OpFn,
}

impl ListOp {
    pub fn new(
        what: impl Into<String>,
        run: impl FnOnce(&mut ListsEditor) -> Result<Outcome, Error> + Send + 'static,
    ) -> Self {
        ListOp { what: what.into(), run: Box::new(run) }
    }
}

/// Résultat d'une opération enregistrée.
#[derive(Debug, Clone)]
pub struct Applied {
    /// Toutes les listes, relues après l'enregistrement.
    pub lists: Vec<ComicList>,
    pub outcome: Outcome,
    /// `ComicDb.xml` modifié (`None` : base de ComicRust).
    pub path: Option<PathBuf>,
    /// Sauvegarde du `ComicDb.xml` d'origine.
    pub backup: Option<PathBuf>,
}

/// Message d'erreur pour l'utilisateur (les refus du cœur sont montrés tels quels).
pub fn error_text(e: &Error) -> String {
    match e {
        Error::Conflict(m) | Error::Database(m) => m.clone(),
        other => other.to_string(),
    }
}

/// Lit, modifie et enregistre les listes (dans le fil de l'opération).
pub fn run_op(target: &ListsTarget, op: OpFn) -> Result<Applied, Error> {
    match target {
        ListsTarget::Store(cfg) => {
            let mut store = Store::open_existing(cfg)?;
            let snap = store.list_rows()?;
            let mut ed = ListsEditor::new(snap.rows);
            let outcome = op(&mut ed)?;
            let lists = ed.lists()?;
            store.save_list_rows(ed.rows(), snap.counter)?;
            Ok(Applied { lists, outcome, path: None, backup: None })
        }
        ListsTarget::ComicDb(path) => {
            let file = ComicDbLists::read(path)?;
            let mut ed = ListsEditor::new(file.rows.clone());
            let outcome = op(&mut ed)?;
            let lists = ed.lists()?;
            let report = file.save(ed.rows())?;
            Ok(Applied { lists, outcome, path: Some(path.clone()), backup: Some(report.backup) })
        }
    }
}

struct Job {
    tag: OpTag,
    what: String,
    rx: Receiver<Result<Applied, String>>,
}

/// Recherche du processus ComicRack dans un fil, au plus toutes les [`PROBE_INTERVAL`].
#[derive(Default)]
struct Probe {
    running: Arc<AtomicBool>,
    checking: Arc<AtomicBool>,
    last: Option<Instant>,
}

impl Probe {
    fn poll(&mut self, ctx: &egui::Context) -> bool {
        let due = self.last.is_none_or(|t| t.elapsed() >= PROBE_INTERVAL);
        if due && !self.checking.swap(true, Ordering::AcqRel) {
            self.last = Some(Instant::now());
            let (running, checking, ctx) = (self.running.clone(), self.checking.clone(), ctx.clone());
            let spawned = std::thread::Builder::new().name("comicrust-comicrack".into()).spawn(move || {
                // Recette hors Windows : variable d'environnement qui simule ComicRack lancé.
                let now = std::env::var_os("COMICRUST_SIMULER_COMICRACK").is_some()
                    || comicrust_core::lists_edit::comicrack_running();
                if running.swap(now, Ordering::AcqRel) != now {
                    ctx.request_repaint();
                }
                checking.store(false, Ordering::Release);
            });
            if spawned.is_err() {
                self.checking.store(false, Ordering::Release);
            }
        }
        // Nouvelle vérification dans quelques secondes, même sans mouvement de souris.
        ctx.request_repaint_after(PROBE_INTERVAL);
        self.running.load(Ordering::Acquire)
    }
}

/// Opérations sur les listes : une à la fois, dans un fil.
#[derive(Default)]
pub struct ListsOps {
    job: Option<Job>,
    probe: Probe,
}

impl ListsOps {
    /// Enregistrement en cours ?
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Libellé de l'opération en cours.
    pub fn current(&self) -> Option<&str> {
        self.job.as_ref().map(|j| j.what.as_str())
    }

    /// ComicRack est-il lancé (dernière vérification faite en arrière-plan) ?
    pub fn comicrack_running(&mut self, ctx: &egui::Context) -> bool {
        self.probe.poll(ctx)
    }

    /// Lance une opération ; refusée (`false`) si une autre est en cours.
    pub fn start(&mut self, target: ListsTarget, tag: OpTag, op: ListOp, ctx: &egui::Context) -> bool {
        if self.job.is_some() {
            return false;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let run = op.run;
        let spawned = std::thread::Builder::new().name("comicrust-listes-modif".into()).spawn(move || {
            let res = run_op(&target, run).map_err(|e| error_text(&e));
            let _ = tx.send(res);
            ctx.request_repaint();
        });
        if spawned.is_err() {
            return false;
        }
        self.job = Some(Job { tag, what: op.what, rx });
        true
    }

    /// Résultat de l'opération en cours, une fois terminée.
    pub fn poll(&mut self) -> Option<(OpTag, String, Result<Applied, String>)> {
        let job = self.job.as_ref()?;
        let res = match job.rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err("l'enregistrement des listes s'est interrompu".into()),
        };
        let job = self.job.take()?;
        Some((job.tag, job.what, res))
    }
}

// ---------------------------------------------------------------------------
// Calculs sur l'arbre des listes affiché
// ---------------------------------------------------------------------------

/// Toutes les listes de l'arbre, à plat (parcours en profondeur), avec leur profondeur.
pub fn flatten(items: &[ComicList]) -> Vec<(&ComicList, usize)> {
    fn walk<'a>(l: &'a [ComicList], depth: usize, out: &mut Vec<(&'a ComicList, usize)>) {
        for x in l {
            out.push((x, depth));
            if let ListKind::Folder { children, .. } = &x.kind {
                walk(children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(items, 0, &mut out);
    out
}

/// Identifiants de la liste et de toutes ses sous-listes (en minuscules).
pub fn subtree(items: &[ComicList], id: &str) -> Vec<String> {
    match ComicList::find(items, id) {
        Some(l) => flatten(std::slice::from_ref(l)).into_iter().map(|(x, _)| x.id.to_ascii_lowercase()).collect(),
        None => Vec::new(),
    }
}

/// Dossier parent (`None` : premier niveau) et rang de la liste parmi ses voisines.
pub fn locate(items: &[ComicList], id: &str) -> Option<(Option<String>, usize)> {
    fn walk(l: &[ComicList], parent: Option<&str>, id: &str) -> Option<(Option<String>, usize)> {
        for (i, x) in l.iter().enumerate() {
            if x.id.eq_ignore_ascii_case(id) {
                return Some((parent.map(str::to_string), i));
            }
            if let ListKind::Folder { children, .. } = &x.kind {
                if let Some(r) = walk(children, Some(&x.id), id) {
                    return Some(r);
                }
            }
        }
        None
    }
    walk(items, None, id)
}

/// Listes d'un dossier (`None` : premier niveau).
pub fn siblings<'a>(items: &'a [ComicList], parent: Option<&str>) -> &'a [ComicList] {
    match parent {
        None => items,
        Some(p) => match ComicList::find(items, p).map(|l| &l.kind) {
            Some(ListKind::Folder { children, .. }) => children,
            _ => &[],
        },
    }
}

/// Dossiers parents de la liste (du plus lointain au plus proche).
pub fn ancestors(items: &[ComicList], id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = locate(items, id).and_then(|(p, _)| p);
    while let Some(p) = cur {
        if out.contains(&p) {
            break;
        }
        cur = locate(items, &p).and_then(|(pp, _)| pp);
        out.push(p);
    }
    out.reverse();
    out
}

fn base_of(l: &ComicList) -> Option<String> {
    match &l.kind {
        ListKind::Smart { query: Some(q), .. } => {
            q.base_list.as_deref().map(|b| b.trim().to_ascii_lowercase()).filter(|b| !b.is_empty())
        }
        _ => None,
    }
}

/// Le calcul de la liste `a` a-t-il besoin de la liste `b` (`a` = `b`, sous-liste d'un
/// dossier, liste de départ d'une liste intelligente, de proche en proche) ? Même règle
/// que `ListsEditor::depends_on` du cœur, sur l'arbre affiché.
pub fn depends_on(items: &[ComicList], a: &str, b: &str) -> bool {
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
        let Some(l) = ComicList::find(items, &cur) else { continue };
        if let ListKind::Folder { children, .. } = &l.kind {
            todo.extend(children.iter().map(|c| c.id.to_ascii_lowercase()));
        }
        if let Some(base) = base_of(l) {
            todo.push(base);
        }
    }
    false
}

/// Listes intelligentes (noms) qui partent de la liste `id` ou d'une de ses sous-listes,
/// hors de cette branche : elles partiront de toute la bibliothèque si on la supprime.
pub fn base_dependents(items: &[ComicList], id: &str) -> Vec<String> {
    let branch: HashSet<String> = subtree(items, id).into_iter().collect();
    flatten(items)
        .into_iter()
        .filter(|(l, _)| !branch.contains(&l.id.to_ascii_lowercase()))
        .filter(|(l, _)| base_of(l).is_some_and(|b| branch.contains(&b)))
        .map(|(l, _)| l.name.clone())
        .collect()
}

/// Liste proposée comme liste de départ d'une liste intelligente.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseChoice {
    pub id: String,
    pub name: String,
    pub depth: usize,
    pub icon: &'static str,
}

pub fn icon(l: &ComicList) -> &'static str {
    match &l.kind {
        ListKind::Library => "📚",
        ListKind::Folder { .. } => "📁",
        ListKind::Ids(_) => "📄",
        ListKind::Smart { .. } => "🔍",
    }
}

/// Listes de départ possibles pour la liste intelligente `editing` (`None` : liste pas
/// encore créée) : toutes, sauf elle-même et celles qui dépendent d'elle (boucle). La
/// « Bibliothèque » n'y figure pas (c'est le choix « Toute la bibliothèque »).
pub fn base_candidates(items: &[ComicList], editing: Option<&str>) -> Vec<BaseChoice> {
    flatten(items)
        .into_iter()
        .filter(|(l, _)| l.kind != ListKind::Library)
        .filter(|(l, _)| editing.is_none_or(|e| !depends_on(items, &l.id, e)))
        .map(|(l, depth)| BaseChoice { id: l.id.to_ascii_lowercase(), name: l.name.clone(), depth, icon: icon(l) })
        .collect()
}

/// Dossiers où la liste `id` peut être déplacée (pas elle-même ni ses sous-dossiers) :
/// (identifiant, nom, profondeur).
pub fn move_targets(items: &[ComicList], id: &str) -> Vec<(String, String, usize)> {
    let branch: HashSet<String> = subtree(items, id).into_iter().collect();
    flatten(items)
        .into_iter()
        .filter(|(l, _)| matches!(l.kind, ListKind::Folder { .. }))
        .filter(|(l, _)| !branch.contains(&l.id.to_ascii_lowercase()))
        .map(|(l, d)| (l.id.clone(), l.name.clone(), d))
        .collect()
}

/// Où déposer une liste glissée sur une autre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPlace {
    Before,
    Into,
    After,
}

/// Dossier et rang (parmi les voisines, sans la liste déplacée) pour `ListsEditor::move_to`
/// quand `dragged` est déposée sur `target` ; `None` si c'est impossible (sur elle-même,
/// dans une de ses sous-listes, « dans » une liste qui n'est pas un dossier).
pub fn drop_position(
    items: &[ComicList],
    dragged: &str,
    target: &str,
    place: DropPlace,
) -> Option<(Option<String>, Option<usize>)> {
    if subtree(items, dragged).contains(&target.to_ascii_lowercase()) {
        return None;
    }
    if place == DropPlace::Into {
        let t = ComicList::find(items, target)?;
        return matches!(t.kind, ListKind::Folder { .. }).then(|| (Some(t.id.clone()), None));
    }
    let (parent, _) = locate(items, target)?;
    let others: Vec<&ComicList> =
        siblings(items, parent.as_deref()).iter().filter(|l| !l.id.eq_ignore_ascii_case(dragged)).collect();
    let at = others.iter().position(|l| l.id.eq_ignore_ascii_case(target))?;
    Some((parent, Some(if place == DropPlace::After { at + 1 } else { at })))
}

/// Rang cible pour monter (`up`) ou descendre une liste d'un cran parmi ses voisines ;
/// `None` si elle est déjà au bout.
pub fn step_position(items: &[ComicList], id: &str, up: bool) -> Option<(Option<String>, usize)> {
    let (parent, i) = locate(items, id)?;
    let n = siblings(items, parent.as_deref()).len();
    match up {
        true if i > 0 => Some((parent, i - 1)),
        false if i + 1 < n => Some((parent, i + 1)),
        _ => None,
    }
}

/// Livres d'une liste de lecture déplacés d'un cran (vers le haut si `up`) : position `to`
/// à passer à `ListsEditor::move_books` (rang dans la liste sans les livres déplacés), ou
/// `None` s'il n'y a rien à faire.
pub fn shift_books(list: &[String], moving: &[String], up: bool) -> Option<usize> {
    let moving: HashSet<String> = moving.iter().map(|b| b.trim().to_ascii_lowercase()).collect();
    let pos: Vec<usize> =
        list.iter().enumerate().filter(|(_, b)| moving.contains(&b.to_ascii_lowercase())).map(|(i, _)| i).collect();
    let (&first, &last) = (pos.first()?, pos.last()?);
    if up {
        // Juste avant le livre qui précède le premier livre déplacé (aucun livre déplacé
        // avant lui : même rang dans la liste sans les livres déplacés).
        (first > 0).then(|| first - 1)
    } else {
        // Juste après le livre qui suit le dernier livre déplacé (rang `last + 1` moins les
        // `k` livres déplacés, tous avant lui).
        (last + 1 < list.len()).then(|| last + 2 - pos.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use comicrust_core::comic_lists::Combine;
    use comicrust_core::smart::SmartQuery;

    fn list(id: &str, kind: ListKind) -> ComicList {
        ComicList { id: id.into(), name: id.to_uppercase(), description: String::new(), kind, comicrack_count: 0 }
    }

    fn smart(id: &str, base: Option<&str>) -> ComicList {
        list(
            id,
            ListKind::Smart {
                query: Some(SmartQuery { base_list: base.map(str::to_string), ..Default::default() }),
                cached: None,
            },
        )
    }

    /// lib, f1 { a, s1 (part de a), f2 { s2 (part de f1) } }, s3 (part de s1), b
    fn tree() -> Vec<ComicList> {
        let f2 = list("f2", ListKind::Folder { combine: Combine::Or, children: vec![smart("s2", Some("f1"))] });
        let f1 = list(
            "f1",
            ListKind::Folder {
                combine: Combine::Or,
                children: vec![list("a", ListKind::Ids(vec![])), smart("s1", Some("a")), f2],
            },
        );
        vec![list("lib", ListKind::Library), f1, smart("s3", Some("s1")), list("b", ListKind::Ids(vec![]))]
    }

    #[test]
    fn tree_navigation() {
        let t = tree();
        assert_eq!(subtree(&t, "f1"), ["f1", "a", "s1", "f2", "s2"]);
        assert_eq!(locate(&t, "s2"), Some((Some("f2".into()), 0)));
        assert_eq!(locate(&t, "b"), Some((None, 3)));
        assert_eq!(ancestors(&t, "s2"), ["f1", "f2"]);
        assert_eq!(base_dependents(&t, "a"), ["S1"], "s1 part de a");
        assert_eq!(base_dependents(&t, "f1"), ["S3"], "s1 et s2 sont dans la branche supprimée");
    }

    #[test]
    fn base_lists_without_loops() {
        let t = tree();
        let ids = |e: Option<&str>| base_candidates(&t, e).into_iter().map(|c| c.id).collect::<Vec<_>>();
        // Nouvelle liste : toutes sauf la Bibliothèque.
        assert_eq!(ids(None), ["f1", "a", "s1", "f2", "s2", "s3", "b"]);
        // s1 : ni elle-même, ni s3 (part de s1), ni f1 (dossier qui la contient), ni f2/s2
        // (s2 part de f1, qui contient s1).
        assert_eq!(ids(Some("s1")), ["a", "b"]);
        assert_eq!(ids(Some("S3")), ["f1", "a", "s1", "f2", "s2", "b"], "casse ignorée");
        assert!(depends_on(&t, "s2", "a"));
        assert!(!depends_on(&t, "a", "s2"));
    }

    #[test]
    fn moving_lists() {
        let t = tree();
        let targets: Vec<String> = move_targets(&t, "f2").into_iter().map(|x| x.0).collect();
        assert_eq!(targets, ["f1"], "pas dans lui-même");
        assert!(move_targets(&t, "f1").is_empty());
        // Glisser b avant s3 (premier niveau : lib, f1, s3, b).
        assert_eq!(drop_position(&t, "b", "s3", DropPlace::Before), Some((None, Some(2))));
        assert_eq!(drop_position(&t, "lib", "b", DropPlace::After), Some((None, Some(3))));
        assert_eq!(drop_position(&t, "b", "f1", DropPlace::Into), Some((Some("f1".into()), None)));
        assert_eq!(drop_position(&t, "b", "a", DropPlace::Into), None, "a n'est pas un dossier");
        assert_eq!(drop_position(&t, "f1", "s2", DropPlace::After), None, "dans sa propre branche");
        assert_eq!(drop_position(&t, "a", "s1", DropPlace::After), Some((Some("f1".into()), Some(1))));
        assert_eq!(step_position(&t, "s1", true), Some((Some("f1".into()), 0)));
        assert_eq!(step_position(&t, "f2", false), None);
        assert_eq!(step_position(&t, "lib", true), None);
    }

    #[test]
    fn shifting_books() {
        let l: Vec<String> = ["a", "b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
        let apply = |moving: &[&str], up: bool| -> Option<Vec<String>> {
            let moving: Vec<String> = moving.iter().map(|s| s.to_string()).collect();
            let to = shift_books(&l, &moving, up)?;
            let (moved, mut rest): (Vec<String>, Vec<String>) = l.iter().cloned().partition(|b| moving.contains(b));
            rest.splice(to..to, moved);
            Some(rest)
        };
        assert_eq!(apply(&["c"], true).unwrap(), ["a", "c", "b", "d", "e"]);
        assert_eq!(apply(&["c"], false).unwrap(), ["a", "b", "d", "c", "e"]);
        assert_eq!(apply(&["b", "d"], true).unwrap(), ["b", "d", "a", "c", "e"]);
        assert_eq!(apply(&["b", "d"], false).unwrap(), ["a", "c", "e", "b", "d"]);
        assert_eq!(apply(&["a"], true), None);
        assert_eq!(apply(&["e"], false), None);
        assert_eq!(apply(&["zz"], false), None);
    }

    /// Une opération appliquée à une base de ComicRust en SQLite, de bout en bout.
    #[test]
    fn run_op_on_store() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = StoreConfig::Sqlite(dir.path().join("comicrust.db"));
        Store::open(&cfg).unwrap();
        let target = ListsTarget::Store(cfg.clone());
        let r = run_op(
            &target,
            Box::new(|ed: &mut ListsEditor| {
                let id = ed.create(
                    None,
                    None,
                    "Thanos",
                    comicrust_core::lists_edit::NewList::Reading { books: vec!["B1".into()] },
                )?;
                Ok(Outcome { created: Some(id), ..Default::default() })
            }),
        )
        .unwrap();
        assert_eq!(r.lists.len(), 1);
        assert_eq!(r.lists[0].kind, ListKind::Ids(vec!["b1".into()]));
        let id = r.outcome.created.unwrap();
        // Relu depuis la base : identique à ce qui a été renvoyé.
        assert_eq!(Store::open_existing(&cfg).unwrap().comic_lists().unwrap(), r.lists);
        let e = run_op(&target, Box::new(move |ed: &mut ListsEditor| ed.rename(&id, " ").map(|_| Outcome::default())))
            .unwrap_err();
        assert!(error_text(&e).contains("vide"), "{e}");
    }
}
