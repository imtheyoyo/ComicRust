# ComicRust Tauri : feuille de route

Version parallèle de ComicRust (branche `tauri2-web`) : **Tauri 2** + front **Svelte + TypeScript
(Vite)**, au-dessus de `comicrust-core`. La version egui (`crates/comicrust`, tags `vX.Y.Z`) reste
la version de référence et n'est **pas modifiée** tant que la parité n'est pas atteinte.

## Décisions

| Sujet | Décision |
|---|---|
| Socle | Tauri 2, WebView2 du système (Windows 11 x64). Pas de MSI/NSIS : zip portable seulement. |
| Front | Svelte 5 + TypeScript + Vite (`frontend/`). **Aucune logique métier** : il affiche et envoie des intentions. |
| Cœur | `comicrust-core` réutilisé tel quel. Les commandes IPC (`crates/comicrust-tauri/src/app.rs`) sont fines : une commande = un appel au cœur + (dé)sérialisation JSON. |
| Portable | Tout dans `data\` à côté de l'exe. Profil WebView2 redirigé vers `data\webview2` (variable `WEBVIEW2_USER_DATA_FOLDER` positionnée avant la création de la fenêtre). Rien dans `%APPDATA%`, `%LOCALAPPDATA%` ni le registre. |
| Thèmes | Fichiers CSS à variables `--cr-*` dans `data\themes\` ; livrés : Moderne clair, Moderne sombre, Classique ; ajout par l'utilisateur sans recompiler ; chargement par la commande `charger_theme`. |
| Parité | Complète avec la version egui **avant la première release** (`tauri-v1.0.0`). Les versions `tauri-v0.x` sont des pré-releases de recette. |
| Releases | Tags `tauri-vX.Y.Z`, workflow `release-tauri.yml` (distinct de `release.yml`). |

## Règles de développement (celles de `CLAUDE.md`, adaptées)

- Chaque version a sa recette `recette/tauri-vX.Y.Z.md` (format de `recette/README.md` : gravités
  [B]/[M]/[m], section « déjà vérifié automatiquement », non-régression, limites connues, bilan
  GO/NO-GO) **et** sa section `## [tauri-vX.Y.Z]` dans `CHANGELOG.md`. La CI refuse de publier sans les deux.
- Page à cocher par recette : `python3 recette/outil/generer_page.py recette/tauri-vX.Y.Z.md …`.
- On ne passe pas à l'étape suivante tant que la recette courante n'est pas **GO**.
- Avant de publier : `cargo clippy --all-targets -- -D warnings` et `cargo test --all` (workspace
  par défaut), plus `cargo clippy -p comicrust-tauri --features app,custom-protocol -- -D warnings` (CI Windows `ci-tauri.yml`).
- Les parités sont jugées sur les mêmes jeux d'essai que la version egui (`recette/echantillons/`,
  bibliothèque réelle du NAS) : une fonction est « portée » quand sa recette Tauri reprend les
  tests [B] de la recette egui correspondante.

## Organisation du dépôt

```
crates/comicrust-core/     inchangé (logique métier)
crates/comicrust/          version egui, inchangée
crates/comicrust-tauri/    commandes IPC, mode portable, thèmes
  src/lib.rs, portable.rs, themes.rs   sans Tauri : testés partout (Linux compris)
  src/app.rs, main.rs                  Tauri : compilés avec --features app (CI Windows)
  themes/*.css                         thèmes livrés (copiés dans data\themes au 1er lancement)
frontend/                  Svelte + TypeScript + Vite
```

Le workspace garde `crates/comicrust-tauri` hors de `default-members` : `cargo build`, `cargo clippy`
et `cargo test` sans `-p` l'ignorent ; `cargo test --all` le compile **sans** la fonction `app`
(donc sans webkit2gtk) et en exécute les tests unitaires.

## Code hors du cœur : on duplique (décision de Yoyo, 02/11/2026)

Une partie de la logique de la version egui vit dans `crates/comicrust` (`library/`, `scripts/`,
`update/`, `book/`). **Décision : on ne déplace rien et on ne touche pas à `crates/comicrust`.**
Le code nécessaire est **dupliqué** dans `crates/comicrust-tauri` (ou repris dans `comicrust-core`
côté branche Tauri uniquement), adapté à l'IPC. Conséquence : une correction de `main` est **reportée
à la main** sur `tauri2-web` quand elle touche du code dupliqué (voir « Reports de correctifs »).

## Décisions complémentaires (02/11/2026)

- Numérotation : `tauri-v0.1.0`, `tauri-v0.2.0`… en parallèle de la série `v0.10.x`.
- Reports de correctifs : les corrections de `main` sont reportées sur `tauri2-web`.
- Thèmes : CSS à variables uniquement pour l'instant (pas de modèles HTML complets).
- Les deux versions (egui sur `main`, Tauri) restent publiées.
- v0.11.0 (OPDS/HTTP) reste sur sa propre branche, après GO ; code du cœur, utilisable par les deux.

## Étapes

Numérotation `tauri-v0.N.0` ; les correctifs d'une recette NO-GO sont des `tauri-v0.N.1`, etc.

