# Recette ComicRust Tauri tauri-v0.9.0

| | |
|---|---|
| Version | tauri-v0.9.0 (pré-release de l'étape **scripts Python**) |
| Paquet | `ComicRust-Tauri-tauri-v0.9.0-windows-x64.zip` (contient `ComicRust-Tauri.exe`, `ComicRust-Tauri-mcp.exe`, `python\`, `scripts\`, `scripts.md`) |
| SHA-256 | `e08a3175f821a05edd7f1943a30e76d3dd950e80cbc53c52efb43453e28b9e41` |
| Contenu | hôte de scripts Python (même moteur que la version egui), menu Scripts, formulaires et choix générés, aperçu puis application, automatismes, scripts d'exemple (nettoyage, rangement, Comic Vine, Bedetheque Scraper 2, Library Organizer), bouton Scripts de la fenêtre Infos ; mise à jour qui préserve les scripts modifiés |
| Origine | [recette tauri-v0.8.1](tauri-v0.8.1.md) : **GO** du 12/11/2026 ; tests repris des recettes egui [v0.10.0](v0.10.0.md) à [v0.10.7](v0.10.7.md) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ **Sauvegardes** : les scripts modifient des fiches et peuvent **renommer ou déplacer des fichiers**.
> Les tests de rangement (S-03, S-04, O-02 à O-05) se font sur des **BD copiées dans un dossier de
> test** (ex. « Echantillons TESTS »), jamais sur ta collection du NAS. Copie `data\` avant de commencer.
> ComicRack reste en lecture seule sauf si tu coches « Lecture et écriture » (S-08 seulement).

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **199 tests** du backend `comicrust-tauri`, dont : faux front qui répond à une question, un formulaire et un choix, puis arrête un script pendant une question ; script d'exemple **nettoyage** réellement exécuté (Python 3.13) sur une base de test : aperçu, application partielle, relecture de la base ; **refus d'appliquer en lecture seule** (ComicDb.xml) ; **secret jamais renvoyé ni écrit en clair** (« •••• » ne l'écrase pas) ; arrêt forcé puis fermeture en moins de 8 s ; automatisme « nouveaux livres » après une vraie analyse ; hook Editor ; mise à jour sur 5 versions successives : script modifié gardé (`.nouveau`), script supprimé non réinstallé, manifeste créé au premier lancement | `cargo test -p comicrust-tauri` | ✅ 199/199 |
| 11 tests des scripts d'exemple du cœur (nettoyage, rangement, Bedetheque, Library Organizer…) ; la CI les relance **sur le contenu du paquet** avant de le zipper | `cargo test -p comicrust-core --test scripts_exemples` | ✅ 11/11 |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours simulé complet : menu, gestion, lancement, questions oui/non, texte masqué, choix, formulaire, choix à vignettes chargées à l'affichage, aperçu et application, arrêt, erreur, lecture seule, Python introuvable, bouton Editor, clavier, thème sombre ; **textes piégés d'un script affichés en texte, jamais exécutés** | local, Playwright | ✅ 95/95 |
| Concordance des 13 commandes et 10 événements entre l'interface et le moteur | relecture | ✅ |
| Rustfmt, Clippy `-D warnings` ; compilation Windows (fenêtre, `-mcp`), Python embarqué (empreinte figée), paquet | local + CI Windows | ✅ local ; CI à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2, Python embarqué Windows, Bedetheque et Comic Vine en réseau, DPAPI, ComicRack MySQL.

## 1. Installation

- [ ] **P-01 [B]** Ta tauri-v0.8.1 propose la tauri-v0.9.0 ; « Installer » puis « Redémarrer maintenant » → ⚙ Réglages : **Version 0.9.0** (dans chacune des deux copies). À côté de l'exe : dossiers `python\` et `scripts\`, fichier `scripts.md`. Noter : ……… / ………
- [ ] **P-02 [B]** **📜 Scripts** → « Gérer les scripts… » : les scripts d'exemple sont listés (nettoyage, rangement, Comic Vine, nouveaux livres, Bedetheque Scraper 2, Library Organizer), **Python est indiqué comme trouvé**, aucun avertissement ; « Exécuter les automatismes » est **décoché**.

## 2. Scripts : exécution, aperçu, application (repris de v0.10.0)

- [ ] **S-01 [B]** Copie **autonome** : sélectionner quelques livres, clic droit → **Scripts ▸** « Nettoyer les fiches » : la fenêtre montre l'avancement puis l'**aperçu** (livre / champ / avant → après). « Annuler » : rien n'est modifié.
- [ ] **S-02 [B]** Relancer, décocher un livre dans l'aperçu, « Appliquer » : seuls les livres cochés sont modifiés ; bilan affiché. Fermer et relancer ComicRust : les modifications sont gardées.
- [ ] **S-03 [B]** Sur 3 BD **copiées dans un dossier de test** (ajouté à la bibliothèque autonome) : « Ranger les fichiers » (⚙ Configurer : modèle de nom, sous-dossiers par série) : l'aperçu montre ancien → nouveau chemin ; « Appliquer » : les fichiers sont renommés/déplacés, les livres s'ouvrent toujours.
- [ ] **S-04 [M]** Relancer « Ranger les fichiers » sur les mêmes livres : rien à renommer. Préparer un conflit (un fichier existant au nom visé) : il est **signalé** dans l'aperçu et **pas** appliqué ; aucun fichier écrasé.
- [ ] **S-05 [M]** Pendant un script long (« Nettoyer » sur toute la bibliothèque par 📜 Scripts), « Arrêter » : le script s'arrête ; l'aperçu propose ce qui a déjà été calculé.
- [ ] **S-06 [m]** (si tu as une clé d'API Comic Vine) « Comic Vine » → Configurer : saisir la clé (**champ masqué**) ; sur un comics connu : choix proposé, aperçu des champs, « Appliquer ». Rouvrir les réglages : la clé reste masquée (« •••• »).
- [ ] **S-07 [M]** Écrire ton propre petit script dans `scripts\` (exemple dans `scripts.md`, par ex. mettre `Tags` à « test »), « Recharger » : il apparaît dans le menu et fonctionne avec l'aperçu.
- [ ] **S-08 [M]** Copie **liée**, ComicRack en **écriture** (sauvegarde faite, « Lecture et écriture » coché), ComicRack fermé : « Nettoyer les fiches » sur 2 livres → Appliquer ; ouvrir ComicRack : fiches modifiées, rien d'autre ; ComicRust n'affiche pas « modifié ailleurs ».
- [ ] **R-01 [B]** Copie **liée** en **lecture seule** : un script s'exécute et montre son aperçu, mais « Appliquer » est **désactivé** avec l'explication ; ComicRack n'est pas modifié.
- [ ] **A-01 [M]** « Gérer les scripts… » : cocher « Exécuter les automatismes » et activer l'automatisme « nouveaux livres » ; ajouter un CBZ dans un dossier surveillé, analyser les dossiers : le nouveau livre reçoit l'étiquette du script ; le journal des automatismes le montre.
- [ ] **S-09 [M]** Pendant l'application d'un rangement (quelques livres de test) : la fenêtre affiche « Application en cours… » avec **Arrêter** ; Annuler et Fermer sont indisponibles ; lancer un autre script est refusé. « Arrêter » : arrêt entre deux livres, bilan « arrêté ».
- [ ] **A-02 [B]** Fermer ComicRust pendant l'application d'un aperçu (rangement de quelques livres de test) : ComicRust finit le livre en cours puis se ferme ; à la relance, aucun livre à moitié renommé.
- [ ] **A-03 [m]** Après fermeture de ComicRust, aucun `python.exe` ne reste dans le gestionnaire des tâches.

## 3. Bedetheque Scraper 2 (repris de v0.10.1 à v0.10.7)

- [ ] **B-01 [B]** Sur un album franco-belge connu (Astérix, Blacksad…) : clic droit → Scripts ▸ « Bedetheque Scraper 2 » : choix de la série / de l'album **avec couvertures** (elles arrivent une à une), la fenêtre **reste ouverte sans compte à rebours** ; puis aperçu des champs remplis ; « Appliquer ».
- [ ] **B-02 [M]** Sur 3 à 5 albums d'une même série : traitement l'un après l'autre, sans erreur ni « site bloqué » ; valeurs identiques à celles de Bedetheque Scraper 2 dans ComicRack.
- [ ] **B-03 [M]** « Configurer BD2 » : mêmes onglets et options que dans ComicRack ; « Importer les réglages de ComicRack… » avec ton `App.Config` : tes options sont reprises.
- [ ] **B-04 [M]** Fenêtre **Infos** d'un livre (Ctrl+I) → bouton **📜 Scripts** → Bedetheque : les champs de la fenêtre se remplissent, **rien n'est enregistré avant « OK »**.
- [ ] **B-05 [M]** **QuickScrape BD2** : coller un lien d'album, « 👁 Aperçu du lien » (couverture, titre), « Chercher » : aperçu des champs, « Appliquer ».
- [ ] **B-06 [M]** « Bedetheque Scraper 2 – tester le site » : rapport ✔/✖ des champs sur **Blacksad – Quelque part entre les ombres**, conclusion « Tout est trouvé ».
- [ ] **B-07 [m]** Album Akira (Glénat cartonnés en couleur) tome 3 : fiche remplie, aucune ligne « site bloqué ».
- [ ] **B-08 [m]** Créer `data\bedetheque\motifs.json` contenant `{"version": "1.0", "motifs": {"ALBUM_EVAL": "("}}` : le script fonctionne toujours, avertissement dans le journal ; supprimer le fichier ensuite.

## 4. Library Organizer (repris de v0.10.1), sur des BD de test

- [ ] **O-01 [B]** « Library Organizer » → configuration : « Importer les réglages de Library Organizer (ComicRack)… » avec ton `losettingsx.dat` : profils et modèles repris ; dossier de base → le dossier de test.
- [ ] **O-02 [B]** Mode **Simuler** : le rapport montre les chemins, **identiques à ceux de Library Organizer dans ComicRack** ; aucun fichier n'a bougé.
- [ ] **O-03 [B]** Mode **Déplacer** : aperçu ancien → nouveau chemin, « Appliquer » : les fichiers sont rangés, les livres s'ouvrent.
- [ ] **O-04 [M]** Relancer : rien à déplacer. Un doublon à la destination : fenêtre avec les deux fichiers, « garder les deux » ou « ne pas déplacer », jamais d'écrasement.
- [ ] **O-05 [M]** « Annuler le dernier rangement » : les fichiers reviennent ; les dossiers vides sont proposés à la suppression (jamais au-dessus du dossier de base).

## 5. Mise à jour et scripts

- [ ] **U-01 [M]** Modifier un script d'exemple (ex. une ligne de commentaire dans `scripts\exemples\nettoyage.py`), puis copie de travail « ComicRust-Tauri MAJ » lancée par `MAJ.cmd` (adapter la ligne à `set COMICRUST_SIMULER_VERSION=0.8.1`) → Installer : ton script modifié est **gardé**, la version livrée est posée à côté en `nettoyage.py.nouveau` ; le bilan le signale.

## 6. Non-régression

- [ ] **N-01 [B]** Bibliothèque (empilée ou non), listes, recherche, tri ; Ctrl+I sur un et plusieurs livres.
- [ ] **N-02 [B]** Lecteur : CBZ et PDF, Échap (Détail à jour), relecture d'un livre lu.
- [ ] **N-03 [M]** `data\comicrust-tauri.json` : **aucun mot de passe ni clé en clair** (seulement `dpapi:…`).
- [ ] **N-04 [m]** Copie liée en MCP : `infos` répond (0.9.0).

## Limites connues (acceptées pour cette version)

- Pas de bouton « Parcourir… » pour les champs fichier ou dossier d'un formulaire de script (saisie du chemin) ; pas de bouton « Ouvrir le dossier des scripts ».
- Les automatismes ne vérifient pas si un transfert de bibliothèque est en cours (la version egui le fait) ; un livre fermé par éviction (trop de livres ouverts) ne déclenche pas « livre fermé ».
- Le « copier dans le presse-papiers » d'un script passe par `clip.exe` : à confirmer en recette.
- Mise à jour : un fichier retiré du paquet d'une version à l'autre (ancien exemple, fichier de `python\`) reste sur le disque.
- Reprises : pas d'écran « Changer de connexion » ; pas de retour arrière global si un fichier est verrouillé pendant une mise à jour ; relancer Claude Desktop après une mise à jour.

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| (relecture) | [M] | Pendant l'application d'un aperçu, un second script pouvait être lancé et la fenêtre fermée (« rien n'a été appliqué » affiché à tort) | **Corrigé** : « Application en cours… », Arrêter (entre deux livres), lancement refusé jusqu'au bilan |
| (relecture) | [M] | Fermer ComicRust pendant qu'un automatisme renommait des fichiers pouvait couper un livre au milieu | **Corrigé** : la fermeture attend la fin du livre en cours |
| (relecture) | [m] | Fenêtre console au « copier » d'un script ; automatismes bloqués après un échec de lancement ; ordre d'écriture des réglages ; secret affichable en clair ; images SVG acceptées | **Corrigés** |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | | | |
| [M] majeurs | | | |
| [m] mineurs | | | |

**Décision :** ☐ GO ☐ NO-GO — Yoyo, le …
