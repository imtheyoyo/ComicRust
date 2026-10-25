//! Assistant de configuration (premier démarrage, ou menu ⚙ → « Assistant de
//! configuration… ») : ComicRust autonome (sa propre base, ses dossiers de BD) ou lié à
//! ComicRack (ComicRack.ini, ComicDb.xml, Config.xml ; base MySQL ou ComicDb.xml).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use comicrust_core::db::{test_connection, DbConfig};
use comicrust_core::store::{Store, StoreConfig};
use eframe::egui::{self, Context, RichText, Ui};

use crate::app::ComicRustApp;
use crate::settings::{DbSource, LibraryMode, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Choice,
    Standalone,
    ComicRack,
}

/// Paramètres MySQL saisis.
#[derive(Debug, Clone)]
struct MySqlForm {
    host: String,
    port: String,
    user: String,
    password: String,
    database: String,
}

impl MySqlForm {
    fn new(c: Option<&DbConfig>) -> Self {
        Self {
            host: c.map(|c| c.host.clone()).unwrap_or_default(),
            port: c.map(|c| c.port.to_string()).unwrap_or_else(|| "3306".into()),
            user: c.map(|c| c.user.clone()).unwrap_or_default(),
            password: c.map(|c| c.password.clone()).unwrap_or_default(),
            database: c.map(|c| c.database.clone()).unwrap_or_default(),
        }
    }

    fn config(&self) -> Result<DbConfig, String> {
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
        Ok(cfg)
    }

    fn ui(&mut self, ui: &mut Ui, id: &str) {
        egui::Grid::new(id).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Serveur");
            ui.text_edit_singleline(&mut self.host);
            ui.end_row();
            ui.label("Port");
            ui.add(egui::TextEdit::singleline(&mut self.port).desired_width(80.0));
            ui.end_row();
            ui.label("Utilisateur");
            ui.text_edit_singleline(&mut self.user);
            ui.end_row();
            ui.label("Mot de passe");
            ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
            ui.end_row();
            ui.label("Base");
            ui.text_edit_singleline(&mut self.database);
            ui.end_row();
        });
    }
}

/// Vérifications des fichiers de ComicRack, faites dans un fil (un chemin réseau
/// `\\SERVEUR\…` en cours de saisie peut bloquer plusieurs secondes).
#[derive(Default, Clone)]
struct Checks {
    ini: String,
    comicdb: String,
    ini_found: bool,
    db_found: bool,
    /// Paramètres MySQL lus dans ComicRack.ini (description) ou erreur.
    ini_cfg: Option<Result<String, String>>,
    /// ComicDb.xml et Config.xml retrouvés d'après ComicRack.ini.
    located: (Option<PathBuf>, Option<PathBuf>),
}

fn run_checks(ini: String, comicdb: String) -> Checks {
    let ini_p = path_of(&ini);
    let ini_found = ini_p.as_ref().is_some_and(|p| p.is_file());
    let ini_cfg =
        ini_p.as_ref().map(|p| DbConfig::from_comicrack_ini(p).map(|c| c.describe()).map_err(|e| e.to_string()));
    let located = if ini_found { locate_from_ini(ini_p.as_deref()) } else { (None, None) };
    let db_found = path_of(&comicdb).is_some_and(|p| p.is_file());
    Checks { ini, comicdb, ini_found, db_found, ini_cfg, located }
}

/// Réglages validés par l'assistant (après vérification dans un fil).
enum Finish {
    Standalone(StoreConfig),
    ComicRack { mode: LibraryMode, db_source: Option<DbSource>, write: bool },
}

pub struct SetupWindow {
    page: Page,
    first_run: bool,
    // Autonome
    store_mysql: bool,
    store_form: MySqlForm,
    folders: Vec<PathBuf>,
    new_folder: String,
    folders_rx: Option<Receiver<Result<Vec<PathBuf>, String>>>,
    // Lié à ComicRack
    ini: String,
    comicdb: String,
    config: String,
    use_xml: bool,
    manual_mysql: bool,
    cr_form: MySqlForm,
    /// Base MySQL : écrire dans la base de ComicRack (lecture, fichiers convertis, infos écrites).
    write: bool,
    // Vérification des fichiers (dans un fil)
    checks: Checks,
    checks_rx: Option<Receiver<Checks>>,
    /// ComicRack.ini pour lequel ComicDb.xml / Config.xml ont déjà été proposés.
    located_for: String,
    // Vérification en cours / résultat
    pending: Option<Receiver<Result<(Finish, String), String>>>,
    message: Option<Result<String, String>>,
}

