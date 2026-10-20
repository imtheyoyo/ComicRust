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

**Mode portable** : créer un fichier vide `ComicRust.portable` (ou un dossier `data`) à côté de
l'exe ; les réglages sont alors enregistrés dans `data\` au lieu de `%APPDATA%\ComicRust`.

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
TIFF, WebP, JPEG XL. Les métadonnées `ComicInfo.xml` sont lues (fenêtre *Infos*).

WebComic : URL, variables `{…}` et plages `[000:1-24]` pris en charge ; les « scrapers »
(`!`, `?`) et la composition de plusieurs images par page ne le sont pas encore.

## Lecteur

- page simple ou **double page** (couverture seule, pages larges affichées seules), **sens manga** ;
- ajustement page entière / largeur / hauteur / taille réelle, zoom ;
- vignettes, plein écran, glisser-déposer, fichiers récents, reprise à la dernière page lue ;
- pages décodées et préchargées en arrière-plan (l'interface ne bloque pas sur une archive lente) ;
- **listes `.cbl`** : chaque entrée est retrouvée dans la base `ComicDB.xml` (si indiquée) ou dans
  le dossier des BD (noms du type `Série 012 (1991).cbz`) ; numéros manquants barrés et
  copiables ; en fin de livre, la page suivante ouvre le livre suivant de la liste.

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
comicrust-cli missing "F:\BD" Thanos.cbl            # ou un ComicDB.xml à la place du dossier
```

## Organisation

| Crate | Rôle |
|---|---|
| `comicrust-core` | formats (`providers/`), `ComicInfo.xml`, listes `.cbl`, `ComicDB.xml`, index de fichiers |
| `comicrust` | l'application `ComicRust.exe` (egui) |
| `comicrust-cli` | outil en ligne de commande |

```powershell
cargo test --all                 # tests PDF actifs si PDFIUM_DYNAMIC_LIB_PATH pointe vers PDFium
cargo run --release -p comicrust -- "F:\BD\Thanos Quest 01.cbz"
```

## Publier une release

Créer la release sur GitHub (ou `gh release create v0.3.0 --generate-notes`), pousser un tag
`v*`, ou lancer le workflow *Release* à la main avec le nom du tag : la CI teste, compile la
version Windows x64 (`ComicRust.exe` + `comicrust-cli.exe` + `pdfium.dll`) et attache le zip.

## Correspondance .NET → Rust

| ComicRack (.NET) | ComicRust |
|---|---|
| WinForms + `ComicRack.Engine.Display.Forms` | `egui`/`eframe` |
| SharpZipLib, SharpCompress | `zip`, `unrar-ng`, `sevenz-rust2`, `tar` |
| PDFiumSharp | `pdfium-render` |
| DjVuLibre (`djvm.exe`) | `djvu-rs` |
| libwebp / libjxl | `image` (WebP) / `jxl-oxide` |
| `XmlSerializer` (ComicDB.xml, CBL, ComicInfo, CBW) | `quick-xml` + `serde` |
| MySqlConnector / SQLite | à venir (`sqlx`) |
| IronPython, WCF | à venir |

## Feuille de route

1. **Bibliothèque** : modèle `ComicBook` complet, stockage SQLite, puis lecture de la base
   **MySQL** de ComicRack CE.
2. **Navigateur** : grille de couvertures avec cache disque, tri, groupement, recherche.
3. **Listes intelligentes** (syntaxe de requête ComicRack).
4. Écriture de `ComicInfo.xml`, conversion CBR→CBZ, HEIF/AVIF, scrapers WebComic.
5. Associations de fichiers Windows, serveur HTTP/OPDS, scripts.

## Licences

ComicRust est sous **GPL-2.0-or-later** (ComicRack CE est sous GPL-2.0). Composants tiers
distribués dans le zip : PDFium (BSD-3, `licences/PDFium-LICENSE.txt`) et le code de
décompression UnRAR (licence freeware de RARLAB, `licences/UnRAR-license.txt` — utilisable pour
lire les RAR, pas pour créer un compresseur RAR).
