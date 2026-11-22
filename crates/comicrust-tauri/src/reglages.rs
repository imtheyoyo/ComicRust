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
    /// Mise à jour intégrée (vérification au démarrage, version ignorée).
    #[serde(skip_serializing_if = "ReglagesMaj::est_vide")]
    pub maj: ReglagesMaj,
    /// Scripts Python (automatismes, scripts activés / désactivés, réglages de chaque script ;
    /// valeurs secrètes chiffrées par `comicrust_core::secret`). Lecture tolérante.
    #[serde(skip_serializing_if = "ReglagesScripts::est_vide")]
    pub scripts: ReglagesScripts,
    /// Analyse des dossiers au démarrage et suivi des modifications faites dans ComicRack.
    #[serde(skip_serializing_if = "ReglagesAnalyse::est_vide")]
    pub analyse: ReglagesAnalyse,
    /// Taille, position et état maximisé de la fenêtre principale (restaurés au démarrage).
    #[serde(skip_serializing_if = "ReglagesFenetre::est_vide")]
    pub fenetre: ReglagesFenetre,
    /// Fichiers récemment ouverts, le plus récent en premier (bloc `recents`, 15 au plus).
    #[serde(skip_serializing_if = "Vec::is_empty", deserialize_with = "recents_tolerants")]
    pub recents: Vec<String>,
}

// ───────────────────────────── Fichiers récents (lot L6) ─────────────────────────────
// Menu 🕘 de la bibliothèque : `Settings::recent` et `push_recent` de la version egui (15 chemins,
// le plus récent d'abord, sans doublon). Contrat : `docs/tauri/ipc-lecteur.md`.

/// Nombre maximal de fichiers récents mémorisés (`Settings::MAX_RECENT` de la version egui).
pub const MAX_RECENTS: usize = 15;

/// Deux chemins désignent-ils le même fichier ? Sous Windows, la casse est ignorée (NTFS).
fn meme_chemin(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Met `chemin` en tête de la liste : l'ancienne occurrence est retirée (pas de doublon), la liste
/// est plafonnée à [`MAX_RECENTS`]. Un chemin vide est ignoré. Renvoie vrai si la liste a changé.
pub fn pousser_recent(liste: &mut Vec<String>, chemin: &str) -> bool {
    let chemin = chemin.trim();
    if chemin.is_empty() {
        return false;
    }
    if liste.first().is_some_and(|p| p == chemin) {
        return false;
    }
    liste.retain(|p| !meme_chemin(p, chemin));
    liste.insert(0, chemin.to_string());
    liste.truncate(MAX_RECENTS);
    true
}

/// Retire `chemin` de la liste. Renvoie vrai si la liste a changé.
pub fn retirer_recent_de(liste: &mut Vec<String>, chemin: &str) -> bool {
    let avant = liste.len();
    liste.retain(|p| !meme_chemin(p, chemin.trim()));
    liste.len() != avant
}

/// Liste lue dans du JSON quelconque : seules les chaînes non vides sont gardées, dans l'ordre,
/// sans doublon, 15 au plus (fichier modifié à la main ou d'une autre version : jamais d'erreur).
pub fn recents_depuis_json(v: &Value) -> Vec<String> {
    let mut liste: Vec<String> = Vec::new();
    for p in v.as_array().into_iter().flatten().filter_map(Value::as_str) {
        let p = p.trim();
        if !p.is_empty() && !liste.iter().any(|d| meme_chemin(d, p)) && liste.len() < MAX_RECENTS {
            liste.push(p.to_string());
        }
    }
    liste
}

fn recents_tolerants<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(recents_depuis_json(&Value::deserialize(d)?))
}

/// Un fichier récent tel que le menu l'affiche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub chemin: String,
    /// Nom du fichier (dernier élément du chemin, séparateurs `/` et `\`).
    pub nom: String,
    /// Le fichier (ou dossier) existe encore : sinon l'entrée est grisée.
    pub existe: bool,
}

/// Nom du fichier d'un chemin, quel que soit le séparateur (chemins Windows lus sous Linux compris).
pub fn nom_du_chemin(chemin: &str) -> String {
    let net = chemin.trim_end_matches(['/', '\\']);
    net.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).unwrap_or(chemin).to_string()
}

/// Entrées du menu : chaque chemin est vérifié (`existe`) ; l'ordre est conservé.
pub fn verifier_recents(liste: &[String]) -> Vec<Recent> {
    liste.iter().map(|c| Recent { chemin: c.clone(), nom: nom_du_chemin(c), existe: Path::new(c).exists() }).collect()
}

/// Fichiers récents enregistrés, vérifiés.
pub fn lire_recents(data: &Path) -> Result<Vec<Recent>, String> {
    Ok(verifier_recents(&lire(data)?.recents))
}

/// Ouverture d'un fichier : il passe en tête des récents. Le fichier n'est réécrit que si la
/// liste change.
pub fn ajouter_recent(data: &Path, chemin: &str) -> Result<Vec<Recent>, String> {
    let liste = modifier_recents(data, |l| pousser_recent(l, chemin))?;
    Ok(verifier_recents(&liste))
}

