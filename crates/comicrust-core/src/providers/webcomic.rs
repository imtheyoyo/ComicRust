//! WebComic (`.cbw`) : fichier XML ComicRack décrivant une BD en ligne.
//!
//! ```xml
//! <WebComic>
//!   <Info><Series>Ma BD</Series></Info>            <!-- ComicInfo -->
//!   <Variables><Variable Key="base" Value="https://exemple.org/bd"/></Variables>
//!   <Images>
//!     <Image Url="{base}/couv.jpg"/>
//!     <Image Url="{base}/p[000:1-24].jpg"/>        <!-- p001.jpg … p024.jpg -->
//!   </Images>
//! </WebComic>
//! ```
//!
//! Pris en charge : URL http(s)/file, chemins relatifs au `.cbw`, variables
//! (`{ComicFileName}`, `{ComicFilePath}` et celles du fichier), plages
//! numérotées `[format:début-fin]`. Pas encore : les « scrapers » (`!`, `?`)
//! et la composition de plusieurs images par page.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use super::{BookFormat, PageProvider};
use crate::{ComicInfo, Error};

#[derive(Debug, Default, Deserialize)]
struct Variable {
    #[serde(rename = "@Key")]
    key: String,
    #[serde(rename = "@Value", default)]
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct Variables {
    #[serde(rename = "Variable", default)]
    variable: Vec<Variable>,
}

#[derive(Debug, Default, Deserialize)]
struct ImageLink {
    #[serde(rename = "@Url")]
    url: String,
}

#[derive(Debug, Default, Deserialize)]
struct Images {
    #[serde(rename = "Image", default)]
    image: Vec<ImageLink>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename = "WebComic")]
pub struct WebComic {
    #[serde(rename = "Info", default)]
    pub info: Option<ComicInfo>,
    #[serde(rename = "Variables", default)]
    variables: Variables,
    #[serde(rename = "Images", default)]
    images: Images,
}

impl WebComic {
    pub fn from_xml(xml: &str) -> Result<Self, Error> {
        Ok(quick_xml::de::from_str(xml.trim_start_matches('\u{feff}'))?)
    }

    /// Liste des URL de pages, variables et plages résolues.
    pub fn page_urls(&self, cbw_path: &Path) -> Result<Vec<String>, Error> {
        let dir = cbw_path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut vars: Vec<(String, String)> = self
            .variables
            .variable
            .iter()
            .map(|v| (v.key.clone(), v.value.clone()))
            .collect();
        vars.push((
            "ComicFileName".into(),
            cbw_path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
        ));
        vars.push(("ComicFilePath".into(), format!("file://{}", dir.display())));

        let rx = Regex::new(r"\[(?<format>\d+):(?<start>\d+)-(?<end>\d+)\]").unwrap();
        let mut out = Vec::new();
        for img in &self.images.image {
            // Comme ComicRack : `|` sépare l'URL des motifs de scraper.
            let mut url = img.url.split('|').next().unwrap_or_default().trim().to_string();
            for (k, v) in &vars {
                url = url.replace(&format!("{{{k}}}"), v);
            }
            if url.starts_with('!') || url.starts_with('?') {
                return Err(Error::Unsupported("WebComic avec scraper (! ou ?)".into()));
            }
            match rx.captures(&url) {
                None => out.push(url),
                Some(c) => {
                    let pad = c["format"].chars().filter(|&ch| ch == '0').count().max(1);
                    let (start, end): (u64, u64) = (c["start"].parse().unwrap(), c["end"].parse().unwrap());
                    for i in start..=end {
                        out.push(rx.replace(&url, format!("{i:0pad$}").as_str()).into_owned());
                    }
                }
            }
        }
        Ok(out)
    }
}

pub struct WebComicProvider {
    base_dir: PathBuf,
    info: Option<ComicInfo>,
    urls: Vec<String>,
    pages: Vec<String>,
    cache: HashMap<usize, Vec<u8>>,
}

impl WebComicProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let wc = WebComic::from_xml(&std::fs::read_to_string(path)?)?;
        let urls = wc.page_urls(path)?;
        let pages = urls
            .iter()
            .map(|u| u.rsplit(['/', '\\']).next().unwrap_or(u).to_string())
            .collect();
        Ok(Self {
            base_dir: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            info: wc.info,
            urls,
            pages,
            cache: HashMap::new(),
        })
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        if let Some(rest) = url.strip_prefix("file://") {
            let p = rest.strip_prefix('/').filter(|r| r.as_bytes().get(1) == Some(&b':')).unwrap_or(rest);
            return Ok(std::fs::read(p)?);
        }
        if url.starts_with("http://") || url.starts_with("https://") {
            let resp = ureq::get(url)
                .header("User-Agent", concat!("ComicRust/", env!("CARGO_PKG_VERSION")))
                .call()
                .map_err(|e| Error::Network(format!("{url} : {e}")))?;
            return resp
                .into_body()
                .with_config()
                .limit(64 * 1024 * 1024)
                .read_to_vec()
                .map_err(|e| Error::Network(format!("{url} : {e}")));
        }
        Ok(std::fs::read(self.base_dir.join(url))?)
    }
}

impl PageProvider for WebComicProvider {
    fn format(&self) -> BookFormat {
        BookFormat::WebComic
    }
    fn pages(&self) -> &[String] {
        &self.pages
    }
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        if let Some(b) = self.cache.get(&index) {
            return Ok(b.clone());
        }
        let url = self.urls.get(index).ok_or(Error::PageOutOfRange(index))?;
        let bytes = self.fetch(url)?;
        self.cache.insert(index, bytes.clone());
        Ok(bytes)
    }
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        Ok(self.info.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::testutil::png;

    #[test]
    fn cbw_with_variables_and_ranges() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("img")).unwrap();
        for i in 1..=3u8 {
            std::fs::write(dir.path().join(format!("img/p{i:03}.png")), png(i)).unwrap();
        }
        std::fs::write(dir.path().join("img/cover.png"), png(0)).unwrap();
        let path = dir.path().join("t.cbw");
        std::fs::write(
            &path,
            r#"<?xml version="1.0"?>
<WebComic>
  <Info><Series>Web BD</Series><Number>1</Number></Info>
  <Variables><Variable Key="d" Value="img"/></Variables>
  <Images>
    <Image Url="{d}/cover.png"/>
    <Image Url="{d}/p[000:1-3].png"/>
  </Images>
</WebComic>"#,
        )
        .unwrap();
        assert_eq!(BookFormat::detect(&path), Some(BookFormat::WebComic));
        let mut book = WebComicProvider::open(&path).unwrap();
        assert_eq!(book.pages(), ["cover.png", "p001.png", "p002.png", "p003.png"]);
        assert_eq!(book.read_page(3).unwrap(), png(3));
        assert_eq!(book.render_page(1).unwrap().get_pixel(0, 0).0, [1, 254, 128, 255]);
        assert_eq!(book.comic_info().unwrap().unwrap().series.as_deref(), Some("Web BD"));
    }
}
