# -*- coding: utf-8 -*-
"""Library Organizer pour ComicRust : ranger et renommer les fichiers selon des profils.

Portage en Python 3 pour ComicRust, modifié, du script « Library Organizer » 2.1.13 pour
ComicRack (``libraryorganizer.py``, ``configureform.py``, ``configformcontrols.py``,
``loforms.py``, ``loworkerform.py``, ``loduplicate.py``) :

    Copyright 2010-2012 Stonepaw

    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use this file except in compliance with the License.
    You may obtain a copy of the License at

        http://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.

Texte complet de la licence : ``LICENCE-library-organizer.txt`` (dossier ``scripts\\``).
Le moteur (profils, modèles, règles) est dans ``library_organizer_moteur.py`` (même
dossier). Ce portage n'est ni écrit ni approuvé par l'auteur de l'original.

Modifications par rapport à l'original :

- fenêtres WinForms/WPF remplacées par les formulaires de ComicRust (``comicrust.form``),
  textes en français ; la fenêtre de configuration garde l'organisation de l'original
  (profil, Vue d'ensemble, Fichiers, Dossiers, Règles, Options, Jetons) ; l'aperçu se
  recalcule avec le bouton « Aperçu » ; le menu « Insérer ▾ » remplace les boutons
  d'insertion (préfixes et suffixes réglés dans l'onglet « Jetons ») ;
- profils enregistrés dans les réglages du script (``data\\comicrust.ron``), importables
  et exportables au format de l'original (``losettingsx.dat``, ``.lop``) ;
- **aucun fichier n'est déplacé par le script** : chaque déplacement est une demande de
  renommage que ComicRust montre en aperçu puis applique, sans jamais écraser un fichier ;
- impossibles depuis un script ComicRust, donc signalés : mode « Copier » (exécuté comme
  une simulation), images des livres sans fichier, remplacement d'un fichier existant
  (« Garder les deux » ou « Ne pas déplacer » à la place), suppression des dossiers vides
  pendant le rangement (liste dans le rapport + commande « Library Organizer – supprimer
  les dossiers vides » après application) ;
- automatisme « Startup » (ComicRust ne transmet aucun livre au démarrage) remplacé par
  « Library Organizer (Quick) – toute la bibliothèque » ;
- journal enregistré automatiquement dans ``data\\library_organizer\\`` (ou le dossier
  choisi dans les options).
"""

import glob
import json
import os
import re
import time
import zipfile

import comicrust
from comicrust import ui

import library_organizer_moteur as lo

TITRE = "Library Organizer %s (ComicRust)" % lo.VERSION_ORIGINALE
NOM_DEFAUT = "Défaut"
NB_JOURNAUX = 10
# Déplacements demandés par le dernier rangement (profils), relus par l'annulation.
FICHIER_DEMANDE = "rangement_demande.json"
CLE_ANNULER = "library-organizer-annuler"


# ---------------------------------------------------------------------------------------
# Réglages : profils et dossier des journaux
# ---------------------------------------------------------------------------------------

def charger():
    """Profils enregistrés (un profil par défaut s'il n'y en a aucun) et profils utilisés
    en dernier."""
    s = comicrust.settings()
    profils = {}
    for nom, d in (s.get("profils") or {}).items():
        try:
            p = lo.Profile.from_json(d)
        except Exception as ex:  # réglage abîmé : profil ignoré, signalé
            comicrust.log("Profil « %s » illisible, ignoré : %s" % (nom, ex), level="warning")
            continue
        p.Name = nom
        profils[nom] = p
    if not profils:
        profils[NOM_DEFAUT] = lo.profil_par_defaut(NOM_DEFAUT)
    derniers = [n for n in (s.get("derniers") or []) if n in profils]
    if not derniers:
        derniers = [sorted(profils)[0]]
    return profils, derniers


def enregistrer(profils, derniers):
    s = comicrust.settings()
    s["profils"] = {n: p.to_json() for n, p in profils.items()}
    s["derniers"] = [n for n in derniers if n in profils] or [sorted(profils)[0]]
    s.save()


def dossier_donnees():
    """Dossier du journal, du rangement demandé et de la liste des dossiers vides :
    réglage « Dossier du journal », sinon ``data\\library_organizer`` à côté de
    ComicRust.exe (le module comicrust est dans ``data\\python``) : ComicRust reste
    portable."""
    d = (comicrust.settings().get("dossier_journal") or "").strip()
    if not d:
        data = os.path.dirname(os.path.dirname(os.path.abspath(comicrust.__file__)))
        d = os.path.join(data, "library_organizer")
    return d


def _ecrire(nom, contenu):
    """Écrit un fichier du dossier de données ; rend son chemin ou None (signalé)."""
    d = dossier_donnees()
    try:
        os.makedirs(d, exist_ok=True)
        chemin = os.path.join(d, nom)
        with open(chemin, "w", encoding="utf-8", newline="\n") as f:
            f.write(contenu)
        return chemin
    except OSError as ex:
        comicrust.log("Impossible d'écrire %s : %s" % (nom, ex), level="warning")
        return None


def _lire(nom):
    try:
        with open(os.path.join(dossier_donnees(), nom), encoding="utf-8") as f:
            return f.read()
    except OSError:
        return None


def _livres(books):
    return [lo.Livre(b.to_dict(), b) for b in books]


def _bibliotheque(livres):
    """Toute la bibliothèque, pour les valeurs calculées sur une série entière (année et
    mois de début, de fin, premier et dernier numéro, ``<number0>``, valeurs « series » des
    champs multiples) et la recherche des doublons, comme ``locommon.get_earliest_book`` /
    ``get_last_book`` de l'original (``ComicRack.App.GetLibraryBooks()``).

    Rend une fonction qui lit la bibliothèque **à la première demande** seulement (lecture
    seule, par lots : ``comicrust.library_books()``) puis la garde ; les livres traités
    (``livres``) y sont les mêmes objets. Sans accès à la bibliothèque : les livres traités
    seulement (signalé au journal)."""
    cache = []

    def charger():
        if cache:
            return cache[0]
        par_id = {l.id: l for l in livres}
        try:
            tous = []
            for b in comicrust.library_books():
                l = par_id.pop(b.id, None)
                tous.append(l if l is not None else lo.Livre(b.to_dict(), b))
            tous.extend(par_id.values())
        except comicrust.ScriptError as ex:
            comicrust.log("Bibliothèque non lue (%s) : valeurs de série calculées sur les livres traités "
                          "seulement." % ex, level="warning")
            tous = list(livres)
        cache.append(tous)
        return tous

    return charger


# ---------------------------------------------------------------------------------------
# Fenêtres posées pendant le rangement
# ---------------------------------------------------------------------------------------

IMAGES = (".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp")


def _naturel(t):
    return [int(x) if x.isdigit() else x.lower() for x in re.split(r"(\d+)", t)]


def couverture(chemin):
    """Première image d'un CBZ (ou l'image elle-même), 2 Mo au plus ; None sinon."""
    try:
        if not chemin or not os.path.isfile(chemin):
            return None
        if chemin.lower().endswith(IMAGES):
            return chemin if os.path.getsize(chemin) <= 2 * 1024 * 1024 else None
        if zipfile.is_zipfile(chemin):
            with zipfile.ZipFile(chemin) as z:
                noms = sorted((i for i in z.infolist() if i.filename.lower().endswith(IMAGES)),
                              key=lambda i: _naturel(i.filename))
                if noms and noms[0].file_size <= 2 * 1024 * 1024:
                    return z.read(noms[0])
    except Exception:
        pass
    return None


def _taille(chemin):
    try:
        return "%.2f Mo" % (os.path.getsize(chemin) / 1048576.0)
    except OSError:
        return ""


def _infos(livre, chemin):
    if livre is None:
        return "Livre absent de la bibliothèque\nTaille : %s\nChemin : %s" % (_taille(chemin), chemin)
    vol = livre.get("ShadowVolume")
    mois = livre.get("Month")
    annee = livre.get("ShadowYear")
    date = ", ".join(x for x in ((lo.MOIS_FR[mois - 1] if 1 <= mois <= 12 else ""),
                                 (str(annee) if annee != -1 else "")) if x)
    lignes = [
        "Série : " + livre.get("ShadowSeries"),
        "Volume : " + (str(vol) if vol != -1 else ""),
        "Numéro : " + livre.get("ShadowNumber"),
        "Pages : " + (str(livre.data.get("PageCount") or "")),
        "Taille : " + _taille(livre.FilePath or chemin),
        "Parution : " + date,
        "Ajouté le : " + (lo.format_date(livre.get("AddedTime"), "G") if livre.get("AddedTime") else ""),
        "Scan : " + livre.get("ScanInformation"),
        "Chemin : " + (livre.FilePath or chemin),
    ]
    return "\n".join(lignes)


