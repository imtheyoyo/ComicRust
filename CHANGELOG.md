# Journal des modifications

Toutes les versions de ComicRust, de la plus récente à la plus ancienne. Chaque section est
publiée telle quelle comme notes de la release GitHub correspondante (et affichée dans
ComicRust par « Nouveautés… » lors d'une mise à jour). Chaque version a sa recette dans
[`recette/`](recette).

## [v0.10.0] — 2026-10-26

Scripts Python, avec Python embarqué dans le zip.

### Nouveautés
- **Scripts Python** dans le dossier `scripts\` à côté de l'exe, avec **Python 3.13 embarqué**
  (dossier `python\` : rien à installer). Le guide est dans `scripts.md`.
  - Un script qui plante ne fait pas tomber ComicRust : Python tourne dans un processus à part.
  - Les scripts ne modifient jamais directement la base ni les fichiers : ils demandent les
    modifications à ComicRust, qui les applique avec ses protections.
- **Menu 📜 Scripts**, et **Scripts ▸** dans le menu contextuel des livres. Une fenêtre suit
  l'exécution : avancement, journal, bouton Arrêter, questions du script.
- **Aperçu avant application** de ce que le script veut faire :
  - pour chaque livre : le champ, la valeur avant et la valeur après ;
  - les renommages (ancien → nouveau chemin) et les écritures dans les fichiers ;
  - une case à cocher par livre ; les conflits sont signalés et ne sont pas appliqués.
- **Automatismes** (au démarrage, à l'ouverture ou à la fermeture d'un livre, sur les nouveaux
  livres d'une analyse) : **désactivés par défaut** (« Gérer les scripts… »), jamais lancés en
  lecture seule.
- **Modification des fiches et des fichiers** par les scripts :
  - 57 champs de ComicRack, valeurs personnalisées comprises, contrôlés selon leur type ;
  - renommage et déplacement des fichiers avec mise à jour de la fiche. Pas d'écrasement, nom
    vérifié pour Windows ; si la fiche ne peut pas être mise à jour, le fichier est remis en
    place ;
  - en mode ComicRack, il faut l'écriture activée ; seuls les éléments modifiés sont réécrits.
- **Scripts fournis** dans `scripts\exemples\` :
  - **Nettoyage des fiches** : espaces, numéros, champs complétés depuis le nom du fichier ;
  - **Ranger les fichiers** : modèle de nom et sous-dossiers série ou éditeur ;
  - **Comic Vine** : infos d'un livre en ligne, avec ta clé d'API, gardée chiffrée et jamais
    affichée ;
  - un automatisme pour les nouveaux livres, désactivé.
- **Mises à jour** : un script d'exemple que tu as modifié n'est jamais écrasé. La nouvelle
  version est posée à côté (`.py.nouveau`).

### Corrections (relecture avant publication)
- Fermer ComicRust pendant « Appliquer » : il attend la fin du livre en cours (un fichier ne
  peut plus être séparé de sa fiche).
- Chemins des renommages normalisés (pas de `..`, même écriture de la racine que le livre :
  pas de doublon dans ComicRack).
- Rien n'est écrit hors du dossier de ComicRust (module Python dans `data\python\`).
- Aperçu préparé en arrière-plan (pas de blocage sur un NAS), processus Python toujours arrêtés.

### Recette
[recette/v0.10.0.md](recette/v0.10.0.md).

## [v0.9.1] — 2026-10-25

Les livres se sélectionnent et se déplacent à la souris, comme dans ComicRack et l'explorateur
Windows (demande de la recette de la v0.9.0).

### Nouveautés
- **Sélection à la souris** :
  - vue des couvertures : tirer un **rectangle de sélection** depuis un espace vide (Ctrl :
    ajouté à la sélection) ; la vue défile toute seule quand le pointeur approche du bord ;
  - vue tableau : cliquer sur une ligne **non sélectionnée** et glisser vers le haut ou le bas
    sélectionne les lignes balayées (Ctrl : ajoutées à la sélection) ;
  - **Ctrl+A** : tous les livres affichés ; **Échap** ou **clic dans le vide** : plus de
    sélection.
- **Réordonner une liste de lecture en glissant** (couvertures et tableau) : glisser un livre,
  ou la sélection, et le déposer entre deux livres. Un trait montre où il ira, la vue défile
  près des bords ; la liste est enregistrée une seule fois, au lâcher.
  - Vue pas triée dans l'ordre de la liste : curseur « interdit » et message « Trier dans
    l'ordre de la liste pour réordonner ».
  - Dans le tableau, on glisse depuis une ligne **déjà sélectionnée** (depuis une autre ligne,
    c'est une sélection).
- **Ajouter des livres à une liste par glisser-déposer** : déposer la sélection sur une liste
  de lecture du panneau des listes. La liste survolée est mise en évidence ; un badge « N
  livres » suit le pointeur et dit ce que fera le dépôt. Listes intelligentes, dossiers,
  Bibliothèque, liste affichée et listes en lecture seule : curseur « interdit », avec la
  raison.
- Un glisser ne démarre qu'après un petit déplacement (un clic reste un clic) ; double-clic
  pour ouvrir et clic droit inchangés ; Échap pendant un glisser l'annule. **Aucun glisser ne
  déplace ni ne supprime de fichier** : seules les listes changent.

### Recette
[recette/v0.9.1.md](recette/v0.9.1.md) — GO (19/19 OK).

## [v0.9.0] — 2026-10-25

Gestion complète des listes et écriture dans la base de ComicRack.

### Nouveautés
- **Gestion des listes** (panneau des listes) :
  - bouton « ＋ » : nouvelle liste de lecture, liste intelligente ou dossier ;
  - menu contextuel : Modifier, Renommer (F2), Dupliquer, Supprimer (avec avertissement des
    listes intelligentes qui en partent), Convertir en liste de lecture, Monter / Descendre,
    Déplacer dans ▸ ;
  - glisser-déposer dans l'arbre.
- **Livres et listes de lecture** : sélection multiple (Ctrl / Maj + clic), « Ajouter à la
  liste ▸ » (ou nouvelle liste avec les livres choisis) ; dans une liste de lecture : « Retirer
  de la liste » (Suppr : ne supprime jamais de fichier), Monter / Descendre.
- **Éditeur de listes intelligentes**, comme celui de ComicRack :
  - critères (tous / au moins un), Non, groupes imbriqués ;
  - liste de départ, limite ;
  - aperçu en direct du nombre de livres.

  Les critères que tu ne touches pas sont gardés à l'identique.
- **Où sont enregistrées les listes** :
  - en autonome : dans la base de ComicRust ;
  - avec ComicRack : dans `ComicDb.xml`, **seulement quand ComicRack est fermé** et si
    l'écriture est activée. Une sauvegarde est faite avant chaque modification : la première,
    `ComicDb.comicrust-origine-….xml.bak`, est gardée pour toujours, plus les 10 dernières.
- **Écriture dans la base MySQL de ComicRack** (assistant → « Lecture et écriture » ; lecture
  seule par défaut) :
  - ce qui est enregistré : la lecture (page, date, nombre d'ouvertures), les fichiers convertis
    en CBZ, les infos écrites dans les fichiers ;
  - ComicRack lancé voit les modifications en quelques secondes ;
  - seuls les éléments concernés de la fiche sont modifiés ;
  - les lectures faites dans ComicRust en lecture seule sont proposées au report, sans jamais
    faire reculer ComicRack.

### Corrections (relecture avant publication)
- Caractères interdits en XML retirés des noms et descriptions (ComicRack aurait refusé
  `ComicDb.xml`).
- Nombres décimaux écrits avec le séparateur de Windows (« 4,5 »), comme ComicRack les lit.
- Mode lecture seule : les listes de ComicRack ne sont plus modifiables.
- ComicRack lancé pendant l'enregistrement : vérifié à nouveau juste avant d'écrire.
- Écritures en attente gardées à la fermeture de ComicRust et refaites au lancement suivant.

### Recette
[recette/v0.9.0.md](recette/v0.9.0.md) — GO (21/21 OK).

## [v0.8.0] — 2026-10-24

Se passer de ComicRack : sa bibliothèque peut être transférée dans la base de ComicRust.

### Nouveautés
- **⚙ → « Transférer la bibliothèque ComicRack… »** : copie dans la base de ComicRust (fichier
  local portable `data\comicrust.db` ou base MySQL configurée dans l'assistant) :
  - les **livres** avec toutes leurs infos : lecture (page, date, nombre d'ouvertures), notes,
    champs « Book… », signets et attributs de page, valeurs personnalisées ;
  - les **listes** : dossiers, listes de lecture, listes intelligentes, avec leurs
    identifiants d'origine ;
  - les **dossiers** de BD surveillés par ComicRack.
  - Source : la base MySQL de ComicRack (avec `ComicDb.xml` pour les listes et les dossiers) ou
    un `ComicDb.xml` seul.
  - Ensuite, ComicRust passe en autonome (case cochée par défaut).
- **ComicRack n'est jamais modifié** : sa base et `ComicDb.xml` sont seulement lus.
- **Livres déjà ajoutés par l'analyse des dossiers** : remplacés par leur fiche ComicRack (même
  identifiant), la lecture la plus récente est gardée.
- **Transfert relancé** : pas de doublon de livres ni de listes. La lecture faite dans ComicRust
  et les fichiers convertis ou déplacés dans ComicRust sont gardés.
- **Même partage sous deux noms** (`Z:\…` et `\\SERVEURNAS\…`) : le dossier n'est pas ajouté
  deux fois, les chemins des livres sont ramenés sous le dossier surveillé ; les livres hors de
  tout dossier surveillé sont signalés.
- **Mode autonome : les listes s'affichent** (relues dans la base de ComicRust quand elles
  changent) et les listes intelligentes sont calculées.

### Corrections (relecture avant publication)
- Transfert interrompu : ComicRust ne bascule plus sur une bibliothèque incomplète.
- Pas de transfert pendant une analyse des dossiers, ni d'analyse pendant un transfert
  (doublons évités) ; la fenêtre ne peut plus lancer deux transferts à la fois.
- `ComicDb.xml` illisible avec MySQL : les livres sont transférés, un avertissement le signale.
- Message clair quand un `ComicDb.xml` ne contient aucun livre (bibliothèque MySQL).

### Recette
[recette/v0.8.0.md](recette/v0.8.0.md) — GO (17/17 OK).

## [v0.7.3] — 2026-10-24

Correctif de sécurité signalé pendant la recette de la v0.7.2.

### Corrections
- **Mot de passe MySQL plus jamais en clair** dans `data\comicrust.ron`. Cela concerne celui
  saisi dans l'assistant (base de ComicRack « Saisir les paramètres », ou base autonome
  MySQL).
  - Il est chiffré avec la protection de données de Windows (DPAPI), liée à ton compte
    Windows : seul ce compte, sur ce PC, peut le relire.
  - ComicRust copié sur un autre PC ou ouvert par un autre compte garde tous ses réglages,
    sauf ce mot de passe, à saisir à nouveau dans l'assistant (le message d'erreur le dit).
  - Les réglages existants sont réécrits chiffrés au premier enregistrement.
  - Rappel : avec « Paramètres lus dans ComicRack.ini », le mot de passe n'est jamais recopié
    dans ComicRust.

### Recette
[recette/v0.7.3.md](recette/v0.7.3.md) — GO (8/8 OK).

## [v0.7.2] — 2026-10-24

Correctif de la v0.7.1 (recette NO-GO, A-03).

### Corrections
- **Changement de bibliothèque** (assistant) vers une bibliothèque plus petite, par exemple une
  base MySQL neuve : ComicRust se fermait. La présentation calculée pour l'ancienne
  bibliothèque était relue dans la nouvelle. Elle est maintenant oubliée à chaque changement
  (A-03, cause trouvée grâce au rapport de plantage de la v0.7.1).

### Recette
[recette/v0.7.2.md](recette/v0.7.2.md) — GO (7/7 OK).

## [v0.7.1] — 2026-10-24

Correctif de la v0.7.0 (recette : 21/22, A-03).

### Corrections
- **Assistant, ComicRust autonome avec MySQL** : ComicRust se fermait au clic sur
  « Terminer » sans garder la configuration (A-03, non reproduit dans l'atelier).
  - Piles agrandies pour tous les fils (16 Mio) et pour le fil principal (8 Mio) : un
    débordement de pile fermait le programme sans message.
  - Les réglages sont enregistrés dès la fin de l'assistant.
  - Une erreur inattendue pendant la vérification s'affiche dans l'assistant.
- **Rapport de plantage** : si ComicRust se ferme de façon inattendue, le détail est écrit
  dans `data\plantage.txt` et montré au lancement suivant (fichier gardé sous
  `plantage-<date>.txt`, à joindre au rapport de recette).

### Recette
[recette/v0.7.1.md](recette/v0.7.1.md) — NO-GO (A-03 : plantage localisé par le nouveau rapport, corrigé en v0.7.2).

## [v0.7.0] — 2026-10-24

Première étape vers un ComicRust qui peut se passer de ComicRack : assistant de premier
démarrage et mode autonome.

### Nouveautés
- **Assistant de premier démarrage** (et menu ⚙ → « Assistant de configuration… ») :
  - **ComicRust autonome** : sa propre base, en fichier local portable (`data\comicrust.db`,
    SQLite) ou sur un serveur MySQL / MariaDB (tables `comicrust_…`, qui peuvent partager la
    base de ComicRack sans la toucher), et un ou plusieurs **dossiers de BD** ;
  - **avec ComicRack** : `ComicRack.ini` cherché à son emplacement habituel, `ComicDb.xml` et
    `Config.xml` retrouvés d'après lui ; bibliothèque lue dans la base **MySQL** de ComicRack
    ou dans son **`ComicDb.xml`** (ComicRack sans MySQL) ; lecture seule (l'écriture viendra
    en v0.9.0).
  - Une version précédente déjà reliée à la base MySQL de ComicRack garde ses réglages sans
    passer par l'assistant.
- **Analyse des dossiers de BD** (mode autonome, au démarrage et bouton « 🔍 Analyser les
  dossiers ») : nouveaux fichiers ajoutés avec les infos de leur `ComicInfo.xml`, fichiers
  modifiés relus, fichiers déplacés suivis, fichiers disparus marqués « manquants » ; bilan
  avec les fichiers illisibles. Les ajouts sont enregistrés au fil de l'analyse.
  - Un dossier vide (partage du NAS non monté) ou une disparition massive ne marque aucun
    livre manquant ; la corbeille du NAS (`.recycle`, `#recycle`, `@eaDir`, `$RECYCLE.BIN`…)
    et `_originaux` sont ignorés.
- **Lecture enregistrée dans la base de ComicRust** (mode autonome) : page, dernière page lue,
  date et nombre d'ouvertures, à la fermeture du livre ou de ComicRust.
- Bibliothèque `ComicDb.xml` relue automatiquement quand ComicRack l'enregistre.
- Écrire les infos dans les CBZ et convertir en CBZ fonctionnent avec les trois bibliothèques ;
  en mode autonome, la fiche du livre est mise à jour (sans toucher à sa lecture).

### Recette
[recette/v0.7.0.md](recette/v0.7.0.md) — GO (21/22 OK ; A-03 → v0.7.1).

## [v0.6.0] — 2026-10-22

Piste « Fichiers BD » : ComicRust peut modifier des fichiers BD ; la base ComicRack reste en
lecture seule.

### Nouveautés
- **Écrire les infos dans le fichier** (menu « 🛠 Fichiers » de la bibliothèque, ou clic droit
  sur une couverture ; livre sélectionné ou tous les livres affichés) : la fiche du livre dans
  la base ComicRack (série, numéro, auteurs, résumé, pages…) est écrite dans le
  `ComicInfo.xml` du CBZ, comme « Mettre à jour les fichiers » de ComicRack : mêmes éléments,
  même ordre, éléments ajoutés par d'autres logiciels gardés. Les pages sont recopiées sans
  recompression dans un fichier temporaire, vérifié avant de remplacer l'original ; un fichier
  déjà à jour n'est pas touché ; un fichier que ComicRack est en train d'enregistrer est
  laissé. Un livre écrit sort de « Fichiers à mettre à jour » dans ComicRust, tant que sa
  fiche ne change pas dans ComicRack.
- **Convertir en CBZ** les CBR, CB7 et CBT : nouveau `.cbz` à côté de l'original (mêmes pages,
  `ComicInfo.xml` de la base) ; l'original est gardé ou rangé dans le sous-dossier
  `_originaux`. La base ComicRack indiquant toujours l'ancien fichier, ComicRust ouvre le CBZ
  à sa place quand l'original n'est plus là.
