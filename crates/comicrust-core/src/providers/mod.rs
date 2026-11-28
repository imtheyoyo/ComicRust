//! Fournisseurs de pages — équivalent des `ComicProvider` de ComicRack.Engine.
//!
//! Comme ComicRack, le format est d'abord reconnu **par signature** (un `.cbr`
//! qui est en réalité un ZIP s'ouvre quand même), puis par extension.
//!
//! | Format     | Extensions        | Moteur                          |
//! |------------|-------------------|---------------------------------|
//! | ZIP        | cbz, zip          | `zip`                           |
//! | RAR 4/5    | cbr, rar          | `unrar-ng` (UnRAR de RARLAB)    |
//! | 7-Zip      | cb7, 7z           | `sevenz-rust2` (pur Rust)       |
//! | TAR        | cbt, tar          | `tar`                           |
//! | PDF        | pdf               | PDFium (`pdfium.dll` à côté)    |
//! | DjVu       | djvu, djv         | `djvu-rs` (pur Rust)            |
//! | WebComic   | cbw               | XML ComicRack + HTTP            |
//! | Dossier    | —                 | système de fichiers             |
//!
//! **Aucun fournisseur ne garde son fichier ouvert** entre deux lectures (ZIP, TAR et PDF
//! passent par [`on_demand::OnDemandFile`], RAR et 7z rouvrent l'archive à chaque lecture,
//! DjVu la lit en mémoire) : sous Windows, un livre affiché peut ainsi être réécrit,
//! renommé ou déplacé.

mod djvu;
mod folder;
pub mod heif;
pub mod on_demand;
mod pdf;
mod rar;
mod sevenz;
mod tar;
mod webcomic;
mod zip;

use std::io::Read;
use std::path::Path;

pub use self::djvu::DjvuProvider;
pub use self::folder::FolderProvider;
pub use self::pdf::{pdfium_status, PdfProvider};
pub use self::rar::RarProvider;
pub(crate) use self::sevenz::read_entries as sevenz_read_entries;
pub use self::sevenz::SevenZipProvider;
pub use self::tar::TarProvider;
pub use self::webcomic::{WebComic, WebComicProvider};
pub use self::zip::ZipProvider;

use crate::{ComicInfo, Error};

/// Formats de livres pris en charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BookFormat {
    Zip,
    Rar,
    SevenZip,
    Tar,
    Pdf,
    Djvu,
    WebComic,
    Folder,
}

impl BookFormat {
    pub fn label(self) -> &'static str {
        match self {
            BookFormat::Zip => "eComic (ZIP)",
            BookFormat::Rar => "eComic (RAR)",
            BookFormat::SevenZip => "eComic (7z)",
            BookFormat::Tar => "eComic (TAR)",
            BookFormat::Pdf => "Document PDF",
            BookFormat::Djvu => "Document DjVu",
            BookFormat::WebComic => "eComic (WebComic)",
            BookFormat::Folder => "Dossier d'images",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "cbz" | "zip" => BookFormat::Zip,
            "cbr" | "rar" => BookFormat::Rar,
            "cb7" | "7z" => BookFormat::SevenZip,
            "cbt" | "tar" => BookFormat::Tar,
            "pdf" => BookFormat::Pdf,
            "djvu" | "djv" => BookFormat::Djvu,
            "cbw" => BookFormat::WebComic,
            _ => return None,
        })
    }

    /// Reconnaissance par les premiers octets du fichier.
    pub fn sniff(head: &[u8]) -> Option<Self> {
        if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
            Some(BookFormat::Zip)
        } else if head.starts_with(b"Rar!\x1a\x07") {
            Some(BookFormat::Rar)
        } else if head.starts_with(b"7z\xbc\xaf\x27\x1c") {
            Some(BookFormat::SevenZip)
        } else if head.starts_with(b"%PDF") {
            Some(BookFormat::Pdf)
        } else if head.starts_with(b"AT&TFORM")
            || head.starts_with(b"FORM") && head.get(8..12).is_some_and(|t| t.starts_with(b"DJV"))
        {
            Some(BookFormat::Djvu)
        } else if head.len() > 262 && &head[257..262] == b"ustar" {
            Some(BookFormat::Tar)
        } else {
            let text = String::from_utf8_lossy(&head[..head.len().min(512)]);
            text.contains("<WebComic").then_some(BookFormat::WebComic)
        }
    }

    /// Format d'un chemin : dossier, puis signature, puis extension.
    pub fn detect(path: &Path) -> Option<Self> {
        if path.is_dir() {
            return Some(BookFormat::Folder);
        }
        Self::detect_file(path).0
    }

    /// Format d'un **fichier** (signature, puis extension), et le fichier resté ouvert
    /// (position quelconque) pour le lire sans le rouvrir : sur un partage réseau, chaque
    /// ouverture est un aller-retour. `None` pour le fichier s'il n'a pas pu être ouvert.
    pub(crate) fn detect_file(path: &Path) -> (Option<Self>, Option<std::fs::File>) {
        let mut head = vec![0u8; 512];
        let mut file = std::fs::File::open(path).ok();
        if let Some(f) = file.as_mut() {
            let mut n = 0;
            while n < head.len() {
                match f.read(&mut head[n..]) {
                    Ok(0) | Err(_) => break,
                    Ok(k) => n += k,
                }
            }
            head.truncate(n);
            if let Some(fmt) = Self::sniff(&head) {
                return (Some(fmt), file);
            }
        }
        (path.extension().and_then(|e| e.to_str()).and_then(Self::from_extension), file)
    }
}

