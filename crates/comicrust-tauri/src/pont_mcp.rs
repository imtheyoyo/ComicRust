//! Logique du pont MCP stdio → HTTP (`ComicRust-Tauri-mcp.exe --mcp-stdio`).
//!
//! Séparée de `mcp::stdio` (qui fournit l'hôte réel : socket, processus) pour être testée sans
//! WebView2. Règles :
//! - connexion **refusée** (rien n'écoute) : l'application est (re)lancée avec `--mcp`, **à chaque
//!   fois que nécessaire** (elle a pu être fermée entre deux requêtes), et non plus une seule fois ;
//!   après un échec de démarrage, une pause évite de la relancer en boucle ;
//! - port occupé par un autre programme (il accepte la connexion mais ne parle pas MCP) : **aucune
//!   relance**, un avertissement explicite va sur la sortie d'erreur du pont **et** dans la réponse
//!   JSON-RPC (pour que le client MCP l'affiche) ;
//! - l'application lancée qui n'ouvre pas son port (démarrage lent, port refusé) : même avertissement.
// Les éléments de ce module ne servent qu'à la fonction `mcp` (hors tests).
#![cfg_attr(not(feature = "mcp"), allow(dead_code))]

use std::io::{self, ErrorKind};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Délais du pont (réglables pour les tests).
#[derive(Debug, Clone, Copy)]
pub struct Delais {
    /// Temps laissé à l'application lancée pour ouvrir son port.
    pub attente_demarrage: Duration,
    /// Intervalle entre deux essais pendant le démarrage.
    pub pas: Duration,
    /// Après un démarrage raté, délai avant une nouvelle relance.
    pub repos_apres_echec: Duration,
}

impl Default for Delais {
    fn default() -> Self {
        Self {
            attente_demarrage: Duration::from_secs(30),
            pas: Duration::from_millis(300),
            repos_apres_echec: Duration::from_secs(20),
        }
    }
}

/// Ce que le pont attend de son environnement (réel : `mcp::stdio` ; tests : un faux).
pub trait Hote {
    /// Envoie une ligne JSON-RPC au serveur HTTP : (code HTTP, corps).
    fn poster(&mut self, ligne: &str) -> io::Result<(u16, String)>;
    /// Démarre l'application avec `--mcp`.
    fn lancer(&mut self) -> io::Result<()>;
    /// L'application lancée par le pont s'est-elle arrêtée ? Son statut si oui.
    fn lanceur_termine(&mut self) -> Option<String>;
    /// Avertissement visible (sortie d'erreur du pont).
    fn avertir(&mut self, message: &str);
    /// Pause entre deux essais.
    fn attendre(&mut self, duree: Duration) {
        std::thread::sleep(duree);
    }
}

pub struct Pont<H: Hote> {
    hote: H,
    port: u16,
    delais: Delais,
    /// Dernier démarrage raté : instant et message (pause avant de réessayer).
    echec: Option<(Instant, String)>,
}

impl<H: Hote> Pont<H> {
    pub fn nouveau(hote: H, port: u16, delais: Delais) -> Self {
        Self { hote, port, delais, echec: None }
    }

    /// Relaie une ligne JSON-RPC ; renvoie la ligne à écrire sur la sortie standard (aucune pour
    /// une notification acceptée, ou une erreur sans `id`, qui ne peut pas être renvoyée).
    pub fn relayer(&mut self, ligne: &str) -> Option<String> {
        let id = serde_json::from_str::<Value>(ligne).ok().and_then(|v| v.get("id").cloned());
        let resultat = match self.hote.poster(ligne) {
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => self.relancer(ligne),
            Err(e) => Err(self.avertissement(&format!(
                "le port {} est occupé par un programme qui ne répond pas comme un serveur MCP ({e}) ; \
                 aucune relance. Fermer ce programme, ou lancer le pont et l'application avec --mcp=AUTRE_PORT",
                self.port
            ))),
            Ok(r) => Ok(r),
        };
        match resultat {
            Ok((200, corps)) if !corps.trim().is_empty() => Some(corps),
            Ok((200 | 202, _)) => None,
            Ok((401, _)) => id.map(|id| erreur(&id, "jeton MCP refusé : vérifier --mcp-token / MCP_TOKEN")),
            Ok((code, corps)) => id.map(|id| {
                erreur(
                    &id,
                    &format!(
                        "HTTP {code} : {corps} (si ce n'est pas le serveur de ComicRust-Tauri, un autre \
                         programme occupe le port {} : utiliser --mcp=AUTRE_PORT)",
                        self.port
                    ),
                )
            }),
            Err(message) => id.map(|id| erreur(&id, &message)),
        }
    }

