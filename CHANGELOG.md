# Journal des modifications

Toutes les versions de ComicRust, de la plus récente à la plus ancienne. Chaque section est
publiée telle quelle comme notes de la release GitHub correspondante (et affichée dans
ComicRust par « Nouveautés… » lors d'une mise à jour). Chaque version a sa recette dans
[`recette/`](recette).

## [tauri-v0.6.1] — 2026-11-10

Version corrective de l'étape 5 (**listes ComicRack**), issue d'essais pilotés par l'IA sur la
tauri-v0.6.0.

### Corrections
- **Éditeur de listes intelligentes** : changer le champ d'un critère ne provoque plus l'erreur
  « Aperçu impossible : missing field `genre` » (aperçu et enregistrement fonctionnent).
- **Série, numéro et volume déduits du nom de fichier** quand la fiche n'en a pas, comme ComicRack :
  affichage, tri et regroupement par série, rattachement des livres d'un `.cbl` (`Silver Surfer
  #3 ok.pdf`…) et export `.cbl`. La fiche reste prioritaire et rien de déduit n'est écrit dans les
  fichiers.
- **Focus clavier** : après un menu contextuel ou la fermeture d'une fenêtre (Échap), Suppr et F2
  répondent sans qu'il faille recliquer.
- « Ajouter à une liste… » propose la dernière liste utilisée ; la liste native de « Déplacer
  dans… » est lisible en thème sombre.

### Recette
[`recette/tauri-v0.6.1.md`](recette/tauri-v0.6.1.md) : correctifs, non-régression et tests de la
tauri-v0.6.0 à reprendre.

## [tauri-v0.6.0] — 2026-11-09

Étape 5 de la version **Tauri 2** (branche `tauri2-web`) : les **listes ComicRack**. Voir
[`docs/tauri/PLAN.md`](docs/tauri/PLAN.md) et le contrat
[`docs/tauri/ipc-listes.md`](docs/tauri/ipc-listes.md).

> ⚠ Avec « Lecture et écriture », cette version peut réécrire les listes de ComicRack
> (`ComicDb.xml`, ComicRack fermé) : sauvegarder `ComicDb.xml` et la base MySQL de ComicRack avant
> les essais.

### Nouveautés
- **Panneau des listes** sous la navigation : arbre **identique à ComicRack** (ordre, dossiers,
  types), nombre de livres calculé par ComicRust à côté de chaque liste, infobulle « ComicRust : n
  livres / ComicRack (dernier enregistrement) : m livres », « 📋 Copier le bilan des listes »
  (comparaison avec ComicRack, lignes différentes marquées « ≠ »). Clavier : ↑ ↓, Début / Fin,
  Pg préc. / suiv., → ← pour déplier / replier, Entrée, F2, Suppr, Maj+F10.
- **Vue d'une liste** : titre de la liste, tri « # Ordre de la liste » et colonne « # » pour une
  liste de lecture ; retour à « ▦ Bibliothèque » avec son propre tri.
- **Listes de lecture** : ajout de livres par **glisser-déposer** sur l'arbre ou par « ➕ Ajouter …
  à une liste… » (ou dans une nouvelle liste) ; retrait par **Suppr** (aucun fichier supprimé) ;
  « ⬆ Monter / ⬇ Descendre dans la liste » ; réordonnancement **au glisser** (couvertures et
  tableau) ; **séries entières** déplacées par leur en-tête (« ⬆ Monter / ⬇ Descendre la série »).
- **Gestion de l'arbre** : nouvelle liste de lecture, nouvelle liste intelligente, nouveau dossier,
  Renommer (F2), « ✏ Modifier… » (description, combinaison d'un dossier), « ⎘ Dupliquer »,
  « 🗑 Supprimer… » (sous-listes et listes intelligentes dépendantes signalées), « 📄 Convertir en
  liste de lecture… », « Déplacer dans… », glisser-déposer des listes, « ⬆ Monter / ⬇ Descendre ».
- **Éditeur de listes intelligentes** (comme ComicRack et la version egui) : critères et opérateurs
  du catalogue de ComicRack, groupes imbriqués, case **Non**, liste de départ (« pas dans cette
  liste »), limite (livres, Mo, Go ; au hasard, par position, par série), aperçu du nombre de livres,
  **OK / Appliquer** (Ctrl+Entrée) ; consultation seule en lecture seule.
- **Écriture de `ComicDb.xml`** avec « Lecture et écriture », **jamais ComicRack lancé** (vérifié
  avant l'opération et juste avant d'écrire) : seul `<ComicLists>` est remplacé, une liste non
  modifiée reste identique à l'octet ; sauvegardes `ComicDb.comicrust-origine-….xml.bak` (la
  première, gardée) et `ComicDb.comicrust-….xml.bak` (10 dernières) ; arbre relu tout seul quand
  ComicRack réécrit le fichier. En autonome, les listes sont dans la base de ComicRust.
- **Import / export `.cbl`** (nouveau par rapport à la version egui) : analyse avant import
  (trouvés par identifiant ou par série et numéro, numéros manquants, bilan copiable), création
  d'une liste de lecture dans l'ordre du fichier ; export au format ComicRack, possible en lecture
  seule.

### Corrections
- Recette tauri-v0.5.0 D-01 : Échap ne ferme plus « Renommer / déplacer » quand le modèle de nom ou
  le dossier cible a été modifié (Annuler pour fermer) ; même règle pour « Ouvrir un fichier… »,
  « Dossiers surveillés » et les fenêtres des listes.

### Recette
[recette/tauri-v0.6.0.md](recette/tauri-v0.6.0.md)

## [tauri-v0.5.0] — 2026-11-08

Étape 4 de la version **Tauri 2** (branche `tauri2-web`) : les **pages**. Voir
[`docs/tauri/PLAN.md`](docs/tauri/PLAN.md) et le contrat
[`docs/tauri/ipc-pages.md`](docs/tauri/ipc-pages.md).

> ⚠ Cette version réécrit l'ordre et le type des pages (base, et sur demande fichier CBZ et base
> de ComicRack) : faire une sauvegarde de la base ComicRack avant les essais en « Lecture et écriture ».

### Nouveautés
- Onglet **« Pages »** dans la fenêtre « Infos du livre » (un seul livre), repris de la version egui
  et de ComicRack : **vignettes** (trois tailles, Ctrl+molette) ou liste **détaillée** ; sélection
  multiple (clic, Ctrl, Maj, flèches, Ctrl+A) ; **glisser-déposer** avec trait d'insertion ;
  ◀ ▶ (Alt+← / Alt+→), **Au début / À la fin** (Ctrl+Alt+T / B).
