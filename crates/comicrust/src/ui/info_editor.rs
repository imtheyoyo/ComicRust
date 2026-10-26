//! Fenêtre « Infos du livre » (v0.10.1) : modification manuelle des fiches, comme la
//! fenêtre « Informations » de ComicRack (`ComicBookDialog` pour un livre,
//! `MultipleComicBooksDialog` pour plusieurs).
//!
//! - Ouverture : Ctrl+I (comme ComicRack), menu contextuel « Modifier les infos… », bouton
//!   « ✏ Modifier » ou double-clic sur la couverture du panneau de détails.
//! - Un livre : boutons Précédent / Suivant (livres de la vue, la sélection suit ; les
//!   modifications en cours sont enregistrées avant de changer de livre).
//! - Plusieurs livres : un champ dont les valeurs diffèrent affiche « (valeurs multiples) »
//!   et n'est écrit que si l'utilisateur le change ou le vide (✖).
//! - Écriture : dans un fil (`scripts::apply::apply_plan`, comme les scripts), jamais sur
//!   le fil de l'interface ; la bibliothèque affichée et la surveillance sont mises à jour.
//! - Lecture seule (ComicRack sans écriture, `ComicDb.xml`) : consultation seulement, avec
//!   la marche à suivre.
//! - Scripts `#@Hook Editor` : bouton « 📜 Scripts » ; les modifications du script
//!   remplissent les champs de la fenêtre, rien n'est écrit avant OK / Appliquer.
//!
//! La logique (valeurs, valeurs multiples, validation, suggestions) est dans
//! `library/info_form.rs`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use comicrust_core::db::ComicDatabase;
use comicrust_core::edit::{self, FieldDef, FieldKind};
use comicrust_core::library::LibraryBook;
use comicrust_core::scripting::{Hook, RunState, ScriptChange};
use eframe::egui::{self, vec2, Context, Id, RichText, Sense, Ui};

use crate::app::ComicRustApp;
use crate::library::files::FileAction;
use crate::library::info_form::{self, InfoForm, TABS};
use crate::scripts::apply::{apply_plan, ApplyOutcome, ApplyPlan, ApplyTarget};
use crate::ui::library_view::{paint_cover, ACCENT};

/// Que faire une fois l'enregistrement fini.
#[derive(Debug, Clone, PartialEq)]
enum After {
    Close,
    Stay,
    /// Passer à ce livre (Précédent / Suivant).
    Go(String),
}

struct Saving {
    rx: Receiver<ApplyOutcome>,
    then: After,
}

/// Fenêtre ouverte.
pub struct InfoEditor {
    pub form: InfoForm,
    /// Livres de la vue à l'ouverture (Précédent / Suivant) ; vide pour plusieurs livres.
    order: Vec<String>,
    /// Onglet affiché (rang dans [`TABS`] ; `TABS.len()` : « Personnalisé »).
    tab: usize,
    target: ApplyTarget,
    saving: Option<Saving>,
    /// Bilan ou erreur (texte, erreur ?).
    message: Option<(String, bool)>,
    /// Valeurs existantes dans la bibliothèque, par champ (calculées à la demande).
    suggestions: HashMap<&'static str, Arc<Vec<String>>>,
    new_custom: String,
    /// Script `Editor` en cours (identifiant du point d'entrée).
    script: Option<String>,
}

impl InfoEditor {
    fn new(books: &[&LibraryBook], order: Vec<String>, target: ApplyTarget) -> InfoEditor {
        InfoEditor {
            form: InfoForm::new(books),
            order,
            tab: 0,
            target,
            saving: None,
            message: None,
            suggestions: HashMap::new(),
            new_custom: String::new(),
            script: None,
        }
    }

    pub fn dirty(&self) -> bool {
        self.form.dirty()
    }

    fn read_only(&self) -> Option<&str> {
        self.target.read_only()
    }

    /// Livre voisin dans la vue (`step` : -1 précédent, +1 suivant).
    fn neighbour(&self, step: isize) -> Option<String> {
        if self.form.multiple() {
            return None;
        }
        let pos = self.order.iter().position(|i| *i == self.form.ids[0])?;
        let k = pos.checked_add_signed(step)?;
        self.order.get(k).cloned()
    }

    fn suggestions_for(&mut self, db: &ComicDatabase, def: &'static FieldDef) -> Arc<Vec<String>> {
        self.suggestions.entry(def.name).or_insert_with(|| Arc::new(info_form::suggestions(db, def))).clone()
    }
}

/// Action demandée dans la fenêtre.
enum Action {
    Ok,
    Apply,
    Cancel,
    Go(String),
    Script(String),
}

/// Choix des listes Oui / Non / Inconnu : (valeur du champ, libellé).
fn yes_no_choices(kind: FieldKind) -> &'static [(&'static str, &'static str)] {
    match kind {
        FieldKind::Manga => {
            &[("", "Inconnu"), ("No", "Non"), ("Yes", "Oui"), ("YesAndRightToLeft", "Oui, de droite à gauche")]
        }
        FieldKind::Bool { .. } => &[("Oui", "Oui"), ("Non", "Non")],
        _ => &[("", "Inconnu"), ("No", "Non"), ("Yes", "Oui")],
    }
}

