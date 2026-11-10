//! Recherche, tri et regroupement des livres. Logique **dupliquée** de
//! `crates/comicrust/src/library/sorting.rs` (décision du plan : on ne touche pas à la version
//! egui), réduite aux clés de l'IPC et adaptée (texte en entrée au lieu d'énumérations egui).
//!
//! Report de correctif : une correction de `sorting.rs` sur `main` se reporte ici à la main.
//!
//! Exception : la recherche rapide ([`filtrer`]) ne reprend plus `sorting::matches` (qui
//! s'écartait de ComicRack, recette tauri-v0.2.0 R-01) mais l'évaluateur fidèle des listes
//! intelligentes de `comicrust-core` (critère « Toutes les propriétés »).

use comicrust_core::library::LibraryBook;
use comicrust_core::smart::{Matcher, Mode, SmartQuery};
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
    /// « Ordre de la liste » : rang du livre dans la liste affichée (bibliothèque : ordre de la
    /// base). Avec un groupe, les groupes suivent leur premier livre dans la liste (v0.10.2).
    Ordre,
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
            "ordre" | "liste" | "position" | "ordreliste" => Self::Ordre,
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
    pub(crate) fn titre(self, b: &LibraryBook) -> String {
        let texte = |v: &Option<String>, vide: &str| {
            v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| vide.into())
        };
        match self {
            Regroupement::Aucun => String::new(),
            Regroupement::Serie => texte(&Some(b.shadow_series()), "(sans série)"),
            Regroupement::Editeur => texte(&b.publisher, "(sans éditeur)"),
        }
    }
}

/// Recherche rapide de la vue livres, **fidèle à ComicRack CE** (boîte de recherche en mode
/// « Tout », `ComicBrowserControl.UpdateQuickFilter`) : un critère
/// `ComicBookAllPropertiesMatcher` (option `All`) avec l'opérateur 3 « contient tous les mots »,
/// casse ignorée. On réutilise l'évaluateur des listes intelligentes de `comicrust-core`
/// (`smart.rs`, `all_properties_values` = `GetOptionValueSet`), donc les mêmes règles :
///
/// - un livre est retenu si **un même champ** contient **tous** les mots (pas un mot par champ) ;
/// - champs examinés : série (ou proposée), série alternative, titre, groupe de séries, arc,
///   scénariste, dessinateur, encreur, coloriste, lettreur, éditeur, traducteur, couverture,
///   **résumé**, chemin du fichier, genre, **notes**, **critique**, éditeur, label, volume (« V3 »),
///   numéro, numéro alternatif, année, format, âge, étiquettes, **personnages**, **équipes**,
///   **personnage principal**, **lieux**, champs du catalogue (âge, état, notes, propriétaire,
///   magasin, emplacement, ISBN), **infos de scan** et valeurs personnalisées ;
/// - découpage : sur `,` ou `;` s'il y en a, sinon sur les espaces (pas de guillemets) ;
/// - casse ignorée, accents **non** ignorés (`StringComparison.OrdinalIgnoreCase`).
///
/// Non repris : les requêtes commençant par `NOT` ou `MATCH` (langage des listes
/// intelligentes) sont cherchées comme de simples mots.
///
/// Renvoie les indices (ordre conservé) des livres retenus ; recherche vide = tous.
pub fn filtrer(livres: &[LibraryBook], recherche: &str) -> Vec<usize> {
    if recherche.trim().is_empty() {
        return (0..livres.len()).collect();
    }
    let requete = SmartQuery {
        mode: Mode::And,
        matchers: vec![Matcher::value("ComicBookAllPropertiesMatcher", 3, recherche, "", false, true)],
        ..Default::default()
    };
    requete.select(livres, 0..livres.len())
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
    // Valeurs « fantômes » : à défaut de fiche, celles déduites du nom de fichier (comme ComicRack).
    a.shadow_series()
        .to_lowercase()
        .cmp(&b.shadow_series().to_lowercase())
        .then(a.shadow_volume().cmp(&b.shadow_volume()))
        .then_with(|| natord::compare(&a.shadow_number(), &b.shadow_number()))
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
        // Comparé par `trier` (rang dans la liste) ; ici, à égalité : série.
        ClefTri::Ordre => sens(cmp_serie(a, b), desc),
    }
}

