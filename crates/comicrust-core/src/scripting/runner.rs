//! Exécution d'un script dans un processus Python séparé.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::forms::{check_url, ChooseSpec, FormAnswer, FormSpec, ProgressDetails};
use super::protocol::{ChangeItem, FromScript};
use super::{find_python, Hook, ScriptEntry, EDITABLE_FIELDS, PROTOCOL_VERSION, PYTHON_MODULE, READ_ONLY_FIELDS};
use crate::library::LibraryBook;

/// Nombre maximal de livres par demande `books`.
const MAX_BOOKS_PER_REQUEST: usize = 5_000;
/// Nombre maximal de modifications retenues pour une exécution.
const MAX_CHANGES: usize = 1_000_000;
/// Longueur maximale d'une ligne du journal (au-delà : tronquée).
const MAX_LOG_LINE: usize = 16 * 1024;
/// Longueur maximale d'un texte copié dans le presse-papiers.
const MAX_CLIPBOARD: usize = 1024 * 1024;

/// Lancement du module : `sys.argv[1]` = dossier où `comicrust.py` a été écrit. Le Python
/// « embeddable » ignore `PYTHONPATH` : le chemin est donc ajouté ici.
pub(super) const BOOT: &str = "import sys; sys.path.insert(0, sys.argv[1]); import comicrust; comicrust._main()";

// ---------------------------------------------------------------------------------------
// Types publics
// ---------------------------------------------------------------------------------------

/// Modification demandée par un script (jamais appliquée par l'hôte lui-même).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ScriptChange {
    /// Champ de fiche (nom ComicRack, voir [`EDITABLE_FIELDS`]) ; `Null` = vider.
    SetField { book_id: String, field: String, value: Value },
    /// Renommer/déplacer le fichier du livre. Chemin absolu, ou relatif au dossier actuel
    /// du fichier (simple changement de nom).
    Rename { book_id: String, new_path: String },
    /// Écrire les informations de la fiche (ComicInfo.xml) dans le fichier du livre.
    WriteInfo { book_id: String },
}

impl ScriptChange {
    pub fn book_id(&self) -> &str {
        match self {
            ScriptChange::SetField { book_id, .. }
            | ScriptChange::Rename { book_id, .. }
            | ScriptChange::WriteInfo { book_id } => book_id,
        }
    }
}

/// Modifications regroupées par livre (pour l'aperçu).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookChanges {
    /// Valeur finale de chaque champ modifié (la dernière demande l'emporte).
    pub fields: BTreeMap<String, Value>,
    pub rename: Option<String>,
    pub write_info: bool,
}

/// Demande refusée par l'hôte (livre hors contexte, champ inconnu…).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejectedChange {
    /// Élément tel qu'envoyé par le script.
    pub item: Value,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum JournalKind {
    /// Sortie du script (`print()`, stderr).
    Output,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalLine {
    pub kind: JournalKind,
    pub text: String,
}

/// État final d'une exécution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RunState {
    /// Fonction terminée normalement.
    Finished,
    /// Arrêtée à la demande de l'utilisateur.
    Cancelled,
    /// Exception Python, Python introuvable, protocole rompu…
    Failed,
    /// Délai maximum dépassé (automatismes).
    TimedOut,
}

/// Erreur d'exécution lisible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScriptFailure {
    /// Type d'exception Python (`ValueError`…) ou catégorie (`ComicRust`).
    pub kind: String,
    pub message: String,
    /// Traceback Python (vide si sans objet).
    pub traceback: String,
}

impl std::fmt::Display for ScriptFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.kind.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{} : {}", self.kind, self.message)
        }
    }
}

/// Résultat d'une exécution.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunResult {
    pub state: RunState,
    /// Modifications acceptées, dans l'ordre des demandes. En mode « au fil de l'eau »,
    /// elles ont déjà été transmises à [`ScriptHost::apply_live`].
    pub changes: Vec<ScriptChange>,
    pub rejected: Vec<RejectedChange>,
    pub journal: Vec<JournalLine>,
    pub failure: Option<ScriptFailure>,
    /// Valeur renvoyée par la fonction (si convertible en JSON, sinon son `repr`).
    pub return_value: Value,
    /// Vrai si des modifications ont été transmises à [`ScriptHost::apply_live`].
    pub applied_live: bool,
    pub python_version: Option<String>,
    pub duration: Duration,
}

impl RunResult {
    fn failed(kind: &str, message: String) -> RunResult {
        RunResult {
            state: RunState::Failed,
            changes: Vec::new(),
            rejected: Vec::new(),
            journal: vec![JournalLine { kind: JournalKind::Error, text: message.clone() }],
            failure: Some(ScriptFailure { kind: kind.into(), message, traceback: String::new() }),
            return_value: Value::Null,
            applied_live: false,
            python_version: None,
            duration: Duration::ZERO,
        }
    }

    /// Modifications regroupées par livre (identifiant → modifications).
    pub fn changes_by_book(&self) -> BTreeMap<String, BookChanges> {
        let mut out: BTreeMap<String, BookChanges> = BTreeMap::new();
        for c in &self.changes {
            let e = out.entry(c.book_id().to_string()).or_default();
            match c {
                ScriptChange::SetField { field, value, .. } => {
                    e.fields.insert(field.clone(), value.clone());
                }
                ScriptChange::Rename { new_path, .. } => e.rename = Some(new_path.clone()),
                ScriptChange::WriteInfo { .. } => e.write_info = true,
            }
        }
        out
    }

