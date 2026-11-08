# Contrat IPC : onglet Pages (étape 4, `tauri-v0.5.0`)

Contrat entre le backend (`crates/comicrust-tauri/src/pages_livre.rs`, `edition.rs`, `lecteur.rs`)
et le front. Mêmes règles que [`ipc-bibliotheque.md`](ipc-bibliotheque.md) : commandes asynchrones
(`invoke`), promesse rejetée avec un **texte en français**, JSON en **camelCase**, **aucune logique
métier côté front** : déplacements, types, réinitialisations, tri et vérification sont calculés par
le backend (`comicrust_core::pages`, comme l'onglet « Pages » de la version egui
`ui/info_pages.rs`, lui-même calqué sur `PagesView` de ComicRack).

L'onglet « Pages » de la fenêtre « Infos du livre » n'existe que pour **un seul livre** ayant un
fichier (comme la version egui). En lecture seule (ComicRack sans écriture, `ComicDb.xml`) il sert
à la consultation (sélection, copie) sans modification.

## Désignation des pages

- `image` : rang de l'image **dans le fichier** (ordre naturel des noms : 2 avant 10), à partir de 0.
  C'est l'identifiant **stable** d'une page : il ne change pas quand l'ordre change. Sélection,
  opérations et enregistrement désignent les pages par leur `image`.
- `index` : rang dans l'**ordre de lecture** (liste complète, pages supprimées comprises), à partir
  de 0 ; recalculé par le backend après chaque opération (`pages[i].index === i`).

## Types

```ts
type TypePage = 'FrontCover' | 'InnerCover' | 'Roundup' | 'Story' | 'Advertisement' | 'Editorial'
              | 'Letters' | 'Preview' | 'BackCover' | 'Other' | 'Deleted';

interface Choix { valeur: string; libelle: string; raccourci?: string; }

interface PageLivre {
  index: number;        // rang de lecture (voir plus haut)
  image: number;        // rang de l'image dans le fichier : identifiant stable de la page
  nom: string;          // nom de l'image dans l'archive (« 012.jpg », « Page 012 » pour un PDF)
  type: string;         // valeur ComicRack : une des TypePage ; rarement des drapeaux combinés
                        //   (« FrontCover, Deleted »), à renvoyer tels quels
  libelleType: string;  // libellé français (« Couverture avant », « Supprimée »…), drapeaux joints
  supprimee: boolean;   // type « Deleted » (page masquée par le lecteur)
  double: boolean;      // page double, LECTURE SEULE : calculée comme ComicRack
                        //   (ComicPageInfo.IsDoublePage = largeur > hauteur connues),
                        //   ou attribut DoublePage du ComicInfo.xml du fichier
  rotation: number;     // 0 | 90 | 180 | 270 (à appliquer à l'affichage : la vignette n'est pas tournée)
  position: string;     // position en double page : 'Default' | 'Near' | 'Far'
  favori?: string;      // signet (Bookmark) ; absent = pas de favori
  largeur?: number;     // ImageWidth / ImageHeight de la fiche (taille d'origine, avant rotation) ;
  hauteur?: number;     //   absents si inconnus (la vraie taille arrive avec la vignette)
  urlVignette: string;  // `${urlBase}/${image}?l=${LARGEUR_VIGNETTE}` (protocole cr-page)
}

interface PagesLivre {
  id: string;
  jeton: string;        // séance de vignettes (protocole cr-page) : à fermer par fermer_pages
  urlBase: string;      // base des URL cr-page de la séance (comme LivreOuvert.urlBase)
  largeurVignette: number;   // 192 : largeur demandée par urlVignette
  nbImages: number;     // images du fichier = pages.length
  pages: PageLivre[];   // liste complète dans l'ordre de lecture ENREGISTRÉ (fiche), supprimées comprises
  modifiable: boolean;  // false : consultation seulement
  lectureSeule?: string;     // présent si !modifiable : la marche à suivre (même texte que lire_fiches)
  ecrireDansFichierPossible: boolean; // le fichier est un CBZ (ComicInfo.xml <Pages> écrit si demandé)
  types: Choix[];       // les 11 types, ordre du menu de ComicRack, raccourci « Alt+Maj+A »…« Alt+Maj+K »
  rotations: Choix[];   // valeur '0' | '90' | '180' | '270', raccourci « Alt+Maj+6 »…« Alt+Maj+9 »
  positions: Choix[];   // Default « Par défaut », Near « Proche », Far « Éloignée »
}
```

