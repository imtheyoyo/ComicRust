//! Onglet « Pages » de la fenêtre « Infos du livre » : ordre et type des pages, comme
//! l'onglet « Pages » de ComicRack (`ComicBookDialog` + `PagesView`).
//!
//! - Grille de vignettes numérotées (ou liste détaillée), sélection multiple (clic,
//!   Ctrl+clic, Maj+clic, Ctrl+A), **glisser-déposer** pour changer l'ordre.
//! - Menu contextuel : Type de page (Alt+Maj+A…K), Rotation (Alt+Maj+6…9), Position,
//!   favoris (Ctrl+Maj+B / Ctrl+Maj+D), Mettre au début / à la fin (Ctrl+Alt+T / B),
//!   ordre d'origine, Copier (Ctrl+C), Tout sélectionner, Inverser, Rafraîchir, Marquer
//!   comme Supprimée (Suppr).
//! - « Pages ▾ » : affichage (vignettes / détails) et taille des vignettes ;
//!   « Réinitialiser ▾ » : ordre d'origine (clic), tri par nom, types, tout.
//! - Les vignettes sont décodées par un [`BookLoader`] (fil dédié, ordre brut du fichier) :
//!   l'interface ne bloque jamais.
//! - Lecture seule : consultation (sélection, copie), aucune modification.
//!
//! La logique (déplacements, types, réinitialisations, vérification) est dans
//! `comicrust_core::pages` ; l'enregistrement passe par `edit::FieldEdit::pages`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use comicrust_core::edit::FieldEdit;
use comicrust_core::library::{LibraryBook, PageInfo};
use comicrust_core::pages::{self, PAGE_TYPES, POSITIONS, ROTATIONS};
use eframe::egui::{self, pos2, vec2, Color32, Context, Key, Modifiers, Rect, RichText, Sense, TextureHandle, Ui};

use crate::book::loader::{BookLoader, Event, Order, Request};

/// Affichage de la liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagesView {
    Thumbnails,
    Details,
}

/// Tailles de vignettes proposées : (hauteur, libellé).
const SIZES: &[(f32, &str)] = &[(84.0, "Petites"), (124.0, "Moyennes"), (172.0, "Grandes")];

