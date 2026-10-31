# Scripts d'exemple de ComicRust

Ces scripts Python sont livrés dans le dossier `scripts\exemples\` à côté de
`ComicRust.exe` (dans le dépôt : `scripts-exemples/exemples/` ; ce dossier
`scripts-exemples/` correspond à `scripts\`). Ils servent tels quels et comme modèles :
copiez-en un à la racine de `scripts\` sous un autre nom (et un autre `#@Key`) avant de
le modifier. La mise à jour intégrée n'écrase jamais un fichier que vous avez modifié : la
nouvelle version est posée à côté (`nom.py.nouveau`) ; un exemple supprimé n'est pas
réinstallé.
Documentation complète : [`docs/scripts.md`](../docs/scripts.md) (dans le zip :
`scripts.md`).

Aucun de ces scripts ne supprime de fichier. Ceux du menu « Scripts » ne font que
**demander** des modifications : ComicRust les montre en aperçu avant de les appliquer.

| Fichier | Menu / déclenchement | Ce qu'il fait | Réglages |
|---|---|---|---|
| `nettoyage.py` | **Nettoyer les fiches** (livres sélectionnés ou toute la bibliothèque) | Espaces en trop, listes d'auteurs/genres/étiquettes remises en forme (doublons retirés), numéro normalisé (`001` → `1`, `#12` → `12`, `T03` → `3` ; `1.5`, `½`, `12a` gardés), série/numéro/année/titre vides complétés depuis le nom du fichier, casse des séries TOUT EN MAJUSCULES (option, désactivée par défaut). | numéro, complément depuis le nom, casse des séries |
| `ranger_fichiers.py` | **Ranger les fichiers** (livres sélectionnés) | Renomme/déplace les fichiers selon un modèle (`{series}[ v{volume}][ #{number}][ ({year})]`, même syntaxe que la fenêtre « Renommer »), dans un dossier racine, avec sous-dossiers par série ou éditeur/série. Conflits : suffixe « (2) », jamais d'écrasement. | modèle, dossier racine, sous-dossiers |
| `comic_vine.py` | **Infos depuis Comic Vine** (livres sélectionnés) | Cherche la série puis le numéro sur [Comic Vine](https://comicvine.gamespot.com) (choix demandé si ambigu) et complète titre, résumé, auteurs, dessinateurs, coloristes, date de couverture (année/mois), éditeur, personnages, équipes, lieux, arcs, lien Web. Par défaut, seuls les champs vides sont remplis. | **clé d'API personnelle** (secrète), remplacer les champs remplis, écrire ComicInfo.xml |
| `automatisme_nouveaux.py` | automatique, après une analyse qui ajoute des livres (`NewBooks`) | Étiquette « à lire » et série/numéro/année depuis le nom du fichier. **Désactivé par défaut** (`#@Enabled false`) : le cocher dans 📜 Scripts ▸ Gérer les scripts. | étiquette, complément depuis le nom |
| `bedetheque_scraper2.py` (+ dossier `bd2\`) | **Bedetheque Scraper 2** (livres sélectionnés, ou bouton Scripts de la fenêtre « Infos du livre »), **QuickScrape BD2** (lien direct), **Configurer BD2** | Portage du script de ComicRack : informations de [bedetheque.com](https://www.bedetheque.com) (série, numéro, titre, auteurs, éditeur, collection, ISBN, dépôt légal, résumé, genre, note…), choix de la série / de l'album / de l'édition avec couvertures et fermeture automatique. | mêmes réglages que l'original (3 onglets), import de l'`App.Config` de ComicRack, champs vides seulement |
| `library_organizer.py` (+ `library_organizer_moteur.py`) | **Library Organizer**, **Library Organizer (Quick)** (livres sélectionnés), **… (Quick) – toute la bibliothèque**, **Configurer Library Organizer**, **… annuler le dernier rangement**, **… supprimer les dossiers vides** | Portage du script de ComicRack : range et renomme les fichiers selon des **profils** (modèles de dossier et de nom avec la syntaxe de l'original `{préfixe<champ>suffixe}`, règles, modes Déplacer / Simuler), doublons (« garder les deux » ou « ne pas déplacer »), journal, annulation. Les déplacements passent par l'aperçu. | profils (nouveau, renommer, dupliquer, supprimer, importer/exporter `.lop`), **import du `losettingsx.dat` de ComicRack** |

Les réglages se changent avec le bouton **Réglages** du script (fonction `ConfigScript`).

## Comic Vine

1. Créer un compte gratuit sur <https://comicvine.gamespot.com> puis récupérer la clé
   d'API sur <https://comicvine.gamespot.com/api/>.
2. Scripts → « Comic Vine : réglages » → coller la clé (saisie masquée ; vide = garder la
   clé enregistrée ; enregistrée chiffrée sous Windows).
3. Sélectionner quelques livres → Scripts → « Infos depuis Comic Vine ».

Comic Vine limite le nombre de requêtes (environ 200 par heure et par type) : le script
attend 1,5 s entre deux requêtes et en fait 2 ou 3 par livre. Comic Vine recense surtout
les comics américains : les BD franco-belges y sont rarement.

## Library Organizer

Portage de *Library Organizer* 2.1.13 (Stonepaw, licence Apache 2.0 :
`LICENCE-library-organizer.txt`). Pour reprendre vos réglages de ComicRack : Scripts →
« Configurer Library Organizer » → **Importer les réglages de Library Organizer
(ComicRack)…** et choisir `losettingsx.dat` (dans
`%APPDATA%\cYo\ComicRack Community Edition\Scripts\Library Organizer\` ou
`%APPDATA%\cYo\ComicRack\Scripts\Library Organizer\`). Les modèles gardent la syntaxe
de l'original. Différences imposées par ComicRust :

- rien n'est déplacé par le script : ComicRust montre l'aperçu, puis applique sans jamais
  écraser un fichier (pas de « Remplacer » dans la fenêtre des doublons) ;
- mode **Copier** et images des livres sans fichier : impossibles depuis un script
  (un profil Copier est exécuté en simulation, c'est signalé) ;
- dossiers vides : listés dans le rapport ; après « Appliquer », lancer « Library Organizer
  – supprimer les dossiers vides » (confirmation ; seuls les dossiers vraiment vides sont
  supprimés) ;
- année/mois de début, dernier numéro… : calculés sur les livres transmis (lancer depuis
  le menu « bibliothèque » pour tenir compte de toute la bibliothèque) ;
- journal (10 derniers), rangement demandé, dossiers vides : `data\library_organizer\` ;
  l'annulation part du bilan des renommages réellement appliqués écrit par ComicRust
  (`data\renommages\`) ;
- valeurs calculées sur la série (année de début, dernier numéro…) : sur toute la
  bibliothèque, comme l'original.

## Modules partagés

`lib\` : modules Python importables par tous les scripts (`import mon_module`), jamais
proposés comme scripts. Un module **pur Python** (sans extension compilée) s'installe en
copiant son dossier dans `scripts\lib\`. Voir `docs/scripts.md`.
