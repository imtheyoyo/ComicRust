//! Écriture dans les fichiers BD : `ComicInfo.xml` des CBZ et conversion en CBZ.
//!
//! Seuls les **fichiers** sont modifiés : la base ComicRack reste en lecture seule.
//!
//! **Infos du livre** : comme ComicRack (« Mettre à jour les fichiers »), les champs
//! `ComicInfo` de la fiche du livre en base (`comics.data`, `<ComicBook>`) sont écrits dans
//! le `ComicInfo.xml` de l'archive, dans l'ordre de la classe `ComicInfo` de ComicRack.
//! Chaque élément est recopié tel qu'il est en base (texte, entités, attributs des
//! `<Page>`). Les éléments inconnus de ComicRack déjà présents dans l'archive (ajoutés
//! par d'autres logiciels) sont gardés, comme le fait ComicRack (`UnparsedElements`).
//!
//! **Archive réécrite sans risque** : les pages sont recopiées telles quelles (sans
//! recompression) dans un fichier temporaire du même dossier, qui est vérifié puis
//! remplace l'original. En cas d'erreur, l'original n'est pas touché.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::providers::{self, BookFormat};
use crate::{ComicInfo, Error};

/// Éléments de `ComicInfo`, dans l'ordre où ComicRack les écrit (`XmlSerializer`).
pub const COMIC_INFO_ELEMENTS: &[&str] = &[
    "Title",
    "Series",
    "Number",
    "Count",
    "Volume",
    "AlternateSeries",
    "AlternateNumber",
    "StoryArc",
    "SeriesGroup",
    "AlternateCount",
    "Summary",
    "Notes",
    "Review",
    "Year",
    "Month",
    "Day",
    "Writer",
    "Penciller",
    "Inker",
    "Colorist",
    "Letterer",
    "CoverArtist",
    "Editor",
    "Translator",
    "Publisher",
    "Imprint",
    "Genre",
    "Web",
    "PageCount",
    "LanguageISO",
    "Format",
    "AgeRating",
    "BlackAndWhite",
    "Manga",
    "PreferredFrontCover",
    "Characters",
    "Teams",
    "MainCharacterOrTeam",
    "Locations",
    "CommunityRating",
    "ScanInformation",
    "Tags",
    "Pages",
];

/// Éléments propres à la bibliothèque de ComicRack (fiche `<ComicBook>`), jamais écrits
/// dans un fichier : utile pour ne pas les prendre pour des éléments « inconnus ».
const LIBRARY_ONLY: &[&str] = &[
    "Added",
    "Opened",
    "Released",
    "FileSize",
    "FileModifiedTime",
    "FileCreationTime",
    "CurrentPage",
    "LastPageRead",
    "OpenedCount",
    "Rating",
    "ReadPercentage",
    "Checked",
    "SeriesComplete",
    "ComicInfoIsDirty",
    "ComicBookIsDirty",
    "EnableProposed",
    "Missing",
    "BookPrice",
    "BookAge",
    "BookCondition",
    "BookStore",
    "BookOwner",
    "BookCollectionStatus",
    "BookNotes",
    "BookLocation",
    "ISBN",
    "CustomValuesStore",
    "CustomThumbnailKey",
    "ColorAdjustment",
    "LastOpenedFromListId",
    "NewPages",
];

const HEADER: &str = "<?xml version=\"1.0\"?>\n<ComicInfo xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" \
                      xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n";

fn xml_err(e: impl std::fmt::Display) -> Error {
    Error::Decode(format!("XML : {e}"))
}