/// Extensions de livres proposées dans les dialogues d'ouverture.
pub const BOOK_EXTENSIONS: &[&str] =
    &["cbz", "zip", "cbr", "rar", "cb7", "7z", "cbt", "tar", "pdf", "djvu", "djv", "cbw"];

/// Extensions d'image reconnues dans les archives.
pub const IMAGE_EXTS: &[&str] =
    &["jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "tif", "tiff", "webp", "jxl", "heic", "heif", "hif", "avif"];

pub fn is_book_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| BOOK_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Nom d'entrée d'archive considéré comme une page.
pub fn is_image_name(name: &str) -> bool {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    // ComicRack ignore les fichiers ressources macOS "._xxx" (chaîne "._" relevée).
    if file.starts_with("._") || lower.contains("__macosx/") || file == "thumbs.db" {
        return false;
    }
    file.rsplit_once('.').is_some_and(|(_, ext)| IMAGE_EXTS.contains(&ext))
}

/// Tri "naturel" des noms de pages (page2 < page10), comme ComicRack.
pub fn sort_pages(names: &mut [String]) {
    names.sort_by(|a, b| compare_natural(a, b));
}

/// Comparaison « naturelle » de ComicRack : sans casse, les suites de chiffres comparées
/// **par leur valeur** (`img_9` < `img_010`, zéros de tête ignorés pour la valeur), les
/// espaces comptent comme des caractères. À valeur égale (`9` et `09`), le nombre le plus
/// court passe d'abord, puis l'ordre des octets départage : l'ordre est total et stable.
///
/// (`natord` comparait `010` à `9` comme des décimales, « 0,10 » < « 0,9 » : `img_010`
/// passait avant `img_9`, et ignorait les espaces.)
pub fn compare_natural(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    // Premier écart de longueur entre nombres de même valeur (départage final).
    let mut zeros = Ordering::Equal;
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (n, m) = (take(&mut x), take(&mut y));
                let (vn, vm) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let ord = vn.len().cmp(&vm.len()).then_with(|| vn.cmp(vm));
                if ord != Ordering::Equal {
                    return ord;
                }
                if zeros == Ordering::Equal {
                    zeros = n.len().cmp(&m.len());
                }
            }
            (Some(c), Some(d)) => {
                let ord = c.to_lowercase().cmp(d.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                x.next();
                y.next();
            }
        }
    }
    zeros.then_with(|| a.cmp(b))
}

/// Taille maximale d'une entrée d'archive (page) lue en mémoire : au-delà, l'archive est
/// considérée comme corrompue ou piégée (« bombe » ZIP, taille déclarée aberrante).
pub const MAX_ENTRY_SIZE: u64 = 256 * 1024 * 1024;

