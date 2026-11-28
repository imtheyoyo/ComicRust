//! ZIP / CBZ.
//!
//! Le répertoire central est lu une fois à l'ouverture ; le fichier, lui, n'est ouvert que
//! le temps de lire une entrée ([`OnDemandFile`]) : un CBZ affiché peut être réécrit
//! (« Écrire les infos dans le fichier ») sans que Windows le dise « ouvert ».

use std::path::Path;

use super::on_demand::OnDemandFile;
use super::{entry_limit, index_entries, parse_comic_info, read_bounded, BookFormat, PageProvider};
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
            // Taille déclarée bornée : un CBZ piégé ne fait plus avorter le processus.
            let declared = entry.size();
            Ok(read_bounded(&mut entry, declared, entry_limit(name), name)?)
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

    /// CBZ piégé : l'entrée déclare (ZIP64) 1 Tio décompressé pour 4 octets réels. Avant la
    /// correction, la lecture réservait 1 Tio d'avance et le processus avortait.
    pub fn write_zip_declaring(path: &Path, usize_declared: u64) {
        let name = b"001.jpg";
        let data = b"abcd";
        let crc = {
            let mut h = 0xFFFF_FFFFu32;
            for &b in data {
                h ^= b as u32;
                for _ in 0..8 {
                    h = if h & 1 != 0 { (h >> 1) ^ 0xEDB8_8320 } else { h >> 1 };
                }
            }
            !h
        };
        let mut extra = Vec::new();
        extra.extend_from_slice(&1u16.to_le_bytes());
        extra.extend_from_slice(&8u16.to_le_bytes());
        extra.extend_from_slice(&usize_declared.to_le_bytes());
        let mut local = Vec::new();
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for v in [45u16, 0, 0, 0, 0x21] {
            local.extend_from_slice(&v.to_le_bytes());
        }
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        local.extend_from_slice(name);
        local.extend_from_slice(&extra);
        local.extend_from_slice(data);
        let mut cd = Vec::new();
        cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for v in [45u16, 45, 0, 0, 0, 0x21] {
            cd.extend_from_slice(&v.to_le_bytes());
        }
        cd.extend_from_slice(&crc.to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        for v in [0u16, 0, 0] {
            cd.extend_from_slice(&v.to_le_bytes());
        }
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(name);
        cd.extend_from_slice(&extra);
        let mut eocd = Vec::new();
        eocd.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        for v in [0u16, 0, 1, 1] {
            eocd.extend_from_slice(&v.to_le_bytes());
        }
        eocd.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        eocd.extend_from_slice(&(local.len() as u32).to_le_bytes());
        eocd.extend_from_slice(&0u16.to_le_bytes());
        std::fs::write(path, [local, cd, eocd].concat()).unwrap();
    }

    #[test]
    fn huge_declared_size_is_refused_without_aborting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("piege.cbz");
        write_zip_declaring(&path, 1 << 40);
        let mut book = ZipProvider::open(&path).unwrap();
        let err = book.read_page(0).unwrap_err().to_string();
        assert!(err.contains("trop volumineuse"), "{err}");
        // Taille déclarée raisonnable : lue normalement.
        write_zip_declaring(&path, 4);
        assert_eq!(ZipProvider::open(&path).unwrap().read_page(0).unwrap(), b"abcd");
    }

    #[test]
    fn cbz() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.cbz");
        write_cbz(&path);
        check_book(&mut ZipProvider::open(&path).unwrap());
    }
}
