//! Panneaux de la bibliothèque : listes de ComicRack (à gauche) et fiche du livre
//! sélectionné (à droite).

use comicrust_core::comic_lists::{ComicList, ListKind};
use comicrust_core::library::LibraryBook;
use eframe::egui::{self, vec2, Rect, RichText, Ui};

use super::library_table::{human_size, long_date, short_date};
use super::library_view::{paint_cover, ACCENT, COVER_RATIO};
use super::lists_ui::{handle_drop, list_menu, new_menu, rename_field, NodeCtx, PanelAction};
use crate::app::ComicRustApp;

fn icon(l: &ComicList) -> &'static str {
    match &l.kind {
        ListKind::Library => "📚",
        ListKind::Folder { .. } => "📁",
        ListKind::Ids(_) => "📄",
        ListKind::Smart { .. } => "🔍",
    }
}

impl ComicRustApp {
    pub(crate) fn comic_lists_panel(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let target = self.lists_target(&ctx);
        let busy = self.lists_ui.ops.busy();
        let reason = match (&target, busy) {
            (Err(r), _) => Some(r.clone()),
            (_, true) => Some("Enregistrement en cours…".to_string()),
            _ => None,
        };
        let mut acts: Vec<PanelAction> = Vec::new();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading("Listes");
            if self.library.lists.computing() {
                ui.spinner().on_hover_text("Calcul des listes…");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mysql = self.settings.library_mode == crate::settings::LibraryMode::ComicRackMySql;
                if mysql && ui.small_button("…").on_hover_text("Choisir le ComicDb.xml de ComicRack").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_title("ComicDb.xml de ComicRack (listes)")
                        .add_filter("Base ComicRack", &["xml"])
                        .pick_file()
                    {
                        self.settings.library_db = Some(p);
                        self.restart_watch(ui.ctx());
                    }
                }
                let (parent, position) = self.new_list_place();
                let plus = ui.add_enabled_ui(reason.is_none(), |ui| {
                    ui.menu_button(RichText::new("＋").strong(), |ui| {
                        new_menu(ui, &None, parent, position, &mut acts);
                    })
                    .response
                    .on_hover_text("Nouvelle liste de lecture, liste intelligente ou dossier")
                });
                if let Some(r) = &reason {
                    plus.response.on_disabled_hover_text(r);
                }
                if let Err(r) = &target {
                    ui.label(RichText::new("🔒").color(crate::ui::theme::warning(ui)))
                        .on_hover_text(format!("Listes en lecture seule : {r}"));
                }
            });
        });
        ui.separator();
        self.library.poll_lists(ui.ctx());
        // F2 : renommer la liste affichée.
        if reason.is_none()
            && self.lists_ui.renaming.is_none()
            && !ctx.egui_wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::F2))
        {
            if let Some(id) = self.library.selected_list().filter(|l| l.kind != ListKind::Library).map(|l| l.id.clone())
            {
                acts.push(PanelAction::Rename(id));
            }
        }
        let db = self.library.database();
        let mut chosen: Option<Option<String>> = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let all = self.library.selected_list.is_none();
            let n = db.as_ref().map(|d| d.len()).unwrap_or(0);
            if ui.selectable_label(all, format!("📚 Toute la bibliothèque ({n})")).clicked() {
                chosen = Some(None);
            }
            ui.add_space(4.0);
            let items = std::mem::take(&mut self.library.lists.items);
            let mut nc = NodeCtx {
                items: &items,
                editable: target.as_ref().map(|_| ()).map_err(Clone::clone),
                busy,
                renaming: self.lists_ui.renaming.take(),
                expand: std::mem::take(&mut self.lists_ui.expand),
                acts: std::mem::take(&mut acts),
            };
            for l in &items {
                // La « Bibliothèque » de ComicRack fait doublon avec la ligne ci-dessus.
                if l.kind != ListKind::Library {
                    self.list_node(ui, l, &mut chosen, &mut nc);
                }
            }
            self.lists_ui.renaming = nc.renaming;
            self.lists_ui.expand = nc.expand;
            acts = nc.acts;
            self.library.lists.items = items;
            ui.add_space(8.0);
            if let Some(what) = self.lists_ui.ops.current() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("{what}…")).small());
                });
            } else if let Some(info) = self.lists_ui.info.clone() {
                let r = ui.add(egui::Label::new(RichText::new(info).small().weak()).wrap().sense(egui::Sense::click()));
                if r.on_hover_text("Clic : effacer ce message").clicked() {
                    self.lists_ui.info = None;
                }
            }
            if let Err(r) = &target {
                if self.library.lists.error.is_none() {
                    ui.label(RichText::new(format!("🔒 {r}")).small().color(crate::ui::theme::warning(ui)));
                }
            }
            if let Some(e) = &self.library.lists.error {
                ui.label(RichText::new(format!("⚠ {e}")).small().color(crate::ui::theme::warning(ui)));
                if self.settings.library_mode == crate::settings::LibraryMode::ComicRackMySql {
                    ui.label(RichText::new("« … » en haut du panneau pour choisir le fichier.").small().weak());
                }
            } else if let Some(p) = &self.library.lists.path {
                ui.label(RichText::new(p.display().to_string()).small().weak());
                if ui
                    .small_button("📋 Copier le bilan des listes")
                    .on_hover_text("Nombre de livres de chaque liste selon ComicRust et selon ComicRack")
                    .clicked()
                {
                    let text = self.lists_report();
                    ui.ctx().copy_text(text);
                }
            } else if self.library.standalone().is_some() {
                if self.library.lists.items.is_empty() {
                    ui.label(
                        RichText::new(
                            "Pas encore de listes : « ＋ » pour en créer, ou ⚙ → « Transférer la bibliothèque \
                             ComicRack… ».",
                        )
                        .small()
                        .weak(),
                    );
                }
            } else {
                ui.label(RichText::new("Recherche du ComicDb.xml…").small().weak());
            }
        });
        if let Some(c) = chosen {
            self.select_list(c);
        }
        for a in acts {
            self.handle_panel_action(&ctx, a);
        }
    }

    /// Infobulle d'une liste : description, et comparaison avec le compte de ComicRack.
    fn list_tooltip(&self, ui: &mut Ui, l: &ComicList) {
        if !l.description.is_empty() {
            ui.label(&l.description);
        }
        match self.library.lists.count(&l.id) {
            Some(n) => ui.label(format!("ComicRust : {n} livres")),
            None => ui.label("ComicRust : liste intelligente pas encore calculable"),
        };
        ui.label(RichText::new(format!("ComicRack (dernier enregistrement) : {} livres", l.comicrack_count)).weak());
    }

    /// Ligne d'une liste : sélection, menu contextuel, renommage, glisser-déposer.
    fn list_label(&self, ui: &mut Ui, l: &ComicList, chosen: &mut Option<Option<String>>, nc: &mut NodeCtx) {
        if rename_field(ui, l, nc) {
            return;
        }
        let selected = self.library.selected_list.as_deref() == Some(l.id.as_str());
        let count = self.library.lists.count(&l.id).map(|n| format!(" ({n})")).unwrap_or_default();
        let text = format!("{} {}{count}", icon(l), l.name);
        let r = ui
            .add(egui::Button::selectable(selected, text).sense(egui::Sense::click_and_drag()))
            .on_hover_ui(|ui| self.list_tooltip(ui, l));
        if r.clicked() {
            *chosen = Some(Some(l.id.clone()));
        }
        handle_drop(ui, &r, l, nc);
        r.context_menu(|ui| list_menu(ui, l, nc));
    }

    fn list_node(&self, ui: &mut Ui, l: &ComicList, chosen: &mut Option<Option<String>>, nc: &mut NodeCtx) {
        match &l.kind {
            ListKind::Folder { children, .. } => {
                let id = ui.make_persistent_id(("dossier", &l.id));
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
                if nc.expand.remove(&l.id.to_ascii_lowercase()) || nc.expand.remove(&l.id) {
                    state.set_open(true);
                }
                state.show_header(ui, |ui| self.list_label(ui, l, chosen, nc)).body(|ui| {
                    for c in children {
                        self.list_node(ui, c, chosen, nc);
                    }
                });
            }
            _ => self.list_label(ui, l, chosen, nc),
        }
    }

    /// Bilan texte : pour chaque liste, livres comptés par ComicRust et par ComicRack.
    pub(crate) fn lists_report(&self) -> String {
        fn walk(app: &ComicRustApp, l: &[ComicList], depth: usize, out: &mut String) {
            for x in l {
                let ours = match app.library.lists.count(&x.id) {
                    Some(n) => n.to_string(),
                    None => "?".into(),
                };
                let flag = if ours == x.comicrack_count.to_string() { "" } else { "  ≠" };
                out.push_str(&format!(
                    "{}{} {} — ComicRust {ours} / ComicRack {}{flag}\n",
                    "  ".repeat(depth),
                    icon(x),
                    x.name,
                    x.comicrack_count
                ));
                if let ListKind::Folder { children, .. } = &x.kind {
                    walk(app, children, depth + 1, out);
                }
            }
        }
        let mut out = format!(
            "Bilan des listes ComicRust v{} — {}\n(ComicRack : nombre enregistré dans ComicDb.xml à sa dernière sauvegarde)\n\n",
            env!("CARGO_PKG_VERSION"),
            self.library.lists.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
        );
        walk(self, &self.library.lists.items, 0, &mut out);
        out
    }

    /// Fiche du livre sélectionné.
    pub(crate) fn details_panel(&mut self, ui: &mut Ui) {
        let Some(db) = self.library.database() else { return };
        let Some(book) = self.library.selected_book.as_deref().and_then(|id| db.get(id)) else {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| ui.label(RichText::new("Clique sur un livre pour voir sa fiche.").weak()));
            return;
        };
        let book = book.clone();
        let ctx = ui.ctx().clone();
        self.ensure_covers(&ctx);
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(6.0);
            let w = ui.available_width().min(260.0);
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), w / COVER_RATIO), egui::Sense::hover());
            let cover = Rect::from_center_size(rect.center(), vec2(w, w / COVER_RATIO));
            let drawn = paint_cover(ui, self.covers.as_mut().unwrap(), &book, cover);
            if let Some(pct) = self.read_percent(&book) {
                let bar = Rect::from_min_size(
                    egui::pos2(drawn.min.x, drawn.max.y - 5.0),
                    vec2(drawn.width() * pct as f32 / 100.0, 5.0),
                );
                ui.painter().rect_filled(bar, 0.0, ACCENT);
            }
            ui.add_space(6.0);
            ui.label(RichText::new(book.caption()).heading().strong());
            if let Some(t) = book.title.as_deref().filter(|t| !t.is_empty()) {
                ui.label(RichText::new(t).italics());
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let resume = book.current_page.filter(|p| *p > 0);
                let label = match resume {
                    Some(p) => format!("📖 Lire (page {})", p + 1),
                    None => "📖 Lire".to_string(),
                };
                if ui.button(label).clicked() {
                    open = Some(false);
                }
                if resume.is_some() && ui.button("⏮ Depuis le début").clicked() {
                    open = Some(true);
                }
            });
            ui.separator();
            let standalone = self.library.standalone().is_some();
            details_grid(ui, &book, if standalone { None } else { self.comicrust_progress(&book) }, standalone);
            for (title, text) in [("Résumé", &book.summary), ("Annotations", &book.notes), ("Critique", &book.review)]
            {
                if let Some(t) = text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                    ui.add_space(6.0);
                    ui.label(RichText::new(title).strong());
                    ui.label(t);
                }
            }
        });
        if let Some(from_start) = open {
            // « Lire (page N) » : la page de ComicRack, telle qu'affichée sur le bouton.
            let page = if from_start { 0 } else { book.current_page.unwrap_or(0) as usize };
            self.open_library_book(&ctx, &book.id, crate::app::Start::Page(page));
        }
    }
}

