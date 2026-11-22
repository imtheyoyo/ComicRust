//! Recherche, tri et regroupement des livres de la bibliothèque.

use std::cmp::Ordering;

use comicrust_core::library::LibraryBook;
use comicrust_core::smart::{Matcher, Mode, SmartQuery};
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

/// Recherche rapide de la vue bibliothèque, **fidèle à ComicRack CE** (boîte de recherche en
/// mode « Tout », `ComicBrowserControl.UpdateQuickFilter`) : un critère
/// `ComicBookAllPropertiesMatcher` (option `All`) avec l'opérateur 3 « contient tous les mots »,
/// casse ignorée. On réutilise l'évaluateur des listes intelligentes de `comicrust-core`
/// (`smart.rs`, `all_properties_values` = `GetOptionValueSet`), donc les mêmes règles :
///
/// - un livre est retenu si **un même champ** contient **tous** les mots (pas un mot par champ) ;
/// - champs examinés : série (ou proposée), série alternative, titre, groupe de séries, arc,
///   scénariste, dessinateur, encreur, coloriste, lettreur, éditeur, traducteur, couverture,
///   **résumé**, chemin du fichier, genre, **notes**, **critique**, label, volume (« V3 »),
///   numéro, numéro alternatif, année, format, âge, étiquettes, **personnages**, **équipes**,
///   **personnage principal**, **lieux**, champs du catalogue, **infos de scan** et valeurs
///   personnalisées ;
/// - découpage : sur `,` ou `;` s'il y en a, sinon sur les espaces (pas de guillemets) ;
/// - casse ignorée, accents **non** ignorés (`StringComparison.OrdinalIgnoreCase`).
///
/// Non repris : les requêtes commençant par `NOT` ou `MATCH` (langage des listes
/// intelligentes) sont cherchées comme de simples mots.
///
/// Même correctif que `tri::filtrer` de la branche Tauri (ancienne version : une douzaine de
/// champs seulement, mots répartis sur plusieurs champs, « thanos » 130 livres au lieu de 483).
///
/// Renvoie, dans l'ordre de `candidates`, ceux qui correspondent ; recherche vide = tous.
pub fn filter(books: &[LibraryBook], candidates: Vec<usize>, search: &str) -> Vec<usize> {
    if search.trim().is_empty() {
        return candidates;
    }
    let query = SmartQuery {
        mode: Mode::And,
        matchers: vec![Matcher::value("ComicBookAllPropertiesMatcher", 3, search, "", false, true)],
        ..Default::default()
    };
    query.select(books, candidates)
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
///
/// Dans l'ordre de la liste, les groupes se suivent dans l'ordre de leur **premier livre
/// dans la liste** (et non par ordre alphabétique) : déplacer une série dans la liste de
/// lecture la déplace donc aussi à l'écran (v0.10.2).
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
    // Ordre de la liste : rang du premier livre de chaque groupe.
    let mut first: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    if opts.sort == SortKey::Position && !groups.is_empty() {
        for &i in v.iter() {
            if let Some(p) = position(i) {
                let e = first.entry(groups[i].as_str()).or_insert(p);
                *e = (*e).min(p);
            }
        }
    }
    v.sort_by(|&x, &y| {
        let (a, b) = (&books[x], &books[y]);
        let desc = opts.descending;
        let mut o = if groups.is_empty() {
            Ordering::Equal
        } else if first.is_empty() {
            rev(groups[x].cmp(&groups[y]), desc)
        } else {
            let rank = |i: usize| first.get(groups[i].as_str()).copied();
            opt_cmp(rank(x), rank(y), desc).then_with(|| rev(groups[x].cmp(&groups[y]), desc))
        };
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

    fn livre(id: &str, series: &str, number: &str) -> LibraryBook {
        LibraryBook { id: id.into(), series: Some(series.into()), number: Some(number.into()), ..Default::default() }
    }

    /// Indices retenus par la recherche rapide parmi tous les livres.
    fn cherche(l: &[LibraryBook], q: &str) -> Vec<usize> {
        filter(l, (0..l.len()).collect(), q)
    }

    #[test]
    fn recherche_par_mots() {
        let mut b = livre("a", "Thanos", "1");
        b.publisher = Some("Marvel".into());
        let l = [b];
        assert_eq!(cherche(&l, "thanos"), [0]);
        assert_eq!(cherche(&l, "THANOS"), [0], "casse ignorée");
        assert_eq!(cherche(&l, ""), [0]);
        assert_eq!(cherche(&l, "   "), [0]);
        assert_eq!(cherche(&l, "thanos dc"), [] as [usize; 0]);
    }

    #[test]
    fn recherche_dans_resume_personnages_et_autres_champs() {
        // « thanos » n'apparaît que dans le résumé, les personnages, les notes, etc.
        let mk = |id: &str| LibraryBook { id: id.into(), series: Some("Avengers".into()), ..Default::default() };
        let mut l: Vec<LibraryBook> = (0..9).map(|i| mk(&i.to_string())).collect();
        l[0].summary = Some("Les Vengeurs affrontent Thanos.".into());
        l[1].characters = Some("Captain America, Thanos".into());
        l[2].notes = Some("Scanned by thanos-team".into());
        l[3].locations = Some("Titan, Thanos' ship".into());
        l[4].main_character_or_team = Some("Thanos".into());
        l[5].scan_information = Some("THANOS-DCP".into());
        l[6].review = Some("Le meilleur Thanos".into());
        l[7].series_group = Some("Thanos Saga".into());
        // l[8] : aucun champ ne mentionne Thanos.
        assert_eq!(cherche(&l, "thanos"), [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn tous_les_mots_dans_un_meme_champ() {
        // ComicRack : « contient tous les mots » est testé champ par champ.
        let mut a = livre("a", "Thanos", "1");
        a.publisher = Some("Marvel".into());
        let mut b = livre("b", "Avengers", "1");
        b.summary = Some("Thanos attaque la Terre (Marvel)".into());
        let l = [a, b];
        assert_eq!(cherche(&l, "thanos marvel"), [1], "mots répartis sur deux champs : non retenu");
        assert_eq!(cherche(&l, "marvel thanos"), [1], "ordre des mots indifférent");
    }

    #[test]
    fn decoupage_virgules_et_accents() {
        let mut a = livre("a", "Silver Surfer", "1");
        a.summary = Some("Le Surfeur d'argent rencontre Galactus".into());
        let mut b = livre("b", "Héros", "1");
        b.title = Some("Été".into());
        let mut c = livre("c", "Infinity Gauntlet", "1");
        c.characters = Some("Adam Warlock, Thanos".into());
        let l = [a, b, c];
        // Avec une virgule, découpage sur la virgule seulement : « silver surfer » reste un seul mot.
        assert_eq!(cherche(&l, "silver surfer, galactus"), [] as [usize; 0]);
        assert_eq!(cherche(&l, "silver surfer"), [0]);
        assert_eq!(cherche(&l, "argent;galactus"), [0]);
        assert_eq!(cherche(&l, "adam warlock, thanos"), [2], "les deux dans les personnages");
        assert_eq!(cherche(&l, "infinity gauntlet, thanos"), [] as [usize; 0], "série et personnages : deux champs");
        // Accents non ignorés (OrdinalIgnoreCase), casse ignorée.
        assert_eq!(cherche(&l, "ÉTÉ"), [1]);
        assert_eq!(cherche(&l, "ete"), [] as [usize; 0]);
    }

    #[test]
    fn recherche_numero_annee_fichier_et_candidats() {
        let mut a = livre("a", "Infinity Gauntlet", "3");
        a.year = Some(1991);
        a.file = Some(r"D:\BD\Marvel\Infinity Gauntlet 003.cbz".into());
        let b = livre("b", "Infinity Gauntlet", "4");
        let l = [a, b];
        assert_eq!(cherche(&l, "1991"), [0]);
        assert_eq!(cherche(&l, "bd\\marvel"), [0], "chemin complet du fichier");
        // Ordre et sous-ensemble des candidats (liste affichée) conservés.
        assert_eq!(filter(&l, vec![1, 0], "gauntlet"), [1, 0]);
        assert_eq!(filter(&l, vec![1], "gauntlet"), [1]);
    }

    /// Recherche en direct : 5 000 livres bien remplis filtrés à chaque frappe sans gel.
    #[test]
    fn recherche_rapide_sur_5000_livres() {
        let l: Vec<LibraryBook> = (0..5000)
            .map(|i| LibraryBook {
                id: format!("{i}"),
                series: Some(format!("Série {}", i % 300)),
                number: Some(format!("{}", i % 50)),
                title: Some(format!("Titre du numéro {i}")),
                writer: Some("Jim Starlin".into()),
                penciller: Some("George Pérez".into()),
                summary: Some("Un long résumé qui raconte toute l'histoire, avec Thanos ".repeat(8)),
                characters: Some("Adam Warlock, Gamora, Pip, Drax".into()),
                file: Some(format!(r"D:\BD\Marvel\Série {}\{i:05}.cbz", i % 300)),
                ..Default::default()
            })
            .collect();
        let t = std::time::Instant::now();
        let n = cherche(&l, "infinity gauntlet").len();
        let ms = t.elapsed().as_millis();
        assert_eq!(n, 0);
        assert!(ms < 2000, "recherche sur 5 000 livres : {ms} ms");
    }

    #[test]
    fn read_state_groups() {
        let mut b = LibraryBook { page_count: Some(20), last_page_read: Some(0), ..Default::default() };
        assert_eq!(GroupKey::ReadState.label(&b), "Non lu");
        b.last_page_read = Some(9);
        assert_eq!(GroupKey::ReadState.label(&b), "En cours");
        b.last_page_read = Some(19);
        assert_eq!(GroupKey::ReadState.label(&b), "Lu");
    }

    /// Ordre de la liste, livres groupés : les groupes suivent leur premier livre dans la
    /// liste ; à l'intérieur d'un groupe, l'ordre de la liste.
    #[test]
    fn groups_follow_the_list_order() {
        let mk = |id: &str, series: &str| LibraryBook {
            id: id.into(),
            series: Some(series.into()),
            number: Some(id[1..].into()),
            ..Default::default()
        };
        // Liste : Z1, A1, Z2, M1, A2 (Z avant A et M, malgré l'alphabet).
        let books = vec![mk("a1", "A"), mk("a2", "A"), mk("m1", "M"), mk("z1", "Z"), mk("z2", "Z")];
        let pos = [1usize, 4, 3, 0, 2];
        let mut v = vec![0, 1, 2, 3, 4];
        let o = crate::library::ViewOptions { sort: SortKey::Position, descending: false, group: GroupKey::Series };
        sort(&mut v, &books, &o, |i| Some(pos[i]));
        assert_eq!(v, [3, 4, 0, 1, 2], "Z (z1, z2), A (a1, a2), M");
        // Décroissant : groupes et livres à l'envers.
        let o = crate::library::ViewOptions { descending: true, ..o };
        sort(&mut v, &books, &o, |i| Some(pos[i]));
        assert_eq!(v, [2, 1, 0, 4, 3]);
        // Autre tri : groupes par ordre alphabétique, comme avant.
        let o = crate::library::ViewOptions { sort: SortKey::Series, descending: false, group: GroupKey::Series };
        sort(&mut v, &books, &o, |i| Some(pos[i]));
        assert_eq!(v, [0, 1, 2, 3, 4]);
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
