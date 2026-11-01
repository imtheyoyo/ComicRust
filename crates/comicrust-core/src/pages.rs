//! Pages d'un livre : ordre de lecture, types, rotation, position, favoris — logique de
//! l'onglet « Pages » de la fenêtre « Infos du livre » et du lecteur.
//!
//! Sémantique de ComicRack (`ComicInfo`, `ComicPageInfo`, `PagesView`) :
//!
//! - la liste `<Pages>` donne l'**ordre de lecture** : la page n° `p` du livre est l'image
//!   `Pages[p].Image` du fichier ; au-delà de la liste, la page `p` est l'image `p`
//!   (`ComicInfo.GetPage`), de type « FrontCover » pour la première, « Story » sinon
//!   (`new ComicPageInfo(index)`) ;
//! - `Type` est un ensemble de drapeaux (`ComicPageType`), écrit « FrontCover, Story » ; la
//!   valeur par défaut « Story » n'est pas écrite ;
//! - les pages « Deleted » sont **masquées à la lecture** (filtre de pages par défaut de
//!   ComicRack : `ComicPageType.All`, qui exclut `Deleted`) ;
//! - `Rotation` (`None`, `Rotate90`, `Rotate180`, `Rotate270`) tourne la page à l'affichage ;
//! - `Bookmark` vide = pas de favori (le setter de ComicRack remplace "" par `null`).
//!
//! Toutes les fonctions désignent les pages par leur **image** (`PageInfo::image`), stable
//! quand l'ordre change, comme `PagesView` (sélection par `ImageIndex`).

use std::collections::HashSet;

use crate::library::PageInfo;

/// Types de page de ComicRack (`ComicPageType`, dans l'ordre du menu) : (valeur, libellé,
/// lettre du raccourci Alt+Maj+lettre).
pub const PAGE_TYPES: &[(&str, &str, char)] = &[
    ("FrontCover", "Couverture avant", 'A'),
    ("InnerCover", "Couverture intérieure", 'B'),
    ("Roundup", "Résumé", 'C'),
    ("Story", "Histoire", 'D'),
    ("Advertisement", "Publicité", 'E'),
    ("Editorial", "Éditorial", 'F'),
    ("Letters", "Courrier des lecteurs", 'G'),
    ("Preview", "Aperçu", 'H'),
    ("BackCover", "Couverture arrière", 'I'),
    ("Other", "Autre", 'J'),
    ("Deleted", "Supprimée", 'K'),
];

/// Rotations (`ImageRotation`) : (valeur, libellé, chiffre du raccourci Alt+Maj+chiffre).
pub const ROTATIONS: &[(&str, &str, char)] =
    &[("None", "Aucune", '6'), ("Rotate90", "90°", '7'), ("Rotate180", "180°", '8'), ("Rotate270", "270°", '9')];

/// Positions en double page (`ComicPagePosition`) : (valeur, libellé).
pub const POSITIONS: &[(&str, &str)] = &[("Default", "Par défaut"), ("Near", "Proche"), ("Far", "Éloignée")];

/// Nombre maximal de pages acceptées dans une liste (garde-fou).
pub const MAX_PAGES: usize = 100_000;

/// Type de la page (« Story » si absent), tel que ComicRack l'écrit.
pub fn kind(p: &PageInfo) -> &str {
    p.kind.as_deref().map(str::trim).filter(|k| !k.is_empty()).unwrap_or("Story")
}

/// La page a le type `t` (parmi ses drapeaux).
pub fn is_type(p: &PageInfo, t: &str) -> bool {
    kind(p).split(',').any(|x| x.trim() == t)
}

