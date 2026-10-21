# Recettes

Chaque version de ComicRust a son document de recette : la liste des tests manuels à réaliser
et à valider **avant de passer à l'étape suivante** du développement.

| Version | Document | Décision |
|---|---|---|
| v0.2.0 | [v0.2.0.md](v0.2.0.md) — [page](https://claude.ai/artifact/WhQ9pNG5E5i4sbx4Kt9cPn) — rapports [1](rapports/v0.2.0-2026-10-21.txt), [2](rapports/v0.2.0-2026-10-21-b.txt) | **NO-GO** : C-06 en échec (listes .cbl) → corrigé en v0.2.1 |
| v0.2.1 | [v0.2.1.md](v0.2.1.md) — [page à cocher](https://claude.ai/artifact/A79VA2D8U3f494YCspRkmE) — [rapport](rapports/v0.2.1-2026-10-21.txt) | **GO** (21/10/2026) : 26/26 OK |
| v0.3.0 | [v0.3.0.md](v0.3.0.md) — [page à cocher](https://claude.ai/artifact/Ea1G6KJm9S2cm3Q79Gkkd5) — [rapports 1](rapports/v0.3.0-2026-10-21.txt), [2](rapports/v0.3.0-2026-10-21-b.txt) | **GO** (21/10/2026) : 44/46 OK, 0 échec (V-08 et N-08 non testés, non bloquants) |
| v0.4.0 | [v0.4.0.md](v0.4.0.md) — [page à cocher](https://claude.ai/artifact/UM6713Lkk2Nf3ccxgoRVsT) — [rapport](rapports/v0.4.0-2026-10-21.txt) | **NO-GO** : R-02 en échec (fichier en cours de réécriture par ComicRack) → corrigé en v0.4.1 avec R-03 et D-02 ; L-04 (listes intelligentes) → v0.5.0 |
| v0.4.1 | [v0.4.1.md](v0.4.1.md) — [page à cocher](https://claude.ai/artifact/EPiv8UMvedto36yVW3aDy9) — [rapport](rapports/v0.4.1-2026-10-21.txt) | **GO** (21/10/2026) : 13/14 OK, 0 échec (C-02 non testé) ; bug « Lire » et 2 améliorations → v0.5.0 |
| v0.5.0 | [v0.5.0.md](v0.5.0.md) — [page à cocher](https://claude.ai/artifact/EZEYDSV9tsp51VbZA8dmzt) — [rapport](rapports/v0.5.0-2026-10-22.txt) | **GO** (22/10/2026) : 27/27 OK ; 43/47 listes identiques à ComicRack, écart « Doublons » corrigé ensuite |
| v0.5.1 | [v0.5.1.md](v0.5.1.md) | en attente |

## Déroulé

1. La release est publiée en pré-release avec son document `recette/vX.Y.Z.md`
   (la CI refuse de publier une version sans recette, et la joint au zip sous `RECETTE.md`).
2. Une **page à cocher** est générée depuis le document
   (`python3 recette/outil/generer_page.py recette/vX.Y.Z.md recette-vX.Y.Z.html`) et publiée :
   boutons OK / Échec par test, note à l'échec, avancement, filtres, et « Copier le rapport »
   qui résume les échecs avec les notes. Les coches restent dans le navigateur du testeur.
   (On peut aussi cocher directement le Markdown.)
3. Le rapport copié est collé dans la conversation : il sert de base à la version suivante.
4. **GO** si tous les tests [B] sont OK (et les [M] en échec ont un correctif prévu) : on passe
   à l'étape suivante de la feuille de route. **NO-GO** : correctifs, nouvelle version `vX.Y.Z+1`
   avec sa propre recette (reprise des tests en échec + non-régression).
5. Le bilan est enregistré dans le dépôt (commit) et la décision reportée dans le
   tableau ci-dessus.

## Gravités

- **[B] bloquant** : fonction principale de la version ; un échec empêche le passage.
- **[M] majeur** : important ; un échec doit être corrigé dans la version suivante.
- **[m] mineur** : confort ; corrigé quand c'est possible.

## Échantillons

Le dossier [`echantillons/`](echantillons) contient des BD de test **générées** (aucune œuvre
réelle) dans chaque format, plus des cas limites : faux CBR (ZIP renommé), fichier tronqué,
dossier d'images, liste `Thanos.cbl` avec un numéro volontairement absent.

## Rédiger la recette d'une nouvelle version

Partir de la recette précédente et :

- garder une section **0** « déjà vérifié automatiquement » (CI, tests, captures) et ce qui ne l'a pas été ;
- ajouter les tests des **nouveautés** de la version, en [B] pour la fonction principale ;
- garder en **non-régression** les tests [B] des versions précédentes ;
- mettre à jour les limites connues, le SHA-256 du zip et le bilan.
