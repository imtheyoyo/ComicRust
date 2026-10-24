//! Catalogue des critères de liste intelligente, pour l'éditeur de l'interface.
//!
//! Repris des classes `ComicBook…Matcher` / `SmartListSeries…Matcher` de ComicRack CE :
//! pour chaque type (`xsi:type`), son libellé français, la nature de sa valeur, ses
//! opérateurs (`MatchOperator` : le numéro est l'indice dans `OperatorsList` de la classe
//! de base) et le nombre de valeurs attendues, et si ComicRust sait l'évaluer
//! ([`crate::smart`]).
//!
//! Libellés : ceux de l'interface française de ComicRack (`Languages/fr/Matchers.xml` de
//! la référence), corrigés quand la traduction est fautive ou ambiguë : « fini par » →
//! « finit par », « regular expression » (non traduit) → « expression régulière », « est
//! en dernier » → « dans les derniers » (+ unité « jours »), « Série : Début/Fin du blanc »
//! (inversés dans la traduction), « État de la série » → « État de la collection »
//! (`BookCollectionStatus`), « Lieu(x) dans le Livre » → « Emplacement du Livre »
//! (`BookLocation`), « Nombre de favoris » → « Nombre de signets », « Notes » distinct
//! d'« Annotations » (`BookNotes`), « Série : Livre publié » → « Livre sorti »
//! (`Released`) ; « Lettreur », absent de la traduction, ajouté. Manga, opérateur 1 :
//! « est Oui (de droite à gauche) » (valeur `YesAndRightToLeft` ; le libellé anglais de
//! ComicRack, « Left to Right », est faux).
//!
//! Opérateurs (vérifiés dans la référence) :
//!
//! | base | opérateurs (numéro : libellé) |
//! |---|---|
//! | `ComicBookStringMatcher` | 0 est, 1 contient, 2 contient une partie de, 3 contient tout, 4 commence par, 5 finit par, 6 liste contient, 7 expression régulière |
//! | `ComicBookNumericMatcher` | 0 est, 1 est plus grand, 2 est plus petit, 3 est dans la fourchette (2 valeurs) |
//! | `ComicBookDateMatcher` | 0 est, 1 est après, 2 est avant, 3 dans les derniers (n jours), 4 est dans la fourchette (2 valeurs) |
//! | `ComicBookYesNoMatcher` | 0 est Oui, 1 est Non, 2 est Inconnu (aucune valeur) |
//! | `ComicBookMangaMatcher` | 0 est Oui, 1 est Oui (de droite à gauche), 2 est Non, 3 est Inconnu |
//! | `ComicBookDuplicateMatcher` | 0 Oui, 1 Non |
//! | `ComicBookExpressionMatcher` (module) | 0 est Vrai, 1 est Faux (1 valeur : l'expression) |
//!
//! Valeurs : un nombre décimal est relu par ComicRack avec `float.TryParse` dans la
//! culture du poste (virgule en français) — préférer les entiers ; une date avec
//! `DateTime.TryParse` (culture du poste) : écrire `AAAA-MM-JJ`, lu partout. Valeur
//! personnalisée : `MatchValue` = nom de la valeur, `MatchValue2` = valeur cherchée.

/// Nature de la valeur d'un critère.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Text,
    Number,
    Date,
    /// Oui / non / inconnu (aucune valeur à saisir).
    YesNo,
    /// Manga : oui / oui de droite à gauche / non / inconnu (aucune valeur à saisir).
    Manga,
    /// Activé / désactivé (« seulement les doublons »).
    OnOff,
    /// Valeur personnalisée : nom (`MatchValue`) puis texte cherché (`MatchValue2`).
    CustomValue,
    /// Expression de script (module de ComicRack) : vrai / faux.
    Expression,
}

impl ValueKind {
    /// Critère texte (`ComicBookStringMatcher` : attribut `IgnoreCase`).
    pub fn is_text(self) -> bool {
        matches!(self, ValueKind::Text | ValueKind::CustomValue)
    }

    /// Opérateurs de cette nature de valeur, dans l'ordre (le numéro est `op`).
    pub fn operators(self) -> &'static [OperatorInfo] {
        match self {
            ValueKind::Text | ValueKind::CustomValue => TEXT_OPS,
            ValueKind::Number => NUMBER_OPS,
            ValueKind::Date => DATE_OPS,
            ValueKind::YesNo => YES_NO_OPS,
            ValueKind::Manga => MANGA_OPS,
            ValueKind::OnOff => ON_OFF_OPS,
            ValueKind::Expression => EXPRESSION_OPS,
        }
    }
}