class InterfaceComicRust(lo.Interface):
    """Questions de Library Organizer sous forme de formulaires ComicRust (sans interface,
    ``comicrust.form`` rend None : même effet que fermer la fenêtre dans l'original)."""

    def __init__(self, interactive=True):
        self.interactive = interactive

    def choisir_valeurs(self, valeurs, selection, champ, texte_livre, serie):
        if not self.interactive:
            return None
        nom = lo.NOMS_FR.get(lo.FIELD_TO_NAME.get(champ, champ), champ)
        if serie:
            intro = ("Choisissez les valeurs de « %s » à utiliser pour chaque numéro de la série %s."
                     % (nom, texte_livre))
        else:
            intro = "Choisissez les valeurs de « %s » à utiliser pour le numéro %s." % (nom, texte_livre)
        controles = [
            ui.label(intro),
            ui.list_edit("selection", "Valeurs choisies (dans l'ordre)", default=selection, choices=valeurs,
                         help="Ajoutez les valeurs voulues (liste des valeurs du livre en suggestion)."),
            ui.label("Valeurs disponibles : " + " ; ".join(valeurs), style="small"),
            ui.checkbox("dossier", "Mettre chaque valeur dans son propre dossier"),
        ]
        if serie:
            controles.append(ui.checkbox(
                "tous", "Utiliser chaque valeur choisie pour tous les numéros de la série, même s'ils "
                        "ne l'ont pas"))
        else:
            controles += [
                ui.checkbox("toujours", "Utiliser ces valeurs pour tous les livres de cette opération qui "
                                        "les ont"),
                ui.checkbox("sans_demander", "Ne pas demander s'il y a d'autres valeurs",
                            enabled_if="toujours"),
            ]
        r = comicrust.form("Valeurs de « %s »" % nom, controles, buttons=("OK", "Annuler"))
        if r is None:
            return None
        v = r[1]
        sel = [x for x in (v.get("selection") or []) if x]
        return lo.ResultatSelection(sel, bool(v.get("tous")), bool(v.get("dossier")),
                                    bool(v.get("toujours")),
                                    bool(v.get("sans_demander")) and bool(v.get("toujours")))

    def raccourcir_chemin(self, chemin):
        if not self.interactive:
            return None
        message = ("Le chemin calculé est trop long (%d caractères, %d au plus). Raccourcissez-le."
                   % (len(chemin), lo.MAX_CHEMIN))
        while True:
            r = comicrust.form("Chemin trop long", [ui.text("chemin", "Chemin", default=chemin, required=True)],
                               buttons=("OK", "Ignorer ce livre"), message=message, width=760)
            if r is None:
                return None
            chemin = (r[1].get("chemin") or "").strip()
            erreur = verifier_chemin(chemin)
            if not erreur:
                return chemin
            message = "⚠ " + erreur

    def doublon(self, profil, livre, ancien, chemin, nom_renomme, restants):
        if not self.interactive:
            return ("annuler", False)
        copie = profil.Mode == lo.Mode.Copy
        verbe = "copiez" if copie else "déplacez"
        garder = "Copier, mais garder les deux" if copie else "Déplacer, mais garder les deux"
        image_n = couverture(livre.FilePath)
        image_a = couverture(ancien.FilePath if ancien is not None else chemin)
        col_n = [ui.label("Fichier que vous %s" % verbe, style="bold")]
        col_a = [ui.label("Fichier déjà présent", style="bold")]
        if image_n:
            col_n.append(ui.image(image_n, max_width=200, max_height=280))
        if image_a:
            col_a.append(ui.image(image_a, max_width=200, max_height=280))
        col_n.append(ui.label(_infos(livre, livre.FilePath), style="small"))
        col_a.append(ui.label(_infos(ancien, chemin), style="small"))
        controles = [
            ui.columns(col_n, col_a),
            ui.label("« %s » : le fichier que vous %s sera renommé %s" % (garder, verbe, nom_renomme)),
            ui.label("« Remplacer » n'existe pas ici : ComicRust n'écrase jamais un fichier.", style="weak"),
            ui.checkbox("tous", "Faire de même pour les conflits suivants (%d)" % restants),
        ]
        msg = "Un fichier porte déjà ce nom à la destination : %s" % chemin
        if profil.Mode == lo.Mode.Simulate:
            msg += "\n(simulation : aucun fichier ne sera déplacé ni supprimé)"
        r = comicrust.form("Ce fichier existe déjà", controles,
                           buttons=(garder, "Ne pas déplacer", "Annuler"), cancel="Annuler",
                           message=msg, width=640)
        if r is None:
            return ("annuler", False)
        bouton, v = r
        action = "renommer" if bouton == garder else "annuler"
        return (action, bool(v.get("tous")))


def verifier_chemin(chemin):
    """Contrôles de la fenêtre « chemin trop long » de l'original (loforms.PathTooLongForm)."""
    if len(chemin) > lo.MAX_CHEMIN:
        return "Le chemin doit faire moins de 260 caractères (actuellement %d)." % len(chemin)
    if any(c in chemin for c in '<>|*?"'):
        return "Le chemin ne peut contenir aucun de ces caractères : < > | * ? \""
    if ":" in chemin[2:] or (":" in chemin[:2] and chemin[1:2] != ":"):
        return "Le chemin ne peut contenir « : » qu'après la lettre du lecteur."
    p = lo.module_chemins(chemin)
    if len(p.dirname(chemin)) >= 248:
        return "Le dossier doit faire moins de 248 caractères (actuellement %d)." % len(p.dirname(chemin))
    return None


# ---------------------------------------------------------------------------------------
# Rangement (loworkerform.WorkerForm)
# ---------------------------------------------------------------------------------------

def choisir_profils(profils, derniers):
    """Profil(s) à utiliser (loworkerform.ProfileSelector) ; None si annulé."""
    if len(profils) <= 1:
        return list(profils)
    if not comicrust.interactive():
        return derniers
    noms = sorted(profils)
    r = comicrust.form("Profil(s) à utiliser", [
        ui.list_edit("profils", "Profils, dans l'ordre", default=derniers, choices=noms, required=True,
                     help="Un livre concerné par plusieurs profils « Déplacer » est déplacé par le "
                          "dernier de la liste."),
        ui.label("Profils existants : " + " ; ".join(noms), style="small"),
    ], buttons=("OK", "Annuler"))
    if r is None:
        return None
    choisis = [n for n in (r[1].get("profils") or []) if n in profils]
    inconnus = [n for n in (r[1].get("profils") or []) if n not in profils]
    if inconnus:
        comicrust.log("Profils inconnus ignorés : " + ", ".join(inconnus), level="warning")
    if not choisis:
        return None
    s = comicrust.settings()
    s["derniers"] = choisis
    s.save()
    return choisis


def executer(books, profils, noms):
    """Calcule et demande les déplacements des livres pour les profils ``noms``."""
    utilises = [profils[n] for n in noms]
    livres = _livres(books)
    logger = lo.Logger()
    entete_sup = []
    for p in utilises:
        if p.Mode == lo.Mode.Copy:
            texte = ("Profil « %s » en mode Copier : ComicRust ne peut pas copier un fichier depuis un "
                     "script ; ce profil est exécuté comme une simulation (rien n'est copié)." % p.Name)
            comicrust.log(texte, level="warning")
            entete_sup.append(texte)
    interface = InterfaceComicRust(comicrust.interactive())
    mover = lo.BookMover(logger, interface,
                         renommer=lambda livre, chemin: comicrust.rename_file(livre.objet, chemin),
                         tous_les_livres=_bibliotheque(livres), annule=comicrust.cancelled,
                         progression=lambda f, t: comicrust.progress(f, "Library Organizer", t))
    entete = mover.process_books(livres, utilises)
    comicrust.progress(1.0, "Library Organizer")
    if len(mover.undo_collection):
        # Rangement *demandé* (profil de chaque déplacement) : l'annulation part du bilan
        # des renommages *appliqués* écrit par ComicRust après « Appliquer »
        # (comicrust.applied_renames()) et n'y prend que le nom des profils.
        _ecrire(FICHIER_DEMANDE, json.dumps({"date": time.strftime("%Y-%m-%dT%H:%M:%S"),
                                             "entrees": mover.undo_collection.to_json()},
                                            ensure_ascii=False, indent=1))
    vides = lo.dossiers_qui_seront_vides(mover.deplacements, mover.dossiers_retirer)
    return _terminer(logger, entete, entete_sup, vides, mover.failed_or_skipped,
                     any(p.Mode != lo.Mode.Move for p in utilises),
                     {"profils": {r._name: r.to_json() for r in mover.profile_reports},
                      "renommages": len(mover.deplacements), "annule": mover.annulation})


