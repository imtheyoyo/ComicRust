# Audit de parité egui (main v0.10.9) ↔ Tauri (tauri-v0.10.0)

Audit du 14/11/2026 (étape 10 du plan). Préfixes de preuve : **E** = `crates/comicrust/src/` (main),
**T** = `crates/comicrust-tauri/src/`, **F** = `frontend/src/`. Statuts : ✅ identique, 🟡 partiel,
❌ absent, ➕ nouveauté Tauri.

## 0. Delta main / Tauri
- La branche Tauri est née de main à v0.10.7. Depuis, main n'a que v0.10.8 (recherche rapide fidèle à
  ComicRack, déjà dans Tauri : `T/tri.rs::filtrer`) et v0.10.9 (serveur MCP, équivalent : `T/mcp/`).
- **Aucune fonction egui livrée après le démarrage de la branche n'est sans équivalent Tauri.**
  OPDS/HTTP, associations de fichiers et Docker restent hors périmètre (PLAN.md).
- Le README de la branche Tauri est celui d'egui (il cite `ComicRust.exe` et `comicrust-cli.exe`).
- Attention : « tauri-v0.11.0 » = audit de parité ; « v0.11.0 » (sans préfixe) = OPDS côté egui.

## 1. Tableau de parité

### Réglages
| Fonction | Statut | Remarque |
|---|---|---|
| Thème | ➕ / 🟡 | 3 thèmes CSS + perso ; « Comme Windows » absent (`prefers-color-scheme`) |
| Assistant de configuration rejouable (changer autonome/ComicRack/base) | ❌ | `PremierDemarrage.svelte` seulement au 1er démarrage ou sur erreur |
| Transfert ComicRack, MAJ, Scripts | ✅ | |
| « Suivre les modifs faites dans ComicRack » (`auto_refresh`) | ❌ | surveillance toujours active, sans option |
| Reprise de lecture (Demander / ComicRack / ComicRust) | 🟡 | pas de réglage pour revenir à « Demander » |
| Écriture ComicRack / fichier / ranger l'original | ✅ | |
| Panneaux Listes et Détails masquables | 🟡 | Listes non masquable |

### Raccourcis clavier
| Contexte | Statut | Remarque |
|---|---|---|
| Bibliothèque | 🟡 | pas de Ctrl+O ni F11 ; ➕ `/`, Ctrl+F |
| Lecteur : pages, 1-4, +/-/0, D, M, T, F/F11, Échap, Ctrl+W | ✅ | ➕ V, ↑↓, Ctrl+molette |
| Lecteur : `I` infos, `L` liste, Ctrl+O | ✅/🟡 | `I` ✅ (lot L6) ; `L` = loupe côté Tauri, décision documentée dans l'aide (`?`) |

### Glisser-déposer
| Fonction | Statut | Remarque |
|---|---|---|
| Fichier/dossier/.cbl déposé sur la fenêtre | ❌ | aucun écouteur |
| Fichier passé en argument de ligne de commande | ❌ | seuls `--attendre-pid` et `--mcp` lus |
| Livres/séries → liste, réordonner une liste, listes dans l'arbre, pages | ✅ | |
| Piles → liste | ✅ | |
| **Piles réordonnées par glisser** | ❌ | egui : v0.10.2, `library_view.rs` ; voir § 2 |

### Menus
| Fonction | Statut | Remarque |
|---|---|---|
| Menu livre, menu des listes | ✅ | ➕ Renommer/déplacer, Ouvrir le dossier |
| Entrées « manquants » | ❌ | |
| Monter/Descendre la série sur une pile | 🟡 | présent pour les livres groupés seulement |
| Fichiers récents (🕘, 15 entrées) | ✅ | lot L6, `ipc-lecteur.md` ; `.cbl` non mémorisés |
| À propos / aide / raccourcis / état PDFium | ❌ | |

