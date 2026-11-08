//! Onglet « Pages » de la fenêtre « Infos du livre » : liste des pages d'un livre, opérations
//! (déplacer, type, rotation, supprimée, réinitialisations…) et préparation de la liste à
//! enregistrer. Contrat : `docs/tauri/ipc-pages.md`.
//!
//! Repris de la version egui (`ui/info_pages.rs`, **dupliqué** : le plan interdit de toucher à
//! `crates/comicrust`) ; la logique elle-même est celle de `comicrust_core::pages` (sémantique de
//! `PagesView` / `ComicPageInfo` de ComicRack). Le front ne fait qu'afficher et envoyer des
//! intentions : chaque opération lui rend la nouvelle liste. Aucune dépendance à Tauri.

use comicrust_core::library::PageInfo as PageFiche;
use comicrust_core::pages::{self, PAGE_TYPES, POSITIONS, ROTATIONS};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Largeur demandée pour les vignettes (`l=` du protocole `cr-page`) : sous le seuil des
/// miniatures du lecteur (pas de préchargement).
pub const LARGEUR_VIGNETTE: u32 = 192;

/// Choix d'une liste (type, rotation, position).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choix {
    pub valeur: String,
    pub libelle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raccourci: Option<String>,
}

/// Une page du livre (voir le contrat). Reçue aussi en entrée de [`transformer`] : les champs
/// calculés (`index`, `libelleType`, `supprimee`) y sont ignorés.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLivre {
    #[serde(default)]
    pub index: usize,
    pub image: i64,
    #[serde(default)]
    pub nom: String,
    #[serde(rename = "type", default = "histoire")]
    pub type_page: String,
    #[serde(default)]
    pub libelle_type: String,
    #[serde(default)]
    pub supprimee: bool,
    #[serde(default)]
    pub double: bool,
    #[serde(default)]
    pub rotation: u16,
    #[serde(default = "par_defaut")]
    pub position: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favori: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub largeur: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hauteur: Option<u32>,
    #[serde(default)]
    pub url_vignette: String,
}

fn histoire() -> String {
    "Story".into()
}

fn par_defaut() -> String {
    "Default".into()
}

/// Résultat de `lire_pages`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagesLivre {
    pub id: String,
    pub jeton: String,
    pub url_base: String,
    pub largeur_vignette: u32,
    pub nb_images: usize,
    pub pages: Vec<PageLivre>,
    pub modifiable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lecture_seule: Option<String>,
    pub ecrire_dans_fichier_possible: bool,
    pub types: Vec<Choix>,
    pub rotations: Vec<Choix>,
    pub positions: Vec<Choix>,
}

/// Opération de l'onglet (menu contextuel, raccourcis, glisser-déposer, boutons).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OperationPages {
    Deplacer {
        images: Vec<i64>,
        position: usize,
    },
    MettreAuDebut {
        images: Vec<i64>,
    },
    MettreALaFin {
        images: Vec<i64>,
    },
    Type {
        images: Vec<i64>,
        valeur: String,
    },
    BasculerSupprimee {
        images: Vec<i64>,
    },
    Rotation {
        images: Vec<i64>,
        valeur: u16,
    },
    Position {
        images: Vec<i64>,
        valeur: String,
    },
    Favori {
        image: i64,
        #[serde(default)]
        texte: Option<String>,
    },
    OrdreOrigine,
    TrierParNom {
        #[serde(default)]
        numerique: bool,
    },
    ReinitialiserTypes,
    ToutReinitialiser,
}

/// Page envoyée à l'enregistrement (un `PageLivre` convient : champs en trop ignorés).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageModifiee {
    pub image: i64,
    #[serde(rename = "type", default)]
    pub type_page: Option<String>,
    #[serde(default)]
    pub supprimee: Option<bool>,
    #[serde(default)]
    pub rotation: Option<u16>,
    #[serde(default)]
    pub position: Option<String>,
    /// Absent : gardé ; `null` ou vide : supprimé.
    #[serde(default, deserialize_with = "option_presente")]
    pub favori: Option<Option<String>>,
}

/// Distingue un champ absent (`None`) d'un `null` explicite (`Some(None)`).
fn option_presente<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

// ───────────────────────────── Listes de choix ─────────────────────────────

pub fn choix_types() -> Vec<Choix> {
    PAGE_TYPES
        .iter()
        .map(|(v, l, k)| Choix {
            valeur: v.to_string(),
            libelle: l.to_string(),
            raccourci: Some(format!("Alt+Maj+{k}")),
        })
        .collect()
}