    /// Journal en un seul texte (une ligne par entrée).
    pub fn journal_text(&self) -> String {
        self.journal.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

/// Type de question posée par `comicrust.ask…`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum QuestionKind {
    /// Texte libre (`ask`).
    Text,
    /// Choix dans une liste (`ask_choice`).
    Choice,
    /// Oui / non (`confirm`).
    Confirm,
}

/// Question d'un script à l'utilisateur.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Question {
    pub kind: QuestionKind,
    pub text: String,
    pub default: Value,
    /// Choix proposés (textes) pour [`QuestionKind::Choice`].
    pub choices: Vec<String>,
    /// Saisie secrète (`ask(..., secret=True)` : clé d'API…) : champ masqué, aucune valeur
    /// par défaut affichée ; une réponse vide garde la valeur actuelle (côté script).
    pub secret: bool,
}

/// Réglages propres à un script (par fichier, voir [`ScriptEntry::settings_key`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptSettings {
    pub values: Map<String, Value>,
    /// Clés des valeurs secrètes (clé d'API…) : à stocker chiffrées.
    pub secret: BTreeSet<String>,
}

impl ScriptSettings {
    /// Forme à enregistrer : les valeurs secrètes sont remplacées par
    /// `{"$secret": "dpapi:…"}` (chiffrement de [`crate::secret`] : DPAPI sous Windows ;
    /// ailleurs la valeur secrète **n'est pas enregistrée**).
    pub fn to_stored(&self) -> Value {
        let mut values = Map::new();
        for (k, v) in &self.values {
            if self.secret.contains(k) {
                let plain = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                values.insert(k.clone(), json!({ "$secret": crate::secret::protect(&plain) }));
            } else {
                values.insert(k.clone(), v.clone());
            }
        }
        Value::Object(values)
    }

