//! Interface des scripts Python (v0.10.0), sur le modèle de ComicRack :
//! - menu « 📜 Scripts » de la barre d'outils (≈ Fichier ▸ Automatisation) : scripts
//!   `Library` (toute la bibliothèque) et `Books` (sélection), dossier, rechargement,
//!   gestion ;
//! - menu contextuel des livres « 📜 Scripts » (≈ Automatisation du navigateur) : scripts
//!   `Books` sur la sélection ;
//! - « Gérer les scripts… » (≈ Préférences ▸ Scripts) : liste par hook avec cases
//!   d'activation, description, fichier, bouton « Réglages… » (fonction `ConfigScript`) ;
//! - fenêtre d'exécution non bloquante (avancement, journal, Arrêter), questions dans une
//!   petite fenêtre modale, puis **aperçu** des modifications avant application ;
//! - automatismes (`Startup`, `BookOpened`, `BookClosed`, `NewBooks`) en arrière-plan.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use comicrust_core::scripting::{Hook, JournalKind, QuestionKind, RunState, ScriptEntry};
use eframe::egui::{self, Context, RichText, Ui};

use crate::app::ComicRustApp;
use crate::scripts::apply::{ApplyOutcome, ApplyTarget};
use crate::scripts::{AutoEvent, Pending, ScriptChoices, ScriptsError};

/// Script `Books` proposé dans un menu : (identifiant, libellé, description).
pub type ScriptMenuItem = (String, String, Option<String>);

/// Sous-menu « 📜 Scripts » du menu contextuel des livres (scripts `Books` sur `books`).
pub(crate) fn book_scripts_menu(
    ui: &mut Ui,
    scripts: &[ScriptMenuItem],
    books: &[String],
    request: &mut Option<(String, Vec<String>)>,
) {
    if scripts.is_empty() {
        return;
    }
    ui.menu_button("📜 Scripts", |ui| {
        ui.label(RichText::new(format!("Sur {} livre(s)", books.len())).weak().small());
        for (id, name, description) in scripts {
            let mut b = ui.button(name);
            if let Some(d) = description {
                b = b.on_hover_text(d);
            }
            if b.clicked() {
                *request = Some((id.clone(), books.to_vec()));
                ui.close();
            }
        }
    });
}

/// Ouvre un dossier dans l'explorateur de fichiers.
fn open_folder(dir: &Path) -> Result<(), String> {
    let program = if cfg!(windows) { "explorer" } else { "xdg-open" };
    std::process::Command::new(program)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossible d'ouvrir {} : {e}", dir.display()))
}

/// Identifiant de la fenêtre d'exécution.
fn run_window_id() -> egui::Id {
    egui::Id::unique("comicrust-script-execution")
}

