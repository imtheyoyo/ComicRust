# -*- coding: utf-8 -*-
"""Renommer et ranger les fichiers selon un modèle (script d'exemple fourni avec ComicRust).

Réglages (bouton « Réglages » du script) :

- **modèle de nom**, même syntaxe que la fenêtre « Renommer » de ComicRust (et que le
  format d'export de ComicRack) : ``{series}[ v{volume}][ #{number}][ ({year})]``
  - ``{champ}`` : valeur du champ (``series``, ``number``, ``volume``, ``year``, ``month``,
    ``title``, ``publisher``, ``imprint``, ``writer``, ``format``, ``language``,
    ``filename`` = nom actuel sans extension, et tout autre champ par son nom ComicRack) ;
  - ``{number:000}`` : nombre complété par des zéros (``007``) ;
  - ``[ … ]`` : partie omise si un champ qu'elle contient est vide ;
  - ``/`` : sous-dossier (``{publisher}/{series}/{series} #{number:000}``) ;
  - ``\\x`` : caractère ``x`` tel quel ;
- **dossier racine** : où ranger les fichiers (vide = laisser chaque fichier dans son
  dossier actuel, simple renommage) ;
- **sous-dossiers** : aucun, par série, ou par éditeur puis série.

Prudence :

- rien n'est déplacé tout de suite : les renommages sont **mis en attente** et ComicRust
  montre l'aperçu (ancien → nouveau chemin) avant de les appliquer ; il refuse lui-même
  une destination déjà prise ;
- conflits traités ici aussi : deux livres qui donneraient le même nom, ou un fichier
  déjà présent à la destination → suffixe « (2) », « (3) »… (jamais d'écrasement) ;
- aucun fichier n'est supprimé ; un livre sans série (ni dans la fiche, ni dans le nom
  du fichier) est laissé en place.
"""

import os
import re

import comicrust

MODELE_DEFAUT = "{series}[ v{volume}][ #{number}][ ({year})]"
SOUS_DOSSIERS = ["aucun", "série", "éditeur/série"]
DEFAUTS = {"modele": MODELE_DEFAUT, "racine": "", "sous_dossiers": "aucun"}

# Longueur maximale d'un chemin (ComicRack, programme .NET Framework, ne gère pas plus).
MAX_CHEMIN = 259
# Noms réservés de Windows.
RESERVES = {"CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"} | {
    "%s%d" % (p, i) for p in ("COM", "LPT") for i in range(1, 10)}


def _reglage(nom):
    v = comicrust.settings().get(nom)
    return DEFAUTS[nom] if v is None else v


#@Name Ranger les fichiers : réglages
#@Hook ConfigScript
#@Key ranger-fichiers-reglages
#@Description Modèle de nom, dossier racine et sous-dossiers.
def reglages():
    s = comicrust.settings()
    modele = comicrust.ask(
        "Modèle de nom (ex. {series}[ v{volume}][ #{number:000}][ ({year})]) :", _reglage("modele"))
    s["modele"] = (modele or "").strip() or MODELE_DEFAUT
    s["racine"] = (comicrust.ask(
        "Dossier racine (vide = garder le dossier actuel de chaque fichier) :",
        _reglage("racine")) or "").strip()
    choix = comicrust.ask_choice("Sous-dossiers :", SOUS_DOSSIERS, _reglage("sous_dossiers"))
    s["sous_dossiers"] = choix if choix in SOUS_DOSSIERS else "aucun"
    s.save()


# ---------------------------------------------------------------------------------------
# Modèles de nom (même règles que comicrust-core, edit/files.rs)
# ---------------------------------------------------------------------------------------

def nettoyer_nom(texte):
    """Texte utilisable comme nom de fichier Windows : ``:`` → `` -``, ``/ \\ |`` → ``-``,
    ``"`` → ``'``, ``< > ? *`` et caractères de contrôle retirés, espaces réduits, points
    et espaces finaux retirés, nom réservé suffixé de ``_``, 200 caractères au plus."""
    sortie = []
    for c in str(texte):
        if c == ":":
            sortie.append(" -")
        elif c in "/\\|":
            sortie.append("-")
        elif c == '"':
            sortie.append("'")
        elif c in "<>?*":
            continue
        elif ord(c) < 32:
            sortie.append(" ")
        else:
            sortie.append(c)
    t = " ".join("".join(sortie).split())[:200]
    t = t.rstrip(". ").lstrip()
    if t.split(".")[0].rstrip().upper() in RESERVES:
        t += "_"
    return t


