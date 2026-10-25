//! Sélection et glisser-déposer des livres à la souris (v0.9.1) : calculs indépendants de
//! l'interface, testés à part.
//!
//! - vue des couvertures : rectangle de sélection, position d'insertion entre deux vignettes ;
//! - vue tableau : ligne sous le pointeur, position d'insertion entre deux lignes ;
//! - réordonnancement d'une liste de lecture (`move_books`) ;
//! - règles d'acceptation d'un dépôt de livres (liste du panneau, vue elle-même) ;
//! - défilement automatique près des bords.
//!
//! Aucun glisser ne touche aux fichiers : seules les listes de lecture sont modifiées.

use std::collections::HashSet;

use comicrust_core::comic_lists::{ComicList, ListKind};
use eframe::egui::{pos2, Pos2, Rect, Vec2};

/// Message affiché quand la vue n'est pas triée dans l'ordre de la liste.
pub const SORT_TO_REORDER: &str = "Trier dans l'ordre de la liste pour réordonner";

/// Rangée de couvertures : ordonnée de son haut dans le contenu, rang dans la vue de son
/// premier livre et nombre de livres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverRow {
    pub top: f32,
    pub first: usize,
    pub len: usize,
}

/// Géométrie de la grille des couvertures (coordonnées du contenu : x = 0 au bord gauche de
/// la première vignette).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridGeom {
    pub cell: Vec2,
    pub gap: f32,
}

impl GridGeom {
    /// Rectangle de la `k`-ième vignette d'une rangée.
    pub fn cell_rect(&self, row: &CoverRow, k: usize) -> Rect {
        Rect::from_min_size(pos2(k as f32 * (self.cell.x + self.gap), row.top), self.cell)
    }
}

/// Livres (rangs dans la vue) dont la vignette touche le rectangle de sélection.
pub fn marquee_hits(rows: &[CoverRow], geom: &GridGeom, marquee: Rect) -> Vec<usize> {
    let mut out = Vec::new();
    for row in rows {
        if row.top > marquee.max.y || row.top + geom.cell.y < marquee.min.y {
            continue;
        }
        for k in 0..row.len {
            if geom.cell_rect(row, k).intersects(marquee) {
                out.push(row.first + k);
            }
        }
    }
    out
}

/// Dépôt au point `p` de la grille : rang d'insertion dans la vue (0 ..= nombre de livres)
/// et trait vertical à dessiner (haut, bas), en coordonnées du contenu.
pub fn grid_insertion(rows: &[CoverRow], geom: &GridGeom, p: Pos2) -> Option<(usize, [Pos2; 2])> {
    // Rangée la plus proche verticalement du pointeur.
    let dist = |r: &CoverRow| {
        if p.y < r.top {
            r.top - p.y
        } else if p.y > r.top + geom.cell.y {
            p.y - (r.top + geom.cell.y)
        } else {
            0.0
        }
    };
    let row = rows.iter().filter(|r| r.len > 0).min_by(|a, b| dist(a).total_cmp(&dist(b)))?;
    // Nombre de vignettes dont le centre est à gauche du pointeur.
    let i = (0..row.len).filter(|&k| geom.cell_rect(row, k).center().x < p.x).count();
    let x = if i < row.len {
        geom.cell_rect(row, i).left() - geom.gap / 2.0
    } else {
        geom.cell_rect(row, row.len - 1).right() + geom.gap / 2.0
    };
    Some((row.first + i, [pos2(x, row.top), pos2(x, row.top + geom.cell.y)]))
}

/// Tableau : ligne sous l'ordonnée `y` (`top0` : haut de la ligne 0, `pitch` : hauteur d'une
/// ligne espacement compris), bornée aux `n` lignes.
pub fn table_row_at(y: f32, top0: f32, pitch: f32, n: usize) -> Option<usize> {
    if n == 0 || pitch <= 0.0 {
        return None;
    }
    let r = ((y - top0) / pitch).floor();
    Some((r.max(0.0) as usize).min(n - 1))
}

/// Tableau : rang d'insertion (0 ..= `n`) pour un dépôt à l'ordonnée `y` (frontière entre
/// lignes la plus proche).
pub fn table_insertion(y: f32, top0: f32, pitch: f32, n: usize) -> usize {
    if pitch <= 0.0 {
        return 0;
    }
    let r = ((y - top0) / pitch).round();
    (r.max(0.0) as usize).min(n)
}

