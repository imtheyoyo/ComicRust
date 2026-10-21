//! RAR / CBR (RAR 4 et RAR 5) via la bibliothèque UnRAR de RARLAB.
//!
//! UnRAR ne lit les entrées que séquentiellement. Pour une archive solide
//! (cas courant), toutes les pages sont décompressées en mémoire en une passe ;
//! sinon chaque page est lue en sautant les entrées précédentes (rapide).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use unrar_ng::Archive;

use super::{index_entries, parse_comic_info, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct RarProvider {
    path: PathBuf,
    solid: bool,
    pages: Vec<String>,
    info_entry: Option<String>,
    cache: HashMap<String, Vec<u8>>,
}

fn err(e: unrar_ng::error::UnrarError) -> Error {
    Error::Archive(format!("RAR : {e}"))
}

fn entry_name(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

impl RarProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let listing = Archive::new(path).open_for_listing().map_err(err)?;
        let solid = listing.is_solid();
        let mut names = Vec::new();
        for header in listing {
            let header = header.map_err(err)?;
            if header.is_file() {
                names.push(entry_name(&header.filename));
            }
        }
        let (pages, info_entry) = index_entries(names);
        Ok(Self { path: path.to_path_buf(), solid, pages, info_entry, cache: HashMap::new() })
    }

    /// Archive « solide » : lire une entrée oblige à décompresser les précédentes.
    pub fn is_solid(&self) -> bool {
        self.solid
    }

    /// Une passe sur l'archive ; lit `target` (ou tout ce qui est utile si solide).
    fn scan(&mut self, target: &str) -> Result<(), Error> {
        let mut wanted: std::collections::HashSet<String> = if self.solid {
            self.pages.iter().cloned().chain(self.info_entry.clone()).collect()
        } else {
            [target.to_string()].into()
        };
        wanted.retain(|n| !self.cache.contains_key(n));
        let mut archive = Archive::new(&self.path).open_for_processing().map_err(err)?;
        while let Some(header) = archive.read_header().map_err(err)? {
            if wanted.is_empty() {
                break;
            }
            let name = entry_name(&header.entry().filename);
            archive = if header.entry().is_file() && wanted.remove(&name) {
                let (data, next) = header.read().map_err(err)?;
                self.cache.insert(name, data);
                next
            } else {
                header.skip().map_err(err)?
            };
        }
        Ok(())
    }

    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        if !self.cache.contains_key(name) {
            self.scan(name)?;
        }
        let data = if self.solid { self.cache.get(name).cloned() } else { self.cache.remove(name) };
        data.ok_or_else(|| Error::Archive(format!("entrée absente : {name}")))
    }
}

impl PageProvider for RarProvider {
    fn format(&self) -> BookFormat {
        BookFormat::Rar
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

    /// Archives générées avec `rar a -ep -s` (solide, RAR5) et `rar a -ep -ma4`
    /// (RAR4) à partir des mêmes fichiers que les autres tests.
    #[test]
    fn cbr_solid_rar5_and_rar4() {
        for fixture in ["solid_rar5.cbr", "rar4.cbr"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
            assert_eq!(BookFormat::detect(&path), Some(BookFormat::Rar), "{fixture}");
            let mut book = RarProvider::open(&path).unwrap();
            check_book(&mut book);
        }
    }
}