def _terminer(logger, entete, entete_sup, vides, echecs, simulation, resultat):
    """Journal, dossiers vides, rapport final."""
    if vides:
        anciens = [x for x in (_lire("dossiers_vides.txt") or "").splitlines() if x.strip()]
        _ecrire("dossiers_vides.txt", "\n".join(lo.nettoyer_liste_dossiers(anciens + vides)) + "\n")
        texte = ("Dossiers qui seront vides une fois les déplacements appliqués (%d) : ComicRust ne "
                 "supprime pas de dossier pendant l'application ; lancez ensuite « Library Organizer – "
                 "supprimer les dossiers vides ».\n  " % len(vides)) + "\n  ".join(vides)
        entete_sup.append(texte)
    if entete_sup:
        logger.add_header("\n\n" + "\n\n".join(entete_sup))
    nom = "journal-%s.txt" % time.strftime("%Y%m%d-%H%M%S")
    chemin = _ecrire(nom, logger.texte())
    _purger_journaux()
    comicrust.log(logger.header)
    for profil, action, path, message in logger.ToArray():
        niveau = "warning" if action.startswith("Échec") else "info"
        comicrust.log("[%s] %s : %s%s" % (profil, action, path, (" — " + message) if message else ""),
                      level=niveau)
    if chemin:
        comicrust.log("Journal : " + chemin)
    resultat["journal"] = chemin
    resultat["dossiers_vides"] = vides
    if comicrust.interactive():
        resume = logger.header
        if resultat.get("renommages") and not simulation:
            resume += ("\n\nLes déplacements sont proposés dans l'aperçu de ComicRust : rien n'est "
                       "déplacé avant « Appliquer ».")
        if echecs or simulation:
            details = "".join("\n%s — %s : %s%s" % (p, a, c, ("\n    " + m) if m else "")
                              for p, a, c, m in logger.ToArray()).strip()
            comicrust.form("Library Organizer – rapport", [
                ui.label(resume),
                ui.label("Journal complet : %s" % (chemin or "(non enregistré)"), style="small"),
                ui.textarea("details", "Livres ignorés, en échec ou simulés", default=details, rows=16),
            ], buttons=("OK",), cancel=None, width=760)
        else:
            comicrust.message(resume + ("\n\nJournal : %s" % chemin if chemin else ""))
    return resultat


def _purger_journaux():
    try:
        journaux = sorted(glob.glob(os.path.join(dossier_donnees(), "journal-*.txt")))
        for f in journaux[:-NB_JOURNAUX]:
            os.remove(f)
    except OSError:
        pass


def lancer(books, profils, derniers):
    noms = choisir_profils(profils, derniers)
    if not noms:
        comicrust.log("Aucun profil choisi : rien n'est fait.")
        return None
    return executer(books, profils, noms)


# ---------------------------------------------------------------------------------------
# Fenêtre de configuration (configureform.ConfigureForm)
# ---------------------------------------------------------------------------------------

def _fr(nom):
    return lo.NOMS_FR.get(nom, nom)


# Contrôles d'insertion de l'original : (nom du réglage, jeton, sorte, onglet).
JETONS = [
    ("Age Rating", "ageRating", "texte", "Texte"),
    ("Alternate Series", "altSeries", "texte", "Texte"),
    ("Format", "format", "texte", "Texte"),
    ("Imprint", "imprint", "texte", "Texte"),
    ("Language", "language", "texte", "Texte"),
    ("Main Character Or Team", "maincharacter", "texte", "Texte"),
    ("Month", "month", "texte", "Texte"),
    ("Publisher", "publisher", "texte", "Texte"),
    ("Series", "series", "texte", "Texte"),
    ("Series Group", "seriesgroup", "texte", "Texte"),
    ("Story Arc", "storyarc", "texte", "Texte"),
    ("Title", "title", "texte", "Texte"),
    ("Custom", "Custom", "perso", "Texte"),
    ("Alternate Count", "altCount", "nombre", "Nombres"),
    ("Alternate Number", "altNumber", "nombre", "Nombres"),
    ("Count", "count", "nombre", "Nombres"),
    ("Day", "Day", "nombre", "Nombres"),
    ("Month Number", "month#", "nombre", "Nombres"),
    ("Number", "number", "nombre", "Nombres"),
    ("Volume", "volume", "nombre", "Nombres"),
    ("Year", "year", "nombre", "Nombres"),
    ("Rating", "Rating", "texte", "Nombres"),
    ("Rating", "CommunityRating", "texte", "Nombres"),
    ("Manga", "manga", "ouinon", "Oui/Non"),
    ("Series Complete", "seriesComplete", "ouinon", "Oui/Non"),
    ("Alternate Series Multi", "altSeries", "multi", "Valeurs multiples"),
    ("Characters", "characters", "multi", "Valeurs multiples"),
    ("Colorist", "colorist", "multi", "Valeurs multiples"),
    ("Cover Artist", "coverartist", "multi", "Valeurs multiples"),
    ("Editor", "editor", "multi", "Valeurs multiples"),
    ("Genre", "genre", "multi", "Valeurs multiples"),
    ("Inker", "inker", "multi", "Valeurs multiples"),
    ("Letterer", "letterer", "multi", "Valeurs multiples"),
    ("Locations", "locations", "multi", "Valeurs multiples"),
    ("Penciller", "penciller", "multi", "Valeurs multiples"),
    ("Scan Information", "scaninfo", "multi", "Valeurs multiples"),
    ("Tags", "tags", "multi", "Valeurs multiples"),
    ("Teams", "teams", "multi", "Valeurs multiples"),
    ("Writer", "writer", "multi", "Valeurs multiples"),
    ("Start Year", "startyear", "texte", "Calculées"),
    ("Start Month", "startmonth", "mois", "Calculées"),
    ("End Year", "EndYear", "texte", "Calculées"),
    ("End Month", "EndMonth", "mois", "Calculées"),
    ("Added Date", "AddedDate", "date", "Calculées"),
    ("Released Date", "ReleasedDate", "date", "Calculées"),
    ("First Issue Number", "firstissuenumber", "numero", "Calculées"),
    ("Last Issue Number", "lastissuenumber", "numero", "Calculées"),
    ("First Letter", "first", "lettre", "Calculées"),
    ("Counter", "counter", "compteur", "Calculées"),
    ("Read Percentage", "read", "lecture", "Calculées"),
]
TABLES_JETONS = {"Texte": "jetons_texte", "Nombres": "jetons_nombres", "Oui/Non": "jetons_ouinon",
                 "Valeurs multiples": "jetons_multi", "Calculées": "jetons_calcules"}


def _libelle_jeton(nom, jeton):
    if jeton == "CommunityRating":
        return lo.NOMS_FR["Community Rating"]
    return _fr(nom)