pub fn choix_rotations() -> Vec<Choix> {
    ROTATIONS
        .iter()
        .map(|(v, l, k)| Choix {
            valeur: degres_de(v).to_string(),
            libelle: l.to_string(),
            raccourci: Some(format!("Alt+Maj+{k}")),
        })
        .collect()
}

pub fn choix_positions() -> Vec<Choix> {
    POSITIONS.iter().map(|(v, l)| Choix { valeur: v.to_string(), libelle: l.to_string(), raccourci: None }).collect()
}

// ───────────────────────────── Conversions ─────────────────────────────

/// Rotation ComicRack (`Rotate90`…) → degrés.
fn degres_de(r: &str) -> u16 {
    match r {
        "Rotate90" => 90,
        "Rotate180" => 180,
        "Rotate270" => 270,
        _ => 0,
    }
}

/// Degrés → rotation ComicRack ; `Err` si la valeur n'est pas 0, 90, 180 ou 270.
fn rotation_de(degres: u16) -> Result<&'static str, String> {
    Ok(match degres {
        0 => "None",
        90 => "Rotate90",
        180 => "Rotate180",
        270 => "Rotate270",
        d => return Err(format!("rotation refusée : {d}° (0, 90, 180 ou 270)")),
    })
}

fn verifier_type(t: &str) -> Result<(), String> {
    if t.split(',').map(str::trim).all(|x| PAGE_TYPES.iter().any(|(v, _, _)| *v == x)) {
        Ok(())
    } else {
        Err(format!("type de page inconnu : « {t} »"))
    }
}

fn verifier_position(p: &str) -> Result<(), String> {
    if POSITIONS.iter().any(|(v, _)| *v == p) {
        Ok(())
    } else {
        Err(format!("position inconnue : « {p} » (Default, Near ou Far)"))
    }
}

fn dimension(v: &Option<String>) -> Option<u32> {
    v.as_deref().and_then(|s| s.trim().parse::<u32>().ok()).filter(|n| *n > 0)
}

/// Pages de la fiche → pages de l'onglet (liste déjà complète, dans l'ordre de lecture).
/// `noms` : images du fichier ; `doubles` : images `DoublePage` du ComicInfo.xml.
pub fn vers_pages_livre(
    liste: &[PageFiche],
    noms: &[String],
    doubles: &HashSet<usize>,
    url_base: &str,
) -> Vec<PageLivre> {
    liste
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let image = p.image.max(0) as usize;
            let (largeur, hauteur) = (dimension(&p.image_width), dimension(&p.image_height));
            let k = pages::kind(p).to_string();
            PageLivre {
                index,
                image: image as i64,
                nom: noms.get(image).cloned().unwrap_or_default(),
                libelle_type: pages::kind_label(&k),
                supprimee: pages::is_type(p, "Deleted"),
                type_page: k,
                // ComicRack : `ComicPageInfo.IsDoublePage => ImageWidth > ImageHeight`.
                double: doubles.contains(&image) || matches!((largeur, hauteur), (Some(l), Some(h)) if l > h),
                rotation: pages::rotation_degrees(p),
                position: pages::position(p).to_string(),
                favori: p.bookmark.clone().filter(|b| !b.is_empty()),
                largeur,
                hauteur,
                url_vignette: format!("{url_base}/{image}?l={LARGEUR_VIGNETTE}"),
            }
        })
        .collect()
}

/// Page de l'onglet → page de la fiche (attributs modifiables seulement ; tailles gardées).
fn vers_fiche(p: &PageLivre) -> Result<PageFiche, String> {
    let t = p.type_page.trim();
    let t = if t.is_empty() { "Story" } else { t };
    verifier_type(t)?;
    verifier_position(&p.position)?;
    let r = rotation_de(p.rotation)?;
    Ok(PageFiche {
        image: i32::try_from(p.image).map_err(|_| format!("image invalide : {}", p.image))?,
        kind: (t != "Story").then(|| t.to_string()),
        rotation: (r != "None").then(|| r.to_string()),
        page_position: (p.position != "Default").then(|| p.position.clone()),
        bookmark: p.favori.clone().filter(|b| !b.trim().is_empty()),
        image_width: p.largeur.map(|v| v.to_string()),
        image_height: p.hauteur.map(|v| v.to_string()),
        ..Default::default()
    })
}

fn images32(images: &[i64]) -> Vec<i32> {
    images.iter().filter_map(|&i| i32::try_from(i).ok()).collect()
}

