//! Garde des requêtes du serveur MCP local (`mcp::http`) : logique pure, testée sans WebView2.
//!
//! Le serveur écoute sur 127.0.0.1 ; le jeton reste **facultatif** (décision de Yoyo : débogage
//! sans jeton). Pour qu'une page web ouverte dans un navigateur ne puisse pas le piloter :
//! - origine `null` refusée (iframe `sandbox`, fichier local) : seules l'absence d'origine (client
//!   MCP) et les origines de la machine sont admises ;
//! - `Content-Type: application/json` exigé pour un POST : une requête « simple » du navigateur
//!   (`text/plain`, formulaire) part sans pré-vol CORS ; avec ce type, le navigateur doit d'abord
//!   envoyer un pré-vol `OPTIONS`, auquel le serveur ne répond pas par un
//!   `Access-Control-Allow-Origin` : la vraie requête n'est jamais envoyée ;
//! - en-tête `Host` limité à `127.0.0.1`, `localhost` ou `[::1]` (avec ou sans port) : parade au
//!   « DNS rebinding ».

use std::collections::HashMap;

/// Refus d'une requête : statut HTTP et message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refus {
    pub statut: &'static str,
    pub message: &'static str,
}

/// Origine acceptée : absente, ou provenant de la machine elle-même (`null` refusée).
pub fn origine_ok(origine: Option<&str>) -> bool {
    let Some(o) = origine else { return true };
    let o = o.trim().to_ascii_lowercase();
    ["http://127.0.0.1", "http://localhost", "https://127.0.0.1", "https://localhost", "http://[::1]"]
        .iter()
        .any(|p| o == *p || o.strip_prefix(p).and_then(|r| r.strip_prefix(':')).is_some_and(port_valide))
}

fn port_valide(p: &str) -> bool {
    !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit())
}

/// En-tête `Host` accepté : `127.0.0.1`, `localhost` ou `[::1]`, port facultatif. Absent : refusé
/// (HTTP/1.1 l'impose ; le client de la passerelle stdio l'envoie).
pub fn hote_ok(hote: Option<&str>) -> bool {
    let Some(h) = hote else { return false };
    let h = h.trim().to_ascii_lowercase();
    let (nom, port) = if let Some(reste) = h.strip_prefix("[::1]") {
        ("[::1]", reste)
    } else {
        match h.rsplit_once(':') {
            // `port` garde le « : » initial.
            Some((n, _)) => (n, &h[n.len()..]),
            None => (h.as_str(), ""),
        }
    };
    matches!(nom, "127.0.0.1" | "localhost" | "[::1]")
        && (port.is_empty() || port.strip_prefix(':').is_some_and(port_valide))
}

/// `Content-Type` d'un POST : `application/json` (paramètres comme `; charset=utf-8` admis).
pub fn type_json(content_type: Option<&str>) -> bool {
    content_type.and_then(|c| c.split(';').next()).is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
}

/// Contrôle d'une requête (en-têtes en minuscules) avant tout traitement : origine, hôte, jeton
/// (s'il est configuré), type du corps pour un POST. `Ok(())` : la requête peut être traitée.
pub fn controler(methode: &str, entetes: &HashMap<String, String>, jeton: Option<&str>) -> Result<(), Refus> {
    let entete = |k: &str| entetes.get(k).map(String::as_str);
    if !origine_ok(entete("origin")) {
        return Err(Refus { statut: "403 Forbidden", message: "origine refusee" });
    }
    if !hote_ok(entete("host")) {
        return Err(Refus { statut: "403 Forbidden", message: "hote refuse (127.0.0.1 ou localhost)" });
    }
    if let Some(jeton) = jeton {
        let porteur =
            entete("authorization").and_then(|a| a.strip_prefix("Bearer ").or_else(|| a.strip_prefix("bearer ")));
        if porteur != Some(jeton) && entete("x-mcp-token") != Some(jeton) {
            return Err(Refus { statut: "401 Unauthorized", message: "jeton manquant ou invalide" });
        }
    }
    if methode == "POST" && !type_json(entete("content-type")) {
        return Err(Refus { statut: "415 Unsupported Media Type", message: "Content-Type application/json attendu" });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entetes(paires: &[(&str, &str)]) -> HashMap<String, String> {
        paires.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn origines() {
        assert!(origine_ok(None));
        assert!(origine_ok(Some("http://localhost:3000")));
        assert!(origine_ok(Some("http://127.0.0.1")));
        assert!(!origine_ok(Some("null")), "iframe sandbox : refusée");
        assert!(!origine_ok(Some("NULL")));
        assert!(!origine_ok(Some("http://localhost.evil.com")));
        assert!(!origine_ok(Some("http://localhost:3000.evil.com")));
        assert!(!origine_ok(Some("https://example.org")));
    }

    #[test]
    fn hotes() {
        for h in ["127.0.0.1", "127.0.0.1:47802", "localhost", "LOCALHOST:1", "[::1]", "[::1]:47802"] {
            assert!(hote_ok(Some(h)), "{h}");
        }
        for h in ["evil.com", "evil.com:47802", "127.0.0.1.evil.com", "localhost:abc", "[::1]x", ""] {
            assert!(!hote_ok(Some(h)), "{h}");
        }
        assert!(!hote_ok(None));
    }

    #[test]
    fn requete_simple_du_navigateur_refusee() {
        // Preuve de la relecture : Origin null + text/plain déclenchait un outil.
        let e = entetes(&[("origin", "null"), ("host", "127.0.0.1:47802"), ("content-type", "text/plain")]);
        assert_eq!(controler("POST", &e, None).unwrap_err().statut, "403 Forbidden");
        // Sans origine (page « no-cors » qui la masquerait) mais en text/plain : 415.
        let e = entetes(&[("host", "127.0.0.1:47802"), ("content-type", "text/plain;charset=UTF-8")]);
        assert_eq!(controler("POST", &e, None).unwrap_err().statut, "415 Unsupported Media Type");
        // DNS rebinding : hôte étranger.
        let e = entetes(&[("host", "evil.example:47802"), ("content-type", "application/json")]);
        assert_eq!(controler("POST", &e, None).unwrap_err().statut, "403 Forbidden");
    }

    #[test]
    fn client_mcp_accepte_avec_ou_sans_jeton() {
        let e = entetes(&[("host", "127.0.0.1:47802"), ("content-type", "application/json; charset=utf-8")]);
        assert!(controler("POST", &e, None).is_ok(), "jeton facultatif (décision de Yoyo)");
        assert_eq!(controler("POST", &e, Some("s3cret")).unwrap_err().statut, "401 Unauthorized");
        let mut e2 = e.clone();
        e2.insert("authorization".into(), "Bearer s3cret".into());
        assert!(controler("POST", &e2, Some("s3cret")).is_ok());
        // OPTIONS / DELETE : pas de corps, pas d'exigence de type.
        assert!(controler("OPTIONS", &entetes(&[("host", "localhost")]), None).is_ok());
    }
}
