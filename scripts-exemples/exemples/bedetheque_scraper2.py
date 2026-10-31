# -*- coding: utf-8 -*-
#
# Bedetheque Scraper 2 - Avril 2021- v 5.4 -> by kiwi13 & maforget
#
# Original work by Franck (c) - revised by Mizio66 (c)
#
# Thanks to atagal, Bert and Bruno for the thorough testing !
#
# You are free to modify, enhance (possibly) and distribute this file, but only if mentioning the (c) part
#
# (C)'2011 Franck (C)'2007-2017 cYo Soft & (C)'2008 DouglasBubbletrousers & (C)'2008 wadegiles & (C)'2010 cbanack & (C)'2017 Mizio66 & (C)'2021 kiwi13 & maforget
#
# Version d'origine : 6.08 (https://github.com/maforget/Bedetheque-Scrapper-2, script
# IronPython 2 / WinForms pour ComicRack).
#
# Portage en Python 3 pour ComicRust (https://github.com/imtheyoyo/ComicRust), 2026 :
# mêmes options, mêmes analyses des pages de bedetheque.com, mêmes fenêtres (réglages à
# onglets, choix de la série / de l'album / de l'édition, lien direct), au travers de l'API
# `comicrust` (formulaires, listes de choix avec vignettes, modifications montrées en
# aperçu avant application). Les mentions (c) ci-dessus sont conservées comme l'exige la
# licence de l'original.
"""Bedetheque Scraper 2 — portage Python 3 pour ComicRust.

Récupère sur https://www.bedetheque.com les informations des BD sélectionnées : série,
numéro, titre, scénaristes, dessinateurs, coloristes, lettreurs, encreurs, artistes de
couverture, éditeur, collection, format, ISBN, dépôt légal, évaluation, résumé (série et
album, info édition), genre, langue, état de parution, nombre d'albums, lien Web, notes.

Points d'entrée (comme l'original) :

- « Bedetheque Scraper 2 » (hooks ``Books`` et ``Editor``) : recherche par le nom de la
  série puis le numéro (``BD_start``) ; avec l'option « rescrape », relit les fiches à
  partir de leur lien Web ;
- « QuickScrape BD2 » (``Books``) : demande un lien bedetheque.com (série, album ou
  revue) pour chaque livre ;
- « Configurer BD2 » (``ConfigScript`` et ``Library``) : réglages à trois onglets
  (Préférences, Données, Logging), avec un bouton d'import du fichier ``App.Config`` de
  l'original (réglages de ComicRack).

Rien n'est écrit directement : les champs sont demandés à ComicRust, qui les montre en
aperçu (menu) ou remplit la fenêtre « Infos du livre » (hook ``Editor``).

Fichiers à côté du script, dans ``bd2/`` : ``BDTranslations.Config`` (textes français et
anglais de l'original, option « Langue du plugin »), ``bd2.png`` et ``bd2q.png`` (icônes).
Les journaux de l'original (``BD2_Rename_Log.txt``, ``BD2_Debug_Log.txt``) sont écrits dans
``data\\bedetheque\\`` à côté de ComicRust.exe (ou dans le dossier du réglage
``dossier_journaux``) ; leur contenu va aussi dans le journal du script de ComicRust.
Couvertures des fenêtres de choix : chargées pendant que la fenêtre est ouverte (survol
d'abord), gardées dans ``cache\\`` de ce dossier (borné, voir ``CacheDisque``).

Réglages avancés (``comicrust.settings()``, sans interface) : ``url_base`` (autre adresse du
site, pour les essais ; aussi variable d'environnement ``COMICRUST_BEDETHEQUE_URL``),
``dossier_journaux``.
"""

import datetime
import hashlib
import html
import http.cookiejar
import json
import os
import re
import ssl
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET

import comicrust
from comicrust import ui

VERSION = "6.08"
DOSSIER = os.path.dirname(os.path.abspath(__file__))
FICHIERS = os.path.join(DOSSIER, "bd2")
URL_SITE = "https://www.bedetheque.com"

# Réglages et valeurs par défaut : mêmes noms et mêmes valeurs que l'App.Config de
# l'original (LoadSetting) ; « CBStop » = AllowUserChoice ("0" ignorer, "1" demander,
# "2" demander avec fermeture automatique de la fenêtre de choix).
DEFAUTS = {
    "SHOWRENLOG": False, "SHOWDBGLOG": False, "DBGONOFF": False,
    "DBGLOGMAX": 10000, "RENLOGMAX": 10000, "LANGENFR": "FR", "TBTags": "",
    "CBCover": True, "CBStatus": True, "CBGenre": True, "CBNotes": True, "CBWeb": True,
    "ShortWebLink": False, "CBCount": True, "CBSynopsys": True, "CBImprint": True,
    "CBLetterer": True, "CBInker": True, "CBPrinted": True, "CBRating": True, "CBISBN": True,
    "CBLanguage": True, "CBEditor": True, "CBFormat": True, "CBColorist": True,
    "CBPenciller": True, "CBWriter": True, "CBSeries": True, "CBTitle": True,
    "CBDefault": False, "CBRescrape": False, "use_cloudscraper": False, "CBStop": "2",
    "ARTICLES": "Le,La,Les,L',The", "SUBPATT": " - - ", "COUNTOF": True, "CBCouverture": True,
    "COUNTFINIE": True, "TITLEIT": True, "TIMEOUT": "1000", "TIMEOUTS": "7", "TIMEPOPUP": "30",
    "FORMATARTICLES": True, "PopUpEditionForm": False, "SerieResumeEverywhere": True,
    "PadNumber": "0", "ONESHOTFORMAT": False, "AlwaysChooseSerie": False,
    "AcceptGenericArtists": False,
    # Ajouts du portage (absents de l'original) : champs vides seulement ; fenêtres de choix
    # (hauteur des vignettes, couvertures des séries lues sur la page des premières séries).
    "ChampsVidesSeulement": False, "TailleVignettes": 150, "VignettesSeries": True, "NbVignettesSeries": 6,
    "url_base": "", "dossier_journaux": "",
}
TEXTES = {"LANGENFR", "TBTags", "ARTICLES", "SUBPATT", "TIMEOUT", "TIMEOUTS", "TIMEPOPUP", "PadNumber",
          "CBStop", "url_base", "dossier_journaux"}
ENTIERS = {"DBGLOGMAX", "RENLOGMAX", "TailleVignettes", "NbVignettesSeries"}
# Réglages du portage : jamais lus dans l'App.Config de l'original.
AJOUTS = {"ChampsVidesSeulement", "TailleVignettes", "VignettesSeries", "NbVignettesSeries", "url_base",
          "dossier_journaux"}

# Liste « Données » de l'onglet 2 : clé de la liste → (réglage, n° de texte), dans l'ordre
# de l'original (BDConfigForm._scrapedData).
DONNEES = [
    ("Series", "CBSeries", 71), ("Title", "CBTitle", 72), ("Writer", "CBWriter", 30),
    ("Penciller", "CBPenciller", 31), ("Colorist", "CBColorist", 33), ("Letterer", "CBLetterer", 38),
    ("Inker", "CBInker", 73), ("CoverArtist", "CBCouverture", 121), ("Publisher", "CBEditor", 78),
    ("Language", "CBLanguage", 79), ("Format", "CBFormat", 77), ("Rating", "CBRating", 81),
    ("Printed", "CBPrinted", 82), ("Imprint", "CBImprint", 83), ("Synopsys", "CBSynopsys", 84),
    ("Count", "CBCount", 85), ("Notes", "CBNotes", 86), ("SerieStatus", "CBStatus", 87),
    ("Web", "CBWeb", 88), ("Genre", "CBGenre", 89), ("Cover", "CBCover", 90), ("ISBN", "CBISBN", 80),
]

# En-têtes HTTP de BedethequeFetcher.py (le « daemon » de l'original, qui faisait les
# requêtes à la place d'IronPython : une requête directe était refusée par le site).
EN_TETES = [
    ("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) "
                   "Chrome/124.0.0.0 Safari/537.36"),
    ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    ("Accept-Language", "en-US,en;q=0.5"),
    ("Connection", "keep-alive"),
    ("Upgrade-Insecure-Requests", "1"),
]
DELAI_RESEAU = 30

# Vignettes des fenêtres de choix (limites de comicrust.choose) : images chargées d'office
# au plus par fenêtre (les autres au survol), pause entre deux images téléchargées
# (serveur d'images du site ; une image du cache disque n'attend pas).
MAX_VIGNETTES = 24
MAX_IMAGE = 2 * 1024 * 1024
PAUSE_IMAGE = 0.15
# Couvertures des séries (et des albums sans couverture dans la page de la série) : lues
# sur leur page, **pendant** que la fenêtre de choix est ouverte, une page à la fois, avec
# au moins PAUSE_PAGE_SERIE secondes entre deux pages ; NbVignettesSeries pages d'office
# (12 au plus), les autres seulement si l'utilisateur survole la ligne.
MAX_PAGES_SERIES = 12
PAUSE_PAGE_SERIE = 1.0
# Cache disque des vignettes (data\bedetheque\cache\) : nombre de fichiers et taille au
# plus (les moins récemment utilisés sont supprimés) ; adresses des couvertures des pages
# gardées COUV_JOURS jours (COUV_ABSENTE_JOURS si la page n'en avait pas).
MAX_CACHE_FICHIERS = 800
MAX_CACHE_OCTETS = 120 * 1024 * 1024
MAX_CACHE_COUVERTURES = 5000
COUV_JOURS = 60
COUV_ABSENTE_JOURS = 7
# Accept des images : WebP, PNG, JPEG, GIF (pas d'AVIF, que ComicRust ne lit pas).
ACCEPT_IMAGES = "image/webp,image/png,image/jpeg,image/gif,image/*;q=0.8,*/*;q=0.5"


class _Etat:
    """Variables globales de l'original (dlgName, dlgNumber, RenameSeries, PickSeries…)."""

    def __init__(self):
        self.C = dict(DEFAUTS)
        self.mots = []
        self.base = URL_SITE
        self.web = None
        self.journaux = None
        self.dlgNumber = ""
        self.dlgName = ""
        self.dlgNameClean = ""
        self.dlgAltNumber = ""
        self.AlbumNumNum = False
        self.Shadow1 = False
        self.Shadow2 = False
        self.RenameSeries = False
        self.PickSeries = False
        self.PickSeriesLink = ""
        self.serie_rech_prev = None
        self.Serie_Resume = ""
        self.SkipAlbum = False
        self.bError = False
        self.nRenamed = 0
        self.nIgnored = 0
        self.images = {}
        self.pages = {}
        self.cache = None
        # Dernière requête par sorte (« page », « image ») : pauses des vignettes.
        self.derniere = {}
        self.couverture = None
        self.fraction = 0.0
        self.detail = ""


S = _Etat()


def C(nom):
    return S.C[nom]


# ---------------------------------------------------------------------------------------
# Expressions régulières (celles de l'original, version 6.08)
# ---------------------------------------------------------------------------------------

I, M, D = re.IGNORECASE, re.MULTILINE, re.DOTALL

LAST_FIRST_NAMES = re.compile(r'(?P<name>[^,]*?), (?P<firstname>[^,]*?)$')

SERIE_LIST_PATTERN = r'<a\shref="%s/serie-([^<>"]*?)">.+?class="bdt-liste-libelle">.*?<b>(.+?)</b>.+?</a>'
SERIE_LIST_CHECK = re.compile(r'séries\strouvées.{40,80}?La\srecherche\srenvoie\splus\sde', I | D)
SERIE_URL_PATTERN = r'<a href="([^"]+?)">(?:(?!</a>).)+?<span class="bdt-liste-libelle"><b>%s</b></span>'

INFOS_ALBUMS = re.compile(r'<dl class="bdt-sheet">.+?</div>', I | D)
SERIE_LANGUE = re.compile(r'<li[^<>]+?(?<="Langue de parution").+?class="bdt-sh-flag"[^<>]+?>([^<>]+?)<', I)
SERIE_GENRE = re.compile(r'class="bdt-tag bdt-sh-genre"[^<>]+?>([^<>]+?)<', I | M | D)
SERIE_RESUME = re.compile(r'class="bdt-sh-resume">([^<>]*?)<', I | M | D)
SERIE_STATUS = re.compile(r'class="bdt-sh-parution">([^<>]+?)<', I | M | D)
SERIE_NOTE = re.compile(r'<p\sclass="static">Note:\s<strong>\s(?P<note>[^<]*?)</strong>', I | M | D)
SERIE_COUNT = re.compile(r'title="Nombre de tomes de la série">(\d+)', I | M | D)
SERIE_COUNT_REAL = re.compile(r'class="bdt-box-albums-liste">(.+?)</ul>', I | M | D)
SERIE_COUNTOF = re.compile(r'class="bdt-box-albums-num"[^<>]+?>([^<>]+?)<span', I | M | D)
SERIE_HEADER2 = re.compile(r'<h3(.+?)</p', I | M | D)
SERIE_QSERIE = re.compile(r'<h1>.+?<a href="[^<>]+/serie-[^\.]+\.html">([^"<>]+)</a>.*?</h1>', I | M | D)
SERIE_ALBUMS = re.compile(r'<li[^>]*>\s+?<a href="(?P<url>[^"]+?)" title="(?P<title>[^"]+?)">.+?'
                          r'class="bdt-box-albums-num"[^<>]+?>(?P<tome>[^<>]*?)<span class="bdt-numa">'
                          r'(?P<alt>[^<>]*?)<', I | D)
SERIE_PREMIER = re.compile(r'class="bdt-edition-head">\s*<h3>\s*<a[^<>]+?href="([^"]+?)"', I | D)

ALBUM_EVAL = re.compile(r'ratingValue">(.*?)<', I | M | D)
ALBUM_MULTI_AUTHOR_NAMES = re.compile(r'">([^<>]*?)</', I | M | D)
ALBUM_SCENAR_MULTI_AUTHOR = re.compile(r"<dt>Sc.nario</dt>\s*<dd>(.+?)<dt>[^<>]+?</dt>", I | M | D)
ALBUM_STORYBOARD_MULTI_AUTHOR = re.compile(r"<dt>storyboard</dt>\s*<dd>(.+?)<dt>[^<>]+?</dt>", I | M | D)
ALBUM_DESSIN_MULTI_AUTHOR = re.compile(r"<dt>dessin</dt>\s*<dd>(.+?)<dt>[^<>]+?</dt>", I | M | D)
ALBUM_COLOR_MULTI_AUTHOR = re.compile(r"<dt>Couleurs</dt>\s*<dd>(.+?)<dt>[^<>]+?</dt>", I | M | D)
ALBUM_COUVERT_MULTI_AUTHOR = re.compile(r"<dt>Couverture</dt>\s*<dd>(.+?)<dt>[^<>]+?</dt>", I | M | D)
ALBUM_LETTRAGE_MULTI_AUTHOR = re.compile(r"<dt>Lettrage</dt>\s*<dd>(.+?)<dt>[^<>]+?</dt>", I | M | D)
ALBUM_INKER_MULTI_AUTHOR = re.compile(r"<dt>Encrage</dt>\s*<dd>(.+?)<dt>[^<>]+?</dt>", I | M | D)
ALBUM_DEPOT = re.compile(r"<dt>D.p.t L.gal</dt>\s+<dd>(?P<month>[\d|-]{0,2})/?(?P<year>[\d]{2,4})?", I | M | D)
ALBUM_ACHEVE = re.compile(r'<dt>Achev.*?\s:\s</dt>\s+<dd>(?P<month>[\d|-]{0,2})/?(?P<year>[\d]{2,4})?<', I | M | D)
ALBUM_EDITEUR = re.compile(r'<dt>.diteur</dt>\s+<dd>(.*?)</dd>', I | M | D)
ALBUM_COLLECTION = re.compile(r"<dt>Collection</dt><dd>(?:<a href.+?>)*([^><]+?)<", I | M | D)
ALBUM_TAILLE = re.compile(r"<dt>Format</dt>\s*<dd>(.*?)</dd>", I | M | D)
ALBUM_ISBN = re.compile(r"<dt>EAN/ISBN</dt>\s*<dd>(.*?)</dd>", I | D)
ALBUM_PLANCHES = re.compile(r"<dt>Planches</dt>\s*<dd>(.*?)</dd>", I | M | D)
ALBUM_INFOEDITION = re.compile(r"<b>Info\s.*?dition</b>\s?(.*?)<", I | M | D)
ALBUM_TOME = re.compile(r'<h2 class="bdt-ah-sub">\s*([^<>]+?)<span class="bdt-numa">(.*?)</span>', I | D)
ALBUM_EDITIONS = re.compile(
    r'<article class="bdt-edition bdt-edition--detail" id="ed-(?P<id>[^"]+)">.+?<h3>.+?'
    r'<span class="bdt-numa">(?P<alt>[^\s<]+)*.+?</h3>.+?<a class="bdt-img bdt-ecov[^"]+"\shref="(?P<couv>[^"]+)"'
    r'.+?<dt>Titre</dt><dd><b>(?P<titre>[^<]+).+?<dt>Identifiant</dt>(?P<infos>.+?)</article>', I | D | M)
