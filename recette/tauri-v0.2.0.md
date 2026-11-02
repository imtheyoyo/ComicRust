# Recette ComicRust Tauri tauri-v0.2.0

| | |
|---|---|
| Version | tauri-v0.2.0 (pré-release : vue Bibliothèque de la version Tauri 2, sans lecteur) |
| Paquet | `ComicRust-Tauri-tauri-v0.2.0-windows-x64.zip` |
| SHA-256 | `af1b713f0ab4d102bd57b72a9005707200064ba1b0033224b152e748a5098cd0` |
| Contenu | **premier démarrage** (autonome ou lié à ComicRack), **vue Bibliothèque** (grille virtualisée, tableau, recherche, tri, groupement, panneau de détails), **dossiers surveillés** (scan, ajout, mise à jour, manquants), couvertures en cache (`data\couvertures`) servies par le protocole `cr-couv` |
| Origine | étape 1 de `docs/tauri/PLAN.md` ; tauri-v0.1.0 ; tests L-/C- des recettes v0.3.0 à v0.5.x et v0.7.0 (version egui) ; contrat `docs/tauri/ipc-bibliotheque.md` |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> **ComicRack n'est jamais modifié** : en mode « lié », la base (MySQL ou `ComicDb.xml`) est lue
> en lecture seule, rien n'est copié ni écrit. Le mot de passe MySQL reste dans `ComicRack.ini`.
> Pour le mode autonome, dézipper le paquet dans un **dossier vide** (la copie « liée » se teste
> dans un second dossier, pour que les deux modes ne se mélangent pas).
> Les chemins (`ComicRack.ini`, dossiers surveillés) se **saisissent à la main** : il n'y a pas encore de sélecteur de fichiers.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| 22 tests du backend `comicrust-tauri` : réglages (aller-retour, fichier corrompu), premier démarrage et choix autonome persistant, mode ComicRack avec `ComicDb.xml` en lecture seule, MySQL injoignable sans rien enregistrer, dossiers surveillés, scan + liste + détail + couverture + manquants, **5 000 livres fluides**, contrat JSON en camelCase, événements de scan, URL de couverture, cache des couvertures, recherche par mots, tri naturel des numéros, valeurs vides en dernier, groupes, clefs de tri et synonymes, thèmes, portable (sous-dossiers de `data\`) | `cargo test` (`comicrust-tauri`) | ✅ 22/22 |
| Front : `svelte-check` (0 erreur), `vite build` | CI Windows (`ci-tauri.yml`) | ✅ |
| Clippy `-D warnings`, compilation de l'application complète (`--features app,custom-protocol`) | CI Windows (`ci-tauri.yml`) | ✅ verte |
| Tests et clippy de la version egui (inchangée) | `ci.yml` Linux + Windows | ✅ |

**Non vérifiable ici** : tout ce qui touche à la fenêtre et au WebView2 (affichage, protocole
`cr-couv`, thèmes, fluidité), à la vraie bibliothèque ComicRack (MySQL / `ComicDb.xml`) et au
mode portable sous Windows (sections 1 à 12).

## 1. Préparation