/// Éléments de premier niveau sous la racine : (nom, texte XML brut de l'élément).
fn top_elements(xml: &str) -> Result<Vec<(String, String)>, Error> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut name = String::new();
    loop {
        let pos = reader.buffer_position() as usize;
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                depth += 1;
                if depth == 2 {
                    start = pos;
                    name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                }
            }
            Event::Empty(e) if depth == 1 => {
                let end = reader.buffer_position() as usize;
                out.push((String::from_utf8_lossy(e.local_name().as_ref()).into_owned(), xml[pos..end].to_string()));
            }
            Event::End(_) => {
                if depth == 2 {
                    let end = reader.buffer_position() as usize;
                    out.push((std::mem::take(&mut name), xml[start..end].to_string()));
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 {
        return Err(xml_err("document incomplet"));
    }
    Ok(out)
}

/// `ComicInfo.xml` à écrire pour une fiche de la base (`<ComicBook>`), en gardant les
/// éléments inconnus du `ComicInfo.xml` actuel (`existing`).
pub fn comic_info_from_record(record: &str, existing: Option<&str>) -> Result<String, Error> {
    let record = record.trim_start_matches('\u{feff}');
    let fields = top_elements(record)?;
    let old = match existing.map(|e| top_elements(e.trim_start_matches('\u{feff}'))) {
        Some(Ok(v)) => v,
        _ => Vec::new(), // ComicInfo.xml actuel illisible : remplacé entièrement
    };
    let mut xml = String::from(HEADER);
    let mut push = |raw: &str| {
        xml.push_str("  ");
        xml.push_str(raw.trim());
        xml.push('\n');
    };
    for &name in COMIC_INFO_ELEMENTS {
        let from_db = fields.iter().find(|(n, _)| n == name);
        // Fiche sans liste de pages : celle du fichier est gardée (elle décrit l'archive).
        let found = from_db.or_else(|| (name == "Pages").then(|| old.iter().find(|(n, _)| n == name)).flatten());
        if name == "Pages" {
            // Éléments inconnus de ComicRack (ex. « GTIN » d'un autre logiciel), avant les
            // pages comme `UnparsedElements`.
            for (n, raw) in &old {
                if !COMIC_INFO_ELEMENTS.contains(&n.as_str()) && !LIBRARY_ONLY.contains(&n.as_str()) {
                    push(raw);
                }
            }
        }
        if let Some((_, raw)) = found {
            push(raw);
        }
    }
    xml.push_str("</ComicInfo>\n");
    // Garde-fou : le résultat doit se relire.
    ComicInfo::from_xml(&xml)?;
    Ok(xml)
}

/// Forme comparable d'un élément : espaces entre balises ignorés.
fn normalized(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_space = false;
    let mut last = '\0';
    for c in raw.trim().chars() {
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !(last == '>' && c == '<') && c != '/' && c != '>' {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
        last = c;
    }
    out
}

/// Deux `ComicInfo.xml` disent-ils la même chose (mêmes éléments, quel que soit l'ordre
/// ou la mise en forme) ?
pub fn same_comic_info(a: &str, b: &str) -> bool {
    let key = |x: &str| {
        top_elements(x.trim_start_matches('\u{feff}')).ok().map(|v| {
            let mut v: Vec<(String, String)> = v.into_iter().map(|(n, r)| (n, normalized(&r))).collect();
            v.sort();
            v
        })
    };
    matches!((key(a), key(b)), (Some(x), Some(y)) if x == y)
}

/// Résultat de l'écriture des infos dans un fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// `ComicInfo.xml` écrit (créé ou remplacé).
    Written,
    /// Le fichier contenait déjà ces infos : rien n'a été modifié.
    Unchanged,
}

/// Date des entrées écrites (sans elle, le format ZIP note 1980).
fn now_zip() -> zip::DateTime {
    use chrono::{Datelike, Timelike};
    let n = chrono::Local::now();
    zip::DateTime::from_date_and_time(
        n.year().clamp(1980, 2107) as u16,
        n.month() as u8,
        n.day() as u8,
        n.hour() as u8,
        n.minute() as u8,
        n.second() as u8,
    )
    .unwrap_or_default()
}

fn temp_path(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".comicrust.tmp");
    PathBuf::from(tmp)
}

/// Attentes entre les tentatives de remplacement (environ 3 s en tout) : un antivirus,
/// l'indexation de Windows ou un partage SMB peuvent tenir le fichier un court instant
/// (après sa lecture, le serveur relâche parfois le verrou avec un léger délai).
const REPLACE_RETRY_DELAYS_MS: &[u64] = &[100, 200, 300, 500, 800, 1200];

