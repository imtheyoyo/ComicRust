# Recette ComicRust Tauri tauri-v0.5.0

| | |
|---|---|
| Version | tauri-v0.5.0 (pré-release de la version Tauri 2 : étape 4, **pages**) |
| Paquet | `ComicRust-Tauri-tauri-v0.5.0-windows-x64.zip` |
| SHA-256 | à reporter |
| Contenu | onglet **« Pages »** de la fenêtre « Infos du livre » (vignettes ou détails, sélection multiple, glisser-déposer, ◀ ▶ / Alt+flèches, Au début / À la fin, types de page, rotation, position en double page, ⧉ page double en lecture seule, Supprimer / Restaurer, favori, aperçu agrandi, Ctrl+C, Réinitialiser ▾) ; pages enregistrées avec les champs par **OK / Appliquer** (base de ComicRust, base MySQL de ComicRack en « Lecture et écriture », `<Pages>` du `ComicInfo.xml` si « Écrire aussi les infos dans le fichier ») ; **fichier libéré avant toute écriture** puis rechargé (os error 5) ; correctif **I-03** (Échap) |
| Origine | étape 4 de [`docs/tauri/PLAN.md`](../docs/tauri/PLAN.md), contrat [`docs/tauri/ipc-pages.md`](../docs/tauri/ipc-pages.md) ; tauri-v0.4.0 **GO** le 08/11/2026 ([rapport](rapports/tauri-v0.4.0-2026-11-08.txt) : I-03 en échec, repris ici). Tests repris des recettes egui [v0.10.6](v0.10.6.md) (P-01 à P-05) et [v0.10.7](v0.10.7.md) (P-01 / P-02 : « Appliquer » sans os error 5 sur le NAS) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ **Cette version réécrit l'ordre et le type des pages** dans la base et, sur demande, dans les
> fichiers et dans la base de ComicRack. **Avant les tests d'écriture, faire une sauvegarde de la
> base ComicRack** (PR-04) et travailler sur des **copies** de BD (dossiers d'essai de la
> tauri-v0.4.0), jamais sur les originaux de la collection.
>
> Garder les **deux copies** : une **autonome** et une **liée** à ComicRack.
> **Échantillons** : `F:\Projet REA par Claude\REA ComicRack\ComicRust-echantillons`.
>
> **Valeurs de référence** : « thanos » **484** livres (tauri-v0.4.0 ; ComicRack : 483) ; la
> bibliothèque liée compte **5 325** livres.
>
> **Lire les `<Pages>` d'un CBZ** : l'ouvrir avec 7-Zip, afficher `ComicInfo.xml` ; chaque page est
> une ligne `<Page Image="…" Type="…" … />` dans l'**ordre de lecture** (l'attribut `Image` est le
> rang de l'image dans le fichier, à partir de 0). Comparer deux fichiers extraits dans `avant\` et
> `apres\` : `Compare-Object (Get-Content avant\ComicInfo.xml) (Get-Content apres\ComicInfo.xml)`.
>
> **Lecteur et fenêtre d'infos** : dans la version Tauri, le lecteur occupe toute la fenêtre ; on
> ne peut pas modifier les infos d'un livre **pendant** sa lecture. Le cas « livre ouvert dans le
> lecteur pendant l'écriture » de la v0.10.7 se teste donc par une lecture **fermée juste avant**
> l'écriture (préchargement des pages et bandeau des miniatures encore en cours) et par les
> **vignettes de l'onglet Pages**, ouvertes sur le fichier pendant « Appliquer » (cause exacte de
> l'os error 5 de la v0.10.6).

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **79 tests** du backend `comicrust-tauri` (69 de la tauri-v0.4.0 + 10, dont favori retiré et libération sans interblocage) : pages d'une fiche **complétées comme ComicRack** (images manquantes ajoutées, couverture avant par défaut, doublons écartés), **opérations comme ComicRack** (déplacer, début / fin, type, Supprimée ↔ Histoire, rotation, position, favori, réinitialisations, tri par nom) et leur forme JSON, liste vérifiée à l'enregistrement (chaque image une fois, tailles reprises de la fiche) ; onglet Pages sur un CBZ généré (lecture, vignettes, réordonner, supprimer, type, rotation, favori), **base modifiée puis fichier écrit seulement si demandé** ; `ComicDb.xml` en **consultation seulement** ; **fichier libéré pendant l'écriture puis rechargé** (aucun descripteur ouvert après le service des pages, lectures de pages / vignettes / couvertures suspendues jusqu'à la fin de l'écriture) | `cargo test` (`comicrust-tauri`) | ✅ 79/79 (08/11/2026) |
| **Base MySQL de test** (MariaDB locale, jamais la base de démonstration) : onglet Pages en **« Lecture et écriture »** : élément `<Pages>` de la fiche ComicRack réécrit (protocole des compteurs, fiche relue avant l'écriture), puis `ComicInfo.xml` du fichier ; lecture seule : enregistrement refusé | `cargo test` avec `COMICRUST_TEST_MYSQL` (Linux) | ✅ (08/11/2026) |
| Front : `svelte-check` (179 fichiers, 0 erreur, 0 avertissement), `vite build` | local + CI Windows (`ci-tauri.yml`) | ✅ svelte-check (08/11/2026) ; build ✅ |
| Parcours simulé dans un navigateur (Playwright, mode simulé sans Tauri) : onglet Pages (vignettes / détails, sélection, glisser-déposer au pointeur avec trait d'insertion, ◀ ▶, types, rotation, Supprimer, favori, aperçu, Réinitialiser ▾, ● « Pages » puis Appliquer et OK) ; **Échap** 1, 2 et 3 fois dans Infos avec et sans modification, réouverture par Ctrl+I ; aucune erreur JavaScript | Playwright (mode simulé) | ✅ pages, Échap, édition, lecteur, course Suppr+Ctrl+Entrée (08/11/2026) |
| Clippy `-D warnings`, rustfmt, tests, compilation de l'application complète (`--features app,custom-protocol`) | CI Windows (`ci-tauri.yml`) | ✅ CI Tauri verte (run 42456349554) |

**Non vérifiable ici** : la vraie fenêtre WebView2 (glisser-déposer, presse-papiers, comportement
d'Échap de Chromium), les écritures sur le partage SMB du NAS (os error 5), le verrouillage des
fichiers par Windows, la vraie base MySQL de ComicRack et ce que ComicRack affiche après relance
(sections 1 à 9).

## 1. Préparation

- [ ] **PR-01 [B]** Vérifier l'empreinte : `Get-FileHash .\ComicRust-Tauri-tauri-v0.5.0-windows-x64.zip` → identique au SHA-256 ci-dessus. Le zip contient `ComicRust-Tauri.exe`, `pdfium.dll`, `README.md`, `RECETTE.md` et `licences\`.
- [ ] **PR-02 [B]** Copier le contenu du zip **par-dessus** les deux copies tauri-v0.4.0 (dossiers `data\` conservés) et lancer chacune : pas d'assistant, même mode (Autonome / Lié), mêmes livres, mêmes préférences d'affichage, réglages du lecteur et réglages d'édition (case « Écrire aussi », « Lecture et écriture » du mode lié) qu'en tauri-v0.4.0.
- [ ] **PR-03 [B]** **Dossiers d'essai** de la tauri-v0.4.0 (local et NAS) : remettre au besoin des **copies neuves** de quelques vrais CBZ (dont un d'au moins 20 pages avec une page double, ex. un poster central) et de `Thanos Quest 01 (1990).cbz` ; un CBR et un PDF d'essai. Copie autonome : « Lancer le scan » dans « 📁 Dossiers surveillés ».
- [ ] **PR-04 [B]** **Sauvegarde de la base ComicRack** avant la section 6 : ComicRack fermé, exporter la base MySQL (ex. `mysqldump -h <serveur> -u <utilisateur> -p <base> > comicrack-avant-tauri-v0.5.0.sql`, valeurs de `ComicRack.ini`). Taille du fichier de sauvegarde : ………
- [ ] **PR-05 [M]** Copie liée : les livres d'essai du NAS (ajoutés à ComicRack en tauri-v0.4.0) sont toujours là (recherche par leur titre).

## 2. Échap dans les fenêtres (correctif I-03)

Copie autonome. Défaut de la tauri-v0.4.0 : avec des modifications, Échap deux fois fermait la
fenêtre Infos, qui ne se rouvrait plus sans relancer ComicRust.

- [ ] **I-03 [B]** Fenêtre **Infos** d'un livre, **avec modifications** (changer la Série : « ● 1 champ(s) modifié(s) ») : Échap **1 fois, 2 fois, 3 fois** (lentement puis rapidement) : la fenêtre **reste ouverte**, la saisie est intacte ; « Annuler » la ferme. Rouvrir **par Ctrl+I**, puis refaire et rouvrir **par clic droit → « ✏ Modifier les infos… »**, puis **par le bouton « ✏ Modifier »** du panneau de détails : la fenêtre s'ouvre à chaque fois, sur le bon livre, sans la modification abandonnée.
- [ ] **I-13 [B]** Fenêtre Infos **sans modification** : Échap la ferme du premier coup ; rouvrir aussitôt par Ctrl+I, Échap, menu, Échap, bouton « ✏ Modifier », Échap : ouverture et fermeture à chaque fois. Après un « ↺ » qui annule la seule modification, Échap ferme de nouveau.
- [ ] **I-14 [M]** Échap à l'intérieur d'Infos : liste de suggestions ouverte (Éditeur) → Échap ferme **la liste seulement** ; onglet Pages, aperçu agrandi ouvert → Échap ferme **l'aperçu seulement** ; boîte « Favori » ouverte → Échap l'annule seulement. Avec des **pages modifiées** seulement (aucun champ) : Échap 1, 2, 3 fois ne ferme pas la fenêtre.
- [ ] **D-01 [M]** **Renommer / déplacer…** : Échap 1, 2, 3 fois ferme la fenêtre dès le 1er (aperçu seulement, rien renommé) ; rouvrir par le menu : elle s'ouvre. Pendant un lot (« Renommer N fichier(s) » sur plusieurs livres) : Échap ne ferme pas.
- [ ] **D-02 [M]** **Convertir en CBZ** et **Écrire les infos dans le(s) fichier(s)** : Échap avant de lancer ferme ; pendant la progression, Échap 1, 2, 3 fois ne ferme pas (Arrêter reste possible) ; après le bilan, Échap ferme ; rouvrir par le menu : elle s'ouvre.
- [ ] **D-03 [M]** **⚙ Réglages**, **📁 Dossiers surveillés** et **« Ouvrir un fichier… »** : Échap ferme ; Échap 2 et 3 fois de suite n'ont pas d'effet de bord (pas de fenêtre fantôme, la liste reprend la main) ; chaque fenêtre se rouvre aussitôt par son bouton, 3 fois de suite.

## 3. Onglet « Pages »

Copie autonome, livres du dossier d'essai local (copies). Ctrl+I sur **un** livre → onglet
**« Pages »** (dernier onglet).

- [ ] **PG-01 [B]** L'onglet « Pages » n'existe que pour **un seul livre ayant un fichier** (absent avec plusieurs livres sélectionnés). Ouverture : « Lecture des pages du fichier… » puis vignettes numérotées 1…N dans l'ordre enregistré, légende du type sous les pages qui ne sont pas « Histoire » ; en-tête « Glisser-déposer, boutons ou menu contextuel (clic droit) pour changer l'ordre ou le type des pages : » ; la fenêtre s'élargit sur cet onglet.
- [ ] **PG-02 [M]** **Vignettes / Détails** : « Détails » montre la liste N°, Image, Type, Rotation, Double page, Favori, Dimensions ; la liste **Taille** (Petites, Moyennes, Grandes) et **Ctrl+molette** changent la taille des vignettes (liste grisée en Détails). Fermer, rouvrir Infos : vue et taille retrouvées.
- [ ] **PG-03 [B]** **Sélection** : clic, **Ctrl+clic**, **Maj+clic**, **Ctrl+A** ; au clavier dans la liste : flèches, Début / Fin, Pg préc. / Pg suiv., Maj+flèches (plage), Ctrl+flèche puis **Espace** (ajoute / retire) ; pied : « Page n — <nom> » ou « N pages choisies ». Clic droit → « Inverser la sélection ».
- [ ] **PG-04 [B]** **Glisser-déposer sous WebView2** : glisser une page (puis une sélection de 3 pages non contiguës) vers une autre place : un **trait d'insertion** suit le pointeur, la liste défile quand on approche du bord haut / bas ; au relâché, les pages sont à la nouvelle place, groupées dans l'ordre de la liste, et l'onglet devient « ● Pages », le pied « ● Pages modifiées : enregistrées par OK ou Appliquer ». Même chose en vue Détails. Rien ne s'ouvre ni ne se copie hors de la fenêtre (pas de glisser de fichier Windows).
- [ ] **PG-05 [B]** **◀ ▶** (« Une place vers le début (Alt+←) » / « … vers la fin (Alt+→) ») et **Alt+← / Alt+→** (Alt+↑ / Alt+↓ aussi) : la sélection avance ou recule d'une place ; ◀ grisé quand la sélection est déjà au début, ▶ à la fin. Raccourcis enchaînés sans recliquer (le focus reste sur la liste).
- [ ] **PG-06 [B]** **« ⇤ Au début »** (Ctrl+Alt+T) et **« À la fin ⇥ »** (Ctrl+Alt+B) sur une sélection, par bouton, raccourci et clic droit (« Mettre au début » / « Mettre à la fin ») ; la couverture avant mise à la fin devient « Autre » et la page suivante devient « Couverture avant » (comme ComicRack).
- [ ] **PG-07 [B]** **Types** : liste « Type » de la barre, clic droit « Type : … » (✓ sur le type actuel) et raccourcis **Alt+Maj+A** Couverture avant, **B** Couverture intérieure, **C** Résumé, **D** Histoire, **E** Publicité, **F** Éditorial, **G** Courrier des lecteurs, **H** Aperçu, **I** Couverture arrière, **J** Autre, **K** Supprimée. Sur plusieurs pages de types différents, la liste montre « (valeurs multiples) ».
- [ ] **PG-08 [B]** **Rotation** : liste « Rotation » (Aucune, 90°, 180°, 270°), clic droit « Rotation : … », **Alt+Maj+6 / 7 / 8 / 9** : la vignette est tournée **dans son cadre** (sans déborder), la colonne Rotation de Détails est à jour.
- [ ] **PG-09 [M]** **Position en double page** : liste « Double page » (Par défaut, Proche, Éloignée) sur une page : légende « · Proche » / « · Éloignée » sous la vignette, colonne Double page en Détails.
- [ ] **PG-10 [B]** **⧉ Page double en lecture seule (comme ComicRack)** : la page double du livre (plus large que haute) porte le badge **⧉** (bulle « Page double »), « ⧉ » en Détails ; **aucune** commande ne permet de le changer (ni menu, ni liste, ni raccourci). Comparer avec ComicRack (onglet Pages du même livre, si présent dans ComicRack) : mêmes pages marquées doubles.
- [ ] **PG-11 [B]** **« ✖ Supprimer » / « ↺ Restaurer »** (bouton, touche **Suppr**, clic droit « ✖ Marquer comme Supprimée » / « ↺ Restaurer (ne plus marquer Supprimée) ») : la page est grisée avec une croix ✖, type « Supprimée » ; restaurer la remet en « Histoire ».
- [ ] **PG-12 [M]** **Favori** : ★ ou **Ctrl+Maj+B** sur une page : boîte « Favori », texte proposé « Page n » (ou le favori actuel), Entrée = OK ; légende « ★ » et colonne Favori ; clic droit « Supprimer favoris » ou **Ctrl+Maj+D** le retire ; avec plusieurs pages choisies, ★ est grisé.
- [ ] **PG-13 [M]** **Aperçu agrandi** : 🔍, **Entrée** ou **double-clic** : « Page n / N » avec nom, type, rotation, « page double », favori ; ◀ ▶, ← →, Début / Fin parcourent les pages (la sélection suit) ; Suppr y marque la page Supprimée ; Échap ou ✖ ferme l'aperçu (pas la fenêtre).
- [ ] **PG-14 [M]** **Ctrl+C** (ou clic droit « 📋 Copier la page ») sur **une** page : « Page n copiée dans le presse-papiers. » ; coller dans Paint ou WordPad : l'image en **taille d'origine**. Sur **plusieurs** pages : « N chemin(s) de page copié(s). » ; coller dans le Bloc-notes : une ligne « <chemin du fichier> | <nom de l'image> » par page.
- [ ] **PG-15 [B]** **Réinitialiser ▾** : « Réinitialiser » remet l'ordre du fichier (types gardés) ; ▾ : « Remettre dans l'ordre d'origine », « Trier par nom », « Trier par nom (numérique) », « Réinitialiser les types » (1re page Couverture avant, les autres Histoire), « Tout réinitialiser (ordre, types, rotation, position, favoris) » : effets visibles ; revenir exactement à la liste enregistrée enlève le « ● » de « Pages ».
- [ ] **PG-16 [B]** **Lecture seule** : copie **liée en lecture seule** (« Lecture et écriture » décochée), Ctrl+I sur un livre → Pages : « Pages du livre (consultation seulement) : » et « 🔒 … » (marche à suivre) ; **pas** de barre d'outils ni de « Réinitialiser » ; glisser, Suppr, Alt+Maj+… sont sans effet ; le clic droit ne propose que Aperçu agrandi, Copier la page, Tout sélectionner, Inverser la sélection, Rafraîchir ; sélection, aperçu et Ctrl+C marchent ; « OK » ferme sans rien écrire (relancer ComicRack : pages du livre inchangées).
- [ ] **PG-17 [m]** Clic droit → « 🔄 Rafraîchir » : vignettes relues, modifications non enregistrées gardées. Touche **Menu** / **Maj+F10** ouvre le menu sur la page courante. Un livre PDF : onglet Pages avec « Page 001 »… en noms d'images.
- [ ] **PG-18 [M]** Favori retiré : poser un favori sur une page, **Appliquer**, le retirer (Ctrl+Maj+D), **OK**, rouvrir Ctrl+I → Pages : le favori n'est plus là (vérifier aussi dans le `ComicInfo.xml` si la case était cochée).

## 4. Enregistrer les pages : Appliquer, OK

Copie autonome, **copie** d'un vrai CBZ du dossier d'essai local. Avant de commencer, extraire
son `ComicInfo.xml` dans `avant\` et noter la date de modification du fichier.

- [ ] **W-01 [B]** **Sans « Écrire aussi les infos dans le fichier »** : déplacer une page, en marquer une Supprimée, une autre « Publicité », tourner une page de 90°, **« Appliquer »** : « Enregistré : 1 livre(s) modifié(s). », plus de « ● », « Appliquer » grisé, les vignettes se rechargent dans le nouvel ordre. **Le fichier est inchangé** (même date, même `ComicInfo.xml`) ; l'en-tête de la fenêtre indique « **infos pas encore écrites dans le fichier** ». Fermer, rouvrir Infos → Pages : ordre, types et rotation gardés.
- [ ] **W-02 [B]** **Avec la case cochée** : modifier encore l'ordre et un type, « Appliquer » : « Enregistré : … », la mention « infos pas encore écrites » disparaît ; extraire le `ComicInfo.xml` dans `apres\` : l'élément **`<Pages>`** liste les pages dans le **nouvel ordre de lecture** (attributs `Image` dans cet ordre), avec `Type="Deleted"`, `Type="Advertisement"`, `Rotation`, et les autres champs de la fiche **inchangés** (Compare-Object : seules les lignes de l’élément `<Pages>` diffèrent).
- [ ] **W-03 [B]** Contre-vérification du fichier avec **la version egui** (« 📂 Ouvrir » puis « ℹ Infos » → onglet Pages) **ou ComicRack** (ouvrir le fichier hors bibliothèque, infos lues depuis le fichier) : même ordre, mêmes types, même rotation ; la page Supprimée n'est pas lue. Outil utilisé : ………
- [ ] **W-04 [B]** **OK après Appliquer n'écrit rien** : après W-02, noter la date du fichier, puis « OK » : la fenêtre se ferme, **la date ne change pas**. De même, rouvrir, ne rien changer, « OK » : rien n'est écrit.
- [ ] **W-05 [M]** Pages **et** champs ensemble : modifier le Titre et l'ordre des pages, « OK » (case cochée) : un seul enregistrement, `<Title>` et `<Pages>` à jour dans le fichier ; pied « ● 1 champ(s) modifié(s), pages modifiées » avant le OK.
- [ ] **W-06 [M]** Sur un **CBR** : la case est grisée ; modifier les pages, OK : enregistrées dans la base (rouvrir : gardées), fichier intact.
- [ ] **W-07 [m]** Après W-01, clic droit sur le livre → « Écrire les infos dans le fichier » : le `<Pages>` du fichier reprend l'ordre enregistré ; la mention « infos pas encore écrites » disparaît.

## 5. Reprises de la v0.10.7 : NAS (os error 5) et lecteur

Dossier d'essai du **NAS** (copies), copie autonome puis copie liée en écriture (section 6).

- [ ] **P-01 [B]** Ctrl+I sur un CBZ **du NAS**, case « Écrire aussi les infos dans le fichier » cochée → onglet Pages (vignettes chargées, ouvertes sur le fichier) : déplacer une page, changer un type, **« Appliquer »** : « Enregistré : 1 livre(s) modifié(s). », **aucune erreur « Accès refusé. (os error 5) »** ; les vignettes se rechargent depuis le fichier réécrit ; puis **OK** : rien n'est réécrit (date inchangée). **Lecteur juste avant l'écriture** : ouvrir ce livre dans le lecteur, bandeau des miniatures (**T**) affiché, aller à la page 5, Échap, **aussitôt** Ctrl+I → Pages → modifier l'ordre → « Appliquer » : pas d'os error 5 ; rouvrir le livre dans le lecteur : il **recharge le fichier réécrit** (nouvel ordre, aucune page cassée ni « rouvrir le livre »), reprise à la page lue. Refaire 3 fois.
- [ ] **P-02 [B]** **Le lecteur respecte les pages** : sur un livre modifié (ordre changé, une page Supprimée, une page tournée de 90°), l'ouvrir dans le lecteur : les pages sont dans le **nouvel ordre**, la page supprimée **n'apparaît pas** (ni dans le bandeau des miniatures), la page tournée s'affiche **tournée** (et sa miniature aussi, dans son cadre) ; la page ⧉ double reste seule en mode double page (**D**). Même vérification en copie liée sur un livre dont ComicRack a modifié les pages.
- [ ] **P-03 [M]** Même chose que P-01 sur 3 autres CBZ du NAS, à la suite, avec « Précédent / Suivant » entre chaque : aucune erreur, chaque fichier contient son `<Pages>`.
- [ ] **P-04 [m]** NAS débranché pendant « Appliquer » sur un CBZ du NAS : erreur lisible dans le bilan (« Pas tout enregistré … »), la fenêtre ne gèle pas, les modifications restent saisies ; NAS rebranché, « Appliquer » réussit ; le fichier n'est pas abîmé.

## 6. ComicRack MySQL en écriture

Copie liée, **sauvegarde PR-04 faite**, livres d'essai du NAS, ⚙ Réglages → « Lecture et écriture »
activée. **ComicRack fermé** pendant les écritures ; le relancer pour vérifier.

- [ ] **L-01 [B]** Ctrl+I sur un livre d'essai → Pages (plus de « consultation seulement ») : déplacer deux pages, en marquer une Supprimée, une « Publicité », tourner une page, OK (case **décochée**) ; relancer ComicRack : **onglet Pages** du livre (Infos) : même ordre, mêmes types, même rotation ; ComicRack **lit** le livre dans cet ordre, sans la page supprimée ; tous les autres champs de la fiche inchangés.
- [ ] **L-02 [B]** Même chose **avec la case cochée** : ComicRack relancé montre les pages modifiées, et le livre n'est **pas** dans « Fichiers à mettre à jour » ; le `ComicInfo.xml` du fichier contient le même `<Pages>`. Sans la case (L-01) : le livre est dans « Fichiers à mettre à jour ».
- [ ] **L-03 [M]** **Modification faite dans ComicRack conservée** : dans ComicRack, changer les Notes d'un livre d'essai, fermer ; dans ComicRust (relancé), modifier seulement ses pages, OK ; relancer ComicRack : **les deux** modifications sont là.
- [ ] **L-04 [M]** Retour : dans ComicRack, modifier l'ordre des pages d'un livre d'essai, fermer ComicRack ; relancer la copie liée de ComicRust : l'onglet Pages et le lecteur montrent l'ordre de ComicRack.
- [ ] **L-05 [B]** Fin de section : ComicRack rouvre ta bibliothèque intacte (hors livres d'essai), aucune erreur ; remettre au besoin la sauvegarde PR-04.

## 7. Précédent / Suivant avec des pages modifiées

Copie autonome, plusieurs livres du dossier d'essai à la suite dans la vue.

- [ ] **V-01 [B]** Onglet Pages, modifier l'ordre, **« Suivant ▶ »** (bulle « … (les modifications sont enregistrées) ») : les pages du livre courant sont enregistrées, la fenêtre montre le **livre suivant**, l'onglet Pages affiche **ses** vignettes (pas celles du précédent), sans « ● » ; « ◀ Précédent » : le premier livre a bien son nouvel ordre.
- [ ] **V-02 [M]** Enchaîner Suivant 5 fois rapidement sur l'onglet Pages : chaque livre montre ses propres pages, aucune vignette d'un autre livre, aucun « Fichier illisible » ; une modification sur le 3ᵉ livre n'arrive ni au 2ᵉ ni au 4ᵉ.
- [ ] **V-03 [m]** Arrivé sur un livre **sans fichier** (fichier manquant) avec Suivant : l'onglet Pages disparaît, la fenêtre revient au premier onglet sans erreur.

## 8. Mode autonome

- [ ] **A-01 [B]** Fermer et relancer la copie autonome : ordres, types, rotations et favoris enregistrés en sections 3 à 7 sont retrouvés (onglet Pages et lecteur) ; une modification faite **sans** la case « Écrire aussi » n'a modifié **aucun fichier**.

## 9. Non-régression courte (tauri-v0.4.0)

- [ ] **N-01 [B]** Copie liée : recherche « thanos » → **484** livres ; Échap vide la recherche. Nombre : ………
- [ ] **N-02 [B]** **Édition d'un champ** : Ctrl+I, modifier la Série, OK : grille, tableau et panneau de détails à jour ; avec la case cochée sur un CBZ, seule la ligne `<Series>` du `ComicInfo.xml` change (les `<Pages>` restent ceux d'avant).
- [ ] **N-03 [B]** **Lecteur** : double-clic, Entrée et « Lire » ouvrent le livre ; → ← D V M T fonctionnent ; Échap revient à la bibliothèque, même livre sélectionné ; reprise à la dernière page lue.
- [ ] **N-04 [B]** **Rien hors de `data\`** : après tous ces tests, aucun dossier `ComicRust` ni `fr.imtheyoyo.comicrust` dans `%APPDATA%` ni `%LOCALAPPDATA%` ; à côté des BD, aucun fichier temporaire restant (dossier local et NAS).

---

## Limites connues (ne pas compter comme anomalies)

- Ouvrir 4 livres dans le lecteur pendant que la fenêtre Infos est ouverte peut fermer la séance de vignettes de l'onglet Pages (vignettes vides : rouvrir l'onglet ou « 🔄 Rafraîchir »).
- Sur un NAS très lent (lecture d'une page de plus de 10 s), l'écriture peut démarrer pendant la lecture : nouvelles tentatives automatiques, os error 5 possible dans ce cas extrême.
- **Page double (⧉) non modifiable** : elle est **calculée** comme ComicRack (`IsDoublePage` : image
  plus large que haute, ou attribut `DoublePage` du fichier), qui ne la rend pas modifiable non plus.
  Pour l'affichage en double page, seule la **position** (Proche / Éloignée) se règle.
- **Position en double page** : enregistrée (base, ComicRack, `ComicInfo.xml`) mais pas encore
  appliquée par le lecteur Tauri, qui place les pages comme avant.
- **Dimensions** (colonne de la vue Détails) **absentes si inconnues** de la fiche (livre jamais
  ouvert dans ComicRack ni analysé) : la vraie taille arrive avec la vignette, sans remplir la colonne.
- **Copie d'une page (Ctrl+C) à confirmer sous WebView2** : l'image est copiée en PNG par le
  presse-papiers du navigateur ; si Windows la refuse, un message « Copie impossible : … » s'affiche
  (à noter en anomalie [m], pas bloquant). Plusieurs pages : chemins en texte seulement.
- **Lecteur et fenêtre d'infos ne s'affichent pas en même temps** (une seule fenêtre) : voir l'encadré
  en tête ; le fichier est de toute façon libéré avant chaque écriture et rechargé après.
- L'onglet Pages ne sert qu'à **un livre à la fois** (comme la version egui et ComicRack) ;
  `ComicDb.xml` et ComicRack sans « Lecture et écriture » : **consultation seulement**.
- Les pages ne s'écrivent dans le fichier que pour les **CBZ** (comme les infos).
- Bouton 📜 Scripts de la fenêtre d'infos et scripts Editor : étape 7 ; listes de ComicRack : étape 5.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 32 | | |
| Majeurs [M] | 17 | | |
| Mineurs [m] | 4 | | |

Décision et remarques :
