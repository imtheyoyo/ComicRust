//! Fenêtres des travaux sur les fichiers : confirmation, avancement et bilan.

use std::sync::atomic::Ordering;

use eframe::egui::{self, Context, RichText, Ui};

use crate::app::ComicRustApp;
use crate::library::files::{Done, FileAction, FileDialog, FileJob, Target, ORIGINALS_DIR};

impl ComicRustApp {
    /// Prépare la confirmation d'une action sur des livres de la base.
    pub(crate) fn ask_file_action(&mut self, action: FileAction, ids: &[String]) {
        let Some(db) = self.library.database() else { return };
        let mut targets = Vec::new();
        let mut ignored = 0;
        for id in ids {
            let Some(book) = db.get(id) else { continue };
            match book.file.as_deref().map(|f| self.settings.effective_path(std::path::Path::new(f))) {
                Some(path) if action.accepts(&path) => targets.push(Target::of(book, path)),
                _ => ignored += 1,
            }
        }
        self.file_dialog =
            Some(FileDialog { action, targets, ignored, move_original: self.settings.move_converted_original });
    }

    /// Identifiants des livres affichés (liste ou bibliothèque, recherche comprise).
    pub(crate) fn shown_ids(&mut self) -> Vec<String> {
        let Some(db) = self.library.database() else { return Vec::new() };
        let opts = self.settings.view_options();
        self.library.view(&opts).iter().map(|&i| db.books[i].id.clone()).collect()
    }

    /// Entrées du menu « Fichiers » pour un livre (menu contextuel d'une couverture).
    pub(crate) fn book_file_menu(
        ui: &mut Ui,
        settings: &crate::settings::Settings,
        id: &str,
        file: Option<&str>,
        request: &mut Option<(FileAction, String)>,
    ) {
        // Chemin réel (CBZ converti si l'original a été rangé) ; calculé seulement menu ouvert.
        let path = file.map(|f| settings.effective_path(std::path::Path::new(f))).unwrap_or_default();
        if ui
            .add_enabled(FileAction::WriteInfo.accepts(&path), egui::Button::new("📝 Écrire les infos dans le fichier"))
            .on_disabled_hover_text("Seuls les CBZ reçoivent les infos (convertir d'abord en CBZ)")
            .clicked()
        {
            *request = Some((FileAction::WriteInfo, id.to_string()));
            ui.close();
        }
        if ui
            .add_enabled(FileAction::Convert.accepts(&path), egui::Button::new("📦 Convertir en CBZ"))
            .on_disabled_hover_text("Seuls les CBR, CB7 et CBT se convertissent")
            .clicked()
        {
            *request = Some((FileAction::Convert, id.to_string()));
            ui.close();
        }
    }

    /// Menu « 🛠 Fichiers » de la barre de la bibliothèque.
    pub(crate) fn files_menu(&mut self, ui: &mut Ui) {
        let busy = self.file_job.as_ref().is_some_and(|j| !j.is_finished());
        ui.add_enabled_ui(!busy, |ui| {
            ui.menu_button("🛠 Fichiers", |ui| {
                let note = if self.comicrack_write_config().is_some() {
                    "Fichiers BD modifiés ; leur fiche ComicRack est mise à jour."
                } else {
                    "Seuls les fichiers BD sont modifiés."
                };
                ui.label(RichText::new(note).weak().small());
                let selected = self.library.selected_book.clone();
                let n_shown = self.library.shown();
                ui.label(RichText::new("Livre sélectionné").strong());
                ui.add_enabled_ui(selected.is_some(), |ui| {
                    if ui.button("📝 Écrire les infos dans le fichier").clicked() {
                        self.ask_file_action(FileAction::WriteInfo, &[selected.clone().unwrap_or_default()]);
                        ui.close();
                    }
                    if ui.button("📦 Convertir en CBZ").clicked() {
                        self.ask_file_action(FileAction::Convert, &[selected.clone().unwrap_or_default()]);
                        ui.close();
                    }
                });
                ui.separator();
                ui.label(RichText::new(format!("Livres affichés ({n_shown})")).strong());
                if ui.button("📝 Écrire les infos dans les fichiers…").clicked() {
                    let ids = self.shown_ids();
                    self.ask_file_action(FileAction::WriteInfo, &ids);
                    ui.close();
                }
                if ui.button("📦 Convertir les CBR / CB7 / CBT en CBZ…").clicked() {
                    let ids = self.shown_ids();
                    self.ask_file_action(FileAction::Convert, &ids);
                    ui.close();
                }
                if self.file_job.is_some() {
                    ui.separator();
                    if ui.button("📋 Revoir le dernier bilan").clicked() {
                        self.show_file_job = true;
                        ui.close();
                    }
                }
            });
        });
    }