- [ ] **P-01 [B]** Décompresser le zip dans un dossier **vide** (ex. `D:\Tauri-autonome`), lancer `ComicRust-Tauri.exe` : la fenêtre s'ouvre et affiche l'assistant « Bienvenue dans ComicRust » (aucune bibliothèque n'est ouverte avant le choix).
- [ ] **P-02 [m]** Préparer un petit dossier d'essai (ex. `recette\echantillons` du dépôt, copié sur le disque) pour les sections 3 à 8, et noter le chemin : ………

## 2. Premier démarrage

- [ ] **D-01 [B]** Choisir **Bibliothèque autonome** puis « Continuer » : la vue Bibliothèque s'ouvre, vide (« La bibliothèque est vide : ajoutez un dossier surveillé, puis lancez un scan. »), le bas de la barre latérale indique **Autonome**. Le fichier `data\comicrust.db` et `data\comicrust-tauri.json` existent.
- [ ] **D-02 [B]** Fermer puis relancer : l'assistant ne réapparaît pas, la bibliothèque s'ouvre directement, mode **Autonome**.
- [ ] **D-03 [B]** Dans un second dossier (copie du zip), choisir **Lié à ComicRack**, saisir le chemin du `ComicRack.ini` de ton installation (MySQL) : « Continuer » ouvre ta bibliothèque ; le nombre de livres correspond à ComicRack (5 323 attendus sur ta base) ; la barre latérale indique **Lié à ComicRack**. Nombre affiché : ………
- [ ] **D-04 [M]** Même copie, relancée : la bibliothèque liée est rouverte sans question. Rien n'a été écrit dans ComicRack (ouvrir ComicRack en parallèle : fonctionne normalement, bibliothèque intacte après fermeture des deux).
- [ ] **D-05 [M]** Mode lié avec un `ComicDb.xml` (saisir son chemin à la place du `.ini`, ou une installation sans MySQL) : les livres du fichier s'affichent.
- [ ] **D-06 [M]** Mode lié avec un chemin faux, ou MySQL arrêté : message d'erreur **lisible en français** sous le formulaire (sans mot de passe ni trace technique), l'assistant reste utilisable ; corriger le chemin permet de continuer.
- [ ] **D-07 [m]** Mode lié : « Continuer » reste grisé tant que le chemin est vide ; le mode autonome n'exige rien.
- [ ] **D-08 [m]** Mode lié : le menu « Dossiers surveillés » ne permet pas d'ajouter de dossier (message explicatif) et le scan est refusé : la bibliothèque ComicRack reste en lecture seule.

## 3. Dossiers surveillés et scan

- [ ] **S-01 [B]** Mode autonome : « 📁 Dossiers surveillés » → saisir à la main le chemin du dossier d'essai → « Ajouter » : il apparaît dans la liste. Un chemin inexistant est refusé avec un message lisible ; ajouter deux fois le même dossier ne crée pas de doublon.
- [ ] **S-02 [B]** « Lancer le scan » : la barre de progression avance (la fenêtre reste réactive), puis le pied de page affiche « Scan terminé : N ajoutés, 0 mis à jour, 0 manquants » ; les livres apparaissent dans la vue. Noter N et la durée : ……… / ……… s.
- [ ] **S-03 [B]** Relancer le scan sans rien changer : 0 ajouté, 0 manquant (pas de doublons dans la liste).
- [ ] **S-04 [B]** Ajouter un fichier BD dans le dossier, scanner : « 1 ajouté ». Modifier un fichier (le renommer n'en est pas un ; en remplacer le contenu) puis scanner : il est compté « mis à jour ».
- [ ] **S-05 [B]** Supprimer un fichier du dossier, scanner : « 1 manquant » ; le livre **reste dans la liste**, sa couverture s'affiche encore (cache disque), et son détail indique que le fichier est introuvable.
- [ ] **S-06 [M]** Remettre le fichier supprimé, scanner : il n'est plus manquant.
- [ ] **S-07 [M]** « Retirer » un dossier : il disparaît de la liste ; les livres déjà en base restent affichés ; après relance, le dossier n'est pas revenu.
- [ ] **S-08 [M]** Scan en cours : un second « Lancer le scan » est refusé (bouton grisé ou message) ; sans dossier, le scan est refusé avec un message.
- [ ] **S-09 [m]** Fermer la boîte « Dossiers surveillés » pendant un scan : la progression continue dans le pied de page.
- [ ] **S-10 [m]** Dossier sur le NAS (`Z:\…` ou chemin UNC saisi à la main) : ajouté et scanné ; si le réseau est coupé, message d'échec clair (« Échec du scan : … ») sans plantage.

## 4. Grille et tableau

- [ ] **G-01 [B]** Vue **▦ Grille** : une couverture par livre, avec le titre dessous ; le défilement est continu, sans trou ni clignotement ; le curseur « Taille » agrandit et réduit les couvertures et la grille se réorganise.
- [ ] **G-02 [B]** Vue **☰ Tableau** : colonnes Titre, Série, N°, Éditeur, Année, Pages, Lu ; un clic sur l'en-tête **Titre / Série / Année / Pages** trie la colonne, un second clic inverse le sens ; une colonne non triable (N°, Éditeur, Lu) ne fait rien.
- [ ] **G-03 [B]** Basculer Grille ↔ Tableau : la sélection est conservée (même livre), le choix de vue est mémorisé au redémarrage (ainsi que la taille et l'affichage du panneau de détails).
- [ ] **G-04 [M]** Les livres 100 % lus portent la coche ✓ (grille) ; la colonne « Lu » du tableau est cohérente avec ComicRack (comparer sur 3 livres lus et 3 non lus).
- [ ] **G-05 [m]** Lignes du tableau et légendes tronquées par « … » avec l'info-bulle du titre complet au survol ; aucun débordement quand on rétrécit la fenêtre à 800 px.

## 5. Recherche, tri, groupement

- [ ] **R-01 [B]** Zone de recherche : taper un mot de série filtre la liste en direct (sans geler la saisie) ; le compteur du pied de page indique « N livres (filtré) ». Noter pour « thanos » : ……… livres (comparer avec ComicRack).
- [ ] **R-02 [B]** Plusieurs mots (« thanos marvel ») : tous sont requis ; l'ordre n'importe pas ; la recherche porte aussi sur titre, auteurs, éditeur et nom de fichier ; accents et casse sans effet.
- [ ] **R-03 [B]** Échap dans la zone de recherche : vide la recherche et rend la liste complète ; une recherche sans résultat affiche « Aucun livre ne correspond à la recherche. ».
- [ ] **R-04 [B]** Menu **Tri** : essayer chaque valeur (Série, Titre, Éditeur, Parution, Ajout, Lecture, Note, Pages, Avancement, Taille, Fichier) : la liste se réordonne, le bouton ↑/↓ inverse le sens ; les valeurs vides sont **toujours en dernier**.
- [ ] **R-05 [B]** Tri « Série » : numéros en ordre **naturel** (Thanos #2 avant #10), séries côte à côte.
- [ ] **R-06 [M]** Menu **Groupe** : « Série » puis « Éditeur » : les livres d'un même groupe se suivent (groupes par ordre alphabétique, inversé en tri décroissant, « (sans série) » / « (sans éditeur) » en dernier ou pour les vides) ; le tri joue à l'intérieur de chaque groupe ; un repère de groupe (nom) apparaît sur le premier livre. « Aucun » revient à la liste simple.
- [ ] **R-07 [M]** Recherche + tri + groupe combinés : le résultat reste cohérent (le compteur et la sélection sont recalculés, aucun doublon).
- [ ] **R-08 [m]** Les réglages de tri et de groupe sont conservés d'un lancement à l'autre (ou reviennent proprement aux valeurs par défaut) : noter le comportement : ………

## 6. Panneau de détails

- [ ] **T-01 [B]** Un clic sur un livre l'affiche dans le panneau de détails (couverture, titre, série, numéro, éditeur, année, pages, auteurs, résumé, genre, etc.) ; seuls les champs renseignés apparaissent.
- [ ] **T-02 [B]** Comparer sur trois livres avec ComicRack : série, numéro, année, éditeur, pages, scénariste, dessinateur identiques ; progression et « lu » cohérents.
- [ ] **T-03 [M]** Le chemin du fichier est affiché (info-bulle = chemin complet) ; un fichier disparu est signalé comme introuvable / manquant.
- [ ] **T-04 [M]** Bouton **Détails** (barre d'outils) : masque / affiche le panneau ; la grille se redimensionne sans saut.
- [ ] **T-05 [m]** Le bouton « Lire » du panneau (ou double-clic / Entrée) affiche seulement un message « Lecteur : disponible à l'étape 2 » : aucun plantage (le lecteur n'existe pas encore, voir limites connues).
- [ ] **T-06 [m]** Passer d'un livre à l'autre très vite (flèches maintenues) : le panneau suit sans afficher le livre précédent (pas d'ancien détail qui revient en retard).

## 7. Clavier et souris

- [ ] **K-01 [B]** Grille ou tableau pris au clavier (Tab, ou ↓ depuis la zone de recherche) : flèches ←/→/↑/↓ déplacent la sélection (←/→ sans effet dans le tableau), **Début / Fin** vont au premier / dernier livre, **Page préc. / Page suiv.** avancent d'une page ; la ligne sélectionnée reste visible (défilement automatique).
- [ ] **K-02 [B]** Entrée = ouvre (message du lecteur à venir) ; double-clic = idem ; simple clic = sélectionne et affiche les détails.
- [ ] **K-03 [M]** Raccourci **/** (hors saisie) ou **Ctrl+F** : le focus passe à la zone de recherche et sélectionne son texte ; **Entrée** valide aussitôt ; **↓** valide et rend le focus à la liste.
- [ ] **K-04 [M]** Molette et barre de défilement : défilement fluide dans la grille et dans le tableau ; saut direct en tirant l'ascenseur jusqu'en bas (le dernier livre s'affiche).
- [ ] **K-05 [m]** Le texte du pied de page indique le rang de la sélection (« sélection 123 ») et suit les déplacements.

## 8. Couvertures sous WebView2

- [ ] **C-01 [B]** Les couvertures de la grille s'affichent (pas d'icône « image cassée »), y compris pour CBZ, CBR, CB7, PDF et dossier d'images des échantillons ; à la première ouverture elles arrivent progressivement sans bloquer la fenêtre.
- [ ] **C-02 [B]** `data\couvertures\` contient des `<id>.jpg` (miniatures d'environ 300 px de haut) ; après relance, les couvertures reviennent **instantanément** depuis le cache.
- [ ] **C-03 [B]** Aucune requête réseau externe pour les couvertures ; pas de base64 volumineux : l'affichage reste fluide (comparer avec ta version egui).
- [ ] **C-04 [M]** Couverture identique à celle de ComicRack sur trois livres ; pour un livre dont la page de couverture est définie ailleurs qu'en première page dans ComicRack (type « Couverture »), la couverture affichée est la bonne. Noter les livres testés : ………
- [ ] **C-05 [M]** Fichier illisible, tronqué ou absent (échantillons « tronqué », fichier supprimé sans cache) : visuel de remplacement (pas d'image cassée, pas d'erreur bloquante) ; une couverture déjà en cache reste servie même si le fichier a disparu.
- [ ] **C-06 [m]** Supprimer `data\couvertures\` puis relancer : les couvertures sont régénérées à la demande.

## 9. Mode portable (rien hors de `data\`)

- [ ] **M-01 [B]** Après les sections précédentes, à côté de l'exe le dossier `data\` contient `comicrust-tauri.json`, `comicrust.db` (autonome), `couvertures\`, `themes\` et `webview2\`. **Aucun** dossier `ComicRust` ni `fr.imtheyoyo.comicrust` dans `%APPDATA%` ni `%LOCALAPPDATA%` ; aucune clé de registre `ComicRust` (HKCU\Software).
- [ ] **M-02 [B]** Dans le mode lié, `data\` ne contient **aucune copie** de la bibliothèque ComicRack ni de mot de passe : ouvrir `comicrust-tauri.json` (seulement `mode` et `cheminIni`) et rechercher le mot de passe MySQL dans `data\` : introuvable.
- [ ] **M-03 [M]** Déplacer tout le dossier (autre disque, clé USB) : relance sans rien perdre (mode, dossiers surveillés, livres, couvertures, réglages d'affichage).
- [ ] **M-04 [m]** Zip dézippé dans un dossier en lecture seule : message d'erreur clair, pas de plantage silencieux.

## 10. Volume : 5 000 livres

- [ ] **V-01 [B]** Sur la bibliothèque liée (≈ 5 300 livres) : le démarrage jusqu'à la liste affichée prend quelques secondes ; noter la durée : ……… s.
- [ ] **V-02 [B]** Défilement maintenu (molette, ascenseur, Page suiv. maintenue) en grille et en tableau : fluide, sans gel de la fenêtre ni saccade marquée ; aucune zone blanche prolongée.
- [ ] **V-03 [B]** Recherche en direct sur la bibliothèque complète : chaque frappe met à jour la liste en moins d'une seconde environ, sans bloquer la saisie ; tri et groupe : changement en moins de 2 s. Noter les durées perçues : ……… s.
- [ ] **V-04 [M]** Mémoire : l'application reste raisonnable après dix minutes de défilement et de recherches (Gestionnaire des tâches : noter ComicRust + WebView2) : ……… Mo.
- [ ] **V-05 [M]** Scan d'un gros dossier du NAS en mode autonome (plusieurs milliers de fichiers) : la fenêtre reste réactive pendant le scan et la progression avance ; noter nombre / durée : ……… / ……… s.

## 11. Thèmes

- [ ] **H-01 [B]** Menu ⚙ **Réglages** → Apparence : Moderne clair, Moderne sombre, Classique s'appliquent immédiatement à **toute** la vue Bibliothèque (barre latérale, barre d'outils, grille, tableau, panneau de détails, boîtes de dialogue, pied de page) ; tout reste lisible (textes, sélection, bordures).
- [ ] **H-02 [B]** Le thème choisi est repris au redémarrage.
- [ ] **H-03 [M]** Un thème perso copié dans `data\themes\` apparaît avec « (perso) » après « Actualiser la liste » et s'applique à toutes les vues, sans redémarrer.
- [ ] **H-04 [m]** La sélection d'un livre, le survol et le focus clavier sont visibles dans les trois thèmes.

## 12. Non-régression (tauri-v0.1.0 et version egui)

- [ ] **N-01 [B]** tauri-v0.1.0 T-02 : mode portable respecté (voir M-01) ; `data\themes\` contient les 3 thèmes livrés.
- [ ] **N-02 [B]** tauri-v0.1.0 T-03 : changement de thème immédiat, thème repris au redémarrage.
- [ ] **N-03 [M]** tauri-v0.1.0 T-04 / T-05 / T-06 : thème perso ajouté sans recompiler ; thème perso supprimé alors qu'il est mémorisé → retour à Moderne clair sans erreur bloquante ; thème livré supprimé → recréé à « Actualiser la liste ».
- [ ] **N-04 [m]** tauri-v0.1.0 T-08 : la fenêtre ne rétrécit pas sous 800 x 500 ; la page reste lisible.
- [ ] **N-05 [B]** La version egui (`ComicRust.exe` de la dernière release `vX.Y.Z`) n'est pas touchée : même `data\comicrust.db` ouvert par les deux versions (pas en même temps) sans corruption ni perte de livres.

---

## Limites connues (ne pas compter comme anomalies)

- **Pas de lecteur** : « Lire », double-clic et Entrée affichent seulement un message ; le lecteur arrive à l'étape 2 (`tauri-v0.3.0`).
- **Pas de sélection multiple** : un seul livre est sélectionné à la fois ; pas d'actions groupées.
- **Groupes sans vraies sections** : le groupement regroupe et ordonne les livres, mais sans en-têtes de section repliables ; seul un repère de nom apparaît sur le premier livre de chaque groupe.
- **Chemins saisis à la main** : `ComicRack.ini` / `ComicDb.xml` et les dossiers surveillés se tapent ; pas de sélecteur de fichiers ni de dossiers.
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
| Bloquants [B] | 34 | | |
| Majeurs [M] | 20 | | |
| Mineurs [m] | 14 | | |

Décision et remarques :