fn plural(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

impl ComicRustApp {
    /// Où appliquer les modifications des scripts.
    pub(crate) fn script_target(&self) -> ApplyTarget {
        ApplyTarget::of(self.settings.library_mode, self.settings.comicrack_write, self.library.source.as_ref())
    }

    /// Choix d'activation des scripts faits par l'utilisateur.
    pub(crate) fn script_choices(&self) -> ScriptChoices<'_> {
        ScriptChoices { disabled: &self.settings.scripts_disabled, enabled: &self.settings.scripts_enabled }
    }

    /// Scripts `Books` actifs, pour les menus contextuels.
    pub(crate) fn book_scripts(&self) -> Vec<ScriptMenuItem> {
        self.scripts
            .enabled(Hook::Books, &self.script_choices())
            .map(|e| (e.id(), e.name.clone(), e.description.clone()))
            .collect()
    }

    /// Lance le script `id` (hook `hook`) sur `books`.
    pub(crate) fn run_script(&mut self, ctx: &Context, id: &str, hook: Hook, books: Vec<String>) {
        if self.scripts.busy() {
            self.error =
                Some("Un script est déjà en cours : attends sa fin (ou arrête-le) avant d'en lancer un autre.".into());
            return;
        }
        if self.transfer_busy() {
            self.error = Some("Transfert de la bibliothèque en cours : les scripts attendront sa fin.".into());
            return;
        }
        let Some(entry) = self.scripts.entry(id).cloned() else { return };
        let db = match self.library.database() {
            Some(db) => db,
            None if hook == Hook::ConfigScript => std::sync::Arc::new(Default::default()),
            None => return,
        };
        let target = self.script_target();
        let sink = self.library.watcher.as_ref().map(|w| w.written.clone());
        match self.scripts.start_run(entry, hook, books, db, target, sink, ctx) {
            // Fenêtre de l'exécution au premier plan (devant « Gérer les scripts »).
            Ok(()) => ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, run_window_id())),
            Err(e) => self.error = Some(e),
        }
    }

    /// Réglages d'un script (sa fonction `ConfigScript`, interactive).
    fn configure_script(&mut self, ctx: &Context, entry: &ScriptEntry) {
        if let Some(cfg) = self.scripts.config_of(entry).map(|e| e.id()) {
            self.run_script(ctx, &cfg, Hook::ConfigScript, Vec::new());
        }
    }

    /// Menu « 📜 Scripts » de la barre d'outils.
    pub(crate) fn scripts_menu(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.menu_button("📜 Scripts", |ui| {
            let busy = self.scripts.busy() || self.transfer_busy();
            let library = self.library.database().is_some();
            let mut launch: Option<(String, Hook, Vec<String>)> = None;
            if self.transfer_busy() {
                ui.label(RichText::new("Transfert en cours : scripts indisponibles").weak().small());
            } else if self.scripts.busy() {
                ui.label(RichText::new("Un script est en cours").weak().small());
            }
            let lib: Vec<ScriptMenuItem> = self
                .scripts
                .enabled(Hook::Library, &self.script_choices())
                .map(|e| (e.id(), e.name.clone(), e.description.clone()))
                .collect();
            let books = self.book_scripts();
            if lib.is_empty() && books.is_empty() {
                ui.label(RichText::new("Aucun script : place des fichiers .py dans le dossier des scripts.").weak());
            }
            if !lib.is_empty() {
                ui.label(RichText::new("Toute la bibliothèque").strong());
                ui.add_enabled_ui(!busy && library, |ui| {
                    for (id, name, d) in &lib {
                        let mut b = ui.button(name);
                        if let Some(d) = d {
                            b = b.on_hover_text(d);
                        }
                        if b.clicked() {
                            let all = self.library.database().map(|db| db.books.iter().map(|b| b.id.clone()).collect());
                            launch = Some((id.clone(), Hook::Library, all.unwrap_or_default()));
                            ui.close();
                        }
                    }
                });
            }
            if !books.is_empty() {
                let selection = self.library.selection();
                ui.label(RichText::new(format!("Livres sélectionnés ({})", selection.len())).strong());
                ui.add_enabled_ui(!busy && !selection.is_empty(), |ui| {
                    for (id, name, d) in &books {
                        let mut b = ui.button(name);
                        if let Some(d) = d {
                            b = b.on_hover_text(d);
                        }
                        if b.clicked() {
                            launch = Some((id.clone(), Hook::Books, selection.clone()));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            if ui
                .button("📂 Ouvrir le dossier des scripts")
                .on_hover_text(self.scripts.dir.display().to_string())
                .clicked()
            {
                let _ = std::fs::create_dir_all(&self.scripts.dir);
                if let Err(e) = open_folder(&self.scripts.dir) {
                    self.error = Some(e);
                }
                ui.close();
            }
            if ui.button("⟳ Recharger les scripts").clicked() {
                self.scripts.reload();
                ui.close();
            }
            let warn = self.scripts.warnings.len();
            let manage = if warn > 0 {
                format!("⚙ Gérer les scripts… (⚠ {warn})")
            } else {
                "⚙ Gérer les scripts…".into()
            };
            if ui.button(manage).clicked() {
                self.scripts.show_manager = true;
                ui.close();
            }
            if ui
                .checkbox(&mut self.settings.scripts_auto, "Exécuter les automatismes")
                .on_hover_text(
                    "Scripts lancés tout seuls (démarrage, ouverture ou fermeture d'un livre, nouveaux livres) ; leurs \
                     modifications sont appliquées sans aperçu",
                )
                .changed()
                && !self.settings.scripts_auto
            {
                self.scripts.stop_autos();
            }
            if let Some((id, hook, books)) = launch {
                self.run_script(&ctx, &id, hook, books);
            }
        });
    }

    // -----------------------------------------------------------------------------------
    // Automatismes
    // -----------------------------------------------------------------------------------

    /// Déclenche les automatismes d'un hook (si activés).
    pub(crate) fn trigger_scripts(&mut self, hook: Hook, book_ids: Vec<String>) {
        if !self.settings.scripts_auto {
            return;
        }
        let (disabled, enabled) = (self.settings.scripts_disabled.clone(), self.settings.scripts_enabled.clone());
        self.scripts.trigger(hook, book_ids, &ScriptChoices { disabled: &disabled, enabled: &enabled });
    }

    /// Livre ouvert ou fermé dans le lecteur : automatismes `BookOpened` / `BookClosed`
    /// (livres de la bibliothèque seulement).
    pub(crate) fn script_book_event(&mut self, hook: Hook, path: &Path) {
        if !self.settings.scripts_auto {
            return;
        }
        let Some(db) = self.library.database() else { return };
        let p = path.to_string_lossy();
        let id = db
            .books
            .iter()
            .find(|x| {
                x.file.as_deref().is_some_and(|f| {
                    f.eq_ignore_ascii_case(&p) || self.settings.effective_path(Path::new(f)).as_path() == path
                })
            })
            .map(|b| b.id.clone());
        if let Some(id) = id {
            self.trigger_scripts(hook, vec![id]);
        }
    }

    /// Bibliothèque chargée : automatismes `Startup` (une fois par session).
    pub(crate) fn scripts_library_loaded(&mut self) {
        if !self.scripts.startup_done {
            self.scripts.startup_done = true;
            self.trigger_scripts(Hook::Startup, Vec::new());
        }
    }

    /// Message signalé une seule fois par session.
    fn warn_once(&mut self, key: String, message: String) {
        if self.scripts.warned.insert(key) {
            self.bg_errors.lock().unwrap().push(message);
        }
    }

    // -----------------------------------------------------------------------------------
    // Application des bilans
    // -----------------------------------------------------------------------------------

    /// Reporte un bilan d'application dans la bibliothèque affichée, puis lance l'écriture
    /// des infos dans les fichiers.
    fn apply_script_outcome(&mut self, outcome: &ApplyOutcome, show_job: bool) {
        let fresh = outcome.fresh_books();
        if let Some(c) = &mut self.covers {
            for b in &fresh {
                c.forget_failure(&b.id, b.front_cover_image());
            }
        }
        if self.library.standalone().is_some() {
            self.library.update_books(fresh, outcome.written());
        } else {
            let reports = outcome
                .edit
                .iter()
                .filter_map(|r| r.comicrack.as_ref())
                .chain(outcome.moves.iter().filter_map(|m| m.comicrack.as_ref()));
            for w in reports.collect::<Vec<_>>() {
                self.library.apply_written(w);
            }
        }
        if !outcome.write_info.is_empty() {
            self.scripts.pending_write_info.extend(outcome.write_info.iter().map(|id| (id.clone(), show_job)));
        }
    }

    /// Lance l'écriture des infos en attente si aucun travail « Fichiers » ne tourne.
    fn start_pending_write_info(&mut self, ctx: &Context) {
        if self.scripts.pending_write_info.is_empty() || self.file_job.as_ref().is_some_and(|j| !j.is_finished()) {
            return;
        }
        let Some(db) = self.library.database() else { return };
        let pending = std::mem::take(&mut self.scripts.pending_write_info);
        let show = pending.iter().any(|(_, s)| *s);
        let mut targets = Vec::new();
        for (id, _) in &pending {
            let Some(book) = db.get(id) else { continue };
            let Some(path) = book.file.as_deref().map(|f| self.settings.effective_path(Path::new(f))) else {
                continue;
            };
            if crate::library::files::FileAction::WriteInfo.accepts(&path)
                && !targets.iter().any(|t: &crate::library::files::Target| t.id == book.id)
            {
                targets.push(crate::library::files::Target::of(book, path));
            }
        }
        if !targets.is_empty() {
            self.start_file_job(ctx, crate::library::files::FileAction::WriteInfo, targets, false, show);
        }
    }

    /// À chaque image : réglages enregistrés, exécution en cours, automatismes.
    pub(crate) fn poll_scripts(&mut self, ctx: &Context) {
        if self.scripts.settings_dirty.swap(false, Ordering::SeqCst) {
            self.settings.script_settings = self.scripts.stored.lock().unwrap().clone();
            self.save_now = true;
        }
        // Exécution lancée d'un menu.
        let outcome = self.scripts.run.as_mut().and_then(|r| r.poll());
        if let Some(outcome) = outcome {
            self.apply_script_outcome(&outcome, true);
            let db = self.library.database();
            let caption =
                |id: &str| db.as_ref().and_then(|d| d.get(id)).map(|b| b.caption()).unwrap_or_else(|| id.to_string());
            let report = (outcome.summary(), outcome.problems(caption));
            if let Some(run) = &mut self.scripts.run {
                run.report = Some(report);
                run.preview = None;
            }
        }
        // Automatismes.
        let allowed = self.settings.scripts_auto && !self.transfer_busy() && self.scripts_closing.is_none();
        let target = self.script_target();
        let sink = self.library.watcher.as_ref().map(|w| w.written.clone());
        match self.scripts.start_autos(allowed, self.library.database(), &target, sink, ctx) {
            Ok(()) => {}
            Err(ScriptsError::Python(e)) => {
                self.warn_once("python".into(), format!("Automatismes des scripts non lancés :\n{e}"))
            }
            Err(ScriptsError::ReadOnly(m)) => self.warn_once(
                "automatismes-lecture-seule".into(),
                format!("Automatismes des scripts non lancés : la bibliothèque est en lecture seule.\n{m}"),
            ),
        }
        for ev in self.scripts.poll_autos() {
            match ev {
                AutoEvent::Applied(name, outcome) => {
                    self.apply_script_outcome(&outcome, false);
                    let line = format!("[{name}] modifications appliquées : {}", outcome.summary());
                    self.scripts.auto_log(line);
                    if !outcome.is_complete() {
                        let db = self.library.database();
                        let caption = |id: &str| {
                            db.as_ref().and_then(|d| d.get(id)).map(|b| b.caption()).unwrap_or_else(|| id.to_string())
                        };
                        let problems = outcome.problems(caption).join("\n");
                        self.bg_errors.lock().unwrap().push(format!("Automatisme « {name} » :\n{problems}"));
                    }
                }
                AutoEvent::Refused(name, message) => {
                    self.warn_once(
                        format!("lecture-seule:{name}"),
                        format!("Automatisme « {name} » : modifications non appliquées.\n{message}"),
                    );
                }
                AutoEvent::Finished(job, r) => {
                    if matches!(r.state, RunState::Failed | RunState::TimedOut) {
                        let why = r.failure.map(|f| f.to_string()).unwrap_or_default();
                        self.bg_errors.lock().unwrap().push(format!(
                            "Automatisme « {} » ({}, {}) en erreur :\n{why}\n\n(Journal : 📜 Scripts ▸ Gérer les scripts)",
                            job.entry.name,
                            job.hook.description(),
                            job.entry.rel_path
                        ));
                    }
                }
            }
        }
        self.start_pending_write_info(ctx);
        if self.scripts.run.as_ref().is_some_and(|r| r.running() || r.applying() || r.preparing())
            || !self.scripts.autos.is_empty()
        {
            // La fin d'un script ne réveille pas l'interface : on vérifie régulièrement.
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    // -----------------------------------------------------------------------------------
    // Fenêtres
    // -----------------------------------------------------------------------------------

    /// Fermeture en attente des scripts : « Application des modifications en cours… ».
    pub(crate) fn scripts_closing_window(&mut self, ctx: &Context) {
        if self.scripts_closing.is_none() {
            return;
        }
        let status = self.scripts.shutdown_status().unwrap_or("Fermeture…");
        egui::Modal::new(egui::Id::unique("comicrust-fermeture-scripts")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(status).strong());
            });
            ui.label(
                RichText::new("ComicRust se ferme dès que c'est fini : un fichier déplacé a toujours sa fiche à jour.")
                    .weak()
                    .small(),
            );
        });
    }

    pub(crate) fn scripts_windows(&mut self, ctx: &Context) {
        self.scripts_manager_window(ctx);
        self.script_run_window(ctx);
        self.script_question_window(ctx);
    }

    /// Question ou message du script (fenêtre modale).
    fn script_question_window(&mut self, ctx: &Context) {
        let Some(run) = &mut self.scripts.run else { return };
        let Some(pending) = &mut run.pending else { return };
        let name = run.entry.name.clone();
        // None : rien ; Some(None) : défaut ; Some(Some(v)) : réponse ; arrêt à part.
        let mut answer: Option<Option<serde_json::Value>> = None;
        let mut stop = false;
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Modal::new(egui::Id::unique("comicrust-script-question")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.label(RichText::new(format!("📜 {name}")).weak().small());
            match pending {
                Pending::Message { text, .. } => {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| ui.label(text.as_str()));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let ok = ui.button(RichText::new("OK").strong());
                        ok.request_focus();
                        if ok.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            answer = Some(None);
                        }
                        if ui.button("⏹ Arrêter le script").clicked() {
                            stop = true;
                        }
                    });
                }
                Pending::Ask { question, text, choice, .. } => {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&question.text).strong());
                    ui.add_space(4.0);
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    match question.kind {
                        QuestionKind::Text => {
                            // Saisie secrète (clé d'API…) : masquée, jamais pré-remplie.
                            let mut edit = egui::TextEdit::singleline(text).desired_width(480.0);
                            if question.secret {
                                edit = edit.password(true).hint_text("vide = garder la valeur actuelle");
                            }
                            let r = ui.add(edit);
                            if !r.has_focus() && !r.lost_focus() {
                                r.request_focus();
                            }
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("OK").strong()).clicked() || (enter && r.lost_focus()) {
                                    answer = Some(Some(serde_json::Value::String(text.clone())));
                                }
                                if ui.button("Annuler").on_hover_text("Le script reçoit la valeur par défaut").clicked()
                                {
                                    answer = Some(None);
                                }
                                if ui.button("⏹ Arrêter le script").clicked() {
                                    stop = true;
                                }
                            });
                        }
                        QuestionKind::Choice => {
                            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                for (i, c) in question.choices.iter().enumerate() {
                                    ui.radio_value(choice, i, c);
                                }
                            });
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let ok = ui.add_enabled(
                                    !question.choices.is_empty(),
                                    egui::Button::new(RichText::new("OK").strong()),
                                );
                                if ok.clicked() || (enter && !question.choices.is_empty()) {
                                    let c = question.choices.get(*choice).cloned().unwrap_or_default();
                                    answer = Some(Some(serde_json::Value::String(c)));
                                }
                                if ui.button("Annuler").on_hover_text("Le script reçoit la valeur par défaut").clicked()
                                {
                                    answer = Some(None);
                                }
                                if ui.button("⏹ Arrêter le script").clicked() {
                                    stop = true;
                                }
                            });
                        }
                        QuestionKind::Confirm => {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("Oui").strong()).clicked() {
                                    answer = Some(Some(serde_json::Value::Bool(true)));
                                }
                                if ui.button("Non").clicked() {
                                    answer = Some(Some(serde_json::Value::Bool(false)));
                                }
                                if ui.button("⏹ Arrêter le script").clicked() {
                                    stop = true;
                                }
                            });
                        }
                    }
                }
            }
        });
        if escape && answer.is_none() {
            answer = Some(None);
        }
        if stop {
            run.stop();
            return;
        }
        let Some(answer) = answer else { return };
        match run.pending.take() {
            Some(Pending::Ask { reply, .. }) => {
                let _ = reply.send(answer);
            }
            Some(Pending::Message { reply, .. }) => {
                let _ = reply.send(());
            }
            None => {}
        }
    }

    /// Fenêtre d'une exécution lancée depuis un menu.
    fn script_run_window(&mut self, ctx: &Context) {
        let db_now = self.library.database();
        let Some(run) = &mut self.scripts.run else { return };
        let mut open = true;
        let mut close = false;
        let mut apply = false;
        let mut copy = false;
        let busy = run.running() || run.applying();
        let mut window = egui::Window::new(format!("📜 {}", run.entry.name))
            .id(run_window_id())
            .collapsible(true)
            .resizable(true)
            .default_width(820.0)
            .default_height(560.0);
        if !busy {
            window = window.open(&mut open);
        }
        window.show(ctx, |ui| {
            let what = if run.hook == Hook::ConfigScript {
                "Réglages du script".to_string()
            } else {
                format!("{} — {} livre{}", run.hook.description(), run.count, plural(run.count))
            };
            ui.label(RichText::new(what).weak());
            ui.label(RichText::new(format!("{} : {}()", run.entry.rel_path, run.entry.function)).weak().small());
            ui.add_space(4.0);
            if run.running() {
                let bar = match run.progress {
                    Some(f) => egui::ProgressBar::new(f).show_percentage(),
                    None => egui::ProgressBar::new(0.0).animate(true),
                };
                let text = if run.progress_text.is_empty() {
                    format!("En cours depuis {} s", run.started.elapsed().as_secs())
                } else {
                    run.progress_text.clone()
                };
                ui.add(bar.text(text));
                ui.horizontal(|ui| {
                    if run.stopping {
                        ui.spinner();
                        ui.label("Arrêt en cours…");
                    } else if ui.button("⏹ Arrêter").on_hover_text("Demande au script de s'arrêter").clicked() {
                        run.stop();
                    }
                });
            } else if let Some(r) = &run.result {
                let secs = r.duration.as_secs_f32();
                match r.state {
                    RunState::Finished => {
                        ui.label(RichText::new(format!("✔ Terminé en {secs:.1} s.")).color(crate::ui::theme::ok(ui)));
                    }
                    RunState::Cancelled => {
                        ui.label(RichText::new("⏹ Script arrêté.").color(crate::ui::theme::warning(ui)));
                    }
                    RunState::Failed | RunState::TimedOut => {
                        let why = r.failure.as_ref().map(|f| f.to_string()).unwrap_or_default();
                        ui.label(RichText::new(format!("✖ Erreur : {why}")).color(crate::ui::theme::error(ui)));
                    }
                }
                if !r.return_value.is_null() {
                    ui.label(RichText::new(format!("Valeur renvoyée : {}", r.return_value)).weak().small());
                }
            }
            // Journal.
            let n = run.journal.len();
            let open_journal = run.running() || run.preview.is_none();
            egui::CollapsingHeader::new(format!("Journal ({n} ligne{})", plural(n)))
                .id_salt("journal-script")
                .default_open(open_journal)
                .show(ui, |ui| {
                    // Seules les lignes visibles sont dessinées (`show_rows`) : un journal de
                    // 1 Mo ne ralentit pas chaque image. Une ligne de journal = une ligne
                    // affichée (coupée si elle dépasse la largeur).
                    let row_h = ui.text_style_height(&egui::TextStyle::Small) + ui.spacing().item_spacing.y;
                    egui::ScrollArea::vertical()
                        .id_salt("journal-script-defilement")
                        .max_height(170.0)
                        .stick_to_bottom(true)
                        .auto_shrink([false, true])
                        .show_rows(ui, row_h, run.journal.len(), |ui, range| {
                            for l in &run.journal[range] {
                                let color = match l.kind {
                                    JournalKind::Output => ui.visuals().text_color(),
                                    JournalKind::Info => ui.visuals().weak_text_color(),
                                    JournalKind::Warning => crate::ui::theme::warning(ui),
                                    JournalKind::Error => crate::ui::theme::error(ui),
                                };
                                ui.add(
                                    egui::Label::new(RichText::new(&l.text).monospace().small().color(color))
                                        .truncate(),
                                )
                                .on_hover_text(&l.text);
                            }
                        });
                    if ui.small_button("📋 Copier le journal").clicked() {
                        copy = true;
                    }
                });
            if let Some(r) = &run.result {
                if !r.rejected.is_empty() {
                    let n = r.rejected.len();
                    egui::CollapsingHeader::new(
                        RichText::new(format!("⚠ {n} demande{} refusée{}", plural(n), plural(n)))
                            .color(crate::ui::theme::warning(ui)),
                    )
                    .id_salt("refus-script")
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("refus-defilement").max_height(120.0).show(ui, |ui| {
                            for x in &r.rejected {
                                ui.label(RichText::new(&x.reason).small());
                            }
                        });
                    });
                }
            }
            ui.separator();
            // Après l'exécution : aperçu, application, bilan.
            if run.applying() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Application des modifications…");
                });
            } else if run.preparing() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Préparation de l'aperçu…");
                });
            } else if let Some((summary, problems)) = &run.report {
                ui.label(RichText::new(format!("Bilan : {summary}.")).strong());
                if !problems.is_empty() {
                    egui::ScrollArea::vertical().id_salt("bilan-script").max_height(160.0).show(ui, |ui| {
                        for p in problems {
                            ui.label(RichText::new(format!("• {p}")).color(crate::ui::theme::error(ui)));
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("📋 Copier le bilan").clicked() {
                        copy = true;
                    }
                    if ui.button("Fermer").clicked() {
                        close = true;
                    }
                });
            } else if run.discarded {
                ui.label("Modifications abandonnées : rien n'a été appliqué.");
            } else if run.preview.is_some() {
                apply = preview_ui(ui, run, db_now.is_some());
                if run.discarded {
                    close = true;
                }
            } else if let Some(r) = &run.result {
                let n = r.changes.len();
                if n == 0 {
                    if run.hook != Hook::ConfigScript {
                        ui.label("Aucune modification demandée.");
                    }
                } else {
                    ui.label(format!(
                        "{n} modification{} avai{} déjà été demandée{} avant l'arrêt ; rien n'est appliqué sans ton \
                         accord.",
                        plural(n),
                        if n > 1 { "ent" } else { "t" },
                        plural(n)
                    ));
                    if ui.button("Voir l'aperçu de ces modifications").clicked() {
                        run.show_partial_preview();
                    }
                }
                if ui.button("Fermer").clicked() {
                    close = true;
                }
            }
        });
        if copy {
            ctx.copy_text(run.journal_text());
        }
        if apply {
            if let Some(db) = &db_now {
                run.start_apply(db, ctx);
            }
        }
        if close || !open {
            if let Some(r) = &mut self.scripts.run {
                if !r.running() && !r.applying() {
                    self.scripts.run = None;
                }
            }
        }
    }

    /// Gestion des scripts (≈ Préférences ▸ Scripts de ComicRack).
    fn scripts_manager_window(&mut self, ctx: &Context) {
        if !self.scripts.show_manager {
            return;
        }
        let mut open = true;
        let mut configure: Option<ScriptEntry> = None;
        let mut reload = false;
        let mut open_dir = false;
        let mut copy_journal = false;
        let target = self.script_target();
        egui::Window::new("📜 Scripts")
            .id(egui::Id::unique("comicrust-scripts-gestion"))
            .open(&mut open)
            .resizable(true)
            .default_width(680.0)
            .default_height(560.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Dossier :").strong());
                    ui.label(RichText::new(self.scripts.dir.display().to_string()).monospace().small());
                });
                ui.horizontal(|ui| {
                    if ui.button("📂 Ouvrir le dossier").clicked() {
                        open_dir = true;
                    }
                    if ui.button("⟳ Recharger").clicked() {
                        reload = true;
                    }
                });
                if let Some(e) = &self.scripts.dir_error {
                    ui.label(RichText::new(e).color(crate::ui::theme::error(ui)));
                }
                match crate::scripts::python() {
                    Ok(p) => {
                        ui.label(RichText::new(format!("Python : {}", p.display())).weak().small());
                    }
                    Err(e) => {
                        ui.label(RichText::new(e).color(crate::ui::theme::error(ui)));
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.settings.scripts_auto, "Exécuter les automatismes").on_hover_text(
                    "Démarrage, ouverture et fermeture d'un livre, nouveaux livres après une analyse des dossiers",
                );
                ui.label(
                    RichText::new(
                        "Désactivé par défaut : un automatisme est lancé sans que tu le demandes et ses modifications \
                         sont appliquées sans aperçu (comme dans ComicRack). Active-le pour les scripts que tu \
                         connais.",
                    )
                    .weak()
                    .small(),
                );
                if let Some(m) = target.read_only() {
                    ui.label(RichText::new(m).color(crate::ui::theme::warning(ui)).small());
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("liste-scripts")
                    .max_height((ui.available_height() - 160.0).max(220.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if self.scripts.entries.is_empty() {
                            ui.label(
                            "Aucun script. Place des fichiers .py (avec des en-têtes #@Name et #@Hook) dans le dossier \
                             des scripts, puis clique sur « Recharger ».",
                        );
                        }
                        for hook in Hook::ALL {
                            let entries: Vec<&ScriptEntry> =
                                self.scripts.entries.iter().filter(|e| e.has_hook(hook)).collect();
                            if entries.is_empty() {
                                continue;
                            }
                            ui.add_space(4.0);
                            ui.label(RichText::new(hook.description()).strong());
                            for e in entries {
                                let id = e.id();
                                let mut on = self.script_choices().is_active(e);
                                ui.horizontal(|ui| {
                                    let mut check = ui.checkbox(&mut on, RichText::new(&e.name));
                                    if !e.enabled_by_default {
                                        check = check.on_hover_text(
                                            "Désactivé par défaut (en-tête #@Enabled false) : coche-le pour l'activer",
                                        );
                                    }
                                    if check.changed() {
                                        // Le choix de l'utilisateur prime sur l'en-tête #@Enabled.
                                        if on {
                                            self.settings.scripts_disabled.remove(&id);
                                            if !e.enabled_by_default {
                                                self.settings.scripts_enabled.insert(id.clone());
                                            }
                                        } else {
                                            self.settings.scripts_disabled.insert(id.clone());
                                            self.settings.scripts_enabled.remove(&id);
                                        }
                                    }
                                    ui.label(
                                        RichText::new(format!("{}:{} ({}())", e.rel_path, e.line, e.function))
                                            .weak()
                                            .small(),
                                    );
                                    let has_config = hook != Hook::ConfigScript && self.scripts.config_of(e).is_some();
                                    if has_config
                                        && ui
                                            .add_enabled(!self.scripts.busy(), egui::Button::new("Réglages…").small())
                                            .clicked()
                                    {
                                        configure = Some(e.clone());
                                    }
                                });
                                if let Some(d) = &e.description {
                                    ui.indent(("desc", &id, hook.name()), |ui| {
                                        ui.label(RichText::new(d).small());
                                    });
                                }
                            }
                        }
                    });
                if !self.scripts.warnings.is_empty() {
                    ui.separator();
                    let n = self.scripts.warnings.len();
                    egui::CollapsingHeader::new(
                        RichText::new(format!("⚠ {n} avertissement{}", plural(n))).color(crate::ui::theme::warning(ui)),
                    )
                    .id_salt("avertissements-scripts")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("avertissements-defilement").max_height(120.0).show(
                            ui,
                            |ui| {
                                for w in &self.scripts.warnings {
                                    let rel = w.file.strip_prefix(&self.scripts.dir).unwrap_or(&w.file);
                                    let place = if w.line > 0 {
                                        format!("{}:{}", rel.display(), w.line)
                                    } else {
                                        rel.display().to_string()
                                    };
                                    ui.label(RichText::new(format!("{place} — {}", w.message)).small());
                                }
                            },
                        );
                    });
                }
                ui.separator();
                let running = self.scripts.queue.running();
                let waiting = self.scripts.queue.waiting();
                egui::CollapsingHeader::new(format!(
                    "Journal des automatismes ({running} en cours, {waiting} en attente)"
                ))
                .id_salt("journal-automatismes")
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("journal-automatismes-defilement")
                        .max_height(150.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.scripts.auto_journal.is_empty() {
                                ui.label(RichText::new("Rien pour l'instant.").weak());
                            }
                            for l in &self.scripts.auto_journal {
                                ui.add(egui::Label::new(RichText::new(l).monospace().small()).selectable(true));
                            }
                        });
                    if ui.small_button("📋 Copier").clicked() {
                        copy_journal = true;
                    }
                });
            });
        if copy_journal {
            let text: Vec<&str> = self.scripts.auto_journal.iter().map(String::as_str).collect();
            ctx.copy_text(text.join("\n"));
        }
        if reload {
            self.scripts.reload();
        }
        if open_dir {
            let _ = std::fs::create_dir_all(&self.scripts.dir);
            if let Err(e) = open_folder(&self.scripts.dir) {
                self.error = Some(e);
            }
        }
        if let Some(e) = configure {
            self.configure_script(ctx, &e);
        }
        if !self.settings.scripts_auto {
            self.scripts.stop_autos();
        }
        if !open {
            self.scripts.show_manager = false;
        }
    }
}

