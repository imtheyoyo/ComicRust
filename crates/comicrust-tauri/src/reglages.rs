//! Réglages de la version Tauri : fichier JSON `data\comicrust-tauri.json`.
//!
//! Distinct du `comicrust.ron` de la version egui (formats et besoins différents ; les deux
//! versions peuvent cohabiter dans le même dossier). Aucun mot de passe n'est écrit ici : en
//! mode ComicRack, les paramètres MySQL sont relus à chaque ouverture dans `ComicRack.ini`.
//!
//! La section `vue` mémorise l'affichage (tri, groupe, grille/tableau, taille des couvertures,
//! panneau de détails, thème) : mode portable strict, rien dans le `localStorage` de la WebView.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

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
    /// Préférences d'affichage (absentes = valeurs par défaut du front).
    #[serde(skip_serializing_if = "PreferencesVue::est_vide")]
    pub vue: PreferencesVue,
}

/// Clés de tri connues du front (`ClefTri` de `frontend/src/lib/api.ts`).
pub const TRIS: &[&str] =
    &["serie", "titre", "parution", "ajout", "lecture", "note", "pages", "avancement", "taille", "fichier"];
/// Sens de tri.
pub const SENS: &[&str] = &["asc", "desc"];
/// Regroupements.
pub const GROUPES: &[&str] = &["aucun", "serie", "editeur"];
/// Types de vue de la bibliothèque.
pub const VUES: &[&str] = &["grille", "tableau"];
/// Bornes de la taille des couvertures (px, curseur « Taille »).
pub const ZOOM: (u32, u32) = (100, 260);
/// Bornes de la largeur du panneau de détails (px).
pub const LARGEUR_DETAILS: (u32, u32) = (220, 800);
/// Modes du lecteur (`ModeLecture` de `frontend/src/lib/api.ts`).
pub const MODES_LECTURE: &[&str] = &["simple", "double", "defilement"];
/// Ajustements de la page (`Ajustement` de `frontend/src/lib/api.ts`).
pub const AJUSTEMENTS: &[&str] = &["meilleur", "largeur", "hauteur", "reelle"];
/// Choix de la page de reprise quand la bibliothèque et ce poste diffèrent (`ChoixReprise`).
pub const REPRISES: &[&str] = &["demander", "comicrack", "comicrust"];
/// Bornes du grossissement de la loupe.
pub const GROSSISSEMENT_LOUPE: (f64, f64) = (1.5, 6.0);

/// Préférences d'affichage (section `vue`). Chaque champ absent garde la valeur par défaut du
/// front. **Lecture tolérante** : une valeur inconnue ou d'un mauvais type (fichier modifié à la
/// main, version plus récente) est ignorée champ par champ, sans rendre le fichier illisible ;
/// les nombres hors bornes sont ramenés dans les bornes.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesVue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sens_tri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groupe: Option<String>,
    /// `grille` ou `tableau`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vue: Option<String>,
    /// Taille des couvertures (px).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<u32>,
    /// Panneau de détails affiché.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<bool>,
    /// Largeur du panneau de détails (px).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largeur_details: Option<u32>,
    /// Identifiant du thème (nom du fichier CSS de `data\themes`, sans extension). Un thème
    /// disparu est remplacé par le thème par défaut côté front.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Réglages du lecteur (sous-objet `lecteur`, remplacé en entier à chaque enregistrement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lecteur: Option<PreferencesLecteur>,
}

/// Réglages du lecteur (section `vue.lecteur`, `PreferencesLecteur` de `frontend/src/lib/api.ts`).
/// Même lecture tolérante que [`PreferencesVue`] : champ invalide ignoré, nombre ramené dans ses
/// bornes.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesLecteur {
    /// `simple`, `double` ou `defilement`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// `meilleur`, `largeur`, `hauteur` ou `reelle`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ajustement: Option<String>,
    /// Double page : couverture affichée seule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub couverture_seule: Option<bool>,
    /// Sens manga pour les livres non marqués manga.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manga: Option<bool>,
    /// Bandeau de miniatures affiché.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub miniatures: Option<bool>,
    /// Grossissement de la loupe (1,5 à 6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grossissement_loupe: Option<f64>,
    /// `demander`, `comicrack` ou `comicrust` (réglage « Reprise » de la version egui).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reprise: Option<String>,
}

impl PreferencesLecteur {
    /// Construit les réglages depuis du JSON quelconque ; `None` si ce n'est pas un objet.
    pub fn depuis_json(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let parmi = |clef: &str, permis: &[&str]| {
            o.get(clef).and_then(Value::as_str).filter(|s| permis.contains(s)).map(str::to_string)
        };
        let booleen = |clef: &str| o.get(clef).and_then(Value::as_bool);
        let (min, max) = GROSSISSEMENT_LOUPE;
        Some(Self {
            mode: parmi("mode", MODES_LECTURE),
            ajustement: parmi("ajustement", AJUSTEMENTS),
            couverture_seule: booleen("couvertureSeule"),
            manga: booleen("manga"),
            miniatures: booleen("miniatures"),
            grossissement_loupe: o
                .get("grossissementLoupe")
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
                .map(|n| n.clamp(min, max)),
            reprise: parmi("reprise", REPRISES),
        })
    }
}