/// Taille maximale d'un `ComicInfo.xml` (quelques Ko en pratique).
pub const MAX_INFO_SIZE: u64 = 8 * 1024 * 1024;

/// Mémoire réservée d'avance au plus pour une entrée (la taille déclarée n'est qu'une
/// indication : elle peut être fausse).
const PREALLOC_MAX: u64 = 16 * 1024 * 1024;

/// Taille maximale admise pour l'entrée `name` (ComicInfo.xml ou page).
pub(crate) fn entry_limit(name: &str) -> u64 {
    if is_comic_info_name(name) {
        MAX_INFO_SIZE
    } else {
        MAX_ENTRY_SIZE
    }
}

/// Erreur « entrée trop volumineuse ».
pub(crate) fn too_big(name: &str, max: u64) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("entrée d'archive trop volumineuse (plus de {} Mio) : {name}", max >> 20),
    )
}

/// Lit une entrée d'archive en mémoire **sans dépasser `max` octets** : refus immédiat si
/// la taille déclarée dépasse `max`, réservation d'avance plafonnée, lecture coupée à
/// `max + 1` octets (une entrée qui ment sur sa taille est refusée, pas tronquée).
pub(crate) fn read_bounded(r: &mut dyn Read, declared: u64, max: u64, name: &str) -> std::io::Result<Vec<u8>> {
    if declared > max {
        return Err(too_big(name, max));
    }
    let mut buf = Vec::with_capacity(declared.min(PREALLOC_MAX) as usize);
    r.take(max.saturating_add(1)).read_to_end(&mut buf)?;
    if buf.len() as u64 > max {
        return Err(too_big(name, max));
    }
    Ok(buf)
}

pub(crate) fn is_comic_info_name(name: &str) -> bool {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    lower == "comicinfo.xml" || lower.ends_with("/comicinfo.xml")
}

/// Sépare une liste d'entrées d'archive en (pages triées, ComicInfo.xml).
pub(crate) fn index_entries<I: IntoIterator<Item = String>>(names: I) -> (Vec<String>, Option<String>) {
    let mut pages = Vec::new();
    let mut info = None;
    for name in names {
        if is_comic_info_name(&name) {
            // Préférer celui à la racine.
            if info.as_ref().is_none_or(|cur: &String| name.len() < cur.len()) {
                info = Some(name);
            }
        } else if is_image_name(&name) {
            pages.push(name);
        }
    }
    sort_pages(&mut pages);
    (pages, info)
}

pub fn parse_comic_info(bytes: &[u8]) -> Result<ComicInfo, Error> {
    let text = String::from_utf8_lossy(bytes);
    ComicInfo::from_xml(text.trim_start_matches('\u{feff}'))
}

/// Image décodée prête à l'affichage.
pub type PageImage = image::RgbaImage;

/// Décode les octets d'une page (JPEG, PNG, GIF, BMP, TIFF, WebP, JPEG XL, HEIF, AVIF).
pub fn decode_image(bytes: &[u8]) -> Result<PageImage, Error> {
    if heif::is_heif_family(bytes) {
        return heif::decode(bytes);
    }
    if bytes.starts_with(&[0xFF, 0x0A]) || bytes.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return decode_jxl(bytes);
    }
    let img = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?.decode()?;
    Ok(img.into_rgba8())
}

/// Plafond d'allocation du décodage JPEG XL (aligné sur la limite par défaut du crate `image`).
const MAX_DECODE_BYTES: u64 = 512 * 1024 * 1024;

/// Refuse une image JPEG XL dont les dimensions dépasseraient le plafond de décodage
/// (4 canaux de `f32` par pixel au pire).
fn verifier_taille_jxl(w: u32, h: u32) -> Result<(), Error> {
    let octets = (w as u64).saturating_mul(h as u64).saturating_mul(16);
    if w == 0 || h == 0 || octets > MAX_DECODE_BYTES {
        return Err(Error::Decode(format!("JPEG XL : dimensions refusées ({w} × {h})")));
    }
    Ok(())
}

