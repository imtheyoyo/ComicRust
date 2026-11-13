//! Modifications demandées par un script : aperçu (avant → après), choix des lignes, application
//! et bilan. Logique de `crates/comicrust/src/scripts/apply.rs` (version egui) **dupliquée** et
//! branchée sur l'existant Tauri :
//! 1. fiches : `edit::apply_edits` sur la cible de [`Edition::cible`] (base de ComicRust ou base
//!    MySQL de ComicRack en écriture) ;
//! 2. renommages / déplacements : `edit::rename_or_move`, fichier libéré par le lecteur, arrêt
//!    possible **entre deux livres** seulement (un fichier déplacé a toujours sa fiche à jour) ;
//! 3. ComicInfo.xml : écriture des infos existante ([`Edition::ecrire_infos_fichiers`]).
//!
//! Les écritures sont signalées à la surveillance (comme `note_written`) et la bibliothèque
//! affichée est mise à jour (`Biblio::remplacer_fiches`, événement `bibliotheque-modifiee`).

use crate::biblio::Biblio;
use crate::edition::{self, Cible, Edition, EvenementEdition, Statut};
use crate::lecteur::Lecteur;
use comicrust_core::db::ComicDatabase;
use comicrust_core::edit::{self, EditReport, EditTarget, FieldEdit, MoveReport, CUSTOM_VALUE_PREFIX};
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{field_edit, AppliedRename, ScriptChange};
use comicrust_core::store::Store;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// ───────────────────────────── Contrat IPC ─────────────────────────────

/// Une ligne de l'aperçu (`LigneApercu` du contrat).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneApercu {
    /// Clé de la ligne, rendue par `script_appliquer` : `<livre>|fiche|<champ>`,
    /// `<livre>|renommage` ou `<livre>|comicinfo`.
    pub cle: String,
    pub livre_id: String,
    pub titre: String,
    /// Nom ComicRack du champ (`Series`, `CustomValue:<clé>`), `Fichier` ou `ComicInfo.xml`.
    pub champ: String,
    /// Libellé français du champ.
    pub libelle: String,
    pub avant: String,
    pub apres: String,
    /// `fiche`, `renommage` ou `comicinfo`.
    pub genre: &'static str,
    /// Valeur refusée, conflit de renommage… : ligne non applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflit: Option<String>,
}

/// Aperçu d'une exécution (`Apercu` du contrat).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Apercu {
    pub lignes: Vec<LigneApercu>,
    /// Problèmes en une ligne chacun (valeurs refusées, conflits, livres disparus).
    pub conflits: Vec<String>,
    /// Livres déjà dans l'état demandé.
    pub inchanges: usize,
    pub lecture_seule: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif: Option<String>,
}

/// Bilan d'une application (`Bilan` du contrat).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bilan {
    /// Fiches modifiées.
    pub fiches: usize,
    /// Fichiers renommés ou déplacés.
    pub renommages: usize,
    /// Fichiers dont le ComicInfo.xml a été écrit.
    pub comicinfo: usize,
    pub erreurs: Vec<String>,
    /// Arrêt demandé avant la fin (renommages ou écritures non faits).
    pub arrete: bool,
    /// Résumé d'une ligne.
    pub resume: String,
}

// ───────────────────────────── Aperçu ─────────────────────────────

/// Champ modifié d'un livre.
#[derive(Debug, Clone, PartialEq)]
struct LigneChamp {
    field: String,
    before: String,
    after: String,
    error: Option<String>,
    edit: FieldEdit,
}

/// Renommage demandé.
#[derive(Debug, Clone, PartialEq)]
struct LigneRenommage {
    from: PathBuf,
    to: PathBuf,
    problem: Option<String>,
}

/// Modifications d'un livre.
#[derive(Debug, Clone, PartialEq)]
struct LivreApercu {
    id: String,
    caption: String,
    fields: Vec<LigneChamp>,
    rename: Option<LigneRenommage>,
    /// `Some(None)` : demandé ; `Some(Some(problème))` : impossible.
    write_info: Option<Option<String>>,
}

/// Aperçu calculé (gardé par l'exécution pour `script_appliquer`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApercuCalcule {
    livres: Vec<LivreApercu>,
    inchanges: usize,
    manquants: Vec<String>,
}

