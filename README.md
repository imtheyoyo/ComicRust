# ComicRust

[![CI](https://github.com/imtheyoyo/ComicRust/actions/workflows/ci.yml/badge.svg)](https://github.com/imtheyoyo/ComicRust/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/imtheyoyo/ComicRust?include_prereleases)](https://github.com/imtheyoyo/ComicRust/releases)

**ComicRust** est un lecteur de BD écrit en Rust, réimplémentation de
[ComicRack Community Edition](https://github.com/maforget/ComicRackCE). L'analyse du binaire
d'origine est dans [`docs/INVENTAIRE_REA.md`](docs/INVENTAIRE_REA.md).

## Télécharger

Chaque [release](https://github.com/imtheyoyo/ComicRust/releases) contient
`ComicRust-<version>-windows-x64.zip` pour Windows 10/11 x64 : dézipper et lancer
`ComicRust.exe`. Aucun runtime à installer. Le zip contient aussi `pdfium.dll` (lecture des
PDF, à laisser à côté de l'exe) et `comicrust-cli.exe`.

**ComicRust est exclusivement portable** : il n'écrit rien en dehors de son dossier. Réglages,
cache de l'index des BD et miniatures des couvertures vont dans `data\` à côté de l'exe (le
dossier doit donc être accessible en écriture : éviter `C:\Program Files`). Copier le dossier
suffit pour emporter ComicRust et ses réglages. Les réglages d'une version précédente
(`%APPDATA%\ComicRust`) sont repris au premier lancement.

## Premier démarrage : autonome ou avec ComicRack

Au premier lancement, un assistant (aussi dans le menu ⚙) propose :

- **ComicRust autonome** : sa propre base — fichier local portable `data\comicrust.db`
  (SQLite) ou serveur MySQL / MariaDB (tables `comicrust_…`) — et un ou plusieurs dossiers
  de BD, analysés au démarrage (« 🔍 Analyser les dossiers ») : nouveaux fichiers ajoutés
  d'après leur `ComicInfo.xml`, fichiers modifiés relus, disparus marqués manquants. La
  lecture (page, date) est enregistrée dans cette base.
- **Avec ComicRack** : `ComicRack.ini`, `ComicDb.xml` et `Config.xml` retrouvés
  automatiquement ; bibliothèque lue dans la base MySQL de ComicRack ou dans son
  `ComicDb.xml`, en lecture seule (détails ci-dessous).

**Se passer de ComicRack** (v0.8.0) : ⚙ → « Transférer la bibliothèque ComicRack… » copie ses
livres (infos, lecture, notes, signets), ses listes et ses dossiers dans la base de ComicRust,
puis ComicRust passe en autonome. ComicRack n'est que lu ; le transfert peut être relancé sans
créer de doublons.

## Bibliothèque (base MySQL de ComicRack)

ComicRust lit la base **MySQL** de ComicRack CE, **en lecture seule** (uniquement des `SELECT`) :
assistant de configuration (menu ⚙), puis soit choisir le `ComicRack.ini` (ligne
`DataSource=mysql:Server=…; Port=…; Uid=…; Pwd=…; Database=…;`, relue à chaque connexion — le
mot de passe n'est pas recopié dans ComicRust), soit saisir les paramètres (mot de passe enregistré chiffré
pour le compte Windows, jamais en clair). *Tester la
connexion* affiche le nombre de livres.

L'onglet **Bibliothèque** affiche alors toutes les BD de la base, en **couvertures** (la page
choisie comme couverture dans ComicRack, miniatures mises en cache dans `data\couvertures`) ou
en **tableau** (colonnes triables d'un clic) :

- recherche sur série, titre, auteurs, personnages, équipes, étiquettes, arc, année et chemin ;
- **tri** (série, titre, parution, ajout, dernière lecture, note, pages, avancement, taille,
  chemin, ordre de la liste) croissant ou décroissant, et **regroupement** (série, éditeur,
  scénariste, année, arc, genre, format, état de lecture, ou aucun) ;
- panneau **Listes** : dossiers, listes de lecture et listes intelligentes de ComicRack, lues
  dans son `ComicDb.xml` (trouvé d'après `ComicRack.ini`, ou choisi avec « … ») ; une liste de
  lecture s'affiche dans son ordre ; les **listes intelligentes sont calculées par ComicRust**
  avec les critères et les règles de ComicRack (ComicRack ne garde pas leur contenu avec MySQL) ;
- panneau **Détails** : fiche complète du livre sélectionné (résumé, auteurs, personnages,
  lieux, note, lecture…) ;
- **empilement par série** : une pile par série (double-clic pour l'ouvrir) ; chaque liste
  garde son tri, son regroupement et son empilement ;
- un clic sélectionne, un **double-clic** ou **Entrée** ouvre la BD (« ✖ Fermer » ou Ctrl+W
  dans le lecteur pour revenir) ; si ComicRack et
  ComicRust n'ont pas retenu la même page, ComicRust demande laquelle reprendre (réglable) ;
- thème **clair** ou **sombre** (menu ⚙).

Les **modifications faites dans ComicRack** (livre ajouté, modifié, supprimé, liste changée)
apparaissent en quelques secondes, sans recharger : ComicRust suit les compteurs de la table
`changes`, comme ComicRack lui-même (désactivable dans le menu ⚙).

Les listes `.cbl` sont d'abord résolues par l'identifiant des livres dans la base, puis par la
recherche dans un dossier (v0.2.1).

### Fichiers BD (v0.6.0)

Menu **🛠 Fichiers** de la bibliothèque (livre sélectionné ou livres affichés) et clic droit
sur une couverture. **ComicRust modifie alors des fichiers, jamais la base ComicRack.**

- **Écrire les infos dans le fichier** : la fiche du livre dans la base ComicRack (série,
  numéro, auteurs, résumé, pages…) est écrite dans le `ComicInfo.xml` du CBZ, comme
  « Mettre à jour les fichiers » de ComicRack (mêmes éléments, même ordre ; éléments ajoutés
  par d'autres logiciels gardés). Les pages sont recopiées sans recompression dans un fichier
  temporaire vérifié qui remplace l'original ; un fichier déjà à jour n'est pas touché. Un
  livre écrit sort de « Fichiers à mettre à jour » dans ComicRust (pas dans ComicRack).
- **Convertir en CBZ** (CBR, CB7, CBT) : nouveau `.cbz` à côté de l'original, mêmes pages,
  `ComicInfo.xml` de la base. L'original est gardé, ou rangé dans le sous-dossier
  `_originaux` ; la base ComicRack indique toujours l'ancien fichier : ComicRust ouvre alors le
  CBZ à sa place.
- Bilan avec les erreurs, copiable (« 📋 Copier le bilan »).

## Mises à jour

Au démarrage (désactivable dans le menu ⚙), ComicRust consulte les releases GitHub. Si une
version plus récente existe, un bandeau propose de l'installer : le zip est téléchargé, son
empreinte SHA-256 vérifiée, puis les fichiers remplacés (l'ancien exe est renommé en `.old` et
supprimé au lancement suivant). Le dossier `data\` n'est jamais touché.

## Formats

| Format | Extensions | Moteur |
|---|---|---|
| ZIP | `.cbz` `.zip` | `zip` |
| RAR 4 et 5 | `.cbr` `.rar` | UnRAR (RARLAB) via `unrar-ng` |
| 7-Zip | `.cb7` `.7z` | `sevenz-rust2` (pur Rust) |
| TAR | `.cbt` `.tar` | `tar` |
| PDF | `.pdf` | PDFium (`pdfium.dll`) |
| DjVu | `.djvu` `.djv` | `djvu-rs` (pur Rust) |
| WebComic | `.cbw` | XML ComicRack, images locales ou HTTP(S) |
| Dossier d'images | — | |
| Liste de lecture | `.cbl` | ComicRack Reading List |

Comme ComicRack, le format est reconnu **par la signature du fichier** : un `.cbr` qui est en
réalité un ZIP s'ouvre normalement. Images reconnues dans les archives : JPEG, PNG, GIF, BMP,
TIFF, WebP, JPEG XL, **HEIF/HEIC** et **AVIF**. Les métadonnées `ComicInfo.xml` sont lues
(fenêtre *Infos*).

HEIF / AVIF : l'AVIF est décodé en pur Rust (`oxideav-avif`) ; le HEIC par le décodeur de
Windows (extensions « Extensions d'image HEIF » et « Extensions vidéo HEVC » du Microsoft
Store), à défaut par un décodeur pur Rust (`heif-oxide`) plus lent et encore imparfait.

WebComic (comme ComicRack) : URL, variables `{…}`, plages `[000:1-24]`, **scrapers** d'index
(`!`, `Type="IndexScraper"`) et de navigation (`?`, `Type="BrowseScraper"`) avec leurs options
(`<Part>`, `Cut`, `Reverse`, `AddOwn`, `Sort`, `MaximumMatches`, groupe `link`, `<base href>`),
et **composition** de plusieurs images par page (`<Compositing>` : lignes × colonnes ou taille
de page fixe, bordure, fond, droite à gauche). Exemples dans `recette/echantillons`.

## Lecteur

- page simple ou **double page** (couverture seule, pages larges affichées seules), **sens manga** ;
- ajustement page entière / largeur / hauteur / taille réelle, zoom ;
- vignettes, plein écran, glisser-déposer, fichiers récents, reprise à la dernière page lue ;
- pages décodées et préchargées en arrière-plan (l'interface ne bloque pas sur une archive lente) ;
- **listes `.cbl`** : chaque entrée est retrouvée dans la base `ComicDB.xml` (si indiquée) ou dans
  le dossier des BD, d'après le **`ComicInfo.xml`** de chaque archive (série, numéro, volume, comme
  ComicRack) puis, à défaut, d'après le nom du fichier (`Série #061 …`, `Série T03`, `Série 012 (1991)`) ;
  l'index est mis en cache, la recherche affiche sa progression et un diagnostic copiable ;
  numéros manquants barrés ; en fin de livre, la page suivante ouvre le livre suivant de la liste.

| Touche | Action |
|---|---|
| → Espace PgSuiv / ← RetourArr PgPréc | page suivante / précédente (inversé en manga pour les flèches) |
| Début / Fin | première / dernière page |
| clic sur le tiers gauche / droit | page précédente / suivante |
| molette | change de page en mode page entière |
| 1 2 3 4 | page entière, largeur, hauteur, taille réelle |
| + − 0 | zoom |
| D / M | double page / manga |
| T / L / I | vignettes / liste de lecture / infos |
| F11 ou F, Échap | plein écran |
| Ctrl+O | ouvrir |

## Ligne de commande

```powershell
comicrust-cli info    "F:\BD\Thanos Quest 01.cbr"
comicrust-cli render  "F:\BD\Silver Surfer 03.pdf" 1 page1.png
comicrust-cli cbl     Thanos.cbl
comicrust-cli convert "F:\BD\Silver Surfer 04.cbr"     # crée le .cbz à côté
comicrust-cli missing "F:\BD" Thanos.cbl            # ou un ComicDB.xml à la place du dossier
```

## Organisation

| Crate | Rôle |
|---|---|
| `comicrust-core` | formats (`providers/`), `ComicInfo.xml`, écriture dans les fichiers (`writer`), base propre (`store`), analyse des dossiers (`scanner`), listes `.cbl`, `ComicDB.xml` et ses listes (`comic_lists`), listes intelligentes (`smart`), index de fichiers, base MySQL et suivi des modifications (`db/`) |
| `comicrust` | l'application `ComicRust.exe` (egui) : `app`, `book/`, `library/`, `reading_list`, `update/`, `ui/`… |
| `comicrust-cli` | outil en ligne de commande |

```powershell
cargo test --all                 # tests PDF actifs si PDFIUM_DYNAMIC_LIB_PATH pointe vers PDFium
                                 # test MySQL actif si COMICRUST_TEST_MYSQL contient une chaîne de connexion
cargo run --release -p comicrust -- "F:\BD\Thanos Quest 01.cbz"
```

## Recette

Chaque version a sa liste de tests manuels à valider avant l'étape suivante :
[`recette/`](recette) (actuelle : [v0.8.0](recette/v0.8.0.md)), avec des BD d'essai dans
[`recette/echantillons`](recette/echantillons).

## Publier une release

Créer la release sur GitHub (ou `gh release create v0.3.0 --generate-notes`), pousser un tag
`v*`, ou lancer le workflow *Release* à la main avec le nom du tag : la CI teste, compile la
version Windows x64 (`ComicRust.exe` + `comicrust-cli.exe` + `pdfium.dll`) et attache le zip,
la recette et les échantillons. **La release échoue si `recette/<tag>.md` n'existe pas.**

Les notes de chaque release viennent de [`CHANGELOG.md`](CHANGELOG.md) (section `## [vX.Y.Z]`) :
**la release échoue aussi si cette section manque**. Le workflow *Notes de version* recopie le
journal dans les releases déjà publiées.

## Correspondance .NET → Rust

| ComicRack (.NET) | ComicRust |
|---|---|
| WinForms + `ComicRack.Engine.Display.Forms` | `egui`/`eframe` |
| SharpZipLib, SharpCompress | `zip`, `unrar-ng`, `sevenz-rust2`, `tar` |
| PDFiumSharp | `pdfium-render` |
| DjVuLibre (`djvm.exe`) | `djvu-rs` |
| libwebp / libjxl | `image` (WebP) / `jxl-oxide` |
| libheif (HEIF, AVIF) | `oxideav-avif` (AVIF) ; WIC de Windows ou `heif-oxide` (HEIC) |
| `XmlSerializer` (ComicDB.xml, CBL, ComicInfo, CBW) | `quick-xml` + `serde` |
| MySqlConnector | `mysql` (lecture seule) |
| Mise à jour (ClickOnce / site) | releases GitHub + SHA-256 (`ureq`, `sha2`) |
| IronPython, WCF | à venir |

## Feuille de route

1. ~~Lecture de la base **MySQL** de ComicRack CE, vue Bibliothèque (couvertures, recherche,
   groupement)~~ (v0.3.0).
2. ~~Panneau de détails, tri et groupements au choix, vue tableau, listes de ComicRack,
   couverture choisie, suivi des modifications~~ (v0.4.0).
3. ~~**Listes intelligentes** calculées par ComicRust, thème clair/sombre, choix de la page
   de reprise~~ (v0.5.0).
4. ~~Écriture de `ComicInfo.xml`, conversion CBR→CBZ, HEIF/AVIF, scrapers WebComic~~ (v0.6.0).
5. ~~Premier démarrage : ComicRust autonome (base SQLite ou MySQL, dossiers de BD) ou avec
   ComicRack (MySQL ou ComicDb.xml)~~ (v0.7.0).
6. ~~Transfert de la bibliothèque ComicRack (livres, lecture, listes, dossiers) vers la base
   de ComicRust~~ (v0.8.0).
7. Gestion complète des listes ; écriture dans la base de ComicRack (v0.9.0).
8. Scripts Python (v0.10.0), serveur OPDS / HTTP (v0.11.0) ; plus tard, associations de
   fichiers Windows et un **mode serveur sous Docker**.


## Licences

ComicRust est sous **GPL-2.0-or-later** (ComicRack CE est sous GPL-2.0). Composants tiers
distribués dans le zip : PDFium (BSD-3, `licences/PDFium-LICENSE.txt`) et le code de
décompression UnRAR (licence freeware de RARLAB, `licences/UnRAR-license.txt` — utilisable pour
lire les RAR, pas pour créer un compresseur RAR).
