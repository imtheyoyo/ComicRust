//! Panneaux de la bibliothèque : listes de ComicRack (à gauche) et fiche du livre
//! sélectionné (à droite).

use comicrust_core::comic_lists::{ComicList, ListKind};
use comicrust_core::library::LibraryBook;
use eframe::egui::{self, vec2, Color32, Rect, RichText, Ui};

use super::library_table::{human_size, long_date, short_date};
use super::library_view::{paint_cover, ACCENT, COVER_RATIO};
use crate::app::ComicRustApp;
use crate::library::SortKey;

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
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading("Listes");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("…").on_hover_text("Choisir le ComicDb.xml de ComicRack").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_title("ComicDb.xml de ComicRack (listes)")
                        .add_filter("Base ComicRack", &["xml"])
                        .pick_file()
                    {
                        self.settings.library_db = Some(p);
                        self.restart_watch(ui.ctx());
                    }
                }
            });
        });
        ui.separator();
        self.library.refresh_list_counts();
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
            for l in &items {
                // La « Bibliothèque » de ComicRack fait doublon avec la ligne ci-dessus.
                if l.kind != ListKind::Library {
                    self.list_node(ui, l, &mut chosen);
                }
            }
            self.library.lists.items = items;
            ui.add_space(8.0);
            if let Some(e) = &self.library.lists.error {
                ui.label(RichText::new(format!("⚠ {e}")).small().color(Color32::from_rgb(240, 170, 90)));
                ui.label(RichText::new("« … » en haut du panneau pour choisir le fichier.").small().weak());
            } else if let Some(p) = &self.library.lists.path {
                ui.label(RichText::new(p.display().to_string()).small().weak());
            } else {
                ui.label(RichText::new("Recherche du ComicDb.xml…").small().weak());
            }
        });
        if let Some(c) = chosen {
            let is_reading_list = c
                .as_deref()
                .and_then(|id| ComicList::find(&self.library.lists.items, id))
                .is_some_and(|l| matches!(l.kind, ListKind::Ids(_)));
            self.library.selected_list = c;
            // Une liste de lecture s'affiche dans son ordre, sans groupes.
            if is_reading_list {
                self.settings.library_sort = SortKey::Position;
                self.settings.library_descending = false;
                self.settings.library_group = crate::library::GroupKey::None;
            }
            if let Some(cv) = &mut self.covers {
                cv.cancel_pending();
            }
        }
    }

    fn list_node(&self, ui: &mut Ui, l: &ComicList, chosen: &mut Option<Option<String>>) {
        let selected = self.library.selected_list.as_deref() == Some(l.id.as_str());
        let count = self.library.lists.count(&l.id).map(|n| format!(" ({n})")).unwrap_or_default();
        let text = format!("{} {}{count}", icon(l), l.name);
        match &l.kind {
            ListKind::Folder { children, .. } => {
                let id = ui.make_persistent_id(("dossier", &l.id));
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                    .show_header(ui, |ui| {
                        if ui.selectable_label(selected, text).clicked() {
                            *chosen = Some(Some(l.id.clone()));
                        }
                    })
                    .body(|ui| {
                        for c in children {
                            self.list_node(ui, c, chosen);
                        }
                    });
            }
            _ => {
                let mut r = ui.selectable_label(selected, text);
                if !l.description.is_empty() {
                    r = r.on_hover_text(&l.description);
                }
                if matches!(l.kind, ListKind::Smart { cached: None }) {
                    r = r.on_hover_text("Liste intelligente : contenu non mémorisé par ComicRack");
                }
                if r.clicked() {
                    *chosen = Some(Some(l.id.clone()));
                }
            }
        }
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
            if let Some(pct) = book.read_percent().filter(|p| *p > 0) {
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
            details_grid(ui, &book);
            for (title, text) in [("Résumé", &book.summary), ("Notes", &book.notes), ("Critique", &book.review)] {
                if let Some(t) = text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                    ui.add_space(6.0);
                    ui.label(RichText::new(title).strong());
                    ui.label(t);
                }
            }
        });
        if let Some(from_start) = open {
            self.open_library_book(&ctx, &book.id, from_start);
        }
    }
}