fn decode_jxl(bytes: &[u8]) -> Result<PageImage, Error> {
    let jxl = jxl_oxide::JxlImage::builder()
        .read(std::io::Cursor::new(bytes))
        .map_err(|e| Error::Decode(format!("JPEG XL : {e}")))?;
    // Dimensions annoncées vérifiées AVANT le rendu : un en-tête forgé ne doit pas
    // provoquer une allocation démesurée (même plafond que le crate `image`, 512 Mio).
    verifier_taille_jxl(jxl.width(), jxl.height())?;
    let render = jxl.render_frame(0).map_err(|e| Error::Decode(format!("JPEG XL : {e}")))?;
    let mut stream = render.stream();
    let (w, h, ch) = (stream.width(), stream.height(), stream.channels());
    verifier_taille_jxl(w, h)?;
    let n = (w as u64)
        .checked_mul(h as u64)
        .and_then(|p| p.checked_mul(ch as u64))
        .filter(|n| n.saturating_mul(4) <= MAX_DECODE_BYTES)
        .ok_or_else(|| Error::Decode(format!("JPEG XL : image trop grande ({w} × {h} × {ch})")))?;
    let mut buf = vec![0f32; n as usize];
    stream.write_to_buffer(&mut buf);
    let mut out = PageImage::new(w, h);
    for (i, px) in out.pixels_mut().enumerate() {
        let s = &buf[i * ch as usize..(i + 1) * ch as usize];
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        *px = match ch {
            1 => image::Rgba([to8(s[0]), to8(s[0]), to8(s[0]), 255]),
            2 => image::Rgba([to8(s[0]), to8(s[0]), to8(s[0]), to8(s[1])]),
            3 => image::Rgba([to8(s[0]), to8(s[1]), to8(s[2]), 255]),
            _ => image::Rgba([to8(s[0]), to8(s[1]), to8(s[2]), to8(s[3])]),
        };
    }
    Ok(out)
}

pub(crate) fn encode_png(img: &PageImage) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// Un livre ouvert : liste de pages + accès aux images + métadonnées.
pub trait PageProvider {
    fn format(&self) -> BookFormat;
    /// Noms des pages, dans l'ordre de lecture.
    fn pages(&self) -> &[String];
    /// Octets bruts de la page n°`index` (image encodée).
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error>;
    /// Page décodée. Les formats "document" (PDF, DjVu) la rendent directement.
    fn render_page(&mut self, index: usize) -> Result<PageImage, Error> {
        decode_image(&self.read_page(index)?)
    }
    /// `ComicInfo.xml` embarqué, s'il existe.
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error>;
}

/// Ouvre un livre en choisissant le fournisseur adapté (fabrique type `ComicBookFactory`).
pub fn open_book(path: &Path) -> Result<Box<dyn PageProvider>, Error> {
    let format = BookFormat::detect(path).ok_or_else(|| {
        Error::Unsupported(path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "?".into()))
    })?;
    Ok(match format {
        BookFormat::Folder => Box::new(FolderProvider::open(path)?),
        BookFormat::Zip => Box::new(ZipProvider::open(path)?),
        BookFormat::Rar => Box::new(RarProvider::open(path)?),
        BookFormat::SevenZip => Box::new(SevenZipProvider::open(path)?),
        BookFormat::Tar => Box::new(TarProvider::open(path)?),
        BookFormat::Pdf => Box::new(PdfProvider::open(path)?),
        BookFormat::Djvu => Box::new(DjvuProvider::open(path)?),
        BookFormat::WebComic => Box::new(WebComicProvider::open(path)?),
    })
}

/// Le fichier est-il en train d'être réécrit ? ComicRack, quand il enregistre les
/// informations d'un livre dans l'archive, renomme l'original en `<fichier>.tmp` le temps
/// d'écrire le nouveau : le fichier est alors absent ou incomplet pendant quelques secondes.
pub fn is_being_rewritten(path: &Path) -> bool {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    Path::new(&tmp).exists()
}

