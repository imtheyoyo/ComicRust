# Recette ComicRust Tauri tauri-v0.9.1

| | |
|---|---|
| Version | tauri-v0.9.1 (pré-release corrective) |
| Paquet | `ComicRust-Tauri-tauri-v0.9.1-windows-x64.zip` |
| SHA-256 | `17a9ef24254e0e55f842da7a1763535b6387c18d75380cd50fa5c1679fccbb72` |
| Contenu | sélection par rectangle ; onglet « Données » de Configurer BD2 lisible ; aide de « Ranger les fichiers » ; base absente → assistant |
| Origine | [recette tauri-v0.9.0](tauri-v0.9.0.md) : **GO** du 14/11/2026 ; remarques de Yoyo ([rapport](rapports/tauri-v0.9.0-2026-11-14.txt)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Installation par la mise à jour intégrée de ta 0.9.0 (copie autonome, copie liée, et si tu veux la
> copie de test « ComicRust-Tauri RECETTE 0.9.0 »). Les tests de rangement : sur les échantillons, jamais sur le NAS.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **202 tests** du backend dont : vrais formulaires des scripts exécutés avec Python (Configurer BD2, QuickScrape, Library Organizer) — tous les types de contrôles connus de l'interface ; copie autonome sans base → assistant proposé, rien créé sans clic, jamais si une base MySQL est réglée ou si l'emplacement est occupé | `cargo test -p comicrust-tauri` | ✅ 202/202 |
| 11 tests des scripts d'exemple (dont les réglages de « Ranger les fichiers ») | `cargo test -p comicrust-core --test scripts_exemples` | ✅ 11/11 |
| Parcours simulés : rectangle (4 livres, Ctrl, Maj, Échap, clic dans le vide, défilement auto avec un livre hors de l'écran, tableau, pile ouverte, thème sombre), glisser vers une liste et L-08 inchangés ; onglet « Données » de la vraie spec Configurer BD2 (liste 410×330 au lieu de 38 px) ; écran « base introuvable » | local, Playwright | ✅ |
| Rustfmt, Clippy `-D warnings` ; compilation Windows | local + CI | ✅ local ; CI à la publication |

## 1. Installation

- [ ] **P-01 [B]** Ta 0.9.0 propose la 0.9.1 ; « Installer » puis « Redémarrer » → ⚙ Réglages : **Version 0.9.1** (dans chacune de tes deux copies). Noter : ……… / ………

## 2. Sélection par rectangle

- [ ] **R-01 [B]** Grille de couvertures : cliquer dans un **espace vide** puis glisser : un rectangle apparaît et les livres touchés sont sélectionnés au fur et à mesure ; relâcher : la sélection reste.
- [ ] **R-02 [M]** **Maj** + rectangle : ajoute à la sélection ; **Ctrl** + rectangle : inverse les livres touchés ; **Échap** pendant le geste : la sélection d'avant revient.
- [ ] **R-03 [M]** Glisser vers le bas de la fenêtre pendant le geste : la vue défile toute seule et la sélection continue.
- [ ] **R-04 [B]** Glisser **un livre** vers une liste de lecture fonctionne toujours (le rectangle ne part que du vide) ; clic, Ctrl+clic, Maj+clic, clic droit : comme avant.
- [ ] **R-05 [m]** Un simple clic dans le vide vide la sélection ; dans une pile ouverte et sous les lignes du tableau, le rectangle marche aussi.

## 3. Scripts

- [ ] **F-01 [B]** 📜 Scripts → « Configurer BD2 » → onglet **Données** : la liste des champs est **visible et lisible** (cases à cocher, compteur, « Tout cocher / Tout décocher ») ; parcourir les autres onglets : rien n'est écrasé.
- [ ] **F-02 [M]** « Library Organizer » → configuration : tous les onglets lisibles.
- [ ] **F-03 [M]** « Ranger les fichiers » → Configurer : la première question explique les champs utilisables (`{series}`, `{volume}`, `{number:000}`, `[ v{volume}]`…) avec un exemple ; la deuxième montre le **nom obtenu sur un de tes livres**.

## 4. Base absente

- [ ] **D-01 [m]** Dans la copie de test (pas ta copie habituelle), renommer `data\comicrust.db` en `comicrust.db.bak` puis lancer : écran « Base ComicRust introuvable » avec **« Créer une nouvelle base »** ; fermer sans cliquer ; remettre `comicrust.db` : la bibliothèque revient intacte.

## 5. Non-régression

- [ ] **N-01 [B]** Un script avec aperçu puis application (ex. « Nettoyer les fiches » sur 2 livres) ; Bedetheque sur un album.
- [ ] **N-02 [B]** Bibliothèque, listes, lecteur : comme à la tauri-v0.9.0.

## Limites connues (acceptées pour cette version)

- Pas de rectangle sur les **piles fermées** (une seule pile sélectionnée à la fois, comme dans la version egui).
- Dans le tableau, les lignes prennent toute la largeur : le rectangle ne part que du vide **sous** la dernière ligne.
- Reprises de la tauri-v0.9.0 : pas de bouton « Parcourir… » dans les formulaires de scripts ; automatismes sans contrôle d'un transfert en cours.

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | | | |
| [M] majeurs | | | |
| [m] mineurs | | | |

**Décision :** ☐ GO ☐ NO-GO — Yoyo, le …
