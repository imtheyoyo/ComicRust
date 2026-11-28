//! 7-Zip / CB7, décodeur pur Rust.
//!
//! Les archives 7z de BD sont presque toujours "solides" : extraire la page N
//! oblige à décompresser tout ce qui précède. Voir [`SevenZipProvider::read_entry`] :
//! une première lecture s'arrête à la page voulue, la suivante décompresse tout.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sevenz_rust2::{ArchiveReader, Password};

use super::{entry_limit, index_entries, parse_comic_info, read_bounded, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct SevenZipProvider {
    path: PathBuf,
    pages: Vec<String>,
    info_entry: Option<String>,
    /// Entrées déjà décompressées.
    cache: HashMap<String, Vec<u8>>,
    /// Toutes les entrées utiles sont dans `cache`.
    complete: bool,
    /// Une première lecture partielle a déjà eu lieu (voir [`Self::read_entry`]).
    partial_done: bool,
}

fn err(e: sevenz_rust2::Error) -> Error {
    Error::Archive(format!("7z : {e}"))
}

/// Une passe sur l'archive : garde les entrées de `wanted` (lecture bornée, voir
/// [`super::read_bounded`]) et saute les autres ; avec `stop_after`, s'arrête dès que cette
/// entrée est lue (inutile de décompresser la suite pour une couverture).
pub(crate) fn read_entries(
    reader: &mut ArchiveReader<std::fs::File>,
    wanted: &HashSet<String>,
    stop_after: Option<&str>,
) -> Result<HashMap<String, Vec<u8>>, Error> {
    let mut map = HashMap::new();
    let mut done = false;
    reader
        .for_each_entries(|entry, data| {
            if done {
                // `for_each_entries` passe aux blocs suivants : on les refuse aussitôt.
                return Ok(false);
            }
            let name = entry.name.replace('\\', "/");
            if wanted.contains(&name) {
                let buf = read_bounded(data, entry.size, entry_limit(&name), &name)?;
                done = stop_after == Some(name.as_str());
                map.insert(name, buf);
            } else {
                std::io::copy(data, &mut std::io::sink())?;
            }
            Ok(!done)
        })
        .map_err(err)?;
    Ok(map)
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
        Ok(Self {
            path: path.to_path_buf(),
            pages,
            info_entry,
            cache: HashMap::new(),
            complete: false,
            partial_done: false,
        })
    }

    /// Les 7z de BD sont presque toujours solides : lire une page oblige à décompresser ce
    /// qui la précède. Première lecture (souvent la seule : couverture) : passe arrêtée à la
    /// page voulue, seule cette page est gardée. Lecture suivante d'une page absente du
    /// cache (lecteur) : une passe complète qui garde toutes les pages.
    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        if let Some(b) = self.cache.get(name) {
            return Ok(b.clone());
        }
        if !self.complete {
            let mut reader = ArchiveReader::open(&self.path, Password::empty()).map_err(err)?;
            if self.partial_done {
                let wanted: HashSet<String> = self
                    .pages
                    .iter()
                    .chain(self.info_entry.as_ref())
                    .filter(|n| !self.cache.contains_key(*n))
                    .cloned()
                    .collect();
                self.cache.extend(read_entries(&mut reader, &wanted, None)?);
                self.complete = true;
            } else {
                let wanted: HashSet<String> = [name.to_string()].into();
                self.cache.extend(read_entries(&mut reader, &wanted, Some(name))?);
                self.partial_done = true;
            }
        }
        self.cache.get(name).cloned().ok_or_else(|| Error::Archive(format!("entrée absente : {name}")))
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
        // Couverture seule : passe arrêtée à la page voulue, rien d'autre gardé en mémoire.
        let mut book = SevenZipProvider::open(&path).unwrap();
        book.read_page(0).unwrap();
        assert_eq!(book.cache.len(), 1);
        assert!(!book.complete);
        // Page suivante : passe complète.
        book.read_page(2).unwrap();
        assert!(book.complete);
        assert_eq!(book.cache.len(), 4, "3 pages + ComicInfo.xml");
    }
}