const MIXED: &str = "(valeurs multiples)";

/// Champ texte avec suggestions : la valeur en cours de saisie (dernière valeur d'une
/// liste) est complétée par les valeurs existantes (↑/↓ puis Entrée, ou clic) ; « ▾ »
/// montre toutes les valeurs. Rend `true` si le texte a changé.
fn suggest_edit(
    ui: &mut Ui,
    id: Id,
    text: &mut String,
    list: bool,
    sugg: Option<&[String]>,
    hint: &str,
    width: f32,
) -> bool {
    let all_id = id.with("tout");
    let sel_id = id.with("choix");
    let mut show_all: bool = ui.data(|d| d.get_temp(all_id)).unwrap_or(false);
    let mut sel: usize = ui.data(|d| d.get_temp(sel_id)).unwrap_or(0);
    let focused = ui.memory(|m| m.has_focus(id));
    let token = info_form::current_token(text, list).to_lowercase();
    let items: Vec<&str> = match sugg {
        Some(s) if show_all => s.iter().map(String::as_str).filter(|v| v.to_lowercase().contains(&token)).collect(),
        Some(s) if focused => info_form::matching(&token, s, 8),
        _ => Vec::new(),
    };
    let mut pick: Option<usize> = None;
    if !items.is_empty() && focused {
        sel = sel.min(items.len() - 1);
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                sel = (sel + 1).min(items.len() - 1);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                sel = sel.saturating_sub(1);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                pick = Some(sel);
            }
            if show_all && i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                show_all = false;
            }
        });
    }
    let r = egui::TextEdit::singleline(text).id(id).hint_text(hint).desired_width(width).show(ui).response;
    let mut changed = r.changed();
    if changed {
        sel = 0;
    }
    let toggled =
        sugg.is_some() && ui.small_button("▾").on_hover_text("Valeurs déjà présentes dans la bibliothèque").clicked();
    if toggled {
        show_all = !show_all;
        r.request_focus();
        ui.ctx().request_repaint(); // la liste s'affiche à l'image suivante
    }
    let mut popup_hovered = false;
    if !items.is_empty() && (focused || show_all) {
        let area = egui::Area::new(id.with("suggestions"))
            .order(egui::Order::Foreground)
            .fixed_pos(r.rect.left_bottom() + vec2(0.0, 2.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(r.rect.width());
                    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
                    egui::ScrollArea::vertical().max_height(220.0).show_rows(ui, row_h, items.len(), |ui, range| {
                        for k in range {
                            let l = ui.selectable_label(k == sel, items[k]);
                            if k == sel && !show_all {
                                l.scroll_to_me(None);
                            }
                            if l.clicked() {
                                pick = Some(k);
                            }
                        }
                    });
                });
            });
        popup_hovered = area.response.contains_pointer();
    }
    if let Some(k) = pick {
        *text = info_form::accept(text, list, items[k]);
        changed = true;
        show_all = false;
        sel = 0;
        r.request_focus();
        // Curseur en fin de texte.
        if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
            let n = text.chars().count();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(n))));
            st.store(ui.ctx(), id);
        }
    }
    // Clic ailleurs : la liste se ferme (pas le clic qui vient de l'ouvrir).
    if show_all && !toggled && !focused && !popup_hovered && ui.input(|i| i.pointer.any_pressed()) {
        show_all = false;
    }
    ui.data_mut(|d| {
        d.insert_temp(all_id, show_all);
        d.insert_temp(sel_id, sel);
    });
    changed
}

impl ComicRustApp {
    /// Ouvre la fenêtre d'infos sur `ids` (un livre : Précédent / Suivant dans la vue).
    pub(crate) fn open_info_editor(&mut self, ids: Vec<String>) {
        let Some(db) = self.library.database() else { return };
        if let Some(ed) = &mut self.info_editor {
            if ed.dirty() || ed.saving.is_some() || ed.script.is_some() {
                ed.message = Some((
                    "Enregistre (OK, Appliquer) ou annule les modifications en cours avant d'ouvrir d'autres livres."
                        .into(),
                    true,
                ));
                return;
            }
        }
        let books: Vec<&LibraryBook> = ids.iter().filter_map(|i| db.get(i)).collect();
        if books.is_empty() {
            return;
        }
        let order = if books.len() == 1 { self.library.view_ids() } else { Vec::new() };
        let target = self.script_target();
        self.info_editor = Some(InfoEditor::new(&books, order, target));
    }

    /// Livres concernés par Ctrl+I : la sélection, ou les livres de la pile sélectionnée.
    pub(crate) fn info_editor_books(&mut self) -> Vec<String> {
        let piles = self.settings.library_stack && !self.settings.library_table && self.library.open_stack.is_none();
        if piles {
            let Some(series) = self.library.selected_stack.clone() else { return Vec::new() };
            let opts = self.settings.view_options();
            let Some(db) = self.library.database() else { return Vec::new() };
            return self
                .library
                .stacks(&opts)
                .into_iter()
                .find(|s| s.series == series)
                .map(|s| s.books.iter().map(|&i| db.books[i].id.clone()).collect())
                .unwrap_or_default();
        }
        self.library.selection()
    }

