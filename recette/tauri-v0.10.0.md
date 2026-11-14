# Recette ComicRust Tauri tauri-v0.10.0

| | |
|---|---|
| Version | tauri-v0.10.0 (pré-release de l'étape **thèmes**) |
| Paquet | `ComicRust-Tauri-tauri-v0.10.0-windows-x64.zip` |
| SHA-256 | `88bb1acb815508d52aa5e5e9565e6a103a779ef4c2da00cd7ef8705998cb5038` |
| Contenu | thèmes (contrat `--cr-*`, choix/aperçu/import/export/rechargement, filtrage du CSS) ; sélection par rectangle avec les piles |
| Origine | [recette tauri-v0.9.1](tauri-v0.9.1.md) : **GO** du 14/11/2026 ; remarque « rectangle avec Empiler par série » ([rapport](rapports/tauri-v0.9.1-2026-11-14.txt)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Installation par la mise à jour intégrée de ta 0.9.1 (copies autonome et liée). Les essais de thèmes
> créent des fichiers dans `data\themes` : sans risque pour la bibliothèque. Guide : `docs/tauri/themes.md`.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| Tests du backend `comicrust-tauri` dont le **filtrage du CSS** (url, `@import`, échappements, CRLF, commentaires, svg refusé…), le contrat de variables (toute variable `--cr-*` du front est au contrat), les **contrastes WCAG** des 3 thèmes, import/export/suppression/réinitialisation, noms de fichiers sûrs, mise à jour des thèmes livrés sans toucher un thème modifié | `cargo test` | ✅ |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours Playwright simulé avec un thème très contrasté sur toutes les vues, aperçu/annuler, CSS invalide, `@import` ; rectangle sur piles, menu, glisser, ouvrir/fermer une pile | local | ✅ |
| Relecture indépendante (filtrage CSS, fichiers, `app.rs`, front, sélection des piles) : défauts corrigés avant publication (voir « Anomalies ») | | ✅ |
| Rustfmt, Clippy `-D warnings` ; compilation de la fenêtre et variante `-mcp` | local + CI Windows | ✅ local ; CI à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2, l'ouverture de l'Explorateur Windows.

## 1. Installation

- [ ] **P-01 [B]** Ta 0.9.1 propose la 0.10.0 ; « Installer » puis « Redémarrer » → ⚙ Réglages : **Version 0.10.0** (deux copies). Noter : ……… / ………

## 2. Thèmes

- [ ] **T-01 [B]** Réglages ▸ Thème : les 3 thèmes livrés (Moderne clair, Moderne sombre, Classique) s'appliquent à **toutes** les vues : grille, tableau, piles, Détail, lecteur, listes, Infos, Réglages, Scripts, Mise à jour, menus, bandeaux. Rien ne reste dans l'ancienne couleur.
- [ ] **T-02 [B]** Créer un **thème perso sans recompiler** : copier un `.css` de `data\themes`, changer des couleurs, **Recharger** (ou Importer) → il apparaît « perso » et s'applique partout.
- [ ] **T-03 [B]** CSS **invalide** (accolade ou parenthèse non fermée, fichier vide) : l'application ne casse pas, le thème précédent reste, un message clair s'affiche ; le thème choisi n'est **pas** remplacé par le défaut. Au démarrage aussi.
- [ ] **T-04 [B]** CSS avec `@import url(http://…)`, `url(https://…)`, `@font-face`, `url(data:image/svg+xml…)` : rien n'est chargé, des **avertissements** apparaissent.
- [ ] **T-05 [M]** **Aperçu en direct** (survol, flèches) ; **Annuler** et Échap restaurent l'ancien thème ; **OK** le garde après redémarrage.
- [ ] **T-06 [M]** « Recharger » après modification du fichier : appliqué sans redémarrer ; retour sur la fenêtre après édition : rechargé tout seul ; fichier supprimé : retour au Moderne clair.
- [ ] **T-07 [M]** **Importer un thème…** (fichier .css), **Copier le CSS** (coller dans un éditeur), **Ouvrir le dossier des thèmes** (Explorateur), **Supprimer** un thème perso (confirmation) ; un thème livré n'est pas supprimable ; un import au nom déjà pris n'écrase rien.
- [ ] **T-08 [M]** « Réinitialiser les thèmes livrés » : les 3 reviennent à l'état d'origine, l'ancien fichier est gardé en `.css.avant`. Un thème livré que tu avais modifié n'est jamais écrasé au simple démarrage.
- [ ] **T-09 [M]** Lisibilité des 3 thèmes : texte, texte discret, sélection, survol, focus, étoiles, barre de lecture des piles, boutons désactivés, listes déroulantes, cases à cocher. Classique ressemble à la version egui.
- [ ] **T-10 [m]** Un ancien thème à 12 variables (sans les nouvelles) fonctionne ; « Thème par défaut » est toujours accessible.

## 3. Rectangle avec « Empiler par série »

- [ ] **PR-01 [B]** « Empiler par série » actif : glisser un rectangle dans les marges entre piles sélectionne les piles touchées (défilement automatique près des bords).
- [ ] **PR-02 [B]** Clic droit sur plusieurs piles sélectionnées : « Modifier les infos », ajouter à une liste, etc. portent sur les livres de **toutes** les piles (le nombre affiché est juste).
- [ ] **PR-03 [B]** Glisser les piles sélectionnées vers une liste : tous leurs livres sont ajoutés ; une pile hors sélection ne part que seule.
- [ ] **PR-04 [M]** Maj/Ctrl + rectangle, Échap, clic dans le vide ; Maj/Ctrl + clic, Ctrl+A, Maj + flèches ; Ctrl+I et Suppr (liste de lecture) sur la sélection.
- [ ] **PR-05 [M]** Entrée ou double-clic ouvre la pile courante ; refermer : on retrouve la pile ; après un scan ou un ajout de livres, la sélection reste sur les **mêmes séries**.
- [ ] **PR-06 [m]** Détail : une pile → carte « série, N livres, M lus » ; plusieurs → « N piles sélectionnées » ; barre d'état « N piles · M livres ».

## 4. Non-régression

- [ ] **N-01 [B]** Rectangle en grille, tableau, pile ouverte ; glisser de livres vers une liste.
- [ ] **N-02 [B]** Bibliothèque, listes, lecteur (CBZ, PDF), mise à jour intégrée : comme à la tauri-v0.9.1.
- [ ] **N-03 [M]** Scripts (aperçu + application), Bedetheque : comme à la tauri-v0.9.1.

## Limites connues (acceptées pour cette version)

- Pas de modèles HTML complets : un thème = variables et règles CSS.
- Pas de « Enregistrer sous » pour l'export : le CSS est copié dans le presse-papiers ; le dossier des thèmes s'ouvre pour récupérer le fichier.
- Rechargement à la demande (bouton, ouverture des Réglages, retour sur la fenêtre), pas de surveillance continue du dossier.
- `attr()` avec `type(<url>)` n'est pas filtré : seule la politique de sécurité de la fenêtre le bloque.
- La sélection de **livres** (grille) reste repérée par rang : après un rechargement de la liste (scan), elle peut viser d'autres livres (la sélection des piles, elle, suit les séries).
- Reprises : pas de « Changer de connexion » dans Réglages ; piles non réordonnables par glisser ; scripts : pas de bouton « Parcourir… ».

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| (relecture) | [M] | Contournement du filtre CSS par `\` + CRLF dans une chaîne ; thème par défaut invalide empêchant le démarrage ; thème refusé remplacé (et enregistré) par le défaut ; sélection des piles décalée après un rechargement | **Corrigés avant publication** |
| (relecture) | [m] | Noms réservés Windows incomplets, casse d'un thème livré, faux positifs du filtre, import sans `:root`, fin de geste du rectangle (perte de focus) | **Corrigés** |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | | | |
| [M] majeurs | | | |
| [m] mineurs | | | |

**Décision :** ☐ GO ☐ NO-GO — Yoyo, le …
