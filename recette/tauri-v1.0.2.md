# Recette ComicRust Tauri tauri-v1.0.2

| | |
|---|---|
| Version | tauri-v1.0.2 (version **corrective** issue de la relecture complète du code du 24/11/2026) |
| Paquet | `ComicRust-Tauri-tauri-v1.0.2-windows-x64.zip` |
| SHA-256 | `(à renseigner à la publication)` |
| Contenu | défauts de la relecture (archives piégées, écritures sûres, mot de passe conservé, sélection, focus du lecteur, mise à jour tout ou rien, instance unique, MCP…), décisions de Yoyo du 25/11/2026 (lecture seule des fichiers, sauvegardes dans data\, parité regex / ordre des pages / livre d'une page), optimisations |
| Origine | [recette tauri-v1.0.1](tauri-v1.0.1.md) : **GO** du 24/11/2026 ; relecture complète et décisions de Yoyo |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Installation par la mise à jour intégrée de ta 1.0.1 (copies autonome et liée). Travaille sur une
> **copie** de la bibliothèque et de la base pour les essais d'écriture (jamais `comickrack_demo`).
> Les deux fichiers de test « Test ordre des pages.cbz » et « Test une page.cbz » envoyés le 25/11 servent
> à TP-01 et LU-01. Nouveauté importante : **une seule instance par dossier** (voir IU-01).

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| Tests du backend (≈ 390 application + ≈ 300 cœur), dont : archives piégées (taille déclarée énorme, bombe ZIP, entrée qui ment, CBT de 1 Gio), WebComic (accents, bornes), JPEG XL démesuré, ComicInfo avec types de page ComicRack, écriture CBZ synchronisée sans écrasement, sauvegardes ComicDb.xml dans data\, mot de passe illisible conservé, mise à jour **tout ou rien** (retour arrière complet), avancement.json protégé, serveur MCP (origine `null`, type, hôte), lecture seule des fichiers (D1), regex sans casse, tri des pages par valeur, livre d'une page lu | `cargo test` | ✅ |
| Scripts : Bedetheque (« & », « # », accents, erreur sur un livre, pages gardées en mémoire), moteur (caractères invalides, clés en double, journal) | `cargo test` | ✅ |
| Front : `svelte-check` (0 erreur), `vite build` ; Playwright simulé : sélection suivie par identifiant, focus piégé dans le lecteur, « Sélectionner tout le groupe » (71 s → 0,3 s), menus | local | ✅ |
| Compilation de la fenêtre (feature `app`) et Clippy `-D warnings` sous Linux et Windows | local + CI | ✅ |
| Relecture indépendante de l'ensemble des correctifs (voir « Anomalies ») | | ✅ |

**Non vérifiable ici** : WebView2 (inert, CORS, permissions réduites, instance unique), vraie base MySQL, coupure de courant réelle, DPAPI sur un autre poste, vrai site bedetheque.com. **La mise à jour 1.0.1 → 1.0.2 utilise l'installateur de la 1.0.1** : le nouveau mécanisme « tout ou rien » ne servira qu'à partir de la mise à jour suivante.

## 1. Installation et correctifs

- [ ] **P-01 [B]** Ta 1.0.1 propose la 1.0.2 ; « Installer » puis « Redémarrer » → titre « ComicRust 1.0.2 » et ⚙ Réglages ▸ À propos : **Version 1.0.2** (deux copies). Noter : ……… / ………
- [ ] **CA-01 [B]** Permissions réduites, rien de cassé : **F11** plein écran et Échap, **Ctrl+O** (dialogue Windows), **glisser-déposer** d'un CBZ et d'un `.cbl` sur la fenêtre, export d'un `.cbl` (dialogue « Enregistrer »), couvertures et pages affichées, mise à jour des listes après une analyse.
- [ ] **IU-01 [B]** **Instance unique** : ComicRust ouvert, double-clic sur un CBZ (ou `ComicRust-Tauri.exe livre.cbz`) → le livre s'ouvre **dans la fenêtre existante**, qui passe au premier plan, sans 2ᵉ fenêtre ; les copies **autonome et liée** (dossiers différents) tournent toujours **en même temps**.
- [ ] **LS-01 [B]** **Lecture seule (copie liée, mode par défaut)** : « Écrire les infos dans les fichiers », « Convertir », « Renommer / ranger » sont **grisés avec la raison** ; un script qui modifie les fichiers est refusé ; aucun CBZ n'est modifié (date de modification inchangée).
- [ ] **LS-02 [M]** Ces mêmes actions restent possibles dans la copie **autonome**, et dans la copie liée en mode « Lecture et écriture » (sur une **copie** de la base).
- [ ] **SE-01 [B]** **Sélection après mise à jour de la vue** : tri par « Avancement », sélectionne un livre, « Marquer comme lu » → le panneau Détails et la sélection restent **sur le même livre** (même s'il change de place) ; idem après fermeture du lecteur et fin d'analyse ; Infos du livre (Ctrl+I) : Suivant / Précédent restent cohérents avec la vue.
- [ ] **FO-01 [M]** **Lecteur** : Tab et Maj+Tab ne sortent plus du lecteur (aucun bouton de la bibliothèque ou du bandeau de mise à jour n'est atteint) ; les fenêtres ouvertes depuis le lecteur (infos `I`, aide `F1`) restent utilisables ; après fermeture du lecteur, la bibliothèque répond normalement au clavier.
- [ ] **MP-01 [B]** **Mot de passe MySQL** (copie liée) : après deux redémarrages, la connexion marche sans ressaisie ; dans `data\comicrust-tauri.json` (et sa copie `.json.bak`), le mot de passe n'apparaît **jamais en clair** (forme `dpapi:…`).
- [ ] **MP-02 [m]** (si possible) Lance la copie liée sous un **autre compte Windows** (ou un autre PC), ferme, reviens sur ton compte : la connexion MySQL marche toujours sans ressaisie.
- [ ] **CD-01 [M]** **Sauvegardes de ComicDb.xml** (mode ComicDb.xml avec « Lecture et écriture », ComicRack fermé, sur une **copie**) : modifie une liste → la sauvegarde est dans `data\sauvegardes\comicdb\`, **aucun** `.bak` dans le dossier de ComicRack ; ComicRack relit la liste modifiée.
- [ ] **SL-01 [M]** **Expression régulière sans casse** : liste intelligente « Série correspond à l'expression régulière `cosmic` » → contient les livres de « COSMIC COLLECTION » (comme ComicRack).
- [ ] **TP-01 [M]** **Ordre des pages** : « Test ordre des pages.cbz » s'affiche dans l'ordre **2, 9, 010, 011** (comme ComicRack).
- [ ] **LU-01 [M]** **Livre d'une page** : « Test une page.cbz » → « Marquer comme lu » le rend **Lu / 100 %** ; « non lu » le remet à 0 ; un livre d'une page marqué lu dans ComicRack apparaît lu dans ComicRust.
- [ ] **CB-01 [M]** **Couverture à jour** (copie autonome) : remplace un CBZ par une autre version au même chemin, lance l'analyse → la nouvelle couverture s'affiche.
- [ ] **BQ-01 [M]** **Bedetheque Scraper 2** : une série au nom avec « & » (« Tif & Tondu », « Blake & Mortimer ») est trouvée ; sur un lot de plusieurs livres, un livre introuvable ou en erreur n'arrête pas les suivants ; une coupure réseau n'affiche qu'**une** fenêtre d'erreur.
- [ ] **GR-01 [m]** Clic droit ▸ « Sélectionner tout le groupe » (regroupement par éditeur, grande bibliothèque) : quasi instantané.
- [ ] **MC-01 [m]** Ton débogage MCP (copies autonome et liée) fonctionne comme avant, **sans jeton** ; lancer l'exe MCP sur le **même** dossier qu'une instance déjà ouverte la ramène au premier plan et s'arrête (instance unique : utiliser une copie distincte pour déboguer en parallèle).
- [ ] **PF-02 [m]** Taille du zip et de `ComicRust-Tauri.exe` (plus petits grâce au profil release) ; démarrage ; noter : zip ……… Mo, exe ……… Mo

## 2. Nouveautés de la 1.0.1 (non-régression)

- [ ] **TI-01 [B]** La **version** (« ComicRust 1.0.1 ») figure dans le titre de la fenêtre au démarrage ; livre ouvert dans le lecteur : « <livre> — ComicRust 1.0.1 » ; elle revient à la fermeture du lecteur.
- [ ] **JG-01 [B]** **Jauges** : chaque liste (dossier, liste de lecture, liste intelligente) et la Bibliothèque affichent à droite trois compteurs : **rouge** nouveaux, **jaune** non lus, **vert** total (les compteurs rouge/jaune à 0 sont masqués) ; lis un livre, marque-en un comme lu / non lu, importe un .cbl, lance une analyse : les jauges se mettent à jour.
- [ ] **JG-02 [M]** Compare avec ComicRack sur quelques listes : le **total** est identique ; **à trancher** : « nouveau » = non lu, jamais ouvert et ajouté depuis moins de 7 jours. Note ce que ComicRack affiche (rouge/jaune/vert) pour 2 ou 3 listes : ………
- [ ] **JG-03 [m]** Les jauges sont lisibles dans les 3 thèmes (Moderne clair, sombre, Classique) et avec un thème perso (variables facultatives avec repli) ; Réglages ▸ « Afficher les jauges des listes » décoché : retour au compteur unique ; les lecteurs d'écran annoncent « N nouveaux, N non lus, N livres ».
- [ ] **MS-01 [B]** Clic droit sur un livre : l'entrée **« Scripts ▸ »** ouvre, au **survol** ou au **clic**, un **sous-menu à côté du menu** (à droite, ou à gauche s'il n'y a pas la place) ; on peut le traverser en diagonale sans qu'il se ferme ; un script choisi s'applique aux livres sélectionnés ; « Gérer les scripts… » en bas.
- [ ] **MS-02 [M]** Clavier : Flèche droite / Entrée ouvrent le sous-menu, Flèche gauche / Échap le referment (focus rendu à « Scripts ») ; Échap ferme ensuite le menu ; beaucoup de scripts : le sous-menu défile ; sans Python : entrées grisées avec la raison.
- [ ] **MS-03 [M]** Même sous-menu depuis le clic droit d'une **pile** ; les autres menus (Lire, Ajouter à la liste, Infos…) fonctionnent comme avant.
- [ ] **PG-01 [M]** Menu des piles (liste de lecture, « Ordre de la liste », « Empiler par série ») : **Monter** est grisé sur la première pile et **Descendre** sur la dernière, avec la raison ; Alt+↑ / Alt+↓ dans ce cas n'a pas d'effet et affiche la raison ; plusieurs piles sélectionnées : le bloc monte / descend d'un cran.
- [ ] **BD-01 [M]** **Bedetheque Scraper 2** : sur un album **sans résumé**, le résumé de la **série** est écrit à la place (rien si la série n'en a pas) ; un album avec résumé n'est pas modifié ; case « Utiliser le résumé de la série si l'album n'en a pas » (onglet Données de la configuration) active par défaut, décochée : pas de résumé de série.
- [ ] **BD-02 [m]** Plusieurs livres d'une même série, ou un lien direct vers un album : pas de blocage, pas de requêtes inutiles ; page de série illisible : un avertissement dans `comicrust.log`, le reste de la fiche est traité.
- [ ] **DL-01 [B]** **Dernière liste** : sélectionne une liste, ferme et rouvre ComicRust (aussi après une mise à jour / un redémarrage) : on revient **sur cette liste** ; si Réglages ou le lecteur étaient ouverts à la fermeture, on revient quand même à la bibliothèque sur cette liste ; aucun livre n'est rouvert.
- [ ] **DL-02 [M]** Liste **supprimée** ou renommée entre-temps : renommée → retrouvée ; supprimée → bibliothèque entière, sans erreur ; Réglages ▸ Démarrage ▸ « Rouvrir la dernière liste au démarrage » décochée : bibliothèque entière à chaque démarrage.
- [ ] **DO-01 [M]** Documentation : `README.md` du zip et À propos / Aide (F1) cohérents avec la version 1.0.1 (raccourcis → / ← dans les menus, jauges).

## 3. Piles réordonnables (liste de lecture, tri « Ordre de la liste », « Empiler par série »)

- [ ] **PI-01 [B]** Glisser une pile entre deux autres : un trait d'insertion apparaît, la pile et **tous ses livres** se placent à cet endroit ; l'ordre est conservé après redémarrage (et dans ComicRack si liste écrite).
- [ ] **PI-02 [B]** Sélectionner plusieurs piles non contiguës (Ctrl+clic ou rectangle) et les glisser : elles se placent ensemble, dans leur ordre relatif.
- [ ] **PI-03 [B]** Une série dont les livres sont dispersés dans la liste est regroupée au dépôt.
- [ ] **PI-04 [B]** Hors liste de lecture modifiable, ou avec un tri autre que « Ordre de la liste » croissant : le glisser est refusé avec la raison affichée ; liste intelligente ou en lecture seule : rien ne bouge.
- [ ] **PI-05 [M]** Clic droit sur une pile : « ⬆ Monter / ⬇ Descendre la pile » ; Alt+↑ / Alt+↓ ; impossible de monter la première / descendre la dernière (entrée grisée).
- [ ] **PI-06 [M]** Après un déplacement, la sélection reste sur les mêmes séries ; Échap annule un glisser en cours ; défilement automatique près des bords.
- [ ] **PI-07 [m]** Regroupement par série + piles : même comportement ; Annuler un déplacement n'est pas prévu (noté dans les limites).

## 4. Livres manquants

- [ ] **LM-01 [B]** Renommer ou déplacer hors de ComicRust un livre de la copie : après analyse, il est marqué « manquant » (icône/mention) sans être supprimé ; pendant l'analyse en cours, aucune fausse marque.
- [ ] **LM-02 [B]** Dialogue « Livres manquants » (menu ou clic droit) : liste des manquants ; **Rechercher les fichiers** retrouve un livre déplacé (par nom, taille) et propose la relocalisation ; deux livres ne peuvent viser le même fichier.
- [ ] **LM-03 [B]** « Retirer de la bibliothèque » un livre manquant : il disparaît de la bibliothèque et des listes, le fichier disque (absent) n'est pas concerné ; confirmation avant.
- [ ] **LM-04 [M]** Un livre manquant ne s'ouvre pas dans le lecteur (message clair) ; il reste visible, filtrable ; les compteurs sont justes.
- [ ] **LM-05 [m]** Dossier hors ligne (disque débranché) : tous ses livres sont marqués, et redeviennent normaux au retour du disque.

## 5. Analyse (scan), fenêtre, fichiers

- [ ] **SC-01 [B]** Au démarrage, les dossiers de la bibliothèque sont analysés automatiquement (réglage désactivable) ; **Arrêter** interrompt proprement ; le **bilan** (ajoutés, modifiés, manquants, erreurs) s'affiche à la fin.
- [ ] **FE-01 [B]** Fichier passé en argument (`ComicRust-Tauri.exe "F:\…\livre.cbz"`) ou déposé sur la fenêtre : s'ouvre dans le lecteur ; `.cbl` déposé ou choisi par Ctrl+O : **importé** comme liste ; dossier déposé : choix lire / ajouter aux dossiers ; chemin inexistant : message d'erreur.
- [ ] **FE-02 [M]** Taille et position de la fenêtre mémorisées d'un lancement à l'autre (y compris maximisée) ; fenêtre ramenée à l'écran si le moniteur a changé ; titre dynamique (livre ouvert, section).
- [ ] **FE-03 [M]** **F11** plein écran en bibliothèque, Échap en sort ; Ctrl+O ouvre le **dialogue natif Windows** (fichiers/dossiers) ; Annuler ne fait rien.
- [ ] **FE-04 [M]** Plantage simulé (lancer avec la variable d'environnement `COMICRUST_SIMULER_PLANTAGE=1`) : `data\plantage.txt` est écrit ; au lancement suivant (sans la variable) la fenêtre « s'est fermé de façon inattendue » s'affiche avec « Copier » ; la fermer supprime le fichier.
- [ ] **FE-05 [m]** Fichiers récents : menu « Récents » (jamais de fichier supprimé), vidage de l'historique.

## 6. Lecteur

- [ ] **LE-01 [M]** Fin de livre : « page suivante » sur la dernière page ouvre **automatiquement** le livre suivant (comme en egui), « page précédente » sur la première ouvre le précédent.
- [ ] **LE-02 [M]** Touche **I** : panneau d'infos du livre dans le lecteur ; F1 ou ? dans le lecteur ouvre/ferme l'aide du lecteur (et n'ouvre **pas** l'À propos).
- [ ] **LE-03 [m]** `L` : voir la liste des raccourcis du lecteur (À propos ▸ raccourcis) ; reprise de lecture à la dernière page.

## 7. Réglages, aide, assistant

- [ ] **RE-01 [M]** **À propos / Aide** (F1 en bibliothèque ou entrée de la barre latérale) : version, état de PDFium, dossier `data`, lien du dépôt, raccourcis ; « Copier les informations de diagnostic » ne contient **aucun mot de passe ni clé**.
- [ ] **RE-02 [M]** **Assistant de configuration rejouable** (Réglages) : relance le choix de connexion / des dossiers ; en fin, la bibliothèque et les listes se rechargent sans redémarrer ; « Changer de connexion » possible.
- [ ] **RE-03 [m]** Thème **« Comme Windows »** (suit clair/sombre de Windows) ; réglage « Suivre ComicRack » ; **Ctrl+Maj+L** masque / affiche le panneau Listes (mémorisé) ; ignoré sous le lecteur, l'assistant et une fenêtre ouverte.
- [ ] **RE-04 [m]** Placeholder de recherche à jour ; `prefers-reduced-motion` / mode contraste forcé pris en compte (si activé dans Windows).

## 8. Bibliothèque : regroupements, tableau, copie

- [ ] **RG-01 [M]** **9 regroupements** : Aucun, Série, Éditeur, Scénariste, Année, Arc, Genre, Format, État de lecture ; chacun donne les mêmes groupes et nombres qu'en egui ; en-têtes de groupe en mode empilé.
- [ ] **RG-02 [M]** **Tableau 11 colonnes** : choisir les colonnes, les redimensionner (largeurs mémorisées), tri par colonne.
- [ ] **RG-03 [m]** « Copier les livres » (liste de chemins/titres dans le presse-papiers) ; « Relire » un livre (relit ses infos du fichier) ; la section « Séries » de la maquette a disparu.

## 9. Performance (indicatif)

- [ ] **PF-01 [m]** Sur la vraie bibliothèque (≈ 5 300 livres) : défilement fluide de la grille, ouverture < 3 s, mémoire de `msedgewebview2` raisonnable (noter les valeurs du Gestionnaire des tâches) : ……… Mo

## 10. Non-régression

- [ ] **N-01 [B]** Bibliothèque : grille, tableau, piles, recherche, tris, sélection par rectangle, glisser de livres vers une liste.
- [ ] **N-02 [B]** Listes (importer .cbl, listes intelligentes, ajout/retrait), transfert ComicRack, mise à jour intégrée : comme à la tauri-v0.10.0.
- [ ] **N-03 [B]** Lecteur (CBZ, PDF, double page, zoom), édition des infos (5 onglets, pages, ComicInfo), conversion.
- [ ] **N-04 [M]** Thèmes (3 livrés, perso, aperçu, import/export), Réglages, MySQL (mot de passe chiffré, jamais affiché).
- [ ] **N-05 [M]** Scripts (aperçu + application), Bedetheque, serveur MCP : comme à la tauri-v0.10.0.
- [ ] **N-07 [B]** « Marquer comme lu / non lu » (livres et piles, copie autonome) : pastilles et jauges à jour, sélection conservée ; ComicRack en lecture seule : entrées grisées avec la raison ; livre ouvert dans le lecteur ou d'une seule page : ignoré avec la raison.
- [ ] **N-06 [M]** Copie **portable** : rien n'est écrit hors de `data\` (profil WebView2 dans `data\webview2`).

## Limites connues (acceptées pour cette version)

- Mêmes limites que la [tauri-v1.0.1](tauri-v1.0.1.md).
- La mise à jour « tout ou rien », le blocage de la fermeture pendant l'installation et les redirections HTTPS seulement ne s'appliquent qu'**à partir de la 1.0.2** (la mise à jour 1.0.1 → 1.0.2 utilise l'ancien installateur).
- Un livre « lu » reste à 100 % dans ComicRust (ComicRack le considère lu dès 95 %) ; « Marquer comme lu / non lu » ne modifie pas le nombre d'ouvertures (ComicRack, si).
- Instance unique par dossier `data\` : une 2ᵉ instance sur le même dossier transmet son fichier et s'arrête.
- Non traités dans cette version : signature cryptographique des mises à jour, base SQLite en mode WAL, écritures MySQL par lots, couverture PDF rendue directement à la taille de la miniature, onglet Pages qui mémorise encore sa vue dans le profil WebView2.

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