/// Comme [`open_book`], mais attend jusqu'à `patience` si le fichier est en cours de
/// réécriture (voir [`is_being_rewritten`]) au lieu d'échouer tout de suite.
pub fn open_book_patiently(path: &Path, patience: std::time::Duration) -> Result<Box<dyn PageProvider>, Error> {
    let deadline = std::time::Instant::now() + patience;
    loop {
        match open_book(path) {
            Ok(b) => return Ok(b),
            Err(e) => {
                if !is_being_rewritten(path) || std::time::Instant::now() >= deadline {
                    return Err(e);
                }
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        }
    }
}

#[cfg(test)]
mod rewrite_tests {
    use super::*;

    #[test]
    fn waits_while_comicrack_rewrites_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("Thanos 01.cbz");
        let tmp = dir.path().join("Thanos 01.cbz.tmp");
        // Fichier absent, original renommé en .tmp : réécriture en cours.
        crate::providers::zip::tests::write_cbz(&tmp);
        assert!(is_being_rewritten(&book));
        let (b, t) = (book.clone(), tmp.clone());
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(900));
            std::fs::copy(&t, &b).unwrap();
            std::fs::remove_file(&t).unwrap();
        });
        let start = std::time::Instant::now();
        let opened = open_book_patiently(&book, std::time::Duration::from_secs(10)).unwrap();
        assert!(!opened.pages().is_empty());
        assert!(start.elapsed() >= std::time::Duration::from_millis(800));
        writer.join().unwrap();
        // Fichier vraiment absent (pas de .tmp) : échec immédiat.
        let missing = dir.path().join("absent.cbz");
        let start = std::time::Instant::now();
        assert!(open_book_patiently(&missing, std::time::Duration::from_secs(10)).is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use image::{Rgba, RgbaImage};

    /// Petite page PNG de test, couleur dépendant de `n`.
    pub fn png(n: u8) -> Vec<u8> {
        let img = RgbaImage::from_pixel(8, 12, Rgba([n, 255 - n, 128, 255]));
        super::encode_png(&img).unwrap()
    }

    pub const NAMES: [&str; 4] = ["10.png", "2.png", "1.png", "__MACOSX/._1.png"];
    pub const INFO: &str = "<ComicInfo><Series>Thanos</Series><Number>1</Number></ComicInfo>";

    /// Vérifie un livre construit à partir de `NAMES` + `INFO`.
    pub fn check_book(book: &mut dyn super::PageProvider) {
        assert_eq!(book.pages(), ["1.png", "2.png", "10.png"]);
        let px = |bytes: Vec<u8>| super::decode_image(&bytes).unwrap().get_pixel(0, 0).0;
        assert_eq!(px(book.read_page(2).unwrap()), [10, 245, 128, 255]);
        assert_eq!(px(book.read_page(0).unwrap()), [1, 254, 128, 255]);
        let img = book.render_page(1).unwrap();
        assert_eq!((img.width(), img.height()), (8, 12));
        assert_eq!(img.get_pixel(0, 0).0, [2, 253, 128, 255]);
        assert_eq!(book.comic_info().unwrap().unwrap().series.as_deref(), Some("Thanos"));
    }

    pub fn page_number(name: &str) -> u8 {
        name.trim_end_matches(".png").rsplit('/').next().unwrap().trim_start_matches("._").parse().unwrap()
    }
}

#[cfg(test)]
mod tests_jxl {
    use super::*;