## Commandes

| Commande | Arguments | Résultat |
|---|---|---|
| `lire_pages` | `{id, remplace?: string}` | `PagesLivre`. Ouvre une séance de vignettes sur le fichier du livre (index de l'URL = `image`, ordre brut du fichier) et rend la liste complète d'après la fiche (complétée comme ComicRack : images manquantes ajoutées, couverture avant par défaut, doublons écartés). `remplace` : jeton d'une séance précédente à fermer (après « Appliquer »). Erreur : livre inconnu, sans fichier, fichier illisible, délai dépassé (40 s). |
| `fermer_pages` | `{jeton}` | `void`. Ferme la séance (pages retirées du cache). Jeton inconnu : sans effet. À appeler à la fermeture de la fenêtre ou au changement de livre (Précédent / Suivant). |
| `transformer_pages` | `{pages: PageLivre[], operation: OperationPages}` | `PageLivre[]` : la liste après l'opération (index recalculés, libellés à jour). Rien n'est enregistré. Erreur lisible si la liste est incohérente (image en double, type inconnu…). |

Une séance de vignettes compte parmi les 4 livres ouverts du lecteur (`ipc-lecteur.md`) mais
n'écrit **aucun avancement**. Vignettes : protocole `cr-page` (mêmes statuts, cache et règles que le
lecteur) ; une vignette (`l` ≤ 384) ne déclenche pas de préchargement.

```ts
type OperationPages =
  | { op: 'deplacer'; images: number[]; position: number } // glisser-déposer : insérer avant la place
                                                            //   `position` (rang AVANT déplacement,
                                                            //   pages.length = à la fin), ordre de la liste gardé
  | { op: 'mettreAuDebut'; images: number[] }               // Ctrl+Alt+T
  | { op: 'mettreALaFin'; images: number[] }                // Ctrl+Alt+B (couverture avant déplacée :
                                                            //   elle devient « Autre », la suivante devient couverture)
  | { op: 'type'; images: number[]; valeur: TypePage }      // Alt+Maj+A…K (remplace les drapeaux)
  | { op: 'basculerSupprimee'; images: number[] }           // Suppr : Supprimée ↔ Histoire
  | { op: 'rotation'; images: number[]; valeur: number }    // Alt+Maj+6…9 : 0 | 90 | 180 | 270
  | { op: 'position'; images: number[]; valeur: string }    // Default | Near | Far
  | { op: 'favori'; image: number; texte?: string }         // Ctrl+Maj+B ; texte vide/absent = supprimer (Ctrl+Maj+D)
  | { op: 'ordreOrigine'; images?: number[] }               // « Réinitialiser » / « Remettre dans l'ordre d'origine »
                                                            //   (images absentes : toutes) : ordre du fichier
  | { op: 'trierParNom'; numerique: boolean }               // « Trier par nom » (ordinal) / « (numérique) »
  | { op: 'reinitialiserTypes' }                            // 1re page « Couverture avant », les autres « Histoire »
  | { op: 'toutReinitialiser' };                            // ordre, types, rotation, position, favoris
```

`proposition_favori` n'existe pas : le texte proposé à la saisie d'un favori est le `favori` actuel,
sinon « Page n » (n = `index + 1`), comme ComicRack.

« Modifié » (● devant « Pages », bouton « Appliquer ») : le front compare la liste courante à celle
reçue de `lire_pages` (même suite de `image`, mêmes `type`, `rotation`, `position`, `favori`).

## Enregistrement : avec « OK » / « Appliquer » de la fenêtre Infos

Les pages s'enregistrent **avec les champs**, par `appliquer_modifications` (`ipc-edition.md`), en
une seule opération (comme la version egui) : `DemandeModification` reçoit un champ facultatif

