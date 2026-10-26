//! Dessin des formulaires (`comicrust.form`) et des listes de choix avec vignettes
//! (`comicrust.choose`) des scripts (v0.10.1), dans des fenêtres modales qui suivent le
//! thème clair/sombre. La logique (champs, validation, filtre, tri) est dans
//! `scripts/forms.rs`.

use std::collections::HashMap;
use std::time::Duration;

use comicrust_core::scripting::forms::{Control, ControlKind, LabelStyle};
use comicrust_core::scripting::FormAnswer;
use eframe::egui::{self, Context, Id, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use serde_json::{Map, Value};

use crate::scripts::forms::{choice_label, insert_at, list_add, move_item, Cell, ChooseState, Field, FormState};

/// Largeur de la colonne des libellés.
const LABEL_W: f32 = 170.0;

/// Ce que l'utilisateur a fait dans une fenêtre de script.
pub(crate) enum Action<T> {
    Nothing,
    /// Réponse à rendre au script (`None` = annulé).
    Reply(Option<T>),
    /// « Arrêter le script ».
    Stop,
}

fn sized(ctx: &Context, wanted: f32) -> (f32, f32) {
    let screen = ctx.content_rect();
    let w = wanted.clamp(320.0, (screen.width() - 48.0).max(320.0));
    let h = (screen.height() - 220.0).max(160.0);
    (w, h)
}

fn styled(text: &str, style: LabelStyle, ui: &Ui) -> RichText {
    let t = RichText::new(text);
    match style {
        LabelStyle::Normal => t,
        LabelStyle::Bold => t.strong(),
        LabelStyle::Small => t.small(),
        LabelStyle::Weak => t.weak(),
        LabelStyle::Heading => t.heading(),
        LabelStyle::Warning => t.color(crate::ui::theme::warning(ui)),
        LabelStyle::Error => t.color(crate::ui::theme::error(ui)),
    }
}

/// Libellé à gauche d'un champ (largeur fixe, infobulle d'aide).
fn label_cell(ui: &mut Ui, label: &str, help: &str) {
    let h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(egui::vec2(LABEL_W, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(LABEL_W);
        let r = ui.add(egui::Label::new(label).truncate());
        if !help.is_empty() {
            r.on_hover_text(help);
        }
    });
}

/// Erreur de validation sous un champ.
fn error_line(ui: &mut Ui, st: &FormState, key: &str) {
    if let Some(e) = st.errors.get(key) {
        ui.label(RichText::new(format!("⚠ {e}")).small().color(crate::ui::theme::error(ui)));
    }
}

fn hover(r: egui::Response, help: &str) -> egui::Response {
    if help.is_empty() {
        r
    } else {
        r.on_hover_text(help)
    }
}

/// Formulaire d'un script (fenêtre modale).
pub(crate) fn form_window(ctx: &Context, script: &str, st: &mut FormState) -> Action<FormAnswer> {
    let mut action = Action::Nothing;
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let (width, max_h) = sized(ctx, st.spec.width.unwrap_or(560.0));
    let mut clicked: Option<String> = None;
    let mut stop = false;
    egui::Modal::new(Id::unique("comicrust-script-formulaire")).show(ctx, |ui| {
        ui.set_width(width);
        ui.label(RichText::new(format!("📜 {script}")).weak().small());
        if !st.spec.title.is_empty() {
            ui.label(RichText::new(&st.spec.title).heading());
        }
        if !st.spec.message.is_empty() {
            ui.label(&st.spec.message);
        }
        ui.add_space(4.0);
        let values = st.values();
        // Erreurs affichées : mises à jour pendant la saisie (une erreur corrigée disparaît).
        if !st.errors.is_empty() {
            st.errors = st.spec.validate(&values).into_iter().map(|e| (e.key, e.message)).collect();
        }
        // Les contrôles sont sortis le temps du dessin (les champs restent modifiables).
        let controls = std::mem::take(&mut st.spec.controls);
        egui::ScrollArea::vertical()
            .id_salt("formulaire-script-defilement")
            .max_height(max_h)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                draw_controls(ui, &controls, st, &values, true, &mut clicked);
            });
        st.spec.controls = controls;
        if !st.errors.is_empty() {
            let mut keys: Vec<&String> = st.errors.keys().collect();
            keys.sort_by_key(|k| st.spec.control(k).map(|c| c.id));
            let names: Vec<String> = keys.iter().map(|k| st.label_of(k)).collect();
            ui.label(RichText::new(format!("⚠ À corriger : {}", names.join(", "))).color(crate::ui::theme::error(ui)));
        }
        ui.separator();
        ui.horizontal(|ui| {
            for (i, b) in st.spec.buttons.iter().enumerate() {
                let main = i == 0 && st.spec.cancel != Some(0);
                let text = if main { RichText::new(b).strong() } else { RichText::new(b) };
                if ui.button(text).clicked() {
                    clicked = Some(b.clone());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⏹ Arrêter le script").clicked() {
                    stop = true;
                }
            });
        });
    });
    if stop {
        return Action::Stop;
    }
    if let Some(b) = clicked {
        let cancel = st.spec.cancel.and_then(|i| st.spec.buttons.get(i)).is_some_and(|c| *c == b);
        if cancel {
            action = Action::Reply(None);
        } else if let Some(answer) = st.submit(&b) {
            action = Action::Reply(Some(answer));
        }
    } else if escape {
        action = Action::Reply(None);
    }
    action
}

