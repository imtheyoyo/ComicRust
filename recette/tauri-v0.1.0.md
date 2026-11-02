# Recette ComicRust Tauri tauri-v0.1.0

| | |
|---|---|
| Version | tauri-v0.1.0 (pré-release : squelette de la version Tauri 2, aucune fonction de ComicRust portée) |
| Paquet | `ComicRust-Tauri-tauri-v0.1.0-windows-x64.zip` |
| SHA-256 | à renseigner après la publication (fichier `.sha256` de la release) |
| Contenu | fenêtre Tauri 2 + Svelte, mode portable (`data\`), thèmes CSS dans `data\themes\` |
| Origine | étape 0 de `docs/tauri/PLAN.md` |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| Thèmes : installation sans écrasement, liste, nom `@nom`, refus des identifiants avec chemin (`..`, `\`, `/`), variables CSS requises présentes dans chaque thème livré | tests `comicrust-tauri` | à confirmer par la CI |
| Front : `svelte-check` (0 erreur), `vite build` | CI Windows (`ci-tauri.yml`) | à confirmer par la CI |
| Clippy `-D warnings`, compilation de l'application complète | CI Windows (`ci-tauri.yml`) | à confirmer par la CI |
| Tests, clippy de la version egui (inchangée) | `ci.yml` Linux + Windows | ✅ en local (Linux) |

**Non vérifiable ici** : tout ce qui touche à la fenêtre et au WebView2 (section 1).

## 1. Tests

- [ ] **T-01 [B]** Décompresser le zip dans un dossier **vide**, lancer `ComicRust-Tauri.exe` : une fenêtre « ComicRust » s'ouvre, la page d'accueil affiche « v0.1.0 (Tauri) » et le thème **Moderne clair**.
- [ ] **T-02 [B]** Portable : à côté de l'exe, le dossier `data\` contient `themes\` (3 fichiers `.css`) et `webview2\`. Aucun dossier `ComicRust` ni `fr.imtheyoyo.comicrust` n'est créé dans `%APPDATA%` ni `%LOCALAPPDATA%` ; aucune clé de registre `ComicRust` (HKCU\Software).
- [ ] **T-03 [B]** Menu Thème : choisir **Moderne sombre**, puis **Classique** : les couleurs, la police et les angles changent immédiatement. Fermer, relancer : le dernier thème est repris.
- [ ] **T-04 [B]** Copier `data\themes\moderne-sombre.css` en `data\themes\mon-theme.css`, y changer `/* @nom: … */` et la couleur `--cr-accent` ; **Actualiser la liste** : « … (perso) » apparaît, sans redémarrer ; le choisir applique la couleur.
- [ ] **T-05 [M]** Supprimer `mon-theme.css` alors qu'il est le thème mémorisé, relancer : retour à Moderne clair, sans message d'erreur bloquant.
- [ ] **T-06 [M]** Supprimer `data\themes\classique.css` puis « Actualiser la liste » : le fichier est recréé (thème livré).
- [ ] **T-07 [m]** Déplacer tout le dossier ailleurs (autre disque, clé USB) : relance sans rien perdre (thème mémorisé, `data\` suivi).
- [ ] **T-08 [m]** La fenêtre se redimensionne ; en dessous de 800 x 500 elle ne rétrécit plus ; la page reste lisible.
- [ ] **N-01 [B]** Non-régression : la version egui (`ComicRust.exe` de la dernière release `vX.Y.Z`) n'est pas touchée et fonctionne comme avant.

---

## Limites connues

- Aucune fonction de ComicRust n'est encore portée : bibliothèque, lecteur, listes, etc. suivent le plan (`docs/tauri/PLAN.md`).
- WebView2 doit être présent (Windows 11 : intégré). Il n'est pas embarqué dans le zip.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 4 | | |
| Majeurs [M] | 2 | | |
| Mineurs [m] | 2 | | |

Décision et remarques :
