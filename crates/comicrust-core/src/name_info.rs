//! Valeurs « proposées » d'après le nom du fichier, portage de `ComicNameInfo.NewParser`
//! (ComicRack CE). Quand la fiche d'un livre laisse vide la série, le numéro, le volume,
//! l'année ou le format, ComicRack utilise ces valeurs (`ShadowSeries`, `ShadowNumber`…)
//! pour les tris, les statistiques de série, les doublons et les listes intelligentes.
//!
//! Les expressions régulières sont celles de ComicRack (insensibles à la casse). Certaines
//! sont évaluées « de droite à gauche » (`RegexOptions.RightToLeft`) : on garde alors la
//! correspondance qui finit le plus à droite, étendue au maximum vers la gauche pour les
//! chiffres (voir [`rightmost`]).

use std::sync::OnceLock;

use fancy_regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameInfo {
    pub series: String,
    pub number: String,
    pub format: String,
    pub volume: i32,
    pub count: i32,
    pub year: i32,
}

impl Default for NameInfo {
    fn default() -> Self {
        Self { series: String::new(), number: String::new(), format: String::new(), volume: -1, count: -1, year: -1 }
    }
}

fn rx(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(&format!("(?is){pattern}")).expect("expression de ComicRack"))
}

macro_rules! regex {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static CELL: OnceLock<Regex> = OnceLock::new();
            rx(&CELL, $pat)
        }
    };
}

regex!(rx_dot_replace, r"((?<!\d)\.|\.(?!\d)|_)");
regex!(rx_remove, r"\b(ctc|c2c|\d+p|\d{1,2}\-\d{1,2}|\d{1,2}\-(?=\d{4}))\b");
regex!(rx_brackets, r"\(.*?\)|\[.*?\]");
regex!(rx_count, r"\b(of|von|de)\s*\d+\b");
// `(?<!part\s+)` (longueur variable, non accepté ici) est vérifié à part : `after_part`.
regex!(rx_number, r"(\b|#|(c\w*\s*))\d[\d\.]*\b(?!\s*(pa|cov))");
regex!(rx_volume, r"\b(v|vol\.?|volume)\s*\d+\b");
regex!(rx_year, r"\b(?<!#)(19|2[0-3])\d\d\b(?!\spa)");
regex!(rx_year_with_month, r"(19|2[0-3])\d\d[-/\\\s]\d{1,2}\b");
regex!(rx_format, r"\b(annual|director's cut|preview|b(lack)?\s*&\s*w(hite)?|king\s*size|giant\s*size)|sketch\b");
regex!(rx_num, r"\d+\.?\d*");

/// Première correspondance (de gauche à droite) : (début, fin) en octets.
fn leftmost(re: &Regex, text: &str) -> Option<(usize, usize)> {
    re.find(text).ok().flatten().map(|m| (m.start(), m.end()))
}

/// Équivalent de `RegexOptions.RightToLeft` : parmi toutes les correspondances (une par
/// position de départ), celle qui finit le plus à droite ; à égalité, celle dont la partie
/// numérique finale est la plus longue, puis la plus courte (le moteur de .NET étend les
/// chiffres vers la gauche avant d'essayer les préfixes `#` ou `c…`).
fn rightmost(re: &Regex, text: &str) -> Option<(usize, usize)> {
    rightmost_where(re, text, |_| true)
}

/// Le texte avant `start` finit-il par « part » suivi d'espaces ?
fn after_part(text: &str, start: usize) -> bool {
    let before = &text[..start];
    let trimmed = before.trim_end();
    trimmed.len() < before.len() && trimmed.to_lowercase().ends_with("part")
}

fn rightmost_where(re: &Regex, text: &str, ok: impl Fn(usize) -> bool) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let digits = |s: &str| s.chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').count();
    // Les expressions évaluées de droite à gauche commencent toutes par un chiffre, « # »
    // ou « c » (éventuellement précédé d'une limite de mot) : inutile d'essayer ailleurs.
    let starts = text.char_indices().filter(|(_, c)| c.is_ascii_digit() || matches!(c, '#' | 'c' | 'C'));
    for (pos, _) in starts {
        let Ok(Some(m)) = re.find_from_pos(text, pos) else { continue };
        if m.start() != pos {
            continue; // trouvée plus loin : elle sera essayée depuis sa propre position
        }
        if !ok(m.start()) {
            continue;
        }
        let cand = (m.start(), m.end());
        best = Some(match best {
            None => cand,
            Some(b) => {
                let key = |(s, e): (usize, usize)| (e, digits(&text[s..e]), usize::MAX - (e - s));
                if key(cand) > key(b) {
                    cand
                } else {
                    b
                }
            }
        });
    }
    best
}

fn remove(text: &mut String, (s, e): (usize, usize)) {
    text.replace_range(s..e, "");
}

/// `GetNumber` : le nombre le plus à droite du texte.
fn get_number(text: &str) -> String {
    rightmost(rx_num(), text).map(|(s, e)| text[s..e].to_string()).unwrap_or_default()
}