/// Texte d'un champ (ou d'une valeur personnalisée).
pub fn texte_champ(field: &str, book: &LibraryBook) -> String {
    if let Some(key) = field.strip_prefix(CUSTOM_VALUE_PREFIX) {
        return edit::custom_value(book.custom_values_store.as_deref(), key).unwrap_or_default();
    }
    edit::field(field).map(|d| d.text(book)).unwrap_or_default()
}

/// Libellé français d'un champ.
pub fn libelle_champ(field: &str) -> String {
    if let Some(key) = field.strip_prefix(CUSTOM_VALUE_PREFIX) {
        return format!("Valeur perso. « {key} »");
    }
    edit::field(field).map(|d| d.label.to_string()).unwrap_or_else(|| field.to_string())
}

/// Chemin cible normalisé d'un renommage (relatif = dans le dossier actuel du fichier).
fn resoudre_renommage(current: &Path, requested: &str) -> Result<PathBuf, String> {
    edit::files::normalize_rename(&current.to_string_lossy(), requested).map(PathBuf::from)
}

fn textes_renommage(from: &Path, to: &Path) -> (String, String) {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if from.parent() == to.parent() {
        (name(from), name(to))
    } else {
        (name(from), to.display().to_string())
    }
}

fn meme_chemin(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

fn texte_valeur(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

const SEULS_CBZ: &str = "seuls les CBZ reçoivent les infos (convertir d'abord en CBZ)";

impl ApercuCalcule {
    /// Aperçu des modifications `changes` (la dernière valeur d'un champ l'emporte) sur les fiches
    /// de `db`. Rien n'est écrit.
    pub fn construire(changes: &[ScriptChange], db: &ComicDatabase) -> ApercuCalcule {
        #[derive(Default)]
        struct Voulu {
            fields: Vec<(String, serde_json::Value)>,
            rename: Option<String>,
            write_info: bool,
        }
        let mut ordre: Vec<String> = Vec::new();
        let mut voulus: HashMap<String, Voulu> = HashMap::new();
        for c in changes {
            let key = c.book_id().to_ascii_lowercase();
            let w = voulus.entry(key.clone()).or_insert_with(|| {
                ordre.push(key.clone());
                Voulu::default()
            });
            match c {
                ScriptChange::SetField { field, value, .. } => match w.fields.iter_mut().find(|(f, _)| f == field) {
                    Some(slot) => slot.1 = value.clone(),
                    None => w.fields.push((field.clone(), value.clone())),
                },
                ScriptChange::Rename { new_path, .. } => w.rename = Some(new_path.clone()),
                ScriptChange::WriteInfo { .. } => w.write_info = true,
            }
        }
        let mut out = ApercuCalcule::default();
        let mut destinations: HashMap<String, String> = HashMap::new();
        for key in ordre {
            let w = voulus.remove(&key).unwrap_or_default();
            let Some(book) = db.get(&key) else {
                out.manquants.push(key);
                continue;
            };
            let mut after = book.clone();
            let mut fields = Vec::new();
            for (field, value) in &w.fields {
                let edit = field_edit(&book.id, field, value);
                let before = texte_champ(field, book);
                let error = edit::apply_to_book(&mut after, &edit).err().map(|e| e.message);
                let new = texte_champ(field, &after);
                if error.is_some() || new != before {
                    let after = if error.is_some() { texte_valeur(value) } else { new };
                    fields.push(LigneChamp { field: field.clone(), before, after, error, edit });
                }
            }
            let rename = w.rename.as_deref().and_then(|req| {
                let from = PathBuf::from(book.file.as_deref().unwrap_or_default());
                if from.as_os_str().is_empty() {
                    return Some(LigneRenommage {
                        from,
                        to: PathBuf::from(req),
                        problem: Some("livre sans fichier".into()),
                    });
                }
                let to = match resoudre_renommage(&from, req) {
                    Ok(to) => to,
                    Err(e) => return Some(LigneRenommage { from, to: PathBuf::from(req), problem: Some(e) }),
                };
                if to == from {
                    return None;
                }
                let mut problem = edit::files::check_path(&to).err();
                if problem.is_none() && !from.is_file() {
                    problem = Some("fichier introuvable".into());
                }
                if problem.is_none() && to.exists() && !meme_chemin(&from, &to) {
                    problem = Some("la destination existe déjà".into());
                }
                let k = to.to_string_lossy().to_lowercase();
                if problem.is_none() {
                    if let Some(other) = destinations.get(&k) {
                        problem = Some(format!("même destination que « {other} »"));
                    }
                }
                if problem.is_none() {
                    destinations.insert(k, book.caption());
                }
                Some(LigneRenommage { from, to, problem })
            });
            let write_info = w.write_info.then(|| {
                let path = PathBuf::from(book.file.as_deref().unwrap_or_default());
                let final_path = rename.as_ref().filter(|r| r.problem.is_none()).map(|r| r.to.clone()).unwrap_or(path);
                (!edition::accepte_infos(&final_path)).then(|| SEULS_CBZ.to_string())
            });
            if fields.is_empty() && rename.is_none() && write_info.is_none() {
                out.inchanges += 1;
                continue;
            }
            out.livres.push(LivreApercu { id: book.id.clone(), caption: book.caption(), fields, rename, write_info });
        }
        out
    }

    /// Lignes et conflits pour le front.
    pub fn pour_le_front(&self, motif_lecture_seule: Option<String>) -> Apercu {
        let mut lignes = Vec::new();
        let mut conflits = Vec::new();
        for b in &self.livres {
            for f in &b.fields {
                if let Some(e) = &f.error {
                    conflits.push(format!("{} ({}) : {e}", b.caption, libelle_champ(&f.field)));
                }
                lignes.push(LigneApercu {
                    cle: format!("{}|fiche|{}", b.id, f.field),
                    livre_id: b.id.clone(),
                    titre: b.caption.clone(),
                    champ: f.field.clone(),
                    libelle: libelle_champ(&f.field),
                    avant: f.before.clone(),
                    apres: f.after.clone(),
                    genre: "fiche",
                    conflit: f.error.clone(),
                });
            }
            if let Some(r) = &b.rename {
                if let Some(p) = &r.problem {
                    conflits.push(format!("{} : renommage refusé — {p}", b.caption));
                }
                let (avant, apres) = textes_renommage(&r.from, &r.to);
                lignes.push(LigneApercu {
                    cle: format!("{}|renommage", b.id),
                    livre_id: b.id.clone(),
                    titre: b.caption.clone(),
                    champ: "Fichier".into(),
                    libelle: "Fichier (renommage / déplacement)".into(),
                    avant,
                    apres,
                    genre: "renommage",
                    conflit: r.problem.clone(),
                });
            }
            if let Some(w) = &b.write_info {
                if let Some(p) = w {
                    conflits.push(format!("{} : {p}", b.caption));
                }
                lignes.push(LigneApercu {
                    cle: format!("{}|comicinfo", b.id),
                    livre_id: b.id.clone(),
                    titre: b.caption.clone(),
                    champ: "ComicInfo.xml".into(),
                    libelle: "Infos écrites dans le fichier".into(),
                    avant: String::new(),
                    apres: "écrire les infos dans le fichier".into(),
                    genre: "comicinfo",
                    conflit: w.clone(),
                });
            }
        }
        conflits.extend(self.manquants.iter().map(|id| format!("{id} : livre disparu de la bibliothèque")));
        Apercu {
            lignes,
            conflits,
            inchanges: self.inchanges,
            lecture_seule: motif_lecture_seule.is_some(),
            motif: motif_lecture_seule,
        }
    }

    /// Plan des lignes `cles` choisies (lignes en conflit ignorées). `db` : fiches actuelles.
    pub fn plan(&self, cles: &[String], db: &ComicDatabase) -> Plan {
        let voulues: HashSet<String> = cles.iter().map(|c| c.to_ascii_lowercase()).collect();
        let veut = |c: String| voulues.contains(&c.to_ascii_lowercase());
        let mut plan = Plan::default();
        for b in &self.livres {
            for f in b.fields.iter().filter(|f| f.error.is_none()) {
                if veut(format!("{}|fiche|{}", b.id, f.field)) {
                    plan.edits.push(f.edit.clone());
                }
            }
            if let Some(r) = b.rename.as_ref().filter(|r| r.problem.is_none()) {
                if veut(format!("{}|renommage", b.id)) {
                    if let Some(book) = db.get(&b.id) {
                        plan.renames.push((book.clone(), r.to.clone()));
                    }
                }
            }
            if matches!(b.write_info, Some(None)) && veut(format!("{}|comicinfo", b.id)) {
                plan.write_info.push(b.id.clone());
            }
        }
        plan
    }
}

// ───────────────────────────── Application ─────────────────────────────

/// Modifications à appliquer.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub edits: Vec<FieldEdit>,
    /// Fiche (actuelle) et nouveau chemin.
    pub renames: Vec<(LibraryBook, PathBuf)>,
    /// Livres dont les infos sont à écrire dans le fichier (après les fiches).
    pub write_info: Vec<String>,
}

impl Plan {
    pub fn est_vide(&self) -> bool {
        self.edits.is_empty() && self.renames.is_empty() && self.write_info.is_empty()
    }

    /// Plan d'un lot appliqué au fil de l'eau (automatismes) ; livre inconnu ignoré.
    pub fn depuis_changements(changes: &[ScriptChange], book: impl Fn(&str) -> Option<LibraryBook>) -> Plan {
        let mut plan = Plan::default();
        for c in changes {
            match c {
                ScriptChange::SetField { book_id, field, value } => plan.edits.push(field_edit(book_id, field, value)),
                ScriptChange::Rename { book_id, new_path } => {
                    if let Some(b) = book(book_id) {
                        let from = PathBuf::from(b.file.as_deref().unwrap_or_default());
                        let to = resoudre_renommage(&from, new_path).unwrap_or_else(|_| PathBuf::from(new_path));
                        plan.renames.retain(|(x, _)| !x.id.eq_ignore_ascii_case(book_id));
                        plan.renames.push((b, to));
                    }
                }
                ScriptChange::WriteInfo { book_id } => {
                    if !plan.write_info.iter().any(|i| i.eq_ignore_ascii_case(book_id)) {
                        plan.write_info.push(book_id.clone());
                    }
                }
            }
        }
        plan
    }
}

/// Résultat d'une application.
#[derive(Debug, Default)]
pub struct Resultat {
    pub edit: Option<EditReport>,
    pub moves: Vec<MoveReport>,
    /// Renommages refusés : (livre, message).
    pub rename_errors: Vec<(String, String)>,
    /// ComicInfo.xml écrits, et problèmes de cette étape.
    pub comicinfo: usize,
    pub comicinfo_errors: Vec<String>,
    /// Lecture seule, base inaccessible… : rien (ou pas tout) n'a été appliqué.
    pub fatal: Option<String>,
    /// Renommages ou écritures non faits : arrêt demandé avant leur tour.
    pub interrupted: usize,
    /// Identifiants des livres mis à jour dans la bibliothèque affichée.
    pub modifies: Vec<String>,
    /// Fiches relues après écriture (les plus récentes).
    pub fraiches: Vec<LibraryBook>,
}

impl Resultat {
    pub fn complet(&self) -> bool {
        self.fatal.is_none()
            && self.rename_errors.is_empty()
            && self.comicinfo_errors.is_empty()
            && self.interrupted == 0
            && self.edit.as_ref().is_none_or(|r| r.is_complete())
    }

    /// Erreurs et conflits, une ligne chacun.
    pub fn problemes(&self, caption: impl Fn(&str) -> String) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(f) = &self.fatal {
            out.push(f.clone());
        }
        if let Some(r) = &self.edit {
            out.extend(r.missing.iter().map(|id| format!("{} : livre absent de la base", caption(id))));
            out.extend(r.conflicts.iter().map(|id| format!("{} : fiche modifiée ailleurs entre-temps", caption(id))));
            out.extend(r.errors.iter().map(|e| {
                let what = if e.field.is_empty() { String::new() } else { format!(" ({})", libelle_champ(&e.field)) };
                format!("{}{what} : {}", caption(&e.book_id), e.message)
            }));
        }
        out.extend(self.rename_errors.iter().map(|(id, m)| format!("{} : renommage refusé — {m}", caption(id))));
        out.extend(self.comicinfo_errors.iter().cloned());
        if self.interrupted > 0 {
            out.push(format!("{} opération(s) non faite(s) : arrêt demandé avant leur tour", self.interrupted));
        }
        out
    }

    /// Bilan pour le front.
    pub fn bilan(&self, caption: impl Fn(&str) -> String) -> Bilan {
        let fiches = self.edit.as_ref().map_or(0, |r| r.changed.len());
        let erreurs = self.problemes(caption);
        let mut parts = vec![format!("{fiches} fiche(s) modifiée(s)")];
        if !self.moves.is_empty() {
            parts.push(format!("{} fichier(s) renommé(s)", self.moves.len()));
        }
        if self.comicinfo > 0 {
            parts.push(format!("{} ComicInfo.xml écrit(s)", self.comicinfo));
        }
        if !erreurs.is_empty() {
            parts.push(format!("{} problème(s)", erreurs.len()));
        }
        if self.interrupted > 0 {
            parts.push("arrêté".into());
        }
        Bilan {
            fiches,
            renommages: self.moves.len(),
            comicinfo: self.comicinfo,
            erreurs,
            arrete: self.interrupted > 0,
            resume: parts.join(", "),
        }
    }

    /// Renommages réussis (bilan relu par `comicrust.applied_renames()`).
    pub fn renommages_faits(&self) -> Vec<AppliedRename> {
        self.moves
            .iter()
            .map(|m| AppliedRename {
                book: m.book.id.clone(),
                from: m.moved.from.to_string_lossy().into_owned(),
                to: m.moved.to.to_string_lossy().into_owned(),
            })
            .collect()
    }
}