/// Plage de lignes balayée (bornes incluses, dans l'ordre).
pub fn sweep_range(anchor: usize, current: usize) -> std::ops::RangeInclusive<usize> {
    anchor.min(current)..=anchor.max(current)
}

/// Repère du réordonnancement, indépendant des rangs (la liste a pu changer entre-temps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// Insérer juste avant ce livre.
    Before(String),
    /// Insérer juste après ce livre (dépôt en fin de vue).
    After(String),
}

fn lower_set(ids: &[String]) -> HashSet<String> {
    ids.iter().map(|b| b.trim().to_ascii_lowercase()).collect()
}

/// Dépôt au rang `ins` de la vue (`view` : identifiants des livres affichés dans l'ordre) :
/// premier livre non déplacé à partir de `ins`, sinon dernier livre non déplacé avant lui.
pub fn reorder_anchor(view: &[String], moving: &[String], ins: usize) -> Option<Anchor> {
    let moving = lower_set(moving);
    let free = |b: &&String| !moving.contains(&b.to_ascii_lowercase());
    let ins = ins.min(view.len());
    if let Some(b) = view[ins..].iter().find(free) {
        return Some(Anchor::Before(b.clone()));
    }
    view[..ins].iter().rev().find(free).map(|b| Anchor::After(b.clone()))
}

/// Position `to` à passer à `move_books` (rang dans la liste sans les livres déplacés), ou
/// `None` si le dépôt ne change rien (ou si le repère n'est plus dans la liste).
pub fn reorder_target(list: &[String], moving: &[String], anchor: &Anchor) -> Option<usize> {
    let set = lower_set(moving);
    let is_moving = |b: &String| set.contains(&b.to_ascii_lowercase());
    let (moved, rest): (Vec<&String>, Vec<&String>) = list.iter().partition(|b| is_moving(b));
    if moved.is_empty() {
        return None;
    }
    let find = |id: &str| rest.iter().position(|b| b.eq_ignore_ascii_case(id));
    let to = match anchor {
        Anchor::Before(id) => find(id)?,
        Anchor::After(id) => find(id)? + 1,
    };
    let mut new = rest.clone();
    new.splice(to..to, moved);
    let unchanged = new.iter().zip(list).all(|(a, b)| a.eq_ignore_ascii_case(b));
    (!unchanged).then_some(to)
}

/// Livres déposés sur une liste du panneau : acceptés ? Sinon la raison (curseur
/// « interdit »). `reason` : listes non modifiables (lecture seule, enregistrement en
/// cours) ; `shown` : liste affichée dans la vue (d'où viennent les livres).
pub fn books_drop_on_list(list: &ComicList, reason: Option<&str>, shown: Option<&str>) -> Result<(), String> {
    match list.kind {
        ListKind::Library => return Err("La bibliothèque contient déjà tous les livres".into()),
        ListKind::Folder { .. } => return Err("Dossier : dépose les livres sur une liste de lecture".into()),
        ListKind::Smart { .. } => {
            return Err("Liste intelligente : son contenu dépend de ses critères".into());
        }
        ListKind::Ids(_) => {}
    }
    if shown.is_some_and(|s| s.eq_ignore_ascii_case(&list.id)) {
        return Err("Les livres sont déjà dans cette liste".into());
    }
    match reason {
        Some(r) => Err(r.to_string()),
        None => Ok(()),
    }
}

/// Livres déposés dans la vue elle-même : réordonnancement possible ? Seulement dans une
/// liste de lecture affichée dans l'ordre de la liste (croissant), listes modifiables.
pub fn reorder_allowed(reading_list: bool, sorted_by_position: bool, reason: Option<&str>) -> Result<(), String> {
    if !reading_list {
        return Err("Dépose les livres sur une liste de lecture du panneau des listes".into());
    }
    if !sorted_by_position {
        return Err(SORT_TO_REORDER.into());
    }
    match reason {
        Some(r) => Err(r.to_string()),
        None => Ok(()),
    }
}

