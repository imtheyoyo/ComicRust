# Recette ComicRust Tauri tauri-v0.4.0

| | |
|---|---|
| Version | tauri-v0.4.0 (pré-release de la version Tauri 2 : étape 3, **infos et édition**) |
| Paquet | `ComicRust-Tauri-tauri-v0.4.0-windows-x64.zip` |
| SHA-256 | `b3918ba674cbf1106bbb51278b3006903e1ac24e05723a0ac1cb9fa9c287336d` |
| Contenu | **sélection multiple** (grille et tableau : Ctrl, Maj, Ctrl+A) ; **menu contextuel** ; fenêtre **« Infos du livre »** (un livre ou plusieurs, onglets, autocomplétion, validation, ↺ / ✖, Précédent / Suivant) ; **écriture du `ComicInfo.xml`** dans les CBZ ; **conversion CBR / CB7 / CBT → CBZ** ; **renommer / déplacer** avec aperçu ; garde-fou au-delà de 100 livres ; option **« Lecture et écriture »** du mode lié ComicRack (fiches, fichiers et avancement de lecture écrits dans la base MySQL de ComicRack) ; « Ouvrir le dossier du fichier » ; correctif E-03 (miniature d'une page tournée) |
| Origine | étape 3 de [`docs/tauri/PLAN.md`](../docs/tauri/PLAN.md), contrat [`docs/tauri/ipc-edition.md`](../docs/tauri/ipc-edition.md) ; tauri-v0.3.0 **GO** le 03/11/2026 ([rapport](rapports/tauri-v0.3.0-2026-11-03.txt) : M-08 non testé, repris ici ; E-03 corrigé). Tests repris des recettes egui [v0.4.0](v0.4.0.md) / [v0.4.1](v0.4.1.md) (D-), [v0.6.0](v0.6.0.md) (W-, C-), [v0.10.0](v0.10.0.md) (rangement), [v0.10.1](v0.10.1.md) (F-) et [v0.10.7](v0.10.7.md) (P-01 sur le NAS) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ **Première version Tauri qui modifie des fichiers et, sur demande, la base de ComicRack.**
> **Avant les tests d'écriture, faire une sauvegarde de la base ComicRack** (P-04) et travailler
> sur des **copies** de BD rangées dans un dossier d'essai (P-03), jamais sur les originaux de la
> collection. Par défaut, le mode lié reste en **lecture seule** : rien n'est écrit dans ComicRack
> tant que « Lecture et écriture » n'est pas activé (section 9).
>
> Comme pour la tauri-v0.3.0, garder **deux copies** : une **autonome** et une **liée** à ComicRack.
> **Échantillons** : `F:\Projet REA par Claude\REA ComicRack\ComicRust-echantillons`.
>
> **Valeurs de référence** : « thanos » **484** livres en tauri-v0.3.0 (ComicRack : 483) ; la
> bibliothèque liée compte **5 325** livres.
>
> **Comparer deux `ComicInfo.xml`** (PowerShell, après les avoir extraits avec 7-Zip dans
> `avant\` et `apres\`) : `Compare-Object (Get-Content avant\ComicInfo.xml) (Get-Content apres\ComicInfo.xml)`
> n'affiche que les lignes différentes.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **69 tests** du backend `comicrust-tauri` (53 de la tauri-v0.3.0 + 16) : fiches d'un livre et de plusieurs (onglets, **valeurs multiples**), modification de la base **et du `ComicInfo.xml` des seuls champs touchés**, valeurs refusées et valeurs personnalisées, suggestions tirées de la bibliothèque, **garde-fou au-delà de 100 livres** (rien fait sans confirmation), **renommage** (aperçu, collisions refusées, application, fiche suivie), **conversion CBR et CB7 → CBZ**, **`ComicDb.xml` en lecture seule** pour les fiches, événements pour le front, réglages `edition` (défaut lecture seule, fusion) ; corrections de revue : « (valeurs multiples) » jamais écrit, original rangé seulement après la mise à jour de la fiche, livres non traités après « Arrêter » signalés, édition et analyse des dossiers exclusives, « Ouvrir le dossier du fichier » (commande Explorateur) | `cargo test` (`comicrust-tauri`) | ✅ 69/69 |
| **Base MySQL de test** (MariaDB locale, jamais la base de démonstration) : **lecture seule par défaut** (fiches refusées, fichiers écrits, base ComicRack intacte, lecture non écrite) ; **« Lecture et écriture »** : seuls les éléments touchés de la fiche réécrits, compteurs `update_counter` de ComicRack avancés, `ComicInfoIsDirty` remis à faux et `FileSize` à jour après l'écriture du fichier, fiche relue au moment d'écrire (modification faite entre-temps dans ComicRack conservée), **avancement écrit à la fermeture du livre** (`CurrentPage`, `LastPageRead`, `Opened`, `OpenCount`), renommage suivi par la fiche ComicRack | `cargo test` avec `COMICRUST_TEST_MYSQL` (Linux) | ✅ |
| Front : `svelte-check` (0 erreur, 0 avertissement), `vite build` | local + CI Windows (`ci-tauri.yml`) | ✅ |
| Parcours simulé dans un navigateur (Playwright, mode simulé sans Tauri, 300 livres) : Ctrl+clic et Maj+clic (4 sélectionnés), **Ctrl+A** (300 sélectionnés), clic droit sur un livre de la sélection (menu « des 300 livres »), « Infos » de plusieurs livres (« (valeurs multiples) »), **garde-fou** au OK (> 100 livres), Année « abcd » refusée (OK grisé), ↺, **Appliquer** puis **OK**, aperçu de « Renommer / déplacer… », mode ComicRack en **consultation seulement** ; aucune erreur JavaScript | Playwright (mode simulé) | ✅ |
| Clippy `-D warnings`, rustfmt, tests, compilation de l'application complète (`--features app,custom-protocol`) | CI Windows (`ci-tauri.yml`) | ✅ verte |

**Non vérifiable ici** : la vraie fenêtre WebView2, les écritures sur le partage SMB du NAS (os
error 5 de la v0.10.6), le verrouillage des fichiers par Windows ou par ComicRack, la vraie base
MySQL de ComicRack et ce que ComicRack en affiche après relance, l'Explorateur Windows (sections 1 à 14).

## 1. Préparation

- [ ] **P-01 [B]** Vérifier l'empreinte : `Get-FileHash .\ComicRust-Tauri-tauri-v0.4.0-windows-x64.zip` → identique au SHA-256 ci-dessus. Le zip contient `ComicRust-Tauri.exe`, `pdfium.dll`, `README.md`, `RECETTE.md` et `licences\`.
- [ ] **P-02 [B]** Copier le contenu du zip **par-dessus** les deux copies tauri-v0.3.0 (dossiers `data\` conservés) et lancer chacune : pas d'assistant, même mode (Autonome / Lié), mêmes livres, mêmes préférences d'affichage et réglages du lecteur qu'en tauri-v0.3.0.
- [ ] **P-03 [B]** **Dossier d'essai** : copier dans un dossier local (ex. `F:\…\Essai-edition`) les échantillons `Thanos Quest 01 (1990).cbz`, `Silver Surfer 04 (1987).cbr`, `Warlock 09 (1976).cb7`, `Silver Surfer 03.pdf`, plus 2 ou 3 **vrais** CBZ et CBR de la collection ; et sur le NAS un dossier d'essai (ex. `\\SERVEURNAS\…\_essai_comicrust`) avec **une dizaine de copies** de vrais CBZ et CBR. Copie autonome : ajouter ces deux dossiers dans « 📁 Dossiers surveillés » puis « Lancer le scan ».
- [ ] **P-04 [B]** **Sauvegarde de la base ComicRack** avant la section 9 : ComicRack fermé, exporter la base MySQL (ex. `mysqldump -h <serveur> -u <utilisateur> -p <base> > comicrack-avant-tauri-v0.4.0.sql`, valeurs de `ComicRack.ini`) et garder une copie du dossier de ComicRack (`ComicDb.xml`, `ComicRack.ini`). Taille du fichier de sauvegarde : ………
- [ ] **P-05 [M]** Dans **ComicRack**, ajouter le dossier d'essai du NAS à la bibliothèque (les copies y ont une fiche) ; relancer la copie liée de ComicRust : les livres d'essai y apparaissent (recherche par leur titre).

## 2. Sélection multiple

Copie autonome, vue **grille** puis vue **tableau**.

- [ ] **S-01 [B]** Grille : clic sur un livre (seul sélectionné), **Ctrl+clic** sur deux autres (ajoutés ; Ctrl+clic sur un livre sélectionné le retire), **Maj+clic** (plage depuis le dernier livre cliqué), **Ctrl+Maj+clic** (plage ajoutée à la sélection) : les cadres de sélection suivent ; la barre d'état indique « · N sélectionnés ».
- [ ] **S-02 [B]** Tableau : mêmes clics que S-01 ; **Maj+↑ / Maj+↓**, Maj+Début / Maj+Fin étendent la sélection ; ↑ ↓ sans Maj reviennent à un seul livre.
- [ ] **S-03 [B]** **Ctrl+A** dans la grille puis dans le tableau : tous les livres de la vue sont sélectionnés (« · N sélectionnés » = nombre de livres). Avec une recherche active : seuls les livres trouvés. Copie liée, recherche « thanos », Ctrl+A : « 484 sélectionnés ».
- [ ] **S-04 [B]** **Clic droit** sur un livre **de la sélection** : la sélection est gardée, le menu propose « ✏ Modifier les infos des N livres… » ; clic droit sur un livre **hors sélection** : il devient seul sélectionné, menu « ✏ Modifier les infos… ». Entrées : Lire (grisé si plusieurs), ✏ Modifier les infos… (Ctrl+I), Écrire les infos dans le(s) fichier(s), Convertir en CBZ, Renommer / déplacer…, Ouvrir le dossier du fichier.
- [ ] **S-05 [M]** Menu au clavier : touche **Menu** ou **Maj+F10** sur la sélection ; ↑ ↓ Début Fin, Entrée choisit, Échap (ou clic ailleurs) ferme et rend la main à la liste.
- [ ] **S-06 [M]** Panneau de détails avec plusieurs livres : « N livres sélectionnés (détails du livre courant) » et bouton « ✏ Modifier (N) ».
- [ ] **S-07 [m]** Copie liée, Ctrl+A sur les 5 325 livres : instantané, la grille reste fluide ; un clic simple revient à un seul livre.

## 3. Fenêtre « Infos du livre » (un livre)

Copie autonome, livres du dossier d'essai.

- [ ] **I-01 [B]** Ouverture de trois façons : **Ctrl+I**, clic droit → « ✏ Modifier les infos… », bouton « ✏ Modifier » du panneau de détails. Titre « Infos — <titre> », couverture, « N pages · taille », chemin du fichier ; onglets **Résumé, Détails, Intrigue et notes, Catalogue, Personnalisé** (mêmes champs que la fenêtre egui).
- [ ] **I-02 [B]** Modifier **Série** et **Numéro**, « OK » : la fenêtre se ferme, la fiche est modifiée (grille, tableau, panneau de détails). Fermer et relancer : modification gardée.
- [ ] **I-03 [M]** Un champ modifié montre « ● » devant son libellé et le pied « ● N champ(s) modifié(s) » ; **↺** « Revenir à la valeur enregistrée » remet la valeur d'origine. Échap ne ferme **pas** la fenêtre s'il y a des modifications (il faut « Annuler ») ; sans modification, Échap la ferme. **Ctrl+Entrée** = OK.
- [ ] **I-04 [M]** **Autocomplétion** : dans « Éditeur », taper les premières lettres d'un éditeur de la bibliothèque : au plus 8 propositions (celles qui commencent par la saisie d'abord) ; ↓ ↑ Entrée en choisit une ; Échap ferme la liste **sans** fermer la fenêtre ; **▾** affiche toutes les valeurs de la bibliothèque. Dans « Scénariste » (liste) : `Jim Starlin, Ro` ne complète que la dernière valeur.
- [ ] **I-05 [M]** **Validation** : Année « abcd », Mois « 13 », Note « 7 » : un message « ⚠ … » sous chaque champ, « OK » et « Appliquer » grisés (bulle « Valeur(s) refusée(s) : Année, … ») ; corriger : ils se réactivent. Taper `(valeurs multiples)` dans un champ : refusé de même. « Annuler » : rien n'est modifié.
- [ ] **I-06 [m]** Contrôles : étoiles de la Note (clic sur la 4ᵉ → 4), Date de parution `JJ/MM/AAAA`, Manga « Inconnu / Non / Oui / Oui, de droite à gauche », Noir et blanc « Inconnu / Non / Oui ». Onglet Personnalisé : « ➕ Ajouter » une valeur `essai`, la remplir, OK ; rouvrir : onglet « Personnalisé (1) » ; ✖ « Supprimer cette valeur » puis OK la retire.
- [ ] **I-07 [B]** **Précédent / Suivant** : modifier un champ puis « Suivant ▶ » : la modification est enregistrée et la fenêtre montre le **livre suivant de la vue** (la sélection suit dans la grille) ; « ◀ Précédent » revient. Sur le premier (dernier) livre de la vue, Précédent (Suivant) est grisé.
- [ ] **I-08 [B]** **Appliquer puis OK** : modifier un champ, « Appliquer » : message « Enregistré : 1 livre(s) modifié(s). », la fenêtre reste ouverte, plus de « ● », « Appliquer » grisé ; puis « OK » : la fenêtre se ferme **sans rien écrire de plus** (avec la case « Écrire aussi les infos dans le fichier » cochée sur un CBZ : la date de modification du fichier, notée après « Appliquer », ne change pas au « OK »).

## 4. Plusieurs livres

Copie autonome : 3 livres du dossier d'essai de **séries différentes**.

- [ ] **I-09 [B]** Ctrl+I : titre « Infos de 3 livres », couvertures empilées, « Seuls les champs modifiés sont écrits ; « (valeurs multiples) » : les livres diffèrent. » ; Série, Numéro… affichent « (valeurs multiples) ».
- [ ] **I-10 [B]** Changer **seulement l'Éditeur**, « OK » : les 3 livres ont le nouvel éditeur et **chacun garde sa série, son numéro, son titre**.
- [ ] **I-11 [B]** ✖ « Vider ce champ pour tous les livres » sur un champ multiple (ex. Genre), OK : le champ est vide pour les 3. Taper puis effacer un texte dans un autre champ multiple : il n'est **pas** compté comme modifié (rien écrit pour ce champ, les 3 valeurs différentes restent).
- [ ] **I-12 [M]** Pendant l'enregistrement : « Enregistrement… n / 3 » et barre ; puis bilan « Résultats par livre » (OK / Déjà à jour / Ignoré / Erreur) et « Copier le rapport » (coller dans la note). Liste déroulante sur un champ multiple (ex. Manga) : « (valeurs multiples) » proposé tant qu'on n'a rien choisi.

## 5. Écriture du `ComicInfo.xml` dans un CBZ

Copie autonome, **copie** de `Thanos Quest 01 (1990).cbz` et un vrai CBZ du dossier d'essai.

- [ ] **W-01 [B]** Clic droit → « Écrire les infos dans le fichier » → « Écrire » : bilan « 1 fichier(s) écrit(s) » (ou « 1 déjà à jour »). Copier ce CBZ en `avant.cbz` (référence).
- [ ] **W-02 [B]** Ctrl+I, case **« Écrire aussi les infos dans le fichier (ComicInfo.xml) »** cochée, modifier **seulement le Titre**, OK. Extraire le `ComicInfo.xml` de `avant.cbz` et du CBZ modifié, les comparer (Compare-Object) : **seule la ligne `<Title>`** diffère. Le CBZ s'ouvre avec toutes ses pages, dans le même ordre.
- [ ] **W-03 [B]** Contre-vérification avec la **version egui** (« 📂 Ouvrir » puis « ℹ Infos ») **ou ComicRack** (ouvrir le fichier, infos lues depuis le fichier) : nouveau titre, tous les autres champs identiques à ceux d'`avant.cbz`. Outil utilisé : ………
- [ ] **W-04 [M]** Relancer « Écrire les infos dans le fichier » sur ce livre : bilan « 0 fichier(s) écrit(s), 1 déjà à jour » ; la date du fichier ne change pas.
- [ ] **W-05 [M]** Sélection CBZ + CBR + PDF, « Écrire les infos dans les fichiers… » : seuls les CBZ sont écrits, les autres « Ignoré » (« 2 ignoré(s) ») ; Ctrl+I sur un CBR seul : la case est grisée (bulle « Seuls les CBZ reçoivent les infos (convertir d'abord en CBZ) »).
- [ ] **W-06 [M]** Lire quelques pages du CBZ, fermer le lecteur, puis tout de suite Ctrl+I avec la case cochée, modifier un champ, OK : écriture réussie (pas d'erreur « fichier utilisé ») ; rouvrir le livre : toutes les pages s'affichent, reprise à la même page ; la couverture de la grille est à jour.
- [ ] **W-07 [m]** Modifier un champ **sans** cocher la case : à la réouverture, l'en-tête indique « infos pas encore écrites dans le fichier » ; après « Écrire les infos dans le fichier », la mention disparaît.

## 6. Garde-fou, lots et Arrêter

- [ ] **G-01 [B]** Copie autonome avec **plus de 100 livres** (au besoin ajouter un dossier de vraies BD du NAS aux dossiers surveillés : seule la base de ComicRust est modifiée tant que la case « Écrire aussi » est **décochée**) : Ctrl+A, Ctrl+I, Étiquettes `essai-gardefou`, OK : message « Modifier les infos : N livres concernés (plus de 100). Confirme pour lancer l'opération sur tous ces livres. » avec Annuler / Confirmer ; **Annuler** : rien n'est modifié (vérifier deux livres).
- [ ] **G-02 [M]** Même chose, **Confirmer** : tous les livres reçoivent l'étiquette (bilan « N livre(s) modifié(s) ») ; puis Ctrl+A, Ctrl+I, ✖ sur Étiquettes, OK, Confirmer : l'étiquette est retirée partout. Nombre de livres : ………
- [ ] **G-03 [B]** Copie liée (lecture seule), recherche « thanos », Ctrl+A → « Écrire les infos dans les fichiers… » → « Écrire » : la confirmation (plus de 100) apparaît ; **Annuler** (ne pas confirmer sur la vraie collection) : aucun fichier modifié.
- [ ] **G-04 [B]** **Arrêter un lot** : dossier d'essai du NAS, sélectionner la dizaine de CBR / CBZ copiés, « Convertir les CBR / CB7 / CBT en CBZ… » → « Convertir », et pendant la progression « n / N — titre », **Arrêter** : le lot s'arrête **après le livre en cours** ; bilan « ⚠ k livre(s) converti(s), …, interrompu (opération arrêtée) » ; les livres non traités sont listés « Ignoré » avec « non fait : arrêt demandé » ; chaque CBZ créé s'ouvre avec toutes ses pages, aucun fichier temporaire ni CBZ à moitié écrit dans le dossier ; relancer la conversion termine les autres.
- [ ] **G-05 [m]** « Renommer / déplacer… » et « Écrire les infos dans les fichiers… » montrent la même progression et le même « Arrêter » ; Échap ne ferme pas la fenêtre pendant un lot. Pendant un « Lancer le scan » (📁 Dossiers surveillés), une opération d'édition est refusée avec un message (et inversement), sans plantage.

## 7. Conversion CBR → CBZ

Copie autonome, copies du dossier d'essai.

- [ ] **C-01 [B]** Clic droit sur `Silver Surfer 04 (1987).cbr` (copie) → « Convertir en CBZ » → case « Ranger l'original… » **décochée** → « Convertir » : bilan « 1 livre(s) converti(s) » (« 11 pages ») ; `Silver Surfer 04 (1987).cbz` apparaît à côté du `.cbr` resté en place ; dans la bibliothèque, le livre pointe sur le **CBZ** (panneau de détails : format et chemin), s'ouvre avec ses 11 pages, sa lecture est gardée.
- [ ] **C-02 [B]** Sur un vrai CBR (ou `Warlock 09 (1976).cb7`) : cocher **« Ranger l'original dans le dossier « _originaux » (sinon il reste en place) »** : l'original est dans `_originaux\` (même dossier), le CBZ à sa place ; bilan « … pages, original rangé dans … ». La case est retrouvée cochée à la conversion suivante.
- [ ] **C-03 [B]** Le CBZ converti contient le `ComicInfo.xml` de la fiche (vérifier avec la version egui « ℹ Infos » ou 7-Zip) ; pages identiques à l'original, couleurs et ordre compris.
- [ ] **C-04 [M]** Sélection CBZ + PDF + CBR, « Convertir les CBR / CB7 / CBT en CBZ… » : seul le CBR est converti, les autres « Ignoré » (« seuls les CBR, CB7 et CBT se convertissent »).
- [ ] **C-05 [m]** Re-convertir un CBR dont le CBZ existe déjà (copie liée en lecture seule, ou original remis à côté) : erreur « … existe déjà », rien n'est écrasé.

## 8. Renommer / déplacer

Copie autonome, 3 ou 4 livres du dossier d'essai (copies).

- [ ] **R-01 [B]** Clic droit → « Renommer / déplacer… » : modèle « Série v2 #12 (2024) » par défaut (liste : « Export ComicRack », « Éditeur / Série / Série #012 », « (personnalisé) »), « Dossier cible » vide ; l'**aperçu** (Actuel / Nouveau) se met à jour pendant la frappe ; « Aperçu : N fichier(s) à renommer… » ; **rien n'est renommé** tant qu'on n'a pas cliqué « Renommer N fichier(s) ».
- [ ] **R-02 [B]** « Renommer N fichier(s) » : les fichiers sont renommés sur le disque ; bilan « N fichier(s) renommé(s) ou déplacé(s) » ; **la fiche suit** : chaque livre s'ouvre, garde ses infos et sa lecture ; « Lancer le scan » ne signale ni livre manquant ni nouveau livre.
- [ ] **R-03 [B]** **Collisions refusées** : créer dans le dossier un fichier portant le nom visé pour un livre, rouvrir l'aperçu : la ligne indique « ⚠ la destination existe déjà » et n'est **pas** appliquée ; deux livres qui donneraient le même nom : « ⚠ même destination que « … » » ; après « Renommer », **aucun fichier n'est écrasé**.
- [ ] **R-04 [M]** Relancer sur les mêmes livres : lignes « inchangé », « Renommer 0 fichier(s) » grisé. Modèle `{publisher}/{series}/{series}[ #{number:000}]` avec un **Dossier cible** (chemin complet du dossier d'essai) : sous-dossiers créés, fichiers déplacés, livres toujours lisibles. Un dossier cible relatif (`essai`) : message « chemin complet attendu ».
- [ ] **R-05 [m]** Le modèle et le dossier cible utilisés sont proposés à l'ouverture suivante ; un livre sans fichier ou manquant apparaît avec son problème dans l'aperçu.

## 9. Mode lié à ComicRack

Copie liée, **sauvegarde P-04 faite**, livres d'essai de P-05. **ComicRack fermé** pendant les
écritures sauf mention contraire ; le relancer pour vérifier.

- [ ] **L-01 [B]** **Lecture seule par défaut** : Ctrl+I sur un livre : « 🔒 Consultation seulement : cette bibliothèque est en lecture seule. » avec la marche à suivre ; champs grisés, « Appliquer » grisé, « OK » ferme sans rien écrire. « Renommer / déplacer… » : message de lecture seule, rien renommé.
- [ ] **L-02 [B]** **Rien n'est écrit dans ComicRack en lecture seule** : lire quelques pages d'un livre d'essai, convertir un CBR d'essai (case « Ranger » décochée), « Écrire les infos dans le fichier » sur un CBZ d'essai ; relancer ComicRack : fiches, page de lecture, date d'ouverture et chemin de ces livres **inchangés** (le CBR converti est toujours le livre de ComicRack ; le fichier CBZ, lui, est écrit).
- [ ] **L-03 [M]** Lecture seule, conversion avec « Ranger l'original dans « _originaux » » coché : refus « bibliothèque en lecture seule : l'original ne peut pas être rangé dans « _originaux » … », rien n'est fait.
- [ ] **L-04 [B]** **Activer l'écriture** : ⚙ Réglages → « Bibliothèque ComicRack » → cocher « Lecture et écriture (…) » : avertissement « ⚠ ComicRust va écrire dans la base de ComicRack. » (« Faites d'abord une sauvegarde de la base (MySQL)… ») avec Annuler / « Activer l'écriture » ; **Annuler** : la case reste décochée ; « Activer l'écriture » : « Lecture et écriture activée. » ; `data\comicrust-tauri.json` contient `"ecritureComicrack": true` dans la section `edition`.
- [ ] **L-05 [B]** **Fiche modifiée visible dans ComicRack** : Ctrl+I sur un livre d'essai (plus de « Consultation seulement »), modifier Titre et Note, OK ; relancer ComicRack : ce livre a le nouveau titre et la nouvelle note, **tous les autres champs inchangés**.
- [ ] **L-06 [B]** **Seuls les champs touchés sont écrits** : dans ComicRack, modifier les Notes d'un livre d'essai, fermer ComicRack ; relancer ComicRust, Ctrl+I sur ce livre, modifier seulement le Genre, OK ; relancer ComicRack : **les deux** modifications sont là (les Notes de ComicRack ne sont pas écrasées).
- [ ] **L-07 [B]** **Avancement de lecture écrit dans ComicRack** : lire un livre d'essai dans ComicRust jusqu'à une page X, fermer le lecteur ; relancer ComicRack : ce livre est à la page X (barre de lecture, reprise), date d'ouverture d'aujourd'hui, nombre d'ouvertures + 1. Page X : ………
- [ ] **L-08 [M]** Plusieurs livres : 3 livres d'essai, Étiquettes `essai-tauri`, OK ; ComicRack : les 3 ont l'étiquette, chacun garde sa série.
- [ ] **L-09 [M]** **Conversion** en écriture, « Ranger l'original » coché : ComicRack relancé ouvre le **CBZ** pour ce livre (même fiche, même lecture), le livre n'est pas « manquant », l'original est dans `_originaux\`.
- [ ] **L-10 [M]** **Renommage** en écriture : ComicRack relancé ouvre le livre sous son nouveau nom (fiche suivie, pas de livre manquant ni de doublon).
- [ ] **L-11 [M]** « Fichiers à mettre à jour » : modifier dans ComicRust un champ d'un CBZ d'essai **sans** la case « Écrire aussi » : ComicRack relancé le range dans « Fichiers à mettre à jour » ; « Écrire les infos dans le fichier » dans ComicRust : à la relance de ComicRack, il n'y est plus.
- [ ] **L-12 [M]** Décocher « Lecture et écriture » : « Lecture seule rétablie. » immédiatement (sans avertissement) ; Ctrl+I repasse en consultation ; une lecture faite ensuite n'est plus écrite dans ComicRack.
- [ ] **L-13 [m]** ComicRack **lancé** pendant une modification faite dans ComicRust (écriture activée) : ComicRack affiche la fiche modifiée en quelques secondes, sans erreur ni blocage. Délai : ……… s.
- [ ] **L-14 [m]** Bibliothèque `ComicDb.xml` (copie liée de test, si tu en as une) : l'option « Lecture et écriture » est sans effet, Ctrl+I reste en consultation (« la bibliothèque ComicDb.xml de ComicRack ne se modifie pas depuis ComicRust… »). *(« Sans objet » sinon.)*
- [ ] **L-15 [B]** Fin de section : ComicRack rouvre ta bibliothèque intacte (hors livres d'essai), aucune erreur ; remettre au besoin la sauvegarde P-04.

## 10. Ouvrir le dossier du fichier

- [ ] **O-01 [M]** Clic droit sur un livre → « Ouvrir le dossier du fichier » (bulle « Montre le fichier dans l'Explorateur de Windows ») : l'Explorateur s'ouvre sur le dossier du livre, **le fichier y est sélectionné** ; ComicRust reste utilisable. Avec plusieurs livres sélectionnés, l'entrée est grisée (« Un seul livre à la fois »). Marche aussi en copie liée lecture seule.
- [ ] **O-02 [m]** Même chose sur un livre du NAS (`\\SERVEURNAS\…` ou `Z:\…`) ; livre dont le fichier a disparu mais dont le dossier existe : le dossier s'ouvre ; dossier introuvable (ou NAS débranché) : message « Ouvrir le dossier : … » dans la barre d'état en 2 s environ, pas de gel.

## 11. Fichiers sur le NAS

Dossier d'essai du NAS (copies), copie autonome puis copie liée en écriture.

- [ ] **X-01 [B]** Ctrl+I sur un CBZ du NAS, case « Écrire aussi les infos dans le fichier » cochée, modifier un champ, **« Appliquer »** : « Enregistré : 1 livre(s) modifié(s). », **aucune erreur « Accès refusé. (os error 5) »** (défaut de la v0.10.6) ; puis **OK** : rien n'est réécrit. Le `ComicInfo.xml` du fichier contient la modification.
- [ ] **X-02 [B]** Lire ce CBZ du NAS quelques pages, fermer le lecteur, puis tout de suite modifier ses infos avec la case cochée : écriture réussie, pas d'os error 5 ; rouvert, le livre se lit normalement.
- [ ] **X-03 [M]** Conversion (avec « Ranger l'original ») et renommage d'une copie sur le NAS : réussis, pas d'os error 5 ; bilan sans erreur.
- [ ] **X-04 [m]** NAS débranché pendant « Écrire les infos dans les fichiers… » sur plusieurs CBZ du NAS : erreurs lisibles par livre dans le bilan, la fenêtre ne gèle pas, les fichiers déjà écrits sont intacts, ceux qui restent ne sont pas abîmés.

## 12. Reprises de la tauri-v0.3.0

- [ ] **M-08 [M]** **Manga d'après la fiche** (non testé en tauri-v0.3.0) : copie autonome, Ctrl+I sur un livre non manga, Manga « Oui, de droite à gauche », OK ; l'ouvrir : il s'ouvre **directement en sens manga** (⇐ Manga actif, ← avance, curseur inversé). Copie liée : un livre marqué Manga (droite à gauche) dans ComicRack s'ouvre de même.
- [ ] **M-09 [m]** Le CBZ de M-08 écrit avec la case cochée, copié hors de la bibliothèque et ouvert par « Ouvrir un fichier… » : sens manga d'après son `ComicInfo.xml`.
- [ ] **E-03 [M]** **Miniature d'une page tournée** (correctif) : copie liée, livre de test de la tauri-v0.3.0 avec une page tournée de 90° dans ComicRack : la page s'affiche tournée et, dans le bandeau des miniatures (**T**), sa miniature est tournée **et reste dans son cadre** (ne déborde pas sur ses voisines).

## 13. Mode autonome

- [ ] **A-01 [B]** Après les sections 3 à 8 : fermer et relancer la copie autonome : toutes les fiches modifiées, converties et renommées sont retrouvées telles quelles ; `data\comicrust.db` a changé ; une modification faite **sans** la case « Écrire aussi » n'a modifié **aucun fichier** (date du CBZ inchangée).
- [ ] **A-02 [M]** `data\comicrust-tauri.json` : une section `edition` (`ecrireDansFichier`, `rangerOriginal`, `modeleNom`, `dossierCible`) ; la case « Écrire aussi les infos dans le fichier » est retrouvée comme laissée ; les sections `vue` et `vue.lecteur` sont inchangées.

## 14. Non-régression courte (tauri-v0.3.0)

- [ ] **N-01 [B]** Copie liée : recherche « thanos » → **484** livres ; Échap vide la recherche. Nombre : ………
- [ ] **N-02 [B]** Lecteur : double-clic, Entrée et « Lire » ouvrent le livre ; → ← D V M T fonctionnent ; Échap revient à la bibliothèque, même livre sélectionné ; reprise à la dernière page lue.
- [ ] **N-03 [B]** Mémorisation de la vue : tri, groupe, vue (Tableau), thème et réglages du lecteur : fermer, relancer : tout revient tel quel ; ouvrir la fenêtre d'infos ou les réglages d'édition ne les change pas.
- [ ] **N-04 [B]** **Rien hors de `data\`** : après tous ces tests, aucun dossier `ComicRust` ni `fr.imtheyoyo.comicrust` dans `%APPDATA%` ni `%LOCALAPPDATA%` ; à côté des BD, seuls les CBZ convertis et les dossiers `_originaux` ont été ajoutés (aucun fichier temporaire restant).
- [ ] **N-05 [M]** Grille : après une écriture ou une conversion, la couverture du livre est à jour (pas d'image cassée) ; défilement continu sans trou ; tri Série en ordre naturel.

---

## Limites connues (ne pas compter comme anomalies)

- **Garde-fou nouveau** : au-delà de 100 livres, modifier les infos, écrire les infos, convertir ou renommer demande confirmation ; la version egui ne le demande pas (seuil repris de sa confirmation des retraits en masse).
- **`ComicDb.xml` : fiches en lecture seule** (ComicRack réécrit ce fichier lui-même), comme la version egui ; seuls les fichiers (infos, conversion) sont modifiables, et l'option « Lecture et écriture » est sans effet.
- **Ranger l'original est refusé en lecture seule** (l'original reste en place) ; la version egui garde une table « converti → CBZ » dans ses réglages.
- **Écritures ComicRack échouées non rejouées automatiquement** : si la fiche ComicRack n'a pas pu suivre une écriture de fichier (base injoignable, conflit), c'est signalé en erreur dans le bilan (le fichier, lui, est écrit) ; relancer « Écrire les infos dans le fichier » refait le report. La version egui les refait au lancement suivant.
- **Onglet « Pages »** (ordre, types, rotation, suppression) : étape 4 (tauri-v0.5.0) ; bouton 📜 Scripts de la fenêtre d'infos et scripts Editor : étape 7.
- **Renommer / déplacer** est une commande propre à Tauri (la version egui passe par les scripts et Library Organizer) : mêmes modèles et mêmes refus de conflit, pas de suffixe « (2) » automatique ni d'« Annuler le dernier rangement ».
- « Arrêter » agit **entre deux livres**, jamais au milieu d'un fichier ou d'un déplacement : le livre en cours se termine.
- Listes de ComicRack non modifiables (étape 5) ; pas de sélecteur de dossier natif (le dossier cible se tape ou se colle).
- Le mot de passe MySQL n'est jamais écrit dans les réglages d'édition ; les réglages de connexion MySQL autonome viendront à l'étape 6.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 41 | | |
| Majeurs [M] | 25 | | |
| Mineurs [m] | 11 | | |

Décision et remarques :