    /// Écrit l'avertissement sur la sortie d'erreur du pont et le renvoie (pour la réponse).
    fn avertissement(&mut self, message: &str) -> String {
        let texte = format!("pont MCP ComicRust-Tauri : {message}");
        self.hote.avertir(&texte);
        texte
    }

    /// Rien n'écoute : lance l'application (sauf pause après un échec récent) et attend son port.
    fn relancer(&mut self, ligne: &str) -> Result<(u16, String), String> {
        if let Some((quand, message)) = &self.echec {
            if quand.elapsed() < self.delais.repos_apres_echec {
                return Err(format!(
                    "{message} (nouvelle relance dans {} s)",
                    (self.delais.repos_apres_echec - quand.elapsed()).as_secs() + 1
                ));
            }
        }
        self.avertissement(&format!("application injoignable sur le port {}, lancement avec --mcp", self.port));
        if let Err(e) = self.hote.lancer() {
            return Err(self.echouer(format!("impossible de lancer l'application ({e})")));
        }
        let debut = Instant::now();
        let mut derniere = String::new();
        while debut.elapsed() < self.delais.attente_demarrage {
            self.hote.attendre(self.delais.pas);
            match self.hote.poster(ligne) {
                Ok(r) => {
                    self.echec = None;
                    return Ok(r);
                }
                Err(e) if e.kind() == ErrorKind::ConnectionRefused => derniere = e.to_string(),
                Err(e) => {
                    return Err(self.echouer(format!(
                        "le port {} est pris par un autre programme ({e}) : le serveur MCP de l'application n'a pas pu \
                         démarrer. Utiliser --mcp=AUTRE_PORT",
                        self.port
                    )))
                }
            }
            if let Some(statut) = self.hote.lanceur_termine() {
                return Err(self.echouer(format!(
                    "l'application s'est arrêtée ({statut}) avant d'ouvrir le port {} ; lancer \
                     ComicRust-Tauri-mcp.exe --mcp à la main pour voir pourquoi",
                    self.port
                )));
            }
        }
        Err(self.echouer(format!(
            "l'application n'a pas ouvert le port {} en {} s ({derniere}) : démarrage trop lent, ou port occupé \
             (le serveur MCP ne peut pas s'y ouvrir). Essayer --mcp=AUTRE_PORT",
            self.port,
            self.delais.attente_demarrage.as_secs()
        )))
    }

    /// Mémorise l'échec de démarrage (pause avant la relance suivante) et l'annonce.
    fn echouer(&mut self, message: String) -> String {
        let texte = self.avertissement(&message);
        self.echec = Some((Instant::now(), texte.clone()));
        texte
    }
}

