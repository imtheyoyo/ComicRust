# -*- coding: utf-8 -*-
"""Informations depuis Comic Vine (script d'exemple fourni avec ComicRust).

Comic Vine (https://comicvine.gamespot.com) est une base de données collaborative des
comics, surtout américains (peu de BD franco-belges). Son API officielle demande une
**clé personnelle gratuite** : créer un compte puis la copier depuis
https://comicvine.gamespot.com/api/ ; la saisir avec le bouton « Réglages » du script
(elle est enregistrée comme valeur **secrète** : chiffrée sous Windows).

Pour chaque livre sélectionné :

1. recherche de la **série** (« volume » chez Comic Vine) par son nom et son année ;
   si plusieurs séries conviennent, l'utilisateur choisit (choix retenu pour les autres
   livres de la même série) ;
2. recherche du **numéro** dans cette série ;
3. lecture de la fiche du numéro et remplissage : titre, résumé, scénaristes,
   dessinateurs, encreurs, coloristes, lettreurs, couverture, éditeurs (personnes),
   personnages, équipes, lieux, arcs, date de couverture → Année/Mois, éditeur, nombre
   de numéros, Web = page Comic Vine du numéro.

Par défaut seuls les champs **vides** sont remplis (réglage « remplacer » pour tout
réécrire). Rien n'est écrit sans l'aperçu de ComicRust ; en option, la fiche est ensuite
écrite dans le fichier (ComicInfo.xml).

API utilisée (format JSON, https://comicvine.gamespot.com/api/documentation) :

- ``GET /api/volumes/?api_key=…&format=json&filter=name:<série>&field_list=…&limit=100``
  (``filter`` : ``champ:valeur``, plusieurs séparés par des virgules) ; à défaut
  ``GET /api/search/?query=<série>&resources=volume`` (recherche plus souple) ;
- ``GET /api/issues/?filter=volume:<id>,issue_number:<numéro>&field_list=…`` ;
- ``GET /api/issue/4000-<id>/?field_list=…`` : fiche complète (``person_credits`` avec
  leur ``role``, ``character_credits``, ``description`` en HTML…).

Réponse : ``{"status_code": 1, "error": "OK", "results": …}`` ; ``status_code`` 100 =
clé invalide, 101 = objet introuvable, 107 = limite de requêtes dépassée.

Limites de l'API : environ 200 requêtes par heure et par type de ressource, et Comic Vine
bloque les rafales : le script attend ``delai`` secondes (1,5 par défaut) entre deux
requêtes et s'identifie par un ``User-Agent`` (le « Python-urllib » par défaut est refusé).
Comptez 2 à 3 requêtes par livre.

Réglages avancés (``comicrust.settings()``) : ``url_base`` (autre adresse de l'API, pour
les essais ; aussi variable d'environnement ``COMICRUST_COMICVINE_URL``), ``delai``.
"""

import html
import html.parser
import json
import os
import re
import time
import urllib.error
import urllib.parse

import comicrust

URL_BASE = "https://comicvine.gamespot.com/api/"
USER_AGENT = "ComicRust/0.10 (script comic_vine.py ; https://github.com/imtheyoyo/ComicRust)"
DEFAUTS = {"api_key": "", "remplacer": False, "ecrire_fichier": False, "delai": 1.5, "url_base": ""}
IGNORER = "(ignorer ce livre)"

CHAMPS_VOLUME = "id,name,start_year,publisher,count_of_issues,site_detail_url"
CHAMPS_NUMEROS = "id,name,issue_number,cover_date,store_date,volume"
CHAMPS_NUMERO = ("id,name,issue_number,cover_date,store_date,description,deck,person_credits,"
                 "character_credits,team_credits,location_credits,story_arc_credits,"
                 "site_detail_url,volume")

# Rôles Comic Vine (« role » des person_credits, plusieurs séparés par des virgules)
# → champs ComicRack.
ROLES = {
    "writer": ["Writer"], "plotter": ["Writer"], "scripter": ["Writer"],
    "penciler": ["Penciller"], "penciller": ["Penciller"], "breakdowns": ["Penciller"],
    "artist": ["Penciller", "Inker"],
    "inker": ["Inker"], "finishes": ["Inker"],
    "colorist": ["Colorist"], "colorer": ["Colorist"],
    "letterer": ["Letterer"],
    "cover": ["CoverArtist"],
    "editor": ["Editor"],
    "translator": ["Translator"],
}


