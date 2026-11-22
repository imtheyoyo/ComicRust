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

- Raisons d'`ignores` : « ouvert dans le lecteur » (la fermeture réécrirait la page courante), « livre d'une
  seule page » (lu indiscernable de non lu), « nombre de pages inconnu », « absent de la base » (Store) /
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
- **Lu** : `CurrentPage = LastPageRead = pages - 1` (100 % selon `LibraryBook::read_percent`). Le nombre de
  pages vient de la fiche (`PageCount`), à défaut de l'avancement local ; sinon le livre est ignoré.
- **Non lu** : `CurrentPage = LastPageRead = 0`.
- `Opened` et `OpenCount` ne changent pas (ce n'est pas une ouverture).
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
- **Scripts au premier niveau** (plus de sous-menu « Scripts ▸ ») : séparateur, intitulé non interactif
  « Scripts » (`role="presentation"`, pas un menuitem), puis une entrée `books:<id>` par script actif du hook `Books`.
  - `scripts.charger()` est lancé à l'ouverture du menu, sans attendre (les entrées apparaissent à
    l'arrivée de la liste ; « Chargement des scripts… » en attendant).
  - Indisponible (`raisonIndisponible` : Python absent, script en cours) : entrées désactivées, raison en
    infobulle. Erreur de chargement : « Scripts indisponibles » + « Gérer les scripts… ».
  - Aucun script : « Aucun script » (désactivé) + « Gérer les scripts… ».
  - Plus de 12 scripts : les 12 premiers, puis « … et N autre(s) : gérer les scripts… » (`scripts-gerer`).
  - Le menu défile (`max-height`) s'il dépasse la fenêtre.
- Le menu « 📜 Scripts ▾ » de la barre d'outils est inchangé.

## Simulé

`?lecture=lectureSeule` refuse l'écriture (message affiché dans la barre d'état) ;
`?manquants=lectureSeule` désactive les entrées ; `?scripts=vide` / `?scripts=sansPython` montrent les replis.
`window.__comicrustSimule.lectureEtat.appels` liste les appels reçus.

## Retours dans la barre d'état

Livres non modifiés : message groupé par raison (« Livres non modifiés — 3 : pages inconnues ; 2 : modifié par
ailleurs »). Tous déjà dans l'état demandé : « Déjà marqué comme lu / non lu. » pendant 3 s.