fn draw_controls(
    ui: &mut Ui,
    controls: &[Control],
    st: &mut FormState,
    values: &Map<String, Value>,
    enabled: bool,
    clicked: &mut Option<String>,
) {
    for c in controls {
        let on = enabled && c.enabled_if.as_ref().is_none_or(|cond| cond.eval(values));
        ui.push_id(("script-controle", c.id), |ui| {
            ui.add_enabled_ui(on, |ui| draw_control(ui, c, st, values, on, clicked));
        });
    }
}

fn draw_control(
    ui: &mut Ui,
    c: &Control,
    st: &mut FormState,
    values: &Map<String, Value>,
    enabled: bool,
    clicked: &mut Option<String>,
) {
    let help = c.help.as_str();
    match &c.kind {
        ControlKind::Label { text, style } => {
            let t = styled(text, *style, ui);
            hover(ui.add(egui::Label::new(t).wrap()), help);
        }
        ControlKind::Separator => {
            ui.separator();
        }
        ControlKind::Image { caption, max_width, max_height, .. } => {
            if let Some(img) = st.images.remove(&c.id) {
                let tex = ui.ctx().load_texture(format!("script-image-{}", c.id), img, egui::TextureOptions::LINEAR);
                st.textures.insert(c.id, tex);
            }
            ui.vertical(|ui| {
                match st.textures.get(&c.id) {
                    Some(tex) => {
                        let r = ui.add(
                            egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
                                .max_size(egui::vec2(*max_width, *max_height)),
                        );
                        hover(r, help);
                    }
                    None => {
                        ui.label(RichText::new("(image illisible)").weak());
                    }
                }
                if !caption.is_empty() {
                    ui.label(RichText::new(caption).small().weak());
                }
            });
        }
        ControlKind::Group { title, controls } => {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !title.is_empty() {
                    hover(ui.label(RichText::new(title).strong()), help);
                }
                draw_controls(ui, controls, st, values, enabled, clicked);
            });
        }
        ControlKind::Columns { columns } => {
            ui.columns(columns.len(), |uis| {
                for (u, col) in uis.iter_mut().zip(columns) {
                    draw_controls(u, col, st, values, enabled, clicked);
                }
            });
        }
        ControlKind::Tabs { tabs } => {
            let sel = st.tabs.get(&c.id).copied().unwrap_or(0).min(tabs.len() - 1);
            ui.horizontal_wrapped(|ui| {
                for (i, (title, controls)) in tabs.iter().enumerate() {
                    // Onglet contenant une erreur : signalé.
                    let has_error = controls_keys(controls).iter().any(|k| st.errors.contains_key(k));
                    let text = if has_error { format!("⚠ {title}") } else { title.clone() };
                    if ui.selectable_label(sel == i, RichText::new(text).strong()).clicked() {
                        st.tabs.insert(c.id, i);
                    }
                }
            });
            ui.separator();
            draw_controls(ui, &tabs[sel].1, st, values, enabled, clicked);
        }
        ControlKind::Button { .. } => {
            if hover(ui.button(&c.label), help).clicked() {
                *clicked = Some(c.key.clone());
            }
        }
        _ => draw_field(ui, c, st),
    }
}

