//! Éditeur de listes intelligentes (contrat `docs/tauri/ipc-listes.md`, « Éditeur de listes
//! intelligentes »). Modèle **dupliqué** de `crates/comicrust/src/ui/smart_editor.rs` (règle du
//! plan : on ne touche pas à la version egui) et adapté à l'IPC :
//!
//! - conversion critères ↔ `Matcher` de `comicrust-core::smart`, contrôle des valeurs (nombres,
//!   dates `AAAA-MM-JJ`, nombre de jours, note de 0 à 5, expression régulière) ;
//! - **critères non modifiés gardés à l'octet près** : `lire_criteres` donne à chaque critère et
//!   groupe son `origine` (chemin « 0/2/1 » dans les critères enregistrés) ; à l'enregistrement,
//!   le critère d'origine est relu dans la liste **au moment de l'écriture** : saisie identique →
//!   recopié tel quel (valeur « 4,5 », date « 01/10/2019 », XML d'origine), modifiée → régénéré
//!   en gardant ses attributs et sous-éléments inconnus (`Name`…). Critère absent du catalogue :
//!   toujours recopié ;
//! - aperçu : la liste calculée comme si elle était enregistrée (liste de départ, limite).
//!
//! Report de correctif : une correction de `smart_editor.rs` sur `main` se reporte ici à la main.

use crate::biblio::Biblio;
use crate::listes::{depends_on, flatten, Issue, ResultatListes, TypeListe};
use chrono::NaiveDate;
use comicrust_core::comic_lists::{ComicList, ListKind, Members};
use comicrust_core::lists_edit::NewList;
use comicrust_core::matcher_catalog::{matcher_catalog, matcher_info, MatcherInfo, ValueKind};
use comicrust_core::smart::{describe_kind, Limit, LimitKind, Matcher, MatcherXml, Mode, Selection, SmartQuery};
use serde::{Deserialize, Serialize};

/// Type d'un nouveau critère (« Série contient … »).
pub const TYPE_DEFAUT: &str = "ComicBookSeriesMatcher";
pub const OP_DEFAUT: i32 = 1;
/// Nom d'une nouvelle liste intelligente.
pub const NOM_DEFAUT: &str = "Nouvelle liste intelligente";

// ───────────────────────────── Types de l'IPC ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModeCriteres {
    Tous,
    AuMoinsUn,
}