def inserts(profil, livres, dossier):
    """Menu « Insérer ▾ » : un jeton par contrôle d'insertion de l'original, avec les
    préfixes, suffixes, séparateurs et textes du profil (onglet « Jetons »)."""
    s = " " if profil.AutoSpaceFields else ""
    out = []

    def tok(nom, coeur):
        return "{" + s + profil.Prefix.get(nom, "") + "<" + coeur + ">" + profil.Postfix.get(nom, "") + "}"

    for nom, jeton, sorte, onglet in JETONS:
        lib = _libelle_jeton(nom, jeton)
        if sorte == "texte":
            out.append((tok(nom, jeton), "%s : %s" % (onglet, lib)))
        elif sorte == "perso":
            cles = []
            for b in livres:
                for k, _v in lo.valeurs_personnalisees(b.data.get("CustomValuesStore")):
                    if k not in cles:
                        cles.append(k)
            for k in sorted(cles)[:50] or ["clé"]:
                out.append((tok(nom, "Custom(%s)" % k), "Texte : valeur personnalisée « %s »" % k))
        elif sorte == "nombre":
            out.append((tok(nom, jeton), "Nombres : %s" % lib))
            for pad, expl in (("0", "complété automatiquement"), ("2", "sur 2 chiffres"), ("3", "sur 3 chiffres")):
                out.append((tok(nom, jeton + pad), "Nombres : %s (%s)" % (lib, expl)))
        elif sorte == "ouinon":
            texte = profil.TextBox.get(nom, "") or lib
            out.append((tok(nom, "%s(%s)" % (jeton, texte)), "Oui/Non : « %s » si %s = oui" % (texte, lib)))
            out.append((tok(nom, "%s(%s)(!)" % (jeton, texte)), "Oui/Non : « %s » si %s = non" % (texte, lib)))
        elif sorte == "multi":
            sep = profil.Seperator.get(nom, "") or ","
            if s:
                sep = sep.rstrip() + s
            out.append((tok(nom, "%s(%s)(issue)" % (jeton, sep)), "Multiples : %s (choix par numéro)" % lib))
            out.append((tok(nom, "%s(%s)(series)" % (jeton, sep)), "Multiples : %s (choix pour la série)" % lib))
        elif sorte == "mois":
            out.append((tok(nom, jeton), "Calculées : %s (nom du mois)" % lib))
            out.append((tok(nom, jeton + "#0"), "Calculées : %s (nombre)" % lib))
            out.append((tok(nom, jeton + "#2"), "Calculées : %s (nombre sur 2 chiffres)" % lib))
        elif sorte == "date":
            for f in lo.FORMATS_DATE:
                out.append((tok(nom, "%s(%s)" % (jeton, f)), "Calculées : %s (%s)" % (lib, f)))
        elif sorte == "numero":
            out.append((tok(nom, jeton), "Calculées : %s" % lib))
            out.append((tok(nom, jeton + "3"), "Calculées : %s (sur 3 chiffres)" % lib))
        elif sorte == "lettre":
            for champ in ("Series", "Publisher", "Imprint", "AlternateSeries"):
                out.append((tok(nom, "first(%s)" % champ), "Calculées : première lettre de %s"
                            % _fr(lo.FIELD_TO_NAME.get(champ, champ)).lower()))
        elif sorte == "compteur":
            out.append((tok(nom, "counter(1)(1)(3)"), "Calculées : compteur (début 1, pas 1, 3 chiffres)"))
        elif sorte == "lecture":
            texte = profil.TextBox.get(nom, "") or "Lu"
            for op, pct, expl in (("=", "100", "= 100 %"), (">", "0", "> 0 %"), ("<", "100", "< 100 %")):
                out.append((tok(nom, "read(%s)(%s)(%s)" % (texte, op, pct)),
                            "Calculées : « %s » si pourcentage lu %s" % (texte, expl)))
    out += [
        ("{texte<?series(valeur)>}", "Conditionnel : texte si Série = valeur"),
        ("{texte<?series(!^expression)>}", "Conditionnel : texte si Série correspond à l'expression"),
        ("{texte<!series>}", "Inversé : texte si Série est vide"),
        ("{texte<!series(valeur)>}", "Inversé : texte si Série ≠ valeur"),
        ("{texte<!series(!^expression)>}", "Inversé : texte si Série ne correspond pas à l'expression"),
    ]
    if dossier:
        out.insert(0, ("\\", "Dossiers : séparateur de dossier « \\ »"))
    return grouper_inserts(out)


# Groupes du menu « Insérer ▾ » (catégories de jetons de l'original, onglets de la
# fenêtre d'insertion), dans l'ordre d'affichage.
GROUPES_JETONS = {"Conditionnel": "Conditions", "Inversé": "Conditions", "Multiples": "Valeurs multiples"}


def grouper_inserts(morceaux):
    """[(jeton, « Catégorie : libellé »)] → {catégorie: [(jeton, libellé)]} (menu par
    groupes de ComicRust, avec filtre) ; l'ordre des catégories est celui des morceaux."""
    groupes = {}
    for jeton, libelle in morceaux:
        cat, sep, reste = libelle.partition(" : ")
        if not sep:
            cat, reste = "Divers", libelle
        groupe = GROUPES_JETONS.get(cat, cat)
        groupes.setdefault(groupe, []).append((jeton, libelle if groupe == "Conditions" else reste))
    return groupes


# --- profil ↔ valeurs du formulaire ---

CHOIX_CHAMPS_REGLES = sorted(((n, _fr(n)) for n in lo.CHAMPS_REGLES), key=lambda c: c[1].lower())
CHOIX_OPERATEURS = [(o, lo.OPERATEURS_FR[o]) for o in lo.OPERATEURS_REGLES]
CHOIX_GROUPE = [("All", "toutes"), ("Any", "au moins une")]
CHOIX_SUBSTITUTION = sorted(((n, _fr(n)) for n in lo.CHAMPS_SUBSTITUTION), key=lambda c: c[1].lower())
CHOIX_ECHEC = sorted(((n, _fr(n)) for n in lo.CHAMPS_ECHEC), key=lambda c: c[1].lower())


def _regles_vers_tables(regles):
    lignes, groupes = [], []

    def parcourir(liste, chemin):
        n = 0
        for r in liste:
            if isinstance(r, lo.ExcludeGroup):
                n += 1
                gid = (chemin + "." if chemin else "") + str(n)
                groupes.append([gid, r.operator])
                parcourir(r.rules, gid)
            else:
                lignes.append([chemin, r.field, r.operator, r.value])

    parcourir(regles, "")
    return lignes, groupes


def _tables_vers_regles(lignes, groupes):
    """Règles et groupes (identifiant « 1 », « 1.2 »… = groupe 2 dans le groupe 1)."""
    par_id = {}
    ordre = []

    def groupe(gid, operateur=None):
        gid = gid.strip(". ")
        if gid not in par_id:
            par_id[gid] = lo.ExcludeGroup(operateur or "Any")
            ordre.append(gid)
        elif operateur:
            par_id[gid].operator = operateur
        return par_id[gid]

    for ligne in groupes:
        gid = str(ligne[0] if ligne else "").strip()
        if gid:
            groupe(gid, (ligne[1] if len(ligne) > 1 else "Any") or "Any")
    haut = []
    for ligne in lignes:
        ligne = list(ligne) + ["", "", "", ""]
        gid, champ, op, valeur = str(ligne[0]).strip(), ligne[1], ligne[2], str(ligne[3] or "")
        if champ in lo.VALEURS_OUI_NON:
            valeur = lo.VALEURS_OUI_NON_FR.get(valeur.strip().lower(), valeur.strip())
            if op not in ("is", "is not"):
                op = "is"
        r = lo.ExcludeRule(champ, op, valeur)
        if gid:
            groupe(gid).rules.append(r)
        else:
            haut.append(r)
    # Groupes imbriqués : « 1.2 » va dans « 1 » (créé au besoin).
    for gid in list(ordre):
        if "." in gid:
            groupe(gid.rsplit(".", 1)[0])
    racines = []
    for gid in sorted(ordre, key=lambda g: [int(x) if x.isdigit() else x for x in g.split(".")]):
        if "." in gid:
            par_id[gid.rsplit(".", 1)[0]].rules.append(par_id[gid])
        else:
            racines.append(par_id[gid])
    return haut + racines