/// Clés des contrôles (imbriqués compris).
fn controls_keys(controls: &[Control]) -> Vec<String> {
    let mut out = Vec::new();
    for c in controls {
        if !c.key.is_empty() {
            out.push(c.key.clone());
        }
        out.extend(c.children().into_iter().flat_map(|ch| controls_keys(std::slice::from_ref(ch))));
    }
    out
}

/// Menu « Insérer » d'un champ texte : insère au curseur (à la fin si le champ n'a jamais
/// eu le focus).
fn inserts_menu(ui: &mut Ui, id: Id, text: &mut String, inserts: &[comicrust_core::scripting::forms::ChoiceItem]) {
    ui.menu_button("Insérer ▾", |ui| {
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for ins in inserts {
                let snippet = comicrust_core::scripting::forms::value_text(&ins.value);
                let r = ui.button(&ins.label);
                let r = if ins.label != snippet { r.on_hover_text(&snippet) } else { r };
                if r.clicked() {
                    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                    let at = state.cursor.char_range().map(|r| usize::from(r.primary.index));
                    let after = insert_at(text, at, &snippet);
                    let cursor = egui::text::CCursor::new(after);
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(cursor)));
                    state.store(ui.ctx(), id);
                    ui.ctx().memory_mut(|m| m.request_focus(id));
                    ui.close();
                }
            }
        });
    });
}