def _depuis_nom(chemin):
    """Série, numéro, volume, année devinés depuis le nom du fichier (valeurs proposées,
    utilisées quand la fiche est vide, comme ComicRust)."""
    info = {}
    nom = os.path.splitext(os.path.basename(str(chemin or "").replace("\\", "/")))[0]
    m = re.search(r"[(\[]((?:19|20)\d{2})(?:[-./]\d{1,2})*[)\]]", nom)
    if m:
        info["year"] = m.group(1)
    reste = " ".join(re.sub(r"\s*[(\[][^)\]]*[)\]]", " ", nom).replace("_", " ").split())
    m = re.match(
        r"^(?P<s>.+?)(?:\s*[-–]\s*|\s+)(?:v(?P<v>\d+)\s+)?(?:(?:tome|t|n°|no\.?)\s*|#\s*)?"
        r"(?P<n>\d+(?:[.,]\d+)?|½)(?:\s*[-–:]\s*.+?)?\s*$", reste, re.IGNORECASE)
    if m:
        info["series"] = m.group("s").strip(" -–")
        info["number"] = re.sub(r"^0+(?=\d)", "", m.group("n"))
        if m.group("v"):
            info["volume"] = m.group("v")
    return info


def _texte(v):
    if v is None or isinstance(v, bool):
        return "" if v is None else ("Yes" if v else "No")
    if isinstance(v, (int, float)):
        if v < 0:
            return ""  # -1 = vide dans ComicRack
        return str(int(v)) if float(v).is_integer() else str(v)
    return str(v).strip()


def valeur_champ(book, nom, proposees=None):
    """Valeur (texte) d'un champ du modèle pour ``book`` ; ``""`` si vide."""
    nom = nom.strip().lower()
    if nom == "filename":
        return os.path.splitext(os.path.basename(str(book.file or "").replace("\\", "/")))[0]
    if nom == "language":
        nom = "languageiso"
    fiche = {k.lower(): v for k, v in book.to_dict().items()}
    v = _texte(fiche.get(nom))
    if not v and nom in ("series", "number", "volume", "year"):
        if proposees is None:
            proposees = _depuis_nom(book.file)
        v = proposees.get(nom, "")
    return v


def _partie(car, i, ouvrant, fermant):
    """Contenu entre ``car[i] == ouvrant`` et la fermeture correspondante ; renvoie
    ``(contenu, indice de la fermeture)``."""
    prof = 0
    debut = i + 1
    while i < len(car):
        c = car[i]
        if c == "\\":
            i += 2
            continue
        if c == ouvrant:
            prof += 1
        elif c == fermant:
            prof -= 1
            if prof == 0:
                return "".join(car[debut:i]), i
        i += 1
    return "".join(car[debut:]), i


def _format_valeur(spec, lire):
    nom, _, fmt = spec.partition(":")
    v = lire(nom)
    if v and fmt and set(fmt) == {"0"}:
        try:
            x = float(v.replace(",", "."))
            r = int(abs(x) + 0.5)  # arrondi « au plus loin de zéro », comme .NET
            return ("-" if x < 0 and r else "") + str(r).zfill(len(fmt))
        except ValueError:
            pass
    return v


def _developper(modele, lire):
    """Rend ``(texte, réussi)`` ; « réussi » = aucun champ vide (règle de ComicRack : une
    partie ``[…]`` imbriquée réussie rend la partie englobante réussie)."""
    car = list(modele)
    sortie = []
    ok = True
    i = 0
    while i < len(car):
        c = car[i]
        if c == "[":
            dedans, i = _partie(car, i, "[", "]")
            texte, ok2 = _developper(dedans, lire)
            if ok2:
                sortie.append(texte)
            ok = ok or ok2
        elif c == "{":
            spec, i = _partie(car, i, "{", "}")
            v = _format_valeur(spec, lire)
            ok = ok and bool(v)
            sortie.append(v)
        elif c == "\\":
            if i + 1 < len(car):
                sortie.append(car[i + 1])
            i += 1
        else:
            sortie.append(c)
        i += 1
    return "".join(sortie), ok


