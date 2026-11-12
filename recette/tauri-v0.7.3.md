# Recette ComicRust Tauri tauri-v0.7.3

| | |
|---|---|
| Version | tauri-v0.7.3 (pré-release corrective) |
| Paquet | `ComicRust-Tauri-tauri-v0.7.3-windows-x64.zip` (contient aussi `ComicRust-Tauri-mcp.exe`) |
| SHA-256 | `4833f8e4951daa2740260874fb9105292505de472e88aed9258dacbb8d8ce9be` |
| Contenu | C-06 : Détail rafraîchi quelle que soit la façon de fermer le lecteur (Échap compris) ; relecture d'un livre déjà lu affichée dans le Détail |
| Origine | [recette tauri-v0.7.2](tauri-v0.7.2.md) : 12 OK sur 13, C-06 [M] en échec ([rapport](rapports/tauri-v0.7.2-2026-11-12.txt)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ Installer le nouveau programme dans les **deux copies** (autonome et liée), `data\` conservé.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **159 tests** du backend `comicrust-tauri` dont la fusion lecture fiche / ComicRust (relecture, lecture plus avancée, fiche seule, 0 page) et le champ `relecture` de `avancement.json` (anciens fichiers compatibles) | `cargo test -p comicrust-tauri` | ✅ 159/159 |
| Front : `svelte-check` (0 erreur), `vite build` | local | ✅ |
| Parcours Playwright en mode simulé, rejoué sur la version fusionnée : fermeture par Échap, bouton, Ctrl+W, Échap immédiat après la dernière page, **backend lent** (Échap pendant l'enregistrement) ; relecture affichée, reprise à la page de relecture, terminée en revoyant la fin | local | ✅ 10/10 |
| Rustfmt, Clippy `-D warnings` ; compilation de la fenêtre et variante `-mcp` | local + CI Windows | ✅ local ; CI à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2.

## 1. Installation

- [ ] **P-01 [B]** Dans **chacune** des deux copies : « ⚙ Réglages » indique **Version 0.7.3**. Noter : ……… / ………

## 2. Corrections (C-06)

- [ ] **C-01 [B]** Lire quelques pages d'un livre non lu puis fermer avec **Échap** juste après avoir tourné la page : le Détail affiche aussitôt « Lu dans ComicRust : x % (page n / total) » avec la bonne page.
- [ ] **C-02 [B]** Même chose avec le **bouton Fermer** et avec **Ctrl+W**.
- [ ] **C-03 [M]** Ouvrir un livre **déjà marqué lu**, lire quelques pages, fermer : le Détail affiche « Lu — relecture en cours : x % (page n / total) » et la barre suit la relecture ; le livre reste coché lu dans la grille.
- [ ] **C-04 [M]** Rouvrir ce livre : la lecture reprend à la page de la relecture ; aller jusqu'à la dernière page puis fermer : le Détail indique de nouveau « Lu » sans relecture.
- [ ] **C-05 [m]** Ouvrir un autre livre à la place du premier (sans fermer) puis fermer : le Détail du premier est à jour quand on le resélectionne.
- [ ] **C-06 [m]** Copie liée : la fiche ComicRack du livre relu n'est pas modifiée par la relecture (contrôle dans ComicRack, ou la colonne « Lu » inchangée).

## 3. Non-régression

- [ ] **N-01 [B]** Lecteur : CBZ et PDF, page suivante bandeaux rétractés (rien de coupé), modes d'ajustement, double page.
- [ ] **N-02 [B]** Clic droit → « Modifier les infos » et autres entrées du menu ; Ctrl+I.
- [ ] **N-03 [B]** Bibliothèque empilée et non empilée, listes, poignées au clavier : comme à la tauri-v0.7.2.
- [ ] **N-04 [m]** Réglages et largeurs conservés après relance.

## Limites connues (acceptées pour cette version)

- Reprises : pas de réordonnancement des piles par glisser ; Détail vide pour une pile ; un livre lu seulement dans ComicRust n'apparaît « lu » que dans le Détail ; jaune des étoiles fixe ; pas d'écran « Changer de connexion » ; annulation du transfert entre deux lots.
- Mise à jour automatique : avancée par Yoyo le 12/11/2026, ce sera la tauri-v0.8.0.

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| C-05 | [m] | Ouvrir un autre livre à la place du premier (sans le fermer) : la page lue du premier n'est pas mise à jour dans son Détail | tauri-v0.8.0 |
| C-06 | [m] | Non testé (fiche ComicRack inchangée après une relecture) | — |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | 6 | 6 | 0 |
| [M] majeurs | 2 | 2 | 0 |
| [m] mineurs | 3 | 1 | 1 (C-05) + 1 non testé (C-06) |

Rapport de Yoyo du 12/11/2026 20:07 ([rapport](rapports/tauri-v0.7.3-2026-11-12.txt)) : 9 OK, 1 échec mineur (C-05), 1 non testé (C-06 [m]) ; tous les bloquants et majeurs OK.

**Décision : GO** confirmé par Yoyo le 12/11/2026 (20:13) ; C-05 [m] corrigé en tauri-v0.8.0.
