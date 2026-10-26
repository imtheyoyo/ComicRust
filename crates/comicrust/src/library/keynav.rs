//! Navigation au clavier dans la bibliothèque (v0.10.1), sans interface : déplacement
//! dans la grille des couvertures (rangées incomplètes, groupes), dans le tableau, et
//! parcours de l'arbre des listes. Le dessin (`ui/library_view.rs`, `ui/library_table.rs`,
//! `ui/library_panels.rs`) appelle ces fonctions et applique le résultat.
//!
//! Touches (comme l'explorateur de Windows et ComicRack) :
//! - couvertures : ←/→ élément précédent/suivant, ↑/↓ rangée précédente/suivante (même
//!   colonne, ou dernier élément d'une rangée plus courte), Début/Fin, Page préc./suiv. ;
//! - tableau : ↑/↓, Début/Fin, Page préc./suiv. ;
//! - arbre : ↑/↓ ligne visible précédente/suivante, → déplie un dossier puis va à son
//!   premier enfant, ← replie un dossier ouvert ou remonte au parent.

use std::collections::HashMap;

use eframe::egui;

/// Touche de déplacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

impl NavKey {
    /// Touches de déplacement, avec leur touche egui.
    pub const ALL: [(egui::Key, NavKey); 8] = [
        (egui::Key::ArrowLeft, NavKey::Left),
        (egui::Key::ArrowRight, NavKey::Right),
        (egui::Key::ArrowUp, NavKey::Up),
        (egui::Key::ArrowDown, NavKey::Down),
        (egui::Key::Home, NavKey::Home),
        (egui::Key::End, NavKey::End),
        (egui::Key::PageUp, NavKey::PageUp),
        (egui::Key::PageDown, NavKey::PageDown),
    ];
}

/// Panneau qui reçoit les touches de déplacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyFocus {
    /// Vue des livres (couvertures ou tableau).
    #[default]
    Books,
    /// Arbre des listes.
    Lists,
}

/// Effet des modificateurs sur la sélection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    /// Sans modificateur : le livre atteint devient la sélection.
    Replace,
    /// Maj : la sélection va du livre de départ (ancre) au livre atteint.
    Extend,
    /// Ctrl : seul le curseur bouge ; Espace (dé)sélectionne le livre sous le curseur.
    MoveOnly,
}

impl SelectMode {
    pub fn of(m: egui::Modifiers) -> SelectMode {
        if m.shift {
            SelectMode::Extend
        } else if m.command {
            SelectMode::MoveOnly
        } else {
            SelectMode::Replace
        }
    }
}

/// Rangée de la grille : rang (dans la vue) de son premier élément et nombre d'éléments.
/// Les rangées se suivent dans l'ordre de la vue ; un titre de groupe n'est pas une rangée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub first: usize,
    pub len: usize,
}

/// Rangées d'une grille de `columns` colonnes, groupes compris : `groups` donne le nombre
/// d'éléments de chaque groupe (chaque groupe commence une rangée). (La vue construit
/// ses rangées elle-même ; sert aux tests.)
#[cfg(test)]
pub fn spans(groups: &[usize], columns: usize) -> Vec<Span> {
    let columns = columns.max(1);
    let mut out = Vec::new();
    let mut first = 0;
    for &g in groups {
        let mut left = g;
        while left > 0 {
            let len = left.min(columns);
            out.push(Span { first, len });
            first += len;
            left -= len;
        }
    }
    out
}

/// Rang atteint depuis `current` (rang dans la vue ; `None` ou hors vue : aucun) avec la
/// touche `key` dans la grille `rows`. `page` : nombre de rangées par page. `None` si la
/// grille est vide.
pub fn grid_move(rows: &[Span], current: Option<usize>, key: NavKey, page: usize) -> Option<usize> {
    let rows: Vec<Span> = rows.iter().copied().filter(|r| r.len > 0).collect();
    let last = rows.last()?;
    let n = last.first + last.len;
    let Some(cur) = current.filter(|&c| c < n) else {
        // Rien de sélectionné : on part du début (de la fin pour Fin).
        return Some(if key == NavKey::End { n - 1 } else { 0 });
    };
    let r = rows.iter().position(|s| cur >= s.first && cur < s.first + s.len).unwrap_or(0);
    let col = cur - rows[r].first;
    let at = |row: usize| rows[row].first + col.min(rows[row].len - 1);
    let page = page.max(1);
    Some(match key {
        NavKey::Left => cur.saturating_sub(1),
        NavKey::Right => (cur + 1).min(n - 1),
        NavKey::Up if r == 0 => cur,
        NavKey::Up => at(r - 1),
        NavKey::Down if r + 1 == rows.len() => cur,
        NavKey::Down => at(r + 1),
        NavKey::PageUp if r == 0 => 0,
        NavKey::PageUp => at(r.saturating_sub(page)),
        NavKey::PageDown if r + 1 == rows.len() => n - 1,
        NavKey::PageDown => at((r + page).min(rows.len() - 1)),
        NavKey::Home => 0,
        NavKey::End => n - 1,
    })
}

