//! Fenêtres : informations du livre, à propos, erreurs.

use eframe::egui::{self, Color32, Context, RichText};

use crate::app::ComicRustApp;

impl ComicRustApp {
    pub(crate) fn info_window(&mut self, ctx: &Context) {
        let Some(book) = &self.book else { return };
        let mut open = self.show_info;
        egui::Window::new("Informations").open(&mut open).default_width(360.0).show(ctx, |ui| {
            egui::Grid::new("info").num_columns(2).striped(true).show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    if !v.is_empty() {
                        ui.label(RichText::new(k).strong());
                        ui.label(v);
                        ui.end_row();
                    }
                };
                row("Fichier", book.path.display().to_string());
                row("Format", book.format.map(|f| f.label().to_string()).unwrap_or_default());
                row("Pages", book.page_count().to_string());
                if let Some(ci) = &book.info {
                    let o = |v: &Option<String>| v.clone().unwrap_or_default();
                    let n = |v: Option<i32>| v.map(|x| x.to_string()).unwrap_or_default();
                    row("Série", o(&ci.series));
                    row("Numéro", o(&ci.number));
                    row("Volume", n(ci.volume));
                    row("Titre", o(&ci.title));
                    row("Année", n(ci.year));
                    row("Scénario", o(&ci.writer));
                    row("Dessin", o(&ci.penciller));
                    row("Encrage", o(&ci.inker));
                    row("Couleurs", o(&ci.colorist));
                    row("Couverture", o(&ci.cover_artist));
                    row("Éditeur", o(&ci.publisher));
                    row("Genre", o(&ci.genre));
                    row("Personnages", o(&ci.characters));
                    row("Arc", o(&ci.story_arc));
                    row("Langue", o(&ci.language_iso));
                    row("Manga", o(&ci.manga));
                }
            });
            if let Some(summary) = book.info.as_ref().and_then(|c| c.summary.clone()) {
                ui.separator();
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| ui.label(summary));
            }
        });
        self.show_info = open;
    }

    pub(crate) fn about_window(&mut self, ctx: &Context) {
        let mut open = self.show_about;
        egui::Window::new("À propos de ComicRust").open(&mut open).collapsible(false).resizable(false).show(
            ctx,
            |ui| {
                ui.label(RichText::new(format!("ComicRust {}", env!("CARGO_PKG_VERSION"))).strong());
                ui.label("Lecteur de BD en Rust, inspiré de ComicRack.");
                ui.hyperlink("https://github.com/imtheyoyo/ComicRust");
                if ui.button("Rechercher une mise à jour").clicked() {
                    self.update.check(ctx, true);
                }
                ui.label(
                    RichText::new(format!(
                        "Version portable — données : {}",
                        crate::paths::exe_dir().join("data").display()
                    ))
                    .weak()
                    .small(),
                );
                ui.separator();
                match comicrust_core::providers::pdfium_status() {
                    Ok(()) => ui.label("PDF : PDFium chargé ✔"),
                    Err(e) => ui.label(RichText::new(format!("PDF : {e}")).color(Color32::LIGHT_RED)),
                };
                ui.separator();
                egui::Grid::new("keys").num_columns(2).show(ui, |ui| {
                    for (k, v) in [
                        ("→ / Espace / Pg↓", "Page suivante"),
                        ("← / Retour arrière / Pg↑", "Page précédente"),
                        ("Début / Fin", "Première / dernière page"),
                        ("Clic à gauche / à droite", "Page précédente / suivante"),
                        ("Molette", "Changer de page (mode page entière)"),
                        ("1 2 3 4", "Page entière, largeur, hauteur, taille réelle"),
                        ("+ / - / 0", "Zoom"),
                        ("D", "Double page"),
                        ("M", "Sens manga"),
                        ("T / L / I", "Vignettes / liste / infos"),
                        ("F11 ou F, Échap", "Plein écran"),
                        ("Ctrl+O", "Ouvrir"),
                    ] {
                        ui.label(RichText::new(k).monospace());
                        ui.label(v);
                        ui.end_row();
                    }
                });
            },
        );
        self.show_about = open;
    }

    pub(crate) fn error_window(&mut self, ctx: &Context) {
        let Some(msg) = self.error.clone() else { return };
        let mut close = false;
        egui::Modal::new(egui::Id::unique("comicrust-error")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.heading("⚠ Erreur");
            ui.label(msg);
            ui.add_space(8.0);
            if ui.button("OK").clicked() {
                close = true;
            }
        });
        if close {
            self.error = None;
        }
    }
}
