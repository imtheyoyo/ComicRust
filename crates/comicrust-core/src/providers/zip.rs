//! ZIP / CBZ.
//!
//! Le répertoire central est lu une fois à l'ouverture ; le fichier, lui, n'est ouvert que
//! le temps de lire une entrée ([`OnDemandFile`]) : un CBZ affiché peut être réécrit
//! (« Écrire les infos dans le fichier ») sans que Windows le dise « ouvert ».

use std::io::Read;
use std::path::Path;

use super::on_demand::OnDemandFile;
use super::{index_entries, parse_comic_info, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct ZipProvider {
    zip: ::zip::ZipArchive<OnDemandFile>,
    /// Copie partagée du fichier, pour le refermer après chaque lecture.
    file: OnDemandFile,
    pages: Vec<String>,
    info_entry: Option<String>,
}

impl ZipProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = OnDemandFile::open(path)?;
        let zip = ::zip::ZipArchive::new(file.clone());
        file.close();
        let zip = zip?;
        let (pages, info_entry) = index_entries(zip.file_names().map(str::to_string));
        Ok(Self { zip, file, pages, info_entry })
    }

    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        let read = (|| {
            let mut entry = self.zip.by_name(name)?;
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)?;
            Ok(buf)
        })();
        self.file.close();
        // Erreur d'E/S (fichier changé, partage réseau perdu…) : son message, pas « i/o error ».
        read.map_err(|e: ::zip::result::ZipError| match e {
            ::zip::result::ZipError::Io(io) => Error::Io(io),
            e => e.into(),
        })
    }
}

impl PageProvider for ZipProvider {
    fn format(&self) -> BookFormat {
        BookFormat::Zip
    }
    fn pages(&self) -> &[String] {
        &self.pages
    }
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        let name = self.pages.get(index).ok_or(Error::PageOutOfRange(index))?.clone();
        self.read_entry(&name)
    }
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        match self.info_entry.clone() {
            Some(name) => Ok(Some(parse_comic_info(&self.read_entry(&name)?)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::fs::File;
    use std::io::Write;

    use super::*;
    use crate::providers::testutil::*;

    pub fn write_cbz(path: &Path) {
        let mut w = ::zip::ZipWriter::new(File::create(path).unwrap());
        let opt = ::zip::write::SimpleFileOptions::default();
        for n in NAMES {
            w.start_file(n, opt).unwrap();
            w.write_all(&png(page_number(n))).unwrap();
        }
        w.start_file("ComicInfo.xml", opt).unwrap();
        w.write_all(INFO.as_bytes()).unwrap();
        w.finish().unwrap();
    }

    /// Aucune poignée gardée sur le fichier entre deux lectures (Linux : `/proc/self/fd`).
    #[cfg(target_os = "linux")]
    #[test]
    fn cbz_keeps_no_handle_open() {
        use crate::providers::on_demand::open_descriptors;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.cbz");
        write_cbz(&path);
        let mut book = super::super::open_book(&path).unwrap();
        assert_eq!(open_descriptors(&path), 0, "après l'ouverture");
        book.render_page(1).unwrap();
        book.comic_info().unwrap();
        assert_eq!(open_descriptors(&path), 0, "après lecture des pages et du ComicInfo.xml");
        // Fichier réécrit pendant que le livre est « ouvert » : erreur claire, pas de déchets.
        let mut w = ::zip::ZipWriter::new(File::create(&path).unwrap());
        w.start_file("1.png", ::zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(&png(9)).unwrap();
        w.finish().unwrap();
        let err = book.read_page(0).unwrap_err().to_string();
        assert!(err.contains("a changé"), "{err}");
        assert_eq!(open_descriptors(&path), 0);
    }

    #[test]
    fn cbz() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.cbz");
        write_cbz(&path);
        check_book(&mut ZipProvider::open(&path).unwrap());
    }
}
