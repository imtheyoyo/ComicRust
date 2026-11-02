//! Réglages de la version Tauri : fichier JSON `data\comicrust-tauri.json`.
//!
//! Distinct du `comicrust.ron` de la version egui (formats et besoins différents ; les deux
//! versions peuvent cohabiter dans le même dossier). Aucun mot de passe n'est écrit ici : en
//! mode ComicRack, les paramètres MySQL sont relus à chaque ouverture dans `ComicRack.ini`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Nom du fichier de réglages dans `data\`.
pub const FICHIER_REGLAGES: &str = "comicrust-tauri.json";

/// Bibliothèque utilisée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Base propre à ComicRust (SQLite `data\comicrust.db`) et dossiers surveillés.
    Autonome,
    /// Lié à ComicRack, en lecture seule (MySQL du `ComicRack.ini`, ou `ComicDb.xml`).
    Comicrack,
}

/// Contenu du fichier de réglages.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Reglages {
    /// `None` : premier démarrage, rien n'a encore été choisi.
    pub mode: Option<Mode>,
    /// Mode ComicRack : `ComicRack.ini` (MySQL) ou `ComicDb.xml`.
    pub chemin_ini: Option<PathBuf>,
}

/// Chemin du fichier de réglages.
pub fn chemin_reglages(data: &Path) -> PathBuf {
    data.join(FICHIER_REGLAGES)
}

/// Lit les réglages ; fichier absent = réglages vides (premier démarrage). Un fichier
/// illisible est une erreur (on n'écrase pas en silence ce que l'utilisateur avait choisi).
pub fn lire(data: &Path) -> Result<Reglages, String> {
    let p = chemin_reglages(data);
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(t.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("réglages illisibles ({}) : {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Reglages::default()),
        Err(e) => Err(format!("réglages illisibles ({}) : {e}", p.display())),
    }
}

/// Écrit les réglages (fichier temporaire puis renommage : jamais à moitié écrit).
pub fn ecrire(data: &Path, r: &Reglages) -> Result<(), String> {
    let p = chemin_reglages(data);
    let tmp = p.with_extension("json.tmp");
    let texte = serde_json::to_string_pretty(r).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(data).map_err(|e| format!("{} : {e}", data.display()))?;
    std::fs::write(&tmp, texte).map_err(|e| format!("{} : {e}", tmp.display()))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("{} : {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_puis_aller_retour() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(lire(d.path()).unwrap(), Reglages::default());
        let r = Reglages { mode: Some(Mode::Comicrack), chemin_ini: Some("C:/CR/ComicRack.ini".into()) };
        ecrire(d.path(), &r).unwrap();
        assert_eq!(lire(d.path()).unwrap(), r);
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(texte.contains("\"mode\": \"comicrack\"") && texte.contains("cheminIni"), "{texte}");
    }

    #[test]
    fn fichier_corrompu_est_une_erreur() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(chemin_reglages(d.path()), "{pas du json").unwrap();
        assert!(lire(d.path()).is_err());
    }
}
