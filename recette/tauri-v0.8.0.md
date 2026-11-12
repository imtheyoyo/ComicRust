# Recette ComicRust Tauri tauri-v0.8.0

| | |
|---|---|
| Version | tauri-v0.8.0 (pré-release de l'étape **mises à jour**, avancée par Yoyo le 12/11/2026) |
| Paquet | `ComicRust-Tauri-tauri-v0.8.0-windows-x64.zip` (contient aussi `ComicRust-Tauri-mcp.exe`) |
| SHA-256 | `6bc161350c34fb96a4a35fdcf716dc506ac16367360691ebff21f5e385e4f8f5` |
| Contenu | mise à jour automatique depuis les releases `tauri-v*` (vérification, bandeau, notes de version, téléchargement, SHA-256, remplacement `.old`, redémarrage) ; correctif C-05 |
| Origine | [recette tauri-v0.7.3](tauri-v0.7.3.md) : **GO** du 12/11/2026 (C-05 [m] à corriger) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> **Comment tester une mise à jour dès maintenant ?** La tauri-v0.7.3 installée n'a pas la mise à
> jour intégrée, et la 0.8.0 est la plus récente. On fait donc **croire à la 0.8.0 qu'elle est une
> 0.7.3** avec la variable `COMICRUST_SIMULER_VERSION=0.7.3` : elle propose alors la tauri-v0.8.0 de
> GitHub et la réinstalle par-dessus elle-même — exactement le chemin d'une vraie mise à jour.
> Faire ces tests dans une **copie de travail** (ex. copier le dossier « ComicRust-Tauri autonome »
> en « ComicRust-Tauri MAJ ») et la lancer par un fichier `MAJ.cmd` placé à côté de l'exe :
>
> ```bat
> @echo off
> set COMICRUST_SIMULER_VERSION=0.7.3
> start "" "%~dp0ComicRust-Tauri.exe"
> ```

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **178 tests** du backend dont **19 sur la mise à jour** : seules les releases `tauri-vX.Y.Z` bien formées sont retenues (egui `v0.10.x`, brouillons, tags malformés écartés ; finale préférée à une rc), choix du bon zip et du `.sha256`, empreinte fausse → rien modifié, extraction sûre (`..`, chemins absolus refusés), **`data\` intact (dont `data\webview2`)**, deux exe renommés en `.old`, restauration si une copie échoue, nettoyage limité aux `.old` notés, arguments de relance (`--mcp` gardé), filtre des liens ouverts dans le navigateur ; **test de bout en bout avec un faux serveur de releases** (y compris une redirection) | `cargo test -p comicrust-tauri` | ✅ 178/178 |
| Essai réel contre GitHub (test manuel) : téléchargement et installation de la tauri-v0.7.3 dans un dossier de test (12 fichiers, 3 `.old`) | local | ✅ |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours simulé : bandeau, Nouveautés, installation jusqu'à Redémarrer, Plus tard, Ignorer mémorisé, échecs réseau et SHA-256, clavier/Échap, non installable, thème sombre, notes de version piégées affichées en texte ; C-05 et C-06 | local, Playwright | ✅ 45/45 + C-06 |
| Relecture indépendante (sécurité de l'installation, Windows, concurrence) : 1 majeur et 6 mineurs corrigés avant publication (voir « Anomalies ») | | ✅ |
| Rustfmt, Clippy `-D warnings` ; compilation de la fenêtre et variante `-mcp` | local + CI Windows | ✅ local ; CI à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2, le remplacement d'un exe en cours d'exécution sous Windows, le redémarrage.

## 1. Installation

- [ ] **P-01 [B]** Copier le zip dans les deux copies (autonome et liée), `data\` conservé : « ⚙ Réglages » indique **Version 0.8.0**. Noter : ……… / ………
- [ ] **P-02 [M]** Lancement normal (sans `MAJ.cmd`) : **aucun bandeau** (la 0.8.0 est la plus récente), aucun message d'erreur.

## 2. Mise à jour (copie de travail lancée par `MAJ.cmd`)

- [ ] **U-01 [B]** Au démarrage, un **bandeau** annonce « ComicRust tauri-v0.8.0 est disponible » (après quelques secondes) avec Installer / Nouveautés… / Plus tard / Ignorer cette version.
- [ ] **U-02 [M]** « Nouveautés… » : la fenêtre « Mise à jour » affiche les **notes de version** (celles du CHANGELOG) ; le lien « Page de la release sur GitHub » s'ouvre dans **ton navigateur**.
- [ ] **U-03 [B]** « Installer » : barre de progression en Mo, puis bilan et bouton **« Redémarrer maintenant »**. Avant de redémarrer, le dossier contient `ComicRust-Tauri.exe.old`, `ComicRust-Tauri-mcp.exe.old`, `pdfium.dll.old`.
- [ ] **U-04 [B]** « Redémarrer maintenant » : l'ancienne fenêtre se ferme, la nouvelle s'ouvre seule (en quelques secondes) **avec ta bibliothèque, tes réglages et tes listes** ; plus aucun fichier `.old` dans le dossier.
- [ ] **U-05 [B]** `data\` intact : réglages (thème, tri, largeurs), `data\comicrust-tauri.json`, la bibliothèque et l'avancement de lecture sont les mêmes qu'avant.
- [ ] **U-06 [M]** « Ignorer cette version » puis relancer par `MAJ.cmd` : plus de bandeau. Réglages ▸ « Rechercher une mise à jour… » la propose quand même (recherche manuelle).
- [ ] **U-07 [M]** Fenêtre « Mise à jour » : décocher « Vérifier les mises à jour au démarrage », relancer par `MAJ.cmd` : pas de bandeau ; recocher : le bandeau revient.
- [ ] **U-08 [M]** Hors ligne (couper le réseau) : au démarrage, rien ne s'affiche ; « Rechercher une mise à jour… » donne un **message d'erreur lisible**, sans plantage.
- [ ] **U-09 [m]** « Plus tard » : le bandeau disparaît jusqu'au prochain lancement.
- [ ] **U-10 [M]** Copie liée lancée en MCP (`ComicRust-Tauri-mcp.exe --mcp=47803` avec `COMICRUST_SIMULER_VERSION=0.7.3`) : mise à jour puis redémarrage → l'appli revient **en mode MCP sur le même port** ; après avoir **relancé Claude Desktop**, l'outil `infos` répond avec la version 0.8.0.

## 3. Correction C-05

- [ ] **C-01 [m]** Fermer un premier livre, en ouvrir un second, fermer ; resélectionner le premier : son Détail affiche sa page lue. *(Scénario précisé par Yoyo après publication ; la vraie cause — reprise d'un livre déjà lu — est corrigée en [tauri-v0.8.1](tauri-v0.8.1.md) : ce test peut échouer ici.)*

## 4. Non-régression

- [ ] **N-01 [B]** Bibliothèque (empilée ou non), listes, recherche, tri : comme à la tauri-v0.7.3.
- [ ] **N-02 [B]** Lecteur : CBZ et PDF, fermeture par Échap (Détail à jour), relecture d'un livre lu.
- [ ] **N-03 [M]** Clic droit → « Modifier les infos » ; poignées au clavier.
- [ ] **N-04 [m]** Copie liée : pas de bandeau « Base injoignable » ; réglages conservés.

## Limites connues (acceptées pour cette version)

- **Pas de retour arrière global** : si un fichier est verrouillé au milieu de l'installation (README ouvert dans un éditeur, antivirus), les fichiers déjà copiés restent dans la nouvelle version (le fichier en échec est restauré, un message le signale). Fermer ce qui ouvre ces fichiers et relancer l'installation. Même comportement que la version egui.
- `COMICRUST_MAJ_URL` (faux serveur) et `COMICRUST_SIMULER_VERSION` restent actives dans la version livrée : elles servent à la recette ; ne pas les définir dans l'environnement de tous les jours.
- Après une mise à jour, **relancer Claude Desktop** : le pont MCP qu'il a lancé reste sur l'ancien programme.
- Les liens des notes de version qui ne pointent pas vers le dépôt ComicRust sont affichés en texte (avec leur adresse), non cliquables.
- Reprises : piles non réordonnables par glisser ; Détail vide pour une pile ; « lu dans ComicRust » visible seulement dans le Détail ; pas d'écran « Changer de connexion ».

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| C-01 | [m] | Détail du premier livre (déjà lu) non à jour après fermeture puis ouverture d'un second | tauri-v0.8.1 (reprise d'un livre lu comptée « terminée ») |
| (relecture) | [M] | Une vérification lancée après une installation terminée reproposait la même release et pouvait tout réinstaller | **Corrigé** (aucune vérification tant que le redémarrage n'est pas fait) |
| (relecture) | [m] | Release retirée de GitHub encore installable ; rc préférée à la finale ; tags malformés acceptés ; virgule dans une adresse passée à explorer.exe ; nom de release affiché en double ; clic sans effet sur un lien externe | **Corrigés** |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | 7 | 7 | 0 |
| [M] majeurs | 7 | 7 | 0 |
| [m] mineurs | 3 | 2 | 1 (C-01) |

Rapport de Yoyo du 12/11/2026 21:14 ([rapport](rapports/tauri-v0.8.0-2026-11-12.txt)) : 6 OK, 1 échec (C-01 [m], attendu : cause corrigée en tauri-v0.8.1), 10 non testés (toute la section « Mise à jour », dont 4 bloquants).

Second rapport du 12/11/2026 21:21 ([rapport](rapports/tauri-v0.8.0-2026-11-12-b.txt)) : 16 OK, 1 échec (C-01 [m], attendu) ; section « Mise à jour » faite avec `MAJ.cmd` (copie « ComicRust-Tauri MAJ »).

**Décision :** GO proposé (seul un mineur en échec, corrigé en tauri-v0.8.1) — en attente de la confirmation de Yoyo.