/// Pourquoi les modifications des scripts ne peuvent pas être appliquées maintenant (`None` :
/// elles le peuvent) : bibliothèque en lecture seule (ComicRack sans « Lecture et écriture »,
/// ComicDb.xml, aucune bibliothèque), configuration MySQL incomplète ou base hors ligne.
pub fn motif_lecture_seule(edition: &Edition, biblio: &Biblio) -> Option<String> {
    match edition.cible() {
        Cible::LectureSeule(m) => Some(m),
        Cible::Comicrack(cfg) if !cfg.is_complete() => Some(crate::surveillance::CONFIG_MYSQL_INCOMPLETE.into()),
        Cible::Store(_) | Cible::Comicrack(_) => biblio.motif_hors_ligne(),
    }
}

/// Cible d'écriture de `edit` (base de ComicRust ouverte, ou base MySQL de ComicRack).
fn cible_ecriture<'a>(store: &'a mut Option<Store>, cible: &'a Cible) -> EditTarget<'a> {
    match (store.as_mut(), cible) {
        (Some(s), _) => EditTarget::Store(s),
        (None, Cible::Comicrack(cfg)) => EditTarget::ComicRack(cfg),
        _ => EditTarget::ReadOnly,
    }
}

/// Ce qu'il faut pour appliquer : bibliothèque, édition (cible, ComicInfo) et lecteur
/// (libération des fichiers).
#[derive(Clone)]
pub struct Applicateur {
    pub biblio: Biblio,
    pub edition: Edition,
    pub lecteur: Lecteur,
}