def valeurs_depuis_profil(p):
    lignes, groupes = _regles_vers_tables(p.ExcludeRules)
    v = {
        "mode": p.Mode if p.Mode in (lo.Mode.Move, lo.Mode.Copy, lo.Mode.Simulate) else lo.Mode.Move,
        "copie_biblio": p.CopyMode,
        "utiliser_fichier": p.UseFileName,
        "utiliser_dossier": p.UseFolder,
        "dossier_base": p.BaseFolder,
        "sans_fichier": p.MoveFileless,
        "format_image": p.FilelessFormat if p.FilelessFormat in (".bmp", ".jpg", ".png") else ".jpg",
        "modele_fichier": p.FileTemplate,
        "modele_dossier": p.FolderTemplate,
        "regles_mode": p.ExcludeMode if p.ExcludeMode in ("Do not", "Only") else "Do not",
        "regles_operateur": p.ExcludeOperator if p.ExcludeOperator in ("All", "Any") else "Any",
        "regles": lignes,
        "groupes": groupes,
        "dossiers_exclus": list(p.ExcludeFolders),
        "espaces_multiples": p.ReplaceMultipleSpaces,
        "copier_lecture": p.CopyReadPercentage,
        "multi_un": p.DontAskWhenMultiOne,
        "suppr_vides": p.RemoveEmptyFolder,
        "vides_exceptions": list(p.ExcludedEmptyFolder),
        "nom_dossier_vide": p.EmptyFolder,
        "car_interdits": [[k, x] for k, x in p.IllegalCharacters.items()],
        "mois": [[k, p.Months[k]] for k in sorted(p.Months)],
        "echec_vides": p.FailEmptyValues,
        "champs_echec": [lo.FIELD_TO_NAME.get(f, f) for f in p.FailedFields],
        "deplacer_echecs": p.MoveFailed,
        "dossier_echecs": p.FailedFolder,
        "substitutions": [[lo.FIELD_TO_NAME.get(f, f), x] for f, x in p.EmptyData.items() if x],
        "espace_auto": p.AutoSpaceFields,
    }
    for onglet, cle in TABLES_JETONS.items():
        lignes_j = []
        vus = set()
        for nom, jeton, sorte, og in JETONS:
            if og != onglet or nom in vus:
                continue
            vus.add(nom)
            ligne = [_fr(nom), p.Prefix.get(nom, ""), p.Postfix.get(nom, "")]
            if onglet == "Valeurs multiples":
                ligne.append(p.Seperator.get(nom, ""))
            elif onglet in ("Oui/Non", "Calculées"):
                ligne.append(p.TextBox.get(nom, ""))
            lignes_j.append(ligne)
        v[cle] = lignes_j
    return v


def profil_depuis_valeurs(p, v):
    """Enregistre les valeurs du formulaire dans le profil (configureform.save_profile)."""
    p.BaseFolder = (v.get("dossier_base") or "").strip()
    p.Mode = v.get("mode") or lo.Mode.Move
    if p.Mode == lo.Mode.Copy:
        p.CopyMode = bool(v.get("copie_biblio"))
    p.UseFileName = bool(v.get("utiliser_fichier"))
    p.UseFolder = bool(v.get("utiliser_dossier"))
    p.MoveFileless = bool(v.get("sans_fichier"))
    p.FilelessFormat = v.get("format_image") or ".jpg"
    p.FileTemplate = v.get("modele_fichier") or ""
    p.FolderTemplate = v.get("modele_dossier") or ""
    p.ExcludeMode = v.get("regles_mode") or "Do not"
    p.ExcludeOperator = v.get("regles_operateur") or "Any"
    p.ExcludeRules = _tables_vers_regles(v.get("regles") or [], v.get("groupes") or [])
    p.ExcludeFolders = [x for x in (v.get("dossiers_exclus") or []) if x]
    p.ReplaceMultipleSpaces = bool(v.get("espaces_multiples"))
    p.CopyReadPercentage = bool(v.get("copier_lecture"))
    p.DontAskWhenMultiOne = bool(v.get("multi_un"))
    p.RemoveEmptyFolder = bool(v.get("suppr_vides"))
    p.ExcludedEmptyFolder = [x for x in (v.get("vides_exceptions") or []) if x]
    p.EmptyFolder = v.get("nom_dossier_vide") or ""
    car = {}
    for ligne in v.get("car_interdits") or []:
        if ligne and str(ligne[0]):
            car[str(ligne[0])] = str(ligne[1] if len(ligne) > 1 and ligne[1] is not None else "")
    p.IllegalCharacters = car
    mois = dict(lo.MOIS_DEFAUT)
    for ligne in v.get("mois") or []:
        try:
            mois[int(float(ligne[0]))] = str(ligne[1] or "")
        except (TypeError, ValueError, IndexError):
            pass
    p.Months = mois
    p.FailEmptyValues = bool(v.get("echec_vides"))
    p.FailedFields = [lo.NAME_TO_FIELD[n] for n in (v.get("champs_echec") or []) if n in lo.NAME_TO_FIELD]
    p.MoveFailed = bool(v.get("deplacer_echecs"))
    p.FailedFolder = (v.get("dossier_echecs") or "").strip()
    vides = {}
    for ligne in v.get("substitutions") or []:
        if ligne and ligne[0] in lo.NAME_TO_FIELD and len(ligne) > 1 and ligne[1]:
            vides[lo.NAME_TO_FIELD[ligne[0]]] = str(ligne[1])
    p.EmptyData = vides
    p.AutoSpaceFields = bool(v.get("espace_auto"))
    inverse = {}
    for nom, _j, _s, onglet in JETONS:
        inverse.setdefault((onglet, _fr(nom)), nom)
    for onglet, cle in TABLES_JETONS.items():
        for ligne in v.get(cle) or []:
            nom = inverse.get((onglet, str(ligne[0]) if ligne else ""))
            if nom is None:
                continue
            ligne = list(ligne) + ["", "", ""]
            p.Prefix[nom] = str(ligne[1] or "")
            p.Postfix[nom] = str(ligne[2] or "")
            if onglet == "Valeurs multiples":
                p.Seperator[nom] = str(ligne[3] or "")
            elif onglet in ("Oui/Non", "Calculées"):
                p.TextBox[nom] = str(ligne[3] or "")
    return p


def verifier(v):
    """Contrôles avant d'enregistrer un profil (configureform.check_profile_settings)."""
    erreurs = []
    if not (v.get("dossier_base") or "").strip() and v.get("utiliser_dossier"):
        erreurs.append("Le dossier de base ne peut pas être vide quand l'organisation des dossiers est activée.")
    if v.get("utiliser_fichier") and not (v.get("modele_fichier") or "").strip():
        erreurs.append("La structure du nom de fichier ne peut pas être vide quand l'organisation des "
                       "fichiers est activée.")
    if not v.get("utiliser_fichier") and not v.get("utiliser_dossier"):
        erreurs.append("Activez l'organisation des dossiers ou celle des noms de fichiers, sinon le script "
                       "ne fait rien.")
    if v.get("deplacer_echecs") and not (v.get("dossier_echecs") or "").strip():
        erreurs.append("Le dossier des livres en échec ne peut pas être vide quand « déplacer ou copier "
                       "dans ce dossier » est coché.")
    return erreurs


def apercu(profil, livres, n, interactive=False, bibliotheque=None):
    """(dossier, fichier) calculés pour le livre n° ``n`` (1 = premier), comme les zones
    « Aperçu » de l'original ; None sans livre. ``bibliotheque`` : fonction rendant toute la
    bibliothèque (:func:`_bibliotheque`) pour les valeurs calculées sur la série."""
    if not livres:
        return None
    livre = livres[max(1, min(int(n or 1), len(livres))) - 1]
    calculs = lo.Calculs(bibliotheque or livres)
    if not (lo.check_excluded_folders(livre.FilePath, profil)
            and lo.check_metadata_rules(livre, profil, calculs)):
        return livre.FileDirectory, livre.FileNameWithExtension + "   (livre exclu par les règles)"
    pm = lo.PathMaker(profil, calculs, InterfaceComicRust(interactive))
    dossier, fichier, echec = pm.make_path(livre, profil.FolderTemplate, profil.FileTemplate)
    if echec:
        fichier += "   (échec : %s vide)" % ", ".join(lo._noms(pm.failed_fields))
    return dossier, fichier