### Ligne de commande, fenêtre
- `comicrust-cli` : identique sur les deux branches mais **absent du zip Tauri** (`release-tauri.yml`) ❌ ; le README du zip l'affirme à tort.
- Taille/position de fenêtre mémorisées ❌ (fixe 1280×800) ; titre dynamique ❌ ; plein écran lecteur seulement 🟡 ; taille minimale 800×500 contre 480×360 🟡.

### Lecteur
- Formats, modes simple/double/manga, ajustements, zoom, miniatures, reprise : ✅ ; ➕ défilement continu, loupe, rotation.
- ✅ Fin de livre → livre suivant de la vue affichée, Infos du livre (`I`) et barre d'état (lot L6, `ipc-lecteur.md`).
- 🟡 Listes `.cbl` : import dans les listes (conception différente).
- ❌ **Dialogue natif Ouvrir/Dossier** : aucun plugin dialog, chemins saisis à la main (Ouvrir, Dossiers, Import/Export .cbl, Transfert, scripts).

### Bibliothèque
- Tri (10 clés + ordre de la liste), recherche rapide, listes (lecture, intelligentes, dossiers, copie liée) : ✅.
- Regroupement : egui 9 (aucun, série, éditeur, scénariste, année, arc, genre, format, état de lecture) ; Tauri 3 🟡.
- Tableau : egui 11 colonnes redimensionnables ; Tauri colonnes réduites, sans redimensionnement 🟡.
- Piles : ✅ ; 🟡 pas d'en-têtes de groupe en mode empilé.
- ❌ « Copier les livres », « ⟳ Relire » ; ➕ section « Séries » de la barre latérale = maquette à retirer.

