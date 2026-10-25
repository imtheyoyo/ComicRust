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
| v0.5.1 | [v0.5.1.md](v0.5.1.md) — [page à cocher](https://claude.ai/artifact/QfPJazk15xis6XsNScquMW) — [rapport](rapports/v0.5.1-2026-10-22.txt) | **GO** (22/10/2026) : 16/18, bloquants OK ; D-01 (doublons 166/154) et E-06 (progression lue dans ComicRust) → v0.5.2 |
| v0.5.2 | [v0.5.2.md](v0.5.2.md) — [page à cocher](https://claude.ai/artifact/3B28PkHQxair4gh9yEc9bR) — [rapport](rapports/v0.5.2-2026-10-22.txt) | **GO** (22/10/2026) : 18/18 OK ; doublons 154/154, 46/47 listes identiques à ComicRack |
| v0.6.0 | [v0.6.0.md](v0.6.0.md) — [page à cocher](https://claude.ai/artifact/UKA82YSL9ATVfgRcFwnmkh) — rapports [1](rapports/v0.6.0-2026-10-22.txt), [2](rapports/v0.6.0-2026-10-23.txt) | **GO** (23/10/2026) : 28/28 OK ; écriture des infos, conversion CBZ, HEIF/AVIF et WebComic validés sur le NAS |
| v0.7.0 | [v0.7.0.md](v0.7.0.md) — [page à cocher](https://claude.ai/artifact/WGxsZ9FPK4rfLdTN3sHnjL) — [rapport](rapports/v0.7.0-2026-10-24.txt) | **GO** (24/10/2026) : 21/22 OK ; 4 941 livres analysés en 120 s ; A-03 (autonome + MySQL, fermeture) → v0.7.1 |
| v0.7.1 | [v0.7.1.md](v0.7.1.md) — [page à cocher](https://claude.ai/artifact/5vKobnHjkLK7GiM1rtNEGL) — [rapport](rapports/v0.7.1-2026-10-24.txt) | **NO-GO** (24/10/2026) : A-03, plantage localisé par le rapport → corrigé en v0.7.2 |
| v0.7.2 | [v0.7.2.md](v0.7.2.md) — [page à cocher](https://claude.ai/artifact/Q45DAwc61hJLLRe9ePuEU3) — [rapport](rapports/v0.7.2-2026-10-24.txt) | **GO** (24/10/2026) : 7/7 OK ; mot de passe en clair dans les réglages → v0.7.3 |
| v0.7.3 | [v0.7.3.md](v0.7.3.md) — [page à cocher](https://claude.ai/artifact/L6X8JvG2M2MuJPrXHPxbik) — [rapport](rapports/v0.7.3-2026-10-24.txt) | **GO** (24/10/2026) : 8/8 OK ; mot de passe chiffré vérifié |
| v0.8.0 | [v0.8.0.md](v0.8.0.md) — [page à cocher](https://claude.ai/artifact/LcCkUQpinxSQ8Pam8eXSvs) | **GO** (24/10/2026) : 17/17 OK ; 5 323 livres et 47 listes transférés, 0 doublon |
| v0.9.0 | [v0.9.0.md](v0.9.0.md) — [page à cocher](https://claude.ai/artifact/JWGKwvo1khckvqU7V1x7Sb) — [rapport](rapports/v0.9.0-2026-10-25.txt) | **GO** (25/10/2026) : 21/21 OK ; glisser-déposer des livres demandé → v0.9.1 |
| v0.9.1 | [v0.9.1.md](v0.9.1.md) — [page à cocher](https://claude.ai/artifact/FtSwg3kxQAoJhZDouYeuzg) | en recette |

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
dossier d'images, liste `Thanos.cbl` avec un numéro volontairement absent ; depuis la v0.6.0,
pages HEIF/AVIF (`Images HEIF AVIF.cbz`) et WebComic à scrapers et composition (`scraper/`).

## Rédiger la recette d'une nouvelle version

Partir de la recette précédente et :

- garder une section **0** « déjà vérifié automatiquement » (CI, tests, captures) et ce qui ne l'a pas été ;
- ajouter les tests des **nouveautés** de la version, en [B] pour la fonction principale ;
- garder en **non-régression** les tests [B] des versions précédentes ;
- mettre à jour les limites connues, le SHA-256 du zip et le bilan.
