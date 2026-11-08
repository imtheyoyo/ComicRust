# Recette ComicRust Tauri tauri-v0.6.0

| | |
|---|---|
| Version | tauri-v0.6.0 (pré-release de la version Tauri 2 : étape 5, **listes ComicRack**) |
| Paquet | `ComicRust-Tauri-tauri-v0.6.0-windows-x64.zip` |
| SHA-256 | *à reporter (calculé par la CI à la publication)* |
| Contenu | **panneau des listes** (arbre identique à ComicRack, compteurs ComicRust / ComicRack, infobulle, « 📋 Copier le bilan des listes », clavier) ; **vue d'une liste** (tri « # Ordre de la liste », colonne « # ») ; **listes de lecture** (ajout par glisser-déposer et par menu, retrait par Suppr, Monter / Descendre, réordonnancement au glisser, séries entières) ; **gestion de l'arbre** (nouvelle liste / dossier / liste intelligente, Renommer F2, Modifier, Dupliquer, Supprimer, Convertir, Déplacer) ; **éditeur de listes intelligentes** ; **écriture de `ComicDb.xml`** (ComicRack fermé, sauvegardes) ; **import / export `.cbl`** ; correctif **D-01** (Échap dans « Renommer / déplacer » modifiée) |
| Origine | étape 5 de [`docs/tauri/PLAN.md`](../docs/tauri/PLAN.md), contrat [`docs/tauri/ipc-listes.md`](../docs/tauri/ipc-listes.md) ; tauri-v0.5.0 **GO** le 08/11/2026 ([rapport](rapports/tauri-v0.5.0-2026-11-08.txt) : remarque D-01 reprise ici). Tests repris des recettes egui [v0.5.0](v0.5.0.md), [v0.5.1](v0.5.1.md), [v0.5.2](v0.5.2.md) (bilan des listes : **46/47** identiques à ComicRack), [v0.9.0](v0.9.0.md), [v0.9.1](v0.9.1.md) (gestion, écriture, glisser-déposer), [v0.10.1](v0.10.1.md) (clavier de l'arbre) et [v0.10.2](v0.10.2.md) (série entière dans une liste triée par ordre) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ **Cette version peut réécrire les listes de ComicRack** (`ComicDb.xml`) quand « Lecture et
> écriture » est activée. **Avant toute écriture, sauvegarder `ComicDb.xml` ET la base MySQL de
> ComicRack** (PR-03, PR-04). Les tests d'écriture (section 9) se font **ComicRack fermé** ; on le
> relance seulement pour vérifier.
>
> Garder les **deux copies** : une **autonome** (listes dans la base de ComicRust : sections 6, 7,
> 8, 10 et 11) et une **liée** à ComicRack (sections 3 à 5, 9). **Échantillons** :
> `F:\Projet REA par Claude\REA ComicRack\ComicRust-echantillons`.
>
> **Valeurs de référence** : bibliothèque liée **5 325** livres (tauri-v0.5.0 ; 5 323 lors du
> transfert egui v0.8.0) ; **47 listes** (en octobre : 1 bibliothèque, 8 dossiers, 9 listes de
> lecture, 29 listes intelligentes) ; recherche « thanos » **484** livres. Bilan egui v0.5.2 :
> **46/47** listes identiques, seul écart « Lecture en cours » (ComicRust 1 / ComicRack 0, une
> lecture faite dans ComicRust). Comptes d'octobre, **à titre indicatif** (la référence est le
> nombre de ComicRack dans le bilan) : COSMIC COLLECTION (Chemin) 345, BD_X 488, Séries complétes
> (Chemin) 3151, Séries complétes (A scrapper) 3137, Séries à définir 5298, Séries complétes 14,
> Doublons 154, Fichiers à mettre à jour 20.
>
> **Comptes de ComicRack** : il n'enregistre le nombre de livres de chaque liste (`BookCount`)
> qu'en sauvegardant `ComicDb.xml` (à sa fermeture, et toutes les 10 minutes) : **fermer ComicRack
> une fois avant la section 3**. Le chemin de `ComicDb.xml` utilisé est écrit en tête du bilan.
>
> **Comparer deux `ComicDb.xml`** (PowerShell) :
> `Compare-Object (Get-Content avant\ComicDb.xml) (Get-Content apres\ComicDb.xml) | Measure-Object`
> puis sans `| Measure-Object` pour voir les lignes.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **95 tests** du backend `comicrust-tauri` (79 de la tauri-v0.5.0 + 16), dont 15 pour les listes : **arbre identique à ComicRack sur la fixture réelle anonymisée** (ton `ComicDb.xml`, **47 listes**, référence produite indépendamment du Rust par ElementTree : noms, types, ordre, imbrication, `BookCount`) et bilan de 47 lignes ; livres d'une liste dans son ordre ; **écriture de `ComicDb.xml` sûre** (lecture seule sans l'option, refus ComicRack lancé — vérifié aussi juste avant le remplacement —, fichier modifié depuis la lecture refusé, seul `<ComicLists>` remplacé, sauvegardes `…-origine-….xml.bak` et 10 dernières) ; **critères non modifiés gardés à l'octet** ; toutes les opérations en autonome (créer, renommer, modifier, dupliquer, supprimer avec dépendances, convertir, déplacer, ajouter / retirer / réordonner des livres, séries entières) ; une opération à la fois, erreurs lisibles ; **import / export `.cbl`** (trouvés par identifiant, par série et numéro, manquants ; export possible en lecture seule, import refusé) ; mode lié MySQL : listes lues dans le `ComicDb.xml` trouvé d'après `ComicRack.ini` ; ajoutés par la revue : identifiants de livres contrôlés, `ComicDb.xml` avec BOM gardé à l'octet, critère non modifiable gardé, fichiers `.cbl` hostiles, **5 000 livres et 47 listes** (temps de calcul) | `cargo test -p comicrust-tauri` (Linux) | ✅ 90/90 avant la revue (08/11/2026) ; 95 après la revue : *à reporter* |
| **Base MySQL de test** (MariaDB locale, jamais la base de démonstration) : base de ComicRust partagée par **deux postes**, modification des listes concurrente refusée sans rien écrire | `cargo test` avec `COMICRUST_TEST_MYSQL` | *à reporter* |
| Cœur (`comicrust-core`, déjà validé par la version egui) : ton `ComicDb.xml` relu puis réécrit sans écart (`real_comicdb_roundtrip`), critères au format `XmlSerializer` de ComicRack, listes intelligentes calculées comme ComicRack CE | `cargo test --all` | *à reporter* |
| Front : `svelte-check` (**185 fichiers**, 0 erreur, 0 avertissement), `vite build` | local + CI Windows (`ci-tauri.yml`) | ✅ svelte-check (08/11/2026) ; build *à reporter* |
| Parcours simulé dans un navigateur (Playwright, mode simulé sans Tauri, listes simulées) : arbre et clavier, menu contextuel, création / renommage / suppression, glisser de livres sur une liste, réordonnancement, séries entières, éditeur de listes intelligentes (aperçu, OK / Appliquer), import / export `.cbl`, Échap dans les fenêtres (D-01) ; aucune erreur JavaScript | Playwright (mode simulé) | *à reporter* |
| Clippy `-D warnings`, rustfmt, tests, compilation de l'application complète (`--features app,custom-protocol`) | CI Windows (`ci-tauri.yml`) | *à reporter* |

