//! Gestion des listes dans l'interface (v0.9.0) : commandes du panneau des listes (bouton
//! « ＋ », menu contextuel, renommage, glisser-déposer), fenêtres de confirmation et de
//! propriétés, éditeur de listes intelligentes, et commandes des livres (« Ajouter à la
//! liste ▸ », « Retirer de la liste », « Monter / Descendre »).
//!
//! Les modifications passent par [`crate::library::lists_ops`] (un fil, une opération à la
//! fois) ; en mode « avec ComicRack », elles ne sont possibles que ComicRack fermé.

use std::collections::HashSet;

use comicrust_core::comic_lists::{Combine, ComicList, ListKind, Members};
use comicrust_core::lists_edit::NewList;
use eframe::egui::{self, Context, RichText, Ui};

use crate::app::ComicRustApp;
use crate::library::lists_ops::{
    self, ancestors, base_dependents, locate, step_position, subtree, DropPlace, ListOp, ListsOps, ListsTarget, OpTag,
    Outcome,
};
use crate::library::mouse::{books_drop_on_list, reorder_target, Anchor};
use crate::ui::book_drag::{BooksDrag, DropHint};
use crate::ui::smart_editor::{EditorEnv, EditorRequest, SmartEditor};

/// Message montré quand ComicRack est lancé.
pub const COMICRACK_RUNNING: &str =
    "Ferme ComicRack pour modifier ses listes : il réécrit ComicDb.xml en se fermant et effacerait les modifications.";

/// Commandes des listes grisées en mode « avec ComicRack » sans l'option d'écriture.
pub const READ_ONLY: &str = "Lecture seule : active l'écriture dans ⚙ → Assistant de configuration";

/// Les listes de ComicRack sont-elles modifiables d'après les réglages ? Mode autonome :
/// toujours ; avec ComicRack (MySQL ou ComicDb.xml seul) : seulement avec l'option
/// d'écriture de l'assistant (`comicrack_write`), sinon `ComicDb.xml` n'est jamais
/// réécrit.
pub fn lists_writable(mode: crate::settings::LibraryMode, comicrack_write: bool) -> Result<(), String> {
    use crate::settings::LibraryMode;
    match mode {
        LibraryMode::Standalone => Ok(()),
        LibraryMode::ComicRackMySql | LibraryMode::ComicRackXml if comicrack_write => Ok(()),
        LibraryMode::ComicRackMySql | LibraryMode::ComicRackXml => Err(READ_ONLY.into()),
        LibraryMode::Unset => Err("Bibliothèque non configurée".into()),
    }
}

/// Renommage en cours dans l'arbre.
#[derive(Debug, Clone, PartialEq)]
pub struct Renaming {
    pub id: String,
    pub text: String,
    pub focus: bool,
}

/// Fenêtres de dialogue des listes.
#[derive(Debug, Clone, PartialEq)]
pub enum ListDialog {
    /// Nom, description et (dossier) mode de combinaison.
    Props {
        id: String,
        name: String,
        description: String,
        combine: Option<Combine>,
        original: Box<(String, String, Option<Combine>)>,
    },
    Delete {
        id: String,
        name: String,
        sublists: usize,
        dependents: Vec<String>,
    },
    Convert {
        id: String,
        name: String,
        books: Vec<String>,
    },
    /// Nouvelle liste de lecture avec les livres sélectionnés.
    NewReading {
        name: String,
        books: Vec<String>,
    },
}

/// État de l'interface des listes (un champ de l'application).
#[derive(Default)]
pub struct ListsUi {
    pub ops: ListsOps,
    pub editor: Option<SmartEditor>,
    pub dialog: Option<ListDialog>,
    pub renaming: Option<Renaming>,
    /// Dossiers à déplier à la prochaine image (liste créée ou déplacée dedans).
    pub expand: HashSet<String>,
    /// Bilan de la dernière opération (affiché sous le panneau).
    pub info: Option<String>,
    /// Glisser de livres : ce que ferait le dépôt à l'endroit survolé (remis à zéro à chaque
    /// image, voir [`crate::ui::book_drag`]).
    pub drag_hint: Option<DropHint>,
    serial: u64,
}

/// Nouvelle liste demandée depuis le panneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewKind {
    Reading,
    Smart,
    Folder,
}

/// Commande du panneau des listes.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelAction {
    New {
        parent: Option<String>,
        position: Option<usize>,
        kind: NewKind,
    },
    Edit(String),
    Rename(String),
    CommitRename(String, String),
    Duplicate(String),
    Delete(String),
    Convert(String),
    Step(String, bool),
    MoveInto(String, Option<String>),
    Drop {
        dragged: String,
        target: String,
        place: DropPlace,
    },
    /// Livres glissés depuis la vue et déposés sur une liste de lecture.
    AddBooks {
        list: String,
        books: Vec<String>,
    },
}

/// Charge utile du glisser-déposer d'une liste.
#[derive(Debug, Clone)]
pub struct ListDrag(pub String);

/// Contexte de dessin de l'arbre des listes.
pub struct NodeCtx<'a> {
    pub items: &'a [ComicList],
    /// `Err` : lecture seule, avec la raison.
    pub editable: Result<(), String>,
    pub busy: bool,
    pub renaming: Option<Renaming>,
    pub expand: HashSet<String>,
    pub acts: Vec<PanelAction>,
    /// Liste affichée dans la vue (d'où viennent les livres glissés).
    pub shown: Option<String>,
    /// Glisser de livres au-dessus d'une liste : ce que ferait le dépôt.
    pub hint: Option<DropHint>,
    /// Navigation au clavier (v0.10.1) : lignes visibles relevées pendant le dessin.
    pub tree: crate::library::keynav::TreeRecorder,
}

