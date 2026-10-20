# Recettes

Chaque version de ComicRust a son document de recette : la liste des tests manuels à réaliser
et à valider **avant de passer à l'étape suivante** du développement.

| Version | Document | Décision |
|---|---|---|
| v0.2.0 | [v0.2.0.md](v0.2.0.md) | en attente |

## Déroulé

1. La release est publiée en pré-release avec son document `recette/vX.Y.Z.md`
   (la CI refuse de publier une version sans recette, et la joint au zip sous `RECETTE.md`).
2. Le testeur coche les tests réussis, note les anomalies et remplit le bilan.
3. **GO** si tous les tests [B] sont OK (et les [M] en échec ont un correctif prévu) : on passe
   à l'étape suivante de la feuille de route. **NO-GO** : correctifs, nouvelle version `vX.Y.Z+1`
   avec sa propre recette (reprise des tests en échec + non-régression).
4. Le document rempli est enregistré dans le dépôt (commit) et la décision reportée dans le
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
