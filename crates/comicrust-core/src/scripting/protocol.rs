//! Protocole entre ComicRust et le processus Python : un objet JSON par ligne (UTF-8).
//!
//! Canaux :
//! - **stdin** du processus : ComicRust → script ;
//! - **sortie standard d'origine** (dupliquée par le module Python avant de rediriger
//!   `sys.stdout` vers stderr) : script → ComicRust ;
//! - **stderr** : journal libre du script (`print()`, tracebacks imprévus…).
//!
//! Déroulement :
//! 1. le script envoie `{"t":"hello","protocol":1,"python":"3.13.1"}` ;
//! 2. ComicRust répond `{"t":"run", "protocol":1, "script":…, "function":…, "hook":…,
//!    "book_ids":[…], "interactive":…, "live":…, "settings":{"values":{…},"secret":[…]},
//!    "fields":{"editable":[…],"read_only":[…]}, "lib_dirs":[…], "args":…}` ;
//! 3. pendant l'exécution, le script envoie des **demandes** `{"t":"req","id":N,"m":…}`
//!    auxquelles ComicRust répond `{"t":"reply","id":N,"ok":true,"value":…}` ou
//!    `{"t":"reply","id":N,"ok":false,"error":"…"}` :
//!    - `books` `{"ids":[…]}` → fiches JSON ([`book_json`](super::book_json)) ;
//!    - `ask` `{"kind":"text"|"choice"|"confirm","question":…,"default":…,"choices":[…]}` ;
//!    - `message` `{"text":…}` ;
//!    - `changes` `{"items":[{"op":"set","book":…,"field":…,"value":…},
//!      {"op":"rename","book":…,"path":…}, {"op":"write_info","book":…}]}` →
//!      `{"accepted":N,"rejected":[{"index":i,"reason":…}],"live_error":…}` ;
//!    - `save_settings` `{"values":{…},"secret":[…]}` ;
//! 4. et des **notifications** sans réponse : `{"t":"log","level":…,"text":…}`,
//!    `{"t":"progress","fraction":0.5,"text":…}` ;
//! 5. fin : `{"t":"done","value":…}`, `{"t":"error","type":…,"message":…,"traceback":…}`
//!    ou `{"t":"cancelled"}`.
//!
//! ComicRust peut envoyer à tout moment `{"t":"cancel"}` : `comicrust.cancelled()` devient
//! vrai tout de suite ; si le script n'a pas fini 1 s plus tard, le module lève
//! `KeyboardInterrupt` dans son fil principal ; sans réaction après le délai de grâce, le
//! processus est tué.

use serde::Deserialize;
use serde_json::{Map, Value};

/// Message reçu du script.
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum FromScript {
    Hello {
        protocol: u32,
        #[serde(default)]
        python: String,
    },
    Req {
        id: u64,
        m: String,
        #[serde(flatten)]
        params: Map<String, Value>,
    },
    Log {
        #[serde(default)]
        level: String,
        #[serde(default)]
        text: String,
    },
    Progress {
        #[serde(default)]
        fraction: Option<f64>,
        #[serde(default)]
        text: String,
    },
    Done {
        #[serde(default)]
        value: Value,
    },
    Error {
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        traceback: String,
    },
    Cancelled,
}

/// Élément d'une demande `changes`.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChangeItem {
    Set {
        book: String,
        field: String,
        #[serde(default)]
        value: Value,
    },
    Rename {
        book: String,
        path: String,
    },
    WriteInfo {
        book: String,
    },
}
