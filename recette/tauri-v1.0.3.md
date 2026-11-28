# Recette ComicRust Tauri tauri-v1.0.3

| | |
|---|---|
| Version | tauri-v1.0.3 (petits correctifs issus de la recette de la tauri-v1.0.2) |
| Paquet | `ComicRust-Tauri-tauri-v1.0.3-windows-x64.zip` |
| SHA-256 | `(à renseigner à la publication)` |
| Contenu | « Publicité » pour le type ComicRack « Advertisment », raison de lecture seule propre aux fichiers, tri « Avancement » (jamais ouverts = 0 %), série déduite du nom de fichier sans mot tronqué, dépôt en fin de liste dans l'espace vide, README du paquet complété |
| Origine | [recette tauri-v1.0.2](tauri-v1.0.2.md) : **GO** du 28/11/2026 ; anomalies mineures notées à cette recette |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Recette déroulée par Claude via le MCP (copies autonome et liée, fichiers et listes de test de
> `Echantillons RECETTE\_recette_1.0.2`), Yoyo pour les tests avec ComicRack ou l'Explorateur.
> **Première mise à jour faite par le nouvel installateur « tout ou rien » de la 1.0.2** (MJ-01).

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| Tests du backend et du cœur, dont : synonyme « Advertisment » (libellé, comparaisons, opérations sur les pages sans réécriture), raison « fichiers » en lecture seule, tri « Avancement », noms de fichiers (« Recette102 - Ordre des pages », « Spirou 12 », « Thanos v1 #20 », « XIII - 05 - Rouge total », « 2000AD 1234 », « Blake & Mortimer 01 ») | `cargo test` | ✅ |
| Front : `svelte-check`, `vite build` ; Playwright simulé : dépôt d'une pile et de livres en fin de liste, raisons grisées, non-régression des tests 1.0.2 | local | ✅ |
| Rustfmt, Clippy `-D warnings` dont la fenêtre (feature `app`) | local + CI | ✅ |

## 1. Installation et correctifs

- [ ] **P-01 [B]** Ta 1.0.2 propose la 1.0.3 ; « Installer » puis « Redémarrer » → titre « ComicRust 1.0.3 » et À propos : **Version 1.0.3** (deux copies). Noter : ……… / ………
- [ ] **MJ-01 [M]** Pendant l'installation de la mise à jour (sur **une** des deux copies), ferme la fenêtre : message « Installation en cours : ComicRust se fermera à la fin », l'application se ferme seule à la fin, aucun `ComicRust-Tauri.exe` ne reste dans le Gestionnaire des tâches, et la copie redémarre normalement en 1.0.3.
- [ ] **PU-01 [M]** Livre de test « Types de page ComicRack » : onglet Pages → la 3ᵉ page s'affiche **« Publicité »** ; changer le type d'une page puis l'annuler fonctionne (plus de « type inconnu »).
- [ ] **LS-03 [M]** Copie liée en lecture seule : la raison sur « Écrire les infos », « Convertir » et « Renommer / déplacer » parle des **fichiers des livres** ; « Marquer comme lu » garde la raison sur les fiches.
- [ ] **TA-01 [M]** Tri « Avancement » croissant : les livres **jamais ouverts** viennent en tête (avec les 0 %), les lus en fin ; décroissant : l'inverse.
- [ ] **NS-01 [M]** Nouveau fichier de test « Recette103 - Nom de série.cbz » analysé : série « Recette103 - Nom de série » (aucun mot tronqué, pas de n° 103).
- [ ] **PL-01 [M]** Liste de lecture de test, « Ordre de la liste » + « Empiler par série » : déposer une pile dans l'espace vide à droite de la dernière pile (ou sous la dernière rangée) la place **en fin de liste** ; déposer entre deux piles reste inchangé ; même chose pour des livres sans « Empiler par série ».
- [ ] **DO-01 [m]** `README.md` du paquet : décrit jauges, sous-menu Scripts, dernière liste, instance unique, seuil « lu » à 95 %, lecture seule des fichiers, sauvegardes de ComicDb.xml dans `data\`, mise à jour « tout ou rien ».

## 2. Non-régression (abrégée)

- [ ] **N-01 [B]** Bibliothèque : grille, tableau, recherche, tris, regroupements, sélection suivie après « Marquer comme lu », jauges des listes.
- [ ] **N-02 [B]** Listes : piles réordonnables (glisser, Monter / Descendre, Alt+↑/↓), glisser de livres vers une liste, livres manquants (marquage, relocalisation, retrait).
- [ ] **N-03 [B]** Lecteur : pages dans l'ordre (2, 9, 010, 011), livre suivant, reprise, focus gardé dans le lecteur, F11.
- [ ] **N-04 [M]** Instance unique (double-clic sur un CBZ avec ComicRust ouvert), dernière liste rouverte, serveur MCP sans jeton.
- [ ] **N-05 [M]** Copie liée : lecture seule complète (fiches et fichiers), diagnostic sans mot de passe.

## Limites connues (acceptées pour cette version)

- Mêmes limites que la [tauri-v1.0.2](tauri-v1.0.2.md).
- Série déduite du nom de fichier : **écart volontaire** avec ComicRack, qui tronque « Recette102 - … » en « Re - … » n° 102 ; seuls les livres nouvellement analysés (ou relus) en profitent.
- « XIII - 05 - Rouge total » donne la série « XIII -  - Rouge total » (deux espaces), comme ComicRack.

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

**Décision : (à remplir)**