ALBUM_SERIE_ENTETE = re.compile(r'bdt-ah-top.+?<h1>.+?>([^"]+?)[<>]', I | D | M)
ALBUM_SERIE_ENCART = re.compile(r"<dt>S.rie</dt><dd><a[^>]+?>([^<]+?)</a>", I | D | M)

REVUE_LIST_PATTERN = r'<a\shref="%s/revue-([^<>"]*?)">.+?class="bdt-liste-libelle">.*?<b>(.+?)</b>.+?</a>'
REVUE_LIST_EXISTS = re.compile(r'<h2>\d{1,3} revue\w?? trouvée\w??</h2>', I | D)
REVUE_LIST_CHECK = re.compile(r'revues\strouvées.{40,80}?La\srecherche\srenvoie\splus\sde', I | D)
REVUE_HEADER_PATTERN = (r'class="couv"(.{1,100}?couvertures"\shref="(https?.{1,150}?)">.{1,600}?class="titre"'
                        r'.{1,100}?#(%s)\..+?class="autres".+?)</li>')
REVUE_HEADER_PATTERN_ALT = (r'<a name="%s">.+?class="couv"(.{1,100}?couvertures"\shref="(https?.{1,150}?)">.+?'
                            r'class="titre".{1,100}?#(.+?)\..+?class="autres".+?)</li>')
REVUE_RESUME = re.compile(r'<em>Sommaire.*?</em>(.*?)</p', I | M | D)
REVUE_PLANCHES = re.compile(r'>Nb\sPages\s:\s??</label>(.*?)<', I | M | D)
REVUE_DEPOT = re.compile(r'<label>Parution\s:s??</label>(.*?)</', I | M | D)
REVUE_PERIOD = re.compile(r'<label>P.riodicit.\s:\s??</label>(.*?)</', I | M | D)
REVUE_EDITEUR = re.compile(r'<label>Editeur\s:\s?</label>(.*?)</', I | M | D)

IMG_SRC = re.compile(r'<img[^>]+?src="([^"]+?)"', I)
# Ajouts du portage (vignettes des fenêtres de choix) : balises <img> (adresse dans src ou,
# images chargées au défilement, data-src / data-original / data-lazy-src), drapeaux et
# icônes écartés ; infos d'un résultat de recherche ; couverture d'une page de série.
IMG_TAG = re.compile(r'<img\b[^>]*>', I)
IMG_ATTR = re.compile(r'\b(data-src|data-original|data-lazy-src|src)\s*=\s*"([^"]+)"', I)
IMG_PAS_COUV = re.compile(r'(?i)/flags/|^data:|\.svg(?:\?|$)|blank\.gif|spacer|pixel\.gif|/static\d*/')
IMG_CLASSE = re.compile(r'\bclass\s*=\s*"([^"]*)"', I)
IMG_CLASSE_PAS_COUV = re.compile(r'(?i)\b(?:flag|ico|icon|avatar|logo)\b')
LISTE_INFO = re.compile(r'class="bdt-liste-info"[^>]*>(.*?)</span>', I | D)
LISTE_DRAPEAU = re.compile(r'/flags/([A-Za-z_-]+)\.(?:png|gif|jpg|svg)', I)
PAGE_OG_IMAGE = re.compile(r'<meta[^>]+property="og:image"[^>]+content="([^"]+)"', I)
PAGE_COUV = re.compile(r'<a[^>]+class="bdt-img bdt-ecov[^"]*"[^>]+href="([^"]+)"', I)
# Infos des albums d'une page de série (fenêtre de choix de l'album) : liens des albums
# (numéro en fin d'adresse), liens vers une couverture, dépôt légal et éditeur, dans les
# deux mises en page connues du site (« <dt>Dépot légal</dt><dd>11/2000 » et
# « <label>Dépot légal :</label>11/2000 »).
LIEN_ALBUM_PAGE = re.compile(r'href="[^"]*?-(\d+)\.html', I)
LIEN_COUV = re.compile(r'href="([^"]+?/(?:media/Couvertures|cache/thb_couv)/[^"]+)"', I)
INFO_DEPOT = re.compile(r'D.p.t\s+l.gal\s*:?\s*(?:<[^>]+>\s*)*((?:\d{1,2}|-)/\d{4}|\d{4})', I)
INFO_EDITEUR = re.compile(r'.diteur\s*:?\s*(?:<[^>]+>\s*)*([^<>]+?)\s*<', I)

# Langue de parution (2 premières lettres du libellé) → code ISO. L'original avait « du »
# pour le néerlandais : ComicRust n'accepte que les codes ISO, d'où « nl ».
LANGUES = {"Fr": "fr", "Al": "de", "An": "en", "It": "it", "Es": "es", "Ne": "nl", "Po": "pt", "Ja": "ja"}


# ---------------------------------------------------------------------------------------
# Traductions (BDTranslations.Config) et réglages
# ---------------------------------------------------------------------------------------

def Translate():
    """Textes T0001… de la langue choisie (LANGENFR), comme l'original."""
    chemin = os.path.join(FICHIERS, "BDTranslations.Config")
    racine = _lire_xml(chemin)
    langue = "EN" if str(C("LANGENFR")).upper() == "EN" else "FR"
    mots = []
    for i in range(1, 200):
        noeud = racine.find("T%04d/%s" % (i, langue))
        mots.append((noeud.text or "") if noeud is not None else "")
    return mots


def Trans(n):
    return S.mots[n - 1] if 0 < n <= len(S.mots) else ""


def _t(fr, en):
    """Texte ajouté par le portage (absent de BDTranslations.Config), dans la langue du
    plugin (LANGENFR), comme les textes de l'original."""
    return en if str(S.C.get("LANGENFR", "FR")).upper() == "EN" else fr


def _lire_xml(chemin):
    """Document XML de .NET : encodage déclaré parfois faux (« utf-16 » dans un fichier
    UTF-8 écrit par XmlTextWriter sur un StringWriter) : on lit le texte nous-mêmes."""
    with open(chemin, "rb") as fic:
        brut = fic.read()
    if brut.startswith(b"\xff\xfe") or brut.startswith(b"\xfe\xff"):
        texte = brut.decode("utf-16")
    else:
        texte = brut.decode("utf-8-sig", "replace")
    texte = re.sub(r"^\s*<\?xml[^>]*\?>", "", texte)
    return ET.fromstring(texte)


def _bool(v, defaut=False):
    if isinstance(v, bool):
        return v
    if v is None:
        return defaut
    if isinstance(v, (int, float)):
        return v != 0
    t = str(v).strip().lower()
    if t in ("1", "true", "vrai", "oui", "yes"):
        return True
    if t in ("0", "false", "faux", "non", "no", ""):
        return False
    return defaut


def _convertir(cle, v):
    """Valeur d'un réglage au type attendu (booléen, entier ou texte)."""
    defaut = DEFAUTS[cle]
    if cle in TEXTES:
        if v is None:
            return defaut
        if isinstance(v, float) and v.is_integer():
            v = int(v)
        return str(v)
    if cle in ENTIERS:
        try:
            return int(float(v))
        except (TypeError, ValueError):
            return defaut
    return _bool(v, defaut)


def dossier_donnees():
    """Dossier des journaux par défaut : ``data\\bedetheque`` à côté de ComicRust.exe (le
    module comicrust est dans ``data\\python``), comme ``data\\library_organizer`` :
    ComicRust reste portable et le dossier des scripts n'est pas encombré."""
    data = os.path.dirname(os.path.dirname(os.path.abspath(comicrust.__file__)))
    return os.path.join(data, "bedetheque")


def LoadSetting():
    """Réglages enregistrés (comicrust.settings()), avec les corrections de LoadSetting."""
    s = comicrust.settings()
    cfg = dict(DEFAUTS)
    for cle in DEFAUTS:
        if cle in s:
            cfg[cle] = _convertir(cle, s.get(cle))
    if cfg["CBStop"] not in ("0", "1", "2"):
        cfg["CBStop"] = "2"
    if cfg["ONESHOTFORMAT"] and cfg["CBFormat"]:
        cfg["CBFormat"] = False
    # Étiquettes faites seulement d'espaces (App.Config importé par la v0.10.1, qui gardait
    # l'indentation du fichier) : vides, comme dans l'original. Sinon les étiquettes des
    # livres auraient été effacées (texte blanc → champ vidé).
    if not str(cfg["TBTags"]).strip():
        cfg["TBTags"] = ""
    cfg["TailleVignettes"] = min(320, max(48, cfg["TailleVignettes"]))
    cfg["NbVignettesSeries"] = min(MAX_PAGES_SERIES, max(0, cfg["NbVignettesSeries"]))
    S.C = cfg
    base = cfg["url_base"] or os.environ.get("COMICRUST_BEDETHEQUE_URL") or URL_SITE
    S.base = base.strip().rstrip("/")
    dossier = (cfg["dossier_journaux"] or "").strip() or dossier_donnees()
    S.journaux = Journaux(dossier)
    try:
        S.mots = Translate()
    except (OSError, ET.ParseError) as e:
        comicrust.message("Fichier bd2/BDTranslations.Config manquant ou illisible (%s) : réinstallez le "
                          "script Bedetheque Scraper 2." % e)
        return False
    return True


def SaveSetting():
    s = comicrust.settings()
    for cle in DEFAUTS:
        s[cle] = S.C[cle]
    s.save()


def AllowUserChoice():
    """« CBStop » de l'original : False (ignorer), True (demander) ou "2" (demander, avec
    fermeture automatique de la fenêtre après TIMEPOPUP secondes)."""
    v = C("CBStop")
    return "2" if v == "2" else v == "1"


def lire_app_config(chemin):
    """Réglages lus dans l'App.Config de Bedetheque Scraper 2 pour ComicRack (dossier du
    script, par ex. %AppData%\\cYo\\ComicRack\\Scripts\\Bedetheque Scraper 2\\App.Config) :
    ``<configuration><CBCover>1</CBCover>…</configuration>``. Renvoie un dictionnaire des
    réglages reconnus (mêmes noms) ; corrections de LoadSetting appliquées."""
    racine = _lire_xml(chemin)
    if racine.tag != "configuration":
        raise ValueError("ce fichier n'est pas un App.Config de Bedetheque Scraper 2")
    lus = {}
    ancien_lien = False
    for noeud in racine:
        cle = noeud.tag
        if cle not in DEFAUTS or cle in AJOUTS:
            continue
        brut = "".join(noeud.itertext())
        # XmlDocument de .NET (PreserveWhitespace faux) : un contenu fait seulement
        # d'espaces et de retours à la ligne (« <TBTags>⏎  </TBTags> » écrit par l'indentation
        # d'AppSettings.Save) est lu vide ; un texte est gardé tel quel (SUBPATT « - - »).
        if not brut.strip():
            brut = ""
        texte = brut.strip() if cle not in ("SUBPATT", "ARTICLES", "TBTags") else brut
        if cle == "CBWeb" and texte == "2":  # compatibilité avec les anciennes versions
            ancien_lien = True
            continue
        if cle == "CBStop":
            if texte in ("0", "1", "2"):
                lus[cle] = texte
            continue
        lus[cle] = _convertir(cle, texte)
    if ancien_lien:
        lus["CBWeb"] = True
        lus["ShortWebLink"] = True
    if lus.get("ONESHOTFORMAT") and lus.get("CBFormat", True):
        lus["CBFormat"] = False
    if not lus:
        raise ValueError("aucun réglage de Bedetheque Scraper 2 dans ce fichier")
    return lus


# ---------------------------------------------------------------------------------------
# Journaux : rapport (BD2_Rename_Log.txt) et débogage (BD2_Debug_Log.txt)
# ---------------------------------------------------------------------------------------

class Journaux:
    def __init__(self, dossier):
        self.dossier = dossier
        self.rapport = os.path.join(dossier, "BD2_Rename_Log.txt")
        self.debug = os.path.join(dossier, "BD2_Debug_Log.txt")
        self.messages = []
        self.lignes_rapport = []

    def taille(self, chemin):
        try:
            return os.path.getsize(chemin)
        except OSError:
            return 0

    def vider(self, chemin):
        try:
            os.remove(chemin)
        except OSError:
            pass

    def ecrire(self, chemin, texte):
        try:
            os.makedirs(os.path.dirname(chemin), exist_ok=True)
            with open(chemin, "a", encoding="utf-8") as fic:
                fic.write(texte)
        except OSError as e:  # dossier en lecture seule : le journal du script suffit
            print("Journal %s non écrit : %s" % (os.path.basename(chemin), e))


def _maintenant():
    return datetime.datetime.now().strftime("%A %d %B %Y %H:%M:%S")


def log_BD(bdstr, bdstat, lTime):
    """Ligne du rapport (comme log_BD de l'original), aussi dans le journal du script."""
    cDT = _maintenant() + " > " if lTime == 1 else ""
    ligne = cDT + str(bdstr) + "   " + str(bdstat)
    S.journaux.lignes_rapport.append(ligne)
    S.journaux.ecrire(S.journaux.rapport, ligne + "\n")
    texte = (str(bdstr) + "   " + str(bdstat)).strip()
    if texte:
        comicrust.log(texte)


def debuglog(*args):
    message = " ".join(str(a) for a in args)
    if C("DBGONOFF"):
        print(message)
    S.journaux.messages.append(message)


def flush_debuglog():
    if S.journaux.messages:
        S.journaux.ecrire(S.journaux.debug, "\n\n" + _maintenant() + "\n" + "\n".join(S.journaux.messages) + "\n")
        del S.journaux.messages[:]


def debuglogOnError():
    """Trace de l'erreur en cours dans le journal de débogage ; renvoie le message."""
    S.bError = True
    cError = traceback.format_exc().strip().splitlines()[-1]
    S.journaux.ecrire(S.journaux.debug, "\n\n" + _maintenant() + "\n" + traceback.format_exc())
    comicrust.log("Erreur : " + cError, level="warning")
    return cError


# ---------------------------------------------------------------------------------------
# Accès au site
# ---------------------------------------------------------------------------------------

class NoContentError(Exception):
    pass