def chemin_relatif(modele, book):
    """Morceaux du chemin (dossiers…, nom sans extension) d'après le modèle, chaque
    valeur nettoyée (un « / » dans une série ne crée pas de dossier). Liste vide si le
    modèle ne donne rien pour ce livre."""
    proposees = _depuis_nom(book.file)
    texte, _ = _developper(modele, lambda n: nettoyer_nom(valeur_champ(book, n, proposees)))
    morceaux = [nettoyer_nom(p) for p in re.split(r"[/\\]", texte)]
    return [p for p in morceaux if p and p not in (".", "..")]


# ---------------------------------------------------------------------------------------
# Script
# ---------------------------------------------------------------------------------------

def _cle(chemin):
    """Clé de comparaison de chemins (casse ignorée sous Windows)."""
    return os.path.normcase(os.path.normpath(chemin))


def _meme_fichier(a, b):
    try:
        return os.path.samefile(a, b)
    except OSError:
        return False


def destination_libre(chemin, source, pris):
    """``chemin``, ou ``… (2)``, ``… (3)``… si la place est prise (par un autre livre de
    cette exécution ou par un fichier existant). ``None`` après 99 essais."""
    base, ext = os.path.splitext(chemin)
    for n in range(1, 100):
        essai = chemin if n == 1 else "%s (%d)%s" % (base, n, ext)
        cle = _cle(essai)
        if cle in pris:
            continue
        if os.path.lexists(essai) and not _meme_fichier(essai, source):
            continue
        return essai
    return None


#@Name Ranger les fichiers
#@Hook Books
#@Key ranger-fichiers
#@Description Renomme/déplace les fichiers selon le modèle des réglages (aperçu avant
#@Description application ; conflits suffixés « (2) » ; rien n'est supprimé).
def ranger(books):
    modele = _reglage("modele") or MODELE_DEFAUT
    racine = (_reglage("racine") or "").strip()
    sous = _reglage("sous_dossiers")
    comicrust.log("Modèle : %s ; racine : %s ; sous-dossiers : %s"
                  % (modele, racine or "(dossier actuel)", sous))
    pris = set()
    a_renommer = inchanges = ignores = 0
    total = len(books)
    for i, book in enumerate(books):
        if comicrust.cancelled():
            comicrust.log("Arrêt demandé.", level="warning")
            break
        comicrust.progress(i / max(total, 1), book.series or "")
        source = book.file
        if not source:
            ignores += 1
            continue
        nom = os.path.basename(source)
        if not valeur_champ(book, "series"):
            comicrust.log("Sans série, laissé en place : %s" % nom, level="warning")
            ignores += 1
            continue
        morceaux = chemin_relatif(modele, book)
        if not morceaux:
            comicrust.log("Le modèle donne un nom vide : %s" % nom, level="warning")
            ignores += 1
            continue
        dossier = racine or os.path.dirname(source)
        if sous == "série":
            morceaux.insert(0, nettoyer_nom(valeur_champ(book, "series")) or "Sans série")
        elif sous == "éditeur/série":
            morceaux.insert(0, nettoyer_nom(valeur_champ(book, "series")) or "Sans série")
            morceaux.insert(0, nettoyer_nom(valeur_champ(book, "publisher")) or "Éditeur inconnu")
        ext = os.path.splitext(source)[1]
        cible = os.path.join(dossier, *morceaux) + ext
        if _cle(cible) == _cle(source) and os.path.basename(cible) == os.path.basename(source):
            inchanges += 1
            pris.add(_cle(cible))
            continue
        libre = destination_libre(cible, source, pris)
        if libre is None:
            comicrust.log("Aucun nom libre pour %s" % cible, level="warning")
            ignores += 1
            continue
        if len(libre) > MAX_CHEMIN:
            comicrust.log("Chemin trop long (%d caractères), ignoré : %s" % (len(libre), libre),
                          level="warning")
            ignores += 1
            continue
        if libre != cible:
            comicrust.log("Conflit : %s → %s" % (os.path.basename(cible), os.path.basename(libre)),
                          level="warning")
        pris.add(_cle(libre))
        comicrust.rename_file(book, libre)
        a_renommer += 1
    comicrust.log("%d à renommer, %d déjà bien nommé(s), %d ignoré(s)." % (a_renommer, inchanges, ignores))
    return {"a_renommer": a_renommer, "inchanges": inchanges, "ignores": ignores}
