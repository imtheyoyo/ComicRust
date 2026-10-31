//! Bilan des renommages **réellement appliqués** d'un script (v0.10.1).
//!
//! Un script ne fait que *demander* des renommages ; ComicRust les montre en aperçu puis
//! les applique (ou non) **après la fin du script**. Un script qui veut proposer
//! d'annuler un rangement (Library Organizer) ne peut donc pas savoir lui-même ce qui a été
//! fait. ComicRust enregistre, après chaque application qui a renommé au moins un fichier,
//! le bilan des renommages réussis dans `data\renommages\<script>.json` (un fichier par
//! fichier de script, remplacé à chaque application) ; le script le relit à l'exécution
//! suivante avec `comicrust.applied_renames()`.
//!
//! Format (JSON) : `{"script": "exemples/library_organizer.py", "entry": "library-organizer",
//! "function": "library_organizer", "date": "2026-10-31T10:20:30+01:00",
//! "renames": [{"book": "…", "from": "D:\\in\\a.cbz", "to": "D:\\BD\\a.cbz"}]}`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ScriptEntry;

/// Un renommage fait : livre, ancien et nouveau chemin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedRename {
    pub book: String,
    pub from: String,
    pub to: String,
}

/// Bilan d'une application (voir le [module](self)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedRenames {
    /// Clé des réglages du script (chemin relatif du fichier).
    pub script: String,
    /// Point d'entrée dont l'exécution a été appliquée (`#@Key`, sinon `fichier:fonction`).
    pub entry: String,
    pub function: String,
    /// Date de l'application (RFC 3339, heure locale).
    pub date: String,
    pub renames: Vec<AppliedRename>,
}

impl AppliedRenames {
    /// Bilan daté de maintenant.
    pub fn new(entry: &ScriptEntry, renames: Vec<AppliedRename>) -> AppliedRenames {
        AppliedRenames {
            script: entry.settings_key(),
            entry: entry.id(),
            function: entry.function.clone(),
            date: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            renames,
        }
    }

    /// Fichier du bilan d'un script dans `dir` : nom lisible (caractères sûrs) suivi d'une
    /// empreinte de la clé (deux clés voisines, `a/b.py` et `a_b.py`, ne se mélangent pas).
    pub fn file(dir: &Path, settings_key: &str) -> PathBuf {
        let safe: String = settings_key
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
            .take(80)
            .collect();
        // FNV-1a 32 bits : stable d'une version à l'autre (pas le hachage de std).
        let hash = settings_key.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
        dir.join(format!("{safe}-{hash:08x}.json"))
    }

    /// Enregistre le bilan (écriture dans un fichier temporaire puis remplacement : jamais
    /// de bilan à moitié écrit). Rend le chemin du fichier.
    pub fn save(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let file = Self::file(dir, &self.script);
        let tmp = file.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &file)?;
        Ok(file)
    }

    /// Relit le bilan d'un script (`None` : aucun, ou illisible).
    pub fn load(dir: &Path, settings_key: &str) -> Option<AppliedRenames> {
        let text = std::fs::read_to_string(Self::file(dir, settings_key)).ok()?;
        serde_json::from_str::<AppliedRenames>(&text).ok().filter(|b| b.script == settings_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entree() -> ScriptEntry {
        let code = "#@Name LO\n#@Hook Books\n#@Key lo\ndef ranger(books):\n    pass\n";
        super::super::parse_script(code, Path::new("/s/exemples/lo.py"), "exemples/lo.py").0.remove(0)
    }

    #[test]
    fn bilan_aller_retour_et_fichier_par_script() {
        let dir = tempfile::tempdir().unwrap();
        let r = AppliedRename { book: "a".into(), from: "D:\\in\\a.cbz".into(), to: "D:\\BD\\a | b.cbz".into() };
        let b = AppliedRenames::new(&entree(), vec![r]);
        assert_eq!((b.script.as_str(), b.entry.as_str(), b.function.as_str()), ("exemples/lo.py", "lo", "ranger"));
        let f = b.save(dir.path()).unwrap();
        assert!(f.file_name().unwrap().to_string_lossy().starts_with("exemples_lo.py-"));
        assert_eq!(AppliedRenames::load(dir.path(), "exemples/lo.py"), Some(b));
        assert_eq!(AppliedRenames::load(dir.path(), "exemples_lo.py"), None, "autre script : autre fichier");
        assert_ne!(AppliedRenames::file(dir.path(), "a/b.py"), AppliedRenames::file(dir.path(), "a_b.py"));
        // Aucun fichier temporaire laissé.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
