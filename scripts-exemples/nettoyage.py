# -*- coding: utf-8 -*-
"""Nettoyage des fiches (script d'exemple fourni avec ComicRust).

Sur les livres sélectionnés (ou toute la bibliothèque) :

- espaces en trop retirés dans les champs texte (« Les  Tuniques bleues » → « Les
  Tuniques bleues ») ;
- listes de noms (auteurs, genres, personnages, étiquettes…) remises en forme :
  « Goscinny ;Uderzo,  Goscinny » → « Goscinny, Uderzo » (doublons retirés) ;
- numéro normalisé : « 001 » → « 1 », « #12 » → « 12 », « T03 » → « 3 » ; « 1.5 »,
  « ½ », « 12a » et « -1 » sont gardés tels quels (à part les zéros de tête) ;
- série, numéro, année et titre vides complétés depuis le nom du fichier quand il est
  clair (« Blacksad T03 - Âme rouge (2005).cbz ») ;
- en option, casse des séries écrites TOUT EN MAJUSCULES ou tout en minuscules
  (« LES TUNIQUES BLEUES » → « Les Tuniques Bleues ») ; une série en casse mixte
  (« iZombie », « X-Men ») n'est jamais touchée.

Rien n'est écrit tout de suite : ComicRust montre l'aperçu des modifications et
l'utilisateur les accepte ou non. Aucun fichier n'est modifié ni supprimé (les fiches
seulement, dans la bibliothèque).

Réglages : bouton « Réglages » du script (fonction ``reglages`` ci-dessous).
"""

import os
import re

import comicrust

# Réglages par défaut (modifiables par le bouton « Réglages »).
DEFAUTS = {
    "casse_series": False,  # corriger la casse des séries tout en majuscules/minuscules
    "numero": True,  # normaliser le numéro
    "depuis_nom": True,  # compléter série/numéro/année/titre vides depuis le nom du fichier
}

# Champs texte simples : espaces en trop retirés.
CHAMPS_TEXTE = [
    "Series", "Title", "Publisher", "Imprint", "StoryArc", "SeriesGroup", "AlternateSeries",
    "Format", "AgeRating", "MainCharacterOrTeam", "Web", "BookOwner", "BookStore",
    "BookLocation", "BookCondition", "BookAge", "BookCollectionStatus", "ScanInformation",
]

# Champs « liste » (valeurs séparées par des virgules dans ComicRack).
CHAMPS_LISTE = [
    "Writer", "Penciller", "Inker", "Colorist", "Letterer", "CoverArtist", "Editor",
    "Translator", "Genre", "Characters", "Teams", "Locations", "Tags",
]

# Champs texte longs : seules les lignes vides en série et les espaces de fin sont retirés.
CHAMPS_LONGS = ["Summary", "Notes", "Review", "BookNotes"]


# ---------------------------------------------------------------------------------------
# Réglages
# ---------------------------------------------------------------------------------------

def _reglage(nom):
    return comicrust.settings().get(nom, DEFAUTS[nom])


#@Name Nettoyage : réglages
#@Hook ConfigScript
#@Key nettoyage-reglages
#@Description Choisir les corrections faites par « Nettoyer les fiches ».
def reglages():
    s = comicrust.settings()
    s["numero"] = comicrust.confirm(
        "Normaliser les numéros (« 001 » → « 1 », « #12 » → « 12 ») ?", _reglage("numero"))
    s["depuis_nom"] = comicrust.confirm(
        "Compléter série, numéro, année et titre vides depuis le nom du fichier ?",
        _reglage("depuis_nom"))
    s["casse_series"] = comicrust.confirm(
        "Corriger la casse des séries écrites tout en MAJUSCULES ou tout en minuscules ?",
        _reglage("casse_series"))
    s.save()


# ---------------------------------------------------------------------------------------
# Outils (utilisables depuis un autre script : « from nettoyage import analyser_nom »)
# ---------------------------------------------------------------------------------------

def espaces(texte):
    """Retire les espaces en trop (début, fin, doublés) ; ``None`` si le texte est vide."""
    if texte is None:
        return None
    t = " ".join(str(texte).split())
    return t or None


def liste(texte):
    """« a ;b,  a » → « a, b » : séparateurs , ou ; doublons retirés (sans casse)."""
    if texte is None:
        return None
    vus = set()
    sortie = []
    for morceau in re.split(r"[,;]", str(texte)):
        m = espaces(morceau)
        if m and m.casefold() not in vus:
            vus.add(m.casefold())
            sortie.append(m)
    return ", ".join(sortie) or None


