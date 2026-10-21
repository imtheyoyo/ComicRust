//! Recherche, tri et regroupement des livres de la bibliothèque.

use std::cmp::Ordering;

use comicrust_core::library::LibraryBook;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortKey {
    /// Série, volume, numéro (ordre naturel), titre.
    #[default]
    Series,
    Title,
    /// Date de publication (année, mois, jour).
    Published,
    Added,
    /// Dernière lecture.
    Opened,
    Rating,
    PageCount,
    ReadPercent,
    FileSize,
    File,
    /// Ordre de la liste ComicRack affichée (liste de lecture).
    Position,
}

impl SortKey {
    pub const ALL: [SortKey; 11] = [
        SortKey::Series,
        SortKey::Title,
        SortKey::Published,
        SortKey::Added,
        SortKey::Opened,
        SortKey::Rating,
        SortKey::PageCount,
        SortKey::ReadPercent,
        SortKey::FileSize,
        SortKey::File,
        SortKey::Position,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Series => "Série",
            SortKey::Title => "Titre",
            SortKey::Published => "Date de parution",
            SortKey::Added => "Date d'ajout",
            SortKey::Opened => "Dernière lecture",
            SortKey::Rating => "Note",
            SortKey::PageCount => "Nombre de pages",
            SortKey::ReadPercent => "Avancement de lecture",
            SortKey::FileSize => "Taille du fichier",
            SortKey::File => "Chemin du fichier",
            SortKey::Position => "Ordre de la liste",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroupKey {
    None,
    #[default]
    Series,
    Publisher,
    Writer,
    Year,
    StoryArc,
    Genre,
    Format,
    ReadState,
}

impl GroupKey {
    pub const ALL: [GroupKey; 9] = [
        GroupKey::None,
        GroupKey::Series,
        GroupKey::Publisher,
        GroupKey::Writer,
        GroupKey::Year,
        GroupKey::StoryArc,
        GroupKey::Genre,
        GroupKey::Format,
        GroupKey::ReadState,
    ];

    pub fn label_of_key(self) -> &'static str {
        match self {
            GroupKey::None => "Aucun",
            GroupKey::Series => "Série",
            GroupKey::Publisher => "Éditeur",
            GroupKey::Writer => "Scénariste",
            GroupKey::Year => "Année",
            GroupKey::StoryArc => "Arc",
            GroupKey::Genre => "Genre",
            GroupKey::Format => "Format",
            GroupKey::ReadState => "État de lecture",
        }
    }

    /// Titre du groupe d'un livre.
    pub fn label(self, b: &LibraryBook) -> String {
        let text = |v: &Option<String>, empty: &str| {
            v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| empty.into())
        };
        match self {
            GroupKey::None => String::new(),
            GroupKey::Series => text(&b.series, "(sans série)"),
            GroupKey::Publisher => text(&b.publisher, "(sans éditeur)"),
            GroupKey::Writer => text(&b.writer, "(sans scénariste)"),
            GroupKey::Year => b.year.filter(|y| *y > 0).map(|y| y.to_string()).unwrap_or_else(|| "(sans année)".into()),
            GroupKey::StoryArc => text(&b.story_arc, "(sans arc)"),
            GroupKey::Genre => text(&b.genre, "(sans genre)"),
            GroupKey::Format => text(&b.format, "(sans format)"),
            GroupKey::ReadState => match b.read_percent() {
                None | Some(0) => "Non lu".into(),
                Some(100) => "Lu".into(),
                Some(_) => "En cours".into(),
            },
        }
    }
}

/// Recherche : chaque mot doit apparaître dans un des champs principaux.
pub fn matches(b: &LibraryBook, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let fields = [
        &b.series,
        &b.title,
        &b.writer,
        &b.penciller,
        &b.story_arc,
        &b.publisher,
        &b.characters,
        &b.teams,
        &b.tags,
        &b.genre,
        &b.alternate_series,
        &b.file,
    ];
    let mut hay = b.caption();
    for f in fields.into_iter().flatten() {
        hay.push(' ');
        hay.push_str(f);
    }
    if let Some(y) = b.year {
        hay.push_str(&format!(" {y}"));
    }
    let hay = hay.to_lowercase();
    words.iter().all(|w| hay.contains(w))
}

fn lower(v: &Option<String>) -> String {
    v.as_deref().unwrap_or("").to_lowercase()
}

