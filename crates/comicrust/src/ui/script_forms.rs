//! Dessin des formulaires (`comicrust.form`) et des listes de choix avec vignettes
//! (`comicrust.choose`) des scripts (v0.10.1), dans des fenêtres modales qui suivent le
//! thème clair/sombre. La logique (champs, validation, filtre, tri) est dans
//! `scripts/forms.rs`.

use std::collections::HashMap;
use std::time::Duration;

use comicrust_core::scripting::forms::{ChooseLayout, Control, ControlKind, LabelStyle, MAX_THUMB_SIZE};
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

/// Libellé à gauche d'un champ (largeur fixe `width`, infobulle d'aide ; un libellé trop
/// long est coupé et montré en entier au survol). Libellé vide : rien (le champ n'est pas
/// décalé).
fn label_cell(ui: &mut Ui, width: f32, label: &str, help: &str) {
    if label.is_empty() {
        return;
    }
    let h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(egui::vec2(width, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        let r = ui.add(egui::Label::new(label).truncate());
        let full = if help.is_empty() { label.to_string() } else { format!("{label}\n\n{help}") };
        if r.hovered() && (r.rect.width() + 1.0 >= width || !help.is_empty()) {
            r.on_hover_text(full);
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
    crate::ui::theme::modal(ctx, Id::unique("comicrust-script-formulaire")).show(ctx, |ui| {
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
        // Boutons du script à droite, de même taille, dans l'ordre donné (le premier, mis en
        // valeur, est celui d'Entrée) ; « Arrêter le script » à gauche, à l'écart.
        ui.horizontal(|ui| {
            if ui.small_button("⏹ Arrêter le script").clicked() {
                stop = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let main = st.main_button().cloned();
                for b in st.spec.buttons.iter().rev() {
                    let primary = main.as_ref() == Some(b);
                    let mut button =
                        egui::Button::new(if primary { RichText::new(b).strong() } else { RichText::new(b) })
                            .min_size(egui::vec2(96.0, 28.0));
                    if primary {
                        button = button.fill(ui.visuals().selection.bg_fill);
                    }
                    if ui.add(button).clicked() {
                        clicked = Some(b.clone());
                    }
                }
            });
        });
    });
    // Entrée dans un champ d'une ligne : bouton principal.
    if std::mem::take(&mut st.enter) && clicked.is_none() {
        clicked = st.main_button().cloned();
    }
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
            egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !title.is_empty() {
                    hover(ui.label(RichText::new(title).strong()), help);
                    ui.add_space(2.0);
                }
                draw_controls(ui, controls, st, values, enabled, clicked);
            });
            ui.add_space(4.0);
        }
        ControlKind::Columns { columns, widths } if widths.iter().all(Option::is_none) => {
            ui.columns(columns.len(), |uis| {
                for (u, col) in uis.iter_mut().zip(columns) {
                    draw_controls(u, col, st, values, enabled, clicked);
                }
            });
        }
        ControlKind::Columns { columns, widths } => {
            // Largeurs imposées (pixels) ; les autres colonnes se partagent le reste.
            let gap = ui.spacing().item_spacing.x;
            let total = ui.available_width() - gap * (columns.len().saturating_sub(1)) as f32;
            let fixes: f32 = widths.iter().flatten().sum();
            let libres = columns.len() - widths.iter().flatten().count();
            let part = ((total - fixes) / libres.max(1) as f32).max(40.0);
            ui.horizontal_top(|ui| {
                for (i, col) in columns.iter().enumerate() {
                    let w = widths.get(i).copied().flatten().unwrap_or(part).min(total.max(40.0));
                    ui.allocate_ui_with_layout(egui::vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |u| {
                        u.set_width(w);
                        draw_controls(u, col, st, values, enabled, clicked);
                    });
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

/// Menu « Insérer » d'un champ texte : champ de filtre en tête (libellé ou texte inséré),
/// puis les morceaux, sous le titre de leur groupe s'il en a un. Insère au curseur (à la
/// fin si le champ n'a jamais eu le focus).
fn inserts_menu(ui: &mut Ui, id: Id, text: &mut String, inserts: &[comicrust_core::scripting::forms::InsertGroup]) {
    use comicrust_core::scripting::forms::{filter_inserts, value_text};
    ui.menu_button("Insérer ▾", |ui| {
        let filter_id = id.with("filtre-insertions");
        let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
        let total: usize = inserts.iter().map(|g| g.items.len()).sum();
        if total > 8 {
            let r = ui.add(egui::TextEdit::singleline(&mut filter).hint_text("🔍 Filtrer…").desired_width(220.0));
            if r.changed() {
                ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            }
            ui.separator();
        }
        let shown = filter_inserts(inserts, &filter);
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            if shown.is_empty() {
                ui.weak("Aucun élément");
            }
            for (title, items) in shown {
                if !title.is_empty() {
                    ui.add_space(2.0);
                    ui.label(RichText::new(title).strong().small());
                }
                for ins in items {
                    let snippet = value_text(&ins.value);
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
                        ui.data_mut(|d| d.remove::<String>(filter_id));
                        ui.close();
                    }
                }
            }
        });
    });
}

fn draw_field(ui: &mut Ui, c: &Control, st: &mut FormState) {
    let help = c.help.as_str();
    let key = c.key.clone();
    let lw = st.spec.label_width.unwrap_or(LABEL_W);
    let mut enter = false;
    draw_field_inner(ui, c, st, &key, lw, help, &mut enter);
    if enter {
        st.enter = true;
    }
}

fn draw_field_inner(ui: &mut Ui, c: &Control, st: &mut FormState, key: &str, lw: f32, help: &str, enter: &mut bool) {
    let key = key.to_string();
    let Some(field) = st.fields.get_mut(&key) else { return };
    match (&c.kind, field) {
        (ControlKind::Text { placeholder, rows, secret, inserts, paste, hints, hint_none, .. }, Field::Text(text)) => {
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
                    label_cell(ui, lw, &c.label, help);
                    let extra = if inserts.is_empty() { 0.0 } else { 90.0 } + if *paste { 100.0 } else { 0.0 };
                    let w = c.width.unwrap_or((ui.available_width() - extra).max(80.0));
                    let mut edit = egui::TextEdit::singleline(text)
                        .id(id)
                        .desired_width(w)
                        .hint_text(hint)
                        .margin(egui::vec2(6.0, 4.0));
                    if *secret {
                        edit = edit.password(true);
                    }
                    let r = hover(ui.add(edit), help);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        *enter = true;
                    }
                    if *paste
                        && ui.button("📋 Coller").on_hover_text("Remplacer par le texte du presse-papiers").clicked()
                    {
                        // Le texte du presse-papiers arrive au champ (qui reprend le focus,
                        // tout son texte sélectionné) à l'image suivante.
                        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                        let all = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(text.chars().count()),
                        );
                        state.cursor.set_char_range(Some(all));
                        state.store(ui.ctx(), id);
                        ui.ctx().memory_mut(|m| m.request_focus(id));
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    }
                    if !inserts.is_empty() {
                        inserts_menu(ui, id, text, inserts);
                    }
                });
                if let Some((ok, t)) = comicrust_core::scripting::forms::text_hint(hints, hint_none, text) {
                    ui.horizontal(|ui| {
                        if !c.label.is_empty() {
                            ui.add_space(lw + ui.spacing().item_spacing.x);
                        }
                        let rt = if ok {
                            RichText::new(format!("✔ {t}")).color(crate::ui::theme::ok(ui))
                        } else {
                            RichText::new(format!("⚠ {t}")).color(crate::ui::theme::warning(ui))
                        };
                        ui.add(egui::Label::new(rt).wrap());
                    });
                }
            }
        }
        (ControlKind::Path { folder, save, filters, .. }, Field::Text(text)) => {
            ui.horizontal(|ui| {
                label_cell(ui, lw, &c.label, help);
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
                label_cell(ui, lw, &c.label, help);
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
                label_cell(ui, lw, &c.label, help);
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
                label_cell(ui, lw, &c.label, help);
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
                label_cell(ui, lw, &c.label, help);
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

/// Liste de choix d'un script (fenêtre modale) : tableau triable ou fiches (vignette,
/// titre en gras, autres colonnes en dessous), filtre, taille des vignettes réglable, aperçu
/// agrandi à droite, double-clic ou Entrée = choisir, flèches ↑/↓ pour changer de ligne.
pub(crate) fn choose_window(ctx: &Context, script: &str, st: &mut ChooseState) -> Action<Vec<usize>> {
    if let Some(expired) = st.expired() {
        return Action::Reply(expired);
    }
    let images = st.has_images();
    let cards = st.spec.layout == ChooseLayout::Cards;
    let (width, max_h) = sized(
        ctx,
        if cards {
            1120.0
        } else if images {
            900.0
        } else {
            640.0
        },
    );
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
    let arrows = ctx.input(|i| {
        i.key_pressed(egui::Key::ArrowDown) as i32 - i.key_pressed(egui::Key::ArrowUp) as i32
            + 5 * (i.key_pressed(egui::Key::PageDown) as i32 - i.key_pressed(egui::Key::PageUp) as i32)
    });
    let mut action = Action::Nothing;
    let mut validate = false;
    let mut hovered: Option<usize> = None;
    let view = st.view();
    if arrows != 0 {
        st.step(&view, arrows);
    }
    crate::ui::theme::modal(ctx, Id::unique("comicrust-script-choix")).show(ctx, |ui| {
        ui.set_width(width);
        ui.label(RichText::new(format!("📜 {script}")).weak().small());
        if !st.spec.title.is_empty() {
            ui.label(RichText::new(&st.spec.title).heading());
        }
        if !st.spec.message.is_empty() {
            ui.label(&st.spec.message);
        }
        // Barre d'outils : filtre, tri (fiches), taille des vignettes.
        ui.horizontal(|ui| {
            if st.spec.filter {
                ui.label("🔍");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut st.filter)
                        .desired_width(240.0)
                        .hint_text("filtrer (tous les mots)"),
                );
                if r.changed() {
                    st.deadline = None;
                }
                if !st.filter.is_empty() && ui.small_button("✖").on_hover_text("Effacer le filtre").clicked() {
                    st.filter.clear();
                }
                ui.label(RichText::new(format!("{} / {}", view.len(), st.spec.rows.len())).small().weak());
                ui.separator();
            }
            if cards {
                sort_menu(ui, st);
            }
            if images {
                ui.separator();
                ui.label("Taille");
                let r = ui
                    .add(egui::Slider::new(&mut st.zoom, 48.0..=MAX_THUMB_SIZE).show_value(false))
                    .on_hover_text("Hauteur des vignettes");
                if r.changed() {
                    st.deadline = None;
                }
                // Vignettes chargées après l'ouverture : combien sont encore attendues.
                let attendues = (0..st.spec.rows.len()).filter(|&i| st.is_loading(i) && !st.waits_hover(i)).count();
                if attendues > 0 {
                    ui.separator();
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(RichText::new(format!("{attendues} image(s) en chargement")).small().weak())
                        .on_hover_text("Les images arrivent une à une ; survoler une ligne charge la sienne d'abord.");
                }
            }
        });
        if let Some(s) = st.remaining() {
            ui.label(
                RichText::new(format!("Choix automatique dans {s} s (toute action l'annule)"))
                    .color(crate::ui::theme::warning(ui)),
            );
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
        ui.add_space(4.0);
        // Aperçu agrandi : environ un tiers de la fenêtre (une couverture lisible).
        let preview_w = if images { (width * if cards { 0.32 } else { 0.28 }).clamp(240.0, 380.0) } else { 0.0 };
        let table_h = (max_h - 120.0).clamp(160.0, if cards { 620.0 } else { 480.0 });
        ui.horizontal_top(|ui| {
            let table_w = (ui.available_width() - preview_w - 12.0).max(200.0);
            ui.allocate_ui_with_layout(egui::vec2(table_w, table_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_min_size(egui::vec2(table_w, table_h));
                if cards {
                    choose_cards(ui, st, &view, table_h, &mut hovered, &mut validate);
                } else {
                    choose_table(ui, st, &view, table_h, &mut hovered, &mut validate);
                }
            });
            if images {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(preview_w);
                    let shown = hovered.or(st.current);
                    if let Some(i) = shown {
                        // Vignette attendue : demandée au script en priorité.
                        st.want(i);
                    }
                    match shown {
                        Some(i) => {
                            let lines = (st.spec.columns.len().saturating_sub(1) as f32 + 2.0) * 18.0;
                            match st.texture(ui.ctx(), i) {
                                Some(tex) => {
                                    // Agrandie jusqu'à la largeur de l'aperçu (proportions gardées).
                                    ui.add(
                                        egui::Image::from_texture(egui::load::SizedTexture::from_handle(&tex))
                                            .fit_to_exact_size(egui::vec2(preview_w, (table_h - lines).max(120.0))),
                                    );
                                }
                                None if st.is_loading(i) => {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Spinner::new());
                                        ui.label(RichText::new("Chargement de l'image…").weak().small());
                                    });
                                }
                                None => {
                                    ui.label(RichText::new("(pas d'image)").weak().small());
                                }
                            }
                            card_text(ui, &st.spec.columns, &st.spec.rows[i], true);
                        }
                        None => {
                            ui.label(
                                RichText::new("Survoler ou choisir une ligne pour voir son image.").weak().small(),
                            );
                        }
                    }
                });
            }
        });
        ui.separator();
        // Comme les formulaires : « Arrêter le script » à gauche, Choisir / Annuler à droite.
        ui.horizontal(|ui| {
            if ui.small_button("⏹ Arrêter le script").clicked() {
                action = Action::Stop;
            }
            if st.spec.multi {
                ui.separator();
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
                let size = egui::vec2(96.0, 28.0);
                if ui.add(egui::Button::new("Annuler").min_size(size)).on_hover_text("Le script reçoit None").clicked()
                {
                    action = Action::Reply(None);
                }
                let can = st.can_validate();
                let label = if st.spec.multi {
                    format!("Choisir ({})", st.selected.iter().filter(|s| **s).count())
                } else {
                    "Choisir".to_string()
                };
                let choisir = egui::Button::new(RichText::new(label).strong())
                    .min_size(size)
                    .fill(ui.visuals().selection.bg_fill);
                if ui.add_enabled(can, choisir).clicked() || (enter && can) {
                    validate = true;
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

/// « Trier par » des fiches : ordre d'origine (celui du script) ou une colonne, croissant
/// ou décroissant.
fn sort_menu(ui: &mut Ui, st: &mut ChooseState) {
    ui.label("Trier par");
    let current = match st.sort {
        Some((c, _)) => {
            st.spec.columns.get(c).cloned().filter(|t| !t.is_empty()).unwrap_or(format!("colonne {}", c + 1))
        }
        None => "ordre d'origine".to_string(),
    };
    let mut wanted = st.sort.map(|(c, _)| c);
    egui::ComboBox::from_id_salt("choix-tri").selected_text(current).width(150.0).show_ui(ui, |ui| {
        ui.selectable_value(&mut wanted, None, "ordre d'origine");
        for (i, title) in st.spec.columns.iter().enumerate() {
            let t = if title.is_empty() { format!("colonne {}", i + 1) } else { title.clone() };
            ui.selectable_value(&mut wanted, Some(i), t);
        }
    });
    if wanted != st.sort.map(|(c, _)| c) {
        st.sort = wanted.map(|c| (c, false));
        st.deadline = None;
    }
    if let Some((c, desc)) = st.sort {
        let r = ui.small_button(if desc { "⬇" } else { "⬆" }).on_hover_text("Inverser l'ordre");
        if r.clicked() {
            st.sort = Some((c, !desc));
            st.deadline = None;
        }
    }
}

/// Texte d'une fiche : première cellule en gras, puis « Colonne : valeur » pour les autres
/// cellules non vides (une par ligne, coupées si trop longues).
fn card_text(ui: &mut Ui, columns: &[String], row: &[String], wrap: bool) {
    let title = row.first().map(String::as_str).unwrap_or_default();
    let t = egui::Label::new(RichText::new(title).strong().size(15.0)).selectable(false);
    ui.add(if wrap { t.wrap() } else { t.truncate() });
    for (col, cell) in columns.iter().zip(row.iter()).skip(1) {
        if cell.trim().is_empty() {
            continue;
        }
        let mut job = egui::text::LayoutJob::default();
        let body = egui::TextStyle::Body.resolve(ui.style());
        // Nom de la colonne un peu plus petit et estompé, mais lisible.
        let small = egui::FontId::new(body.size * 0.88, body.family.clone());
        if !col.is_empty() {
            job.append(&format!("{col} : "), 0.0, egui::TextFormat::simple(small, ui.visuals().weak_text_color()));
        }
        job.append(cell, 0.0, egui::TextFormat::simple(body, ui.visuals().text_color()));
        if !wrap {
            job.wrap = egui::text::TextWrapping::truncate_at_width(ui.available_width());
        }
        let l = egui::Label::new(job).selectable(false);
        ui.add(if wrap { l.wrap() } else { l });
    }
}

/// Fiches de la liste de choix : une par ligne, à hauteur fixe (défilement rapide même
/// avec des centaines de résultats : seules les fiches visibles sont dessinées).
fn choose_cards(
    ui: &mut Ui,
    st: &mut ChooseState,
    view: &[usize],
    height: f32,
    hovered: &mut Option<usize>,
    validate: &mut bool,
) {
    let images = st.has_images();
    let thumb = if images { st.zoom } else { 0.0 };
    let text_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    let lines = st.spec.columns.len().max(1) as f32;
    // Hauteur d'une fiche : la vignette, ou assez pour toutes les lignes de texte.
    let row_h = thumb.max(text_h * (lines + 0.3)) + 10.0;
    let thumb_w = (thumb * 0.72).round();
    let pos = st.scroll_to.take().and_then(|i| view.iter().position(|&v| v == i));
    let mut area = egui::ScrollArea::vertical().id_salt("fiches-choix").max_height(height).auto_shrink([false, false]);
    if let Some(p) = pos {
        // Ligne amenée dans la partie visible si elle n'y est pas.
        let offset = egui::scroll_area::State::load(ui.ctx(), ui.make_persistent_id("fiches-choix"))
            .map(|s| s.clamped_offset().y)
            .unwrap_or(0.0);
        let top = p as f32 * row_h;
        if top < offset {
            area = area.vertical_scroll_offset(top);
        } else if top + row_h > offset + height {
            area = area.vertical_scroll_offset(top + row_h - height);
        }
    }
    let mut click = None;
    let mut double = None;
    area.show_rows(ui, row_h, view.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for k in range {
            let i = view[k];
            let w = ui.available_width();
            let (rect, r) = ui.allocate_exact_size(egui::vec2(w, row_h), egui::Sense::click());
            let selected = st.selected[i];
            let visuals = ui.visuals();
            let fill = if selected {
                visuals.selection.bg_fill
            } else if r.hovered() {
                visuals.widgets.hovered.weak_bg_fill
            } else if k % 2 == 1 {
                visuals.faint_bg_color
            } else {
                egui::Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect.shrink2(egui::vec2(0.0, 1.0)), 4.0, fill);
            let inner = rect.shrink(5.0);
            let mut x = inner.left();
            if st.spec.multi {
                let mark = if selected { "☑" } else { "☐" };
                ui.painter().text(
                    egui::pos2(x + 8.0, inner.center().y),
                    egui::Align2::CENTER_CENTER,
                    mark,
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().text_color(),
                );
                x += 22.0;
            }
            if images {
                let slot = egui::Rect::from_min_size(egui::pos2(x, inner.top()), egui::vec2(thumb_w, inner.height()));
                match st.texture(ui.ctx(), i) {
                    Some(tex) => {
                        let size = tex.size_vec2();
                        let scale = (slot.width() / size.x).min(slot.height() / size.y);
                        let shown = egui::Rect::from_center_size(slot.center(), size * scale);
                        ui.painter().image(
                            tex.id(),
                            shown,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                    None => {
                        ui.painter().rect_filled(slot, 2.0, crate::ui::theme::placeholder(ui));
                        if st.is_loading(i) && !st.waits_hover(i) {
                            // Image en route : petit indicateur animé au centre.
                            let r = egui::Rect::from_center_size(slot.center(), egui::vec2(18.0, 18.0));
                            egui::Spinner::new().size(18.0).paint_at(ui, r);
                        } else {
                            let mark = if st.waits_hover(i) { "👁" } else { "—" };
                            ui.painter().text(
                                slot.center(),
                                egui::Align2::CENTER_CENTER,
                                mark,
                                egui::TextStyle::Body.resolve(ui.style()),
                                ui.visuals().weak_text_color(),
                            );
                        }
                    }
                }
                x += thumb_w + 10.0;
            }
            let text_rect = egui::Rect::from_min_max(egui::pos2(x, inner.top()), inner.right_bottom());
            let mut child = ui
                .new_child(egui::UiBuilder::new().max_rect(text_rect).layout(egui::Layout::top_down(egui::Align::Min)));
            child.set_clip_rect(text_rect.intersect(ui.clip_rect()));
            child.spacing_mut().item_spacing.y = 2.0;
            card_text(&mut child, &st.spec.columns, &st.spec.rows[i], false);
            if pos == Some(k) {
                r.scroll_to_me(None);
            }
            if r.hovered() {
                *hovered = Some(i);
            }
            if r.double_clicked() {
                double = Some(i);
            } else if r.clicked() {
                click = Some(i);
            }
        }
    });
    if view.is_empty() {
        ui.label(RichText::new("Aucun résultat (filtre ?)").weak());
    }
    if let Some(i) = click {
        st.click(i);
    }
    if let Some(i) = double.filter(|_| !st.spec.multi) {
        st.click(i);
        *validate = true;
    }
}

fn choose_table(
    ui: &mut Ui,
    st: &mut ChooseState,
    view: &[usize],
    height: f32,
    hovered: &mut Option<usize>,
    validate: &mut bool,
) {
    let images = st.has_images();
    let thumb = st.zoom;
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
    if let Some(p) = st.scroll_to.take().and_then(|i| view.iter().position(|&v| v == i)) {
        t = t.scroll_to_row(p, None);
    }
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
                    None if st.is_loading(i) && !st.waits_hover(i) => {
                        ui.add(egui::Spinner::new());
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