- **Types de page** (liste, menu, Alt+Maj+A…K), **rotation** (Alt+Maj+6…9), **position en double
  page** (Proche / Éloignée), **Supprimer / Restaurer** (Suppr), **favori** (Ctrl+Maj+B / D),
  **aperçu agrandi** (Entrée, double-clic), **copie** de la page (Ctrl+C), **Réinitialiser ▾**
  (ordre d'origine, tri par nom, types, tout). Page double **⧉** affichée en lecture seule,
  calculée comme ComicRack.
- Les pages s'enregistrent **avec les champs** (OK / Appliquer, Précédent / Suivant) : base de
  ComicRust ; base MySQL de ComicRack en « Lecture et écriture » (sans écraser une modification
  faite dans ComicRack) ; élément `<Pages>` du `ComicInfo.xml` si « Écrire aussi les infos dans le
  fichier » est cochée (sinon : « infos pas encore écrites dans le fichier »). Le lecteur suit
  l'ordre, les suppressions et la rotation.
- Consultation seule de l'onglet en lecture seule (ComicRack sans écriture, `ComicDb.xml`).

### Corrections
- Recette tauri-v0.4.0 I-03 : Échap appuyé plusieurs fois ne ferme plus de force la fenêtre
  « Infos » (qui ne se rouvrait plus) ; même correctif pour Renommer, Convertir / Écrire les infos,
  Réglages, Dossiers surveillés et « Ouvrir un fichier ».
- Fichier **libéré avant toute écriture** (infos, pages, rangement de l'original, renommage) puis
  rechargé : lecteur, vignettes et couvertures ne gardent plus le fichier ouvert pendant son
  remplacement (« Accès refusé (os error 5) » sur le NAS, recette egui v0.10.6).

### Recette
[recette/tauri-v0.5.0.md](recette/tauri-v0.5.0.md)

## [tauri-v0.4.0] — 2026-11-07

Étape 3 de la version **Tauri 2** (branche `tauri2-web`) : **infos et édition**. Voir
[`docs/tauri/PLAN.md`](docs/tauri/PLAN.md) et le contrat
[`docs/tauri/ipc-edition.md`](docs/tauri/ipc-edition.md).

> ⚠ Première version Tauri qui modifie des fichiers BD et, sur demande, la base de ComicRack :
> faire une sauvegarde de la base ComicRack avant d'activer « Lecture et écriture ».

### Nouveautés
- **Sélection multiple** dans la grille et le tableau : Ctrl+clic, Maj+clic, Ctrl+Maj+clic,
  Maj+flèches, **Ctrl+A** ; nombre de livres sélectionnés dans la barre d'état et le panneau de détails.
- **Menu contextuel** (clic droit, touche Menu, Maj+F10) : Lire, Modifier les infos (Ctrl+I), Écrire
  les infos dans le fichier, Convertir en CBZ, Renommer / déplacer, **Ouvrir le dossier du fichier**
  (le fichier sélectionné dans l'Explorateur).
- Fenêtre **« Infos du livre »** (Ctrl+I, menu, bouton « ✏ Modifier » du panneau) reprise de la
  version egui : onglets Résumé, Détails, Intrigue et notes, Catalogue, Personnalisé ;
  **autocomplétion** avec les valeurs de la bibliothèque ; **validation** pendant la saisie ;
  ↺ pour revenir à la valeur enregistrée ; OK / Appliquer / Annuler ; **Précédent / Suivant**.
- **Plusieurs livres** à la fois : « (valeurs multiples) » quand les livres diffèrent, **seuls les
  champs touchés sont écrits**, ✖ pour vider un champ pour tous ; progression, Arrêter, bilan par
  livre et « Copier le rapport ».
- **Écriture du `ComicInfo.xml`** dans les CBZ (case « Écrire aussi les infos dans le fichier » ou
  menu « Écrire les infos dans le fichier ») : archive réécrite sans recompression puis remplacée
  d'un coup, nouvelles tentatives si le fichier est momentanément refusé (NAS, antivirus).
- **Conversion CBR / CB7 / CBT → CBZ**, original laissé en place ou rangé dans `_originaux`.
- **Renommer / déplacer** d'après un modèle de nom (modèles de ComicRack, sous-dossiers), avec
  **aperçu** ; collisions refusées, jamais d'écrasement ; la fiche suit le fichier.
- **Garde-fou** : au-delà de 100 livres, une opération demande confirmation.
- **Mode lié ComicRack** : **lecture seule par défaut** (rien n'est écrit dans ComicRack ; fichiers
  seulement) ; nouvelle option **« Lecture et écriture »** (⚙ Réglages, avec avertissement) : fiches,
  fichiers convertis ou renommés et **avancement de lecture** écrits dans la base MySQL de ComicRack,
  seuls les éléments touchés de la fiche, sans écraser une modification faite dans ComicRack.
  `ComicDb.xml` reste en lecture seule pour les fiches.

### Corrections
- Recette tauri-v0.3.0 E-03 : la miniature d'une page tournée reste dans son cadre dans le bandeau
  des miniatures.

### Recette
[recette/tauri-v0.4.0.md](recette/tauri-v0.4.0.md)

## [tauri-v0.3.0] — 2026-11-03

Étape 2 de la version **Tauri 2** (branche `tauri2-web`) : le **lecteur**. Voir
[`docs/tauri/PLAN.md`](docs/tauri/PLAN.md) et le contrat
[`docs/tauri/ipc-lecteur.md`](docs/tauri/ipc-lecteur.md).

### Nouveautés
- **Lecteur plein écran** ouvert depuis la bibliothèque (double-clic, Entrée, bouton « Lire ») ou
  par **« Ouvrir un fichier… »** (chemin d'une BD, d'un dossier d'images ou d'une image ; un
  fichier de la bibliothèque y garde sa fiche). Échap ou Ctrl+W ferment le lecteur et rendent la
  main à la liste.
- **Formats** : CBZ/ZIP, CBR/RAR, CB7/7z, CBT/TAR, PDF, DjVu, WebComic (`.cbw`), dossiers d'images ;
  pages JPEG, PNG, GIF, WebP, BMP, TIFF, JPEG XL, HEIF/HEIC et AVIF.
- **Modes** : page simple, **double page** (couverture seule, pages doubles et paysage seules),
  **défilement vertical continu** (touche V, nouveau par rapport à la version egui), sens
  **manga** (M, ou automatique d'après la fiche).
- **Ajustements** page entière / largeur / hauteur / taille réelle (1 à 4), **zoom** (+ − 0,
  Ctrl+molette autour du pointeur, glisser quand la page déborde), **loupe** (L), **plein écran**
  (F, F11).
- **Clavier et souris** comme la version egui : flèches, Espace, Page suiv./préc., Début, Fin,
  clic sur les tiers de la page, clic droit, molette ; barres masquées automatiquement ;
  **miniatures** (T) et curseur de pages.
- **Ordre, pages supprimées et rotation** de la fiche (ComicRack ou ComicRust), à défaut ordre du
  `ComicInfo.xml` du fichier.
- **Préchargement** des pages suivantes et de la précédente, cache mémoire borné ; le fichier
  n'est **jamais gardé ouvert** (on peut le renommer ou le déplacer pendant la lecture) ; NAS muet :
  message par page, pas de gel.
- **Reprise de lecture** à la dernière page lue ; en mode lié, fenêtre **« Reprendre la
  lecture »** quand ComicRack et ce poste n'ont pas retenu la même page (« Toujours faire ce
  choix » possible).
- **Avancement** : en mode autonome, écrit dans la base de ComicRust **une fois par séance**
  (page, page la plus loin, date et nombre d'ouvertures) ; en mode lié, mémorisé seulement dans
  `data\avancement.json` (**rien n'est écrit dans ComicRack** avant l'étape 6).
- **Réglages du lecteur mémorisés** (mode, ajustement, couverture seule, miniatures, loupe, manga,
  choix de reprise) dans `data\comicrust-tauri.json` (`vue.lecteur`).

### Recette
[recette/tauri-v0.3.0.md](recette/tauri-v0.3.0.md)

## [tauri-v0.2.1] — 2026-11-02

Correctifs de la tauri-v0.2.0 (recette NO-GO, [rapport](recette/rapports/tauri-v0.2.0-2026-11-02.txt)).

### Corrections
- **Recherche fidèle à ComicRack** (R-01 : « thanos » donnait 130 livres contre 483 dans
  ComicRack) : la zone de recherche reproduit la recherche rapide de ComicRack en mode « Tout ».
  Elle porte sur tous les champs (dont résumé, notes, personnages, lieux, groupe de séries, infos
  de scan…), tous les mots doivent se trouver dans un même champ, la virgule ou le point-virgule
  séparent les termes (sinon les espaces), la casse est ignorée mais pas les accents, comme dans
  ComicRack. Une recherche à plusieurs mots peut donc donner moins de résultats qu'avant.
- **Préférences d'affichage mémorisées** (R-08, G-03) : tri, sens, groupe, vue grille/tableau,
  taille des couvertures, panneau de détails (affiché, largeur) et thème sont enregistrés dans
  `data\comicrust-tauri.json` (section `vue`) et repris au redémarrage ; plus rien dans le stockage
  local du WebView2 (les anciens réglages sont repris une fois). Valeur inconnue ou fausse dans le
  fichier : ignorée sans erreur.
- **Panneau de détails redimensionnable** : nouvelle poignée (souris ou flèches au clavier).
- D-05 : le message « aucun livre… indique son ComicRack.ini » sur le `ComicDb.xml` d'une
  installation MySQL est le comportement attendu ; le test est reformulé.

### Recette
[recette/tauri-v0.2.1.md](recette/tauri-v0.2.1.md)

## [tauri-v0.2.0] — 2026-11-02

Étape 1 de la version **Tauri 2** (branche `tauri2-web`) : la **vue Bibliothèque**. Pas encore de
lecteur. Voir [`docs/tauri/PLAN.md`](docs/tauri/PLAN.md) et le contrat
[`docs/tauri/ipc-bibliotheque.md`](docs/tauri/ipc-bibliotheque.md).

### Nouveautés
- **Premier démarrage** : assistant au premier lancement, au choix **bibliothèque autonome**
  (base SQLite `data\comicrust.db`) ou **liée à ComicRack** (`ComicRack.ini` / MySQL ou
  `ComicDb.xml`, en lecture seule : rien n'est copié ni écrit dans ComicRack). Le choix est
  mémorisé dans `data\comicrust-tauri.json`.
- **Vue Bibliothèque** : grille de couvertures virtualisée et tableau, taille des couvertures
  réglable, recherche en direct (tous les mots requis), tri (11 critères, ordre naturel des
  numéros, valeurs vides en dernier), groupement par série ou éditeur, panneau de détails,
  navigation au clavier (flèches, Début/Fin, pages, Entrée, `/` et Ctrl+F) et à la souris.
- **Dossiers surveillés** (mode autonome) : ajout et retrait, scan avec progression, livres
  ajoutés, mis à jour ou **manquants** (conservés dans la liste).
- **Couvertures** : miniatures en cache dans `data\couvertures`, servies par le protocole
  `cr-couv` (aucune image en base64 dans l'IPC), visuel de remplacement si illisible.
- Volume : 5 000 livres restent fluides (défilement, recherche, tri).
- Les thèmes de la tauri-v0.1.0 s'appliquent à toute la vue ; le mode portable est conservé
  (rien hors de `data\`).

### Corrections
- Aucune : première version de cette vue. Limites connues : pas de lecteur, pas de sélection
  multiple, groupes sans vraies sections, chemins saisis à la main.

### Recette
[recette/tauri-v0.2.0.md](recette/tauri-v0.2.0.md)

## [tauri-v0.1.0] — 2026-11-02

Squelette de la version **Tauri 2** (branche `tauri2-web`), parallèle à la version egui : aucune
fonction de ComicRust n'est encore portée. Voir [`docs/tauri/PLAN.md`](docs/tauri/PLAN.md).

### Nouveautés
- Fenêtre Tauri 2 (WebView2) avec un front Svelte + TypeScript ; la logique métier reste dans
  `comicrust-core`.
- **Toujours portable** : données dans `data\` à côté de l'exécutable, profil WebView2 dans
  `data\webview2`, rien dans `%APPDATA%` ni dans le registre.
- **Thèmes** : fichiers CSS à variables dans `data\themes\` (Moderne clair, Moderne sombre et
  Classique livrés) ; on en ajoute sans recompiler.
- Paquet : zip portable, sans installateur, avec empreinte SHA-256.

### Recette
[recette/tauri-v0.1.0.md](recette/tauri-v0.1.0.md)

## [v0.10.7] — 2026-11-01

Correctifs de la v0.10.6 (recette NO-GO).

### Corrections
- **Onglet Pages, « Appliquer » avec écriture dans le fichier** : l'erreur « Accès refusé (os
  error 5) » est corrigée. ComicRust gardait lui-même le CBZ ouvert, pour les vignettes et le
  lecteur. Désormais :
  - les fichiers CBZ, CBT et PDF ne sont ouverts que le temps d'une lecture ;
  - les vignettes sont libérées avant l'écriture, puis rechargées ;
  - le lecteur rouvre un livre réécrit ;
  - en cas d'accès refusé passager (antivirus, partage réseau), l'écriture est retentée
    pendant environ 3 s.
- **Bedetheque Scraper 2** : une page normale du site n'est plus prise pour une page
  anti-robot, même si Cloudflare y ajoute un script. Seules les vraies pages de défi
  déclenchent la proposition d'installer cloudscraper.

### Recette
[recette/v0.10.7.md](recette/v0.10.7.md).

## [v0.10.6] — 2026-11-01

### Nouveautés
- **Onglet « Pages » de la fenêtre « Infos du livre »**, comme dans ComicRack :
  - vignettes numérotées, sélection multiple, glisser-déposer pour changer l'ordre ;
  - menu contextuel : Type de page (Alt+Maj+A à K), Rotation, Position, favoris, Mettre au
    début / à la fin, Remettre dans l'ordre d'origine, Copier la page, Tout sélectionner,
    Inverser la sélection, Marquer comme Supprimée (Suppr) ;
  - « Pages ▾ » (vignettes ou détails, taille) et « Réinitialiser ▾ » ;
  - enregistrement dans la base ComicRust, ou dans la base MySQL de ComicRack (écriture
    activée), au format exact de ComicRack ; au choix aussi dans le ComicInfo.xml du CBZ.
- **Lecteur** : il suit l'ordre des pages de la fiche, masque les pages « Supprimée » et
  applique la rotation.
- **Bedetheque Scraper 2** : quand le site bloque les requêtes (403 répétés, page anti-robot),
  une fenêtre propose d'installer cloudscraper, avec Python 3.13 et la commande à copier.

### Recette
[recette/v0.10.6.md](recette/v0.10.6.md) — NO-GO (6/8) : corrigé en v0.10.7.

## [v0.10.5] — 2026-11-01

### Corrections
- **Bedetheque Scraper 2 – tester le site** :
  - l'album de référence est bien Blacksad tome 1 (adresses données par Yoyo ; les anciennes
    pointaient vers Achille Talon #19 et Astérix) ;
  - le coloriste et le résumé de l'album sont facultatifs : s'ils manquent, ils sont marqués ⚠
    sans conclure que le site a changé.

### Recette
[recette/v0.10.5.md](recette/v0.10.5.md) — GO (3/3 OK).

## [v0.10.4] — 2026-11-01

Bedetheque Scraper 2 face aux changements du site.

### Nouveautés
- **Alerte quand le site change.** Une page d'album, de série ou de recherche lue sans ses
  éléments essentiels déclenche un message unique (« La page de Bedetheque semble avoir changé
  de structure… »). Le détail va dans le journal de débogage. Seuls les champs trouvés sont
  proposés.
- **Motifs dans un fichier à part.** Les 69 expressions qui lisent les pages sont dans
  `scripts\exemples\bd2\motifs.json`. Pour une correction, `data\bedetheque\motifs.json`
  remplace seulement les motifs voulus et n'est jamais écrasé par une mise à jour. Un motif
  invalide reprend la valeur intégrée au script.
- **📜 Scripts → « Bedetheque Scraper 2 – tester le site »** :
  - lit un album de référence (Blacksad tome 1, modifiable dans `motifs.json`) et sa série ;
  - affiche ✔/✖ pour 20 champs, avec une conclusion ;
  - bouton « Copier le rapport ».

### Recette
[recette/v0.10.4.md](recette/v0.10.4.md) — GO (5/5 OK).

## [v0.10.3] — 2026-11-01

Demandes de la recette de la v0.10.2.

### Nouveautés
- **Thème « Moderne »**, inspiré de la maquette Nebula BD, devenu le thème par défaut. Il existe
  en sombre et en clair :
  - accent violet, panneaux et fenêtres en plusieurs niveaux, coins arrondis, ombres douces ;
  - boutons « pilule » dans les barres d'outils ;
  - couvertures arrondies avec bord d'accent pour la sélection ;
  - tableau à lignes alternées ;
  - contrastes AA vérifiés.

  ⚙ → Thème : Style (Moderne / Classique) et Luminosité (Sombre / Clair / Comme Windows).
  L'ancien thème reste disponible sous le nom « Classique ».

### Corrections
- **Bedetheque Scraper 2 / QuickScrape BD2** : plus de choix automatique, la fenêtre de choix
  attend toujours ta décision. L'option « fermeture automatique » est retirée des réglages ;
  un `App.Config` importé garde sa valeur pour ComicRack.

### Recette
[recette/v0.10.3.md](recette/v0.10.3.md) — GO (11/11 OK).

## [v0.10.2] — 2026-11-01

Améliorations demandées à la recette de la v0.10.1.

### Nouveautés
- **Icônes** : elles ne s'affichent plus en carrés. Les polices de symboles d'egui sont
  réactivées, et Segoe UI Symbol / Segoe UI / Segoe UI Emoji de Windows servent de secours.
- **Déplacer une série entière dans une liste de lecture** (liste triée par « ordre de la
  liste », livres groupés par série) :
  - un clic sur l'en-tête ou sur la pile sélectionne toute la série ; Ctrl ajoute une série,
    Maj une plage de séries ;
  - on la glisse d'un bloc entre deux autres séries ;
  - le menu de l'en-tête propose Monter / Descendre la série ;
  - une série dispersée dans la liste est regroupée à l'endroit du dépôt.
- **Livres dont le fichier a disparu** :
  - bandeau « ⚠ manquant » sur la couverture, icône dans le tableau ;
  - liste « Livres manquants (N) » dans le panneau des listes ;
  - « Retrouver le fichier… » et « Chercher dans un dossier… » (la lecture, la note et les infos
    sont gardées) ;
  - « Retirer de la bibliothèque… » : la fiche seulement, jamais le fichier, avec confirmation.
    Avec ComicRack en écriture, le retrait se fait comme ComicRack le fait ;
  - à l'ouverture, une fenêtre propose ces choix ; si le disque ou le NAS ne répond pas, elle ne
    propose que « Réessayer » ;
  - un NAS injoignable ou une disparition massive ne marque rien.
- **Bedetheque Scraper 2** :
  - miniatures des couvertures dans les choix de série, d'album et d'édition. Elles sont
    chargées après l'ouverture de la fenêtre, avec une pause entre les images et un cache dans
    `data\bedetheque\cache\` ;
  - **QuickScrape** : nouvelle fenêtre avec un bouton Coller, des exemples de liens et
    l'aperçu du lien (couverture et titre).
- **Tes réglages ComicRack** (`App.Config` de Bedetheque, `losettingsx.dat` de Library
  Organizer) : import vérifié valeur par valeur. Les chemins de tes 7 profils sont identiques à
  ceux de l'original.

### Corrections (relecture avant publication)
- Un livre déplacé pendant une vérification n'est plus marqué manquant.
- Les vérifications de fichiers ne bloquent plus l'interface (ouverture, Réessayer, Retrouver).
- Suppr ne vise plus un livre masqué par la recherche.
- Il faut cocher une case pour retirer un livre dont le fichier existe encore.
- Plus de seconde fiche pour un fichier vu deux fois pendant une analyse.

### Recette
[recette/v0.10.2.md](recette/v0.10.2.md) — GO (19/19 OK).

## [v0.10.1] — 2026-10-31

Demandes de la recette de la v0.10.0 : modifier les fiches à la main, les flèches du clavier,
et les deux scripts de ComicRack les plus utilisés.

### Nouveautés
- **Fenêtre « Infos du livre »** (Ctrl+I, clic droit → « Modifier les infos… », double-clic sur
  la couverture des détails) :
  - onglets Résumé, Détails, Intrigue et notes, Catalogue, Personnalisé ;
  - suggestions tirées de la bibliothèque, note en étoiles ; chaque erreur s'affiche sous son
    champ ;
  - **plusieurs livres à la fois** : un champ qui diffère affiche « (valeurs multiples) » et
    n'est modifié que si tu le changes ;
  - Précédent / Suivant ; case « Écrire aussi les infos dans le fichier » ;
  - en lecture seule : consultation seulement ;
  - l'analyse des dossiers n'écrase pas une fiche modifiée à la main.
- **Flèches du clavier** :
  - couvertures : ← → ↑ ↓, Début/Fin, Page préc./suiv., Maj pour étendre la sélection, Ctrl
    pour déplacer le curseur, Espace pour (dé)sélectionner, Entrée pour ouvrir ;
  - tableau : mêmes touches ;
  - arbre des listes : ↑/↓ pour changer de liste, → pour déplier, ← pour replier ;
  - Tab passe de la vue à l'arbre.
- **Bedetheque Scraper 2**, portage en Python 3 du script de ComicRack (v6.08, crédits
  conservés) :
  - mêmes réglages et mêmes fenêtres de choix (série, album, édition) avec couvertures, choix
    automatique après un délai, QuickScrape par lien ;
  - mêmes pauses entre les requêtes ;
  - bouton dans la fenêtre « Infos du livre » ;
  - import de l'`App.Config` de ComicRack ;
  - option en plus : ne remplir que les champs vides.
- **Library Organizer**, portage en Python 3 du script de ComicRack (2.1.13, licence Apache
  2.0) :
  - profils, syntaxe d'origine des modèles `{préfixe<champ>suffixe}` avec tous les jetons
    (valeurs calculées sur toute la bibliothèque, comme l'original), règles ;
  - Déplacer / Simuler, doublons (garder les deux ou ne pas déplacer, jamais d'écrasement) ;
  - journal, annulation du dernier rangement réellement appliqué, suppression des dossiers
    devenus vides sur confirmation ;
  - import de `losettingsx.dat` de ComicRack.
- **Scripts** : formulaires à onglets, liste de choix avec vignettes, `open_url`, lecture de toute
  la bibliothèque (`library_books()`), bilan des renommages appliqués.

### Corrections (relecture avant publication)
- Clavier sans effet sur la bibliothèque quand une fenêtre de script est ouverte.
- Nettoyage des dossiers vides limité au dossier de base du profil (jamais une racine).
- Journaux de Bedetheque dans `data\bedetheque\`.

### Recette
[recette/v0.10.1.md](recette/v0.10.1.md) — GO (23/23 OK).

## [v0.10.0] — 2026-10-26

Scripts Python, avec Python embarqué dans le zip.

### Nouveautés
- **Scripts Python** dans le dossier `scripts\` à côté de l'exe, avec **Python 3.13 embarqué**
  (dossier `python\` : rien à installer). Le guide est dans `scripts.md`.
  - Un script qui plante ne fait pas tomber ComicRust : Python tourne dans un processus à part.
  - Les scripts ne modifient jamais directement la base ni les fichiers : ils demandent les
    modifications à ComicRust, qui les applique avec ses protections.
- **Menu 📜 Scripts**, et **Scripts ▸** dans le menu contextuel des livres. Une fenêtre suit
  l'exécution : avancement, journal, bouton Arrêter, questions du script.
- **Aperçu avant application** de ce que le script veut faire :
  - pour chaque livre : le champ, la valeur avant et la valeur après ;
  - les renommages (ancien → nouveau chemin) et les écritures dans les fichiers ;
  - une case à cocher par livre ; les conflits sont signalés et ne sont pas appliqués.
- **Automatismes** (au démarrage, à l'ouverture ou à la fermeture d'un livre, sur les nouveaux
  livres d'une analyse) : **désactivés par défaut** (« Gérer les scripts… »), jamais lancés en
  lecture seule.
- **Modification des fiches et des fichiers** par les scripts :
  - 57 champs de ComicRack, valeurs personnalisées comprises, contrôlés selon leur type ;
  - renommage et déplacement des fichiers avec mise à jour de la fiche. Pas d'écrasement, nom
    vérifié pour Windows ; si la fiche ne peut pas être mise à jour, le fichier est remis en
    place ;
  - en mode ComicRack, il faut l'écriture activée ; seuls les éléments modifiés sont réécrits.
- **Scripts fournis** dans `scripts\exemples\` :
  - **Nettoyage des fiches** : espaces, numéros, champs complétés depuis le nom du fichier ;
  - **Ranger les fichiers** : modèle de nom et sous-dossiers série ou éditeur ;
  - **Comic Vine** : infos d'un livre en ligne, avec ta clé d'API, gardée chiffrée et jamais
    affichée ;
  - un automatisme pour les nouveaux livres, désactivé.
- **Mises à jour** : un script d'exemple que tu as modifié n'est jamais écrasé. La nouvelle
  version est posée à côté (`.py.nouveau`).

### Corrections (relecture avant publication)
- Fermer ComicRust pendant « Appliquer » : il attend la fin du livre en cours (un fichier ne
  peut plus être séparé de sa fiche).
- Chemins des renommages normalisés (pas de `..`, même écriture de la racine que le livre :
  pas de doublon dans ComicRack).
- Rien n'est écrit hors du dossier de ComicRust (module Python dans `data\python\`).
- Aperçu préparé en arrière-plan (pas de blocage sur un NAS), processus Python toujours arrêtés.

### Recette
[recette/v0.10.0.md](recette/v0.10.0.md) — GO (18/18 OK).

## [v0.9.1] — 2026-10-25

Les livres se sélectionnent et se déplacent à la souris, comme dans ComicRack et l'explorateur
Windows (demande de la recette de la v0.9.0).

### Nouveautés
- **Sélection à la souris** :
  - vue des couvertures : tirer un **rectangle de sélection** depuis un espace vide (Ctrl :
    ajouté à la sélection) ; la vue défile toute seule quand le pointeur approche du bord ;
  - vue tableau : cliquer sur une ligne **non sélectionnée** et glisser vers le haut ou le bas
    sélectionne les lignes balayées (Ctrl : ajoutées à la sélection) ;
  - **Ctrl+A** : tous les livres affichés ; **Échap** ou **clic dans le vide** : plus de
    sélection.
- **Réordonner une liste de lecture en glissant** (couvertures et tableau) : glisser un livre,
  ou la sélection, et le déposer entre deux livres. Un trait montre où il ira, la vue défile
  près des bords ; la liste est enregistrée une seule fois, au lâcher.
  - Vue pas triée dans l'ordre de la liste : curseur « interdit » et message « Trier dans
    l'ordre de la liste pour réordonner ».
  - Dans le tableau, on glisse depuis une ligne **déjà sélectionnée** (depuis une autre ligne,
    c'est une sélection).
- **Ajouter des livres à une liste par glisser-déposer** : déposer la sélection sur une liste
  de lecture du panneau des listes. La liste survolée est mise en évidence ; un badge « N
  livres » suit le pointeur et dit ce que fera le dépôt. Listes intelligentes, dossiers,
  Bibliothèque, liste affichée et listes en lecture seule : curseur « interdit », avec la
  raison.
- Un glisser ne démarre qu'après un petit déplacement (un clic reste un clic) ; double-clic
  pour ouvrir et clic droit inchangés ; Échap pendant un glisser l'annule. **Aucun glisser ne
  déplace ni ne supprime de fichier** : seules les listes changent.

### Recette
[recette/v0.9.1.md](recette/v0.9.1.md) — GO (19/19 OK).

## [v0.9.0] — 2026-10-25

Gestion complète des listes et écriture dans la base de ComicRack.

### Nouveautés
- **Gestion des listes** (panneau des listes) :
  - bouton « ＋ » : nouvelle liste de lecture, liste intelligente ou dossier ;
  - menu contextuel : Modifier, Renommer (F2), Dupliquer, Supprimer (avec avertissement des
    listes intelligentes qui en partent), Convertir en liste de lecture, Monter / Descendre,
    Déplacer dans ▸ ;
  - glisser-déposer dans l'arbre.
- **Livres et listes de lecture** : sélection multiple (Ctrl / Maj + clic), « Ajouter à la
  liste ▸ » (ou nouvelle liste avec les livres choisis) ; dans une liste de lecture : « Retirer
  de la liste » (Suppr : ne supprime jamais de fichier), Monter / Descendre.
- **Éditeur de listes intelligentes**, comme celui de ComicRack :
  - critères (tous / au moins un), Non, groupes imbriqués ;
  - liste de départ, limite ;
  - aperçu en direct du nombre de livres.

  Les critères que tu ne touches pas sont gardés à l'identique.
- **Où sont enregistrées les listes** :
  - en autonome : dans la base de ComicRust ;
  - avec ComicRack : dans `ComicDb.xml`, **seulement quand ComicRack est fermé** et si
    l'écriture est activée. Une sauvegarde est faite avant chaque modification : la première,
    `ComicDb.comicrust-origine-….xml.bak`, est gardée pour toujours, plus les 10 dernières.
- **Écriture dans la base MySQL de ComicRack** (assistant → « Lecture et écriture » ; lecture
  seule par défaut) :
  - ce qui est enregistré : la lecture (page, date, nombre d'ouvertures), les fichiers convertis
    en CBZ, les infos écrites dans les fichiers ;
  - ComicRack lancé voit les modifications en quelques secondes ;
  - seuls les éléments concernés de la fiche sont modifiés ;
  - les lectures faites dans ComicRust en lecture seule sont proposées au report, sans jamais
    faire reculer ComicRack.

### Corrections (relecture avant publication)
- Caractères interdits en XML retirés des noms et descriptions (ComicRack aurait refusé
  `ComicDb.xml`).
- Nombres décimaux écrits avec le séparateur de Windows (« 4,5 »), comme ComicRack les lit.
- Mode lecture seule : les listes de ComicRack ne sont plus modifiables.
- ComicRack lancé pendant l'enregistrement : vérifié à nouveau juste avant d'écrire.
- Écritures en attente gardées à la fermeture de ComicRust et refaites au lancement suivant.

### Recette
[recette/v0.9.0.md](recette/v0.9.0.md) — GO (21/21 OK).

## [v0.8.0] — 2026-10-24

Se passer de ComicRack : sa bibliothèque peut être transférée dans la base de ComicRust.

### Nouveautés
- **⚙ → « Transférer la bibliothèque ComicRack… »** : copie dans la base de ComicRust (fichier
  local portable `data\comicrust.db` ou base MySQL configurée dans l'assistant) :
  - les **livres** avec toutes leurs infos : lecture (page, date, nombre d'ouvertures), notes,
    champs « Book… », signets et attributs de page, valeurs personnalisées ;
  - les **listes** : dossiers, listes de lecture, listes intelligentes, avec leurs
    identifiants d'origine ;
  - les **dossiers** de BD surveillés par ComicRack.
  - Source : la base MySQL de ComicRack (avec `ComicDb.xml` pour les listes et les dossiers) ou
    un `ComicDb.xml` seul.
  - Ensuite, ComicRust passe en autonome (case cochée par défaut).
- **ComicRack n'est jamais modifié** : sa base et `ComicDb.xml` sont seulement lus.
- **Livres déjà ajoutés par l'analyse des dossiers** : remplacés par leur fiche ComicRack (même
  identifiant), la lecture la plus récente est gardée.
- **Transfert relancé** : pas de doublon de livres ni de listes. La lecture faite dans ComicRust
  et les fichiers convertis ou déplacés dans ComicRust sont gardés.
- **Même partage sous deux noms** (`Z:\…` et `\SERVEURNAS\…`) : le dossier n'est pas ajouté
  deux fois, les chemins des livres sont ramenés sous le dossier surveillé ; les livres hors de
  tout dossier surveillé sont signalés.
- **Mode autonome : les listes s'affichent** (relues dans la base de ComicRust quand elles
  changent) et les listes intelligentes sont calculées.

### Corrections (relecture avant publication)
- Transfert interrompu : ComicRust ne bascule plus sur une bibliothèque incomplète.
- Pas de transfert pendant une analyse des dossiers, ni d'analyse pendant un transfert
  (doublons évités) ; la fenêtre ne peut plus lancer deux transferts à la fois.
- `ComicDb.xml` illisible avec MySQL : les livres sont transférés, un avertissement le signale.
- Message clair quand un `ComicDb.xml` ne contient aucun livre (bibliothèque MySQL).

### Recette
[recette/v0.8.0.md](recette/v0.8.0.md) — GO (17/17 OK).

## [v0.7.3] — 2026-10-24

Correctif de sécurité signalé pendant la recette de la v0.7.2.

### Corrections
- **Mot de passe MySQL plus jamais en clair** dans `data\comicrust.ron`. Cela concerne celui
  saisi dans l'assistant (base de ComicRack « Saisir les paramètres », ou base autonome
  MySQL).
  - Il est chiffré avec la protection de données de Windows (DPAPI), liée à ton compte
    Windows : seul ce compte, sur ce PC, peut le relire.
  - ComicRust copié sur un autre PC ou ouvert par un autre compte garde tous ses réglages,
    sauf ce mot de passe, à saisir à nouveau dans l'assistant (le message d'erreur le dit).
  - Les réglages existants sont réécrits chiffrés au premier enregistrement.
  - Rappel : avec « Paramètres lus dans ComicRack.ini », le mot de passe n'est jamais recopié
    dans ComicRust.

### Recette
[recette/v0.7.3.md](recette/v0.7.3.md) — GO (8/8 OK).

## [v0.7.2] — 2026-10-24

Correctif de la v0.7.1 (recette NO-GO, A-03).

### Corrections
- **Changement de bibliothèque** (assistant) vers une bibliothèque plus petite, par exemple une
  base MySQL neuve : ComicRust se fermait. La présentation calculée pour l'ancienne
  bibliothèque était relue dans la nouvelle. Elle est maintenant oubliée à chaque changement
  (A-03, cause trouvée grâce au rapport de plantage de la v0.7.1).

### Recette
[recette/v0.7.2.md](recette/v0.7.2.md) — GO (7/7 OK).

## [v0.7.1] — 2026-10-24

Correctif de la v0.7.0 (recette : 21/22, A-03).

### Corrections
- **Assistant, ComicRust autonome avec MySQL** : ComicRust se fermait au clic sur
  « Terminer » sans garder la configuration (A-03, non reproduit dans l'atelier).
  - Piles agrandies pour tous les fils (16 Mio) et pour le fil principal (8 Mio) : un
    débordement de pile fermait le programme sans message.
  - Les réglages sont enregistrés dès la fin de l'assistant.
  - Une erreur inattendue pendant la vérification s'affiche dans l'assistant.
- **Rapport de plantage** : si ComicRust se ferme de façon inattendue, le détail est écrit
  dans `data\plantage.txt` et montré au lancement suivant (fichier gardé sous
  `plantage-<date>.txt`, à joindre au rapport de recette).

### Recette
[recette/v0.7.1.md](recette/v0.7.1.md) — NO-GO (A-03 : plantage localisé par le nouveau rapport, corrigé en v0.7.2).

## [v0.7.0] — 2026-10-24

Première étape vers un ComicRust qui peut se passer de ComicRack : assistant de premier
démarrage et mode autonome.

### Nouveautés
- **Assistant de premier démarrage** (et menu ⚙ → « Assistant de configuration… ») :
  - **ComicRust autonome** : sa propre base, en fichier local portable (`data\comicrust.db`,
    SQLite) ou sur un serveur MySQL / MariaDB (tables `comicrust_…`, qui peuvent partager la
    base de ComicRack sans la toucher), et un ou plusieurs **dossiers de BD** ;
  - **avec ComicRack** : `ComicRack.ini` cherché à son emplacement habituel, `ComicDb.xml` et
    `Config.xml` retrouvés d'après lui ; bibliothèque lue dans la base **MySQL** de ComicRack
    ou dans son **`ComicDb.xml`** (ComicRack sans MySQL) ; lecture seule (l'écriture viendra
    en v0.9.0).
  - Une version précédente déjà reliée à la base MySQL de ComicRack garde ses réglages sans
    passer par l'assistant.
- **Analyse des dossiers de BD** (mode autonome, au démarrage et bouton « 🔍 Analyser les
  dossiers ») : nouveaux fichiers ajoutés avec les infos de leur `ComicInfo.xml`, fichiers
  modifiés relus, fichiers déplacés suivis, fichiers disparus marqués « manquants » ; bilan
  avec les fichiers illisibles. Les ajouts sont enregistrés au fil de l'analyse.
  - Un dossier vide (partage du NAS non monté) ou une disparition massive ne marque aucun
    livre manquant ; la corbeille du NAS (`.recycle`, `#recycle`, `@eaDir`, `$RECYCLE.BIN`…)
    et `_originaux` sont ignorés.
- **Lecture enregistrée dans la base de ComicRust** (mode autonome) : page, dernière page lue,
  date et nombre d'ouvertures, à la fermeture du livre ou de ComicRust.
- Bibliothèque `ComicDb.xml` relue automatiquement quand ComicRack l'enregistre.
- Écrire les infos dans les CBZ et convertir en CBZ fonctionnent avec les trois bibliothèques ;
  en mode autonome, la fiche du livre est mise à jour (sans toucher à sa lecture).

### Recette
[recette/v0.7.0.md](recette/v0.7.0.md) — GO (21/22 OK ; A-03 → v0.7.1).

## [v0.6.0] — 2026-10-22

Piste « Fichiers BD » : ComicRust peut modifier des fichiers BD ; la base ComicRack reste en
lecture seule.

### Nouveautés
- **Écrire les infos dans le fichier** (menu « 🛠 Fichiers » de la bibliothèque, ou clic droit
  sur une couverture ; livre sélectionné ou tous les livres affichés) : la fiche du livre dans
  la base ComicRack (série, numéro, auteurs, résumé, pages…) est écrite dans le
  `ComicInfo.xml` du CBZ, comme « Mettre à jour les fichiers » de ComicRack : mêmes éléments,
  même ordre, éléments ajoutés par d'autres logiciels gardés. Les pages sont recopiées sans
  recompression dans un fichier temporaire, vérifié avant de remplacer l'original ; un fichier
  déjà à jour n'est pas touché ; un fichier que ComicRack est en train d'enregistrer est
  laissé. Un livre écrit sort de « Fichiers à mettre à jour » dans ComicRust, tant que sa
  fiche ne change pas dans ComicRack.
- **Convertir en CBZ** les CBR, CB7 et CBT : nouveau `.cbz` à côté de l'original (mêmes pages,
  `ComicInfo.xml` de la base) ; l'original est gardé ou rangé dans le sous-dossier
  `_originaux`. La base ComicRack indiquant toujours l'ancien fichier, ComicRust ouvre le CBZ
  à sa place quand l'original n'est plus là.
- Fenêtre d'avancement (bouton « ⏹ Arrêter ») et bilan copiable avec les erreurs.
- **Pages HEIF/HEIC et AVIF** dans les archives et les dossiers d'images : AVIF décodé en pur
  Rust ; HEIC par le décodeur de Windows (extensions HEIF et HEVC du Microsoft Store), à défaut
  par un décodeur pur Rust.
- **WebComic : scrapers et composition**, comme ComicRack : scraper d'index (`!`) et de
  navigation (`?`), motifs `<Part>` avec leurs options (`Cut`, `Reverse`, `AddOwn`, `Sort`,
  `MaximumMatches`), `<base href>` ; plusieurs images par page (lignes × colonnes ou taille de
  page fixe, bordure, fond, droite à gauche). Exemples dans les échantillons de recette.
- `comicrust-cli convert <livre.cbr>` : conversion en ligne de commande.

### Corrections
- WebComic composé de droite à gauche avec une bordure : images décalées de deux bordures
  dans ComicRack ; centrées dans ComicRust. Page de taille fixe : une image qui ne tient plus
  en hauteur passe à la page suivante au lieu de déborder.

### Recette
[recette/v0.6.0.md](recette/v0.6.0.md) — GO (28/28 OK).

## [v0.5.2] — 2026-10-22

Améliorations et correctifs demandés après la recette v0.5.1.

### Nouveautés
- **Chaque liste garde sa présentation** : tri, sens, regroupement et « Empiler par série »
  sont mémorisés pour chaque liste et pour toute la bibliothèque. Une liste ouverte pour la
  première fois reprend la présentation de la bibliothèque (une liste de lecture : son ordre,
  sans groupes).
- **Piles : double-clic** (ou Entrée) pour ouvrir une pile ; un clic la sélectionne.
- « 📋 Copier les livres » au-dessus d'une liste : la liste des livres affichés (série,
  numéro, volume, année, fichier), pour comparer avec ComicRack.

### Corrections
- **Lecture faite dans ComicRust** : la barre orange, la part lue des piles, la colonne « Lu »
  et la fiche (« Lu dans ComicRust : x % (page n) ») en tiennent compte ; le plus avancé entre
  ComicRack et ComicRust est affiché. La base ComicRack n'est toujours pas modifiée.
- **Valeurs proposées d'après le nom du fichier** : quand la fiche ne donne pas la série, le
  numéro, le volume, l'année ou le format, ComicRust les déduit du nom du fichier avec
  l'algorithme de ComicRack (`ComicNameInfo`), comme ComicRack pour ses doublons, ses
  statistiques de série et ses listes intelligentes (recette v0.5.1 : 166 doublons contre 154).

### Recette
[recette/v0.5.2.md](recette/v0.5.2.md) — GO (18/18 OK ; doublons 154 / 154 comme ComicRack).

## [v0.5.1] — 2026-10-22

Améliorations demandées après la recette v0.5.0 (GO), avant la v0.6.0.

### Nouveautés
- **Bouton « ✖ Fermer »** dans le lecteur (et Ctrl+W) : ferme le livre en mémorisant sa page
  et revient à la bibliothèque.
- **« 🗂 Empiler par série »** (vue en couvertures, aussi dans les listes) : une pile par série,
  avec le nombre de livres et la part déjà lue ; un clic ouvre la pile, « ◀ Toutes les piles »
  ou Échap y revient. Compatible avec le regroupement (éditeur, scénariste, année…).

### Corrections
- « Doublons » : regroupement sensible à la casse (série et chemin), comme ComicRack
  (recette v0.5.0 : 166 doublons trouvés contre 154 pour ComicRack).

### Recette
[recette/v0.5.1.md](recette/v0.5.1.md) — GO (16/18, bloquants OK) ; D-01 et E-06 → v0.5.2.

## [v0.5.0] — 2026-10-22

### Nouveautés
- **Listes intelligentes calculées par ComicRust** : ComicRack ne garde pas leur contenu quand
  la bibliothèque est dans MySQL ; ComicRust lit leurs critères dans `ComicDb.xml` et les
  applique lui-même, avec les règles de ComicRack : critères texte (est, contient, contient
  l'un de / tous, commence, finit, liste contient, expression régulière), nombres, dates
  (« dans les N derniers jours »), oui/non, groupes « tous / au moins un », critères inversés,
  liste de départ, limites, **série entièrement complète**, **doublons**, **fichiers à mettre
  à jour**. Les 29 listes intelligentes de ta bibliothèque sont toutes prises en charge.
- Bulle d'aide de chaque liste : nombre de livres selon ComicRust et selon ComicRack ;
  « 📋 Copier le bilan des listes » compare les deux pour toutes les listes.
- **Reprise de lecture** : quand ComicRack et ComicRust n'ont pas retenu la même page, une
  question propose les deux (ou le début), avec « Toujours faire ce choix » ; réglable dans le
  menu ⚙ (Demander / Page de ComicRack / Page de ComicRust).
- **Thème clair ou sombre** (ou celui de Windows), dans le menu ⚙.

### Corrections
- « 📖 Lire (page N) » dans la fiche ouvre bien à la page N enregistrée par ComicRack (la page
  mémorisée par ComicRust passait avant).
- Les listes sont calculées dans un fil séparé : l'interface ne se fige pas quand ComicRack
  modifie la base.

### Recette
[recette/v0.5.0.md](recette/v0.5.0.md) — GO (27/27 OK).

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
[recette/v0.4.1.md](recette/v0.4.1.md) — GO (13/14 OK, 0 échec) ; corrige R-02, R-03, D-02 de la v0.4.0.
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