impl PreferencesVue {
    /// Aucune préférence enregistrée.
    pub fn est_vide(&self) -> bool {
        *self == Self::default()
    }

    /// Construit les préférences depuis du JSON quelconque, en ignorant ce qui n'est pas valide.
    pub fn depuis_json(v: &Value) -> Self {
        let Some(o) = v.as_object() else { return Self::default() };
        let parmi = |clef: &str, permis: &[&str]| {
            o.get(clef).and_then(Value::as_str).filter(|s| permis.contains(s)).map(str::to_string)
        };
        let borne = |clef: &str, (min, max): (u32, u32)| {
            o.get(clef)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
                .map(|n| n.round().clamp(f64::from(min), f64::from(max)) as u32)
        };
        Self {
            tri: parmi("tri", TRIS),
            sens_tri: parmi("sensTri", SENS),
            groupe: parmi("groupe", GROUPES),
            vue: parmi("vue", VUES),
            zoom: borne("zoom", ZOOM),
            details: o.get("details").and_then(Value::as_bool),
            largeur_details: borne("largeurDetails", LARGEUR_DETAILS),
            theme: o.get("theme").and_then(Value::as_str).filter(|s| id_theme_valide(s)).map(str::to_string),
            lecteur: o.get("lecteur").and_then(PreferencesLecteur::depuis_json),
        }
    }

    /// Applique les champs présents de `m` (les absents gardent la valeur actuelle).
    pub fn fusionner(&mut self, m: PreferencesVue) {
        macro_rules! prendre {
            ($($c:ident),*) => { $( if m.$c.is_some() { self.$c = m.$c; } )* };
        }
        prendre!(tri, sens_tri, groupe, vue, zoom, details, largeur_details, theme, lecteur);
    }
}

/// Identifiant de thème plausible (nom de fichier simple, sans chemin).
fn id_theme_valide(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        && !s.starts_with('.')
}

impl<'de> Deserialize<'de> for PreferencesVue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::depuis_json(&Value::deserialize(d)?))
    }
}

/// Sérialise les lecture-modification-écriture du fichier (commandes IPC concurrentes).
static VERROU: Mutex<()> = Mutex::new(());

fn verrou() -> MutexGuard<'static, ()> {
    VERROU.lock().unwrap_or_else(|e| e.into_inner())
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

/// Lit, modifie puis réécrit les réglages (atomique, sous verrou) : chaque appelant ne change
/// que sa partie (le mode ne perd pas les préférences d'affichage, et inversement).
pub fn modifier<T>(data: &Path, f: impl FnOnce(&mut Reglages) -> T) -> Result<T, String> {
    let _v = verrou();
    let mut r = lire(data)?;
    let resultat = f(&mut r);
    ecrire(data, &r)?;
    Ok(resultat)
}

/// Préférences d'affichage enregistrées (vides si le fichier n'existe pas encore).
pub fn lire_preferences_vue(data: &Path) -> Result<PreferencesVue, String> {
    Ok(lire(data)?.vue)
}