/// Refus passager ? (« Accès refusé », fichier en cours d'utilisation, verrouillé.)
fn is_transient_refusal(e: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    if matches!(e.kind(), ErrorKind::PermissionDenied | ErrorKind::ResourceBusy) {
        return true;
    }
    // Windows : ERROR_ACCESS_DENIED (5), ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33).
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Remplace `path` par `tmp` (même dossier). Sous Windows, échoue si le fichier est ouvert
/// par un autre programme : quelques nouvelles tentatives courtes sont faites, puis le
/// fichier temporaire est supprimé.
fn replace(tmp: &Path, path: &Path) -> Result<(), Error> {
    let delays: Vec<std::time::Duration> =
        REPLACE_RETRY_DELAYS_MS.iter().map(|&ms| std::time::Duration::from_millis(ms)).collect();
    replace_with(tmp, path, &delays, |a, b| std::fs::rename(a, b))
}

/// [`replace`] avec ses attentes et sa fonction de renommage (tests).
fn replace_with(
    tmp: &Path,
    path: &Path,
    delays: &[std::time::Duration],
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), Error> {
    let mut attempt = 0usize;
    loop {
        match rename(tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient_refusal(&e) && attempt < delays.len() => {
                std::thread::sleep(delays[attempt]);
                attempt += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(tmp);
                return Err(Error::Archive(format!("{} existe déjà", path.display())));
            }
            Err(e) => {
                let _ = std::fs::remove_file(tmp);
                let tries = if attempt > 0 { format!(", {} tentatives", attempt + 1) } else { String::new() };
                return Err(Error::Archive(format!(
                    "impossible de remplacer le fichier ({e}{tries}) : il est peut-être ouvert dans ComicRack ou un autre programme"
                )));
            }
        }
    }
}

fn check_not_rewritten(path: &Path) -> Result<(), Error> {
    if providers::is_being_rewritten(path) {
        return Err(Error::Archive("ComicRack est en train d'enregistrer ce fichier : réessayer plus tard".into()));
    }
    Ok(())
}

/// Taille et date de modification d'un fichier : relevées avant la lecture et comparées
/// juste avant le remplacement (un autre programme, ComicRack par exemple, a pu réécrire
/// le fichier entre-temps : sa version ne doit pas être écrasée).
fn file_stamp(path: &Path) -> Result<(u64, Option<std::time::SystemTime>), Error> {
    let m = std::fs::metadata(path)?;
    Ok((m.len(), m.modified().ok()))
}

/// Texte d'un `ComicInfo.xml` selon sa marque d'ordre des octets : UTF-16 (petit ou gros
/// boutiste), UTF-8 avec ou sans BOM. Sans BOM, UTF-8 (caractères invalides remplacés).
fn decode_xml_bytes(bytes: &[u8]) -> String {
    let utf16 = |be: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| if be { u16::from_be_bytes(*c) } else { u16::from_le_bytes(*c) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(false),
        [0xFE, 0xFF, ..] => utf16(true),
        _ => String::from_utf8_lossy(bytes).trim_start_matches('\u{feff}').to_string(),
    }
}

/// Écrit dans le `ComicInfo.xml` d'un CBZ les infos de la fiche `record` (base ComicRack).
pub fn write_comic_info(path: &Path, record: &str) -> Result<Outcome, Error> {
    write_comic_info_with(path, record, &mut || {})
}

/// [`write_comic_info`] avec un point d'arrêt juste avant le contrôle final de
/// l'original (tests : fichier modifié pendant l'écriture).
fn write_comic_info_with(path: &Path, record: &str, before_replace: &mut dyn FnMut()) -> Result<Outcome, Error> {
    check_not_rewritten(path)?;
    match BookFormat::detect(path) {
        Some(BookFormat::Zip) => {}
        Some(f) => {
            return Err(Error::Archive(format!(
                "{} : seules les archives CBZ (ZIP) reçoivent les infos ; convertir d'abord en CBZ",
                f.label()
            )))
        }
        None => return Err(Error::Archive("format de fichier inconnu".into())),
    }
    let tmp = temp_path(path);
    let result = (|| {
        let before = file_stamp(path)?;
        let mut zip = zip::ZipArchive::new(File::open(path)?)?;
        let (_, info_name) = providers::index_entries(zip.file_names().map(str::to_string));
        let existing = match &info_name {
            Some(name) => {
                let mut bytes = Vec::new();
                zip.by_name(name)?.read_to_end(&mut bytes)?;
                // UTF-16 (écrit par certains logiciels) : décodé, sinon ses éléments
                // inconnus de ComicRack seraient perdus.
                Some(decode_xml_bytes(&bytes))
            }
            None => None,
        };
        let xml = comic_info_from_record(record, existing.as_deref())?;
        if existing.as_deref().is_some_and(|old| same_comic_info(old, &xml)) {
            return Ok(Outcome::Unchanged);
        }
        // Nom gardé s'il était à la racine (casse comprise), sinon « ComicInfo.xml ».
        let new_name = info_name.as_deref().filter(|n| !n.contains(['/', '\\'])).unwrap_or("ComicInfo.xml").to_string();
        let mut w = zip::ZipWriter::new(File::create(&tmp)?);
        let mut kept = 0usize;
        for i in 0..zip.len() {
            let entry = zip.by_index_raw(i)?;
            if info_name.as_deref() == Some(entry.name()) {
                continue;
            }
            w.raw_copy_file(entry)?;
            kept += 1;
        }
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(now_zip());
        w.start_file(new_name.as_str(), opts)?;
        w.write_all(xml.as_bytes())?;
        w.set_raw_comment(zip.comment().to_vec().into_boxed_slice());
        // Données sur le disque avant le remplacement : sinon une coupure de courant juste
        // après le `rename` peut laisser un CBZ vide ou tronqué à la place de l'original.
        w.finish()?.sync_all()?;
        drop(zip); // fermer l'original avant de le remplacer (Windows)

        // Vérification : toutes les entrées présentes et ComicInfo.xml relisible.
        let mut check = zip::ZipArchive::new(File::open(&tmp)?)?;
        if check.len() != kept + 1 {
            return Err(Error::Archive("vérification : nombre d'entrées différent".into()));
        }
        let mut written = String::new();
        check.by_name(&new_name)?.read_to_string(&mut written)?;
        ComicInfo::from_xml(&written)?;
        drop(check);
        before_replace();
        // Original modifié pendant la copie (ComicRack, autre programme) : on abandonne.
        check_not_rewritten(path)?;
        if file_stamp(path)? != before {
            return Err(Error::Archive(
                "le fichier a été modifié pendant l'écriture (ComicRack ?) : rien n'a été changé, réessayer".into(),
            ));
        }
        replace(&tmp, path)?;
        Ok(Outcome::Written)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Formats convertibles en CBZ (archives d'images).
pub fn can_convert(path: &Path) -> bool {
    matches!(BookFormat::detect(path), Some(BookFormat::Rar | BookFormat::SevenZip | BookFormat::Tar))
}

/// Chemin du CBZ produit par [`convert_to_cbz`] : même nom, extension `.cbz`.
pub fn cbz_path_for(src: &Path) -> PathBuf {
    src.with_extension("cbz")
}

/// Résultat d'une conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    pub path: PathBuf,
    pub pages: usize,
}

/// Convertit une archive CBR / CB7 / CBT en CBZ à côté de l'original (qui n'est pas
/// modifié). Les pages sont recopiées telles quelles (mêmes noms, sans recompression) ; les
/// autres fichiers de l'archive (textes, vignettes…) ne sont pas repris.
/// `ComicInfo.xml` : la fiche de la base (`record`) si elle est donnée, sinon celui de
/// l'archive d'origine.
pub fn convert_to_cbz(src: &Path, record: Option<&str>) -> Result<Converted, Error> {
    check_not_rewritten(src)?;
    let format = BookFormat::detect(src);
    if !matches!(format, Some(BookFormat::Rar | BookFormat::SevenZip | BookFormat::Tar)) {
        return Err(Error::Archive(match format {
            Some(BookFormat::Zip) => "déjà au format CBZ (ZIP)".into(),
            Some(f) => format!("{} : seules les archives CBR, CB7 et CBT se convertissent", f.label()),
            None => "format de fichier inconnu".into(),
        }));
    }
    let dest = cbz_path_for(src);
    if dest.exists() {
        return Err(Error::Archive(format!("{} existe déjà", dest.display())));
    }
    let tmp = temp_path(&dest);
    let result = (|| {
        let mut book = providers::open_book(src)?;
        let names: Vec<String> = book.pages().to_vec();
        let existing = book.comic_info()?.map(|i| i.to_xml()).transpose()?;
        let xml = match record {
            Some(r) => Some(comic_info_from_record(r, existing.as_deref())?),
            None => existing,
        };
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(now_zip())
            .large_file(false);
        let mut w = zip::ZipWriter::new(File::create(&tmp)?);
        let mut used = std::collections::HashSet::new();
        for (i, name) in names.iter().enumerate() {
            let bytes = book.read_page(i)?;
            let mut entry = name.replace('\\', "/");
            // Noms en double (rare) : suffixe pour ne rien perdre.
            if !used.insert(entry.to_ascii_lowercase()) {
                entry = format!("{i:04}-{entry}");
                used.insert(entry.to_ascii_lowercase());
            }
            w.start_file(entry.as_str(), stored.large_file(bytes.len() as u64 >= u32::MAX as u64))?;
            w.write_all(&bytes)?;
        }
        if let Some(xml) = &xml {
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(now_zip());
            w.start_file("ComicInfo.xml", opts)?;
            w.write_all(xml.as_bytes())?;
        }
        w.finish()?.sync_all()?;
        drop(book);
        // Vérification : mêmes pages, dans le même ordre.
        let check = providers::open_book(&tmp)?;
        if check.pages().len() != names.len() {
            return Err(Error::Archive(format!(
                "vérification : {} pages au lieu de {}",
                check.pages().len(),
                names.len()
            )));
        }
        drop(check);
        // Sans jamais écraser un CBZ apparu entre-temps (`rename_no_replace`).
        let delays: Vec<std::time::Duration> =
            REPLACE_RETRY_DELAYS_MS.iter().map(|&ms| std::time::Duration::from_millis(ms)).collect();
        replace_with(&tmp, &dest, &delays, crate::edit::files::rename_no_replace)?;
        Ok(Converted { path: dest.clone(), pages: names.len() })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::testutil::png;

    /// Fiche de la base au format de ComicRack (extrait), avec des champs propres à la
    /// bibliothèque qui ne doivent pas aller dans le fichier.
    const RECORD: &str = r#"<?xml version="1.0"?>
<ComicBook xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Id="30815f16-abb9-4c38-8c24-ecae11e25b1b" File="Z:\BD\Thanos 01.cbz">
  <Series>Thanos &amp; Cie</Series>
  <Number>1</Number>
  <Year>2013</Year>
  <Title>L'ascension</Title>
  <Writer>Jason Aaron</Writer>
  <Manga>No</Manga>
  <Added>2019-10-01T12:58:00+02:00</Added>
  <CurrentPage>12</CurrentPage>
  <ComicInfoIsDirty>true</ComicInfoIsDirty>
  <Pages>
    <Page Image="0" Type="FrontCover" ImageWidth="8" ImageHeight="12" />
    <Page Image="1" Bookmark="Combat" />
  </Pages>
</ComicBook>"#;

    fn cbz(path: &Path, info: Option<&str>) {
        let mut w = zip::ZipWriter::new(File::create(path).unwrap());
        let deflate = zip::write::SimpleFileOptions::default();
        for (i, n) in ["p01.png", "p02.png", "p03.png"].iter().enumerate() {
            w.start_file(*n, deflate).unwrap();
            w.write_all(&png(i as u8 * 40)).unwrap();
        }
        if let Some(info) = info {
            w.start_file("ComicInfo.xml", deflate).unwrap();
            w.write_all(info.as_bytes()).unwrap();
        }
        w.set_comment("commentaire");
        w.finish().unwrap();
    }

    #[test]
    fn record_to_comic_info() {
        let xml = comic_info_from_record(
            RECORD,
            Some("<ComicInfo><Series>Ancien</Series><GTIN>9781302</GTIN><Summary>x</Summary></ComicInfo>"),
        )
        .unwrap();
        // Ordre de ComicRack, champs de la bibliothèque exclus, inconnus gardés.
        let names: Vec<String> = top_elements(&xml).unwrap().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["Title", "Series", "Number", "Year", "Writer", "Manga", "GTIN", "Pages"]);
        assert!(xml.contains("<Series>Thanos &amp; Cie</Series>"));
        assert!(xml.contains(r#"Bookmark="Combat""#));
        let info = ComicInfo::from_xml(&xml).unwrap();
        assert_eq!(info.series.as_deref(), Some("Thanos & Cie"));
        assert_eq!(info.pages.unwrap().page.len(), 2);
        assert!(info.summary.is_none(), "champ effacé dans ComicRack : effacé dans le fichier");
    }

    #[test]
    fn pages_of_the_file_kept_when_record_has_none() {
        let rec = r#"<ComicBook Id="x"><Series>S</Series></ComicBook>"#;
        let old = r#"<ComicInfo><Pages><Page Image="0" Type="FrontCover"/></Pages></ComicInfo>"#;
        let xml = comic_info_from_record(rec, Some(old)).unwrap();
        assert!(xml.contains(r#"<Page Image="0" Type="FrontCover"/>"#));
    }

    /// Pages modifiées dans l'onglet « Pages » (base ComicRust) : le ComicInfo.xml les
    /// reçoit, ordre et attributs compris, et le lecteur les relit.
    #[test]
    fn edited_pages_go_to_the_comic_info() {
        let mut l = crate::pages::full_list(&[], 3);
        crate::pages::move_to_start(&mut l, &[2]);
        crate::pages::set_type(&mut l, &[1], "Deleted");
        crate::pages::set_rotation(&mut l, &[0], "Rotate180");
        let b = crate::library::LibraryBook {
            id: "x".into(),
            series: Some("S".into()),
            pages: crate::library::PageList { page: l },
            ..Default::default()
        };
        let old = r#"<ComicInfo><Pages><Page Image="0" Type="FrontCover"/></Pages></ComicInfo>"#;
        let xml = comic_info_from_record(&b.to_record_xml().unwrap(), Some(old)).unwrap();
        let ci = ComicInfo::from_xml(&xml).unwrap();
        let read = crate::pages::from_comic_info(&ci);
        assert_eq!(read.iter().map(|p| p.image).collect::<Vec<_>>(), [2, 0, 1], "{xml}");
        let order = crate::pages::reading_order(&read, 3);
        assert_eq!(order.iter().map(|p| (p.image, p.rotation)).collect::<Vec<_>>(), [(2, 0), (0, 180)], "{xml}");
    }

    #[test]
    fn comparison_ignores_layout() {
        let a = "<ComicInfo><Series>S</Series>\n<Pages>\n  <Page Image=\"0\" />\n</Pages></ComicInfo>";
        let b = "<?xml version=\"1.0\"?>\n<ComicInfo>\n  <Pages><Page Image=\"0\"/></Pages>\n  <Series>S</Series>\n</ComicInfo>";
        assert!(same_comic_info(a, b));
        assert!(!same_comic_info(a, &b.replace(">S<", ">T<")));
    }

    #[test]
    fn writes_into_cbz() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Thanos 01.cbz");
        cbz(&path, Some("<ComicInfo><Series>Ancien</Series><GTIN>978</GTIN></ComicInfo>"));
        let before = std::fs::read(&path).unwrap();

        assert_eq!(write_comic_info(&path, RECORD).unwrap(), Outcome::Written);
        assert!(!temp_path(&path).exists());
        let mut book = providers::open_book(&path).unwrap();
        assert_eq!(book.pages(), ["p01.png", "p02.png", "p03.png"]);
        assert_eq!(book.read_page(2).unwrap(), png(80), "pages recopiées à l'identique");
        let info = book.comic_info().unwrap().unwrap();
        assert_eq!((info.series.as_deref(), info.year), (Some("Thanos & Cie"), Some(2013)));
        drop(book);
        let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
        assert_eq!(zip.len(), 4);
        assert_eq!(zip.comment(), b"commentaire");
        let mut text = String::new();
        zip.by_name("ComicInfo.xml").unwrap().read_to_string(&mut text).unwrap();
        assert!(text.contains("<GTIN>978</GTIN>"));
        drop(zip);

        // Deuxième fois : rien à faire, fichier intact.
        let after = std::fs::read(&path).unwrap();
        assert_ne!(before, after);
        assert_eq!(write_comic_info(&path, RECORD).unwrap(), Outcome::Unchanged);
        assert_eq!(std::fs::read(&path).unwrap(), after);

        // CBZ sans ComicInfo.xml : créé.
        let bare = dir.path().join("nu.cbz");
        cbz(&bare, None);
        assert_eq!(write_comic_info(&bare, RECORD).unwrap(), Outcome::Written);
        let mut book = providers::open_book(&bare).unwrap();
        assert_eq!(book.comic_info().unwrap().unwrap().number.as_deref(), Some("1"));
    }

    /// Refus passager (antivirus, indexation, verrou SMB relâché avec retard) : nouvelles
    /// tentatives, puis succès ; refus persistant : abandon, fichier temporaire supprimé.
    #[test]
    fn replace_retries_on_access_denied() {
        use std::io::{Error as IoError, ErrorKind};
        let dir = tempfile::tempdir().unwrap();
        let (tmp, path) = (dir.path().join("a.cbz.comicrust.tmp"), dir.path().join("a.cbz"));
        std::fs::write(&tmp, b"nouveau").unwrap();
        std::fs::write(&path, b"ancien").unwrap();
        let delays = [std::time::Duration::from_millis(1); 4];

        let mut calls = 0;
        replace_with(&tmp, &path, &delays, |a, b| {
            calls += 1;
            if calls < 3 {
                Err(IoError::new(ErrorKind::PermissionDenied, "Accès refusé. (os error 5)"))
            } else {
                std::fs::rename(a, b)
            }
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(std::fs::read(&path).unwrap(), b"nouveau");
        assert!(!tmp.exists());

        std::fs::write(&tmp, b"encore").unwrap();
        let mut calls = 0;
        let err = replace_with(&tmp, &path, &delays, |_, _| {
            calls += 1;
            Err(IoError::new(ErrorKind::PermissionDenied, "Accès refusé. (os error 5)"))
        })
        .unwrap_err()
        .to_string();
        assert_eq!(calls, delays.len() + 1, "une tentative + une par attente");
        assert!(err.contains("5 tentatives") && err.contains("ouvert dans ComicRack"), "{err}");
        assert!(!tmp.exists(), "fichier temporaire supprimé");
        assert_eq!(std::fs::read(&path).unwrap(), b"nouveau", "original intact");

        // Autre erreur (dossier disparu…) : pas d'attente inutile.
        std::fs::write(&tmp, b"x").unwrap();
        let mut calls = 0;
        assert!(replace_with(&tmp, &path, &delays, |_, _| {
            calls += 1;
            Err(IoError::new(ErrorKind::NotFound, "introuvable"))
        })
        .is_err());
        assert_eq!(calls, 1);
    }

    /// Le livre affiché (fournisseur ouvert, pages et ComicInfo.xml lus) ne garde aucune
    /// poignée : l'écriture des infos réussit et l'ancien fournisseur signale le changement
    /// au lieu de lire des octets décalés.
    #[test]
    fn writes_while_the_book_is_displayed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Akira 02.cbz");
        cbz(&path, Some("<ComicInfo><Series>Ancien</Series></ComicInfo>"));
        let mut shown = providers::open_book(&path).unwrap();
        shown.render_page(0).unwrap();
        shown.comic_info().unwrap();
        #[cfg(target_os = "linux")]
        assert_eq!(providers::on_demand::open_descriptors(&path), 0);
        assert_eq!(write_comic_info(&path, RECORD).unwrap(), Outcome::Written);
        #[cfg(target_os = "linux")]
        assert_eq!(providers::on_demand::open_descriptors(&path), 0, "écriture : tout refermé");
        assert!(shown.read_page(0).unwrap_err().to_string().contains("a changé"));
        let mut fresh = providers::open_book(&path).unwrap();
        assert_eq!(fresh.comic_info().unwrap().unwrap().series.as_deref(), Some("Thanos & Cie"));
    }

    #[test]
    fn refuses_other_formats_and_files_being_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let rar = dir.path().join("a.cbr");
        std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rar4.cbr"), &rar).unwrap();
        let err = write_comic_info(&rar, RECORD).unwrap_err().to_string();
        assert!(err.contains("convertir d'abord en CBZ"), "{err}");

        let path = dir.path().join("b.cbz");
        cbz(&path, None);
        std::fs::write(dir.path().join("b.cbz.tmp"), b"").unwrap();
        assert!(write_comic_info(&path, RECORD).unwrap_err().to_string().contains("ComicRack"));
    }

    #[test]
    fn converts_rar_to_cbz() {
        let dir = tempfile::tempdir().unwrap();
        for fixture in ["rar4.cbr", "solid_rar5.cbr"] {
            let src = dir.path().join(fixture);
            std::fs::copy(format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR")), &src).unwrap();
            assert!(can_convert(&src));
            let original = std::fs::read(&src).unwrap();
            let mut rar = providers::open_book(&src).unwrap();
            let rar_pages: Vec<Vec<u8>> = (0..rar.pages().len()).map(|i| rar.read_page(i).unwrap()).collect();
            drop(rar);

            let done = convert_to_cbz(&src, Some(RECORD)).unwrap();
            assert_eq!(done.path, src.with_extension("cbz"));
            assert_eq!(done.pages, 3);
            assert_eq!(std::fs::read(&src).unwrap(), original, "original intact");
            let mut cbz = providers::open_book(&done.path).unwrap();
            assert_eq!(cbz.format(), BookFormat::Zip);
            assert_eq!(cbz.pages(), ["1.png", "2.png", "10.png"]);
            for (i, bytes) in rar_pages.iter().enumerate() {
                assert_eq!(&cbz.read_page(i).unwrap(), bytes);
            }
            assert_eq!(cbz.comic_info().unwrap().unwrap().series.as_deref(), Some("Thanos & Cie"));
            drop(cbz);
            // Déjà converti : refus, rien d'écrasé.
            assert!(convert_to_cbz(&src, None).unwrap_err().to_string().contains("existe déjà"));
            std::fs::remove_file(&done.path).unwrap();
        }
        // Sans fiche : le ComicInfo.xml de l'archive est repris.
        let src = dir.path().join("rar4.cbr");
        let done = convert_to_cbz(&src, None).unwrap();
        let mut cbz = providers::open_book(&done.path).unwrap();
        assert_eq!(cbz.comic_info().unwrap().unwrap().series.as_deref(), Some("Thanos"));
        assert!(!can_convert(&done.path));
    }

    /// Fiche ComicRack avec une page de publicité (`Advertisment`) et un type combiné :
    /// l'écriture des infos dans le CBZ réussit (refusée avant la v1.0.2).
    const RECORD_ADS: &str = r#"<?xml version="1.0"?>
<ComicBook xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Id="30815f16-abb9-4c38-8c24-ecae11e25b1b" File="Z:\BD\Thanos 01.cbz">
  <Series>Thanos</Series>
  <Number>1</Number>
  <Pages>
    <Page Image="0" Type="FrontCover, Story" />
    <Page Image="1" Type="Advertisment" />
    <Page Image="2" />
  </Pages>
</ComicBook>"#;

    #[test]
    fn writes_comicrack_page_types() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.cbz");
        cbz(&path, None);
        assert_eq!(write_comic_info(&path, RECORD_ADS).unwrap(), Outcome::Written);
        let mut book = providers::open_book(&path).unwrap();
        let info = book.comic_info().unwrap().unwrap();
        let p = info.pages.unwrap().page;
        assert_eq!(p[1].page_type.as_str(), "Advertisment");
        assert_eq!(p[0].page_type.as_str(), "FrontCover, Story");
        drop(book);
        // L'analyse lit aussi ce ComicInfo.xml (il était ignoré en entier).
        let b = crate::scanner::book_from_file(&path, "x".into()).unwrap();
        assert_eq!(b.series.as_deref(), Some("Thanos"));
        assert_eq!(b.pages.page[1].kind.as_deref(), Some("Advertisment"));
    }

    /// ComicInfo.xml existant en UTF-16 (avec BOM) : décodé, ses éléments inconnus de
    /// ComicRack sont gardés (ils étaient perdus).
    #[test]
    fn utf16_comic_info_keeps_unknown_elements() {
        for be in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("b.cbz");
            let text = "<?xml version=\"1.0\" encoding=\"utf-16\"?><ComicInfo><Series>Ancien</Series><GTIN>978</GTIN></ComicInfo>";
            let mut bytes = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
            bytes.extend(text.encode_utf16().flat_map(|u| if be { u.to_be_bytes() } else { u.to_le_bytes() }));
            let mut w = zip::ZipWriter::new(File::create(&path).unwrap());
            w.start_file("p01.png", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(&png(0)).unwrap();
            w.start_file("ComicInfo.xml", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(&bytes).unwrap();
            w.finish().unwrap();
            assert_eq!(write_comic_info(&path, RECORD).unwrap(), Outcome::Written);
            let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
            let mut s = String::new();
            zip.by_name("ComicInfo.xml").unwrap().read_to_string(&mut s).unwrap();
            assert!(s.contains("<GTIN>978</GTIN>") && s.contains("Thanos &amp; Cie"), "{s}");
        }
    }

    /// Original réécrit par un autre programme pendant l'écriture : rien n'est remplacé
    /// (sa version n'est pas écrasée) et le fichier temporaire est supprimé.
    #[test]
    fn original_modified_during_write_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.cbz");
        cbz(&path, None);
        let err = write_comic_info_with(&path, RECORD, &mut || {
            std::fs::write(&path, b"version de ComicRack").unwrap();
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("modifié pendant l'écriture"), "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), b"version de ComicRack");
        assert!(!temp_path(&path).exists());
    }

    /// Conversion : un CBZ apparu juste avant le remplacement n'est jamais écrasé.
    #[test]
    fn conversion_never_overwrites_an_existing_cbz() {
        let dir = tempfile::tempdir().unwrap();
        let (tmp, dest) = (dir.path().join("x.cbz.comicrust.tmp"), dir.path().join("x.cbz"));
        std::fs::write(&tmp, b"converti").unwrap();
        std::fs::write(&dest, b"apparu entre-temps").unwrap();
        let err = replace_with(&tmp, &dest, &[], crate::edit::files::rename_no_replace).unwrap_err().to_string();
        assert!(err.contains("existe déjà"), "{err}");
        assert_eq!(std::fs::read(&dest).unwrap(), b"apparu entre-temps");
        assert!(!tmp.exists());
    }

    /// Archive CBT dont le ComicInfo.xml a été écrit par ComicRack (publicité, types
    /// combinés, nombre vide) : la conversion en CBZ réussit et garde les types.
    #[test]
    fn converts_archive_with_comicrack_page_types() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("t.cbt");
        let info = r#"<ComicInfo><Series>Thanos</Series><Count></Count><Pages><Page Image="0" Type="FrontCover, Story"/><Page Image="1" Type="Advertisment"/></Pages></ComicInfo>"#;
        let mut t = tar::Builder::new(File::create(&src).unwrap());
        for (name, data) in [("p01.png", png(0)), ("p02.png", png(40)), ("ComicInfo.xml", info.as_bytes().to_vec())] {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            t.append_data(&mut h, name, data.as_slice()).unwrap();
        }
        t.into_inner().unwrap();
        let done = convert_to_cbz(&src, None).unwrap();
        assert_eq!(done.pages, 2);
        let mut cbz = providers::open_book(&done.path).unwrap();
        let ci = cbz.comic_info().unwrap().unwrap();
        assert_eq!(ci.series.as_deref(), Some("Thanos"));
        let p = ci.pages.unwrap().page;
        assert_eq!((p[0].page_type.as_str(), p[1].page_type.as_str()), ("FrontCover, Story", "Advertisment"));
    }
}