    /// Reporte un enregistrement dans la bibliothèque affichée (comme les scripts), puis
    /// demande l'écriture des infos dans les fichiers.
    fn apply_edit_outcome(&mut self, outcome: &ApplyOutcome) {
        let fresh = outcome.fresh_books();
        if let Some(c) = &mut self.covers {
            for b in &fresh {
                c.forget_failure(&b.id, b.front_cover_image());
            }
        }
        if self.library.standalone().is_some() {
            self.library.update_books(fresh, outcome.written());
        } else {
            for w in outcome.edit.iter().filter_map(|r| r.comicrack.as_ref()) {
                self.library.apply_written(w);
            }
        }
        if !outcome.write_info.is_empty() {
            // Écrit par le travail « Fichiers » dès qu'aucun autre ne tourne (`poll_scripts`).
            self.scripts.pending_write_info.extend(outcome.write_info.iter().map(|id| (id.clone(), true)));
        }
    }

    /// Lance l'enregistrement (fil d'arrière-plan).
    fn start_info_save(&mut self, ed: &mut InfoEditor, then: After, ctx: &Context) {
        let edits = ed.form.edits();
        let write_info: Vec<String> = if self.settings.info_write_file && !edits.is_empty() {
            let db = self.library.database();
            ed.form
                .ids
                .iter()
                .filter(|id| {
                    db.as_ref()
                        .and_then(|d| d.get(id))
                        .and_then(|b| b.file.as_deref())
                        .is_some_and(|f| FileAction::WriteInfo.accepts(&self.settings.effective_path(Path::new(f))))
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let plan = ApplyPlan { edits, renames: Vec::new(), write_info };
        if plan.is_empty() {
            self.info_after(ed, then);
            return;
        }
        let target = ed.target.clone();
        let sink = self.library.watcher.as_ref().map(|w| w.written.clone());
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("comicrust-infos-livre".into()).spawn(move || {
            let outcome = apply_plan(&target, &plan, &AtomicBool::new(false));
            if let Some(sink) = &sink {
                sink.lock().unwrap().extend(outcome.written());
            }
            let _ = tx.send(outcome);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => {
                ed.saving = Some(Saving { rx, then });
                ed.message = None;
            }
            Err(e) => ed.message = Some((format!("Enregistrement impossible : {e}"), true)),
        }
    }

    /// Après un enregistrement réussi (ou sans rien à écrire).
    fn info_after(&mut self, ed: &mut InfoEditor, then: After) -> bool {
        let Some(db) = self.library.database() else { return false };
        match then {
            After::Close => return false,
            After::Stay => {
                let books: Vec<&LibraryBook> = ed.form.ids.iter().filter_map(|i| db.get(i)).collect();
                if books.is_empty() {
                    return false;
                }
                ed.form = InfoForm::new(&books);
            }
            After::Go(id) => {
                let Some(b) = db.get(&id) else { return false };
                ed.form = InfoForm::new(&[b]);
                ed.message = None;
                // La sélection de la vue suit, comme dans ComicRack.
                self.library.click_book(&id, false, false);
                self.library.nav.scroll_to_cursor = true;
            }
        }
        true
    }

    /// Fin de l'enregistrement en cours.
    fn poll_info_save(&mut self, ed: &mut InfoEditor) -> bool {
        let Some(s) = &ed.saving else { return true };
        let outcome = match s.rx.try_recv() {
            Ok(o) => o,
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => {
                ed.saving = None;
                ed.message = Some(("Enregistrement interrompu.".into(), true));
                return true;
            }
        };
        let then = ed.saving.take().map(|s| s.then).unwrap_or(After::Stay);
        self.apply_edit_outcome(&outcome);
        if outcome.is_complete() {
            let summary = outcome.summary();
            let keep = self.info_after(ed, then.clone());
            if keep && then == After::Stay {
                ed.message = Some((format!("Enregistré : {summary}."), false));
            }
            keep
        } else {
            let db = self.library.database();
            let caption =
                |id: &str| db.as_ref().and_then(|d| d.get(id)).map(|b| b.caption()).unwrap_or_else(|| id.to_string());
            let problems = outcome.problems(caption).join("\n");
            ed.message = Some((format!("Pas tout enregistré ({}) :\n{problems}", outcome.summary()), true));
            true
        }
    }

    /// Script `Editor` terminé : ses modifications remplissent la fenêtre (sans aperçu ni
    /// écriture).
    fn poll_info_script(&mut self, ed: &mut InfoEditor) {
        let Some(sid) = ed.script.clone() else { return };
        let result = match &self.scripts.run {
            Some(run) if run.hook == Hook::Editor && run.entry.id() == sid => match &run.result {
                None => return,
                Some(r) => r.clone(),
            },
            _ => {
                ed.script = None;
                return;
            }
        };
        ed.script = None;
        if result.state != RunState::Finished {
            ed.message =
                Some(("Script arrêté ou en erreur : rien n'a été repris (voir la fenêtre du script).".into(), true));
            return;
        }
        // Pas d'aperçu : la fenêtre du script se ferme, les valeurs vont dans les champs.
        self.scripts.run = None;
        let Some(db) = self.library.database() else { return };
        let Some(book) = db.get(&ed.form.ids[0]) else { return };
        let mut filled: Vec<String> = Vec::new();
        let mut problems: Vec<String> = Vec::new();
        for c in &result.changes {
            match c {
                ScriptChange::SetField { book_id, field, value } if book_id.eq_ignore_ascii_case(&book.id) => {
                    match ed.form.apply_script_value(book, field, value) {
                        Ok(name) => {
                            if !filled.contains(&name) {
                                filled.push(name);
                            }
                        }
                        Err(e) => problems.push(e),
                    }
                }
                ScriptChange::WriteInfo { .. } => self.settings.info_write_file = true,
                ScriptChange::Rename { .. } => problems.push("renommage du fichier ignoré dans cette fenêtre".into()),
                ScriptChange::SetField { .. } => problems.push("modification d'un autre livre ignorée".into()),
            }
        }
        if !result.rejected.is_empty() {
            problems.push(format!("{} modification(s) refusée(s) par ComicRust", result.rejected.len()));
        }
        let mut m = match filled.len() {
            0 => "Script terminé : aucun champ rempli.".to_string(),
            n => format!(
                "Script terminé : {n} champ(s) rempli(s) ({}). Vérifie, puis OK ou Appliquer pour enregistrer.",
                filled.iter().map(|f| crate::scripts::apply::field_label(f)).collect::<Vec<_>>().join(", ")
            ),
        };
        if !problems.is_empty() {
            m.push_str(&format!("\n{}", problems.join("\n")));
        }
        ed.message = Some((m, !problems.is_empty()));
    }

    /// Fenêtre « Infos du livre » (chaque image).
    pub(crate) fn info_editor_window(&mut self, ctx: &Context) {
        let Some(mut ed) = self.info_editor.take() else { return };
        let keep = self.info_editor_frame(ctx, &mut ed);
        if keep && self.info_editor.is_none() {
            self.info_editor = Some(ed);
        }
    }

    fn info_editor_frame(&mut self, ctx: &Context, ed: &mut InfoEditor) -> bool {
        if !self.poll_info_save(ed) {
            return false;
        }
        self.poll_info_script(ed);
        let Some(db) = self.library.database() else { return false };
        let books: Vec<LibraryBook> = ed.form.ids.iter().filter_map(|i| db.get(i)).cloned().collect();
        if books.is_empty() {
            return false;
        }
        self.ensure_covers(ctx);
        let scripts: Vec<(String, String, Option<String>)> = self
            .scripts
            .enabled(Hook::Editor, &self.script_choices())
            .map(|e| (e.id(), e.name.clone(), e.description.clone()))
            .collect();
        let busy = ed.saving.is_some() || ed.script.is_some();
        let editable = ed.read_only().is_none() && !busy;
        let multiple = ed.form.multiple();
        let title = if multiple {
            format!("Infos de {} livres", books.len())
        } else {
            format!("Infos — {}", books[0].caption())
        };
        let mut action: Option<Action> = None;
        let mut open = true;
        // Échap : fermer, si rien n'est modifié (sinon Annuler, explicitement).
        if !ed.dirty()
            && !busy
            && ctx.memory(|m| m.focused().is_none())
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            action = Some(Action::Cancel);
        }
        egui::Window::new(title)
            .id(Id::unique("comicrust-infos-livre"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([740.0, 780.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .min_width(520.0)
            .show(ctx, |ui| {
                self.info_header(ui, ed, &books);
                if let Some(why) = ed.read_only() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label(
                            RichText::new("🔒 Consultation seulement : cette bibliothèque est en lecture seule.")
                                .strong()
                                .color(crate::ui::theme::warning(ui)),
                        );
                        ui.label(why);
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    for (k, (name, _)) in TABS.iter().enumerate() {
                        ui.selectable_value(&mut ed.tab, k, *name);
                    }
                    let n = ed.form.custom_fields().count();
                    let custom = if n > 0 { format!("Personnalisé ({n})") } else { "Personnalisé".into() };
                    ui.selectable_value(&mut ed.tab, TABS.len(), custom);
                });
                ui.separator();
                let footer = 92.0 + if ed.message.is_some() { 40.0 } else { 0.0 };
                egui::ScrollArea::vertical()
                    .id_salt(("infos-onglet", ed.tab, &ed.form.ids[0]))
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - footer).max(120.0))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(editable, |ui| {
                            if ed.tab < TABS.len() {
                                info_fields(ui, ed, &db, TABS[ed.tab].1);
                            } else {
                                custom_values(ui, ed);
                            }
                        });
                    });
                ui.separator();
                if let Some((m, err)) = &ed.message {
                    let color = if *err { crate::ui::theme::error(ui) } else { crate::ui::theme::ok(ui) };
                    egui::ScrollArea::vertical().id_salt("infos-message").max_height(60.0).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(m).color(color)).wrap());
                    });
                }
                self.info_footer(ui, ed, &books, &scripts, editable, &mut action);
            });
        if !open {
            action = Some(Action::Cancel);
        }
        match action {
            None => true,
            Some(Action::Cancel) => {
                // Pendant l'enregistrement, la fenêtre reste : le bilan doit arriver.
                busy && ed.saving.is_some()
            }
            Some(Action::Ok) => {
                if ed.dirty() {
                    self.start_info_save(ed, After::Close, ctx);
                    true
                } else {
                    false
                }
            }
            Some(Action::Apply) => {
                self.start_info_save(ed, After::Stay, ctx);
                true
            }
            Some(Action::Go(id)) => {
                if ed.dirty() {
                    self.start_info_save(ed, After::Go(id), ctx);
                    true
                } else {
                    self.info_after(ed, After::Go(id))
                }
            }
            Some(Action::Script(id)) => {
                let book = ed.form.ids[0].clone();
                self.run_script(ctx, &id, Hook::Editor, vec![book]);
                if self.scripts.run.as_ref().is_some_and(|r| r.hook == Hook::Editor && r.entry.id() == id) {
                    ed.script = Some(id);
                    ed.message = Some(("Script en cours… (ses réponses rempliront les champs)".into(), false));
                }
                true
            }
        }
    }

    /// En-tête : couverture(s), nom, fichier.
    fn info_header(&mut self, ui: &mut Ui, ed: &InfoEditor, books: &[LibraryBook]) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(96.0, 140.0), Sense::hover());
            if let Some(covers) = self.covers.as_mut() {
                // Plusieurs livres : jusqu'à trois couvertures décalées (la première devant).
                let shown = books.len().min(3);
                let step = 12.0;
                let size = rect.size() - vec2(step, step) * (shown - 1) as f32;
                for (k, b) in books.iter().take(shown).enumerate().rev() {
                    let off = vec2(step * k as f32, step * (shown - 1 - k) as f32);
                    let r = egui::Rect::from_min_size(rect.min + off, size);
                    if shown > 1 {
                        ui.painter().rect_filled(r, 2.0, crate::ui::theme::placeholder(ui));
                    }
                    paint_cover(ui, covers, b, r);
                }
            }
            ui.vertical(|ui| {
                if ed.form.multiple() {
                    ui.heading(format!("{} livres", books.len()));
                    let names: Vec<String> = books.iter().take(4).map(|b| b.caption()).collect();
                    let more = if books.len() > 4 { format!(" et {} autres", books.len() - 4) } else { String::new() };
                    ui.label(RichText::new(format!("{}{more}", names.join(" · "))).weak());
                    ui.label(
                        RichText::new(
                            "Seuls les champs modifiés sont écrits ; « (valeurs multiples) » : les livres diffèrent.",
                        )
                        .small()
                        .weak(),
                    );
                } else {
                    let b = &books[0];
                    ui.heading(b.caption());
                    if let Some(t) = b.title.as_deref().filter(|t| !t.is_empty()) {
                        ui.label(RichText::new(t).italics());
                    }
                    let mut facts = Vec::new();
                    if let Some(p) = b.page_count.filter(|p| *p > 0) {
                        facts.push(format!("{p} pages"));
                    }
                    if let Some(s) = b.file_size.filter(|s| *s > 0) {
                        facts.push(crate::ui::library_table::human_size(s));
                    }
                    if let Some(p) = b.read_percent().filter(|p| *p > 0) {
                        facts.push(format!("lu à {p} %"));
                    }
                    if b.comic_info_is_dirty == Some(true) {
                        facts.push("infos pas encore écrites dans le fichier".into());
                    }
                    if !facts.is_empty() {
                        ui.label(RichText::new(facts.join(" · ")).weak());
                    }
                    if let Some(f) = &b.file {
                        ui.add(egui::Label::new(RichText::new(f).small().weak()).wrap());
                    }
                }
            });
        });
    }

    /// Boutons du bas.
    fn info_footer(
        &mut self,
        ui: &mut Ui,
        ed: &InfoEditor,
        books: &[LibraryBook],
        scripts: &[(String, String, Option<String>)],
        editable: bool,
        action: &mut Option<Action>,
    ) {
        let errors = ed.form.errors();
        let dirty = ed.dirty();
        ui.horizontal(|ui| {
            if !ed.form.multiple() && ed.order.len() > 1 {
                let prev = ed.neighbour(-1);
                let next = ed.neighbour(1);
                let can = errors.is_empty() && ed.saving.is_none() && ed.script.is_none();
                let hint = if dirty { " (les modifications sont enregistrées)" } else { "" };
                if ui
                    .add_enabled(can && prev.is_some(), egui::Button::new("◀ Précédent"))
                    .on_hover_text(format!("Livre précédent de la vue{hint}"))
                    .clicked()
                {
                    *action = prev.map(Action::Go);
                }
                if ui
                    .add_enabled(can && next.is_some(), egui::Button::new("Suivant ▶"))
                    .on_hover_text(format!("Livre suivant de la vue{hint}"))
                    .clicked()
                {
                    *action = next.map(Action::Go);
                }
                ui.separator();
            }
            // Scripts `Editor` (Bedetheque Scraper…).
            let why = if scripts.is_empty() {
                Some("Aucun script « Editor » installé (en-tête #@Hook Editor)".to_string())
            } else if ed.form.multiple() {
                Some("Les scripts de cette fenêtre portent sur un seul livre".to_string())
            } else if !editable {
                Some("Indisponible (lecture seule ou opération en cours)".to_string())
            } else {
                None
            };
            ui.add_enabled_ui(why.is_none(), |ui| {
                let r = ui.menu_button("📜 Scripts", |ui| {
                    for (id, name, desc) in scripts {
                        let b = ui.button(name);
                        let b = match desc {
                            Some(d) => b.on_hover_text(d),
                            None => b,
                        };
                        if b.clicked() {
                            *action = Some(Action::Script(id.clone()));
                            ui.close();
                        }
                    }
                });
                if let Some(w) = &why {
                    r.response.on_disabled_hover_text(w);
                } else {
                    r.response.on_hover_text("Lancer un script sur ce livre : il remplit les champs (rien n'est enregistré avant OK)");
                }
            });
            let any_cbz = books.iter().any(|b| {
                b.file.as_deref().is_some_and(|f| FileAction::WriteInfo.accepts(&self.settings.effective_path(Path::new(f))))
            });
            ui.add_enabled(editable && any_cbz, egui::Checkbox::new(&mut self.settings.info_write_file, "Écrire aussi les infos dans le fichier"))
                .on_hover_text("À l'enregistrement, écrire aussi le ComicInfo.xml dans le fichier (CBZ seulement), comme « Écrire les infos dans le fichier »")
                .on_disabled_hover_text("Seuls les CBZ reçoivent les infos (convertir d'abord en CBZ)");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ed.saving.is_some() {
                ui.spinner();
                ui.label("Enregistrement…");
            } else if ed.script.is_some() {
                ui.spinner();
                ui.label("Script en cours…");
            } else if dirty {
                let n = ed.form.fields.iter().filter(|f| f.touched).count();
                ui.label(RichText::new(format!("● {n} champ(s) modifié(s)")).color(ACCENT));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_save = editable && dirty && errors.is_empty();
                let blocked = if !errors.is_empty() {
                    format!(
                        "Valeur(s) refusée(s) : {}",
                        errors
                            .iter()
                            .map(|(n, _)| crate::scripts::apply::field_label(n))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                } else if ed.read_only().is_some() {
                    "Lecture seule".to_string()
                } else {
                    "Rien à enregistrer".to_string()
                };
                if ui.add_enabled(can_save, egui::Button::new("Appliquer")).on_disabled_hover_text(&blocked).clicked() {
                    *action = Some(Action::Apply);
                }
                if ui.add_enabled(ed.saving.is_none(), egui::Button::new("Annuler")).clicked() {
                    *action = Some(Action::Cancel);
                }
                let ok_enabled = ed.saving.is_none() && ed.script.is_none() && (!dirty || can_save);
                if ui
                    .add_enabled(ok_enabled, egui::Button::new(RichText::new("   OK   ").strong()))
                    .on_disabled_hover_text(&blocked)
                    .clicked()
                {
                    *action = Some(Action::Ok);
                }
            });
        });
    }
}

