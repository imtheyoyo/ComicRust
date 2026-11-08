# Contrat IPC : lecteur (étape 2, `tauri-v0.3.0`)

Contrat entre le backend (`crates/comicrust-tauri/src/lecteur.rs`, `avancement.rs`) et le front.
Mêmes règles que [`ipc-bibliotheque.md`](ipc-bibliotheque.md) : commandes asynchrones (`invoke`),
promesse rejetée avec un **texte en français**, JSON en **camelCase**, aucune logique métier côté
front (l'ordre des pages, la reprise et l'avancement sont calculés par le backend).

## Index des pages

Une page est désignée par son **index = numéro de page du livre** (ComicRack : rang dans la liste
complète des pages, **pages supprimées comprises**, à partir de 0). C'est le numéro que ComicRack
enregistre (`CurrentPage`, `LastPageRead`) ; il sert dans l'URL des pages, `pageReprise` et
`enregistrer_avancement`.

`pages` est la **liste complète, dans l'ordre de lecture** de la fiche de la bibliothèque (à défaut,
du `ComicInfo.xml` du fichier, à défaut l'ordre naturel des noms : 2 avant 10) ; `pages[i].index === i`.
Comme la version egui à l'affichage :

- le front **masque** les pages `supprimee` (si toutes le sont, aucune n'est marquée) ;
- le front **applique** `rotation` : l'image servie n'est **pas** tournée ;
- une page `double` est affichée seule en mode double page.

## Types

```ts
interface PageInfo {
  index: number;       // numéro de page du livre (voir plus haut)
  image: number;       // image du fichier (rang dans l'ordre naturel des noms) ; ≠ index si réordonné
  largeur?: number;    // taille de l'image d'origine (avant rotation) si la fiche / le ComicInfo.xml
  hauteur?: number;    //   la donne ; sinon absente (la vraie taille arrive avec l'image)
  double: boolean;     // DoublePage du ComicInfo.xml, ou largeur > hauteur connue
  type?: string;       // type ComicRack (FrontCover, Other, Deleted…) ; absent = page d'histoire
  supprimee: boolean;  // page « Supprimée » : masquée par le lecteur
  rotation: number;    // 0 | 90 | 180 | 270, à appliquer à l'affichage
}

interface LivreOuvert {
  jeton: string;       // séance de lecture : URL des pages, avancement, fermeture
  id?: string;         // GUID du livre ; absent pour un fichier hors bibliothèque
  titre: string;       // titre, sinon « Série #n » ; hors bibliothèque : nom du fichier
  fichier: string;     // chemin complet (extension au contrat)
  format: string;      // « eComic (ZIP) », « Document PDF », « Dossier d'images »… (extension)
  nbPages: number;     // = pages.length (pages supprimées comprises)
  pageReprise: number; // index où reprendre (jamais une page supprimée : la suivante visible)
  pageRepriseComicrack?: number; // extension : présent si la page de ComicRack (base) diffère de
                                 // celle de ComicRust (mémoire locale) ; pageReprise = celle de
                                 // ComicRust. Le front peut demander (réglage egui « Demander »).
  manga: boolean;      // Manga = YesAndRightToLeft dans la fiche (ou le ComicInfo.xml hors bibliothèque)
  urlBase: string;     // base des URL de pages, sans barre finale (voir « Pages »)
  pages: PageInfo[];
}
```

## Commandes

| Commande | Arguments | Résultat |
|---|---|---|
| `ouvrir_livre` | `{id}` | `LivreOuvert`. Erreur si le livre est inconnu, sans fichier, illisible, ou si l'ouverture dépasse 40 s (NAS muet ; un fichier en cours de réécriture par ComicRack est attendu 20 s). |
| `ouvrir_fichier` | `{chemin}` | `LivreOuvert` pour un fichier ou un dossier d'images hors bibliothèque (guillemets autour tolérés). Si le fichier **est** dans la bibliothèque (même chemin, casse indifférente), sa fiche sert (ordre des pages, reprise, `id`), comme la version egui. Une **image seule** ouvre son dossier, à cette image. |
| `url_page_base` | `{jeton}` | `string` = `urlBase` de la séance ; erreur si le jeton est inconnu. |
| `enregistrer_avancement` | `{jeton, page, termine}` | `void`. `page` : index de la page affichée (borné à la dernière). `termine` : la dernière page a été vue. Voir « Avancement ». Erreur si le jeton est inconnu. |
| `fermer_livre` | `{jeton}` | `void`. Écrit la lecture dans la base (voir « Avancement »), arrête le fil du livre, retire ses pages du cache. Jeton inconnu : sans effet. |

Formats : CBZ/ZIP, CBR/RAR, CB7/7z, CBT/TAR, PDF (PDFium), DjVu, WebComic (`.cbw`), dossiers
d'images ; images JPEG, PNG, GIF, WebP, BMP, TIFF, JPEG XL, HEIF/HEIC, AVIF (celles du cœur).

Au plus **4 livres ouverts** : à l'ouverture d'un cinquième, le moins récemment utilisé est fermé
(lecture enregistrée) ; son jeton devient inconnu (404).

## Pages : protocole `cr-page`

```
Windows (WebView2) : http://cr-page.localhost/<jeton>/<index>?l=<largeurMax>
ailleurs           : cr-page://localhost/<jeton>/<index>?l=<largeurMax>
```

Le front construit l'URL avec `${urlBase}/${index}?l=${largeurMax}` (pas de reconstruction de l'hôte).

- `l` : largeur maximale en pixels (bornée à 16–16384) ; absent ou `0` = taille d'origine. L'image
  n'est **jamais agrandie**. Pour profiter du cache, demander la même largeur pour la page affichée
  et les pages préchargées (le front arrondit au multiple de 512).
