//! Paramètres de connexion à la base MySQL de ComicRack.
//!
//! ComicRack CE les garde dans `ComicRack.ini`, sur une ligne de la forme :
//!
//! ```text
//! DataSource=mysql:Server=192.168.1.10; Port=3306; Uid=comicrack; Pwd=secret; Database=comicrack;
//! ```
//!
//! La partie après `mysql:` est une chaîne de connexion MySqlConnector : paires
//! `clé=valeur` séparées par `;`, clés sans casse avec plusieurs synonymes, valeurs
//! éventuellement entre guillemets (simples ou doubles, doublés pour s'échapper).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self { host: String::new(), port: 3306, user: String::new(), password: String::new(), database: String::new() }
    }
}

// Jamais de mot de passe dans les journaux ni les messages d'erreur.
impl std::fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &if self.password.is_empty() { "" } else { "***" })
            .field("database", &self.database)
            .finish()
    }
}

impl DbConfig {
    /// « utilisateur@serveur:port/base », sans mot de passe (affichage).
    pub fn describe(&self) -> String {
        format!("{}@{}:{}/{}", self.user, self.host, self.port, self.database)
    }

    pub fn is_complete(&self) -> bool {
        !self.host.is_empty() && !self.user.is_empty() && !self.database.is_empty()
    }

    /// Lit une chaîne de connexion MySqlConnector (avec ou sans préfixe `mysql:`).
    pub fn from_connection_string(s: &str) -> Result<Self, Error> {
        let s = s.trim();
        let s = s.strip_prefix("mysql:").or_else(|| s.strip_prefix("MySQL:")).unwrap_or(s);
        let mut cfg = DbConfig::default();
        for (key, value) in split_pairs(s) {
            let key: String = key.to_ascii_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
            match key.as_str() {
                "server" | "host" | "datasource" | "address" | "addr" | "networkaddress" => cfg.host = value,
                "port" => {
                    cfg.port = value.parse().map_err(|_| Error::Database(format!("port invalide : {value}")))?;
                }
                "uid" | "user" | "userid" | "username" => cfg.user = value,
                "pwd" | "password" => cfg.password = value,
                "database" | "initialcatalog" => cfg.database = value,
                _ => {} // délais, SSL, pool… : options gérées par défaut
            }
        }
        if !cfg.is_complete() {
            return Err(Error::Database(
                "chaîne de connexion incomplète (serveur, utilisateur et base sont nécessaires)".into(),
            ));
        }
        Ok(cfg)
    }

    /// Cherche la connexion MySQL dans le contenu d'un `ComicRack.ini`.
    pub fn from_comicrack_ini_text(text: &str) -> Result<Self, Error> {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim();
                if key.trim().eq_ignore_ascii_case("DataSource") && value.to_ascii_lowercase().starts_with("mysql:") {
                    return Self::from_connection_string(value);
                }
            }
        }
        Err(Error::Database("aucune ligne « DataSource=mysql:… » dans ce fichier".into()))
    }

    pub fn from_comicrack_ini(path: &Path) -> Result<Self, Error> {
        let bytes = std::fs::read(path)?;
        // ComicRack.ini peut être en UTF-8 (avec BOM) ou en UTF-16.
        let text = if bytes.starts_with(&[0xFF, 0xFE]) {
            let u16s: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&u16s)
        } else {
            String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string()
        };
        Self::from_comicrack_ini_text(&text)
    }
}

/// Découpe `a=1; b='x;y'; c="z"` en paires (gère les guillemets doublés).
fn split_pairs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // clé
        let start = i;
        while i < chars.len() && chars[i] != '=' && chars[i] != ';' {
            i += 1;
        }
        let key: String = chars[start..i].iter().collect::<String>().trim().to_string();
        if i >= chars.len() || chars[i] == ';' {
            i += 1;
            continue;
        }
        i += 1; // '='
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        let mut value = String::new();
        if i < chars.len() && (chars[i] == '\'' || chars[i] == '"') {
            let q = chars[i];
            i += 1;
            while i < chars.len() {
                if chars[i] == q {
                    if i + 1 < chars.len() && chars[i + 1] == q {
                        value.push(q);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                value.push(chars[i]);
                i += 1;
            }
            while i < chars.len() && chars[i] != ';' {
                i += 1;
            }
        } else {
            while i < chars.len() && chars[i] != ';' {
                value.push(chars[i]);
                i += 1;
            }
            value = value.trim().to_string();
        }
        i += 1; // ';'
        if !key.is_empty() {
            out.push((key, value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comicrack_ini_line() {
        // Même forme que le ComicRack.ini de Yoyo (valeurs fictives).
        let ini = "; réglages\r\nDataSource=mysql:Server=192.168.1.20;  Port=1236;  Uid=lecteur; Pwd=P@s;s0\"rd; \
                   Database=comics_test; default command timeout=6000;\r\nOther=1\r\n";
        // Un ';' non protégé coupe la valeur : comme MySqlConnector, on lit « P@s ».
        let cfg = DbConfig::from_comicrack_ini_text(ini).unwrap();
        assert_eq!(cfg.host, "192.168.1.20");
        assert_eq!(cfg.port, 1236);
        assert_eq!(cfg.user, "lecteur");
        assert_eq!(cfg.password, "P@s");
        assert_eq!(cfg.database, "comics_test");
        assert!(!format!("{cfg:?}").contains("P@s"));
    }

    #[test]
    fn quoted_values_and_synonyms() {
        let cfg =
            DbConfig::from_connection_string("mysql:Host=nas; User Id=yo; Password='a;b''c'; Initial Catalog=\"base\"")
                .unwrap();
        assert_eq!((cfg.host.as_str(), cfg.port, cfg.user.as_str()), ("nas", 3306, "yo"));
        assert_eq!(cfg.password, "a;b'c");
        assert_eq!(cfg.database, "base");
        assert_eq!(cfg.describe(), "yo@nas:3306/base");
    }

    #[test]
    fn missing_parts_are_reported() {
        assert!(DbConfig::from_connection_string("Server=x; Port=12").is_err());
        assert!(DbConfig::from_comicrack_ini_text("DataSource=C:\\ComicDB.xml").is_err());
    }
}
