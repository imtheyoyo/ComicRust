//! Fenêtre « Base ComicRack » : où lire les paramètres MySQL, test de connexion.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use comicrust_core::db::{test_connection, DbConfig};
use eframe::egui::{self, Color32, Context, RichText};

use crate::app::ComicRustApp;
use crate::settings::DbSource;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum SourceKind {
    Ini,
    Manual,
}

/// État de la fenêtre (le formulaire n'est enregistré qu'à la validation).
pub struct DbWindow {
    pub kind: SourceKind,
    pub ini_path: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub database: String,
    testing: Option<Receiver<Result<usize, String>>>,
    test_result: Option<Result<String, String>>,
}

impl DbWindow {
    pub fn from_source(src: &DbSource) -> Self {
        let mut w = Self {
            kind: SourceKind::Ini,
            ini_path: String::new(),
            host: String::new(),
            port: "3306".into(),
            user: String::new(),
            password: String::new(),
            database: String::new(),
            testing: None,
            test_result: None,
        };
        match src {
            DbSource::None => {}
            DbSource::ComicRackIni(p) => w.ini_path = p.display().to_string(),
            DbSource::Manual(c) => {
                w.kind = SourceKind::Manual;
                w.host = c.host.clone();
                w.port = c.port.to_string();
                w.user = c.user.clone();
                w.password = c.password.clone();
                w.database = c.database.clone();
            }
        }
        w
    }

    fn to_source(&self) -> Result<DbSource, String> {
        match self.kind {
            SourceKind::Ini => {
                let p = PathBuf::from(self.ini_path.trim().trim_matches('"'));
                DbConfig::from_comicrack_ini(&p).map_err(|e| e.to_string())?;
                Ok(DbSource::ComicRackIni(p))
            }
            SourceKind::Manual => {
                let cfg = DbConfig {
                    host: self.host.trim().to_string(),
                    port: self.port.trim().parse().map_err(|_| "port invalide".to_string())?,
                    user: self.user.trim().to_string(),
                    password: self.password.clone(),
                    database: self.database.trim().to_string(),
                };
                if !cfg.is_complete() {
                    return Err("serveur, utilisateur et base sont nécessaires".into());
                }
                Ok(DbSource::Manual(cfg))
            }
        }
    }
}

impl ComicRustApp {
    pub(crate) fn open_db_window(&mut self) {
        self.db_window = Some(DbWindow::from_source(&self.settings.db_source));
    }

    pub(crate) fn db_window_ui(&mut self, ctx: &Context) {
        let Some(w) = &mut self.db_window else { return };
        let mut open = true;
        let mut save: Option<DbSource> = None;
        let mut disable = false;
        if let Some(rx) = &w.testing {
            if let Ok(r) = rx.try_recv() {
                w.test_result = Some(r.map(|n| format!("Connexion réussie : {n} livres dans la base.")));
                w.testing = None;
            }
        }
        egui::Window::new("Base ComicRack (MySQL)")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("Lecture seule : ComicRust ne modifie jamais ta base.").weak());
                ui.add_space(6.0);
                ui.radio_value(&mut w.kind, SourceKind::Ini, "Lire les paramètres dans ComicRack.ini (recommandé)");
                ui.radio_value(&mut w.kind, SourceKind::Manual, "Saisir les paramètres");
                ui.add_space(6.0);
                match w.kind {
                    SourceKind::Ini => {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut w.ini_path)
                                    .hint_text("…\\ComicRack.ini")
                                    .desired_width(330.0),
                            );
                            if ui.button("Parcourir…").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_title("ComicRack.ini")
                                    .add_filter("ComicRack.ini", &["ini"])
                                    .pick_file()
                                {
                                    w.ini_path = p.display().to_string();
                                }
                            }
                        });
                        let path = PathBuf::from(w.ini_path.trim().trim_matches('"'));
                        if !w.ini_path.trim().is_empty() {
                            match DbConfig::from_comicrack_ini(&path) {
                                Ok(c) => ui.label(RichText::new(format!("✔ {}", c.describe())).color(Color32::LIGHT_GREEN)),
                                Err(e) => ui.label(RichText::new(format!("✖ {e}")).color(Color32::LIGHT_RED)),
                            };
                        }
                        ui.label(
                            RichText::new(
                                "Le fichier est relu à chaque connexion : le mot de passe n'est pas recopié dans ComicRust.",
                            )
                            .weak()
                            .small(),
                        );
                    }
                    SourceKind::Manual => {
                        egui::Grid::new("db-champs").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            ui.label("Serveur");
                            ui.text_edit_singleline(&mut w.host);
                            ui.end_row();
                            ui.label("Port");
                            ui.add(egui::TextEdit::singleline(&mut w.port).desired_width(80.0));
                            ui.end_row();
                            ui.label("Utilisateur");
                            ui.text_edit_singleline(&mut w.user);
                            ui.end_row();
                            ui.label("Mot de passe");
                            ui.add(egui::TextEdit::singleline(&mut w.password).password(true));
                            ui.end_row();
                            ui.label("Base");
                            ui.text_edit_singleline(&mut w.database);
                            ui.end_row();
                        });
                        ui.label(
                            RichText::new("Le mot de passe est gardé dans data\\comicrust.ron, à côté de l'exe.")
                                .weak()
                                .small(),
                        );
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let busy = w.testing.is_some();
                    if ui.add_enabled(!busy, egui::Button::new("Tester la connexion")).clicked() {
                        match w.to_source().and_then(|s| s.config()) {
                            Ok(Some(cfg)) => {
                                let (tx, rx) = mpsc::channel();
                                let ctx = ui.ctx().clone();
                                std::thread::spawn(move || {
                                    let _ = tx.send(test_connection(&cfg).map_err(|e| e.to_string()));
                                    ctx.request_repaint();
                                });
                                w.testing = Some(rx);
                                w.test_result = None;
                            }
                            Ok(None) => {}
                            Err(e) => w.test_result = Some(Err(e)),
                        }
                    }
                    if busy {
                        ui.spinner();
                    }
                    if ui.button("Enregistrer et charger").clicked() {
                        match w.to_source() {
                            Ok(s) => save = Some(s),
                            Err(e) => w.test_result = Some(Err(e)),
                        }
                    }
                    if self.settings.db_source != DbSource::None && ui.button("Ne plus utiliser de base").clicked() {
                        disable = true;
                    }
                });
                match &w.test_result {
                    Some(Ok(m)) => {
                        ui.label(RichText::new(format!("✔ {m}")).color(Color32::LIGHT_GREEN));
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(format!("✖ {e}")).color(Color32::LIGHT_RED));
                    }
                    None => {}
                }
            });
        if let Some(src) = save {
            self.settings.db_source = src;
            self.db_window = None;
            self.load_library(ctx);
            self.mode = crate::app::Mode::Library;
        } else if disable {
            self.settings.db_source = DbSource::None;
            self.library = Default::default();
            self.db_window = None;
        } else if !open {
            self.db_window = None;
        }
    }
}
