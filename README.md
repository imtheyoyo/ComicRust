# ComicRust

[![CI Tauri](https://github.com/imtheyoyo/ComicRust/actions/workflows/ci-tauri.yml/badge.svg)](https://github.com/imtheyoyo/ComicRust/actions/workflows/ci-tauri.yml)
[![Release](https://img.shields.io/github/v/release/imtheyoyo/ComicRust?include_prereleases)](https://github.com/imtheyoyo/ComicRust/releases)

**ComicRust** est un lecteur et gestionnaire de BD écrit en Rust, réimplémentation de
[ComicRack Community Edition](https://github.com/maforget/ComicRackCE) (interface web Tauri 2 + Svelte).
L'analyse du binaire d'origine est dans [`docs/INVENTAIRE_REA.md`](docs/INVENTAIRE_REA.md).

## Télécharger

Les [releases](https://github.com/imtheyoyo/ComicRust/releases) `tauri-vX.Y.Z` contiennent
`ComicRust-Tauri-<tag>-windows-x64.zip` pour **Windows 11 x64** : dézipper et lancer
`ComicRust-Tauri.exe`. Version portable, sans installateur ; mise à jour intégrée.
Le mode d'emploi complet (premier démarrage autonome ou avec ComicRack, scripts Python, thèmes,
ligne de commande, limites) est dans [`README-Tauri.md`](README-Tauri.md), livré dans le zip sous le
nom `README.md`.

## Organisation du dépôt

| Dossier | Rôle |
| --- | --- |
| `crates/comicrust-tauri`, `frontend/` | l'application (backend Rust + interface Svelte/TypeScript) |
| `crates/comicrust-core` | cœur partagé : formats, bibliothèque, base, scripts |
| `crates/comicrust-cli` | l'outil en ligne de commande |
| `crates/comicrust`, `crates/egui-mcp` | ancienne interface **egui** (archivée, voir ci-dessous) |
| `docs/tauri/` | contrats IPC, audit de parité, plan, thèmes, performances |
| `recette/` | document de recette de chaque version et rapports |

## Compiler

```
cd frontend && npm ci && npm run build && cd ..
cargo build --release -p comicrust-tauri --features app,custom-protocol
cargo test --all
```

La fenêtre (fonction `app`) exige les dépendances système de Tauri (WebView2 sous Windows). Voir
[`CLAUDE.md`](CLAUDE.md) pour les règles de développement (recette et CHANGELOG à chaque version).

## Version egui (archivée)

La version egui (`ComicRust.exe`, versions `vX.Y.Z`, dernière : **v0.10.9**) n'est plus développée :
la version Tauri a repris toutes ses fonctions (audit : `docs/tauri/audit-parite.md`). Son code reste
dans ce dépôt (`crates/comicrust`) et dans la branche
[`egui-archive`](https://github.com/imtheyoyo/ComicRust/tree/egui-archive) ; son ancien README est dans
`docs/egui-archive/README-egui.md`, ses releases restent téléchargeables.

## Licence

GPL-2.0-or-later. Voir `licences\` dans le zip pour les composants tiers.
