# Thèmes de ComicRust (étape 8, `tauri-v0.10.0`)

Un thème est un fichier CSS de variables `--cr-*` dans `data\themes\`. Aucun code, aucune recompilation :
on copie un thème livré, on change les couleurs, il apparaît dans la liste. Le contrat IPC (commandes,
types) est dans `ipc-themes.md` ; ce document décrit le contrat de variables, le filtrage et les limites.

## Thèmes livrés

| Identifiant | Nom | Origine des couleurs |
| --- | --- | --- |
| `moderne-clair` (défaut) | Moderne clair | maquette « Moderne » de la version egui, variante claire |
| `moderne-sombre` | Moderne sombre | idem, variante sombre |
| `classique` | Classique | style « Classique » clair d'egui (`crates/comicrust/src/ui/theme.rs` : gris d'egui, accent orange) |

Les trois sont copiés au premier lancement si le fichier manque. Ils portent `/* @contrat: 2 */`.
Un thème livré **non modifié** et identique à une ancienne version livrée est remplacé silencieusement par la
nouvelle au démarrage ; un thème modifié n'est jamais touché. « Réinitialiser » recopie les trois (l'ancien
fichier modifié est renommé `<id>.css.avant`, jamais perdu).

## Contrat de variables

Obligatoires (O) : un thème doit les définir, sinon avertissement et repli sur le thème par défaut.
Facultatives (F) : on peut les omettre, le front applique le repli (indicatif ici : la source de vérité des
replis est `frontend/src/app.css`). La table du contrat est aussi dans le code :
`crates/comicrust-tauri/src/themes.rs`, constante `CONTRAT`. Un test parcourt `frontend/src` et refuse toute
variable `--cr-*` utilisée mais absente de `CONTRAT` : en ajouter une impose de l'ajouter ici et dans le code.

| Variable | Rôle | O/F | Repli |
| --- | --- | --- | --- |
| `--cr-fond` | Fond général de la fenêtre | O | |
| `--cr-fond-panneau` | Panneaux, cartes, dialogues et menus | O | |
| `--cr-fond-survol` | Survol d'une ligne ou d'un bouton | O | |
| `--cr-texte` | Texte courant | O | |
| `--cr-texte-discret` | Texte secondaire : légendes, aides, valeurs inactives | O | |
| `--cr-bordure` | Contours des champs, séparateurs, cadres | O | |
| `--cr-accent` | Couleur d'accent : sélection, boutons principaux, liens, progression | O | |
| `--cr-accent-texte` | Texte posé sur l'accent | O | |
| `--cr-erreur` | Erreurs et actions destructrices | O | |
| `--cr-rayon` | Rayon des angles | O | |
| `--cr-police` | Police de l'interface | O | |
| `--cr-ombre` | Ombre douce des cartes et menus | O | |
| `--cr-fond-lecteur` | Fond de la zone de lecture des pages | F | `var(--cr-fond)` |
| `--cr-fond-entete` | Fond des en-têtes de tableau et de panneau | F | `var(--cr-fond-panneau)` |
| `--cr-fond-selection` | Fond d'un élément sélectionné | F | `color-mix(in srgb, var(--cr-accent) 18%, var(--cr-fond-panneau))` |
| `--cr-texte-inverse` | Texte posé sur une image ou un fond sombre (badges sur couvertures) | F | `#ffffff` |
| `--cr-focus` | Anneau de focus clavier | F | `var(--cr-accent)` |
| `--cr-accent-survol` | Accent au survol d'un bouton principal | F | `color-mix(in srgb, var(--cr-accent) 85%, #000)` |
| `--cr-succes` | Succès : opération réussie, fichier présent | F | `#2e7d32` |
| `--cr-avertissement` | Avertissement : attention, valeur à vérifier | F | `#b26a00` |
| `--cr-etoile` | Étoiles de la note | F | `#f2b01e` |
| `--cr-rectangle-fond` | Fond du rectangle de sélection (glisser sur la grille) | F | `color-mix(in srgb, var(--cr-accent) 20%, transparent)` |
| `--cr-rectangle-bordure` | Contour du rectangle de sélection | F | `var(--cr-accent)` |
| `--cr-lecture-piste` | Piste vide des barres de lecture et de progression | F | `var(--cr-bordure)` |
| `--cr-ombre-forte` | Ombre marquée des fenêtres flottantes et du lecteur | F | `var(--cr-ombre)` |

En plus des variables : `color-scheme: light | dark;` dans le `:root`. Il commande l'apparence des contrôles
natifs (cases à cocher, barres de défilement, champs de date). Son absence produit un avertissement.

Les valeurs sont du CSS libre (couleurs `#rrggbb`, `rgb()`, `hsl()`, `color-mix()`, `var()`…). Seules les
valeurs **hexadécimales** des variables de couleur principales sont vérifiées par les tests de contraste (thèmes
livrés) ; pour un thème perso, c'est à son auteur de vérifier.

## Créer un thème