fn text(p: &Option<PathBuf>) -> String {
    p.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
}

fn path_of(s: &str) -> Option<PathBuf> {
    let t = s.trim().trim_matches('"');
    (!t.is_empty()).then(|| PathBuf::from(t))
}

/// Emplacements habituels de ComicRack.ini (à côté de ComicRack.exe).
fn guess_ini() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(d) = std::env::var_os(var) {
            dirs.push(PathBuf::from(&d).join("ComicRack Community Edition"));
            dirs.push(PathBuf::from(&d).join("ComicRack"));
        }
    }
    dirs.into_iter().map(|d| d.join("ComicRack.ini")).find(|p| p.is_file())
}

/// Dossier des réglages de ComicRack dans %APPDATA% (ComicDb.xml, Config.xml).
fn appdata_comicrack() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("cYo").join("ComicRack Community Edition"))
}

/// ComicDb.xml et Config.xml d'après ComicRack.ini (ou %APPDATA%).
fn locate_from_ini(ini: Option<&Path>) -> (Option<PathBuf>, Option<PathBuf>) {
    let db = ini
        .and_then(comicrust_core::comic_lists::find_comicdb)
        .or_else(|| appdata_comicrack().map(|d| d.join("ComicDb.xml")).filter(|p| p.is_file()));
    let config = db
        .as_ref()
        .and_then(|d| d.parent())
        .map(|d| d.join("Config.xml"))
        .filter(|p| p.is_file())
        .or_else(|| appdata_comicrack().map(|d| d.join("Config.xml")).filter(|p| p.is_file()));
    (db, config)
}

