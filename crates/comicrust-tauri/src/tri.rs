//! Recherche, tri et regroupement des livres. Logique **dupliquée** de
//! `crates/comicrust/src/library/sorting.rs` (décision du plan : on ne touche pas à la version
//! egui), réduite aux clés de l'IPC et adaptée (texte en entrée au lieu d'énumérations egui).
//!
//! Report de correctif : une correction de `sorting.rs` sur `main` se reporte ici à la main.

use comicrust_core::library::LibraryBook;
use std::cmp::Ordering;

/// Clé de tri (texte de l'IPC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClefTri {
    /// Série, volume, numéro (ordre naturel), titre.
    Serie,
    Titre,
    Editeur,
    /// Date de parution (année, mois, jour).
    Parution,
    Ajout,
    Lecture,
    Note,
    Pages,
    Avancement,
    Taille,
    Fichier,
}

impl ClefTri {
    /// Texte de l'IPC → clé. `numero` (série puis numéro) et `annee` sont des synonymes
    /// acceptés pour `serie` et `parution`.
    pub fn depuis_texte(t: &str) -> Result<Self, String> {
        Ok(match t.trim().to_lowercase().as_str() {
            "serie" | "série" | "numero" | "numéro" | "" => Self::Serie,
            "titre" => Self::Titre,
            "editeur" | "éditeur" => Self::Editeur,
            "parution" | "annee" | "année" => Self::Parution,
            "ajout" => Self::Ajout,
            "lecture" => Self::Lecture,
            "note" => Self::Note,
            "pages" => Self::Pages,
            "avancement" => Self::Avancement,
            "taille" => Self::Taille,
            "fichier" => Self::Fichier,
            autre => return Err(format!("tri inconnu : « {autre} »")),
        })
    }
}

/// Regroupement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Regroupement {
    Aucun,
    Serie,
    Editeur,
}

impl Regroupement {
    /// Titre du groupe d'un livre.
    fn titre(self, b: &LibraryBook) -> String {
        let texte = |v: &Option<String>, vide: &str| {
            v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| vide.into())
        };
        match self {
            Regroupement::Aucun => String::new(),
            Regroupement::Serie => texte(&b.series, "(sans série)"),
            Regroupement::Editeur => texte(&b.publisher, "(sans éditeur)"),
        }
    }
}