/// Libellé d'un champ (● si modifié).
fn field_label(ui: &mut Ui, label: &str, touched: bool) {
    if touched {
        ui.label(RichText::new(format!("● {label}")).color(ACCENT)).on_hover_text("Modifié : sera enregistré");
    } else {
        ui.label(label);
    }
}

/// Champs d'un onglet.
fn info_fields(ui: &mut Ui, ed: &mut InfoEditor, db: &Arc<ComicDatabase>, names: &[&str]) {
    egui::Grid::new(("infos-champs", ed.tab)).num_columns(2).spacing(vec2(12.0, 6.0)).min_col_width(140.0).show(
        ui,
        |ui| {
            for name in names {
                let Some(def) = edit::field(name) else { continue };
                let Some(i) = ed.form.fields.iter().position(|f| f.name == *name) else { continue };
                field_label(ui, def.label, ed.form.fields[i].touched);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| field_control(ui, ed, db, i, def));
                    if let Some(e) = ed.form.fields[i].error() {
                        ui.label(RichText::new(format!("⚠ {e}")).small().color(crate::ui::theme::error(ui)));
                    }
                });
                ui.end_row();
            }
        },
    );
}

/// Contrôle d'un champ, selon son type, avec ↺ (revenir) et ✖ (vider pour tous).
fn field_control(ui: &mut Ui, ed: &mut InfoEditor, db: &Arc<ComicDatabase>, i: usize, def: &'static FieldDef) {
    let id = Id::unique(("infos-champ", def.name));
    let mixed_hint = if ed.form.fields[i].mixed() { MIXED } else { "" };
    let mut changed = false;
    match def.kind {
        FieldKind::LongText => {
            let f = &mut ed.form.fields[i];
            changed |= ui
                .add(
                    egui::TextEdit::multiline(&mut f.text)
                        .id(id)
                        .hint_text(mixed_hint)
                        .desired_rows(5)
                        .desired_width(ui.available_width() - 40.0),
                )
                .changed();
        }
        FieldKind::YesNo | FieldKind::Manga | FieldKind::Bool { .. } => {
            let f = &mut ed.form.fields[i];
            let choices = yes_no_choices(def.kind);
            let current = if f.mixed() && !f.touched {
                MIXED.to_string()
            } else {
                choices.iter().find(|(v, _)| *v == f.text).map(|(_, l)| l.to_string()).unwrap_or_else(|| f.text.clone())
            };
            egui::ComboBox::from_id_salt(id).selected_text(current).width(220.0).show_ui(ui, |ui| {
                for (v, l) in choices {
                    let on = f.text == *v && (f.touched || !f.mixed());
                    if ui.selectable_label(on, *l).clicked() {
                        f.text = v.to_string();
                        if v.is_empty() && f.mixed() {
                            f.clear_all = true;
                        }
                        changed = true;
                    }
                }
            });
        }
        FieldKind::Rating => {
            let f = &mut ed.form.fields[i];
            let value: f32 = f.text.replace(',', ".").trim().parse().unwrap_or(0.0);
            for s in 1..=5 {
                let full = value >= s as f32 - 0.25 && !(f.mixed() && !f.touched);
                let star = RichText::new(if full { "★" } else { "☆" }).size(18.0).color(ACCENT);
                if ui.add(egui::Button::new(star).frame(false)).on_hover_text(format!("{s} / 5")).clicked() {
                    f.text = s.to_string();
                    changed = true;
                }
            }
            changed |= ui
                .add(egui::TextEdit::singleline(&mut f.text).id(id).hint_text(mixed_hint).desired_width(60.0))
                .on_hover_text("Note de 0 à 5 (décimales permises : 3,5)")
                .changed();
        }
        FieldKind::Int { .. } | FieldKind::Price | FieldKind::Date => {
            let f = &mut ed.form.fields[i];
            let (w, hint) = match def.kind {
                FieldKind::Date => {
                    (if f.mixed() { 140.0 } else { 110.0 }, if f.mixed() { MIXED } else { "JJ/MM/AAAA" })
                }
                _ if f.mixed() => (140.0, mixed_hint),
                FieldKind::Price => (90.0, mixed_hint),
                _ => (80.0, mixed_hint),
            };
            changed |=
                ui.add(egui::TextEdit::singleline(&mut f.text).id(id).hint_text(hint).desired_width(w)).changed();
        }
        _ => {
            let list = def.kind == FieldKind::List;
            let sugg = if info_form::has_suggestions(def) {
                // Calculées quand le champ est utilisé (ou la liste ▾ ouverte).
                let wanted = ui.memory(|m| m.has_focus(id))
                    || ui.data(|d| d.get_temp::<bool>(id.with("tout"))).unwrap_or(false)
                    || ed.suggestions.contains_key(def.name);
                wanted.then(|| ed.suggestions_for(db, def))
            } else {
                None
            };
            let f = &mut ed.form.fields[i];
            let hint = if f.mixed() {
                MIXED
            } else if list {
                "valeurs séparées par des virgules"
            } else {
                ""
            };
            let empty: [String; 0] = [];
            let s: Option<&[String]> = match &sugg {
                Some(v) => Some(v.as_slice()),
                None if info_form::has_suggestions(def) => Some(&empty),
                None => None,
            };
            let w = (ui.available_width() - 70.0).clamp(120.0, 420.0);
            changed |= suggest_edit(ui, id, &mut f.text, list, s, hint, w);
        }
    }
    let f = &mut ed.form.fields[i];
    if changed {
        f.update();
    }
    if f.touched {
        if ui.small_button("↺").on_hover_text("Revenir à la valeur enregistrée").clicked() {
            f.revert();
        }
    } else if f.mixed() && ui.small_button("✖").on_hover_text("Vider ce champ pour tous les livres").clicked() {
        f.clear();
    }
}