impl SetupWindow {
    pub fn new(s: &Settings) -> Self {
        let ini = s
            .comicrack_ini
            .clone()
            .or_else(|| match &s.db_source {
                DbSource::ComicRackIni(p) => Some(p.clone()),
                _ => None,
            })
            .or_else(guess_ini);
        let (found_db, found_config) = locate_from_ini(ini.as_deref());
        let comicdb = s.library_db.clone().or(found_db);
        let config = s.comicrack_config.clone().or(found_config);
        let manual = match &s.db_source {
            DbSource::Manual(c) => Some(c.clone()),
            _ => None,
        };
        let store_mysql = match &s.store {
            Some(StoreConfig::MySql(c)) => Some(c.clone()),
            _ => None,
        };
        let page = match s.library_mode {
            LibraryMode::Unset => Page::Choice,
            LibraryMode::Standalone => Page::Standalone,
            LibraryMode::ComicRackMySql | LibraryMode::ComicRackXml => Page::ComicRack,
        };
        let mut w = Self {
            page,
            first_run: s.library_mode == LibraryMode::Unset,
            store_mysql: store_mysql.is_some(),
            store_form: MySqlForm::new(store_mysql.as_ref()),
            folders: Vec::new(),
            new_folder: String::new(),
            folders_rx: None,
            ini: text(&ini),
            comicdb: text(&comicdb),
            config: text(&config),
            use_xml: s.library_mode == LibraryMode::ComicRackXml
                || (s.library_mode == LibraryMode::Unset
                    && ini.as_deref().is_some_and(|i| DbConfig::from_comicrack_ini(i).is_err())),
            manual_mysql: manual.is_some(),
            cr_form: MySqlForm::new(manual.as_ref()),
            write: s.comicrack_write,
            checks: Checks::default(),
            checks_rx: None,
            located_for: String::new(),
            pending: None,
            message: None,
        };
        w.located_for = w.ini.clone();
        // Dossiers déjà enregistrés dans la base ComicRust (relus dans un fil : MySQL peut tarder).
        if let Some(cfg) = s.store.clone() {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(Store::open(&cfg).and_then(|mut st| st.folders()).map_err(|e| e.to_string()));
            });
            w.folders_rx = Some(rx);
        }
        w
    }

    fn store_config(&self) -> Result<StoreConfig, String> {
        if self.store_mysql {
            Ok(StoreConfig::MySql(self.store_form.config()?))
        } else {
            StoreConfig::portable_sqlite().map_err(|e| e.to_string())
        }
    }

    /// Lance la vérification et l'enregistrement dans un fil.
    fn start_finish(&mut self, ctx: &Context) {
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        match self.page {
            Page::Standalone => {
                let cfg = match self.store_config() {
                    Ok(c) => c,
                    Err(e) => return self.message = Some(Err(e)),
                };
                // Chemin tapé sans « Ajouter » : ajouté quand même.
                if let Some(d) = path_of(&self.new_folder) {
                    if !self.folders.contains(&d) {
                        self.folders.push(d);
                    }
                    self.new_folder.clear();
                }
                if self.folders.is_empty() {
                    return self.message = Some(Err("ajoute au moins un dossier de BD".into()));
                }
                let folders = self.folders.clone();
                std::thread::spawn(move || {
                    let run = || -> Result<(Finish, String), String> {
                        let mut st = Store::open(&cfg).map_err(|e| e.to_string())?;
                        st.set_folders(&folders).map_err(|e| e.to_string())?;
                        let n = st.count().map_err(|e| e.to_string())?;
                        Ok((Finish::Standalone(cfg), format!("Base prête ({n} livres déjà enregistrés).")))
                    };
                    let _ = tx.send(catch(run));
                    ctx2.request_repaint();
                });
            }
            Page::ComicRack => {
                let ini = path_of(&self.ini);
                let comicdb = path_of(&self.comicdb);
                let use_xml = self.use_xml;
                let write = self.write;
                let db_source = if use_xml {
                    None
                } else if self.manual_mysql {
                    match self.cr_form.config() {
                        Ok(c) => Some(DbSource::Manual(c)),
                        Err(e) => return self.message = Some(Err(e)),
                    }
                } else {
                    match ini {
                        Some(i) => Some(DbSource::ComicRackIni(i)),
                        None => return self.message = Some(Err("indique ComicRack.ini".into())),
                    }
                };
                std::thread::spawn(move || {
                    let run = || -> Result<(Finish, String), String> {
                        if use_xml {
                            let p = comicdb.ok_or("indique ComicDb.xml")?;
                            let db = crate::library::load_comicdb_xml(&p)?;
                            if db.is_empty() {
                                return Err(format!(
                                    "{} ne contient aucun livre : ComicRack utilise sans doute une base MySQL",
                                    p.display()
                                ));
                            }
                            Ok((
                                Finish::ComicRack { mode: LibraryMode::ComicRackXml, db_source: None, write },
                                format!("{} livres dans ComicDb.xml.", db.len()),
                            ))
                        } else {
                            let src = db_source.unwrap_or(DbSource::None);
                            let cfg = src.config()?.ok_or("base MySQL non indiquée")?;
                            let n = test_connection(&cfg).map_err(|e| e.to_string())?;
                            Ok((
                                Finish::ComicRack { mode: LibraryMode::ComicRackMySql, db_source: Some(src), write },
                                format!("Connexion réussie : {n} livres dans la base."),
                            ))
                        }
                    };
                    let _ = tx.send(catch(run));
                    ctx2.request_repaint();
                });
            }
            Page::Choice => return,
        }
        self.pending = Some(rx);
        self.message = None;
    }
}

/// Vérification protégée : un plantage devient un message dans l'assistant (le détail est
/// dans `data\plantage.txt`) au lieu d'une attente sans fin.
/// Libellé de l'option d'écriture de l'assistant (`xml` : ComicDb.xml seul, sans MySQL).
pub fn write_label(xml: bool) -> &'static str {
    if xml {
        "Lecture et écriture des listes (ComicRack fermé)"
    } else {
        "Lecture et écriture (lecture, fichiers convertis, infos écrites sont enregistrés dans la base de ComicRack ; \
         listes modifiables, ComicRack fermé)"
    }
}

