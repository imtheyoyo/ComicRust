//! Barre d'état (livre, format, page, taille).

use eframe::egui::{RichText, Ui};

use crate::app::ComicRustApp;

impl ComicRustApp {
    pub(crate) fn status_bar(&self, ui: &mut Ui) {
        ui.horizontal(|ui| match &self.book {
            None => {
                ui.label("Aucune BD ouverte — glissez un fichier dans la fenêtre ou cliquez sur « Ouvrir ».");
            }
            Some(b) if !b.opened => {
                ui.spinner();
                ui.label(format!("Ouverture de {}…", b.path.display()));
            }
            Some(b) => {
                ui.label(RichText::new(&b.title).strong());
                ui.separator();
                ui.label(b.format.map(|f| f.label()).unwrap_or(""));
                ui.separator();
                let visible = self.spread(b, b.current);
                let pages: Vec<String> = visible.iter().map(|i| (i + 1).to_string()).collect();
                ui.label(format!("Page {} sur {}", pages.join("-"), b.page_count()));
                if let Some(name) = b.pages.get(b.current) {
                    ui.separator();
                    ui.label(RichText::new(name).weak());
                }
                if let Some([w, h]) = b.sizes.get(&b.current) {
                    ui.separator();
                    ui.label(RichText::new(format!("{w} × {h}")).weak());
                }
                if !b.requested.is_empty() {
                    ui.separator();
                    ui.spinner();
                }
            }
        });
    }
}