    #[test]
    fn dimensions_jpeg_xl_demesurees_refusees() {
        assert!(verifier_taille_jxl(100_000, 100_000).is_err());
        assert!(verifier_taille_jxl(u32::MAX, u32::MAX).is_err());
        assert!(verifier_taille_jxl(0, 10).is_err());
        assert!(verifier_taille_jxl(4000, 6000).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_sort_and_filter() {
        let mut v: Vec<String> = ["p10.jpg", "p2.jpg", "p1.JPG"].map(String::from).to_vec();
        sort_pages(&mut v);
        assert_eq!(v, ["p1.JPG", "p2.jpg", "p10.jpg"]);
        assert!(!is_image_name("__MACOSX/x/p1.jpg"));
        assert!(!is_image_name("dir/._p1.jpg"));
        assert!(!is_image_name("readme.txt"));
        assert!(is_image_name("Sub\\p1.WEBP"));
    }

    /// « Bombe » : l'entrée annonce peu mais en livre bien plus. Lecture coupée et refusée,
    /// sans tout charger en mémoire.
    #[test]
    fn bounded_read_refuses_lying_or_huge_entries() {
        let data = vec![7u8; 5000];
        assert_eq!(read_bounded(&mut data.as_slice(), 10, 5000, "p.jpg").unwrap().len(), 5000);
        let err = read_bounded(&mut data.as_slice(), 10, 1000, "p.jpg").unwrap_err();
        assert!(err.to_string().contains("trop volumineuse"), "{err}");
        // Lecture infinie (flux qui ne finit jamais) : s'arrête à la limite.
        let err = read_bounded(&mut std::io::repeat(0), 0, 1 << 20, "p.jpg").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(read_bounded(&mut data.as_slice(), u64::MAX, 1000, "p.jpg").is_err());
        assert_eq!(entry_limit("Sub/ComicInfo.xml"), MAX_INFO_SIZE);
        assert_eq!(entry_limit("p.jpg"), MAX_ENTRY_SIZE);
    }

    /// D3b : nombres comparés par valeur, comme ComicRack.
    #[test]
    fn natural_order_compares_numbers_by_value() {
        let mut v: Vec<String> = ["img_010.jpg", "img_9.jpg", "img_011.jpg", "img_2.jpg"].map(String::from).to_vec();
        sort_pages(&mut v);
        assert_eq!(v, ["img_2.jpg", "img_9.jpg", "img_010.jpg", "img_011.jpg"]);
        use std::cmp::Ordering::*;
        assert_eq!(compare_natural("p9", "p09"), Less, "même valeur : le plus court d'abord");
        assert_eq!(compare_natural("P1", "p1"), Less, "même texte sans casse : ordre des octets");
        assert_eq!(compare_natural("Chap 2/01.jpg", "Chap 10/01.jpg"), Less);
        assert_eq!(compare_natural("p 2.jpg", "p1.jpg"), Less, "l'espace compte");
        assert_eq!(compare_natural("a", "a1"), Less);
        assert_eq!(compare_natural("x99999999999999999999999", "x100000000000000000000000"), Less, "grands nombres");
        assert_eq!(compare_natural("abc", "abc"), Equal);
    }

    #[test]
    fn sniffing() {
        assert_eq!(BookFormat::sniff(b"PK\x03\x04...."), Some(BookFormat::Zip));
        assert_eq!(BookFormat::sniff(b"Rar!\x1a\x07\x01\x00"), Some(BookFormat::Rar));
        assert_eq!(BookFormat::sniff(b"%PDF-1.7"), Some(BookFormat::Pdf));
        assert_eq!(BookFormat::sniff(b"AT&TFORM\0\0\0\0DJVU"), Some(BookFormat::Djvu));
        assert_eq!(BookFormat::sniff(b"<?xml version=\"1.0\"?><WebComic>"), Some(BookFormat::WebComic));
        assert_eq!(BookFormat::from_extension("CB7"), Some(BookFormat::SevenZip));
    }

    /// Aucun format ne garde son fichier ouvert après ouverture, lecture des pages et du
    /// ComicInfo.xml (sinon Windows refuse de remplacer le fichier : os error 5).
    #[cfg(target_os = "linux")]
    #[test]
    fn no_provider_keeps_its_file_open() {
        let dir = tempfile::tempdir().unwrap();
        let cbz = dir.path().join("a.cbz");
        zip::tests::write_cbz(&cbz);
        let mut books = vec![cbz];
        for fixture in ["rar4.cbr", "solid_rar5.cbr"] {
            let p = dir.path().join(fixture);
            std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture), &p).unwrap();
            books.push(p);
        }
        for path in &books {
            let mut book = open_book(path).unwrap();
            for i in 0..book.pages().len() {
                book.render_page(i).unwrap();
            }
            book.comic_info().unwrap();
            assert_eq!(on_demand::open_descriptors(path), 0, "{}", path.display());
        }
    }

    #[test]
    fn misnamed_cbr_is_opened_as_zip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.cbr");
        zip::tests::write_cbz(&path);
        assert_eq!(BookFormat::detect(&path), Some(BookFormat::Zip));
        let mut book = open_book(&path).unwrap();
        testutil::check_book(book.as_mut());
    }
}
