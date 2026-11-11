# Recette ComicRust Tauri tauri-v0.7.0

| | |
|---|---|
| Version | tauri-v0.7.0 (pré-release de l'étape 6 : **MySQL, transfert, surveillance, hors ligne**) |
| Paquet | `ComicRust-Tauri-tauri-v0.7.0-windows-x64.zip` (contient aussi `ComicRust-Tauri-mcp.exe`) |
| SHA-256 | à renseigner à la publication |
| Contenu | connexion MySQL avec bouton Tester (base de ComicRust SQLite ou MySQL, base ComicRack saisie à la main), mot de passe chiffré pour ton compte Windows ; transfert de la bibliothèque ComicRack vers la base de ComicRust ; surveillance de la base toutes les 5 s et mode hors ligne ; 7 anomalies mineures de la tauri-v0.6.3 (dont L-08) ; pont MCP signalant un port occupé |
| Origine | [recette tauri-v0.6.3](tauri-v0.6.3.md) : **GO** de Yoyo le 11/11/2026 (34 OK sur 35, S-02 vérifié par le MCP) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ **Mot de passe MySQL** : il se saisit uniquement dans l'application sur ton PC. Ne le colle
> jamais dans un rapport de recette, une note d'échec ni une conversation.
>
> ⚠ **Sauvegardes** : le transfert (section 3) n'écrit que dans `data\comicrust.db` de la copie
> **autonome**, mais fais d'abord une copie de ce dossier `data\`. ComicRack reste en lecture seule
> sauf si tu coches « Lecture et écriture ».

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **147 tests** du backend `comicrust-tauri` : réglages MySQL chiffrés et lecture tolérante des anciens fichiers, mot de passe masqué (Debug, erreurs, aperçus), choix SQLite/MySQL de la base autonome, transfert (**5 323 livres, 47 listes, 0 doublon** sur la fixture, second passage sans doublon, `ComicDb.xml` identique à l'octet), doublon évité, arrêt, une seule opération à la fois, surveillance (transitions en ligne / hors ligne, écritures propres non renotifiées), refus des écritures hors ligne, suppression d'un dossier de listes avec ses vues, pont MCP (port occupé, relances) | `cargo test -p comicrust-tauri` | ✅ 147/147 |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours simulés des nouvelles boîtes (formulaire MySQL, transfert, bandeau hors ligne) | local | ✅ |
| Rustfmt, Clippy `-D warnings` | local | ✅ |
| Compilation de la fenêtre (`--features app`), variante `-mcp` | CI Windows (WebView2 absent ici) | à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2, ta vraie base MySQL (les tests MySQL réels
exigent `COMICRUST_TEST_MYSQL`, absente ici), ComicRack, DPAPI (chiffrement Windows).

## 1. Installation

- [ ] **P-01 [B]** Copier le zip **par-dessus** les copies **autonome** et **liée** (`data\` conservé), y compris `ComicRust-Tauri-mcp.exe`. « ⚙ Réglages » indique **Version 0.7.0** dans chacune. Noter : ……… / ………
- [ ] **P-02 [B]** Les bibliothèques et listes existantes s'ouvrent comme avant (réglages d'avant la 0.7.0 relus sans erreur).

## 2. Connexion MySQL

Pour ces tests, utiliser un **dossier neuf** : extraire le zip dans un nouveau dossier (sans `data\`), afin de voir l'assistant de premier démarrage.

- [ ] **C-01 [B]** Premier démarrage, ComicRust autonome : choisir **SQLite** (par défaut) → la bibliothèque s'ouvre, `data\comicrust.db` est créé.
- [ ] **C-02 [B]** Dossier neuf, ComicRust autonome avec **base MySQL** : saisir serveur, port, utilisateur, mot de passe, base (champ mot de passe masqué) ; **Tester** affiche un succès lisible. Puis un test avec un **mot de passe volontairement faux** : message d'erreur en français, **sans** le mot de passe dedans.
- [ ] **C-03 [B]** Valider : la base est ouverte ; `data\comicrust-tauri.json` contient les paramètres MySQL avec un mot de passe **chiffré (`dpapi:…`)**, jamais en clair. Ouvrir le fichier et vérifier.
- [ ] **C-04 [B]** Fermer et relancer : la connexion est rouverte **sans ressaisir** le mot de passe.
- [ ] **C-05 [M]** Dossier neuf, mode **ComicRack saisi à la main** (sans `ComicRack.ini`) : formulaire + Tester ; la bibliothèque ComicRack se charge en **lecture seule** (rien n'est écrit dans ComicRack).
- [ ] **C-06 [M]** Mode ComicRack lié par `ComicRack.ini` (copie liée) : toujours fonctionnel comme avant.
- [ ] **C-07 [m]** Savoir : « Tester » en mode autonome crée les tables `comicrust_…` dans la base cible (comportement attendu, documenté) — vérifier qu'aucune table `comics` de ComicRack n'est modifiée.

## 3. Transfert ComicRack → ComicRust

Sur la copie **autonome**, `data\` sauvegardé.

- [ ] **T-01 [B]** « ⚙ Réglages » → « Transférer la bibliothèque ComicRack… » : la boîte s'ouvre, source MySQL (`ComicRack.ini`) ou `ComicDb.xml` ; l'aperçu ne montre **aucun mot de passe**.
- [ ] **T-02 [B]** Lancer le transfert : progression visible, bilan final. Attendu : **5 323 livres, 47 listes, 0 doublon**. Noter le bilan : ………
- [ ] **T-03 [B]** Relancer le même transfert : **aucun doublon** (livres mis à jour, 0 ajouté), toujours 47 listes.
- [ ] **T-04 [B]** ComicRack n'a **rien** perdu : `ComicDb.xml` (date et taille inchangées) et la base MySQL intacts ; ComicRack s'ouvre normalement.
- [ ] **T-05 [M]** Annuler un transfert en cours : arrêt propre (entre deux lots), la bibliothèque reste utilisable, un nouveau transfert reprend sans doublon.
- [ ] **T-06 [M]** Pendant un transfert, une seconde opération longue (édition, scan) est refusée avec un message clair.
- [ ] **T-07 [m]** Chemin `ComicDb.xml` erroné : message d'erreur lisible, rien n'est modifié.

## 4. Surveillance et hors ligne

Copie liée, base ComicRack MySQL.

- [ ] **S-01 [B]** ComicRack modifie un livre (note, lu) pendant que ComicRust est ouvert : en ≤ 10 s, ComicRust affiche le changement **sans recharger**.
- [ ] **S-02 [B]** Couper l'accès à la base (arrêter le serveur MySQL ou débrancher le réseau) : un bandeau « ⚠ Base injoignable, nouvel essai… » apparaît (cause en info-bulle) ; la lecture des livres déjà chargés reste possible.
- [ ] **S-03 [B]** Hors ligne : une modification de fiche (Infos), un renommage, « Écrire les infos dans le fichier » sont **refusés avec un message en français** ; le formulaire Infos est grisé et les saisies conservées ; rien ne plante.
- [ ] **S-04 [B]** Rétablir la base : le bandeau disparaît tout seul (ou par « Réessayer ») ; les écritures redeviennent possibles.
- [ ] **S-05 [M]** Les écritures faites par ComicRust lui-même ne se « re-notifient » pas (pas de rafraîchissement parasite après une modification de fiche).
- [ ] **S-06 [M]** Pendant un scan de dossier, le sondage ne perturbe pas l'opération.
- [ ] **S-07 [m]** Copie autonome SQLite : pas de bandeau, pas de message hors ligne.

## 5. Anomalies de la tauri-v0.6.3 corrigées

- [ ] **A-01 [M]** Bibliothèque en lecture seule : « Monter » / « Descendre » des listes sont **grisés** ; le message de refus mentionne « Réglages ▸ Bibliothèque ComicRack ».
- [ ] **A-02 [m]** Liste intelligente : le tri « # Ordre de la liste » n'est **pas proposé** ; il l'est pour une liste de lecture.
- [ ] **A-03 [m]** Thème sombre, « Déplacer dans… » : l'option sélectionnée est **lisible** (texte clair sur fond contrasté).
- [ ] **A-04 [m]** Supprimer un dossier de listes réglées : `comicrust-tauri.json` n'a plus d'entrée `vue.listes` pour le dossier **ni** ses sous-listes.
- [ ] **A-05 [M]** **L-08 avec une vraie souris** : Ctrl+clic sur deux étiquettes de série, glisser vers une liste : **les deux séries** sont ajoutées. Refaire en glissant **très vite** dès le Ctrl+clic.
- [ ] **A-06 [m]** MCP : occuper le port 47802 avec un autre programme puis lancer `ComicRust-Tauri-mcp.exe --mcp` : alerte dans le titre de la fenêtre ; le pont `--mcp-stdio` signale « port occupé » au lieu d'échouer en silence.

## 6. Non-régression

- [ ] **N-01 [B]** Bibliothèque : grille, tableau, recherche, tri, groupement, couvertures (copie liée : 5 3xx livres).
- [ ] **N-02 [B]** Lecteur : ouvrir un CBZ et un PDF, tourner les pages, progression enregistrée.
- [ ] **N-03 [B]** Listes (47) : mêmes comptes qu'à la tauri-v0.6.3 (dont « Lu » inchangé) ; tri/vue par liste mémorisés.
- [ ] **N-04 [M]** Édition d'une fiche en mode « Lecture et écriture » (copie liée, ComicRack fermé) puis ouverture dans ComicRack : la fiche suit.
- [ ] **N-05 [m]** Réglages (thème, vue) : conservés après relance.

## Limites connues (acceptées pour cette version)

- Le transfert écrit toujours dans `data\comicrust.db` (pas de choix MySQL ni de fichier portable comme dans la version egui) ; le chemin de `ComicDb.xml` se saisit à la main (pas de bouton Parcourir).
- L'annulation du transfert n'agit qu'entre deux lots, pas pendant la lecture MySQL.
- Pas d'écran « Changer de connexion » dans les Réglages : une erreur d'ouverture au démarrage (MySQL injoignable, mot de passe perdu après changement de compte Windows) affiche seulement un message ; pour reconfigurer, supprimer la connexion dans `data\comicrust-tauri.json` ou repartir d'un dossier neuf. À faire en tauri-v0.7.1.
- Lecture faite hors ligne en mode « Lecture et écriture » : conservée dans `avancement.json`, non reportée dans la fiche ComicRack au retour de la base.
- « Tester » en mode autonome crée le schéma ComicRust dans la base cible.

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

**Décision :** ☐ GO ☐ NO-GO — Yoyo, le …
