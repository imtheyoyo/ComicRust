# Recette ComicRust Tauri tauri-v1.0.0

| | |
|---|---|
| Version | tauri-v1.0.0 (**première release** de la version Tauri, étape 11) |
| Paquet | `ComicRust-Tauri-tauri-v1.0.0-windows-x64.zip` |
| SHA-256 | `fd2237dbb2d4f81d7e49f351df82e57454d726085d3674a8b32d66ae8ee0cf8f` |
| Contenu | gel des fonctions ; menu du livre : scripts affichés directement, « Marquer comme lu / non lu » ; documentation finalisée ; refait **toute** la recette de non-régression de la 0.11.0 |
| Origine | [recette tauri-v0.11.0](tauri-v0.11.0.md) : **GO** du 22/11/2026 ; demandes de Yoyo ([rapport](rapports/tauri-v0.11.0-2026-11-22.txt)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Après le **GO**, la version egui est archivée et la version Tauri devient la seule maintenue
> (fusion sur `main`) : cette recette est donc la dernière avant ce changement. Installation par la
> mise à jour intégrée de ta 0.11.0 (copies autonome et liée). Travaille sur une **copie** de la
> bibliothèque pour « lu / non lu » et les livres manquants (jamais `comickrack_demo`).

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| Tests du backend (≈ 340) dont « Marquer comme lu / non lu » (autonome, lecture seule refusée, pages inconnues, sélection vide) | `cargo test` | ✅ |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours Playwright simulé du menu du livre (scripts, lu / non lu, replis) | local | ✅ |
| Relecture indépendante du menu du livre et de l'écriture de l'état de lecture | | ✅ |
| Rustfmt, Clippy `-D warnings` dont la fenêtre (feature `app`) | CI Windows | ✅ |

**Non vérifiable ici** : la vraie fenêtre WebView2, l'écriture de l'état de lecture dans une vraie base ComicRack (MySQL « Lecture et écriture »).

## 1. Installation et nouveautés

- [ ] **P-01 [B]** Ta 0.11.0 propose la 1.0.0 ; « Installer » puis « Redémarrer » → ⚙ Réglages ▸ À propos : **Version 1.0.0** (deux copies). Noter : ……… / ………
- [ ] **P-02 [M]** Le zip contient `ComicRust-Tauri.exe`, `comicrust-cli.exe` et un `README.md` à jour (version Tauri, sans mention d'egui comme version à utiliser) ; `comicrust-cli.exe --help` répond.
- [ ] **ML-01 [B]** Clic droit sur un livre (grille, tableau, pile ouverte) : la liste des **scripts** apparaît directement dans le menu, sous un titre « Scripts », sans cliquer sur un sous-menu ; en choisir un l'applique au(x) livre(s) sélectionné(s) ; « Gérer les scripts… » ouvre la gestion.
- [ ] **ML-02 [M]** Menu des scripts : sans Python ou pendant l'exécution d'un script, les entrées sont grisées avec la raison en infobulle ; aucun script : « Aucun script » ; plus de 12 scripts : 12 affichés puis « … et N autre(s) » ; le menu défile s'il dépasse l'écran.
- [ ] **ML-03 [B]** « ✔ Marquer comme lu » sur un ou plusieurs livres (copie **autonome**) : pastille « lu » / 100 %, le filtre « non lus » et les listes intelligentes se mettent à jour, la sélection est conservée ; « ↺ Marquer comme non lu » remet à 0 ; le compteur d'ouvertures ne bouge pas.
- [ ] **ML-04 [M]** Même chose sur un clic droit de **pile(s)** : tous les livres des piles sélectionnées sont concernés (le nombre annoncé est juste).
- [ ] **ML-05 [M]** Copie **liée** à ComicRack en lecture seule (mode par défaut) : les deux entrées sont grisées avec la raison, rien n'est écrit ; en mode « Lecture et écriture » (sur une **copie** de la base) : l'état est écrit et ComicRack le voit.
- [ ] **ML-06 [m]** Cas ignorés avec leur raison (les autres livres de la sélection sont traités) : livre **ouvert dans le lecteur**, livre d'**une seule page**, livre sans nombre de pages connu ; si tout est déjà dans l'état demandé : message « Déjà marqué comme lu / non lu. ».
- [ ] **DO-01 [M]** Documentation : `README.md` du zip, À propos / Aide (F1), `docs\` du dépôt : cohérents avec la version (pas de texte périmé, liens valides).

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
- [ ] **N-06 [M]** Copie **portable** : rien n'est écrit hors de `data\` (profil WebView2 dans `data\webview2`).

## Limites connues (acceptées pour cette version)

- Mêmes limites que la [tauri-v0.11.0](tauri-v0.11.0.md) (écritures disque courtes sur le fil principal, chemins absolus dans les textes copiés, taille minimale 800 × 500, séries dispersées entrelacées entre plusieurs piles, sélection de livres repérée par rang après rechargement…).
- Un livre d'une seule page ne peut pas être marqué lu (indiscernable de non lu) ; un livre ouvert dans le lecteur est ignoré.
- « Marquer comme lu / non lu » est refusé en ComicRack lecture seule et avec ComicDb.xml (les pastilles et filtres lisent la fiche, pas le fichier d'avancement local) ; l'écriture dans une vraie base ComicRack n'a pas pu être testée automatiquement.

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | 15 | 15 | 0 |
| [M] majeurs | 20 | 20 | 0 |
| [m] mineurs | 9 | 9 | 0 |

Rapport de Yoyo du 22/11/2026 20:59 ([rapport](rapports/tauri-v1.0.0-2026-11-22.txt)) : 44 OK, 0 échec. P-01 : 1.0.0 / 1.0.0 ; PF-01 : 63 Mo. PI-05 : première/dernière pile OK mais l'entrée Monter/Descendre n'est pas grisée (→ 1.0.1). Demandes → 1.0.1 : jauges des listes (rouge nouveaux, jaune non lus, vert total), scripts en sous-menu latéral, résumé de série dans Bedetheque Scraper 2, version dans le titre, réouverture sur la dernière liste.

**Décision : GO** confirmé par Yoyo le 22/11/2026, avec fusion de `tauri2-web` dans `main` et archivage d'egui (branche `egui-archive`) ; les demandes ci-dessus sont traitées dans la tauri-v1.0.1.
