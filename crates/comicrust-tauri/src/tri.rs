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

/// Regroupement : clé de groupe, libellés et ordre **identiques** à `GroupKey` de la version egui
/// (`crates/comicrust/src/library/sorting.rs`). Les groupes se suivent dans l'ordre alphabétique de
/// leur titre en minuscules (sens inversé si le tri est décroissant) ; pour « Année » le titre est
/// l'année en texte (`(sans année)` avant les chiffres, car `(` précède `0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Regroupement {
    Aucun,
    Serie,
    Editeur,
    Scenariste,
    Annee,
    Arc,
    Genre,
    Format,
    /// État de lecture : « Non lu », « En cours », « Lu ».
    EtatLecture,
}

impl Regroupement {
    /// Tous les regroupements, dans l'ordre du menu egui (`GroupKey::ALL`).
    pub const TOUS: [Regroupement; 9] = [
        Regroupement::Aucun,
        Regroupement::Serie,
        Regroupement::Editeur,
        Regroupement::Scenariste,
        Regroupement::Annee,
        Regroupement::Arc,
        Regroupement::Genre,
        Regroupement::Format,
        Regroupement::EtatLecture,
    ];

    /// Texte de l'IPC et des réglages (`vue.groupe`) → regroupement.
    pub fn depuis_texte(t: &str) -> Option<Self> {
        Some(match t {
            "aucun" => Self::Aucun,
            "serie" => Self::Serie,
            "editeur" => Self::Editeur,
            "scenariste" => Self::Scenariste,
            "annee" => Self::Annee,
            "arc" => Self::Arc,
            "genre" => Self::Genre,
            "format" => Self::Format,
            "etat" => Self::EtatLecture,
            _ => return None,
        })
    }

