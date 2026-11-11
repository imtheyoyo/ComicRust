//! Lecteur : livres ouverts, pages servies par le protocole `cr-page`, avancement de lecture.
//!
//! Logique reprise de la version egui (`crates/comicrust/src/book/` : `loader.rs`,
//! `navigation.rs`, et `app.rs` : `open_book_at`, `remember_position`, `store_progress`),
//! **dupliquée** (décision du plan : on ne touche pas à `crates/comicrust`) et adaptée à l'IPC :
//! au lieu de textures egui, on sert des octets d'image lisibles par la WebView.
//!
//! - Les pages sont désignées par leur **numéro dans le livre** (ComicRack : rang dans la liste
//!   complète, pages supprimées comprises) ; la liste est donnée complète, dans l'ordre de la
//!   fiche, avec `supprimee` et `rotation` : le front masque les pages supprimées et applique la
//!   rotation (l'image servie n'est pas tournée), comme la version egui à l'affichage.
//! - Un **fil par livre ouvert** détient le fournisseur de pages (PDFium n'est pas transférable
//!   entre fils) ; les pages demandées passent avant le préchargement (±N pages lues).
//! - Les fournisseurs du cœur ne gardent **aucune poignée** sur le fichier entre deux lectures
//!   (`providers/on_demand.rs`) ; si le fichier a été réécrit, le livre est rouvert avant la
//!   lecture suivante (comme `loader.rs::refresh`).
//! - Les pages produites vont dans un **cache mémoire borné** (octets et nombre d'entrées).
//! - Le **NAS lent** ne bloque rien : une page qui n'est pas prête après [`DELAI_PAGE`] reçoit
//!   une réponse 504 (le travail continue et remplira le cache s'il aboutit).
//! - Avancement : page courante notée à chaque appel dans `data\avancement.json` (tous modes,
//!   comme `last_pages` / `read_progress` de la version egui) ; **base de ComicRust** écrite une
//!   fois par séance (fermeture du livre, fin du livre, fermeture de l'application) en mode
//!   autonome. Mode lié à ComicRack MySQL avec l'option « Lecture et écriture » (réglage
//!   `edition.ecritureComicrack`) : la lecture est écrite dans la fiche ComicRack au même moment,
//!   comme `write_reading_to_comicrack` de la version egui (`library/comicrack_write.rs`) ;
//!   sans l'option (défaut), rien n'est écrit dans ComicRack.

use crate::avancement::Memoire;
use crate::biblio::Biblio;
use crate::liberation;
use comicrust_core::db::{BookEdit, Value};
use comicrust_core::library::{LibraryBook, PageInfo as PageFiche};
use comicrust_core::pages::{self, ReadingPage};
use comicrust_core::providers::on_demand::file_stamp;
use comicrust_core::providers::{is_image_name, FolderProvider};
use comicrust_core::store::{Store, StoreConfig};
use comicrust_core::{decode_image, open_book_patiently, BookFormat, ComicInfo, PageImage, PageProvider};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant, SystemTime};

/// Nom du protocole de ressources des pages.
pub const PROTOCOLE_PAGES: &str = "cr-page";
/// Pages préchargées après la page demandée (`PRELOAD_AHEAD` de la version egui).
pub const PRECHARGE_AVANT: usize = 3;
/// Pages préchargées avant la page demandée (la précédente, comme la version egui).
pub const PRECHARGE_ARRIERE: usize = 1;
/// Livres ouverts en même temps ; au-delà, le moins récemment utilisé est fermé (lecture
/// enregistrée).
pub const SESSIONS_MAX: usize = 4;
/// Cache des pages produites : plafond en octets…
pub const CACHE_OCTETS: usize = 256 * 1024 * 1024;
/// … et en nombre de pages.
pub const CACHE_ENTREES: usize = 48;
/// Attente maximale de l'ouverture d'un livre (fichier en cours de réécriture par ComicRack :
/// 20 s de patience, puis la lecture de l'index sur un NAS).
pub const DELAI_OUVERTURE: Duration = Duration::from_secs(40);
/// Attente maximale d'une page (comme les couvertures).
pub const DELAI_PAGE: Duration = Duration::from_secs(25);
/// Attente maximale de la lecture en cours d'une séance avant l'écriture de son fichier.
pub const DELAI_LIBERATION: Duration = Duration::from_secs(10);
/// Bornes du paramètre `l=` (largeur maximale demandée, en pixels).
pub const LARGEUR: (u32, u32) = (16, 16384);
/// Largeur demandée en dessous de laquelle une page est une **miniature** (bandeau du lecteur,
/// `l=256`) : elle ne déclenche pas de préchargement (sinon chaque miniature remplacerait le
/// préchargement des pages lues par celui de miniatures voisines).
pub const LARGEUR_MINIATURE: u32 = 384;
/// Qualité JPEG des pages réencodées.
const QUALITE_JPEG: u8 = 90;

// ───────────────────────────── Types de l'IPC ─────────────────────────────

/// Une page du livre. La liste est complète (pages supprimées comprises) et dans l'ordre de
/// lecture de la fiche (ou du ComicInfo.xml) : `pages[i].index == i`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// Numéro de la page dans le livre (ComicRack : rang dans la liste complète, pages
    /// supprimées comprises), celui de `CurrentPage` / `LastPageRead` ; c'est l'index des URL
    /// `cr-page`, de `pageReprise` et de `enregistrer_avancement`.
    pub index: usize,
    /// Image du fichier (rang dans l'ordre naturel des noms) : diffère de `index` quand les
    /// pages ont été réordonnées.
    pub image: usize,
    /// Taille de l'image d'origine (avant rotation) connue par la fiche ou le ComicInfo.xml,
    /// absente sinon : la vraie taille arrive avec l'image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largeur: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hauteur: Option<u32>,
    /// Page double (attribut `DoublePage` du ComicInfo.xml, ou paysage d'après la taille connue) :
    /// affichée seule en mode double page.
    pub double: bool,
    /// Type ComicRack (`FrontCover`, `Other`…) ; absent pour une page d'histoire.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_page: Option<String>,
    /// Page « Supprimée » : masquée par le lecteur (comme la version egui). Si toutes les pages
    /// le sont, aucune n'est marquée (un livre vide n'aurait pas de sens).
    pub supprimee: bool,
    /// Rotation à appliquer à l'affichage (0, 90, 180, 270) : l'image servie n'est **pas** tournée.
    pub rotation: u16,
}