/// Valeurs vides en dernier, quel que soit le sens.
fn opt_cmp<T: PartialOrd>(a: Option<T>, b: Option<T>, desc: bool) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => {
            let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
            if desc {
                o.reverse()
            } else {
                o
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn series_cmp(a: &LibraryBook, b: &LibraryBook) -> Ordering {
    lower(&a.series)
        .cmp(&lower(&b.series))
        .then(a.volume.unwrap_or(-1).cmp(&b.volume.unwrap_or(-1)))
        .then_with(|| natord::compare(a.number.as_deref().unwrap_or(""), b.number.as_deref().unwrap_or("")))
        .then_with(|| lower(&a.title).cmp(&lower(&b.title)))
}

/// Dates ComicRack (`2019-10-01T12:58:00…`) : l'ordre du texte ISO est l'ordre chronologique.
fn date(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty() && !s.starts_with("0001-01-01"))
}

fn rev(o: Ordering, desc: bool) -> Ordering {
    if desc {
        o.reverse()
    } else {
        o
    }
}

fn key_cmp(key: SortKey, a: &LibraryBook, b: &LibraryBook, desc: bool) -> Ordering {
    let published = |x: &LibraryBook| x.year.filter(|y| *y > 0).map(|y| (y, x.month.unwrap_or(0), x.day.unwrap_or(0)));
    match key {
        SortKey::Series | SortKey::Position => rev(series_cmp(a, b), desc),
        SortKey::Title => rev(lower(&a.title).cmp(&lower(&b.title)), desc),
        SortKey::Published => opt_cmp(published(a), published(b), desc),
        SortKey::Added => opt_cmp(date(&a.added), date(&b.added), desc),
        SortKey::Opened => opt_cmp(date(&a.opened), date(&b.opened), desc),
        SortKey::Rating => opt_cmp(a.rating.filter(|r| *r > 0.0), b.rating.filter(|r| *r > 0.0), desc),
        SortKey::PageCount => opt_cmp(a.page_count, b.page_count, desc),
        SortKey::ReadPercent => opt_cmp(a.read_percent(), b.read_percent(), desc),
        SortKey::FileSize => opt_cmp(a.file_size, b.file_size, desc),
        SortKey::File => rev(lower(&a.file).cmp(&lower(&b.file)), desc),
    }
}

/// Trie `v` (indices dans `books`) : par groupe, puis par la clé choisie, puis par série.
/// `position` donne le rang dans la liste ComicRack affichée (clé « Ordre de la liste »).
pub fn sort(
    v: &mut [usize],
    books: &[LibraryBook],
    opts: &super::ViewOptions,
    position: impl Fn(usize) -> Option<usize>,
) {
    let groups: Vec<String> = if opts.group == super::GroupKey::None {
        Vec::new()
    } else {
        // Calculé une fois par livre affiché (le tri compare beaucoup).
        let mut g = vec![String::new(); books.len()];
        for &i in v.iter() {
            g[i] = opts.group.label(&books[i]).to_lowercase();
        }
        g
    };
    v.sort_by(|&x, &y| {
        let (a, b) = (&books[x], &books[y]);
        let desc = opts.descending;
        let mut o = if groups.is_empty() { Ordering::Equal } else { rev(groups[x].cmp(&groups[y]), desc) };
        if o == Ordering::Equal {
            o = if opts.sort == SortKey::Position {
                opt_cmp(position(x), position(y), desc)
            } else {
                key_cmp(opts.sort, a, b, desc)
            };
        }
        o.then_with(|| series_cmp(a, b))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_state_groups() {
        let mut b = LibraryBook { page_count: Some(20), last_page_read: Some(0), ..Default::default() };
        assert_eq!(GroupKey::ReadState.label(&b), "Non lu");
        b.last_page_read = Some(9);
        assert_eq!(GroupKey::ReadState.label(&b), "En cours");
        b.last_page_read = Some(19);
        assert_eq!(GroupKey::ReadState.label(&b), "Lu");
    }

    #[test]
    fn dates_and_empty_values_last() {
        let mk = |id: &str, added: Option<&str>| LibraryBook {
            id: id.into(),
            added: added.map(str::to_string),
            ..Default::default()
        };
        let books = vec![mk("a", Some("2021-03-01T10:00:00")), mk("b", None), mk("c", Some("2019-10-01T12:58:00"))];
        let mut v = vec![0, 1, 2];
        let o = crate::library::ViewOptions { sort: SortKey::Added, descending: false, group: GroupKey::None };
        sort(&mut v, &books, &o, |_| None);
        assert_eq!(v, [2, 0, 1]);
        let o = crate::library::ViewOptions { descending: true, ..o };
        sort(&mut v, &books, &o, |_| None);
        assert_eq!(v, [0, 2, 1], "le livre sans date reste à la fin");
    }
}
