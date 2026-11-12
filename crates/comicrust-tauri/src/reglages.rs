//! Réglages de la version Tauri : fichier JSON `data\comicrust-tauri.json`.
//!
//! Distinct du `comicrust.ron` de la version egui (formats et besoins différents ; les deux
//! versions peuvent cohabiter dans le même dossier). Un mot de passe MySQL n'est **jamais écrit en
//! clair** : en mode ComicRack avec `ComicRack.ini`, il reste dans ce fichier (relu à chaque
//! ouverture) ; les connexions saisies dans ComicRust (`storeMysql`, `comicrackMysql`) passent par
//! `comicrust_core::secret` (DPAPI sous Windows ; ailleurs, aucun coffre : mot de passe non gardé).
//!
//! La section `vue` mémorise l'affichage (tri, groupe, grille/tableau, taille des couvertures,
//! panneau de détails, thème) : mode portable strict, rien dans le `localStorage` de la WebView.

use comicrust_core::db::DbConfig;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
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
    /// Mode autonome : la base de ComicRust est MySQL (absent = SQLite `data\comicrust.db`).
    /// Mot de passe chiffré ; lecture tolérante (entrée invalide = ignorée).
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "connexion_tolerante")]
    pub store_mysql: Option<DbConfig>,
    /// Mode ComicRack saisi à la main (sans `ComicRack.ini`) : MySQL de ComicRack. Utilisé quand
    /// `chemin_ini` est absent. Mot de passe chiffré.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "connexion_tolerante")]
    pub comicrack_mysql: Option<DbConfig>,
    /// Préférences d'affichage (absentes = valeurs par défaut du front).
    #[serde(skip_serializing_if = "PreferencesVue::est_vide")]
    pub vue: PreferencesVue,
    /// Réglages de l'édition (infos, fichiers, écriture dans ComicRack).
    #[serde(skip_serializing_if = "ReglagesEdition::est_vide")]
    pub edition: ReglagesEdition,
}

/// Lit une connexion MySQL enregistrée depuis du JSON quelconque (champs `host`, `port`, `user`,
/// `password` chiffré, `database`) ; `None` si ce n'est pas un objet ou si elle est incomplète.
/// Le mot de passe illisible (autre PC, autre compte Windows) devient vide : il sera redemandé.
fn connexion_tolerante<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DbConfig>, D::Error> {
    let v = Value::deserialize(d)?;
    let Some(o) = v.as_object() else { return Ok(None) };
    let texte = |c: &str| o.get(c).and_then(Value::as_str).filter(|s| s.len() <= 1024).unwrap_or("").to_string();
    let cfg = DbConfig {
        host: texte("host"),
        port: o.get("port").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()).unwrap_or(3306),
        user: texte("user"),
        password: comicrust_core::secret::unprotect(&texte("password")),
        database: texte("database"),
    };
    Ok(cfg.is_complete().then_some(cfg))
}

/// Réglages de l'édition (section `edition`, `ReglagesEdition` de `docs/tauri/ipc-edition.md`).
///
/// `ecriture_comicrack` est l'option « Lecture et écriture » du mode lié de la version egui
/// (`Settings::comicrack_write`, faux par défaut : lecture seule) ; `ecrire_dans_fichier` la case
/// « Écrire aussi les infos dans le fichier » (`info_write_file`) ; `ranger_original` le choix
/// « ranger l'original dans _originaux » (`move_converted_original`). Lecture tolérante, comme
/// [`PreferencesVue`].
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesEdition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecriture_comicrack: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecrire_dans_fichier: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranger_original: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modele_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dossier_cible: Option<String>,
}

impl ReglagesEdition {
    pub fn est_vide(&self) -> bool {
        *self == Self::default()
    }

    /// Depuis du JSON quelconque (champ invalide ignoré).
    pub fn depuis_json(v: &Value) -> Self {
        let Some(o) = v.as_object() else { return Self::default() };
        let booleen = |c: &str| o.get(c).and_then(Value::as_bool);
        let texte = |c: &str| o.get(c).and_then(Value::as_str).filter(|s| s.len() <= 4096).map(str::to_string);
        Self {
            ecriture_comicrack: booleen("ecritureComicrack"),
            ecrire_dans_fichier: booleen("ecrireDansFichier"),
            ranger_original: booleen("rangerOriginal"),
            modele_nom: texte("modeleNom"),
            dossier_cible: texte("dossierCible"),
        }
    }