impl Applicateur {
    /// Applique `plan`. `stop` est vérifié **entre deux livres**, jamais au milieu d'un
    /// renommage ; `progression` reçoit l'avancement de l'écriture des ComicInfo.xml.
    pub fn appliquer(&self, plan: &Plan, stop: &AtomicBool, progression: &(dyn Fn(String) + Sync)) -> Resultat {
        let mut out = Resultat::default();
        if plan.est_vide() {
            return out;
        }
        if let Some(m) = motif_lecture_seule(&self.edition, &self.biblio) {
            out.fatal = Some(m);
            return out;
        }
        let cible = self.edition.cible();
        {
            // Une opération d'édition à la fois, jamais pendant une analyse des dossiers.
            let _garde = match self.biblio.prendre_edition() {
                Ok(g) => g,
                Err(e) => {
                    out.fatal = Some(format!("Modifications non appliquées : {e}"));
                    return out;
                }
            };
            let mut store = match &cible {
                Cible::Store(cfg) => match Store::open_existing(cfg) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        out.fatal = Some(format!("Base ComicRust inaccessible : {e}"));
                        return out;
                    }
                },
                _ => None,
            };
            // 1. Fiches.
            let mut fraiches: HashMap<String, LibraryBook> = HashMap::new();
            if !plan.edits.is_empty() {
                match edit::apply_edits(cible_ecriture(&mut store, &cible), &plan.edits) {
                    Ok(r) => {
                        self.biblio.signaler_ecriture_autonome(r.changed.iter().map(|b| b.id.clone()), r.store_counter);
                        if let Some(w) = &r.comicrack {
                            self.biblio.signaler_rapport_comicrack(w);
                        }
                        let couv = plan.edits.iter().any(|e| e.field.eq_ignore_ascii_case("PreferredFrontCover"));
                        out.modifies.extend(self.biblio.remplacer_fiches(&r.changed, couv));
                        fraiches.extend(r.changed.iter().map(|b| (b.id.to_ascii_lowercase(), b.clone())));
                        out.edit = Some(r);
                    }
                    Err(e) => {
                        out.fatal = Some(format!("Fiches non modifiées : {e}"));
                        return out;
                    }
                }
            }
            // 2. Fichiers (fiche relue après l'étape 1 si elle a changé).
            for (i, (book, to)) in plan.renames.iter().enumerate() {
                if stop.load(Ordering::SeqCst) {
                    out.interrupted += plan.renames.len() - i;
                    break;
                }
                let current = fraiches.get(&book.id.to_ascii_lowercase()).cloned().unwrap_or_else(|| book.clone());
                let de = PathBuf::from(current.file.as_deref().unwrap_or_default());
                let deplacement = {
                    let _libere = self.lecteur.liberer(std::slice::from_ref(&de));
                    edit::rename_or_move(cible_ecriture(&mut store, &cible), &current, to)
                };
                match deplacement {
                    Ok(m) => {
                        self.biblio.signaler_ecriture_autonome([m.book.id.clone()], m.store_counter);
                        if let Some(w) = &m.comicrack {
                            self.biblio.signaler_rapport_comicrack(w);
                        }
                        self.lecteur.oublier_fichier(&de);
                        out.modifies.extend(self.biblio.remplacer_fiches(std::slice::from_ref(&m.book), false));
                        fraiches.insert(m.book.id.to_ascii_lowercase(), m.book.clone());
                        out.moves.push(m);
                    }
                    Err(e) => out.rename_errors.push((book.id.clone(), e.to_string())),
                }
            }
            out.fraiches = fraiches.into_values().collect();
        }
        // 3. Infos dans les fichiers : écriture existante (garde reprise par l'édition).
        if !plan.write_info.is_empty() {
            if stop.load(Ordering::SeqCst) {
                out.interrupted += plan.write_info.len();
                return out;
            }
            let modifies = std::sync::Mutex::new(Vec::new());
            let sortie = |e: EvenementEdition| match e {
                EvenementEdition::BibliothequeModifiee { ids } => modifies.lock().unwrap().extend(ids),
                EvenementEdition::Progression { fait, total, livre, .. } => {
                    progression(format!("ComicInfo.xml {fait}/{total} {livre}"))
                }
            };
            match self.edition.ecrire_infos_fichiers(&plan.write_info, true, &sortie) {
                Ok(b) => {
                    out.comicinfo = b.livres.iter().filter(|l| l.statut == Statut::Ok).count();
                    for l in b.livres.iter().filter(|l| l.statut == Statut::Erreur) {
                        out.comicinfo_errors.push(format!(
                            "{} : ComicInfo.xml non écrit — {}",
                            l.titre,
                            l.message.as_deref().unwrap_or("erreur")
                        ));
                    }
                    if b.interrompu {
                        out.interrupted += b.livres.iter().filter(|l| l.statut == Statut::Ignore).count().max(1);
                    }
                }
                Err(e) => out.comicinfo_errors.push(format!("ComicInfo.xml non écrits : {e}")),
            }
            out.modifies.extend(modifies.into_inner().unwrap());
            let mut vus = HashSet::new();
            out.modifies.retain(|i| vus.insert(i.to_ascii_lowercase()));
        }
        out
    }
}