/// Retire une entrée (fichier qui ne s'ouvre pas, ou choix de l'utilisateur).
pub fn retirer_recent(data: &Path, chemin: &str) -> Result<Vec<Recent>, String> {
    let liste = modifier_recents(data, |l| retirer_recent_de(l, chemin))?;
    Ok(verifier_recents(&liste))
}

/// Vide la liste.
pub fn vider_recents(data: &Path) -> Result<Vec<Recent>, String> {
    let liste = modifier_recents(data, |l| {
        let change = !l.is_empty();
        l.clear();
        change
    })?;
    Ok(verifier_recents(&liste))
}

/// Lecture-modification des seuls récents, sous verrou ; le fichier est réécrit seulement si `f`
/// rend vrai (rien à écrire = pas d'usure du disque, ni de fichier créé pour rien).
fn modifier_recents(data: &Path, f: impl FnOnce(&mut Vec<String>) -> bool) -> Result<Vec<String>, String> {
    let _v = verrou();
    let mut r = lire(data)?;
    if f(&mut r.recents) {
        ecrire(data, &r)?;
    }
    Ok(r.recents)
}

/// Géométrie mémorisée de la fenêtre principale (section `fenetre`, `docs/tauri/ipc-fenetre.md`).
///
/// Unités : pixels physiques. `x` et `y` : coin haut-gauche de la fenêtre (peut être négatif sur un
/// écran à gauche ou au-dessus de l'écran principal) ; `largeur` et `hauteur` : zone de contenu.
/// Ce sont toujours la taille et la position « normales » (non maximisées) : `maximisee` dit si la
/// fenêtre doit être rouverte maximisée. Lecture tolérante : un champ invalide est ignoré.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesFenetre {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largeur: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hauteur: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub maximisee: bool,
}

/// Bornes de lecture d'une coordonnée ou d'une dimension (au-delà : valeur absurde, ignorée).
const COORD_MAX: i64 = 100_000;
const DIM_MAX: u64 = 20_000;

impl ReglagesFenetre {
    pub fn est_vide(&self) -> bool {
        *self == Self::default()
    }

    /// Depuis du JSON quelconque (champ invalide ou hors bornes ignoré).
    pub fn depuis_json(v: &Value) -> Self {
        let Some(o) = v.as_object() else { return Self::default() };
        let coord = |c: &str| {
            o.get(c).and_then(Value::as_i64).filter(|n| n.abs() <= COORD_MAX).and_then(|n| i32::try_from(n).ok())
        };
        let dim = |c: &str| {
            o.get(c).and_then(Value::as_u64).filter(|n| (1..=DIM_MAX).contains(n)).and_then(|n| u32::try_from(n).ok())
        };
        Self {
            x: coord("x"),
            y: coord("y"),
            largeur: dim("largeur"),
            hauteur: dim("hauteur"),
            maximisee: o.get("maximisee").and_then(Value::as_bool).unwrap_or(false),
        }
    }
}

impl<'de> Deserialize<'de> for ReglagesFenetre {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::depuis_json(&Value::deserialize(d)?))
    }
}

/// Géométrie de la fenêtre enregistrée (vide si rien n'a encore été mémorisé).
pub fn lire_fenetre(data: &Path) -> Result<ReglagesFenetre, String> {
    Ok(lire(data)?.fenetre)
}

/// Enregistre la géométrie de la fenêtre sans toucher au reste des réglages.
pub fn enregistrer_fenetre(data: &Path, f: ReglagesFenetre) -> Result<(), String> {
    modifier(data, |r| r.fenetre = f)
}

/// Réglages des scripts Python (section `scripts`, `docs/tauri/ipc-scripts.md`) :
/// - `auto` : exécuter les automatismes (absent = **non**, comme la version egui) ;
/// - `desactives` / `actives` : choix de l'utilisateur (identifiants de points d'entrée) qui
///   priment sur l'en-tête `#@Enabled` ;
/// - `valeurs` : réglages de chaque script (clé = chemin relatif du fichier), forme
///   `ScriptSettings::to_stored` (valeur secrète = `{"$secret": "dpapi:…"}`, jamais en clair).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesScripts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
    #[serde(skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub desactives: std::collections::BTreeSet<String>,
    #[serde(skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub actives: std::collections::BTreeSet<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub valeurs: BTreeMap<String, Value>,
}

impl ReglagesScripts {
    pub fn est_vide(&self) -> bool {
        *self == Self::default()
    }

    /// Depuis du JSON quelconque (entrée invalide ignorée).
    pub fn depuis_json(v: &Value) -> Self {
        let Some(o) = v.as_object() else { return Self::default() };
        let ids = |c: &str| {
            o.get(c)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).filter(|s| s.len() <= 1024).map(str::to_string).collect())
                .unwrap_or_default()
        };
        Self {
            auto: o.get("auto").and_then(Value::as_bool),
            desactives: ids("desactives"),
            actives: ids("actives"),
            valeurs: o
                .get("valeurs")
                .and_then(Value::as_object)
                .map(|m| m.iter().filter(|(_, v)| v.is_object()).map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        }
    }
}