/// Défilement automatique près des bords d'une zone (`top`, `bottom`) : vitesse en points par
/// seconde, positive vers le haut (convention de `Ui::scroll_with_delta`), nulle hors des
/// marges. Le pointeur peut être au-delà de la zone (rectangle de sélection).
pub fn edge_scroll_speed(top: f32, bottom: f32, y: f32) -> f32 {
    const MARGIN: f32 = 36.0;
    const MAX: f32 = 1500.0;
    let margin = MARGIN.min((bottom - top) / 4.0).max(1.0);
    let d = if y < top + margin {
        top + margin - y
    } else if y > bottom - margin {
        -(y - (bottom - margin))
    } else {
        0.0
    };
    (d * 14.0).clamp(-MAX, MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use comicrust_core::comic_lists::Combine;
    use eframe::egui::vec2;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn geom() -> GridGeom {
        GridGeom { cell: vec2(100.0, 150.0), gap: 10.0 }
    }

    /// Deux rangées de 3, puis une de 2 (après un titre de groupe de 34 points).
    fn rows() -> Vec<CoverRow> {
        vec![
            CoverRow { top: 0.0, first: 0, len: 3 },
            CoverRow { top: 160.0, first: 3, len: 3 },
            CoverRow { top: 354.0, first: 6, len: 2 },
        ]
    }

    #[test]
    fn marquee_selects_touched_covers() {
        let g = geom();
        // Rectangle dans la marge entre les vignettes : rien.
        assert!(marquee_hits(&rows(), &g, Rect::from_two_pos(pos2(101.0, 10.0), pos2(109.0, 140.0))).is_empty());
        // Coin de la 2e vignette de la rangée 1 jusqu'à la 1re vignette de la rangée 2 (tracé
        // de bas en haut, de droite à gauche : même résultat).
        let r = Rect::from_two_pos(pos2(150.0, 200.0), pos2(50.0, 100.0));
        assert_eq!(marquee_hits(&rows(), &g, r), [0, 1, 3, 4]);
        // Toute la hauteur, colonne de droite seulement : la dernière rangée n'a que 2 livres.
        let r = Rect::from_two_pos(pos2(230.0, 0.0), pos2(300.0, 600.0));
        assert_eq!(marquee_hits(&rows(), &g, r), [2, 5]);
    }

    #[test]
    fn grid_insertion_between_covers() {
        let g = geom();
        // Moitié gauche de la 1re vignette : avant elle.
        let (i, line) = grid_insertion(&rows(), &g, pos2(20.0, 50.0)).unwrap();
        assert_eq!(i, 0);
        assert_eq!(line[0], pos2(-5.0, 0.0));
        // Moitié droite de la 2e vignette de la rangée 2 : entre la 2e et la 3e.
        let (i, line) = grid_insertion(&rows(), &g, pos2(170.0, 200.0)).unwrap();
        assert_eq!(i, 5);
        assert_eq!(line, [pos2(215.0, 160.0), pos2(215.0, 310.0)]);
        // À droite de la dernière vignette de la rangée 3 : en fin de vue.
        let (i, line) = grid_insertion(&rows(), &g, pos2(500.0, 400.0)).unwrap();
        assert_eq!(i, 8);
        assert_eq!(line[0].x, 215.0);
        // Sur le titre de groupe (entre deux rangées) : rangée la plus proche.
        assert_eq!(grid_insertion(&rows(), &g, pos2(20.0, 320.0)).unwrap().0, 3);
        assert_eq!(grid_insertion(&rows(), &g, pos2(20.0, 345.0)).unwrap().0, 6);
        assert!(grid_insertion(&[], &g, pos2(0.0, 0.0)).is_none());
    }

    #[test]
    fn table_rows_and_insertion() {
        // Lignes de 20 points, la ligne 0 commence à 100.
        assert_eq!(table_row_at(105.0, 100.0, 20.0, 10), Some(0));
        assert_eq!(table_row_at(141.0, 100.0, 20.0, 10), Some(2));
        assert_eq!(table_row_at(50.0, 100.0, 20.0, 10), Some(0), "au-dessus : première ligne");
        assert_eq!(table_row_at(900.0, 100.0, 20.0, 10), Some(9), "en dessous : dernière ligne");
        assert_eq!(table_row_at(105.0, 100.0, 20.0, 0), None);
        assert_eq!(table_insertion(108.0, 100.0, 20.0, 10), 0);
        assert_eq!(table_insertion(112.0, 100.0, 20.0, 10), 1);
        assert_eq!(table_insertion(900.0, 100.0, 20.0, 10), 10);
        assert_eq!(sweep_range(7, 3), 3..=7);
        assert_eq!(sweep_range(2, 5), 2..=5);
    }

    #[test]
    fn reorder_positions() {
        let list = s(&["a", "b", "c", "d", "e"]);
        let view = list.clone();
        // « d » déposé entre « a » et « b » (rang 1).
        let anchor = reorder_anchor(&view, &s(&["d"]), 1).unwrap();
        assert_eq!(anchor, Anchor::Before("b".into()));
        assert_eq!(reorder_target(&list, &s(&["d"]), &anchor), Some(1));
        // « a » et « c » déposés en fin de vue : après « e ».
        let anchor = reorder_anchor(&view, &s(&["a", "c"]), 5).unwrap();
        assert_eq!(anchor, Anchor::After("e".into()));
        assert_eq!(reorder_target(&list, &s(&["a", "c"]), &anchor), Some(3));
        // Déposé juste à côté de lui-même : rien ne change.
        for ins in [1, 2] {
            let anchor = reorder_anchor(&view, &s(&["b"]), ins).unwrap();
            assert_eq!(reorder_target(&list, &s(&["b"]), &anchor), None, "rang {ins}");
        }
        // Dépôt au milieu de la sélection (rang entre « b » et « c », tous deux déplacés) :
        // devant le premier livre non déplacé qui suit.
        let anchor = reorder_anchor(&view, &s(&["b", "c"]), 2).unwrap();
        assert_eq!(anchor, Anchor::Before("d".into()));
        // Sélection non contiguë regroupée : « b », « d » avant « c » → a b d c e.
        assert_eq!(reorder_target(&list, &s(&["b", "d"]), &Anchor::Before("c".into())), Some(1));
        // Vue filtrée (recherche) : seuls « a », « c », « e » visibles ; « e » déposé entre
        // « a » et « c ».
        let filtered = s(&["a", "c", "e"]);
        let anchor = reorder_anchor(&filtered, &s(&["e"]), 1).unwrap();
        assert_eq!(reorder_target(&list, &s(&["e"]), &anchor), Some(2), "a b e c d");
        // Casse des identifiants différente entre la vue et la liste.
        assert_eq!(reorder_target(&list, &s(&["D"]), &Anchor::Before("A".into())), Some(0));
        // Tous les livres déplacés : aucun repère.
        assert_eq!(reorder_anchor(&s(&["a"]), &s(&["a"]), 0), None);
        // Repère disparu de la liste (modifiée entre-temps).
        assert_eq!(reorder_target(&list, &s(&["a"]), &Anchor::Before("z".into())), None);
    }

    fn l(id: &str, kind: ListKind) -> ComicList {
        ComicList { id: id.into(), name: id.into(), description: String::new(), kind, comicrack_count: 0 }
    }

    #[test]
    fn drop_acceptance() {
        let reading = l("r", ListKind::Ids(vec![]));
        assert_eq!(books_drop_on_list(&reading, None, None), Ok(()));
        assert_eq!(books_drop_on_list(&reading, None, Some("x")), Ok(()));
        assert!(books_drop_on_list(&reading, None, Some("R")).unwrap_err().contains("déjà dans cette liste"));
        assert_eq!(books_drop_on_list(&reading, Some("Lecture seule"), None), Err("Lecture seule".into()));
        let smart = l("s", ListKind::Smart { query: None, cached: None });
        assert!(books_drop_on_list(&smart, None, None).unwrap_err().contains("intelligente"));
        let folder = l("f", ListKind::Folder { combine: Combine::Or, children: vec![] });
        assert!(books_drop_on_list(&folder, None, None).unwrap_err().contains("Dossier"));
        assert!(books_drop_on_list(&l("b", ListKind::Library), None, None).is_err());

        assert_eq!(reorder_allowed(true, true, None), Ok(()));
        assert_eq!(reorder_allowed(true, false, None), Err(SORT_TO_REORDER.into()));
        assert!(reorder_allowed(false, true, None).unwrap_err().contains("panneau des listes"));
        assert_eq!(reorder_allowed(true, true, Some("ComicRack lancé")), Err("ComicRack lancé".into()));
    }

    #[test]
    fn edge_scrolling() {
        assert_eq!(edge_scroll_speed(100.0, 700.0, 400.0), 0.0);
        assert!(edge_scroll_speed(100.0, 700.0, 110.0) > 0.0, "près du haut : vers le haut");
        assert!(edge_scroll_speed(100.0, 700.0, 690.0) < 0.0, "près du bas : vers le bas");
        assert!(edge_scroll_speed(100.0, 700.0, 50.0) > edge_scroll_speed(100.0, 700.0, 110.0));
        assert_eq!(edge_scroll_speed(100.0, 700.0, -5000.0), 1500.0, "bornée");
    }
}