impl ModeCriteres {
    fn depuis(m: Mode) -> Self {
        match m {
            Mode::And => ModeCriteres::Tous,
            Mode::Or => ModeCriteres::AuMoinsUn,
        }
    }
    fn vers(self) -> Mode {
        match self {
            ModeCriteres::Tous => Mode::And,
            ModeCriteres::AuMoinsUn => Mode::Or,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NatureValeur {
    Texte,
    Nombre,
    Date,
    OuiNon,
    Manga,
    Activation,
    ValeurPerso,
    Expression,
}

impl NatureValeur {
    fn depuis(v: ValueKind) -> Self {
        match v {
            ValueKind::Text => NatureValeur::Texte,
            ValueKind::Number => NatureValeur::Nombre,
            ValueKind::Date => NatureValeur::Date,
            ValueKind::YesNo => NatureValeur::OuiNon,
            ValueKind::Manga => NatureValeur::Manga,
            ValueKind::OnOff => NatureValeur::Activation,
            ValueKind::CustomValue => NatureValeur::ValeurPerso,
            ValueKind::Expression => NatureValeur::Expression,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operateur {
    pub op: i32,
    pub libelle: String,
    pub valeurs: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unite: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeCritere {
    #[serde(rename = "type")]
    pub type_critere: String,
    pub libelle: String,
    pub nature: NatureValeur,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unite: Option<String>,
    pub stat_serie: bool,
    pub evalue: bool,
    pub texte: bool,
    pub operateurs: Vec<Operateur>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorieCriteres {
    pub nom: String,
    pub criteres: Vec<TypeCritere>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CritereDefaut {
    #[serde(rename = "type")]
    pub type_critere: String,
    pub op: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueCriteres {
    pub categories: Vec<CategorieCriteres>,
    pub defaut: CritereDefaut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Critere {
    #[serde(default)]
    pub non: bool,
    #[serde(rename = "type")]
    pub type_critere: String,
    #[serde(default)]
    pub op: i32,
    #[serde(default)]
    pub valeur: String,
    #[serde(default)]
    pub valeur2: String,
    #[serde(default = "vrai")]
    pub ignorer_casse: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origine: Option<String>,
    #[serde(default = "vrai")]
    pub modifiable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libelle: Option<String>,
}

fn vrai() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupeCriteres {
    #[serde(default)]
    pub non: bool,
    pub mode: ModeCriteres,
    #[serde(default)]
    pub noeuds: Vec<NoeudCritere>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origine: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "genre")]
pub enum NoeudCritere {
    #[serde(rename = "critere")]
    Critere(Critere),
    #[serde(rename = "groupe")]
    Groupe(GroupeCriteres),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UniteLimite {
    Livres,
    Mo,
    Go,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionLimite {
    Hasard,
    Position,
    Serie,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimiteCriteres {
    pub valeur: String,
    pub unite: UniteLimite,
    pub selection: SelectionLimite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriteresListe {
    #[serde(default)]
    pub nom: String,
    #[serde(default)]
    pub description: String,
    pub mode: ModeCriteres,
    #[serde(default)]
    pub noeuds: Vec<NoeudCritere>,
    #[serde(default)]
    pub depart: Option<String>,
    #[serde(default)]
    pub pas_dans_depart: bool,
    #[serde(default)]
    pub limite: Option<LimiteCriteres>,
    #[serde(default)]
    pub retires: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reinitialiser_retires: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeDepart {
    pub id: String,
    pub nom: String,
    pub profondeur: usize,
    #[serde(rename = "type")]
    pub type_liste: TypeListe,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditeurListe {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub criteres: CriteresListe,
    pub listes_depart: Vec<ListeDepart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lecture_seule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Apercu {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nb_livres: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inconnu: Option<String>,
    pub erreurs: Vec<String>,
}

// ───────────────────────────── Catalogue ─────────────────────────────

/// Catégorie d'un critère dans la liste des champs (comme egui).
fn categorie(i: &MatcherInfo) -> &'static str {
    if i.series_stat {
        return "Statistiques de série";
    }
    match i.value {
        ValueKind::Text | ValueKind::CustomValue => "Texte",
        ValueKind::Number => "Nombres",
        ValueKind::Date => "Dates",
        ValueKind::YesNo | ValueKind::Manga | ValueKind::OnOff => "Oui / Non",
        ValueKind::Expression => "Modules de ComicRack",
    }
}

const CATEGORIES: [&str; 6] =
    ["Texte", "Nombres", "Dates", "Oui / Non", "Statistiques de série", "Modules de ComicRack"];

/// Clé de tri d'un libellé (casse et accents ignorés).
fn cle_tri(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

fn type_critere(i: &MatcherInfo) -> TypeCritere {
    TypeCritere {
        type_critere: i.kind.to_string(),
        libelle: i.label.to_string(),
        nature: NatureValeur::depuis(i.value),
        unite: i.unit.map(str::to_string),
        stat_serie: i.series_stat,
        evalue: i.evaluated(),
        texte: i.value.is_text(),
        operateurs: i
            .operators()
            .iter()
            .map(|o| Operateur {
                op: o.op,
                libelle: o.label.to_string(),
                valeurs: o.values,
                unite: o.unit.map(str::to_string),
            })
            .collect(),
    }
}

/// Champs par catégorie, triés par libellé.
pub fn catalogue_criteres() -> CatalogueCriteres {
    let categories = CATEGORIES
        .iter()
        .map(|cat| {
            let mut v: Vec<&MatcherInfo> = matcher_catalog().iter().filter(|i| categorie(i) == *cat).collect();
            v.sort_by_key(|i| cle_tri(i.label));
            CategorieCriteres { nom: cat.to_string(), criteres: v.into_iter().map(type_critere).collect() }
        })
        .filter(|c| !c.criteres.is_empty())
        .collect();
    CatalogueCriteres { categories, defaut: CritereDefaut { type_critere: TYPE_DEFAUT.into(), op: OP_DEFAUT } }
}

fn critere_vide(type_critere: &str, op: i32) -> Critere {
    Critere {
        non: false,
        type_critere: type_critere.to_string(),
        op,
        valeur: String::new(),
        valeur2: String::new(),
        ignorer_casse: true,
        origine: None,
        modifiable: true,
        libelle: None,
    }
}

/// Changement de champ d'un critère : opérateur et valeurs gardés si la nature de la valeur est
/// la même (`Rule::set_kind` de la version egui).
pub fn nouveau_critere(type_critere: &str, ancien: Option<Critere>) -> Result<Critere, String> {
    let info = matcher_info(type_critere).ok_or_else(|| format!("critère inconnu : {type_critere}"))?;
    let Some(mut c) = ancien else {
        let op = if type_critere == TYPE_DEFAUT { OP_DEFAUT } else { 0 };
        return Ok(critere_vide(type_critere, op));
    };
    let avant = matcher_info(&c.type_critere).map(|i| i.value);
    c.type_critere = type_critere.to_string();
    c.modifiable = true;
    c.libelle = None;
    if avant != Some(info.value) {
        c.op = 0;
        c.valeur.clear();
        c.valeur2.clear();
    } else if info.operator(c.op).is_none() {
        c.op = 0;
    }
    Ok(c)
}

// ───────────────────────────── Conversions ─────────────────────────────

fn chemin(parent: &str, i: usize) -> String {
    if parent.is_empty() {
        i.to_string()
    } else {
        format!("{parent}/{i}")
    }
}

/// Critères enregistrés → noeuds de l'éditeur (avec leur origine).
fn noeuds_depuis(matchers: &[Matcher], parent: &str) -> Vec<NoeudCritere> {
    matchers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let origine = chemin(parent, i);
            match m {
                Matcher::Group { mode, not, matchers, .. } => NoeudCritere::Groupe(GroupeCriteres {
                    non: *not,
                    mode: ModeCriteres::depuis(*mode),
                    noeuds: noeuds_depuis(matchers, &origine),
                    origine: Some(origine),
                }),
                Matcher::Value { .. } => NoeudCritere::Critere(critere_depuis(m, Some(origine))),
            }
        })
        .collect()
}

/// Critère enregistré → saisie (`node_from_matcher` de la version egui).
fn critere_depuis(m: &Matcher, origine: Option<String>) -> Critere {
    let Matcher::Value { kind, op, value, value2, not, ignore_case, .. } = m else {
        return critere_vide(TYPE_DEFAUT, OP_DEFAUT);
    };
    let Some(info) = matcher_info(kind).filter(|i| i.operator(*op).is_some()) else {
        return Critere {
            non: *not,
            type_critere: kind.clone(),
            op: *op,
            valeur: value.clone(),
            valeur2: value2.clone(),
            ignorer_casse: *ignore_case,
            origine,
            modifiable: false,
            libelle: Some(describe_kind(kind)),
        };
    };
    let jours = info.operator(*op).is_some_and(|o| o.unit.is_some());
    let montrer = |v: &str| if info.value == ValueKind::Date && !jours { afficher_date(v) } else { v.to_string() };
    Critere {
        non: *not,
        type_critere: kind.clone(),
        op: *op,
        valeur: montrer(value),
        valeur2: montrer(value2),
        ignorer_casse: *ignore_case,
        origine,
        modifiable: true,
        libelle: None,
    }
}

/// Même saisie (case Non, champ, opérateur, valeurs, casse) ?
fn meme_saisie(a: &Critere, b: &Critere) -> bool {
    (a.non, &a.type_critere, a.op, &a.valeur, &a.valeur2, a.ignorer_casse)
        == (b.non, &b.type_critere, b.op, &b.valeur, &b.valeur2, b.ignorer_casse)
}

/// Critère d'origine d'après son chemin dans les critères enregistrés.
fn trouver<'a>(matchers: &'a [Matcher], origine: Option<&str>) -> Option<&'a Matcher> {
    let o = origine?;
    let mut cur = matchers;
    let mut found = None;
    for part in o.split('/') {
        let i: usize = part.parse().ok()?;
        let m = cur.get(i)?;
        found = Some(m);
        cur = match m {
            Matcher::Group { matchers, .. } => matchers,
            Matcher::Value { .. } => &[],
        };
    }
    found
}

/// Notes (0 à 5).
fn est_note(kind: &str) -> bool {
    matches!(kind, "ComicBookRatingMatcher" | "ComicBookCommunityRatingMatcher")
        || kind.starts_with("SmartListSeriesAverage")
}

/// Contrôle et conversion d'un critère saisi (`rule_to_matcher` de la version egui).
fn critere_vers(c: &Critere, origine: Option<&Matcher>) -> Result<Matcher, String> {
    let orig = origine.filter(|m| matches!(m, Matcher::Value { .. }));
    // Critère absent du catalogue : toujours recopié tel quel… à condition que l'origine désigne
    // toujours ce critère (liste modifiée dans ComicRack pendant que la fenêtre était ouverte :
    // le chemin « 0/2 » pourrait désigner un autre critère, qui serait alors dupliqué).
    if !c.modifiable || matcher_info(&c.type_critere).is_none() {
        return match orig {
            Some(o @ Matcher::Value { kind, op, .. }) if *kind == c.type_critere && *op == c.op => Ok(o.clone()),
            Some(_) => Err("la liste a changé depuis l'ouverture de la fenêtre (dans ComicRack ?) : ferme et \
                            rouvre l'éditeur"
                .into()),
            None => Err(format!("critère inconnu ({})", c.type_critere)),
        };
    }
    // Critère existant non modifié : rendu tel quel, jamais réécrit.
    if let Some(o) = orig {
        if meme_saisie(&critere_depuis(o, None), c) {
            return Ok(o.clone());
        }
    }
    let info = matcher_info(&c.type_critere).ok_or_else(|| format!("critère inconnu ({})", c.type_critere))?;
    let op = info.operator(c.op).ok_or_else(|| format!("opérateur {} inconnu", c.op))?;
    let controler = |v: &str| -> Result<String, String> {
        match info.value {
            ValueKind::Number => {
                let n = lire_nombre(v)?;
                if est_note(&c.type_critere)
                    && !(0.0..=5.0).contains(&n.replace(',', ".").parse::<f64>().unwrap_or(-1.0))
                {
                    return Err(format!("la note doit être entre 0 et 5 (« {} »)", v.trim()));
                }
                Ok(n)
            }
            ValueKind::Date if op.unit.is_some() => lire_jours(v),
            ValueKind::Date => lire_date(v),
            ValueKind::Text if c.op == 7 => {
                expression_ok(v)?;
                Ok(v.to_string())
            }
            _ => Ok(v.to_string()),
        }
    };
    let (v1, v2) = match (info.value, op.values) {
        (ValueKind::CustomValue, _) => {
            if c.valeur.trim().is_empty() {
                return Err("nom de la valeur personnalisée manquant".into());
            }
            if c.op == 7 {
                expression_ok(&c.valeur2)?;
            }
            (c.valeur.trim().to_string(), c.valeur2.clone())
        }
        (_, 0) => (String::new(), String::new()),
        (_, 1) => (controler(&c.valeur)?, String::new()),
        _ => (controler(&c.valeur)?, controler(&c.valeur2)?),
    };
    let m = Matcher::value(&c.type_critere, c.op, &v1, &v2, c.non, c.ignorer_casse);
    // Critère modifié : ses attributs et sous-éléments inconnus (`Name`…) sont gardés.
    Ok(match orig {
        Some(o) => m.with_xml(o.xml().clone()),
        None => m,
    })
}

fn noeuds_vers(
    noeuds: &[NoeudCritere],
    origine: &[Matcher],
    compteur: &mut usize,
    erreurs: &mut Vec<String>,
) -> Vec<Matcher> {
    let mut out = Vec::new();
    for n in noeuds {
        match n {
            NoeudCritere::Critere(c) => {
                *compteur += 1;
                match critere_vers(c, trouver(origine, c.origine.as_deref())) {
                    Ok(m) => out.push(m),
                    Err(e) => {
                        let libelle = matcher_info(&c.type_critere)
                            .map(|i| i.label.to_string())
                            .unwrap_or_else(|| describe_kind(&c.type_critere));
                        erreurs.push(format!("Critère {} ({libelle}) : {e}", *compteur));
                    }
                }
            }
            NoeudCritere::Groupe(g) => {
                let xml = match trouver(origine, g.origine.as_deref()) {
                    Some(Matcher::Group { xml, .. }) => xml.clone(),
                    _ => MatcherXml::default(),
                };
                out.push(Matcher::Group {
                    mode: g.mode.vers(),
                    not: g.non,
                    matchers: noeuds_vers(&g.noeuds, origine, compteur, erreurs),
                    xml,
                });
            }
        }
    }
    out
}

/// Critères saisis → requête prête à enregistrer (`Draft::to_query`), ou les erreurs de saisie.
/// `origine` : critères enregistrés de la liste (pour garder ce qui n'a pas changé, la graine
/// du hasard et les livres retirés à la main).
pub fn vers_requete(c: &CriteresListe, origine: Option<&SmartQuery>) -> Result<SmartQuery, Vec<String>> {
    let vide = Vec::new();
    let anciens = origine.map(|q| &q.matchers).unwrap_or(&vide);
    let mut erreurs = Vec::new();
    let mut compteur = 0;
    let matchers = noeuds_vers(&c.noeuds, anciens, &mut compteur, &mut erreurs);
    let graine = origine.and_then(|q| q.limit.as_ref()).map(|l| l.seed).unwrap_or(0);
    let limit = match &c.limite {
        None => None,
        Some(l) => match l.valeur.trim().parse::<i64>() {
            Ok(v) if v >= 0 => Some(Limit {
                kind: match l.unite {
                    UniteLimite::Livres => LimitKind::Count,
                    UniteLimite::Mo => LimitKind::Mb,
                    UniteLimite::Go => LimitKind::Gb,
                },
                value: v,
                selection: match l.selection {
                    SelectionLimite::Hasard => Selection::Random,
                    SelectionLimite::Position => Selection::Position,
                    SelectionLimite::Serie => Selection::SortedBySeries,
                },
                seed: graine,
            }),
            _ => {
                erreurs.push(format!("Limite : « {} » n'est pas un nombre entier positif", l.valeur.trim()));
                None
            }
        },
    };
    if !erreurs.is_empty() {
        return Err(erreurs);
    }
    let base = c.depart.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(str::to_ascii_lowercase);
    let filtered = if c.reinitialiser_retires == Some(true) {
        Vec::new()
    } else {
        origine.map(|q| q.filtered.clone()).unwrap_or_default()
    };
    Ok(SmartQuery {
        mode: c.mode.vers(),
        matchers,
        not_in_base: c.pas_dans_depart && base.is_some(),
        base_list: base,
        limit,
        filtered,
    })
}

/// Requête enregistrée → saisie (`Draft::from_query`).
pub fn depuis_requete(q: &SmartQuery, nom: &str, description: &str) -> CriteresListe {
    CriteresListe {
        nom: nom.to_string(),
        description: description.to_string(),
        mode: ModeCriteres::depuis(q.mode),
        noeuds: noeuds_depuis(&q.matchers, ""),
        depart: q.base_list.clone().filter(|b| !b.trim().is_empty()).map(|b| b.trim().to_ascii_lowercase()),
        pas_dans_depart: q.not_in_base,
        limite: q.limit.as_ref().map(|l| LimiteCriteres {
            valeur: l.value.to_string(),
            unite: match l.kind {
                LimitKind::Count => UniteLimite::Livres,
                LimitKind::Mb => UniteLimite::Mo,
                LimitKind::Gb => UniteLimite::Go,
            },
            selection: match l.selection {
                Selection::Random => SelectionLimite::Hasard,
                Selection::Position => SelectionLimite::Position,
                Selection::SortedBySeries => SelectionLimite::Serie,
            },
        }),
        retires: q.filtered.len(),
        reinitialiser_retires: None,
    }
}

// ───────────────────────────── Valeurs ─────────────────────────────

/// Vérification sommaire d'une expression régulière (le moteur exact est celui de .NET).
fn expression_ok(v: &str) -> Result<(), String> {
    let (mut paren, mut crochet, mut echappe) = (0i32, false, false);
    let r: Result<(), String> = (|| {
        for c in v.chars() {
            if echappe {
                echappe = false;
                continue;
            }
            match c {
                '\\' => echappe = true,
                '[' if !crochet => crochet = true,
                ']' if crochet => crochet = false,
                '(' if !crochet => paren += 1,
                ')' if !crochet => {
                    paren -= 1;
                    if paren < 0 {
                        return Err("« ) » sans « ( »".into());
                    }
                }
                _ => {}
            }
        }
        if paren > 0 {
            Err("« ( » non fermée".into())
        } else if crochet {
            Err("« [ » non fermé".into())
        } else if echappe {
            Err("« \\ » en fin d'expression".into())
        } else {
            Ok(())
        }
    })();
    r.map_err(|e| format!("expression régulière invalide : {e}"))
}

/// Nombre saisi → texte écrit (entier tel quel ; décimal avec le séparateur du poste, relu
/// par ComicRack avec la culture de Windows).
pub fn lire_nombre(v: &str) -> Result<String, String> {
    lire_nombre_avec(v, comicrust_core::locale::decimal_separator())
}

pub fn lire_nombre_avec(v: &str, separateur: char) -> Result<String, String> {
    let t = v.trim().replace(',', ".");
    if t.is_empty() {
        return Err("nombre manquant".into());
    }
    if let Ok(i) = t.parse::<i64>() {
        return Ok(i.to_string());
    }
    match t.parse::<f64>() {
        Ok(f) if f.is_finite() && t.matches('.').count() == 1 => Ok(t.replace('.', &separateur.to_string())),
        _ => Err(format!("« {} » n'est pas un nombre", v.trim())),
    }
}

/// Date `AAAA-MM-JJ` (lue par ComicRack quelle que soit la langue du poste).
pub fn lire_date(v: &str) -> Result<String, String> {
    let t = v.trim();
    NaiveDate::parse_from_str(t, "%Y-%m-%d")
        .map(|d| d.format("%Y-%m-%d").to_string())
        .map_err(|_| format!("date attendue au format AAAA-MM-JJ (« {t} »)"))
}

/// Nombre de jours (« dans les derniers … jours »).
pub fn lire_jours(v: &str) -> Result<String, String> {
    match v.trim().parse::<u32>() {
        Ok(n) => Ok(n.to_string()),
        Err(_) => Err(format!("nombre de jours attendu (« {} »)", v.trim())),
    }
}

/// Date enregistrée par ComicRack (`01/10/2019`, `2019-10-01T…`) montrée en `AAAA-MM-JJ`.
pub fn afficher_date(v: &str) -> String {
    let t = v.trim();
    let premier = t.split([' ', 'T']).next().unwrap_or(t);
    for fmt in ["%Y-%m-%d", "%d/%m/%Y", "%d.%m.%Y", "%d-%m-%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(premier, fmt) {
            return d.format("%Y-%m-%d").to_string();
        }
    }
    t.to_string()
}

// ───────────────────────────── Listes de départ, aperçu ─────────────────────────────

/// Listes de départ possibles pour la liste `editee` (`None` : nouvelle) : toutes sauf la
/// « Bibliothèque », elle-même et celles qui dépendent d'elle (`base_candidates` egui).
pub fn listes_depart(items: &[ComicList], editee: Option<&str>) -> Vec<ListeDepart> {
    flatten(items)
        .into_iter()
        .filter(|(l, _)| l.kind != ListKind::Library)
        .filter(|(l, _)| editee.is_none_or(|e| !depends_on(items, &l.id, e)))
        .map(|(l, profondeur)| ListeDepart {
            id: l.id.to_ascii_lowercase(),
            nom: l.name.clone(),
            profondeur,
            type_liste: match l.kind {
                ListKind::Library => TypeListe::Bibliotheque,
                ListKind::Folder { .. } => TypeListe::Dossier,
                ListKind::Ids(_) => TypeListe::Lecture,
                ListKind::Smart { .. } => TypeListe::Intelligente,
            },
        })
        .collect()
}

/// Copie des listes où la liste éditée (ou une nouvelle liste) a les critères `q`.
fn listes_avec(items: &[ComicList], id: Option<&str>, q: &SmartQuery) -> (Vec<ComicList>, String) {
    fn remplacer(l: &mut [ComicList], id: &str, q: &SmartQuery) -> bool {
        for x in l {
            if x.id.eq_ignore_ascii_case(id) {
                x.kind = ListKind::Smart { query: Some(q.clone()), cached: None };
                return true;
            }
            if let ListKind::Folder { children, .. } = &mut x.kind {
                if remplacer(children, id, q) {
                    return true;
                }
            }
        }
        false
    }
    let mut out = items.to_vec();
    if let Some(id) = id {
        if remplacer(&mut out, id, q) {
            return (out, id.to_string());
        }
    }
    let tmp = "comicrust-apercu".to_string();
    out.push(ComicList {
        id: tmp.clone(),
        name: String::new(),
        description: String::new(),
        kind: ListKind::Smart { query: Some(q.clone()), cached: None },
        comicrack_count: 0,
    });
    (out, tmp)
}

/// Critères enregistrés d'une liste intelligente.
fn requete_de(items: &[ComicList], id: &str) -> Result<(ComicList, SmartQuery), String> {
    let l = ComicList::find(items, id).ok_or_else(|| format!("liste introuvable : {id}"))?;
    match &l.kind {
        ListKind::Smart { query, .. } => Ok((l.clone(), query.clone().unwrap_or_default())),
        _ => Err(format!("« {} » n'est pas une liste intelligente", l.name)),
    }
}

impl Biblio {
    /// Fenêtre de l'éditeur : liste existante (`id`) ou nouvelle.
    pub fn lire_criteres(&self, id: Option<&str>) -> Result<EditeurListe, String> {
        self.rafraichir_listes(false);
        let (items, _) = self.listes_lues();
        let id = id.map(str::trim).filter(|i| !i.is_empty());
        let (criteres, id) = match id {
            Some(id) => {
                let (l, q) = requete_de(&items, id)?;
                (depuis_requete(&q, &l.name, &l.description), Some(l.id))
            }
            None => {
                let mut c = depuis_requete(&SmartQuery::default(), NOM_DEFAUT, "");
                c.noeuds = vec![NoeudCritere::Critere(critere_vide(TYPE_DEFAUT, OP_DEFAUT))];
                (c, None)
            }
        };
        let lecture_seule = self.etat_listes().lecture_seule;
        Ok(EditeurListe { listes_depart: listes_depart(&items, id.as_deref()), id, criteres, lecture_seule })
    }

    /// Aperçu : erreurs de saisie, sinon nombre de livres trouvés.
    pub fn apercu_criteres(&self, id: Option<&str>, c: &CriteresListe) -> Result<Apercu, String> {
        self.rafraichir_listes(false);
        let (items, _) = self.listes_lues();
        let id = id.map(str::trim).filter(|i| !i.is_empty());
        let origine = match id {
            Some(id) => Some(requete_de(&items, id)?.1),
            None => None,
        };
        let q = match vers_requete(c, origine.as_ref()) {
            Ok(q) => q,
            Err(erreurs) => return Ok(Apercu { nb_livres: None, inconnu: None, erreurs }),
        };
        let base = self.base()?;
        let (listes, cle) = listes_avec(&items, id, &q);
        Ok(match ComicList::find(&listes, &cle).map(|l| l.resolve(&base, &listes)) {
            Some(Members::All) => Apercu { nb_livres: Some(base.len()), inconnu: None, erreurs: Vec::new() },
            Some(Members::Ids(ids)) => Apercu {
                nb_livres: Some(ids.iter().filter(|i| base.index_of(i).is_some()).count()),
                inconnu: None,
                erreurs: Vec::new(),
            },
            Some(Members::Unknown(why)) => Apercu { nb_livres: None, inconnu: Some(why), erreurs: Vec::new() },
            None => Apercu { nb_livres: None, inconnu: Some("liste introuvable".into()), erreurs: Vec::new() },
        })
    }

    /// Enregistre les critères (liste existante : seul ce qui a changé est réécrit).
    pub fn enregistrer_criteres(
        &self,
        id: Option<&str>,
        c: &CriteresListe,
        parent: Option<String>,
        position: Option<usize>,
        apres_liste: Option<&str>,
    ) -> Result<ResultatListes, String> {
        let nom = c.nom.trim().to_string();
        if nom.is_empty() {
            return Err("le nom de la liste ne peut pas être vide".into());
        }
        // Contrôle de la saisie avant toute écriture (critères relus de la liste au moment de
        // l'écriture ensuite).
        self.rafraichir_listes(false);
        let (items, _) = self.listes_lues();
        let id = id.map(str::trim).filter(|i| !i.is_empty()).map(str::to_string);
        if let Some(i) = &id {
            let origine = requete_de(&items, i)?.1;
            vers_requete(c, Some(&origine)).map_err(|e| e.join("\n"))?;
        } else {
            vers_requete(c, None).map_err(|e| e.join("\n"))?;
        }
        let c = c.clone();
        match id {
            Some(id) => self.executer(format!("Modification de « {nom} »"), move |ed| {
                let liste = ed.lists()?;
                let (l, ancienne) = requete_de(&liste, &id).map_err(comicrust_core::Error::Database)?;
                let q = vers_requete(&c, Some(&ancienne)).map_err(|e| comicrust_core::Error::Database(e.join("\n")))?;
                if nom != l.name {
                    ed.rename(&id, &nom)?;
                }
                if c.description != l.description {
                    ed.set_description(&id, &c.description)?;
                }
                if q != ancienne {
                    ed.set_query(&id, &q)?;
                }
                Ok(Issue::default())
            }),
            None => {
                let (parent, position) = self.emplacement(parent, position, apres_liste);
                self.executer(format!("Création de « {nom} »"), move |ed| {
                    let q = vers_requete(&c, None).map_err(|e| comicrust_core::Error::Database(e.join("\n")))?;
                    let id = ed.create(parent.as_deref(), position, &nom, NewList::Smart { query: q })?;
                    if !c.description.is_empty() {
                        ed.set_description(&id, &c.description)?;
                    }
                    Ok(Issue { selection: Some(Some(id.clone())), creee: Some(id), ..Default::default() })
                })
            }
        }
    }
}
