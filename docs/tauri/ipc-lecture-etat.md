# IPC — « Marquer comme lu / non lu » et scripts du menu contextuel

Lot « menu du livre » (après la recette tauri-v0.11.0). Code : `crates/comicrust-tauri/src/lecture_etat.rs`
(logique, testée sans Tauri : `tests_lecture_etat.rs`) et `cmd_lecture_etat.rs` (commande fine).
Front : `VueBibliotheque.svelte` (menu contextuel), `lib/api.ts` (`marquerLecture`), simulé
`lib/simuleLectureEtat.ts`.

## Commande `marquer_lecture`

| Argument | Type | Sens |
|---|---|---|
| `ids` | `string[]` | Livres visés (sélection, ou livres de toutes les piles sélectionnées). Doublons et inconnus ignorés. |
| `lu` | `bool` | `true` : marquer lu ; `false` : marquer non lu. |

Réponse `BilanLecture` :

```json
{ "lu": true, "modifies": ["id1"], "inchanges": 1, "ignores": [{ "id": "id2", "raison": "nombre de pages inconnu" }] }
```

- Raisons d'`ignores` : « ouvert dans le lecteur » (la fermeture réécrirait la page courante), « nombre de
  pages inconnu », « absent de la base » (Store) /
  « absent de la base ComicRack », « modifié par ailleurs », erreur SQL, et « avancement local non mis à jour »
  (échec d'écriture de `avancement.json` : le livre est alors **à la fois** dans `modifies`, sa fiche est écrite,
  et dans `ignores`).
- `modifies` : livres dont la fiche a été écrite ; `inchanges` : déjà dans l'état demandé ;
  `ignores` : non modifiés (pages inconnues, conflit d'écriture ComicRack, absent de la base…).
- Erreur (`Err(String)`, message lisible) : fiches non modifiables, base hors ligne, analyse en cours,
  aucun livre connu dans la sélection.
- Si au moins un livre est modifié, l'événement **`bibliotheque-modifiee`** (`{ ids }`) est émis, comme
  après une édition : la vue se recharge sans perdre défilement ni sélection, et filtres « non lus »,
  listes intelligentes et panneau de détails se recalculent.

## Règles d'écriture (réutilise le mécanisme du lecteur)

| Mode | Effet |
|---|---|
| Autonome (base ComicRust) | `current_page` et `last_page_read` écrits (`Store::upsert` sur la fiche relue), compteur signalé à la surveillance. |
| ComicRack MySQL, « Lecture et écriture » | Seuls `CurrentPage` et `LastPageRead` réécrits (`BookEdit`, protocole des compteurs). |
| ComicRack MySQL en lecture seule, ComicDb.xml, aucune bibliothèque | **Refusé** avec la raison habituelle (`LECTURE_SEULE_*`) ; rien n'est écrit (ni base ComicRack, ni `avancement.json`). |

- `PageCount` est aussi écrit quand la fiche n'en a pas (nombre de pages pris de l'avancement local), comme
  le lecteur (`edition_lecture_comicrack`).
- Chemin autonome : fiche relue (`Store::get`) puis `Store::upsert`, et non `set_progress`, car celui-ci
  incrémente `OpenCount` et change `Opened` (ce n'est pas une ouverture) et n'écrit pas `PageCount`.
- **Lu** : `CurrentPage = LastPageRead = pages - 1` (100 % selon `LibraryBook::read_percent`), comme
  `ComicBook.MarkAsRead` de ComicRack ; **livre d'une seule page : `1`** (ComicRack écrit 1, car
  `LastPageRead = 0` vaut « non lu » : `ReadPercentage = LastPageRead <= 0 ? 0 : (LastPageRead + 1) × 100 /
  PageCount`, borné à 1–100). Avant la 1.0.2, ces livres étaient ignorés (« livre d'une seule page »). Le
  nombre de pages vient de la fiche (`PageCount`), à défaut de l'avancement local ; sinon le livre est ignoré.
  L'avancement local garde une position valide (page 0 pour un livre d'une page).