def texte_long(texte):
    """Espaces de fin de ligne retirés, au plus une ligne vide de suite."""
    if texte is None:
        return None
    lignes = [l.rstrip() for l in str(texte).replace("\r\n", "\n").split("\n")]
    t = re.sub(r"\n{3,}", "\n\n", "\n".join(lignes)).strip()
    return t or None


def normaliser_numero(numero):
    """« 001 » → « 1 », « #12 » → « 12 », « T03 » / « Tome 3 » → « 3 », « n°7 » → « 7 ».

    Gardés : « 1.5 », « 1,5 », « ½ », « 12a » (zéros de tête retirés : « 007b » → « 7b »),
    « -1 », « 0 ». Un numéro qui ne commence pas par un chiffre (« HS », « Annual 2 »)
    n'est pas modifié."""
    if numero is None:
        return None
    s = espaces(numero)
    if s is None:
        return None
    s = re.sub(r"^(?:#|n°|no\.?(?=\s*\d)|tome(?=\s*\d)|t(?=\d))\s*", "", s, flags=re.IGNORECASE)
    m = re.match(r"^(-?)0*(\d+)(.*)$", s)
    if m:
        s = m.group(1) + m.group(2) + m.group(3)
    return s


# Mots laissés en minuscules (sauf en début de titre).
PETITS_MOTS = {
    "de", "du", "des", "la", "le", "les", "et", "à", "au", "aux", "en", "un", "une", "sur",
    "pour", "par", "dans", "ou", "of", "the", "and", "a", "an", "in", "on", "to", "for",
}
# Élisions : « l'ombre », « d'Arabie », « qu'il ».
ELISIONS = {"l", "d", "j", "m", "n", "s", "t", "c", "qu", "jusqu", "lorsqu", "puisqu"}
ROMAIN = re.compile(r"(?=[IVX])X{0,3}(?:IX|IV|V?I{0,3})")


def casse_titre(texte):
    """Casse de titre pour un texte tout en MAJUSCULES ou tout en minuscules ; un texte en
    casse mixte est rendu tel quel (« iZombie », « X-Men », « Spirou et Fantasio »)."""
    if not texte or (texte != texte.upper() and texte != texte.lower()):
        return texte
    mots = texte.split(" ")
    sortie = []
    for i, mot in enumerate(mots):
        if ROMAIN.fullmatch(mot.upper()) and mot.isupper() and len(mot) > 1:
            sortie.append(mot)  # chiffres romains : « Star Wars II »
            continue
        bas = mot.lower()
        if i > 0 and bas in PETITS_MOTS:
            sortie.append(bas)
            continue
        # Morceaux séparés par - ou ' : « SPIDER-MAN » → « Spider-Man », « L'INCAL » → « L'Incal ».
        morceaux = re.split(r"([-'’])", bas)
        for j in range(0, len(morceaux), 2):
            p = morceaux[j]
            suivi_apostrophe = j + 1 < len(morceaux) and morceaux[j + 1] in "'’"
            if suivi_apostrophe and p in ELISIONS and (i > 0 or j > 0):
                continue  # « de l'ombre » : élision en minuscules
            morceaux[j] = p[:1].upper() + p[1:]
        sortie.append("".join(morceaux))
    return " ".join(sortie)


# « Série - T03 - Titre », « Série #12 », « Série 001 », « Série - 12 - Titre ».
_MOTIF_NOM = re.compile(
    r"^(?P<serie>.+?)(?:\s*[-–]\s*|\s+)"
    r"(?:(?:tome|vol\.?|volume|t|v|n°|no\.?)\s*|#\s*)?"
    r"(?P<numero>\d+(?:[.,]\d+)?|½)"
    r"(?:\s*[-–:]\s*(?P<titre>.+?))?\s*$",
    re.IGNORECASE,
)


