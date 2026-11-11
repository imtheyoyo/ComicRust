//! Passerelle stdio → HTTP.
//!
//! Certains clients MCP (Claude Desktop notamment) lancent les serveurs comme
//! des programmes qui dialoguent sur l'entrée/sortie standard. L'application
//! peut alors être lancée avec `--mcp-stdio` : ce mode relaie chaque ligne
//! JSON-RPC vers le serveur HTTP de l'instance en cours, et la (re)démarre si
//! besoin (avec `--mcp`). La logique (relances, port occupé, avertissements) est
//! dans `crate::pont_mcp` ; ce fichier fournit l'environnement réel.

use std::io::{self, BufRead, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::process::Child;

use super::http;
use crate::pont_mcp::{Delais, Hote, Pont};

/// Environnement réel du pont : socket local, processus lancé, sortie d'erreur.
struct HoteReel<L: FnMut() -> io::Result<Child>> {
    addr: SocketAddr,
    jeton: Option<String>,
    lanceur: L,
    enfant: Option<Child>,
}

impl<L: FnMut() -> io::Result<Child>> Hote for HoteReel<L> {
    fn poster(&mut self, ligne: &str) -> io::Result<(u16, String)> {
        http::post(self.addr, self.jeton.as_deref(), ligne)
    }

    fn lancer(&mut self) -> io::Result<()> {
        self.enfant = Some((self.lanceur)()?);
        Ok(())
    }

    fn lanceur_termine(&mut self) -> Option<String> {
        let statut = self.enfant.as_mut()?.try_wait().ok().flatten()?;
        self.enfant = None;
        Some(statut.to_string())
    }

    fn avertir(&mut self, message: &str) {
        eprintln!("{message}");
    }
}

/// Relaie l'entrée standard vers `127.0.0.1:port` jusqu'à sa fermeture.
///
/// `launch` démarre l'application (à chaque connexion refusée, avec une pause après un
/// démarrage raté) et renvoie son processus pour surveiller un arrêt prématuré.
pub fn run_bridge(port: u16, token: Option<&str>, launch: impl FnMut() -> io::Result<Child>) -> io::Result<()> {
    let hote = HoteReel {
        addr: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        jeton: token.map(str::to_string),
        lanceur: launch,
        enfant: None,
    };
    let mut pont = Pont::nouveau(hote, port, Delais::default());
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(out) = pont.relayer(&line) {
            writeln!(stdout, "{}", out.trim())?;
            stdout.flush()?;
        }
    }
    Ok(())
}