class Web:
    """Requêtes vers bedetheque.com : un seul « opener » avec ses cookies (le jeton
    csrf_cookie_bel est renouvelé si le site répond 403), mêmes en-têtes que
    BedethequeFetcher.py. Option « cloudscraper » : module ``cloudscraper`` à installer
    dans ``scripts\\lib`` (avec ses dépendances), comme le mode « --cloudscraper » du
    fetcher de l'original."""

    def __init__(self, base, cloudscraper):
        self.base = base
        self.accueil = False
        self.scraper = None
        if cloudscraper:
            try:
                import cloudscraper as cs  # noqa: facultatif
                self.scraper = cs.create_scraper(browser={"browser": "chrome", "platform": "windows",
                                                          "desktop": True})
                self.scraper.headers.update({"Accept-Language": "fr-FR,fr;q=0.9,en;q=0.8"})
            except Exception as e:  # module absent : requêtes ordinaires
                comicrust.log("cloudscraper indisponible (%s) : requêtes ordinaires. Pour l'utiliser, copiez le "
                              "module cloudscraper et ses dépendances dans scripts\\lib (voir scripts.md)." % e,
                              level="warning")
        self._nouvel_opener()

    def _nouvel_opener(self):
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(self.jar),
            urllib.request.HTTPSHandler(context=ssl.create_default_context()))
        self.opener.addheaders = list(EN_TETES) + [("Referer", self.base + "/")]

    def _lire(self, url, referer=None):
        req = urllib.request.Request(url)
        if referer:
            req.add_header("Referer", referer)
        with self.opener.open(req, timeout=DELAI_RESEAU) as rep:
            charset = rep.headers.get_content_charset() or "utf-8"
            return rep.read().decode(charset, "replace")

    def page(self, url):
        """Texte HTML de ``url`` (exception en cas d'erreur)."""
        recherche = "/search/tout" in url.lower()
        if self.scraper is not None:
            if recherche:
                self.scraper.get(self.base + "/", timeout=DELAI_RESEAU).raise_for_status()
            r = self.scraper.get(url, headers={"Referer": self.base + "/"} if recherche else None,
                                 timeout=DELAI_RESEAU)
            r.raise_for_status()
            if not r.text:
                raise NoContentError("aucun contenu")
            return r.text
        referer = None
        if recherche and not self.accueil:
            # Comme le mode cloudscraper de l'original : page d'accueil d'abord (cookies).
            self.accueil = True
            try:
                self._lire(self.base + "/")
            except Exception as e:
                debuglog("Page d'accueil : " + str(e))
            referer = self.base + "/"
        try:
            texte = self._lire(url, referer)
        except urllib.error.HTTPError as e:
            if e.code != 403:
                raise
            # Jeton refusé : nouveaux cookies et un nouvel essai.
            debuglog("403 sur %s : renouvellement des cookies" % url)
            self._nouvel_opener()
            texte = self._lire(url, referer)
        if not texte:
            raise NoContentError("aucun contenu")
        return texte

    def octets(self, url, referer=None):
        """Image : en-têtes d'un navigateur qui affiche une image de la page (Referer du
        site : le serveur d'images peut refuser les images demandées hors du site)."""
        entetes = {"Accept": ACCEPT_IMAGES, "Referer": referer or self.base + "/", "Sec-Fetch-Dest": "image"}
        if self.scraper is not None:
            r = self.scraper.get(url, headers=entetes, timeout=DELAI_RESEAU)
            r.raise_for_status()
            return r.content[:MAX_IMAGE + 1]
        req = urllib.request.Request(url_fix(url), headers=entetes)
        with self.opener.open(req, timeout=DELAI_RESEAU) as rep:
            return rep.read(MAX_IMAGE + 1)


def url_fix(s):
    scheme, netloc, path, qs, anchor = urllib.parse.urlsplit(s)
    path = urllib.parse.quote(path, "%/:=&~#+$!,?;'@()*[]")
    qs = urllib.parse.quote_plus(qs, ":&=")
    return urllib.parse.urlunsplit((scheme, netloc, path, qs, anchor))


def _sur_le_site(url):
    return url.lower().startswith(S.base.lower() + "/")


def _hote_site():
    return re.sub(r"^https?://", "", S.base, flags=re.I)


def _relatif(url):
    """Lien sans adresse du site : « www.bedetheque.com/BD--12.html » (liens Web
    raccourcis) → « BD--12.html »."""
    u = url.strip()
    for prefixe in (_hote_site() + "/", "www.bedetheque.com/", "bedetheque.com/"):
        if u.lower().startswith(prefixe.lower()):
            return u[len(prefixe):]
    return u


def GetFullURL(url):
    if url:
        if S.base.lower() + "/" in url.lower():
            return url
        return S.base + "/" + _relatif(url).lstrip("/")
    return ""


def _read_url(url, bSingle, silencieux=False, garder=False):
    """Page du site (``url`` complète ou relative) ; '' en cas d'erreur (message affiché,
    comme l'original ; seulement noté au journal de débogage si ``silencieux``).

    ``garder`` : page gardée pour la prochaine demande de la même adresse (pages des séries
    lues pour leur couverture : la page de la série choisie n'est pas redemandée)."""
    page = ""
    if stop():
        debuglog("Cancelled from _read_url Start")
        return page
    if not bSingle and _sur_le_site(url):
        bSingle = True
    if bSingle:
        target_url = url_fix(url)
    else:
        target_url = url_fix(S.base + "/" + _relatif(url).lstrip("/"))
    if target_url in S.pages:
        debuglog("Page déjà lue : " + target_url)
        return S.pages[target_url] if garder else S.pages.pop(target_url)
    debuglog("Final fetcher URL: " + target_url)
    try:
        page = S.web.page(target_url)
        if stop():
            debuglog("Cancelled from _read_url End")
            return ""
        if garder:
            S.pages[target_url] = page
    except NoContentError as nce:
        debuglog("No content returned for URL: " + target_url + ": " + str(nce))
    except Exception as e:
        if silencieux:
            debuglog("%s : %s" % (target_url, _texte_erreur(e)))
            return ""
        debuglog(Trans(60))
        debuglog(Trans(61), e)
        cError = debuglogOnError()
        log_BD("   [" + str(S.dlgName) + "] " + str(S.dlgNumber) + " Alt.No " + str(S.dlgAltNumber) + " -> ",
               cError, 1)
        comicrust.message(Trans(97) + "\n" + Trans(98) + "%s\n%s" % (target_url, _texte_erreur(e)))
    return page


def _texte_erreur(e):
    if isinstance(e, urllib.error.HTTPError):
        return "HTTP %d %s" % (e.code, e.reason)
    return "%s: %s" % (type(e).__name__, e)


def _url_image(url):
    """Adresse complète d'une image trouvée dans une page : entités HTML décodées, adresse
    sans schéma (« //www.bedetheque.com/… » : celui du site), relative (« /media/… »,
    « media/… ») ; None pour une image intégrée (« data: ») ou vide."""
    u = checkWebChar(url).strip()
    if not u or u.lower().startswith("data:"):
        return None
    if u.startswith("//"):
        hote, _, chemin = u[2:].partition("/")
        if hote.lower() in ("www.bedetheque.com", "bedetheque.com"):
            return S.base + "/" + chemin
        return S.base.split(":", 1)[0] + ":" + u
    if re.match(r"(?i)^https?://", u):
        return u
    return GetFullURL(u)


# Signatures des formats d'image lisibles par ComicRust (une page d'erreur HTML servie à
# la place d'une image n'est ni montrée ni gardée en cache).
def _est_image(d):
    return bool(d) and (d[:3] == b"\xff\xd8\xff" or d[:8] == b"\x89PNG\r\n\x1a\n" or d[:4] == b"GIF8"
                        or (d[:4] == b"RIFF" and d[8:12] == b"WEBP") or d[:2] == b"BM")


def _attendre(sorte, pause):
    """Pause avant une requête de vignette : au moins ``pause`` secondes depuis la
    précédente de la même sorte (pages, images) ; l'arrêt du script est guetté."""
    fin = S.derniere.get(sorte, 0.0) + pause
    while time.monotonic() < fin and not stop():
        time.sleep(min(0.05, max(0.0, fin - time.monotonic())))
    S.derniere[sorte] = time.monotonic()


class CacheDisque:
    """Cache des vignettes des fenêtres de choix, dans ``cache\\`` du dossier des journaux
    (``data\\bedetheque\\cache\\`` à côté de ComicRust.exe) : images téléchargées
    (``<empreinte SHA-1 de l'adresse>.img``) et adresse de la couverture de chaque page de
    série ou d'album déjà lue (``couvertures.json``). Une série déjà proposée n'est donc
    plus relue. Borné : MAX_CACHE_FICHIERS images et MAX_CACHE_OCTETS au plus (les moins
    récemment utilisées partent d'abord). Toute erreur disque est ignorée (cache vide)."""

    def __init__(self, dossier):
        self.dossier = dossier
        self.index_chemin = os.path.join(dossier, "couvertures.json")
        self.index = None
        self.modifie = False
        self.ecrits = 0

    def _chemin(self, url):
        return os.path.join(self.dossier, hashlib.sha1(url.encode("utf-8")).hexdigest() + ".img")

    def image(self, url):
        chemin = self._chemin(url)
        try:
            with open(chemin, "rb") as f:
                d = f.read(MAX_IMAGE + 1)
            os.utime(chemin, None)  # utilisée : gardée plus longtemps
            return d if _est_image(d) and len(d) <= MAX_IMAGE else None
        except OSError:
            return None

    def garder_image(self, url, donnees):
        try:
            os.makedirs(self.dossier, exist_ok=True)
            tmp = self._chemin(url) + ".tmp"
            with open(tmp, "wb") as f:
                f.write(donnees)
            os.replace(tmp, self._chemin(url))
            self.ecrits += 1
            if self.ecrits % 50 == 1:
                self.purger()
        except OSError as e:
            debuglog("Cache : %s" % e)

    def _index(self):
        if self.index is None:
            try:
                with open(self.index_chemin, encoding="utf-8") as f:
                    self.index = dict(json.load(f))
            except (OSError, ValueError, TypeError):
                self.index = {}
        return self.index

    def couverture(self, page):
        """``(connue, adresse ou None)`` de la couverture d'une page déjà lue."""
        e = self._index().get(page)
        if not isinstance(e, list) or len(e) != 2:
            return False, None
        img, quand = e
        jours = COUV_JOURS if img else COUV_ABSENTE_JOURS
        if not isinstance(quand, (int, float)) or time.time() - quand > jours * 86400:
            return False, None
        return True, img or None

    def garder_couverture(self, page, img):
        idx = self._index()
        idx[page] = [img or "", int(time.time())]
        if len(idx) > MAX_CACHE_COUVERTURES:
            for k, _ in sorted(idx.items(), key=lambda kv: kv[1][1] if isinstance(kv[1], list) else 0)[
                    :len(idx) - MAX_CACHE_COUVERTURES]:
                del idx[k]
        self.modifie = True

    def enregistrer(self):
        if not self.modifie:
            return
        try:
            os.makedirs(self.dossier, exist_ok=True)
            tmp = self.index_chemin + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(self.index, f, ensure_ascii=False)
            os.replace(tmp, self.index_chemin)
            self.modifie = False
        except OSError as e:
            debuglog("Cache : %s" % e)

    def purger(self):
        """Supprime les images les moins récemment utilisées au-delà des bornes."""
        try:
            fichiers = []
            for n in os.listdir(self.dossier):
                if n.endswith(".img") or n.endswith(".tmp"):
                    c = os.path.join(self.dossier, n)
                    st = os.stat(c)
                    fichiers.append((st.st_mtime, st.st_size, c))
        except OSError:
            return
        fichiers.sort()
        total = sum(f[1] for f in fichiers)
        while fichiers and (len(fichiers) > MAX_CACHE_FICHIERS or total > MAX_CACHE_OCTETS):
            _, taille, c = fichiers.pop(0)
            try:
                os.remove(c)
                total -= taille
            except OSError:
                pass


def image(url, referer=None):
    """Octets d'une image (couverture) : mémoire, cache disque, sinon le site (pause
    PAUSE_IMAGE entre deux téléchargements) ; ``None`` en cas d'erreur, si trop grosse ou
    si ce n'est pas une image."""
    url = _url_image(url) if url else None
    if not url:
        return None
    if url in S.images:
        return S.images[url]
    if len(S.images) > 200:  # mémoire bornée sur une longue sélection
        S.images.clear()
    donnees = S.cache.image(url) if S.cache else None
    if donnees is None:
        _attendre("image", PAUSE_IMAGE)
        try:
            donnees = S.web.octets(url, referer)
            if not donnees or len(donnees) > MAX_IMAGE:
                debuglog("Image %s : trop grosse ou vide" % url)
                donnees = None
            elif not _est_image(donnees):
                debuglog("Image %s : pas une image lisible (JPEG, PNG, WebP, GIF, BMP)" % url)
                donnees = None
            elif S.cache:
                S.cache.garder_image(url, donnees)
        except Exception as e:
            debuglog("Image %s : %s" % (url, _texte_erreur(e)))
    S.images[url] = donnees
    return donnees


def _grande(url):
    """Couverture en grand d'une vignette du site (``/cache/thb_couv/Couv_1.jpg`` →
    ``/media/Couvertures/Couv_1.jpg``, l'adresse que donnent les pages d'album) ; None si
    l'adresse n'est pas une vignette."""
    if url and re.search(r"(?i)/cache/thb_couv/", url):
        return re.sub(r"(?i)/cache/thb_couv/", "/media/Couvertures/", url)
    return None


def _vignette(url):
    """Image d'une ligne de fenêtre de choix : la couverture en grand quand l'adresse est
    une vignette du site (aperçu agrandi lisible), sinon (ou si elle manque) la vignette."""
    if not url or stop():
        return None
    grande = _grande(url)
    img = image(grande) if grande else None
    return img if img is not None else image(url)


def _couverture_page(url, sorte):
    """Couverture d'une série (``sorte`` = SERIE) ou d'un album (ALBUM) d'après sa page :
    cache disque, sinon la page est lue (pause PAUSE_PAGE_SERIE entre deux pages, erreurs
    seulement notées au journal de débogage ; page gardée pour la suite si elle est
    choisie)."""
    if stop():
        return None
    connue, img = S.cache.couverture(url) if S.cache else (False, None)
    if not connue:
        if len(S.pages) > 3 * MAX_PAGES_SERIES:  # mémoire bornée sur une longue sélection
            S.pages.clear()
        _attendre("page", PAUSE_PAGE_SERIE)
        if stop():
            return None
        page = _read_url(url, True, silencieux=True, garder=True)
        if not page:
            return None  # erreur (peut-être passagère) : rien en cache
        img = _couverture_page_serie(page) if sorte == SERIE else _couverture_page_album(page)
        img = _url_image(img) if img else None
        if S.cache:
            S.cache.garder_couverture(url, img)
            S.cache.enregistrer()
    return _vignette(img) if img else None


def _images_choix(liste, formType):
    """Images d'une fenêtre de choix pour ``comicrust.choose`` : fonctions appelées pendant
    que la fenêtre est ouverte (elle s'ouvre tout de suite, les couvertures arrivent une à
    une, celle de la ligne survolée d'abord). Séries : page des NbVignettesSeries premières
    séries d'office, des autres au survol (revues : aucune). Albums : couverture trouvée
    dans la page de la série, sinon page de l'album au survol. Éditions : couverture de la
    page de l'album. None si aucune image possible."""
    images, auto_pages, auto_images = [], 0, 0
    for x in liste:
        img = x[3] if len(x) > 3 else None
        f = None
        if img:
            f = (lambda u=img: _vignette(u))
            auto_images += 1
            if auto_images > MAX_VIGNETTES:
                f = comicrust.on_demand(f)
        elif formType == SERIE and C("VignettesSeries") and x[0].lower().startswith("serie-"):
            url = GetFullURL(x[0]).lower().replace(".html", "__10000.html")
            f = (lambda u=url: _couverture_page(u, SERIE))
            auto_pages += 1
            if auto_pages > C("NbVignettesSeries"):
                f = comicrust.on_demand(f)
        elif formType == ALBUM and x[0] and ".html" in x[0].lower():
            f = comicrust.on_demand(lambda u=GetFullURL(x[0]).split("#")[0]: _couverture_page(u, ALBUM))
        images.append(f)
    return images if any(images) else None


