# ComicRust

[![CI](https://github.com/imtheyoyo/ComicRust/actions/workflows/ci.yml/badge.svg)](https://github.com/imtheyoyo/ComicRust/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/imtheyoyo/ComicRust?include_prereleases)](https://github.com/imtheyoyo/ComicRust/releases)

**ComicRust** est une réimplémentation en Rust de [ComicRack Community Edition](https://github.com/maforget/ComicRackCE),
le gestionnaire/lecteur de BD .NET. L'analyse du binaire d'origine est dans
[`docs/INVENTAIRE_REA.md`](docs/INVENTAIRE_REA.md).

## État actuel (v0.1)

| Crate | Contenu | État |
|---|---|---|
| `comicrust-core` | `ComicInfo.xml` (lecture/écriture, champs identiques à ComicRack), fournisseurs CBZ et dossier, tri naturel des pages, listes `.cbl`, lecture de `ComicDB.xml`, rapprochement liste ↔ bibliothèque avec la normalisation de série de ComicRack | ✅ testé |
| `comicrust` | exécutable `comicrust.exe` (CLI pour l'instant, interface egui à venir) : `info`, `cover`, `cbl`, `missing` | ✅ |

```powershell
cargo test
cargo run -p comicrust -- info  "F:\BD\Thanos\Infinity Gauntlet 001.cbz"
cargo run -p comicrust -- cover "F:\BD\Thanos\Infinity Gauntlet 001.cbz" cover.jpg
cargo run -p comicrust -- missing "%APPDATA%\cYo\ComicRack Community Edition\ComicDB.xml" Thanos.cbl
```

## Télécharger

Chaque [release](https://github.com/imtheyoyo/ComicRust/releases) contient
`ComicRust-<version>-windows-x64.zip` (Windows 10/11 x64, aucun runtime à installer :
CRT liée statiquement).

Publier une release : `git tag v0.2.0 && git push origin v0.2.0` — la CI compile et attache le zip.

## Correspondance .NET → Rust

| ComicRack (.NET) | Rust |
|---|---|
| WinForms + `ComicRack.Engine.Display.Forms` | `egui`/`eframe` (même pile que PLaunch) |
| SharpZipLib (CBZ) | `zip` |
| SharpCompress (CBR/CB7) | `unrar` (RAR, licence unRAR) / `sevenz-rust2` |
| PDFiumSharp | `pdfium-render` (DLL PDFium à côté de l'exe) |
| libwebp / libjxl / libheif / CSJ2K | `image` (webp, png, jpeg, gif, tiff) + `jxl-oxide` + `libheif-rs` |
| `XmlSerializer` (ComicDB.xml, CBL, ComicInfo) | `quick-xml` + `serde` |
| MySqlConnector / SQLite | `sqlx` (features `mysql`, `sqlite`) |
| cache disque de miniatures | `cacache` ou dossier + clé SHA-1 |
| Scanner + QueueManager | `notify` + `rayon`/`tokio` |
| IronPython (scripts `.py`) | `rhai` ou `pyo3` (compatibilité scripts existants = gros chantier) |
| WCF (serveur distant / Android) | `axum` + API HTTP (ou OPDS) |

## Feuille de route

1. **Formats** : CBR, CB7, PDF ; décodage d'images + redimensionnement (`image`, `fast_image_resize`).
2. **Bibliothèque** : modèle `ComicBook` complet (champs listés dans l'inventaire), stockage
   SQLite, puis **backend MySQL** compatible avec le schéma de ComicRack CE pour lire la base existante.
3. **Lecteur egui** : page simple/double, ajustement largeur/hauteur, sens manga, préchargement.
4. **Navigateur** : grille de miniatures avec cache, tri/groupement, recherche.
5. **Listes intelligentes** : parseur de la syntaxe de requête ComicRack (`Match [Series] contains "…"`).
6. **Écriture** de `ComicInfo.xml` dans les CBZ, conversion CBR→CBZ, WebP/JXL.
7. Serveur HTTP/OPDS, scripts.

## Licence

ComicRack CE est publié sous GPL-2.0. Ce projet ne copie pas son code mais reprend ses
formats et son comportement ; il est placé sous **GPL-2.0-or-later** pour pouvoir, si besoin,
transposer directement des algorithmes du dépôt d'origine.