```ts
interface DemandeModification {
  ids: string[];                 // un seul livre si `pages` est présent (sinon erreur)
  champs: ChampModifie[];        // peut être vide si seules les pages changent
  pages?: PageModifiee[];        // SEULEMENT si l'onglet Pages a été modifié : liste COMPLÈTE,
                                 //   dans le nouvel ordre de lecture
  ecrireDansFichier: boolean;
  confirme?: boolean;
}

interface PageModifiee {         // un PageLivre convient tel quel (champs en trop ignorés)
  image: number;
  type?: string;                 // absent : type enregistré gardé
  supprimee?: boolean;           // true : type « Deleted » ; false sur une page Deleted : « Story »
  rotation?: number;             // 0 | 90 | 180 | 270
  position?: string;             // Default | Near | Far
  favori?: string | null;        // '' ou null : supprimé ; absent : gardé
}
```

Règles :

1. `pages` doit contenir **chaque image du fichier une fois et une seule** (0 … nbImages − 1) ; sinon
   erreur, rien n'est écrit. Les attributs non modifiables (tailles, `ImageSize`, `Key`) sont repris de
   la fiche. `double` n'est pas modifiable (calculé, comme ComicRack).
2. Liste identique à la liste enregistrée (après normalisation) : rien n'est écrit pour les pages.
3. **Base** : autonome → colonne des pages de la base ComicRust ; ComicRack MySQL avec « Lecture et
   écriture » → élément `<Pages>` de la fiche réécrit en entier (`Value::Pages`, protocole des
   compteurs de `db/write.rs`, conflit signalé si la fiche a changé dans ComicRack) ; lecture seule →
   refusé (comme les champs).
4. **Fichier** : si `ecrireDansFichier` et le fichier est un CBZ, le `ComicInfo.xml` est réécrit
   d'après la fiche relue (élément `<Pages>` compris, au format ComicInfo : `Image`, `Type`,
   `Rotation`, `PagePosition`, `Bookmark`, tailles). Sans la case : seule la base change, la fiche est
   marquée « infos pas encore écrites dans le fichier » (`ComicInfoIsDirty`).
5. La couverture du livre est oubliée (nouvelle `urlCouverture ?v=n`) quand les pages changent.
6. Bilan : celui d'`appliquer_modifications` (`statut` du livre : `ok` / `inchange` / `erreur`).

Après « Appliquer » (fenêtre gardée), le front rappelle `lire_fiches` **et** `lire_pages({id,
remplace: ancienJeton})` : nouvelle séance, vignettes relues du fichier réécrit.

## Fichier libéré pendant les écritures (os error 5)

Comme la version egui (P-01 de la recette v0.10.7) : sous Windows, un fichier ouvert ne peut pas être
remplacé (« Accès refusé (os error 5) »). Garanties du backend :

- lecteur, vignettes (`cr-page`) et couvertures (`cr-couv`) n'ouvrent le fichier **que le temps
  d'une lecture** (`providers/on_demand.rs` du cœur) : aucun descripteur ne reste ouvert entre deux
  pages ;
- avant **toute** écriture d'un fichier (infos, pages, conversion avec rangement de l'original,
  renommage / déplacement), le backend **libère** ce fichier : les séances qui l'affichent (lecteur ou
  vignettes) lâchent leur fournisseur après la lecture en cours, leurs pages en cache sont oubliées,
  et toute nouvelle lecture de ce fichier (page, vignette, couverture) **attend la fin de
  l'écriture** (au plus 25 s, sinon 504 comme une page lente) ;
- après l'écriture, les séances **rechargent** le fichier (fichier réécrit avec les mêmes images :
  lecture normale ; images différentes : erreur 500 « rouvrir le livre » ; fichier renommé ou
  déplacé : le front rouvre avec le nouveau chemin).

Le front n'a rien à faire de plus que recharger les URL (nouvelle séance ou nouvelle page).

## Écarts avec la version egui (assumés)

- « Double » est montré (lecture seule) ; la version egui ne l'affiche pas, ComicRack le calcule
  (`IsDoublePage`) sans le rendre modifiable. Pour l'affichage en double page, `position` (Proche /
  Éloignée) comme le menu « Position de la page ».
- Copier la page (Ctrl+C) : le front copie l'image servie par `urlVignette` sans `l` (taille d'origine)
  ou, pour plusieurs pages, les lignes « chemin | nom » (fichier : `LivreEdite.fichier` de `lire_fiches`).
- « Rafraîchir » : le front recharge les URL (nouvelle séance par `lire_pages({id, remplace})`).