/// Déplacement dans une liste de `n` lignes (tableau) : ←/→ sans effet.
pub fn list_move(n: usize, current: Option<usize>, key: NavKey, page: usize) -> Option<usize> {
    if matches!(key, NavKey::Left | NavKey::Right) {
        return current.filter(|&c| c < n).or_else(|| (n > 0).then_some(0));
    }
    let rows: Vec<Span> = (0..n).map(|i| Span { first: i, len: 1 }).collect();
    grid_move(&rows, current, key, page)
}

/// Plage de rangs sélectionnée par Maj+flèche : de l'ancre au curseur, bornes comprises.
pub fn extend_range(anchor: usize, cursor: usize) -> std::ops::RangeInclusive<usize> {
    anchor.min(cursor)..=anchor.max(cursor)
}

// ---------------------------------------------------------------------------------------
// Arbre des listes
// ---------------------------------------------------------------------------------------

/// Ligne **visible** de l'arbre des listes, dans l'ordre d'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// Liste (`None` : « Toute la bibliothèque »).
    pub id: Option<String>,
    /// Profondeur (0 : premier niveau).
    pub depth: usize,
    /// Dossier (dépliable).
    pub folder: bool,
    /// Dossier déplié.
    pub open: bool,
}

/// Effet d'une touche dans l'arbre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeAction {
    /// Afficher la ligne de ce rang.
    Select(usize),
    /// Déplier / replier le dossier de ce rang.
    Open(usize),
    Close(usize),
    None,
}

/// Effet de `key` sur l'arbre `rows`, depuis la ligne `current`.
pub fn tree_move(rows: &[TreeRow], current: Option<usize>, key: NavKey, page: usize) -> TreeAction {
    if rows.is_empty() {
        return TreeAction::None;
    }
    let Some(cur) = current.filter(|&c| c < rows.len()) else {
        return TreeAction::Select(if key == NavKey::End { rows.len() - 1 } else { 0 });
    };
    let row = &rows[cur];
    match key {
        NavKey::Right if row.folder && !row.open => TreeAction::Open(cur),
        NavKey::Right if row.folder && rows.get(cur + 1).is_some_and(|n| n.depth > row.depth) => {
            TreeAction::Select(cur + 1)
        }
        NavKey::Right => TreeAction::None,
        NavKey::Left if row.folder && row.open => TreeAction::Close(cur),
        NavKey::Left => match rows[..cur].iter().rposition(|r| r.depth < row.depth) {
            Some(p) => TreeAction::Select(p),
            None => TreeAction::None,
        },
        other => match list_move(rows.len(), Some(cur), other, page) {
            Some(i) if i != cur => TreeAction::Select(i),
            _ => TreeAction::None,
        },
    }
}

/// Lignes de l'arbre relevées pendant son dessin, et dossiers à déplier / replier.
#[derive(Debug, Default)]
pub struct TreeRecorder {
    pub rows: Vec<TreeRow>,
    /// Profondeur courante pendant le dessin.
    pub depth: usize,
    /// Dessin à l'intérieur d'un dossier replié (animation de fermeture) : lignes non
    /// relevées.
    pub hidden: usize,
    /// Demandes du clavier : identifiant de dossier → ouvert.
    pub fold: HashMap<String, bool>,
    /// L'arbre a le focus du clavier (cadre autour de la liste affichée).
    pub focused: bool,
    /// Faire défiler jusqu'à la liste affichée (après un déplacement au clavier).
    pub scroll: bool,
}

impl TreeRecorder {
    /// Relève une ligne dessinée.
    pub fn push(&mut self, id: Option<String>, folder: bool, open: bool) {
        if self.hidden == 0 {
            self.rows.push(TreeRow { id, depth: self.depth, folder, open });
        }
    }
}