/// Un opérateur (`MatchOperator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorInfo {
    pub op: i32,
    pub label: &'static str,
    /// Nombre de valeurs à saisir (0, 1 ou 2 : `MatchValue`, `MatchValue2`).
    pub values: u8,
    /// Unité affichée après la valeur (« jours »).
    pub unit: Option<&'static str>,
}

const fn op(op: i32, label: &'static str, values: u8) -> OperatorInfo {
    OperatorInfo { op, label, values, unit: None }
}

const TEXT_OPS: &[OperatorInfo] = &[
    op(0, "est", 1),
    op(1, "contient", 1),
    op(2, "contient une partie de", 1),
    op(3, "contient tout", 1),
    op(4, "commence par", 1),
    op(5, "finit par", 1),
    op(6, "liste contient", 1),
    op(7, "expression régulière", 1),
];

const NUMBER_OPS: &[OperatorInfo] =
    &[op(0, "est", 1), op(1, "est plus grand", 1), op(2, "est plus petit", 1), op(3, "est dans la fourchette", 2)];

const DATE_OPS: &[OperatorInfo] = &[
    op(0, "est", 1),
    op(1, "est après", 1),
    op(2, "est avant", 1),
    OperatorInfo { op: 3, label: "dans les derniers", values: 1, unit: Some("jours") },
    op(4, "est dans la fourchette", 2),
];

const YES_NO_OPS: &[OperatorInfo] = &[op(0, "est Oui", 0), op(1, "est Non", 0), op(2, "est Inconnu", 0)];

const MANGA_OPS: &[OperatorInfo] =
    &[op(0, "est Oui", 0), op(1, "est Oui (de droite à gauche)", 0), op(2, "est Non", 0), op(3, "est Inconnu", 0)];

const ON_OFF_OPS: &[OperatorInfo] = &[op(0, "Oui", 0), op(1, "Non", 0)];

const EXPRESSION_OPS: &[OperatorInfo] = &[op(0, "est Vrai", 1), op(1, "est Faux", 1)];

/// Un type de critère.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatcherInfo {
    /// `xsi:type` (nom de la classe ComicRack).
    pub kind: &'static str,
    /// Libellé français.
    pub label: &'static str,
    pub value: ValueKind,
    /// Unité de la valeur (« Mo » pour la taille du fichier).
    pub unit: Option<&'static str>,
    /// Statistique calculée sur toute la série (`SmartListSeries…`).
    pub series_stat: bool,
}

impl MatcherInfo {
    pub fn operators(&self) -> &'static [OperatorInfo] {
        self.value.operators()
    }

    pub fn operator(&self, op: i32) -> Option<&'static OperatorInfo> {
        self.operators().iter().find(|o| o.op == op)
    }

    /// ComicRust sait-il évaluer ce critère ?
    pub fn evaluated(&self) -> bool {
        crate::smart::is_supported(self.kind)
    }
}

const fn m(kind: &'static str, label: &'static str, value: ValueKind) -> MatcherInfo {
    MatcherInfo { kind, label, value, unit: None, series_stat: false }
}

const fn s(kind: &'static str, label: &'static str, value: ValueKind) -> MatcherInfo {
    MatcherInfo { kind, label, value, unit: None, series_stat: true }
}

use ValueKind::{CustomValue, Date, Expression, Manga, Number, OnOff, Text, YesNo};