class Arret(Exception):
    """Erreur qui arrête tout le script (clé refusée, limite atteinte…)."""


def _reglage(nom):
    v = comicrust.settings().get(nom)
    return DEFAUTS[nom] if v is None else v


#@Name Comic Vine : réglages
#@Hook ConfigScript
#@Key comic-vine-reglages
#@Description Clé d'API Comic Vine (secrète) et options de remplissage.
def reglages():
    s = comicrust.settings()
    actuelle = (_reglage("api_key") or "").strip()
    # Saisie masquée : la clé n'est jamais affichée ; réponse vide = garder l'actuelle.
    question = "Clé d'API Comic Vine (https://comicvine.gamespot.com/api/) :"
    if actuelle:
        question += "\n(une clé est déjà enregistrée : laisser vide pour la garder)"
    cle = comicrust.ask(question, actuelle, secret=True)
    s["api_key"] = (cle or "").strip() or actuelle
    s.mark_secret("api_key")
    s["remplacer"] = comicrust.confirm(
        "Remplacer aussi les champs déjà remplis ? (sinon : compléter les champs vides)",
        bool(_reglage("remplacer")))
    s["ecrire_fichier"] = comicrust.confirm(
        "Écrire ensuite la fiche dans le fichier (ComicInfo.xml) ?", bool(_reglage("ecrire_fichier")))
    s.save()


# ---------------------------------------------------------------------------------------
# Accès à l'API
# ---------------------------------------------------------------------------------------

class ComicVine:
    """Client minimal : délai entre requêtes, User-Agent, erreurs de l'API."""

    def __init__(self, cle, url_base, delai):
        self.cle = cle
        self.url_base = url_base.rstrip("/") + "/"
        self.delai = max(0.0, float(delai))
        self.derniere = 0.0
        self.requetes = 0

    def _attendre(self):
        reste = self.derniere + self.delai - time.monotonic()
        while reste > 0:
            comicrust.check_cancelled()
            time.sleep(min(reste, 0.2))
            reste = self.derniere + self.delai - time.monotonic()

    def masquer(self, texte):
        """``texte`` sans la clé d'API (en clair ou encodée dans une URL)."""
        texte = str(texte)
        if self.cle:
            for forme in {self.cle, urllib.parse.quote_plus(self.cle), urllib.parse.quote(self.cle, safe="")}:
                texte = texte.replace(forme, "…")
        return texte

    def get(self, chemin, **params):
        """Requête ``GET`` : renvoie ``results`` (liste ou dictionnaire). Toute erreur devient
        :class:`Arret` avec un message **sans la clé** (``from None`` : l'exception
        d'origine, qui peut contenir l'URL complète, n'apparaît pas dans la trace)."""
        self._attendre()
        params.update({"api_key": self.cle, "format": "json"})
        url = self.url_base + chemin + "?" + urllib.parse.urlencode(params)
        # Jamais la clé dans le journal.
        visible = self.masquer(url)
        erreur = None
        try:
            donnees = comicrust.http_get(url, headers={"User-Agent": USER_AGENT,
                                                       "Accept": "application/json"})
        except urllib.error.HTTPError as e:
            if e.code in (401, 403, 420, 429):
                erreur = ("Comic Vine refuse les requêtes (HTTP %d) : clé invalide ou limite "
                          "atteinte, réessayez plus tard." % e.code)
            else:
                erreur = "Comic Vine : erreur HTTP %d sur %s" % (e.code, visible)
        except OSError as e:
            erreur = "Comic Vine injoignable : %s" % self.masquer(e)
        except Exception as e:  # URL refusée (ValueError…), réponse incomplète…
            erreur = "Comic Vine : %s (%s)" % (self.masquer(e), type(e).__name__)
        finally:
            self.derniere = time.monotonic()
            self.requetes += 1
        if erreur is not None:
            raise Arret(erreur) from None
        try:
            r = json.loads(donnees.decode("utf-8"))
        except ValueError:
            raise Arret("Réponse illisible de Comic Vine (%s)" % visible) from None
        code = r.get("status_code")
        if code == 100:
            raise Arret("Clé d'API refusée par Comic Vine : vérifiez-la (Réglages du script).")
        if code == 107:
            raise Arret("Limite de requêtes Comic Vine atteinte : réessayez dans une heure.")
        if code == 101:
            return None
        if code != 1:
            raise Arret("Comic Vine : %s (code %s)" % (r.get("error"), code))
        return r.get("results")