impl<'de> Deserialize<'de> for ReglagesScripts {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::depuis_json(&Value::deserialize(d)?))
    }
}

/// Réglages de la mise à jour (section `maj`, `docs/tauri/ipc-maj.md`) : `verifierAuDemarrage`
/// (absent = vrai) et `versionIgnoree` (tag d'une version que l'utilisateur ne veut pas
/// installer : la vérification automatique ne la propose plus). Lecture tolérante.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesMaj {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifier_au_demarrage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_ignoree: Option<String>,
}

impl ReglagesMaj {
    pub fn est_vide(&self) -> bool {
        *self == Self::default()
    }

    /// Depuis du JSON quelconque (champ invalide ignoré).
    pub fn depuis_json(v: &Value) -> Self {
        let Some(o) = v.as_object() else { return Self::default() };
        Self {
            verifier_au_demarrage: o.get("verifierAuDemarrage").and_then(Value::as_bool),
            version_ignoree: o
                .get("versionIgnoree")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 100)
                .map(str::to_string),
        }
    }
}

impl<'de> Deserialize<'de> for ReglagesMaj {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::depuis_json(&Value::deserialize(d)?))
    }
}

/// Réglages de l'analyse et du suivi (section `analyse`, `docs/tauri/ipc-scan.md`) :
/// - `auDemarrage` : analyser les dossiers dès que la bibliothèque est chargée (absent = **oui**,
///   comme `scan_after_load = true` de la version egui) ; jamais en mode lié ni hors ligne ;
/// - `suivreComicrack` : suivre les modifications faites dans ComicRack (absent = **oui**,
///   `auto_refresh` de la version egui). Sans effet sur la base de ComicRust (toujours suivie).
///
/// Lecture tolérante (champ invalide ignoré).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesAnalyse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub au_demarrage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suivre_comicrack: Option<bool>,
}

impl ReglagesAnalyse {
    pub fn est_vide(&self) -> bool {
        *self == Self::default()
    }

    /// Depuis du JSON quelconque (champ invalide ignoré).
    pub fn depuis_json(v: &Value) -> Self {
        let Some(o) = v.as_object() else { return Self::default() };
        Self {
            au_demarrage: o.get("auDemarrage").and_then(Value::as_bool),
            suivre_comicrack: o.get("suivreComicrack").and_then(Value::as_bool),
        }
    }

    /// Les champs présents dans `m` remplacent les nôtres.
    pub fn fusionner(&mut self, m: ReglagesAnalyse) {
        if m.au_demarrage.is_some() {
            self.au_demarrage = m.au_demarrage;
        }
        if m.suivre_comicrack.is_some() {
            self.suivre_comicrack = m.suivre_comicrack;
        }
    }
}

impl<'de> Deserialize<'de> for ReglagesAnalyse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::depuis_json(&Value::deserialize(d)?))
    }
}

/// Réglages de l'analyse avec leurs valeurs par défaut appliquées (ce que voit le front).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesAnalyseComplets {
    pub au_demarrage: bool,
    pub suivre_comicrack: bool,
}

impl From<&ReglagesAnalyse> for ReglagesAnalyseComplets {
    fn from(r: &ReglagesAnalyse) -> Self {
        Self { au_demarrage: r.au_demarrage.unwrap_or(true), suivre_comicrack: r.suivre_comicrack.unwrap_or(true) }
    }
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
pub const GROUPES: &[&str] = &["aucun", "serie", "editeur", "scenariste", "annee", "arc", "genre", "format", "etat"];
/// Colonnes du tableau, dans l'ordre d'affichage de la version egui (`COLS` de `library_table.rs`).
pub const COLONNES_TABLEAU: &[&str] =
    &["serie", "numero", "titre", "parution", "editeur", "scenariste", "pages", "lu", "note", "ajoute", "fichier"];
/// Bornes de la largeur d'une colonne du tableau (px).
pub const LARGEUR_COLONNE: (u32, u32) = (30, 2000);
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
    /// Panneau des listes affiché (barre latérale ; absent = affiché). Masqué, la barre latérale se
    /// réduit à une colonne de boutons (lot L7).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listes_visibles: Option<bool>,
    /// Jauges des listes (compteurs rouge / jaune / vert à droite de chaque liste ; absent =
    /// affichées). Voir `docs/tauri/ipc-jauges.md`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jauges_listes: Option<bool>,
    /// Réglages du lecteur (sous-objet `lecteur`, remplacé en entier à chaque enregistrement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lecteur: Option<PreferencesLecteur>,
    /// Vue propre à chaque liste (tri, sens, groupe, grille / tableau), comme ComicRack : clé =
    /// identifiant de la liste. **Fusion par liste** à l'enregistrement : une entrée `null` efface
    /// la liste (liste supprimée), une entrée objet remplace celle de la liste ; les autres listes
    /// sont gardées. Jamais écrit dans `ComicDb.xml` ni dans la base de ComicRack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listes: Option<BTreeMap<String, Option<VueListe>>>,
    /// Colonnes du tableau (sous-objet `tableau`, remplacé en entier à chaque enregistrement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tableau: Option<PreferencesTableau>,
    /// Réglage « Rouvrir la dernière liste au démarrage » (absent = **activé**).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rouvrir_derniere_liste: Option<bool>,
    /// Identifiant (stable : GUID de la liste) de la dernière liste sélectionnée : dossier, liste de
    /// lecture ou liste intelligente. Absent = toute la bibliothèque. À l'enregistrement, une chaîne
    /// vide l'efface (retour à la bibliothèque) ; le fichier ne contient jamais de chaîne vide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derniere_liste: Option<String>,
}

