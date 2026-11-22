# Recette ComicRust Tauri tauri-v0.11.0

| | |
|---|---|
| Version | tauri-v0.11.0 (pré-release de l'étape **audit de parité** avec la version egui v0.10.9) |
| Paquet | `ComicRust-Tauri-tauri-v0.11.0-windows-x64.zip` |
| SHA-256 | `(à renseigner à la publication)` |
| Contenu | tous les écarts [B] et [M] de `docs/tauri/audit-parite.md` : piles réordonnables, livres manquants, scan au démarrage, fenêtre et fichiers déposés, dialogue natif, assistant rejouable, À propos / F1, récents, livre suivant, regroupements, tableau, CLI dans le zip |
| Origine | [recette tauri-v0.10.0](tauri-v0.10.0.md) : **GO** du 14/11/2026 ; demande « déplacer les séries dans une liste de lecture » ([rapport](rapports/tauri-v0.10.0-2026-11-14.txt)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> Version volumineuse (audit complet) : fais d'abord les blocs **Installation**, **Piles** et
> **Non-régression**, puis le reste. Installation par la mise à jour intégrée de ta 0.10.0 (copies
> autonome et liée). Pour les essais de livres manquants et de déplacement, travaille sur une **copie**
> de la bibliothèque (jamais `comickrack_demo`) ; ComicDb.xml reste en lecture seule sauf « Lecture et écriture ».

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| Tests du backend (≈ 330) : réordonnancement des piles (série dispersée, début/fin, plusieurs piles, liste en lecture seule, tri décroissant), livres manquants (marquage, relocalisation, retrait), scan, fenêtre, plantage, dialogue, regroupements, tableau, récents | `cargo test` | ✅ |
| Front : `svelte-check` (0 erreur), `vite build` ; parcours Playwright simulé de chaque nouvel écran | local | ✅ |
| Comparaison avec la version egui : algorithmes de réordonnancement, de relocalisation et de regroupement identiques | relecture indépendante | ✅ |
| Relectures indépendantes (Rust, front) : défauts corrigés avant publication (voir « Anomalies ») | | ✅ |
| Rustfmt, Clippy `-D warnings` dont la fenêtre (feature `app`) | CI Windows | ✅ |

**Non vérifiable ici** : la vraie fenêtre WebView2, le dialogue natif Windows, F11, le dépôt de fichiers
depuis l'Explorateur, les mesures de mémoire.

## 1. Installation

- [ ] **P-01 [B]** Ta 0.10.0 propose la 0.11.0 ; « Installer » puis « Redémarrer » → ⚙ Réglages ▸ À propos : **Version 0.11.0** (deux copies). Noter : ……… / ………
- [ ] **P-02 [M]** Le zip contient `ComicRust-Tauri.exe`, **`comicrust-cli.exe`** et un `README.md` propre à la version Tauri ; `comicrust-cli.exe --help` répond.

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

- [ ] **LE-01 [M]** Fin de livre : proposition du **livre suivant** (même série / même liste) ; accepter l'ouvre, refuser reste sur la dernière page.
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

- Écritures disque courtes sur le fil principal (géométrie de la fenêtre, réglages d'analyse) ; la liste des récents vérifie jusqu'à 15 fichiers.
- Une panique rattrapée dans un fil de travail peut laisser un rapport ; les textes copiés (diagnostic, plantage, rapport) contiennent des chemins absolus, donc possiblement le nom d'utilisateur Windows : relis-les avant de les poster.
- Taille minimale de la fenêtre 800 × 500 (egui : 480 × 360).
- Plusieurs piles avec des séries dispersées restent entrelacées (comme en egui) ; la sélection de **livres** reste repérée par rang après un rechargement.
- Le thème « Comme Windows » masque un thème perso nommé `systeme.css` ; un `.cbl` ouvert n'est pas mis dans les récents ; pas de tableau « converted ».
- Pas de Monter/Descendre sur la grille hors piles ni d'entrées « Retirer… » dans le menu des piles.

## Anomalies

| Test | Gravité | Constat | Correctif prévu |
|---|---|---|---|
| (relecture) | [M] | Compilation `app.rs` (clippy), fausses marques « manquant » pendant l'analyse, faux rapports de plantage d'un fil secondaire, deux livres relocalisés sur le même fichier, F1 doublé dans le lecteur, Ctrl+O sur `.cbl`, raccourcis sous les fenêtres modales | **Corrigés avant publication** |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| [B] bloquants | | | |
| [M] majeurs | | | |
| [m] mineurs | | | |

**Décision : (à remplir)**