/// Recherche : chaque mot (déjà en minuscules) doit apparaître dans un des champs principaux.
pub fn correspond(b: &LibraryBook, mots: &[String]) -> bool {
    if mots.is_empty() {
        return true;
    }
    let champs = [
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
    let mut botte = b.caption();
    for c in champs.into_iter().flatten() {
        botte.push(' ');
        botte.push_str(c);
    }
    if let Some(y) = b.year {
        botte.push_str(&format!(" {y}"));
    }
    let botte = botte.to_lowercase();
    mots.iter().all(|m| botte.contains(m))
}

/// Mots d'une recherche (minuscules, séparés par des espaces).
pub fn mots(recherche: &str) -> Vec<String> {
    recherche.split_whitespace().map(str::to_lowercase).collect()
}

fn minuscules(v: &Option<String>) -> String {
    v.as_deref().unwrap_or("").to_lowercase()
}

/// Valeurs vides en dernier, quel que soit le sens.
fn cmp_opt<T: PartialOrd>(a: Option<T>, b: Option<T>, desc: bool) -> Ordering {
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

fn cmp_serie(a: &LibraryBook, b: &LibraryBook) -> Ordering {
    minuscules(&a.series)
        .cmp(&minuscules(&b.series))
        .then(a.volume.unwrap_or(-1).cmp(&b.volume.unwrap_or(-1)))
        .then_with(|| natord::compare(a.number.as_deref().unwrap_or(""), b.number.as_deref().unwrap_or("")))
        .then_with(|| minuscules(&a.title).cmp(&minuscules(&b.title)))
}

/// Dates ComicRack (`2019-10-01T12:58:00…`) : l'ordre du texte ISO est l'ordre chronologique.
fn date(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty() && !s.starts_with("0001-01-01"))
}

fn sens(o: Ordering, desc: bool) -> Ordering {
    if desc {
        o.reverse()
    } else {
        o
    }
}

fn cmp_clef(clef: ClefTri, a: &LibraryBook, b: &LibraryBook, desc: bool) -> Ordering {
    let parution = |x: &LibraryBook| x.year.filter(|y| *y > 0).map(|y| (y, x.month.unwrap_or(0), x.day.unwrap_or(0)));
    match clef {
        ClefTri::Serie => sens(cmp_serie(a, b), desc),
        ClefTri::Titre => sens(minuscules(&a.title).cmp(&minuscules(&b.title)), desc),
        ClefTri::Editeur => sens(minuscules(&a.publisher).cmp(&minuscules(&b.publisher)), desc),
        ClefTri::Parution => cmp_opt(parution(a), parution(b), desc),
        ClefTri::Ajout => cmp_opt(date(&a.added), date(&b.added), desc),
        ClefTri::Lecture => cmp_opt(date(&a.opened), date(&b.opened), desc),
        ClefTri::Note => cmp_opt(a.rating.filter(|r| *r > 0.0), b.rating.filter(|r| *r > 0.0), desc),
        ClefTri::Pages => cmp_opt(a.page_count, b.page_count, desc),
        ClefTri::Avancement => cmp_opt(a.read_percent(), b.read_percent(), desc),
        ClefTri::Taille => cmp_opt(a.file_size, b.file_size, desc),
        ClefTri::Fichier => sens(minuscules(&a.file).cmp(&minuscules(&b.file)), desc),
    }
}

/// Trie `v` (indices dans `livres`) : par groupe, puis par la clé choisie, puis par série.
pub fn trier(v: &mut [usize], livres: &[LibraryBook], clef: ClefTri, desc: bool, groupe: Regroupement) {
    // Titre de groupe calculé une fois par livre affiché (le tri compare beaucoup).
    let groupes: Vec<String> = if groupe == Regroupement::Aucun {
        Vec::new()
    } else {
        let mut g = vec![String::new(); livres.len()];
        for &i in v.iter() {
            g[i] = groupe.titre(&livres[i]).to_lowercase();
        }
        g
    };
    v.sort_by(|&x, &y| {
        let (a, b) = (&livres[x], &livres[y]);
        let mut o = if groupes.is_empty() { Ordering::Equal } else { sens(groupes[x].cmp(&groupes[y]), desc) };
        if o == Ordering::Equal {
            o = cmp_clef(clef, a, b, desc);
        }
        o.then_with(|| cmp_serie(a, b))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn livre(id: &str, serie: &str, numero: &str) -> LibraryBook {
        LibraryBook { id: id.into(), series: Some(serie.into()), number: Some(numero.into()), ..Default::default() }
    }

    #[test]
    fn ordre_naturel_des_numeros_et_groupes() {
        let l =
            vec![livre("a", "Zeta", "2"), livre("b", "Alpha", "10"), livre("c", "Alpha", "9"), livre("d", "Zeta", "1")];
        let mut v: Vec<usize> = (0..l.len()).collect();
        trier(&mut v, &l, ClefTri::Serie, false, Regroupement::Serie);
        assert_eq!(v, [2, 1, 3, 0], "Alpha 9, Alpha 10, Zeta 1, Zeta 2");
        trier(&mut v, &l, ClefTri::Serie, true, Regroupement::Serie);
        assert_eq!(v, [0, 3, 1, 2]);
    }

    #[test]
    fn dates_vides_en_dernier() {
        let mk = |id: &str, ajout: Option<&str>| LibraryBook {
            id: id.into(),
            added: ajout.map(str::to_string),
            ..Default::default()
        };
        let l = vec![mk("a", Some("2021-03-01T10:00:00")), mk("b", None), mk("c", Some("2019-10-01T12:58:00"))];
        let mut v = vec![0, 1, 2];
        trier(&mut v, &l, ClefTri::Ajout, false, Regroupement::Aucun);
        assert_eq!(v, [2, 0, 1]);
        trier(&mut v, &l, ClefTri::Ajout, true, Regroupement::Aucun);
        assert_eq!(v, [0, 2, 1], "le livre sans date reste à la fin");
    }

    #[test]
    fn recherche_par_mots() {
        let mut b = livre("a", "Thanos", "1");
        b.publisher = Some("Marvel".into());
        assert!(correspond(&b, &mots("thanos marvel")));
        assert!(!correspond(&b, &mots("thanos dc")));
        assert!(correspond(&b, &mots("")));
    }

    #[test]
    fn clefs_et_synonymes() {
        assert_eq!(ClefTri::depuis_texte("numero").unwrap(), ClefTri::Serie);
        assert_eq!(ClefTri::depuis_texte("annee").unwrap(), ClefTri::Parution);
        assert!(ClefTri::depuis_texte("couleur").is_err());
    }
}