- Fenêtre d'avancement (bouton « ⏹ Arrêter ») et bilan copiable avec les erreurs.
- **Pages HEIF/HEIC et AVIF** dans les archives et les dossiers d'images : AVIF décodé en pur
  Rust ; HEIC par le décodeur de Windows (extensions HEIF et HEVC du Microsoft Store), à défaut
  par un décodeur pur Rust.
- **WebComic : scrapers et composition**, comme ComicRack : scraper d'index (`!`) et de
  navigation (`?`), motifs `<Part>` avec leurs options (`Cut`, `Reverse`, `AddOwn`, `Sort`,
  `MaximumMatches`), `<base href>` ; plusieurs images par page (lignes × colonnes ou taille de
  page fixe, bordure, fond, droite à gauche). Exemples dans les échantillons de recette.
- `comicrust-cli convert <livre.cbr>` : conversion en ligne de commande.

### Corrections
- WebComic composé de droite à gauche avec une bordure : images décalées de deux bordures
  dans ComicRack ; centrées dans ComicRust. Page de taille fixe : une image qui ne tient plus
  en hauteur passe à la page suivante au lieu de déborder.

### Recette
[recette/v0.6.0.md](recette/v0.6.0.md) — GO (28/28 OK).

## [v0.5.2] — 2026-10-22