def stop():
    return comicrust.cancelled()


# ---------------------------------------------------------------------------------------
# Outils de texte (portés de l'original)
# ---------------------------------------------------------------------------------------

_NOMBRE = re.compile(r"^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$")


def isnumeric(nNum):
    return bool(_NOMBRE.match(str(nNum or "")))


def isPositiveInt(value):
    try:
        return int(value) >= 0
    except (TypeError, ValueError):
        return False


def checkWebChar(strIn):
    strIn = str(strIn or "")
    strIn = strIn.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&").replace("&nbsp;", " ")
    strIn = strIn.replace("<br />", "").replace("&quot;", '"').replace("\x92", "'").replace("&#039;", "'")
    return html.unescape(strIn)


def checkRegExp(strIn):
    """Nom de série → motif : parenthèses remplacées par « . » (« (Les) »), & et " sous leur
    forme HTML, comme l'original ; les autres caractères spéciaux sont échappés (l'original
    plantait sur « ? » ou « + »)."""
    out = []
    for ch in strIn:
        if ch in "()":
            out.append(".")
        elif ch == "&":
            out.append("&amp;")
        elif ch == '"':
            out.append("&quot;")
        else:
            out.append(re.escape(ch))
    return "".join(out)


def strip_tags(texte):
    return re.sub(r"<[^<>]+?>", "", texte or "")


def remove_accents(raw_text):
    raw_text = re.sub("[àáâãäåÀÁÂÄÅÃ]", "a", raw_text)
    raw_text = re.sub("[èéêëÉÈÊË]", "e", raw_text)
    raw_text = re.sub("[çÇ]", "c", raw_text)
    raw_text = re.sub("[ìíîïÍÌÎÏ]", "i", raw_text)
    raw_text = re.sub("[òóôõöÓÒÔÖÕ]", "o", raw_text)
    raw_text = re.sub("[ùúûüÚÙÛÜ]", "u", raw_text)
    raw_text = re.sub("[œŒ]", "oe", raw_text)
    return raw_text


def if_else(condition, trueVal, falseVal):
    return trueVal if condition else falseVal


def _articles():
    return "|".join(a for a in str(C("ARTICLES")).split(",") if a != "") or "(?!)"


def cleanARTICLES(s):
    ns = re.search(r"^(" + _articles() + r")\s*(?<=['\s])((?=[^/\r\n\(:\,!]*(?:\s[-–]\s))[^-–\r\n]*|.[^/\r\n\(:\,!]*)",
                   s, re.IGNORECASE)
    if ns:
        s = ns.group(2).strip()
    ns2 = re.search(r"^[#]*(.(?=[^/\r\n\(:\,!]*(?:\s[-–]\s))[^-–\r\n]*|.[^/\r\n\(:\,!]*)", s, re.IGNORECASE)
    if ns2:
        s = ns2.group(1).strip()
    return s


def formatARTICLES(s):
    # L'original passait re.IGNORECASE en 4e argument de re.sub, c'est-à-dire comme nombre
    # maximal de remplacements (2) : la recherche des articles respecte donc la casse.
    ns = re.sub(r"^(" + _articles() + r")\s*(?<=['\s])((?=[^(]*(?:\s[-–:]\s))[^-–:\r\n]*|[^\(/\r\n]*)"
                r"(?![-–:/\(])\s*([^\r\n]*)", r"\2 (\1) \3", s, count=int(re.IGNORECASE))
    if ns:
        s = Capitalize(ns.strip())
    return s


def titlize(s, formatArticles=False):
    s = str(s or "")
    if formatArticles and C("FORMATARTICLES"):
        s = formatARTICLES(s)
    if C("TITLEIT"):
        NewString = ""
        Ucase = False
        for i in range(len(s.strip())):
            if Ucase or i == 0:
                NewString += s[i:i + 1].upper()
            else:
                NewString += s[i:i + 1]
            Ucase = not (s[i:i + 1]).isalnum() and s[i:i + 2].lower() != "'s"
        return NewString
    return s


def Capitalize(s):
    return s[0:1].upper() + s[1:]


def parseName(extractedName):
    name = extractedName.strip()
    if not C("AcceptGenericArtists") and re.match(r"&lt;", extractedName):
        return ""
    nameRegex = LAST_FIRST_NAMES.search(extractedName)
    if nameRegex:
        name = nameRegex.group("firstname") + " " + nameRegex.group("name")
    return checkWebChar(name).strip()


def getGenericBookArtists(patterns, book_info, label):
    """Artistes (scénario, dessin…) : noms des liens de la case ``<dd>`` du libellé."""
    result = ""
    artist_list = []
    for pattern in patterns:
        nameRegex = pattern.search(book_info, 0)
        if nameRegex:
            if "&lt;Indéterminé&gt;" in nameRegex.group(1):
                if C("AcceptGenericArtists"):
                    thisArtist = "<Indéterminé>"
                    if thisArtist not in artist_list:
                        artist_list.append(thisArtist)
            else:
                for artist_multi in ALBUM_MULTI_AUTHOR_NAMES.finditer(nameRegex.group(1)):
                    thisArtist = parseName(artist_multi.group(1).strip())
                    if thisArtist and thisArtist not in artist_list:
                        artist_list.append(thisArtist)
            result = ", ".join(artist_list)
    debuglog(label + ":", result)
    return result


# ---------------------------------------------------------------------------------------
# Fiche : état du livre pendant l'analyse, puis modifications demandées à ComicRust
# ---------------------------------------------------------------------------------------

_ENTIERS_FICHE = {"Count", "Year", "Month", "Day", "Volume", "AlternateCount", "PageCount"}
_REELS_FICHE = {"CommunityRating", "Rating"}
_OUI_NON = {"SeriesComplete", "BlackAndWhite"}
# Lus par l'original mais non modifiables dans ComicRust.
_NON_MODIFIABLES = {"PageCount"}


class Fiche:
    """Le ``book`` de l'original : on lit et on écrit les champs comme dans ComicRack
    (vide = "" ; -1 pour un nombre ; "Unknown" pour Oui/Non) ; les écritures sont notées
    puis demandées à ComicRust par :meth:`appliquer`."""

    def __init__(self, book):
        object.__setattr__(self, "book", book)
        object.__setattr__(self, "vals", book.to_dict())
        object.__setattr__(self, "modifs", {})

    @property
    def FilePath(self):
        return self.book.file or ""

    def __getattr__(self, nom):
        v = self.vals.get(nom)
        if v is None:
            if nom in _ENTIERS_FICHE:
                return -1
            if nom in _REELS_FICHE:
                return 0.0
            if nom in _OUI_NON:
                return "Unknown"
            if nom == "EnableProposed":
                return True
            return ""
        return v

    def __setattr__(self, nom, valeur):
        # « Ne remplacer que les champs vides » : un champ déjà rempli garde sa valeur, y
        # compris pour la suite de l'analyse (titre repris dans le résumé…).
        if C("ChampsVidesSeulement") and nom not in _NON_MODIFIABLES and not _vide(self.book.get(nom)):
            return
        self.vals[nom] = valeur
        self.modifs[nom] = valeur

    def appliquer(self, vides_seulement):
        """Demande à ComicRust les champs modifiés ; renvoie leur nombre."""
        n = 0
        for champ, v in self.modifs.items():
            if champ in _NON_MODIFIABLES:
                debuglog("%s non modifiable dans ComicRust (%s), ignoré" % (champ, v))
                continue
            val = _normaliser(champ, v)
            avant = self.book.get(champ)
            if vides_seulement and not _vide(avant):
                continue
            if _egal(avant, val):
                continue
            try:
                self.book.set(champ, val)
                n += 1
            except (AttributeError, ValueError) as e:
                debuglog("%s refusé : %s" % (champ, e))
        return n


def _normaliser(champ, v):
    if isinstance(v, str):
        v = v.strip()
        if v == "" or (champ in _OUI_NON and v == "Unknown"):
            return None
        return v
    if champ in _ENTIERS_FICHE and isinstance(v, (int, float)) and v < 0:
        return None
    if champ in _REELS_FICHE and isinstance(v, (int, float)) and not v:
        return None
    return v


def _vide(v):
    if v is None:
        return True
    if isinstance(v, str):
        return not v.strip()
    if isinstance(v, bool):
        return False
    if isinstance(v, (int, float)):
        return v < 0
    return False


def _egal(a, b):
    if a is None or b is None:
        return a is None and b is None
    if isinstance(a, (int, float)) and isinstance(b, (int, float)) and not isinstance(a, bool):
        return float(a) == float(b)
    return str(a) == str(b)


def write_book_notes(f):
    f.Notes = "Bedetheque.com - " + _maintenant() + "\n" + "BD2 scraper v" + VERSION


def _proposition(book):
    """Série et numéro « proposés » d'après le nom du fichier (ShadowSeries / ShadowNumber de
    ComicRack, approximation), sauf si l'utilisateur a désactivé les valeurs proposées."""
    if book.get("EnableProposed") is False or not book.file:
        return "", ""
    nom = os.path.splitext(os.path.basename(str(book.file).replace("\\", "/")))[0]
    reste = " ".join(re.sub(r"\s*[(\[][^)\]]*[)\]]", " ", nom).replace("_", " ").split())
    m = re.match(r"^(.+?)(?:\s*[-–]\s*|\s+)(?:(?:tome|t|n°|no\.?)\s*|#\s*)?(\d+(?:[.,]\d+)?)"
                 r"(?:\s*[-–:]\s*.+?)?\s*$", reste, re.IGNORECASE)
    if m:
        return m.group(1).strip(" -–"), m.group(2)
    return reste, ""


# ---------------------------------------------------------------------------------------
# Fenêtres de choix (SeriesForm de l'original)
# ---------------------------------------------------------------------------------------

SERIE, ALBUM, EDITION = 1, 2, 3


def _lien_court(lien):
    """Lien affiché dans une fenêtre de choix : sans l'adresse du site."""
    u = str(lien or "")
    if _sur_le_site(u):
        u = u[len(S.base) + 1:]
    return _relatif(u)


def _infos_edition(info):
    """Dépôt légal, éditeur et ISBN d'une édition (encart de la page de l'album), pour
    distinguer les éditions dans la fenêtre de choix."""
    def champ(regex):
        m = regex.search(info or "")
        return checkWebChar(strip_tags(m.group(1))).strip() if m else ""
    depot = ALBUM_DEPOT.search(info or "")
    depot = (depot.group(0).split("<dd>")[-1].strip() if depot else "")
    return depot, champ(ALBUM_EDITEUR), champ(ALBUM_ISBN)


def SeriesForm(serie, liste, formType):
    """Fenêtre de choix ; renvoie le rang choisi ou ``None`` (annulé ou délai expiré sans
    sélection). Avec le choix automatique (CBStop = "2"), la fenêtre se ferme toute seule
    après TIMEPOPUP secondes : pour une série, rien n'est choisi (comme l'original) ;
    pour un album ou une édition, la première ligne.

    Fiches avec couverture (comicrust.choose, présentation « cards ») : titre en gras puis
    les infos utiles au choix ; l'original n'avait qu'une liste de textes
    « (001) - titre (lien) »."""
    auto = AllowUserChoice() == "2"
    if formType == SERIE:
        titre = Trans(132) + serie
        colonnes = [_t("Série", "Series"), _t("Éditeur / pays", "Publisher / country"), _t("Résultat", "Result"),
                    _t("Lien", "Link")]
        lignes = [[x[1], x[4] if len(x) > 4 else "", x[2], _lien_court(x[0])] for x in liste]
        defaut = None
    elif formType == EDITION:
        titre = Trans(145) + serie
        colonnes = [_t("Titre", "Title"), _t("N°", "No."), _t("Dépôt légal", "Legal deposit"),
                    _t("Éditeur", "Publisher"), "ISBN", _t("Identifiant", "Id")]
        lignes = [[x[1].Title, " ".join(v for v in (x[1].N, x[1].A) if v)] + list(_infos_edition(x[1].Info)) + [x[1].ID]
                  for x in liste]
        defaut = 0
    else:
        titre = Trans(146) + serie
        colonnes = [_t("Titre", "Title"), _t("N°", "No."), _t("Parution", "Published"), _t("Éditeur", "Publisher"),
                    _t("Lien", "Link")]
        lignes = []
        for x in liste:
            d = x[4] if len(x) > 4 and isinstance(x[4], dict) else {}
            lignes.append([d.get("titre") or x[1], d.get("num", ""), d.get("parution", ""), d.get("editeur", ""),
                           _lien_court(x[0])])
        defaut = 0
    images = _images_choix(liste, formType)
    try:
        delai = int(C("TIMEPOPUP")) if auto else None
    except ValueError:
        delai = 30
    return comicrust.choose(titre, colonnes, lignes, images=images, default=defaut,
                            timeout=delai if delai and delai > 0 else None,
                            thumb_size=C("TailleVignettes"), layout="cards",
                            message=_t("Double-clic ou Entrée pour choisir, ↑/↓ pour changer de ligne ; Annuler "
                                       "pour ignorer ce livre.",
                                       "Double-click or Enter to pick, ↑/↓ to move; Cancel to skip this book."))


# ---------------------------------------------------------------------------------------
# Recherche de la série
# ---------------------------------------------------------------------------------------