impl NodeCtx<'_> {
    /// Commandes possibles ? Sinon la raison (infobulle des commandes grisées).
    pub fn reason(&self) -> Option<String> {
        match (&self.editable, self.busy) {
            (Err(r), _) => Some(r.clone()),
            (_, true) => Some("Enregistrement en cours…".into()),
            _ => None,
        }
    }
}

/// Bouton de menu actif seulement si les listes sont modifiables.
pub fn cmd(ui: &mut Ui, reason: &Option<String>, enabled: bool, text: &str) -> bool {
    let r = ui.add_enabled(reason.is_none() && enabled, egui::Button::new(text));
    let r = match reason {
        Some(why) => r.on_disabled_hover_text(why),
        None => r,
    };
    if r.clicked() {
        ui.close();
        true
    } else {
        false
    }
}

/// Sous-menu « Nouveau ▸ ».
pub fn new_menu(
    ui: &mut Ui,
    reason: &Option<String>,
    parent: Option<String>,
    position: Option<usize>,
    acts: &mut Vec<PanelAction>,
) {
    for (kind, text) in [
        (NewKind::Reading, "📄 Nouvelle liste de lecture"),
        (NewKind::Smart, "🔍 Nouvelle liste intelligente…"),
        (NewKind::Folder, "📁 Nouveau dossier"),
    ] {
        if cmd(ui, reason, true, text) {
            acts.push(PanelAction::New { parent: parent.clone(), position, kind });
        }
    }
}

/// Menu contextuel d'une liste du panneau.
pub fn list_menu(ui: &mut Ui, l: &ComicList, nc: &mut NodeCtx) {
    let reason = nc.reason();
    let folder = matches!(l.kind, ListKind::Folder { .. });
    let (parent, index) = locate(nc.items, &l.id).unwrap_or((None, 0));
    ui.menu_button("Nouveau", |ui| {
        if folder {
            new_menu(ui, &reason, Some(l.id.clone()), None, &mut nc.acts);
        } else {
            new_menu(ui, &reason, parent.clone(), Some(index + 1), &mut nc.acts);
        }
    });
    ui.separator();
    let edit_label = match l.kind {
        ListKind::Smart { .. } => "✏ Modifier les critères…",
        _ => "✏ Modifier…",
    };
    // L'éditeur s'ouvre même en lecture seule (consultation des critères).
    let edit_reason = match l.kind {
        ListKind::Smart { .. } => None,
        _ => reason.clone(),
    };
    if cmd(ui, &edit_reason, true, edit_label) {
        nc.acts.push(PanelAction::Edit(l.id.clone()));
    }
    if cmd(ui, &reason, true, "Renommer (F2)") {
        nc.acts.push(PanelAction::Rename(l.id.clone()));
    }
    if cmd(ui, &reason, true, "⧉ Dupliquer") {
        nc.acts.push(PanelAction::Duplicate(l.id.clone()));
    }
    if cmd(ui, &reason, true, "🗑 Supprimer…") {
        nc.acts.push(PanelAction::Delete(l.id.clone()));
    }
    if matches!(l.kind, ListKind::Smart { .. }) {
        ui.separator();
        if cmd(ui, &reason, true, "📄 Convertir en liste de lecture…") {
            nc.acts.push(PanelAction::Convert(l.id.clone()));
        }
    }
    ui.separator();
    if cmd(ui, &reason, step_position(nc.items, &l.id, true).is_some(), "⬆ Monter") {
        nc.acts.push(PanelAction::Step(l.id.clone(), true));
    }
    if cmd(ui, &reason, step_position(nc.items, &l.id, false).is_some(), "⬇ Descendre") {
        nc.acts.push(PanelAction::Step(l.id.clone(), false));
    }
    ui.add_enabled_ui(reason.is_none(), |ui| {
        ui.menu_button("Déplacer dans", |ui| {
            if cmd(ui, &None, parent.is_some(), "(premier niveau)") {
                nc.acts.push(PanelAction::MoveInto(l.id.clone(), None));
            }
            for (id, name, depth) in lists_ops::move_targets(nc.items, &l.id) {
                let here = parent.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(&id));
                if cmd(ui, &None, !here, &format!("{}📁 {name}", "    ".repeat(depth))) {
                    nc.acts.push(PanelAction::MoveInto(l.id.clone(), Some(id)));
                }
            }
        });
    })
    .response
    .on_disabled_hover_text(reason.clone().unwrap_or_default());
}

/// Où déposer selon la position du pointeur sur la ligne visée.
pub fn drop_place(rect: egui::Rect, y: f32, folder: bool) -> DropPlace {
    let t = (y - rect.top()) / rect.height().max(1.0);
    match (folder, t) {
        (true, t) if t < 0.25 => DropPlace::Before,
        (true, t) if t > 0.75 => DropPlace::After,
        (true, _) => DropPlace::Into,
        (false, t) if t < 0.5 => DropPlace::Before,
        _ => DropPlace::After,
    }
}