    /// Applique les champs présents de `m`.
    pub fn fusionner(&mut self, m: ReglagesEdition) {
        macro_rules! prendre {
            ($($c:ident),*) => { $( if m.$c.is_some() { self.$c = m.$c; } )* };
        }
        prendre!(ecriture_comicrack, ecrire_dans_fichier, ranger_original, modele_nom, dossier_cible);
    }
}

impl<'de> Deserialize<'de> for ReglagesEdition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::depuis_json(&Value::deserialize(d)?))
    }
}

/// Réglages de l'édition vus par le front : valeurs par défaut remplies.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesEditionComplets {
    pub ecriture_comicrack: bool,
    pub ecrire_dans_fichier: bool,
    pub ranger_original: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modele_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dossier_cible: Option<String>,
}

impl From<&ReglagesEdition> for ReglagesEditionComplets {
    fn from(r: &ReglagesEdition) -> Self {
        Self {
            ecriture_comicrack: r.ecriture_comicrack.unwrap_or(false),
            ecrire_dans_fichier: r.ecrire_dans_fichier.unwrap_or(false),
            ranger_original: r.ranger_original.unwrap_or(false),
            modele_nom: r.modele_nom.clone(),
            dossier_cible: r.dossier_cible.clone(),
        }
    }
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
/// Bornes de la largeur de la colonne des listes (barre latérale, px) ; 250 par défaut côté front.
pub const LARGEUR_LISTES: (u32, u32) = (180, 500);
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
    /// Vue empilée par série (« 📚 Empiler par série »), grille seulement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empiler: Option<bool>,
    /// Taille des couvertures (px).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<u32>,
    /// Panneau de détails affiché.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<bool>,
    /// Largeur du panneau de détails (px).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largeur_details: Option<u32>,
    /// Largeur de la colonne des listes (barre latérale, px).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largeur_listes: Option<u32>,
    /// Identifiant du thème (nom du fichier CSS de `data\themes`, sans extension). Un thème
    /// disparu est remplacé par le thème par défaut côté front.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Réglages du lecteur (sous-objet `lecteur`, remplacé en entier à chaque enregistrement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lecteur: Option<PreferencesLecteur>,
    /// Vue propre à chaque liste (tri, sens, groupe, grille / tableau), comme ComicRack : clé =
    /// identifiant de la liste. **Fusion par liste** à l'enregistrement : une entrée `null` efface
    /// la liste (liste supprimée), une entrée objet remplace celle de la liste ; les autres listes
    /// sont gardées. Jamais écrit dans `ComicDb.xml` ni dans la base de ComicRack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listes: Option<BTreeMap<String, Option<VueListe>>>,
}

/// Vue mémorisée d'une liste (section `vue.listes.<id>`). Lecture tolérante champ par champ.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueListe {
    /// Clé de tri ([`TRIS`] ou `ordre`, « # Ordre de la liste »).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sens_tri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groupe: Option<String>,
    /// `grille` ou `tableau`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vue: Option<String>,
    /// Vue empilée par série de cette liste.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empiler: Option<bool>,
}

/// Nombre maximal de listes dont la vue est gardée (fichier modifié à la main, etc.).
pub const MAX_VUES_LISTES: usize = 5000;

impl VueListe {
    /// Construit la vue d'une liste depuis du JSON ; `None` si ce n'est pas un objet ou si aucun
    /// champ n'est valide.
    pub fn depuis_json(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let parmi = |clef: &str, permis: &[&str]| {
            o.get(clef).and_then(Value::as_str).filter(|s| permis.contains(s)).map(str::to_string)
        };
        let tri =
            o.get("tri").and_then(Value::as_str).filter(|s| *s == "ordre" || TRIS.contains(s)).map(str::to_string);
        let r = Self {
            tri,
            sens_tri: parmi("sensTri", SENS),
            groupe: parmi("groupe", GROUPES),
            vue: parmi("vue", VUES),
            empiler: o.get("empiler").and_then(Value::as_bool),
        };
        (r != Self::default()).then_some(r)
    }
}

/// Identifiant de liste plausible (GUID de ComicRack, identifiant de la base de ComicRust).
fn id_liste_valide(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '{' | '}'))
}