/// Réponse JSON-RPC d'erreur.
pub fn erreur(id: &Value, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": message } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    type Reponse = io::Result<(u16, String)>;

    /// Hôte simulé : réponses de `poster` jouées dans l'ordre (la dernière se répète).
    #[derive(Default)]
    struct Faux {
        reponses: VecDeque<Reponse>,
        lancements: usize,
        /// Après un lancement, ces réponses remplacent les précédentes (le port « s'ouvre »).
        apres_lancement: VecDeque<Reponse>,
        termine: Option<String>,
        avertissements: Vec<String>,
        echec_lancement: bool,
    }

    fn refuse() -> Reponse {
        Err(io::Error::new(ErrorKind::ConnectionRefused, "connexion refusée"))
    }
    fn ok(corps: &str) -> Reponse {
        Ok((200, corps.to_string()))
    }
    fn copie(r: &Reponse) -> Reponse {
        match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
        }
    }

    impl Hote for Faux {
        fn poster(&mut self, _: &str) -> Reponse {
            if self.reponses.len() > 1 {
                self.reponses.pop_front().unwrap()
            } else {
                self.reponses.front().map(copie).unwrap_or_else(refuse)
            }
        }
        fn lancer(&mut self) -> io::Result<()> {
            self.lancements += 1;
            if self.echec_lancement {
                return Err(io::Error::new(ErrorKind::NotFound, "introuvable"));
            }
            if !self.apres_lancement.is_empty() {
                self.reponses = std::mem::take(&mut self.apres_lancement);
            }
            Ok(())
        }
        fn lanceur_termine(&mut self) -> Option<String> {
            self.termine.clone()
        }
        fn avertir(&mut self, message: &str) {
            self.avertissements.push(message.to_string());
        }
        fn attendre(&mut self, _: Duration) {}
    }

    fn rapide() -> Delais {
        Delais {
            attente_demarrage: Duration::from_millis(30),
            pas: Duration::ZERO,
            repos_apres_echec: Duration::from_millis(80),
        }
    }
    const REQ: &str = r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#;
    fn message(sortie: &str) -> String {
        serde_json::from_str::<Value>(sortie).unwrap()["error"]["message"].as_str().unwrap().to_string()
    }

    #[test]
    fn relaie_simplement_quand_le_serveur_repond() {
        let mut h = Faux::default();
        h.reponses.push_back(ok(r#"{"id":7,"result":{}}"#));
        let mut p = Pont::nouveau(h, 47802, rapide());
        assert_eq!(p.relayer(REQ).as_deref(), Some(r#"{"id":7,"result":{}}"#));
        assert_eq!(p.hote.lancements, 0);
        // Notification acceptée : aucune sortie.
        p.hote.reponses = VecDeque::from([Ok((202, String::new()))]);
        assert_eq!(p.relayer(r#"{"method":"notifications/initialized"}"#), None);
    }

    #[test]
    fn relance_l_application_a_chaque_fois_qu_elle_a_disparu() {
        let mut h = Faux::default();
        h.reponses.push_back(refuse());
        h.apres_lancement.push_back(ok("{\"id\":7}"));
        let mut p = Pont::nouveau(h, 47802, rapide());
        assert!(p.relayer(REQ).is_some());
        assert_eq!(p.hote.lancements, 1);
        // L'application est fermée par l'utilisateur : la requête suivante la relance (2e fois).
        p.hote.reponses = VecDeque::from([refuse()]);
        p.hote.apres_lancement.push_back(ok("{\"id\":7}"));
        assert_eq!(p.relayer(REQ).as_deref(), Some("{\"id\":7}"));
        assert_eq!(p.hote.lancements, 2, "relance multiple, pas une seule fois");
        assert!(p.hote.avertissements.iter().any(|a| a.contains("lancement avec --mcp")));
    }

    #[test]
    fn port_occupe_par_un_autre_programme_avertit_sans_relancer() {
        let mut h = Faux::default();
        h.reponses.push_back(Err(io::Error::new(ErrorKind::InvalidData, "réponse illisible")));
        let mut p = Pont::nouveau(h, 47802, rapide());
        let msg = message(&p.relayer(REQ).unwrap());
        assert!(msg.contains("47802") && msg.contains("occupé") && msg.contains("--mcp=AUTRE_PORT"), "{msg}");
        assert_eq!(p.hote.lancements, 0);
        assert_eq!(p.hote.avertissements.len(), 1, "avertissement aussi sur la sortie d'erreur");
        // Un autre serveur HTTP sur ce port (404) : message explicite aussi.
        p.hote.reponses = VecDeque::from([Ok((404, "introuvable".into()))]);
        let msg = message(&p.relayer(REQ).unwrap());
        assert!(msg.contains("HTTP 404") && msg.contains("occupe le port 47802"), "{msg}");
        // Jeton refusé.
        p.hote.reponses = VecDeque::from([Ok((401, String::new()))]);
        assert!(message(&p.relayer(REQ).unwrap()).contains("jeton MCP refusé"));
    }

    #[test]
    fn demarrage_sans_port_ouvert_puis_pause_avant_nouvelle_relance() {
        let mut h = Faux::default();
        h.reponses.push_back(refuse()); // le port reste fermé : l'application n'a pas pu s'y lier
        let mut p = Pont::nouveau(h, 47802, rapide());
        let msg = message(&p.relayer(REQ).unwrap());
        assert!(msg.contains("n'a pas ouvert le port 47802") && msg.contains("occupé"), "{msg}");
        assert_eq!(p.hote.lancements, 1);
        // Tout de suite après : pas de relance en boucle, le message est rappelé avec le délai.
        let msg = message(&p.relayer(REQ).unwrap());
        assert!(msg.contains("nouvelle relance dans"), "{msg}");
        assert_eq!(p.hote.lancements, 1);
        // La pause écoulée : nouvelle relance, qui réussit cette fois.
        std::thread::sleep(Duration::from_millis(100));
        p.hote.apres_lancement.push_back(ok("{\"id\":7}"));
        assert!(p.relayer(REQ).is_some());
        assert_eq!(p.hote.lancements, 2);
    }

    #[test]
    fn application_arretee_et_echec_de_lancement() {
        let mut h = Faux::default();
        h.reponses.push_back(refuse());
        h.termine = Some("code 1".into());
        let mut p = Pont::nouveau(h, 47802, rapide());
        let msg = message(&p.relayer(REQ).unwrap());
        assert!(msg.contains("s'est arrêtée (code 1)"), "{msg}");

        let mut h = Faux::default();
        h.reponses.push_back(refuse());
        h.echec_lancement = true;
        let mut p = Pont::nouveau(h, 47802, rapide());
        assert!(message(&p.relayer(REQ).unwrap()).contains("impossible de lancer l'application"));
        // Notification sans id : rien à renvoyer, mais l'avertissement reste visible sur stderr.
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(p.relayer(r#"{"method":"notifications/initialized"}"#), None);
        assert!(p.hote.avertissements.iter().any(|a| a.contains("impossible de lancer")));
    }
}
