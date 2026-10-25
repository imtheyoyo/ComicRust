# Scripts Python de ComicRust

Depuis la v0.10.0, ComicRust exécute des **scripts Python 3** pour modifier les fiches,
renommer ou ranger les fichiers, récupérer des informations en ligne (Comic Vine…) ou
automatiser des tâches (scripts déclenchés tout seuls).

> Ce ne sont **pas** les scripts IronPython de ComicRack : leur API (`ComicRack.App`,
> objets .NET) n'existe pas ici. Le principe des en-têtes `#@Name`, `#@Hook`… est repris.

## Où placer les scripts

- Dossier `scripts\` à côté de `ComicRust.exe` (sous-dossiers acceptés).
- `scripts\lib\` : modules Python partagés (importables par tous les scripts, jamais
  proposés comme scripts eux-mêmes).
- Le dossier du script est aussi importable : un script peut avoir ses propres modules
  à côté de lui.

Python est fourni : la distribution officielle « embeddable » est livrée dans `python\` à
côté de l'exe. Pour utiliser un autre interpréteur, définir la variable d'environnement
`COMICRUST_PYTHON` (chemin complet de `python.exe`).

ComicRust **lit** les scripts pour trouver leurs points d'entrée, sans les exécuter. Un
script n'est exécuté que lorsqu'on le lance (menu) ou que son événement survient.

## Scripts fournis

Le zip Windows contient dans `scripts\` quatre scripts prêts à l'emploi (dans le dépôt :
[`scripts-exemples/`](../scripts-exemples)), commentés pour servir de modèles. Aucun ne
supprime de fichier ; ceux du menu passent par l'aperçu. Copiez-en un sous un autre nom
avant de le modifier : la mise à jour intégrée de ComicRust remplace les fichiers livrés
(les vôtres, et ceux de `scripts\lib`, ne sont pas touchés).

| Script | Menu / déclenchement | Rôle | Réglages (bouton « Réglages ») |
|---|---|---|---|
| `nettoyage.py` | **Nettoyer les fiches** (`Books`, `Library`) | Espaces en trop ; listes d'auteurs, genres, personnages, étiquettes remises en forme (séparateur « , », doublons retirés) ; numéro normalisé (`001` → `1`, `#12` → `12`, `T03` → `3` ; `1.5`, `½`, `12a` gardés) ; série, numéro, année, titre vides complétés depuis le nom du fichier ; en option, casse des séries écrites TOUT EN MAJUSCULES ou tout en minuscules (une casse mixte comme « iZombie » n'est jamais touchée). | numéro, complément depuis le nom, casse des séries |
| `ranger_fichiers.py` | **Ranger les fichiers** (`Books`) | Renomme/déplace selon un modèle de nom (syntaxe de la fenêtre « Renommer » : `{series}[ v{volume}][ #{number:000}][ ({year})]`, `/` = sous-dossier), dans un dossier racine, avec sous-dossiers par série ou par éditeur/série. Deux livres de même nom ou un fichier déjà présent : suffixe « (2) », jamais d'écrasement ; chemin de plus de 259 caractères ignoré. | modèle, dossier racine (vide = dossier actuel), sous-dossiers |
| `comic_vine.py` | **Infos depuis Comic Vine** (`Books`) | Cherche la série (nom + année) puis le numéro sur Comic Vine, demande de choisir si c'est ambigu, puis complète titre, résumé, scénaristes, dessinateurs, encreurs, coloristes, lettreurs, couverture, éditeurs, personnages, équipes, lieux, arcs, année/mois (date de couverture), éditeur, nombre de numéros, Web (page Comic Vine). Seuls les champs vides par défaut. | **clé d'API** (secrète), remplacer les champs remplis, écrire ComicInfo.xml |
| `automatisme_nouveaux.py` | automatique (`NewBooks`) | Étiquette « à lire » et série/numéro/année depuis le nom du fichier sur les nouveaux livres. **Désactivé par défaut** : l'activer dans ses réglages. | activer, étiquette, complément depuis le nom |

### Comic Vine

