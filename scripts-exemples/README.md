# Scripts d'exemple de ComicRust

Ces scripts Python sont livrés dans le dossier `scripts\` à côté de `ComicRust.exe` (dans le
dépôt : `scripts-exemples/`). Ils servent tels quels et comme modèles : copiez-en un sous
un autre nom avant de le modifier (une mise à jour de ComicRust remplace les originaux).
Documentation complète : [`docs/scripts.md`](../docs/scripts.md) (dans le zip :
`scripts.md`).

Aucun de ces scripts ne supprime de fichier. Ceux du menu « Scripts » ne font que
**demander** des modifications : ComicRust les montre en aperçu avant de les appliquer.

| Fichier | Menu / déclenchement | Ce qu'il fait | Réglages |
|---|---|---|---|
| `nettoyage.py` | **Nettoyer les fiches** (livres sélectionnés ou toute la bibliothèque) | Espaces en trop, listes d'auteurs/genres/étiquettes remises en forme (doublons retirés), numéro normalisé (`001` → `1`, `#12` → `12`, `T03` → `3` ; `1.5`, `½`, `12a` gardés), série/numéro/année/titre vides complétés depuis le nom du fichier, casse des séries TOUT EN MAJUSCULES (option, désactivée par défaut). | numéro, complément depuis le nom, casse des séries |
| `ranger_fichiers.py` | **Ranger les fichiers** (livres sélectionnés) | Renomme/déplace les fichiers selon un modèle (`{series}[ v{volume}][ #{number}][ ({year})]`, même syntaxe que la fenêtre « Renommer »), dans un dossier racine, avec sous-dossiers par série ou éditeur/série. Conflits : suffixe « (2) », jamais d'écrasement. | modèle, dossier racine, sous-dossiers |
| `comic_vine.py` | **Infos depuis Comic Vine** (livres sélectionnés) | Cherche la série puis le numéro sur [Comic Vine](https://comicvine.gamespot.com) (choix demandé si ambigu) et complète titre, résumé, auteurs, dessinateurs, coloristes, date de couverture (année/mois), éditeur, personnages, équipes, lieux, arcs, lien Web. Par défaut, seuls les champs vides sont remplis. | **clé d'API personnelle** (secrète), remplacer les champs remplis, écrire ComicInfo.xml |
| `automatisme_nouveaux.py` | automatique, après une analyse qui ajoute des livres (`NewBooks`) | Étiquette « à lire » et série/numéro/année depuis le nom du fichier. **Désactivé par défaut** : l'activer dans ses réglages. | activer, étiquette, complément depuis le nom |

Les réglages se changent avec le bouton **Réglages** du script (fonction `ConfigScript`).

## Comic Vine

1. Créer un compte gratuit sur <https://comicvine.gamespot.com> puis récupérer la clé
   d'API sur <https://comicvine.gamespot.com/api/>.
2. Scripts → « Comic Vine : réglages » → coller la clé (enregistrée chiffrée sous Windows).
3. Sélectionner quelques livres → Scripts → « Infos depuis Comic Vine ».

Comic Vine limite le nombre de requêtes (environ 200 par heure et par type) : le script
attend 1,5 s entre deux requêtes et en fait 2 ou 3 par livre. Comic Vine recense surtout
les comics américains : les BD franco-belges y sont rarement.

## Modules partagés

`lib\` : modules Python importables par tous les scripts (`import mon_module`), jamais
proposés comme scripts. Un module **pur Python** (sans extension compilée) s'installe en
copiant son dossier dans `scripts\lib\`. Voir `docs/scripts.md`.