/// Oui / Non d'après les valeurs de ComicRack (`Yes`, `YesAndRightToLeft`, `No`, `Unknown`).
fn yes_no(v: &Option<String>) -> Option<String> {
    match v.as_deref()? {
        "Yes" => Some("Oui".into()),
        "YesAndRightToLeft" => Some("Oui (de droite à gauche)".into()),
        "No" => Some("Non".into()),
        _ => None,
    }
}

const MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

/// Rangée de la fiche ; `always` : affichée même vide (« — »), comme dans ComicRack.
struct Field {
    label: &'static str,
    value: Option<String>,
    always: bool,
}

fn f(label: &'static str, value: Option<String>) -> Field {
    Field { label, value: value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()), always: false }
}

fn always(label: &'static str, value: Option<String>) -> Field {
    Field { always: true, ..f(label, value) }
}

fn section(ui: &mut Ui, id: &str, title: &str, fields: Vec<Field>) {
    let fields: Vec<Field> = fields.into_iter().filter(|x| x.always || x.value.is_some()).collect();
    if fields.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong());
    egui::Grid::new(("fiche", id)).num_columns(2).min_col_width(125.0).spacing(vec2(10.0, 3.0)).show(ui, |ui| {
        for x in fields {
            ui.label(RichText::new(x.label).weak());
            match x.value {
                Some(v) => ui.add(egui::Label::new(v).wrap()),
                None => ui.label(RichText::new("—").weak()),
            };
            ui.end_row();
        }
    });
}