/// Cellule de tableau : texte aligné à gauche, coupé (« … ») s'il dépasse `width`.
fn cell(ui: &mut Ui, width: f32, row_h: f32, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.allocate_ui_with_layout(egui::vec2(width, row_h - 4.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(width);
        ui.add(egui::Label::new(text).truncate())
    })
    .inner
}

/// Aperçu des modifications (tableau livre / champ / avant → après), cases, boutons.
/// Renvoie `true` si « Appliquer » est cliqué.
fn preview_ui(ui: &mut Ui, run: &mut crate::scripts::ScriptRun, library: bool) -> bool {
    let read_only = run.target.read_only().map(str::to_string);
    let Some(preview) = &mut run.preview else { return false };
    let (books, fields, renames, infos, problems) = preview.counts();
    ui.label(RichText::new("Aperçu des modifications").strong());
    let mut parts = vec![format!("{books} livre{}", plural(books))];
    if fields > 0 {
        parts.push(format!("{fields} champ{}", plural(fields)));
    }
    if renames > 0 {
        parts.push(format!("{renames} renommage{}", plural(renames)));
    }
    if infos > 0 {
        parts.push(format!("infos à écrire dans {infos} fichier{}", plural(infos)));
    }
    if preview.unchanged > 0 {
        parts.push(format!("{} déjà à jour", preview.unchanged));
    }
    ui.label(parts.join(" · "));
    if problems > 0 {
        ui.label(
            RichText::new(format!(
                "⚠ {problems} problème{} (valeur refusée, conflit de renommage…) : non appliqué{}.",
                plural(problems),
                plural(problems)
            ))
            .color(crate::ui::theme::warning(ui)),
        );
    }
    if !preview.missing.is_empty() {
        ui.label(
            RichText::new(format!("{} livre(s) disparu(s) de la bibliothèque : ignoré(s).", preview.missing.len()))
                .weak(),
        );
    }
    if let Some(m) = &read_only {
        ui.label(RichText::new(m).color(crate::ui::theme::error(ui)).strong());
    }
    ui.horizontal(|ui| {
        if ui.small_button("Tout cocher").clicked() {
            preview.set_all(true);
        }
        if ui.small_button("Tout décocher").clicked() {
            preview.set_all(false);
        }
    });
    // Lignes à plat (défilement rapide même pour des milliers de livres).
    #[derive(Clone, Copy)]
    enum Line {
        Book(usize),
        Field(usize, usize),
        Rename(usize),
        Info(usize),
    }
    let mut lines = Vec::new();
    for (i, b) in preview.books.iter().enumerate() {
        lines.push(Line::Book(i));
        lines.extend((0..b.fields.len()).map(|k| Line::Field(i, k)));
        if b.rename.is_some() {
            lines.push(Line::Rename(i));
        }
        if b.write_info.is_some() {
            lines.push(Line::Info(i));
        }
    }
    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let width = ui.available_width();
    let col = ((width - 230.0) / 2.0).max(120.0);
    let err = crate::ui::theme::error(ui);
    let warn = crate::ui::theme::warning(ui);
    egui::ScrollArea::vertical().id_salt("apercu-script").max_height(330.0).auto_shrink([false, true]).show_rows(
        ui,
        row_h,
        lines.len(),
        |ui, range| {
            for line in &lines[range] {
                ui.horizontal(|ui| {
                    ui.set_height(row_h - 2.0);
                    match *line {
                        Line::Book(i) => {
                            let b = &mut preview.books[i];
                            let enabled = b.applicable();
                            ui.add_enabled(enabled, egui::Checkbox::without_text(&mut b.checked));
                            ui.add(egui::Label::new(RichText::new(&b.caption).strong()).truncate());
                        }
                        Line::Field(i, k) => {
                            let f = &preview.books[i].fields[k];
                            let shown =
                                |t: &str| if t.is_empty() { "(vide)".to_string() } else { t.replace('\n', " ⏎ ") };
                            ui.add_space(26.0);
                            cell(ui, 150.0, row_h, RichText::new(&f.label).weak());
                            cell(ui, col, row_h, shown(&f.before));
                            ui.label("→");
                            match &f.error {
                                None => {
                                    cell(ui, col, row_h, RichText::new(shown(&f.after)).strong());
                                }
                                Some(e) => {
                                    cell(
                                        ui,
                                        col,
                                        row_h,
                                        RichText::new(format!("{} — {e}", shown(&f.after))).color(err),
                                    );
                                }
                            }
                        }
                        Line::Rename(i) => {
                            let Some(r) = &preview.books[i].rename else { return };
                            let (from, to) = crate::scripts::apply::rename_texts(&r.from, &r.to);
                            ui.add_space(26.0);
                            cell(ui, 150.0, row_h, RichText::new("Fichier").weak());
                            cell(ui, col, row_h, from).on_hover_text(r.from.display().to_string());
                            ui.label("→");
                            let text = match &r.problem {
                                None => RichText::new(to).strong(),
                                Some(p) => RichText::new(format!("⚠ {to} — {p}")).color(warn),
                            };
                            cell(ui, col, row_h, text).on_hover_text(r.to.display().to_string());
                        }
                        Line::Info(i) => {
                            ui.add_space(26.0);
                            cell(ui, 150.0, row_h, RichText::new("ComicInfo.xml").weak());
                            match &preview.books[i].write_info {
                                Some(Some(p)) => ui.label(RichText::new(format!("⚠ {p}")).color(warn)),
                                _ => ui.label("écrire les infos dans le fichier"),
                            };
                        }
                    }
                });
            }
        },
    );
    ui.add_space(6.0);
    let n = preview.checked();
    let mut apply = false;
    ui.horizontal(|ui| {
        let can = read_only.is_none() && library && n > 0;
        let label = format!("✔ Appliquer ({n} livre{})", plural(n));
        let b = ui.add_enabled(can, egui::Button::new(RichText::new(label).strong()));
        let b = if let Some(m) = &read_only { b.on_disabled_hover_text(m) } else { b };
        if b.clicked() {
            apply = true;
        }
        if ui.button("Annuler").on_hover_text("Abandonner ces modifications").clicked() {
            run.discarded = true;
        }
    });
    apply
}
