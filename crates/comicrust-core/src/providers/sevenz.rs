//! 7-Zip / CB7, décodeur pur Rust.
//!
//! Les archives 7z de BD sont presque toujours "solides" : extraire la page N
//! oblige à décompresser tout ce qui précède. On décompresse donc toutes les
//! pages en mémoire en une seule passe, au premier accès.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sevenz_rust2::{ArchiveReader, Password};

use super::{index_entries, parse_comic_info, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct SevenZipProvider {
    path: PathBuf,
    pages: Vec<String>,
    info_entry: Option<String>,
    cache: Option<HashMap<String, Vec<u8>>>,
}

fn err(e: sevenz_rust2::Error) -> Error {
    Error::Archive(format!("7z : {e}"))
}

impl SevenZipProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let reader = ArchiveReader::open(path, Password::empty()).map_err(err)?;
        let names = reader
            .archive()
            .files
            .iter()
            .filter(|e| e.has_stream && !e.is_directory)
            .map(|e| e.name.replace('\\', "/"));
        let (pages, info_entry) = index_entries(names);
        Ok(Self { path: path.to_path_buf(), pages, info_entry, cache: None })
    }

    fn load_all(&mut self) -> Result<&HashMap<String, Vec<u8>>, Error> {
        if self.cache.is_none() {
            let mut wanted: std::collections::HashSet<String> = self.pages.iter().cloned().collect();
            wanted.extend(self.info_entry.clone());
            let mut map = HashMap::new();
            let mut reader = ArchiveReader::open(&self.path, Password::empty()).map_err(err)?;
            reader
                .for_each_entries(|entry, data| {
                    let name = entry.name.replace('\\', "/");
                    let mut buf = Vec::new();
                    if wanted.contains(&name) {
                        buf.reserve(entry.size as usize);
                        data.read_to_end(&mut buf)?;
                        map.insert(name, buf);
                    } else {
                        std::io::copy(data, &mut std::io::sink())?;
                    }
                    Ok(true)
                })
                .map_err(err)?;
            self.cache = Some(map);
        }
        Ok(self.cache.as_ref().unwrap())
    }

    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        self.load_all()?.get(name).cloned().ok_or_else(|| Error::Archive(format!("entrée absente : {name}")))
    }
}

impl PageProvider for SevenZipProvider {
    fn format(&self) -> BookFormat {
        BookFormat::SevenZip
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
    fn cb7() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("__MACOSX")).unwrap();
        for n in NAMES {
            std::fs::write(src.join(n), png(page_number(n))).unwrap();
        }
        std::fs::write(src.join("ComicInfo.xml"), INFO).unwrap();
        let path = dir.path().join("t.cb7");
        sevenz_rust2::compress_to_path(&src, &path).unwrap();
        assert_eq!(BookFormat::detect(&path), Some(BookFormat::SevenZip));
        check_book(&mut SevenZipProvider::open(&path).unwrap());
    }
}