# ---------------------------------------------------------------------------------------
# Outils
# ---------------------------------------------------------------------------------------

def _simple(texte):
    """Forme de comparaison : minuscules, sans accents ni ponctuation, « the » retiré."""
    import unicodedata
    t = unicodedata.normalize("NFKD", str(texte or "")).encode("ascii", "ignore").decode()
    t = re.sub(r"[^a-z0-9]+", " ", t.lower()).strip()
    return re.sub(r"^the ", "", t)


def _vide(v):
    if v is None:
        return True
    if isinstance(v, str):
        return not v.strip()
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return v < 0
    return False


def _annee(v):
    try:
        a = int(v)
        return a if a > 0 else None
    except (TypeError, ValueError):
        return None


def _numero(n):
    """Numéro pour Comic Vine : « 001 » → « 1 », « #12 » → « 12 »."""
    s = str(n or "").strip().lstrip("#").strip()
    return re.sub(r"^0+(?=\d)", "", s).replace(",", ".")


def _serie_numero_depuis_nom(chemin):
    nom = os.path.splitext(os.path.basename(str(chemin or "").replace("\\", "/")))[0]
    annee = None
    m = re.search(r"[(\[]((?:19|20)\d{2})[)\]]", nom)
    if m:
        annee = int(m.group(1))
    reste = " ".join(re.sub(r"\s*[(\[][^)\]]*[)\]]", " ", nom).replace("_", " ").split())
    m = re.match(r"^(.+?)(?:\s*[-–]\s*|\s+)(?:(?:tome|t|n°|no\.?)\s*|#\s*)?(\d+(?:[.,]\d+)?)"
                 r"(?:\s*[-–:]\s*.+?)?\s*$", reste, re.IGNORECASE)
    if m:
        return m.group(1).strip(" -–"), m.group(2), annee
    return None, None, annee