/// Applique une opération à la liste affichée et rend la nouvelle liste (index recalculés,
/// libellés à jour ; noms, vignettes et « double » repris de l'entrée). Rien n'est enregistré.
pub fn transformer(entree: &[PageLivre], op: &OperationPages) -> Result<Vec<PageLivre>, String> {
    let mut liste: Vec<PageFiche> = entree.iter().map(vers_fiche).collect::<Result<_, _>>()?;
    let mut vues = HashSet::new();
    if let Some(p) = liste.iter().find(|p| p.image < 0 || p.image as usize >= pages::MAX_PAGES || !vues.insert(p.image))
    {
        return Err(format!("liste de pages incohérente : image {} invalide ou présente deux fois", p.image));
    }
    let noms: Vec<String> = {
        let n = liste.iter().map(|p| p.image as usize + 1).max().unwrap_or(0);
        let mut v = vec![String::new(); n];
        for p in entree {
            v[p.image as usize] = p.nom.clone();
        }
        v
    };
    match op {
        OperationPages::Deplacer { images, position } => pages::move_pages(&mut liste, *position, &images32(images)),
        OperationPages::MettreAuDebut { images } => pages::move_to_start(&mut liste, &images32(images)),
        OperationPages::MettreALaFin { images } => pages::move_to_end(&mut liste, &images32(images)),
        OperationPages::Type { images, valeur } => {
            if !PAGE_TYPES.iter().any(|(v, _, _)| v == valeur) {
                return Err(format!("type de page inconnu : « {valeur} »"));
            }
            pages::set_type(&mut liste, &images32(images), valeur)
        }
        OperationPages::BasculerSupprimee { images } => pages::toggle_deleted(&mut liste, &images32(images)),
        OperationPages::Rotation { images, valeur } => {
            pages::set_rotation(&mut liste, &images32(images), rotation_de(*valeur)?)
        }
        OperationPages::Position { images, valeur } => {
            verifier_position(valeur)?;
            pages::set_position(&mut liste, &images32(images), valeur)
        }
        OperationPages::Favori { image, texte } => {
            if let Some(c) = texte.as_deref().and_then(|t| t.chars().find(|c| c.is_control())) {
                return Err(format!("caractère interdit dans le favori (U+{:04X})", c as u32));
            }
            if let Ok(i) = i32::try_from(*image) {
                pages::set_bookmark(&mut liste, i, texte.as_deref())
            }
        }
        OperationPages::OrdreOrigine => pages::reset_order(&mut liste),
        OperationPages::TrierParNom { numerique } => pages::sort_by_name(&mut liste, &noms, *numerique),
        OperationPages::ReinitialiserTypes => pages::reset_types(&mut liste),
        OperationPages::ToutReinitialiser => pages::reset_all(&mut liste),
    }
    let par_image: HashMap<i64, &PageLivre> = entree.iter().map(|p| (p.image, p)).collect();
    Ok(liste
        .iter()
        .enumerate()
        .map(|(index, f)| {
            let avant = par_image[&(f.image as i64)];
            let k = pages::kind(f).to_string();
            PageLivre {
                index,
                image: f.image as i64,
                nom: avant.nom.clone(),
                libelle_type: pages::kind_label(&k),
                supprimee: pages::is_type(f, "Deleted"),
                type_page: k,
                double: avant.double,
                rotation: pages::rotation_degrees(f),
                position: pages::position(f).to_string(),
                favori: f.bookmark.clone(),
                largeur: avant.largeur,
                hauteur: avant.hauteur,
                url_vignette: avant.url_vignette.clone(),
            }
        })
        .collect())
}