def SetSerieId(f, serie, num, nBooksIn):
    if serie:
        letter = remove_accents(serie[0]) if re.match("[a-z]", remove_accents(serie[0]).lower()) else "0"
    else:
        return ""
    urlN = "/bandes_dessinees_" + letter + ".html"
    S.RenameSeries = False
    serieUrl = ""
    alt = if_else(S.dlgAltNumber == "", "", " AltNo. " + str(S.dlgAltNumber))
    try:
        debuglog("AlwaysChooseSerie: " + str(C("AlwaysChooseSerie")))
        if not C("AlwaysChooseSerie"):
            request = _read_url(urlN, False)
            if stop():
                return ""
            if request:
                nameRegex = re.compile(SERIE_URL_PATTERN % checkRegExp(serie.strip()), I | D).search(request)
                if nameRegex:
                    serieUrl = nameRegex.group(1)
                    if ".html" not in serieUrl:
                        serieUrl += ".html"
                    debuglog(Trans(23) + serieUrl)
                    return serieUrl

        serie_rech = remove_accents(serie.lower())
        if serie_rech == S.serie_rech_prev and S.PickSeries:
            S.RenameSeries = S.PickSeries
            return S.PickSeriesLink
        S.serie_rech_prev = serie_rech
        S.PickSeries = False

        ListSeries = []
        debuglog("Nom de Série pour recherche = " + S.dlgNameClean)
        # RechWhere : 0 = tout le site, 7 = toute la bédéthèque, 9 = revues, 10 = séries
        urlN = "/search/tout?RechTexte=" + remove_accents(S.dlgNameClean.lower().strip()) + "&RechWhere=7"
        debuglog(Trans(113), _hote_site() + urlN)
        request = _read_url(urlN, False)
        if stop():
            return ""
        if SERIE_LIST_CHECK.search(request) or REVUE_LIST_CHECK.search(request):
            comicrust.message(Trans(2) + "\n" + Trans(114) + "[" + titlize(f.Series) + "] !")
            debuglog(Trans(114) + "[" + titlize(f.Series) + "] !")
            return ""

        i = 1
        for m in re.finditer(SERIE_LIST_PATTERN % re.escape(S.base), request, I | D):
            ListSeries.append(["serie-" + m.group(1), checkWebChar(strip_tags(m.group(2))), str(i).zfill(3),
                               _img(m.group(0)), _infos_resultat(m.group(0))])
            i += 1
        if REVUE_LIST_EXISTS.search(request):
            for m in re.finditer(REVUE_LIST_PATTERN % re.escape(S.base), request, I | D):
                ListSeries.append(["revue-" + m.group(1), checkWebChar(strip_tags(m.group(2))), str(i).zfill(3),
                                   _img(m.group(0)), _infos_resultat(m.group(0)) or "Revue"])
                i += 1
        ListSeries.sort(key=lambda x: x[2])

        def journal_choix(nom, lien):
            debuglog(Trans(24) + checkWebChar(serie) + "]")
            debuglog(Trans(111) + nom)
            log_BD("** [" + serie + "] " + num + alt + " - " + titlize(f.Title) + " (" + _hote_site() + serieUrl + ")",
                   Trans(25), 1)
            log_BD(Trans(111), "[" + nom + "] " + num + alt + " - " + titlize(f.Title) + " (" + _hote_site() + "\\" +
                   lien + ")", 1)

        if len(ListSeries) == 1 and not C("AlwaysChooseSerie"):
            journal_choix(ListSeries[0][1], ListSeries[0][0])
            S.RenameSeries = ListSeries[0][1]
            return ListSeries[0][0]

        if len(ListSeries) > 1 or (C("AlwaysChooseSerie") and len(ListSeries) >= 1):
            if AllowUserChoice() or nBooksIn == 1:
                lUnique = False
                nItem = 0
                for k, x in enumerate(ListSeries):
                    same = remove_accents(x[1].lower()) == remove_accents(S.dlgName.lower().strip())
                    if same:
                        lUnique = True
                        nItem = k
                    if same and re.search(r"\(.{4,}?\)", x[1].lower()):
                        lUnique = False
                    if C("AlwaysChooseSerie"):
                        lUnique = False
                if lUnique:
                    journal_choix(ListSeries[nItem][1], ListSeries[nItem][0])
                    S.RenameSeries = ListSeries[nItem][1]
                    return ListSeries[nItem][0]
                rang = SeriesForm(serie, ListSeries, SERIE)
                if rang is None:
                    debuglog(Trans(24) + checkWebChar(serie) + "]")
                    log_BD("** [" + serie + "] " + num + alt + " - " + titlize(f.Title), Trans(25), 1)
                    return ""
                NewLink, NewSeries = ListSeries[rang][0], ListSeries[rang][1]
                journal_choix(NewSeries, NewLink)
                S.RenameSeries = NewSeries
                S.PickSeries = NewSeries
                S.PickSeriesLink = NewLink
                return NewLink
            debuglog(Trans(142) + checkWebChar(serie) + "]")
            log_BD("** [" + serie + "] " + num + alt + " - " + titlize(f.Title), Trans(142), 1)
            return ""
    except Exception:
        cError = debuglogOnError()
        log_BD("** Error [" + serie + "] " + num + " - " + titlize(f.Title), cError, 1)
    return serieUrl


def _img(fragment):
    """Première image d'un morceau de page qui peut être une couverture (pas un drapeau,
    une icône ou une image de remplissage) ; adresse complète, ou None."""
    for balise in IMG_TAG.finditer(fragment or ""):
        tag = balise.group(0)
        classe = IMG_CLASSE.search(tag)
        if classe and IMG_CLASSE_PAS_COUV.search(classe.group(1)):
            continue
        attrs = {m.group(1).lower(): m.group(2) for m in IMG_ATTR.finditer(tag)}
        for nom in ("data-src", "data-original", "data-lazy-src", "src"):
            u = attrs.get(nom, "").strip()
            if u and not IMG_PAS_COUV.search(u):
                return checkWebChar(u)
    return None


def _infos_resultat(fragment):
    """Infos secondaires d'un résultat de recherche (éditeur… de « bdt-liste-info », pays
    d'après le drapeau), pour les fiches de la fenêtre de choix."""
    morceaux = [checkWebChar(strip_tags(m.group(1))).strip() for m in LISTE_INFO.finditer(fragment or "")]
    drapeau = LISTE_DRAPEAU.search(fragment or "")
    if drapeau:
        morceaux.append(drapeau.group(1).replace("_", " "))
    return " · ".join(x for x in morceaux if x)


def _couverture_page_album(page):
    """Couverture d'un album d'après sa page : celle de la première édition, sinon l'image
    de partage (og:image)."""
    m = PAGE_COUV.search(page or "")
    if m:
        return checkWebChar(m.group(1))
    m = PAGE_OG_IMAGE.search(page or "")
    return checkWebChar(m.group(1)) if m and not IMG_PAS_COUV.search(m.group(1)) else None


def _infos_albums(page, urls):
    """Couverture, dépôt légal et éditeur des albums ``urls`` d'une page de série, pour la
    fenêtre de choix de l'album : autour de chaque lien vers un album (numéro en fin
    d'adresse), jusqu'aux liens des albums voisins, l'image ou le lien de couverture
    nommé d'après l'album (« Couv_901.jpg »), sinon le plus proche, puis « Dépot légal »
    et « Editeur ». Rend {numéro: {"img", "parution",
    "editeur"}} (champs trouvés seulement)."""
    page = page or ""
    voulus = {}
    for u in urls:
        m = re.search(r"-(\d+)\.html", u or "")
        if m:
            voulus[m.group(1)] = True
    liens = [(m.start(), m.group(1)) for m in LIEN_ALBUM_PAGE.finditer(page) if m.group(1) in voulus]
    out = {}
    for k, (pos, n) in enumerate(liens):
        avant = [p for p, x in liens[:k] if x != n]
        apres = [p for p, x in liens[k + 1:] if x != n]
        debut = max(avant[-1] if avant else 0, pos - 4000)
        fin = min(apres[0] if apres else len(page), pos + 4000)
        bloc = page[debut:fin]
        d = out.setdefault(n, {})
        if "img" not in d:
            candidats = []
            for b in IMG_TAG.finditer(bloc):
                u = _img(b.group(0))
                if u:
                    candidats.append((abs(debut + b.start() - pos), u))
            for m in LIEN_COUV.finditer(bloc):
                candidats.append((abs(debut + m.start() - pos), checkWebChar(m.group(1))))
            # D'abord l'image nommée d'après l'album (« Couv_901.jpg »), puis les couvertures
            # (adresse « couv »), puis la plus proche du lien.
            candidats.sort(key=lambda c: (not re.search(r"(?i)\D%s\D" % n, c[1]),
                                          not re.search(r"(?i)couv", c[1]), c[0]))
            if candidats:
                d["img"] = candidats[0][1]
        suite = page[pos:fin]
        if "parution" not in d:
            m = INFO_DEPOT.search(suite)
            if m:
                d["parution"] = m.group(1)
        if "editeur" not in d:
            m = INFO_EDITEUR.search(suite)
            if m and checkWebChar(m.group(1)).strip():
                d["editeur"] = checkWebChar(m.group(1)).strip()
    return out


def _couverture_page_serie(page):
    """Couverture d'une série d'après sa page : celle du premier album de la liste (image
    dans la liste, sinon trouvée autour du lien du premier album), sinon celle de l'album
    affiché, sinon l'image de partage (og:image), sinon la première couverture de la page."""
    r = SERIE_ALBUMS.search(page or "")
    img = _img(r.group(0)) if r else None
    if not img and r:
        m = re.search(r"-(\d+)\.html", r.group("url"))
        img = _infos_albums(page, [r.group("url")]).get(m.group(1), {}).get("img") if m else None
    if not img:
        m = PAGE_COUV.search(page or "")
        img = checkWebChar(m.group(1)) if m else None
    if not img:
        m = PAGE_OG_IMAGE.search(page or "")
        img = checkWebChar(m.group(1)) if m and not IMG_PAS_COUV.search(m.group(1)) else None
    if not img:
        m = LIEN_COUV.search(page or "")
        img = checkWebChar(m.group(1)) if m else None
    return img


# ---------------------------------------------------------------------------------------
# Page de la série
# ---------------------------------------------------------------------------------------

def SetAlbumInformation(f, serieUrl, serie, num):
    albumUrl = parseSerieInfo(f, serieUrl, False)
    if stop():
        return False
    if albumUrl and "/revue-" not in serieUrl:
        debuglog(Trans(26), albumUrl)
        return bool(parseAlbumInfo(f, albumUrl, num))
    if "/revue-" in serieUrl:
        return bool(albumUrl)
    alt = if_else(S.dlgAltNumber == "", "", " AltNo." + str(S.dlgAltNumber))
    debuglog(Trans(26), Trans(25))
    debuglog(Trans(27) + serie + "] " + num + alt + "\n")
    log_BD("   [" + serie + "] " + num + alt + " - " + titlize(f.Title) + " (" + serieUrl + ")", Trans(28), 1)
    return False


def parseSerieInfo(f, serieUrl, lDirect):
    debuglog("=" * 60)
    debuglog("parseSerieInfo", "a)", serieUrl, "b)", lDirect)
    debuglog("=" * 60)
    S.SkipAlbum = False
    albumURL = ""
    if stop():
        return False
    request = _read_url(serieUrl, lDirect)
    if stop():
        return False

    if "/revue-" in serieUrl:
        return _revue(f, serieUrl, lDirect, request)

    if not request:
        return ""
    Entete = request
    if S.RenameSeries and C("CBSeries"):
        f.Series = titlize(S.RenameSeries)

    # Série d'un lien direct (QuickScrape)
    if lDirect and C("CBSeries"):
        nameRegex = SERIE_QSERIE.search(Entete)
        if nameRegex:
            qserie = checkWebChar(nameRegex.group(1).strip())
            f.Series = titlize(qserie)
            debuglog(Trans(9), qserie)
        else:
            return ""

    if C("CBGenre"):
        nameRegex = SERIE_GENRE.search(Entete)
        genre = checkWebChar(nameRegex.group(1).strip()) if nameRegex else ""
        if genre != "":
            f.Genre = genre
            # Genre érotique : public « PG », comme l'original.
            if "rotique" in genre.lower():
                f.AgeRating = "PG"
        debuglog(Trans(51), f.Genre)

    if C("CBSynopsys"):
        nameRegex = SERIE_RESUME.search(checkWebChar(Entete.replace("\r\n", "")), 0)
        resume = strip_tags(nameRegex.group(1)).strip() if nameRegex else ""
        resume = re.sub(r"Tout sur la série.*?:\s?", "", resume, count=2)
        S.Serie_Resume = checkWebChar(resume).strip()
        debuglog(if_else(resume, Trans(52), Trans(53)))

    numero = str(S.dlgNumber or "").lower()
    if C("CBStatus"):
        nameRegex = SERIE_STATUS.search(Entete)
        fin = checkWebChar(nameRegex.group(1).strip()) if nameRegex else ""
        if fin:
            log_BD(fin, Trans(25), 1)
        debuglog("Parution: " + fin)
        if "finie" in fin or numero == "one shot":
            f.SeriesComplete = "Yes"
            SerieState = Trans(54)
        elif "one shot" in fin.lower() and numero != "one shot":
            f.SeriesComplete = "Yes"
            if C("ONESHOTFORMAT") and not C("CBFormat"):
                f.Format = "One Shot"
            SerieState = Trans(54)
        elif "cours" in fin:
            f.SeriesComplete = "No"
            SerieState = Trans(55)
        else:
            f.SeriesComplete = "Unknown"
            SerieState = Trans(56)
        debuglog(Trans(57) + SerieState + if_else(numero == "one shot", " (One Shot)", ""))

    if C("CBLanguage"):
        nameRegex = SERIE_LANGUE.search(Entete)
        if nameRegex:
            langue = nameRegex.group(1).strip()
            debuglog(Trans(36), langue[:2])
            if langue[:2] in LANGUES:
                f.LanguageISO = LANGUES[langue[:2]]

    if not C("CBDefault"):
        f.EnableProposed = False
        debuglog(Trans(136), "No")

    SerieInfoRegex = SERIE_HEADER2.search(request)
    if SerieInfoRegex:
        Entete2 = SerieInfoRegex.group(1)
        if C("CBCount") and not lDirect:
            count = 0
            cCountText = ""
            if C("COUNTFINIE") and f.SeriesComplete == "No":
                f.Count = -1
                cCountText = "---"
            elif not C("COUNTOF"):
                nameRegex = SERIE_COUNT.search(Entete2)
                if nameRegex and S.AlbumNumNum:
                    f.Count = int(checkWebChar(nameRegex.group(1)))
                    cCountText = str(f.Count)
                else:
                    f.Count = -1
                    cCountText = "---"
            else:
                nameRegex = SERIE_COUNT_REAL.search(request)
                if nameRegex:
                    for numof in SERIE_COUNTOF.finditer(nameRegex.group(1)):
                        valeur = numof.group(1).strip()
                        if isnumeric(valeur) and float(valeur) > count:
                            count = int(float(valeur))
                    if count > 0 and S.AlbumNumNum:
                        f.Count = count
                        cCountText = str(count)
                    elif not S.AlbumNumNum:
                        f.Count = -1
                else:
                    f.Count = -1
                    cCountText = "---"
            debuglog(Trans(59) + if_else(numero == "one shot", "1", cCountText))

    ListAlbum, ListAlbumAll = [], []
    albums = list(SERIE_ALBUMS.finditer(request))
    infos = _infos_albums(request, [r.group("url") for r in albums])
    for i, r in enumerate(albums):
        n, a, url, title = r.group("tome"), r.group("alt"), r.group("url"), r.group("title")
        num = if_else(n, n, if_else(a, a, ""))
        m = re.search(r"-(\d+)\.html", url)
        d = dict(infos.get(m.group(1), {}) if m else {})
        d.update(titre=checkWebChar(title), num=(n or "") + (a or ""))
        item = [url, num + ". " + checkWebChar(title), str(i).zfill(3), _img(r.group(0)) or d.get("img"), d]
        ListAlbumAll.append(item)
        if S.dlgNumber != "" and num == S.dlgNumber and not lDirect:
            ListAlbum.append(item)
    if len(ListAlbum) == 0:
        ListAlbum = ListAlbumAll

    albumURL = AlbumChooser(ListAlbum)
    if not albumURL and not S.SkipAlbum:
        # Pas de liste d'albums à côté : sans doute un seul album, pris dans la page.
        r = SERIE_PREMIER.search(request)
        if r:
            albumURL = r.group(1)
            debuglog("---> Numéro n'existe pas dans la liste, choix du 1er item")
        else:
            return ""
    return albumURL


def AlbumChooser(ListAlbum):
    albumURL = ""
    debuglog("Nbr. d'item dans la Liste Album est de: " + str(len(ListAlbum)))
    if len(ListAlbum) > 1:
        if AllowUserChoice():
            rang = SeriesForm(str(S.dlgNumber), ListAlbum, ALBUM)
            if rang is None:
                albumURL = False
                S.SkipAlbum = True
                debuglog("---> Appuyer sur Cancel, ignorons ce livre")
            else:
                albumURL = ListAlbum[rang][0]
        else:
            albumURL = False
            S.SkipAlbum = True
            debuglog("---> Plus d'un item mais l'option pause scrape est désactivé")
    elif len(ListAlbum) == 1:
        albumURL = ListAlbum[0][0]
        debuglog("---> Seulement 1 item dans la liste")
    return albumURL


# ---------------------------------------------------------------------------------------
# Revues
# ---------------------------------------------------------------------------------------