class _Texte(html.parser.HTMLParser):
    """HTML de Comic Vine → texte : paragraphes gardés, figures et tableaux retirés."""

    BLOCS = {"p", "br", "div", "li", "h1", "h2", "h3", "h4", "h5", "h6", "tr", "blockquote"}
    IGNORES = {"figure", "table", "script", "style"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.morceaux = []
        self.ignore = 0

    def handle_starttag(self, tag, attrs):
        if tag in self.IGNORES:
            self.ignore += 1
        elif tag in self.BLOCS and not self.ignore:
            self.morceaux.append("\n")

    def handle_endtag(self, tag):
        if tag in self.IGNORES:
            self.ignore = max(0, self.ignore - 1)
        elif tag in self.BLOCS and not self.ignore:
            self.morceaux.append("\n")

    def handle_data(self, data):
        if not self.ignore:
            self.morceaux.append(data)


def html_en_texte(code):
    if not code:
        return None
    p = _Texte()
    try:
        p.feed(code)
        p.close()
        texte = "".join(p.morceaux)
    except Exception:  # HTML très abîmé : balises retirées grossièrement
        texte = html.unescape(re.sub(r"<[^>]+>", " ", code))
    lignes = [" ".join(l.split()) for l in texte.split("\n")]
    texte = re.sub(r"\n{3,}", "\n\n", "\n".join(lignes)).strip()
    return texte or None


def _noms(liste):
    vus = []
    for e in liste or []:
        n = (e or {}).get("name")
        if n and n not in vus:
            vus.append(n)
    return ", ".join(vus) or None


def _libelle_volume(v):
    editeur = ((v.get("publisher") or {}).get("name")) or "?"
    return "%s (%s) — %s — %s numéro(s) [%s]" % (
        v.get("name"), v.get("start_year") or "?", editeur, v.get("count_of_issues") or "?", v.get("id"))


def _libelle_numero(n):
    return "#%s %s — %s [%s]" % (n.get("issue_number"), n.get("name") or "",
                                 n.get("cover_date") or "?", n.get("id"))


# ---------------------------------------------------------------------------------------
# Recherche
# ---------------------------------------------------------------------------------------

def chercher_volumes(cv, serie):
    nom = re.sub(r"[,:]", " ", serie)  # « , » sépare les filtres
    nom = " ".join(nom.split())
    r = cv.get("volumes/", filter="name:%s" % nom, field_list=CHAMPS_VOLUME, limit=100)
    if not r:
        r = cv.get("search/", query=nom, resources="volume", field_list=CHAMPS_VOLUME, limit=20)
    return [v for v in (r or []) if isinstance(v, dict) and v.get("id")]


def classer_volumes(volumes, serie, annee, numero):
    """Séries candidates, les plus probables d'abord, et « sûr » s'il n'y a pas à demander :
    une seule candidate, ou une seule de même nom exact (et de même année si connue)."""
    cible = _simple(serie)

    def note(v):
        n = 0
        if _simple(v.get("name")) == cible:
            n += 100
        debut = _annee(v.get("start_year"))
        if annee and debut:
            if debut == annee:
                n += 50
            elif debut <= annee:
                n += max(0, 30 - (annee - debut))
            else:
                n -= 50
        try:
            if numero and float(numero) <= float(v.get("count_of_issues") or 0):
                n += 10
        except ValueError:
            pass
        return n

    tries = sorted(volumes, key=note, reverse=True)
    exacts = [v for v in tries if _simple(v.get("name")) == cible]
    if annee:
        meme_annee = [v for v in exacts if _annee(v.get("start_year")) == annee]
        if len(meme_annee) == 1:
            return tries, meme_annee[0]
    if len(tries) == 1:
        return tries, tries[0]
    if len(exacts) == 1:
        return tries, exacts[0]
    return tries, None


def choisir(question, elements, libelle):
    """Choix de l'utilisateur parmi ``elements`` (10 au plus) ; ``None`` = ignorer (aussi
    sans interface : un automatisme ne choisit jamais au hasard)."""
    elements = elements[:10]
    libelles = [libelle(e) for e in elements]
    rep = comicrust.ask_choice(question, libelles + [IGNORER], IGNORER)
    if rep in libelles:
        return elements[libelles.index(rep)]
    return None


# ---------------------------------------------------------------------------------------
# Remplissage
# ---------------------------------------------------------------------------------------

def valeurs_numero(numero, volume):
    """Champs ComicRack tirés de la fiche d'un numéro et de sa série."""
    v = {}
    v["Title"] = numero.get("name") or None
    v["Summary"] = html_en_texte(numero.get("description")) or (numero.get("deck") or None)
    date = numero.get("cover_date") or numero.get("store_date") or ""
    m = re.match(r"^(\d{4})-(\d{2})", date)
    if m:
        v["Year"] = int(m.group(1))
        if int(m.group(2)) >= 1:
            v["Month"] = int(m.group(2))
    roles = {}
    for p in numero.get("person_credits") or []:
        for role in str(p.get("role") or "").split(","):
            for champ in ROLES.get(role.strip().lower(), []):
                noms = roles.setdefault(champ, [])
                if p.get("name") and p["name"] not in noms:
                    noms.append(p["name"])
    for champ, noms in roles.items():
        v[champ] = ", ".join(noms)
    v["Characters"] = _noms(numero.get("character_credits"))
    v["Teams"] = _noms(numero.get("team_credits"))
    v["Locations"] = _noms(numero.get("location_credits"))
    v["StoryArc"] = _noms(numero.get("story_arc_credits"))
    v["Publisher"] = ((volume or {}).get("publisher") or {}).get("name")
    v["Count"] = (volume or {}).get("count_of_issues") or None
    v["Web"] = numero.get("site_detail_url") or None
    return {k: x for k, x in v.items() if not _vide(x)}


def remplir(book, valeurs, remplacer):
    """Met en attente les champs ; renvoie le nombre de champs modifiés."""
    n = 0
    for champ, valeur in valeurs.items():
        actuelle = book.get(champ)
        if not remplacer and not _vide(actuelle):
            continue
        if actuelle != valeur:
            book.set(champ, valeur)
            n += 1
    return n


#@Name Infos depuis Comic Vine
#@Hook Books
#@Key comic-vine
#@Description Cherche chaque livre sur Comic Vine (série, numéro) et complète la fiche :
#@Description résumé, auteurs, date, éditeur… Clé d'API personnelle requise (Réglages).
def infos_comic_vine(books):
    cle = (_reglage("api_key") or "").strip()
    if not cle:
        comicrust.message("Indiquez d'abord votre clé d'API Comic Vine (bouton « Réglages » du "
                          "script). Elle est gratuite : https://comicvine.gamespot.com/api/")
        return None
    url_base = (_reglage("url_base") or os.environ.get("COMICRUST_COMICVINE_URL") or URL_BASE)
    try:
        delai = float(_reglage("delai"))
    except (TypeError, ValueError):
        delai = DEFAUTS["delai"]
    cv = ComicVine(cle, url_base, delai)
    remplacer = bool(_reglage("remplacer"))
    ecrire = bool(_reglage("ecrire_fichier"))

    choix_series = {}  # (série simplifiée, année) → volume choisi (ou None = ignorer)
    trouves = ignores = 0
    total = len(books)
    try:
        for i, book in enumerate(books):
            comicrust.check_cancelled()
            serie = book.series
            numero = book.number
            annee = _annee(book.year)
            if _vide(serie) or _vide(numero):
                s2, n2, a2 = _serie_numero_depuis_nom(book.file)
                serie = serie if not _vide(serie) else s2
                numero = numero if not _vide(numero) else n2
                annee = annee or a2
            nom = os.path.basename(book.file or "") or str(serie)
            comicrust.progress(i / max(total, 1), nom)
            if _vide(serie) or _vide(numero):
                comicrust.log("Série ou numéro inconnu, ignoré : %s" % nom, level="warning")
                ignores += 1
                continue
            numero = _numero(numero)

            cle_serie = (_simple(serie), annee)
            if cle_serie not in choix_series:
                volumes = chercher_volumes(cv, serie)
                if not volumes:
                    comicrust.log("Série introuvable sur Comic Vine : %s" % serie, level="warning")
                    choix_series[cle_serie] = None
                else:
                    tries, sur = classer_volumes(volumes, serie, annee, numero)
                    choix_series[cle_serie] = sur or choisir(
                        "Quelle série Comic Vine pour « %s »%s ?" % (serie, " (%d)" % annee if annee else ""),
                        tries, _libelle_volume)
            volume = choix_series[cle_serie]
            if volume is None:
                ignores += 1
                continue

            numeros = cv.get("issues/", filter="volume:%s,issue_number:%s" % (volume["id"], numero),
                             field_list=CHAMPS_NUMEROS, limit=20) or []
            if not numeros:
                comicrust.log("Numéro %s introuvable dans %s" % (numero, _libelle_volume(volume)),
                              level="warning")
                ignores += 1
                continue
            choisi = numeros[0] if len(numeros) == 1 else choisir(
                "Plusieurs numéros %s dans %s :" % (numero, volume.get("name")), numeros, _libelle_numero)
            if choisi is None:
                ignores += 1
                continue

            fiche = cv.get("issue/4000-%s/" % choisi["id"], field_list=CHAMPS_NUMERO)
            if not fiche:
                ignores += 1
                continue
            n = remplir(book, valeurs_numero(fiche, volume), remplacer)
            comicrust.log("%s : %s (%d champ(s))" % (nom, fiche.get("site_detail_url") or choisi["id"], n))
            trouves += 1
            if ecrire and n:
                comicrust.write_info(book)
    except Arret as e:
        comicrust.log(str(e), level="error")
        comicrust.message(str(e))
    comicrust.log("%d livre(s) trouvé(s), %d ignoré(s), %d requête(s) Comic Vine."
                  % (trouves, ignores, cv.requetes))
    return {"trouves": trouves, "ignores": ignores, "requetes": cv.requetes}
