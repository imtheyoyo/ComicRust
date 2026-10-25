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

Champs modifiables : Series, Number, Count, Volume, Year, Month, Day, Title, Publisher,
Imprint, Writer, Penciller, Inker, Colorist, Letterer, CoverArtist, Editor, Translator,
Genre, StoryArc, SeriesGroup, AlternateSeries, AlternateNumber, AlternateCount, Summary,
Notes, Review, Web, LanguageISO, Format, AgeRating, BlackAndWhite, Manga, Characters, Teams,
MainCharacterOrTeam, Locations, Tags, Rating, CommunityRating, BookAge,
BookCollectionStatus, BookCondition, BookLocation, BookNotes, BookOwner, BookStore,
BookPrice, ISBN, SeriesComplete, ScanInformation, Released.
En lecture seule : Id, File, FileSize, PageCount, CurrentPage, Added, Opened…

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

- Copier le contenu de `python-3.x.y-embed-amd64.zip` dans `python\` : `python.exe`,
  `python3xx.dll`, `python3.dll`, `python3xx.zip` (bibliothèque standard), `python3xx._pth`,
  les `.pyd` (`_ssl`, `_socket`, `select`, `_hashlib`, `unicodedata`, `_ctypes`,
  `_lzma`, `_bz2`, `pyexpat`, `_sqlite3`…) et leurs DLL (`libssl-3*.dll`,
  `libcrypto-3*.dll`, `libffi-8.dll`, `sqlite3.dll`, `vcruntime140*.dll`).
- `python3xx._pth` : rien d'obligatoire (ComicRust ajoute ses chemins). Pour installer des
  paquets avec pip dans `python\Lib\site-packages`, décommenter `import site` ; on peut
  aussi y ajouter `..\scripts\lib`.
- Certificats : `ssl.create_default_context()` charge sous Windows les magasins « ROOT » et
  « CA » du système ; aucun fichier de certificats à livrer. (Un PC très ancien ou hors
  ligne peut ne pas avoir toutes les racines : Windows les télécharge à la demande pour
  ses propres connexions, pas pour Python.)