/// État de la navigation au clavier de la bibliothèque.
#[derive(Debug, Default)]
pub struct NavState {
    /// Panneau qui reçoit les flèches.
    pub focus: KeyFocus,
    /// Faire défiler la vue jusqu'au curseur (après un déplacement au clavier).
    pub scroll_to_cursor: bool,
    /// Lignes visibles de l'arbre au dernier dessin.
    pub tree_rows: Vec<TreeRow>,
    /// Dossiers à déplier / replier au prochain dessin.
    pub fold: HashMap<String, bool>,
    /// Faire défiler l'arbre jusqu'à la liste affichée.
    pub scroll_tree: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use NavKey::*;

    /// Deux groupes : 7 livres puis 3, sur 3 colonnes :
    /// ```text
    /// 0 1 2
    /// 3 4 5
    /// 6
    /// --- groupe 2
    /// 7 8 9
    /// ```
    fn grid() -> Vec<Span> {
        spans(&[7, 3], 3)
    }

    #[test]
    fn spans_cut_groups_in_rows() {
        let s = grid();
        let pairs: Vec<(usize, usize)> = s.iter().map(|s| (s.first, s.len)).collect();
        assert_eq!(pairs, [(0, 3), (3, 3), (6, 1), (7, 3)]);
        assert!(spans(&[], 4).is_empty());
        assert_eq!(spans(&[2], 0), [Span { first: 0, len: 1 }, Span { first: 1, len: 1 }], "au moins une colonne");
    }

    #[test]
    fn grid_left_right_follow_the_view_order() {
        let g = grid();
        assert_eq!(grid_move(&g, Some(0), Left, 1), Some(0), "début : on reste");
        assert_eq!(grid_move(&g, Some(3), Left, 1), Some(2), "fin de la rangée précédente");
        assert_eq!(grid_move(&g, Some(6), Right, 1), Some(7), "passe au groupe suivant");
        assert_eq!(grid_move(&g, Some(9), Right, 1), Some(9), "fin : on reste");
    }

    #[test]
    fn grid_up_down_keep_the_column() {
        let g = grid();
        assert_eq!(grid_move(&g, Some(1), Down, 1), Some(4));
        assert_eq!(grid_move(&g, Some(5), Down, 1), Some(6), "rangée incomplète : son dernier élément");
        assert_eq!(grid_move(&g, Some(6), Down, 1), Some(7), "rangée du groupe suivant");
        assert_eq!(grid_move(&g, Some(8), Up, 1), Some(6), "rangée incomplète au-dessus");
        assert_eq!(grid_move(&g, Some(4), Up, 1), Some(1));
        assert_eq!(grid_move(&g, Some(2), Up, 1), Some(2), "première rangée : on reste");
        assert_eq!(grid_move(&g, Some(9), Down, 1), Some(9), "dernière rangée : on reste");
    }

    #[test]
    fn grid_pages_home_end_and_nothing_selected() {
        let g = grid();
        assert_eq!(grid_move(&g, Some(1), PageDown, 2), Some(6), "2 rangées plus bas, colonne bornée");
        assert_eq!(grid_move(&g, Some(1), PageDown, 10), Some(8), "dernière rangée");
        assert_eq!(grid_move(&g, Some(8), PageDown, 2), Some(9), "déjà en bas : dernier élément");
        assert_eq!(grid_move(&g, Some(8), PageUp, 2), Some(4));
        assert_eq!(grid_move(&g, Some(2), PageUp, 2), Some(0), "déjà en haut : premier élément");
        assert_eq!(grid_move(&g, Some(5), Home, 2), Some(0));
        assert_eq!(grid_move(&g, Some(5), End, 2), Some(9));
        assert_eq!(grid_move(&g, None, Down, 2), Some(0), "rien de sélectionné : premier");
        assert_eq!(grid_move(&g, None, End, 2), Some(9));
        assert_eq!(grid_move(&g, Some(42), Right, 2), Some(0), "rang hors vue : comme rien");
        assert_eq!(grid_move(&[], Some(0), Down, 1), None, "vue vide");
    }

    #[test]
    fn table_moves() {
        assert_eq!(list_move(5, Some(2), Down, 3), Some(3));
        assert_eq!(list_move(5, Some(2), Up, 3), Some(1));
        assert_eq!(list_move(5, Some(0), PageDown, 3), Some(3));
        assert_eq!(list_move(5, Some(3), PageDown, 3), Some(4));
        assert_eq!(list_move(5, Some(4), PageUp, 3), Some(1));
        assert_eq!(list_move(5, Some(2), Left, 3), Some(2), "←/→ sans effet");
        assert_eq!(list_move(5, Some(2), End, 3), Some(4));
        assert_eq!(list_move(0, None, Down, 3), None);
    }

