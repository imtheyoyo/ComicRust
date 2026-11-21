//! Rapport de plantage : `data\plantage.txt` (comme `install_crash_report` et `CRASH_FILE` de la
//! version egui). L'exécutable Windows n'a pas de console : en cas de panique, le message, le fil,
//! l'emplacement et la pile d'appels sont écrits dans ce fichier, puis montrés au lancement suivant
//! (fenêtre avec « Copier »), qui le supprime. Contrat : `docs/tauri/ipc-fenetre.md`.

use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Nom du fichier dans `data\`.
pub const FICHIER: &str = "plantage.txt";
/// Taille au-delà de laquelle on n'ajoute plus de rapport au fichier (rafale de paniques).
const TAILLE_MAX: u64 = 256 * 1024;
/// Séparateur entre deux rapports écrits pendant la même session.
const SEPARATEUR: &str = "\n────────────────────────────────────────\n";

/// Rapport lu au lancement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RapportPlantage {
    /// Texte complet (à copier dans le rapport de recette).
    pub texte: String,
    /// Les premières lignes : version, date, fil, message.
    pub resume: String,
    /// Chemin du fichier (supprimé quand la fenêtre est fermée).
    pub fichier: String,
}

/// Met en forme un rapport. `pile` : pile d'appels déjà rendue en texte.
pub fn formater_rapport(version: &str, horodatage: &str, fil: &str, message: &str, pile: &str) -> String {
    format!("ComicRust-Tauri v{version} — {horodatage}\nFil : {fil}\n{message}\n\n{pile}\n")
}

/// Chemin du fichier de rapport.
pub fn chemin(data: &Path) -> PathBuf {
    data.join(FICHIER)
}

/// Ajoute un rapport au fichier (il est créé s'il n'existe pas). Si le fichier contient déjà un
/// rapport de la session, le nouveau est ajouté à la suite : la première panique est souvent la
/// cause des suivantes.
pub fn ecrire_rapport(data: &Path, texte: &str) -> std::io::Result<()> {
    let p = chemin(data);
    let existant = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    if existant >= TAILLE_MAX {
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p)?;
    if existant > 0 {
        f.write_all(SEPARATEUR.as_bytes())?;
    }
    f.write_all(texte.as_bytes())
}

/// Installe le crochet de panique (à appeler tôt dans `run`, une fois `data\` connu). Le crochet
/// précédent (message sur la console de développement) est conservé.
pub fn installer(data: PathBuf) {
    let precedent = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let fil = std::thread::current();
        let texte = formater_rapport(
            env!("CARGO_PKG_VERSION"),
            &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            fil.name().unwrap_or("?"),
            &info.to_string(),
            &std::backtrace::Backtrace::force_capture().to_string(),
        );
        let _ = ecrire_rapport(&data, &texte);
        precedent(info);
    }));
}

/// Rapport de la session précédente, s'il y en a un (fichier absent ou vide : `None`).
pub fn lire(data: &Path) -> Option<RapportPlantage> {
    let p = chemin(data);
    let texte = std::fs::read(&p).ok().map(|o| String::from_utf8_lossy(&o).into_owned())?;
    if texte.trim().is_empty() {
        return None;
    }
    let resume = texte.lines().take(4).collect::<Vec<_>>().join("\n");
    Some(RapportPlantage { texte, resume, fichier: p.display().to_string() })
}

/// Supprime le rapport (l'utilisateur l'a vu). Déjà absent : succès.
pub fn supprimer(data: &Path) -> Result<(), String> {
    let p = chemin(data);
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{} : {e}", p.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_du_rapport() {
        let t = formater_rapport("0.11.0", "2026-11-21 10:00:00", "main", "panicked at src/x.rs:1:1:\nboum", "pile 1\npile 2");
        assert_eq!(
            t,
            "ComicRust-Tauri v0.11.0 — 2026-11-21 10:00:00\nFil : main\npanicked at src/x.rs:1:1:\nboum\n\npile 1\npile 2\n"
        );
    }

    #[test]
    fn ecriture_lecture_et_suppression() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(lire(d.path()), None);
        let a = formater_rapport("1", "t1", "main", "premier", "p");
        ecrire_rapport(d.path(), &a).unwrap();
        let r = lire(d.path()).unwrap();
        assert_eq!(r.texte, a);
        assert!(r.resume.starts_with("ComicRust-Tauri v1 — t1\nFil : main\npremier"));
        assert!(r.fichier.ends_with(FICHIER));
        // Deuxième panique de la même session : ajoutée à la suite.
        ecrire_rapport(d.path(), &formater_rapport("1", "t2", "fil", "second", "p")).unwrap();
        let r = lire(d.path()).unwrap();
        assert!(r.texte.starts_with(&a) && r.texte.contains("second") && r.texte.contains("────"));
        // Suppression : puis plus rien ; supprimer deux fois est sans erreur.
        supprimer(d.path()).unwrap();
        assert_eq!(lire(d.path()), None);
        supprimer(d.path()).unwrap();
    }

    #[test]
    fn fichier_vide_ou_enorme() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(chemin(d.path()), "  \n").unwrap();
        assert_eq!(lire(d.path()), None);
        std::fs::write(chemin(d.path()), vec![b'x'; usize::try_from(TAILLE_MAX).unwrap()]).unwrap();
        ecrire_rapport(d.path(), "ignoré").unwrap();
        assert!(!lire(d.path()).unwrap().texte.contains("ignoré"));
    }
}