/// Nouvelle liste complète des pages à enregistrer : `stockees` (fiche) complétée pour
/// `total` images, réordonnée et modifiée d'après `modifiees` (chaque image une fois et une
/// seule). Les attributs non envoyés et non modifiables (tailles, `ImageSize`, `Key`) sont
/// repris de la fiche. `Ok(None)` : rien ne change (même liste après normalisation).
pub fn fusionner(
    stockees: &[PageFiche],
    total: usize,
    modifiees: &[PageModifiee],
) -> Result<Option<Vec<PageFiche>>, String> {
    if modifiees.len() != total {
        return Err(format!(
            "liste de pages incomplète : {} page(s) reçue(s) pour {total} image(s) dans le fichier",
            modifiees.len()
        ));
    }
    let base = pages::full_list(stockees, total);
    let par_image: HashMap<i32, &PageFiche> = base.iter().map(|p| (p.image, p)).collect();
    let mut vues = HashSet::new();
    let mut nouvelle = Vec::with_capacity(total);
    for m in modifiees {
        let image = i32::try_from(m.image).ok().filter(|i| par_image.contains_key(i));
        let Some(image) = image else {
            return Err(format!("image {} absente du fichier ({total} images)", m.image));
        };
        if !vues.insert(image) {
            return Err(format!("image {image} présente deux fois dans la liste des pages"));
        }
        let mut p = par_image[&image].clone();
        if let Some(t) = &m.type_page {
            let t = t.trim();
            let t = if t.is_empty() { "Story" } else { t };
            verifier_type(t)?;
            p.kind = (t != "Story").then(|| t.to_string());
        }
        match m.supprimee {
            Some(true) if !pages::is_type(&p, "Deleted") => p.kind = Some("Deleted".into()),
            Some(false) if pages::is_type(&p, "Deleted") => p.kind = None,
            _ => {}
        }
        if let Some(r) = m.rotation {
            let r = rotation_de(r)?;
            p.rotation = (r != "None").then(|| r.to_string());
        }
        if let Some(pos) = &m.position {
            verifier_position(pos)?;
            p.page_position = (pos != "Default").then(|| pos.clone());
        }
        if let Some(f) = &m.favori {
            p.bookmark = f.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(str::to_string);
        }
        nouvelle.push(p);
    }
    let nouvelle = pages::validate(&nouvelle).map_err(|e| format!("pages refusées : {e}"))?;
    Ok((!pages::same(&nouvelle, &base)).then_some(nouvelle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn livre(n: usize) -> Vec<PageLivre> {
        let noms: Vec<String> = (0..n).map(|i| format!("{:02}.png", i + 1)).collect();
        vers_pages_livre(&pages::full_list(&[], n), &noms, &HashSet::from([2]), "cr-page://localhost/j")
    }

    fn ordre(l: &[PageLivre]) -> Vec<i64> {
        l.iter().map(|p| p.image).collect()
    }

    #[test]
    fn pages_de_la_fiche_completees() {
        let l = livre(3);
        assert_eq!(ordre(&l), [0, 1, 2]);
        assert_eq!((l[0].type_page.as_str(), l[0].libelle_type.as_str()), ("FrontCover", "Couverture avant"));
        assert_eq!(l[1].type_page, "Story");
        assert!(l[2].double && !l[1].double, "DoublePage du ComicInfo.xml");
        assert_eq!(l[1].url_vignette, "cr-page://localhost/j/1?l=192");
        assert_eq!(l[1].nom, "02.png");
        // Taille connue en paysage : double (ComicRack : largeur > hauteur).
        let f = vec![PageFiche {
            image: 0,
            image_width: Some("300".into()),
            image_height: Some("200".into()),
            ..Default::default()
        }];
        assert!(vers_pages_livre(&f, &[], &HashSet::new(), "x")[0].double);
    }

    #[test]
    fn operations_comme_comicrack() {
        let l = livre(4);
        let l = transformer(&l, &OperationPages::Deplacer { images: vec![3], position: 0 }).unwrap();
        assert_eq!(ordre(&l), [3, 0, 1, 2]);
        assert_eq!(l.iter().map(|p| p.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
        let l = transformer(&l, &OperationPages::Type { images: vec![1], valeur: "Advertisement".into() }).unwrap();
        assert_eq!(l[2].libelle_type, "Publicité");
        let l = transformer(&l, &OperationPages::BasculerSupprimee { images: vec![2] }).unwrap();
        assert!(l[3].supprimee && l[3].type_page == "Deleted");
        let l = transformer(&l, &OperationPages::Rotation { images: vec![3, 0], valeur: 270 }).unwrap();
        assert_eq!((l[0].rotation, l[1].rotation, l[2].rotation), (270, 270, 0));
        assert!(transformer(&l, &OperationPages::Rotation { images: vec![0], valeur: 45 }).is_err());
        assert!(transformer(&l, &OperationPages::Type { images: vec![0], valeur: "Pub".into() }).is_err());
        let l = transformer(&l, &OperationPages::Favori { image: 1, texte: Some("Combat".into()) }).unwrap();
        assert_eq!(l[2].favori.as_deref(), Some("Combat"));
        let l = transformer(&l, &OperationPages::OrdreOrigine).unwrap();
        assert_eq!(ordre(&l), [0, 1, 2, 3]);
        assert!(l[2].supprimee, "types gardés");
        assert!(l[2].double && l[2].nom == "03.png", "noms et double repris");
        let l = transformer(&l, &OperationPages::MettreALaFin { images: vec![0] }).unwrap();
        assert_eq!(ordre(&l), [1, 2, 3, 0]);
        assert_eq!((l[0].type_page.as_str(), l[3].type_page.as_str()), ("FrontCover", "Other"));
        let l = transformer(&l, &OperationPages::ToutReinitialiser).unwrap();
        assert_eq!(ordre(&l), [0, 1, 2, 3]);
        assert!(l.iter().all(|p| p.rotation == 0 && p.favori.is_none()));
        // Doublon : refusé.
        let mut d = livre(2);
        d[1].image = 0;
        assert!(transformer(&d, &OperationPages::OrdreOrigine).is_err());
    }

    #[test]
    fn operations_en_json() {
        let op: OperationPages = serde_json::from_str(r#"{"op":"deplacer","images":[1,2],"position":0}"#).unwrap();
        assert_eq!(op, OperationPages::Deplacer { images: vec![1, 2], position: 0 });
        let op: OperationPages = serde_json::from_str(r#"{"op":"trierParNom","numerique":true}"#).unwrap();
        assert_eq!(op, OperationPages::TrierParNom { numerique: true });
        let op: OperationPages = serde_json::from_str(r#"{"op":"mettreALaFin","images":[0]}"#).unwrap();
        assert_eq!(op, OperationPages::MettreALaFin { images: vec![0] });
        let op: OperationPages = serde_json::from_str(r#"{"op":"basculerSupprimee","images":[0]}"#).unwrap();
        assert_eq!(op, OperationPages::BasculerSupprimee { images: vec![0] });
        let p = serde_json::to_value(&livre(1)[0]).unwrap();
        for k in [
            "index",
            "image",
            "nom",
            "type",
            "libelleType",
            "supprimee",
            "double",
            "rotation",
            "position",
            "urlVignette",
        ] {
            assert!(p.get(k).is_some(), "{k} absent : {p}");
        }
        // Un PageLivre renvoyé tel quel sert de PageModifiee.
        let m: PageModifiee = serde_json::from_value(p).unwrap();
        assert_eq!((m.image, m.type_page.as_deref(), m.favori), (0, Some("FrontCover"), None));
        let m: PageModifiee = serde_json::from_str(r#"{"image":1,"favori":null}"#).unwrap();
        assert_eq!(m.favori, Some(None));
    }

    #[test]
    fn fusion_garde_les_tailles_et_verifie_la_liste() {
        let stockees = vec![
            PageFiche {
                image: 0,
                kind: Some("FrontCover".into()),
                image_width: Some("100".into()),
                image_height: Some("150".into()),
                ..Default::default()
            },
            PageFiche { image: 1, key: Some("k1".into()), ..Default::default() },
        ];
        let m = |image, t: Option<&str>| PageModifiee {
            image,
            type_page: t.map(str::to_string),
            supprimee: None,
            rotation: None,
            position: None,
            favori: None,
        };
        // Même liste : rien à écrire.
        assert_eq!(fusionner(&stockees, 3, &[m(0, None), m(1, None), m(2, None)]).unwrap(), None);
        // Réordonnée, supprimée, tournée.
        let mut b = m(1, Some("Story"));
        b.supprimee = Some(true);
        b.rotation = Some(90);
        let n = fusionner(&stockees, 3, &[m(2, None), b, m(0, None)]).unwrap().unwrap();
        assert_eq!(n.iter().map(|p| p.image).collect::<Vec<_>>(), [2, 1, 0]);
        assert_eq!(n[1].kind.as_deref(), Some("Deleted"));
        assert_eq!(n[1].rotation.as_deref(), Some("Rotate90"));
        assert_eq!(n[1].key.as_deref(), Some("k1"), "clé gardée");
        assert_eq!(n[2].image_width.as_deref(), Some("100"), "taille gardée");
        // supprimee:false sur une page supprimée : Histoire.
        let mut s = stockees.clone();
        s[1].kind = Some("Deleted".into());
        let mut b = m(1, None);
        b.supprimee = Some(false);
        let n = fusionner(&s, 2, &[m(0, None), b]).unwrap().unwrap();
        assert_eq!(n[1].kind, None);
        // Incomplète, doublon, image hors fichier, valeurs inconnues : refusées.
        assert!(fusionner(&stockees, 3, &[m(0, None), m(1, None)]).unwrap_err().contains("incomplète"));
        assert!(fusionner(&stockees, 2, &[m(0, None), m(0, None)]).unwrap_err().contains("deux fois"));
        assert!(fusionner(&stockees, 2, &[m(0, None), m(5, None)]).is_err());
        assert!(fusionner(&stockees, 2, &[m(0, Some("Pub")), m(1, None)]).is_err());
        let mut r = m(1, None);
        r.rotation = Some(45);
        assert!(fusionner(&stockees, 2, &[m(0, None), r]).is_err());
    }
}
