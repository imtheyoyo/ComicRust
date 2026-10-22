//! `comicrust-cli` — ComicRust en ligne de commande.
//!
//!   comicrust-cli info    <livre>                  format, métadonnées, pages
//!   comicrust-cli render  <livre> <page> <sortie>  décode une page (n° à partir de 1) en PNG/JPEG
//!   comicrust-cli cover   <livre> <sortie>         extrait la 1re page telle quelle
//!   comicrust-cli cbl     <liste.cbl>              affiche une liste de lecture
//!   comicrust-cli convert <livre.cbr|cb7|cbt>      crée le CBZ à côté (original gardé)
//!   comicrust-cli missing <ComicDB.xml|dossier> <liste.cbl>
//!                                                  numéros de la liste introuvables
//!
//! <livre> : cbz, zip, cbr, rar, cb7, 7z, cbt, tar, pdf, djvu, cbw ou dossier d'images.

use std::path::Path;

use anyhow::{bail, Context, Result};
use comicrust_core::index::FileIndex;
use comicrust_core::{open_book, Library, ReadingList};

const USAGE: &str = "usage :
  comicrust-cli info    <livre>
  comicrust-cli render  <livre> <page> <sortie.png>
  comicrust-cli cover   <livre> <sortie>
  comicrust-cli cbl     <liste.cbl>
  comicrust-cli convert <livre.cbr|cb7|cbt>
  comicrust-cli missing <ComicDB.xml|dossier> <liste.cbl>";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["info", path] => info(Path::new(path)),
        ["render", path, page, out] => {
            let n: usize = page.parse().context("numéro de page invalide")?;
            let mut book = open_book(Path::new(path))?;
            let img = book.render_page(n.checked_sub(1).context("les pages commencent à 1")?)?;
            img.save(out).with_context(|| format!("écriture de {out}"))?;
            println!("Page {n} ({}×{}) écrite dans {out}", img.width(), img.height());
            Ok(())
        }
        ["cover", path, out] => {
            let mut book = open_book(Path::new(path))?;
            std::fs::write(out, book.read_page(0)?)?;
            println!("Couverture écrite dans {out}");
            Ok(())
        }
        ["cbl", path] => {
            let rl = ReadingList::from_xml(&std::fs::read_to_string(path)?)?;
            println!("{} — {} livres", rl.name, rl.books.book.len());
            for (i, b) in rl.books.book.iter().enumerate() {
                println!("{:4}. {} #{} ({})", i + 1, b.series, b.number, b.year.unwrap_or_default());
            }
            Ok(())
        }
        ["convert", path] => {
            let done = comicrust_core::writer::convert_to_cbz(Path::new(path), None)?;
            println!("{} pages → {}", done.pages, done.path.display());
            Ok(())
        }
        ["missing", source, list] => missing(Path::new(source), Path::new(list)),
        _ => bail!("{USAGE}"),
    }
}

fn info(path: &Path) -> Result<()> {
    let mut book = open_book(path)?;
    println!("Format   : {}", book.format().label());
    match book.comic_info()? {
        Some(ci) => {
            println!("Titre    : {}", ci.caption());
            if let Some(w) = &ci.writer {
                println!("Scénario : {w}");
            }
            if let Some(p) = &ci.publisher {
                println!("Éditeur  : {p}");
            }
        }
        None => println!("(pas de ComicInfo.xml)"),
    }
    println!("{} pages :", book.pages().len());
    for p in book.pages().iter().take(10) {
        println!("  {p}");
    }
    if book.pages().len() > 10 {
        println!("  …");
    }
    Ok(())
}

fn missing(source: &Path, list: &Path) -> Result<()> {
    let rl = ReadingList::from_xml(&std::fs::read_to_string(list)?)?;
    let found: Vec<bool> = if source.is_dir() {
        let idx = FileIndex::scan(source);
        println!("{} — {}", source.display(), idx.summary());
        rl.books.book.iter().map(|b| idx.find(&b.series, &b.number, b.year, b.volume).is_some()).collect()
    } else {
        let lib = Library::from_xml(&std::fs::read_to_string(source).context("lecture ComicDB.xml")?)?;
        lib.resolve(&rl).iter().map(Option::is_some).collect()
    };
    let mut n = 0;
    for (e, ok) in rl.books.book.iter().zip(&found) {
        if !ok {
            n += 1;
            println!("MANQUANT  {} #{} ({})", e.series, e.number, e.year.unwrap_or_default());
        }
    }
    println!("{n} manquant(s) sur {}", found.len());
    Ok(())
}