### Étape 0 : squelette (`tauri-v0.1.0`)
Crate et front, commandes `infos` / `lister_themes` / `charger_theme`, page d'accueil themée, mode
portable, CI et release Tauri.
**Recette** `recette/tauri-v0.1.0.md` : lancement, `data\` créé sans rien hors du dossier, 3 thèmes, thème
perso ajouté sans recompiler.

### Étape 1 : bibliothèque (`tauri-v0.2.0`)
Premier démarrage (autonome SQLite/MySQL ou ComicRack), ouverture de la base, vue Bibliothèque
(couvertures, grille virtualisée, tableau, recherche, tri, groupements), panneau de détails,
dossiers surveillés (scan, ajout, mise à jour, fichiers manquants), couvertures en cache dans
`data\couvertures`. Protocole de ressources pour les images (pas de base64 dans l'IPC).
**Recette** `recette/tauri-v0.2.0.md` : reprend L-/C- des recettes v0.3.0 à v0.5.x et v0.7.0 ; critère de
volume : 5 000 livres fluides (défilement, recherche).

### Étape 2 : lecteur (`tauri-v0.3.0`)
Ouverture CBZ/CBR/CB7/CBT/PDF/DjVu/images/HEIF/AVIF/WebComic, modes d'affichage (page, double page,
défilement, ajustements, manga), zoom, préchargement, clavier/souris/glisser, avancement de lecture
écrit dans la base, page de reprise.
**Recette** `recette/tauri-v0.3.0.md` : reprend les recettes lecteur (v0.2.x, v0.6.0, v0.9.x).

### Étape 3 : infos et édition (`tauri-v0.4.0`)
Éditeur d'infos (un livre / plusieurs), écriture `ComicInfo.xml` dans les fichiers, base ComicRack
(MySQL / ComicDb.xml), conversion CBR→CBZ, renommage et déplacement (modèles de nom), garde-fou
de modifications en masse (`mass_guard`).
**Recette** `recette/tauri-v0.4.0.md` : reprend R-/D-/E- (v0.4.x, v0.6.0, v0.10.x).

### Étape 4 : pages (`tauri-v0.5.0`)
Onglet Pages : vignettes, réordonner (glisser), types de page, double page, suppression,
« Appliquer » avec écriture dans le fichier, fichiers libérés/rechargés.
**Recette** `recette/tauri-v0.5.0.md` : reprend P-01/P-02 de la v0.10.7 et la non-régression pages.

### Étape 5 : listes ComicRack (`tauri-v0.6.0`)
Listes de lecture, listes intelligentes (éditeur de critères via `matcher_catalog`), dossiers de
listes, import/export `.cbl`, glisser-déposer de livres vers les listes, comparaison avec ComicRack
(47 listes de référence).
**Recette** `recette/tauri-v0.6.0.md` : reprend v0.5.0/v0.5.2/v0.9.0 (46/47 listes identiques).

### Étape 6 : MySQL (`tauri-v0.7.0`)
Connexion MySQL (réglages, mot de passe chiffré `secret`), base de ComicRust (SQLite ou MySQL),
transfert depuis ComicRack, écriture dans la base de ComicRack, comportement hors ligne.
**Recette** `recette/tauri-v0.7.0.md` : reprend v0.7.x, v0.8.0 (5 323 livres, 47 listes, 0 doublon).

### Étape 7 : scripts Python (`tauri-v0.8.0`)
Hôte de scripts (Python embarqué, processus séparé), menu Scripts, formulaires générés,
exécution automatique, application des modifications, scripts d'exemple (Bedetheque, Comic Vine,
nettoyage…). Python embarqué ajouté au paquet (`scripts/python_embarque.ps1`).
**Recette** `recette/tauri-v0.8.0.md` : reprend v0.10.0 à v0.10.7 (scripts d'exemple, Bedetheque).

### Étape 8 : thèmes (`tauri-v0.9.0`)
Au-delà du socle de l'étape 0 : contrat de variables `--cr-*` documenté (`docs/tauri/themes.md`),
couverture de toutes les vues, choix du thème dans les réglages, aperçu en direct, rechargement à
chaud d'un fichier modifié, import/export d'un thème, thèmes Moderne clair/sombre et Classique
fidèles aux équivalents egui, vérification du contraste.
**Recette** `recette/tauri-v0.9.0.md` : thème perso créé sans recompiler et appliqué à **toutes** les vues.

### Étape 9 : mises à jour (`tauri-v0.10.0`)
Mise à jour intégrée depuis les releases `tauri-v*` (vérification, téléchargement, SHA-256,
remplacement par renommage `.old`, `data\` jamais touché, redémarrage), notes de version affichées.
**Recette** `recette/tauri-v0.10.0.md` : mise à jour d'une version à la suivante sur un dossier réel.

### Étape 10 : audit de parité (`tauri-v0.11.0`)
Tableau fonction par fonction egui ↔ Tauri (réglages, raccourcis clavier, glisser-déposer, ligne de
commande `comicrust-cli`, tailles de fenêtre mémorisées, accessibilité, performances, mémoire).
Tout écart est soit corrigé, soit accepté explicitement par Yoyo.
**Recette** `recette/tauri-v0.11.0.md` : la recette de non-régression complète, bloc par bloc.

### Étape 11 : première release (`tauri-v1.0.0`)
Gel des fonctions, documentation (README, aide), décision sur la suite de la version egui
(maintenue, archivée ou remplacée), fusion sur `main`.
**Recette** `recette/tauri-v1.0.0.md`.

## Hors périmètre de la parité

Serveur OPDS / HTTP, associations de fichiers Windows et mode serveur Docker : suivent la feuille
de route principale (README) ; ils seront repris côté Tauri après la `tauri-v1.0.0` si la version
egui les a livrés entre-temps.

## Risques

- **WebView2** : dépendance système (présent sous Windows 11) ; non embarqué dans le zip.
- **Performances** : grille de 5 000+ couvertures et images de pages lourdes ; prévoir virtualisation,
  protocole de ressources dédié et préchargement côté Rust.
- **Portabilité stricte** : Tauri/WebView2 peuvent écrire hors de `data\` (profil, crash dumps) ;
  chaque recette garde le test « rien dans AppData / registre ».
- **Divergence** pendant le développement : le cœur évolue sur `main` ; fusionner `main` dans
  `tauri2-web` régulièrement.