static CATALOG: &[MatcherInfo] = &[
    // Texte
    m("ComicBookAllPropertiesMatcher", "Tout", Text),
    m("ComicBookSeriesMatcher", "Série", Text),
    m("ComicBookTitleMatcher", "Titre", Text),
    m("ComicBookAlternateSeriesMatcher", "Série alternative", Text),
    m("ComicBookSeriesGroupMatcher", "Groupes de la Série", Text),
    m("ComicBookStoryArcMatcher", "Arc narratif", Text),
    m("ComicBookWriterMatcher", "Scénariste", Text),
    m("ComicBookPencillerMatcher", "Dessinateur", Text),
    m("ComicBookInkerMatcher", "Encreur", Text),
    m("ComicBookColoristMatcher", "Coloriste", Text),
    m("ComicBookLettererMatcher", "Lettreur", Text),
    m("ComicBookCoverArtistMatcher", "Artiste de couverture", Text),
    m("ComicBookEditorMatcher", "Collection", Text),
    m("ComicBookTranslatorMatcher", "Traducteur", Text),
    m("ComicBookPublisherMatcher", "Éditeur", Text),
    m("ComicBookImprintMatcher", "Impression", Text),
    m("ComicBookGenreMatcher", "Genre", Text),
    m("ComicBookFormatMatcher", "Format", Text),
    m("ComicBookAgeRatingMatcher", "Limite d'âge", Text),
    m("ComicBookLanguageMatcher", "Langue", Text),
    m("ComicBookCharactersMatcher", "Personnages", Text),
    m("ComicBookTeamsMatcher", "Équipes", Text),
    m("ComicBookMainCharacterOrTeamMatcher", "Équipe/Personnage principal", Text),
    m("ComicBookLocationsMatcher", "Lieu(x)", Text),
    m("ComicBookTagsMatcher", "Étiquettes", Text),
    m("ComicBookSummaryMatcher", "Résumé", Text),
    m("ComicBookNotesMatcher", "Notes", Text),
    m("ComicBookReviewMatcher", "Analyse", Text),
    m("ComicBookWebMatcher", "Web", Text),
    m("ComicBookISBNMatcher", "ISBN", Text),
    m("ComicBookScanInformationMatcher", "Informations sur le scan", Text),
    m("ComicBookBookAgeMatcher", "Âge du Livre", Text),
    m("ComicBookBookConditionMatcher", "État du Livre", Text),
    m("ComicBookBookCollectionStatusMatcher", "État de la collection", Text),
    m("ComicBookBookStoreMatcher", "Librairie", Text),
    m("ComicBookBookOwnerMatcher", "Propriétaire du Livre", Text),
    m("ComicBookBookLocationMatcher", "Emplacement du Livre", Text),
    m("ComicBookBookNotesMatcher", "Annotations", Text),
    m("ComicBookFileMatcher", "Fichier", Text),
    m("ComicBookFullPathMatcher", "Chemin du fichier", Text),
    m("ComicBookDirectoryMatcher", "Dossier parent", Text),
    m("ComicBookFileFormatMatcher", "Format du fichier", Text),
    m("ComicBookCustomValuesMatcher", "Valeur personnalisée", CustomValue),
    // Nombres
    m("ComicBookNumberMatcher", "Numéro", Number),
    m("ComicBookCountMatcher", "Nombre de Livres", Number),
    m("ComicBookVolumeMatcher", "Volume", Number),
    m("ComicBookAlternateNumberMatcher", "Numéro alternatif", Number),
    m("ComicBookAlternateCountMatcher", "Décompte alternatif", Number),
    m("ComicBookYearMatcher", "Année", Number),
    m("ComicBookMonthMatcher", "Mois", Number),
    m("ComicBookDayMatcher", "Jour", Number),
    m("ComicBookWeekMatcher", "Semaine", Number),
    m("ComicBookPageCountMatcher", "Nombre de pages", Number),
    m("ComicBookReadPercentageMatcher", "Pourcentage de lecture", Number),
    m("ComicBookRatingMatcher", "Mon évaluation", Number),
    m("ComicBookCommunityRatingMatcher", "Évaluation communautaire", Number),
    m("ComicBookBookPriceMatcher", "Prix du Livre", Number),
    m("ComicBookBookmarkCountMatcher", "Nombre de signets", Number),
    m("ComicBookNewPagesMatcher", "Nouvelles pages", Number),
    MatcherInfo {
        kind: "ComicBookFileSizeMatcher",
        label: "Taille du fichier",
        value: Number,
        unit: Some("Mo"),
        series_stat: false,
    },
    // Dates
    m("ComicBookAddedMatcher", "Ajouté", Date),
    m("ComicBookOpenedMatcher", "Ouvert", Date),
    m("ComicBookPublishedMatcher", "Publié", Date),
    m("ComicBookReleasedMatcher", "Sorti", Date),
    m("ComicBookCreationMatcher", "Fichier créé le", Date),
    m("ComicBookModifiedMatcher", "Fichier modifié le", Date),
    // Oui / non
    m("ComicBookBlackAndWhiteMatcher", "Noir et Blanc", YesNo),
    m("ComicBookSeriesCompleteMatcher", "Série complète", YesNo),
    m("ComicBookCheckedMatcher", "Sélectionné", YesNo),
    m("ComicBookIsLinkedMatcher", "Lié à un fichier", YesNo),
    m("ComicBookIsMissingMatcher", "Est manquant", YesNo),
    m("ComicBookHasCustomValuesMatcher", "A des valeurs personnalisées", YesNo),
    m("ComicBookModifiedInfoMatcher", "Informations modifiées", YesNo),
    m("ComicBookModifiedLibraryInfoMatcher", "Informations bibliothèque modifiées", YesNo),
    m("ComicBookMangaMatcher", "Manga", Manga),
    m("ComicBookDuplicateMatcher", "Seulement les doublons", OnOff),
    // Statistiques de série
    s("SmartListSeriesAllCompleteMatcher", "Série : Complète", YesNo),
    s("SmartListSeriesCountMatcher", "Série : Numéros", Number),
    s("SmartListSeriesMinCountMatcher", "Série : Numéros minimum", Number),
    s("SmartListSeriesMaxCountMatcher", "Série : Numéros maximum", Number),
    s("SmartListSeriesFirstNumberMatcher", "Série : Premier numéro", Number),
    s("SmartListSeriesLastNumberMatcher", "Série : Dernier numéro", Number),
    s("SmartListSeriesGapsMatcher", "Série : Blancs", Number),
    s("SmartListSeriesMaxGapSizeMatcher", "Série : Plus grand blanc", Number),
    s("SmartListSeriesStartOfGapMatcher", "Série : Début du blanc", YesNo),
    s("SmartListSeriesEndOfGapMatcher", "Série : Fin du blanc", YesNo),
    s("SmartListSeriesMinYearMatcher", "Série : Première année", Number),
    s("SmartListSeriesMaxYearMatcher", "Série : Dernière année", Number),
    s("SmartListSeriesRunningTimeYearsMatcher", "Série : Durée de parution", Number),
    s("SmartListSeriesPageCountMatcher", "Série : Pages", Number),
    s("SmartListSeriesPagesReadMatcher", "Série : Pages lues", Number),
    s("SmartListSeriesPercentReadMatcher", "Série : Pourcentage lu", Number),
    s("SmartListSeriesAverageRatingMatcher", "Série : Évaluation moyenne personnelle", Number),
    s("SmartListSeriesAverageCommunityRatingMatcher", "Série : Évaluation moyenne de la communauté", Number),
    s("SmartListSeriesLastAddedTimeMatcher", "Série : Dernier Livre ajouté", Date),
    s("SmartListSeriesLastOpenedTimeMatcher", "Série : Dernière ouverture", Date),
    s("SmartListSeriesLastPublishedTimeMatcher", "Série : Publiée", Date),
    s("SmartListSeriesLastReleasedTimeMatcher", "Série : Livre sorti", Date),
    // Modules de ComicRack (scripts)
    m("ComicBookExpressionMatcher", "Expression", Expression),
];