- Réponse : JPEG, PNG, GIF, WebP ou BMP d'origine **tels quels** quand ils tiennent dans `l`
  (aucun réencodage) ; sinon image réduite et réencodée en **JPEG** (qualité 90), ou **PNG** si elle
  a de la transparence. HEIF/AVIF/JPEG XL/TIFF sont décodés côté Rust ; PDF et DjVu rendus côté Rust.
- Statuts : `200` (`Cache-Control: private, max-age=3600`), `404` jeton inconnu, page hors du livre
  ou adresse invalide, `500` fichier illisible / réécrit avec d'autres pages, `504` page pas prête
  après **25 s** (NAS muet ; le travail continue et remplit le cache s'il aboutit). Erreurs : texte,
  `no-store`.
- **Préchargement** : après chaque page servie, les 3 pages lues suivantes et la précédente (pages
  supprimées sautées) sont préparées en arrière-plan, à la même largeur. Les pages demandées passent
  toujours avant (la plus récente d'abord). Une **miniature** (`l` ≤ 384, bandeau du lecteur à
  `l=256`) ne déclenche pas de préchargement.
- **Cache mémoire borné** : 256 Mo et 48 pages au plus, tous livres confondus (le moins récemment
  utilisé part en premier) ; pages d'un livre retirées à sa fermeture.
- **Fichier jamais gardé ouvert** : les fournisseurs du cœur ouvrent le fichier le temps d'une
  lecture (`providers/on_demand.rs`) ; un livre affiché peut être réécrit, renommé ou déplacé sous
  Windows. Fichier réécrit (infos écrites par ComicRack…) : le livre est rouvert avant la lecture
  suivante (pages différentes : erreur 500, rouvrir le livre).
- **Écriture par ComicRust** (infos, pages, rangement de l'original converti, renommage) : le
  fichier est d'abord **libéré** (séances qui l'affichent arrêtées après leur lecture en cours,
  pages en cache oubliées, nouvelles lectures de ce fichier — pages, vignettes, couvertures —
  suspendues jusqu'à la fin de l'écriture, 25 s au plus), puis les séances le **rechargent**
  (`liberation.rs`, `Lecteur::liberer` ; voir `ipc-pages.md`). Corrige l'« os error 5 » possible sous
  Windows quand une lecture (préchargement, bandeau de miniatures) tombait pendant le remplacement.
- La CSP (`tauri.conf.json`) autorise ces origines dans `img-src` et `connect-src`.

## Avancement

Repris de la version egui (`remember_position`, `store_progress`) :

1. **Mémoire locale**, à chaque `enregistrer_avancement`, quel que soit le mode :
   `data\avancement.json` (`{ "<chemin du fichier>": {page, pageMax, pages} }`, équivalent de
   `last_pages` / `read_progress` de `comicrust.ron`). `pageMax` ne recule jamais.
2. **Base de ComicRust** (mode autonome, livre de la bibliothèque) : **une fois par séance**, à
   `fermer_livre`, à `enregistrer_avancement` avec `termine: true`, à l'éviction (5ᵉ livre) et à la
   fermeture de l'application : `CurrentPage` = page courante, `LastPageRead` = max(ancienne valeur,
   page la plus loin vue ; avec `termine`, la dernière page lue), `Opened` = maintenant,
   `OpenCount + 1`. La bibliothèque affichée (`lister_livres`, `detail_livre`) est mise à jour sans
   rechargement.
3. **Mode lié à ComicRack** : par défaut lecture seule, **rien n'est écrit dans ComicRack** ; seule
   la mémoire locale garde la lecture. Avec l'option « Lecture et écriture » (réglage
   `edition.ecritureComicrack`, base MySQL seulement, voir `ipc-edition.md`), la lecture est écrite
   dans la fiche ComicRack au même moment que ci-dessus (`CurrentPage`, `LastPageRead` jamais
   reculée, `Opened`, `OpenCount + 1`, `NewPages = 0`, `PageCount` s'il manque), comme la version
   egui ; une erreur d'écriture est rendue par `fermer_livre` (la lecture reste dans la mémoire locale).

**Reprise** à l'ouverture (`Start::Auto` de la version egui) : page de ComicRust (mémoire locale)
sinon page de la fiche (`CurrentPage` : base ComicRust en autonome, base ComicRack en mode lié) ;
si les deux existent et diffèrent, `pageReprise` = ComicRust et `pageRepriseComicrack` = ComicRack.
Une page de reprise supprimée donne la suivante visible.

Côté front, quand `pageRepriseComicrack` est présent, le lecteur suit le réglage `vue.lecteur.reprise`
(`demander` par défaut, `comicrack`, `comicrust`) ; « Demander » affiche la question de la version
egui (page de la bibliothèque, page de ce poste, depuis le début, « Toujours faire ce choix »,
Annuler).

## Réglages du lecteur

Section `vue.lecteur` de `data\comicrust-tauri.json`, envoyée **en entier** par
`enregistrer_preferences_vue` (le sous-objet remplace le précédent) et relue par
`lire_preferences_vue`. Lecture tolérante (`reglages.rs`) : `mode` ∈ {simple, double, defilement},
`ajustement` ∈ {meilleur, largeur, hauteur, reelle}, `couvertureSeule` / `manga` / `miniatures`
booléens, `grossissementLoupe` ramené dans 1,5–6, `reprise` ∈ {demander, comicrack, comicrust} ;
toute autre valeur est ignorée.

## Plein écran

`getCurrentWindow().setFullscreen()` (permission `core:window:allow-set-fullscreen` de
`capabilities/default.json`) ; repli sur le plein écran du document hors de Tauri.