fn draw_field(ui: &mut Ui, c: &Control, st: &mut FormState) {
    let help = c.help.as_str();
    let key = c.key.clone();
    let Some(field) = st.fields.get_mut(&key) else { return };
    match (&c.kind, field) {
        (ControlKind::Text { placeholder, rows, secret, inserts, .. }, Field::Text(text)) => {
            let id = ui.scope_id().with("texte");
            let hint = if *secret && placeholder.is_empty() { "vide = garder la valeur actuelle" } else { placeholder };
            if *rows > 1 {
                ui.horizontal(|ui| {
                    hover(ui.label(&c.label), help);
                    if !inserts.is_empty() {
                        inserts_menu(ui, id, text, inserts);
                    }
                });
                let edit = egui::TextEdit::multiline(text)
                    .id(id)
                    .desired_rows(*rows as usize)
                    .desired_width(c.width.unwrap_or(f32::INFINITY))
                    .hint_text(hint);
                hover(ui.add(edit), help);
            } else {
                ui.horizontal(|ui| {
                    label_cell(ui, &c.label, help);
                    let extra = if inserts.is_empty() { 0.0 } else { 90.0 };
                    let w = c.width.unwrap_or((ui.available_width() - extra).max(80.0));
                    let mut edit = egui::TextEdit::singleline(text).id(id).desired_width(w).hint_text(hint);
                    if *secret {
                        edit = edit.password(true);
                    }
                    hover(ui.add(edit), help);
                    if !inserts.is_empty() {
                        inserts_menu(ui, id, text, inserts);
                    }
                });
            }
        }
        (ControlKind::Path { folder, save, filters, .. }, Field::Text(text)) => {
            ui.horizontal(|ui| {
                label_cell(ui, &c.label, help);
                let w = c.width.unwrap_or((ui.available_width() - 100.0).max(80.0));
                hover(ui.add(egui::TextEdit::singleline(text).desired_width(w)), help);
                if ui.button("📁 Parcourir…").clicked() {
                    let mut d = rfd::FileDialog::new();
                    if !c.label.is_empty() {
                        d = d.set_title(&c.label);
                    }
                    let current = std::path::Path::new(text.as_str());
                    let start = if current.is_dir() { Some(current) } else { current.parent().filter(|p| p.is_dir()) };
                    if let Some(dir) = start {
                        d = d.set_directory(dir);
                    }
                    for (name, exts) in filters {
                        d = d.add_filter(name, exts);
                    }
                    let picked = if *folder {
                        d.pick_folder()
                    } else if *save {
                        d.save_file()
                    } else {
                        d.pick_file()
                    };
                    if let Some(p) = picked {
                        *text = p.display().to_string();
                    }
                }
            });
        }
        (ControlKind::Checkbox { .. }, Field::Bool(b)) => {
            hover(ui.checkbox(b, &c.label), help);
        }
        (ControlKind::Number { min, max, step, decimals, .. }, Field::Number(n)) => {
            ui.horizontal(|ui| {
                label_cell(ui, &c.label, help);
                let mut dv = egui::DragValue::new(n)
                    .range(min.unwrap_or(-1e15)..=max.unwrap_or(1e15))
                    .speed(*step)
                    .max_decimals(*decimals as usize);
                if *decimals > 0 {
                    dv = dv.fixed_decimals(*decimals as usize);
                }
                hover(ui.add_sized([c.width.unwrap_or(90.0), ui.spacing().interact_size.y], dv), help);
                if let (Some(a), Some(b)) = (min, max) {
                    ui.label(RichText::new(format!("({a} à {b})")).small().weak());
                }
            });
        }
        (ControlKind::Choice { choices, editable: true, .. }, Field::Choice { text, .. }) => {
            ui.horizontal(|ui| {
                label_cell(ui, &c.label, help);
                let w = c.width.unwrap_or((ui.available_width() - 40.0).max(80.0));
                hover(ui.add(egui::TextEdit::singleline(text).desired_width(w)), help);
                ui.menu_button("▾", |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for ch in choices {
                            if ui.selectable_label(*text == ch.label, &ch.label).clicked() {
                                *text = ch.label.clone();
                                ui.close();
                            }
                        }
                    });
                });
            });
        }
        (ControlKind::Choice { choices, .. }, Field::Choice { index, .. }) => {
            ui.horizontal(|ui| {
                label_cell(ui, &c.label, help);
                let shown = match choice_label(choices, *index) {
                    "" => "—",
                    l => l,
                };
                let w = c.width.unwrap_or((ui.available_width() - 8.0).clamp(80.0, 360.0));
                let r = egui::ComboBox::from_id_salt("liste").selected_text(shown).width(w).height(320.0).show_ui(
                    ui,
                    |ui| {
                        for (i, ch) in choices.iter().enumerate() {
                            ui.selectable_value(index, Some(i), &ch.label);
                        }
                    },
                );
                hover(r.response, help);
            });
        }
        (ControlKind::Radio { choices, horizontal, .. }, Field::Choice { index, .. }) => {
            ui.horizontal(|ui| {
                label_cell(ui, &c.label, help);
                let mut radios = |ui: &mut Ui| {
                    for (i, ch) in choices.iter().enumerate() {
                        hover(ui.radio_value(index, Some(i), &ch.label), help);
                    }
                };
                if *horizontal {
                    ui.horizontal_wrapped(|ui| radios(ui));
                } else {
                    ui.vertical(|ui| radios(ui));
                }
            });
        }
        (ControlKind::MultiChoice { choices, height, .. }, Field::Multi(checked)) => {
            ui.horizontal(|ui| {
                hover(ui.label(&c.label), help);
                if ui.small_button("Tout").clicked() {
                    checked.iter_mut().for_each(|b| *b = true);
                }
                if ui.small_button("Rien").clicked() {
                    checked.iter_mut().for_each(|b| *b = false);
                }
                let n = checked.iter().filter(|b| **b).count();
                ui.label(RichText::new(format!("{n}/{}", checked.len())).small().weak());
            });
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().max_height(height.unwrap_or(160.0)).auto_shrink([false, true]).show(
                    ui,
                    |ui| {
                        for (ch, on) in choices.iter().zip(checked.iter_mut()) {
                            ui.checkbox(on, &ch.label);
                        }
                    },
                );
            });
        }
        (ControlKind::ListEdit { choices, unique, height, .. }, Field::List { items, input, selected }) => {
            hover(ui.label(&c.label), help);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().max_height(height.unwrap_or(120.0)).auto_shrink([false, true]).show(
                    ui,
                    |ui| {
                        if items.is_empty() {
                            ui.label(RichText::new("(liste vide)").weak().small());
                        }
                        for (i, item) in items.iter().enumerate() {
                            if ui.selectable_label(*selected == Some(i), item).clicked() {
                                *selected = Some(i);
                                *input = item.clone();
                            }
                        }
                    },
                );
            });
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 250.0).max(100.0);
                let r = ui.add(egui::TextEdit::singleline(input).desired_width(w).hint_text("élément à ajouter"));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if !choices.is_empty() {
                    ui.menu_button("▾", |ui| {
                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            for ch in choices {
                                if ui.button(ch).clicked() {
                                    *input = ch.clone();
                                    ui.close();
                                }
                            }
                        });
                    });
                }
                if ui.button("➕ Ajouter").clicked() || enter {
                    if list_add(items, input, *unique) {
                        *selected = Some(items.len() - 1);
                        input.clear();
                    }
                    if enter {
                        r.request_focus();
                    }
                }
                let sel = selected.filter(|i| *i < items.len());
                if ui.add_enabled(sel.is_some(), egui::Button::new("➖ Retirer")).clicked() {
                    if let Some(i) = sel {
                        items.remove(i);
                        *selected = (!items.is_empty()).then(|| i.min(items.len() - 1));
                    }
                }
                if ui.add_enabled(sel.is_some_and(|i| i > 0), egui::Button::new("⬆")).clicked() {
                    *selected = sel.map(|i| move_item(items, i, true));
                }
                if ui.add_enabled(sel.is_some_and(|i| i + 1 < items.len()), egui::Button::new("⬇")).clicked() {
                    *selected = sel.map(|i| move_item(items, i, false));
                }
            });
        }
        (ControlKind::Table { columns, height, .. }, Field::Table { rows, selected }) => {
            hover(ui.label(&c.label), help);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::both().max_height(height.unwrap_or(180.0)).auto_shrink([false, true]).show(
                    ui,
                    |ui| {
                        egui::Grid::new("tableau").striped(true).num_columns(columns.len() + 1).show(ui, |ui| {
                            ui.label("");
                            for col in columns {
                                ui.label(RichText::new(&col.title).strong());
                            }
                            ui.end_row();
                            for (i, row) in rows.iter_mut().enumerate() {
                                if ui
                                    .selectable_label(
                                        *selected == Some(i),
                                        if *selected == Some(i) { "●" } else { "○" },
                                    )
                                    .on_hover_text("Choisir cette ligne")
                                    .clicked()
                                {
                                    *selected = Some(i);
                                }
                                for (j, (col, cell)) in columns.iter().zip(row.iter_mut()).enumerate() {
                                    ui.push_id((i, j), |ui| match cell {
                                        Cell::Text(s) => {
                                            let h = ui.spacing().interact_size.y;
                                            ui.add_sized(
                                                [col.width.unwrap_or(140.0), h],
                                                egui::TextEdit::singleline(s),
                                            );
                                        }
                                        Cell::Number(n) => {
                                            ui.add(egui::DragValue::new(n).speed(1.0));
                                        }
                                        Cell::Bool(b) => {
                                            ui.checkbox(b, "");
                                        }
                                        Cell::Choice(k) => {
                                            egui::ComboBox::from_id_salt("cellule")
                                                .selected_text(choice_label(&col.choices, Some(*k)))
                                                .width(col.width.unwrap_or(120.0))
                                                .show_ui(ui, |ui| {
                                                    for (x, ch) in col.choices.iter().enumerate() {
                                                        ui.selectable_value(k, x, &ch.label);
                                                    }
                                                });
                                        }
                                    });
                                }
                                ui.end_row();
                            }
                        });
                        if rows.is_empty() {
                            ui.label(RichText::new("(aucune ligne)").weak().small());
                        }
                    },
                );
            });
            ui.horizontal(|ui| {
                if ui.button("➕ Ligne").clicked() {
                    rows.push(columns.iter().map(Cell::empty).collect());
                    *selected = Some(rows.len() - 1);
                }
                let sel = selected.filter(|i| *i < rows.len());
                if ui.add_enabled(sel.is_some(), egui::Button::new("➖ Retirer")).clicked() {
                    if let Some(i) = sel {
                        rows.remove(i);
                        *selected = (!rows.is_empty()).then(|| i.min(rows.len() - 1));
                    }
                }
                if ui.add_enabled(sel.is_some_and(|i| i > 0), egui::Button::new("⬆")).clicked() {
                    *selected = sel.map(|i| move_item(rows, i, true));
                }
                if ui.add_enabled(sel.is_some_and(|i| i + 1 < rows.len()), egui::Button::new("⬇")).clicked() {
                    *selected = sel.map(|i| move_item(rows, i, false));
                }
            });
        }
        _ => {}
    }
    error_line(ui, st, &key);
}

