//! Pilotage de ComicRust-Tauri par une IA (serveur MCP), pour les tests de débogage.
//!
//! Compilé seulement avec la fonction `mcp` (`ComicRust-Tauri-mcp.exe`) et activé au
//! lancement par `--mcp` (voir docs/MCP-tauri.md). Même protocole et mêmes noms d'outils que
//! la version egui (crate `egui-mcp`), mais le pilotage passe par le protocole DevTools de
//! WebView2 : captures, souris et clavier réels, JavaScript dans la page.
//!
//! - [`protocol`] : JSON-RPC MCP ; [`http`] : serveur local ; [`stdio`] : passerelle stdio ;
//! - [`cdp`] : appels DevTools ; [`outils`] : les outils.

mod cdp;
mod http;
mod outils;
mod protocol;
mod stdio;

use std::sync::Arc;

use tauri::AppHandle;

pub use outils::SCRIPT;

/// Port par défaut : 47802 (47800 PLaunch, 47801 ComicRust egui), pour pouvoir lancer les deux
/// versions de ComicRust en même temps.
pub const PORT: u16 = 47802;

/// `--mcp`, `--mcp=PORT`, `--mcp-token=JETON` (ou variable `MCP_TOKEN`) ; `None` sans `--mcp`.
fn analyser_arguments() -> Option<(u16, Option<String>)> {
    let mut port = None;
    let mut jeton = std::env::var("MCP_TOKEN").ok().filter(|t| !t.is_empty());
    for a in std::env::args().skip(1) {
        if a == "--mcp" || a == "--mcp-stdio" {
            port.get_or_insert(PORT);
        } else if let Some(p) = a.strip_prefix("--mcp=") {
            port = Some(p.parse().unwrap_or(PORT));
        } else if let Some(t) = a.strip_prefix("--mcp-token=") {
            jeton = Some(t.to_string()).filter(|t| !t.is_empty());
        }
    }
    port.map(|p| (p, jeton))
}

/// Vrai si le serveur sera démarré (la page reçoit alors le script de pilotage).
pub fn actif() -> bool {
    analyser_arguments().is_some() && !mode_stdio()
}

fn mode_stdio() -> bool {
    std::env::args().any(|a| a == "--mcp-stdio")
}

/// Mode `--mcp-stdio` : relaie l'entrée standard vers l'instance pilotable (lancée au besoin)
/// puis renvoie `true` (l'application ne doit pas s'ouvrir dans ce processus).
pub fn passerelle_stdio() -> bool {
    if !mode_stdio() {
        return false;
    }
    let (port, jeton) = analyser_arguments().unwrap_or((PORT, None));
    let lancer = || {
        let mut cmd = std::process::Command::new(std::env::current_exe()?);
        cmd.arg(format!("--mcp={port}"));
        if let Some(t) = &jeton {
            cmd.arg(format!("--mcp-token={t}"));
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
    };
    let _ = stdio::run_bridge(port, jeton.as_deref(), lancer);
    true
}

/// Démarre le serveur si `--mcp` est présent (appelé une fois la fenêtre créée).
pub fn demarrer(app: &AppHandle) {
    let Some((port, token)) = analyser_arguments() else { return };
    let options = protocol::Options {
        name: "ComicRust-Tauri".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port,
        token,
        instructions: "ComicRust (version Tauri : interface web Svelte dans WebView2). Méthode \
            conseillée : get_state et screenshot, ui_tree pour repérer les éléments, click_widget \
            (par libellé) ou click (x, y en pixels CSS), type_text après un clic dans un champ, \
            press_key, puis screenshot pour vérifier. get_logs : console et erreurs JavaScript ; \
            eval_js : JavaScript dans la page ; invoke : commandes du backend en lecture seule."
            .into(),
    };
    let app = app.clone();
    let handler: protocol::Handler = Arc::new(move |nom, args| outils::traiter(&app, nom, args));
    match http::serve(protocol::Server::new(options, outils::liste(), handler)) {
        Ok(addr) => eprintln!("serveur MCP actif sur http://{addr}/mcp"),
        Err(e) => eprintln!("serveur MCP : port {port} indisponible ({e})"),
    }
}
