//! Pilotage par une IA (serveur MCP) pour les tests de débogage.
//!
//! Compilé seulement avec la fonctionnalité `mcp` (exécutable `ComicRust-mcp.exe`) et
//! activé à l'exécution par `--mcp` (voir `docs/MCP.md`). Les outils génériques
//! (capture, arbre des widgets, clics, clavier, journal) viennent du crate `egui-mcp` ;
//! ce module ajoute ceux propres à ComicRust. Aucune action destructive n'est exposée :
//! pas de suppression de fichiers ni d'écriture dans la base de ComicRack.

use std::path::{Path, PathBuf};

use comicrust_core::comic_lists::{ComicList, ListKind};
use eframe::egui::{self, Context};
use egui_mcp::{json, Bridge, Call, Options, Tool, Value};

use crate::app::{ComicRustApp, Mode, Start};
use crate::library::Status;

/// Port par défaut de ComicRust (PLaunch utilise 47800).
pub const PORT: u16 = 47801;

/// Port et jeton demandés sur la ligne de commande (`--mcp`, `--mcp=PORT`, `--mcp-token=…`).
fn requested() -> Option<(u16, Option<String>)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--mcp` seul : port propre à ComicRust (le crate prend celui de PLaunch par défaut).
    let explicit_port = args.iter().any(|a| a.starts_with("--mcp="));
    egui_mcp::parse_args(&args).map(|(p, t)| (if explicit_port { p } else { PORT }, t))
}

/// Mode `--mcp-stdio` : passerelle pour les clients MCP qui lancent un programme (Claude
/// Desktop). Renvoie `true` si ce mode a été demandé (le programme doit alors s'arrêter).
pub fn run_stdio_if_requested() -> bool {
    if !std::env::args().any(|a| a == "--mcp-stdio") {
        return false;
    }
    let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--mcp-stdio").collect();
    let explicit_port = args.iter().any(|a| a.starts_with("--mcp="));
    let (port, token) = egui_mcp::parse_args(args.iter().chain(std::iter::once(&"--mcp".to_string())))
        .map(|(p, t)| (if explicit_port { p } else { PORT }, t))
        .unwrap_or((PORT, None));
    let launch = || -> std::io::Result<()> {
        let exe = std::env::current_exe()?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg(format!("--mcp={port}"));
        if let Some(t) = &token {
            cmd.arg(format!("--mcp-token={t}"));
        }
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP : survit à la passerelle.
            cmd.creation_flags(0x0000_0008 | 0x0000_0200);
        }
        cmd.spawn().map(|_| ())
    };
    if let Err(e) = egui_mcp::stdio::run_bridge(port, token.as_deref(), launch) {
        eprintln!("passerelle MCP : {e}");
    }
    true
}

/// Démarre le serveur si `--mcp` est demandé. `Err` : port occupé, etc.
pub fn start(ctx: &Context, data_dir: Option<&Path>) -> Result<Option<Bridge>, String> {
    let Some((port, token)) = requested() else { return Ok(None) };
    let mut opts = Options::new("ComicRust", env!("CARGO_PKG_VERSION"));
    opts.port = port;
    opts.token = token;
    opts.log_file = data_dir.map(|d| d.join(crate::paths::CRASH_FILE));
    opts.instructions = "ComicRust : lecteur et gestionnaire de BD (portage Rust de ComicRack). Deux vues : \
        bibliothèque (listes à gauche, couvertures ou tableau au centre, détails à droite) et lecteur. \
        get_state résume l'état ; list_books / open_book / reader_goto_page / set_view / select_list \
        pilotent l'application sans passer par la souris. get_logs lit le dernier rapport de plantage \
        non encore affiché ; get_errors donne les erreurs en cours."
        .into();
    Bridge::start(ctx, opts, tools())
        .map(Some)
        .map_err(|e| format!("Serveur MCP impossible à démarrer sur le port {port} : {e}"))
}

fn tools() -> Vec<Tool> {
    let obj = |props: Value| json!({ "type": "object", "properties": props });
    vec![
        Tool::simple(
            "get_state",
            "État de ComicRust : vue, livre ouvert (page, nombre de pages), bibliothèque (source, \
             chargement, nombre de livres, liste et livre sélectionnés, recherche), thème, fenêtres \
             ouvertes, erreur affichée.",
        ),
        Tool::new(
            "list_books",
            "Livres de la bibliothèque chargée (id, libellé, fichier, pages). `filter` : recherche \
             rapide comme le champ de la bibliothèque.",
            obj(json!({
                "filter": { "type": "string" },
                "limit": { "type": "integer", "description": "50 par défaut, 500 au plus" },
                "offset": { "type": "integer" },
            })),
        ),
        Tool::new(
            "open_book",
            "Ouvre un livre dans le lecteur : `id` (livre de la bibliothèque) ou `path` (fichier, \
             dossier d'images ou liste .cbl). `page` facultative (à partir de 1).",
            obj(json!({
                "id": { "type": "string" },
                "path": { "type": "string" },
                "page": { "type": "integer" },
            })),
        ),
        Tool::new(
            "reader_goto_page",
            "Lecteur : va à une page (à partir de 1), ou tourne la page avec direction = next / prev.",
            obj(json!({
                "page": { "type": "integer" },
                "direction": { "type": "string", "enum": ["next", "prev"] },
            })),
        ),
        Tool::simple("close_book", "Ferme le livre ouvert (la page est mémorisée)."),
        Tool::new(
            "set_view",
            "Affiche la bibliothèque ou le lecteur.",
            obj(json!({ "view": { "type": "string", "enum": ["library", "reader"] } })),
        ),
        Tool::simple("list_lists", "Listes de la bibliothèque (id, nom, type), en arbre."),
        Tool::new(
            "select_list",
            "Affiche une liste (par `id` ou `name`) ; sans argument : toute la bibliothèque.",
            obj(json!({ "id": { "type": "string" }, "name": { "type": "string" } })),
        ),
        Tool::new(
            "set_search",
            "Texte de la recherche rapide de la bibliothèque (vide : effacer).",
            obj(json!({ "text": { "type": "string" } })),
        ),
        Tool::new(
            "select_book",
            "Sélectionne un livre de la bibliothèque (panneau de détails).",
            obj(json!({ "id": { "type": "string" } })),
        ),
        Tool::simple(
            "close_dialog",
            "Ferme les fenêtres d'information ouvertes (erreur, infos, à propos, bilans, question de \
             reprise, confirmation de travail sur fichiers). L'assistant de configuration et le \
             transfert ne sont pas fermés.",
        ),
        Tool::simple(
            "get_errors",
            "Erreurs en cours : message affiché, erreurs d'écriture en arrière-plan, échec du \
             chargement de la bibliothèque, surveillance, et derniers rapports de plantage.",
        ),
        Tool::simple("show_window", "Restaure et met au premier plan la fenêtre (si elle est réduite)."),
    ]
}

impl ComicRustApp {
    /// Traite les appels des outils propres à ComicRust (début de `ui`).
    pub(crate) fn mcp_calls(&mut self, ctx: &Context) {
        let calls = match &mut self.mcp {
            Some(b) => b.take_calls(),
            None => return,
        };
        for call in calls {
            match call.name.as_str() {
                "get_state" => {
                    let state = self.mcp_state();
                    call.json(state);
                }
                "list_books" => self.mcp_list_books(call),
                "open_book" => self.mcp_open_book(ctx, call),
                "reader_goto_page" => self.mcp_goto(ctx, call),
                "close_book" => {
                    if self.book.is_none() {
                        call.error("aucun livre ouvert");
                    } else {
                        self.close_book(ctx);
                        call.ok("livre fermé");
                    }
                }
                "set_view" => match call.str("view") {
                    Some("library") => {
                        self.mode = Mode::Library;
                        call.ok("vue bibliothèque");
                    }
                    Some("reader") if self.book.is_some() => {
                        self.mode = Mode::Reader;
                        call.ok("vue lecteur");
                    }
                    Some("reader") => call.error("aucun livre ouvert"),
                    _ => call.error("view : library ou reader"),
                },
                "list_lists" => {
                    let lists: Vec<Value> = self.library.lists.items.iter().map(list_json).collect();
                    call.json(json!({ "lists": lists, "error": self.library.lists.error }));
                }
                "select_list" => self.mcp_select_list(call),
                "set_search" => {
                    self.library.search = call.str("text").unwrap_or_default().to_string();
                    self.mode = Mode::Library;
                    call.ok(format!("recherche : « {} »", self.library.search));
                }
                "select_book" => {
                    let id = call.str("id").unwrap_or_default().to_lowercase();
                    match self.library.database().and_then(|db| db.get(&id).map(|b| b.caption())) {
                        Some(caption) => {
                            self.library.selected_book = Some(id);
                            self.library.selected_books.clear();
                            self.library.cursor = None;
                            call.ok(format!("sélection : {caption}"));
                        }
                        None => call.error(format!("livre introuvable : {id}")),
                    }
                }
                "close_dialog" => {
                    let closed = self.mcp_close_dialogs();
                    if closed.is_empty() {
                        call.ok("aucune fenêtre d'information ouverte");
                    } else {
                        call.ok(format!("fermé : {}", closed.join(", ")));
                    }
                }
                "get_errors" => {
                    let errors = self.mcp_errors();
                    call.json(errors);
                }
                "show_window" => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    call.ok("fenêtre restaurée");
                }
                // Appel non traité : le crate répond « outil non pris en charge ».
                _ => drop(call),
            }
        }
    }

    /// À la fin de `ui`, une fois toutes les fenêtres dessinées.
    pub(crate) fn mcp_end_frame(&mut self, ctx: &Context) {
        if let Some(b) = &mut self.mcp {
            b.end_frame(ctx);
        }
    }

    fn mcp_state(&self) -> Value {
        let book = self.book.as_ref().map(|b| {
            json!({
                "path": b.path.display().to_string(),
                "title": b.title,
                "opened": b.opened,
                "page": b.current + 1,
                "file_page_index": b.number(b.current),
                "page_count": b.page_count(),
                "total": b.total,
                "page_errors": b.errors.len(),
            })
        });
        let (status, count) = match &self.library.status {
            Status::NotConfigured => ("non configurée".to_string(), None),
            Status::Loading { what, .. } => (format!("chargement : {what}"), None),
            Status::Loaded(db) => ("chargée".to_string(), Some(db.books.len())),
            Status::Failed(e) => (format!("échec : {e}"), None),
        };
        let db = self.library.database();
        let selected_book = self.library.selected_book.as_ref().map(|id| {
            json!({
                "id": id,
                "caption": db.as_ref().and_then(|d| d.get(id)).map(|b| b.caption()),
            })
        });
        let selected_list = self.library.selected_list.as_ref().map(|id| {
            json!({
                "id": id,
                "name": ComicList::find(&self.library.lists.items, id).map(|l| l.name.clone()),
            })
        });
        let reading_list =
            self.list.as_ref().map(|l| json!({ "path": l.path.display().to_string(), "current": l.current }));
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "view": match self.mode { Mode::Library => "library", Mode::Reader => "reader" },
            "book": book,
            "library": {
                "mode": format!("{:?}", self.settings.library_mode),
                "status": status,
                "books": count,
                "selected_list": selected_list,
                "selected_book": selected_book,
                "selected_books": self.library.selected_books.len(),
                "search": self.library.search,
                "open_stack": self.library.open_stack,
                "missing_only": self.library.missing_only,
                "scan_running": self.library.scan.is_some(),
                "watch_error": self.library.watch_error,
            },
            "reading_list": reading_list,
            "theme": format!("{:?}", self.settings.theme),
            "ui_style": format!("{:?}", self.settings.ui_style),
            "fullscreen": self.fullscreen,
            "zoom": self.zoom,
            "dialogs": self.mcp_open_dialogs(),
            "error": self.error,
        })
    }

    fn mcp_open_dialogs(&self) -> Vec<&'static str> {
        let mut d = Vec::new();
        let checks = [
            (self.error.is_some(), "erreur"),
            (self.resume_prompt.is_some(), "reprise de lecture"),
            (self.file_dialog.is_some(), "confirmation travail sur fichiers"),
            (self.show_file_job, "travail sur fichiers"),
            (self.setup.is_some(), "assistant de configuration"),
            (self.transfer.is_some(), "transfert ComicRack"),
            (self.info_editor.is_some(), "infos du livre (édition)"),
            (self.show_info, "infos"),
            (self.show_about, "à propos"),
            (self.show_last_scan, "bilan d'analyse"),
            (self.scripts_closing.is_some(), "fermeture des scripts"),
        ];
        for (open, name) in checks {
            if open {
                d.push(name);
            }
        }
        d
    }

    fn mcp_close_dialogs(&mut self) -> Vec<&'static str> {
        let mut closed = Vec::new();
        if self.error.take().is_some() {
            closed.push("erreur");
        }
        if self.resume_prompt.take().is_some() {
            closed.push("reprise de lecture");
        }
        if self.file_dialog.take().is_some() {
            closed.push("confirmation travail sur fichiers");
        }
        for (flag, name) in [
            (&mut self.show_file_job, "travail sur fichiers"),
            (&mut self.show_info, "infos"),
            (&mut self.show_about, "à propos"),
            (&mut self.show_last_scan, "bilan d'analyse"),
        ] {
            if std::mem::take(flag) {
                closed.push(name);
            }
        }
        closed
    }

    fn mcp_errors(&self) -> Value {
        let library = match &self.library.status {
            Status::Failed(e) => Some(e.clone()),
            _ => None,
        };
        // Rapports de plantage déjà montrés (renommés plantage-AAAAMMJJ-HHMMSS.txt).
        let mut crashes: Vec<PathBuf> = self
            .data_dir
            .as_ref()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("plantage") && n.ends_with(".txt"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        crashes.sort();
        let last_crash = crashes.last().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| {
            let head: String = t.chars().take(4000).collect();
            head
        });
        json!({
            "error": self.error,
            "background_errors": *self.bg_errors.lock().unwrap(),
            "library": library,
            "watch_error": self.library.watch_error,
            "lists_error": self.library.lists.error,
            "crash_reports": crashes.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "last_crash_report": last_crash,
        })
    }

    fn mcp_list_books(&self, call: Call) {
        let Some(db) = self.library.database() else {
            return call.error("bibliothèque non chargée");
        };
        let filter = call.str("filter").unwrap_or_default();
        let limit = call.u64("limit").unwrap_or(50).clamp(1, 500) as usize;
        let offset = call.u64("offset").unwrap_or(0) as usize;
        let all: Vec<usize> = (0..db.books.len()).collect();
        let hits = if filter.is_empty() { all } else { crate::library::sorting::filter(&db.books, all, filter) };
        let books: Vec<Value> = hits
            .iter()
            .skip(offset)
            .take(limit)
            .map(|&i| {
                let b = &db.books[i];
                json!({
                    "id": b.id,
                    "caption": b.caption(),
                    "file": b.file,
                    "pages": b.page_count,
                    "current_page": b.current_page,
                })
            })
            .collect();
        call.json(json!({ "total": hits.len(), "offset": offset, "books": books }));
    }

    fn mcp_open_book(&mut self, ctx: &Context, call: Call) {
        let page = call.u64("page").map(|p| (p.max(1) - 1) as usize);
        let (id, path_arg) = (call.str("id").map(str::to_string), call.str("path").map(PathBuf::from));
        let path = if let Some(id) = id {
            let Some(db) = self.library.database() else {
                return call.error("bibliothèque non chargée");
            };
            let Some(file) = db.get(&id.to_lowercase()).and_then(|b| b.file.clone()) else {
                return call.error(format!("livre introuvable ou sans fichier : {id}"));
            };
            self.settings.effective_path(Path::new(&file))
        } else if let Some(p) = path_arg {
            p
        } else {
            return call.error("indiquer « id » ou « path »");
        };
        if !path.exists() {
            return call.error(format!("fichier introuvable : {}", path.display()));
        }
        let is_list = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cbl"));
        match page {
            Some(p) if !is_list => self.open_book_at(ctx, path.clone(), Start::Page(p)),
            _ => self.open_path(ctx, path.clone()),
        }
        let note = if self.resume_prompt.is_some() {
            " (question « reprendre à quelle page ? » affichée)"
        } else {
            " (chargement en cours : vérifier avec get_state ou wait)"
        };
        call.ok(format!("ouverture de {}{note}", path.display()));
    }

    fn mcp_goto(&mut self, ctx: &Context, call: Call) {
        let Some(count) = self.book.as_ref().map(|b| b.page_count()) else {
            return call.error("aucun livre ouvert");
        };
        match (call.u64("page"), call.str("direction")) {
            (Some(p), _) => {
                if count == 0 {
                    return call.error("pages pas encore chargées");
                }
                self.mode = Mode::Reader;
                self.go_to((p.max(1) - 1) as usize);
            }
            (None, Some("next")) => self.next_page(ctx),
            (None, Some("prev")) => self.prev_page(ctx),
            _ => return call.error("indiquer « page » ou « direction » (next, prev)"),
        }
        let page = self.book.as_ref().map(|b| b.current + 1).unwrap_or(0);
        call.ok(format!("page {page} / {count}"));
    }

    fn mcp_select_list(&mut self, call: Call) {
        let (id, name) = (call.str("id").map(str::to_string), call.str("name").map(str::to_string));
        let target = match (id, name) {
            (Some(id), _) => ComicList::find(&self.library.lists.items, &id).map(|l| l.id.clone()),
            (None, Some(name)) => find_by_name(&self.library.lists.items, &name).map(|l| l.id.clone()),
            (None, None) => {
                self.select_list(None);
                self.mode = Mode::Library;
                return call.ok("toute la bibliothèque");
            }
        };
        match target {
            Some(id) => {
                self.select_list(Some(id.clone()));
                self.mode = Mode::Library;
                call.ok(format!("liste affichée : {id}"));
            }
            None => call.error("liste introuvable (voir list_lists)"),
        }
    }
}

fn find_by_name<'a>(lists: &'a [ComicList], name: &str) -> Option<&'a ComicList> {
    for l in lists {
        if l.name.eq_ignore_ascii_case(name) {
            return Some(l);
        }
        if let ListKind::Folder { children, .. } = &l.kind {
            if let Some(found) = find_by_name(children, name) {
                return Some(found);
            }
        }
    }
    None
}

fn list_json(l: &ComicList) -> Value {
    let (kind, children) = match &l.kind {
        ListKind::Library => ("bibliothèque", Vec::new()),
        ListKind::Folder { children, .. } => ("dossier", children.iter().map(list_json).collect()),
        ListKind::Ids(ids) => {
            return json!({ "id": l.id, "name": l.name, "kind": "liste de lecture", "books": ids.len() })
        }
        ListKind::Smart { .. } => ("intelligente", Vec::new()),
    };
    if children.is_empty() {
        json!({ "id": l.id, "name": l.name, "kind": kind })
    } else {
        json!({ "id": l.id, "name": l.name, "kind": kind, "children": children })
    }
}
