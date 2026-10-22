//! Vue Bibliothèque en tableau : une ligne par livre, colonnes triables d'un clic sur
//! leur titre.

use comicrust_core::library::LibraryBook;
use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::app::ComicRustApp;
use crate::library::SortKey;

/// `2019-10-01T12:58:00.123+02:00` → `01/10/2019`.
pub(crate) fn short_date(iso: Option<&str>) -> String {
    let Some(s) = iso.filter(|s| s.len() >= 10 && !s.starts_with("0001-01-01")) else { return String::new() };
    let (y, m, d) = (&s[0..4], &s[5..7], &s[8..10]);
    format!("{d}/{m}/{y}")
}

/// `2019-10-01T12:58:00…` → `01/10/2019 12:58`.
pub(crate) fn long_date(iso: Option<&str>) -> String {
    let d = short_date(iso);
    match iso {
        Some(s) if !d.is_empty() && s.len() >= 16 => format!("{d} {}", &s[11..16]),
        _ => d,
    }
}

pub(crate) fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    let s = if b >= 1e9 {
        format!("{:.2} Go", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} Mo", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} Ko", b / 1e3)
    } else {
        format!("{bytes} o")
    };
    s.replace('.', ",")
}

fn stars(r: Option<f32>) -> String {
    match r.filter(|r| *r > 0.0) {
        Some(r) => {
            let full = r.round().clamp(0.0, 5.0) as usize;
            format!("{}{}", "★".repeat(full), "☆".repeat(5 - full))
        }
        None => String::new(),
    }
}

struct Col {
    title: &'static str,
    sort: Option<SortKey>,
    width: f32,
    cell: fn(&LibraryBook) -> String,
}

const COLS: [Col; 11] = [
    Col { title: "Série", sort: Some(SortKey::Series), width: 220.0, cell: |b| b.series.clone().unwrap_or_default() },
    Col {
        title: "N°",
        sort: None,
        width: 50.0,
        cell: |b| {
            let n = b.number.clone().unwrap_or_default();
            match b.volume.filter(|v| *v >= 0) {
                Some(v) if !n.is_empty() => format!("v{v} #{n}"),
                Some(v) => format!("v{v}"),
                None if n.is_empty() => n,
                None => format!("#{n}"),
            }
        },
    },
    Col { title: "Titre", sort: Some(SortKey::Title), width: 220.0, cell: |b| b.title.clone().unwrap_or_default() },
    Col {
        title: "Parution",
        sort: Some(SortKey::Published),
        width: 70.0,
        cell: |b| match (b.year.filter(|y| *y > 0), b.month.filter(|m| *m > 0)) {
            (Some(y), Some(m)) => format!("{m:02}/{y}"),
            (Some(y), None) => y.to_string(),
            _ => String::new(),
        },
    },
    Col { title: "Éditeur", sort: None, width: 110.0, cell: |b| b.publisher.clone().unwrap_or_default() },
    Col { title: "Scénariste", sort: None, width: 140.0, cell: |b| b.writer.clone().unwrap_or_default() },
    Col {
        title: "Pages",
        sort: Some(SortKey::PageCount),
        width: 50.0,
        cell: |b| b.page_count.map(|p| p.to_string()).unwrap_or_default(),
    },
    Col {
        title: "Lu",
        sort: Some(SortKey::ReadPercent),
        width: 45.0,
        cell: |b| b.read_percent().filter(|p| *p > 0).map(|p| format!("{p} %")).unwrap_or_default(),
    },
    Col { title: "Note", sort: Some(SortKey::Rating), width: 70.0, cell: |b| stars(b.rating) },
    Col { title: "Ajouté", sort: Some(SortKey::Added), width: 80.0, cell: |b| short_date(b.added.as_deref()) },
    Col { title: "Fichier", sort: Some(SortKey::File), width: 300.0, cell: |b| b.file.clone().unwrap_or_default() },
];

impl ComicRustApp {
    pub(crate) fn library_table(&mut self, ui: &mut Ui) {
        let Some(db) = self.library.database() else { return };
        let opts = self.settings.view_options();
        let view = self.library.view(&opts).to_vec();
        if view.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label("Aucun livre ne correspond.");
            });
            return;
        }
        let in_list = self.library.reading_list_shown();
        let mut open = None;
        let mut select = None;
        let mut sort_by = None;
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        // Colonnes plus larges que la fenêtre : défilement horizontal.
        egui::ScrollArea::horizontal().id_salt("tableau-h").auto_shrink(false).show(ui, |ui| {
            let mut t = TableBuilder::new(ui)
                .id_salt("tableau-bibliotheque")
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            if in_list {
                t = t.column(Column::initial(40.0).at_least(30.0));
            }
            for c in &COLS[..COLS.len() - 1] {
                t = t.column(Column::initial(c.width).at_least(30.0).clip(true));
            }
            t = t.column(Column::initial(COLS[COLS.len() - 1].width).at_least(80.0).clip(true));
            t.header(row_h + 2.0, |mut h| {
                if in_list {
                    h.col(|ui| {
                        if ui.button(RichText::new("#").strong()).on_hover_text("Ordre de la liste").clicked() {
                            sort_by = Some(SortKey::Position);
                        }
                    });
                }
                for c in &COLS {
                    h.col(|ui| {
                        let mut title = c.title.to_string();
                        if c.sort == Some(opts.sort) {
                            title.push_str(if opts.descending { " ⬇" } else { " ⬆" });
                        }
                        match c.sort {
                            Some(k) => {
                                if ui.add(egui::Button::new(RichText::new(title).strong()).frame(false)).clicked() {
                                    sort_by = Some(k);
                                }
                            }
                            None => {
                                ui.label(RichText::new(title).strong());
                            }
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, view.len(), |mut row| {
                    let bi = view[row.index()];
                    let book = &db.books[bi];
                    row.set_selected(self.library.selected_book.as_deref() == Some(book.id.as_str()));
                    if in_list {
                        row.col(|ui| {
                            ui.label(self.library.position(bi).map(|p| (p + 1).to_string()).unwrap_or_default());
                        });
                    }
                    for c in &COLS {
                        row.col(|ui| {
                            // « Lu » : le plus avancé entre ComicRack et ComicRust.
                            let text = if c.sort == Some(SortKey::ReadPercent) {
                                self.read_percent(book).map(|p| format!("{p} %")).unwrap_or_default()
                            } else {
                                (c.cell)(book)
                            };
                            ui.label(text);
                        });
                    }
                    let resp = row.response();
                    if resp.double_clicked() {
                        open = Some(book.id.clone());
                    } else if resp.clicked() {
                        select = Some(book.id.clone());
                    }
                });
            });
        });
        if let Some(k) = sort_by {
            if self.settings.library_sort == k {
                self.settings.library_descending = !self.settings.library_descending;
            } else {
                self.settings.library_sort = k;
                self.settings.library_descending = false;
            }
        }
        if let Some(id) = select {
            self.library.selected_book = Some(id);
        }
        if let Some(id) = open {
            self.library.selected_book = Some(id.clone());
            self.open_library_book(ui.ctx(), &id, crate::app::Start::Auto);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_sizes() {
        assert_eq!(short_date(Some("2019-10-01T12:58:00.1234567+02:00")), "01/10/2019");
        assert_eq!(long_date(Some("2019-10-01T12:58:00")), "01/10/2019 12:58");
        assert_eq!(short_date(Some("0001-01-01T00:00:00")), "");
        assert_eq!(human_size(48_173_972), "48,2 Mo");
        assert_eq!(stars(Some(3.5)), "★★★★☆");
    }
}