/// Libellé français d'un type (drapeaux combinés : libellés joints).
pub fn kind_label(k: &str) -> String {
    k.split(',')
        .map(|x| {
            let x = x.trim();
            PAGE_TYPES.iter().find(|(v, _, _)| *v == x).map_or(x, |(_, l, _)| l).to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Rotation de la page en degrés (0, 90, 180, 270).
pub fn rotation_degrees(p: &PageInfo) -> u16 {
    match p.rotation.as_deref().map(str::trim) {
        Some("Rotate90") => 90,
        Some("Rotate180") => 180,
        Some("Rotate270") => 270,
        _ => 0,
    }
}

/// Rotation (`None` si absente).
pub fn rotation(p: &PageInfo) -> &str {
    p.rotation.as_deref().map(str::trim).filter(|r| !r.is_empty()).unwrap_or("None")
}

/// Position en double page (`Default` si absente).
pub fn position(p: &PageInfo) -> &str {
    p.page_position.as_deref().map(str::trim).filter(|r| !r.is_empty()).unwrap_or("Default")
}

/// Page par défaut à la place `index` (`new ComicPageInfo(index)` de ComicRack).
pub fn default_page(index: usize) -> PageInfo {
    PageInfo { image: index as i32, kind: (index == 0).then(|| "FrontCover".to_string()), ..Default::default() }
}

/// Liste complète des pages d'un livre de `count` pages (`ComicInfo.GetPage` pour chaque
/// page) : la liste enregistrée, complétée par les pages par défaut. Les entrées qui
/// désignent une image absente du fichier ou déjà vue sont écartées (comme
/// `ComicPageInfoCollection.Consolidate`), puis les images manquantes sont ajoutées à la
/// fin : chaque image du fichier apparaît une fois et une seule.
pub fn full_list(pages: &[PageInfo], count: usize) -> Vec<PageInfo> {
    let mut seen = HashSet::new();
    let mut out: Vec<PageInfo> = Vec::with_capacity(count);
    for p in pages {
        if p.image >= 0 && (p.image as usize) < count && seen.insert(p.image) {
            out.push(p.clone());
        }
    }
    for i in 0..count {
        if !seen.contains(&(i as i32)) {
            // Page par défaut de sa place ; couverture avant seulement si c'est l'image 0
            // à la première place (cas d'une liste vide).
            let place = out.len();
            let mut d = default_page(place);
            if d.image != i as i32 {
                d = PageInfo { image: i as i32, ..Default::default() };
            }
            out.push(d);
        }
    }
    out
}

/// Normalise une page comme le ferait un aller-retour par ComicRack : valeurs par défaut
/// effacées (`Story`, `None`, `Default`, signet vide), drapeaux réécrits « A, B ».
pub fn normalize(p: &mut PageInfo) {
    let k = kind(p).to_string();
    let parts: Vec<&str> = k.split(',').map(str::trim).filter(|x| !x.is_empty()).collect();
    // Ordre des drapeaux de `Enum.ToString()` : ordre des valeurs.
    let ordered: Vec<&str> = PAGE_TYPES.iter().map(|(v, _, _)| *v).filter(|v| parts.contains(v)).collect::<Vec<_>>();
    let k = if ordered.len() == parts.len() { ordered.join(", ") } else { k };
    p.kind = (k != "Story").then_some(k);
    if rotation(p) == "None" {
        p.rotation = None;
    }
    if position(p) == "Default" {
        p.page_position = None;
    }
    if p.bookmark.as_deref().is_some_and(str::is_empty) {
        p.bookmark = None;
    }
}

/// Vérifie une liste de pages à enregistrer et la rend normalisée ([`normalize`]).
pub fn validate(pages: &[PageInfo]) -> Result<Vec<PageInfo>, String> {
    if pages.len() > MAX_PAGES {
        return Err(format!("trop de pages ({})", pages.len()));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(pages.len());
    for p in pages {
        if p.image < 0 {
            return Err(format!("image négative ({})", p.image));
        }
        if !seen.insert(p.image) {
            return Err(format!("image {} présente deux fois", p.image));
        }
        let mut p = p.clone();
        normalize(&mut p);
        let k = kind(&p);
        if let Some(bad) = k.split(',').map(str::trim).find(|x| !PAGE_TYPES.iter().any(|(v, _, _)| v == x)) {
            return Err(format!("page {} : type inconnu « {bad} »", p.image));
        }
        if !ROTATIONS.iter().any(|(v, _, _)| *v == rotation(&p)) {
            return Err(format!("page {} : rotation inconnue « {} »", p.image, rotation(&p)));
        }
        if !POSITIONS.iter().any(|(v, _)| *v == position(&p)) {
            return Err(format!("page {} : position inconnue « {} »", p.image, position(&p)));
        }
        for (what, v) in [("signet", &p.bookmark), ("clé", &p.key)] {
            if let Some(c) = v.as_deref().and_then(|s| s.chars().find(|&c| !crate::lists_edit::xml::is_xml_char(c))) {
                return Err(format!("page {} : caractère interdit dans le {what} (U+{:04X})", p.image, c as u32));
            }
        }
        for (what, v) in
            [("ImageSize", &p.image_size), ("ImageWidth", &p.image_width), ("ImageHeight", &p.image_height)]
        {
            if let Some(v) = v {
                if v.trim().parse::<i32>().is_err() {
                    return Err(format!("page {} : {what} invalide « {v} »", p.image));
                }
            }
        }
        out.push(p);
    }
    Ok(out)
}

/// Deux listes décrivent les mêmes pages (après normalisation).
pub fn same(a: &[PageInfo], b: &[PageInfo]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            let (mut x, mut y) = (x.clone(), y.clone());
            normalize(&mut x);
            normalize(&mut y);
            x == y
        })
}

fn each_selected(list: &mut [PageInfo], images: &[i32], mut f: impl FnMut(&mut PageInfo)) {
    for p in list.iter_mut().filter(|p| images.contains(&p.image)) {
        f(p);
    }
}

/// Type de page (`PageViewItem.SetPageType` : remplace les drapeaux).
pub fn set_type(list: &mut [PageInfo], images: &[i32], t: &str) {
    each_selected(list, images, |p| p.kind = (t != "Story").then(|| t.to_string()));
}

/// « Marquer comme Supprimée » (Suppr) : Supprimée ↔ Histoire (`PagesView.MarkAsDeleted`).
pub fn toggle_deleted(list: &mut [PageInfo], images: &[i32]) {
    each_selected(list, images, |p| {
        p.kind = (kind(p) != "Deleted").then(|| "Deleted".to_string());
    });
}

/// Rotation (`ImageRotation`).
pub fn set_rotation(list: &mut [PageInfo], images: &[i32], r: &str) {
    each_selected(list, images, |p| p.rotation = (r != "None").then(|| r.to_string()));
}

/// Position en double page.
pub fn set_position(list: &mut [PageInfo], images: &[i32], pos: &str) {
    each_selected(list, images, |p| p.page_position = (pos != "Default").then(|| pos.to_string()));
}

/// Favori (signet) ; `None` ou vide le supprime.
pub fn set_bookmark(list: &mut [PageInfo], image: i32, text: Option<&str>) {
    let t = text.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    each_selected(list, &[image], |p| p.bookmark = t.clone());
}

/// Proposition de nom de favori (`PagesView.BookmarkProposal`) : le signet actuel, sinon
/// « Page n » (n = rang de lecture, à partir de 1).
pub fn bookmark_proposal(list: &[PageInfo], image: i32) -> String {
    match list.iter().position(|p| p.image == image) {
        Some(i) => list[i].bookmark.clone().filter(|b| !b.is_empty()).unwrap_or_else(|| format!("Page {}", i + 1)),
        None => String::new(),
    }
}

/// Déplace les pages `images` à la place `position` (rang dans la liste avant
/// déplacement, `list.len()` : à la fin), dans l'ordre où elles sont dans la liste
/// (`ComicInfo.MovePages`).
pub fn move_pages(list: &mut Vec<PageInfo>, position: usize, images: &[i32]) {
    let mut position = position.min(list.len());
    let moved: Vec<PageInfo> = list.iter().filter(|p| images.contains(&p.image)).cloned().collect();
    for m in &moved {
        if let Some(i) = list.iter().position(|p| p.image == m.image) {
            list.remove(i);
            if i < position {
                position -= 1;
            }
        }
    }
    for (k, m) in moved.into_iter().enumerate() {
        list.insert(position + k, m);
    }
}

/// « Mettre au début » (Ctrl+Alt+T).
pub fn move_to_start(list: &mut Vec<PageInfo>, images: &[i32]) {
    move_pages(list, 0, images);
}

/// « Mettre à la fin » (Ctrl+Alt+B). Comme ComicRack (`MoveSelectedPageEnd`) : si toutes
/// les pages déplacées sont des couvertures avant, elles deviennent « Autre » et la page à
/// la place qui suit leur nombre devient la couverture avant.
pub fn move_to_end(list: &mut Vec<PageInfo>, images: &[i32]) {
    let selected: Vec<&PageInfo> = list.iter().filter(|p| images.contains(&p.image)).collect();
    if !selected.is_empty() && selected.iter().all(|p| kind(p) == "FrontCover") {
        let n = selected.len();
        set_type(list, images, "Other");
        if n < list.len() {
            list[n].kind = Some("FrontCover".into());
        }
    }
    let end = list.len();
    move_pages(list, end, images);
}

/// « Remettre dans l'ordre d'origine » / « Réinitialiser » : ordre des images du fichier
/// (`ComicPageInfoCollection.ResetPageSequence`), types et autres attributs gardés.
pub fn reset_order(list: &mut [PageInfo]) {
    list.sort_by_key(|p| p.image);
}

/// Trie par nom d'image (`names[image]`) : ordinal ou numérique (« Trier par nom »,
/// « Trier par nom (numérique) » du bouton Réinitialiser de ComicRack).
pub fn sort_by_name(list: &mut [PageInfo], names: &[String], numeric: bool) {
    let name = |p: &PageInfo| names.get(p.image as usize).map(String::as_str).unwrap_or("");
    list.sort_by(|a, b| {
        if numeric { natord::compare_ignore_case(name(a), name(b)) } else { name(a).cmp(name(b)) }
            .then(a.image.cmp(&b.image))
    });
}

/// Types par défaut : la première page de la liste « Couverture avant », les autres
/// « Histoire » (rotation, position, favoris gardés).
pub fn reset_types(list: &mut [PageInfo]) {
    for (i, p) in list.iter_mut().enumerate() {
        p.kind = (i == 0).then(|| "FrontCover".to_string());
    }
}

/// Tout réinitialiser : ordre d'origine, types par défaut, sans rotation, position ni
/// favori (tailles d'image et clés gardées : elles décrivent le fichier).
pub fn reset_all(list: &mut [PageInfo]) {
    reset_order(list);
    reset_types(list);
    for p in list.iter_mut() {
        p.rotation = None;
        p.page_position = None;
        p.bookmark = None;
    }
}

/// Pages décrites par le ComicInfo.xml d'un fichier (livre hors bibliothèque).
pub fn from_comic_info(ci: &crate::ComicInfo) -> Vec<PageInfo> {
    ci.pages
        .as_ref()
        .map(|p| {
            p.page
                .iter()
                .map(|p| PageInfo {
                    image: p.image as i32,
                    kind: (p.page_type != crate::comic_info::PageType::Story).then(|| format!("{:?}", p.page_type)),
                    bookmark: p.bookmark.clone(),
                    rotation: p.rotation.clone(),
                    page_position: p.page_position.clone(),
                    ..Default::default()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Page affichée par le lecteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingPage {
    /// Image du fichier (rang dans l'ordre naturel des noms).
    pub image: usize,
    /// Numéro de la page dans le livre (rang dans la liste complète, pages supprimées
    /// comprises) : c'est celui que ComicRack enregistre (`CurrentPage`, `LastPageRead`).
    pub page: usize,
    /// Rotation à appliquer, en degrés.
    pub rotation: u16,
}

/// Pages lues, dans l'ordre de lecture, pages « Supprimée » masquées. Si toutes les
/// pages sont supprimées, elles sont toutes montrées (un livre vide n'aurait pas de sens).
pub fn reading_order(pages: &[PageInfo], count: usize) -> Vec<ReadingPage> {
    let full = full_list(pages, count);
    let visible: Vec<ReadingPage> = full
        .iter()
        .enumerate()
        .filter(|(_, p)| !is_type(p, "Deleted"))
        .map(|(page, p)| ReadingPage { image: p.image as usize, page, rotation: rotation_degrees(p) })
        .collect();
    if visible.is_empty() && count > 0 {
        return full
            .iter()
            .enumerate()
            .map(|(page, p)| ReadingPage { image: p.image as usize, page, rotation: rotation_degrees(p) })
            .collect();
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(image: i32, kind: Option<&str>) -> PageInfo {
        PageInfo { image, kind: kind.map(str::to_string), ..Default::default() }
    }

    fn images(l: &[PageInfo]) -> Vec<i32> {
        l.iter().map(|p| p.image).collect()
    }

    #[test]
    fn full_list_completes_with_defaults() {
        let l = full_list(&[], 3);
        assert_eq!(images(&l), [0, 1, 2]);
        assert_eq!(kind(&l[0]), "FrontCover", "new ComicPageInfo(0) : couverture avant");
        assert_eq!(kind(&l[1]), "Story");
        // Ordre enregistré, images hors fichier et doublons écartés, manquantes ajoutées.
        let l = full_list(&[pg(2, None), pg(9, None), pg(2, Some("Other")), pg(0, Some("FrontCover"))], 4);
        assert_eq!(images(&l), [2, 0, 1, 3]);
        assert_eq!(kind(&l[2]), "Story");
    }

    #[test]
    fn moving_pages_like_comicrack() {
        let mut l = full_list(&[], 6);
        move_pages(&mut l, 4, &[1, 2]);
        assert_eq!(images(&l), [0, 3, 1, 2, 4, 5], "insérées avant la place 4 d'origine");
        move_pages(&mut l, 0, &[4, 3]);
        assert_eq!(images(&l), [3, 4, 0, 1, 2, 5], "ordre de la liste gardé");
        move_to_end(&mut l, &[3]);
        assert_eq!(images(&l), [4, 0, 1, 2, 5, 3]);
        move_to_start(&mut l, &[5]);
        assert_eq!(images(&l), [5, 4, 0, 1, 2, 3]);
        reset_order(&mut l);
        assert_eq!(images(&l), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn moving_the_cover_to_the_end_promotes_the_next_page() {
        let mut l = full_list(&[], 3);
        move_to_end(&mut l, &[0]);
        assert_eq!(images(&l), [1, 2, 0]);
        assert_eq!((kind(&l[0]), kind(&l[2])), ("FrontCover", "Other"));
    }

    #[test]
    fn types_rotation_position_bookmarks() {
        let mut l = full_list(&[], 4);
        set_type(&mut l, &[1, 2], "Advertisement");
        assert_eq!(l.iter().map(kind).collect::<Vec<_>>(), ["FrontCover", "Advertisement", "Advertisement", "Story"]);
        toggle_deleted(&mut l, &[2, 3]);
        assert_eq!((kind(&l[2]), kind(&l[3])), ("Deleted", "Deleted"));
        toggle_deleted(&mut l, &[3]);
        assert_eq!(kind(&l[3]), "Story", "Suppr sur une page supprimée : Histoire");
        set_type(&mut l, &[1], "Story");
        assert_eq!(l[1].kind, None, "« Story » n'est pas écrit");
        set_rotation(&mut l, &[1], "Rotate90");
        set_position(&mut l, &[1], "Far");
        assert_eq!((rotation_degrees(&l[1]), position(&l[1])), (90, "Far"));
        set_rotation(&mut l, &[1], "None");
        assert_eq!(l[1].rotation, None);
        assert_eq!(bookmark_proposal(&l, 1), "Page 2");
        set_bookmark(&mut l, 1, Some("Début"));
        assert_eq!(bookmark_proposal(&l, 1), "Début");
        set_bookmark(&mut l, 1, Some(""));
        assert_eq!(l[1].bookmark, None, "signet vide = supprimé");
        assert_eq!(kind_label("FrontCover, Story"), "Couverture avant, Histoire");
    }

    #[test]
    fn resets_and_sorting() {
        let mut l = full_list(&[pg(2, Some("Other")), pg(1, None), pg(0, Some("FrontCover"))], 3);
        l[0].rotation = Some("Rotate180".into());
        l[0].bookmark = Some("x".into());
        let names: Vec<String> = ["p10.jpg", "p2.jpg", "p1.jpg"].iter().map(|s| s.to_string()).collect();
        sort_by_name(&mut l, &names, false);
        assert_eq!(images(&l), [2, 0, 1], "ordinal : p1 < p10 < p2");
        sort_by_name(&mut l, &names, true);
        assert_eq!(images(&l), [2, 1, 0], "numérique : p1 < p2 < p10");
        reset_types(&mut l);
        assert_eq!(l.iter().map(kind).collect::<Vec<_>>(), ["FrontCover", "Story", "Story"]);
        assert_eq!(l[0].rotation.as_deref(), Some("Rotate180"), "types seulement");
        reset_all(&mut l);
        assert_eq!(images(&l), [0, 1, 2]);
        assert!(l.iter().all(|p| p.rotation.is_none() && p.bookmark.is_none()));
        assert_eq!(kind(&l[0]), "FrontCover");
    }

    #[test]
    fn validation_and_normalization() {
        let mut p = pg(0, Some("Story, FrontCover"));
        p.rotation = Some("None".into());
        p.page_position = Some("Default".into());
        p.bookmark = Some(String::new());
        let v = validate(&[p, pg(1, Some("Story"))]).unwrap();
        assert_eq!(v[0].kind.as_deref(), Some("FrontCover, Story"), "ordre des drapeaux de .NET");
        assert_eq!((v[0].rotation.clone(), v[0].page_position.clone(), v[0].bookmark.clone()), (None, None, None));
        assert_eq!(v[1].kind, None);
        assert!(validate(&[pg(0, None), pg(0, None)]).is_err(), "doublon");
        assert!(validate(&[pg(0, Some("Cover"))]).is_err(), "type inconnu");
        assert!(validate(&[pg(-1, None)]).is_err());
        let mut r = pg(0, None);
        r.rotation = Some("Rotate45".into());
        assert!(validate(&[r]).is_err());
        assert!(same(&[pg(0, Some("Story"))], &[pg(0, None)]));
    }

    #[test]
    fn reading_order_hides_deleted_pages_and_rotates() {
        let mut l = full_list(&[], 4);
        move_to_start(&mut l, &[2]);
        set_type(&mut l, &[0], "Deleted");
        set_rotation(&mut l, &[3], "Rotate270");
        let r = reading_order(&l, 4);
        assert_eq!(
            r,
            [
                ReadingPage { image: 2, page: 0, rotation: 0 },
                ReadingPage { image: 1, page: 2, rotation: 0 },
                ReadingPage { image: 3, page: 3, rotation: 270 },
            ]
        );
        // Sans infos : ordre du fichier.
        assert_eq!(reading_order(&[], 2).iter().map(|p| p.image).collect::<Vec<_>>(), [0, 1]);
        // Tout supprimé : tout montré.
        let mut all = full_list(&[], 2);
        set_type(&mut all, &[0, 1], "Deleted");
        assert_eq!(reading_order(&all, 2).len(), 2);
    }
}