Améliorations et correctifs demandés après la recette v0.5.1.

### Nouveautés
- **Chaque liste garde sa présentation** : tri, sens, regroupement et « Empiler par série »
  sont mémorisés pour chaque liste et pour toute la bibliothèque. Une liste ouverte pour la
  première fois reprend la présentation de la bibliothèque (une liste de lecture : son ordre,
  sans groupes).
- **Piles : double-clic** (ou Entrée) pour ouvrir une pile ; un clic la sélectionne.
- « 📋 Copier les livres » au-dessus d'une liste : la liste des livres affichés (série,
  numéro, volume, année, fichier), pour comparer avec ComicRack.

### Corrections
- **Lecture faite dans ComicRust** : la barre orange, la part lue des piles, la colonne « Lu »
  et la fiche (« Lu dans ComicRust : x % (page n) ») en tiennent compte ; le plus avancé entre
  ComicRack et ComicRust est affiché. La base ComicRack n'est toujours pas modifiée.
- **Valeurs proposées d'après le nom du fichier** : quand la fiche ne donne pas la série, le
  numéro, le volume, l'année ou le format, ComicRust les déduit du nom du fichier avec
  l'algorithme de ComicRack (`ComicNameInfo`), comme ComicRack pour ses doublons, ses
  statistiques de série et ses listes intelligentes (recette v0.5.1 : 166 doublons contre 154).

