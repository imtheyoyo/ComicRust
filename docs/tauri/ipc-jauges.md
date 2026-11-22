# Contrat IPC : jauges des listes (lot jauges)

Comme ComicRack, chaque élément du panneau des listes (dossiers, listes de lecture, listes
intelligentes) et l'entrée « Bibliothèque » affichent à droite **trois petits compteurs colorés** :
rouge = nouveaux livres, jaune = livres non lus, vert = total de livres. Backend :
`crates/comicrust-tauri/src/jauges.rs` (logique, testée sans Tauri) et `cmd_jauges.rs` (commande).
Mêmes règles que `ipc-listes.md` : commande asynchrone, rejet avec un texte en français, JSON en
camelCase, **aucune logique métier côté front**.

## Définitions

| Compte | Règle |
|---|---|
| **total** (vert) | livres de la liste présents dans la base : **exactement** le `nbLivres` de `arbre_listes` (livres manquants compris : leur fiche reste dans la base ; identifiants de liste absents de la base ignorés). |
| **non lus** (jaune) | livres dont la lecture est à moins de 100 % (`LibraryBook::read_percent() != 100`, comme la pastille « lu » de la vue et « Marquer comme lu / non lu »). Les livres en cours comptent donc comme non lus ; nombre de pages inconnu : non lu. |
| **nouveaux** (rouge) | livres non lus **jamais ouverts** (ni `Opened`, ni `OpenCount`, ni page lue) et **ajoutés depuis au plus 7 jours** (`Added`, constante `JOURS_NOUVEAU`). Sans date d'ajout : jamais nouveau. Un nouveau est toujours aussi compté dans les non lus. |

Remarque : ComicRack lui-même ne documente pas la règle exacte de « nouveau » dans les sources
dont nous disposons (`docs/INVENTAIRE_REA.md` ne cite que `AddedTime`, `OpenedTime`,
`OpenedCount`). La règle ci-dessus est l'interprétation retenue (jamais ouvert et ajouté
récemment) ; le seuil est une constante unique, facile à ajuster après comparaison avec ComicRack.

## Commande

| Commande | Arguments | Résultat |
|---|---|---|
| `jauges_listes` | – | `JaugesListes` |

```ts
interface Jauge { nouveaux: number; nonLus: number; total: number }
interface JaugesListes {
  racine: Jauge;                    // toute la bibliothèque (entrée « Bibliothèque »)
  listes: Record<string, Jauge>;    // par identifiant de liste (casse d'origine)
}
```

- Une liste dont le contenu est **inconnu** (critère non pris en charge, résultat non mémorisé par
  ComicRack ; `nbLivres` absent dans `arbre_listes`) n'a **pas** d'entrée : le front n'affiche
  aucune jauge (pas de faux zéro).
- Liste vide : `{ nouveaux: 0, nonLus: 0, total: 0 }`.
- Erreur « aucune bibliothèque ouverte » si aucune base n'est ouverte.

## Performance et cache

- **Une seule passe** : l'état (nouveau, non lu) de chaque livre est calculé une fois, puis
  chaque liste somme l'état de ses membres. Le contenu des listes intelligentes vient de
  `membres_toutes` (déjà résolu et mis en cache pour `arbre_listes` et l'affichage des listes) :
  il n'est **jamais réévalué par liste**.
- Résultat gardé dans le cache des listes (`CacheListes.jauges`) avec la clé
  `(génération de la base, révision des fiches, version des listes, jour)` : il est invalidé par
  tout changement de la base (scan, import, édition, « Marquer comme lu / non lu », lecture
  notée par le lecteur), des listes et au changement de jour (fenêtre « nouveau » de 7 jours).
  Deux appels sans changement renvoient le même résultat sans recalcul.

## Mise à jour côté front

Le store `jauges` (`frontend/src/lib/jauges.svelte.ts`) appelle `jauges_listes` après :
chargement de l'arbre des listes (`listes.charger`), tout événement `bibliotheque-modifiee`
(marquer lu / non lu, édition, scripts, surveillance externe, manquants), `scan-termine`,
fermeture du lecteur (progression notée) et import ; les demandes rapprochées sont regroupées
(300 ms). L'événement n'est pas nécessaire côté backend : aucun nouvel événement n'est émis.

## Réglage

`jaugesListes` (booléen, section `vue` de `comicrust-tauri.json`, absent = activé), case
« Afficher les jauges des listes » dans Réglages. Désactivé, le panneau retrouve le compteur
unique de total d'avant et la commande n'est plus appelée.

## Accessibilité et thème

- Chaque jauge porte un texte alternatif complet : « 12 nouveaux, 40 non lus, 120 livres »
  (singulier/pluriel accordé : « 1 nouveau, 0 non lu, 1 livre »), le détail visuel des trois
  chiffres étant masqué aux lecteurs d'écran.
- Variables de thème (contrat `docs/tauri/themes.md`) : `--cr-jauge-nouveaux-fond`,
  `--cr-jauge-nouveaux-texte`, `--cr-jauge-non-lus-fond`, `--cr-jauge-non-lus-texte`,
  `--cr-jauge-total-fond`, `--cr-jauge-total-texte` ; valeurs de repli dans `app.css` et valeurs
  propres à chacun des trois thèmes livrés (contraste texte/fond ≥ 4,5:1, vérifié par les tests
  de thèmes).