    /// Relit la forme enregistrée par [`ScriptSettings::to_stored`].
    pub fn from_stored(stored: &Value) -> ScriptSettings {
        let mut out = ScriptSettings::default();
        if let Value::Object(map) = stored {
            for (k, v) in map {
                match v.get("$secret").and_then(Value::as_str) {
                    Some(enc) if v.as_object().is_some_and(|o| o.len() == 1) => {
                        out.secret.insert(k.clone());
                        out.values.insert(k.clone(), Value::String(crate::secret::unprotect(enc)));
                    }
                    _ => {
                        out.values.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        out
    }
}

/// Ce que l'application (l'interface) fournit au script. Les méthodes sont appelées depuis
/// le fil de l'exécution : elles peuvent bloquer (question à l'utilisateur), mais
/// l'annulation n'est traitée qu'à leur retour.
pub trait ScriptHost: Send + Sync {
    /// Fiches JSON des livres demandés (voir [`book_json`]), dans l'ordre ; un livre
    /// disparu entre-temps est simplement omis. Les identifiants ont déjà été vérifiés
    /// (tous dans le contexte).
    fn books(&self, ids: &[String]) -> Vec<Value>;

    /// Réponse de l'utilisateur ; `None` = valeur par défaut (fenêtre fermée…). N'est
    /// appelée que si le contexte est interactif.
    fn ask(&self, _question: &Question) -> Option<Value> {
        None
    }

    /// Message à afficher (contexte interactif ; sinon il va au journal).
    fn message(&self, _text: &str) {}

    /// Ligne de journal, au fil de l'eau (elle figure aussi dans [`RunResult::journal`]).
    fn log(&self, _line: &JournalLine) {}

    /// Avancement (`fraction` entre 0 et 1, ou `None` si inconnu).
    fn progress(&self, _fraction: Option<f32>, _text: &str) {}

    /// Sous-texte et image de la fenêtre de progression (appelée après [`progress`]
    /// seulement si l'un des deux est donné).
    ///
    /// [`progress`]: ScriptHost::progress
    fn progress_details(&self, _details: &ProgressDetails) {}

    /// Formulaire (`comicrust.form`) : bouton et valeurs, ou `None` (fenêtre fermée,
    /// annulée, script arrêté). N'est appelée que si le contexte est interactif. Par
    /// défaut : aucune interface (`None`).
    fn form(&self, _form: &FormSpec) -> Option<FormAnswer> {
        None
    }

    /// Liste de choix (`comicrust.choose`) : rangs choisis, ou `None`. N'est appelée que si
    /// le contexte est interactif. Par défaut : aucune interface (`None`).
    fn choose(&self, _choice: &ChooseSpec) -> Option<Vec<usize>> {
        None
    }

    /// Ouvre une adresse http(s) déjà vérifiée dans le navigateur ; vrai si c'est fait.
    /// N'est appelée que si le contexte est interactif.
    fn open_url(&self, _url: &str) -> bool {
        false
    }

    /// Copie un texte dans le presse-papiers ; vrai si c'est fait. N'est appelée que si le
    /// contexte est interactif.
    fn clipboard_set(&self, _text: &str) -> bool {
        false
    }

    /// Réglages enregistrés du script.
    fn load_settings(&self, _settings_key: &str) -> ScriptSettings {
        ScriptSettings::default()
    }

    /// Enregistre les réglages du script (voir [`ScriptSettings::to_stored`]).
    fn save_settings(&self, _settings_key: &str, _settings: &ScriptSettings) -> Result<(), String> {
        Err("réglages non pris en charge".into())
    }

    /// Applique tout de suite des modifications (contexte « au fil de l'eau » : automatismes).
    fn apply_live(&self, _changes: &[ScriptChange]) -> Result<(), String> {
        Err("application immédiate non prise en charge".into())
    }
}

/// Fiche JSON d'un livre pour les scripts : noms de champs ComicRack (`Series`, `Number`…),
/// identifiant `Id`, fichier `File` ; la liste des pages est omise.
pub fn book_json(book: &LibraryBook) -> Value {
    let Ok(Value::Object(map)) = serde_json::to_value(book) else { return Value::Null };
    let mut out = Map::with_capacity(map.len());
    for (k, v) in map {
        if k == "Pages" {
            continue;
        }
        let k = k.strip_prefix('@').map(str::to_string).unwrap_or(k);
        out.insert(k, v);
    }
    Value::Object(out)
}

/// Sur quoi porte une exécution.
#[derive(Debug, Clone, PartialEq)]
pub struct RunContext {
    pub hook: Hook,
    /// Livres du contexte (sélection, bibliothèque, livre ouvert, nouveaux livres…). Le
    /// script ne peut ni lire ni modifier d'autres livres.
    pub book_ids: Vec<String>,
    /// Questions possibles ; sinon `ask` renvoie la valeur par défaut et `message` va au
    /// journal.
    pub interactive: bool,
    /// Modifications transmises au fil de l'eau à [`ScriptHost::apply_live`].
    pub live: bool,
    /// Arguments libres transmis au script (`comicrust.args()`).
    pub args: Value,
}

impl RunContext {
    /// Contexte par défaut d'un hook : menus interactifs avec aperçu ; automatismes non
    /// interactifs, appliqués au fil de l'eau.
    pub fn new(hook: Hook, book_ids: Vec<String>) -> RunContext {
        let auto = hook.is_automatic();
        RunContext { hook, book_ids, interactive: !auto, live: auto, args: Value::Null }
    }
}

/// Réglages de l'exécution.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Interpréteur ; `None` = [`find_python`].
    pub python: Option<PathBuf>,
    /// Dossier où écrire `comicrust.py` ; `None` = `data\python\` à côté de l'exécutable
    /// ([`default_module_dir`] : ComicRust est portable, rien n'est écrit ailleurs). Les
    /// tests passent un dossier temporaire explicite.
    pub module_dir: Option<PathBuf>,
    /// Dossier des scripts : `<scripts>/lib` est ajouté à `sys.path`.
    pub scripts_dir: Option<PathBuf>,
    /// Délai maximum (conseillé pour les automatismes) ; `None` = illimité.
    pub timeout: Option<Duration>,
    /// Délai pour que Python démarre et se présente.
    pub start_timeout: Duration,
    /// Délai entre la demande d'arrêt et l'arrêt forcé du processus. Le module laisse 1 s
    /// au script pour s'arrêter de lui-même (`cancelled()`) avant de lever
    /// `KeyboardInterrupt` : garder plus de 1 s.
    pub grace: Duration,
    /// Taille maximale d'un message du script.
    pub max_message_bytes: usize,
    /// Taille maximale cumulée du journal conservé.
    pub max_journal_bytes: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            python: None,
            module_dir: None,
            scripts_dir: None,
            timeout: None,
            start_timeout: Duration::from_secs(30),
            grace: Duration::from_secs(3),
            // Images des formulaires et listes de choix comprises (base64 : 4/3 des 12 Mo
            // permis au total, voir `forms::MAX_IMAGES_TOTAL`).
            max_message_bytes: 20 * 1024 * 1024,
            max_journal_bytes: 1024 * 1024,
        }
    }
}

impl RunOptions {
    /// Réglages conseillés pour un hook : 5 minutes au plus pour un automatisme.
    pub fn for_hook(hook: Hook) -> RunOptions {
        RunOptions { timeout: hook.is_automatic().then(|| Duration::from_secs(300)), ..Default::default() }
    }
}

/// Lance les scripts.
#[derive(Debug, Clone, Default)]
pub struct ScriptRunner {
    pub options: RunOptions,
}

/// Exécution en cours.
pub struct RunHandle {
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<RunResult>>,
    result: Option<RunResult>,
}

impl RunHandle {
    /// Demande l'arrêt : annulation polie (`KeyboardInterrupt` dans le script), puis arrêt
    /// forcé du processus après [`RunOptions::grace`].
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some() || self.thread.as_ref().is_none_or(|t| t.is_finished())
    }

    /// Résultat si l'exécution est terminée (sans bloquer).
    pub fn poll(&mut self) -> Option<&RunResult> {
        if self.result.is_none() && self.thread.as_ref().is_some_and(|t| t.is_finished()) {
            self.result = Some(Self::join(self.thread.take()));
        }
        self.result.as_ref()
    }