### Import, scan, manquants
- ❌ **Analyse automatique au démarrage** (egui : `scan_after_load = true`).
- ❌ « Arrêter l'analyse », bilan copiable des erreurs.
- ✅ **Livres manquants** (lot L3, `ipc-manquants.md`) : badge, indicateur du tableau, entrée « Livres manquants (N) » (toujours visible, discrète à 0), Retrouver le fichier, Chercher dans un dossier, Retirer (confirmation), fenêtre « Fichier introuvable » / « Disque inaccessible ». 🟡 chemins saisis (pas de dialogue natif : point d'extension `selecteurs`) ; pas de menu « Fichiers » en Tauri ; marque en mémoire seulement pour une bibliothèque ComicRack ; la table `converted` d'egui n'a pas d'équivalent.
- ✅ Base hors ligne.

### Édition : ✅ (Infos 5 onglets, Pages, ComicInfo, conversion) ; ➕ renommer/déplacer.

### Couvertures, performance : ✅ protocoles `cr-couv`/`cr-page` ; grilles virtualisées ; 🟡 mémoire WebView2 non mesurée.
### Accessibilité : rôles ARIA, focus rendu, `:focus-visible` ✅ ; 🟡 pas de `prefers-reduced-motion`/`forced-colors` ; pas de test NVDA.
### Portable, diagnostic : ✅ rien hors `data\` ; ❌ rapport de plantage `data\plantage.txt` ; ✅ MCP.

## 2. Piles réordonnables (« Ordre de la liste » + « Empiler par série »)

**Tauri aujourd'hui** : dans une liste de lecture (tri `ordre` croissant), les piles suivent l'ordre de
première apparition ; un glisser de pile ne sait qu'ajouter à une autre liste ; pas de Monter/Descendre.
`deplacer_groupes_liste` / `decaler_groupe_liste` (T/listes.rs) exigent un groupe ≠ aucun.

**egui** : glisser une pile (ou la sélection) entre deux piles (trait d'insertion), déplacement en bloc
des séries, une série dispersée est regroupée au dépôt ; seulement liste de lecture modifiable + tri
Position croissant, sinon curseur interdit avec la raison ; menu ⬆ Monter / ⬇ Descendre la série.

**Conception** (taille M) :
1. Backend : `piles_de(requete)`, `deplacer_piles_liste(id, requete, series, insertion)`,
   `decaler_pile_liste(id, requete, serie, haut)` (mêmes briques : `controler_vue`, `group_drop_anchor`,
   `group_shift_anchor`, `reordonner`), sans la condition `groupe != aucun`.
2. IPC : 2 commandes, wrappers `api.ts`, simulation `simuleListes.ts`.
3. Glisser : `GlisserLivres` paramétré par le sélecteur de cellule (`[data-pile]`) ; chaque pile = un groupe.
4. `VueBibliotheque.svelte` : `refusReordonner()` réutilisé, `deplacerGroupes` envoie les titres ;
   la ré-sélection par titre existe déjà.
5. Menu des piles : « ⬆ Monter / ⬇ Descendre la pile » (alternative clavier), Alt+↑/↓.
6. Tests : série dispersée, début/fin, déjà en tête, plusieurs piles non contiguës, lecture seule, tri
   décroissant, `groupe = série` + piles.

## 3. Écarts priorisés
**[B]** 1 Réordonner les piles (M) · 2 Livres manquants (L) · 3 Analyse au démarrage + Arrêter + bilan (S/M) ·
4 Dépôt de fichier/dossier/.cbl sur la fenêtre et argument de ligne de commande (M).

**[M]** 5 Dialogue natif de fichiers/dossiers (M) · 6 Taille/position de fenêtre, titre dynamique, F11 en
bibliothèque, Ctrl+O (S) · 7 Assistant de configuration rejouable (M) · 8 Livre suivant en fin de livre,
Infos du lecteur (`I`), fichiers récents (M) · 9 Six regroupements manquants (S/M) · 10 À propos / aide
(S) · 11 Rapport de plantage `data\plantage.txt` (S/M) · 12 `comicrust-cli.exe` dans le zip et README
propre à Tauri (S) · 13 Mesures de performance et mémoire sur 5 323 livres (test).

**[m]** 14 Réglages « Suivre ComicRack » et reprise de lecture · 15 Thème « Comme Windows » · 16 Panneau
Listes masquable · 17 Colonnes du tableau + redimensionnement · 18 « Copier les livres », « Relire » ·
19 En-têtes de groupe en mode empilé · 20 Retirer la section « Séries » (maquette) · 21 `L` loupe/liste ·
22 `prefers-reduced-motion`/`forced-colors` · 23 Placeholder de recherche périmé.

## 4. Écarts proposés « acceptés »
`.cbl` ouvert comme liste éphémère (remplacé par l'import) ; dossiers « surveillés » = analyse manuelle et
au démarrage ; thèmes = fichiers CSS. Les limites « Changer de connexion » et « Parcourir… » des scripts
ne sont plus acceptées (écarts 7 et 5).

## 5. Lots
Règle anti-conflit : fichiers partagés (`T/app.rs`, `F/lib/api.ts`, `F/lib/simule*.ts`,
`VueBibliotheque.svelte`, `App.svelte`) câblés par l'intégrateur ; chaque lot met ses commandes dans son
propre fichier.
L1 Piles (listes.rs, glisserLivres, GrillePiles) · L2 Fenêtre/lancement (fenêtre, titre, F11, arguments,
dépôt de fichiers, plantage) · L3 Livres manquants · L4 Scan et dossiers · L5 Regroupements/tableau ·
L6 Lecteur (infos, livre suivant, récents) · L7 Réglages/aide/assistant · L8 Dialogue natif (après L4 et L1)
· L9 Paquet et docs (CLI dans le zip, README).

## 6. Recette tauri-v0.11.0
Non-régression complète bloc par bloc (installation/MAJ et portable, premiers démarrages, bibliothèque et
performances, listes, piles réordonnables PR-A..G, lecteur, édition, livres manquants, scan au démarrage,
scripts/MySQL/thèmes, fenêtre et dépôt de fichiers, CLI, accessibilité, aide et plantage).