    #[test]
    fn shift_extends_from_the_anchor() {
        assert_eq!(extend_range(4, 1), 1..=4);
        assert_eq!(extend_range(1, 4), 1..=4);
        assert_eq!(extend_range(3, 3), 3..=3);
        assert_eq!(SelectMode::of(egui::Modifiers::SHIFT), SelectMode::Extend);
        assert_eq!(SelectMode::of(egui::Modifiers::COMMAND), SelectMode::MoveOnly);
        assert_eq!(SelectMode::of(egui::Modifiers::NONE), SelectMode::Replace);
    }

    fn row(id: &str, depth: usize, folder: bool, open: bool) -> TreeRow {
        TreeRow { id: (!id.is_empty()).then(|| id.to_string()), depth, folder, open }
    }

    /// ```text
    /// Toute la bibliothèque
    /// 📁 A (ouvert)
    ///    📄 a1
    ///    📁 B (fermé)
    /// 📄 c
    /// ```
    fn tree() -> Vec<TreeRow> {
        vec![
            row("", 0, false, false),
            row("A", 0, true, true),
            row("a1", 1, false, false),
            row("B", 1, true, false),
            row("c", 0, false, false),
        ]
    }

    #[test]
    fn tree_up_down_and_pages() {
        let t = tree();
        assert_eq!(tree_move(&t, Some(0), Down, 5), TreeAction::Select(1));
        assert_eq!(tree_move(&t, Some(3), Down, 5), TreeAction::Select(4));
        assert_eq!(tree_move(&t, Some(4), Down, 5), TreeAction::None, "dernière ligne");
        assert_eq!(tree_move(&t, Some(2), Up, 5), TreeAction::Select(1));
        assert_eq!(tree_move(&t, Some(0), Up, 5), TreeAction::None);
        assert_eq!(tree_move(&t, Some(3), Home, 5), TreeAction::Select(0));
        assert_eq!(tree_move(&t, Some(0), End, 5), TreeAction::Select(4));
        assert_eq!(tree_move(&t, Some(0), PageDown, 2), TreeAction::Select(2));
        assert_eq!(tree_move(&t, None, Down, 5), TreeAction::Select(0), "aucune liste : la première ligne");
        assert_eq!(tree_move(&[], None, Down, 5), TreeAction::None);
    }

    #[test]
    fn tree_right_opens_then_enters_left_closes_then_climbs() {
        let t = tree();
        assert_eq!(tree_move(&t, Some(3), Right, 5), TreeAction::Open(3), "dossier fermé : déplié");
        assert_eq!(tree_move(&t, Some(1), Right, 5), TreeAction::Select(2), "dossier ouvert : premier enfant");
        assert_eq!(tree_move(&t, Some(2), Right, 5), TreeAction::None, "liste : rien");
        assert_eq!(tree_move(&t, Some(1), Left, 5), TreeAction::Close(1), "dossier ouvert : replié");
        assert_eq!(tree_move(&t, Some(3), Left, 5), TreeAction::Select(1), "dossier fermé : parent");
        assert_eq!(tree_move(&t, Some(2), Left, 5), TreeAction::Select(1), "liste : parent");
        assert_eq!(tree_move(&t, Some(4), Left, 5), TreeAction::None, "premier niveau : rien");
        // Dossier ouvert mais vide : → ne fait rien.
        let empty = vec![row("D", 0, true, true), row("e", 0, false, false)];
        assert_eq!(tree_move(&empty, Some(0), Right, 5), TreeAction::None);
    }

    #[test]
    fn recorder_skips_rows_of_closed_folders() {
        let mut r = TreeRecorder::default();
        r.push(None, false, false);
        r.push(Some("A".into()), true, false);
        r.hidden += 1; // contenu de A dessiné pendant l'animation de fermeture
        r.depth += 1;
        r.push(Some("a1".into()), false, false);
        r.depth -= 1;
        r.hidden -= 1;
        r.push(Some("c".into()), false, false);
        let ids: Vec<Option<&str>> = r.rows.iter().map(|x| x.id.as_deref()).collect();
        assert_eq!(ids, [None, Some("A"), Some("c")]);
    }
}