fn details_grid(ui: &mut Ui, b: &LibraryBook) {
    let mut rows: Vec<(&str, String)> = Vec::new();
    let mut push = |k: &'static str, v: Option<String>| {
        if let Some(v) = v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
            rows.push((k, v));
        }
    };
    let s = |v: &Option<String>| v.clone();
    push("Série", s(&b.series));
    push(
        "Numéro",
        b.number.clone().map(|n| match b.count.filter(|c| *c > 0) {
            Some(c) => format!("{n} sur {c}"),
            None => n,
        }),
    );
    push("Volume", b.volume.filter(|v| *v >= 0).map(|v| v.to_string()));
    push(
        "Autre série",
        b.alternate_series.clone().map(|a| match &b.alternate_number {
            Some(n) if !n.is_empty() => format!("{a} #{n}"),
            _ => a,
        }),
    );
    push("Arc", s(&b.story_arc));
    push("Groupe de séries", s(&b.series_group));
    let published = match (b.day.filter(|d| *d > 0), b.month.filter(|m| *m > 0), b.year.filter(|y| *y > 0)) {
        (Some(d), Some(m), Some(y)) => Some(format!("{d:02}/{m:02}/{y}")),
        (None, Some(m), Some(y)) => Some(format!("{m:02}/{y}")),
        (_, _, Some(y)) => Some(y.to_string()),
        _ => None,
    };
    push("Parution", published);
    push("Éditeur", s(&b.publisher));
    push("Collection", s(&b.imprint));
    push("Scénario", s(&b.writer));
    push("Dessin", s(&b.penciller));
    push("Encrage", s(&b.inker));
    push("Couleurs", s(&b.colorist));
    push("Lettrage", s(&b.letterer));
    push("Couverture", s(&b.cover_artist));
    push("Direction", s(&b.editor));
    push("Traduction", s(&b.translator));
    push("Genre", s(&b.genre));
    push("Format", s(&b.format));
    push("Langue", s(&b.language_iso));
    push("Public", s(&b.age_rating).filter(|a| a != "Unknown"));
    push("Noir et blanc", s(&b.black_and_white).filter(|v| v == "Yes").map(|_| "oui".into()));
    push("Manga", s(&b.manga).filter(|v| v.starts_with("Yes")).map(|_| "oui".into()));
    push("Personnages", s(&b.characters));
    push("Équipes", s(&b.teams));
    push("Principal", s(&b.main_character_or_team));
    push("Lieux", s(&b.locations));
    push("Étiquettes", s(&b.tags));
    push("Note", b.rating.filter(|r| *r > 0.0).map(|r| format!("{r:.1} / 5").replace('.', ",")));
    push("Note communauté", b.community_rating.filter(|r| *r > 0.0).map(|r| format!("{r:.1} / 5").replace('.', ",")));
    push("Pages", b.page_count.map(|p| p.to_string()));
    push(
        "Lecture",
        b.read_percent().map(|p| match b.current_page {
            Some(c) if p > 0 => format!("{p} % (page {})", c + 1),
            _ => format!("{p} %"),
        }),
    );
    push("Ouvert", b.open_count.filter(|n| *n > 0).map(|n| format!("{n} fois")));
    push("Dernière lecture", Some(long_date(b.opened.as_deref())));
    push("Ajouté le", Some(short_date(b.added.as_deref())));
    push("Sorti le", Some(short_date(b.released.as_deref())));
    push("Taille", b.file_size.filter(|s| *s > 0).map(human_size));
    push("Scan", s(&b.scan_information));
    push("Fichier", s(&b.file));
    egui::Grid::new("fiche").num_columns(2).spacing(vec2(10.0, 4.0)).striped(false).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(RichText::new(k).weak());
            ui.add(egui::Label::new(v).wrap());
            ui.end_row();
        }
        if let Some(web) = b.web.as_deref().map(str::trim).filter(|w| w.starts_with("http")) {
            ui.label(RichText::new("Web").weak());
            ui.hyperlink_to(web, web);
            ui.end_row();
        }
    });
}
