# Journal des modifications

Toutes les versions de ComicRust, de la plus récente à la plus ancienne. Chaque section est
publiée telle quelle comme notes de la release GitHub correspondante (et affichée dans
ComicRust par « Nouveautés… » lors d'une mise à jour). Chaque version a sa recette dans
[`recette/`](recette).

## [v0.4.1] — 2026-10-21

Correctif de la v0.4.0 (recette NO-GO).

### Corrections
- **Livre en cours d'enregistrement par ComicRack** : quand ComicRack écrit ses informations
  dans un CBZ, il renomme brièvement le fichier en `….cbz.tmp`. ComicRust attend maintenant
  la fin de l'écriture (jusqu'à 20 s, message « ComicRack enregistre ce fichier, patience… »)
  au lieu d'afficher « fichier introuvable ». Les couvertures attendent de la même façon.
- **Suivi de la base plus sûr** : en plus des compteurs de ComicRack (toutes les 5 s), la liste
  complète des livres est comparée chaque minute ; tout livre ajouté, modifié ou retiré est
  rattrapé.
- **Fiche du livre** : libellés identiques à la fenêtre « Informations » de ComicRack
  (« Éditeur » = Publisher, « Collection » = Editor…), rubriques Détails, Artistes, Plus,
  Lecture, Fichier ; Année, Mois, Éditeur et Série complète toujours affichés.
- Colonne « Scénariste » du tableau (au lieu de « Scénario »).

### Recette
[recette/v0.4.1.md](recette/v0.4.1.md) — correctifs R-02, R-03, D-02 de la v0.4.0.
Les listes intelligentes (L-04) sont prévues en v0.5.0.

## [v0.4.0] — 2026-10-21

### Nouveautés
- **Suivi automatique des modifications faites dans ComicRack** : livre ajouté, modifié,
  supprimé ou lu dans ComicRack apparaît en quelques secondes, sans recharger (même méthode que
  ComicRack : compteurs de la table `changes`). Désactivable dans le menu ⚙.
- **Listes de ComicRack** (panneau 📂 Listes) : dossiers, listes de lecture et listes
  intelligentes, lues dans `ComicDb.xml` (trouvé d'après `ComicRack.ini`, ou choisi à la main).
  Une liste de lecture s'affiche dans son ordre, numérotée.
- **Panneau de détails** : fiche complète du livre sélectionné, avec « Lire (page N) » et
  « Depuis le début ».
- **Tri et regroupement au choix** (série, titre, parution, ajout, dernière lecture, note,
  pages, avancement, taille, chemin, ordre de la liste ; groupes par série, éditeur,
  scénariste, année, arc, genre, format, état de lecture), croissant ou décroissant.
- **Vue tableau** : une ligne par livre, tri d'un clic sur les titres de colonnes.
- **Couverture choisie dans ComicRack** (page « Couverture ») au lieu de la première page.
- Un clic sélectionne un livre ; double-clic ou Entrée l'ouvre.

### Recette
[recette/v0.4.0.md](recette/v0.4.0.md) — NO-GO (R-02), corrigé en v0.4.1.

## [v0.3.0] — 2026-10-21

### Nouveautés
- **Base MySQL de ComicRack, en lecture seule** : paramètres lus dans `ComicRack.ini`
  (le mot de passe n'est jamais recopié) ou saisis ; « Tester la connexion ».
- **Vue Bibliothèque** : couvertures (cache `data\couvertures`), recherche, regroupement par
  série, barre de lecture ; un clic ouvre le livre à la page où ComicRack l'avait laissé.
- **Listes `.cbl` résolues par l'identifiant** des livres dans la base, puis par la recherche
  dans un dossier.
- **Mise à jour intégrée** : nouvelles versions cherchées sur GitHub au démarrage, paquet
  vérifié (SHA-256) puis installé, redémarrage en un clic ; le dossier `data` n'est jamais
  touché.
- **Version exclusivement portable** : tout est dans `data\` à côté de l'exe ; les réglages de
  la version précédente sont repris.
- Code de l'application découpé en modules.

### Recette
[recette/v0.3.0.md](recette/v0.3.0.md) — GO (44/46 OK, 0 échec).

## [v0.2.1] — 2026-10-21

Correctif de la v0.2.0 (recette NO-GO).

### Corrections
- **Listes `.cbl` générées par ComicRack** : les livres sont retrouvés d'après leur
  `ComicInfo.xml` (série, numéro, volume), puis d'après le nom du fichier (numéro après `#`) ;
  la v0.2.0 prenait le dernier nombre du nom et ne trouvait rien.
- Cache de l'index des BD (`index-bd.json`) : les recherches suivantes sont quasi immédiates.
- Barre d'avancement pendant la recherche, bilan et « Copier le diagnostic ».

### Recette
[recette/v0.2.1.md](recette/v0.2.1.md) — GO (26/26 OK).

## [v0.2.0] — 2026-10-20

### Nouveautés
- **Lecteur** (egui) : page entière, largeur, hauteur, taille réelle, zoom, double page
  (couverture seule), lecture manga, plein écran, vignettes, navigation clavier et souris,
  reprise à la dernière page lue, fichiers récents, glisser-déposer.
- **Formats** : CBZ/ZIP, CBR/RAR (RAR4, RAR5, archives solides), CB7/7z, CBT/TAR, PDF
  (PDFium fourni), DjVu, CBW (web comics), dossiers d'images ; pages JPEG, PNG, GIF, BMP,
  TIFF, WebP, JPEG XL ; détection par signature (un ZIP renommé en `.cbr` s'ouvre).
- **Listes de lecture `.cbl`** : panneau de liste, livre suivant en fin de livre.
- Outil en ligne de commande `comicrust-cli` (info, render, cover, cbl, missing).

### Recette
[recette/v0.2.0.md](recette/v0.2.0.md) — NO-GO (C-06 : listes `.cbl`), corrigé en v0.2.1.

## [v0.1.0] — 2026-10-20

### Nouveautés
- Inventaire REA de ComicRack CE (`docs/INVENTAIRE_REA.md`) : structure, formats, base,
  correspondance .NET → Rust.
- Cœur `comicrust-core` : `ComicInfo.xml`, archives CBZ, listes `.cbl`, `ComicDB.xml`.
- Première version de l'outil en ligne de commande.
- Intégration continue (Windows + Linux) et release Windows 11 x64 automatique.