/// Liste de choix d'un script (fenêtre modale) : tableau triable, filtre, vignettes avec
/// aperçu agrandi, double-clic = choisir.
pub(crate) fn choose_window(ctx: &Context, script: &str, st: &mut ChooseState) -> Action<Vec<usize>> {
    if let Some(expired) = st.expired() {
        return Action::Reply(expired);
    }
    let images = !st.images.is_empty();
    let (width, max_h) = sized(ctx, if images { 900.0 } else { 640.0 });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
    let mut action = Action::Nothing;
    let mut validate = false;
    let mut hovered: Option<usize> = None;
    let view = st.view();
    egui::Modal::new(Id::unique("comicrust-script-choix")).show(ctx, |ui| {
        ui.set_width(width);
        ui.label(RichText::new(format!("📜 {script}")).weak().small());
        if !st.spec.title.is_empty() {
            ui.label(RichText::new(&st.spec.title).heading());
        }
        if !st.spec.message.is_empty() {
            ui.label(&st.spec.message);
        }
        if st.spec.filter {
            ui.horizontal(|ui| {
                ui.label("🔍");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut st.filter)
                        .desired_width(280.0)
                        .hint_text("filtrer (tous les mots)"),
                );
                if r.changed() {
                    st.deadline = None;
                }
                if !st.filter.is_empty() && ui.small_button("✖").on_hover_text("Effacer le filtre").clicked() {
                    st.filter.clear();
                }
                ui.label(RichText::new(format!("{} / {}", view.len(), st.spec.rows.len())).small().weak());
            });
        }
        if let Some(s) = st.remaining() {
            ui.label(
                RichText::new(format!("Choix automatique dans {s} s (toute action l'annule)"))
                    .color(crate::ui::theme::warning(ui)),
            );
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
        ui.add_space(4.0);
        let preview_w = if images { 250.0 } else { 0.0 };
        let table_h = (max_h - 120.0).clamp(140.0, 480.0);
        ui.horizontal_top(|ui| {
            let table_w = (ui.available_width() - preview_w - 12.0).max(200.0);
            ui.allocate_ui_with_layout(egui::vec2(table_w, table_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_min_size(egui::vec2(table_w, table_h));
                choose_table(ui, st, &view, table_h, &mut hovered, &mut validate);
            });
            if images {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(preview_w);
                    let shown = hovered.or(st.current);
                    match shown.and_then(|i| st.texture(ui.ctx(), i).map(|t| (i, t))) {
                        Some((i, tex)) => {
                            ui.add(
                                egui::Image::from_texture(egui::load::SizedTexture::from_handle(&tex))
                                    .max_size(egui::vec2(preview_w, table_h - 40.0)),
                            );
                            ui.label(RichText::new(st.spec.rows[i].first().cloned().unwrap_or_default()).strong());
                        }
                        None => {
                            ui.label(RichText::new("Survoler une ligne pour voir son image.").weak().small());
                        }
                    }
                });
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let can = st.can_validate();
            let label = if st.spec.multi {
                format!("Choisir ({})", st.selected.iter().filter(|s| **s).count())
            } else {
                "Choisir".to_string()
            };
            if ui.add_enabled(can, egui::Button::new(RichText::new(label).strong())).clicked() || (enter && can) {
                validate = true;
            }
            if ui.button("Annuler").on_hover_text("Le script reçoit None").clicked() {
                action = Action::Reply(None);
            }
            if st.spec.multi {
                if ui.small_button("Tout").clicked() {
                    for &i in &view {
                        st.selected[i] = true;
                    }
                    st.deadline = None;
                }
                if ui.small_button("Rien").clicked() {
                    st.selected.iter_mut().for_each(|s| *s = false);
                    st.deadline = None;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⏹ Arrêter le script").clicked() {
                    action = Action::Stop;
                }
            });
        });
    });
    if !matches!(action, Action::Nothing) {
        return action;
    }
    if validate && st.can_validate() {
        return Action::Reply(Some(st.picked()));
    }
    if escape {
        return Action::Reply(None);
    }
    Action::Nothing
}