/// Explication sous les options d'accès de l'assistant.
pub fn setup_note(xml: bool, write: bool) -> &'static str {
    match (xml, write) {
        (true, true) => {
            "Les listes sont enregistrées dans ComicDb.xml, seulement quand ComicRack est fermé (il réécrit ce \
             fichier en se fermant), avec une sauvegarde à chaque modification. Les livres de ComicDb.xml, eux, ne \
             sont pas modifiés."
        }
        (true, false) => "ComicDb.xml : lecture seule, ni les livres ni les listes ne sont modifiés.",
        (false, true) => {
            "ComicRack lancé voit les modifications en quelques secondes. ComicRust ne réécrit que les éléments \
             concernés de la fiche (page lue, date d'ouverture, fichier…), jamais le reste. Les listes (dans \
             ComicDb.xml) ne sont modifiables que ComicRack fermé."
        }
        (false, false) => {
            "La lecture faite dans ComicRust est gardée dans ses réglages ; ni la base ni les listes de ComicRack ne \
             sont modifiées."
        }
    }
}

fn catch<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let why = p
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        Err(format!("erreur interne : {why} (détails dans data\\plantage.txt)"))
    })
}

fn browse_file(ui: &mut Ui, value: &mut String, title: &str, filter: (&str, &[&str])) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(380.0));
        if ui.button("Parcourir…").clicked() {
            let mut d = rfd::FileDialog::new().set_title(title).add_filter(filter.0, filter.1);
            if let Some(dir) = path_of(value).and_then(|p| p.parent().map(Path::to_path_buf)) {
                d = d.set_directory(dir);
            }
            if let Some(p) = d.pick_file() {
                *value = p.display().to_string();
            }
        }
    });
}

fn status(ui: &mut Ui, ok: bool, msg: &str) {
    if ok {
        ui.label(RichText::new(format!("✔ {msg}")).color(crate::ui::theme::ok(ui)).small());
    } else {
        ui.label(RichText::new(format!("✖ {msg}")).color(crate::ui::theme::error(ui)).small());
    }
}

impl ComicRustApp {
    pub(crate) fn open_setup(&mut self) {
        self.setup = Some(SetupWindow::new(&self.settings));
    }