/// Lit `vue.listes` : entrées `null` gardées (effacement demandé), entrées invalides ignorées.
fn listes_depuis_json(v: &Value) -> Option<BTreeMap<String, Option<VueListe>>> {
    let o = v.as_object()?;
    let m: BTreeMap<_, _> = o
        .iter()
        .filter(|(id, _)| id_liste_valide(id))
        .filter_map(|(id, v)| match v {
            Value::Null => Some((id.clone(), None)),
            v => VueListe::depuis_json(v).map(|l| (id.clone(), Some(l))),
        })
        .take(MAX_VUES_LISTES)
        .collect();
    (!m.is_empty()).then_some(m)
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
            empiler: o.get("empiler").and_then(Value::as_bool),
            zoom: borne("zoom", ZOOM),
            details: o.get("details").and_then(Value::as_bool),
            largeur_details: borne("largeurDetails", LARGEUR_DETAILS),
            largeur_listes: borne("largeurListes", LARGEUR_LISTES),
            theme: o.get("theme").and_then(Value::as_str).filter(|s| id_theme_valide(s)).map(str::to_string),
            lecteur: o.get("lecteur").and_then(PreferencesLecteur::depuis_json),
            listes: o.get("listes").and_then(listes_depuis_json),
        }
    }

    /// Applique les champs présents de `m` (les absents gardent la valeur actuelle).
    pub fn fusionner(&mut self, m: PreferencesVue) {
        macro_rules! prendre {
            ($($c:ident),*) => { $( if m.$c.is_some() { self.$c = m.$c; } )* };
        }
        prendre!(tri, sens_tri, groupe, vue, empiler, zoom, details, largeur_details, largeur_listes, theme, lecteur);
        // Vues des listes : fusion liste par liste (`null` efface), jamais d'entrée `null` gardée.
        if let Some(patch) = m.listes {
            let mut listes = self.listes.take().unwrap_or_default();
            listes.retain(|_, v| v.is_some());
            for (id, v) in patch {
                match v {
                    Some(v) => {
                        if listes.len() < MAX_VUES_LISTES || listes.contains_key(&id) {
                            listes.insert(id, Some(v));
                        }
                    }
                    None => {
                        listes.remove(&id);
                    }
                }
            }
            self.listes = (!listes.is_empty()).then_some(listes);
        }
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

/// Réglages de l'édition (valeurs par défaut si rien n'est enregistré).
pub fn lire_reglages_edition(data: &Path) -> Result<ReglagesEditionComplets, String> {
    Ok((&lire(data)?.edition).into())
}

/// Fusionne `m` dans les réglages de l'édition ; renvoie l'ensemble à jour.
pub fn enregistrer_reglages_edition(data: &Path, m: ReglagesEdition) -> Result<ReglagesEditionComplets, String> {
    modifier(data, |r| {
        r.edition.fusionner(m);
        (&r.edition).into()
    })
}

/// Fusionne `m` dans les préférences enregistrées ; renvoie les préférences à jour.
pub fn enregistrer_preferences_vue(data: &Path, m: PreferencesVue) -> Result<PreferencesVue, String> {
    modifier(data, |r| {
        r.vue.fusionner(m);
        r.vue.clone()
    })
}

/// Oublie les vues mémorisées (`vue.listes`) des listes `ids` (liste supprimée, avec ses sous-listes
/// si c'était un dossier). Les identifiants se comparent sans tenir compte de la casse (GUID de
/// ComicRack). Renvoie le nombre d'entrées retirées ; le fichier n'est réécrit que s'il y en a.
pub fn oublier_vues_listes(data: &Path, ids: &[String]) -> Result<usize, String> {
    if ids.is_empty() {
        return Ok(0);
    }
    let _v = verrou();
    let mut r = lire(data)?;
    let Some(listes) = r.vue.listes.as_mut() else { return Ok(0) };
    let avant = listes.len();
    listes.retain(|cle, _| !ids.iter().any(|i| i.eq_ignore_ascii_case(cle)));
    let retirees = avant - listes.len();
    if listes.is_empty() {
        r.vue.listes = None;
    }
    if retirees > 0 {
        ecrire(data, &r)?;
    }
    Ok(retirees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oublier_les_vues_de_listes_supprimees() {
        let d = tempfile::tempdir().unwrap();
        let patch = serde_json::json!({ "listes": {
            "AAAA-1": { "tri": "titre" }, "bbbb-2": { "groupe": "serie" }, "cccc-3": { "vue": "tableau" }
        }});
        enregistrer_preferences_vue(d.path(), PreferencesVue::depuis_json(&patch)).unwrap();
        // Dossier + sous-liste supprimés (casse différente tolérée) ; une inconnue est sans effet.
        let n = oublier_vues_listes(d.path(), &["aaaa-1".into(), "BBBB-2".into(), "zzzz".into()]).unwrap();
        assert_eq!(n, 2);
        let l = lire_preferences_vue(d.path()).unwrap().listes.unwrap();
        assert_eq!(l.keys().collect::<Vec<_>>(), ["cccc-3"]);
        // Plus rien à oublier : pas d'erreur ; la dernière entrée retirée supprime la section.
        assert_eq!(oublier_vues_listes(d.path(), &["zzzz".into()]).unwrap(), 0);
        assert_eq!(oublier_vues_listes(d.path(), &["cccc-3".into()]).unwrap(), 1);
        assert!(lire_preferences_vue(d.path()).unwrap().listes.is_none());
        assert_eq!(oublier_vues_listes(d.path(), &[]).unwrap(), 0);
    }

    #[test]
    fn absent_puis_aller_retour() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(lire(d.path()).unwrap(), Reglages::default());
        let r = Reglages {
            mode: Some(Mode::Comicrack),
            chemin_ini: Some("C:/CR/ComicRack.ini".into()),
            store_mysql: None,
            comicrack_mysql: None,
            vue: PreferencesVue::default(),
            edition: ReglagesEdition::default(),
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

    /// Vue propre à chaque liste : fusion liste par liste, `null` efface, lecture tolérante,
    /// valeurs relues après redémarrage, tri de la bibliothèque inchangé.
    #[test]
    fn vues_des_listes_memorisees_par_liste() {
        let d = tempfile::tempdir().unwrap();
        enregistrer_preferences_vue(d.path(), PreferencesVue { tri: Some("titre".into()), ..Default::default() })
            .unwrap();
        let patch = |j: serde_json::Value| PreferencesVue::depuis_json(&serde_json::json!({ "listes": j }));
        enregistrer_preferences_vue(
            d.path(),
            patch(serde_json::json!({
                "aaaa-1": { "tri": "ordre", "sensTri": "desc", "groupe": "serie", "vue": "tableau", "empiler": true },
                "bbbb-2": { "tri": "pages" }
            })),
        )
        .unwrap();
        // Une seule liste modifiée : l'autre est gardée.
        enregistrer_preferences_vue(d.path(), patch(serde_json::json!({ "bbbb-2": { "groupe": "editeur" } }))).unwrap();
        let p = lire_preferences_vue(d.path()).unwrap();
        assert_eq!(p.tri.as_deref(), Some("titre"));
        let l = p.listes.clone().unwrap();
        let a = l["aaaa-1"].clone().unwrap();
        assert_eq!(
            a,
            VueListe {
                tri: Some("ordre".into()),
                sens_tri: Some("desc".into()),
                groupe: Some("serie".into()),
                vue: Some("tableau".into()),
                empiler: Some(true)
            }
        );
        assert_eq!(l["bbbb-2"].clone().unwrap(), VueListe { groupe: Some("editeur".into()), ..Default::default() });
        // Liste supprimée : entrée effacée, aucun `null` écrit dans le fichier.
        enregistrer_preferences_vue(d.path(), patch(serde_json::json!({ "aaaa-1": null }))).unwrap();
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(!texte.contains("aaaa-1") && texte.contains("bbbb-2"), "{texte}");
        enregistrer_preferences_vue(d.path(), patch(serde_json::json!({ "bbbb-2": null }))).unwrap();
        assert!(lire_preferences_vue(d.path()).unwrap().listes.is_none());
        // Lecture tolérante : valeurs inconnues, identifiants douteux, mauvais types ignorés.
        let p = patch(serde_json::json!({
            "cccc-3": { "tri": "bidule", "groupe": "serie", "vue": 4 },
            "../x": { "tri": "titre" },
            "dddd-4": { "tri": "n_importe" },
            "eeee-5": "grille"
        }));
        let l = p.listes.unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l["cccc-3"].clone().unwrap(), VueListe { groupe: Some("serie".into()), ..Default::default() });
        assert!(PreferencesVue::depuis_json(&serde_json::json!({ "listes": [1, 2] })).listes.is_none());
        // `ordre` n'est accepté que pour une liste.
        assert!(PreferencesVue::depuis_json(&serde_json::json!({ "tri": "ordre" })).tri.is_none());
    }

    /// Réglage « empiler par série » : valeur de la bibliothèque et de chaque liste, lecture
    /// tolérante (mauvais type ignoré), `false` mémorisé (il se distingue de « jamais réglé »).
    #[test]
    fn empiler_memorise_par_liste_et_tolerant() {
        let d = tempfile::tempdir().unwrap();
        let patch = |j: serde_json::Value| PreferencesVue::depuis_json(&j);
        enregistrer_preferences_vue(
            d.path(),
            patch(serde_json::json!({ "empiler": true, "listes": { "aaaa-1": { "empiler": false }, "bbbb-2": { "empiler": true } } })),
        )
        .unwrap();
        let p = lire_preferences_vue(d.path()).unwrap();
        assert_eq!(p.empiler, Some(true));
        let l = p.listes.unwrap();
        assert_eq!(l["aaaa-1"].clone().unwrap().empiler, Some(false));
        assert_eq!(l["bbbb-2"].clone().unwrap().empiler, Some(true));
        // Une autre préférence ne touche pas au réglage ; le réglage d'une liste ne touche pas aux autres.
        enregistrer_preferences_vue(
            d.path(),
            patch(serde_json::json!({ "zoom": 180, "listes": { "bbbb-2": { "empiler": false } } })),
        )
        .unwrap();
        let p = lire_preferences_vue(d.path()).unwrap();
        assert_eq!(p.empiler, Some(true));
        let l = p.listes.unwrap();
        assert_eq!(l["aaaa-1"].clone().unwrap().empiler, Some(false));
        assert_eq!(l["bbbb-2"].clone().unwrap().empiler, Some(false));
        // Lecture tolérante : un texte à la place du booléen est ignoré, le reste est gardé.
        let p = patch(
            serde_json::json!({ "empiler": "oui", "tri": "titre", "listes": { "cccc-3": { "empiler": 1, "vue": "tableau" } } }),
        );
        assert_eq!(p.empiler, None);
        assert_eq!(p.tri.as_deref(), Some("titre"));
        assert_eq!(p.listes.unwrap()["cccc-3"].clone().unwrap().empiler, None);
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
            empiler: Some(true),
            zoom: Some(200),
            details: Some(false),
            largeur_details: Some(400),
            largeur_listes: Some(310),
            theme: Some("moderne-sombre".into()),
            lecteur: None,
            listes: None,
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
            texte.contains("\"vue\"")
                && texte.contains("\"sensTri\": \"desc\"")
                && texte.contains("largeurDetails")
                && texte.contains("\"largeurListes\": 310"),
            "{texte}"
        );
        assert!(!d.path().join("comicrust-tauri.json.tmp").exists());
    }

    #[test]
    fn largeur_listes_bornee() {
        let lire_largeur = |j: serde_json::Value| PreferencesVue::depuis_json(&j).largeur_listes;
        assert_eq!(lire_largeur(serde_json::json!({ "largeurListes": 300 })), Some(300));
        assert_eq!(lire_largeur(serde_json::json!({ "largeurListes": 5 })), Some(LARGEUR_LISTES.0));
        assert_eq!(lire_largeur(serde_json::json!({ "largeurListes": 99999 })), Some(LARGEUR_LISTES.1));
        assert_eq!(lire_largeur(serde_json::json!({ "largeurListes": "x" })), None);
        assert_eq!(lire_largeur(serde_json::json!({})), None);
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
               "vue":"mosaique","zoom":9999,"details":"oui","largeurDetails":-5,"largeurListes":"large","theme":"../x","futur":true}}"#,
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
    fn reglages_edition_defaut_fusion_et_tolerance() {
        let d = tempfile::tempdir().unwrap();
        let r = lire_reglages_edition(d.path()).unwrap();
        assert!(!r.ecriture_comicrack && !r.ecrire_dans_fichier && !r.ranger_original, "lecture seule par défaut");
        enregistrer_preferences_vue(d.path(), PreferencesVue { tri: Some("titre".into()), ..Default::default() })
            .unwrap();
        let m = ReglagesEdition {
            ecriture_comicrack: Some(true),
            modele_nom: Some("{series}".into()),
            ..Default::default()
        };
        let r = enregistrer_reglages_edition(d.path(), m).unwrap();
        assert!(r.ecriture_comicrack && !r.ecrire_dans_fichier);
        let r = enregistrer_reglages_edition(
            d.path(),
            ReglagesEdition { ecrire_dans_fichier: Some(true), ..Default::default() },
        )
        .unwrap();
        assert!(r.ecriture_comicrack && r.ecrire_dans_fichier && r.modele_nom.as_deref() == Some("{series}"));
        assert_eq!(lire_preferences_vue(d.path()).unwrap().tri.as_deref(), Some("titre"));
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(texte.contains("\"ecritureComicrack\": true"), "{texte}");
        // Valeurs d'un mauvais type ignorées.
        std::fs::write(chemin_reglages(d.path()), r#"{"edition":{"ecritureComicrack":"oui","rangerOriginal":true}}"#)
            .unwrap();
        let r = lire_reglages_edition(d.path()).unwrap();
        assert!(!r.ecriture_comicrack && r.ranger_original);
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