/// Résultat de `ouvrir_livre` / `ouvrir_fichier`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivreOuvert {
    /// Jeton de la séance de lecture (URL des pages, avancement, fermeture).
    pub jeton: String,
    /// Identifiant du livre dans la bibliothèque ; absent pour un fichier qui n'y est pas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub titre: String,
    /// Fichier ouvert (chemin complet).
    pub fichier: String,
    /// Format (libellé de ComicRack : « eComic (ZIP) », « Document PDF »…).
    pub format: String,
    /// Nombre de pages du livre (pages supprimées comprises) = `pages.length`.
    pub nb_pages: usize,
    /// Index (numéro de page) où reprendre ; jamais une page supprimée.
    pub page_reprise: usize,
    /// Les pages de reprise de ComicRack (base) et de ComicRust (mémoire locale) diffèrent :
    /// `pageReprise` est celle de ComicRust, celle-ci celle de ComicRack (le front peut demander,
    /// comme le réglage « Reprise : demander » de la version egui).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_reprise_comicrack: Option<usize>,
    /// Livre en manga de droite à gauche (`Manga = YesAndRightToLeft`).
    pub manga: bool,
    /// Base des URL de pages : `${urlBase}/${index}?l=${largeur}`.
    pub url_base: String,
    pub pages: Vec<PageInfo>,
}

/// Réponse du protocole `cr-page`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReponsePage {
    pub statut: u16,
    pub mime: &'static str,
    pub octets: Vec<u8>,
}

impl ReponsePage {
    fn erreur(statut: u16, message: &str) -> Self {
        ReponsePage { statut, mime: "text/plain; charset=utf-8", octets: message.as_bytes().to_vec() }
    }

    fn page(p: &PageServie) -> Self {
        ReponsePage { statut: 200, mime: p.mime, octets: p.octets.clone() }
    }
}

// ───────────────────────────── URL ─────────────────────────────

/// Base des URL de pages d'une séance (`.../<jeton>`, sans barre finale).
pub fn url_base(jeton: &str) -> String {
    // WebView2 (Windows) : http://<protocole>.localhost ; ailleurs : <protocole>://localhost.
    if cfg!(windows) {
        format!("http://{PROTOCOLE_PAGES}.localhost/{jeton}")
    } else {
        format!("{PROTOCOLE_PAGES}://localhost/{jeton}")
    }
}

/// Jeton, index et largeur maximale d'une requête `cr-page` (`/<jeton>/<index>`, `l=<largeur>`).
/// `None` si le chemin est mal formé. `l` absent, nul ou illisible : taille d'origine.
pub fn analyser_uri(chemin: &str, requete: Option<&str>) -> Option<(String, usize, Option<u32>)> {
    let mut morceaux = chemin.trim_matches('/').split('/');
    let jeton = morceaux.next().filter(|j| !j.is_empty() && j.chars().all(|c| c.is_ascii_alphanumeric()))?;
    let index = morceaux.next()?.parse::<usize>().ok()?;
    if morceaux.next().is_some() {
        return None;
    }
    let largeur = requete
        .unwrap_or("")
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| *k == "l")
        .and_then(|(_, v)| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 1.0)
        .map(|v| (v.round() as u32).clamp(LARGEUR.0, LARGEUR.1));
    Some((jeton.to_string(), index, largeur))
}

// ───────────────────────────── Cache borné ─────────────────────────────

/// Page prête à servir.
#[derive(Debug)]
pub struct PageServie {
    pub mime: &'static str,
    pub octets: Vec<u8>,
}

/// (jeton, index, largeur demandée).
type ClePage = (String, usize, Option<u32>);

/// Cache des pages produites, le moins récemment utilisé évincé en premier.
pub struct CachePages {
    max_octets: usize,
    max_entrees: usize,
    octets: usize,
    entrees: VecDeque<(ClePage, Arc<PageServie>)>,
}

impl CachePages {
    pub fn nouveau(max_octets: usize, max_entrees: usize) -> Self {
        CachePages { max_octets, max_entrees: max_entrees.max(1), octets: 0, entrees: VecDeque::new() }
    }

    fn position(&self, cle: &ClePage) -> Option<usize> {
        self.entrees.iter().position(|(c, _)| c == cle)
    }

    /// Page en cache (devient la plus récente).
    pub fn prendre(&mut self, cle: &ClePage) -> Option<Arc<PageServie>> {
        let i = self.position(cle)?;
        let e = self.entrees.remove(i)?;
        let p = e.1.clone();
        self.entrees.push_back(e);
        Some(p)
    }

    pub fn contient(&self, cle: &ClePage) -> bool {
        self.position(cle).is_some()
    }

    pub fn inserer(&mut self, cle: ClePage, page: Arc<PageServie>) {
        if let Some(i) = self.position(&cle) {
            if let Some((_, p)) = self.entrees.remove(i) {
                self.octets -= p.octets.len();
            }
        }
        self.octets += page.octets.len();
        self.entrees.push_back((cle, page));
        // La page qu'on vient d'ajouter reste, même seule au-delà du plafond.
        while self.entrees.len() > 1 && (self.octets > self.max_octets || self.entrees.len() > self.max_entrees) {
            if let Some((_, p)) = self.entrees.pop_front() {
                self.octets -= p.octets.len();
            }
        }
    }

    /// Retire les pages d'une séance.
    pub fn purger(&mut self, jeton: &str) {
        self.entrees.retain(|(c, _)| c.0 != jeton);
        self.octets = self.entrees.iter().map(|(_, p)| p.octets.len()).sum();
    }

    pub fn octets(&self) -> usize {
        self.octets
    }

    pub fn len(&self) -> usize {
        self.entrees.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entrees.is_empty()
    }
}

// ───────────────────────────── Production d'une page ─────────────────────────────

/// Type MIME des octets bruts si la WebView sait les afficher tels quels.
fn mime_direct(octets: &[u8]) -> Option<&'static str> {
    Some(match image::guess_format(octets).ok()? {
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Bmp => "image/bmp",
        _ => return None,
    })
}

/// Taille d'une image d'après son en-tête (sans la décoder).
fn dimensions(octets: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(octets)).with_guessed_format().ok()?.into_dimensions().ok()
}

/// Rotation de la page (degrés), comme `loader.rs::rotated`.
fn tourner(img: PageImage, degres: u16) -> PageImage {
    match degres {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    }
}

