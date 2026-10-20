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

mod djvu;
mod folder;
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
        } else if head.starts_with(b"AT&TFORM") || head.starts_with(b"FORM") && head.get(8..12).is_some_and(|t| t.starts_with(b"DJV")) {
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
        let mut head = vec![0u8; 512];
        if let Ok(mut f) = std::fs::File::open(path) {
            let mut n = 0;
            while n < head.len() {
                match f.read(&mut head[n..]) {
                    Ok(0) | Err(_) => break,
                    Ok(k) => n += k,
                }
            }
            head.truncate(n);
            if let Some(fmt) = Self::sniff(&head) {
                return Some(fmt);
            }
        }
        path.extension().and_then(|e| e.to_str()).and_then(Self::from_extension)
    }
}

/// Extensions de livres proposées dans les dialogues d'ouverture.
pub const BOOK_EXTENSIONS: &[&str] = &[
    "cbz", "zip", "cbr", "rar", "cb7", "7z", "cbt", "tar", "pdf", "djvu", "djv", "cbw",
];

/// Extensions d'image reconnues dans les archives.
pub const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "tif", "tiff", "webp", "jxl",
];

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
    file.rsplit_once('.')
        .is_some_and(|(_, ext)| IMAGE_EXTS.contains(&ext))
}

/// Tri "naturel" des noms de pages (page2 < page10), comme ComicRack.
pub fn sort_pages(names: &mut [String]) {
    names.sort_by(|a, b| natord::compare_ignore_case(a, b));
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

pub(crate) fn parse_comic_info(bytes: &[u8]) -> Result<ComicInfo, Error> {
    let text = String::from_utf8_lossy(bytes);
    ComicInfo::from_xml(text.trim_start_matches('\u{feff}'))
}

/// Image décodée prête à l'affichage.
pub type PageImage = image::RgbaImage;

/// Décode les octets d'une page (JPEG, PNG, GIF, BMP, TIFF, WebP, JPEG XL).
pub fn decode_image(bytes: &[u8]) -> Result<PageImage, Error> {
    if bytes.starts_with(&[0xFF, 0x0A]) || bytes.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return decode_jxl(bytes);
    }
    let img = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?;
    Ok(img.into_rgba8())
}

fn decode_jxl(bytes: &[u8]) -> Result<PageImage, Error> {
    let jxl = jxl_oxide::JxlImage::builder()
        .read(std::io::Cursor::new(bytes))
        .map_err(|e| Error::Decode(format!("JPEG XL : {e}")))?;
    let render = jxl.render_frame(0).map_err(|e| Error::Decode(format!("JPEG XL : {e}")))?;
    let mut stream = render.stream();
    let (w, h, ch) = (stream.width(), stream.height(), stream.channels());
    let mut buf = vec![0f32; (w * h * ch) as usize];
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
        Error::Unsupported(
            path.extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_else(|| "?".into()),
        )
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

    #[test]
    fn sniffing() {
        assert_eq!(BookFormat::sniff(b"PK\x03\x04...."), Some(BookFormat::Zip));
        assert_eq!(BookFormat::sniff(b"Rar!\x1a\x07\x01\x00"), Some(BookFormat::Rar));
        assert_eq!(BookFormat::sniff(b"%PDF-1.7"), Some(BookFormat::Pdf));
        assert_eq!(BookFormat::sniff(b"AT&TFORM\0\0\0\0DJVU"), Some(BookFormat::Djvu));
        assert_eq!(BookFormat::sniff(b"<?xml version=\"1.0\"?><WebComic>"), Some(BookFormat::WebComic));
        assert_eq!(BookFormat::from_extension("CB7"), Some(BookFormat::SevenZip));
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
