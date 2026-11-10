# Recette ComicRust Tauri tauri-v0.6.1

| | |
|---|---|
| Version | tauri-v0.6.1 (pré-release corrective de l'étape 5 : **listes ComicRack**) |
| Paquet | `ComicRust-Tauri-tauri-v0.6.1-windows-x64.zip` |
| SHA-256 | `ba79933a09cf369706f893ea96c1314ccab2617ef52c23efb757800e3a10a7d1` |
| Contenu | **éditeur de listes intelligentes** : plus d'erreur « missing field `genre` » après un changement de champ ; **série, numéro et volume déduits du nom de fichier** quand la fiche n'en a pas (affichage, tri, regroupement, correspondance et export `.cbl`) ; **focus clavier** rendu après un menu contextuel ou une fenêtre ; « Ajouter à une liste… » propose la dernière liste utilisée ; liste native de « Déplacer dans… » lisible en thème sombre |
| Origine | essais de la tauri-v0.6.0 menés par l'IA avec le pilotage MCP (10/11/2026, copie autonome) ; la tauri-v0.6.0 est toujours en attente de ton rapport : **reprendre ici ses tests non validés** |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ Avec « Lecture et écriture », cette version peut réécrire les listes de ComicRack
> (`ComicDb.xml`, ComicRack fermé) : sauvegarder `ComicDb.xml` et la base MySQL avant les essais
> de la section 4. Copie **autonome** pour les sections 1 à 3, copie **liée** pour la section 4.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **97 tests** du backend `comicrust-tauri` (95 + 2 : rattachement `.cbl` de `Silver Surfer #3 ok.pdf` et `Warlock #9.cbz` sans fiche ; regroupement par série déduite, série de la fiche prioritaire) ; `scan_liste_detail_couverture_et_manquants` adapté (« Hors-serie.cbz » classé dans la série « Hors-serie ») | `cargo test -p comicrust-tauri` | ✅ 97/97 |
| Front : `svelte-check` (0 erreur), test simulé du focus (Échap sur menu, Échap sur Infos, réouverture) | local, Playwright (mode simulé) | ✅ |
| Rustfmt, Clippy `-D warnings`, compilation de l'application complète | local + CI Windows | ✅ local ; CI à la publication |
| Cœur (`comicrust-core`) **non modifié** : `shadow_series/number/volume` existaient déjà | — | ✅ |

**Non vérifiable ici** : la vraie fenêtre WebView2 (focus, liste native), ta vraie bibliothèque,
ton fichier `Thanos.cbl`.

## 1. Éditeur de listes intelligentes (correctif)

- [ ] **E-01 [B]** Nouvelle liste intelligente, premier critère : changer le **champ** (ex. « Série » → « Nombre de pages »), saisir une valeur : l'**aperçu** affiche un nombre de livres, **sans** message « missing field `genre` ». Répéter avec 3 champs différents, puis dans un **groupe imbriqué**.
- [ ] **E-02 [B]** « OK » puis rouvrir la liste (✏ Modifier) : les critères changés sont enregistrés tels quels ; Ctrl+Entrée enregistre.
- [ ] **E-03 [M]** Changer de champ d'une même nature (texte → texte) : opérateur et valeur sont conservés ; de nature différente (texte → nombre) : réinitialisés.

## 2. Série, numéro et volume déduits du nom de fichier (autonome)

- [ ] **S-01 [B]** Un livre sans série en fiche mais au nom du genre `Silver Surfer #3 ok.pdf` ou `COSMIC COLLECTION #009 ….cbz` s'affiche avec sa **série** et son **numéro** (plus « Sans série »). Noter un exemple : ………
- [ ] **S-02 [B]** Importer `Thanos.cbl` (échantillons) dans la copie autonome : **6 trouvés sur 7** (attendu par la recette C-03 : un seul manquant). Noter trouvés / manquants : ……… / ………
- [ ] **S-03 [M]** Tri par série : les livres sans série en fiche ne sont plus tous en tête ; ils sont classés avec leur série déduite, dans l'ordre naturel des numéros.
- [ ] **S-04 [M]** Une fiche qui a une série et un numéro **les garde** (la fiche reste prioritaire) ; ouvrir « Infos du livre » d'un livre sans série : les champs restent **vides** (rien de déduit n'est écrit dans le fichier ni dans la fiche à l'enregistrement).
- [ ] **S-05 [m]** Export `.cbl` d'une liste contenant ces livres : série et numéro déduits présents dans le fichier exporté.

## 3. Clavier et interface (correctifs)

- [ ] **K-01 [B]** Liste de lecture ouverte, clic droit sur un livre ▸ « ⬆ Monter dans la liste » : **Suppr** retire ensuite le livre sélectionné et **F2** (sur l'arbre) renomme, **sans** reclic.
- [ ] **K-02 [B]** Ouvrir « Infos du livre » (ou Renommer, Ouvrir un fichier) puis **Échap** : le focus revient sur la liste ou l'arbre (flèches, Suppr, F2 répondent) ; la fenêtre se rouvre normalement (I-03, D-01 : non-régression).
- [ ] **K-03 [M]** « Ajouter à une liste… » : la liste proposée est la **dernière utilisée** dans la session (sinon la liste courante).
- [ ] **K-04 [m]** « Déplacer dans… » en thème **sombre** : la liste déroulante est lisible (texte clair sur fond sombre).

## 4. Non-régression (liée à ComicRack, ComicRack fermé)

- [ ] **N-01 [B]** Arbre des listes identique à ComicRack (**47** listes), « 📋 Copier le bilan des listes » : 46/47 identiques (seul écart attendu : « Lecture en cours »). Recherche « thanos » : **484**.
- [ ] **N-02 [B]** Créer une liste de lecture, y ajouter 3 livres par glisser-déposer, monter / descendre, retirer par Suppr, la supprimer : `ComicDb.xml` est sauvegardé (`…-origine-….xml.bak`), ComicRack relancé montre les mêmes listes.
- [ ] **N-03 [B]** Esc dans « Renommer » avec un champ modifié : la fenêtre **ne se ferme pas** (D-01) ; sans modification elle se ferme.
- [ ] **N-04 [M]** Les **tests de la tauri-v0.6.0 non encore rapportés** (`recette/tauri-v0.6.0.md`, sections 3 à 12) : les passer sur ce paquet et coller le rapport.

---

## Limites connues (ne pas compter comme anomalies)

- La fiche détail et la fenêtre d'édition montrent les valeurs **brutes** de la fiche (série vide) : seules les vues, le tri, le regroupement et les `.cbl` utilisent la série du nom de fichier.
- Version egui (`main`) : l'affichage utilisait déjà la série déduite ; le rattachement `.cbl` par la base ComicRack y utilise les champs bruts mais retombe sur l'index du dossier. À vérifier séparément.
- Les limites de la tauri-v0.6.0 restent valables.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 9 | | |
| Majeurs [M] | 5 | | |
| Mineurs [m] | 2 | | |

Décision et remarques :