def formulaire(profil, noms, livres, n_apercu, globaux, inserts_f, inserts_d, texte_apercu):
    """Contrôles de la fenêtre de configuration."""
    ap_f = ap_d = "Aperçu non disponible sans livre (lancez « Configurer Library Organizer » depuis le menu " \
                  "Scripts : toute la bibliothèque sert d'aperçu)."
    if texte_apercu:
        ap_d, ap_f = texte_apercu
    tete = [
        ui.columns(
            [ui.choice("profil", "Profil", noms)],
            [ui.button("changer", "Afficher ce profil", help="Enregistre le profil affiché puis ouvre celui "
                                                              "choisi dans la liste.")],
        ),
        ui.columns(
            [ui.button("nouveau", "Nouveau…"), ui.button("renommer", "Renommer…")],
            [ui.button("dupliquer", "Dupliquer…"), ui.button("supprimer", "Supprimer")],
            [ui.button("importer", "Importer…"), ui.button("exporter", "Exporter…")],
            [ui.button("exporter_tout", "Exporter tout…"),
             ui.button("importer_cr", "Importer les réglages de Library Organizer (ComicRack)…")],
        ),
    ]
    if livres:
        tete.append(ui.columns(
            [ui.number("livre_apercu", "Livre de l'aperçu (n° dans la liste)", n_apercu, min=1, max=len(livres))],
            [ui.button("apercu", "Aperçu", help="Recalcule le dossier et le nom avec les réglages saisis.")],
        ))
    vue = [
        ui.radio("mode", "Mode", [(lo.Mode.Move, "Déplacer"), (lo.Mode.Copy, "Copier"),
                                  (lo.Mode.Simulate, "Simuler")], horizontal=True),
        ui.label("Simuler : aucun fichier touché, journal complet créé.", style="small"),
        ui.label("Copier : impossible dans ComicRust (un script ne peut ni copier un fichier ni ajouter un "
                 "livre) ; un profil en mode Copier est exécuté comme une simulation.", style="warning",
                 enabled_if="mode=Copy"),
        ui.checkbox("copie_biblio", "Ajouter le livre copié à la bibliothèque", enabled_if="mode=Copy"),
        ui.separator(),
        ui.checkbox("utiliser_fichier", "Organiser les noms de fichiers"),
        ui.checkbox("utiliser_dossier", "Organiser les dossiers"),
        ui.folder("dossier_base", "Dossier de base", enabled_if="utiliser_dossier"),
        ui.separator(),
        ui.checkbox("sans_fichier", "Copier la vignette personnalisée des livres sans fichier vers le chemin "
                                    "calculé (sans toucher à l'image d'origine)",
                    help="Non disponible dans ComicRust : ces livres sont signalés en échec."),
        ui.choice("format_image", "Format d'image", [".bmp", ".jpg", ".png"], enabled_if="sans_fichier"),
    ]
    fichiers = [
        ui.text("modele_fichier", "Structure du nom de fichier", inserts=inserts_f,
                help="{préfixe<champ>suffixe} : le préfixe et le suffixe ne sont écrits que si le champ "
                     "n'est pas vide. L'extension est ajoutée automatiquement."),
        ui.label("Aperçu : " + ap_f),
    ]
    dossiers = [
        ui.text("modele_dossier", "Structure des dossiers (« \\ » sépare les dossiers)", inserts=inserts_d),
        ui.label("Aperçu : " + ap_d),
    ]
    regles = [
        ui.group("Règles sur les champs", [
            ui.columns(
                [ui.choice("regles_mode", "Livres qui correspondent à…",
                           [("Do not", "Ne pas déplacer"), ("Only", "Déplacer seulement")])],
                [ui.choice("regles_operateur", "…", [("All", "toutes les règles"),
                                                     ("Any", "au moins une règle")])],
            ),
            ui.table("regles", "Règles", [
                ui.column("Groupe", width=70),
                ui.column("Champ", "choice", choices=CHOIX_CHAMPS_REGLES),
                ui.column("Opérateur", "choice", choices=CHOIX_OPERATEURS),
                ui.column("Valeur"),
            ]),
            ui.table("groupes", "Groupes de règles", [
                ui.column("Groupe", width=70),
                ui.column("Correspondre à", "choice", choices=CHOIX_GROUPE),
            ]),
            ui.label("Groupe vide = règle de premier niveau ; « 1 » = règle du groupe 1 ; « 1.2 » = groupe 2 "
                     "à l'intérieur du groupe 1. Série complète, Noir et blanc, Manga : valeur Oui, Non, "
                     "Inconnu (ou Yes, No, Unknown, « Oui (droite à gauche) » pour Manga) avec « est » ou "
                     "« n'est pas ».", style="small"),
        ]),
        ui.group("Règles sur les dossiers", [
            ui.list_edit("dossiers_exclus", "Ne pas déplacer les livres situés dans ces dossiers"),
        ]),
    ]
    options = [ui.tabs({
        "Options": [
            ui.checkbox("espaces_multiples", "Remplacer les espaces multiples par un seul"),
            ui.checkbox("copier_lecture", "En remplaçant un fichier existant, reprendre son pourcentage lu",
                        help="Sans effet dans ComicRust (un fichier existant n'est jamais remplacé)."),
            ui.checkbox("multi_un", "Champ multiple avec une seule valeur : l'insérer sans demander"),
            ui.checkbox("suppr_vides", "Supprimer les dossiers vides",
                        help="ComicRust ne supprime pas de dossier pendant l'application : les dossiers qui "
                             "deviendront vides sont listés dans le rapport ; la commande « Library Organizer "
                             "– supprimer les dossiers vides » les supprime ensuite (après confirmation)."),
            ui.list_edit("vides_exceptions", "Mais ne pas supprimer ces dossiers", enabled_if="suppr_vides"),
            ui.text("nom_dossier_vide", "Remplacer les noms de dossiers vides par (vide = retirer ce niveau)"),
            ui.table("car_interdits", "Caractères interdits et leur remplacement", [
                ui.column("Caractère", width=90), ui.column("Remplacé par")]),
            ui.table("mois", "Noms des mois (13 à 16 : saisons)", [
                ui.column("N°", "number", width=60), ui.column("Nom")]),
            ui.folder("dossier_journal", "Dossier du journal et des fichiers d'annulation (vide = "
                                         "data\\library_organizer à côté de ComicRust)",
                      help="Réglage commun à tous les profils."),
        ],
        "Valeurs vides": [
            ui.checkbox("echec_vides", "Si l'un des champs choisis est vide, marquer l'opération en échec"),
            ui.multichoice("champs_echec", "Champs", CHOIX_ECHEC, enabled_if="echec_vides", height=160),
            ui.checkbox("deplacer_echecs", "et déplacer ces livres dans ce dossier :", enabled_if="echec_vides"),
            ui.folder("dossier_echecs", "Dossier des livres en échec", enabled_if="deplacer_echecs"),
            ui.table("substitutions", "Quand un champ est vide, écrire à la place", [
                ui.column("Champ", "choice", choices=CHOIX_SUBSTITUTION), ui.column("Valeur")]),
        ],
    })]
    jetons = [
        ui.checkbox("espace_auto", "Insérer automatiquement une espace avant les champs"),
        ui.label("Préfixes, suffixes, séparateurs et textes utilisés par le menu « Insérer ▾ » des modèles "
                 "(cliquez sur « Aperçu » ou « Afficher ce profil » pour mettre le menu à jour).",
                 style="small"),
        ui.tabs({
            "Texte": [ui.table("jetons_texte", "", [ui.column("Champ"), ui.column("Préfixe"),
                                                     ui.column("Suffixe")])],
            "Nombres": [ui.table("jetons_nombres", "", [ui.column("Champ"), ui.column("Préfixe"),
                                                         ui.column("Suffixe")]),
                        ui.label("Remplissage 0 : automatique, d'après le dernier numéro de la série parmi "
                                 "les livres transmis (n° 50 → 2 chiffres).", style="small")],
            "Oui/Non": [ui.table("jetons_ouinon", "", [ui.column("Champ"), ui.column("Préfixe"),
                                                        ui.column("Suffixe"), ui.column("Texte")]),
                        ui.label("Le texte est inséré quand le champ vaut Oui ; avec « (!) », quand il vaut "
                                 "Non.", style="small")],
            "Valeurs multiples": [ui.table("jetons_multi", "", [ui.column("Champ"), ui.column("Préfixe"),
                                                                 ui.column("Suffixe"), ui.column("Séparateur")]),
                                  ui.label("(issue) : choix des valeurs pour chaque numéro ; (series) : pour "
                                           "toute la série. La fenêtre de choix permet aussi un dossier par "
                                           "valeur.", style="small")],
            "Calculées": [ui.table("jetons_calcules", "", [ui.column("Champ"), ui.column("Préfixe"),
                                                            ui.column("Suffixe"), ui.column("Texte")]),
                          ui.label("Année et mois de début/fin, premier et dernier numéro : calculés sur les "
                                   "livres de la même série parmi ceux transmis au script (sélection, ou toute "
                                   "la bibliothèque depuis le menu « bibliothèque »).", style="small")],
        }),
    ]
    return [
        ui.group("Profil", tete),
        ui.tabs({"Vue d'ensemble": vue, "Fichiers": fichiers, "Dossiers": dossiers, "Règles": regles,
                 "Options": options, "Jetons": jetons}),
    ]


