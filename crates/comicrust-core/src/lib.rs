//! Cœur du portage Rust de ComicRack (Community Edition).
//!
//! Correspondance avec l'original .NET (`ComicRack.Engine.dll`) :
//! - [`comic_info`] ↔ `cYo.Projects.ComicRack.Engine.ComicInfo`
//! - [`archive`]    ↔ fournisseurs d'images (`ImageProvider`, `ComicBookFactory`)
//! - [`cbl`]        ↔ import/export des listes de lecture `.cbl`
//! - [`library`]    ↔ `ComicLibrary` / `ComicDatabase` (ComicDB.xml)

pub mod archive;
pub mod cbl;
pub mod comic_info;
pub mod library;

pub use archive::{open_book, PageProvider};
pub use cbl::ReadingList;
pub use comic_info::ComicInfo;
pub use library::Library;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
    #[error("archive ZIP : {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("XML (lecture) : {0}")]
    XmlDe(#[from] quick_xml::DeError),
    #[error("XML (écriture) : {0}")]
    XmlSe(#[from] quick_xml::SeError),
    #[error("page {0} hors limites")]
    PageOutOfRange(usize),
    #[error("format non pris en charge : .{0}")]
    Unsupported(String),
}
