//! Cœur du portage Rust de ComicRack (Community Edition).
//!
//! Correspondance avec l'original .NET (`ComicRack.Engine.dll`) :
//! - [`comic_info`] ↔ `cYo.Projects.ComicRack.Engine.ComicInfo`
//! - [`providers`]  ↔ fournisseurs de pages (`ComicProvider`, `ComicBookFactory`)
//! - [`cbl`]        ↔ import/export des listes de lecture `.cbl`
//! - [`library`]    ↔ `ComicLibrary` / `ComicDatabase` (ComicDB.xml)

pub mod cbl;
pub mod comic_info;
pub mod comic_lists;
pub mod db;
pub mod index;
pub mod library;
pub mod providers;

pub use cbl::ReadingList;
pub use comic_info::ComicInfo;
pub use library::Library;
pub use providers::{decode_image, open_book, BookFormat, PageImage, PageProvider};

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
    #[error("archive : {0}")]
    Archive(String),
    #[error("image : {0}")]
    Image(#[from] image::ImageError),
    #[error("décodage : {0}")]
    Decode(String),
    #[error("PDF : {0}")]
    Pdf(String),
    #[error("DjVu : {0}")]
    Djvu(String),
    #[error("réseau : {0}")]
    Network(String),
    #[error("base de données : {0}")]
    Database(String),
    #[error("page {0} hors limites")]
    PageOutOfRange(usize),
    #[error("format non pris en charge : .{0}")]
    Unsupported(String),
}