fn int(s: &str) -> i32 {
    s.trim().parse().unwrap_or(-1)
}

/// `Path.GetFileNameWithoutExtension` et `Path.GetFileName(Path.GetDirectoryName(..))`,
/// quel que soit le séparateur.
fn split_path(path: &str) -> (String, String) {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = match name.rfind('.') {
        Some(i) => &name[..i],
        None => name,
    };
    let dir = path.len().checked_sub(name.len() + 1).map(|n| &path[..n]).unwrap_or("");
    let parent = dir.rsplit(['/', '\\']).next().unwrap_or(dir);
    (stem.to_string(), parent.to_string())
}

/// Valeurs proposées d'après le chemin du fichier (même algorithme que ComicRack).
pub fn from_file_path(path: &str) -> NameInfo {
    let mut info = NameInfo::default();
    let (stem, parent) = split_path(path);
    let mut text = stem;
    if !text.contains(' ') || text.contains('_') {
        text = rx_dot_replace().replace_all(&text, " ").into_owned();
    }
    text = rx_remove().replace_all(&text, "").into_owned();
    if let Some(m) = leftmost(rx_count(), &text) {
        info.count = int(&get_number(&text[m.0..m.1]));
        remove(&mut text, m);
    }
    if let Some(m) = leftmost(rx_volume(), &text) {
        info.volume = int(&get_number(&text[m.0..m.1]));
        remove(&mut text, m);
    }
    if let Some(m) = rightmost(rx_year_with_month(), &text).or_else(|| rightmost(rx_year(), &text)) {
        let found = &text[m.0..m.1];
        info.year = int(&get_number(&found[..4.min(found.len())]));
        remove(&mut text, m);
    }
    if let Some(m) = leftmost(rx_format(), &text) {
        info.format = text[m.0..m.1].to_string();
        remove(&mut text, m);
    }
    let number_match = {
        let t = text.clone();
        rightmost_where(rx_number(), &text, move |start| !after_part(&t, start))
    };
    let found = number_match.map(|(s, e)| text[s..e].to_string()).unwrap_or_default();
    if let Some(m) = number_match {
        remove(&mut text, m);
    }
    info.number = get_number(&found);
    if !info.number.is_empty() && info.number.chars().all(|c| c.is_ascii_digit() || c == '.') {
        if let Ok(f) = info.number.parse::<f32>() {
            info.number = format!("{f}");
        }
    }
    let cut = text.find(['(', '[', '#', ',']).map(|i| &text[..i]).unwrap_or(&text).trim();
    info.series = cut.trim_matches([' ', '-', '.']).to_string();
    if info.series.is_empty() || info.series.chars().all(char::is_numeric) {
        info.series = rx_brackets().replace_all(&parent, "").into_owned();
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comicrack_file_names() {
        let i = from_file_path(r"Z:\BD\Thanos Quest\Thanos Quest 01 (1990).cbz");
        assert_eq!((i.series.as_str(), i.number.as_str(), i.year, i.volume), ("Thanos Quest", "1", 1990, -1));

        let i = from_file_path(r"Z:\BD\Iron Man v2 055 (1998) (Digital).cbr");
        assert_eq!((i.series.as_str(), i.number.as_str(), i.year, i.volume), ("Iron Man", "55", 1998, 2));

        let i = from_file_path(r"X:\Avengers\Avengers_012_(of_40).cbz");
        assert_eq!((i.series.as_str(), i.number.as_str(), i.count), ("Avengers", "12", 40));

        let i = from_file_path(r"X:\Spawn\Spawn Annual 3.cbz");
        assert_eq!((i.series.as_str(), i.number.as_str(), i.format.as_str()), ("Spawn", "3", "Annual"));

        // Numéro avec décimales, nom sans espaces.
        let i = from_file_path(r"X:\Batman\Batman.12.5.cbz");
        assert_eq!((i.series.as_str(), i.number.as_str()), ("Batman", "12.5"));

        // Nom réduit à un nombre : la série vient du dossier.
        let i = from_file_path(r"X:\Blacksad [Dargaud]\03.cbz");
        assert_eq!((i.series.as_str(), i.number.as_str()), ("Blacksad ", "3"));

        // Nom de ta collection : numéro après « # », année avec mois.
        let i = from_file_path(r"Z:\04\COSMIC COLLECTION #224 2012_05 - Thanos Rising 1-5 (Jason Aaron)(2013).cbz");
        assert_eq!(i.series, "COSMIC COLLECTION");
    }

    #[test]
    #[ignore]
    fn speed() {
        let t = std::time::Instant::now();
        for i in 0..5400 {
            let p = format!(
                r"\\NAS\_BD\Séries Terminées\Série {}\COSMIC COLLECTION #{:03} 2012_05 - Thanos Rising 1-5 (Jason Aaron)(2013).cbz",
                i / 12,
                i
            );
            std::hint::black_box(from_file_path(&p));
        }
        eprintln!("5400 noms en {:?}", t.elapsed());
    }
}
