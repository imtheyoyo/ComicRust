# Recette ComicRust Tauri tauri-v0.7.2

| | |
|---|---|
| Version | tauri-v0.7.2 (pré-release corrective) |
| Paquet | `ComicRust-Tauri-tauri-v0.7.2-windows-x64.zip` (contient aussi `ComicRust-Tauri-mcp.exe`) |
| SHA-256 | `45e0cdc94f87e6d7726af6a650a0786f996f92fb6082d88332574efcfcdc5648` |
| Contenu | menu du clic droit de nouveau fonctionnel (« Modifier les infos »…) ; lecteur : page coupée en haut quand les bandeaux sont rétractés ; poignées au clavier (R-02) ; Détail rafraîchi à la fermeture du lecteur (D-02) ; focus à l'ouverture d'une pile |
| Origine | [recette tauri-v0.7.1](tauri-v0.7.1.md) : 24 OK sur 25, R-02 [M] en échec ([rapport](rapports/tauri-v0.7.1-2026-11-12.txt)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ Installer le nouveau programme dans les **deux copies** (autonome et liée), `data\` conservé.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| 153 tests du backend `comicrust-tauri` (inchangé côté Rust) | `cargo test -p comicrust-tauri` | ✅ 153/153 |
| Front : `svelte-check` (0 erreur), `vite build` | local | ✅ |
| Parcours Playwright en mode simulé (Chromium, proche de WebView2), rejoués sur la version fusionnée : clavier réel des deux poignées (flèches, Début, Fin, double-clic) ; clic droit → « Modifier les infos », « Renommer », « Ouvrir la pile » en grille, tableau, sélection multiple, pile, pile ouverte ; focus à l'ouverture et au retour d'une pile ; lecteur 1366×768 et 1920×1080, avec et sans miniatures, simple et double page, tous ajustements, bandeaux masqués, redimensionnement, défilement continu ; Détail rafraîchi à la fermeture | local | ✅ (33 + 0 échec + 1) |
| Rustfmt, Clippy `-D warnings` ; compilation de la fenêtre et variante `-mcp` | local + CI Windows | ✅ local ; CI à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2.

## 1. Installation

- [ ] **P-01 [B]** Dans **chacune** des deux copies : « ⚙ Réglages » indique **Version 0.7.2**. Noter : ……… / ………

## 2. Corrections

- [ ] **C-01 [B]** Clic droit sur un livre → **« Modifier les infos »** : la fenêtre Infos s'ouvre. Idem en vue **tableau**, sur **plusieurs livres** sélectionnés, sur une **pile** et dans une **pile ouverte**.
- [ ] **C-02 [B]** Les autres entrées du menu du clic droit agissent (ex. « Ajouter à une liste », « Renommer » → la boîte s'ouvre ; annuler).
- [ ] **C-03 [B]** **Lecteur** : laisser les bandeaux haut et bas se rétracter, puis passer à la page suivante plusieurs fois (flèche, clic, molette) : la page reste **entière, rien n'est coupé en haut**. Refaire avec les miniatures affichées, en double page et en plein écran.
- [ ] **C-04 [M]** **R-02** : cliquer la poignée de la **colonne des listes**, puis flèches gauche/droite (Maj = pas plus grand), **Début**, **Fin** : la colonne change de largeur. Double-clic : largeur par défaut.
- [ ] **C-05 [m]** Même chose avec la poignée du **panneau Détail** (flèches, Début, Fin, double-clic).
- [ ] **C-06 [M]** **D-02** : lire quelques pages d'un livre puis fermer le lecteur : le panneau Détail affiche aussitôt « Lu dans ComicRust : x % (page n / total) », **sans resélectionner** le livre.
- [ ] **C-07 [m]** Ouvrir une pile (double-clic ou Entrée) : le premier livre est sélectionné et les **flèches marchent tout de suite** ; Échap : retour aux piles, la pile quittée est sélectionnée et visible.

## 3. Non-régression

- [ ] **N-01 [B]** Bibliothèque empilée et non empilée : grille, tableau, recherche, tri, groupement, couvertures ; glisser de livres et de piles vers une liste.
- [ ] **N-02 [B]** Lecteur : ouvrir un CBZ et un PDF, tourner les pages, modes d'ajustement, progression enregistrée.
- [ ] **N-03 [B]** Listes : mêmes comptes qu'à la tauri-v0.7.1 ; tri/vue par liste et largeur de colonne mémorisés.
- [ ] **N-04 [M]** Ctrl+I, Suppr dans une liste de lecture, Échap dans un dialogue : comme avant.
- [ ] **N-05 [m]** Copie liée : pas de bandeau « Base injoignable » ; réglages conservés après relance.

## Limites connues (acceptées pour cette version)

- Reprises de la tauri-v0.7.1 : pas de réordonnancement des piles par glisser dans une liste de lecture ; Détail vide quand une pile est sélectionnée ; un livre lu seulement dans ComicRust n'apparaît « lu » que dans le Détail ; jaune des étoiles fixe.
- Reprises de la tauri-v0.7.0 : pas d'écran « Changer de connexion » dans les Réglages ; annulation du transfert seulement entre deux lots.

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| C-06 | [M] | Détail rafraîchi avec le bouton Fermer, **pas avec Échap** ; un livre marqué lu puis relu n'affiche pas la relecture en cours (x % page n / total) | tauri-v0.7.3 |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | 7 | 7 | 0 |
| [M] majeurs | 3 | 2 | 1 (C-06) |
| [m] mineurs | 3 | 3 | 0 |

Rapport de Yoyo du 12/11/2026 19:01 ([rapport](rapports/tauri-v0.7.2-2026-11-12.txt)) : 12 OK, 1 échec (C-06 [M]), tous les bloquants OK. Demande d'ajout : contrôle et mise à jour automatique des nouvelles versions.

**Décision : GO** confirmé par Yoyo le 12/11/2026 (20:07), C-06 [M] accepté, corrigé en tauri-v0.7.3.
