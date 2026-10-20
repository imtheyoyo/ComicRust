//! TAR / CBT. Les entrées sont indexées une fois (position + taille) puis lues
//! directement par `seek`, sans relire l'archive.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::{index_entries, parse_comic_info, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct TarProvider {
    file: File,
    entries: HashMap<String, (u64, u64)>,
    pages: Vec<String>,
    info_entry: Option<String>,
}

impl TarProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut entries = HashMap::new();
        let mut archive = ::tar::Archive::new(File::open(path)?);
        for entry in archive.entries()? {
            let entry = entry?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let name = entry.path()?.to_string_lossy().replace('\\', "/");
            entries.insert(name, (entry.raw_file_position(), entry.size()));
        }
        let (pages, info_entry) = index_entries(entries.keys().cloned());
        Ok(Self { file: File::open(path)?, entries, pages, info_entry })
    }

    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        let (pos, size) = *self.entries.get(name).ok_or_else(|| Error::Archive(format!("entrée absente : {name}")))?;
        self.file.seek(SeekFrom::Start(pos))?;
        let mut buf = vec![0; size as usize];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

impl PageProvider for TarProvider {
    fn format(&self) -> BookFormat {
        BookFormat::Tar
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
mod tests {
    use super::*;
    use crate::providers::testutil::*;

    #[test]
    fn cbt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.cbt");
        let mut b = ::tar::Builder::new(File::create(&path).unwrap());
        let mut add = |name: &str, data: &[u8]| {
            let mut h = ::tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, data).unwrap();
        };
        for n in NAMES {
            add(n, &png(page_number(n)));
        }
        add("ComicInfo.xml", INFO.as_bytes());
        b.finish().unwrap();
        drop(b);
        assert_eq!(BookFormat::detect(&path), Some(BookFormat::Tar));
        check_book(&mut TarProvider::open(&path).unwrap());
    }
}