/// Glisser-déposer sur une ligne de l'arbre : repère visuel et action au lâcher.
pub fn handle_drop(ui: &Ui, r: &egui::Response, l: &ComicList, nc: &mut NodeCtx) {
    if nc.reason().is_some() {
        return;
    }
    r.dnd_set_drag_payload(ListDrag(l.id.clone()));
    let folder = matches!(l.kind, ListKind::Folder { .. });
    let Some(pointer) = ui.ctx().pointer_interact_pos() else { return };
    if let Some(p) = r.dnd_hover_payload::<ListDrag>() {
        if p.0 != l.id {
            let place = drop_place(r.rect, pointer.y, folder);
            let ok = lists_ops::drop_position(nc.items, &p.0, &l.id, place).is_some();
            let color = if ok { ui.visuals().selection.stroke.color } else { crate::ui::theme::error(ui) };
            let stroke = egui::Stroke::new(2.0, color);
            let painter = ui.painter();
            let _ = match place {
                DropPlace::Before => painter.hline(r.rect.x_range(), r.rect.top(), stroke),
                DropPlace::After => painter.hline(r.rect.x_range(), r.rect.bottom(), stroke),
                DropPlace::Into => painter.rect_stroke(r.rect, 3.0, stroke, egui::StrokeKind::Inside),
            };
        }
    }
    // `dnd_release_payload` retire la charge utile quel que soit son type : vérifier d'abord
    // que c'est bien une liste qui est glissée (et pas des livres).
    if r.dnd_hover_payload::<ListDrag>().is_none() {
        return;
    }
    if let Some(p) = r.dnd_release_payload::<ListDrag>() {
        if p.0 != l.id {
            let place = drop_place(r.rect, pointer.y, folder);
            nc.acts.push(PanelAction::Drop { dragged: p.0.clone(), target: l.id.clone(), place });
        }
    }
}

/// Livres glissés depuis la vue au-dessus d'une liste : mise en évidence de la liste si
/// elle les accepte (liste de lecture modifiable), ajout au lâcher.
pub fn handle_books_drop(ui: &Ui, r: &egui::Response, l: &ComicList, nc: &mut NodeCtx) {
    if r.dnd_hover_payload::<BooksDrag>().is_none() {
        return;
    }
    let reason = nc.reason();
    match books_drop_on_list(l, reason.as_deref(), nc.shown.as_deref()) {
        Ok(()) => {
            let v = ui.visuals();
            let rect = r.rect.expand(1.0);
            ui.painter().rect_filled(rect, 3.0, v.selection.bg_fill.gamma_multiply(0.5));
            ui.painter().rect_stroke(
                rect,
                3.0,
                egui::Stroke::new(2.0, v.selection.stroke.color),
                egui::StrokeKind::Inside,
            );
            nc.hint = Some(DropHint::Add(l.name.clone()));
            if let Some(p) = r.dnd_release_payload::<BooksDrag>() {
                nc.acts.push(PanelAction::AddBooks { list: l.id.clone(), books: p.books.clone() });
            }
        }
        Err(why) => nc.hint = Some(DropHint::Refused(why)),
    }
}

/// Champ de renommage dans l'arbre ; renvoie `true` s'il a été dessiné.
pub fn rename_field(ui: &mut Ui, l: &ComicList, nc: &mut NodeCtx) -> bool {
    let Some(ren) = nc.renaming.as_mut().filter(|r| r.id == l.id) else { return false };
    let r = ui.add(egui::TextEdit::singleline(&mut ren.text).desired_width(ui.available_width() - 8.0));
    if std::mem::take(&mut ren.focus) {
        r.request_focus();
        // Tout le texte sélectionné, comme dans ComicRack.
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), r.id) {
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(ren.text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), r.id);
        }
    }
    if r.lost_focus() {
        let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let text = ren.text.trim().to_string();
        if !cancel && !text.is_empty() && text != l.name {
            nc.acts.push(PanelAction::CommitRename(l.id.clone(), text));
        }
        nc.renaming = None;
    }
    true
}

/// Arbre des listes de lecture pour le menu « Ajouter à la liste ▸ ».
#[derive(Debug, Clone, PartialEq)]
pub enum MenuNode {
    Folder(String, Vec<MenuNode>),
    List(String, String),
}

pub fn reading_tree(items: &[ComicList]) -> Vec<MenuNode> {
    items
        .iter()
        .filter_map(|l| match &l.kind {
            ListKind::Ids(_) => Some(MenuNode::List(l.id.clone(), l.name.clone())),
            ListKind::Folder { children, .. } => {
                let sub = reading_tree(children);
                (!sub.is_empty()).then(|| MenuNode::Folder(l.name.clone(), sub))
            }
            _ => None,
        })
        .collect()
}

/// Données du menu des livres (calculées une fois par image).
pub struct BookMenuData {
    pub reason: Option<String>,
    pub tree: Vec<MenuNode>,
    /// Liste de lecture affichée.
    pub shown_reading: Option<String>,
    /// Réordonner possible (vue triée dans l'ordre de la liste) ? Sinon la raison.
    pub reorder: Result<(), String>,
}

/// Commande des livres sur les listes de lecture.
#[derive(Debug, Clone, PartialEq)]
pub enum BookListAction {
    Add(String),
    NewList,
    Remove,
    Shift(bool),
    /// Glisser-déposer dans la vue : déplacer les livres à ce repère.
    Reorder(Anchor),
}