def _revue(f, serieUrl, lDirect, request):
    ListAlbum = []
    options = re.findall(r'<option\svalue="(%s/revue-[^>]+?)">(.+?)</option>' % re.escape(S.base), request, I | D | M)
    if not options:
        for i, revueNum in enumerate(re.findall(r'<a name="(.+?)">.+?class="titre".{1,100}?#(.+?)\..+?', request,
                                                I | D | M), 1):
            full_url = (serieUrl if _sur_le_site(serieUrl) else S.base + "/" + serieUrl) + "#" + revueNum[0]
            ListAlbum.append([full_url, "Num: " + revueNum[1].strip(), str(i).zfill(5)])
    else:
        for i, albumPick in enumerate(options, 1):
            ListAlbum.append([albumPick[0], "Num: " + albumPick[1].strip(), str(i).zfill(5)])

    matched = next((x for x in ListAlbum if x[1] == "Num: " + str(S.dlgNumber)), None)
    if matched is not None and not lDirect:
        albumURL = matched[0]
    elif lDirect and "#" in serieUrl:
        albumURL = serieUrl
    else:
        albumURL = AlbumChooser(ListAlbum)
    if not albumURL:
        return ""
    request = _read_url(albumURL, False)
    ID = albumURL.split("#")[1] if "#" in albumURL else ""
    if ID:
        motif = REVUE_HEADER_PATTERN_ALT % re.escape(ID)
    else:
        motif = REVUE_HEADER_PATTERN % re.escape(str(S.dlgNumber))
    SerieInfoRegex = re.compile(motif, I | M | D).search(request)
    if SerieInfoRegex:
        return parseRevueInfo(f, SerieInfoRegex, albumURL)
    return ""


def parseRevueInfo(f, SerieInfoRegex, serieUrl):
    debuglog("=" * 60)
    debuglog("parseRevueInfo", "a)", serieUrl)
    debuglog("=" * 60)
    try:
        Entete = SerieInfoRegex.group(1)
        Numero = SerieInfoRegex.group(3)
        if C("CBSeries"):
            f.Series = titlize(S.RenameSeries) if S.RenameSeries else titlize(f.Series)
        if Numero:
            f.Number = Numero
            debuglog(Trans(115), f.Number)
        if C("CBTitle"):
            nameRegex = re.search(r'<h3 class="titre".+?</span>(.+?)</h3>', Entete, I | D | M)
            if nameRegex:
                f.Title = titlize(checkWebChar(nameRegex.group(1).strip()))
            debuglog(Trans(29), f.Title)
        if C("CBGenre"):
            f.Genre = "Revue"
            debuglog(Trans(51), f.Genre)
        if C("CBSynopsys"):
            nameRegex = REVUE_RESUME.search(Entete)
            resume = strip_tags(nameRegex.group(1)).strip() if nameRegex else ""
            f.Summary = checkWebChar(resume).strip()
            debuglog(if_else(resume, Trans(52), Trans(53)))
        if C("CBRating"):
            nameRegex = SERIE_NOTE.search(Entete)
            note = _reel(nameRegex.group("note")) if nameRegex else 0.0
            f.CommunityRating = note
            debuglog(Trans(58) + str(note))
        if C("CBCover"):
            S.couverture = SerieInfoRegex.group(2)
            debuglog(Trans(105), S.couverture)
        if C("CBPrinted"):
            nameRegex = REVUE_DEPOT.search(Entete, 0)
            date = nameRegex.group(1).strip() if nameRegex else "-"
            mois, annee = _entier(date[3:5]), _entier(date[6:10])
            if date != "-" and 1 <= mois <= 12:
                f.Month, f.Year = mois, annee
                debuglog(Trans(34), str(f.Month) + "/" + str(f.Year))
            else:
                f.Month, f.Year = -1, -1
        if C("CBEditor"):
            nameRegex = REVUE_EDITEUR.search(Entete, 0)
            f.Publisher = parseName(strip_tags(nameRegex.group(1))) if nameRegex else ""
            debuglog(Trans(35), f.Publisher)
        if not f.FilePath:
            nameRegex = REVUE_PLANCHES.search(Entete, 0)
            if nameRegex:
                debuglog(Trans(122), nameRegex.group(1).strip())
        if C("CBFormat"):
            nameRegex = REVUE_PERIOD.search(Entete, 0)
            if nameRegex:
                f.Format = nameRegex.group(1).strip()
                debuglog(Trans(131), nameRegex.group(1))
        if C("CBLanguage") and not f.LanguageISO:
            f.LanguageISO = "fr"
        if C("CBWeb") and not C("CBRescrape"):
            f.Web = serieUrl
            debuglog(Trans(123), f.Web)
        # Autres champs remis à zéro (comme l'original).
        f.SeriesComplete = "Unknown"
        f.BlackAndWhite = "Unknown"
        f.AlternateNumber = ""
        f.Tags = ""
        f.Writer = ""
        f.Penciller = ""
        f.CoverArtist = ""
        f.Letterer = ""
        f.Inker = ""
        f.ISBN = ""
        f.Imprint = ""
        f.CommunityRating = 0.0
        if C("CBNotes"):
            write_book_notes(f)
        return True
    except Exception:
        debuglogOnError()
        return False


def _entier(t, defaut=-1):
    try:
        return int(str(t).strip())
    except (TypeError, ValueError):
        return defaut


def _reel(t, defaut=0.0):
    try:
        return float(str(t).strip().replace(",", "."))
    except (TypeError, ValueError):
        return defaut


# ---------------------------------------------------------------------------------------
# Page de l'album
# ---------------------------------------------------------------------------------------

class AlbumInfo:
    def __init__(self, n, a, title, info, couv, url, bookId):
        self.Couv = couv
        self.N = n
        self.A = a
        self.Title = title
        self.Info = info
        self.URL = url
        self.ID = bookId


def _mois_annee(m):
    """Mois et année d'un dépôt légal (« 03/2000 ») ; (-1, -1) si absent ou « -/2000 »
    (comme l'original). Une année seule (« 2000 ») donne l'année sans mois : l'original
    lisait alors le mois 20, refusé par ComicRust."""
    if not m:
        return -1, -1
    mois, annee = m.group("month") or "", m.group("year") or ""
    if "/" not in m.string[m.start("month"):m.end()]:
        chiffres = mois + annee
        return (-1, int(chiffres)) if len(chiffres) == 4 and chiffres.isdigit() else (-1, -1)
    if mois in ("", "-", "--") or not annee:
        return -1, -1
    mois, annee = _entier(mois), _entier(annee)
    if not 1 <= mois <= 12:
        return -1, annee
    return mois, annee


def parseAlbumInfo(f, pageUrl, num, lDirect=False):
    debuglog("=" * 60)
    debuglog("parseAlbumInfo", "a)", pageUrl, "b)", num, "c)", lDirect)
    debuglog("=" * 60)
    AlbumBDThequeNum = ""
    if stop():
        return False
    albumHTML = _read_url(pageUrl, False)
    if stop():
        return False
    if not albumHTML:
        # Page introuvable : l'original comptait quand même le livre comme « traité ».
        log_BD("   " + pageUrl + " " + Trans(43), "", 1)
        return False
    info_album_regex = INFOS_ALBUMS.search(albumHTML)
    info_album = info_album_regex.group() if info_album_regex else ""

    cBDNumS = re.search(r"-(\d+).html", pageUrl)
    if cBDNumS:
        AlbumBDThequeNum = cBDNumS.group(1)
    elif "serie-" in pageUrl:
        m = re.search(r"serie.*?\.html\#(\d+)$", pageUrl)
        if m:
            AlbumBDThequeNum = m.group(1)

    try:
        ListAlbum = []
        pickedVar = None
        picked = False
        tome = ALBUM_TOME.search(albumHTML)
        t = if_else(tome.group(1).strip(), tome.group(1).strip(), checkWebChar(tome.group(2).strip())) if tome else ""
        for i, albumPick in enumerate(ALBUM_EDITIONS.finditer(albumHTML)):
            couv = albumPick.group("couv")
            title = checkWebChar(albumPick.group("titre").strip())
            nfo = albumPick.group("infos")
            altb = albumPick.group("alt")
            a = checkWebChar(altb.strip() if isnumeric(t) else altb.strip().replace(t, "", 1).strip()) if altb else ""
            bookId = albumPick.group("id").strip()
            url = pageUrl + "#reed" if i == 0 else pageUrl + "#" + bookId
            debuglog("Tome)", t, "Alt)", a, "Title)", title, "bookId)", bookId)
            ListAlbum.append([a, AlbumInfo(t, a, title, nfo, couv, url, bookId), str(i).zfill(3), couv])

        if len(ListAlbum) == 1:
            pickedVar = ListAlbum[0][1]
            debuglog("---> Seulement 1 item dans la liste")
        elif len(ListAlbum) > 1:
            for e in ListAlbum:
                # Édition dont le numéro alternatif est celui du livre : choisie d'office.
                if S.dlgAltNumber != "" and e[1].A == S.dlgAltNumber:
                    pickedVar = e[1]
                    picked = True
                    break
            if C("PopUpEditionForm"):
                picked = False
            if C("PopUpEditionForm") and AllowUserChoice() and not picked:
                rang = SeriesForm(str(num), ListAlbum, EDITION)
                if rang is None:
                    pickedVar = ListAlbum[0][1]
                    debuglog("---> Cancel appuyer, on choisi le premier")
                else:
                    pickedVar = ListAlbum[rang][1]
            elif not picked:
                pickedVar = ListAlbum[0][1]
                debuglog("---> Choix du 1er item")

        if pickedVar is None:
            log_BD("   " + pageUrl + " " + Trans(43), "", 1)
            debuglog("Aucune édition reconnue dans la page")
            return False
        info = pickedVar.Info
        debuglog("Choisi #Alt: " + pickedVar.A + " // Titre: " + pickedVar.Title + " // ID: " + pickedVar.ID)
        S.couverture = pickedVar.Couv

        if S.RenameSeries:
            if C("CBSeries"):
                f.Series = titlize(S.RenameSeries)
        elif S.Shadow1 or f.Series != titlize(S.dlgName):
            if C("CBSeries"):
                f.Series = titlize(S.dlgName)
        if S.Shadow2:
            f.Number = S.dlgNumber

        if C("CBWeb") and not C("CBRescrape"):
            if not C("ShortWebLink"):
                f.Web = pickedVar.URL.replace("#reed", "")
            else:
                cBelid = re.search(r"-(\d+).html", pageUrl)
                if cBelid:
                    f.Web = _hote_site() + "/BD--" + cBelid.group(1) + ".html"
            debuglog(Trans(123), f.Web)

        qnum = pickedVar.N
        anum = pickedVar.A
        f.Number = anum if not qnum and anum else qnum
        f.AlternateNumber = S.dlgAltNumber if not qnum and anum else anum
        if C("PadNumber") != "0":
            if isPositiveInt(f.Number):
                f.Number = str(f.Number).zfill(_entier(C("PadNumber"), 0))
            if isPositiveInt(f.AlternateNumber):
                f.AlternateNumber = str(f.AlternateNumber).zfill(_entier(C("PadNumber"), 0))
        debuglog("Num: ", f.Number)
        debuglog("Alt: ", f.AlternateNumber)

        series = f.Series
        seriesFormat = ""
        nameRegex = ALBUM_SERIE_ENTETE.search(albumHTML)  # « Les 5 Terres » : entête
        nameRegex2 = ALBUM_SERIE_ENCART.search(info_album)  # « 5 Terres (Les) » : encart
        if nameRegex:
            series = checkWebChar(nameRegex.group(1).strip())
            seriesFormat = checkWebChar(nameRegex2.group(1).strip()) if nameRegex2 else series
            debuglog(Trans(9) + series + " // Formaté: " + seriesFormat)

        if C("CBTitle"):
            NewTitle = titlize(pickedVar.Title)
            if NewTitle.lower() == series.lower():
                NewTitle = ""
            f.Title = NewTitle
            debuglog(Trans(29), f.Title)

        if C("TBTags") == "DEL":
            f.Tags = ""
        elif C("TBTags") != "":
            f.Tags = C("TBTags")

        if C("CBWriter"):
            f.Writer = getGenericBookArtists([ALBUM_SCENAR_MULTI_AUTHOR, ALBUM_STORYBOARD_MULTI_AUTHOR], info, Trans(30))
        if C("CBPenciller"):
            f.Penciller = getGenericBookArtists([ALBUM_DESSIN_MULTI_AUTHOR], info, Trans(31))
        if C("CBColorist"):
            f.Colorist = getGenericBookArtists([ALBUM_COLOR_MULTI_AUTHOR], info, Trans(33))
            if "<N&B>" in f.Colorist:
                f.BlackAndWhite = "Yes"
            elif f.Colorist:
                f.BlackAndWhite = "No"
        if C("CBCouverture"):
            f.CoverArtist = getGenericBookArtists([ALBUM_COUVERT_MULTI_AUTHOR], info, Trans(121))
        if C("CBLetterer"):
            f.Letterer = getGenericBookArtists([ALBUM_LETTRAGE_MULTI_AUTHOR], info, Trans(38))
        if C("CBInker"):
            f.Inker = getGenericBookArtists([ALBUM_INKER_MULTI_AUTHOR], info, Trans(73))

        if C("CBPrinted"):
            f.Month, f.Year = _mois_annee(ALBUM_DEPOT.search(info, 0))
            if f.Month > 0:
                debuglog(Trans(34), str(f.Month) + "/" + str(f.Year))

        if C("CBEditor"):
            nameRegex = ALBUM_EDITEUR.search(info, 0)
            f.Publisher = parseName(strip_tags(nameRegex.group(1))).strip() if nameRegex else ""
            debuglog(Trans(35), f.Publisher)

        if C("CBISBN"):
            nameRegex = ALBUM_ISBN.search(info, 0)
            isbn = nameRegex.group(1) if nameRegex else ""
            f.ISBN = checkWebChar(strip_tags(isbn)).strip() if isbn != "</span>" else ""
            debuglog("ISBN: ", f.ISBN)

        # Évaluation de l'album : facultative.
        if C("CBRating"):
            nameRegex = ALBUM_EVAL.search(albumHTML, 0)
            if nameRegex:
                evaluation = _reel(nameRegex.group(1), -1.0)
                if 0.0 <= evaluation <= 5.0:
                    f.CommunityRating = evaluation
                    debuglog(Trans(39) + str(evaluation))

        # Achevé d'imprimer : facultatif, utilisé faute de dépôt légal.
        if C("CBPrinted"):
            nameRegex = ALBUM_ACHEVE.search(info, 0)
            if nameRegex and f.Month < 1:
                mois, annee = _mois_annee(nameRegex)
                if mois > 0:
                    f.Month, f.Year = mois, annee
                    debuglog(Trans(40), str(f.Month) + "/" + str(f.Year))

        # Collection : dans l'édition, sinon dans l'encart de l'album.
        if C("CBImprint"):
            nameRegex = ALBUM_COLLECTION.search(info_album, 0)
            nameRegex2 = ALBUM_COLLECTION.search(info, 0)
            if nameRegex or nameRegex2:
                f.Imprint = checkWebChar((nameRegex2 or nameRegex).group(1))
            else:
                f.Imprint = ""
            debuglog(Trans(41), f.Imprint)

        if C("CBFormat"):
            nameRegex = ALBUM_TAILLE.search(info, 0)
            f.Format = checkWebChar(strip_tags(nameRegex.group(1))).strip() if nameRegex else ""
            debuglog(Trans(42), f.Format)

        if C("CBSynopsys"):
            summary, resume = "", ""
            # Le résumé de la page est tronqué : on lit celui de « ajax/resume/album/<id> ».
            if AlbumBDThequeNum:
                resume = _read_url("ajax/resume/album/" + AlbumBDThequeNum, False)
            if resume:
                resume = strip_tags(checkWebChar(resume)).strip()
                PrintSerieResume = True if C("SerieResumeEverywhere") else f.Number == "1"
                titre = if_else(f.Title, ">" + f.Title + "< \n", "")
                if S.Serie_Resume and PrintSerieResume and remove_accents(S.Serie_Resume) != remove_accents(resume):
                    summary = S.Serie_Resume + if_else(resume, "\n\n" + titre + resume, "")
                elif resume:
                    summary = titre + resume
                    debuglog(Trans(100))
            else:
                debuglog(Trans(101))
            nameRegex = ALBUM_INFOEDITION.search(info, 0)
            if nameRegex and nameRegex.group(1) != " &nbsp;":
                infoedition = strip_tags(checkWebChar(nameRegex.group(1))).strip()
                if infoedition:
                    summary = if_else(summary != "", summary + "\n\n" + Trans(118) + infoedition, Trans(118) + infoedition)
                debuglog(Trans(118) + Trans(119))
            if summary:
                f.Summary = summary

        if C("CBSeries"):
            formatted = titlize(seriesFormat, False) if seriesFormat else titlize(series, True)
            f.Series = formatted if C("FORMATARTICLES") else titlize(series, False)
            debuglog(Trans(9), f.Series)

        if C("CBCover") and not f.FilePath and pickedVar.Couv:
            # ComicRack posait la couverture des fiches sans fichier ; ComicRust ne le permet
            # pas aux scripts : elle est seulement montrée dans la fenêtre d'avancement.
            debuglog(Trans(105), pickedVar.Couv)

        # Lien direct : pas de page de série lue, la langue n'est pas connue.
        if C("CBLanguage") and lDirect and not f.LanguageISO:
            f.LanguageISO = "fr"

        if not f.FilePath:
            nameRegex = ALBUM_PLANCHES.search(info, 0)
            if nameRegex:
                debuglog(Trans(122), nameRegex.group(1).strip())

        if C("CBNotes"):
            write_book_notes(f)
    except Exception:
        debuglogOnError()
        log_BD("   " + pageUrl + " " + Trans(43), "", 1)
        return False
    return True