/// Fusionne `m` dans les préférences enregistrées ; renvoie les préférences à jour.
pub fn enregistrer_preferences_vue(data: &Path, m: PreferencesVue) -> Result<PreferencesVue, String> {
    modifier(data, |r| {
        r.vue.fusionner(m);
        r.vue.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_puis_aller_retour() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(lire(d.path()).unwrap(), Reglages::default());
        let r = Reglages {
            mode: Some(Mode::Comicrack),
            chemin_ini: Some("C:/CR/ComicRack.ini".into()),
            vue: PreferencesVue::default(),
        };
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

    #[test]
    fn preferences_vue_aller_retour_et_fusion() {
        let d = tempfile::tempdir().unwrap();
        assert!(lire_preferences_vue(d.path()).unwrap().est_vide());
        let p = PreferencesVue {
            tri: Some("parution".into()),
            sens_tri: Some("desc".into()),
            groupe: Some("editeur".into()),
            vue: Some("tableau".into()),
            zoom: Some(200),
            details: Some(false),
            largeur_details: Some(400),
            theme: Some("moderne-sombre".into()),
            lecteur: None,
        };
        assert_eq!(enregistrer_preferences_vue(d.path(), p.clone()).unwrap(), p);
        assert_eq!(lire_preferences_vue(d.path()).unwrap(), p);
        // Fusion : seul le champ fourni change.
        let m = PreferencesVue { groupe: Some("serie".into()), ..Default::default() };
        let apres = enregistrer_preferences_vue(d.path(), m).unwrap();
        assert_eq!(apres.groupe.as_deref(), Some("serie"));
        assert_eq!(apres.tri.as_deref(), Some("parution"));
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(
            texte.contains("\"vue\"") && texte.contains("\"sensTri\": \"desc\"") && texte.contains("largeurDetails"),
            "{texte}"
        );
        assert!(!d.path().join("comicrust-tauri.json.tmp").exists());
    }

    #[test]
    fn mode_et_preferences_se_conservent_mutuellement() {
        let d = tempfile::tempdir().unwrap();
        let m = PreferencesVue { tri: Some("titre".into()), ..Default::default() };
        enregistrer_preferences_vue(d.path(), m).unwrap();
        modifier(d.path(), |r| r.mode = Some(Mode::Autonome)).unwrap();
        let r = lire(d.path()).unwrap();
        assert_eq!(r.mode, Some(Mode::Autonome));
        assert_eq!(r.vue.tri.as_deref(), Some("titre"));
    }

    #[test]
    fn valeurs_inconnues_ignorees() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            chemin_reglages(d.path()),
            r#"{"mode":"autonome","inconnu":1,"vue":{"tri":"bidule","sensTri":"desc","groupe":3,
               "vue":"mosaique","zoom":9999,"details":"oui","largeurDetails":-5,"theme":"../x","futur":true}}"#,
        )
        .unwrap();
        let r = lire(d.path()).unwrap();
        assert_eq!(r.mode, Some(Mode::Autonome));
        assert_eq!(
            r.vue,
            PreferencesVue {
                sens_tri: Some("desc".into()),
                zoom: Some(ZOOM.1),
                largeur_details: Some(LARGEUR_DETAILS.0),
                ..Default::default()
            }
        );
        // Section `vue` d'un type inattendu : ignorée, le reste du fichier reste lisible.
        std::fs::write(chemin_reglages(d.path()), r#"{"mode":"autonome","vue":"grille"}"#).unwrap();
        let r = lire(d.path()).unwrap();
        assert_eq!(r.mode, Some(Mode::Autonome));
        assert!(r.vue.est_vide());
    }

    #[test]
    fn preferences_lecteur_conservees_et_validees() {
        let d = tempfile::tempdir().unwrap();
        enregistrer_preferences_vue(d.path(), PreferencesVue { tri: Some("titre".into()), ..Default::default() })
            .unwrap();
        // Ce qu'envoie le front (prefs.ts : sous-objet complet).
        let json: Value = serde_json::from_str(
            r#"{"lecteur":{"mode":"double","ajustement":"largeur","couvertureSeule":false,"manga":true,
               "miniatures":true,"grossissementLoupe":3.5,"reprise":"comicrack"}}"#,
        )
        .unwrap();
        let apres = enregistrer_preferences_vue(d.path(), PreferencesVue::depuis_json(&json)).unwrap();
        let attendu = PreferencesLecteur {
            mode: Some("double".into()),
            ajustement: Some("largeur".into()),
            couverture_seule: Some(false),
            manga: Some(true),
            miniatures: Some(true),
            grossissement_loupe: Some(3.5),
            reprise: Some("comicrack".into()),
        };
        assert_eq!(apres.lecteur.as_ref(), Some(&attendu));
        assert_eq!(apres.tri.as_deref(), Some("titre"));
        // Relu après redémarrage ; un autre champ de `vue` ne touche pas au lecteur.
        enregistrer_preferences_vue(d.path(), PreferencesVue { zoom: Some(150), ..Default::default() }).unwrap();
        assert_eq!(lire_preferences_vue(d.path()).unwrap().lecteur, Some(attendu));
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(texte.contains("\"couvertureSeule\": false") && texte.contains("grossissementLoupe"), "{texte}");
        // Le sous-objet est remplacé en entier.
        let json: Value = serde_json::from_str(r#"{"lecteur":{"mode":"defilement"}}"#).unwrap();
        let apres = enregistrer_preferences_vue(d.path(), PreferencesVue::depuis_json(&json)).unwrap();
        assert_eq!(apres.lecteur, Some(PreferencesLecteur { mode: Some("defilement".into()), ..Default::default() }));
    }

    #[test]
    fn preferences_lecteur_invalides_ignorees() {
        let json: Value = serde_json::from_str(
            r#"{"lecteur":{"mode":"triple","ajustement":4,"couvertureSeule":"oui","manga":true,
               "miniatures":null,"grossissementLoupe":42,"reprise":"jamais","futur":1}}"#,
        )
        .unwrap();
        let p = PreferencesVue::depuis_json(&json);
        assert_eq!(
            p.lecteur,
            Some(PreferencesLecteur {
                manga: Some(true),
                grossissement_loupe: Some(GROSSISSEMENT_LOUPE.1),
                ..Default::default()
            })
        );
        let json: Value = serde_json::from_str(r#"{"lecteur":"double","tri":"titre"}"#).unwrap();
        let p = PreferencesVue::depuis_json(&json);
        assert_eq!(p.lecteur, None);
        assert_eq!(p.tri.as_deref(), Some("titre"));
    }
}
