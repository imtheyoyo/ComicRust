# IPC — piles réordonnables (lot L1)

Réordonner les piles d'une liste de lecture affichée « Empiler par série », triée dans l'ordre de la
liste (tri `ordre` croissant). Même logique que les séries entières (`deplacer_groupes_liste`), mais
sans exiger un regroupement de la vue : une pile = une série (`titre_pile`), dans l'ordre de première
apparition de la série dans la vue. Les livres d'une série dispersée dans la liste sont regroupés au
dépôt.

## Commandes

| Commande | Arguments | Résultat |
| --- | --- | --- |
| `deplacer_piles_liste` | `id: string`, `requete: RequeteLivres`, `series: string[]`, `insertion: number` | `ResultatListes` |
| `decaler_pile_liste` | `id: string`, `requete: RequeteLivres`, `serie: string`, `haut: boolean` | `ResultatListes` |

- `insertion` : frontière entre piles (0 : avant la première pile, N : après la dernière).
- `series` : titres des piles déplacées (plusieurs piles non contiguës possibles) ; ces piles gardent
  leur ordre d'apparition dans la liste.
- `info` vide dans le résultat : déjà en place (rien d'écrit).
- `serie` et `groupe` de `requete` sont ignorés (comme `lister_piles`).

## Refus (messages renvoyés en erreur, rien d'écrit)

- Liste non affichée par la vue : « la vue n'affiche pas cette liste ».
- Pas une liste de lecture (dossier, intelligente, bibliothèque) : « « … » n'est pas une liste de lecture ».
- Tri autre que l'ordre de la liste croissant : `TRIER_POUR_REORDONNER`.
- Lecture seule / ComicRack lancé / hors ligne : messages habituels de `executer`.
- Pile absente : « aucune des piles demandées n'est affichée » / « pile « … » non affichée ».

## Front

- `api.ts` : `deplacerPilesListe`, `decalerPileListe` (bloc en fin de fichier).
- `GlisserLivres` : option `cellule: 'pile'` (survol des `[data-pile]`, chaque pile = un groupe).
- `VueBibliotheque.svelte` : `glisserPile` (trait d'insertion + badge, refus avec raison), menu des
  piles « ⬆ Monter / ⬇ Descendre la pile » (grisé avec la raison ; une seule pile sélectionnée),
  Alt+↑ / Alt+↓ dans `GrillePiles`. La sélection de piles est retrouvée par titre après rechargement.
- `simuleListes.ts` : simulation des deux commandes.
