# Scripts Python de ComicRust

Depuis la v0.10.0, ComicRust exécute des **scripts Python 3** pour modifier les fiches,
renommer ou ranger les fichiers, récupérer des informations en ligne (Comic Vine…) ou
automatiser des tâches (scripts déclenchés tout seuls).

> Ce ne sont **pas** les scripts IronPython de ComicRack : leur API (`ComicRack.App`,
> objets .NET) n'existe pas ici. Le principe des en-têtes `#@Name`, `#@Hook`… est repris.

## Où placer les scripts

- Dossier `scripts\` à côté de `ComicRust.exe` (sous-dossiers acceptés). Vos scripts vont
  à la racine de `scripts\` (ou dans vos propres sous-dossiers) ; `scripts\exemples\`
  contient les scripts fournis.
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

Le zip Windows contient dans `scripts\exemples\` des scripts prêts à l'emploi (dans le
dépôt : [`scripts-exemples/exemples/`](../scripts-exemples/exemples)), commentés pour
servir de modèles. Aucun ne supprime de fichier ; ceux du menu passent par l'aperçu.

Mise à jour intégrée et scripts :

- un fichier livré dans `scripts\` que vous n'avez **pas modifié** est mis à jour ;
- un fichier que vous avez **modifié** n'est jamais écrasé : la nouvelle version est posée
  à côté sous le nom `nom.py.nouveau` (pas proposé comme script) et la fenêtre de mise à
  jour la signale ; comparez puis renommez si vous la voulez ;
- un exemple que vous avez **supprimé** n'est pas réinstallé (ComicRust note les fichiers
  livrés et leur empreinte dans `data\scripts-livres.txt`). Limite : la liste est relevée
  au premier lancement de la v0.10.0 ; un exemple supprimé avant ce premier lancement
  revient une fois ;
- vos scripts (racine de `scripts\`, `scripts\lib\`) et vos fichiers `.old` ne sont
  jamais touchés : seuls les `.old` créés par la mise à jour (listés dans
  `data\maj\anciens.txt`) sont supprimés au lancement suivant.

Le plus simple reste de copier un exemple à la racine de `scripts\` sous un autre nom
avant de le modifier (pensez à changer son `#@Key`).