    /// Résultats du travail en cours : mise à jour des réglages et de la bibliothèque.
    pub(crate) fn poll_files(&mut self) {
        let Some(job) = &mut self.file_job else { return };
        let new = job.poll();
        let standalone = self.library.standalone().cloned();
        let comicrack_write = self.comicrack_write_config().is_some();
        let mut written = false;
        let mut store_updates: Vec<(String, Done)> = Vec::new();
        let mut comicrack_updates: Vec<(String, Done, std::path::PathBuf)> = Vec::new();
        for r in new {
            let Ok(done) = r.outcome else { continue };
            if standalone.is_some() {
                // Base de ComicRust : la fiche elle-même est mise à jour (pas d'astuce d'affichage).
                store_updates.push((r.target.id.clone(), done));
                continue;
            }
            if comicrack_write {
                // Base de ComicRack en écriture : la fiche est mise à jour (voir plus bas) ; les
                // réglages ci-dessous servent encore si l'écriture échoue.
                comicrack_updates.push((r.target.id.clone(), done.clone(), r.target.path.clone()));
            }
            match done {
                Done::Info { counter, .. } => {
                    self.settings.info_written.insert(r.target.id.to_ascii_lowercase(), counter);
                    written = true;
                }
                Done::Converted { cbz, .. } => {
                    if let Some(db) = self.library.database() {
                        if let Some(f) = db.get(&r.target.id).and_then(|b| b.file.clone()) {
                            self.settings.converted.insert(f.into(), cbz.clone());
                        }
                    }
                    if let Some(c) = &mut self.covers {
                        c.redirects = self.settings.converted.clone();
                    }
                }
            }
        }
        if written {
            self.library.apply_info_written(&mut self.settings.info_written);
        }
        if !comicrack_updates.is_empty() {
            self.write_files_to_comicrack(comicrack_updates);
        }
        if let (Some(cfg), false) = (standalone, store_updates.is_empty()) {
            // Fiches modifiées en arrière-plan (colonnes ciblées : la lecture n'est jamais
            // écrasée) ; la surveillance de la base les ramène à l'écran.
            let errors = self.bg_errors.clone();
            std::thread::spawn(move || {
                let run = || -> Result<(), String> {
                    let mut st = comicrust_core::store::Store::open_existing(&cfg).map_err(|e| e.to_string())?;
                    let mut written = Vec::new();
                    for (id, done) in store_updates {
                        match done {
                            Done::Info { .. } => written.push(id),
                            Done::Converted { cbz, .. } => {
                                let fresh = comicrust_core::scanner::book_from_file(&cbz, id.clone())
                                    .map_err(|e| format!("{} : {e}", cbz.display()))?;
                                st.replace_file(&id, &fresh).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    if !written.is_empty() {
                        st.set_comic_info_written(&written).map_err(|e| e.to_string())?;
                    }
                    Ok(())
                };
                if let Err(e) = run() {
                    errors.lock().unwrap().push(format!("Base ComicRust non mise à jour :\n{e}"));
                }
            });
        }
    }

    /// Lance un travail sur les fichiers (fiches lues dans la bibliothèque) ; `show` : montrer
    /// sa fenêtre (avancement, bilan). Aussi utilisé par les scripts (`write_info`).
    pub(crate) fn start_file_job(
        &mut self,
        ctx: &Context,
        action: FileAction,
        targets: Vec<Target>,
        move_original: bool,
        show: bool,
    ) {
        use crate::library::files::RecordSource;
        use crate::settings::LibraryMode;
        let source = match (self.settings.library_mode, self.library.database()) {
            (LibraryMode::ComicRackMySql, _) => match self.settings.db_source.config().ok().flatten() {
                Some(c) => RecordSource::ComicRackMySql(c),
                None => RecordSource::None,
            },
            (_, Some(db)) => RecordSource::Loaded(db),
            (_, None) => RecordSource::None,
        };
        self.file_job = Some(FileJob::start(action, targets, source, move_original, ctx));
        self.show_file_job = show;
    }

    pub(crate) fn files_windows(&mut self, ctx: &Context) {
        self.file_dialog_window(ctx);
        self.file_job_window(ctx);
    }

    fn file_dialog_window(&mut self, ctx: &Context) {
        let standalone = self.library.standalone().is_some();
        let comicrack_write = self.comicrack_write_config().is_some();
        let Some(d) = &mut self.file_dialog else { return };
        let mut go = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("comicrust-fichiers-confirmation")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading(d.action.title());
            ui.add_space(4.0);
            let n = d.targets.len();
            match d.action {
                FileAction::WriteInfo => {
                    ui.label(format!(
                        "{n} livre{} au format CBZ : les infos de la bibliothèque (série, numéro, auteurs, \
                         résumé, pages…) seront écrites dans le ComicInfo.xml de chaque fichier.",
                        if n > 1 { "s" } else { "" }
                    ));
                    let note = if standalone {
                        "Les pages ne sont pas recompressées ; un fichier déjà à jour n'est pas touché. Les livres \
                         écrits sortent de « Fichiers à mettre à jour »."
                    } else if comicrack_write {
                        "Les pages ne sont pas recompressées ; un fichier déjà à jour n'est pas touché. La fiche \
                         ComicRack de chaque livre est mise à jour (« Fichiers à mettre à jour » décoché, taille et \
                         date du fichier)."
                    } else {
                        "Les pages ne sont pas recompressées ; un fichier déjà à jour n'est pas touché. La base \
                         ComicRack n'est pas modifiée : ComicRack garde ces livres dans « Fichiers à mettre à jour », \
                         ComicRust ne les y montre plus."
                    };
                    ui.label(RichText::new(note).weak());
                }
                FileAction::Convert => {
                    ui.label(format!(
                        "{n} livre{} CBR / CB7 / CBT : un fichier .cbz est créé à côté de {}, avec les mêmes \
                         pages et les infos de la bibliothèque.",
                        if n > 1 { "s" } else { "" },
                        if n > 1 { "chacun" } else { "l'original" }
                    ));
                    ui.add_space(4.0);
                    ui.radio_value(&mut d.move_original, false, "Garder l'original à sa place");
                    ui.radio_value(
                        &mut d.move_original,
                        true,
                        format!("Ranger l'original dans le sous-dossier « {ORIGINALS_DIR} »"),
                    );
                    if standalone {
                        ui.label(
                            RichText::new(
                                "La fiche du livre indiquera le CBZ (lecture et infos gardées). Un original gardé à sa \
                                 place sera vu comme un nouveau livre à la prochaine analyse des dossiers.",
                            )
                            .weak(),
                        );
                    } else if comicrack_write {
                        ui.label(
                            RichText::new(
                                "La fiche ComicRack du livre indiquera le CBZ (lecture et infos gardées). Un original \
                                 gardé à sa place pourra être ajouté comme nouveau livre par ComicRack s'il analyse \
                                 ce dossier.",
                            )
                            .weak(),
                        );
                    } else {
                        ui.label(
                            RichText::new(
                                "⚠ La base ComicRack (lecture seule) indique toujours l'ancien fichier. ComicRust \
                                 ouvrira le CBZ si l'original n'est plus là ; dans ComicRack, il faudra ajouter le \
                                 CBZ à la bibliothèque.",
                            )
                            .color(crate::ui::theme::warning(ui)),
                        );
                    }
                }
            }
            if d.ignored > 0 {
                ui.label(
                    RichText::new(format!(
                        "{} livre{} ignoré{} (format qui ne convient pas ou sans fichier).",
                        d.ignored,
                        if d.ignored > 1 { "s" } else { "" },
                        if d.ignored > 1 { "s" } else { "" }
                    ))
                    .weak(),
                );
            }
            if n > 0 && n <= 5 {
                for t in &d.targets {
                    ui.label(RichText::new(format!("• {}", t.caption)).small());
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(n > 0, egui::Button::new(RichText::new("Lancer").strong())).clicked() {
                    go = true;
                }
                if ui.button("Annuler").clicked() {
                    cancel = true;
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if go {
            let d = self.file_dialog.take().unwrap();
            self.settings.move_converted_original = d.move_original;
            self.start_file_job(ctx, d.action, d.targets, d.move_original, true);
        } else if cancel {
            self.file_dialog = None;
        }
    }

    fn file_job_window(&mut self, ctx: &Context) {
        if !self.show_file_job {
            return;
        }
        let Some(job) = &self.file_job else { return };
        let mut open = true;
        let mut copy = false;
        let finished = job.is_finished();
        egui::Window::new(job.action.title())
            .id(egui::Id::unique("comicrust-fichiers-travail"))
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let done = job.done.load(Ordering::Relaxed);
                let (ok, same, err) = job.counts();
                let frac = if job.total > 0 { done as f32 / job.total as f32 } else { 1.0 };
                ui.add(egui::ProgressBar::new(frac).text(format!("{done} / {}", job.total)));
                if !finished {
                    ui.label(RichText::new(job.current.lock().unwrap().clone()).weak());
                    if ui.button("⏹ Arrêter").clicked() {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                } else {
                    let verb = if job.action == FileAction::Convert { "converti(s)" } else { "écrit(s)" };
                    let mut summary = format!("Terminé : {ok} {verb}");
                    if job.action == FileAction::WriteInfo {
                        summary.push_str(&format!(", {same} déjà à jour"));
                    }
                    summary.push_str(&format!(", {err} erreur(s)."));
                    ui.label(RichText::new(summary).strong());
                }
                if let Some(e) = job.db_error.lock().unwrap().as_deref() {
                    ui.label(RichText::new(format!("Bibliothèque : {e}")).color(crate::ui::theme::warning(ui)));
                }
                let errors: Vec<_> = job.results.iter().filter(|r| r.outcome.is_err()).collect();
                if !errors.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Erreurs").strong());
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for r in errors {
                            ui.label(RichText::new(&r.target.caption).strong());
                            ui.label(
                                RichText::new(r.outcome.as_ref().err().cloned().unwrap_or_default())
                                    .color(crate::ui::theme::error(ui)),
                            );
                            ui.label(RichText::new(r.target.path.display().to_string()).weak().small());
                        }
                    });
                }
                ui.separator();
                if ui.button("📋 Copier le bilan").clicked() {
                    copy = true;
                }
            });
        if copy {
            ctx.copy_text(job.report());
        }
        if !open {
            self.show_file_job = false;
        }
    }
}
