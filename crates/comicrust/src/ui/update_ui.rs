//! Bandeau « nouvelle version » et fenêtre de mise à jour.

use std::sync::atomic::Ordering;

use eframe::egui::{self, Context, RichText, Ui, ViewportCommand};

use crate::app::ComicRustApp;
use crate::update::{self, Status, CURRENT};

impl ComicRustApp {
    /// Bandeau en haut de la fenêtre quand une version plus récente est disponible.
    pub(crate) fn update_banner(&mut self, ui: &mut Ui) {
        let Status::Available(rel) = &self.update.status else { return };
        if self.update.banner_closed || self.settings.skipped_version.as_deref() == Some(rel.tag.as_str()) {
            return;
        }
        let rel = rel.clone();
        let ctx = ui.ctx().clone();
        egui::Panel::top("maj").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("⬆ {} est disponible", rel.name))
                        .strong()
                        .color(crate::ui::theme::warning(ui)),
                );
                ui.label(RichText::new(format!("(version actuelle : v{CURRENT})")).weak());
                if rel.zip.is_some() && ui.button("Installer").clicked() {
                    self.update.start_install(rel.clone(), &ctx);
                }
                if ui.button("Nouveautés…").clicked() {
                    self.update.window = true;
                }
                if ui.button("Plus tard").clicked() {
                    self.update.banner_closed = true;
                }
                if ui.button("Ignorer cette version").clicked() {
                    self.settings.skipped_version = Some(rel.tag.clone());
                }
            });
        });
    }

    pub(crate) fn update_window(&mut self, ctx: &Context) {
        if !self.update.window {
            return;
        }
        let mut open = true;
        let mut install = None;
        let mut restart = false;
        egui::Window::new("Mise à jour de ComicRust")
            .open(&mut open)
            .collapsible(false)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(format!("Version installée : v{CURRENT}"));
                let simulated = update::compared_version();
                if simulated != CURRENT {
                    ui.label(
                        RichText::new(format!("Mode recette : comparée aux releases comme v{simulated}"))
                            .color(crate::ui::theme::warning(ui)),
                    );
                }
                ui.checkbox(&mut self.settings.check_updates, "Vérifier les nouvelles versions au démarrage");
                ui.separator();
                match &self.update.status {
                    Status::Idle => {
                        ui.label("Aucune vérification en cours.");
                    }
                    Status::Checking { .. } => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Recherche d'une nouvelle version sur GitHub…");
                        });
                    }
                    Status::UpToDate => {
                        ui.label(RichText::new("✔ Tu as la dernière version.").color(crate::ui::theme::ok(ui)));
                    }
                    Status::Available(rel) => {
                        ui.label(RichText::new(format!("{} est disponible.", rel.name)).strong());
                        if !rel.notes.trim().is_empty() {
                            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| ui.label(&rel.notes));
                        }
                        ui.hyperlink_to("Page de la release sur GitHub", &rel.page_url);
                        ui.add_space(6.0);
                        if rel.zip.is_some() {
                            if ui.button("Télécharger et installer").clicked() {
                                install = Some(rel.clone());
                            }
                            ui.label(
                                RichText::new(
                                    "Le paquet est vérifié (SHA-256) avant d'installer. Tes réglages (dossier data) sont conservés.",
                                )
                                .weak()
                                .small(),
                            );
                        } else {
                            ui.label("Cette version n'a pas de paquet Windows : télécharge-la depuis GitHub.");
                        }
                    }
                    Status::Installing { release, done, .. } => {
                        let total = release.zip.as_ref().map(|z| z.1).unwrap_or(0);
                        let got = done.load(Ordering::Relaxed);
                        ui.label(format!("Installation de {}…", release.name));
                        let frac = if total > 0 { got as f32 / total as f32 } else { 0.0 };
                        ui.add(egui::ProgressBar::new(frac.min(1.0)).text(format!(
                            "{:.1} / {:.1} Mo",
                            got as f64 / 1e6,
                            total as f64 / 1e6
                        )));
                        ctx.request_repaint_after(std::time::Duration::from_millis(150));
                    }
                    Status::Installed(rel) => {
                        ui.label(
                            RichText::new(format!("✔ {} est installée. Redémarre ComicRust pour l'utiliser.", rel.name))
                                .color(crate::ui::theme::ok(ui)),
                        );
                        if ui.button("Redémarrer maintenant").clicked() {
                            restart = true;
                        }
                    }
                    Status::Failed(e) => {
                        ui.label(RichText::new(format!("✖ {e}")).color(crate::ui::theme::error(ui)));
                    }
                }
                ui.separator();
                let busy = matches!(self.update.status, Status::Checking { .. } | Status::Installing { .. });
                if ui.add_enabled(!busy, egui::Button::new("Rechercher une mise à jour")).clicked() {
                    self.update.check(ctx, true);
                }
            });
        if let Some(rel) = install {
            self.update.start_install(rel, ctx);
        }
        if restart {
            match update::restart() {
                Ok(()) => ctx.send_viewport_cmd(ViewportCommand::Close),
                Err(e) => self.update.status = Status::Failed(format!("redémarrage impossible : {e}")),
            }
        }
        if !open {
            self.update.window = false;
        }
    }
}