/// Tous les types de critères connus, dans l'ordre de présentation.
pub fn matcher_catalog() -> &'static [MatcherInfo] {
    CATALOG
}

/// Informations sur un type de critère (`xsi:type`).
pub fn matcher_info(kind: &str) -> Option<&'static MatcherInfo> {
    CATALOG.iter().find(|i| i.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let mut seen = std::collections::HashSet::new();
        for i in matcher_catalog() {
            assert!(seen.insert(i.kind), "{} en double", i.kind);
            assert!(i.kind.ends_with("Matcher"));
            for (n, o) in i.operators().iter().enumerate() {
                assert_eq!(o.op, n as i32, "numéros consécutifs depuis 0 : {}", i.kind);
            }
        }
        let date = matcher_info("ComicBookAddedMatcher").unwrap();
        assert_eq!(date.operator(3).unwrap().unit, Some("jours"));
        assert_eq!(date.operator(4).unwrap().values, 2);
        assert_eq!(matcher_info("ComicBookReadPercentageMatcher").unwrap().operator(3).unwrap().values, 2);
        assert_eq!(matcher_info("ComicBookWriterMatcher").unwrap().operator(7).unwrap().label, "expression régulière");
        assert!(matcher_info("ComicBookSeriesMatcher").unwrap().evaluated());
        assert!(!matcher_info("SmartListSeriesCountMatcher").unwrap().evaluated());
    }

    /// Chaque critère évalué par `smart.rs` figure au catalogue.
    #[test]
    fn every_evaluated_kind_is_listed() {
        for kind in crate::smart::supported_kinds() {
            assert!(matcher_info(&kind).is_some(), "{kind} absent du catalogue");
        }
        let evaluated = matcher_catalog().iter().filter(|i| i.evaluated()).count();
        assert_eq!(evaluated, crate::smart::supported_kinds().len());
    }
}