**Non vérifiable ici** : la vraie fenêtre WebView2 (glisser-déposer, curseurs, presse-papiers),
la détection de `ComicRack.exe` sous Windows, ta vraie bibliothèque (5 325 livres : **les comptes
complets** des listes intelligentes), la vraie base MySQL de ComicRack, ce que ComicRack affiche
après relance, ton fichier `.cbl` Thanos (sections 1 à 12).

## 1. Préparation

- [ ] **PR-01 [B]** Vérifier l'empreinte : `Get-FileHash .\ComicRust-Tauri-tauri-v0.6.0-windows-x64.zip` → identique au SHA-256 ci-dessus. Le zip contient `ComicRust-Tauri.exe`, `pdfium.dll`, `README.md`, `RECETTE.md` et `licences\`.
- [ ] **PR-02 [B]** Copier le contenu du zip **par-dessus** les deux copies tauri-v0.5.0 (dossiers `data\` conservés) et lancer chacune : pas d'assistant, même mode (Autonome / Lié), mêmes livres, mêmes préférences d'affichage et réglages d'édition qu'en tauri-v0.5.0. Le panneau de gauche montre, sous « ▦ Bibliothèque » et « ❏ Séries », le titre **« Listes »** avec le bouton **« ＋ »** et l'arbre des listes.
- [ ] **PR-03 [B]** **Sauvegarde de `ComicDb.xml`** : **ComicRack fermé**, copier `ComicDb.xml` (chemin en tête du bilan des listes, section 3) dans un dossier `avant\`. Taille du fichier : ………
- [ ] **PR-04 [B]** **Sauvegarde de la base MySQL de ComicRack** : ComicRack fermé, `mysqldump -h <serveur> -u <utilisateur> -p <base> > comicrack-avant-tauri-v0.6.0.sql` (valeurs de `ComicRack.ini`). Taille du fichier de sauvegarde : ………
- [ ] **PR-05 [M]** Fichiers `.cbl` à portée de main : ta **liste Thanos réelle** (guide Crushing Krisis, numéros non possédés compris) et l'échantillon `Thanos.cbl` (un numéro volontairement absent). Copie autonome : échantillons scannés (« 📁 Dossiers surveillés » → « Lancer le scan »).

## 2. Échap dans les fenêtres (remarque D-01 de la tauri-v0.5.0)

Copie autonome. Défaut de la tauri-v0.5.0 : avec plusieurs fichiers choisis et le **modèle de nom
modifié**, Échap fermait « Renommer / déplacer » (saisie perdue). Règle désormais commune : **une
fenêtre modifiée ne se ferme pas par Échap** (Annuler pour l'abandonner) ; sans modification, Échap
la ferme du premier coup.

- [ ] **D-01 [B]** **Renommer / déplacer…** sur **plusieurs livres** : modifier le **modèle de nom** (bouton « Renommer N fichier(s) » en bleu) puis Échap **1, 2, 3 fois** (lentement puis rapidement) : la fenêtre **reste ouverte**, le modèle modifié est intact ; « Annuler » la ferme, rien n'est renommé. Même chose en modifiant seulement le **dossier cible**. Remettre le modèle exactement comme à l'ouverture : Échap ferme de nouveau. Sans modification : Échap ferme du premier coup ; rouvrir par le menu, 3 fois de suite. Pendant le lot : Échap ne ferme pas ; après le bilan : Échap ferme.
- [ ] **D-02 [M]** **« Ouvrir un fichier… »** : chemin saisi → Échap 1, 2, 3 fois sans effet, « Annuler » ferme ; champ vide → Échap ferme. **📁 Dossiers surveillés** : chemin saisi dans le champ du nouveau dossier → Échap sans effet, « Fermer » ferme ; champ vide → Échap ferme. Chaque fenêtre se rouvre aussitôt, 3 fois de suite.
- [ ] **D-03 [M]** **Fenêtres des listes** : « Modifier la liste » / « Modifier le dossier » et l'**éditeur de liste intelligente** : modifiés → Échap sans effet, non modifiés → Échap ferme ; « Importer une liste de lecture (.cbl) » et « Exporter « … » en .cbl » : chemin saisi → Échap sans effet (après un import réussi, Échap ferme) ; « Supprimer la liste », « Convertir en liste de lecture », « Déplacer « … » dans », « Ajouter … à une liste » : Échap ferme **sans rien faire**. **Renommage dans l'arbre** : Échap annule (ancien nom gardé). Après chaque Échap, le clavier revient à l'arbre ou à la vue (pas de fenêtre fantôme).

## 3. Arbre des listes (copie liée)

Copie liée, **ComicRack fermé une fois** juste avant (comptes de ComicRack à jour), ComicRack
ouvert à côté pour comparer.

- [ ] **T-01 [B]** **Structure identique à ComicRack** : déplier tous les dossiers (▸) et comparer avec le panneau de ComicRack : **47 listes**, mêmes noms, **même ordre**, mêmes dossiers et même imbrication, même type (icônes : ▦ Bibliothèque, 📁 / 📂 dossier, 📄 liste de lecture, 🔍 liste intelligente). Différences : ………
- [ ] **T-02 [B]** **Compteurs** : chaque liste affiche son nombre de livres à droite (aucun « ? ») ; un dossier affiche le nombre de livres de ses sous-listes (réunion). Listes sans nombre : ………
- [ ] **T-03 [M]** **Infobulle** au survol d'une liste : « Liste intelligente « X » » (ou Liste de lecture, Dossier, Bibliothèque), sa description si elle en a une, « **ComicRust : n livres** », « **ComicRack (dernier enregistrement) : m livres** » (pas de ligne ComicRack sur un dossier). Une liste dont ComicRust ne connaît pas le contenu : « ComicRust : pas encore calculable (raison) ».
- [ ] **T-04 [B]** **Bilan des listes** : bouton « ＋ » (ou clic droit dans le vide de l'arbre) → « 📋 Copier le bilan des listes » : message « Bilan des listes copié (ComicRust / ComicRack, « ≠ » : différent). » ; coller dans la note : en-tête « Bilan des listes ComicRust (Tauri) v0.6.0 — <chemin de ComicDb.xml> », puis **47 lignes** « … — ComicRust n / ComicRack m », les différentes marquées « ≠ ». **Attendu : 46/47 identiques, comme la version egui** (écart admis : « Lecture en cours » ou une liste « récemment … », si tu as lu dans ComicRust). Listes identiques : ……/47 ; lignes « ≠ » : ………
- [ ] **T-05 [M]** Listes témoins (même nombre que ComicRack dans le bilan) : COSMIC COLLECTION (Chemin), BD_X, Séries complétes, Séries complétes (A scrapper), Séries à définir (Série complète = inconnue), **Doublons**, **Fichiers à mettre à jour**, le dossier « Complet ».
- [ ] **T-06 [m]** Dossiers : clic sur ▸ / ▾ et double-clic sur le dossier le déplient / replient ; les dossiers dépliés restent dépliés en changeant de liste.

## 4. Clavier de l'arbre

Copie liée (lecture seule), puis copie autonome pour F2 / Suppr.

- [ ] **K-01 [B]** Cliquer une liste (l'arbre prend le focus) puis **↑ ↓** : la liste suivante / précédente est choisie et **la vue affiche ses livres** ; **Début / Fin** : première / dernière ligne visible ; **Pg préc. / Pg suiv.** : une page de l'arbre ; l'arbre défile pour garder la liste choisie visible.
- [ ] **K-02 [B]** Sur un dossier : **→** le déplie (→ encore : descend sur sa première sous-liste), **←** le replie ; sur une sous-liste, **←** remonte au dossier parent ; **Entrée** déplie / replie un dossier. Double-clic sur une liste intelligente : ouvre l'éditeur (section 8).
- [ ] **K-03 [M]** **F2** : renommage de la liste choisie (copie autonome) ; **Suppr** : fenêtre « Supprimer la liste » ; **Maj+F10** ou touche **Menu** : menu contextuel de la liste choisie. En copie liée en lecture seule, F2 et Suppr affichent sous le panneau la raison de la lecture seule (rien ne s'ouvre).
- [ ] **K-04 [m]** Pendant un renommage, les flèches déplacent le curseur dans le nom (la liste choisie ne change pas) ; dans le champ de recherche, les flèches ne changent pas de liste.

## 5. Vue d'une liste

Copie liée.

- [ ] **V-01 [B]** Clic sur la liste de lecture **« Thanos - Guide complet Crushing Krisis »** : titre « 📄 Thanos - Guide complet Crushing Krisis » en haut de la vue, tri **« # Ordre de la liste »** croissant ; vue ☰ Tableau : colonne **« # »** (1, 2, 3…) ; même nombre de livres que le compteur et **même ordre que dans ComicRack** (comparer les 10 premiers et les 5 derniers).
- [ ] **V-02 [B]** Clic sur une liste intelligente (COSMIC COLLECTION (Chemin)), puis sur un dossier : la vue montre leurs livres, au nombre du compteur ; les livres de la liste intelligente correspondent à ses critères (survol de quelques fiches).
- [ ] **V-03 [B]** **Retour à la bibliothèque** : clic sur « ▦ Bibliothèque » (en haut du panneau) : tous les livres (5 325), plus de titre de liste, **le tri de la bibliothèque est retrouvé** (pas « # Ordre de la liste », qui n'est plus proposé dans la liste des tris). Revenir dans la liste Thanos : de nouveau « # Ordre de la liste ».
- [ ] **V-04 [M]** Dans la liste Thanos, trier par « Série », puis revenir à « ▦ Bibliothèque » : le tri de la bibliothèque n'a pas changé. Recherche « quest » dans une liste : seuls ses livres trouvés ; Échap vide la recherche.
- [ ] **V-05 [m]** Double-clic sur un livre d'une liste : il s'ouvre dans le lecteur ; Échap revient à la **même liste**, même livre sélectionné.

## 6. Listes de lecture : ajouter, retirer, réordonner

Copie autonome. Créer d'abord une liste de lecture « Test » (« ＋ » → « 📄 Nouvelle liste de
lecture », taper « Test », Entrée).

- [ ] **L-01 [B]** **Glisser-déposer** : dans « ▦ Bibliothèque », choisir 3 livres (Ctrl+clic) et les glisser sur « Test » dans l'arbre : la liste survolée est mise en évidence, le badge dit « 3 livres — Ajouter à « Test » » ; au lâcher, le compteur passe à 3, message « 3 livres ajoutés à « Test » ». Reglisser les mêmes : badge de refus « Les livres sont déjà dans cette liste ». Même chose depuis la vue ☰ Tableau.
- [ ] **L-02 [B]** **Par le menu** : clic droit sur 2 autres livres → « ➕ Ajouter les 2 livres à une liste… » → fenêtre « Ajouter 2 livres à une liste », « Liste de lecture : » « Test », « Ajouter » : compteur à 5. Avec un livre déjà dedans : « … (1 déjà dans la liste) ». Choix « Nouvelle liste de lecture… » + « Nom de la nouvelle liste : » « Test 2 » : la liste est créée avec ces livres.
- [ ] **L-03 [M]** **Dépôts refusés** : glisser des livres au-dessus d'une liste intelligente (« Liste intelligente : son contenu dépend de ses critères »), d'un dossier (« Dossier : dépose les livres sur une liste de lecture »), de « Bibliothèque » (« La bibliothèque contient déjà tous les livres ») : curseur « interdit », raison dans le badge, rien ne change au lâcher.
- [ ] **L-04 [B]** **Retrait** : dans « Test », choisir un livre, **Suppr** : il quitte la liste, message « 1 livre retiré de « Test » » ; clic droit → « ➖ Retirer le livre de la liste » (bulle « Les fichiers ne sont pas supprimés ») sur un autre. **Le fichier n'est pas supprimé** : le livre est toujours dans « ▦ Bibliothèque » et son fichier toujours sur le disque. Suppr dans « ▦ Bibliothèque » ne retire ni ne supprime rien.
- [ ] **L-05 [B]** **Monter / descendre** : dans « Test » (tri « # » croissant), clic droit → « ⬆ Monter dans la liste » / « ⬇ Descendre dans la liste » : le livre change de place, colonne « # » à jour ; plusieurs livres choisis bougent ensemble.
- [ ] **L-06 [B]** **Réordonner au glisser** : vue des couvertures, glisser un livre entre deux autres : un trait montre la place, badge « 1 livre — Déplacer ici » ; au lâcher, il est à cette place. Vue ☰ Tableau : 3 livres non contigus (Ctrl+clic), glisser depuis l'un d'eux entre deux lignes : les 3 sont là, **groupés dans leur ordre**. Un appui avec Ctrl ou Maj sélectionne sans démarrer de glisser.
- [ ] **L-07 [M]** Trier « Test » par « Série » (ou « # » décroissant) : « Monter / Descendre » grisés et glisser refusé avec « Trie la vue dans l'ordre de la liste (« # » croissant) pour réordonner » ; rien ne change.
- [ ] **L-08 [B]** **Série entière** (egui v0.10.2) : ajouter à « Test » quelques livres de 3 séries, groupement « Par série », tri « # » croissant : les groupes se suivent dans l'ordre de leur **premier livre dans la liste** (pas par ordre alphabétique) ; clic sur l'**en-tête** d'une série = tous ses livres ; glisser cet en-tête entre deux autres séries : trait entre les groupes, au lâcher **toute la série** est à sa nouvelle place, dans son ordre. Ctrl+clic sur deux en-têtes puis glisser : elles bougent ensemble. Clic droit → « ⬆ Monter la série » / « ⬇ Descendre la série ».
- [ ] **L-09 [B]** Fermer et relancer la copie autonome : contenu et ordre de « Test » gardés ; aucun livre perdu (même compteur).

## 7. Gestion de l'arbre

Copie autonome.

- [ ] **G-01 [B]** **Créer** : « ＋ » (ou clic droit sur une liste) → « 📄 Nouvelle liste de lecture » puis « 📁 Nouveau dossier » : créés **juste après la liste choisie**, sélectionnés, **aussitôt en renommage** (« Nouvelle liste » / « Nouveau dossier » tout sélectionné) ; clic droit sur un dossier → les mêmes commandes créent **dedans, à la fin** (dossier déplié). Échap pendant ce renommage garde le nom par défaut.
- [ ] **G-02 [B]** **Renommer** : clic droit → « Renommer » (F2) : champ sur la ligne, Entrée valide, Échap annule, cliquer ailleurs valide ; un nom vide garde l'ancien nom.
- [ ] **G-03 [B]** **Modifier** : « ✏ Modifier… » sur une liste de lecture → « Modifier la liste » (Nom, Description) : la description apparaît dans l'infobulle. Sur un dossier → « Modifier le dossier », « Livres du dossier : » « réunion des sous-listes (au moins une) » / « intersection des sous-listes (toutes) » / « aucun livre (simple rangement) » : le compteur du dossier suit.
- [ ] **G-04 [M]** **Dupliquer** : « ⎘ Dupliquer » → « Copie de X » juste après, sélectionnée, même contenu ; sur un dossier, ses sous-listes sont copiées aussi. Sur « Bibliothèque » : refus lisible.
- [ ] **G-05 [B]** **Supprimer avec dépendances** : créer une liste intelligente dont la liste de départ est « Test 2 », puis « 🗑 Supprimer… » (Suppr) sur « Test 2 » : fenêtre « Supprimer la liste », « Supprimer « Test 2 » ? », « Ces listes intelligentes partent de cette liste ; elles partiront de toute la bibliothèque : « … » », « Les livres eux-mêmes ne sont pas supprimés. » ; « Supprimer » : la liste disparaît, l'éditeur de la liste intelligente montre « Toute la bibliothèque ». Sur un dossier : « Ce dossier contient n sous-listes : elles seront supprimées avec lui. » Supprimer la liste affichée : la vue revient à la bibliothèque. Aucun fichier supprimé.
- [ ] **G-06 [M]** **Convertir** : sur une liste intelligente (une copie), « 📄 Convertir en liste de lecture… » → « « X » devient une liste de lecture contenant ses n livres actuels ; ses critères sont abandonnés. » → « Convertir » : icône 📄, même nom, même nombre de livres, réordonnable.
- [ ] **G-07 [B]** **Déplacer** : « Déplacer dans… » → « Déplacer « X » dans » : « (premier niveau) » puis les dossiers, le dossier actuel grisé « (actuel) » ; valider : la liste est dans le dossier choisi (déplié). **Glisser une liste dans l'arbre** : trait au-dessus / au-dessous d'une liste, sur un dossier le milieu = dedans ; dans elle-même ou sa propre branche : trait d'erreur, rien au lâcher. « ⬆ Monter » / « ⬇ Descendre » (grisés en tête / en fin de dossier).
- [ ] **G-08 [M]** Fermer et relancer : toutes les modifications des sections 7 et 8 sont gardées.

## 8. Éditeur de listes intelligentes

Copie autonome, puis copie liée pour E-07 et E-08.

- [ ] **E-01 [B]** « ＋ » → « 🔍 Nouvelle liste intelligente… » : fenêtre « Nouvelle liste intelligente », Nom, Description, « Correspond à [tous ▾] des critères suivants, dans : [Toute la bibliothèque ▾] », un critère « Série contient » ; saisir « Thanos » : « Aperçu : N livres trouvés » (mise à jour en un quart de seconde environ) ; « OK » : la liste est créée, sélectionnée, avec N livres.
- [ ] **E-02 [B]** **Critères et opérateurs** : changer le champ (Série → Année, Note, Date d'ajout, Lu…) : opérateurs adaptés, valeur gardée si la nature est la même ; nombre « 3,5 » accepté et toujours « 3,5 » en rouvrant ; date `AAAA-MM-JJ` ; « dans les derniers … jours » ; case « Aa » (casse) sur le texte. Valeur invalide (Année « abc ») : erreur sous l'aperçu (« Critère … (Année) : « abc » n'est pas un nombre »), **OK et Appliquer grisés** (bulle « Corrige d'abord les erreurs signalées ») ; nom vide : grisés aussi.
- [ ] **E-03 [B]** **Groupes et Non** : « ➕ Groupe » ajoute un groupe avec sa combinaison (tous / au moins un) ; cases **Non** du groupe et d'un critère : l'aperçu change comme attendu (Non inverse) ; ↑ ↓ ✖ déplacent ou suppriment un critère ou un groupe ; « Correspond à au moins un » change l'aperçu.
- [ ] **E-04 [M]** **Liste de départ** : choisir une liste dans « dans : » : l'aperçu ne dépasse pas son nombre de livres ; « pas dans cette liste » : les livres de la bibliothèque qui n'y sont pas. La liste elle-même et celles qui en partent ne sont pas proposées (pas de boucle).
- [ ] **E-05 [M]** **Limite** : « Limiter à » 25 « livres » « sélectionnés » « au hasard » / « par position » / « par série » : aperçu de 25 au plus ; au hasard : « Sélection « au hasard » : le tirage de ComicRust peut différer de celui de ComicRack. » ; Mo / Go acceptés.
- [ ] **E-06 [B]** **OK / Appliquer** : « Appliquer » grisé tant que rien n'est modifié ; après une modification, « Appliquer » enregistre, **garde la fenêtre** (relue) et met à jour compteur et vue ; **Ctrl+Entrée** = OK ; « Annuler » n'enregistre rien.
- [ ] **E-07 [B]** Copie liée, **une de tes 29 listes intelligentes** : « ✏ Modifier les critères… » (ou double-clic) : nom, combinaison, liste de départ, critères, opérateurs, valeurs, groupes, limite **comme dans la fenêtre de ComicRack** (ouvrir la même liste dans ComicRack) ; « n livres retirés à la main dans ComicRack » si c'est le cas. Liste comparée : ………
- [ ] **E-08 [M]** Copie liée **en lecture seule** : l'éditeur s'ouvre en consultation : « 🔒 Consultation seulement : … », champs grisés, seul « Fermer ».

## 9. Écriture de `ComicDb.xml` (copie liée)

**Sauvegardes PR-03 et PR-04 faites.** ComicRack **fermé** pour les écritures ; le relancer
seulement pour vérifier.

- [ ] **W-01 [B]** **Lecture seule par défaut** (« Lecture et écriture » décochée dans ⚙ Réglages) : 🔒 à côté de « Listes » et sous le panneau « Lecture seule : active « Lecture et écriture » dans les réglages de l'édition pour modifier les listes de ComicRack (ComicRack fermé). » ; commandes de modification grisées (même bulle), glisser de livres refusé ; « Exporter en .cbl… » reste possible ; la date de `ComicDb.xml` ne change pas.
- [ ] **W-02 [B]** **Refus si ComicRack est lancé** : activer « Lecture et écriture », **ComicRack lancé** : en 3 s au plus, 🔒 « Ferme ComicRack pour modifier ses listes : il réécrit ComicDb.xml en se fermant (et toutes les 10 minutes) et effacerait les modifications. » ; commandes grisées, rien n'est écrit. Fermer ComicRack : en 3 s au plus, les commandes se dégrisent (sans relancer ComicRust).
- [ ] **W-03 [B]** ComicRack fermé : créer une liste de lecture « Test ComicRust » avec 3 livres (glisser) et la réordonner, créer un dossier, **dupliquer** une liste intelligente et changer une valeur de la **copie** ; ouvrir une autre liste intelligente et faire « OK » **sans rien changer**. À côté de `ComicDb.xml` : **`ComicDb.comicrust-origine-AAAAMMJJ-HHMMSS.xml.bak`** (créée à la première écriture, jamais supprimée) et des `ComicDb.comicrust-AAAAMMJJ-HHMMSS.xml.bak` (10 dernières).
- [ ] **W-04 [B]** **Listes non modifiées identiques** (avant de relancer ComicRack) : copier le `ComicDb.xml` actuel dans `apres\` et le comparer à `avant\` (PR-03) : seules diffèrent les lignes de « Test ComicRust », du dossier et de la copie modifiée ; **la liste ouverte puis validée sans changement est identique**, les autres listes et la partie livres aussi. Nombre de lignes différentes : ………
- [ ] **W-05 [B]** **Relancer ComicRack** : il démarre sans message d'erreur ; « Test ComicRust » (3 livres, dans l'ordre de ComicRust), le dossier et la copie modifiée y sont, avec le même nombre de livres que dans ComicRust ; tes autres listes sont intactes.
- [ ] **W-06 [M]** **Retour** : dans ComicRack, renommer « Test ComicRust » et lui ajouter un livre, fermer ComicRack : ComicRust (resté ouvert) **relit l'arbre tout seul** en quelques secondes (nouveau nom, compteur à 4).
- [ ] **W-07 [M]** Dans ComicRust, ajouter la liste importée (section 10, C-02) puis relancer ComicRack : elle y est, dans l'ordre du fichier `.cbl`.
- [ ] **W-08 [B]** Fin de section : supprimer les listes d'essai (dans ComicRust, ComicRack fermé) ; ComicRack rouvre ta bibliothèque et tes 47 listes intactes, aucune erreur ; au besoin, remettre `avant\ComicDb.xml` et la sauvegarde PR-04.

## 10. Import / export `.cbl`

Copie autonome (puis copie liée en écriture pour W-07).

- [ ] **C-01 [B]** « ＋ » → « Importer une liste .cbl… » → fenêtre « Importer une liste de lecture (.cbl) » ; chemin de ta **liste Thanos réelle** → « Analyser » : « **N / M numéros trouvés dans la bibliothèque** (a par identifiant, b par série et numéro), k manquants. » et « Numéros manquants » (« #rang — Série n° x, année ») ; aucune liste créée à ce stade. Les manquants sont bien des numéros que tu ne possèdes pas. « 📋 Copier le bilan » et coller dans la note. Trouvés / total : ……… ; manquants : ………
- [ ] **C-02 [B]** « Nom de la nouvelle liste : » (proposé : le nom du `.cbl`), « Importer » : « ✔ Liste « … » créée avec N livres. » ; la liste est dans l'arbre avec N livres, **dans l'ordre du fichier** (comparer les 10 premiers avec la liste Thanos de ComicRack).
- [ ] **C-03 [m]** Échantillon `Thanos.cbl` : le numéro volontairement absent est le seul manquant.
- [ ] **C-04 [B]** **Exporter** : clic droit sur « Test » → « Exporter en .cbl… » → « Exporter « Test » en .cbl », chemin (`.cbl` ajouté si absent) → « N livres exportés dans … » ; réimporter ce fichier (C-01) : N / N trouvés **par identifiant**, dans le même ordre. Copie liée **en lecture seule** : l'export d'une de tes listes fonctionne aussi ; le fichier s'ouvre dans ComicRack (liste de lecture importée, mêmes livres).
- [ ] **C-05 [M]** Copie liée en lecture seule : « Importer une liste .cbl… » grisé (bulle : raison de la lecture seule).

## 11. Mode autonome

- [ ] **A-01 [B]** Copie autonome : fermer et relancer : toutes les listes des sections 6, 7, 8 et 10 sont là (listes dans la base de ComicRust, `data\comicrust.db`) ; **aucun `ComicDb.xml` n'a été touché** (date du `ComicDb.xml` de ComicRack inchangée) ; aucune sauvegarde `.xml.bak` créée par la copie autonome.
- [ ] **A-02 [M]** Copie autonome avec **ComicRack lancé** : pas de 🔒, les listes restent modifiables (ComicRack n'est pas concerné).

## 12. Non-régression courte (tauri-v0.5.0)

- [ ] **N-01 [B]** Copie liée : recherche « thanos » → **484** livres ; Échap vide la recherche. Nombre : ………
- [ ] **N-02 [B]** **Édition** : Ctrl+I, modifier la Série, OK : grille, tableau et panneau de détails à jour ; Échap dans Infos modifiée ne ferme pas (I-03).
- [ ] **N-03 [B]** **Pages** : onglet « Pages », déplacer une page, OK ; rouvrir : ordre gardé, le lecteur le suit.
- [ ] **N-04 [B]** **Lecteur** : double-clic, Entrée et « Lire » ouvrent le livre ; → ← D V M T fonctionnent ; Échap revient à la vue (bibliothèque ou liste), même livre sélectionné ; reprise à la dernière page lue.
- [ ] **N-05 [B]** **Rien hors de `data\`** : après tous ces tests, aucun dossier `ComicRust` ni `fr.imtheyoyo.comicrust` dans `%APPDATA%` ni `%LOCALAPPDATA%` ; à côté des BD et des `.cbl`, aucun fichier temporaire restant ; à côté de `ComicDb.xml`, seulement les sauvegardes `ComicDb.comicrust-….xml.bak`.

---

## Limites connues (ne pas compter comme anomalies)

- **Pas de type « filtres »** : seules les listes de lecture, listes intelligentes et dossiers se créent (comme la version egui) ; pas de liste « filtre » de ComicRack.
- **Glisser-déposer des livres** : un glisser commence par un appui **sans Ctrl ni Maj** (Ctrl / Maj + appui = sélection) ; **pas de rectangle de sélection** à la souris dans la vue (Ctrl+clic, Maj+clic, Ctrl+A, clic sur un en-tête de groupe).
- **Éditeur** : **pas de copier / coller de critères** (pas non plus dans la version egui) ; un critère inconnu du catalogue est affiché « ⚠ » en lecture seule et recopié tel quel.
- **Dossiers repliés** : l'état déplié / replié des dossiers de ComicRack n'est pas repris (tous repliés au démarrage ; ceux que tu déplies le restent pendant la séance).
- **Import / export `.cbl`** : **nouveaux par rapport à la version egui** (qui lisait les `.cbl` sans créer de liste) ; les numéros manquants ne sont pas ajoutés à la liste créée (seuls les livres trouvés).
- **Comptes complets** (listes intelligentes, doublons, séries complètes) vérifiables **seulement sur ta vraie bibliothèque** ; les tests automatiques utilisent ton `ComicDb.xml` anonymisé et une bibliothèque réduite.
- **Listes de ComicRack** modifiables seulement avec « Lecture et écriture » **et ComicRack fermé** ; ComicRack les relit à son démarrage. La détection de ComicRack se fait toutes les 3 s : une commande lancée dans cet intervalle est de toute façon revérifiée juste avant l'écriture.
- La sélection « au hasard » d'une liste limitée ne peut pas être identique à celle de ComicRack.
- Une liste de lecture garde l'ordre de ses livres ; les comptes de ComicRack (infobulle, bilan) sont ceux de **sa dernière sauvegarde** de `ComicDb.xml`.
- Bouton 📜 Scripts : étape 7 ; connexion MySQL et transfert depuis ComicRack dans la version Tauri : étape 6.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 45 | | |
| Majeurs [M] | 19 | | |
| Mineurs [m] | 4 | | |

Décision et remarques :