### Recette
[recette/v0.5.2.md](recette/v0.5.2.md) — GO (18/18 OK ; doublons 154 / 154 comme ComicRack).

## [v0.5.1] — 2026-10-22

Améliorations demandées après la recette v0.5.0 (GO), avant la v0.6.0.

### Nouveautés
- **Bouton « ✖ Fermer »** dans le lecteur (et Ctrl+W) : ferme le livre en mémorisant sa page
  et revient à la bibliothèque.
- **« 🗂 Empiler par série »** (vue en couvertures, aussi dans les listes) : une pile par série,
  avec le nombre de livres et la part déjà lue ; un clic ouvre la pile, « ◀ Toutes les piles »
  ou Échap y revient. Compatible avec le regroupement (éditeur, scénariste, année…).

### Corrections
- « Doublons » : regroupement sensible à la casse (série et chemin), comme ComicRack
  (recette v0.5.0 : 166 doublons trouvés contre 154 pour ComicRack).

### Recette
[recette/v0.5.1.md](recette/v0.5.1.md) — GO (16/18, bloquants OK) ; D-01 et E-06 → v0.5.2.

## [v0.5.0] — 2026-10-22

### Nouveautés
- **Listes intelligentes calculées par ComicRust** : ComicRack ne garde pas leur contenu quand
  la bibliothèque est dans MySQL ; ComicRust lit leurs critères dans `ComicDb.xml` et les
  applique lui-même, avec les règles de ComicRack : critères texte (est, contient, contient
  l'un de / tous, commence, finit, liste contient, expression régulière), nombres, dates
  (« dans les N derniers jours »), oui/non, groupes « tous / au moins un », critères inversés,
  liste de départ, limites, **série entièrement complète**, **doublons**, **fichiers à mettre
  à jour**. Les 29 listes intelligentes de ta bibliothèque sont toutes prises en charge.
- Bulle d'aide de chaque liste : nombre de livres selon ComicRust et selon ComicRack ;
  « 📋 Copier le bilan des listes » compare les deux pour toutes les listes.
- **Reprise de lecture** : quand ComicRack et ComicRust n'ont pas retenu la même page, une
  question propose les deux (ou le début), avec « Toujours faire ce choix » ; réglable dans le
  menu ⚙ (Demander / Page de ComicRack / Page de ComicRust).
- **Thème clair ou sombre** (ou celui de Windows), dans le menu ⚙.

### Corrections
- « 📖 Lire (page N) » dans la fiche ouvre bien à la page N enregistrée par ComicRack (la page
  mémorisée par ComicRust passait avant).
- Les listes sont calculées dans un fil séparé : l'interface ne se fige pas quand ComicRack
  modifie la base.

### Recette
[recette/v0.5.0.md](recette/v0.5.0.md) — GO (27/27 OK).

## [v0.4.1] — 2026-10-21

Correctif de la v0.4.0 (recette NO-GO).

### Corrections
- **Livre en cours d'enregistrement par ComicRack** : quand ComicRack écrit ses informations
  dans un CBZ, il renomme brièvement le fichier en `….cbz.tmp`. ComicRust attend maintenant
  la fin de l'écriture (jusqu'à 20 s, message « ComicRack enregistre ce fichier, patience… »)
  au lieu d'afficher « fichier introuvable ». Les couvertures attendent de la même façon.