    /// Attend la fin de l'exécution au plus `timeout` ; vrai si elle est finie.
    pub fn wait_timeout(&mut self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        loop {
            if self.poll().is_some() {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Attend la fin de l'exécution.
    pub fn wait(mut self) -> RunResult {
        match self.result.take() {
            Some(r) => r,
            None => Self::join(self.thread.take()),
        }
    }

    fn join(t: Option<JoinHandle<RunResult>>) -> RunResult {
        match t.map(JoinHandle::join) {
            Some(Ok(r)) => r,
            _ => RunResult::failed("ComicRust", "erreur interne de l'hôte de scripts".into()),
        }
    }
}

impl ScriptRunner {
    pub fn new(options: RunOptions) -> ScriptRunner {
        ScriptRunner { options }
    }

    /// Lance `entry` sur un fil dédié. Les erreurs de mise en route (Python introuvable…)
    /// se retrouvent dans le [`RunResult`] (état [`RunState::Failed`]).
    pub fn start(&self, entry: &ScriptEntry, context: RunContext, host: Arc<dyn ScriptHost>) -> RunHandle {
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let entry = entry.clone();
        let opts = self.options.clone();
        let thread = std::thread::Builder::new()
            .name(format!("script {}", entry.function))
            .spawn(move || run(entry, context, host, opts, c2))
            .ok();
        let result = if thread.is_none() {
            Some(RunResult::failed("ComicRust", "impossible de créer le fil d'exécution".into()))
        } else {
            None
        };
        RunHandle { cancel, thread, result }
    }
}

// ---------------------------------------------------------------------------------------
// Exécution
// ---------------------------------------------------------------------------------------

/// Écrit `comicrust.py` dans `dir` s'il est absent ou différent (fichier temporaire puis
/// renommage : plusieurs exécutions simultanées ne voient jamais un fichier à moitié écrit).
pub(super) fn ensure_module(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let target = dir.join("comicrust.py");
    if std::fs::read_to_string(&target).is_ok_and(|t| t == PYTHON_MODULE) {
        return Ok(());
    }
    let tmp = dir.join(format!("comicrust.py.{}.tmp", std::process::id()));
    std::fs::write(&tmp, PYTHON_MODULE)?;
    match std::fs::rename(&tmp, &target) {
        Ok(()) => Ok(()),
        // Windows : le fichier cible peut être ouvert par un autre Python ; s'il a déjà le
        // bon contenu, tant mieux.
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            if std::fs::read_to_string(&target).is_ok_and(|t| t == PYTHON_MODULE) {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Dossier par défaut du module `comicrust.py` : `data\python\` à côté de l'exécutable
/// (ComicRust est portable : rien n'est écrit dans `%TEMP%` ni ailleurs hors de son dossier).
pub fn default_module_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("data").join("python")
}

enum Ev {
    Line(Vec<u8>),
    TooLong,
    Eof,
}

/// Lit une ligne d'au plus `max` octets. `Ok(None)` en fin de flux.
fn read_line_bounded<R: BufRead>(r: &mut R, max: usize, buf: &mut Vec<u8>) -> std::io::Result<Option<bool>> {
    buf.clear();
    loop {
        let avail = r.fill_buf()?;
        if avail.is_empty() {
            return Ok(if buf.is_empty() { None } else { Some(true) });
        }
        if let Some(p) = avail.iter().position(|&b| b == b'\n') {
            let ok = buf.len() + p <= max;
            if ok {
                buf.extend_from_slice(&avail[..p]);
            }
            r.consume(p + 1);
            return Ok(Some(ok));
        }
        let n = avail.len();
        if buf.len() + n <= max {
            buf.extend_from_slice(avail);
        } else {
            // Trop long : on vide jusqu'à la fin de ligne sans garder.
            buf.clear();
            r.consume(n);
            loop {
                let avail = r.fill_buf()?;
                if avail.is_empty() {
                    return Ok(Some(false));
                }
                if let Some(p) = avail.iter().position(|&b| b == b'\n') {
                    r.consume(p + 1);
                    return Ok(Some(false));
                }
                let n = avail.len();
                r.consume(n);
            }
        }
        r.consume(n);
    }
}

/// Journal partagé entre le fil principal et le lecteur de stderr.
struct Journal {
    lines: Vec<JournalLine>,
    bytes: usize,
    max: usize,
    truncated: bool,
}

impl Journal {
    fn push(&mut self, host: &dyn ScriptHost, kind: JournalKind, mut text: String) {
        if text.len() > MAX_LOG_LINE {
            let mut cut = MAX_LOG_LINE;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push('…');
        }
        let line = JournalLine { kind, text };
        host.log(&line);
        if self.bytes + line.text.len() > self.max {
            if !self.truncated {
                self.truncated = true;
                self.lines.push(JournalLine {
                    kind: JournalKind::Warning,
                    text: "(journal tronqué : trop de lignes)".into(),
                });
            }
            return;
        }
        self.bytes += line.text.len();
        self.lines.push(line);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
    Cancel,
    Timeout,
    Protocol,
}

struct Session {
    entry: ScriptEntry,
    ctx: RunContext,
    host: Arc<dyn ScriptHost>,
    opts: RunOptions,
    allowed: HashSet<String>,
    stdin: Option<ChildStdin>,
    journal: Arc<Mutex<Journal>>,
    changes: Vec<ScriptChange>,
    rejected: Vec<RejectedChange>,
    applied_live: bool,
    python_version: Option<String>,
    /// Message de fin reçu (`done`, `error`, `cancelled`).
    end: Option<(RunState, Option<ScriptFailure>, Value)>,
    hello: bool,
    protocol_error: Option<String>,
}

fn run(
    entry: ScriptEntry,
    ctx: RunContext,
    host: Arc<dyn ScriptHost>,
    opts: RunOptions,
    cancel: Arc<AtomicBool>,
) -> RunResult {
    let started = Instant::now();
    let mut r = match run_inner(entry, ctx, host, opts, cancel) {
        Ok(r) => r,
        Err(msg) => RunResult::failed("ComicRust", msg),
    };
    r.duration = started.elapsed();
    r
}

fn run_inner(
    entry: ScriptEntry,
    ctx: RunContext,
    host: Arc<dyn ScriptHost>,
    opts: RunOptions,
    cancel: Arc<AtomicBool>,
) -> Result<RunResult, String> {
    let python = match &opts.python {
        Some(p) => p.clone(),
        None => find_python(None).map_err(|e| e.to_string())?,
    };
    let module_dir = opts.module_dir.clone().unwrap_or_else(default_module_dir);
    ensure_module(&module_dir)
        .map_err(|e| format!("impossible d'écrire le module comicrust dans {} : {e}", module_dir.display()))?;

    let mut cmd = Command::new(&python);
    // -I : isolé (ni PYTHONPATH ni site utilisateur, comme l'embeddable) ; -X utf8 :
    // entrées/sorties en UTF-8 sous Windows ; -u : sans tampon ; -B : pas de __pycache__
    // dans le dossier des scripts.
    cmd.args(["-I", "-X", "utf8", "-u", "-B", "-c", BOOT])
        .arg(&module_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = entry.file.parent().filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("impossible de lancer Python ({}) : {e}", python.display()))?;
    // Windows : Python (et les processus lancés par le script) dans un « Job Object » tué
    // à la fermeture de son dernier handle : à la fin de l'exécution, ou si ComicRust se
    // ferme ou plante, rien ne reste orphelin. Ailleurs : le module s'arrête de lui-même
    // quand stdin se ferme (voir `comicrust.py`, `_Channel.reader`).
    #[cfg(windows)]
    let _job = kill_on_close_job(&child);
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().ok_or("sortie de Python indisponible")?;
    let stderr = child.stderr.take().ok_or("erreurs de Python indisponibles")?;

    let journal =
        Arc::new(Mutex::new(Journal { lines: Vec::new(), bytes: 0, max: opts.max_journal_bytes, truncated: false }));

    // Lecteur du protocole.
    let (tx, rx) = mpsc::channel::<Ev>();
    let max = opts.max_message_bytes;
    let _ = std::thread::Builder::new().name("script stdout".into()).spawn(move || {
        let mut r = BufReader::with_capacity(64 * 1024, stdout);
        let mut buf = Vec::new();
        loop {
            match read_line_bounded(&mut r, max, &mut buf) {
                Ok(Some(true)) => {
                    if tx.send(Ev::Line(std::mem::take(&mut buf))).is_err() {
                        return;
                    }
                }
                Ok(Some(false)) => {
                    let _ = tx.send(Ev::TooLong);
                }
                Ok(None) | Err(_) => {
                    let _ = tx.send(Ev::Eof);
                    return;
                }
            }
        }
    });

    // Lecteur du journal (stderr).
    let (etx, erx) = mpsc::channel::<()>();
    {
        let journal = journal.clone();
        let host = host.clone();
        let _ = std::thread::Builder::new().name("script stderr".into()).spawn(move || {
            let mut r = BufReader::new(stderr);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match r.by_ref().take(MAX_LOG_LINE as u64 * 4).read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let text = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                        if let Ok(mut j) = journal.lock() {
                            j.push(&*host, JournalKind::Output, text);
                        }
                    }
                }
            }
            let _ = etx.send(());
        });
    }

    let allowed: HashSet<String> = ctx.book_ids.iter().cloned().collect();
    let mut s = Session {
        entry,
        ctx,
        host,
        opts,
        allowed,
        stdin,
        journal: journal.clone(),
        changes: Vec::new(),
        rejected: Vec::new(),
        applied_live: false,
        python_version: None,
        end: None,
        hello: false,
        protocol_error: None,
    };
    let stop = s.main_loop(&rx, &mut child, &cancel);
    // Fin : stdin fermé (le module s'arrête s'il attendait encore), attente bornée.
    s.stdin = None;
    let status = wait_or_kill(&mut child, s.opts.grace);
    let _ = erx.recv_timeout(Duration::from_secs(2));

    let journal = journal.lock().map(|mut j| std::mem::take(&mut j.lines)).unwrap_or_default();
    let mut result = RunResult {
        state: RunState::Finished,
        changes: std::mem::take(&mut s.changes),
        rejected: std::mem::take(&mut s.rejected),
        journal,
        failure: None,
        return_value: Value::Null,
        applied_live: s.applied_live,
        python_version: s.python_version.take(),
        duration: Duration::ZERO,
    };
    match (stop, s.end.take()) {
        (Some(Stop::Timeout), _) => {
            result.state = RunState::TimedOut;
            result.failure = Some(ScriptFailure {
                kind: "ComicRust".into(),
                message: format!(
                    "délai maximum dépassé ({} s) : script arrêté",
                    s.opts.timeout.map(|d| d.as_secs()).unwrap_or_default()
                ),
                traceback: String::new(),
            });
        }
        (Some(Stop::Cancel), end) => {
            result.state = RunState::Cancelled;
            // Le script a pu se terminer normalement juste avant l'annulation.
            if let Some((RunState::Finished, _, v)) = end {
                result.state = RunState::Finished;
                result.return_value = v;
            }
        }
        (Some(Stop::Protocol), _) => {
            result.state = RunState::Failed;
            result.failure = Some(ScriptFailure {
                kind: "ComicRust".into(),
                message: s.protocol_error.take().unwrap_or_else(|| "dialogue avec Python rompu".into()),
                traceback: String::new(),
            });
        }
        (None, Some((state, failure, value))) => {
            result.state = state;
            result.failure = failure;
            result.return_value = value;
        }
        (None, None) => {
            result.state = RunState::Failed;
            let code = status.map(|c| c.to_string()).unwrap_or_else(|| "?".into());
            let detail = if s.hello { "" } else { " avant même de démarrer (Python est-il complet ?)" };
            result.failure = Some(ScriptFailure {
                kind: "ComicRust".into(),
                message: format!("Python s'est arrêté sans réponse{detail} (code {code}) ; voir le journal"),
                traceback: String::new(),
            });
        }
    }
    if let Some(f) = &result.failure {
        let text = if f.traceback.is_empty() { f.to_string() } else { f.traceback.trim_end().to_string() };
        result.journal.push(JournalLine { kind: JournalKind::Error, text });
    }
    Ok(result)
}

/// Job Object Windows `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` contenant `child` : tous ses
/// processus (sous-processus du script compris, créés après l'affectation) sont tués quand
/// le handle est fermé (`Drop`). `None` si le système refuse (le processus reste alors
/// géré comme ailleurs : annulation puis `kill`).
#[cfg(windows)]
struct KillOnCloseJob(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        // SAFETY : handle obtenu de CreateJobObjectW, fermé une seule fois.
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn kill_on_close_job(child: &Child) -> Option<KillOnCloseJob> {
    use std::os::windows::io::AsRawHandle;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    // SAFETY : appels Win32 sur des handles valides (le job créé ici, le processus enfant
    // encore vivant tant que `child` existe) ; la structure vit pendant l'appel.
    unsafe {
        let job = KillOnCloseJob(CreateJobObjectW(None, PCWSTR::null()).ok()?);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .ok()?;
        AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())).ok()?;
        Some(job)
    }
}

/// Attend la fin du processus pendant `grace`, puis le tue. Code de sortie si connu.
fn wait_or_kill(child: &mut Child, grace: Duration) -> Option<i32> {
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return st.code(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                return child.wait().ok().and_then(|s| s.code());
            }
        }
    }
}

impl Session {
    fn log(&self, kind: JournalKind, text: String) {
        if let Ok(mut j) = self.journal.lock() {
            j.push(&*self.host, kind, text);
        }
    }

    fn send(&mut self, v: &Value) {
        let Some(stdin) = self.stdin.as_mut() else { return };
        let mut line = serde_json::to_vec(v).unwrap_or_default();
        line.push(b'\n');
        if stdin.write_all(&line).and_then(|_| stdin.flush()).is_err() {
            self.stdin = None;
        }
    }

    /// Boucle principale ; renvoie la raison d'un arrêt imposé.
    fn main_loop(&mut self, rx: &Receiver<Ev>, child: &mut Child, cancel: &AtomicBool) -> Option<Stop> {
        let started = Instant::now();
        let mut stop: Option<Stop> = None;
        let mut stop_at: Option<Instant> = None;
        let mut end_at: Option<Instant> = None;
        loop {
            // Arrêt demandé ou délai dépassé : annulation polie, puis arrêt forcé.
            if stop.is_none() && self.end.is_none() {
                if cancel.load(Ordering::SeqCst) {
                    stop = Some(Stop::Cancel);
                } else if self.opts.timeout.is_some_and(|t| started.elapsed() > t) {
                    stop = Some(Stop::Timeout);
                } else if !self.hello && started.elapsed() > self.opts.start_timeout {
                    self.protocol_error = Some("Python ne répond pas au démarrage".into());
                    stop = Some(Stop::Protocol);
                }
                if let Some(st) = stop {
                    stop_at = Some(Instant::now());
                    if st == Stop::Protocol {
                        let _ = child.kill();
                    } else {
                        self.send(&json!({"t": "cancel"}));
                    }
                }
            }
            let forced = stop_at.is_some_and(|t| t.elapsed() > self.opts.grace)
                || end_at.is_some_and(|t| t.elapsed() > self.opts.grace);
            if forced {
                let _ = child.kill();
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ev::Line(line)) => {
                    if let Err(e) = self.handle(&line) {
                        self.protocol_error = Some(e);
                        let _ = child.kill();
                        return Some(Stop::Protocol);
                    }
                    if self.end.is_some() && end_at.is_none() {
                        end_at = Some(Instant::now());
                        // Plus rien à dire au script : stdin fermé.
                        self.stdin = None;
                    }
                }
                Ok(Ev::TooLong) => {
                    self.protocol_error = Some(format!(
                        "message du script trop gros (plus de {} octets) : script arrêté",
                        self.opts.max_message_bytes
                    ));
                    let _ = child.kill();
                    return Some(Stop::Protocol);
                }
                Ok(Ev::Eof) | Err(RecvTimeoutError::Disconnected) => return stop,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Traite un message du script. `Err` = protocole rompu.
    fn handle(&mut self, line: &[u8]) -> Result<(), String> {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(());
        }
        let msg: FromScript = serde_json::from_slice(line).map_err(|e| format!("message illisible du script : {e}"))?;
        match msg {
            FromScript::Hello { protocol, python } => {
                if protocol != PROTOCOL_VERSION {
                    let age = if protocol < PROTOCOL_VERSION { "plus ancien" } else { "plus récent" };
                    return Err(format!(
                        "module Python comicrust {age} que ComicRust (protocole {protocol}, {PROTOCOL_VERSION} \
                         attendu) : un autre fichier comicrust.py est-il importé à la place de celui de ComicRust ?"
                    ));
                }
                self.hello = true;
                self.python_version = Some(python);
                let settings = self.host.load_settings(&self.entry.settings_key());
                let lib_dirs: Vec<String> = self
                    .opts
                    .scripts_dir
                    .iter()
                    .map(|d| d.join("lib"))
                    .filter(|d| d.is_dir())
                    .map(|d| d.display().to_string())
                    .collect();
                let run = json!({
                    "t": "run",
                    "protocol": PROTOCOL_VERSION,
                    "version": env!("CARGO_PKG_VERSION"),
                    "script": self.entry.file.display().to_string(),
                    "function": self.entry.function,
                    "name": self.entry.name,
                    "hook": self.ctx.hook.name(),
                    "book_ids": self.ctx.book_ids,
                    "interactive": self.ctx.interactive,
                    "live": self.ctx.live,
                    "args": self.ctx.args,
                    "settings": settings,
                    "fields": {
                        "editable": &*EDITABLE_FIELDS,
                        "read_only": &*READ_ONLY_FIELDS,
                        "custom_prefix": crate::edit::CUSTOM_VALUE_PREFIX,
                    },
                    "lib_dirs": lib_dirs,
                });
                self.send(&run);
            }
            FromScript::Req { id, m, params } => {
                if !self.hello {
                    return Err("demande avant la présentation".into());
                }
                let reply = match self.request(&m, &params) {
                    Ok(value) => json!({"t": "reply", "id": id, "ok": true, "value": value}),
                    Err(error) => json!({"t": "reply", "id": id, "ok": false, "error": error}),
                };
                self.send(&reply);
            }
            FromScript::Log { level, text } => {
                let kind = match level.as_str() {
                    "warning" => JournalKind::Warning,
                    "error" => JournalKind::Error,
                    _ => JournalKind::Info,
                };
                for l in text.lines() {
                    self.log(kind, l.to_string());
                }
            }
            FromScript::Progress { fraction, text, detail, image } => {
                let f = fraction.filter(|f| f.is_finite()).map(|f| f.clamp(0.0, 1.0) as f32);
                self.host.progress(f, &text);
                match ProgressDetails::parse(detail, image.as_ref()) {
                    Ok(d) if d.is_empty() => {}
                    Ok(d) => self.host.progress_details(&d),
                    Err(e) => self.log(JournalKind::Warning, format!("progression : {e}")),
                }
            }
            FromScript::Done { value } => self.end = Some((RunState::Finished, None, value)),
            FromScript::Error { kind, message, traceback } => {
                self.end = Some((RunState::Failed, Some(ScriptFailure { kind, message, traceback }), Value::Null))
            }
            FromScript::Cancelled => self.end = Some((RunState::Cancelled, None, Value::Null)),
        }
        Ok(())
    }

    fn request(&mut self, method: &str, p: &Map<String, Value>) -> Result<Value, String> {
        match method {
            "books" => {
                let ids: Vec<String> = p
                    .get("ids")
                    .and_then(Value::as_array)
                    .ok_or("liste « ids » attendue")?
                    .iter()
                    .map(|v| v.as_str().map(str::to_string).ok_or("identifiant non textuel"))
                    .collect::<Result<_, _>>()?;
                if ids.len() > MAX_BOOKS_PER_REQUEST {
                    return Err(format!("au plus {MAX_BOOKS_PER_REQUEST} livres par demande"));
                }
                if let Some(bad) = ids.iter().find(|id| !self.allowed.contains(*id)) {
                    return Err(format!("livre {bad} hors du contexte du script"));
                }
                Ok(Value::Array(self.host.books(&ids)))
            }
            "ask" => {
                let kind = match p.get("kind").and_then(Value::as_str).unwrap_or("text") {
                    "choice" => QuestionKind::Choice,
                    "confirm" => QuestionKind::Confirm,
                    _ => QuestionKind::Text,
                };
                let q = Question {
                    kind,
                    text: p.get("question").and_then(Value::as_str).unwrap_or_default().to_string(),
                    default: p.get("default").cloned().unwrap_or(Value::Null),
                    choices: p
                        .get("choices")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter().map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).collect()
                        })
                        .unwrap_or_default(),
                    secret: p.get("secret").and_then(Value::as_bool).unwrap_or(false),
                };
                let answer = if self.ctx.interactive { self.host.ask(&q) } else { None };
                let valid = match (&answer, kind) {
                    (Some(Value::String(_)), QuestionKind::Text) => true,
                    (Some(Value::String(s)), QuestionKind::Choice) => q.choices.contains(s),
                    (Some(Value::Bool(_)), QuestionKind::Confirm) => true,
                    _ => false,
                };
                Ok(if valid { answer.unwrap_or_default() } else { q.default })
            }
            "message" => {
                let text = p.get("text").and_then(Value::as_str).unwrap_or_default();
                if self.ctx.interactive {
                    self.host.message(text);
                } else {
                    self.log(JournalKind::Info, text.to_string());
                }
                Ok(Value::Null)
            }
            "changes" => self.changes(p),
            "form" => {
                let spec = FormSpec::parse(p).map_err(|e| format!("formulaire : {e}"))?;
                if !self.ctx.interactive {
                    self.log(
                        JournalKind::Info,
                        format!("formulaire « {} » non affiché : script sans interface (None)", spec.title),
                    );
                    return Ok(Value::Null);
                }
                let Some(answer) = self.host.form(&spec) else { return Ok(Value::Null) };
                match spec.check_button(&answer.button) {
                    Ok(true) => Ok(json!({"button": answer.button, "values": spec.sanitize(&answer.values)})),
                    Ok(false) => Ok(Value::Null),
                    Err(e) => Err(format!("formulaire : {e}")),
                }
            }
            "choose" => {
                let spec = ChooseSpec::parse(p).map_err(|e| format!("liste de choix : {e}"))?;
                if !self.ctx.interactive {
                    self.log(
                        JournalKind::Info,
                        format!("liste de choix « {} » non affichée : script sans interface (None)", spec.title),
                    );
                    return Ok(Value::Null);
                }
                Ok(match self.host.choose(&spec).and_then(|picked| spec.sanitize(&picked)) {
                    None => Value::Null,
                    Some(picked) if spec.multi => json!(picked),
                    Some(picked) => json!(picked[0]),
                })
            }
            "open_url" => {
                let url = check_url(p.get("url").and_then(Value::as_str).unwrap_or_default())?;
                if !self.ctx.interactive {
                    self.log(JournalKind::Info, format!("adresse non ouverte (script sans interface) : {url}"));
                    return Ok(Value::Bool(false));
                }
                Ok(Value::Bool(self.host.open_url(&url)))
            }
            "clipboard_set" => {
                let text = p.get("text").and_then(Value::as_str).unwrap_or_default();
                if text.len() > MAX_CLIPBOARD {
                    return Err(format!("texte trop long pour le presse-papiers (plus de {MAX_CLIPBOARD} octets)"));
                }
                if !self.ctx.interactive {
                    self.log(JournalKind::Info, "presse-papiers non modifié : script sans interface".into());
                    return Ok(Value::Bool(false));
                }
                Ok(Value::Bool(self.host.clipboard_set(text)))
            }
            "save_settings" => {
                let values = p.get("values").and_then(Value::as_object).cloned().unwrap_or_default();
                let secret = p
                    .get("secret")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                let settings = ScriptSettings { values, secret };
                self.host.save_settings(&self.entry.settings_key(), &settings).map(|_| Value::Null)
            }
            other => Err(format!("demande inconnue « {other} »")),
        }
    }

    fn changes(&mut self, p: &Map<String, Value>) -> Result<Value, String> {
        let items = p.get("items").and_then(Value::as_array).ok_or("liste « items » attendue")?;
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        for (index, raw) in items.iter().enumerate() {
            match self.check_change(raw) {
                Ok(c) if self.changes.len() + accepted.len() < MAX_CHANGES => accepted.push(c),
                Ok(_) => rejected.push((index, "trop de modifications pour une exécution".to_string())),
                Err(reason) => rejected.push((index, reason)),
            }
        }
        for (index, reason) in &rejected {
            self.log(JournalKind::Warning, format!("modification refusée : {reason}"));
            self.rejected.push(RejectedChange { item: items[*index].clone(), reason: reason.clone() });
        }
        let mut live_error = Value::Null;
        if self.ctx.live && !accepted.is_empty() {
            match self.host.apply_live(&accepted) {
                Ok(()) => self.applied_live = true,
                Err(e) => {
                    self.log(JournalKind::Error, format!("application des modifications : {e}"));
                    live_error = Value::String(e);
                }
            }
        }
        let n = accepted.len();
        self.changes.extend(accepted);
        Ok(json!({
            "accepted": n,
            "rejected": rejected.iter().map(|(i, r)| json!({"index": i, "reason": r})).collect::<Vec<_>>(),
            "live_error": live_error,
        }))
    }

    fn check_change(&self, raw: &Value) -> Result<ScriptChange, String> {
        let item: ChangeItem = serde_json::from_value(raw.clone()).map_err(|e| format!("demande illisible : {e}"))?;
        let book = match &item {
            ChangeItem::Set { book, .. } | ChangeItem::Rename { book, .. } | ChangeItem::WriteInfo { book } => book,
        };
        if !self.allowed.contains(book) {
            return Err(format!("livre {book} hors du contexte du script"));
        }
        Ok(match item {
            ChangeItem::Set { book, field, value } => {
                let custom = field.starts_with(crate::edit::CUSTOM_VALUE_PREFIX);
                if !custom && !EDITABLE_FIELDS.contains(&field.as_str()) {
                    return Err(if READ_ONLY_FIELDS.contains(&field.as_str()) {
                        format!("champ « {field} » non modifiable")
                    } else {
                        format!("champ inconnu « {field} »")
                    });
                }
                if matches!(value, Value::Array(_) | Value::Object(_)) {
                    return Err(format!("valeur de « {field} » : texte, nombre, booléen ou None attendu"));
                }
                // Même validation que l'application des modifications : une valeur refusée
                // l'est tout de suite (journal du script), pas seulement à l'aperçu.
                crate::edit::validate(&super::field_edit(&book, &field, &value)).map_err(|e| e.message)?;
                ScriptChange::SetField { book_id: book, field, value }
            }
            ChangeItem::Rename { book, path } => {
                let path = path.trim().to_string();
                if path.is_empty() || path.len() > 4096 || path.contains('\0') {
                    return Err("nouveau chemin vide ou invalide".into());
                }
                ScriptChange::Rename { book_id: book, new_path: path }
            }
            ChangeItem::WriteInfo { book } => ScriptChange::WriteInfo { book_id: book },
        })
    }
}