- **Non lu** : `CurrentPage = LastPageRead = 0` (`MarkAsNotRead`).
- `Opened` et `OpenCount` ne changent pas (ce n'est pas une ouverture). Écart connu : ComicRack, lui, met
  `OpenedCount` à 1 au moins et `OpenedTime` à maintenant dans `MarkAsRead`, et les remet à zéro dans
  `MarkAsNotRead` ; sans effet sur « lu / non lu ».
- L'avancement local (`data\avancement.json`) est aligné : fixé à « lu » (`Memoire::fixer`) ou **oublié**
  pour « non lu » (sinon le détail, qui garde le plus avancé de la fiche et de la mémoire locale,
  contredirait la fiche). La relecture en cours est effacée dans les deux cas.
- Choix de refuser en lecture seule : les pastilles et filtres lisent la fiche ; un avancement gardé
  seulement dans `avancement.json` ne s'y verrait pas.

## Menu contextuel du livre (et des piles)

- **Deux entrées**, sans bascule, comme ComicRack : `✔ Marquer comme lu` (`marquerLu`) et
  `↺ Marquer comme non lu` (`marquerNonLu`), juste sous « Lire ». Désactivées, avec la raison en
  infobulle **et écrite sous le libellé** (`aideVisible`), quand `manquants.raison` (relue à l'ouverture du menu) indique des fiches non modifiables. Pas de raccourci clavier
  (`lib/raccourcis.ts` inchangé).
- **Sous-menu « Scripts ▸ »** (lot menu ; avant : scripts au premier niveau en 1.0.0) : une entrée `scripts`
  (`sousMenu: EntreeMenu[]`, voir « Sous-menus génériques ») ouvre un panneau latéral avec une entrée
  `books:<id>` par script actif du hook `Books`, **sans limite** (le panneau défile), puis « Gérer les
  scripts… » (`scripts-gerer`) en bas.
  - `scripts.charger()` est lancé à l'ouverture du menu ; le sous-menu se met à jour à l'arrivée de la
    liste (« Chargement des scripts… » en attendant).
  - Indisponible (`raisonIndisponible` : Python absent, script en cours) : entrées désactivées, raison en
    infobulle. Erreur de chargement : « Scripts indisponibles » + « Gérer les scripts… ».
  - Aucun script : « Aucun script » (désactivé) + « Gérer les scripts… ».
- **Sous-menus génériques** (`MenuContextuel.svelte`) : `EntreeMenu.sousMenu` (un niveau). L'entrée porte
  `aria-haspopup="menu"`, `aria-expanded`, `aria-controls` ; le panneau est un `role="menu"` à côté du menu
  (à droite, à gauche si pas la place, dans la fenêtre, défile). Ouverture : survol (150 ms), clic, Entrée ou
  Flèche droite (focus sur la 1re entrée). Fermeture : Flèche gauche ou Échap (focus rendu à l'entrée ; le
  menu reste), survol d'une autre entrée ou sortie du pointeur après 400 ms (annulée si le pointeur rejoint le
  sous-menu : traversée en diagonale), clic ailleurs. « Ajouter à la liste… » est une fenêtre, pas un sous-menu.
- **Piles (liste de lecture, « Ordre de la liste »)** : « ⬆ Monter / ⬇ Descendre la pile » sont grisés, raison
  en infobulle et sous le libellé, quand ils sont sans effet : première pile (Monter), dernière (Descendre) ;
  plusieurs piles : bloc déjà en tête / en queue. Plusieurs piles sélectionnées : le bloc est décalé d'un cran
  (`deplacer_piles_liste`). Alt+↑ / Alt+↓ : même règle, la raison s'affiche brièvement (`info-lecture`).
- Le menu « 📜 Scripts ▾ » de la barre d'outils est inchangé.

## Simulé

`?lecture=lectureSeule` refuse l'écriture (message affiché dans la barre d'état) ;
`?manquants=lectureSeule` désactive les entrées ; `?scripts=vide` / `?scripts=sansPython` montrent les replis.
`window.__comicrustSimule.lectureEtat.appels` liste les appels reçus.

## Retours dans la barre d'état

Livres non modifiés : message groupé par raison (« Livres non modifiés — 3 : pages inconnues ; 2 : modifié par
ailleurs »). Tous déjà dans l'état demandé : « Déjà marqué comme lu / non lu. » pendant 3 s.