pub fn book_lists_menu(
    ui: &mut Ui,
    data: &BookMenuData,
    books: &[String],
    request: &mut Option<(BookListAction, Vec<String>)>,
) {
    fn tree(ui: &mut Ui, nodes: &[MenuNode], books: &[String], request: &mut Option<(BookListAction, Vec<String>)>) {
        for n in nodes {
            match n {
                MenuNode::List(id, name) => {
                    if ui.button(format!("📄 {name}")).clicked() {
                        *request = Some((BookListAction::Add(id.clone()), books.to_vec()));
                        ui.close();
                    }
                }
                MenuNode::Folder(name, sub) => {
                    ui.menu_button(format!("📁 {name}"), |ui| tree(ui, sub, books, request));
                }
            }
        }
    }
    ui.separator();
    let n = books.len();
    let what = if n > 1 { format!("{n} livres") } else { "le livre".into() };
    let r = ui.add_enabled_ui(data.reason.is_none(), |ui| {
        ui.menu_button(format!("➕ Ajouter {what} à la liste"), |ui| {
            tree(ui, &data.tree, books, request);
            if !data.tree.is_empty() {
                ui.separator();
            }
            if ui.button("Nouvelle liste de lecture…").clicked() {
                *request = Some((BookListAction::NewList, books.to_vec()));
                ui.close();
            }
        });
    });
    if let Some(why) = &data.reason {
        r.response.on_disabled_hover_text(why);
    }
    if data.shown_reading.is_some() {
        if cmd(ui, &data.reason, true, &format!("➖ Retirer {what} de la liste (Suppr)")) {
            *request = Some((BookListAction::Remove, books.to_vec()));
        }
        let reason = data.reason.clone().or_else(|| data.reorder.clone().err());
        if cmd(ui, &reason, true, "⬆ Monter dans la liste") {
            *request = Some((BookListAction::Shift(true), books.to_vec()));
        }
        if cmd(ui, &reason, true, "⬇ Descendre dans la liste") {
            *request = Some((BookListAction::Shift(false), books.to_vec()));
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

impl ComicRustApp {
    /// Où enregistrer les listes, ou pourquoi elles sont en lecture seule.
    pub(crate) fn lists_target(&mut self, ctx: &Context) -> Result<ListsTarget, String> {
        use crate::settings::LibraryMode;
        lists_writable(self.settings.library_mode, self.settings.comicrack_write)?;
        match self.settings.library_mode {
            LibraryMode::Standalone => self
                .library
                .standalone()
                .cloned()
                .map(ListsTarget::Store)
                .ok_or_else(|| "Bibliothèque ComicRust non chargée".to_string()),
            LibraryMode::ComicRackMySql | LibraryMode::ComicRackXml => {
                if let Some(why) = self.lists_ui.ops.comicrack_refusal(ctx) {
                    return Err(why);
                }
                if let Some(e) = &self.library.lists.error {
                    return Err(format!("Listes de ComicRack illisibles : {e}"));
                }
                match &self.library.lists.path {
                    Some(p) => Ok(ListsTarget::ComicDb(p.clone())),
                    None => Err("ComicDb.xml de ComicRack pas encore trouvé".into()),
                }
            }
            LibraryMode::Unset => Err("Bibliothèque non configurée".into()),
        }
    }

    /// Lance une opération sur les listes.
    fn run_list_op(&mut self, ctx: &Context, tag: OpTag, op: ListOp) {
        let target = match self.lists_target(ctx) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("Listes non modifiées :\n\n{e}"));
                return;
            }
        };
        if !self.lists_ui.ops.start(target, tag, op, ctx) {
            self.error = Some("Un enregistrement des listes est déjà en cours : réessaie dans un instant.".into());
        }
    }

    /// Fenêtres des listes et résultat des enregistrements (à chaque image).
    pub(crate) fn lists_windows(&mut self, ctx: &Context) {
        if let Some((tag, what, res)) = self.lists_ui.ops.poll() {
            self.apply_list_result(tag, &what, res);
        }
        self.list_dialog(ctx);
        self.smart_editor_window(ctx);
    }

    fn apply_list_result(&mut self, tag: OpTag, what: &str, res: Result<lists_ops::Applied, String>) {
        match res {
            Ok(a) => {
                // Évènements de la surveillance déjà reçus (antérieurs à l'enregistrement) d'abord.
                self.poll_watch();
                // `ComicDb.xml` : son chemin ; base de ComicRust : aucun (comme la surveillance).
                self.library.lists.set(a.path.clone(), Ok(a.lists));
                let o = a.outcome;
                for id in o.select.iter().flatten().chain(o.rename.iter()).chain(o.created.iter()) {
                    self.lists_ui.expand.extend(ancestors(&self.library.lists.items, id));
                }
                match o.select {
                    Some(sel) => self.select_list(sel),
                    None => {
                        if self.library.selected_list.is_some() && self.library.selected_list().is_none() {
                            self.select_list(None);
                        }
                    }
                }
                if let Some(id) = o.rename {
                    let name =
                        ComicList::find(&self.library.lists.items, &id).map(|l| l.name.clone()).unwrap_or_default();
                    self.lists_ui.renaming = Some(Renaming { id, text: name, focus: true });
                }
                let mut info = o.info.unwrap_or_else(|| format!("✔ {what}"));
                if let Some(b) = a.backup.as_ref().and_then(|b| b.file_name()) {
                    info.push_str(&format!("\nSauvegarde : {}", b.to_string_lossy()));
                }
                if !a.warnings.is_empty() {
                    self.bg_errors.lock().unwrap().push(format!(
                        "Listes enregistrées, mais problème de sauvegarde de ComicDb.xml :\n{}",
                        a.warnings.join("\n")
                    ));
                }
                self.lists_ui.info = Some(info);
                if tag == OpTag::Editor {
                    if let Some(ed) = &mut self.lists_ui.editor {
                        let close = ed.saving.take() == Some(true);
                        ed.error = None;
                        if let Some(id) = o.created {
                            ed.id = Some(id);
                        }
                        ed.saved = ed.draft.clone();
                        if close {
                            self.lists_ui.editor = None;
                        }
                    }
                }
            }
            Err(e) => {
                if tag == OpTag::Editor {
                    if let Some(ed) = &mut self.lists_ui.editor {
                        ed.saving = None;
                        ed.error = Some(e);
                        return;
                    }
                }
                self.lists_ui.info = None;
                self.error = Some(format!("{what} : rien n'a été enregistré.\n\n{e}"));
            }
        }
    }

    /// Emplacement d'une nouvelle liste : juste après la liste affichée (comme ComicRack),
    /// sinon à la fin du premier niveau.
    pub(crate) fn new_list_place(&self) -> (Option<String>, Option<usize>) {
        let items = &self.library.lists.items;
        match self.library.selected_list.as_deref().and_then(|id| locate(items, id)) {
            Some((parent, i)) => (parent, Some(i + 1)),
            None => (None, None),
        }
    }

    pub(crate) fn handle_panel_action(&mut self, ctx: &Context, a: PanelAction) {
        let items = self.library.lists.items.clone();
        let name_of = |id: &str| ComicList::find(&items, id).map(|l| l.name.clone()).unwrap_or_default();
        match a {
            PanelAction::New { parent, position, kind } => {
                let (name, new) = match kind {
                    NewKind::Smart => {
                        self.lists_ui.serial += 1;
                        self.lists_ui.editor = Some(SmartEditor::create(parent, position, self.lists_ui.serial));
                        return;
                    }
                    NewKind::Reading => ("Nouvelle liste", NewList::Reading { books: Vec::new() }),
                    NewKind::Folder => ("Nouveau dossier", NewList::Folder { combine: Combine::Or }),
                };
                let op = ListOp::new(format!("Création de « {name} »"), move |ed| {
                    let id = ed.create(parent.as_deref(), position, name, new)?;
                    Ok(Outcome {
                        select: Some(Some(id.clone())),
                        rename: Some(id.clone()),
                        created: Some(id),
                        info: None,
                    })
                });
                self.run_list_op(ctx, OpTag::Panel, op);
            }
            PanelAction::Edit(id) => {
                let Some(l) = ComicList::find(&items, &id) else { return };
                match &l.kind {
                    ListKind::Smart { .. } => {
                        self.lists_ui.serial += 1;
                        self.lists_ui.editor = Some(SmartEditor::edit(l, self.lists_ui.serial));
                    }
                    ListKind::Library => {}
                    kind => {
                        let combine = match kind {
                            ListKind::Folder { combine, .. } => Some(*combine),
                            _ => None,
                        };
                        self.lists_ui.dialog = Some(ListDialog::Props {
                            id: l.id.clone(),
                            name: l.name.clone(),
                            description: l.description.clone(),
                            combine,
                            original: Box::new((l.name.clone(), l.description.clone(), combine)),
                        });
                    }
                }
            }
            PanelAction::Rename(id) => {
                let name = name_of(&id);
                self.lists_ui.expand.extend(ancestors(&items, &id));
                self.lists_ui.renaming = Some(Renaming { id, text: name, focus: true });
            }
            PanelAction::CommitRename(id, name) => {
                let op = ListOp::new(format!("Renommage en « {name} »"), move |ed| {
                    ed.rename(&id, &name)?;
                    Ok(Outcome::default())
                });
                self.run_list_op(ctx, OpTag::Panel, op);
            }
            PanelAction::Duplicate(id) => {
                let name = name_of(&id);
                let op = ListOp::new(format!("Copie de « {name} »"), move |ed| {
                    let new = ed.copy(&id)?;
                    Ok(Outcome { select: Some(Some(new.clone())), created: Some(new), ..Default::default() })
                });
                self.run_list_op(ctx, OpTag::Panel, op);
            }
            PanelAction::Delete(id) => {
                self.lists_ui.dialog = Some(ListDialog::Delete {
                    name: name_of(&id),
                    sublists: subtree(&items, &id).len().saturating_sub(1),
                    dependents: base_dependents(&items, &id),
                    id,
                });
            }
            PanelAction::Convert(id) => {
                let books = match self.library.list_members(&id) {
                    Some(Members::Ids(ids)) => ids,
                    Some(Members::All) => self
                        .library
                        .database()
                        .map(|db| db.books.iter().map(|b| b.id.clone()).collect())
                        .unwrap_or_default(),
                    Some(Members::Unknown(why)) => {
                        self.error = Some(format!(
                            "« {} » ne peut pas être convertie : ComicRust ne connaît pas son contenu ({why}).",
                            name_of(&id)
                        ));
                        return;
                    }
                    None => return,
                };
                self.lists_ui.dialog = Some(ListDialog::Convert { name: name_of(&id), id, books });
            }
            PanelAction::Step(id, up) => {
                let Some((parent, to)) = step_position(&items, &id, up) else { return };
                let op = ListOp::new(format!("Déplacement de « {} »", name_of(&id)), move |ed| {
                    ed.move_to(&id, parent.as_deref(), Some(to))?;
                    Ok(Outcome::default())
                });
                self.run_list_op(ctx, OpTag::Panel, op);
            }
            PanelAction::MoveInto(id, folder) => {
                let what = format!("Déplacement de « {} »", name_of(&id));
                if let Some(f) = &folder {
                    self.lists_ui.expand.insert(f.clone());
                }
                let op = ListOp::new(what, move |ed| {
                    ed.move_to(&id, folder.as_deref(), None)?;
                    Ok(Outcome::default())
                });
                self.run_list_op(ctx, OpTag::Panel, op);
            }
            PanelAction::AddBooks { list, books } => self.book_list_request(ctx, BookListAction::Add(list), books),
            PanelAction::Drop { dragged, target, place } => {
                let Some((parent, position)) = lists_ops::drop_position(&items, &dragged, &target, place) else {
                    return;
                };
                if let Some(p) = &parent {
                    self.lists_ui.expand.insert(p.clone());
                }
                let op = ListOp::new(format!("Déplacement de « {} »", name_of(&dragged)), move |ed| {
                    ed.move_to(&dragged, parent.as_deref(), position)?;
                    Ok(Outcome::default())
                });
                self.run_list_op(ctx, OpTag::Panel, op);
            }
        }
    }

    /// Fenêtre de dialogue des listes ouverte (une à la fois).
    fn list_dialog(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.lists_ui.dialog.take() else { return };
        let busy = self.lists_ui.ops.busy();
        let mut close = false;
        let mut confirm = false;
        let modal = egui::Modal::new(egui::Id::unique("dialogue-listes")).show(ctx, |ui| {
            ui.set_width(440.0);
            match &mut dialog {
                ListDialog::Props { name, description, combine, .. } => {
                    ui.heading(if combine.is_some() { "Modifier le dossier" } else { "Modifier la liste" });
                    ui.add_space(6.0);
                    egui::Grid::new("proprietes-liste").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label("Nom :");
                        ui.add(egui::TextEdit::singleline(name).desired_width(320.0));
                        ui.end_row();
                        ui.label("Description :");
                        ui.add(egui::TextEdit::multiline(description).desired_rows(3).desired_width(320.0));
                        ui.end_row();
                    });
                    if let Some(c) = combine {
                        ui.add_space(6.0);
                        ui.label("Livres du dossier :");
                        ui.radio_value(c, Combine::Or, "ceux d'au moins une des sous-listes (réunion)");
                        ui.radio_value(c, Combine::And, "ceux de toutes les sous-listes (intersection)");
                        ui.radio_value(c, Combine::Empty, "aucun (dossier de rangement)");
                    }
                }
                ListDialog::Delete { name, sublists, dependents, .. } => {
                    ui.heading("Supprimer la liste");
                    ui.add_space(6.0);
                    ui.label(format!("Supprimer « {name} » ?"));
                    if *sublists > 0 {
                        ui.label(
                            RichText::new(format!(
                                "Ce dossier contient {sublists} sous-liste{} : {} avec lui.",
                                plural(*sublists),
                                if *sublists > 1 { "elles seront supprimées" } else { "elle sera supprimée" }
                            ))
                            .color(crate::ui::theme::warning(ui)),
                        );
                    }
                    if !dependents.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "Ces listes intelligentes partent de cette liste ; elles partiront de toute la \
                                 bibliothèque : {}",
                                dependents.iter().map(|d| format!("« {d} »")).collect::<Vec<_>>().join(", ")
                            ))
                            .color(crate::ui::theme::warning(ui)),
                        );
                    }
                    ui.label(RichText::new("Les livres eux-mêmes ne sont pas supprimés.").weak());
                }
                ListDialog::Convert { name, books, .. } => {
                    ui.heading("Convertir en liste de lecture");
                    ui.add_space(6.0);
                    ui.label(format!(
                        "« {name} » devient une liste de lecture contenant ses {} livre{} actuels ; ses critères sont \
                         abandonnés.",
                        books.len(),
                        plural(books.len())
                    ));
                    ui.label(
                        RichText::new("Les listes intelligentes qui en partent continuent de fonctionner.").weak(),
                    );
                }
                ListDialog::NewReading { name, books } => {
                    ui.heading("Nouvelle liste de lecture");
                    ui.add_space(6.0);
                    ui.label(format!(
                        "Avec {} livre{} sélectionné{}.",
                        books.len(),
                        plural(books.len()),
                        plural(books.len())
                    ));
                    ui.horizontal(|ui| {
                        ui.label("Nom :");
                        let r = ui.add(egui::TextEdit::singleline(name).desired_width(300.0));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            confirm = true;
                        }
                        r.request_focus();
                    });
                }
            }
            ui.add_space(10.0);
            let label = match &dialog {
                ListDialog::Delete { .. } => "Supprimer",
                ListDialog::Convert { .. } => "Convertir",
                ListDialog::NewReading { .. } => "Créer",
                ListDialog::Props { .. } => "OK",
            };
            let valid = match &dialog {
                ListDialog::Props { name, .. } | ListDialog::NewReading { name, .. } => !name.trim().is_empty(),
                _ => true,
            };
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Annuler").clicked() {
                        close = true;
                    }
                    if ui
                        .add_enabled(valid && !busy, egui::Button::new(label))
                        .on_disabled_hover_text(if busy { "Enregistrement en cours…" } else { "Nom vide" })
                        .clicked()
                    {
                        confirm = true;
                    }
                });
            });
        });
        if modal.should_close() {
            close = true;
        }
        if confirm && !busy {
            self.confirm_dialog(ctx, dialog);
        } else if !close {
            self.lists_ui.dialog = Some(dialog);
        }
    }

    fn confirm_dialog(&mut self, ctx: &Context, dialog: ListDialog) {
        let op = match dialog {
            ListDialog::Props { id, name, description, combine, original } => {
                let (old_name, old_desc, old_combine) = *original;
                if name.trim() == old_name && description == old_desc && combine == old_combine {
                    return;
                }
                ListOp::new(format!("Modification de « {} »", name.trim()), move |ed| {
                    if name.trim() != old_name {
                        ed.rename(&id, &name)?;
                    }
                    if description != old_desc {
                        ed.set_description(&id, &description)?;
                    }
                    if let Some(c) = combine.filter(|c| Some(*c) != old_combine) {
                        ed.set_combine(&id, c)?;
                    }
                    Ok(Outcome::default())
                })
            }
            ListDialog::Delete { id, name, .. } => {
                let selected = self.library.selected_list.clone();
                ListOp::new(format!("Suppression de « {name} »"), move |ed| {
                    let report = ed.delete(&id)?;
                    let gone = selected.as_deref().is_some_and(|s| report.removed.contains(&s.to_ascii_lowercase()));
                    let mut info = format!("« {name} » supprimée");
                    if report.removed.len() > 1 {
                        info.push_str(&format!(" (avec {} sous-listes)", report.removed.len() - 1));
                    }
                    if !report.broken_base.is_empty() {
                        info.push_str(&format!(
                            " ; partent maintenant de toute la bibliothèque : {}",
                            report.broken_base.iter().map(|(_, n)| format!("« {n} »")).collect::<Vec<_>>().join(", ")
                        ));
                    }
                    Ok(Outcome { select: gone.then_some(None), info: Some(info), ..Default::default() })
                })
            }
            ListDialog::Convert { id, name, books } => ListOp::new(format!("Conversion de « {name} »"), move |ed| {
                let n = books.len();
                ed.convert_to_reading_list(&id, books)?;
                Ok(Outcome::info(format!("« {name} » est maintenant une liste de lecture ({n} livre{})", plural(n))))
            }),
            ListDialog::NewReading { name, books } => {
                let (parent, position) = self.new_list_place();
                let name = name.trim().to_string();
                ListOp::new(format!("Création de « {name} »"), move |ed| {
                    let n = books.len();
                    let id = ed.create(parent.as_deref(), position, &name, NewList::Reading { books })?;
                    Ok(Outcome {
                        created: Some(id),
                        info: Some(format!("Liste « {name} » créée avec {n} livre{}", plural(n))),
                        ..Default::default()
                    })
                })
            }
        };
        self.run_list_op(ctx, OpTag::Panel, op);
    }

    fn smart_editor_window(&mut self, ctx: &Context) {
        if self.lists_ui.editor.is_none() {
            return;
        }
        let editable = self.lists_target(ctx).map(|_| ());
        let items = std::mem::take(&mut self.library.lists.items);
        let env = EditorEnv { items: &items, db: self.library.database(), editable, busy: self.lists_ui.ops.busy() };
        let req = self.lists_ui.editor.as_mut().map(|ed| ed.show(ctx, &env)).unwrap_or(EditorRequest::None);
        self.library.lists.items = items;
        match req {
            EditorRequest::None => {}
            EditorRequest::Close => self.lists_ui.editor = None,
            EditorRequest::Save { close } => self.save_smart_editor(ctx, close),
        }
    }

    fn save_smart_editor(&mut self, ctx: &Context, close: bool) {
        let Some(ed) = &mut self.lists_ui.editor else { return };
        let Ok(query) = ed.draft.to_query() else { return };
        let name = ed.draft.name.trim().to_string();
        let description = ed.draft.description.clone();
        let saved_query = ed.saved.to_query().ok();
        let (old_name, old_desc) = (ed.saved.name.clone(), ed.saved.description.clone());
        let op = match ed.id.clone() {
            Some(id) => ListOp::new(format!("Modification de « {name} »"), move |e| {
                if name != old_name {
                    e.rename(&id, &name)?;
                }
                if description != old_desc {
                    e.set_description(&id, &description)?;
                }
                if saved_query.as_ref() != Some(&query) {
                    e.set_query(&id, &query)?;
                }
                Ok(Outcome::default())
            }),
            None => {
                let (parent, position) = (ed.parent.clone(), ed.position);
                ListOp::new(format!("Création de « {name} »"), move |e| {
                    let id = e.create(parent.as_deref(), position, &name, NewList::Smart { query })?;
                    if !description.is_empty() {
                        e.set_description(&id, &description)?;
                    }
                    Ok(Outcome { select: Some(Some(id.clone())), created: Some(id), ..Default::default() })
                })
            }
        };
        ed.saving = Some(close);
        ed.error = None;
        let target = self.lists_target(ctx);
        let started = match target {
            Ok(t) => self.lists_ui.ops.start(t, OpTag::Editor, op, ctx),
            Err(e) => {
                if let Some(ed) = &mut self.lists_ui.editor {
                    ed.error = Some(e);
                }
                false
            }
        };
        if !started {
            if let Some(ed) = &mut self.lists_ui.editor {
                ed.saving = None;
                ed.error.get_or_insert_with(|| "un autre enregistrement est en cours".into());
            }
        }
    }

    // -----------------------------------------------------------------------
    // Livres et listes de lecture
    // -----------------------------------------------------------------------

    /// Données du menu « Ajouter à la liste ▸ » (une fois par image).
    pub(crate) fn book_menu_data(&mut self, ctx: &Context) -> BookMenuData {
        let reason = match (self.lists_target(ctx), self.lists_ui.ops.busy()) {
            (Err(r), _) => Some(r),
            (_, true) => Some("Enregistrement en cours…".into()),
            _ => None,
        };
        let shown_reading = self.library.reading_list_shown().then(|| self.library.selected_list.clone()).flatten();
        let reorder =
            if self.settings.library_sort == crate::library::SortKey::Position && !self.settings.library_descending {
                Ok(())
            } else {
                Err("Trie la vue dans l'ordre de la liste (« # » croissant) pour réordonner".into())
            };
        BookMenuData { reason, tree: reading_tree(&self.library.lists.items), shown_reading, reorder }
    }

    /// Commande des livres choisie dans le menu (ou touche Suppr).
    pub(crate) fn book_list_request(&mut self, ctx: &Context, action: BookListAction, books: Vec<String>) {
        if books.is_empty() {
            return;
        }
        let n = books.len();
        let items = self.library.lists.items.clone();
        let name_of = |id: &str| ComicList::find(&items, id).map(|l| l.name.clone()).unwrap_or_default();
        let shown = self.library.reading_list_shown().then(|| self.library.selected_list.clone()).flatten();
        let op = match action {
            BookListAction::NewList => {
                self.lists_ui.dialog = Some(ListDialog::NewReading { name: "Nouvelle liste de lecture".into(), books });
                return;
            }
            BookListAction::Add(list) => {
                let name = name_of(&list);
                ListOp::new(format!("Ajout à « {name} »"), move |ed| {
                    let added = ed.add_books(&list, &books, None)?;
                    let mut info = format!("{added} livre{} ajouté{} à « {name} »", plural(added), plural(added));
                    if added < n {
                        info.push_str(&format!(" ({} déjà dans la liste)", n - added));
                    }
                    Ok(Outcome::info(info))
                })
            }
            BookListAction::Remove => {
                let Some(list) = shown else { return };
                // Les livres retirés quittent la vue : plus de sélection.
                self.library.selected_book = None;
                self.library.selected_books.clear();
                let name = name_of(&list);
                ListOp::new(format!("Retrait de « {name} »"), move |ed| {
                    let removed = ed.remove_books(&list, &books)?;
                    Ok(Outcome::info(format!(
                        "{removed} livre{} retiré{} de « {name} »",
                        plural(removed),
                        plural(removed)
                    )))
                })
            }
            BookListAction::Shift(up) => {
                let Some(list) = shown else { return };
                ListOp::new("Réordonnancement de la liste", move |ed| {
                    let current = ed.books(&list)?;
                    if let Some(to) = lists_ops::shift_books(&current, &books, up) {
                        ed.move_books(&list, &books, to)?;
                    }
                    Ok(Outcome::default())
                })
            }
            BookListAction::Reorder(anchor) => {
                let Some(list) = shown else { return };
                // Dépôt sans effet (à sa place) : rien à enregistrer.
                let current = match ComicList::find(&items, &list).map(|l| &l.kind) {
                    Some(ListKind::Ids(ids)) => ids.clone(),
                    _ => return,
                };
                if reorder_target(&current, &books, &anchor).is_none() {
                    return;
                }
                let what = format!("Réordonnancement de « {} »", name_of(&list));
                ListOp::new(what, move |ed| {
                    let current = ed.books(&list)?;
                    if let Some(to) = reorder_target(&current, &books, &anchor) {
                        ed.move_books(&list, &books, to)?;
                    }
                    Ok(Outcome::default())
                })
            }
        };
        self.run_list_op(ctx, OpTag::Panel, op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_read_only_without_write_option() {
        use crate::settings::LibraryMode;
        assert_eq!(lists_writable(LibraryMode::Standalone, false), Ok(()), "mode autonome : toujours modifiable");
        for mode in [LibraryMode::ComicRackMySql, LibraryMode::ComicRackXml] {
            assert_eq!(lists_writable(mode, false), Err(READ_ONLY.to_string()), "{mode:?} en lecture seule");
            assert_eq!(lists_writable(mode, true), Ok(()), "{mode:?} avec l'écriture");
        }
        assert!(lists_writable(LibraryMode::Unset, true).is_err());
        assert!(READ_ONLY.contains("Assistant de configuration"));
        // Textes de l'assistant : cohérents avec ce qui est réellement modifiable.
        use crate::ui::setup_window::{setup_note, write_label};
        assert_eq!(write_label(true), "Lecture et écriture des listes (ComicRack fermé)");
        assert!(write_label(false).contains("listes"));
        assert!(setup_note(true, false).contains("ni les listes"));
        assert!(setup_note(false, false).contains("ni la base ni les listes"));
        assert!(setup_note(true, true).contains("livres de ComicDb.xml, eux, ne sont pas modifiés"));
    }

    fn l(id: &str, kind: ListKind) -> ComicList {
        ComicList { id: id.into(), name: id.into(), description: String::new(), kind, comicrack_count: 0 }
    }

    #[test]
    fn reading_tree_keeps_only_reading_lists() {
        let items = vec![
            l("lib", ListKind::Library),
            l("a", ListKind::Ids(vec![])),
            l(
                "f",
                ListKind::Folder {
                    combine: Combine::Or,
                    children: vec![
                        l("s", ListKind::Smart { query: None, cached: None }),
                        l("b", ListKind::Ids(vec![])),
                    ],
                },
            ),
            l("vide", ListKind::Folder { combine: Combine::Or, children: vec![] }),
        ];
        assert_eq!(
            reading_tree(&items),
            [
                MenuNode::List("a".into(), "a".into()),
                MenuNode::Folder("f".into(), vec![MenuNode::List("b".into(), "b".into())])
            ]
        );
    }

    #[test]
    fn drop_zones() {
        let r = egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(100.0, 20.0));
        assert_eq!(drop_place(r, 102.0, true), DropPlace::Before);
        assert_eq!(drop_place(r, 110.0, true), DropPlace::Into);
        assert_eq!(drop_place(r, 118.0, true), DropPlace::After);
        assert_eq!(drop_place(r, 109.0, false), DropPlace::Before);
        assert_eq!(drop_place(r, 111.0, false), DropPlace::After);
    }
}
