# Recette ComicRust Tauri tauri-v0.7.1

| | |
|---|---|
| Version | tauri-v0.7.1 (pré-release corrective de l'étape 6 : **piles par série, Détail, colonne des listes**) |
| Paquet | `ComicRust-Tauri-tauri-v0.7.1-windows-x64.zip` (contient aussi `ComicRust-Tauri-mcp.exe`) |
| SHA-256 | à renseigner à la publication |
| Contenu | **vue empilée par série** (écart de parité N-01 de la tauri-v0.7.0) ; **progression de lecture** et **note en étoiles** dans le panneau Détail (N-02) ; **colonne des listes redimensionnable** ; clic droit dans une sélection multiple |
| Origine | [recette tauri-v0.7.0](tauri-v0.7.0.md) : **GO** de Yoyo le 12/11/2026 ; rapports [1](rapports/tauri-v0.7.0-2026-11-12.txt) et [2](rapports/tauri-v0.7.0-2026-11-12-b.txt) (N-01 : « le regroupement n'empile pas les livres » → piles) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ Installer le nouveau programme dans les **deux copies** (autonome et liée), `data\` conservé.
> Les tests des piles se font sur la copie **liée** (5 3xx livres) ; ComicRack reste en lecture seule.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **153 tests** du backend `comicrust-tauri` : piles par série (ordre, comptes de livres et de lus, recherche, tri décroissant, pile ouverte), réglage « empiler » mémorisé par liste et lu de façon tolérante, progression de lecture (fiche, lecture faite dans ComicRust plus avancée), largeur de la colonne des listes bornée et tolérante | `cargo test -p comicrust-tauri` | ✅ 153/153 |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours de la vue empilée en mode simulé (clic, double-clic, Entrée, Échap, glisser, menu, sélection multiple au clic droit) | local, Playwright | ✅ |
| Rustfmt, Clippy `-D warnings` ; compilation de la fenêtre et variante `-mcp` | local + CI Windows | ✅ local ; CI à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2 (rendu des piles, étoiles, poignée, glisser), ta vraie bibliothèque.

## 1. Installation

- [ ] **P-01 [B]** Dans **chacune** des deux copies : « ⚙ Réglages » indique **Version 0.7.1**. Noter : ……… / ………
- [ ] **P-02 [B]** Les réglages d'avant s'ouvrent comme avant (tri, vue, thème conservés).

## 2. Vue empilée par série

- [ ] **K-01 [B]** Bibliothèque en grille : bouton « 📚 Empiler par série » visible ; activé, la grille affiche **une pile de couvertures par série** (couvertures décalées, nombre de livres, barre de lecture). Absent en vue Tableau.
- [ ] **K-02 [B]** Survoler une pile : infobulle « N livres, M lus » ; la barre de lecture correspond au nombre de livres lus.
- [ ] **K-03 [B]** **Un clic** sélectionne la pile (cadre) sans l'ouvrir ; **double-clic** ou **Entrée** l'ouvre : ses livres s'affichent. « ← Piles » ou **Échap** revient aux piles en gardant la sélection.
- [ ] **K-04 [B]** La recherche et le tri s'appliquent aux piles (chercher « Silver » ne garde que les séries concernées ; les piles suivent le tri).
- [ ] **K-05 [M]** Glisser une **pile** vers une liste de lecture : tous ses livres y sont ajoutés, **une seule fois** (vérifier le nombre dans la liste) ; le badge de glisser affiche le vrai nombre de livres.
- [ ] **K-06 [M]** Menu contextuel d'une pile : ouvrir, infos, ajouter à une liste, … s'appliquent à **tous les livres** de la pile.
- [ ] **K-07 [M]** Le réglage est **mémorisé** : Bibliothèque empilée, une liste de lecture non empilée (par défaut), une liste réglée garde son choix ; relancer ComicRust retrouve tout.
- [ ] **K-08 [m]** Navigation aux flèches entre les piles ; livre sans série regroupé sous « (sans série) ».
- [ ] **K-09 [m]** Liste de lecture : l'ordre des livres s'obtient en ouvrant la pile (pas de glisser de piles, comme noté dans les limites).

## 3. Panneau Détail du livre

- [ ] **D-01 [B]** Un livre partiellement lu : le Détail indique « Lu : x % (page n / total) » avec une barre ; un livre non lu : « Non lu » ; un livre terminé : « Lu ».
- [ ] **D-02 [B]** Lire quelques pages d'un livre dans ComicRust puis fermer : le Détail indique « Lu dans ComicRust : … » (la lecture la plus avancée gagne).
- [ ] **D-03 [M]** **Note en étoiles** : 5 étoiles, jaunes pour la note et grises pour le reste (demi-étoile possible) ; vérifier aussi en thème **sombre** et **clair** (lisible, contrasté). Un livre sans note : 5 étoiles grises.
- [ ] **D-04 [m]** Survol/lecteur d'écran : « Note : 3,5 sur 5 ».

## 4. Colonne des listes redimensionnable

- [ ] **R-01 [B]** Glisser la poignée entre la colonne des listes et la vue : la colonne s'élargit/rétrécit, avec des bornes (ni écrasée ni envahissante).
- [ ] **R-02 [M]** **Double-clic** sur la poignée : largeur par défaut. Au clavier (poignée ayant le focus) : flèches, Début, Fin.
- [ ] **R-03 [M]** La largeur est **retrouvée après relance** ; `data\comicrust-tauri.json` contient `largeurListes` dans `vue`.

## 5. Sélection multiple et clic droit

- [ ] **S-01 [B]** En grille **non empilée** et en tableau : Maj+clic / Ctrl+clic sélectionnent plusieurs livres ; un **clic droit sur un livre de la sélection** garde la sélection (le menu indique « les N livres ») ; un clic droit **hors sélection** sélectionne ce livre seul.
- [ ] **S-02 [M]** Le glisser de plusieurs séries (Ctrl+clic sur deux étiquettes, glisser vers une liste) ajoute bien les deux séries (L-08).

## 6. Non-régression

- [ ] **N-01 [B]** Bibliothèque **non empilée** : grille, tableau, recherche, tri, groupement par série (en-têtes), couvertures comme à la tauri-v0.7.0.
- [ ] **N-02 [B]** Lecteur : ouvrir un CBZ et un PDF, tourner les pages, progression enregistrée.
- [ ] **N-03 [B]** Listes (intelligentes et de lecture) : mêmes comptes qu'à la tauri-v0.7.0 ; tri/vue par liste mémorisés.
- [ ] **N-04 [M]** Connexion MySQL / transfert / bandeau hors ligne de la tauri-v0.7.0 : toujours fonctionnels (contrôle rapide : la copie liée affiche ses livres, pas de bandeau « injoignable »).
- [ ] **N-05 [m]** Réglages (thème, vue) conservés après relance.

## Limites connues (acceptées pour cette version)

- Pas de réordonnancement des piles par glisser dans une liste de lecture (egui le permet) : il se fait en ouvrant la pile.
- Le panneau Détail reste vide quand une **pile** est sélectionnée (il se remplit pour un livre).
- Un livre lu **seulement dans ComicRust** (`avancement.json`) apparaît « lu » dans le panneau Détail, mais pas encore dans le tableau, la grille ni le compte « M lus » d'une pile (ils lisent la fiche) ; à harmoniser plus tard.
- Le jaune des étoiles est fixe (variable `--cr-etoile` non définie par les thèmes).
- Reprises de la tauri-v0.7.0 : pas d'écran « Changer de connexion » dans les Réglages ; annulation du transfert seulement entre deux lots.

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| (relecture) | [M] | Pile ouverte sans résultat de recherche : plus de bouton « ← Piles » | **Corrigé** (barre affichée aussi dans ce cas) |
| (relecture) | [M] | Sélection de livres périmée en activant « Empiler », puis Suppr retirait des livres invisibles | **Corrigé** (sélection vidée ; Suppr agit sur la pile sélectionnée) |
| (relecture) | [m] | Infos depuis une pile : bouton Suivant actif sans effet | **Corrigé** |
| | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | | | |
| [M] majeurs | | | |
| [m] mineurs | | | |

**Décision :** ☐ GO ☐ NO-GO — Yoyo, le …
