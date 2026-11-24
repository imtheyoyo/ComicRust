# Recette ComicRust Tauri tauri-v1.0.1

| | |
|---|---|
| Version | tauri-v1.0.1 (version corrective et d'améliorations suite à la tauri-v1.0.0) |
| Paquet | `ComicRust-Tauri-tauri-v1.0.1-windows-x64.zip` |
| SHA-256 | `20a014fd4a57474f33936cb00e26442a36a5fe1bec0d3279844a35d71cb9949f` |
| Contenu | jauges des listes ; scripts en sous-menu latéral ; Monter/Descendre grisés (et déplacement de plusieurs piles) ; résumé de série dans Bedetheque Scraper 2 ; version dans le titre ; réouverture sur la dernière liste |
| Origine | [recette tauri-v1.0.0](tauri-v1.0.0.md) : **GO** du 22/11/2026 ; demandes de Yoyo ([rapport](rapports/tauri-v1.0.0-2026-11-22.txt)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Installation par la mise à jour intégrée de ta 1.0.0 (copies autonome et liée). Les essais de
> Bedetheque nécessitent le réseau ; travaille sur une **copie** de la bibliothèque pour les écritures.
> **À trancher pendant cette recette (JG-02)** : la règle « nouveau » des jauges est une hypothèse.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| Tests du backend (≈ 360) dont les jauges (nouveaux / non lus / total, listes intelligentes, 5 000 livres × 100 listes, cache), les réglages (anciens fichiers sans les nouveaux champs), le titre avec la version, les contrastes WCAG des jauges dans les 3 thèmes | `cargo test` | ✅ |
| Script Bedetheque : 26 tests sur fixtures (résumé de série, une récupération par série, série sans résumé, page série illisible) | `cargo test -p comicrust-core --test bedetheque` | ✅ |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours Playwright simulé (sous-menu au survol/clic/clavier, bords d'écran, jauges dans les 3 thèmes, restauration de la liste) | local | ✅ |
| Relecture indépendante : défauts corrigés avant publication (voir « Anomalies ») | | ✅ |
| Rustfmt, Clippy `-D warnings` dont la fenêtre (feature `app`) | CI Windows | ✅ |

**Non vérifiable ici** : la vraie fenêtre WebView2 (survol et placement du sous-menu, titre de la fenêtre), le vrai site bedetheque.com, les jauges face à ComicRack lui-même.

## 1. Installation et nouveautés

- [ ] **P-01 [B]** Ta 1.0.0 propose la 1.0.1 ; « Installer » puis « Redémarrer » → ⚙ Réglages ▸ À propos : **Version 1.0.1** (deux copies). Noter : ……… / ………
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

## 2. Piles réordonnables (liste de lecture, tri « Ordre de la liste », « Empiler par série »)

- [ ] **PI-01 [B]** Glisser une pile entre deux autres : un trait d'insertion apparaît, la pile et **tous ses livres** se placent à cet endroit ; l'ordre est conservé après redémarrage (et dans ComicRack si liste écrite).
- [ ] **PI-02 [B]** Sélectionner plusieurs piles non contiguës (Ctrl+clic ou rectangle) et les glisser : elles se placent ensemble, dans leur ordre relatif.
- [ ] **PI-03 [B]** Une série dont les livres sont dispersés dans la liste est regroupée au dépôt.
- [ ] **PI-04 [B]** Hors liste de lecture modifiable, ou avec un tri autre que « Ordre de la liste » croissant : le glisser est refusé avec la raison affichée ; liste intelligente ou en lecture seule : rien ne bouge.
- [ ] **PI-05 [M]** Clic droit sur une pile : « ⬆ Monter / ⬇ Descendre la pile » ; Alt+↑ / Alt+↓ ; impossible de monter la première / descendre la dernière (entrée grisée).
- [ ] **PI-06 [M]** Après un déplacement, la sélection reste sur les mêmes séries ; Échap annule un glisser en cours ; défilement automatique près des bords.
- [ ] **PI-07 [m]** Regroupement par série + piles : même comportement ; Annuler un déplacement n'est pas prévu (noté dans les limites).

## 3. Livres manquants

- [ ] **LM-01 [B]** Renommer ou déplacer hors de ComicRust un livre de la copie : après analyse, il est marqué « manquant » (icône/mention) sans être supprimé ; pendant l'analyse en cours, aucune fausse marque.
- [ ] **LM-02 [B]** Dialogue « Livres manquants » (menu ou clic droit) : liste des manquants ; **Rechercher les fichiers** retrouve un livre déplacé (par nom, taille) et propose la relocalisation ; deux livres ne peuvent viser le même fichier.
- [ ] **LM-03 [B]** « Retirer de la bibliothèque » un livre manquant : il disparaît de la bibliothèque et des listes, le fichier disque (absent) n'est pas concerné ; confirmation avant.
- [ ] **LM-04 [M]** Un livre manquant ne s'ouvre pas dans le lecteur (message clair) ; il reste visible, filtrable ; les compteurs sont justes.
- [ ] **LM-05 [m]** Dossier hors ligne (disque débranché) : tous ses livres sont marqués, et redeviennent normaux au retour du disque.

## 4. Analyse (scan), fenêtre, fichiers

- [ ] **SC-01 [B]** Au démarrage, les dossiers de la bibliothèque sont analysés automatiquement (réglage désactivable) ; **Arrêter** interrompt proprement ; le **bilan** (ajoutés, modifiés, manquants, erreurs) s'affiche à la fin.
- [ ] **FE-01 [B]** Fichier passé en argument (`ComicRust-Tauri.exe "F:\…\livre.cbz"`) ou déposé sur la fenêtre : s'ouvre dans le lecteur ; `.cbl` déposé ou choisi par Ctrl+O : **importé** comme liste ; dossier déposé : choix lire / ajouter aux dossiers ; chemin inexistant : message d'erreur.
- [ ] **FE-02 [M]** Taille et position de la fenêtre mémorisées d'un lancement à l'autre (y compris maximisée) ; fenêtre ramenée à l'écran si le moniteur a changé ; titre dynamique (livre ouvert, section).
- [ ] **FE-03 [M]** **F11** plein écran en bibliothèque, Échap en sort ; Ctrl+O ouvre le **dialogue natif Windows** (fichiers/dossiers) ; Annuler ne fait rien.
- [ ] **FE-04 [M]** Plantage simulé (lancer avec la variable d'environnement `COMICRUST_SIMULER_PLANTAGE=1`) : `data\plantage.txt` est écrit ; au lancement suivant (sans la variable) la fenêtre « s'est fermé de façon inattendue » s'affiche avec « Copier » ; la fermer supprime le fichier.
- [ ] **FE-05 [m]** Fichiers récents : menu « Récents » (jamais de fichier supprimé), vidage de l'historique.

## 5. Lecteur

- [ ] **LE-01 [M]** Fin de livre : « page suivante » sur la dernière page ouvre **automatiquement** le livre suivant (comme en egui), « page précédente » sur la première ouvre le précédent.
- [ ] **LE-02 [M]** Touche **I** : panneau d'infos du livre dans le lecteur ; F1 ou ? dans le lecteur ouvre/ferme l'aide du lecteur (et n'ouvre **pas** l'À propos).
- [ ] **LE-03 [m]** `L` : voir la liste des raccourcis du lecteur (À propos ▸ raccourcis) ; reprise de lecture à la dernière page.

## 6. Réglages, aide, assistant

- [ ] **RE-01 [M]** **À propos / Aide** (F1 en bibliothèque ou entrée de la barre latérale) : version, état de PDFium, dossier `data`, lien du dépôt, raccourcis ; « Copier les informations de diagnostic » ne contient **aucun mot de passe ni clé**.
- [ ] **RE-02 [M]** **Assistant de configuration rejouable** (Réglages) : relance le choix de connexion / des dossiers ; en fin, la bibliothèque et les listes se rechargent sans redémarrer ; « Changer de connexion » possible.
- [ ] **RE-03 [m]** Thème **« Comme Windows »** (suit clair/sombre de Windows) ; réglage « Suivre ComicRack » ; **Ctrl+Maj+L** masque / affiche le panneau Listes (mémorisé) ; ignoré sous le lecteur, l'assistant et une fenêtre ouverte.
- [ ] **RE-04 [m]** Placeholder de recherche à jour ; `prefers-reduced-motion` / mode contraste forcé pris en compte (si activé dans Windows).

## 7. Bibliothèque : regroupements, tableau, copie

- [ ] **RG-01 [M]** **9 regroupements** : Aucun, Série, Éditeur, Scénariste, Année, Arc, Genre, Format, État de lecture ; chacun donne les mêmes groupes et nombres qu'en egui ; en-têtes de groupe en mode empilé.
- [ ] **RG-02 [M]** **Tableau 11 colonnes** : choisir les colonnes, les redimensionner (largeurs mémorisées), tri par colonne.
- [ ] **RG-03 [m]** « Copier les livres » (liste de chemins/titres dans le presse-papiers) ; « Relire » un livre (relit ses infos du fichier) ; la section « Séries » de la maquette a disparu.

## 8. Performance (indicatif)

- [ ] **PF-01 [m]** Sur la vraie bibliothèque (≈ 5 300 livres) : défilement fluide de la grille, ouverture < 3 s, mémoire de `msedgewebview2` raisonnable (noter les valeurs du Gestionnaire des tâches) : ……… Mo

## 9. Non-régression

- [ ] **N-01 [B]** Bibliothèque : grille, tableau, piles, recherche, tris, sélection par rectangle, glisser de livres vers une liste.
- [ ] **N-02 [B]** Listes (importer .cbl, listes intelligentes, ajout/retrait), transfert ComicRack, mise à jour intégrée : comme à la tauri-v0.10.0.
- [ ] **N-03 [B]** Lecteur (CBZ, PDF, double page, zoom), édition des infos (5 onglets, pages, ComicInfo), conversion.
- [ ] **N-04 [M]** Thèmes (3 livrés, perso, aperçu, import/export), Réglages, MySQL (mot de passe chiffré, jamais affiché).
- [ ] **N-05 [M]** Scripts (aperçu + application), Bedetheque, serveur MCP : comme à la tauri-v0.10.0.
- [ ] **N-07 [B]** « Marquer comme lu / non lu » (livres et piles, copie autonome) : pastilles et jauges à jour, sélection conservée ; ComicRack en lecture seule : entrées grisées avec la raison ; livre ouvert dans le lecteur ou d'une seule page : ignoré avec la raison.
- [ ] **N-06 [M]** Copie **portable** : rien n'est écrit hors de `data\` (profil WebView2 dans `data\webview2`).

## Limites connues (acceptées pour cette version)

- Mêmes limites que la [tauri-v1.0.0](tauri-v1.0.0.md).
- Jauges : la règle « nouveau » (non lu, jamais ouvert, ajouté il y a **moins de 7 jours**) est une hypothèse à confirmer en JG-02 ; le premier scan d'une collection existante rend tous ses livres « nouveaux » pendant une semaine ; la bibliothèque entière s'affiche brièvement avant de basculer sur la dernière liste au démarrage.
- Bedetheque : non vérifié sur le vrai site (présence du lien vers la série dans les vraies pages d'album, comportement face à Cloudflare) ; la recherche ordinaire relit la page de la série pour chaque livre de la même série (comportement d'avant).
- Plusieurs piles dispersées : Monter / Descendre les regroupe au point d'arrivée (comme le glisser-déposer).

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| (relecture) | [M] | Sous-menu : fermeture par le pointeur pendant que le clavier y a le focus (plus aucune touche ne répondait) | **Corrigé avant publication** |
| (relecture) | [m] | Règle « nouveau » (7 jours strict, date future), minuit, contraste des pastilles, lien de série pris hors de l'en-tête, GUID de liste sensible à la casse, flash au démarrage | **Corrigés** |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | 18 | 18 | 0 |
| [M] majeurs | 22 | 22 | 0 |
| [m] mineurs | 10 | 10 | 0 |

Rapport de Yoyo du 24/11/2026 21:53 ([rapport](rapports/tauri-v1.0.1-2026-11-24.txt)) : 50 OK, 0 échec. P-01 : 1.0.1 / 1.0.1 ; JG-02 : 5 / 47 / 48 (rouge / jaune / vert) ; PF-01 : 37 Mo.

**Décision : GO** confirmé par Yoyo le 24/11/2026.
