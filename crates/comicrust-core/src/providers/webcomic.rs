//! WebComic (`.cbw`) : fichier XML ComicRack décrivant une BD en ligne (portage de
//! `WebComic` et `WebComicProvider`).
//!
//! ```xml
//! <WebComic>
//!   <Info><Series>Ma BD</Series></Info>            <!-- ComicInfo -->
//!   <Variables><Variable Key="base" Value="https://exemple.org/bd"/></Variables>
//!   <Images>
//!     <Image Url="{base}/couv.jpg"/>
//!     <Image Url="{base}/p[000:1-24].jpg"/>        <!-- p001.jpg … p024.jpg -->
//!     <!-- « scraper » d'index : page d'index, puis expression des liens d'images -->
//!     <Image Url="!{base}/archive.html|&lt;img src=&quot;(?&lt;link&gt;[^&quot;]+\.png)"/>
//!     <!-- « scraper » de navigation : première page, images de chaque page, lien « suivant » -->
//!     <Image Type="BrowseScraper">
//!       <Parts>
//!         <Part>{base}/strip/1</Part>
//!         <Part>class="strip" src="(?&lt;link&gt;[^"]+)"</Part>
//!         <Part>rel="next" href="(?&lt;link&gt;[^"]+)"</Part>
//!       </Parts>
//!     </Image>
//!   </Images>
//! </WebComic>
//! ```
//!
//! Comme ComicRack :
//! - URL `http(s)`, `file://` ou relatives au `.cbw` ; variables (`{ComicFileName}`,
//!   `{ComicFilePath}` et celles du fichier) ; plages numérotées `[format:début-fin]` ;
//! - **scraper d'index** (`!` devant l'URL, ou `Type="IndexScraper"`) : chaque motif
//!   (séparé par `|` ou donné par `<Part>`) relève les liens de la page précédente ; le
//!   dernier relève les images. Options d'un motif : `!` (ordre inversé), `+` (garder aussi
//!   la page elle-même), attributs `Reverse`, `AddOwn`, `Sort`, `Cut` (ne chercher que
//!   dans la partie de la page qui correspond), `MaximumMatches` ;
//! - **scraper de navigation** (`?` ou `Type="BrowseScraper"`) : première page, motifs des
//!   images de chaque page, puis motif du lien vers la page suivante ;
//! - le groupe nommé `link` donne le lien (sinon toute la correspondance) ; `<base href>`
//!   de la page est respecté ; expressions insensibles à la casse ;
//! - **composition** (`<Compositing>` du WebComic ou d'une image) : plusieurs images par
//!   page (`Rows` × `Columns`), ou rangées dans une page de taille fixe (`PageWidth`,
//!   `PageHeight`), avec bordure (`BorderWidth`, en % de la diagonale), couleur de fond et
//!   lecture de droite à gauche.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use fancy_regex::Regex;
use serde::Deserialize;

use super::{decode_image, encode_png, BookFormat, PageImage, PageProvider};
use crate::{ComicInfo, Error};

/// Garde-fou : nombre maximal de pages visitées par un scraper de navigation.
const MAX_BROWSE_PAGES: usize = 20_000;

#[derive(Debug, Default, Clone, Deserialize)]
struct Variable {
    #[serde(rename = "@Key")]
    key: String,
    #[serde(rename = "@Value", default)]
    value: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct Variables {
    #[serde(rename = "Variable", default)]
    variable: Vec<Variable>,
}

fn one() -> i32 {
    1
}

/// `PageCompositing`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Compositing {
    #[serde(rename = "@PageWidth", default)]
    pub page_width: i32,
    #[serde(rename = "@PageHeight", default)]
    pub page_height: i32,
    #[serde(rename = "@Rows", default = "one")]
    pub rows: i32,
    #[serde(rename = "@Columns", default = "one")]
    pub columns: i32,
    #[serde(rename = "@BorderWidth", default)]
    pub border_width: i32,
    #[serde(rename = "@BackgroundColor", default)]
    pub background_color: Option<String>,
    #[serde(rename = "@RightToLeft", default)]
    pub right_to_left: bool,
}

impl Default for Compositing {
    fn default() -> Self {
        Self {
            page_width: 0,
            page_height: 0,
            rows: 1,
            columns: 1,
            border_width: 0,
            background_color: None,
            right_to_left: false,
        }
    }
}

impl Compositing {
    fn page_size_empty(&self) -> bool {
        self.page_width == 0 && self.page_height == 0
    }