def analyser_nom(chemin):
    """Devine ``{"series", "number", "year", "title"}`` depuis le nom du fichier (clés
    absentes si rien de sûr). Les groupes entre parenthèses ou crochets (« (2012) »,
    « (Digital) », « [Team] ») sont écartés ; une année entre parenthèses est retenue.

    >>> analyser_nom("Blacksad T03 - Âme rouge (2005).cbz")
    {'year': 2005, 'series': 'Blacksad', 'number': '3', 'title': 'Âme rouge'}
    """
    info = {}
    if not chemin:
        return info
    nom = os.path.splitext(os.path.basename(str(chemin).replace("\\", "/")))[0]
    m = re.search(r"[(\[]((?:19|20)\d{2})(?:[-./]\d{1,2})*[)\]]", nom)
    if m:
        info["year"] = int(m.group(1))
    reste = re.sub(r"\s*[(\[][^)\]]*[)\]]", " ", nom).replace("_", " ")
    reste = " ".join(reste.split())
    m = _MOTIF_NOM.match(reste)
    if m:
        info["series"] = m.group("serie").strip(" -–")
        info["number"] = normaliser_numero(m.group("numero"))
        if m.group("titre"):
            info["title"] = m.group("titre").strip()
    return info


def vide(valeur):
    """Champ vide : ``None``, texte vide, ou nombre négatif (« -1 » de ComicRack)."""
    if valeur is None:
        return True
    if isinstance(valeur, str):
        return not valeur.strip()
    if isinstance(valeur, (int, float)) and not isinstance(valeur, bool):
        return valeur < 0
    return False


# ---------------------------------------------------------------------------------------
# Script
# ---------------------------------------------------------------------------------------

def nettoyer_livre(book, casse_series, numero, depuis_nom):
    """Met en attente les corrections d'un livre ; renvoie le nombre de champs modifiés."""
    avant = len(book.changes)
    for champ in CHAMPS_TEXTE:
        v = book.get(champ)
        if isinstance(v, str):
            book.set(champ, espaces(v))
    for champ in CHAMPS_LISTE:
        v = book.get(champ)
        if isinstance(v, str):
            book.set(champ, liste(v))
    for champ in CHAMPS_LONGS:
        v = book.get(champ)
        if isinstance(v, str):
            book.set(champ, texte_long(v))
    langue = book.get("LanguageISO")
    if isinstance(langue, str) and langue != langue.strip().lower():
        book.set("LanguageISO", langue.strip().lower() or None)

    if numero:
        for champ in ("Number", "AlternateNumber"):
            v = book.get(champ)
            if isinstance(v, str):
                book.set(champ, normaliser_numero(v))

    if depuis_nom and book.file:
        info = analyser_nom(book.file)
        # Le titre n'est repris que si la série du nom correspond à celle de la fiche
        # (ou si la fiche n'a pas de série) : évite un faux titre sur un nom inhabituel.
        serie = book.get("Series")
        meme_serie = vide(serie) or (
            info.get("series", "").casefold() == str(serie).casefold())
        if "series" in info and vide(serie):
            book.set("Series", info["series"])
        if meme_serie:
            if "number" in info and vide(book.get("Number")):
                book.set("Number", info["number"])
            if "title" in info and vide(book.get("Title")):
                book.set("Title", info["title"])
        if "year" in info and vide(book.get("Year")):
            book.set("Year", info["year"])

    if casse_series:
        for champ in ("Series", "AlternateSeries", "SeriesGroup"):
            v = book.get(champ)
            if isinstance(v, str):
                book.set(champ, casse_titre(v))
    return len(book.changes) - avant


#@Name Nettoyer les fiches
#@Hook Books, Library
#@Key nettoyage
#@Description Espaces en trop, listes d'auteurs, numéros (« 001 » → « 1 »), champs vides
#@Description complétés depuis le nom du fichier, casse des séries (en option).
#@Description Aperçu avant application ; aucun fichier n'est modifié.
def nettoyer(books):
    casse_series = bool(_reglage("casse_series"))
    numero = bool(_reglage("numero"))
    depuis_nom = bool(_reglage("depuis_nom"))
    total = len(books)
    modifies = 0
    for i, book in enumerate(books):
        if comicrust.cancelled():
            comicrust.log("Arrêt demandé : %d livres examinés sur %d." % (i, total), level="warning")
            break
        if i % 50 == 0:
            comicrust.progress(i / max(total, 1), book.series or os.path.basename(book.file or ""))
        if nettoyer_livre(book, casse_series, numero, depuis_nom):
            modifies += 1
    comicrust.log("%d livre(s) à corriger sur %d." % (modifies, total))
    return {"examines": total, "a_corriger": modifies}