| Script | Menu / déclenchement | Rôle | Réglages (bouton « Réglages ») |
|---|---|---|---|
| `nettoyage.py` | **Nettoyer les fiches** (`Books`, `Library`) | Espaces en trop ; listes d'auteurs, genres, personnages, étiquettes remises en forme (séparateur « , », doublons retirés) ; numéro normalisé (`001` → `1`, `#12` → `12`, `T03` → `3` ; `1.5`, `½`, `12a` gardés) ; série, numéro, année, titre vides complétés depuis le nom du fichier ; en option, casse des séries écrites TOUT EN MAJUSCULES ou tout en minuscules (une casse mixte comme « iZombie » n'est jamais touchée). | numéro, complément depuis le nom, casse des séries |
| `ranger_fichiers.py` | **Ranger les fichiers** (`Books`) | Renomme/déplace selon un modèle de nom (syntaxe de la fenêtre « Renommer » : `{series}[ v{volume}][ #{number:000}][ ({year})]`, `/` = sous-dossier), dans un dossier racine, avec sous-dossiers par série ou par éditeur/série. Deux livres de même nom ou un fichier déjà présent : suffixe « (2) », jamais d'écrasement ; chemin de plus de 259 caractères ignoré. | modèle, dossier racine (vide = dossier actuel ; un fichier déjà dans le bon sous-dossier n'y est pas remis une seconde fois), sous-dossiers |
| `comic_vine.py` | **Infos depuis Comic Vine** (`Books`) | Cherche la série (nom + année) puis le numéro sur Comic Vine, demande de choisir si c'est ambigu, puis complète titre, résumé, scénaristes, dessinateurs, encreurs, coloristes, lettreurs, couverture, éditeurs, personnages, équipes, lieux, arcs, année/mois (date de couverture), éditeur, nombre de numéros, Web (page Comic Vine). Seuls les champs vides par défaut. | **clé d'API** (secrète), remplacer les champs remplis, écrire ComicInfo.xml |
| `automatisme_nouveaux.py` | automatique (`NewBooks`) | Étiquette « à lire » et série/numéro/année depuis le nom du fichier sur les nouveaux livres. **Désactivé par défaut** (`#@Enabled false`) : le cocher dans « Gérer les scripts » (et cocher « Exécuter les automatismes »). | étiquette, complément depuis le nom |
| `bedetheque_scraper2.py` | **Bedetheque Scraper 2** (`Books`, `Editor`), **QuickScrape BD2** (`Books`), **Configurer BD2** (`ConfigScript`, `Library`) | Portage Python 3 de *Bedetheque Scraper 2* (Franck, Mizio66, kiwi13 & maforget, v6.08) : cherche la série (liste par lettre, puis recherche du site) et l'album sur www.bedetheque.com, fenêtres de choix de la série / de l'album / de l'édition avec couvertures et fermeture automatique, puis série, numéro, titre, auteurs, éditeur, collection, format, ISBN, dépôt légal, évaluation, résumé, genre, langue, parution, nombre d'albums, lien Web, notes. QuickScrape : lien collé ; option « rescrape » depuis le lien Web. | réglages de l'original en 3 onglets (Préférences, Données, Logging), **import de l'`App.Config` de ComicRack**, « ne remplacer que les champs vides » (ajout) |
| `library_organizer.py` | **Library Organizer**, **Library Organizer (Quick)** (`Books`), **Library Organizer (Quick) – toute la bibliothèque**, **Configurer Library Organizer**, **… annuler le dernier rangement**, **… supprimer les dossiers vides** (`Library`), réglages (`ConfigScript`) | Portage Python 3 de *Library Organizer* 2.1.13 (Stonepaw, Apache 2.0) : profils multiples, modèles de dossier/nom à la syntaxe de l'original, règles (champ/opérateur/valeur, groupes imbriqués, dossiers exclus), modes Déplacer / Copier (simulé) / Simuler, doublons, journal, annulation. Moteur dans `library_organizer_moteur.py` (module, pas un script). | fenêtre de configuration de l'original (Vue d'ensemble, Fichiers, Dossiers, Règles, Options, Jetons), import/export `.lop`, **import du `losettingsx.dat` de ComicRack** |

### Comic Vine

L'API de Comic Vine demande une clé personnelle gratuite (compte sur
<https://comicvine.gamespot.com>, puis <https://comicvine.gamespot.com/api/>) : la coller
dans « Comic Vine : réglages » (saisie masquée, jamais affichée ; laisser vide pour garder
la clé enregistrée ; enregistrée chiffrée sous Windows). La clé n'apparaît dans aucun
message ni journal, même en cas d'erreur. Requêtes utilisées
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

### Bedetheque Scraper 2

Portage du script de ComicRack (mentions (c) de l'original conservées en tête du fichier).
Fichiers de l'original dans `scripts\exemples\bd2\` : `BDTranslations.Config` (textes
français/anglais, option « Langue du plugin »), `bd2.png`, `bd2q.png`. Mêmes réglages (mêmes
noms que l'`App.Config` de l'original : `CBSeries`, `CBStop`, `TIMEPOPUP`, `TIMEOUTS`…),
mêmes expressions régulières (v6.08), mêmes en-têtes HTTP que son `BedethequeFetcher`
(navigateur Chrome, cookies gardés et renouvelés après un 403 ; le Python de ComicRust fait
les requêtes lui-même, sans « daemon »). Option « cloudscraper » : copier le module
`cloudscraper` et ses dépendances dans `scripts\lib`. Journaux `BD2_Rename_Log.txt` et
`BD2_Debug_Log.txt` à côté du script, comme l'original.

« Configurer BD2 » ▸ « Importer les réglages de ComicRack… » : choisir l'`App.Config` du
dossier du script dans ComicRack (par ex.
`%AppData%\cYo\ComicRack\Scripts\Bedetheque Scraper 2\App.Config`).

Différences : la couverture d'une fiche sans fichier n'est pas posée (seulement montrée dans
la fenêtre d'avancement) et le nombre de planches n'est pas écrit (`PageCount` en lecture
seule) ; le double-clic d'une fenêtre de choix choisit au lieu d'ouvrir la page dans le
navigateur ; série et numéro « proposés » (fichier sans série/numéro) déduits du nom du
fichier par le script. Pour les essais : réglage `url_base` ou variable
`COMICRUST_BEDETHEQUE_URL`.

### Library Organizer

Profils enregistrés dans les réglages du script ; import et export au format de l'original
(`losettingsx.dat` : racine `Profiles` et attribut `LastUsed`, ou profil `.lop` exporté ;
format 1.7 `Settings/Setting` relu aussi). Syntaxe des modèles (celle de l'original, pas
celle de la fenêtre « Renommer ») :

| Jeton | Résultat |
|---|---|
| `{préfixe<champ>suffixe}` | valeur du champ entourée du préfixe et du suffixe ; rien si le champ est vide (ou la valeur de substitution des options) |
| `{<number3>}`, `{<year0>}` | nombre complété sur 3 chiffres ; `0` = d'après le dernier numéro de la série |
| `<month>` / `<month#>` / `<month#2>` | mois en texte (noms des options, 13-16 = saisons) / en nombre |
| `<startyear>`, `<startmonth>`, `<startmonth#2>`, `<EndYear>`, `<EndMonth>`, `<firstissuenumber>`, `<lastissuenumber3>` | calculés sur la série (même éditeur, série, volume) parmi les livres transmis |
| `<writer(, )(issue)>`, `<tags(, )(series)>` | champ multiple : choix des valeurs par numéro ou pour la série (fenêtre ; option « un dossier par valeur ») |
| `<manga(Manga)>`, `<seriesComplete(Fini)(!)>` | texte si Oui (avec `(!)` : si Non) |
| `<read(Lu)(=)(100)>` | texte selon le pourcentage lu (`=`, `>`, `<`) |
| `<first(Series)>` | première lettre, sans article (the, le, la, les, der…) |
| `<counter(1)(1)(3)>` | compteur : début, pas, chiffres |
| `<AddedDate(dd/MM/yy)>`, `<ReleasedDate(D)>` | date au format .NET (culture française) |
| `<Custom(clé)>` | valeur personnalisée |
| `{texte<?series(Batman)>}`, `{texte<?series(!^Bat)>}` | conditionnel : préfixe+suffixe si égal / si l'expression correspond |
| `{texte<!series>}`, `{texte<!series(X)>}` | inversé : si vide / si différent |
| `\` (modèle de dossier) | sous-dossier |

Les jetons s'imbriquent (`{ ({<month>, }<year>)}`). Caractères interdits remplacés selon
la table des options (`:` → ` - `, `?` retiré…), espaces multiples réduites, niveaux de
dossier vides retirés ou remplacés, chemin de plus de 259 caractères : fenêtre pour le
raccourcir. Un livre déjà à l'emplacement calculé n'est pas déplacé.

Ce que ComicRust ne permet pas à un script, et ce qui est fait à la place : copier un
fichier (mode Copier → simulation signalée), créer l'image d'un livre sans fichier (échec
signalé), écraser un fichier existant (fenêtre des doublons : « garder les deux » = suffixe
« (1) », ou « ne pas déplacer »), supprimer les dossiers vides pendant l'application
(listés dans le rapport et dans `data\library_organizer\dossiers_vides.txt` ; la commande
« supprimer les dossiers vides » les supprime ensuite, après confirmation, s'ils sont
vraiment vides). L'automatisme « Startup » de l'original devient la commande « Quick –
toute la bibliothèque » (aucun livre n'est transmis au démarrage). Journal (10 derniers)
et `annulation.dat` dans `data\library_organizer\` (ou le dossier choisi dans les options).

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
| `#@Enabled` | `false` : désactivé tant que l'utilisateur ne le coche pas dans « Gérer les scripts » (son choix prime ; conseillé pour un automatisme). Par défaut `true`. |

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

- `comicrust.progress(fraction, texte, detail=None, image=None)` : barre d'avancement
  (0 à 1) ; `detail` : sous-texte sous la barre ; `image` : image affichée à côté
  (couverture de l'album traité : octets JPEG/PNG/WebP/GIF/BMP ou chemin de fichier,
  2 Mo au plus ; `b""` la retire). `None` = inchangé : n'envoyer l'image que quand elle
  change.
- `comicrust.cancelled()` : vrai si l'utilisateur a cliqué sur « Arrêter » — à tester dans
  les boucles. Sans réaction en 1 s, `KeyboardInterrupt` est levée dans le script ; sans
  réaction quelques secondes de plus, le processus est arrêté.
  `comicrust.check_cancelled()` lève `comicrust.Cancelled`.
- `print(...)` et `comicrust.log(..., level="info"|"warning"|"error")` : journal du script.
- `comicrust.message(texte)` : message à l'utilisateur.
- `comicrust.ask(question, défaut)`, `comicrust.ask_choice(question, choix, défaut)`,
  `comicrust.confirm(question, défaut)` : questions (valeur par défaut pour un
  automatisme ou si l'utilisateur ferme la fenêtre).
- `comicrust.ask(question, valeur_actuelle, secret=True)` : saisie masquée (clé d'API,
  mot de passe) ; la valeur actuelle n'est ni affichée ni envoyée à l'interface, une
  réponse vide la garde.
- Valeurs de champ : texte, entier, réel, booléen ou `None`. Un réel non fini (`NaN`,
  infini) lève `ValueError` pour cette seule modification (interceptable) ; si le script
  s'arrête sur une erreur, les modifications valides déjà demandées restent visibles en
  aperçu.
- `comicrust.hook()`, `comicrust.interactive()`, `comicrust.args()`.
- `comicrust.form(...)`, `comicrust.choose(...)` : formulaires et listes de choix avec
  vignettes (section suivante).
- `comicrust.open_url(url)` : ouvre une adresse `http://` ou `https://` dans le
  navigateur (`ValueError` pour toute autre adresse) ; renvoie `False` sans rien ouvrir
  pour un automatisme.
- `comicrust.clipboard_set(texte)` : copie dans le presse-papiers (`False` pour un
  automatisme).

### Fenêtres : formulaires et listes de choix (v0.10.1)

Pour des réglages riches (onglets, listes à cocher, règles…) et le choix d'un album parmi
plusieurs résultats avec sa couverture, comme les fenêtres de Bedetheque Scraper ou de
Library Organizer dans ComicRack.

**Sans interface** (automatismes `NewBooks`, `BookOpened`… : personne pour répondre),
`form` et `choose` **renvoient toujours `None`** sans rien afficher (une ligne est écrite
dans le journal) : prévoir des valeurs par défaut. `None` aussi si l'utilisateur annule,
ferme la fenêtre (Échap) ou clique sur « Arrêter le script ».

#### `comicrust.form`

```python
bouton_et_valeurs = comicrust.form(titre, controles, buttons=("OK", "Annuler"),
                                   values=None, width=None, message=None, cancel=…)
```

- Renvoie `(bouton, valeurs)` : `bouton` = libellé du bouton du bas cliqué, ou clé d'un
  `ui.button` ; `valeurs` = dictionnaire clé → valeur de **tous** les contrôles (y compris
  ceux d'onglets non visités ou désactivés). Ou `None`.
- `buttons` : boutons du bas, le premier est mis en avant ; `cancel` : celui qui annule
  (rend `None`, sans vérification) — par défaut le **dernier** quand il y en a plusieurs,
  `cancel=None` : aucun.
- `values` : valeurs initiales qui remplacent les `default` (pour rouvrir un formulaire
  avec la saisie précédente) ; `width` : largeur en pixels (560 par défaut) ; `message` :
  texte d'introduction. Un formulaire trop haut défile.
- Avant de fermer, ComicRust vérifie les champs **actifs** : champ `required` vide, nombre
  hors de `min`/`max`, doublon dans une `list_edit` ; l'erreur s'affiche sous le champ,
  l'onglet concerné s'ouvre (marqué ⚠), le formulaire reste ouvert.
- Une description invalide (type inconnu, clé en double, `enabled_if` vers une clé
  inconnue, image trop grosse…) lève `comicrust.ScriptError` avec la raison.

Contrôles (`from comicrust import ui`) ; chacun est un simple dictionnaire
(`{"type": "text", "key": …}` s'écrit aussi à la main). Paramètres communs : `key` (nom de
la valeur), `label`, `default`, `help` (infobulle), `enabled_if` (actif seulement si :
`"clé"` cochée ou non vide, `"!clé"`, `"clé=valeur"`, `"clé!=valeur"` ; un conteneur
désactivé désactive son contenu), `width` (pixels).

| Fonction | Valeur rendue | Particularités |
|---|---|---|
| `ui.text(key, label, default="", placeholder=, required=, inserts=)` | `str` | `inserts` : menu « Insérer ▾ » qui insère au curseur, ex. `[("{series}", "Série"), ("{number}", "Numéro")]` (modèles de nom) |
| `ui.textarea(key, label, default="", rows=4, …)` | `str` | plusieurs lignes, `inserts` aussi |
| `ui.secret(key, label, default=valeur_actuelle)` | `str` | saisie masquée ; la valeur actuelle n'est **ni affichée ni envoyée** à l'interface ; vide = valeur actuelle rendue |
| `ui.checkbox(key, label, default=False)` | `bool` | |
| `ui.number(key, label, default=0, min=, max=, step=1, decimals=0)` | `int` (ou `float` si `decimals`) | bornes vérifiées |
| `ui.choice(key, label, choices, default=None, editable=False, required=False)` | valeur du choix | liste déroulante ; `editable` : saisie libre (texte rendu tel quel) |
| `ui.radio(key, label, choices, default=None, horizontal=False)` | valeur du choix | premier choix par défaut |
| `ui.multichoice(key, label, choices, default=(), height=None, required=False)` | `list` des valeurs cochées | liste à cocher avec « Tout » / « Rien » (champs à récupérer…) |
| `ui.list_edit(key, label, default=(), choices=None, unique=True, required=False)` | `list` de `str` | ajouter (Entrée), retirer, monter, descendre ; `choices` : suggestions |
| `ui.table(key, label, columns, default=())` | `list` de lignes (`list`) | tableau éditable : `ui.column(titre, type="text"|"number"|"checkbox"|"choice", choices=, width=)` ; ajouter, retirer, ordonner les lignes |
| `ui.folder(key, label, default="", required=)` | `str` | bouton « Parcourir… » |
| `ui.file(key, label, default="", filters=[("Images", ["jpg", "png"])], save=False)` | `str` | `save=True` : « Enregistrer sous » |
| `ui.label(texte, style=None)` | — | `style` : `"bold"`, `"small"`, `"weak"`, `"heading"`, `"warning"`, `"error"` |
| `ui.separator()` | — | |
| `ui.image(octets_ou_chemin, caption=None, max_width=240, max_height=320)` | — | couverture… (2 Mo au plus) |
| `ui.group(titre, [contrôles])` | — | cadre titré |
| `ui.columns([contrôles], [contrôles], …)` | — | colonnes côte à côte (2 couvertures à comparer…) |
| `ui.tabs({"Onglet": [contrôles], …})` | — | onglets |
| `ui.button(key, label, validate=False)` | — | ferme le formulaire et rend `(key, valeurs)` (voir plus bas) |

`choices` : liste de textes, de paires `(valeur, libellé)` ou dictionnaire
`{valeur: libellé}`.

Exemple (réglages à onglets, à la manière de Bedetheque Scraper) :

```python
import comicrust
from comicrust import ui

CHAMPS = ["Series", "Title", "Writer", "Penciller", "Colorist", "Summary", "Genre"]

#@Name Réglages
#@Hook ConfigScript
def reglages():
    s = comicrust.settings()
    r = comicrust.form("Réglages du scraper", [
        ui.tabs({
            "Options": [
                ui.radio("langue", "Langue", [("fr", "Français"), ("en", "Anglais")], horizontal=True),
                ui.checkbox("debug", "Journal de débogage"),
                ui.number("log_max", "Taille max du journal (Ko)", 500, min=10, max=10000, enabled_if="debug"),
                ui.choice("choix", "Choix de l'album", [("0", "Jamais"), ("1", "Toujours"),
                                                        ("2", "Automatique après délai")]),
                ui.number("delai", "Délai (s)", 5, min=1, max=60, enabled_if="choix=2"),
                ui.secret("cle", "Clé d'API", default=s.get("cle")),
            ],
            "Champs": [ui.multichoice("champs", "Champs à récupérer", CHAMPS, default=CHAMPS)],
        }),
    ], values=s)
    if r is None:
        return                      # annulé : rien n'est changé
    bouton, valeurs = r
    s.update(valeurs)
    s.mark_secret("cle")
```

**Formulaire dynamique** (profils de Library Organizer : nouveau, renommer, supprimer…) :
un `ui.button` ferme le formulaire et rend sa clé avec les valeurs saisies ; le script
fait l'action et rouvre le formulaire avec `values=` :

```python
valeurs = {"profil": "Défaut"}
while True:
    profils = sorted(tous_les_profils)
    r = comicrust.form("Library Organizer", [
        ui.choice("profil", "Profil", profils),
        ui.button("nouveau", "Nouveau profil…"),
        ui.text("modele", "Modèle", "{publisher}/{series}", required=True,
                inserts=[("{series}", "Série"), ("{number}", "Numéro"), ("{year}", "Année")]),
        ui.table("regles", "Ne pas déplacer si", [
            ui.column("Champ", "choice", choices=["Series", "Publisher", "Tags"]),
            ui.column("Opérateur", "choice", choices=["est", "contient"]),
            ui.column("Valeur")]),
        ui.list_edit("exceptions", "Dossiers vides à garder"),
    ], buttons=("Enregistrer", "Fermer"), values=valeurs)
    if r is None:
        break
    bouton, valeurs = r
    if bouton == "nouveau":
        nom = comicrust.ask("Nom du nouveau profil :")
        ...                         # créer le profil, puis la boucle rouvre le formulaire
        continue
    enregistrer(valeurs)
    break
```

Fenêtre des doublons (deux couvertures côte à côte et des boutons) :

```python
r = comicrust.form("Ce fichier existe déjà", [
    ui.columns(
        [ui.label("À déplacer", style="bold"), ui.image(couv_nouveau), ui.label(infos_nouveau)],
        [ui.label("Déjà présent", style="bold"), ui.image(couv_ancien), ui.label(infos_ancien)],
    ),
    ui.checkbox("tous", "Faire de même pour tous les conflits"),
], buttons=("Remplacer", "Garder les deux", "Ne pas déplacer", "Annuler"))
```

#### `comicrust.choose`

```python
rang = comicrust.choose(titre, colonnes, lignes, images=None, multi=False, default=None,
                        message=None, filter=None, timeout=None, thumb_size=None)
```

- Tableau **triable** (clic sur un titre de colonne : numérique si les cellules sont des
  nombres, sinon ordre « naturel » 2 < 10), avec une colonne de **vignettes** si `images`
  est donné ; survoler une vignette l'agrandit, l'image de la ligne survolée ou choisie
  est aussi montrée à droite. **Double-clic** (ou Entrée) = choisir.
- `colonnes` : titres ; `lignes` : une liste de cellules par ligne (ou un simple texte).
- `images` : une par ligne, `None` pour aucune : octets JPEG/PNG/WebP/GIF/BMP ou chemin de
  fichier ; **2 Mo au plus chacune et 12 Mo en tout** (`ValueError` sinon, avant tout
  envoi). Elles sont décodées et réduites par ComicRust, hors de l'interface.
- Renvoie le **rang** de la ligne choisie dans `lignes` (quel que soit le tri ou le
  filtre), la liste des rangs avec `multi=True` (lignes cochées par clic), ou `None`.
- `default` : rang (ou liste) choisi au départ ; `filter` : champ de filtre (tous les mots,
  sans casse ; par défaut au-delà de 8 lignes) ; `timeout` : secondes après lesquelles la
  sélection est validée toute seule (option « choix automatique » de Bedetheque Scraper ;
  toute action de l'utilisateur arrête le compte à rebours ; sans sélection : `None`) ;
  `thumb_size` : hauteur des vignettes (64 px).

```python
albums = chercher(book.series)               # [(titre, numéro, année, url_couv, url), …]
couvs = [comicrust.http_get(a[3]) if a[3] else None for a in albums]
i = comicrust.choose("Quel album ?", ["Titre", "N°", "Année"],
                     [a[:3] for a in albums], images=couvs, default=0,
                     timeout=10 if reglages["choix"] == "2" else None)
if i is None:
    return                                   # ignoré
album = albums[i]
```

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
  s'affiche mais « Appliquer » est refusé, avec la marche à suivre ; les automatismes ne
  sont pas lancés (signalé une fois par session).
- Chemins des renommages : normalisés avant l'aperçu, et c'est ce texte qui est écrit dans
  la fiche (ComicRack compare les chemins comme du texte : `D:\BD\x.cbz` et `D:/BD//x.cbz`
  seraient deux livres). Séparateur `\` pour un chemin Windows (lettre de lecteur ou
  partage `\\serveur\partage`), `/` sinon ; refusés : `.` et `..`, chemin Windows sans
  lecteur ni partage (`\BD\…`). Sous la même racine que le livre, l'écriture d'origine est
  gardée (casse comprise : un livre en `\\SERVEURNAS\BD\…` reste en `\\SERVEURNAS\BD\…`).
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
    s["api_key"] = comicrust.ask("Clé d'API Comic Vine :", s.get("api_key", ""), secret=True)
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
  d'origine (script → ComicRust). `print()` est redirigé vers le journal (stderr). Le
  protocole porte un numéro de version (2 depuis la v0.10.1 : formulaires, listes de
  choix, `open_url`, presse-papiers, image de progression) : un module `comicrust.py` plus
  ancien ou plus récent que ComicRust est refusé avec un message clair. Les images
  voyagent en base64 (un message fait 20 Mo au plus).
- Le module `comicrust.py` est intégré à ComicRust et écrit à chaque lancement dans
  `data\python\` à côté de `ComicRust.exe` (ComicRust est portable : rien n'est écrit
  hors de son dossier) ; ne pas le modifier.
- Fermeture de ComicRust pendant un script : le script est arrêté (annulation, puis arrêt
  forcé de Python) ; pendant « Appliquer », ComicRust attend la fin du livre en cours
  (« Application des modifications en cours… ») et se ferme ensuite : un fichier déplacé a
  toujours sa fiche à jour. Si ComicRust disparaît (plantage), Python s'arrête de lui-même
  (fin du dialogue détectée) ; sous Windows, Python et les programmes lancés par le script
  sont dans un « Job Object » fermé avec ComicRust.
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
