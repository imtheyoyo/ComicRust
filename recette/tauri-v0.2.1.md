# Recette ComicRust Tauri tauri-v0.2.1

| | |
|---|---|
| Version | tauri-v0.2.1 (pré-release corrective de la vue Bibliothèque de la version Tauri 2, sans lecteur) |
| Paquet | `ComicRust-Tauri-tauri-v0.2.1-windows-x64.zip` |
| SHA-256 | `59812b095fb4318343b7b76891e87d834876969c7a0a6fc25a9516740ccbcc7c` |
| Contenu | **recherche fidèle à ComicRack** (R-01), **préférences d'affichage mémorisées** dans `data\comicrust-tauri.json` (R-08, G-03), **poignée** de largeur du panneau de détails ; reprise des tests non testés (D-01, G-03, V-05) et D-05 reformulé |
| Origine | rapport de recette [tauri-v0.2.0 du 02/11/2026](rapports/tauri-v0.2.0-2026-11-02.txt) : **NO-GO** (R-01 [B] en échec : « thanos » 130 livres contre 483 dans ComicRack) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> **ComicRack n'est jamais modifié** : en mode « lié », la base (MySQL ou `ComicDb.xml`) est lue
> en lecture seule. Comme pour la tauri-v0.2.0, dézipper le paquet dans un **dossier vide** pour
> le mode autonome et dans un **second dossier** pour le mode lié.
>
> **Valeurs de référence notées en tauri-v0.2.0** : D-03 **5 324** livres ; S-02 **10** livres en
> **15 s** ; V-05 **4 944** fichiers en **60 s** ; V-04 **52** Mo ; « thanos » dans ComicRack : **483**.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| 33 tests du backend `comicrust-tauri` (22 de la tauri-v0.2.0 + 11) : recherche reproduisant la boîte de ComicRack en mode « Tout » (tous les champs, dont résumé, notes, personnages, lieux, infos de scan ; tous les mots dans un même champ ; découpage sur `,` / `;` sinon sur les espaces ; casse ignorée, accents non ignorés), préférences d'affichage (aller-retour, fusion des champs, valeurs inconnues ou mal typées ignorées champ par champ, nombres bornés), premier démarrage qui conserve les préférences et inversement | `cargo test` (`comicrust-tauri`) | ✅ 33/33 |
| Front : `svelte-check` (0 erreur, 0 avertissement), `vite build` | local + CI Windows (`ci-tauri.yml`) | ✅ |
| Parcours simulé dans un navigateur (Playwright, mode simulé sans Tauri) : préférences rechargées après un redémarrage simulé (`?prefs=`), poignée du panneau de détails | Playwright (mode simulé) | ✅ |
| Clippy `-D warnings`, compilation de l'application complète (`--features app,custom-protocol`) | CI Windows (`ci-tauri.yml`) | ✅ verte |

**Non vérifiable ici** : la vraie fenêtre WebView2, le fichier `data\comicrust-tauri.json` écrit par
l'exe sous Windows, la vraie bibliothèque ComicRack (comparaison des résultats de recherche) et le
NAS (sections 1 à 5).

## 1. Préparation et premier démarrage

- [ ] **P-01 [B]** Décompresser le zip dans un dossier **vide**, lancer `ComicRust-Tauri.exe` : la fenêtre s'ouvre sur l'assistant « Bienvenue dans ComicRust ».
- [ ] **D-01 [B]** *(non testé en tauri-v0.2.0)* Choisir **Bibliothèque autonome** puis « Continuer » : la vue Bibliothèque s'ouvre, vide (« La bibliothèque est vide : ajoutez un dossier surveillé, puis lancez un scan. »), le bas de la barre latérale indique **Autonome**. `data\comicrust.db` et `data\comicrust-tauri.json` existent. Fermer et relancer : l'assistant ne réapparaît pas.
- [ ] **D-03 [B]** Dans le second dossier, mode **Lié à ComicRack** avec ton `ComicRack.ini` (MySQL) : la bibliothèque s'ouvre avec le même nombre de livres qu'en tauri-v0.2.0 (**5 324**). Nombre affiché : ………
- [ ] **D-05 [M]** *(reformulé)* Sur ton installation ComicRack **MySQL**, désigner `…\ComicRack\Data\ComicDb.xml` à la place du `.ini` : le message clair « cette bibliothèque ComicRack ne contient aucun livre (ComicRack utilise sans doute une base MySQL : indique son ComicRack.ini) » s'affiche sous le formulaire, l'assistant reste utilisable et repasser au `.ini` permet de continuer. C'est le comportement attendu : ce fichier est vide quand ComicRack travaille avec MySQL (tu n'utilises pas `ComicDb.xml`).

