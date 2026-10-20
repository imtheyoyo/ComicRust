//! Fournisseurs de pages (équivalent des `ImageProvider` de ComicRack.Engine).
//!
//! ComicRack ouvre un livre via un fournisseur choisi selon l'extension/la
//! signature : ZIP (CBZ, via SharpZipLib), RAR/7z (CBR/CB7, via SharpCompress),
//! PDF (PDFium), DjVu, dossier d'images, eComic chiffré. Ici : CBZ + dossier ;
//! CBR/CB7/PDF sont des emplacements prévus (voir README).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{ComicInfo, Error};

/// Extensions d'image reconnues (ComicRack CE : jpg, png, gif, bmp, tif,
/// webp, heif/avif, jxl, jp2).
pub const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "heif", "avif", "jxl",
    "jp2", "j2k",
];

pub fn is_image_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // ComicRack ignore les fichiers ressources macOS "._xxx" (chaîne "._" relevée).
    let file = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    if file.starts_with("._") || lower.contains("__macosx/") {
        return false;
    }
    lower
        .rsplit('.')
        .next()
        .is_some_and(|ext| IMAGE_EXTS.contains(&ext))
}

/// Tri "naturel" des noms de pages (page2 < page10), comme ComicRack.
pub fn sort_pages(names: &mut [String]) {
    names.sort_by(|a, b| natord::compare_ignore_case(a, b));
}

pub trait PageProvider {
    /// Noms des pages, dans l'ordre de lecture.
    fn pages(&self) -> &[String];
    /// Octets bruts de l'image n°`index`.
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error>;
    /// `ComicInfo.xml` embarqué, s'il existe.
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error>;
}

pub struct CbzProvider {
    zip: zip::ZipArchive<File>,
    pages: Vec<String>,
    info_entry: Option<String>,
}

impl CbzProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let zip = zip::ZipArchive::new(File::open(path)?)?;
        let mut pages = Vec::new();
        let mut info_entry = None;
        for name in zip.file_names() {
            if name.eq_ignore_ascii_case("ComicInfo.xml")
                || name.to_ascii_lowercase().ends_with("/comicinfo.xml")
            {
                info_entry = Some(name.to_string());
            } else if is_image_name(name) {
                pages.push(name.to_string());
            }
        }
        sort_pages(&mut pages);
        Ok(Self { zip, pages, info_entry })
    }
}

impl PageProvider for CbzProvider {
    fn pages(&self) -> &[String] {
        &self.pages
    }

    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        let name = self.pages.get(index).ok_or(Error::PageOutOfRange(index))?.clone();
        let mut entry = self.zip.by_name(&name)?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        let Some(name) = self.info_entry.clone() else { return Ok(None) };
        let mut s = String::new();
        self.zip.by_name(&name)?.read_to_string(&mut s)?;
        Ok(Some(ComicInfo::from_xml(s.trim_start_matches('\u{feff}'))?))
    }
}

/// Livre "dossier d'images" (ComicRack accepte un dossier comme livre).
pub struct FolderProvider {
    root: PathBuf,
    pages: Vec<String>,
}

impl FolderProvider {
    pub fn open(root: &Path) -> Result<Self, Error> {
        let mut pages: Vec<String> = std::fs::read_dir(root)?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| is_image_name(n))
            .collect();
        sort_pages(&mut pages);
        Ok(Self { root: root.to_path_buf(), pages })
    }
}

impl PageProvider for FolderProvider {
    fn pages(&self) -> &[String] {
        &self.pages
    }
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        let name = self.pages.get(index).ok_or(Error::PageOutOfRange(index))?;
        Ok(std::fs::read(self.root.join(name))?)
    }
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        let p = self.root.join("ComicInfo.xml");
        if !p.exists() {
            return Ok(None);
        }
        Ok(Some(ComicInfo::from_xml(&std::fs::read_to_string(p)?)?))
    }
}

/// Choisit le fournisseur selon le chemin (fabrique type `ComicBookFactory`).
pub fn open_book(path: &Path) -> Result<Box<dyn PageProvider>, Error> {
    if path.is_dir() {
        return Ok(Box::new(FolderProvider::open(path)?));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "cbz" | "zip" => Ok(Box::new(CbzProvider::open(path)?)),
        other => Err(Error::Unsupported(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn natural_sort_and_filter() {
        let mut v: Vec<String> = ["p10.jpg", "p2.jpg", "p1.JPG"].map(String::from).to_vec();
        sort_pages(&mut v);
        assert_eq!(v, ["p1.JPG", "p2.jpg", "p10.jpg"]);
        assert!(!is_image_name("__MACOSX/x/p1.jpg"));
        assert!(!is_image_name("dir/._p1.jpg"));
        assert!(!is_image_name("readme.txt"));
    }

    #[test]
    fn cbz_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.cbz");
        let mut w = zip::ZipWriter::new(File::create(&path).unwrap());
        let opt = zip::write::SimpleFileOptions::default();
        for n in ["10.png", "2.png", "1.png"] {
            w.start_file(n, opt).unwrap();
            w.write_all(n.as_bytes()).unwrap();
        }
        w.start_file("ComicInfo.xml", opt).unwrap();
        w.write_all(b"<ComicInfo><Series>Thanos</Series><Number>1</Number></ComicInfo>").unwrap();
        w.finish().unwrap();

        let mut book = open_book(&path).unwrap();
        assert_eq!(book.pages(), ["1.png", "2.png", "10.png"]);
        assert_eq!(book.read_page(2).unwrap(), b"10.png");
        assert_eq!(book.comic_info().unwrap().unwrap().series.as_deref(), Some("Thanos"));
    }
}