/// Trie `v` (indices dans `livres`) : par groupe, puis par la clé choisie, puis par série.
/// `rang` donne la position d'un livre dans la liste affichée (clé [`ClefTri::Ordre`]).
///
/// Dans l'ordre de la liste, les groupes se suivent dans l'ordre de leur **premier livre dans
/// la liste** (et non par ordre alphabétique) : déplacer une série dans la liste de lecture la
/// déplace aussi à l'écran (v0.10.2, `sorting::sort` de la version egui).
pub fn trier(
    v: &mut [usize],
    livres: &[LibraryBook],
    clef: ClefTri,
    desc: bool,
    groupe: Regroupement,
    rang: &dyn Fn(usize) -> Option<usize>,
) {
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
    // Ordre de la liste : rang du premier livre de chaque groupe.
    let mut premier: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    if clef == ClefTri::Ordre && !groupes.is_empty() {
        for &i in v.iter() {
            if let Some(p) = rang(i) {
                let e = premier.entry(groupes[i].as_str()).or_insert(p);
                *e = (*e).min(p);
            }
        }
    }
    v.sort_by(|&x, &y| {
        let (a, b) = (&livres[x], &livres[y]);
        let mut o = if groupes.is_empty() {
            Ordering::Equal
        } else if premier.is_empty() {
            sens(groupes[x].cmp(&groupes[y]), desc)
        } else {
            let r = |i: usize| premier.get(groupes[i].as_str()).copied();
            cmp_opt(r(x), r(y), desc).then_with(|| sens(groupes[x].cmp(&groupes[y]), desc))
        };
        if o == Ordering::Equal {
            o = if clef == ClefTri::Ordre { cmp_opt(rang(x), rang(y), desc) } else { cmp_clef(clef, a, b, desc) };
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
        trier(&mut v, &l, ClefTri::Serie, false, Regroupement::Serie, &|_| None);
        assert_eq!(v, [2, 1, 3, 0], "Alpha 9, Alpha 10, Zeta 1, Zeta 2");
        trier(&mut v, &l, ClefTri::Serie, true, Regroupement::Serie, &|_| None);
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
        trier(&mut v, &l, ClefTri::Ajout, false, Regroupement::Aucun, &|_| None);
        assert_eq!(v, [2, 0, 1]);
        trier(&mut v, &l, ClefTri::Ajout, true, Regroupement::Aucun, &|_| None);
        assert_eq!(v, [0, 2, 1], "le livre sans date reste à la fin");
    }

    /// Indices retenus par la recherche rapide.
    fn cherche(l: &[LibraryBook], q: &str) -> Vec<usize> {
        filtrer(l, q)
    }

    #[test]
    fn recherche_par_mots() {
        let mut b = livre("a", "Thanos", "1");
        b.publisher = Some("Marvel".into());
        let l = [b];
        assert_eq!(cherche(&l, "thanos"), [0]);
        assert_eq!(cherche(&l, "THANOS"), [0], "casse ignorée");
        assert_eq!(cherche(&l, ""), [0]);
        assert_eq!(cherche(&l, "thanos dc"), [] as [usize; 0]);
    }

    #[test]
    fn recherche_dans_resume_personnages_et_autres_champs() {
        // R-01 : « thanos » n'apparaît que dans le résumé, les personnages, etc.
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
        let l = [a, b];
        // Avec une virgule, découpage sur la virgule seulement : « surfer » et « silver » restent ensemble.
        assert_eq!(cherche(&l, "silver surfer, galactus"), [] as [usize; 0]);
        assert_eq!(cherche(&l, "silver surfer"), [0]);
        assert_eq!(cherche(&l, "argent;galactus"), [0]);
        // Accents non ignorés (OrdinalIgnoreCase), casse ignorée.
        assert_eq!(cherche(&l, "ÉTÉ"), [1]);
        assert_eq!(cherche(&l, "ete"), [] as [usize; 0]);
    }

    #[test]
    fn recherche_numero_annee_fichier() {
        let mut a = livre("a", "Infinity Gauntlet", "3");
        a.year = Some(1991);
        a.file = Some(r"D:\BD\Marvel\Infinity Gauntlet 003.cbz".into());
        let l = [a];
        assert_eq!(cherche(&l, "1991"), [0]);
        assert_eq!(cherche(&l, "bd\\marvel"), [0], "chemin complet du fichier");
    }

    #[test]
    fn clefs_et_synonymes() {
        assert_eq!(ClefTri::depuis_texte("numero").unwrap(), ClefTri::Serie);
        assert_eq!(ClefTri::depuis_texte("annee").unwrap(), ClefTri::Parution);
        assert!(ClefTri::depuis_texte("couleur").is_err());
        assert_eq!(ClefTri::depuis_texte("ordre").unwrap(), ClefTri::Ordre);
        assert_eq!(ClefTri::depuis_texte("position").unwrap(), ClefTri::Ordre);
    }

    /// Ordre de la liste, livres groupés : les groupes suivent leur premier livre dans la liste ;
    /// à l'intérieur d'un groupe, l'ordre de la liste (test `groups_follow_the_list_order` egui).
    #[test]
    fn groupes_dans_l_ordre_de_la_liste() {
        let l = vec![
            livre("a1", "A", "1"),
            livre("a2", "A", "2"),
            livre("m1", "M", "1"),
            livre("z1", "Z", "1"),
            livre("z2", "Z", "2"),
        ];
        // Liste : Z1, A1, Z2, M1, A2 (Z avant A et M, malgré l'alphabet).
        let pos = [1usize, 4, 3, 0, 2];
        let mut v = vec![0, 1, 2, 3, 4];
        trier(&mut v, &l, ClefTri::Ordre, false, Regroupement::Serie, &|i| Some(pos[i]));
        assert_eq!(v, [3, 4, 0, 1, 2], "Z (z1, z2), A (a1, a2), M");
        trier(&mut v, &l, ClefTri::Ordre, true, Regroupement::Serie, &|i| Some(pos[i]));
        assert_eq!(v, [2, 1, 0, 4, 3], "décroissant : groupes et livres à l'envers");
        trier(&mut v, &l, ClefTri::Ordre, false, Regroupement::Aucun, &|i| Some(pos[i]));
        assert_eq!(v, [3, 0, 4, 2, 1], "sans groupe : ordre de la liste");
    }
}
