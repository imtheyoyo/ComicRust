//! Livre "dossier d'images" (ComicRack accepte un dossier comme livre).

use std::path::{Path, PathBuf};

use super::{is_image_name, parse_comic_info, sort_pages, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct FolderProvider {
    root: PathBuf,
    pages: Vec<String>,
}

impl FolderProvider {
    pub fn open(root: &Path) -> Result<Self, Error> {
        let mut pages: Vec<String> = std::fs::read_dir(root)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| is_image_name(n))
            .collect();
        sort_pages(&mut pages);
        Ok(Self { root: root.to_path_buf(), pages })
    }
}

impl PageProvider for FolderProvider {
    fn format(&self) -> BookFormat {
        BookFormat::Folder
    }
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
        Ok(Some(parse_comic_info(&std::fs::read(p)?)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::testutil::*;

    #[test]
    fn folder() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["10.png", "2.png", "1.png", "._1.png"] {
            std::fs::write(dir.path().join(n), png(page_number(n))).unwrap();
        }
        std::fs::write(dir.path().join("ComicInfo.xml"), INFO).unwrap();
        check_book(&mut FolderProvider::open(dir.path()).unwrap());
    }
}
