//! PDF, rendu par PDFium (la même bibliothèque que ComicRack CE et Chrome).
//!
//! PDFium est chargé dynamiquement : `pdfium.dll` (Windows), `libpdfium.so`
//! (Linux) ou `libpdfium.dylib` (macOS), cherché à côté de l'exécutable, dans
//! le dossier courant, dans `$PDFIUM_DYNAMIC_LIB_PATH`, puis dans le système.
//! Sans PDFium, seuls les PDF sont indisponibles ; le reste fonctionne.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use super::{encode_png, BookFormat, PageImage, PageProvider};
use crate::{ComicInfo, Error};

/// Hauteur cible du rendu d'une page (≈ 200 dpi pour un format comics).
pub const RENDER_HEIGHT: i32 = 2600;

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("PDFIUM_DYNAMIC_LIB_PATH") {
        dirs.push(PathBuf::from(p));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(dir.join("lib"));
        dirs.push(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    dirs
}

fn pdfium() -> Result<&'static Pdfium, Error> {
    PDFIUM
        .get_or_init(|| {
            for dir in candidate_dirs() {
                let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
                if lib.exists() {
                    if let Ok(b) = Pdfium::bind_to_library(&lib) {
                        return Ok(Pdfium::new(b));
                    }
                }
            }
            Pdfium::bind_to_system_library()
                .map(Pdfium::new)
                .map_err(|_| format!("{} introuvable", Pdfium::pdfium_platform_library_name_at_path("").display()))
        })
        .as_ref()
        .map_err(|e| Error::Pdf(format!("PDFium indisponible : {e}")))
}

/// `Ok(())` si PDFium est chargeable (affiché dans "À propos").
pub fn pdfium_status() -> Result<(), String> {
    pdfium().map(|_| ()).map_err(|e| e.to_string())
}

pub struct PdfProvider {
    doc: PdfDocument<'static>,
    pages: Vec<String>,
}

fn err(e: PdfiumError) -> Error {
    Error::Pdf(e.to_string())
}

impl PdfProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let doc = pdfium()?.load_pdf_from_file(path, None).map_err(err)?;
        let n = doc.pages().len() as usize;
        let width = n.to_string().len().max(3);
        let pages = (1..=n).map(|i| format!("Page {i:0width$}")).collect();
        Ok(Self { doc, pages })
    }
}

impl PageProvider for PdfProvider {
    fn format(&self) -> BookFormat {
        BookFormat::Pdf
    }
    fn pages(&self) -> &[String] {
        &self.pages
    }
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        encode_png(&self.render_page(index)?)
    }
    fn render_page(&mut self, index: usize) -> Result<PageImage, Error> {
        if index >= self.pages.len() {
            return Err(Error::PageOutOfRange(index));
        }
        let page = self.doc.pages().get(index as PdfPageIndex).map_err(err)?;
        let cfg = PdfRenderConfig::new()
            .set_target_height(RENDER_HEIGHT)
            .set_maximum_width(RENDER_HEIGHT * 2)
            .render_form_data(true);
        let bitmap = page.render_with_config(&cfg).map_err(err)?;
        Ok(bitmap.as_image().map_err(err)?.into_rgba8())
    }
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        // ComicRack lit les métadonnées PDF de base (titre, auteur).
        let meta = self.doc.metadata();
        let get = |t| meta.get(t).map(|m| m.value().to_string()).filter(|v| !v.trim().is_empty());
        let title = get(PdfDocumentMetadataTagType::Title);
        let writer = get(PdfDocumentMetadataTagType::Author);
        if title.is_none() && writer.is_none() {
            return Ok(None);
        }
        Ok(Some(ComicInfo { title, writer, page_count: Some(self.pages.len() as u32), ..Default::default() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nécessite PDFium (CI : téléchargé ; en local : `PDFIUM_DYNAMIC_LIB_PATH`).
    #[test]
    fn pdf() {
        if pdfium_status().is_err() {
            eprintln!("PDFium absent : test PDF ignoré");
            return;
        }
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        assert_eq!(BookFormat::detect(&path), Some(BookFormat::Pdf));
        let mut book = PdfProvider::open(&path).unwrap();
        assert_eq!(book.pages(), ["Page 001", "Page 002"]);
        let img = book.render_page(1).unwrap();
        assert_eq!(img.height(), RENDER_HEIGHT as u32);
        // La page 2 est rouge pleine dans le fichier de test.
        let px = img.get_pixel(img.width() / 2, img.height() / 2).0;
        assert!(px[0] > 200 && px[1] < 60, "{px:?}");
        assert!(book.read_page(0).unwrap().starts_with(b"\x89PNG"));
    }
}