L'API de Comic Vine demande une clé personnelle gratuite (compte sur
<https://comicvine.gamespot.com>, puis <https://comicvine.gamespot.com/api/>) : la coller
dans « Comic Vine : réglages » (enregistrée chiffrée sous Windows). Requêtes utilisées
(`format=json`, `field_list` pour ne recevoir que les champs utiles) :

| Requête | Usage |
|---|---|
| `volumes/?filter=name:<série>` | séries (« volumes ») de ce nom, avec `start_year`, `publisher`, `count_of_issues` |
| `search/?query=<série>&resources=volume` | recherche plus souple si la précédente ne trouve rien |
| `issues/?filter=volume:<id>,issue_number:<n>` | le numéro dans la série choisie |
| `issue/4000-<id>/` | fiche complète : `description` (HTML), `person_credits` (avec `role`), `character_credits`, `team_credits`, `location_credits`, `story_arc_credits`, `cover_date`, `site_detail_url` |

Comic Vine limite à environ 200 requêtes par heure et par type de ressource et bloque les
rafales : le script attend 1,5 s entre deux requêtes (2 à 3 requêtes par livre) et
s'identifie par un `User-Agent`. Réponse `status_code` 100 (clé invalide) ou 107 (limite
atteinte) : le script s'arrête avec un message. Pour les essais, l'adresse de l'API se
change par le réglage `url_base` ou la variable d'environnement `COMICRUST_COMICVINE_URL`.

## En-têtes

Chaque fonction proposée par un fichier est précédée d'en-têtes en commentaires, juste
au-dessus de son `def` (fonction de premier niveau, non indentée). Plusieurs fonctions par
fichier sont possibles.

```python
#@Name Nettoyer les séries
#@Hook Books
#@Description Retire les espaces en trop dans le nom des séries.
#@Key nettoyer-series
#@Image nettoyer.png
def nettoyer(books):
    ...
```

| En-tête | Rôle |
|---|---|
| `#@Name` | Libellé dans les menus (obligatoire). |
| `#@Hook` | Quand le script est appelé (obligatoire ; plusieurs séparés par des virgules). |
| `#@Description` | Explication (plusieurs lignes possibles). |
| `#@Key` | Identifiant stable (sinon `fichier.py:fonction`), utilisé pour activer/désactiver. |
| `#@Image` | Icône du menu, relative au dossier du script. |

Hooks :

| Hook | Déclenchement | Appel |
|---|---|---|
| `Books` | Menu Scripts sur les livres sélectionnés | `fonction(books)` |
| `Library` | Menu Scripts sur toute la bibliothèque | `fonction(books)` |
| `BookOpened` | Ouverture d'un livre dans le lecteur | `fonction(book)` |
| `BookClosed` | Fermeture d'un livre | `fonction(book)` |
| `NewBooks` | Après une analyse qui a ajouté des livres | `fonction(books)` |
| `Startup` | Au démarrage de ComicRust | `fonction()` |
| `ConfigScript` | Bouton « Réglages » du script | `fonction()` |

Une fonction sans paramètre est appelée sans argument. `BookOpened`, `BookClosed`,
`NewBooks` et `Startup` sont des **automatismes** : pas de question à l'utilisateur
(valeurs par défaut), modifications appliquées au fil de l'eau, délai maximum (5 min par
défaut). Pour `Books` et `Library`, ComicRust montre un **aperçu** des modifications avant
de les appliquer.

En-têtes mal formés (sans valeur, hook inconnu, `#@Name` manquant…) : la fonction est
ignorée et un avertissement est affiché dans la fenêtre des scripts. Chaque script peut
être désactivé dans les réglages.

## Le module `comicrust`

```python
import comicrust
```

### Livres

- `comicrust.books()` : livres du contexte (sélection, bibliothèque, livre ouvert, nouveaux
  livres). Se parcourt comme une liste (`len`, `books[i]`, `for`), lue par lots de 500 :
  une bibliothèque de 5 000 livres ne pose pas de problème.
- `comicrust.book()` : premier livre du contexte (le livre ouvert).
- Champs : `book.series` ou `book["Series"]` (noms en snake_case ou noms ComicRack :
  `book.language_iso` = `book["LanguageISO"]`). Champ vide → `None`.
- `book.id` (identifiant), `book.file` (chemin complet du fichier), `book.to_dict()`.

