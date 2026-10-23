//! Barre d'état (livre, format, page, taille).

use eframe::egui::{RichText, Ui};

use crate::app::{ComicRustApp, Mode};
use crate::library::Status;

impl ComicRustApp {
    pub(crate) fn status_bar(&self, ui: &mut Ui) {
        if self.mode == Mode::Library {
            ui.horizontal(|ui| match &self.library.status {
                Status::NotConfigured => {
                    ui.label("Bibliothèque non configurée (⚙ → Assistant de configuration).");
                }
                Status::Loading { what, .. } => {
                    ui.spinner();
                    ui.label(format!("Chargement de la {what}…"));
                }
                Status::Failed(_) => {
                    ui.label("Bibliothèque inaccessible.");
                }
                Status::Loaded(db) => {
                    ui.label(RichText::new(format!("{} livres", db.len())).strong());
                    ui.separator();
                    ui.label(format!("{} affichés", self.library_shown()));
                    if let Some(n) = self.library_stacks_shown() {
                        ui.label(format!("en {n} piles"));
                    }
                    if let Some(w) = &self.library.last_loaded_from {
                        ui.separator();
                        ui.label(RichText::new(w).weak());
                    }
                    if let Some(e) = &self.library.watch_error {
                        ui.separator();
                        ui.label(
                            RichText::new("⚠ base injoignable, nouvel essai…").color(crate::ui::theme::warning(ui)),
                        )
                        .on_hover_text(e);
                    } else if let Some(job) = &self.library.scan {
                        ui.separator();
                        ui.spinner();
                        let (done, total) = job.progress.get();
                        ui.label(format!("Analyse des dossiers : {done} / {total} fichiers"));
                    } else if self.library.watcher.is_some() && self.library.standalone().is_some() {
                        if let Some(scan) = &self.last_scan {
                            ui.separator();
                            ui.label(RichText::new(format!("dernière analyse : {}", scan.report.summary())).weak());
                        }
                    } else if self.library.watcher.is_some() && self.settings.auto_refresh {
                        ui.separator();
                        let text = match self.library.last_refresh {
                            Some(a) => {
                                let mut parts = Vec::new();
                                if a.added > 0 {
                                    parts.push(format!("{} ajouté{}", a.added, if a.added > 1 { "s" } else { "" }));
                                }
                                if a.modified > 0 {
                                    parts.push(format!(
                                        "{} modifié{}",
                                        a.modified,
                                        if a.modified > 1 { "s" } else { "" }
                                    ));
                                }
                                if a.removed > 0 {
                                    parts.push(format!("{} retiré{}", a.removed, if a.removed > 1 { "s" } else { "" }));
                                }
                                if parts.is_empty() {
                                    "↻ suivi de ComicRack".into()
                                } else {
                                    format!("↻ suivi de ComicRack, dernière modification : {}", parts.join(", "))
                                }
                            }
                            None => "↻ suivi de ComicRack".into(),
                        };
                        ui.label(RichText::new(text).weak()).on_hover_text(
                            "Les modifications faites dans ComicRack apparaissent ici en quelques secondes",
                        );
                    }
                    if db.unreadable > 0 {
                        ui.separator();
                        ui.label(RichText::new(format!("{} fiches illisibles ignorées", db.unreadable)).weak());
                    }
                }
            });
            return;
        }
        ui.horizontal(|ui| match &self.book {
            None => {
                ui.label("Aucune BD ouverte — glissez un fichier dans la fenêtre ou cliquez sur « Ouvrir ».");
            }
            Some(b) if !b.opened => {
                ui.spinner();
                ui.label(format!("Ouverture de {}…", b.path.display()));
                if comicrust_core::providers::is_being_rewritten(&b.path) {
                    ui.label(RichText::new("(ComicRack enregistre ce fichier, patience…)").weak());
                }
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