def demander_nom(texte, existants, defaut=""):
    """Nom de profil non vide et différent des autres (loforms.GetProfileNameDialog) ;
    None si annulé."""
    precedent = None
    proposition = defaut
    while True:
        nom = (comicrust.ask(texte, proposition) or "").strip()
        if not nom or nom == precedent or (defaut and nom == defaut and precedent is None and defaut in existants):
            return None
        if "," in nom:
            texte_err = "Le nom ne peut pas contenir de virgule (séparateur des profils utilisés)."
        elif nom in existants:
            texte_err = "Un profil porte déjà ce nom."
        else:
            return nom
        comicrust.message(texte_err)
        precedent = nom
        proposition = nom


def chemins_reglages_comicrack():
    """Emplacements probables de losettingsx.dat (dossier du script dans ComicRack)."""
    motifs = []
    for var in ("APPDATA", "ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"):
        base = os.environ.get(var)
        if not base:
            continue
        motifs += [os.path.join(base, "cYo", "*", "Scripts", "*", "losettingsx.dat"),
                   os.path.join(base, "*ComicRack*", "Scripts", "*", "losettingsx.dat")]
    trouves = []
    for m in motifs:
        for f in glob.glob(m):
            if f not in trouves:
                trouves.append(f)
    return trouves


def importer_fichier(chemin, travail, remplacer):
    """Importe les profils d'un fichier de l'original ; rend (noms importés, derniers, message)."""
    try:
        with open(chemin, "rb") as f:
            donnees = f.read()
    except OSError as ex:
        return [], [], "Fichier illisible : %s" % ex
    profils, derniers, erreurs = lo.lire_profils_xml(donnees)
    if not profils:
        return [], [], "Aucun profil importé : " + ("; ".join(erreurs) or "fichier sans profil")
    if remplacer:
        travail.clear()
    importes = []
    for nom, p in profils.items():
        if nom in travail:
            nouveau = demander_nom("Le profil importé « %s » existe déjà. Entrez un autre nom :" % nom,
                                   list(travail), nom)
            if nouveau is None:
                continue
            p.Name = nouveau
            nom = nouveau
        travail[nom] = p
        importes.append(nom)
    message = "%d profil(s) importé(s) : %s." % (len(importes), ", ".join(importes))
    if erreurs:
        message += "\n" + "\n".join(erreurs)
    return importes, [n for n in derniers if n in importes], message


def _defaut_intact(travail):
    if len(travail) != 1:
        return False
    (nom, p), = travail.items()
    return nom == NOM_DEFAUT and p.to_json() == lo.profil_par_defaut(NOM_DEFAUT).to_json()


def configurer(books):
    """Fenêtre de configuration ; vrai si l'utilisateur a validé (profils enregistrés)."""
    profils, derniers = charger()
    travail = {n: p.duplicate() for n, p in profils.items()}
    courant = derniers[0]
    s = comicrust.settings()
    globaux = {"dossier_journal": s.get("dossier_journal") or ""}
    livres = _livres(books) if books else []
    bibliotheque = _bibliotheque(livres)
    n_apercu = 1
    valeurs = None
    message = None
    texte_apercu = None
    while True:
        p = travail[courant]
        if valeurs is None:
            valeurs = valeurs_depuis_profil(p)
            valeurs.update(globaux)
        valeurs["profil"] = courant
        valeurs["livre_apercu"] = n_apercu
        if texte_apercu is None and livres:
            temp = profil_depuis_valeurs(p.duplicate(), valeurs)
            texte_apercu = apercu(temp, livres, n_apercu, bibliotheque=bibliotheque)
        noms = sorted(travail)
        r = comicrust.form(TITRE, formulaire(p, noms, livres, n_apercu, globaux,
                                             inserts(p, livres, False), inserts(p, livres, True), texte_apercu),
                           buttons=("OK", "Annuler"), values=valeurs, message=message, width=820)
        message = None
        texte_apercu = None
        if r is None:
            return False
        bouton, v = r
        n_apercu = int(v.get("livre_apercu") or 1)
        cible = v.get("profil") or courant
        erreurs = verifier(v)
        globaux["dossier_journal"] = (v.get("dossier_journal") or "").strip()

        def garder():
            profil_depuis_valeurs(travail[courant], v)

        if bouton == "OK":
            if erreurs:
                message = "⚠ Corrigez d'abord :\n" + "\n".join(erreurs)
                valeurs = v
                continue
            garder()
            s["dossier_journal"] = globaux["dossier_journal"]
            derniers = [n for n in derniers if n in travail] or [courant]
            enregistrer(travail, derniers)
            return True
        if bouton in ("changer", "nouveau", "dupliquer", "exporter", "exporter_tout") and erreurs:
            message = "⚠ Corrigez d'abord :\n" + "\n".join(erreurs)
            v["profil"] = courant
            valeurs = v
            continue
        garder()
        valeurs = None
        if bouton == "changer":
            courant = cible if cible in travail else courant
        elif bouton == "nouveau":
            nom = demander_nom("Nom du nouveau profil :", list(travail))
            if nom:
                np = lo.Profile()
                np.Name = nom
                np.Version = lo.VERSION
                travail[nom] = np
                courant = nom
        elif bouton == "dupliquer":
            nom = demander_nom("Nom de la copie du profil :", list(travail))
            if nom:
                np = travail[courant].duplicate()
                np.Name = nom
                travail[nom] = np
                courant = nom
        elif bouton == "renommer":
            nom = demander_nom("Nouveau nom du profil :", list(travail), courant)
            if nom:
                pr = travail.pop(courant)
                pr.Name = nom
                travail[nom] = pr
                derniers = [nom if x == courant else x for x in derniers]
                courant = nom
        elif bouton == "supprimer":
            if len(travail) == 1:
                message = "Impossible de supprimer le seul profil."
            else:
                ordre = sorted(travail)
                i = ordre.index(courant)
                del travail[courant]
                # Comme l'original : le profil précédent si c'était le dernier de la liste,
                # sinon celui qui prend sa place.
                courant = sorted(travail)[i - 1 if i == len(ordre) - 1 else i]
        elif bouton in ("importer", "importer_cr"):
            courant, derniers, message = _importer(bouton == "importer_cr", travail, courant, derniers)
        elif bouton in ("exporter", "exporter_tout"):
            message = _exporter(travail, courant, derniers if bouton == "exporter_tout" else None,
                                bouton == "exporter_tout")
        elif bouton == "apercu":
            if livres:
                texte_apercu = apercu(travail[courant], livres, n_apercu, interactive=True,
                                      bibliotheque=bibliotheque)


def _importer(depuis_comicrack, travail, courant, derniers):
    trouves = chemins_reglages_comicrack() if depuis_comicrack else []
    intact = _defaut_intact(travail)
    if depuis_comicrack:
        titre = "Importer les réglages de Library Organizer (ComicRack)"
        aide = [
            ui.label("ComicRack garde les profils de Library Organizer dans le fichier losettingsx.dat du "
                     "dossier du script, par exemple :"),
            ui.label("%APPDATA%\\cYo\\ComicRack Community Edition\\Scripts\\Library Organizer\\losettingsx.dat\n"
                     "%APPDATA%\\cYo\\ComicRack\\Scripts\\Library Organizer\\losettingsx.dat\n"
                     "(ou un profil exporté .lop)", style="small"),
        ]
        if trouves:
            aide.append(ui.label("Trouvé : " + " ; ".join(trouves), style="small"))
    else:
        titre = "Importer des profils"
        aide = [ui.label("Profil(s) exporté(s) par Library Organizer (.lop) ou fichier losettingsx.dat.")]
    r = comicrust.form(titre, aide + [
        ui.file("fichier", "Fichier", default=trouves[0] if trouves else "",
                filters=[("Profils Library Organizer", ["dat", "lop", "xml"])], required=True),
        ui.checkbox("remplacer", "Remplacer les profils actuels (sinon : ajouter les profils importés)",
                    default=intact),
    ], buttons=("Importer", "Annuler"))
    if r is None:
        return courant, derniers, None
    chemin = (r[1].get("fichier") or "").strip()
    importes, derniers_imp, message = importer_fichier(chemin, travail, bool(r[1].get("remplacer")))
    if importes:
        if r[1].get("remplacer"):
            derniers = derniers_imp or [importes[0]]
        courant = (derniers_imp or importes)[0]
    if courant not in travail:
        courant = sorted(travail)[0]
    derniers = [n for n in derniers if n in travail] or [courant]
    return courant, derniers, message