/// Réduit à `largeur` pixels de large au plus, puis encode : JPEG si l'image est opaque, PNG
/// sinon (transparence gardée).
fn encoder(img: PageImage, largeur: Option<u32>) -> Result<PageServie, String> {
    let (l, h) = img.dimensions();
    let img = match largeur {
        Some(max) if l > max => {
            let nh = ((h as f64 * max as f64 / l.max(1) as f64).round() as u32).max(1);
            image::imageops::thumbnail(&img, max, nh)
        }
        _ => img,
    };
    let mut octets = Vec::new();
    if img.pixels().all(|p| p.0[3] == 255) {
        let rgb = image::DynamicImage::ImageRgba8(img).into_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut octets, QUALITE_JPEG)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
        Ok(PageServie { mime: "image/jpeg", octets })
    } else {
        img.write_to(&mut std::io::Cursor::new(&mut octets), image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(PageServie { mime: "image/png", octets })
    }
}

/// Produit l'image `image` du livre, tournée de `rotation` degrés et réduite à `largeur`.
/// Images JPEG/PNG/GIF/WebP/BMP non tournées et assez petites : octets du fichier tels quels
/// (aucun réencodage) ; HEIF/AVIF/JPEG XL/TIFF : décodées puis réencodées ; PDF et DjVu :
/// rendus par le cœur.
pub fn produire_page(
    livre: &mut dyn PageProvider,
    image: usize,
    rotation: u16,
    largeur: Option<u32>,
) -> Result<PageServie, String> {
    let document = matches!(livre.format(), BookFormat::Pdf | BookFormat::Djvu);
    let img = if document {
        livre.render_page(image).map_err(|e| e.to_string())?
    } else {
        let octets = livre.read_page(image).map_err(|e| e.to_string())?;
        if rotation == 0 {
            if let Some(mime) = mime_direct(&octets) {
                let tient = match largeur {
                    None => true,
                    Some(max) => dimensions(&octets).is_some_and(|(l, _)| l <= max),
                };
                if tient {
                    return Ok(PageServie { mime, octets });
                }
            }
        }
        decode_image(&octets).map_err(|e| e.to_string())?
    };
    encoder(tourner(img, rotation), largeur)
}

// ───────────────────────────── Réponses et délais ─────────────────────────────

type Repondeur = Box<dyn FnOnce(ReponsePage) + Send>;

/// Réponse à donner une seule fois (par le fil du livre, ou par le veilleur après le délai).
struct Reponse(Mutex<Option<Repondeur>>);

impl Reponse {
    fn nouvelle(f: impl FnOnce(ReponsePage) + Send + 'static) -> Arc<Self> {
        Arc::new(Reponse(Mutex::new(Some(Box::new(f)))))
    }

    fn repondre(&self, r: ReponsePage) -> bool {
        let f = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        match f {
            Some(f) => {
                f(r);
                true
            }
            None => false,
        }
    }

    fn deja_repondu(&self) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).is_none()
    }
}

type Attentes = Mutex<Vec<(Instant, Arc<Reponse>)>>;

/// Fil qui répond 504 aux requêtes restées sans réponse au-delà de leur échéance. Il s'arrête
/// quand le lecteur disparaît.
fn lancer_veilleur(attentes: Weak<Attentes>) {
    let _ = std::thread::Builder::new().name("comicrust-pages-delais".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let Some(a) = attentes.upgrade() else { return };
        let echues: Vec<Arc<Reponse>> = {
            let mut l = a.lock().unwrap_or_else(|e| e.into_inner());
            let maintenant = Instant::now();
            l.retain(|(_, r)| !r.deja_repondu());
            let (echues, restent): (Vec<_>, Vec<_>) = l.drain(..).partition(|(t, _)| *t <= maintenant);
            *l = restent;
            echues.into_iter().map(|(_, r)| r).collect()
        };
        for r in echues {
            r.repondre(ReponsePage::erreur(504, "délai dépassé (fichier inaccessible ?)"));
        }
    });
}

// ───────────────────────────── Fil d'un livre ─────────────────────────────

enum Commande {
    /// Page demandée par la WebView.
    Page { index: usize, largeur: Option<u32>, reponse: Arc<Reponse> },
    /// Page servie depuis le cache : préchargement autour d'elle.
    Voisines { index: usize, largeur: Option<u32> },
    /// Écriture du fichier imminente : lâcher le fournisseur (après la lecture en cours),
    /// oublier les préchargements, puis accuser réception.
    Liberer(Sender<()>),
    /// Écriture finie : rouvrir le fichier tout de suite (pas d'attente à la page suivante).
    Recharger,
}

/// Ce que le fil renvoie après l'ouverture.
struct Ouverture {
    format: BookFormat,
    noms: Vec<String>,
    /// Pages lues (supprimées exclues), dans l'ordre.
    plan: Vec<ReadingPage>,
    /// Liste complète des pages (fiche de la bibliothèque ou ComicInfo.xml), par numéro.
    liste: Vec<PageFiche>,
    /// Images marquées « double page » dans le ComicInfo.xml.
    doubles: HashSet<usize>,
    total: usize,
    info: Option<ComicInfo>,
}

/// Ouvre `chemin` (dossier d'images : [`FolderProvider`]).
fn ouvrir_fournisseur(chemin: &Path) -> Result<Box<dyn PageProvider>, String> {
    if chemin.is_dir() {
        return FolderProvider::open(chemin).map(|f| Box::new(f) as Box<dyn PageProvider>).map_err(|e| e.to_string());
    }
    open_book_patiently(chemin, Duration::from_secs(20)).map_err(|e| e.to_string())
}

/// Fournisseur prêt à lire : attend la fin d'une écriture en cours de ce fichier
/// ([`liberation`]), rouvre le livre s'il a été libéré ou si le fichier a changé depuis la
/// dernière ouverture (copie de `loader.rs::refresh` de la version egui ; images différentes :
/// erreur, il faut rouvrir le livre).
fn rafraichir<'a>(
    chemin: &Path,
    empreinte: &mut Option<(u64, Option<SystemTime>)>,
    livre: &'a mut Option<Box<dyn PageProvider>>,
    noms: &[String],
) -> Result<&'a mut Box<dyn PageProvider>, String> {
    if !liberation::attendre_libre(chemin, DELAI_PAGE) {
        return Err("fichier en cours d'écriture : réessayer".into());
    }
    let maintenant = if chemin.is_dir() { None } else { file_stamp(chemin) };
    if livre.is_none() || (!chemin.is_dir() && maintenant != *empreinte) {
        *livre = None;
        let neuf = ouvrir_fournisseur(chemin)?;
        if neuf.pages() != noms {
            return Err("le fichier a changé (pages différentes) : rouvrir le livre".into());
        }
        *livre = Some(neuf);
        *empreinte = if maintenant.is_some() || chemin.is_dir() { maintenant } else { file_stamp(chemin) };
    }
    Ok(livre.as_mut().expect("fournisseur ouvert"))
}