### Modifications (mises en attente)

Un script ne modifie jamais directement la base ni les fichiers : il **demande**, ComicRust
vérifie et applique.

- `book.series = "X"`, `book["Series"] = "X"`, `book.set("Series", "X")` ; `None` vide le
  champ ; une liste devient un texte séparé par des virgules.
- `comicrust.rename_file(book, nouveau_chemin)` : chemin complet, ou simple nom de
  fichier (même dossier).
- `comicrust.write_info(book)` : écrire la fiche (ComicInfo.xml) dans le fichier.
- `comicrust.flush()` : envoyer tout de suite (fait automatiquement tous les 500
  changements et à la fin).

Champs modifiables : exactement ceux que ComicRust sait écrire (la même liste sert à
l'interface et aux scripts) — Series, Number, Count, Volume, Year, Month, Day, Title,
Publisher, Imprint, Writer, Penciller, Inker, Colorist, Letterer, CoverArtist, Editor,
Translator, Genre, StoryArc, SeriesGroup, AlternateSeries, AlternateNumber, AlternateCount,
Summary, Notes, Review, Web, LanguageISO, Format, AgeRating, BlackAndWhite, Manga,
PreferredFrontCover, Characters, Teams, MainCharacterOrTeam, Locations, CommunityRating,
ScanInformation, Tags, Released, Rating, EnableProposed, SeriesComplete, Checked, BookPrice,
BookAge, BookCondition, BookStore, BookOwner, BookCollectionStatus, BookNotes, BookLocation,
ISBN, CustomValuesStore.
En lecture seule : Id, File, FileSize, FileModifiedTime, FileCreationTime, PageCount, Pages,
Added, Opened, OpenCount, CurrentPage, LastPageRead, NewPages, Missing, ComicInfoIsDirty,
ComicBookIsDirty, CustomThumbnailKey, LastOpenedFromListId.

Une valeur refusée (année qui n'est pas un nombre, mois 13…) l'est **tout de suite** : elle
est notée dans le journal du script et ne figure pas dans l'aperçu.

Valeurs personnalisées (`GetCustomValue` / `SetCustomValue` de ComicRack) :
`book.custom_value("clé", défaut)` et `book.set_custom_value("clé", "valeur")` (`None` ou
`""` la supprime) ; c'est le champ `CustomValue:<clé>` (`book.set("CustomValue:clé", …)`).

Une demande sur un livre hors du contexte du script est refusée (journal).

### Interface

- `comicrust.progress(fraction, texte)` : barre d'avancement (0 à 1).
- `comicrust.cancelled()` : vrai si l'utilisateur a cliqué sur « Arrêter » — à tester dans
  les boucles. Sans réaction en 1 s, `KeyboardInterrupt` est levée dans le script ; sans
  réaction quelques secondes de plus, le processus est arrêté.
  `comicrust.check_cancelled()` lève `comicrust.Cancelled`.
- `print(...)` et `comicrust.log(..., level="info"|"warning"|"error")` : journal du script.
- `comicrust.message(texte)` : message à l'utilisateur.
- `comicrust.ask(question, défaut)`, `comicrust.ask_choice(question, choix, défaut)`,
  `comicrust.confirm(question, défaut)` : questions (valeur par défaut pour un
  automatisme ou si l'utilisateur ferme la fenêtre).
- `comicrust.hook()`, `comicrust.interactive()`, `comicrust.args()`.

### Réglages du script

```python
s = comicrust.settings()          # dictionnaire, commun aux fonctions du fichier
s["api_key"] = "…"
s.mark_secret("api_key")          # stockée chiffrée (Windows) ; ailleurs non conservée
s.save()                          # aussi fait automatiquement à la fin si modifié
```

### Outils

- `comicrust.format_name("{publisher}/{series} #{number:03} ({year})", book)` : nom de
  fichier à partir des champs ; caractères interdits remplacés par `_` dans les valeurs ;
  champ vide → texte vide.
- `comicrust.http_get(url, headers)`, `http_get_text`, `http_get_json` : téléchargement
  avec `urllib` (bibliothèque standard, utilisable directement aussi). Un `User-Agent` est
  ajouté (Comic Vine refuse celui de Python). Sous Windows, les certificats SSL sont ceux
  du magasin Windows.

## Dans ComicRust

- Menu **📜 Scripts** de la barre d'outils : scripts `Library` (toute la bibliothèque) et
  `Books` (livres sélectionnés), « Ouvrir le dossier des scripts », « Recharger les
  scripts », « Gérer les scripts… » et la case « Exécuter les automatismes ».
- Menu contextuel d'un livre : **📜 Scripts ▸** (scripts `Books` sur la sélection).
- **Gérer les scripts** : scripts rangés par hook, case pour activer / désactiver chacun,
  description, fichier, bouton « Réglages… » quand le fichier a une fonction `ConfigScript`,
  avertissements de lecture des en-têtes, journal des automatismes.
- Un script lancé d'un menu ouvre une fenêtre (avancement, journal copiable, « Arrêter ») ;
  ses questions s'affichent dans une petite fenêtre (Annuler = valeur par défaut ; arrêter le
  script pendant une question rend aussi la valeur par défaut). À la fin, un **aperçu**
  montre pour chaque livre les champs (avant → après), les renommages (conflits signalés :
  destination existante, deux livres vers le même fichier, fichier introuvable) et les
  écritures d'infos ; on coche les livres à modifier puis « Appliquer » (ou « Annuler »).
  Après un arrêt, ce qui avait déjà été demandé peut être vu en aperçu ; rien n'est appliqué
  sans accord.
- Application : fiches, puis renommages, puis écriture des infos dans les fichiers (comme
  « Écrire les infos dans le fichier »), suivies d'un bilan.
- Bibliothèque en lecture seule (ComicRack MySQL sans écriture, `ComicDb.xml`) : l'aperçu
  s'affiche mais « Appliquer » est refusé, avec la marche à suivre.
- **Automatismes** (`Startup` après le chargement de la bibliothèque, `BookOpened` /
  `BookClosed` dans le lecteur, `NewBooks` après une analyse des dossiers qui a ajouté des
  livres) : **désactivés par défaut** (case « Exécuter les automatismes ») car ils modifient
  la bibliothèque sans aperçu. Un seul automatisme à la fois par hook (les suivants
  attendent), jamais pendant un transfert ; erreurs signalées avec le nom du script.
- Réglages des scripts gardés dans `data\comicrust.ron` (valeurs secrètes chiffrées).

## Exemples

### Nettoyer les séries (menu)

```python
import comicrust

#@Name Nettoyer les séries
#@Hook Books, Library
#@Description Retire les espaces en trop dans le nom des séries.
def nettoyer(books):
    for i, book in enumerate(books):
        if comicrust.cancelled():
            break
        comicrust.progress(i / len(books), book.series or "")
        if book.series:
            book.series = " ".join(book.series.split())
```

### Ranger les fichiers

```python
import os
import comicrust

#@Name Ranger par éditeur et série
#@Hook Books
def ranger(books):
    racine = comicrust.ask("Dossier de destination ?", r"D:\BD")
    for book in books:
        if not book.file or not book.series:
            continue
        ext = os.path.splitext(book.file)[1]
        nom = comicrust.format_name("{publisher}/{series}/{series} #{number:03}", book)
        comicrust.rename_file(book, os.path.join(racine, nom + ext))
```

### Comic Vine, avec réglages

```python
import urllib.parse
import comicrust

#@Name Réglages Comic Vine
#@Hook ConfigScript
def reglages():
    s = comicrust.settings()
    s["api_key"] = comicrust.ask("Clé d'API Comic Vine :", s.get("api_key", ""))
    s.mark_secret("api_key")

#@Name Résumé depuis Comic Vine
#@Hook Books
def resume(books):
    cle = comicrust.settings().get("api_key")
    if not cle:
        comicrust.message("Indiquez d'abord la clé d'API (Réglages du script).")
        return
    for book in books:
        q = urllib.parse.urlencode({"api_key": cle, "format": "json",
                                    "filter": "name:%s" % book.series, "limit": 1})
        r = comicrust.http_get_json("https://comicvine.gamespot.com/api/volumes/?" + q)
        if r.get("results"):
            book.summary = r["results"][0].get("deck")
    # puis écrire les fiches dans les fichiers :
    for book in books:
        comicrust.write_info(book)
```

### Automatisme : noter la date de lecture

```python
import datetime

#@Name Noter l'ouverture
#@Hook BookOpened
def ouvert(book):
    book.notes = "Ouvert le %s" % datetime.date.today().isoformat()
```

## Fonctionnement (pour les curieux)

- Chaque exécution lance un processus Python séparé : un script qui plante ou boucle ne
  fait pas tomber ComicRust.
- Dialogue en JSON par lignes : entrée standard (ComicRust → script) et sortie standard
  d'origine (script → ComicRust). `print()` est redirigé vers le journal (stderr).
- Le module `comicrust.py` est intégré à ComicRust et écrit à chaque lancement dans
  `%TEMP%\comicrust-py-<version>\` ; ne pas le modifier.
- Python est lancé en mode isolé (`-I`) : `PYTHONPATH` et le dossier utilisateur sont
  ignorés ; ComicRust ajoute lui-même le dossier du script et `scripts\lib` à `sys.path`.

### Paquet Windows (Python « embeddable »)

- Le zip contient dans `python\` la distribution officielle `python-3.13.17-embed-amd64.zip`
  (Python 3.13.17), téléchargée par la CI depuis python.org et **refusée si son empreinte
  SHA-256 diffère** de celle figée dans `.github/python-embarque.env` (relevée sur la page
  de la version). Sa licence (PSF) est dans `licences\Python-LICENSE.txt`.
- Contenu : `python.exe`, `python313.dll`, `python3.dll`, `python313.zip` (bibliothèque
  standard), `python313._pth`, les `.pyd` (`_ssl`, `_socket`, `select`, `_hashlib`,
  `unicodedata`, `_ctypes`, `_lzma`, `_bz2`, `pyexpat`, `_sqlite3`…) et leurs DLL
  (`libssl-3*.dll`, `libcrypto-3*.dll`, `libffi-8.dll`, `sqlite3.dll`, `vcruntime140*.dll`).
- `python313._pth` est laissé tel quel (`import site` commenté : pas de `site-packages`) ;
  ComicRust ajoute lui-même le dossier du script et `scripts\lib` à `sys.path`.
- Certificats : `ssl.create_default_context()` charge sous Windows les magasins « ROOT » et
  « CA » du système ; aucun fichier de certificats à livrer. (Un PC très ancien ou hors
  ligne peut ne pas avoir toutes les racines : Windows les télécharge à la demande pour
  ses propres connexions, pas pour Python.)
- La CI vérifie le paquet : modules natifs (`ssl`, `sqlite3`…), une connexion HTTPS, puis
  les scripts fournis exécutés avec ce Python (test `tests/scripts_exemples.rs`).

## Modules supplémentaires

- **Module pur Python** (sans fichier `.pyd` compilé) : copier le dossier du paquet (celui
  qui contient `__init__.py`), ou le fichier `.py` d'un module simple, dans `scripts\lib\`.
  Une « wheel » (`.whl`) de PyPI est un zip : l'ouvrir et en copier le dossier. Les
  dépendances s'installent de la même façon.
- **Avec un autre Python 3.13 installé** sur le PC (même version mineure, 64 bits), pip
  peut installer directement dans `scripts\lib` :
  `py -3.13 -m pip install --target "C:\…\ComicRust\scripts\lib" nom-du-paquet` (les
  modules compilés conviennent alors aussi, puisqu'ils visent la même version).
- **pip dans le Python embarqué** : non fourni. Pour l'activer, à vos risques (une mise à
  jour de ComicRust remet le `python313._pth` d'origine : il faut alors décommenter à
  nouveau `import site` ; un changement de version de Python rend les paquets compilés
  inutilisables) : décommenter
  `import site` dans `python\python313._pth`, télécharger `get-pip.py`
  (<https://bootstrap.pypa.io/get-pip.py>) et lancer `python\python.exe get-pip.py` ; les
  paquets vont alors dans `python\Lib\site-packages`. Préférer `scripts\lib`, qui
  survit aux mises à jour.
- Autre solution : un Python complet désigné par `COMICRUST_PYTHON` (voir plus haut).
