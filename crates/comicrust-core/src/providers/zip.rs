//! ZIP / CBZ.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::{index_entries, parse_comic_info, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct ZipProvider {
    zip: ::zip::ZipArchive<File>,
    pages: Vec<String>,
    info_entry: Option<String>,
}

impl ZipProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let zip = ::zip::ZipArchive::new(File::open(path)?)?;
        let (pages, info_entry) = index_entries(zip.file_names().map(str::to_string));
        Ok(Self { zip, pages, info_entry })
    }

    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        let mut entry = self.zip.by_name(name)?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
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

    #[test]
    fn cbz() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.cbz");
        write_cbz(&path);
        check_book(&mut ZipProvider::open(&path).unwrap());
    }
}