/// Fiche du livre, avec les libellés de la fenêtre « Informations » de ComicRack.
fn details_grid(ui: &mut Ui, b: &LibraryBook, comicrust: Option<(u32, usize)>, standalone: bool) {
    let s = |v: &Option<String>| v.clone();
    let num = |n: Option<i32>| n.filter(|v| *v > 0).map(|v| v.to_string());
    let number = b.number.clone().map(|n| match b.count.filter(|c| *c > 0) {
        Some(c) => format!("{n} sur {c}"),
        None => n,
    });
    let alt = b.alternate_series.clone().map(|a| match (&b.alternate_number, b.alternate_count.filter(|c| *c > 0)) {
        (Some(n), Some(c)) if !n.is_empty() => format!("{a} #{n} sur {c}"),
        (Some(n), None) if !n.is_empty() => format!("{a} #{n}"),
        _ => a,
    });
    let month = b.month.filter(|m| (1..=12).contains(m)).map(|m| format!("{} ({m})", MONTHS[m as usize - 1]));
    let rating = |r: Option<f32>| r.filter(|r| *r > 0.0).map(|r| format!("{r:.1} / 5").replace('.', ","));
    section(
        ui,
        "details",
        "Détails",
        vec![
            always("Série :", s(&b.series)),
            always("Numéro :", number),
            always("Volume :", b.volume.filter(|v| *v >= 0).map(|v| v.to_string())),
            always("Titre :", s(&b.title)),
            always("Année :", num(b.year)),
            always("Mois :", month),
            f("Jour :", num(b.day)),
            always("Éditeur :", s(&b.publisher)),
            f("Impression :", s(&b.imprint)),
            f("Collection :", s(&b.editor)),
            always("Série complète :", yes_no(&b.series_complete)),
            f("Format :", s(&b.format)),
            f("Genre :", s(&b.genre)),
            f("Langue :", s(&b.language_iso)),
            f("Limite d'âge :", s(&b.age_rating).filter(|a| a != "Unknown")),
            f("Noir et Blanc :", yes_no(&b.black_and_white)),
            f("Manga :", yes_no(&b.manga)),
        ],
    );
    section(
        ui,
        "artistes",
        "Artistes",
        vec![
            always("Scénariste :", s(&b.writer)),
            always("Dessinateur :", s(&b.penciller)),
            f("Encreur :", s(&b.inker)),
            f("Coloriste :", s(&b.colorist)),
            f("Lettreur :", s(&b.letterer)),
            f("Artiste de couverture :", s(&b.cover_artist)),
            f("Traducteur :", s(&b.translator)),
        ],
    );
    section(
        ui,
        "plus",
        "Plus",
        vec![
            f("Arc narratif :", s(&b.story_arc)),
            f("Groupes de la Série :", s(&b.series_group)),
            f("Série alternative :", alt),
            f("Personnages :", s(&b.characters)),
            f("Équipes :", s(&b.teams)),
            f("Équipe/Personnage principal :", s(&b.main_character_or_team)),
            f("Lieu(x) :", s(&b.locations)),
            f("Étiquettes :", s(&b.tags)),
            f("Informations sur le scan :", s(&b.scan_information)),
        ],
    );
    section(
        ui,
        "lecture",
        "Lecture",
        vec![
            f("Mon évaluation :", rating(b.rating)),
            f("Évaluation communautaire :", rating(b.community_rating)),
            always("Pages :", b.page_count.map(|p| p.to_string())),
            f(
                if standalone { "Lu :" } else { "Lu dans ComicRack :" },
                b.read_percent().map(|p| match b.current_page {
                    Some(c) if p > 0 => format!("{p} % (page {})", c + 1),
                    _ => format!("{p} %"),
                }),
            ),
            f("Lu dans ComicRust :", comicrust.map(|(p, page)| format!("{p} % (page {})", page + 1))),
            f("Ouvert/Lu le :", Some(long_date(b.opened.as_deref()))),
            f("Ouvertures :", b.open_count.filter(|n| *n > 0).map(|n| format!("{n} fois"))),
            f("Ajouté/Acheté le :", Some(short_date(b.added.as_deref()))),
            f("Sorti :", Some(short_date(b.released.as_deref()))),
        ],
    );
    section(
        ui,
        "fichier",
        "Fichier",
        vec![always("Fichier :", s(&b.file)), f("Taille :", b.file_size.filter(|s| *s > 0).map(human_size))],
    );
    if let Some(web) = b.web.as_deref().map(str::trim).filter(|w| w.starts_with("http")) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Web :").weak());
            ui.hyperlink_to(web, web);
        });
    }
}
