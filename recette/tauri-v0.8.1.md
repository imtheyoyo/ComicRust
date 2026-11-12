# Recette ComicRust Tauri tauri-v0.8.1

| | |
|---|---|
| Version | tauri-v0.8.1 (pré-release corrective) |
| Paquet | `ComicRust-Tauri-tauri-v0.8.1-windows-x64.zip` (contient aussi `ComicRust-Tauri-mcp.exe`) |
| SHA-256 | `69c5adf74dfc24282256affd0092cdddbc4b68143341582a9016f29efcd15cdc` |
| Contenu | C-05 : reprise d'un livre déjà lu ; **première vraie mise à jour** 0.8.0 → 0.8.1 par la mise à jour intégrée |
| Origine | [recette tauri-v0.8.0](tauri-v0.8.0.md) ; précision de Yoyo sur C-05 : « J'ai fermé le premier livre et ouvert le second » |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Cette fois, **ne copie pas le zip à la main** : la 0.8.1 s'installe par la mise à jour intégrée de
> la 0.8.0 (copie autonome, puis copie liée). Lancement normal, sans `MAJ.cmd`.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **181 tests** du backend, dont le scénario C-05 réel (livre A lu, fermé ; livre B ouvert, fermé ; Détail de A) en mode autonome et en mode lié ComicRack, et un livre lu rouvert à sa dernière page puis relu (« relecture en cours ») | `cargo test -p comicrust-tauri` | ✅ 181/181 |
| Cause de C-05 établie sur ta copie autonome (lecture seule, outil MCP `detail_livre`) : le premier livre était déjà lu ; rouvert à sa dernière page, il était compté « terminé » au bout de 800 ms | MCP | ✅ |
| Parcours Playwright en mode simulé (simulation corrigée pour reprendre à la page mémorisée comme le vrai backend) : sans le correctif le Détail reste « Lu », avec le correctif « relecture en cours : 10 % (page 6 / 59) » ; scénarios C-05 (0.8.0) et C-06 inchangés | local | ✅ |
| Rustfmt, Clippy `-D warnings` ; compilation Windows et variante `-mcp` | local + CI | ✅ local ; CI à la publication |

## 1. Mise à jour réelle 0.8.0 → 0.8.1

- [ ] **U-01 [B]** Copie **autonome** en 0.8.0, lancement normal : le bandeau annonce **« ComicRust tauri-v0.8.1 est disponible »** ; « Nouveautés… » montre les notes de la 0.8.1.
- [ ] **U-02 [B]** « Installer » puis « Redémarrer maintenant » : l'appli revient seule en **Version 0.8.1** (⚙ Réglages), avec ta bibliothèque, tes listes et tes réglages ; plus de fichiers `.old` dans le dossier.
- [ ] **U-03 [M]** Même chose pour la copie **liée**, lancée en MCP (`ComicRust-Tauri-mcp.exe --mcp=47803`) : retour en mode MCP sur le même port ; après **relance de Claude Desktop**, `infos` donne 0.8.1.
- [ ] **U-04 [m]** Relancer : plus de bandeau (0.8.1 est la plus récente).

## 2. Correction C-05

- [ ] **C-01 [B]** Prendre un livre **déjà lu** : l'ouvrir (il reprend à sa dernière page), revenir quelques pages en arrière, fermer. Le Détail affiche « Lu — relecture en cours : x % (page n / total) ».
- [ ] **C-02 [M]** Ton scénario : fermer ce premier livre, ouvrir un second, lire, fermer ; resélectionner le premier : sa page lue (relecture) est toujours affichée.
- [ ] **C-03 [m]** Rouvrir un livre lu et le refermer **sans tourner de page** : il reste « Lu », sans relecture.

## 3. Non-régression

- [ ] **N-01 [B]** Livre non lu : lire quelques pages, fermer par Échap : « Lu dans ComicRust : x % (page n / total) » (C-06).
- [ ] **N-02 [B]** Bibliothèque, listes, lecteur (CBZ et PDF) : comme à la tauri-v0.8.0.
- [ ] **N-03 [m]** Aller jusqu'à la dernière page d'un livre non lu : il devient « Lu ».

## Limites connues (acceptées pour cette version)

- Celles de la [tauri-v0.8.0](tauri-v0.8.0.md) (pas de retour arrière global si un fichier est verrouillé pendant l'installation ; relancer Claude Desktop après une mise à jour ; variables de recette actives).

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | 5 | 5 | 0 |
| [M] majeurs | 2 | 2 | 0 |
| [m] mineurs | 3 | 3 | 0 |

Rapport de Yoyo du 12/11/2026 21:25 ([rapport](rapports/tauri-v0.8.1-2026-11-12.txt)) : 10 OK sur 10 ; première vraie mise à jour 0.8.0 → 0.8.1 réussie ; C-05 corrigé.

**Décision :** GO proposé (tous les tests OK) — en attente de la confirmation de Yoyo.