    pub(crate) fn setup_window(&mut self, ctx: &Context) {
        let Some(w) = &mut self.setup else { return };
        if let Some(rx) = &w.folders_rx {
            if let Ok(r) = rx.try_recv() {
                if let Ok(f) = r {
                    if w.folders.is_empty() {
                        w.folders = f;
                    }
                }
                w.folders_rx = None;
            }
        }
        if let Some(rx) = &w.checks_rx {
            if let Ok(c) = rx.try_recv() {
                w.checks_rx = None;
                // Nouveau ComicRack.ini : ComicDb.xml et Config.xml retrouvés d'après lui, et
                // pas de ligne « DataSource=mysql » : ComicRack utilise ComicDb.xml.
                if c.ini == w.ini && c.ini_found && w.located_for != c.ini {
                    w.located_for = c.ini.clone();
                    if let Some(db) = &c.located.0 {
                        w.comicdb = db.display().to_string();
                    }
                    if let Some(cf) = &c.located.1 {
                        w.config = cf.display().to_string();
                    }
                    w.use_xml = matches!(c.ini_cfg, Some(Err(_)));
                }
                w.checks = c;
            }
        }
        if w.page == Page::ComicRack
            && w.checks_rx.is_none()
            && (w.checks.ini != w.ini || w.checks.comicdb != w.comicdb)
        {
            let (tx, rx) = mpsc::channel();
            let (ini, db, c) = (w.ini.clone(), w.comicdb.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(run_checks(ini, db));
                c.request_repaint();
            });
            w.checks_rx = Some(rx);
        }
        let mut done: Option<Finish> = None;
        if let Some(rx) = &w.pending {
            if let Ok(r) = rx.try_recv() {
                w.pending = None;
                match r {
                    Ok((f, msg)) => {
                        w.message = Some(Ok(msg));
                        done = Some(f);
                    }
                    Err(e) => w.message = Some(Err(e)),
                }
            }
        }
        let mut close = false;
        let mut finish = false;
        egui::Modal::new(egui::Id::unique("comicrust-assistant")).show(ctx, |ui| {
            ui.set_width(560.0);
            match w.page {
                Page::Choice => {
                    ui.heading(if w.first_run { "Bienvenue dans ComicRust" } else { "Assistant de configuration" });
                    ui.label("Comment veux-tu utiliser ComicRust ?");
                    ui.add_space(10.0);
                    let big = |ui: &mut Ui, title: &str, sub: &str| {
                        ui.add_sized(
                            [540.0, 58.0],
                            egui::Button::new(RichText::new(format!("{title}\n{sub}")).size(14.5)),
                        )
                        .clicked()
                    };
                    if big(
                        ui,
                        "📚  ComicRust autonome",
                        "Sa propre base (fichier local ou MySQL) et tes dossiers de BD, sans ComicRack.",
                    ) {
                        w.page = Page::Standalone;
                        w.message = None;
                    }
                    ui.add_space(6.0);
                    if big(
                        ui,
                        "🔗  Avec ComicRack",
                        "La bibliothèque et les listes de ComicRack (base MySQL ou ComicDb.xml).",
                    ) {
                        w.page = Page::ComicRack;
                        w.message = None;
                    }
                    ui.add_space(10.0);
                    if ui.button(if w.first_run { "Plus tard" } else { "Annuler" }).clicked() {
                        close = true;
                    }
                }
                Page::Standalone => {
                    ui.heading("📚 ComicRust autonome");
                    ui.add_space(4.0);
                    ui.label(RichText::new("Base de ComicRust").strong());
                    ui.radio_value(
                        &mut w.store_mysql,
                        false,
                        "Fichier local (data\\comicrust.db, à côté de l'exe : reste portable)",
                    );
                    ui.radio_value(
                        &mut w.store_mysql,
                        true,
                        "Serveur MySQL / MariaDB (partagé entre plusieurs postes)",
                    );
                    if w.store_mysql {
                        w.store_form.ui(ui, "assistant-mysql-autonome");
                        ui.label(
                            RichText::new(
                                "Les tables de ComicRust (préfixe comicrust_) peuvent aller dans la même base que \
                                 celles de ComicRack sans les toucher. Le mot de passe est enregistré chiffré pour \
                                 ton compte Windows (jamais en clair).",
                            )
                            .weak()
                            .small(),
                        );
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Dossiers de BD").strong());
                    if w.folders_rx.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("lecture des dossiers enregistrés…");
                        });
                    }
                    let mut remove = None;
                    for (i, f) in w.folders.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("✖").on_hover_text("Retirer ce dossier").clicked() {
                                remove = Some(i);
                            }
                            ui.label(f.display().to_string());
                        });
                    }
                    if let Some(i) = remove {
                        w.folders.remove(i);
                    }
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut w.new_folder)
                                .hint_text("chemin du dossier, ex. \\\\SERVEURNAS\\BD")
                                .desired_width(330.0),
                        );
                        let typed = path_of(&w.new_folder);
                        if ui.add_enabled(typed.is_some(), egui::Button::new("Ajouter")).clicked() {
                            if let Some(d) = typed {
                                if !w.folders.contains(&d) {
                                    w.folders.push(d);
                                }
                                w.new_folder.clear();
                            }
                        }
                        if ui.button("📁 Parcourir…").clicked() {
                            if let Some(d) = rfd::FileDialog::new().set_title("Dossier de BD").pick_folder() {
                                if !w.folders.contains(&d) {
                                    w.folders.push(d);
                                }
                            }
                        }
                    });
                    ui.label(
                        RichText::new(
                            "Les sous-dossiers sont parcourus ; les infos sont lues dans le ComicInfo.xml de chaque \
                             fichier. Écrire les dossiers comme tu y accèdes (ex. \\\\SERVEURNAS\\…).",
                        )
                        .weak()
                        .small(),
                    );
                }
                Page::ComicRack => {
                    ui.heading("🔗 Avec ComicRack");
                    ui.add_space(4.0);
                    egui::Grid::new("assistant-fichiers").num_columns(1).spacing([0.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new("ComicRack.ini (à côté de ComicRack.exe)").strong());
                        ui.end_row();
                        browse_file(ui, &mut w.ini, "ComicRack.ini", ("ComicRack.ini", &["ini"]));
                        ui.end_row();
                        if path_of(&w.ini).is_some() && w.checks.ini == w.ini {
                            let ok = w.checks.ini_found;
                            status(ui, ok, if ok { "trouvé" } else { "introuvable" });
                            ui.end_row();
                        }
                        ui.label(RichText::new("ComicDb.xml (bibliothèque et listes)").strong());
                        ui.end_row();
                        browse_file(ui, &mut w.comicdb, "ComicDb.xml", ("ComicDb.xml", &["xml"]));
                        ui.end_row();
                        if path_of(&w.comicdb).is_some() && w.checks.comicdb == w.comicdb {
                            let ok = w.checks.db_found;
                            status(ui, ok, if ok { "trouvé" } else { "introuvable" });
                            ui.end_row();
                        }
                        ui.label(RichText::new("Config.xml (réglages de ComicRack)").strong());
                        ui.end_row();
                        browse_file(ui, &mut w.config, "Config.xml", ("Config.xml", &["xml"]));
                        ui.end_row();
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Base de ComicRack").strong());
                    let ini_cfg = if w.checks.ini == w.ini { w.checks.ini_cfg.clone() } else { None };
                    ui.radio_value(&mut w.use_xml, false, "MySQL");
                    if !w.use_xml {
                        ui.indent("mysql", |ui| {
                            ui.radio_value(
                                &mut w.manual_mysql,
                                false,
                                "Paramètres lus dans ComicRack.ini (recommandé)",
                            );
                            if !w.manual_mysql {
                                match &ini_cfg {
                                    Some(Ok(c)) => status(ui, true, c),
                                    Some(Err(e)) => status(ui, false, e),
                                    None if w.ini.trim().is_empty() => status(ui, false, "ComicRack.ini non indiqué"),
                                    None => {
                                        ui.spinner();
                                    }
                                }
                            }
                            ui.radio_value(&mut w.manual_mysql, true, "Saisir les paramètres");
                            if w.manual_mysql {
                                w.cr_form.ui(ui, "assistant-mysql-comicrack");
                                ui.label(
                                    RichText::new(
                                        "Mot de passe enregistré chiffré pour ton compte Windows (jamais en clair).",
                                    )
                                    .weak()
                                    .small(),
                                );
                            }
                        });
                    }
                    ui.radio_value(&mut w.use_xml, true, "ComicDb.xml (ComicRack sans MySQL)");
                    ui.add_space(8.0);
                    ui.label(RichText::new("Accès à la base de ComicRack").strong());
                    let xml = w.use_xml;
                    ui.radio_value(&mut w.write, false, "Lecture seule");
                    ui.radio_value(&mut w.write, true, write_label(xml));
                    let note = setup_note(xml, w.write);
                    ui.label(RichText::new(note).weak().small());
                }
            }
            if w.page != Page::Choice {
                ui.add_space(8.0);
                match &w.message {
                    Some(Ok(m)) => status(ui, true, m),
                    Some(Err(e)) => status(ui, false, e),
                    None => {}
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("◀ Retour").clicked() {
                        w.page = Page::Choice;
                        w.message = None;
                    }
                    let busy = w.pending.is_some() || w.folders_rx.is_some();
                    if ui.add_enabled(!busy, egui::Button::new(RichText::new("Terminer").strong())).clicked() {
                        finish = true;
                    }
                    if busy {
                        ui.spinner();
                        ui.label("vérification…");
                    }
                    if ui.button("Annuler").clicked() {
                        close = true;
                    }
                });
            }
        });
        if finish {
            w.start_finish(ctx);
        }
        if let Some(f) = done {
            let comicdb = path_of(&w.comicdb);
            let ini = path_of(&w.ini);
            let config = path_of(&w.config);
            match f {
                Finish::Standalone(cfg) => {
                    self.settings.library_mode = LibraryMode::Standalone;
                    self.settings.store = Some(cfg);
                }
                Finish::ComicRack { mode, db_source, write } => {
                    self.settings.library_mode = mode;
                    if let Some(src) = db_source {
                        self.settings.db_source = src;
                    }
                    self.settings.library_db = comicdb;
                    self.settings.comicrack_ini = ini;
                    self.settings.comicrack_config = config;
                    self.settings.comicrack_write = write;
                }
            }
            self.setup = None;
            self.save_now = true;
            self.select_list(None);
            self.scan_after_load = true;
            self.load_library(ctx);
            self.mode = crate::app::Mode::Library;
        } else if close {
            self.setup = None;
        }
    }
}