    /// Une image par page, telle quelle ?
    fn is_empty(&self) -> bool {
        if self.columns >= 1 && self.rows >= 1 {
            if self.columns == 1 && self.rows == 1 && self.border_width == 0 {
                return self.page_size_empty();
            }
            return false;
        }
        true
    }

    /// `ColorTranslator.FromHtml`, blanc par défaut.
    fn back_color(&self) -> image::Rgba<u8> {
        let white = image::Rgba([255, 255, 255, 255]);
        let Some(c) = self.background_color.as_deref().map(str::trim).filter(|c| !c.is_empty()) else {
            return white;
        };
        let hex = |s: &str| u8::from_str_radix(s, 16).ok();
        if let Some(h) = c.strip_prefix('#') {
            let rgb = match h.len() {
                6 => (hex(&h[0..2]), hex(&h[2..4]), hex(&h[4..6])),
                3 => {
                    let d = |i: usize| hex(&h[i..i + 1]).map(|v| v * 17);
                    (d(0), d(1), d(2))
                }
                _ => (None, None, None),
            };
            return match rgb {
                (Some(r), Some(g), Some(b)) => image::Rgba([r, g, b, 255]),
                _ => white,
            };
        }
        let rgb = match c.to_ascii_lowercase().as_str() {
            "black" => [0, 0, 0],
            "gray" | "grey" => [128, 128, 128],
            "darkgray" | "darkgrey" => [169, 169, 169],
            "lightgray" | "lightgrey" => [211, 211, 211],
            "silver" => [192, 192, 192],
            "red" => [255, 0, 0],
            "green" => [0, 128, 0],
            "blue" => [0, 0, 255],
            "yellow" => [255, 255, 0],
            "navy" => [0, 0, 128],
            "beige" => [245, 245, 220],
            "ivory" => [255, 255, 240],
            _ => [255, 255, 255],
        };
        image::Rgba([rgb[0], rgb[1], rgb[2], 255])
    }
}

/// `PagePart` : un motif de scraper.
#[derive(Debug, Clone, Default, Deserialize)]
struct Part {
    #[serde(rename = "@MaximumMatches", default)]
    maximum_matches: Option<usize>,
    #[serde(rename = "@AddOwn", default)]
    add_own: bool,
    #[serde(rename = "@Reverse", default)]
    reverse: bool,
    #[serde(rename = "@Sort", default)]
    sort: bool,
    #[serde(rename = "@Cut", default)]
    cut: Option<String>,
    #[serde(rename = "$text", default)]
    pattern: String,
}

impl Part {
    fn new(pattern: &str) -> Self {
        Self { pattern: pattern.to_string(), ..Default::default() }
    }