/// Action demandée (menu, raccourci, glisser-déposer), appliquée après le dessin.
#[derive(Debug, Clone, PartialEq)]
enum PageAction {
    SetType(&'static str),
    Rotate(&'static str),
    Position(&'static str),
    AskBookmark,
    RemoveBookmark,
    ToStart,
    ToEnd,
    ResetOrder,
    SortByName(bool),
    ResetTypes,
    ResetAll,
    Copy,
    SelectAll,
    Invert,
    Refresh,
    ToggleDeleted,
    /// Déplacer la sélection à cette place.
    MoveTo(usize),
}

/// Glisser-déposer en cours.
struct Drag {
    /// Place d'insertion sous le pointeur, et trait à dessiner.
    target: Option<(usize, [egui::Pos2; 2])>,
}

/// État de l'onglet pour un livre.
pub struct PagesTab {
    pub book_id: String,
    path: PathBuf,
    loader: BookLoader,
    opened: bool,
    error: Option<String>,
    /// Noms des images du fichier (ordre naturel).
    names: Vec<String>,
    /// Pages enregistrées dans la fiche.
    stored: Vec<PageInfo>,
    /// Liste complète à l'ouverture (référence pour « modifié »).
    original: Vec<PageInfo>,
    /// Liste complète modifiée (ordre de lecture).
    pub pages: Vec<PageInfo>,
    thumbs: HashMap<usize, Option<TextureHandle>>,
    requested: HashSet<usize>,
    /// Images sélectionnées.
    selected: BTreeSet<i32>,
    anchor: Option<i32>,
    view: PagesView,
    size: f32,
    drag: Option<Drag>,
    /// Favori en cours de saisie : (image, texte).
    bookmark: Option<(i32, String)>,
    /// Image à copier dès qu'elle est décodée.
    copy_wanted: Option<usize>,
    pub message: Option<String>,
}

impl PagesTab {
    /// Ouvre le fichier `path` du livre `book` (décodage en arrière-plan).
    pub fn new(ctx: &Context, book: &LibraryBook, path: PathBuf) -> PagesTab {
        let max_side = ctx.input(|i| i.max_texture_side).min(16384) as u32;
        PagesTab {
            book_id: book.id.clone(),
            loader: BookLoader::open(path.clone(), ctx.clone(), max_side, Order::Raw),
            path,
            opened: false,
            error: None,
            names: Vec::new(),
            stored: book.pages.page.clone(),
            original: Vec::new(),
            pages: Vec::new(),
            thumbs: HashMap::new(),
            requested: HashSet::new(),
            selected: BTreeSet::new(),
            anchor: None,
            view: PagesView::Thumbnails,
            size: SIZES[1].0,
            drag: None,
            bookmark: None,
            copy_wanted: None,
            message: None,
        }
    }

    /// Ordre ou attributs changés ?
    pub fn dirty(&self) -> bool {
        self.opened && !pages::same(&self.pages, &self.original)
    }

    /// Modification à enregistrer (liste complète), si l'onglet a changé quelque chose.
    pub fn edit(&self) -> Option<FieldEdit> {
        self.dirty().then(|| FieldEdit::pages(&self.book_id, self.pages.clone()))
    }

    /// Fiche enregistrée (relue) : elle devient la référence.
    pub fn saved(&mut self, book: &LibraryBook) {
        self.stored = book.pages.page.clone();
        if self.opened {
            self.original = pages::full_list(&self.stored, self.names.len());
            self.pages = self.original.clone();
        }
    }

    /// Événements du chargeur.
    fn poll(&mut self, ctx: &Context) {
        for ev in self.loader.poll() {
            match ev {
                Event::Opened { pages: names, total, .. } => {
                    self.names = names;
                    self.original = pages::full_list(&self.stored, total);
                    self.pages = self.original.clone();
                    self.opened = true;
                }
                Event::Thumb { index, image } => {
                    let tex =
                        image.map(|i| ctx.load_texture(format!("infos-page-{index}"), i, egui::TextureOptions::LINEAR));
                    self.thumbs.insert(index, tex);
                }
                Event::Page { index, result } => {
                    if self.copy_wanted == Some(index) {
                        self.copy_wanted = None;
                        match result {
                            Ok((img, _)) => {
                                ctx.copy_image(img);
                                self.message = Some(format!(
                                    "Page {} copiée dans le presse-papiers.",
                                    self.rank_of(index as i32) + 1
                                ));
                            }
                            Err(e) => self.message = Some(format!("Copie impossible : {e}")),
                        }
                    }
                }
                Event::Failed(e) => self.error = Some(e),
            }
        }
    }

    fn rank_of(&self, image: i32) -> usize {
        self.pages.iter().position(|p| p.image == image).unwrap_or(0)
    }

    /// Images sélectionnées, dans l'ordre de la liste.
    fn selection(&self) -> Vec<i32> {
        self.pages.iter().map(|p| p.image).filter(|i| self.selected.contains(i)).collect()
    }

    fn selected_pages(&self) -> impl Iterator<Item = &PageInfo> {
        self.pages.iter().filter(|p| self.selected.contains(&p.image))
    }

    /// Valeur commune à la sélection (`None` : aucune ou différentes), pour les coches.
    fn common<'a>(&'a self, f: impl Fn(&'a PageInfo) -> &'a str) -> Option<&'a str> {
        let mut it = self.selected_pages().map(f);
        let first = it.next()?;
        it.all(|v| v == first).then_some(first)
    }

    /// Clic sur la page `image` (au rang `rank`).
    fn click(&mut self, image: i32, mods: Modifiers) {
        if mods.shift {
            let a = self.anchor.map(|a| self.rank_of(a)).unwrap_or(0);
            let b = self.rank_of(image);
            if !mods.command {
                self.selected.clear();
            }
            for p in &self.pages[a.min(b)..=a.max(b)] {
                self.selected.insert(p.image);
            }
        } else if mods.command {
            if !self.selected.remove(&image) {
                self.selected.insert(image);
            }
            self.anchor = Some(image);
        } else {
            self.selected.clear();
            self.selected.insert(image);
            self.anchor = Some(image);
        }
    }

    fn apply(&mut self, ctx: &Context, a: PageAction, editable: bool) {
        let sel = self.selection();
        let modifies =
            !matches!(a, PageAction::Copy | PageAction::SelectAll | PageAction::Invert | PageAction::Refresh);
        if modifies && !editable {
            return;
        }
        match a {
            PageAction::SetType(t) => pages::set_type(&mut self.pages, &sel, t),
            PageAction::Rotate(r) => pages::set_rotation(&mut self.pages, &sel, r),
            PageAction::Position(p) => pages::set_position(&mut self.pages, &sel, p),
            PageAction::AskBookmark => {
                if let [img] = sel[..] {
                    self.bookmark = Some((img, pages::bookmark_proposal(&self.pages, img)));
                }
            }
            PageAction::RemoveBookmark => {
                if let [img] = sel[..] {
                    pages::set_bookmark(&mut self.pages, img, None);
                }
            }
            PageAction::ToStart => pages::move_to_start(&mut self.pages, &sel),
            PageAction::ToEnd => pages::move_to_end(&mut self.pages, &sel),
            PageAction::ResetOrder => pages::reset_order(&mut self.pages),
            PageAction::SortByName(numeric) => pages::sort_by_name(&mut self.pages, &self.names, numeric),
            PageAction::ResetTypes => pages::reset_types(&mut self.pages),
            PageAction::ResetAll => pages::reset_all(&mut self.pages),
            PageAction::ToggleDeleted => pages::toggle_deleted(&mut self.pages, &sel),
            PageAction::MoveTo(at) => pages::move_pages(&mut self.pages, at, &sel),
            PageAction::SelectAll => self.selected = self.pages.iter().map(|p| p.image).collect(),
            PageAction::Invert => {
                self.selected = self.pages.iter().map(|p| p.image).filter(|i| !self.selected.contains(i)).collect()
            }
            PageAction::Refresh => {
                let targets: Vec<i32> = if sel.is_empty() { self.pages.iter().map(|p| p.image).collect() } else { sel };
                for i in targets {
                    self.thumbs.remove(&(i as usize));
                    self.requested.remove(&(i as usize));
                    self.loader.request(Request::RefreshThumb(i as usize));
                    self.requested.insert(i as usize);
                }
            }
            PageAction::Copy => match sel[..] {
                [] => {}
                [img] => {
                    // Image décodée par le chargeur, puis mise dans le presse-papiers.
                    self.copy_wanted = Some(img as usize);
                    self.loader.request(Request::Page(img as usize));
                    self.message = Some("Copie de la page…".into());
                }
                _ => {
                    // Plusieurs pages : chemin du livre et nom de chaque image.
                    let lines: Vec<String> = sel
                        .iter()
                        .map(|&i| {
                            format!(
                                "{} | {}",
                                self.path.display(),
                                self.names.get(i as usize).map_or("", String::as_str)
                            )
                        })
                        .collect();
                    ctx.copy_text(lines.join("\n"));
                    self.message = Some(format!("{} chemin(s) de page copié(s).", lines.len()));
                }
            },
        }
    }

    /// Raccourcis clavier de l'onglet (quand aucun champ n'a le focus).
    fn shortcuts(&self, ctx: &Context) -> Option<PageAction> {
        if ctx.memory(|m| m.focused().is_some()) || self.bookmark.is_some() {
            return None;
        }
        let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
        ctx.input_mut(|i| {
            for (v, _, k) in PAGE_TYPES {
                if let Some(key) = Key::from_name(&k.to_string()) {
                    if i.consume_key(alt_shift, key) {
                        return Some(PageAction::SetType(v));
                    }
                }
            }
            for (v, _, k) in ROTATIONS {
                if let Some(key) = Key::from_name(&k.to_string()) {
                    if i.consume_key(alt_shift, key) {
                        return Some(PageAction::Rotate(v));
                    }
                }
            }
            let ctrl_shift = Modifiers::COMMAND | Modifiers::SHIFT;
            let ctrl_alt = Modifiers::COMMAND | Modifiers::ALT;
            if i.consume_key(ctrl_shift, Key::B) {
                return Some(PageAction::AskBookmark);
            }
            if i.consume_key(ctrl_shift, Key::D) {
                return Some(PageAction::RemoveBookmark);
            }
            if i.consume_key(ctrl_alt, Key::T) {
                return Some(PageAction::ToStart);
            }
            if i.consume_key(ctrl_alt, Key::B) {
                return Some(PageAction::ToEnd);
            }
            if i.consume_key(Modifiers::COMMAND, Key::A) {
                return Some(PageAction::SelectAll);
            }
            if i.consume_key(Modifiers::NONE, Key::Delete) {
                return Some(PageAction::ToggleDeleted);
            }
            // Ctrl+C arrive en événement « Copier » (egui-winit).
            let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
            if copy || i.consume_key(Modifiers::COMMAND, Key::C) {
                i.events.retain(|e| !matches!(e, egui::Event::Copy));
                return Some(PageAction::Copy);
            }
            None
        })
    }

    /// Menu contextuel (sélection non vide).
    fn context_menu(&self, ui: &mut Ui, editable: bool, out: &mut Option<PageAction>) {
        let n = self.selected.len();
        let mut item = |ui: &mut Ui, label: &str, short: &str, enabled: bool, checked: bool, a: PageAction| {
            let b = egui::Button::new(label).shortcut_text(short).selected(checked);
            if ui.add_enabled(enabled, b).clicked() {
                *out = Some(a);
                ui.close();
            }
        };
        if editable {
            let kind = self.common(pages::kind);
            ui.menu_button("Type de page", |ui| {
                for (v, l, k) in PAGE_TYPES {
                    item(ui, l, &format!("Alt+Maj+{k}"), true, kind == Some(v), PageAction::SetType(v));
                }
            });
            let rot = self.common(pages::rotation);
            ui.menu_button("Rotation de la page", |ui| {
                for (v, l, k) in ROTATIONS {
                    item(ui, l, &format!("Alt+Maj+{k}"), true, rot == Some(v), PageAction::Rotate(v));
                }
            });
            let pos = self.common(pages::position);
            ui.menu_button("Position de la page", |ui| {
                for (v, l) in POSITIONS {
                    item(ui, l, "", true, pos == Some(v), PageAction::Position(v));
                }
            });
            ui.separator();
            let one = n == 1;
            let has_bm = one && self.selected_pages().any(|p| p.bookmark.as_deref().is_some_and(|b| !b.is_empty()));
            item(ui, "Mettre en favoris…", "Ctrl+Maj+B", one, false, PageAction::AskBookmark);
            item(ui, "Supprimer favoris", "Ctrl+Maj+D", has_bm, false, PageAction::RemoveBookmark);
            ui.separator();
            item(ui, "Mettre au début", "Ctrl+Alt+T", n > 0, false, PageAction::ToStart);
            item(ui, "Mettre à la fin", "Ctrl+Alt+B", n > 0, false, PageAction::ToEnd);
            item(ui, "Remettre dans l'ordre d'origine", "", n > 0, false, PageAction::ResetOrder);
            ui.separator();
        }
        item(ui, "📋 Copier la page", "Ctrl+C", n > 0, false, PageAction::Copy);
        ui.separator();
        item(ui, "Tout sélectionner", "Ctrl+A", true, false, PageAction::SelectAll);
        item(ui, "Inverser la sélection", "", true, false, PageAction::Invert);
        item(ui, "🔄 Rafraîchir", "", true, false, PageAction::Refresh);
        if editable {
            ui.separator();
            item(ui, "✖ Marquer comme Supprimée", "Suppr", n > 0, false, PageAction::ToggleDeleted);
        }
    }

    /// Dessin de l'onglet (hauteur `height` disponible).
    pub fn show(&mut self, ui: &mut Ui, editable: bool, height: f32) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        let mut action: Option<PageAction> = None;

        ui.horizontal(|ui| {
            let text = if editable {
                "Utiliser un glisser/déposer ou le menu contextuel pour changer l'ordre ou le type des pages :"
            } else {
                "Pages du livre (consultation seulement) :"
            };
            ui.label(text);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Pages ▾", |ui| {
                    ui.label(RichText::new("Affichage").weak());
                    ui.radio_value(&mut self.view, PagesView::Thumbnails, "Vignettes");
                    ui.radio_value(&mut self.view, PagesView::Details, "Détails");
                    ui.separator();
                    ui.label(RichText::new("Taille des vignettes (Ctrl+molette)").weak());
                    for (h, l) in SIZES {
                        if ui.radio(self.size == *h, *l).clicked() {
                            self.size = *h;
                        }
                    }
                });
            });
        });

        if let Some(e) = &self.error {
            ui.colored_label(crate::ui::theme::error(ui), format!("Fichier illisible : {e}"));
            return;
        }
        if !self.opened {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Lecture des pages du fichier…");
            });
            return;
        }

        let list_h = (height - 64.0).max(120.0);
        let frame = egui::Frame::canvas(ui.style()).inner_margin(6.0);
        frame.show(ui, |ui| {
            ui.set_min_height(list_h);
            ui.set_max_height(list_h);
            match self.view {
                PagesView::Thumbnails => self.grid(ui, editable, list_h, &mut action),
                PagesView::Details => self.details(ui, editable, list_h, &mut action),
            }
        });

        ui.horizontal(|ui| {
            if let Some(m) = &self.message {
                ui.label(RichText::new(m).weak());
            } else if self.dirty() {
                ui.label(
                    RichText::new("● Pages modifiées : enregistrées par OK ou Appliquer")
                        .color(crate::ui::theme::accent(ui)),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(editable, |ui| {
                    ui.menu_button("▾", |ui| {
                        let mut item = |ui: &mut Ui, label: &str, a: PageAction| {
                            if ui.button(label).clicked() {
                                action = Some(a);
                                ui.close();
                            }
                        };
                        item(ui, "Remettre dans l'ordre d'origine", PageAction::ResetOrder);
                        item(ui, "Trier par nom", PageAction::SortByName(false));
                        item(ui, "Trier par nom (numérique)", PageAction::SortByName(true));
                        ui.separator();
                        item(ui, "Réinitialiser les types", PageAction::ResetTypes);
                        item(
                            ui,
                            "Tout réinitialiser (ordre, types, rotation, position, favoris)",
                            PageAction::ResetAll,
                        );
                    })
                    .response
                    .on_hover_text("Autres réinitialisations");
                    if ui.button("Réinitialiser").on_hover_text("Remettre les pages dans l'ordre du fichier").clicked()
                    {
                        action = Some(PageAction::ResetOrder);
                    }
                });
            });
        });

        if action.is_none() {
            action = self.shortcuts(&ctx);
        }
        if let Some(a) = action {
            self.message = None;
            self.apply(&ctx, a, editable);
        }
        self.bookmark_prompt(&ctx);
    }

    /// Saisie du nom d'un favori (`SelectItemDialog.GetName` de ComicRack).
    fn bookmark_prompt(&mut self, ctx: &Context) {
        let Some((img, mut text)) = self.bookmark.take() else { return };
        let mut done: Option<bool> = None;
        let modal = crate::ui::theme::modal(ctx, egui::Id::unique("infos-pages-favori"));
        modal.show(ctx, |ui| {
            ui.heading("Favori");
            ui.label(format!("Nom du favori de la page {} :", self.rank_of(img) + 1));
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(280.0));
            r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                done = Some(true);
            }
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    done = Some(true);
                }
                if ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) => {
                if !text.trim().is_empty() {
                    pages::set_bookmark(&mut self.pages, img, Some(&text));
                }
            }
            Some(false) => {}
            None => self.bookmark = Some((img, text)),
        }
    }

    /// Vignettes demandées au chargeur.
    fn want_thumb(&mut self, image: usize) {
        if self.requested.insert(image) {
            self.loader.request(Request::Thumb(image));
        }
    }

    /// Réaction commune d'une case (clic, menu, glisser).
    fn cell_response(
        &mut self,
        ui: &Ui,
        resp: &egui::Response,
        image: i32,
        editable: bool,
        action: &mut Option<PageAction>,
    ) {
        if resp.clicked() {
            let m = ui.input(|i| i.modifiers);
            self.click(image, m);
        }
        if resp.secondary_clicked() && !self.selected.contains(&image) {
            self.click(image, Modifiers::NONE);
        }
        if editable && resp.drag_started() {
            if !self.selected.contains(&image) {
                self.click(image, Modifiers::NONE);
            }
            self.drag = Some(Drag { target: None });
        }
        resp.context_menu(|ui| self.context_menu(ui, editable, action));
    }

    /// Fin d'un glisser-déposer : place d'insertion, défilement près des bords.
    fn drag_frame(&mut self, ui: &mut Ui, area: Rect, action: &mut Option<PageAction>) {
        let Some(drag) = &self.drag else { return };
        let (released, pointer) =
            ui.input(|i| (i.pointer.any_released() || !i.pointer.any_down(), i.pointer.hover_pos()));
        if let Some((_, [a, b])) = drag.target {
            ui.painter().line_segment([a, b], egui::Stroke::new(3.0, crate::ui::theme::accent(ui)));
        }
        if let Some(p) = pointer {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            // Défilement automatique près des bords.
            let edge = 28.0;
            if p.y < area.top() + edge {
                ui.scroll_with_delta(vec2(0.0, 8.0));
            } else if p.y > area.bottom() - edge {
                ui.scroll_with_delta(vec2(0.0, -8.0));
            }
        }
        if released {
            if let Some((at, _)) = drag.target {
                *action = Some(PageAction::MoveTo(at));
            }
            self.drag = None;
        }
    }

    /// Place d'insertion pour le pointeur au-dessus de la case `rank` (avant ou après
    /// selon la moitié survolée).
    fn drop_target(&mut self, ui: &Ui, rect: Rect, rank: usize, vertical: bool) {
        let Some(drag) = &mut self.drag else { return };
        let Some(p) = ui.input(|i| i.pointer.hover_pos()) else { return };
        if !rect.contains(p) {
            return;
        }
        let after = if vertical { p.y > rect.center().y } else { p.x > rect.center().x };
        let at = rank + after as usize;
        let line = if vertical {
            let y = if after { rect.bottom() } else { rect.top() };
            [pos2(rect.left(), y), pos2(rect.right(), y)]
        } else {
            let x = if after { rect.right() + 2.0 } else { rect.left() - 2.0 };
            [pos2(x, rect.top()), pos2(x, rect.bottom())]
        };
        drag.target = Some((at, line));
    }

    /// Grille de vignettes.
    fn grid(&mut self, ui: &mut Ui, editable: bool, height: f32, action: &mut Option<PageAction>) {
        // Ctrl+molette : taille des vignettes (comme ComicRack).
        if ui.rect_contains_pointer(ui.max_rect()) {
            let zoom = ui.input(|i| if i.modifiers.command { i.smooth_scroll_delta.y } else { 0.0 });
            if zoom != 0.0 {
                self.size = (self.size + zoom * 0.25).clamp(64.0, 180.0);
                ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
            }
        }
        if let Some(d) = &mut self.drag {
            d.target = None;
        }
        let h = self.size;
        let cell = vec2(h * 0.75 + 16.0, h + 24.0);
        let gap = 8.0;
        let cols = (((ui.available_width() + gap) / (cell.x + gap)).floor() as usize).max(1);
        let rows = self.pages.len().div_ceil(cols);
        let mut needed = Vec::new();
        let area = egui::ScrollArea::vertical().id_salt("infos-pages-grille").auto_shrink(false).max_height(height);
        let out = area.show_rows(ui, cell.y, rows, |ui, range| {
            ui.spacing_mut().item_spacing = vec2(gap, gap);
            for row in range {
                ui.horizontal(|ui| {
                    for rank in row * cols..((row + 1) * cols).min(self.pages.len()) {
                        let (rect, resp) = ui.allocate_exact_size(cell, Sense::click_and_drag());
                        let p = self.pages[rank].clone();
                        self.paint_cell(ui, rect, rank, &p, &resp, &mut needed);
                        self.cell_response(ui, &resp, p.image, editable, action);
                        self.drop_target(ui, rect, rank, false);
                    }
                });
            }
        });
        for i in needed {
            self.want_thumb(i);
        }
        self.drag_frame(ui, out.inner_rect, action);
    }

    /// Une vignette : image (tournée), badge du numéro, type, favori.
    fn paint_cell(
        &self,
        ui: &Ui,
        rect: Rect,
        rank: usize,
        p: &PageInfo,
        resp: &egui::Response,
        needed: &mut Vec<usize>,
    ) {
        let painter = ui.painter_at(rect.expand(3.0));
        let selected = self.selected.contains(&p.image);
        if selected {
            painter.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.55));
            painter.rect_stroke(rect, 4.0, ui.visuals().selection.stroke, egui::StrokeKind::Inside);
        } else if resp.hovered() {
            painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let img_box = Rect::from_min_max(rect.min + vec2(8.0, 6.0), rect.max - vec2(8.0, 20.0));
        let deleted = pages::is_type(p, "Deleted");
        let degrees = pages::rotation_degrees(p);
        let mut shown = img_box;
        match self.thumbs.get(&(p.image as usize)) {
            Some(Some(tex)) => {
                let s = tex.size_vec2();
                let turned = if degrees % 180 == 90 { vec2(s.y, s.x) } else { s };
                let k = (img_box.width() / turned.x).min(img_box.height() / turned.y);
                shown = Rect::from_center_size(img_box.center(), turned * k);
                let tint = if deleted { Color32::from_white_alpha(90) } else { Color32::WHITE };
                egui::Image::new((tex.id(), s * k))
                    .rotate((degrees as f32).to_radians(), vec2(0.5, 0.5))
                    .tint(tint)
                    .paint_at(ui, Rect::from_center_size(img_box.center(), s * k));
                painter.rect_stroke(shown, 0.0, (1.0, Color32::from_black_alpha(140)), egui::StrokeKind::Outside);
            }
            Some(None) => {
                painter.text(
                    img_box.center(),
                    egui::Align2::CENTER_CENTER,
                    "⚠",
                    egui::FontId::proportional(20.0),
                    crate::ui::theme::error(ui),
                );
            }
            None => {
                painter.rect_stroke(
                    img_box.shrink(img_box.width() * 0.15),
                    2.0,
                    (1.0, crate::ui::theme::placeholder_stroke(ui)),
                    egui::StrokeKind::Inside,
                );
                needed.push(p.image as usize);
            }
        }
        if deleted {
            painter.text(
                shown.center(),
                egui::Align2::CENTER_CENTER,
                "✖",
                egui::FontId::proportional(28.0),
                crate::ui::theme::error(ui),
            );
        }
        // Badge du numéro (coin haut droit de l'image).
        let num = format!("{}", rank + 1);
        let galley = painter.layout_no_wrap(num, egui::FontId::proportional(11.0), Color32::WHITE);
        let badge = Rect::from_min_size(
            pos2(shown.right() - galley.size().x - 10.0, shown.top() + 3.0),
            galley.size() + vec2(8.0, 2.0),
        );
        painter.rect_filled(badge, 3.0, Color32::from_black_alpha(190));
        painter.galley(badge.min + vec2(4.0, 1.0), galley, Color32::WHITE);
        // Légende : type (si ce n'est pas « Histoire »), favori.
        let mut caption = String::new();
        if p.bookmark.as_deref().is_some_and(|b| !b.is_empty()) {
            caption.push_str("★ ");
        }
        let k = pages::kind(p);
        if k != "Story" {
            caption.push_str(&pages::kind_label(k));
        }
        if !caption.is_empty() {
            painter.text(
                pos2(rect.center().x, rect.bottom() - 9.0),
                egui::Align2::CENTER_CENTER,
                caption,
                egui::FontId::proportional(10.5),
                if deleted { crate::ui::theme::error(ui) } else { ui.visuals().weak_text_color() },
            );
        }
        let tip = format!(
            "Page {} — {}{}",
            rank + 1,
            self.names.get(p.image as usize).map_or("", String::as_str),
            p.bookmark.as_deref().map(|b| format!("\nFavori : {b}")).unwrap_or_default()
        );
        if resp.hovered() && self.drag.is_none() {
            resp.clone().on_hover_text(tip);
        }
    }

    /// Liste détaillée.
    fn details(&mut self, ui: &mut Ui, editable: bool, height: f32, action: &mut Option<PageAction>) {
        if let Some(d) = &mut self.drag {
            d.target = None;
        }
        let row_h = 22.0;
        let cols: [(&str, f32); 7] = [
            ("N°", 44.0),
            ("Image", 200.0),
            ("Type", 150.0),
            ("Rotation", 70.0),
            ("Position", 80.0),
            ("Favori", 120.0),
            ("Dimensions", 90.0),
        ];
        ui.horizontal(|ui| {
            for (name, w) in cols {
                ui.add_sized([w, row_h], egui::Label::new(RichText::new(name).strong()));
            }
        });
        ui.separator();
        let area =
            egui::ScrollArea::vertical().id_salt("infos-pages-details").auto_shrink(false).max_height(height - 32.0);
        let out = area.show_rows(ui, row_h, self.pages.len(), |ui, range| {
            for rank in range {
                let p = self.pages[rank].clone();
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click_and_drag());
                if self.selected.contains(&p.image) {
                    ui.painter().rect_filled(rect, 2.0, ui.visuals().selection.bg_fill.gamma_multiply(0.55));
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let dims = match (&p.image_width, &p.image_height) {
                    (Some(w), Some(h)) if w != "0" => format!("{w} × {h}"),
                    _ => String::new(),
                };
                let rot = ROTATIONS.iter().find(|(v, _, _)| *v == pages::rotation(&p)).map_or("", |r| r.1);
                let pos = POSITIONS.iter().find(|(v, _)| *v == pages::position(&p)).map_or("", |r| r.1);
                let cells = [
                    (rank + 1).to_string(),
                    self.names.get(p.image as usize).cloned().unwrap_or_default(),
                    pages::kind_label(pages::kind(&p)),
                    rot.to_string(),
                    pos.to_string(),
                    p.bookmark.clone().unwrap_or_default(),
                    dims,
                ];
                let mut x = rect.left() + 4.0;
                let deleted = pages::is_type(&p, "Deleted");
                for ((_, w), text) in cols.iter().zip(cells) {
                    let r = Rect::from_min_size(pos2(x, rect.top()), vec2(*w, row_h));
                    ui.painter_at(r).text(
                        pos2(r.left(), r.center().y),
                        egui::Align2::LEFT_CENTER,
                        text,
                        egui::FontId::proportional(13.0),
                        if deleted { crate::ui::theme::error(ui) } else { ui.visuals().text_color() },
                    );
                    x += w + ui.spacing().item_spacing.x;
                }
                self.cell_response(ui, &resp, p.image, editable, action);
                self.drop_target(ui, rect, rank, true);
            }
        });
        self.drag_frame(ui, out.inner_rect, action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sélection au clavier/souris : clic, Ctrl+clic, Maj+clic sur l'ordre affiché.
    #[test]
    fn selection_follows_the_displayed_order() {
        let ctx = Context::default();
        let dir = tempfile::tempdir().unwrap();
        let book = LibraryBook { id: "x".into(), ..Default::default() };
        let mut t = PagesTab::new(&ctx, &book, dir.path().join("absent.cbz"));
        t.opened = true;
        t.original = pages::full_list(&[], 5);
        t.pages = t.original.clone();
        pages::move_to_start(&mut t.pages, &[4]);
        t.click(4, Modifiers::NONE);
        t.click(1, Modifiers::SHIFT);
        assert_eq!(t.selection(), [4, 0, 1], "plage dans l'ordre affiché");
        t.click(0, Modifiers::COMMAND);
        assert_eq!(t.selection(), [4, 1]);
        assert!(t.dirty());
        t.apply(&ctx, PageAction::ToEnd, true);
        assert_eq!(t.pages.iter().map(|p| p.image).collect::<Vec<_>>(), [0, 2, 3, 4, 1]);
        t.apply(&ctx, PageAction::SetType("Advertisement"), false);
        assert!(t.pages.iter().all(|p| pages::kind(p) != "Advertisement"), "lecture seule : rien");
        t.apply(&ctx, PageAction::ResetOrder, true);
        t.apply(&ctx, PageAction::Invert, false);
        assert_eq!(t.selection(), [0, 2, 3]);
        assert!(!t.dirty(), "ordre d'origine : plus rien à enregistrer");
        let e = t.edit();
        assert!(e.is_none());
        pages::set_type(&mut t.pages, &[2], "Deleted");
        assert_eq!(t.edit().unwrap().field, "Pages");
    }
}