fn choose_table(
    ui: &mut Ui,
    st: &mut ChooseState,
    view: &[usize],
    height: f32,
    hovered: &mut Option<usize>,
    validate: &mut bool,
) {
    let images = !st.images.is_empty();
    let thumb = st.spec.thumb_size;
    let row_h = if images { thumb } else { ui.text_style_height(&egui::TextStyle::Body) + 6.0 };
    let n_cols = st.spec.columns.len();
    let header_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    // Textures créées avant le tableau (emprunts séparés).
    let mut textures: HashMap<usize, egui::TextureHandle> = HashMap::new();
    if images {
        for &i in view {
            if let Some(tex) = st.texture(ui.ctx(), i) {
                textures.insert(i, tex);
            }
        }
    }
    let mut t = TableBuilder::new(ui)
        .id_salt("choix-script")
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .max_scroll_height(height - 30.0)
        .min_scrolled_height(height - 30.0)
        .auto_shrink([false, false])
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    if st.spec.multi {
        t = t.column(Column::exact(26.0));
    }
    if images {
        t = t.column(Column::exact(thumb * 0.75 + 6.0));
    }
    for i in 0..n_cols {
        t = if i + 1 == n_cols {
            t.column(Column::remainder().at_least(80.0).clip(true))
        } else {
            t.column(Column::initial(if i == 0 { 260.0 } else { 110.0 }).at_least(40.0).clip(true))
        };
    }
    let mut sort_by = None;
    let mut click = None;
    let mut double = None;
    t.header(header_h, |mut h| {
        if st.spec.multi {
            h.col(|_| {});
        }
        if images {
            h.col(|_| {});
        }
        for (i, title) in st.spec.columns.iter().enumerate() {
            h.col(|ui| {
                let mut text = title.clone();
                match st.sort {
                    Some((c, desc)) if c == i => text.push_str(if desc { " ⬇" } else { " ⬆" }),
                    _ => {}
                }
                if ui.add(egui::Button::new(RichText::new(text).strong()).frame(false)).on_hover_text("Trier").clicked()
                {
                    sort_by = Some(i);
                }
            });
        }
    })
    .body(|body| {
        body.rows(row_h, view.len(), |mut row| {
            let i = view[row.index()];
            row.set_selected(st.selected[i]);
            if st.spec.multi {
                row.col(|ui| {
                    // Simple indication : le clic sur la ligne coche ou décoche.
                    ui.add(egui::Label::new(if st.selected[i] { "☑" } else { "☐" }).selectable(false));
                });
            }
            if images {
                row.col(|ui| match textures.get(&i) {
                    Some(tex) => {
                        let r = ui.add(
                            egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
                                .max_size(egui::vec2(thumb * 0.75, thumb - 4.0)),
                        );
                        if r.hovered() {
                            *hovered = Some(i);
                        }
                        r.on_hover_ui(|ui| {
                            ui.add(
                                egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
                                    .max_size(egui::vec2(320.0, 420.0)),
                            );
                        });
                    }
                    None => {
                        ui.label(RichText::new("—").weak());
                    }
                });
            }
            for cell in &st.spec.rows[i] {
                row.col(|ui| {
                    ui.add(egui::Label::new(cell.as_str()).truncate().selectable(false));
                });
            }
            let r = row.response();
            if r.hovered() {
                *hovered = Some(i);
            }
            if r.double_clicked() {
                double = Some(i);
            } else if r.clicked() {
                click = Some(i);
            }
        });
    });
    if let Some(c) = sort_by {
        st.toggle_sort(c);
    }
    if let Some(i) = click {
        st.click(i);
    }
    // Double-clic : choix simple → cette ligne est choisie ; choix multiple : le premier
    // clic a déjà coché la ligne, rien de plus.
    if let Some(i) = double.filter(|_| !st.spec.multi) {
        st.click(i);
        *validate = true;
    }
}