## 2. Recherche fidèle à ComicRack (correction de R-01)

La zone de recherche reproduit désormais la recherche rapide de ComicRack en mode **« Tout »** :
elle porte sur **tous les champs** (série, titre, auteurs, éditeur, mais aussi résumé, notes,
personnages, personnage principal, équipes, lieux, arc, groupe de séries, étiquettes, infos de scan,
nom de fichier…), **tous les mots doivent se trouver dans un même champ**, la virgule ou le
point-virgule séparent les termes (sinon les espaces), la casse est ignorée mais **pas les accents**
(comme ComicRack). Comparer chaque fois avec la boîte de recherche de ComicRack (mode « Tout ») sur
la bibliothèque liée.

- [ ] **R-01 [B]** Bibliothèque liée, taper « thanos » : la liste se filtre en direct sans geler la saisie ; le pied de page indique « N livres (filtré) ». **Attendu : identique ou très proche de 483** (valeur ComicRack). Noter ComicRust / ComicRack : ……… / ……… livres.
- [ ] **R-02 [B]** Un mot qui n'apparaît que dans le **résumé**, les **notes**, les **personnages**, les **lieux** ou les **infos de scan** d'un livre (ex. un nom de personnage secondaire, un nom de scanneur, une ville) : le livre est trouvé, comme dans ComicRack. Noter les mots essayés et les nombres ComicRust / ComicRack : ………
- [ ] **R-03 [B]** Plusieurs mots (ex. « thanos infinity ») : seuls les livres dont **un même champ** contient tous les mots sont gardés ; l'ordre des mots n'importe pas. Le nombre peut être **inférieur à celui de la tauri-v0.2.0** (où les mots pouvaient être dans des champs différents) : c'est voulu, comparer avec ComicRack. Noter ComicRust / ComicRack : ……… / ………
- [ ] **R-04 [M]** Virgule ou point-virgule : « thanos, infinity » (ou « thanos; infinity ») donne le même résultat que dans ComicRack ; une expression avec espaces entre deux virgules (« infinity gauntlet, thanos ») est cherchée comme un seul terme. Noter : ………
- [ ] **R-05 [M]** Casse sans effet (« THANOS » = « thanos ») ; accents **non** ignorés, comme ComicRack : « heros » et « héros » (ou « eté » / « été ») donnent des résultats différents, chacun égal à celui de ComicRack.
- [ ] **R-06 [B]** Échap vide la recherche et rend la liste complète ; une recherche sans résultat affiche « Aucun livre ne correspond à la recherche. » ; sur la bibliothèque complète, chaque frappe met à jour la liste en environ une seconde au plus (référence V-03 : 1 s).
- [ ] **R-07 [M]** Recherche + tri + groupe combinés : compteur et sélection recalculés, aucun doublon (non-régression R-07).

## 3. Préférences d'affichage mémorisées (correction de R-08 et G-03)

Les réglages d'affichage sont enregistrés dans la section `vue` de `data\comicrust-tauri.json`
(quelques instants après chaque changement et à la fermeture), **plus dans le stockage local du
WebView2**.