def _exporter(travail, courant, derniers, tout):
    r = comicrust.form("Exporter " + ("tous les profils" if tout else "le profil « %s »" % courant), [
        ui.file("fichier", "Fichier", filters=[("Profil Library Organizer", ["lop"])], save=True, required=True),
    ], buttons=("Exporter", "Annuler"))
    if r is None:
        return None
    chemin = (r[1].get("fichier") or "").strip()
    if not os.path.splitext(chemin)[1]:
        chemin += ".lop"
    donnees = lo.ecrire_profils_xml(travail, derniers) if tout else lo.ecrire_profil_xml(travail[courant])
    try:
        with open(chemin, "wb") as f:
            f.write(donnees)
    except OSError as ex:
        return "⚠ Export impossible : %s" % ex
    return "Exporté dans %s" % chemin


# ---------------------------------------------------------------------------------------
# Points d'entrée (libraryorganizer.py)
# ---------------------------------------------------------------------------------------

#@Name Library Organizer
#@Hook Books
#@Key library-organizer
#@Description Range et renomme les fichiers des livres sélectionnés selon des profils (modèles de
#@Description dossier et de nom, règles, simulation) : fenêtre de configuration, puis déplacements
#@Description proposés dans l'aperçu de ComicRust. Portage de Library Organizer (ComicRack).
def library_organizer(books):
    if not books:
        return None
    if not configurer(books):
        return None
    profils, derniers = charger()
    return lancer(books, profils, derniers)


#@Name Library Organizer (Quick)
#@Hook Books
#@Key library-organizer-quick
#@Description Range les livres sélectionnés avec le(s) dernier(s) profil(s) utilisé(s), sans
#@Description passer par la configuration.
def library_organizer_quick(books):
    if not books:
        return None
    profils, derniers = charger()
    if len(profils) == 1 and list(profils.values())[0].BaseFolder == "":
        comicrust.message("Library Organizer ne fonctionnera pas comme prévu tant que le dossier de base est "
                          "vide. Lancez d'abord « Library Organizer » ou « Configurer Library Organizer ».")
        return None
    return lancer(books, profils, derniers)


#@Name Library Organizer (Quick) – toute la bibliothèque
#@Hook Library
#@Key library-organizer-bibliotheque
#@Description Range toute la bibliothèque avec le(s) dernier(s) profil(s) utilisé(s) (remplace
#@Description l'automatisme « Startup » de l'original). Les déplacements passent par l'aperçu.
def library_organizer_bibliotheque(books):
    return library_organizer_quick(books)


#@Name Configurer Library Organizer
#@Hook Library
#@Key library-organizer-configurer
#@Description Profils de Library Organizer, avec toute la bibliothèque pour l'aperçu des noms.
def configurer_library_organizer(books):
    configurer(books)


#@Name Library Organizer : réglages
#@Hook ConfigScript
#@Key library-organizer-reglages
#@Description Profils de Library Organizer (modèles, règles, options, import des réglages de
#@Description ComicRack).
def reglages():
    configurer([])


#@Name Library Organizer – annuler le dernier rangement
#@Hook Library
#@Key library-organizer-annuler
#@Description Remet à leur ancien emplacement les livres du dernier rangement (déplacements
#@Description proposés dans l'aperçu).
def annuler(books):
    # Seuls les renommages réellement appliqués comptent (bilan écrit par ComicRust après
    # « Appliquer ») : un aperçu abandonné ou un renommage refusé n'est jamais « annulé ».
    bilan = comicrust.applied_renames()
    if not bilan or not bilan.get("renames"):
        comicrust.message("Rien à annuler : aucun rangement appliqué n'est connu (un rangement n'est "
                          "enregistré qu'une fois ses déplacements appliqués dans ComicRust).")
        return None
    quand = (bilan.get("date") or "").replace("T", " ")[:19]
    if bilan.get("entry") == CLE_ANNULER:
        comicrust.message("Le dernier rangement a déjà été annulé (annulation appliquée le %s)." % quand)
        return {"deja_annule": True}
    demande = None
    try:
        demande = json.loads(_lire(FICHIER_DEMANDE) or "null")
    except ValueError:
        comicrust.log("%s illisible : profils inconnus pour l'annulation." % FICHIER_DEMANDE, level="warning")
    undo = lo.UndoCollection.depuis_renommages(bilan["renames"], (demande or {}).get("entrees"))
    profils, _derniers = charger()
    livres = _livres(books)
    logger = lo.Logger()
    mover = lo.UndoMover(logger, undo, profils, interface=InterfaceComicRust(comicrust.interactive()),
                         renommer=lambda livre, chemin: comicrust.rename_file(livre.objet, chemin),
                         tous_les_livres=livres, annule=comicrust.cancelled,
                         progression=lambda f, t: comicrust.progress(f, "Annulation", t))
    entete = mover.process_books()
    logger.add_header("\nRangement annulé : %d déplacement(s) appliqué(s) le %s." % (len(undo), quand))
    vides = lo.dossiers_qui_seront_vides(mover.deplacements, mover.dossiers_retirer)
    return _terminer(logger, entete, [], vides, mover.failed_or_skipped, False,
                     {"succes": mover.success, "echecs": mover.failed, "ignores": mover.skipped,
                      "renommages": len(mover.deplacements)})


#@Name Library Organizer – supprimer les dossiers vides
#@Hook Library
#@Key library-organizer-dossiers-vides
#@Description Supprime, après confirmation, les dossiers laissés vides par les derniers rangements
#@Description (seulement s'ils sont vraiment vides : aucun fichier n'est supprimé).
def supprimer_dossiers_vides(books):
    # Normalisés, sans doublon ni racine (lecteur, partage) : même une liste modifiée à la
    # main ne fait jamais proposer « D:\\ ».
    liste = lo.nettoyer_liste_dossiers(x.strip() for x in (_lire("dossiers_vides.txt") or "").splitlines()
                                       if x.strip())
    # Les plus profonds d'abord : un parent dont le seul contenu est un dossier vide de la
    # liste sera vide à son tour.
    candidats = sorted(liste, key=lambda d: -len(d.rstrip("\\/")))
    vides = []
    for d in candidats:
        try:
            if not os.path.isdir(d):
                continue
            p = lo.module_chemins(d)
            partis = {lo.cle_chemin(x, p) for x in vides}
            if all(lo.cle_chemin(p.join(d, n), p) in partis for n in os.listdir(d)):
                vides.append(d)
        except OSError:
            pass
    if not vides:
        comicrust.message("Aucun dossier vide à supprimer (les rangements ont-ils été appliqués ?).")
        return {"supprimes": []}
    if not comicrust.confirm("Supprimer ces dossiers vides ?\n\n" + "\n".join(vides), False):
        return {"supprimes": []}
    supprimes = []
    for d in vides:
        try:
            os.rmdir(d)  # échoue si le dossier n'est pas vide : aucun fichier n'est supprimé
            supprimes.append(d)
            comicrust.log("Dossier supprimé : " + d)
        except OSError as ex:
            comicrust.log("Dossier non supprimé %s : %s" % (d, ex), level="warning")
    restants = [d for d in liste if d not in supprimes and os.path.isdir(d)]
    _ecrire("dossiers_vides.txt", "".join(x + "\n" for x in restants))
    comicrust.message("%d dossier(s) vide(s) supprimé(s)." % len(supprimes))
    return {"supprimes": supprimes}