- **Suivi de la base plus sûr** : en plus des compteurs de ComicRack (toutes les 5 s), la liste
  complète des livres est comparée chaque minute ; tout livre ajouté, modifié ou retiré est
  rattrapé.
- **Fiche du livre** : libellés identiques à la fenêtre « Informations » de ComicRack
  (« Éditeur » = Publisher, « Collection » = Editor…), rubriques Détails, Artistes, Plus,
  Lecture, Fichier ; Année, Mois, Éditeur et Série complète toujours affichés.
- Colonne « Scénariste » du tableau (au lieu de « Scénario »).

### Recette
[recette/v0.4.1.md](recette/v0.4.1.md) — GO (13/14 OK, 0 échec) ; corrige R-02, R-03, D-02 de la v0.4.0.
Les listes intelligentes (L-04) sont prévues en v0.5.0.

## [v0.4.0] — 2026-10-21

### Nouveautés
- **Suivi automatique des modifications faites dans ComicRack** : livre ajouté, modifié,
  supprimé ou lu dans ComicRack apparaît en quelques secondes, sans recharger (même méthode que
  ComicRack : compteurs de la table `changes`). Désactivable dans le menu ⚙.
- **Listes de ComicRack** (panneau 📂 Listes) : dossiers, listes de lecture et listes
  intelligentes, lues dans `ComicDb.xml` (trouvé d'après `ComicRack.ini`, ou choisi à la main).
  Une liste de lecture s'affiche dans son ordre, numérotée.
- **Panneau de détails** : fiche complète du livre sélectionné, avec « Lire (page N) » et
  « Depuis le début ».
- **Tri et regroupement au choix** (série, titre, parution, ajout, dernière lecture, note,
  pages, avancement, taille, chemin, ordre de la liste ; groupes par série, éditeur,
  scénariste, année, arc, genre, format, état de lecture), croissant ou décroissant.
- **Vue tableau** : une ligne par livre, tri d'un clic sur les titres de colonnes.
- **Couverture choisie dans ComicRack** (page « Couverture ») au lieu de la première page.
- Un clic sélectionne un livre ; double-clic ou Entrée l'ouvre.

### Recette
[recette/v0.4.0.md](recette/v0.4.0.md) — NO-GO (R-02), corrigé en v0.4.1.

## [v0.3.0] — 2026-10-21

### Nouveautés
- **Base MySQL de ComicRack, en lecture seule** : paramètres lus dans `ComicRack.ini`
  (le mot de passe n'est jamais recopié) ou saisis ; « Tester la connexion ».
- **Vue Bibliothèque** : couvertures (cache `data\couvertures`), recherche, regroupement par
  série, barre de lecture ; un clic ouvre le livre à la page où ComicRack l'avait laissé.
- **Listes `.cbl` résolues par l'identifiant** des livres dans la base, puis par la recherche
  dans un dossier.
- **Mise à jour intégrée** : nouvelles versions cherchées sur GitHub au démarrage, paquet
  vérifié (SHA-256) puis installé, redémarrage en un clic ; le dossier `data` n'est jamais
  touché.
- **Version exclusivement portable** : tout est dans `data\` à côté de l'exe ; les réglages de
  la version précédente sont repris.
- Code de l'application découpé en modules.

### Recette
[recette/v0.3.0.md](recette/v0.3.0.md) — GO (44/46 OK, 0 échec).

## [v0.2.1] — 2026-10-21

Correctif de la v0.2.0 (recette NO-GO).

### Corrections
- **Listes `.cbl` générées par ComicRack** : les livres sont retrouvés d'après leur
  `ComicInfo.xml` (série, numéro, volume), puis d'après le nom du fichier (numéro après `#`) ;
  la v0.2.0 prenait le dernier nombre du nom et ne trouvait rien.
- Cache de l'index des BD (`index-bd.json`) : les recherches suivantes sont quasi immédiates.
- Barre d'avancement pendant la recherche, bilan et « Copier le diagnostic ».

### Recette
[recette/v0.2.1.md](recette/v0.2.1.md) — GO (26/26 OK).

## [v0.2.0] — 2026-10-20

### Nouveautés
- **Lecteur** (egui) : page entière, largeur, hauteur, taille réelle, zoom, double page
  (couverture seule), lecture manga, plein écran, vignettes, navigation clavier et souris,
  reprise à la dernière page lue, fichiers récents, glisser-déposer.
- **Formats** : CBZ/ZIP, CBR/RAR (RAR4, RAR5, archives solides), CB7/7z, CBT/TAR, PDF
  (PDFium fourni), DjVu, CBW (web comics), dossiers d'images ; pages JPEG, PNG, GIF, BMP,
  TIFF, WebP, JPEG XL ; détection par signature (un ZIP renommé en `.cbr` s'ouvre).
- **Listes de lecture `.cbl`** : panneau de liste, livre suivant en fin de livre.
- Outil en ligne de commande `comicrust-cli` (info, render, cover, cbl, missing).

### Recette
[recette/v0.2.0.md](recette/v0.2.0.md) — NO-GO (C-06 : listes `.cbl`), corrigé en v0.2.1.

## [v0.1.0] — 2026-10-20

### Nouveautés
- Inventaire REA de ComicRack CE (`docs/INVENTAIRE_REA.md`) : structure, formats, base,
  correspondance .NET → Rust.
- Cœur `comicrust-core` : `ComicInfo.xml`, archives CBZ, listes `.cbl`, `ComicDB.xml`.
- Première version de l'outil en ligne de commande.
- Intégration continue (Windows + Linux) et release Windows 11 x64 automatique.