- [ ] **A-01 [B]** Mode autonome : changer le **tri** (ex. Éditeur), le **sens** (↓), le **groupe** (Série), la **vue** (Tableau), la **taille** des couvertures, masquer puis réafficher le **panneau de détails**, élargir le panneau, choisir le **thème** Moderne sombre. Fermer, relancer : **tout** revient tel quel.
- [ ] **A-02 [B]** Ouvrir `data\comicrust-tauri.json` : une section `"vue"` contient le tri, le sens, le groupe, la vue, la taille, l'affichage et la largeur du panneau de détails et le thème choisis ; `mode` (et `cheminIni` en mode lié) sont inchangés.
- [ ] **A-03 [B]** Preuve que rien ne vient du `localStorage` : application fermée, supprimer la section `"vue"` du fichier (garder `mode`), relancer : l'affichage revient **aux valeurs par défaut** (Grille, tri Série ↑, aucun groupe, thème Moderne clair). Option : dans `data\webview2\`, aucun fichier `Local Storage` ne contient de clé `comicrust.`.
- [ ] **A-04 [M]** Nouvelle **poignée** entre la liste et le panneau de détails : la tirer élargit ou réduit le panneau (largeur bornée, la liste reste utilisable) ; prise au clavier (Tab), les flèches ←/→ la déplacent ; la largeur est reprise au redémarrage.
- [ ] **A-05 [M]** Mode lié (second dossier) : les préférences sont mémorisées de la même façon, indépendamment de la copie autonome ; `comicrust-tauri.json` ne contient toujours **ni copie de la bibliothèque ni mot de passe** (non-régression M-02).
- [ ] **A-06 [m]** Application fermée, mettre une valeur absurde dans la section `vue` (ex. `"tri": "n_importe_quoi"`, `"zoom": 99999`) : au lancement, aucune erreur bloquante ; la valeur fautive revient à sa valeur par défaut (ou est bornée), les autres réglages sont conservés.
- [ ] **A-07 [m]** Mise à jour : copier le nouvel exe sur la copie tauri-v0.2.0 existante (dossier `data\` conservé) : mode, livres et couvertures sont conservés ; la vue et la taille choisies en tauri-v0.2.0 sont reprises une fois puis enregistrées dans `comicrust-tauri.json`.
- [ ] **G-03 [B]** *(non testé en tauri-v0.2.0)* Basculer Grille ↔ Tableau : la **sélection est conservée** (même livre) ; le choix de vue, la taille et l'affichage du panneau de détails sont mémorisés au redémarrage.

## 4. Volume (repris)

- [ ] **V-05 [M]** *(non testé en tauri-v0.2.0)* Scan d'un gros dossier du NAS en mode autonome (plusieurs milliers de fichiers) : la fenêtre reste réactive pendant le scan (défilement, recherche) et la progression avance ; résultat comparable à la référence (**4 944** fichiers / **60 s**). Noter nombre / durée : ……… / ……… s.
- [ ] **V-04 [M]** Mémoire après dix minutes de défilement et de recherches (ComicRust + WebView2) : du même ordre que la référence (**52** Mo). Noter : ……… Mo.

## 5. Non-régression courte (tauri-v0.2.0)

- [ ] **N-01 [B]** Grille : couvertures affichées (pas d'image cassée), défilement continu sans trou ; tableau trié par clic sur les en-têtes ; tri Série en ordre naturel (#2 avant #10).
- [ ] **N-02 [B]** Couvertures : `data\couvertures\` contient des `<id>.jpg` ; après relance elles reviennent instantanément depuis le cache.
- [ ] **N-03 [B]** Scan du dossier d'essai : « Scan terminé : N ajoutés… » comparable à la référence (**10** livres / **15 s**) ; un second scan sans changement : 0 ajouté, 0 manquant.
- [ ] **N-04 [B]** Mode portable : `data\` contient `comicrust-tauri.json`, `couvertures\`, `themes\` et **`webview2\`** (profil WebView2) ; **aucun** dossier `ComicRust` ni `fr.imtheyoyo.comicrust` dans `%APPDATA%` ni `%LOCALAPPDATA%`.
- [ ] **N-05 [M]** Panneau de détails : un clic sur un livre affiche ses informations ; le bouton **Détails** masque / affiche le panneau sans saut de la grille.

---

## Limites connues (ne pas compter comme anomalies)

- **Recherche** : seule la recherche rapide en mode « Tout » est reproduite ; pas de choix du champ
  (menu Série / Titre / Auteurs… de ComicRack), ni de recherche avancée (`NOT`, `MATCH`, expressions
  régulières, `[champ]`).
- **Pas de lecteur** : « Lire », double-clic et Entrée affichent seulement un message ; le lecteur arrive à l'étape 2 (`tauri-v0.3.0`).
- **Pas de sélection multiple** : un seul livre est sélectionné à la fois ; pas d'actions groupées.
- **Groupes sans vraies sections** : seul un repère de nom apparaît sur le premier livre de chaque groupe.
- **Chemins saisis à la main** : `ComicRack.ini` / `ComicDb.xml` et les dossiers surveillés se tapent ; pas de sélecteur de fichiers ni de dossiers.
- `ComicDb.xml` d'une installation **MySQL** est vide : le mode lié demande alors le `ComicRack.ini` (voir D-05).
- Les sections « Séries » et « Listes » de la barre latérale sont des écrans d'attente (listes ComicRack : étape 5).
- Mode lié : lecture seule, pas de dossiers surveillés ni de scan, pas d'écriture dans ComicRack (étapes 3 et 6).
- Pas de modification des infos, des pages ni de la lecture (étapes 3, 4 et suivantes).
- WebView2 doit être présent (Windows 11 : intégré). Il n'est pas embarqué dans le zip.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 15 | | |
| Majeurs [M] | 9 | | |
| Mineurs [m] | 2 | | |

Décision et remarques :
