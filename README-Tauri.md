# ComicRust (version Tauri)

Lecteur et gestionnaire de BD, réimplémentation en Rust de
[ComicRack Community Edition](https://github.com/maforget/ComicRackCE), avec une interface web
(Tauri 2 + Svelte). Ce fichier est celui du **zip portable** `ComicRust-Tauri-<tag>-windows-x64.zip`
(releases `tauri-vX.Y.Z` de <https://github.com/imtheyoyo/ComicRust/releases>).

## Contenu du dossier

| Fichier | Rôle |
| --- | --- |
| `ComicRust-Tauri.exe` | l'application (à lancer) |
| `comicrust-cli.exe` | l'outil en ligne de commande (voir plus bas) |
| `ComicRust-Tauri-mcp.exe` | variante de test pilotable par une IA (voir plus bas), à ignorer en usage normal |
| `pdfium.dll` | lecture des PDF : à laisser à côté de l'exe |
| `python\` | Python embarqué, pour les scripts (rien à installer) |
| `scripts\` | vos scripts ; exemples livrés dans `scripts\exemples\` |
| `licences\` | licences des composants tiers (PDFium, Python…) |
| `MCP-tauri.md`, `scripts.md`, `RECETTE.md` | documentation du pilotage MCP, des scripts, et recette de la version |
| `data\` | créé au premier lancement : réglages, thèmes, base, miniatures, profil WebView2 |

## Installation : portable

Dézippez où vous voulez (clé USB, disque de données…) et lancez `ComicRust-Tauri.exe`. Il n'y a
**aucun installateur**. ComicRust n'écrit **rien en dehors de son dossier** : tout (réglages, base de
la bibliothèque, miniatures, thèmes, cache et profil de la fenêtre web) va dans `data\`. Copier le
dossier suffit pour tout emporter. Le dossier doit donc être accessible en écriture (éviter
`C:\Program Files`). Pour désinstaller : supprimer le dossier.

Prérequis : **Windows 11 x64** (ou Windows 10 à jour) avec **WebView2**, déjà présent sous Windows 11.
S'il manque, Windows le propose depuis le site de Microsoft (« Microsoft Edge WebView2 Runtime ») ;
il n'est pas livré dans le zip.

## Premier démarrage : autonome ou avec ComicRack

- **Autonome** : ComicRust a sa propre base locale (`data\`) et surveille les dossiers de BD que vous
  indiquez (« Dossiers surveillés » dans la barre latérale).
- **Avec ComicRack** : la bibliothèque de ComicRack est lue (en lecture seule) ; réglages → transfert
  pour la copier dans la base de ComicRust, puis s'en passer. Le transfert peut être relancé sans
  créer de doublons.

## Utilisation

Bibliothèque en grille ou en liste, groupements et tris, recherche rapide (touche `/` : cherche dans
tous les champs des fiches, comme ComicRack), listes (dossiers, listes de lecture, listes
intelligentes), édition des fiches (menu contextuel et fenêtre d'édition), lecteur de pages (CBZ, CBR,
CB7, CBT, PDF, DjVu, dossier d'images).

## Mises à jour

Réglages → « Rechercher une mise à jour… » (et vérification automatique au démarrage, réglable).
La mise à jour télécharge le zip de la dernière release `tauri-v*`, **vérifie son empreinte SHA-256**,
puis remplace les fichiers par renommage (`.old`, supprimés au lancement suivant) et relance
l'application. `data\` n'est jamais touché ; un script que vous avez modifié n'est jamais écrasé (la
nouvelle version est posée à côté, en `.nouveau`). `comicrust-cli.exe` est mis à jour avec le reste.

## Scripts Python

Les scripts `.py` du dossier `scripts\` (à côté de l'exe) ajoutent des commandes aux menus, modifient
les fiches, renomment ou rangent les fichiers, ou se déclenchent tout seuls. Le Python embarqué est
utilisé : rien à installer. Les exemples (nettoyage des séries, Comic Vine, Bedetheque, rangement…)
sont dans `scripts\exemples\`. Documentation complète : `scripts.md`.

## Thèmes

Un thème est un fichier CSS de variables `--cr-*` dans `data\themes\` : copiez un thème livré,
changez les couleurs, il apparaît dans Réglages → Thèmes, sans rien recompiler. Trois thèmes sont
livrés (Moderne clair, Moderne sombre, Classique). Contrat des variables : `docs/tauri/themes.md`
dans le dépôt.

## Ligne de commande : `comicrust-cli.exe`

```
comicrust-cli info    <livre>                  format, métadonnées, pages
comicrust-cli render  <livre> <page> <sortie>  décode une page (n° à partir de 1) en PNG/JPEG
comicrust-cli cover   <livre> <sortie>         extrait la 1re page telle quelle
comicrust-cli cbl     <liste.cbl>              affiche une liste de lecture
comicrust-cli convert <livre.cbr|cb7|cbt>      crée le CBZ à côté (original gardé)
comicrust-cli missing <ComicDB.xml|dossier> <liste.cbl>   numéros de la liste introuvables
```

Sans argument valide, l'outil affiche cette aide. Il n'utilise ni la bibliothèque ni `data\`.

## Pilotage par une IA (MCP)

`ComicRust-Tauri-mcp.exe` est la même application avec un serveur [MCP](https://modelcontextprotocol.io)
(`--mcp`, port 47802) : une IA peut voir la fenêtre et la piloter. C'est un outil de **test et de
débogage** ; `ComicRust-Tauri.exe` n'embarque pas ce serveur. Voir `MCP-tauri.md`.

## Limites connues de cette version

- La version egui (`ComicRust.exe`) est archivée : la version Tauri a repris toutes ses fonctions (audit de parité :
  `docs/tauri/audit-parite.md` dans le dépôt ; recette de la version : `RECETTE.md`).
- Pas de serveur OPDS / HTTP, ni d'association de fichiers Windows (hors périmètre pour l'instant).
- Pas d'installateur : l'application n'est pas enregistrée dans « Applications » de Windows.
- L'exécutable n'est pas signé numériquement : SmartScreen peut afficher un avertissement au premier
  lancement (« Informations complémentaires » puis « Exécuter quand même »).
- Les mises à jour exigent un dossier accessible en écriture et l'accès à GitHub.

## Licence

Voir `licences\` pour les composants tiers. Code source : <https://github.com/imtheyoyo/ComicRust>.