# ---------------------------------------------------------------------------------------
# Traitement des livres
# ---------------------------------------------------------------------------------------

def _preparer(book, separateurs):
    """dlgName, dlgNumber, dlgAltNumber… du livre (début de WorkerThread / QuickScrape)."""
    S.Shadow1 = S.Shadow2 = False
    S.Serie_Resume = ""
    S.couverture = None
    prop_serie, prop_num = None, None
    if book.number:
        S.dlgNumber = str(book.number)
    else:
        prop_serie, prop_num = _proposition(book)
        S.dlgNumber = prop_num
        S.Shadow2 = True
    if book.series:
        S.dlgName = titlize(book.series)
    else:
        if prop_serie is None:
            prop_serie, prop_num = _proposition(book)
        S.dlgName = prop_serie
        S.Shadow1 = True
    S.dlgAltNumber = str(book.alternate_number) if book.alternate_number else ""
    S.dlgNameClean = cleanARTICLES(S.dlgName)
    S.dlgName = formatARTICLES(S.dlgName)
    findCara = S.dlgName.find(C("SUBPATT")) if C("SUBPATT") else -1
    if findCara > 0:
        S.dlgName = S.dlgName[:findCara]

    albumNum = S.dlgNumber
    mPos = re.search(r"([" + separateurs + "])", S.dlgNumber)
    if not isnumeric(S.dlgNumber):
        S.AlbumNumNum = False
    elif not re.search(r"[" + separateurs + "]", S.dlgNumber):
        S.dlgNumber = str(int(float(S.dlgNumber)))
        albumNum = S.dlgNumber
        S.AlbumNumNum = True
    elif mPos:
        nPos = mPos.start(1)
        albumNum = S.dlgNumber[:nPos]
        S.dlgAltNumber = S.dlgNumber[nPos:]
        S.dlgNumber = albumNum
        S.AlbumNumNum = True
    return albumNum


def _libelle_livre(f):
    return ("[" + S.dlgName + "] " + S.dlgNumber + if_else(S.dlgAltNumber == "", "", " AltNo.[" + S.dlgAltNumber + "]")
            + " - " + titlize(f.Title))


def _initialiser():
    S.__init__()
    if not LoadSetting():
        return False
    S.web = Web(S.base, C("use_cloudscraper"))
    S.cache = CacheDisque(os.path.join(S.journaux.dossier, "cache"))
    S.cache.purger()
    return True


def _verifier_journaux():
    j = S.journaux
    if j.taille(j.rapport) > int(C("RENLOGMAX")):
        if comicrust.confirm(Trans(4) + " : " + Trans(3), True):
            j.vider(j.rapport)
    if j.taille(j.debug) > int(C("DBGLOGMAX")):
        if comicrust.confirm(Trans(6) + " : " + Trans(5), True):
            j.vider(j.debug)


def _avancement(i, n, f):
    texte = "[" + str(i + 1) + "/" + str(n) + "] : " + S.dlgName + " - " + S.dlgNumber + \
        if_else(S.dlgAltNumber == "", "", " AltNo.[" + S.dlgAltNumber + "]") + " - " + titlize(f.Title)
    S.fraction, S.detail = i / max(n, 1), texte
    comicrust.progress(S.fraction, Trans(62), detail=texte)


def _montrer_couverture():
    if S.couverture and C("CBCover"):
        img = image(S.couverture)
        if img:
            try:
                comicrust.progress(None, Trans(62), image=img)
            except ValueError:
                pass


def _bilan(nOrigBooks):
    """Fin du traitement (fenêtres de l'original)."""
    texte = Trans(17) + str(S.nRenamed) + ", " + Trans(18) + str(S.nIgnored) + " (" + Trans(108) + str(nOrigBooks) + ")"
    if S.bError and C("SHOWDBGLOG"):
        if comicrust.confirm(Trans(20) + "\n" + texte + "\n\n" + Trans(19), True):
            _montrer_texte(Trans(6), S.journaux.debug)
    elif C("SHOWRENLOG"):
        if comicrust.confirm(Trans(22) + "\n" + texte + "\n\n" + Trans(21), True):
            _montrer_texte(Trans(4), None)
    else:
        comicrust.message(Trans(22) + "\n" + texte)


def _montrer_texte(titre, chemin):
    """Affiche le rapport de ce traitement ou la fin du journal de débogage."""
    if chemin:
        try:
            with open(chemin, encoding="utf-8", errors="replace") as fic:
                texte = fic.read()[-60000:]
        except OSError:
            texte = ""
    else:
        texte = "\n".join(S.journaux.lignes_rapport)
    comicrust.form(titre, [ui.textarea("texte", "", texte, rows=20)], buttons=("OK",), cancel=None, width=760)


#@Name Bedetheque Scraper 2
#@Hook Books, Editor
#@Key Bedetheque2
#@Image bd2/bd2.png
#@Description Cherche sur www.bedetheque.com les informations des BD sélectionnées (série, numéro,
#@Description titre, auteurs, éditeur, collection, dépôt légal, résumé…). Portage de Bedetheque Scraper 2
#@Description (Franck, Mizio66, kiwi13 & maforget) ; réglages : « Configurer BD2 ».
def BD_start(books):
    if not _initialiser():
        return None
    if not books:
        comicrust.message(Trans(2) + "\n" + Trans(1))
        return None
    _verifier_journaux()
    S.nRenamed = S.nIgnored = 0
    if C("CBRescrape"):
        if not comicrust.confirm(Trans(138) + "\n" + Trans(139), True):
            return None
    return WorkerThread(books)


def WorkerThread(books):
    S.bError = False
    TimeStart = time.monotonic()
    nOrigBooks = len(books)
    log_BD(Trans(7) + str(nOrigBooks) + Trans(8), "\n============ " + _maintenant() + " ===========", 0)
    debuglog("\n" + "=" * 25 + "- Begin! -" + "=" * 25 + "\n")
    nTIMEDOUT = 0
    nBooks = len(books)
    S.PickSeries = False
    S.serie_rech_prev = None
    try:
        maximum = int(C("TIMEOUT"))
    except ValueError:
        maximum = 1000
    try:
        pause = max(0, int(float(C("TIMEOUTS"))))
    except ValueError:
        pause = 7
    f = None
    try:
        for i, book in enumerate(books):
            TimeBookStart = time.monotonic()
            debuglog("v" * 60)
            if stop() or nTIMEDOUT == maximum:
                if stop():
                    debuglog("Cancelled from WorkerThread Start")
                break
            nTIMEDOUT += 1
            f = Fiche(book)
            albumNum = _preparer(book, r".,\\/")
            _avancement(i, nBooks, f)

            RetAlb = False
            if C("CBRescrape"):
                if f.Web:
                    RetAlb = _scrape_lien(f, f.Web)
            else:
                alt = if_else(S.dlgAltNumber == "", "", "\tAltNo. = [" + S.dlgAltNumber + "]")
                debuglog(Trans(9) + S.dlgName + "\tNo = [" + albumNum + "]" + alt)
                debuglog(Trans(10), S.dlgName)
                serieUrl = GetFullURL(SetSerieId(f, S.dlgName, albumNum, nBooks))
                if stop():
                    debuglog("Cancelled from WorkerThread after SetSerieId return")
                    f.appliquer(C("ChampsVidesSeulement"))
                    break
                if serieUrl:
                    if "/revue-" not in serieUrl:
                        serieUrl = serieUrl.lower().replace(".html", "__10000.html")
                    if S.AlbumNumNum:
                        debuglog(Trans(11), albumNum + "]", if_else(S.dlgAltNumber == "", "", " - AltNo.: " +
                                                                     S.dlgAltNumber))
                    else:
                        debuglog(Trans(12) + albumNum + "]", if_else(S.dlgAltNumber == "", "", " - AltNo. [" +
                                                                      S.dlgAltNumber + "]"))
                    RetAlb = SetAlbumInformation(f, serieUrl, S.dlgName, albumNum)
                    # SkipAlbum : « Annuler » (ou choix interdit) dans la fenêtre des albums.
                    if not S.SkipAlbum and not RetAlb and "/revue-" not in serieUrl and not stop():
                        # Pas de liste d'albums dans la page (ex. « Croisade (Seconde époque: Nomade) »).
                        RetAlb = parseAlbumInfo(f, serieUrl, albumNum)

            n = f.appliquer(C("ChampsVidesSeulement"))
            _montrer_couverture()
            if RetAlb:
                S.nRenamed += 1
                log_BD(_libelle_livre(f), Trans(13) + " (%d)" % n, 1)
            else:
                S.nIgnored += 1
                log_BD(_libelle_livre(f), Trans(14) + "\n", 1)
            debuglog(Trans(125), str(datetime.timedelta(seconds=int(time.monotonic() - TimeBookStart))) + "\n")
            debuglog("^" * 60)

            # Pause entre deux albums (TIMEOUTS secondes).
            if pause and nOrigBooks > S.nIgnored + S.nRenamed and i + 1 < nBooks:
                comicrust.progress((i + 1) / max(nBooks, 1), Trans(140).replace("%%", str(pause)))
                for _ in range(20 * pause):
                    time.sleep(0.05)
                    if stop():
                        break
            if stop():
                debuglog("Cancelled from WorkerThread End")
                break
    except Exception:
        cError = debuglogOnError()
        log_BD("   [" + str(S.dlgName) + "] " + str(S.dlgNumber), cError, 1)
    finally:
        comicrust.progress(1.0, Trans(16))
        log_BD("\n" + Trans(17) + str(S.nRenamed), "", 0)
        log_BD(Trans(18) + str(S.nIgnored), "", 0)
        log_BD("============= " + _maintenant() + " =============", "\n\n", 0)
        debuglog(Trans(124), str(datetime.timedelta(seconds=int(time.monotonic() - TimeStart))))
        debuglog("=" * 25 + "- End! -" + "=" * 25 + "\n")
        flush_debuglog()
    if not stop():
        _bilan(nOrigBooks)
    return {"traites": S.nRenamed, "ignores": S.nIgnored}


def _scrape_lien(f, lien):
    """Lien direct (QuickScrape, rescrape) : série, revue ou album."""
    serieUrl = lien
    RetVal = serieUrl
    if "/serie-" in serieUrl or "/revue-" in serieUrl:
        if "__10000.html" not in serieUrl and "/revue-" not in serieUrl:
            serieUrl = serieUrl.lower().replace(".html", "__10000.html")
        RetVal = parseSerieInfo(f, serieUrl, True)
    if RetVal and "/revue-" not in serieUrl:
        RetVal = parseAlbumInfo(f, RetVal, S.dlgNumber, True)
    log_BD("[" + serieUrl + "]", Trans(13) if RetVal else Trans(14) + "\n", 1)
    return bool(RetVal)


# Liens reconnus par QuickScrape (aperçu sous le champ) : mêmes distinctions que
# QuickScrapeBD2 de l'original (« /serie- », « /revue- », sinon page d'album), complets ou
# relatifs (« serie-59-BD-Blacksad.html »).
LIEN_SERIE = r"(?i)(^|/)serie-\d+[^/]*\.html(#.*)?$"
LIEN_REVUE = r"(?i)(^|/)revue-[^/]+\.html(#.*)?$"
LIEN_ALBUM = r"(?i)(^|/)BD-[^/]*-\d+\.html(#.*)?$"


def _lien_recherche(f):
    """Recherche de la série du livre sur le site (bouton « Chercher » de QuickScrape)."""
    nom = S.dlgNameClean or S.dlgName or (f.Series if f is not None else "") or ""
    return S.base + "/search/tout?RechTexte=" + urllib.parse.quote(remove_accents(nom.strip().lower())) + \
        "&RechWhere=0"


def DirectScrape(f=None, i=0, n=1):
    """Fenêtre « Scrape directement un lien » (DirectScrape de l'original : un champ, OK,
    Annuler) : lien collé, '' (OK sans lien) ou None (Annuler). Le portage ajoute le livre
    concerné, le bouton « Coller », le type de lien reconnu, un bouton qui ouvre la
    recherche de la série sur le site, deux options pour ce livre et une aide courte.
    Entrée = OK."""
    livre = S.dlgName or "?"
    if S.dlgNumber:
        livre += _t(" – n° ", " – #") + S.dlgNumber
    if S.dlgAltNumber:
        livre += " (" + S.dlgAltNumber + ")"
    if f is not None and f.Title:
        livre += " – " + titlize(f.Title)
    fichier = os.path.basename(str(f.FilePath).replace("\\", "/")) if f is not None and f.FilePath else ""
    livre_ctl = [ui.label(livre, style="bold")]
    if fichier:
        livre_ctl.append(ui.label(fichier, style="weak"))
    controles = [
        ui.group(_t("Livre %d sur %d", "Book %d of %d") % (i + 1, n), livre_ctl),
        ui.group(Trans(102), [
            ui.text("lien", "", "", placeholder=URL_SITE + "/BD-…-12345.html", paste=True,
                    hints=[(LIEN_ALBUM, _t("Lien d'un album : ses infos seront lues", "Book link: its info will be read")),
                           (LIEN_SERIE, _t("Lien d'une série : l'album sera choisi dans la liste de la série",
                                           "Series link: the book will be picked from the series list")),
                           (LIEN_REVUE, _t("Lien d'une revue", "Magazine link"))],
                    hint_none=_t("Lien non reconnu (ni album, ni série, ni revue) : il sera lu comme une page "
                                 "d'album.", "Unrecognised link (not a book, series or magazine): it will be read "
                                             "as a book page.")),
            ui.button("chercher", _t("🔎 Chercher « %s » sur bedetheque.com", "🔎 Search “%s” on bedetheque.com")
                      % ((S.dlgNameClean or S.dlgName or "?").strip()),
                      help=_t("Ouvre la recherche dans le navigateur : copiez-y le lien de l'album, puis « Coller ».",
                              "Opens the search in the browser: copy the book link there, then “Paste”.")),
        ]),
        ui.group(_t("Options pour ce passage", "Options for this run"), [
            ui.checkbox("vides", _t("Ne remplir que les champs vides", "Only fill empty fields")),
            ui.checkbox("edition", _t("Choisir l'édition dans une liste quand l'album en a plusieurs",
                                      "Pick the edition from a list when the book has several")),
        ]),
        ui.label(_t("Liens complets ou sans « https://www.bedetheque.com/ » : album (…/BD-…-21.html), "
                    "série (…/serie-59-BD-Blacksad.html) ou revue (…/revue-Spirou.html). OK sans lien ou "
                    "Annuler : arrêter (les livres déjà traités restent dans l'aperçu).",
                    "Full links or without “https://www.bedetheque.com/”: book (…/BD-…-21.html), series "
                    "(…/serie-59-BD-Blacksad.html) or magazine (…/revue-Spirou.html). OK without a link or "
                    "Cancel: stop (books already done stay in the preview)."), style="weak"),
    ]
    valeurs = {"lien": "", "vides": bool(C("ChampsVidesSeulement")), "edition": bool(C("PopUpEditionForm"))}
    while True:
        r = comicrust.form(Trans(103), controles, buttons=(Trans(92), Trans(93)), values=valeurs, width=820)
        if r is None:
            return None
        bouton, valeurs = r[0], dict(r[1])
        # Options : pour la suite de ce passage seulement (réglages enregistrés inchangés).
        S.C["ChampsVidesSeulement"] = bool(valeurs.get("vides"))
        S.C["PopUpEditionForm"] = bool(valeurs.get("edition"))
        if bouton == "chercher":
            comicrust.open_url(_lien_recherche(f))
            continue
        return (valeurs.get("lien") or "").strip()