/// Colonnes du tableau (section `vue.tableau`) : colonnes visibles et largeurs choisies par
/// l'utilisateur. Les colonnes gardent toujours l'ordre de [`COLONNES_TABLEAU`] ; la colonne
/// « # » des listes n'est pas concernée. Lecture tolérante : identifiants inconnus et doublons
/// ignorés, largeurs ramenées dans [`LARGEUR_COLONNE`].
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesTableau {
    /// Colonnes visibles (absent : toutes). Jamais vide : au moins une colonne reste affichée.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colonnes: Option<Vec<String>>,
    /// Largeur en pixels des colonnes redimensionnées (clé : identifiant de colonne).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largeurs: Option<BTreeMap<String, u32>>,
}

impl PreferencesTableau {
    /// Construit les réglages depuis du JSON ; `None` si ce n'est pas un objet. Un objet sans rien de
    /// valide donne les réglages par défaut (toutes les colonnes, largeurs d'origine), ce que
    /// [`PreferencesVue::fusionner`] prend pour « effacer ».
    pub fn depuis_json(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let colonnes = o.get("colonnes").and_then(Value::as_array).and_then(|a| {
            let demandees: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            // Ordre canonique, sans doublon ni identifiant inconnu.
            let v: Vec<String> =
                COLONNES_TABLEAU.iter().filter(|c| demandees.contains(c)).map(|c| (*c).to_string()).collect();
            (!v.is_empty()).then_some(v)
        });
        let (min, max) = LARGEUR_COLONNE;
        let largeurs = o.get("largeurs").and_then(Value::as_object).and_then(|m| {
            let l: BTreeMap<String, u32> = m
                .iter()
                .filter(|(c, _)| COLONNES_TABLEAU.contains(&c.as_str()))
                .filter_map(|(c, n)| {
                    let n = n.as_f64().filter(|n| n.is_finite())?;
                    Some((c.clone(), n.round().clamp(f64::from(min), f64::from(max)) as u32))
                })
                .collect();
            (!l.is_empty()).then_some(l)
        });
        Some(Self { colonnes, largeurs })
    }
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
            listes_visibles: o.get("listesVisibles").and_then(Value::as_bool),
            jauges_listes: o.get("jaugesListes").and_then(Value::as_bool),
            lecteur: o.get("lecteur").and_then(PreferencesLecteur::depuis_json),
            listes: o.get("listes").and_then(listes_depuis_json),
            tableau: o.get("tableau").and_then(PreferencesTableau::depuis_json),
            rouvrir_derniere_liste: o.get("rouvrirDerniereListe").and_then(Value::as_bool),
            derniere_liste: o
                .get("derniereListe")
                .and_then(Value::as_str)
                .filter(|s| s.is_empty() || id_liste_valide(s))
                .map(str::to_string),
        }
    }

    /// Applique les champs présents de `m` (les absents gardent la valeur actuelle).
    pub fn fusionner(&mut self, m: PreferencesVue) {
        macro_rules! prendre {
            ($($c:ident),*) => { $( if m.$c.is_some() { self.$c = m.$c; } )* };
        }
        prendre!(
            tri,
            sens_tri,
            groupe,
            vue,
            empiler,
            zoom,
            details,
            largeur_details,
            largeur_listes,
            theme,
            listes_visibles,
            jauges_listes,
            lecteur,
            rouvrir_derniere_liste
        );
        // Dernière liste : une chaîne vide efface (retour à la bibliothèque entière).
        if let Some(id) = m.derniere_liste {
            self.derniere_liste = (!id.is_empty()).then_some(id);
        }
        // Colonnes du tableau : sous-objet remplacé en entier ; un objet vide efface les réglages.
        if let Some(t) = m.tableau {
            self.tableau = (t != PreferencesTableau::default()).then_some(t);
        }
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

/// Vrai si le fichier de réglages mentionne une base MySQL pour le mode autonome (clé
/// `storeMysql`, même illisible ou incomplète — donc ignorée par [`lire`]) : on ne propose alors
/// jamais de créer une base SQLite à la place.
pub fn mysql_mentionne(data: &Path) -> bool {
    std::fs::read_to_string(chemin_reglages(data))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .is_some_and(|v| v.get("storeMysql").is_some_and(|m| !m.is_null()))
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

/// Réglages de l'analyse et du suivi (valeurs par défaut : tout activé, comme la version egui).
pub fn lire_reglages_analyse(data: &Path) -> Result<ReglagesAnalyseComplets, String> {
    Ok((&lire(data)?.analyse).into())
}

/// Fusionne `m` dans les réglages de l'analyse ; renvoie l'ensemble à jour.
pub fn enregistrer_reglages_analyse(data: &Path, m: ReglagesAnalyse) -> Result<ReglagesAnalyseComplets, String> {
    modifier(data, |r| {
        r.analyse.fusionner(m);
        (&r.analyse).into()
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
    fn section_fenetre_aller_retour_et_tolerance() {
        let f =
            ReglagesFenetre { x: Some(-1920), y: Some(40), largeur: Some(1280), hauteur: Some(800), maximisee: true };
        let r = Reglages { fenetre: f, ..Default::default() };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json["fenetre"],
            serde_json::json!({"x": -1920, "y": 40, "largeur": 1280, "hauteur": 800, "maximisee": true})
        );
        assert_eq!(serde_json::from_value::<Reglages>(json).unwrap().fenetre, f);
        // Section absente du fichier quand elle est vide (fichiers existants inchangés).
        assert!(serde_json::to_value(Reglages::default()).unwrap().get("fenetre").is_none());
        // Lecture tolérante : champs invalides ou absurdes ignorés, le reste gardé.
        let t = ReglagesFenetre::depuis_json(
            &serde_json::json!({"x": "a", "y": 5, "largeur": 0, "hauteur": 900, "maximisee": "oui"}),
        );
        assert_eq!(t, ReglagesFenetre { x: None, y: Some(5), largeur: None, hauteur: Some(900), maximisee: false });
        assert!(ReglagesFenetre::depuis_json(&serde_json::json!([1, 2])).est_vide());
        assert_eq!(ReglagesFenetre::depuis_json(&serde_json::json!({"largeur": 99999999})).largeur, None);
    }

    #[test]
    fn enregistrer_fenetre_garde_le_reste() {
        let d = tempfile::tempdir().unwrap();
        modifier(d.path(), |r| r.mode = Some(Mode::Autonome)).unwrap();
        let f = ReglagesFenetre { x: Some(10), y: Some(20), largeur: Some(1000), hauteur: Some(700), maximisee: false };
        enregistrer_fenetre(d.path(), f).unwrap();
        assert_eq!(lire_fenetre(d.path()).unwrap(), f);
        assert_eq!(lire(d.path()).unwrap().mode, Some(Mode::Autonome));
    }

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
            maj: ReglagesMaj::default(),
            fenetre: ReglagesFenetre::default(),
            scripts: ReglagesScripts {
                auto: Some(true),
                desactives: ["a.py:f".to_string()].into(),
                actives: Default::default(),
                valeurs: [("a.py".to_string(), serde_json::json!({"n": 3}))].into(),
            },
            analyse: ReglagesAnalyse { au_demarrage: Some(false), suivre_comicrack: None },
            recents: vec!["D:/BD/x.cbz".to_string()],
        };
        ecrire(d.path(), &r).unwrap();
        assert_eq!(lire(d.path()).unwrap(), r);
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(texte.contains("\"mode\": \"comicrack\"") && texte.contains("cheminIni"), "{texte}");
    }

    #[test]
    fn reglages_scripts_tolerants() {
        let v = serde_json::json!({"auto": "oui", "desactives": ["x", 3], "valeurs": {"a.py": {"k": 1}, "b.py": 2}});
        let r = ReglagesScripts::depuis_json(&v);
        assert_eq!(r.auto, None, "booléen attendu");
        assert_eq!(r.desactives.iter().collect::<Vec<_>>(), ["x"]);
        assert_eq!(r.valeurs.len(), 1, "valeur non objet ignorée");
        assert_eq!(ReglagesScripts::depuis_json(&serde_json::json!(12)), ReglagesScripts::default());
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
            listes_visibles: Some(false),
            jauges_listes: Some(false),
            lecteur: None,
            listes: None,
            tableau: None,
            rouvrir_derniere_liste: Some(false),
            derniere_liste: Some("{1f4c-AB}".into()),
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

    #[test]
    fn analyse_options_par_defaut_activees() {
        let d = tempfile::tempdir().unwrap();
        let r = lire_reglages_analyse(d.path()).unwrap();
        assert!(r.au_demarrage && r.suivre_comicrack, "analyse au démarrage et suivi activés par défaut");
        // Rien d'écrit tant que l'utilisateur n'a rien choisi.
        assert!(!chemin_reglages(d.path()).exists());
    }

    #[test]
    fn analyse_options_fusion_et_persistance() {
        let d = tempfile::tempdir().unwrap();
        enregistrer_preferences_vue(d.path(), PreferencesVue { tri: Some("titre".into()), ..Default::default() })
            .unwrap();
        let r =
            enregistrer_reglages_analyse(d.path(), ReglagesAnalyse { au_demarrage: Some(false), ..Default::default() })
                .unwrap();
        assert!(!r.au_demarrage && r.suivre_comicrack);
        let r = enregistrer_reglages_analyse(
            d.path(),
            ReglagesAnalyse { suivre_comicrack: Some(false), ..Default::default() },
        )
        .unwrap();
        assert!(!r.au_demarrage && !r.suivre_comicrack, "le second enregistrement garde le premier choix");
        // Les autres sections ne sont pas perdues, et le choix survit à une relecture.
        let relu = lire(d.path()).unwrap();
        assert_eq!(relu.vue.tri.as_deref(), Some("titre"));
        assert_eq!(relu.analyse, ReglagesAnalyse { au_demarrage: Some(false), suivre_comicrack: Some(false) });
        enregistrer_reglages_analyse(
            d.path(),
            ReglagesAnalyse { au_demarrage: Some(true), suivre_comicrack: Some(true) },
        )
        .unwrap();
        assert!(lire_reglages_analyse(d.path()).unwrap().au_demarrage);
    }

    #[test]
    fn analyse_lecture_tolerante() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(chemin_reglages(d.path()), r#"{"analyse": {"auDemarrage": "oui", "suivreComicrack": false}}"#)
            .unwrap();
        let r = lire_reglages_analyse(d.path()).unwrap();
        assert!(r.au_demarrage, "valeur invalide ignorée : défaut");
        assert!(!r.suivre_comicrack);
        std::fs::write(chemin_reglages(d.path()), r#"{"analyse": 3}"#).unwrap();
        assert!(lire_reglages_analyse(d.path()).unwrap().au_demarrage);
    }

    #[test]
    fn groupes_acceptes_comme_egui() {
        // Les neuf regroupements de `GroupKey::ALL` : chacun est lu par les réglages et par `tri`.
        for g in ["aucun", "serie", "editeur", "scenariste", "annee", "arc", "genre", "format", "etat"] {
            let p = PreferencesVue::depuis_json(&serde_json::json!({ "groupe": g }));
            assert_eq!(p.groupe.as_deref(), Some(g));
            assert!(crate::tri::Regroupement::depuis_texte(g).is_some(), "{g}");
        }
        assert_eq!(GROUPES.len(), crate::tri::Regroupement::TOUS.len());
        let p = PreferencesVue::depuis_json(&serde_json::json!({ "groupe": "couleur" }));
        assert_eq!(p.groupe, None);
    }

    #[test]
    fn tableau_lecture_tolerante_et_ordre_canonique() {
        let p = PreferencesVue::depuis_json(&serde_json::json!({
            "tableau": {
                "colonnes": ["fichier", "titre", "inconnue", "titre", "note", 3],
                "largeurs": { "titre": 10, "fichier": 99999, "serie": 150.4, "bidule": 80, "pages": "large" }
            }
        }));
        let t = p.tableau.unwrap();
        // Ordre egui, sans doublon ni identifiant inconnu.
        assert_eq!(t.colonnes.unwrap(), ["titre", "note", "fichier"]);
        let l = t.largeurs.unwrap();
        assert_eq!(l["titre"], 30, "borne basse");
        assert_eq!(l["fichier"], 2000, "borne haute");
        assert_eq!(l["serie"], 150);
        assert_eq!(l.len(), 3, "colonne inconnue et largeur non numérique ignorées");
        // Aucune colonne valide : toutes visibles (champ absent), jamais un tableau vide.
        let p = PreferencesVue::depuis_json(&serde_json::json!({ "tableau": { "colonnes": ["x"] } }));
        assert_eq!(p.tableau, Some(PreferencesTableau::default()));
        let p = PreferencesVue::depuis_json(&serde_json::json!({ "tableau": 4 }));
        assert_eq!(p.tableau, None);
    }

    #[test]
    fn tableau_enregistre_dans_vue_tableau_sans_toucher_au_reste() {
        let d = tempfile::tempdir().unwrap();
        enregistrer_preferences_vue(d.path(), PreferencesVue { tri: Some("titre".into()), ..Default::default() })
            .unwrap();
        let t = PreferencesTableau {
            colonnes: Some(vec!["serie".into(), "scenariste".into()]),
            largeurs: Some(BTreeMap::from([("scenariste".to_string(), 180)])),
        };
        let apres =
            enregistrer_preferences_vue(d.path(), PreferencesVue { tableau: Some(t.clone()), ..Default::default() })
                .unwrap();
        assert_eq!(apres.tableau, Some(t.clone()));
        assert_eq!(apres.tri.as_deref(), Some("titre"));
        let brut: Value = serde_json::from_str(&std::fs::read_to_string(chemin_reglages(d.path())).unwrap()).unwrap();
        assert_eq!(brut["vue"]["tableau"]["colonnes"], serde_json::json!(["serie", "scenariste"]));
        assert_eq!(brut["vue"]["tableau"]["largeurs"]["scenariste"], 180);
        // Le sous-objet est remplacé en entier (les largeurs disparaissent avec lui).
        let t2 = PreferencesTableau { colonnes: Some(vec!["fichier".into()]), largeurs: None };
        let apres =
            enregistrer_preferences_vue(d.path(), PreferencesVue { tableau: Some(t2.clone()), ..Default::default() })
                .unwrap();
        assert_eq!(apres.tableau, Some(t2));
        assert_eq!(lire_preferences_vue(d.path()).unwrap().tableau.unwrap().largeurs, None);
        // Objet vide : « tout afficher, largeurs d'origine » efface la section.
        let apres = enregistrer_preferences_vue(
            d.path(),
            PreferencesVue { tableau: Some(PreferencesTableau::default()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(apres.tableau, None);
        assert_eq!(apres.tri.as_deref(), Some("titre"));
        let brut: Value = serde_json::from_str(&std::fs::read_to_string(chemin_reglages(d.path())).unwrap()).unwrap();
        assert!(brut["vue"].get("tableau").is_none(), "{brut}");
    }

    // ───── Réouverture sur la dernière liste ─────

    #[test]
    fn anciens_reglages_sans_derniere_liste() {
        // Fichier d'une version antérieure : aucun des deux champs, rien ne casse, réglage activé.
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            chemin_reglages(d.path()),
            r#"{"mode":"autonome","vue":{"zoom":120,"listes":{"{AB-1}":{"tri":"titre"}}}}"#,
        )
        .unwrap();
        let v = lire_preferences_vue(d.path()).unwrap();
        assert_eq!((v.rouvrir_derniere_liste, v.derniere_liste.clone()), (None, None));
        assert_eq!(v.zoom, Some(120));
        // Absent = activé : la valeur par défaut est portée par le front (`!== false`) ; rien n'est
        // réécrit tant que l'utilisateur ne change pas de liste.
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(!texte.contains("derniereListe") && !texte.contains("rouvrirDerniereListe"));
    }

    #[test]
    fn derniere_liste_ecriture_relecture_et_effacement() {
        let d = tempfile::tempdir().unwrap();
        let p = |id: &str| PreferencesVue { derniere_liste: Some(id.into()), ..Default::default() };
        let apres = enregistrer_preferences_vue(d.path(), p("3F2504E0-4F89-11D3-9A0C-0305E82C3301")).unwrap();
        assert_eq!(apres.derniere_liste.as_deref(), Some("3F2504E0-4F89-11D3-9A0C-0305E82C3301"));
        let brut: Value = serde_json::from_str(&std::fs::read_to_string(chemin_reglages(d.path())).unwrap()).unwrap();
        assert_eq!(brut["vue"]["derniereListe"], "3F2504E0-4F89-11D3-9A0C-0305E82C3301", "{brut}");
        // Un enregistrement sans ce champ ne l'efface pas (fusion champ par champ).
        let apres =
            enregistrer_preferences_vue(d.path(), PreferencesVue { zoom: Some(150), ..Default::default() }).unwrap();
        assert!(apres.derniere_liste.is_some());
        // Réglage désactivé : la dernière liste reste mémorisée.
        let apres = enregistrer_preferences_vue(
            d.path(),
            PreferencesVue { rouvrir_derniere_liste: Some(false), ..Default::default() },
        )
        .unwrap();
        assert_eq!((apres.rouvrir_derniere_liste, apres.derniere_liste.is_some()), (Some(false), true));
        // Chaîne vide : retour à la bibliothèque entière, le champ disparaît du fichier.
        let apres = enregistrer_preferences_vue(d.path(), p("")).unwrap();
        assert_eq!(apres.derniere_liste, None);
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(!texte.contains("derniereListe"), "{texte}");
        assert!(texte.contains("\"rouvrirDerniereListe\": false"), "{texte}");
    }

    #[test]
    fn derniere_liste_lecture_tolerante() {
        let lire_json = |j: Value| PreferencesVue::depuis_json(&j);
        // Type invalide, identifiant absurde (chemin, trop long) : ignorés sans erreur.
        for mauvais in [
            serde_json::json!(42),
            serde_json::json!(["a"]),
            serde_json::json!("../../etc/passwd"),
            serde_json::json!("x".repeat(101)),
            serde_json::json!(null),
        ] {
            let v =
                lire_json(serde_json::json!({"derniereListe": mauvais, "rouvrirDerniereListe": "oui", "zoom": 130}));
            assert_eq!((v.derniere_liste, v.rouvrir_derniere_liste, v.zoom), (None, None, Some(130)));
        }
        let v = lire_json(serde_json::json!({"derniereListe": "{AB-12}", "rouvrirDerniereListe": true}));
        assert_eq!((v.derniere_liste.as_deref(), v.rouvrir_derniere_liste), (Some("{AB-12}"), Some(true)));
    }
}

#[cfg(test)]
mod tests_recents {
    use super::*;

    fn liste(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn le_plus_recent_passe_en_tete() {
        let mut l = liste(&["/a.cbz", "/b.cbz"]);
        assert!(pousser_recent(&mut l, "/c.cbz"));
        assert_eq!(l, liste(&["/c.cbz", "/a.cbz", "/b.cbz"]));
    }

    #[test]
    fn dedoublonnage_remonte_l_ancienne_entree() {
        let mut l = liste(&["/a.cbz", "/b.cbz", "/c.cbz"]);
        assert!(pousser_recent(&mut l, "/c.cbz"));
        assert_eq!(l, liste(&["/c.cbz", "/a.cbz", "/b.cbz"]));
        assert_eq!(l.len(), 3, "pas de doublon");
    }

    #[test]
    fn deja_en_tete_rien_ne_change() {
        let mut l = liste(&["/a.cbz", "/b.cbz"]);
        assert!(!pousser_recent(&mut l, "/a.cbz"));
        assert_eq!(l, liste(&["/a.cbz", "/b.cbz"]));
    }

    #[test]
    fn plafond_de_quinze_entrees_la_plus_ancienne_sort() {
        let mut l = Vec::new();
        for i in 0..20 {
            pousser_recent(&mut l, &format!("/livre{i}.cbz"));
        }
        assert_eq!(l.len(), MAX_RECENTS);
        assert_eq!(l[0], "/livre19.cbz");
        assert_eq!(l[MAX_RECENTS - 1], "/livre5.cbz");
        assert!(!l.contains(&"/livre4.cbz".to_string()));
    }

    #[test]
    fn chemin_vide_ignore_et_espaces_retires() {
        let mut l = liste(&["/a.cbz"]);
        assert!(!pousser_recent(&mut l, "   "));
        assert!(pousser_recent(&mut l, "  /b.cbz "));
        assert_eq!(l, liste(&["/b.cbz", "/a.cbz"]));
    }

    #[test]
    fn retrait_d_une_entree() {
        let mut l = liste(&["/a.cbz", "/b.cbz"]);
        assert!(retirer_recent_de(&mut l, "/a.cbz"));
        assert!(!retirer_recent_de(&mut l, "/absent.cbz"));
        assert_eq!(l, liste(&["/b.cbz"]));
    }

    #[test]
    fn lecture_tolerante() {
        let v: Value = serde_json::json!(["/a.cbz", 3, null, "", "/a.cbz", "  /b.cbz  ", {"x": 1}]);
        assert_eq!(recents_depuis_json(&v), liste(&["/a.cbz", "/b.cbz"]));
        assert!(recents_depuis_json(&serde_json::json!("pas une liste")).is_empty());
        let trop: Vec<String> = (0..40).map(|i| format!("/{i}.cbz")).collect();
        assert_eq!(recents_depuis_json(&serde_json::json!(trop)).len(), MAX_RECENTS);
    }

    #[test]
    fn nom_du_chemin_windows_et_unix() {
        assert_eq!(nom_du_chemin(r"D:\BD\Thanos\Thanos 01.cbz"), "Thanos 01.cbz");
        assert_eq!(nom_du_chemin("/srv/bd/x.cbz"), "x.cbz");
        assert_eq!(nom_du_chemin(r"D:\BD\Dossier\"), "Dossier");
        assert_eq!(nom_du_chemin("seul.cbz"), "seul.cbz");
    }

    #[test]
    fn aller_retour_dans_le_fichier_sans_toucher_au_reste() {
        let d = tempfile::tempdir().unwrap();
        // Rien d'écrit tant que rien ne change.
        assert!(lire_recents(d.path()).unwrap().is_empty());
        assert!(!chemin_reglages(d.path()).exists());

        modifier(d.path(), |r| r.mode = Some(Mode::Autonome)).unwrap();
        let existant = d.path().join("existe.cbz");
        std::fs::write(&existant, b"x").unwrap();
        let chemin = existant.to_string_lossy().into_owned();
        ajouter_recent(d.path(), "/absent/fantome.cbz").unwrap();
        let l = ajouter_recent(d.path(), &chemin).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].chemin, chemin);
        assert!(l[0].existe, "fichier présent");
        assert_eq!(l[0].nom, "existe.cbz");
        assert!(!l[1].existe, "fichier absent : entrée grisée");
        // Le reste des réglages est intact et le bloc relu à l'identique.
        let r = lire(d.path()).unwrap();
        assert_eq!(r.mode, Some(Mode::Autonome));
        assert_eq!(r.recents.len(), 2);
        let l = retirer_recent(d.path(), "/absent/fantome.cbz").unwrap();
        assert_eq!(l.len(), 1);
        assert!(vider_recents(d.path()).unwrap().is_empty());
        let texte = std::fs::read_to_string(chemin_reglages(d.path())).unwrap();
        assert!(!texte.contains("recents"), "bloc absent quand la liste est vide");
    }

    #[test]
    fn bloc_illisible_ne_bloque_pas_les_reglages() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(chemin_reglages(d.path()), r#"{"mode":"autonome","recents":42}"#).unwrap();
        let r = lire(d.path()).unwrap();
        assert_eq!(r.mode, Some(Mode::Autonome));
        assert!(r.recents.is_empty());
    }
}