    /// `HasOption` : le motif commence-t-il par `option` ? (l'option est alors retirée)
    fn take_option(&mut self, option: char) -> bool {
        match self.pattern.strip_prefix(option) {
            Some(rest) => {
                self.pattern = rest.to_string();
                true
            }
            None => false,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
struct Parts {
    #[serde(rename = "Part", default)]
    part: Vec<Part>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct ImageLink {
    #[serde(rename = "@Url", default)]
    url: String,
    #[serde(rename = "@Type", default)]
    kind: Option<String>,
    #[serde(rename = "Parts", default)]
    parts: Option<Parts>,
    #[serde(rename = "Compositing", default)]
    compositing: Option<Compositing>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct Images {
    #[serde(rename = "Image", default)]
    image: Vec<ImageLink>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename = "WebComic")]
pub struct WebComic {
    #[serde(rename = "Info", default)]
    pub info: Option<ComicInfo>,
    #[serde(rename = "Variables", default)]
    variables: Variables,
    #[serde(rename = "Images", default)]
    images: Images,
    #[serde(rename = "Compositing", default)]
    compositing: Option<Compositing>,
}

/// Image placée sur une page composée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub url: String,
    pub left: i32,
    pub top: i32,
}

/// Une page du livre (`WebComicImage`) : une ou plusieurs images.
#[derive(Debug, Clone)]
pub struct WebPage {
    pub links: Vec<Placed>,
    pub compositing: Compositing,
}

impl WebPage {
    fn new(c: &Compositing) -> Self {
        Self { links: Vec::new(), compositing: c.clone() }
    }

    /// Nom affiché : nom de fichier de la première image.
    pub fn name(&self) -> String {
        let u = self.links.first().map(|l| l.url.as_str()).unwrap_or("");
        let u = u.split(['?', '#']).next().unwrap_or(u);
        u.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or(u).to_string()
    }
}

/// Lecture des pages web et des images, avec cache (une ouverture = une lecture).
pub struct Fetcher {
    base_dir: PathBuf,
    texts: HashMap<String, String>,
    bytes: HashMap<String, Vec<u8>>,
}

impl Fetcher {
    pub fn new(base_dir: &Path) -> Self {
        Self { base_dir: base_dir.to_path_buf(), texts: HashMap::new(), bytes: HashMap::new() }
    }

    fn file_path(&self, url: &str) -> Option<PathBuf> {
        if url.len() >= 7 && url[..7].eq_ignore_ascii_case("file://") {
            let rest = url[7..].replace('\\', "/");
            if let Ok(u) = url::Url::parse(&format!("file://{rest}")) {
                if let Ok(p) = u.to_file_path() {
                    return Some(p);
                }
            }
            // `file://C:/…` ou chemin non standard : on retire le préfixe.
            let p = rest.strip_prefix('/').filter(|r| r.as_bytes().get(1) == Some(&b':')).unwrap_or(&rest);
            return Some(PathBuf::from(p));
        }
        if is_web(url) {
            return None;
        }
        Some(self.base_dir.join(url))
    }

    fn download(&self, url: &str) -> Result<Vec<u8>, Error> {
        if let Some(p) = self.file_path(url) {
            return Ok(std::fs::read(p)?);
        }
        let resp = ureq::get(url)
            .header("User-Agent", concat!("ComicRust/", env!("CARGO_PKG_VERSION")))
            .call()
            .map_err(|e| Error::Network(format!("{url} : {e}")))?;
        resp.into_body()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| Error::Network(format!("{url} : {e}")))
    }

    pub fn bytes(&mut self, url: &str) -> Result<Vec<u8>, Error> {
        if let Some(b) = self.bytes.get(url) {
            return Ok(b.clone());
        }
        let b = self.download(url)?;
        self.bytes.insert(url.to_string(), b.clone());
        Ok(b)
    }

    /// Texte d'une page web (UTF-8, sinon Latin-1).
    pub fn text(&mut self, url: &str) -> Result<String, Error> {
        if let Some(t) = self.texts.get(url) {
            return Ok(t.clone());
        }
        let b = self.download(url)?;
        let t = match String::from_utf8(b) {
            Ok(s) => s,
            Err(e) => e.into_bytes().iter().map(|&c| c as char).collect(),
        };
        self.texts.insert(url.to_string(), t.clone());
        Ok(t)
    }
}

fn is_web(url: &str) -> bool {
    let l = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

/// Forme « URL » d'une adresse : chemins `file://` normalisés pour la résolution des liens.
fn parse_url(s: &str) -> Option<url::Url> {
    if s.len() >= 7 && s[..7].eq_ignore_ascii_case("file://") {
        let rest = s[7..].replace('\\', "/");
        let rest = if rest.as_bytes().get(1) == Some(&b':') { format!("/{rest}") } else { rest };
        return url::Url::parse(&format!("file://{rest}")).ok();
    }
    url::Url::parse(s).ok()
}

/// `MakeAbsolute` : lien relatif à la page (et à son `<base href>`).
fn make_absolute(page: &str, relative: &str, base: Option<&str>) -> String {
    let Some(mut uri) = parse_url(page) else { return relative.to_string() };
    if let Some(b) = base.filter(|b| !b.is_empty()) {
        if let Ok(u) = uri.join(b) {
            uri = u;
        }
    }
    uri.join(relative).map(|u| u.to_string()).unwrap_or_else(|_| relative.to_string())
}

fn base_href(text: &str) -> Option<String> {
    static RX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let rx = RX.get_or_init(|| regex::Regex::new(r#"(?is)<base\s+?href\s*?=\s*?"(?<base>.+?)""#).unwrap());
    rx.captures(text).map(|c| c["base"].to_string())
}

/// Expressions des scrapers (syntaxe .NET, insensibles à la casse), mises en cache.
#[derive(Default)]
struct Regexes(HashMap<String, Regex>);

impl Regexes {
    fn get(&mut self, pattern: &str) -> Result<&Regex, Error> {
        if !self.0.contains_key(pattern) {
            let rx = Regex::new(&format!("(?i){pattern}"))
                .map_err(|e| Error::Decode(format!("WebComic : expression « {pattern} » invalide : {e}")))?;
            self.0.insert(pattern.to_string(), rx);
        }
        Ok(&self.0[pattern])
    }

    /// `GetValue` : groupe `link` s'il existe, sinon toute la correspondance, nettoyée.
    fn value(c: &fancy_regex::Captures) -> String {
        let v = c.name("link").map(|m| m.as_str()).filter(|s| !s.is_empty()).or_else(|| c.get(0).map(|m| m.as_str()));
        v.unwrap_or("").replace(['"', '\'', '\r', '\n'], "").replace("&amp;", "&")
    }

    /// `MatchesRegex`.
    fn matches(&mut self, text: &str, part: &Part) -> Result<Vec<String>, Error> {
        let mut text = text.to_string();
        if let Some(cut) = part.cut.as_deref().filter(|c| !c.is_empty()) {
            match self.get(cut)?.captures(&text).ok().flatten() {
                Some(c) => text = Self::value(&c),
                None => return Ok(Vec::new()),
            }
        }
        let rx = self.get(&part.pattern)?;
        Ok(rx
            .captures_iter(&text)
            .filter_map(Result::ok)
            .take(part.maximum_matches.unwrap_or(usize::MAX))
            .map(|c| Self::value(&c))
            .collect())
    }
}

/// Insensible à la casse, comme `StringComparer.OrdinalIgnoreCase`.
#[derive(Default)]
struct Trackback(HashSet<String>);

impl Trackback {
    fn contains(&self, s: &str) -> bool {
        self.0.contains(&s.to_lowercase())
    }
    fn insert(&mut self, s: &str) -> bool {
        self.0.insert(s.to_lowercase())
    }
    fn remove(&mut self, s: &str) {
        self.0.remove(&s.to_lowercase());
    }
}

struct Scraper<'a> {
    fetch: &'a mut Fetcher,
    rx: Regexes,
}

impl Scraper<'_> {
    /// `GetIndexScraperPages`.
    fn index_pages(
        &mut self,
        url: &str,
        text: &str,
        parts: &[Part],
        trackback: &mut Trackback,
        out: &mut Vec<String>,
    ) -> Result<(), Error> {
        if text.is_empty() {
            return Ok(());
        }
        let Some(first) = parts.first() else {
            return Err(Error::Decode("WebComic : il manque le motif des images".into()));
        };
        let mut part = first.clone();
        part.reverse |= part.take_option('!');
        part.add_own |= part.take_option('+');
        let end = parts.len() == 1;
        let add_own = part.add_own && parts.len() > 1;
        let base = base_href(text);
        let mut matches: Vec<String> = Vec::new();
        for m in self.rx.matches(text, &part)? {
            let abs = make_absolute(url, &m, base.as_deref());
            if !trackback.contains(&abs) && !matches.contains(&abs) {
                matches.push(abs);
            }
        }
        let own = add_own.then(|| url.to_string());
        if let Some(own) = &own {
            matches.retain(|m| m != own);
            matches.insert(0, own.clone());
        }
        if part.sort {
            matches.sort_by_key(|u| u.to_lowercase());
        }
        if part.reverse {
            matches.reverse();
        }
        for u in matches {
            // Liens déjà vus pendant la visite (ComicRack filtre à la volée).
            if !trackback.insert(&u) && own.as_deref() != Some(u.as_str()) {
                continue;
            }
            if end {
                out.push(u);
                continue;
            }
            match self.fetch.text(&u) {
                Ok(t) => self.index_pages(&u, &t, &parts[1..], trackback, out)?,
                Err(_) => continue, // page de l'index inaccessible : ses images sont sautées
            }
        }
        Ok(())
    }

    /// `GetBrowseScraperPages` : pages successives par le lien « suivant ».
    fn browse_pages(&mut self, parts: &[Part], trackback: &mut Trackback, out: &mut Vec<String>) -> Result<(), Error> {
        if parts.len() < 3 {
            return Err(Error::Decode(
                "WebComic : un scraper de navigation demande la première page, le motif des images et celui du \
                 lien « suivant »"
                    .into(),
            ));
        }
        let next_link = &parts[parts.len() - 1];
        let image_links = &parts[1..parts.len() - 1];
        let mut current = parts[0].pattern.clone();
        for _ in 0..MAX_BROWSE_PAGES {
            let page = current.clone();
            if trackback.contains(&page) {
                break;
            }
            let Ok(text) = self.fetch.text(&page) else { break };
            let next = self.rx.matches(&text, next_link)?.into_iter().next();
            trackback.insert(&page);
            match next {
                Some(n) => {
                    current = make_absolute(&page, &n, base_href(&text).as_deref());
                    self.index_pages(&page, &text, image_links, trackback, out)?;
                }
                None => {
                    trackback.remove(&page);
                    self.index_pages(&page, &text, image_links, trackback, out)?;
                    break;
                }
            }
        }
        Ok(())
    }
}

impl WebComic {
    pub fn from_xml(xml: &str) -> Result<Self, Error> {
        Ok(quick_xml::de::from_str(xml.trim_start_matches('\u{feff}'))?)
    }

    fn variables(&self, cbw_path: &Path) -> Vec<(String, String)> {
        let dir = cbw_path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut vars: Vec<(String, String)> =
            self.variables.variable.iter().map(|v| (v.key.clone(), v.value.clone())).collect();
        vars.push((
            "ComicFileName".into(),
            cbw_path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
        ));
        vars.push(("ComicFilePath".into(), format!("file://{}", dir.display())));
        vars
    }

    /// `GetImages` : URL des images d'une entrée `<Image>`.
    fn image_urls(
        link: &ImageLink,
        vars: &[(String, String)],
        fetch: &mut Fetcher,
        trackback: &mut Trackback,
    ) -> Result<Vec<String>, Error> {
        let mut parts: Vec<Part> = link.url.split('|').map(Part::new).collect();
        if let Some(p) = &link.parts {
            parts.extend(p.part.iter().cloned());
        }
        // `<Image><Parts>…` sans attribut Url : la première partie est l'adresse.
        if parts.len() > 1 && parts[0].pattern.trim().is_empty() {
            parts.remove(0);
        }
        for part in &mut parts {
            for (k, v) in vars {
                part.pattern = part.pattern.replace(&format!("{{{k}}}"), v);
            }
        }
        let mut kind = link.kind.clone().unwrap_or_default();
        if parts[0].take_option('!') {
            kind = "IndexScraper".into();
        } else if parts[0].take_option('?') {
            kind = "BrowseScraper".into();
        }
        let mut scraper = Scraper { fetch, rx: Regexes::default() };
        let mut out = Vec::new();
        match kind.as_str() {
            "BrowseScraper" => scraper.browse_pages(&parts, trackback, &mut out)?,
            "IndexScraper" => {
                let start = parts[0].pattern.trim().to_string();
                let text = scraper.fetch.text(&start)?;
                scraper.index_pages(&start, &text, &parts[1..], trackback, &mut out)?;
            }
            _ => out.extend(expand_numbering(parts[0].pattern.trim())),
        }
        Ok(out)
    }

    /// `GetParsedImages` : pages du livre, scrapers parcourus et images composées.
    pub fn pages(&self, cbw_path: &Path, fetch: &mut Fetcher) -> Result<Vec<WebPage>, Error> {
        let vars = self.variables(cbw_path);
        let mut comp = self.compositing.clone().unwrap_or_default();
        let mut pages = Vec::new();
        let mut cur = WebPage::new(&comp);
        let (mut x, mut y, mut row_h) = (0i32, 0i32, 0i32);
        let mut trackback = Trackback::default();
        for link in &self.images.image {
            if let Some(c) = &link.compositing {
                if !cur.links.is_empty() {
                    pages.push(std::mem::replace(&mut cur, WebPage::new(c)));
                }
                comp = c.clone();
                cur = WebPage::new(&comp);
                (x, y, row_h) = (0, 0, 0);
            }
            for url in Self::image_urls(link, &vars, fetch, &mut trackback)? {
                if comp.page_size_empty() {
                    let per_page = comp.rows.max(1) * comp.columns.max(1);
                    cur.links.push(Placed { url, left: 0, top: 0 });
                    if cur.links.len() as i32 >= per_page {
                        pages.push(std::mem::replace(&mut cur, WebPage::new(&comp)));
                    }
                    continue;
                }
                // Page de taille fixe : les images sont rangées de gauche à droite puis de
                // haut en bas ; il faut donc connaître leur taille.
                let Some((w, h)) = fetch.bytes(&url).ok().and_then(|b| image_size(&b)) else { continue };
                let grow = |p: &mut WebPage| {
                    p.compositing.page_width = p.compositing.page_width.max(w);
                    p.compositing.page_height = p.compositing.page_height.max(h);
                };
                grow(&mut cur);
                while x + w > cur.compositing.page_width {
                    // Rangée suivante si l'image y tient en entier (ComicRack ne vérifie que
                    // le haut de la rangée, et l'image peut alors déborder de la page).
                    if y + row_h + h <= cur.compositing.page_height {
                        y += row_h;
                        x = 0;
                        row_h = 0;
                    } else {
                        if !cur.links.is_empty() {
                            pages.push(std::mem::replace(&mut cur, WebPage::new(&comp)));
                        }
                        grow(&mut cur);
                        (x, y, row_h) = (0, 0, 0);
                    }
                }
                cur.links.push(Placed { url, left: x, top: y });
                x += w;
                row_h = row_h.max(h);
            }
        }
        if !cur.links.is_empty() {
            pages.push(cur);
        }
        Ok(pages)
    }
}

/// Plage numérotée `[format:début-fin]` (le format .NET « 000 » donne trois chiffres).
fn expand_numbering(url: &str) -> Vec<String> {
    static RX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let rx = RX.get_or_init(|| regex::Regex::new(r"\[(?<format>\d+):(?<start>\d+)-(?<end>\d+)\]").unwrap());
    match rx.captures(url) {
        None => vec![url.to_string()],
        Some(c) => {
            let pad = c["format"].chars().filter(|&ch| ch == '0').count().max(1);
            let (Ok(start), Ok(end)) = (c["start"].parse::<u64>(), c["end"].parse::<u64>()) else {
                return vec![url.to_string()];
            };
            (start..=end).map(|i| rx.replace(url, format!("{i:0pad$}").as_str()).into_owned()).collect()
        }
    }
}

/// Taille d'une image sans la décoder entièrement (sinon décodage complet).
fn image_size(bytes: &[u8]) -> Option<(i32, i32)> {
    let dims = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .or_else(|| decode_image(bytes).ok().map(|i| i.dimensions()))?;
    Some((dims.0 as i32, dims.1 as i32))
}

/// `OnRetrieveSourceByteImage` : assemble les images d'une page composée.
fn compose(page: &WebPage, images: &[(Placed, PageImage)]) -> PageImage {
    let c = &page.compositing;
    let bg = c.back_color();
    let border = |w: i64, h: i64| ((((w * w + h * h) as f64).sqrt()) * c.border_width as f64 / 100.0) as i64;
    let draw = |canvas: &mut PageImage, img: &PageImage, x: i64, y: i64| {
        image::imageops::overlay(canvas, img, x, y);
    };
    if c.page_size_empty() {
        let cols = c.columns.max(1) as usize;
        let (mut width, mut height) = (0i64, 0i64);
        for row in images.chunks(cols) {
            width = width.max(row.iter().map(|(_, i)| i.width() as i64).sum());
            height += row.iter().map(|(_, i)| i.height() as i64).max().unwrap_or(0);
        }
        let b = border(width, height);
        let mut canvas = PageImage::from_pixel((width + 2 * b).max(1) as u32, (height + 2 * b).max(1) as u32, bg);
        let mut ty = b;
        for row in images.chunks(cols) {
            let mut tx = b;
            for (_, img) in row {
                // Miroir dans la page bordure comprise (ComicRack décale de deux bordures).
                let x = if c.right_to_left { width + 2 * b - tx - img.width() as i64 } else { tx };
                draw(&mut canvas, img, x, ty);
                tx += img.width() as i64;
            }
            ty += row.iter().map(|(_, i)| i.height() as i64).max().unwrap_or(0);
        }
        canvas
    } else {
        let (pw, ph) = (c.page_width as i64, c.page_height as i64);
        let b = border(pw, ph);
        let (cw, ch) = (pw + 2 * b, ph + 2 * b);
        let mut canvas = PageImage::from_pixel(cw.max(1) as u32, ch.max(1) as u32, bg);
        let (mut r0, mut r1) = ((i64::MAX, i64::MAX), (i64::MIN, i64::MIN));
        for (p, img) in images {
            r0 = (r0.0.min(p.left as i64), r0.1.min(p.top as i64));
            r1 = (r1.0.max(p.left as i64 + img.width() as i64), r1.1.max(p.top as i64 + img.height() as i64));
        }
        let (ox, oy) = ((cw - (r1.0 - r0.0)) / 2, (ch - (r1.1 - r0.1)) / 2);
        for (p, img) in images {
            let x = if c.right_to_left { cw - ox - p.left as i64 - img.width() as i64 } else { ox + p.left as i64 };
            draw(&mut canvas, img, x, oy + p.top as i64);
        }
        canvas
    }
}

pub struct WebComicProvider {
    info: Option<ComicInfo>,
    pages: Vec<WebPage>,
    names: Vec<String>,
    fetch: Fetcher,
}

impl WebComicProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let wc = WebComic::from_xml(&std::fs::read_to_string(path)?)?;
        let mut fetch = Fetcher::new(path.parent().unwrap_or(Path::new(".")));
        let pages = wc.pages(path, &mut fetch)?;
        let names = pages.iter().map(WebPage::name).collect();
        Ok(Self { info: wc.info, pages, names, fetch })
    }

    fn composed(&mut self, index: usize) -> Result<PageImage, Error> {
        let page = self.pages.get(index).ok_or(Error::PageOutOfRange(index))?.clone();
        let mut images = Vec::new();
        let mut last_err = None;
        for l in &page.links {
            match self.fetch.bytes(&l.url).and_then(|b| decode_image(&b)) {
                Ok(img) => images.push((l.clone(), img)),
                Err(e) => last_err = Some(e),
            }
        }
        if images.is_empty() {
            return Err(last_err.unwrap_or(Error::PageOutOfRange(index)));
        }
        Ok(compose(&page, &images))
    }
}

impl PageProvider for WebComicProvider {
    fn format(&self) -> BookFormat {
        BookFormat::WebComic
    }
    fn pages(&self) -> &[String] {
        &self.names
    }
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        let page = self.pages.get(index).ok_or(Error::PageOutOfRange(index))?;
        if page.compositing.is_empty() {
            let url = page.links[0].url.clone();
            return self.fetch.bytes(&url);
        }
        encode_png(&self.composed(index)?)
    }
    fn render_page(&mut self, index: usize) -> Result<PageImage, Error> {
        let page = self.pages.get(index).ok_or(Error::PageOutOfRange(index))?;
        if page.compositing.is_empty() {
            return decode_image(&self.read_page(index)?);
        }
        self.composed(index)
    }
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        Ok(self.info.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::testutil::png;

    fn colored(w: u32, h: u32, n: u8) -> Vec<u8> {
        encode_png(&PageImage::from_pixel(w, h, image::Rgba([n, 0, 0, 255]))).unwrap()
    }

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

    /// Petit « site » local : index → chapitres → images, et pages reliées par « suivant ».
    fn site(dir: &Path) {
        let w = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
        w(
            "index.html",
            r#"<html><head><base href="chap/"></head><body>
<ul class="chapitres"><li><a href="c1.html">Un</a></li><li><a href='c2.html'>Deux</a></li></ul>
<a href="c1.html">doublon</a><a href="pub.html">pub</a></body></html>"#,
        );
        std::fs::create_dir(dir.join("chap")).unwrap();
        w("chap/c1.html", r#"<img class="planche" src="../img/a1.png"><img class="planche" src="../img/a2.png">"#);
        w("chap/c2.html", r#"<IMG CLASS="planche" SRC="../img/b1.png?x=1&amp;y=2">"#);
        w("strip1.html", r#"<img id="strip" src="img/a1.png"><a rel="next" href="strip2.html">›</a>"#);
        w("strip2.html", r#"<img id="strip" src="img/a2.png"><a rel="next" href="strip3.html">›</a>"#);
        w("strip3.html", r#"<img id="strip" src="img/b1.png"><a rel="prev" href="strip2.html">‹</a>"#);
        std::fs::create_dir(dir.join("img")).unwrap();
        std::fs::write(dir.join("img/a1.png"), colored(4, 6, 10)).unwrap();
        std::fs::write(dir.join("img/a2.png"), colored(4, 6, 20)).unwrap();
        std::fs::write(dir.join("img/b1.png"), colored(4, 6, 30)).unwrap();
    }

    fn open(dir: &Path, images: &str) -> WebComicProvider {
        let path = dir.join("site.cbw");
        std::fs::write(&path, format!("<WebComic><Images>{images}</Images></WebComic>")).unwrap();
        WebComicProvider::open(&path).unwrap()
    }

    #[test]
    fn index_scraper() {
        let dir = tempfile::tempdir().unwrap();
        site(dir.path());
        // Chapitres de l'index (zone « Cut », base href), puis planches de chaque chapitre.
        let book = open(
            dir.path(),
            r#"<Image Url="!{ComicFilePath}/index.html|href=(?&lt;link&gt;.+?\.html)|class=.planche. src=(?&lt;link&gt;\S+)&gt;">
                 <Parts/></Image>"#,
        );
        // « pub.html » n'existe pas : sautée.
        assert_eq!(book.pages(), ["a1.png", "a2.png", "b1.png"]);
        let dir2 = tempfile::tempdir().unwrap();
        site(dir2.path());
        let mut book = open(
            dir2.path(),
            r#"<Image Type="IndexScraper"><Parts>
                 <Part>{ComicFilePath}/index.html</Part>
                 <Part Cut="(?s)&lt;ul class=.chapitres.&gt;(?&lt;link&gt;.*?)&lt;/ul&gt;" Reverse="true">href=(?&lt;link&gt;.+?\.html)</Part>
                 <Part MaximumMatches="1">class=.planche. src=(?&lt;link&gt;\S+)&gt;</Part>
               </Parts></Image>"#,
        );
        // Chapitres inversés (c2 puis c1), une planche par chapitre.
        assert_eq!(book.pages(), ["b1.png", "a1.png"]);
        assert_eq!(book.render_page(1).unwrap().get_pixel(0, 0).0, [10, 0, 0, 255]);
    }

    #[test]
    fn browse_scraper() {
        let dir = tempfile::tempdir().unwrap();
        site(dir.path());
        let mut book = open(
            dir.path(),
            r#"<Image Url="?{ComicFilePath}/strip1.html|id=.strip. src=.(?&lt;link&gt;[^&quot;]+)|rel=.next. href=.(?&lt;link&gt;[^&quot;]+)"/>"#,
        );
        assert_eq!(book.pages(), ["a1.png", "a2.png", "b1.png"]);
        assert_eq!(book.render_page(2).unwrap().get_pixel(1, 1).0, [30, 0, 0, 255]);
    }

    #[test]
    fn compositing_rows_columns_and_page_size() {
        let dir = tempfile::tempdir().unwrap();
        site(dir.path());
        // Deux images côte à côte par page, bordure, fond noir, de droite à gauche.
        let mut book = open(
            dir.path(),
            r##"<Image Url="img/a1.png"><Compositing Columns="2" BackgroundColor="#000000" RightToLeft="true"/></Image>
                <Image Url="img/a2.png"/><Image Url="img/b1.png"/>"##,
        );
        assert_eq!(book.pages(), ["a1.png", "b1.png"]);
        let p = book.render_page(0).unwrap();
        assert_eq!(p.dimensions(), (8, 6));
        assert_eq!(p.get_pixel(0, 0).0, [20, 0, 0, 255], "droite à gauche : a2 à gauche");
        assert_eq!(p.get_pixel(7, 0).0, [10, 0, 0, 255]);
        assert!(!book.read_page(0).unwrap().is_empty());
        let last = book.render_page(1).unwrap();
        assert_eq!(last.dimensions(), (4, 6), "dernière page incomplète");

        // Avec une bordure, de droite à gauche : bordure égale des deux côtés.
        let mut book = open(
            dir.path(),
            r#"<Image Url="img/a1.png"><Compositing Columns="2" BorderWidth="20" RightToLeft="true"/></Image>
               <Image Url="img/a2.png"/>"#,
        );
        let p = book.render_page(0).unwrap();
        // Bordure : 20 % de la diagonale de 8 × 6 (10) → 2 pixels.
        assert_eq!(p.dimensions(), (12, 10));
        assert_eq!(p.get_pixel(1, 2).0, [255, 255, 255, 255]);
        assert_eq!(p.get_pixel(2, 2).0, [20, 0, 0, 255], "a2 à gauche, après la bordure");
        assert_eq!(p.get_pixel(9, 2).0, [10, 0, 0, 255]);
        assert_eq!(p.get_pixel(10, 2).0, [255, 255, 255, 255]);

        // Page de taille fixe 8 × 12 : deux images par rangée, deux rangées.
        let mut book = open(
            dir.path(),
            r#"<Image Url="img/a1.png"><Compositing PageWidth="8" PageHeight="12" BorderWidth="10"/></Image>
               <Image Url="img/a2.png"/><Image Url="img/b1.png"/><Image Url="img/a1.png"/><Image Url="img/a2.png"/>"#,
        );
        assert_eq!(book.pages().len(), 2);
        let p = book.render_page(0).unwrap();
        // Bordure : 10 % de la diagonale (≈ 14,4) → 1 pixel de chaque côté.
        assert_eq!(p.dimensions(), (10, 14));
        assert_eq!(p.get_pixel(0, 0).0, [255, 255, 255, 255], "fond blanc par défaut");
        assert_eq!(p.get_pixel(1, 1).0, [10, 0, 0, 255]);
        assert_eq!(p.get_pixel(5, 1).0, [20, 0, 0, 255]);
        assert_eq!(p.get_pixel(1, 7).0, [30, 0, 0, 255]);
    }

    #[test]
    fn helpers() {
        assert_eq!(make_absolute("https://ex.org/bd/index.html", "../img/1.png", None), "https://ex.org/img/1.png");
        assert_eq!(make_absolute("https://ex.org/a/b.html", "c.html", Some("/x/")), "https://ex.org/x/c.html");
        assert_eq!(expand_numbering("p[00:8-10].jpg"), ["p08.jpg", "p09.jpg", "p10.jpg"]);
        let c = Compositing { background_color: Some("#f0a".into()), ..Default::default() };
        assert_eq!(c.back_color().0, [255, 0, 170, 255]);
        assert!(Compositing::default().is_empty());
        assert!(!Compositing { columns: 2, ..Default::default() }.is_empty());
    }
}
