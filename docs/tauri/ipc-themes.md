# Contrat IPC : thèmes (étape 8, `tauri-v0.10.0`, lot A)

Source de vérité entre le backend (`crates/comicrust-tauri`, modules `themes` et `themes_css`) et le front
(`frontend/`). Mêmes règles que `ipc-bibliotheque.md` : commandes `invoke`, promesse rejetée avec un **texte
en français**, JSON en **camelCase**, **aucune logique métier côté front** (validation, assainissement et
choix des identifiants sont faits par le backend).

Les thèmes sont des fichiers `data\themes\<id>.css` (variables `--cr-*`, voir `themes.md`). Le CSS renvoyé au
front est **toujours assaini** : le front l'injecte tel quel (élément `<style>` rempli avec `textContent`).

## Résumé

| Commande | Arguments | Résultat |
| --- | --- | --- |
| `lister_themes` | aucun | `ThemeInfo[]` |
| `charger_theme` | `id: string` | `ThemeCharge` (**changement** : un objet, plus une chaîne) |
| `importer_theme` | `nom: string`, `css: string` | `ThemeInfo` |
| `exporter_theme` | `id: string` | `string` (CSS assaini, avec en-tête) |
| `supprimer_theme` | `id: string` | `void` |
| `reinitialiser_themes_livres` | aucun | `string[]` (identifiants remis à neuf) |
| `reveler_dossier_themes` | aucun | `void` |

## Types

```ts
type CouleursTheme = {
  fond: string;    // valeur CSS de --cr-fond, telle qu'écrite dans le :root (ex. "#f5f6f8")
  accent: string;  // --cr-accent
  texte: string;   // --cr-texte
};

type ThemeInfo = {
  id: string;        // nom de fichier sans .css : lettres, chiffres, "-" et "_" (64 max)
  nom: string;       // commentaire /* @nom: … */, à défaut l'id
  livre: boolean;    // vrai pour moderne-clair, moderne-sombre, classique
  couleurs: CouleursTheme | null; // lues dans le :root ; null si l'une des trois manque
  avertissements: string[];       // problèmes non bloquants (variable manquante, valeur vide, ligne supprimée…)
};

type ThemeCharge = {
  css: string;                // CSS assaini, prêt à injecter
  avertissements: string[];   // mêmes avertissements que ci-dessus
};
```

`couleurs` sert aux pastilles d'aperçu de la liste des thèmes (fond, accent, texte).
Un thème dont le fichier est illisible ou refusé par le filtrage reste listé (nom = id, `couleurs: null`,
un avertissement explique pourquoi) ; `charger_theme` échoue alors avec le motif.

## Commandes

### `lister_themes() -> ThemeInfo[]`

Installe d'abord les thèmes livrés manquants et met à jour silencieusement ceux qui sont identiques à une
ancienne version livrée (voir « Mise à jour des thèmes livrés »). Triés par nom affiché (sans casse), puis id.
Les fichiers qui ne sont pas `*.css` ou dont l'id est invalide (espaces, point, séparateur, nom réservé
Windows) sont ignorés.

### `charger_theme(id) -> ThemeCharge`

Échecs : id invalide, thème introuvable, fichier > 1 Mo, fichier refusé par le filtrage (NUL, construction
obfusquée… : message « thème refusé : … »). Les constructions interdites **simples** (`@import`, `url(http…)`,
`@font-face`…) sont retirées et signalées dans `avertissements`, le reste du thème est appliqué.

### `importer_theme(nom, css) -> ThemeInfo`

- Valide et assainit `css` (mêmes règles que le chargement). Si `erreurs` : promesse rejetée, rien n'est écrit.
- L'id est dérivé de `nom` (minuscules, accents retirés, suites de caractères non alphanumériques → `-`),
  `mon-theme` si le résultat est vide. S'il est pris (livré ou non, ou nom réservé Windows), le backend essaie
  `<id>-2`, `<id>-3`… : **un fichier existant n'est jamais écrasé**.
- Écrit `/* @nom: <nom> */` en tête si le CSS n'en a pas. Le fichier écrit est le CSS assaini.
- Le `ThemeInfo` renvoyé contient l'id attribué et les avertissements du filtrage : le front sélectionne ce thème
  et affiche les avertissements.

### `exporter_theme(id) -> string`

CSS assaini précédé d'un commentaire d'en-tête (nom, id, rappel de la procédure d'import). Le front le propose
en téléchargement / copie ; il ne touche pas au disque.

### `supprimer_theme(id) -> void`

Refusé pour un thème livré (message : « un thème livré ne se supprime pas : utilisez Réinitialiser »).
Introuvable : erreur. Si le thème supprimé était le thème actif, le front retombe sur `moderne-clair`
(défaut) et réenregistre le réglage.

### `reinitialiser_themes_livres() -> string[]`

Recopie les trois thèmes livrés. Un fichier installé **différent** de la version livrée est d'abord renommé
`<id>.css.avant` (`<id>.css.avant2`… si ce nom existe déjà) : rien n'est jamais perdu. Renvoie les id réellement
remis à neuf (fichier absent ou différent) ; liste vide si tout était déjà à jour. Le front recharge ensuite le
thème actif et la liste.

### `reveler_dossier_themes() -> void`

Crée `data\themes` au besoin et l'ouvre dans l'Explorateur (comme « ouvrir le dossier du fichier »).

## Rechargement à chaud

Pas d'événement : le front relit `lister_themes` + `charger_theme(idActif)` quand la fenêtre reprend le focus
(ou à la demande, bouton « Recharger »). Le contenu renvoyé est comparé côté front au précédent pour éviter un
reflux inutile.

## Mise à jour des thèmes livrés

Les trois CSS livrés commencent par `/* @contrat: 2 */`. Au démarrage (et à chaque `lister_themes`), un fichier
installé **strictement identique** (fins de ligne et espaces de fin ignorés) à une ancienne version livrée
connue est remplacé silencieusement par la nouvelle. Un thème modifié par l'utilisateur n'est jamais touché.
Le front n'a rien à faire ; il verra simplement le nouveau contenu au prochain chargement.

## Avertissements : formats

Chaînes courtes en français, affichables telles quelles :

- `variable obligatoire absente : --cr-bordure (repli du thème par défaut)`
- `valeur vide : --cr-accent`
- `variable inconnue : --cr-xyz`
- `color-scheme absent : les cases à cocher et barres de défilement natives peuvent mal s'afficher`
- `ligne 12 : url() supprimé (seules les images data: png, jpeg, webp, gif, avif ≤ 64 Ko sont admises)`
- `ligne 3 : @import supprimé`

## Erreurs possibles (texte du rejet)

`identifiant de thème invalide : « … »`, `thème introuvable : « … »`, `fichier de thème trop gros : « … »`,
`thème refusé : <motif>` (NUL, > 1 Mo, construction obfusquée, commentaire non terminé…),
`un thème livré ne se supprime pas : utilisez Réinitialiser`, `E/S : …`.

## Simulation (mode navigateur sans Tauri)

`frontend/src/lib/simule.ts` doit imiter ce contrat : `charger_theme` renvoie `{ css, avertissements: [] }`,
`lister_themes` des `ThemeInfo` complets, `importer_theme` / `supprimer_theme` sur une liste en mémoire.