/// Onglet « Personnalisé » : valeurs personnalisées (`CustomValue:<clé>`).
fn custom_values(ui: &mut Ui, ed: &mut InfoEditor) {
    ui.label(
        RichText::new("Valeurs personnalisées (« Personnalisé » de ComicRack), utilisées par les scripts et les listes intelligentes.")
            .small()
            .weak(),
    );
    ui.add_space(4.0);
    let idx: Vec<usize> =
        ed.form.fields.iter().enumerate().filter(|(_, f)| f.custom_key().is_some()).map(|(i, _)| i).collect();
    if idx.is_empty() {
        ui.label(RichText::new("Aucune valeur personnalisée.").weak());
    }
    egui::Grid::new("infos-perso").num_columns(2).spacing(vec2(12.0, 6.0)).min_col_width(140.0).show(ui, |ui| {
        for i in idx {
            let f = &mut ed.form.fields[i];
            let key = f.custom_key().unwrap_or_default().to_string();
            field_label(ui, &key, f.touched);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let hint = if f.mixed() { MIXED } else { "" };
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut f.text)
                            .id(Id::unique(("infos-perso", &key)))
                            .hint_text(hint)
                            .desired_width(320.0),
                    );
                    if r.changed() {
                        f.update();
                    }
                    if f.touched {
                        if ui.small_button("↺").on_hover_text("Revenir à la valeur enregistrée").clicked() {
                            f.revert();
                        }
                    } else if !(f.text.is_empty() && !f.mixed())
                        && ui.small_button("✖").on_hover_text("Supprimer cette valeur").clicked()
                    {
                        f.clear();
                    }
                });
                if let Some(e) = f.error() {
                    ui.label(RichText::new(format!("⚠ {e}")).small().color(crate::ui::theme::error(ui)));
                }
            });
            ui.end_row();
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut ed.new_custom).hint_text("Nom de la nouvelle valeur").desired_width(220.0),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("＋ Ajouter").clicked() || enter) && !ed.new_custom.trim().is_empty() {
            match ed.form.add_custom(&ed.new_custom) {
                Ok(()) => {
                    let key = ed.new_custom.trim().to_string();
                    ed.new_custom.clear();
                    ui.memory_mut(|m| m.request_focus(Id::unique(("infos-perso", key))));
                    ed.message = None;
                }
                Err(e) => ed.message = Some((format!("Valeur personnalisée : {e}"), true)),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_hook_is_known() {
        assert_eq!(Hook::parse("editor"), Some(Hook::Editor));
        assert!(!Hook::Editor.is_automatic(), "lancé par l'utilisateur, interactif");
        assert!(Hook::ALL.contains(&Hook::Editor));
    }

    /// Enregistrement comme la fenêtre : modifications du formulaire, plan, `apply_plan`
    /// sur la base ComicRust ; fiches relues ; lecture seule refusée.
    #[test]
    fn saving_the_form_writes_the_store() {
        use comicrust_core::store::{Store, StoreConfig};
        let dir = tempfile::tempdir().unwrap();
        let cfg = StoreConfig::Sqlite(dir.path().join("comicrust.sqlite"));
        let mk = |id: &str, n: &str| LibraryBook {
            id: id.into(),
            series: Some("Thanos".into()),
            number: Some(n.into()),
            file: Some(format!("{}/{id}.cbz", dir.path().display())),
            ..Default::default()
        };
        let (a, b) = (mk("a", "1"), mk("b", "2"));
        Store::open(&cfg).unwrap().upsert(&[a.clone(), b.clone()]).unwrap();
        let mut form = InfoForm::new(&[&a, &b]);
        form.set("Series", "Thanos Quest");
        form.set("Publisher", "Marvel");
        form.set("Number", ""); // valeurs multiples effacées : non écrit
        let plan = ApplyPlan { edits: form.edits(), renames: Vec::new(), write_info: Vec::new() };
        let out = apply_plan(&ApplyTarget::Store(cfg.clone()), &plan, &AtomicBool::new(false));
        assert!(out.is_complete(), "{:?}", out.problems(|i| i.to_string()));
        let mut fresh = out.fresh_books();
        fresh.sort_by(|x, y| x.id.cmp(&y.id));
        let got: Vec<(&str, &str, &str)> = fresh
            .iter()
            .map(|x| (x.series.as_deref().unwrap(), x.publisher.as_deref().unwrap(), x.number.as_deref().unwrap()))
            .collect();
        assert_eq!(got, [("Thanos Quest", "Marvel", "1"), ("Thanos Quest", "Marvel", "2")]);
        assert!(!out.written().is_empty(), "écritures signalées à la surveillance");
        let ro = apply_plan(&ApplyTarget::ReadOnly("lecture seule".into()), &plan, &AtomicBool::new(false));
        assert!(ro.fatal.is_some() && ro.fresh_books().is_empty());
    }

    #[test]
    fn neighbours_follow_the_view_order() {
        let a = LibraryBook { id: "b".into(), ..Default::default() };
        let order = vec!["a".to_string(), "b".into(), "c".into()];
        let ed = InfoEditor::new(&[&a], order, ApplyTarget::ReadOnly("x".into()));
        assert_eq!(ed.neighbour(-1).as_deref(), Some("a"));
        assert_eq!(ed.neighbour(1).as_deref(), Some("c"));
        assert_eq!(ed.read_only(), Some("x"));
        let ed = InfoEditor::new(&[&a], vec!["b".into()], ApplyTarget::ReadOnly("x".into()));
        assert_eq!((ed.neighbour(-1), ed.neighbour(1)), (None, None));
        let c = LibraryBook { id: "c".into(), ..Default::default() };
        let ed = InfoEditor::new(&[&a, &c], vec![], ApplyTarget::ReadOnly("x".into()));
        assert_eq!(ed.neighbour(1), None, "plusieurs livres : pas de Précédent / Suivant");
    }
}
