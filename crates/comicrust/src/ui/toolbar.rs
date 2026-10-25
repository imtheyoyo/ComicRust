//! Barre d'outils : choix de la vue (Bibliothèque / Lecteur), ouverture, commandes du lecteur.

use eframe::egui::{self, Align, Layout, Ui};

use crate::app::{ComicRustApp, Mode};
use crate::settings::FitMode;

impl ComicRustApp {
    pub(crate) fn toolbar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.mode, Mode::Library, "📚 Bibliothèque");
            ui.selectable_value(&mut self.mode, Mode::Reader, "📖 Lecteur");
            ui.separator();
            if ui.button("📂 Ouvrir").on_hover_text("Ouvrir une BD ou une liste .cbl (Ctrl+O)").clicked() {
                self.open_dialog(&ctx);
            }
            ui.menu_button("🕘 Récents", |ui| {
                if self.settings.recent.is_empty() {
                    ui.label("Aucun fichier récent");
                }
                let mut chosen = None;
                for p in &self.settings.recent {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    if ui.button(name.as_ref()).on_hover_text(p.display().to_string()).clicked() {
                        chosen = Some(p.clone());
                    }
                }
                ui.separator();
                if ui.button("📁 Ouvrir un dossier d'images…").clicked() {
                    self.open_folder_dialog(&ctx);
                    ui.close();
                }
                if let Some(p) = chosen {
                    self.open_path(&ctx, p);
                    ui.close();
                }
            });
            ui.separator();

            if self.mode == Mode::Reader {
                if self.book.is_some() && ui.button("✖ Fermer").on_hover_text("Fermer le livre (Ctrl+W)").clicked() {
                    self.close_book(&ctx);
                    return;
                }
                let has_book = self.book.as_ref().is_some_and(|b| b.opened && b.page_count() > 0);
                ui.add_enabled_ui(has_book, |ui| {
                    let (first, prev, next, last) =
                        if self.settings.manga { ("⏭", "▶", "◀", "⏮") } else { ("⏮", "◀", "▶", "⏭") };
                    if ui.button(first).on_hover_text("Première page (Début)").clicked() {
                        self.go_to(0);
                    }
                    if ui.button(prev).on_hover_text("Page précédente").clicked() {
                        self.prev_page(&ctx);
                    }
                    if let Some(book) = &self.book {
                        let n = book.page_count().max(1);
                        let mut page = book.current + 1;
                        let resp = ui.add(egui::Slider::new(&mut page, 1..=n).show_value(false).trailing_fill(true));
                        ui.label(format!("{} / {}", book.current + 1, book.page_count()));
                        if resp.changed() {
                            self.go_to(page - 1);
                        }
                    }
                    if ui.button(next).on_hover_text("Page suivante").clicked() {
                        self.next_page(&ctx);
                    }
                    if ui.button(last).on_hover_text("Dernière page (Fin)").clicked() {
                        self.go_to(usize::MAX);
                    }
                });
                ui.separator();

                egui::ComboBox::from_id_salt("fit").selected_text(self.settings.fit.label()).show_ui(ui, |ui| {
                    for (i, m) in FitMode::ALL.into_iter().enumerate() {
                        ui.selectable_value(&mut self.settings.fit, m, format!("{}  ({})", m.label(), i + 1));
                    }
                });
                if ui.button("➖").on_hover_text("Zoom arrière (-)").clicked() {
                    self.zoom = (self.zoom / 1.25).max(0.1);
                }
                if ui.button(format!("{:.0} %", self.zoom * 100.0)).on_hover_text("Zoom 100 % (0)").clicked() {
                    self.zoom = 1.0;
                }
                if ui.button("➕").on_hover_text("Zoom avant (+)").clicked() {
                    self.zoom = (self.zoom * 1.25).min(8.0);
                }
                ui.separator();
                ui.toggle_value(&mut self.settings.double_page, "📖 Double page").on_hover_text("D");
                ui.toggle_value(&mut self.settings.manga, "🈁 Manga").on_hover_text("Lecture de droite à gauche (M)");
                ui.toggle_value(&mut self.settings.show_thumbnails, "▦ Vignettes").on_hover_text("T");
                ui.toggle_value(&mut self.show_info, "ℹ Infos").on_hover_text("I");
                if ui.button("⛶ Plein écran").on_hover_text("F11 / F, Échap pour sortir").clicked() {
                    self.toggle_fullscreen(&ctx);
                }
            }
            if self.list.is_some() || self.list_loading.is_some() {
                ui.toggle_value(&mut self.show_list_panel, "☰ Liste").on_hover_text("L");
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("?").on_hover_text("À propos et raccourcis").clicked() {
                    self.show_about = true;
                }
                ui.menu_button("⚙", |ui| {
                    if ui
                        .button("Assistant de configuration…")
                        .on_hover_text("ComicRust autonome ou avec ComicRack, base, dossiers de BD")
                        .clicked()
                    {
                        self.open_setup();
                        ui.close();
                    }
                    if ui
                        .button("Transférer la bibliothèque ComicRack…")
                        .on_hover_text(
                            "Copier livres, lectures, listes et dossiers de ComicRack dans la base de ComicRust",
                        )
                        .clicked()
                    {
                        self.open_transfer();
                        ui.close();
                    }
                    if ui.button("Rechercher une mise à jour…").clicked() {
                        self.update.check(&ctx, true);
                        ui.close();
                    }
                    if let Some(mode) = self.library_mode_label() {
                        ui.label(egui::RichText::new(mode).weak().small())
                            .on_hover_text("Assistant de configuration pour changer");
                    }
                    if self.settings.library_mode.is_comicrack()
                        && ui
                            .checkbox(&mut self.settings.auto_refresh, "Suivre les modifications faites dans ComicRack")
                            .on_hover_text("La bibliothèque se met à jour seule quand ComicRack modifie la base")
                            .changed()
                    {
                        self.restart_watch(&ctx);
                    }
                    ui.menu_button(format!("Thème : {}", self.settings.theme.label()), |ui| {
                        for t in crate::ui::theme::Theme::ALL {
                            if ui.radio_value(&mut self.settings.theme, t, t.label()).changed() {
                                t.apply(&ctx);
                            }
                        }
                    });
                    ui.menu_button(format!("Reprise de lecture : {}", self.settings.resume.label()), |ui| {
                        use crate::settings::ResumeChoice;
                        ui.label(
                            egui::RichText::new("Quand ComicRack et ComicRust n'ont pas retenu la même page")
                                .weak()
                                .small(),
                        );
                        for c in [ResumeChoice::Ask, ResumeChoice::ComicRack, ResumeChoice::ComicRust] {
                            ui.radio_value(&mut self.settings.resume, c, c.label());
                        }
                    });
                    ui.checkbox(&mut self.settings.check_updates, "Vérifier les mises à jour au démarrage");
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("Données : {}", crate::paths::exe_dir().join("data").display()))
                            .weak()
                            .small(),
                    );
                });
            });
        });
    }
}