#@Name QuickScrape BD2
#@Hook Books
#@Key QuickScrapeBD2
#@Image bd2/bd2q.png
#@Description Bedetheque Scraper 2 avec un lien direct : pour chaque livre, collez l'adresse de l'album
#@Description (ou de la série, de la revue) sur www.bedetheque.com.
def QuickScrapeBD2(books):
    if not _initialiser():
        return None
    S.RenameSeries = False
    if not books:
        comicrust.message(Trans(2) + "\n" + Trans(1))
        return None
    S.nRenamed = S.nIgnored = 0
    n = len(books)
    log_BD(Trans(7) + str(n) + Trans(8), "\n============ " + _maintenant() + " ===========", 0)
    try:
        for i, book in enumerate(books):
            if stop():
                break
            f = Fiche(book)
            _preparer(book, r".,\\/-")
            S.fraction = i / max(n, 1)
            S.detail = S.dlgName + if_else(S.dlgNumber != "", " - " + S.dlgNumber, " ") + \
                if_else(S.dlgAltNumber == "", "", " AltNo.[" + S.dlgAltNumber + "]") + " - " + titlize(f.Title)
            comicrust.progress(S.fraction, Trans(62), detail=S.detail)
            lien = DirectScrape(f, i, n)
            if not lien:
                break
            debuglog(Trans(104), lien)
            ok = _scrape_lien(f, GetFullURL(lien))
            f.appliquer(C("ChampsVidesSeulement"))
            _montrer_couverture()
            if ok:
                S.nRenamed += 1
            else:
                S.nIgnored += 1
    except Exception:
        cError = debuglogOnError()
        log_BD("   [error]", cError, 1)
    finally:
        comicrust.progress(1.0, Trans(16))
        log_BD("\n" + Trans(17) + str(S.nRenamed), "", 0)
        log_BD(Trans(18) + str(S.nIgnored), "", 0)
        flush_debuglog()
    return {"traites": S.nRenamed, "ignores": S.nIgnored}


# ---------------------------------------------------------------------------------------
# Réglages (BDConfigForm)
# ---------------------------------------------------------------------------------------

def _valeurs_formulaire(cfg):
    v = {k: cfg[k] for k in DEFAUTS if k not in ("url_base", "dossier_journaux")}
    v["Donnees"] = [cle for cle, reglage, _ in DONNEES if cfg[reglage]]
    v["ChoixUtilisateur"] = "0" if cfg["CBStop"] == "0" else "1"
    v["FermetureAuto"] = cfg["CBStop"] == "2"
    v["ChoixAutoEdition"] = not cfg["PopUpEditionForm"]
    v["ResumePremierTome"] = not cfg["SerieResumeEverywhere"]
    for k in ("TIMEOUT", "TIMEOUTS", "TIMEPOPUP", "PadNumber"):
        v[k] = _entier(cfg[k], _entier(DEFAUTS[k]))
    v["RENLOGMAX"] = round(cfg["RENLOGMAX"] / (1024 * 1024), 2)
    v["DBGLOGMAX"] = round(cfg["DBGLOGMAX"] / (1024 * 1024), 2)
    return v


def _depuis_formulaire(v, cfg):
    """Réglages d'après les valeurs du formulaire (button_Click de l'original)."""
    out = dict(cfg)
    for k in ("SHOWRENLOG", "SHOWDBGLOG", "DBGONOFF", "ShortWebLink", "CBDefault", "CBRescrape",
              "use_cloudscraper", "COUNTOF", "COUNTFINIE", "TITLEIT", "FORMATARTICLES", "ONESHOTFORMAT",
              "AlwaysChooseSerie", "AcceptGenericArtists", "ChampsVidesSeulement"):
        out[k] = bool(v.get(k))
    out["LANGENFR"] = "EN" if v.get("LANGENFR") == "EN" else "FR"
    out["TBTags"] = str(v.get("TBTags") or "")
    out["ARTICLES"] = str(v.get("ARTICLES") or "")
    out["SUBPATT"] = str(v.get("SUBPATT") or "")
    coches = set(v.get("Donnees") or [])
    for cle, reglage, _ in DONNEES:
        out[reglage] = cle in coches
    out["CBFormat"] = "Format" in coches and not out["ONESHOTFORMAT"]
    if v.get("ChoixUtilisateur") == "1":
        out["CBStop"] = "2" if v.get("FermetureAuto") else "1"
    else:
        out["CBStop"] = "0"
    out["PopUpEditionForm"] = not v.get("ChoixAutoEdition")
    out["SerieResumeEverywhere"] = not v.get("ResumePremierTome")

    def nombre(cle):
        try:
            return int(float(v.get(cle)))
        except (TypeError, ValueError):
            return -1

    def positif(cle, defaut):
        n = nombre(cle)
        return str(n) if n > 0 else defaut

    out["TIMEOUT"] = positif("TIMEOUT", "1000")
    out["TIMEOUTS"] = positif("TIMEOUTS", "7")
    out["TIMEPOPUP"] = positif("TIMEPOPUP", "30")
    out["PadNumber"] = str(nombre("PadNumber")) if nombre("PadNumber") >= 0 else "0"
    for k in ("RENLOGMAX", "DBGLOGMAX"):
        try:
            mo = float(v.get(k))
        except (TypeError, ValueError):
            continue
        # Valeur affichée inchangée : la taille exacte est gardée (20000 octets restent
        # 20000, au lieu de 0,02 Mo × 1 048 576 = 20971 comme dans l'original).
        if round(cfg[k] / (1024 * 1024), 2) != round(mo, 2):
            out[k] = int(mo * 1024 * 1024)
    def entier(cle):
        try:
            return int(float(v.get(cle)))
        except (TypeError, ValueError):
            return cfg[cle]

    out["TailleVignettes"] = min(320, max(48, entier("TailleVignettes")))
    out["VignettesSeries"] = bool(v.get("VignettesSeries"))
    out["NbVignettesSeries"] = min(MAX_PAGES_SERIES, max(1, entier("NbVignettesSeries")))
    return out


def BDConfigForm(valeurs):
    """Formulaire à trois onglets de l'original (Préférences, Données, Logging), mêmes
    options, regroupées en cadres titrés ; options du portage à part (« ComicRust »)."""
    en = S.C["LANGENFR"] == "EN"
    importer = "Import settings from ComicRack…" if en else "Importer les réglages de ComicRack…"
    vides = ("Only fill empty fields (ComicRust)" if en
             else "Ne remplacer que les champs vides (ajout ComicRust)")
    preferences = [
        ui.group(_t("Langue et noms", "Language and names"), [
            ui.radio("LANGENFR", Trans(112), [("FR", "Français"), ("EN", "English")], horizontal=True,
                     help=_t("Langue des fenêtres du script et du libellé « Info édition » ajouté au résumé.",
                             "Language of the script windows and of the “Edition info” label added to the "
                             "summary.")),
            ui.checkbox("FORMATARTICLES", Trans(144)),
            ui.text("ARTICLES", Trans(109), enabled_if="FORMATARTICLES"),
            ui.checkbox("TITLEIT", Trans(127)),
        ]),
        ui.group(Trans(141), [
            ui.radio("ChoixUtilisateur", "", [("0", Trans(45)), ("1", Trans(46))]),
            ui.group("", [
                ui.checkbox("FermetureAuto", Trans(44)),
                ui.number("TIMEPOPUP", _t("Délai (s)", "Delay (s)"), 30, min=1, max=999, enabled_if="FermetureAuto"),
            ], enabled_if="ChoixUtilisateur=1"),
            ui.checkbox("AlwaysChooseSerie", Trans(151)),
            ui.checkbox("ChoixAutoEdition", Trans(147)),
        ]),
        ui.group(_t("Traitement", "Processing"), [
            ui.number("TIMEOUTS", Trans(129), 7, min=1, max=999),
            ui.number("TIMEOUT", Trans(128), 1000, min=1, max=1000000),
            ui.checkbox("CBRescrape", Trans(137)),
            ui.checkbox("use_cloudscraper", Trans(152),
                        help=_t("Module « cloudscraper » (et ses dépendances) à copier dans scripts\\lib.",
                                "Copy the “cloudscraper” module (and its dependencies) into scripts\\lib.")),
        ]),
        ui.group(_t("Fenêtres de choix (ComicRust)", "Choice windows (ComicRust)"), [
            ui.number("TailleVignettes", _t("Hauteur des couvertures (pixels)", "Cover height (pixels)"), 150,
                      min=48, max=320, step=4,
                      help=_t("Aussi réglable dans la fenêtre de choix (curseur « Taille »).",
                              "Can also be changed in the choice window (“Size” slider).")),
            ui.checkbox("VignettesSeries", _t("Couvertures des séries : lire la page des premières séries proposées",
                                              "Series covers: read the page of the first proposed series"),
                        help=_t("Les résultats de recherche du site n'ont pas de couverture : la fenêtre s'ouvre "
                                "tout de suite et la page des premières séries est lue pendant ce temps (une page à "
                                "la fois, une seconde au moins entre deux pages ; les autres séries au survol). "
                                "Couvertures gardées dans data\\bedetheque\\cache : une série déjà vue n'est pas "
                                "relue.",
                                "The site's search results have no cover: the window opens at once and the page of "
                                "the first series is read meanwhile (one page at a time, at least one second "
                                "between pages; other series on hover). Covers are kept in data\\bedetheque\\cache: "
                                "a series already seen is not read again.")),
            ui.number("NbVignettesSeries", _t("Nombre de séries", "Number of series"), 6, min=1,
                      max=MAX_PAGES_SERIES, enabled_if="VignettesSeries"),
        ]),
    ]
    donnees = [
        ui.columns(
            [ui.multichoice("Donnees", _t("Champs à récupérer", "Fields to scrape"),
                            [(cle, Trans(n)) for cle, _, n in DONNEES], height=330)],
            [ui.label(_t("Options", "Options"), style="bold"),
             ui.checkbox("ONESHOTFORMAT", Trans(150)),
             ui.checkbox("CBDefault", Trans(136)),
             ui.checkbox("COUNTOF", Trans(110)),
             ui.checkbox("COUNTFINIE", Trans(126)),
             ui.checkbox("ResumePremierTome", Trans(149)),
             ui.checkbox("ShortWebLink", Trans(48)),
             ui.checkbox("AcceptGenericArtists", Trans(74)),
             ui.separator(),
             ui.checkbox("ChampsVidesSeulement", vides)],
        ),
        ui.group("", [
            ui.text("TBTags", Trans(91), help="« DEL » : vide les étiquettes" if not en else "“DEL”: clear tags",
                    placeholder=_t("vide = étiquettes inchangées ; DEL = les effacer",
                                   "empty = tags unchanged; DEL = clear them")),
            ui.text("SUBPATT", Trans(37)),
            ui.number("PadNumber", Trans(148), 0, min=0, max=9),
        ]),
    ]
    logging_ = [
        ui.group("", [
            ui.checkbox("SHOWRENLOG", Trans(66)),
            ui.checkbox("DBGONOFF", Trans(68)),
            ui.checkbox("SHOWDBGLOG", Trans(67)),
            ui.number("RENLOGMAX", Trans(70), 0.01, min=0.01, max=100, step=0.01, decimals=2),
            ui.number("DBGLOGMAX", Trans(69), 0.01, min=0.01, max=100, step=0.01, decimals=2),
        ]),
        ui.label("%s : %s" % ("Folder" if en else "Dossier", S.journaux.dossier), style="weak"),
    ]
    return comicrust.form("Bedetheque Scraper 2", [
        ui.tabs([(Trans(95), preferences), (Trans(96), donnees), (Trans(47), logging_)]),
        ui.button("importer", importer,
                  help=_t("App.Config du dossier du script dans ComicRack", "App.Config from the script folder in "
                                                                             "ComicRack")),
        ui.label("Version " + VERSION + " (c) 2021 kiwi13 & maforget — portage Python 3 pour ComicRust",
                 style="small"),
    ], buttons=(Trans(92), Trans(93)), values=valeurs, width=840, label_width=360)


def _demander_fichier():
    en = S.C["LANGENFR"] == "EN"
    r = comicrust.form("Bedetheque Scraper 2", [
        ui.label("App.Config of Bedetheque Scraper 2 for ComicRack (script folder, e.g. "
                 "%AppData%\\cYo\\ComicRack\\Scripts\\…)" if en else
                 "Fichier App.Config de Bedetheque Scraper 2 pour ComicRack (dossier du script, par ex. "
                 "%AppData%\\cYo\\ComicRack\\Scripts\\…)"),
        ui.file("fichier", "App.Config", "", filters=[("App.Config", ["config", "xml"])], required=True),
    ], buttons=(Trans(92), Trans(93)), width=640)
    if r is None:
        return None
    return (r[1].get("fichier") or "").strip() or None


def _configurer():
    S.__init__()
    if not LoadSetting():
        return None
    valeurs = _valeurs_formulaire(S.C)
    while True:
        r = BDConfigForm(valeurs)
        if r is None:
            return None
        bouton, v = r
        if bouton == "importer":
            # Reprend la saisie en cours, puis le contenu du fichier choisi.
            S.C = _depuis_formulaire(v, S.C)
            chemin = _demander_fichier()
            if chemin:
                try:
                    lus = lire_app_config(chemin)
                except (OSError, ET.ParseError, ValueError) as e:
                    comicrust.message("%s\n%s : %s" % (Trans(2), chemin, e))
                else:
                    S.C.update(lus)
                    S.mots = Translate()
                    comicrust.log("%d réglage(s) importé(s) de %s" % (len(lus), chemin))
            valeurs = _valeurs_formulaire(S.C)
            continue
        S.C = _depuis_formulaire(v, S.C)
        SaveSetting()
        return {k: S.C[k] for k in DEFAUTS}


#@Name Configurer BD2
#@Hook ConfigScript
#@Key Bedetheque2Config
#@Image bd2/bd2.png
#@Description Réglages de Bedetheque Scraper 2 (Préférences, Données, Logging), import des réglages de
#@Description ComicRack (App.Config).
def ConfigureBD2Quick():
    return _configurer()


#@Name Configurer BD2
#@Hook Library
#@Key ConfigureBD2
#@Image bd2/bd2.png
#@Description Réglages de Bedetheque Scraper 2 depuis le menu Scripts (comme le menu Bibliothèque de
#@Description ComicRack) ; ne modifie aucun livre.
def ConfigureBD2(books):
    return _configurer()
