//! Panneau de la liste de lecture `.cbl`.

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText, Ui};

use crate::app::ComicRustApp;

#[derive(Debug)]
pub(crate) enum ListAction {
    Open(PathBuf),
    PickRoot,
    PickDb,
}

impl ComicRustApp {
    pub(crate) fn list_panel(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.list_loading.is_some() {
            let (done, total) = self.list_progress.get();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Recherche des fichiers de la liste…");
            });
            if total > 0 {
                ui.add(
                    egui::ProgressBar::new(done as f32 / total as f32).text(format!("{done} / {total} fichiers lus")),
                );
            } else {
                ui.label(RichText::new("Parcours des dossiers…").weak());
            }
            ctx.request_repaint_after(Duration::from_millis(150));
            return;
        }
        let Some(list) = &self.list else { return };
        ui.heading(&list.name);
        ui.label(format!("{} / {} numéros trouvés", list.found(), list.entries.len()));
        ui.label(RichText::new(&list.source).weak().small());
        if let Some(summary) = &list.index_summary {
            ui.label(RichText::new(summary).weak().small());
        }
        if list.found() == 0 && list.default_root {
            ui.label(
                RichText::new(
                    "Aucun fichier trouvé dans le dossier de la liste : choisis le dossier où sont rangées tes BD.",
                )
                .color(Color32::from_rgb(230, 170, 60)),
            );
        }
        let mut action = None::<ListAction>;
        ui.horizontal_wrapped(|ui| {
            if ui.button("📁 Dossier des BD…").on_hover_text("Dossier parcouru pour retrouver les fichiers").clicked()
            {
                action = Some(ListAction::PickRoot);
            }
            if ui.button("🗄 ComicDB.xml…").on_hover_text("Base ComicRack (optionnelle)").clicked() {
                action = Some(ListAction::PickDb);
            }
            if ui
                .button("📋 Copier le diagnostic")
                .on_hover_text("Bilan, numéros manquants et fichiers non reconnus")
                .clicked()
            {
                ui.ctx().copy_text(list.diagnostic());
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, e) in list.entries.iter().enumerate() {
                let text = format!("{:>3}. {}", i + 1, e.label());
                let rich =
                    if e.file.is_some() { RichText::new(text) } else { RichText::new(text).weak().strikethrough() };
                let resp = ui.selectable_label(list.current == Some(i), rich);
                let resp = match &e.file {
                    Some(f) => resp.on_hover_text(f.display().to_string()),
                    None => resp.on_hover_text("Fichier introuvable"),
                };
                if resp.clicked() {
                    if let Some(f) = &e.file {
                        action = Some(ListAction::Open(f.clone()));
                    }
                }
            }
        });
        match action {
            Some(ListAction::Open(f)) => self.open_book(&ctx, f),
            Some(ListAction::PickRoot) => {
                if let Some(d) = rfd::FileDialog::new().set_title("Dossier des BD").pick_folder() {
                    self.settings.comics_root = Some(d);
                    self.reload_list(&ctx);
                }
            }
            Some(ListAction::PickDb) => {
                if let Some(f) = rfd::FileDialog::new().add_filter("ComicDB.xml", &["xml"]).pick_file() {
                    self.settings.library_db = Some(f);
                    self.reload_list(&ctx);
                }
            }
            None => {}
        }
    }
}
