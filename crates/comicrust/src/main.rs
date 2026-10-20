//! `comicrust` — petite CLI pour tester le portage sur de vrais fichiers.
//!
//!   comicrust info   <livre.cbz|dossier>          métadonnées + liste des pages
//!   comicrust cover  <livre.cbz> <sortie>          extrait la 1re page
//!   comicrust cbl    <liste.cbl>                   affiche une liste de lecture
//!   comicrust missing <ComicDB.xml> <liste.cbl>    numéros de la liste absents de la base

use std::path::Path;

use anyhow::{bail, Context, Result};
use comicrust_core::{open_book, Library, ReadingList};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["info", path] => info(Path::new(path)),
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
        ["missing", db, list] => {
            let lib = Library::from_xml(&std::fs::read_to_string(db).context("lecture ComicDB.xml")?)?;
            let rl = ReadingList::from_xml(&std::fs::read_to_string(list)?)?;
            let res = lib.resolve(&rl);
            let mut missing = 0;
            for (e, hit) in rl.books.book.iter().zip(&res) {
                if hit.is_none() {
                    missing += 1;
                    println!("MANQUANT  {} #{} ({})", e.series, e.number, e.year.unwrap_or_default());
                }
            }
            println!("{missing} manquant(s) sur {}", res.len());
            Ok(())
        }
        _ => bail!("usage : comicrust info|cover|cbl|missing … (voir README)"),
    }
}

fn info(path: &Path) -> Result<()> {
    let mut book = open_book(path)?;
    match book.comic_info()? {
        Some(ci) => {
            println!("{}", ci.caption());
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
    Ok(())
}