struct ContexteFil {
    chemin: PathBuf,
    /// `Some` : liste de pages de la fiche (bibliothèque) ; `None` : celle du ComicInfo.xml.
    ordre: Option<Vec<PageFiche>>,
    jeton: String,
    cache: Arc<Mutex<CachePages>>,
}

/// Lance le fil du livre et attend son ouverture (au plus [`DELAI_OUVERTURE`]).
fn lancer_fil(ctx: ContexteFil) -> Result<(Sender<Commande>, Ouverture), String> {
    let (tx, rx) = mpsc::channel::<Commande>();
    let (tx_ouv, rx_ouv) = mpsc::channel::<Result<Ouverture, String>>();
    std::thread::Builder::new()
        .name("comicrust-lecteur".into())
        .spawn(move || corps_du_fil(ctx, rx, tx_ouv))
        .map_err(|e| format!("fil du lecteur impossible : {e}"))?;
    match rx_ouv.recv_timeout(DELAI_OUVERTURE) {
        Ok(Ok(o)) => Ok((tx, o)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("délai dépassé à l'ouverture du livre (fichier inaccessible ?)".into()),
    }
}

fn corps_du_fil(ctx: ContexteFil, rx: Receiver<Commande>, tx_ouv: Sender<Result<Ouverture, String>>) {
    // Fichier en cours d'écriture (infos, pages…) : l'ouverture attend la fin.
    if !liberation::attendre_libre(&ctx.chemin, DELAI_PAGE) {
        let _ = tx_ouv.send(Err("fichier en cours d'écriture : réessayer".into()));
        return;
    }
    let mut empreinte = file_stamp(&ctx.chemin);
    let mut livre = match ouvrir_fournisseur(&ctx.chemin) {
        Ok(l) => l,
        Err(e) => {
            let _ = tx_ouv.send(Err(e));
            return;
        }
    };
    let noms = livre.pages().to_vec();
    let total = noms.len();
    let info = livre.comic_info().ok().flatten();
    // Ordre de lecture : fiche de la bibliothèque, sinon ComicInfo.xml du fichier (`loader.rs::plan`).
    // Pages « double page » du ComicInfo.xml (attribut `DoublePage`, absent des fiches).
    let pages_ci = info.as_ref().and_then(|i| i.pages.as_ref()).map(|p| p.page.as_slice()).unwrap_or(&[]);
    let doubles: HashSet<usize> = pages_ci.iter().filter(|p| p.double_page).map(|p| p.image as usize).collect();
    let source = match &ctx.ordre {
        Some(l) => l.clone(),
        None => {
            let mut l = info.as_ref().map(pages::from_comic_info).unwrap_or_default();
            // Tailles du ComicInfo.xml (non reprises par `from_comic_info`).
            for ci in pages_ci {
                if let Some(p) = l.iter_mut().find(|p| p.image == ci.image as i32) {
                    p.image_width = ci.image_width.map(|w| w.to_string());
                    p.image_height = ci.image_height.map(|h| h.to_string());
                }
            }
            l
        }
    };
    let plan = pages::reading_order(&source, total);
    let liste = pages::full_list(&source, total);
    // Numéro de page → image du fichier (pages supprimées comprises : elles restent servies).
    let images: Vec<usize> = liste.iter().map(|p| p.image.max(0) as usize).collect();
    let visibles: Vec<usize> = plan.iter().map(|p| p.page).collect();
    let ouverture = Ouverture { format: livre.format(), noms: noms.clone(), plan, liste, doubles, total, info };
    if tx_ouv.send(Ok(ouverture)).is_err() {
        return; // ouverture abandonnée (délai dépassé)
    }
    // `None` : fournisseur lâché (écriture du fichier) ; rouvert à la lecture suivante.
    let mut livre: Option<Box<dyn PageProvider>> = Some(livre);
    let mut liberations: Vec<Sender<()>> = Vec::new();
    let mut recharger = false;
    let mut urgentes: VecDeque<(usize, Option<u32>, Arc<Reponse>)> = VecDeque::new();
    let mut prechargements: VecDeque<(usize, Option<u32>)> = VecDeque::new();
    let cle = |i: usize, l: Option<u32>| (ctx.jeton.clone(), i, l);
    let cache = || ctx.cache.lock().unwrap_or_else(|e| e.into_inner());
    let voisines = |i: usize, l: Option<u32>, file: &mut VecDeque<(usize, Option<u32>)>| {
        if l.is_some_and(|l| l <= LARGEUR_MINIATURE) {
            return; // miniature : la position de lecture n'a pas changé
        }
        // Nouvelle position : les anciens préchargements ne servent plus.
        file.clear();
        // Voisines dans l'ordre de lecture, pages supprimées sautées.
        let pos = visibles.iter().position(|&n| n >= i).unwrap_or(visibles.len());
        let pos_apres = if visibles.get(pos) == Some(&i) { pos + 1 } else { pos };
        let apres = visibles.iter().skip(pos_apres).take(PRECHARGE_AVANT);
        let avant = visibles[..pos].iter().rev().take(PRECHARGE_ARRIERE);
        for &j in apres.chain(avant) {
            if !cache().contient(&cle(j, l)) {
                file.push_back((j, l));
            }
        }
    };
    let ranger = |c: Commande,
                  urgentes: &mut VecDeque<(usize, Option<u32>, Arc<Reponse>)>,
                  pre: &mut VecDeque<(usize, Option<u32>)>,
                  liberations: &mut Vec<Sender<()>>,
                  recharger: &mut bool| match c {
        // La dernière demande est la plus urgente (page affichée).
        Commande::Page { index, largeur, reponse } => urgentes.push_front((index, largeur, reponse)),
        Commande::Voisines { index, largeur } => voisines(index, largeur, pre),
        Commande::Liberer(ack) => {
            pre.clear();
            *recharger = false;
            liberations.push(ack);
        }
        Commande::Recharger => *recharger = true,
    };
    // Fichier en cours d'écriture : le fournisseur est lâché et l'on attend ici la fin de
    // l'écriture **en accusant réception des libérations** arrivées entre-temps. Sans cela, un
    // préchargement commencé juste après la réservation (`Lecteur::liberer` réserve avant
    // d'envoyer `Liberer`) bloquerait le fil dans `attendre_libre` et l'écriture attendrait
    // l'accusé de réception jusqu'à `DELAI_LIBERATION`. `false` : livre fermé.
    let attendre_ecriture = |urgentes: &mut VecDeque<(usize, Option<u32>, Arc<Reponse>)>,
                             pre: &mut VecDeque<(usize, Option<u32>)>,
                             liberations: &mut Vec<Sender<()>>,
                             recharger: &mut bool,
                             livre: &mut Option<Box<dyn PageProvider>>|
     -> bool {
        if !liberation::en_ecriture(&ctx.chemin) {
            return true;
        }
        *livre = None;
        let fin = Instant::now() + DELAI_PAGE;
        loop {
            loop {
                match rx.try_recv() {
                    Ok(c) => ranger(c, urgentes, pre, liberations, recharger),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return false,
                }
            }
            for ack in liberations.drain(..) {
                let _ = ack.send(());
            }
            if !liberation::en_ecriture(&ctx.chemin) || Instant::now() >= fin {
                return true;
            }
            liberation::attendre_libre(&ctx.chemin, Duration::from_millis(50));
        }
    };
    loop {
        if urgentes.is_empty() && prechargements.is_empty() && !recharger {
            match rx.recv() {
                Ok(c) => ranger(c, &mut urgentes, &mut prechargements, &mut liberations, &mut recharger),
                Err(_) => return, // livre fermé
            }
        }
        loop {
            match rx.try_recv() {
                Ok(c) => ranger(c, &mut urgentes, &mut prechargements, &mut liberations, &mut recharger),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        // Libération : entre deux lectures seulement (aucune lecture en cours ici).
        if !liberations.is_empty() {
            livre = None;
            for ack in liberations.drain(..) {
                let _ = ack.send(());
            }
        }
        if recharger {
            recharger = false;
            if !attendre_ecriture(&mut urgentes, &mut prechargements, &mut liberations, &mut recharger, &mut livre) {
                return;
            }
            // Erreur (fichier renommé, pages différentes…) : rendue à la page suivante.
            if livre.is_none() && rafraichir(&ctx.chemin, &mut empreinte, &mut livre, &noms).is_err() {
                livre = None;
            }
        }
        let (index, largeur, reponse) = match urgentes.pop_front() {
            Some((i, l, r)) => {
                if r.deja_repondu() {
                    continue; // abandonnée (délai) : rien à produire pour elle
                }
                (i, l, Some(r))
            }
            None => match prechargements.pop_front() {
                Some((i, l)) => (i, l, None),
                None => continue,
            },
        };
        let k = cle(index, largeur);
        let deja = cache().prendre(&k);
        if deja.is_none()
            && !attendre_ecriture(&mut urgentes, &mut prechargements, &mut liberations, &mut recharger, &mut livre)
        {
            return;
        }
        let resultat = match deja {
            Some(p) => Ok(p),
            None => match images.get(index).copied() {
                None => Err((404, "page hors du livre".to_string())),
                // Image servie non tournée : la rotation est appliquée par le front.
                Some(image) => rafraichir(&ctx.chemin, &mut empreinte, &mut livre, &noms)
                    .and_then(|l| produire_page(l.as_mut(), image, 0, largeur))
                    .map(|p| {
                        let p = Arc::new(p);
                        cache().inserer(k, p.clone());
                        p
                    })
                    .map_err(|e| (500, e)),
            },
        };
        if let Some(r) = reponse {
            r.repondre(match &resultat {
                Ok(p) => ReponsePage::page(p),
                Err((statut, message)) => ReponsePage::erreur(*statut, message),
            });
            if resultat.is_ok() {
                voisines(index, largeur, &mut prechargements);
            }
        }
    }
}

// ───────────────────────────── Séances ─────────────────────────────

/// Séance de vignettes ouverte par [`Lecteur::ouvrir_vignettes`].
#[derive(Debug, Clone)]
pub struct SeanceVignettes {
    pub jeton: String,
    pub url_base: String,
    /// Noms des images du fichier (ordre naturel) : leur nombre est celui des images.
    pub noms: Vec<String>,
    /// Images marquées `DoublePage` dans le ComicInfo.xml du fichier.
    pub doubles: HashSet<usize>,
}

struct Session {
    id: Option<String>,
    chemin: PathBuf,
    tx: Sender<Commande>,
    /// Nombre de pages (liste complète).
    nb: usize,
    /// Numéros des pages lues (supprimées exclues), dans l'ordre.
    visibles: Vec<usize>,
    /// Nombre d'images du fichier.
    total: usize,
    /// Numéro de la page courante et de la page la plus loin atteinte.
    courante: usize,
    plus_loin: usize,
    /// Mode autonome : base où écrire la lecture.
    store: Option<StoreConfig>,
    /// Heure d'ouverture (`Opened` de la fiche ComicRack).
    ouvert_a: chrono::DateTime<chrono::FixedOffset>,
    /// Lecture déjà écrite dans la base (une fois par séance, comme `Book::stored`).
    enregistree: bool,
    touchee: Instant,
}

/// Lecture à écrire dans la base (numéros de page du livre).
struct Ecriture {
    id: String,
    /// Mode autonome : base de ComicRust ; `None` : base ComicRack si l'écriture y est permise.
    cfg: Option<StoreConfig>,
    courante: u32,
    vue: u32,
    /// Pages du fichier (0 : inconnu).
    pages: u32,
    ouvert_a: chrono::DateTime<chrono::FixedOffset>,
}

/// Lecture d'un livre dans le lecteur, pour la fiche ComicRack (`Reading` de la version egui).
#[derive(Debug, Clone, Copy)]
pub struct LectureComicrack {
    /// Page courante (à partir de 0).
    pub courante: u32,
    /// Page la plus loin affichée pendant cette lecture.
    pub plus_loin: u32,
    /// Pages du fichier (0 : inconnu).
    pub pages: u32,
    /// Heure d'ouverture du livre.
    pub ouvert_a: chrono::DateTime<chrono::FixedOffset>,
}

/// Fiche ComicRack à écrire après une lecture (copie de `reading_edit` de la version egui) :
/// `CurrentPage`, `LastPageRead` (maximum, jamais reculée), `Opened` + `OpenCount + 1`,
/// `NewPages = 0`, et `PageCount` seulement s'il manque.
pub fn edition_lecture_comicrack(fiche: &LibraryBook, l: &LectureComicrack) -> BookEdit {
    let mut e = BookEdit::new(&fiche.id)
        .set("CurrentPage", Value::Int(l.courante.into()))
        .set("LastPageRead", Value::Max(l.plus_loin.max(l.courante).into()))
        .opened(l.ouvert_a)
        .set("NewPages", Value::Int(0));
    if l.pages > 0 && fiche.page_count.unwrap_or(0) == 0 {
        e = e.set("PageCount", Value::Int(l.pages.into()));
    }
    e
}

impl Session {
    /// Lecture à écrire, une seule fois par séance (marque la séance comme enregistrée).
    /// `None` : déjà fait ou fichier hors bibliothèque.
    fn a_ecrire(&mut self) -> Option<Ecriture> {
        if self.enregistree || self.nb == 0 {
            return None;
        }
        self.enregistree = true;
        Some(Ecriture {
            id: self.id.clone()?,
            cfg: self.store.clone(),
            courante: self.courante as u32,
            vue: self.plus_loin as u32,
            pages: self.total.max(self.nb) as u32,
            ouvert_a: self.ouvert_a,
        })
    }
}

/// Livres ouverts ; partageable entre les commandes, le protocole et les fils (clonage bon marché).
#[derive(Clone)]
pub struct Lecteur {
    biblio: Biblio,
    memoire: Arc<Memoire>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    cache: Arc<Mutex<CachePages>>,
    attentes: Arc<Attentes>,
    delai_page: Duration,
}

impl Lecteur {
    pub fn nouveau(data: PathBuf, biblio: Biblio) -> Self {
        Self::avec_reglages(data, biblio, CachePages::nouveau(CACHE_OCTETS, CACHE_ENTREES), DELAI_PAGE)
    }

    /// Cache et délai choisis (tests).
    pub fn avec_reglages(data: PathBuf, biblio: Biblio, cache: CachePages, delai_page: Duration) -> Self {
        let attentes: Arc<Attentes> = Arc::default();
        lancer_veilleur(Arc::downgrade(&attentes));
        Lecteur {
            biblio,
            memoire: Arc::new(Memoire::nouvelle(data)),
            sessions: Arc::default(),
            cache: Arc::new(Mutex::new(cache)),
            attentes,
            delai_page,
        }
    }

    fn verrou(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Octets et nombre de pages en cache (tests, diagnostic).
    pub fn etat_cache(&self) -> (usize, usize) {
        let c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        (c.octets(), c.len())
    }

    /// Ouvre un livre de la bibliothèque.
    pub fn ouvrir_livre(&self, id: &str) -> Result<LivreOuvert, String> {
        let fiche = self.biblio.livre(id).ok_or_else(|| format!("livre introuvable : {id}"))?;
        let fichier = fiche.file.as_deref().map(str::trim).filter(|f| !f.is_empty()).ok_or("livre sans fichier")?;
        let chemin = PathBuf::from(fichier);
        self.ouvrir_chemin(chemin, Some(fiche), None)
    }

    /// Ouvre un fichier (ou dossier d'images) hors bibliothèque. S'il est dans la bibliothèque,
    /// sa fiche sert (ordre des pages, reprise), comme la version egui. Une image seule ouvre
    /// son dossier, à cette image.
    pub fn ouvrir_fichier(&self, chemin: &str) -> Result<LivreOuvert, String> {
        let chemin = PathBuf::from(chemin.trim().trim_matches('"').trim());
        if chemin.as_os_str().is_empty() {
            return Err("chemin vide".into());
        }
        if !chemin.exists() {
            return Err(format!("fichier introuvable : {}", chemin.display()));
        }
        let nom = chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if chemin.is_file() && is_image_name(&nom) {
            let dossier = chemin.parent().map(Path::to_path_buf).unwrap_or_default();
            return self.ouvrir_chemin(dossier, None, Some(nom));
        }
        let fiche = self.biblio.livre_par_chemin(&chemin);
        self.ouvrir_chemin(chemin, fiche, None)
    }

    fn ouvrir_chemin(
        &self,
        chemin: PathBuf,
        fiche: Option<LibraryBook>,
        image_depart: Option<String>,
    ) -> Result<LivreOuvert, String> {
        let jeton = comicrust_core::store::new_book_id().replace('-', "");
        let ordre = fiche.as_ref().map(|f| f.pages.page.clone());
        let (tx, o) =
            lancer_fil(ContexteFil { chemin: chemin.clone(), ordre, jeton: jeton.clone(), cache: self.cache.clone() })?;
        let visibles: Vec<usize> = o.plan.iter().map(|p| p.page).collect();
        let lues: HashSet<usize> = visibles.iter().copied().collect();
        // Page supprimée : la suivante visible (`Book::position_of`), sinon la dernière visible.
        let visible_de = |numero: usize| -> usize {
            visibles.iter().copied().find(|&n| n >= numero).or(visibles.last().copied()).unwrap_or(0)
        };
        // Reprise (`open_book_at`, `Start::Auto`) : ComicRust (mémoire locale) et ComicRack (base).
        let crust = self.memoire.lire(&chemin).map(|a| a.page).filter(|p| *p > 0);
        let crack = fiche.as_ref().and_then(|f| f.current_page).map(|p| p as usize).filter(|p| *p > 0);
        let (reprise, autre) = match (crack, crust) {
            (Some(a), Some(b)) if a != b => (b, Some(a)),
            (a, b) => (b.or(a).unwrap_or(0), None),
        };
        let mut page_reprise = visible_de(reprise);
        let mut page_reprise_comicrack = autre.map(visible_de).filter(|r| *r != page_reprise);
        if let Some(nom) = &image_depart {
            if let Some(img) = o.noms.iter().position(|n| n == nom) {
                page_reprise = o.liste.iter().position(|p| p.image == img as i32).unwrap_or(0);
                page_reprise_comicrack = None;
            }
        }
        let dim = |v: &Option<String>| v.as_deref().and_then(|s| s.trim().parse::<u32>().ok()).filter(|n| *n > 0);
        let pages = o
            .liste
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let image = f.image.max(0) as usize;
                let (l, h) = (dim(&f.image_width), dim(&f.image_height));
                let type_page =
                    f.kind.as_deref().map(str::trim).filter(|k| !k.is_empty() && *k != "Story").map(str::to_string);
                PageInfo {
                    index,
                    image,
                    largeur: l,
                    hauteur: h,
                    double: o.doubles.contains(&image) || matches!((l, h), (Some(l), Some(h)) if l > h),
                    type_page,
                    supprimee: !lues.contains(&index),
                    rotation: pages::rotation_degrees(f),
                }
            })
            .collect::<Vec<_>>();
        let rtl = |m: &Option<String>| m.as_deref().is_some_and(|m| m.trim().eq_ignore_ascii_case("YesAndRightToLeft"));
        let manga = match &fiche {
            Some(f) => rtl(&f.manga),
            None => o.info.as_ref().is_some_and(|i| rtl(&i.manga)),
        };
        let titre = match &fiche {
            Some(f) => f
                .title
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| f.caption()),
            None => chemin.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        };
        let id = fiche.as_ref().map(|f| f.id.clone());
        let store = if id.is_some() { self.biblio.store_autonome() } else { None };
        let pages_len = pages.len();
        let ouvert = LivreOuvert {
            jeton: jeton.clone(),
            id: id.clone(),
            titre,
            fichier: chemin.display().to_string(),
            format: o.format.label().to_string(),
            nb_pages: pages.len(),
            page_reprise,
            page_reprise_comicrack,
            manga,
            url_base: url_base(&jeton),
            pages,
        };
        let session = Session {
            id,
            chemin,
            tx,
            nb: pages_len,
            visibles,
            total: o.total,
            courante: page_reprise,
            plus_loin: page_reprise,
            store,
            ouvert_a: comicrust_core::db::write::now(),
            enregistree: false,
            touchee: Instant::now(),
        };
        self.inserer_session(jeton, session);
        Ok(ouvert)
    }

    /// Ouvre une **séance de vignettes** sur `chemin` (onglet Pages) : index des URL `cr-page`
    /// = rang de l'image dans le fichier (ordre brut), aucun avancement enregistré.
    pub fn ouvrir_vignettes(&self, chemin: &Path) -> Result<SeanceVignettes, String> {
        let jeton = comicrust_core::store::new_book_id().replace('-', "");
        let (tx, o) = lancer_fil(ContexteFil {
            chemin: chemin.to_path_buf(),
            ordre: Some(Vec::new()),
            jeton: jeton.clone(),
            cache: self.cache.clone(),
        })?;
        let session = Session {
            id: None,
            chemin: chemin.to_path_buf(),
            tx,
            nb: o.total,
            visibles: (0..o.total).collect(),
            total: o.total,
            courante: 0,
            plus_loin: 0,
            store: None,
            ouvert_a: comicrust_core::db::write::now(),
            enregistree: true,
            touchee: Instant::now(),
        };
        self.inserer_session(jeton.clone(), session);
        Ok(SeanceVignettes { url_base: url_base(&jeton), jeton, noms: o.noms, doubles: o.doubles })
    }

    /// Ajoute une séance ; trop de livres ouverts : le moins récemment utilisé est fermé
    /// (lecture enregistrée).
    fn inserer_session(&self, jeton: String, session: Session) {
        let evinces: Vec<(String, Session)> = {
            let mut g = self.verrou();
            g.insert(jeton.clone(), session);
            let mut ev = Vec::new();
            while g.len() > SESSIONS_MAX {
                let Some(vieux) =
                    g.iter().filter(|(j, _)| **j != jeton).min_by_key(|(_, s)| s.touchee).map(|(j, _)| j.clone())
                else {
                    break;
                };
                if let Some(s) = g.remove(&vieux) {
                    ev.push((vieux, s));
                }
            }
            ev
        };
        for (j, mut s) in evinces {
            if let Some(e) = s.a_ecrire() {
                let _ = self.ecrire_base(e);
            }
            self.cache.lock().unwrap_or_else(|e| e.into_inner()).purger(&j);
        }
    }

    /// Préfixe des URL de pages d'une séance ouverte.
    pub fn url_page_base(&self, jeton: &str) -> Result<String, String> {
        if self.verrou().contains_key(jeton) {
            Ok(url_base(jeton))
        } else {
            Err(format!("livre non ouvert (jeton inconnu) : {jeton}"))
        }
    }

    /// Sert une page au protocole `cr-page`. Le cache est consulté tout de suite (réponse
    /// immédiate) ; sinon la demande passe au fil du livre, et une réponse 504 part après le
    /// délai si la page n'est pas prête. `repondre` est appelé exactement une fois.
    pub fn servir(
        &self,
        jeton: &str,
        index: usize,
        largeur: Option<u32>,
        repondre: impl FnOnce(ReponsePage) + Send + 'static,
    ) {
        let tx = {
            let mut g = self.verrou();
            match g.get_mut(jeton) {
                Some(s) if index < s.nb => {
                    s.touchee = Instant::now();
                    s.tx.clone()
                }
                Some(_) => return repondre(ReponsePage::erreur(404, "page hors du livre")),
                None => return repondre(ReponsePage::erreur(404, "livre non ouvert (jeton inconnu)")),
            }
        };
        let deja = self.cache.lock().unwrap_or_else(|e| e.into_inner()).prendre(&(jeton.to_string(), index, largeur));
        if let Some(p) = deja {
            let _ = tx.send(Commande::Voisines { index, largeur });
            return repondre(ReponsePage::page(&p));
        }
        let reponse = Reponse::nouvelle(repondre);
        self.attentes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((Instant::now() + self.delai_page, reponse.clone()));
        if tx.send(Commande::Page { index, largeur, reponse: reponse.clone() }).is_err() {
            reponse.repondre(ReponsePage::erreur(500, "le fil du livre s'est arrêté"));
        }
    }

    /// Note la page courante (index = numéro de page). `termine` : la dernière page a été vue
    /// (livre lu jusqu'au bout) ; la lecture est alors écrite dans la base sans attendre la
    /// fermeture.
    pub fn enregistrer_avancement(&self, jeton: &str, page: usize, termine: bool) -> Result<(), String> {
        let (chemin, numero, numero_loin, total) = {
            let mut g = self.verrou();
            let s = g.get_mut(jeton).ok_or_else(|| format!("livre non ouvert (jeton inconnu) : {jeton}"))?;
            if s.nb == 0 {
                return Ok(());
            }
            s.courante = page.min(s.nb - 1);
            s.plus_loin = s.plus_loin.max(s.courante);
            if termine {
                s.plus_loin = s.plus_loin.max(s.visibles.last().copied().unwrap_or(s.nb - 1));
            }
            s.touchee = Instant::now();
            (s.chemin.clone(), s.courante, s.plus_loin, s.total.max(s.nb))
        };
        self.memoire.noter(&chemin, numero, numero_loin, total)?;
        if termine {
            // Écrit hors du verrou des séances (la base peut être lente).
            let e = self.verrou().get_mut(jeton).and_then(Session::a_ecrire);
            if let Some(e) = e {
                self.ecrire_base(e)?;
            }
        }
        Ok(())
    }

    /// Ferme un livre : lecture écrite dans la base (si pas déjà fait), fil arrêté, pages
    /// retirées du cache. Jeton inconnu : rien à faire.
    pub fn fermer_livre(&self, jeton: &str) -> Result<(), String> {
        let s = self.verrou().remove(jeton);
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).purger(jeton);
        match s.and_then(|mut s| s.a_ecrire()) {
            Some(e) => self.ecrire_base(e),
            None => Ok(()),
        }
    }

    /// Ferme tous les livres (fermeture de l'application).
    pub fn fermer_tout(&self) {
        let toutes: Vec<String> = self.verrou().keys().cloned().collect();
        for j in toutes {
            if let Err(e) = self.fermer_livre(&j) {
                eprintln!("ComicRust : lecture non enregistrée : {e}");
            }
        }
    }

    /// **Libère** les fichiers `chemins` avant de les écrire (infos, pages, conversion,
    /// renommage) : nouvelles lectures de ces fichiers suspendues (pages, vignettes, couvertures :
    /// [`liberation`]), séances qui les affichent arrêtées après leur lecture en cours (fournisseur
    /// lâché), pages en cache oubliées. Quand la garde rendue disparaît (écriture finie), les
    /// séances rechargent le fichier. Comme `PagesTab::release` / `reload` de la version egui.
    pub fn liberer(&self, chemins: &[PathBuf]) -> GardeLiberation {
        let reservation = liberation::reserver(chemins.iter().map(PathBuf::as_path));
        let txs: Vec<Sender<Commande>> = self
            .verrou()
            .values()
            .filter(|s| chemins.iter().any(|c| meme_chemin(&s.chemin, c)))
            .map(|s| s.tx.clone())
            .collect();
        let acks: Vec<Receiver<()>> = txs
            .iter()
            .filter_map(|tx| {
                let (a, r) = mpsc::channel();
                tx.send(Commande::Liberer(a)).ok().map(|()| r)
            })
            .collect();
        // Lecture en cours sur un NAS lent : on ne l'attend pas indéfiniment (l'écriture refait
        // elle-même quelques tentatives si le fichier est encore tenu).
        let fin = Instant::now() + DELAI_LIBERATION;
        for r in acks {
            let _ = r.recv_timeout(fin.saturating_duration_since(Instant::now()));
        }
        for c in chemins {
            self.oublier_fichier(c);
        }
        GardeLiberation { reservation: Some(reservation), lecteur: self.clone(), chemins: chemins.to_vec() }
    }

    /// Oublie les pages en cache des livres ouverts sur `chemin` (fichier réécrit) ; le fil du
    /// livre relit le fichier à la page suivante (empreinte changée).
    pub fn oublier_fichier(&self, chemin: &Path) {
        let jetons: Vec<String> =
            self.verrou().iter().filter(|(_, s)| meme_chemin(&s.chemin, chemin)).map(|(j, _)| j.clone()).collect();
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        for j in jetons {
            c.purger(&j);
        }
    }

    /// Écrit la lecture préparée par [`Session::a_ecrire`] : base de ComicRust en mode autonome
    /// (page courante, page la plus loin, date, comme `store_progress` de la version egui), fiche
    /// ComicRack en mode lié avec l'option « Lecture et écriture » ; rien sinon.
    fn ecrire_base(&self, e: Ecriture) -> Result<(), String> {
        let Some(cfg) = e.cfg.clone() else {
            return self.ecrire_comicrack(&e);
        };
        let derniere = self.biblio.livre(&e.id).and_then(|f| f.last_page_read).unwrap_or(0).max(e.vue);
        let ouvert = comicrust_core::scanner::now_iso();
        let compteur = Store::open_existing(&cfg)
            .and_then(|mut st| st.set_progress(&e.id, e.courante, derniere, &ouvert))
            .map_err(|err| format!("lecture non enregistrée dans la base ComicRust : {err}"))?;
        self.biblio.signaler_ecriture_autonome([e.id.clone()], Some(compteur));
        self.biblio.noter_lecture(&e.id, e.courante, derniere, &ouvert);
        Ok(())
    }

    /// Mode lié, écriture permise : lecture dans la fiche ComicRack (seuls ces éléments sont
    /// réécrits, protocole des compteurs de `db/write.rs`), fiche relue reportée dans la
    /// bibliothèque affichée. La lecture reste de toute façon dans `data\avancement.json`.
    fn ecrire_comicrack(&self, e: &Ecriture) -> Result<(), String> {
        let Some(cfg) = self.biblio.config_comicrack_ecriture() else { return Ok(()) };
        let Some(fiche) = self.biblio.livre(&e.id) else { return Ok(()) };
        let l = LectureComicrack { courante: e.courante, plus_loin: e.vue, pages: e.pages, ouvert_a: e.ouvert_a };
        let rapport =
            comicrust_core::db::update_books(&cfg, &[edition_lecture_comicrack(&fiche, &l)]).map_err(|err| {
                format!("lecture non enregistrée dans la base de ComicRack (gardée dans ComicRust) : {err}")
            })?;
        self.biblio.signaler_rapport_comicrack(&rapport);
        let fraiches: Vec<LibraryBook> = rapport.written.iter().map(|w| w.book.clone()).collect();
        self.biblio.remplacer_fiches(&fraiches, false);
        if !rapport.is_complete() {
            return Err(format!(
                "lecture non enregistrée dans la base de ComicRack (gardée dans ComicRust) : {}",
                rapport.summary()
            ));
        }
        Ok(())
    }
}

/// Écriture en cours de fichiers libérés par [`Lecteur::liberer`] ; à sa disparition, les
/// lectures reprennent et les séances concernées rechargent le fichier.
#[must_use = "la libération prend fin dès que la garde est abandonnée"]
pub struct GardeLiberation {
    reservation: Option<liberation::Reservation>,
    lecteur: Lecteur,
    chemins: Vec<PathBuf>,
}

impl Drop for GardeLiberation {
    fn drop(&mut self) {
        drop(self.reservation.take());
        for c in &self.chemins {
            self.lecteur.oublier_fichier(c);
        }
        let txs: Vec<Sender<Commande>> = self
            .lecteur
            .verrou()
            .values()
            .filter(|s| self.chemins.iter().any(|c| meme_chemin(&s.chemin, c)))
            .map(|s| s.tx.clone())
            .collect();
        for tx in txs {
            let _ = tx.send(Commande::Recharger);
        }
    }
}

/// Même fichier ? (casse ignorée, comme les chemins Windows de la bibliothèque)
fn meme_chemin(a: &Path, b: &Path) -> bool {
    a == b || a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(test)]
#[path = "tests_lecteur.rs"]
pub(crate) mod tests;