    /// Libellé du menu « Groupe » (`GroupKey::label_of_key` d'egui).
    pub fn libelle(self) -> &'static str {
        match self {
            Self::Aucun => "Aucun",
            Self::Serie => "Série",
            Self::Editeur => "Éditeur",
            Self::Scenariste => "Scénariste",
            Self::Annee => "Année",
            Self::Arc => "Arc",
            Self::Genre => "Genre",
            Self::Format => "Format",
            Self::EtatLecture => "État de lecture",
        }
    }

    /// Titre du groupe d'un livre (`GroupKey::label` d'egui, sauf la série : celle affichée, déduite
    /// du nom de fichier à défaut de fiche, comme les piles).
    pub(crate) fn titre(self, b: &LibraryBook) -> String {
        let texte = |v: &Option<String>, vide: &str| {
            v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| vide.into())
        };
        match self {
            Regroupement::Aucun => String::new(),
            Regroupement::Serie => texte(&Some(b.shadow_series()), "(sans série)"),
            Regroupement::Editeur => texte(&b.publisher, "(sans éditeur)"),
            Regroupement::Scenariste => texte(&b.writer, "(sans scénariste)"),
            Regroupement::Annee => {
                b.year.filter(|y| *y > 0).map(|y| y.to_string()).unwrap_or_else(|| "(sans année)".into())
            }
            Regroupement::Arc => texte(&b.story_arc, "(sans arc)"),
            Regroupement::Genre => texte(&b.genre, "(sans genre)"),
            Regroupement::Format => texte(&b.format, "(sans format)"),
            Regroupement::EtatLecture => match b.read_percent() {
                None | Some(0) => "Non lu".into(),
                Some(100) => "Lu".into(),
                Some(_) => "En cours".into(),
            },
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

/// Clés de comparaison d'un livre, calculées **une fois** avant le tri (le tri compare chaque livre
/// une dizaine de fois : sans elles, chaque comparaison allouait une demi-douzaine de chaînes).
#[derive(Default)]
struct Cles {
    /// Série affichée en minuscules, volume affiché, numéro affiché, titre en minuscules
    /// (ordre de [`cmp_serie`]).
    serie: String,
    volume: i32,
    numero: String,
    titre: String,
    /// Texte en minuscules de la clé de tri textuelle choisie (éditeur, fichier), sinon vide.
    texte: String,
}

impl Cles {
    fn de(b: &LibraryBook, clef: ClefTri) -> Self {
        Cles {
            serie: b.shadow_series().to_lowercase(),
            volume: b.shadow_volume(),
            numero: b.shadow_number(),
            titre: minuscules(&b.title),
            texte: match clef {
                ClefTri::Editeur => minuscules(&b.publisher),
                ClefTri::Fichier => minuscules(&b.file),
                _ => String::new(),
            },
        }
    }

    /// Même ordre que [`cmp_serie`].
    fn cmp_serie(&self, o: &Cles) -> Ordering {
        self.serie
            .cmp(&o.serie)
            .then(self.volume.cmp(&o.volume))
            .then_with(|| natord::compare(&self.numero, &o.numero))
            .then_with(|| self.titre.cmp(&o.titre))
    }
}

/// Comme [`cmp_clef`], avec les clés précalculées pour les clés textuelles.
fn cmp_clef_cles(clef: ClefTri, a: &LibraryBook, b: &LibraryBook, ka: &Cles, kb: &Cles, desc: bool) -> Ordering {
    match clef {
        ClefTri::Serie | ClefTri::Ordre => sens(ka.cmp_serie(kb), desc),
        ClefTri::Titre => sens(ka.titre.cmp(&kb.titre), desc),
        ClefTri::Editeur | ClefTri::Fichier => sens(ka.texte.cmp(&kb.texte), desc),
        _ => cmp_clef(clef, a, b, desc),
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
    // Titre de groupe et clés calculés une fois par livre affiché (le tri compare beaucoup).
    let groupes: Vec<String> = if groupe == Regroupement::Aucun {
        Vec::new()
    } else {
        let mut g = vec![String::new(); livres.len()];
        for &i in v.iter() {
            g[i] = groupe.titre(&livres[i]).to_lowercase();
        }
        g
    };
    let mut cles: Vec<Cles> = (0..livres.len()).map(|_| Cles::default()).collect();
    for &i in v.iter() {
        cles[i] = Cles::de(&livres[i], clef);
    }
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
        let (ka, kb) = (&cles[x], &cles[y]);
        let mut o = if groupes.is_empty() {
            Ordering::Equal
        } else if premier.is_empty() {
            sens(groupes[x].cmp(&groupes[y]), desc)
        } else {
            let r = |i: usize| premier.get(groupes[i].as_str()).copied();
            cmp_opt(r(x), r(y), desc).then_with(|| sens(groupes[x].cmp(&groupes[y]), desc))
        };
        if o == Ordering::Equal {
            o = if clef == ClefTri::Ordre { cmp_opt(rang(x), rang(y), desc) } else { cmp_clef_cles(clef, a, b, ka, kb, desc) };
        }
        o.then_with(|| ka.cmp_serie(kb))
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

    /// Livre d'essai pour les clés de groupe.
    fn fiche(id: &str) -> LibraryBook {
        LibraryBook { id: id.into(), series: Some("S".into()), ..Default::default() }
    }

    #[test]
    fn titres_des_groupes_comme_egui() {
        let vide = fiche("v");
        assert_eq!(Regroupement::Aucun.titre(&vide), "");
        assert_eq!(Regroupement::Editeur.titre(&vide), "(sans éditeur)");
        assert_eq!(Regroupement::Scenariste.titre(&vide), "(sans scénariste)");
        assert_eq!(Regroupement::Annee.titre(&vide), "(sans année)");
        assert_eq!(Regroupement::Arc.titre(&vide), "(sans arc)");
        assert_eq!(Regroupement::Genre.titre(&vide), "(sans genre)");
        assert_eq!(Regroupement::Format.titre(&vide), "(sans format)");
        assert_eq!(Regroupement::EtatLecture.titre(&vide), "Non lu");
        let mut b = fiche("b");
        b.writer = Some("  Jim Starlin ".into());
        b.year = Some(1991);
        b.story_arc = Some("Infinity Gauntlet".into());
        b.genre = Some("Super-héros".into());
        b.format = Some("Series".into());
        assert_eq!(Regroupement::Scenariste.titre(&b), "Jim Starlin", "espaces retirés");
        assert_eq!(Regroupement::Annee.titre(&b), "1991");
        assert_eq!(Regroupement::Arc.titre(&b), "Infinity Gauntlet");
        assert_eq!(Regroupement::Genre.titre(&b), "Super-héros");
        assert_eq!(Regroupement::Format.titre(&b), "Series");
        // Année 0 = pas d'année ; texte blanc = absent.
        b.year = Some(0);
        b.writer = Some("   ".into());
        assert_eq!(Regroupement::Annee.titre(&b), "(sans année)");
        assert_eq!(Regroupement::Scenariste.titre(&b), "(sans scénariste)");
    }

    #[test]
    fn etat_de_lecture_non_lu_en_cours_lu() {
        let mut b = fiche("b");
        b.page_count = Some(20);
        assert_eq!(Regroupement::EtatLecture.titre(&b), "Non lu", "jamais ouvert");
        b.last_page_read = Some(0);
        assert_eq!(Regroupement::EtatLecture.titre(&b), "Non lu");
        b.last_page_read = Some(9);
        assert_eq!(Regroupement::EtatLecture.titre(&b), "En cours");
        b.last_page_read = Some(19);
        assert_eq!(Regroupement::EtatLecture.titre(&b), "Lu");
    }

    #[test]
    fn textes_et_libelles_des_regroupements() {
        let attendu = [
            ("aucun", "Aucun"),
            ("serie", "Série"),
            ("editeur", "Éditeur"),
            ("scenariste", "Scénariste"),
            ("annee", "Année"),
            ("arc", "Arc"),
            ("genre", "Genre"),
            ("format", "Format"),
            ("etat", "État de lecture"),
        ];
        // Même ordre que `GroupKey::ALL` d'egui.
        for (r, (texte, libelle)) in Regroupement::TOUS.iter().zip(attendu) {
            assert_eq!(Regroupement::depuis_texte(texte), Some(*r));
            assert_eq!(r.libelle(), libelle);
        }
        assert_eq!(Regroupement::depuis_texte("couleur"), None);
    }

    #[test]
    fn groupes_par_annee_ordre_alphabetique_du_titre() {
        // egui compare les titres en minuscules : « (sans année) » avant les chiffres ; en tri
        // décroissant, les années décroissent et « (sans année) » passe en dernier.
        let mk = |id: &str, an: Option<i32>| LibraryBook { id: id.into(), year: an, ..Default::default() };
        let l = vec![mk("a", Some(1995)), mk("b", None), mk("c", Some(1991)), mk("d", Some(2003))];
        let mut v = vec![0, 1, 2, 3];
        trier(&mut v, &l, ClefTri::Titre, false, Regroupement::Annee, &|_| None);
        assert_eq!(v, [1, 2, 0, 3]);
        trier(&mut v, &l, ClefTri::Titre, true, Regroupement::Annee, &|_| None);
        assert_eq!(v, [3, 0, 2, 1]);
    }

    #[test]
    fn groupes_par_scenariste_et_etat_ordre_egui() {
        let mk = |id: &str, w: Option<&str>, lu: Option<u32>| LibraryBook {
            id: id.into(),
            writer: w.map(str::to_string),
            page_count: Some(10),
            last_page_read: lu,
            ..Default::default()
        };
        let l = vec![mk("a", Some("Zoe"), Some(9)), mk("b", None, None), mk("c", Some("alan"), Some(4))];
        let mut v = vec![0, 1, 2];
        trier(&mut v, &l, ClefTri::Titre, false, Regroupement::Scenariste, &|_| None);
        assert_eq!(v, [1, 2, 0], "(sans scénariste), alan, Zoe : titres comparés en minuscules");
        // « en cours » < « lu » < « non lu » (ordre alphabétique des titres en minuscules).
        trier(&mut v, &l, ClefTri::Titre, false, Regroupement::EtatLecture, &|_| None);
        assert_eq!(v, [2, 0, 1], "En cours (c), Lu (a), Non lu (b)");
    }

    #[test]
    fn groupes_dans_l_ordre_de_la_liste_pour_les_nouvelles_cles() {
        // Ordre de la liste + groupe par scénariste : le groupe suit son premier livre dans la liste.
        let mk = |id: &str, w: &str| LibraryBook { id: id.into(), writer: Some(w.into()), ..Default::default() };
        let l = vec![mk("a1", "A"), mk("z1", "Z"), mk("a2", "A")];
        let pos = [1usize, 0, 2]; // liste : z1, a1, a2
        let mut v = vec![0, 1, 2];
        trier(&mut v, &l, ClefTri::Ordre, false, Regroupement::Scenariste, &|i| Some(pos[i]));
        assert_eq!(v, [1, 0, 2], "Z d'abord (premier de la liste), puis A");
    }

    /// Relecture tauri-v1.0.2 : le tri à clés précalculées donne exactement l'ordre de l'ancienne
    /// comparaison (calculée à chaque comparaison), pour toutes les clés, sens et groupes.
    #[test]
    fn cles_precalculees_meme_ordre_que_la_comparaison_directe() {
        let l: Vec<LibraryBook> = (0..300)
            .map(|i| LibraryBook {
                id: format!("{i}"),
                series: (i % 3 != 0).then(|| ["Zeta", "alpha", "Alpha", "Béta"][i % 4].to_string()),
                number: (i % 5 != 0).then(|| format!("{}", (i * 7) % 23)),
                volume: (i % 4 == 0).then_some((i % 3) as i32),
                title: (i % 2 == 0).then(|| format!("Titre {}", (i * 13) % 17)),
                publisher: (i % 6 != 0).then(|| ["Marvel", "dargaud", "DC"][i % 3].to_string()),
                file: Some(format!("D:\\BD\\Serie {} #{:02}.cbz", i % 7, i % 11)),
                year: (i % 5 != 0).then_some(1990 + (i % 9) as i32),
                page_count: Some((i % 40) as u32),
                ..Default::default()
            })
            .collect();
        let rang = |i: usize| (i % 4 != 0).then_some((i * 31) % 97);
        let reference = |v: &mut Vec<usize>, clef: ClefTri, desc: bool, groupe: Regroupement| {
            // Ancienne implémentation (sans le cas « premier livre du groupe », non utilisé ici).
            v.sort_by(|&x, &y| {
                let (a, b) = (&l[x], &l[y]);
                let g = |i: usize| groupe.titre(&l[i]).to_lowercase();
                let mut o = if groupe == Regroupement::Aucun { Ordering::Equal } else { sens(g(x).cmp(&g(y)), desc) };
                if o == Ordering::Equal {
                    o = if clef == ClefTri::Ordre { cmp_opt(rang(x), rang(y), desc) } else { cmp_clef(clef, a, b, desc) };
                }
                o.then_with(|| cmp_serie(a, b))
            });
        };
        use ClefTri::*;
        for clef in [Serie, Titre, Editeur, Parution, Ajout, Lecture, Note, Pages, Avancement, Taille, Fichier] {
            for desc in [false, true] {
                for groupe in [Regroupement::Aucun, Regroupement::Serie, Regroupement::Editeur, Regroupement::Annee] {
                    let mut attendu: Vec<usize> = (0..l.len()).rev().collect();
                    reference(&mut attendu, clef, desc, groupe);
                    let mut v: Vec<usize> = (0..l.len()).rev().collect();
                    trier(&mut v, &l, clef, desc, groupe, &rang);
                    assert_eq!(v, attendu, "{clef:?} desc={desc} {groupe:?}");
                }
            }
        }
    }
}