1. Réglages, Thèmes, **Ouvrir le dossier des thèmes** (ou `data\themes` dans l'Explorateur).
2. Copier `moderne-clair.css` (ou `moderne-sombre.css`) sous un autre nom : lettres, chiffres, `-` et `_`
   seulement, 64 caractères au plus, sans nom réservé Windows (`con`, `prn`, `aux`, `nul`, `com1`-`com9`,
   `lpt1`-`lpt9`).
3. Changer `/* @nom: … */` (le nom affiché) et les valeurs. Les facultatives peuvent être supprimées.
4. Le thème apparaît dans la liste (bouton « Recharger » ou retour dans la fenêtre) et s'applique en direct.

Autre voie : **Importer** (coller du CSS et donner un nom) : le thème est validé, reçoit un identifiant libre
(`mon-theme`, `mon-theme-2`…) et n'écrase jamais un fichier existant. **Exporter** produit le CSS assaini avec
un en-tête de commentaire, à partager tel quel.

Le thème peut aussi contenir des règles CSS ordinaires (ajustements fins), dans les limites du filtrage
ci-dessous ; les variables du contrat sont le seul comportement garanti stable d'une version à l'autre.

## Filtrage appliqué (sécurité)

Le CSS d'un thème peut venir d'Internet : il est assaini par le backend (`themes_css::valider_css`) avant
d'être renvoyé au front, au chargement comme à l'import. Aucune requête réseau ni exécution n'est possible
depuis un thème.

**Fichier refusé en entier** (erreur, rien n'est appliqué ni écrit) :

- octet NUL ; fichier de plus de **1 Mo** ;
- `</style` ou `<script` (sortie de l'élément `<style>`) ;
- commentaire, chaîne ou parenthèse non terminés (texte ambigu) ;
- **obfuscation** : une construction interdite qui n'apparaît qu'après décodage des échappements CSS
  (`\75rl(`, `u\72l(`, `@\69mport`, `b\65havior`…). En cas de doute, on rejette plutôt que réparer ;
- résultat d'assainissement instable (contrôle final : le résultat est ré-analysé, il ne doit plus rien
  contenir à retirer).

**Retiré et signalé** (le reste du thème est conservé ; avertissement « ligne N : … ») :

| Construction | Traitement |
| --- | --- |
| `@import`, `@namespace` | instruction supprimée |
| `@font-face` | bloc entier supprimé (y compris imbriqué dans `@media`) |
| `behavior:`, `-moz-binding:` | déclaration supprimée (`scroll-behavior:` reste valide) |
| `expression(`, `image-set(`, `image(`, `src(`, `cross-fade(` (préfixes `-webkit-` compris) | déclaration supprimée |
| toute `url(...)` | déclaration supprimée, **sauf** une image `data:` (voir ci-dessous) |

**Images `data:` admises** : `url("data:image/(png|jpeg|webp|gif|avif);base64,…")` (guillemets doubles,
simples ou aucun), partie base64 de **64 Ko au plus**, charge utile strictement en base64. Tout le reste est
supprimé : `data:image/svg+xml` (SVG : jamais), `data:text/html`, `http:`, `https:`, `//hôte`, chemins
relatifs, `javascript:`, `#id`…

Détail de la normalisation avant détection : commentaires retirés (`ur/**/l(` devient `url(` et est supprimé),
échappements décodés, casse ignorée, espaces et sauts de ligne internes tolérés. Les caractères de contrôle
(hors saut de ligne, retour chariot, tabulation) et le BOM sont ôtés.

Les commentaires sont conservés : `@nom` et `@contrat` en tête de fichier survivent même si l'instruction qui
les suivait est retirée.

## Avertissements (non bloquants)

Variable obligatoire absente ; valeur vide ; variable `--cr-*` inconnue ; `color-scheme` absent ; lignes
retirées par le filtrage. Ils sont affichés dans la liste des thèmes et à l'import ; le thème reste utilisable.

## Contraste (WCAG)

Les trois thèmes livrés sont testés (`themes::tests::contrastes_wcag_des_themes_livres`) sur leurs valeurs
hexadécimales :

| Paire | Minimum |
| --- | --- |
| `texte` / `fond`, `fond-panneau`, `fond-survol` | 4,5:1 |
| `texte-discret` / `fond`, `fond-panneau` | 4,5:1 |
| `accent-texte` / `accent` | 4,5:1 |
| `erreur`, `succes`, `avertissement` / `fond`, `fond-panneau` | 4,5:1 |
| `bordure` / `fond` | 3:1 |

Valeurs mesurées : Moderne clair 14,5 / 5,3 / 4,5 / 3,1 ; Moderne sombre 14,8 / 6,9 / 6,0 / 3,1 ;
Classique 10,4 / 6,5 / 4,7 / 3,2 (texte/fond, discret/fond, accent-texte/accent, bordure/fond).
Les bordures des thèmes livrés ont été **assombries** par rapport à la v0.9 (elles étaient autour de 1,3:1)
pour atteindre 3:1 ; l'accent du Classique (orange `#d66e28` d'egui, 3,4:1 avec du blanc) est assombri en
`#b85a14` pour porter du texte blanc.

## Limites connues

- Pas de rechargement automatique sur modification du fichier : le front relit au retour dans la fenêtre ou
  sur demande (pas de surveillance du disque).
- Un thème ne peut ni charger de police ni d'image externe (`@font-face`, `url()` externes retirés) : on utilise
  les polices installées sur le poste (`--cr-police`) et des images `data:` de 64 Ko au plus.
- Un `@import` sans `;` final emporte l'en-tête suivant (la règle qui suit est supprimée avec lui) : écrire le `;`.
- Le filtrage travaille sur des constructions connues ; il ne valide pas la syntaxe CSS générale. Un CSS
  invalide est simplement ignoré par le navigateur.
- Seul le `:root` fournit les couleurs d'aperçu de la liste (`fond`, `accent`, `texte` en hexadécimal ou valeur CSS littérale).
- Les contrastes ne sont pas vérifiés pour les thèmes de l'utilisateur, seulement pour les livrés.
- Mise à jour automatique des thèmes livrés : seules les anciennes versions listées dans
  `themes.rs` (`THEMES_ANCIENS`) sont reconnues ; à chaque nouvelle version d'un thème livré, y ajouter la précédente.
