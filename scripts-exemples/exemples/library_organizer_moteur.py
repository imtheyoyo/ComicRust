# -*- coding: utf-8 -*-
"""Moteur de Library Organizer pour ComicRust (profils, modèles, règles, rangement).

Portage en Python 3 pour ComicRust, modifié, de « Library Organizer » 2.1.13 pour
ComicRack (fichiers ``losettings.py``, ``locommon.py``, ``lobookmover.py``,
``lologger.py``) :

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

Modifications par rapport à l'original (portage ComicRust) :

- plus de .NET/IronPython : XML lu et écrit avec ``xml.etree``, chemins avec
  ``ntpath``/``posixpath``, dates mises en forme par un équivalent des formats .NET
  (culture française) ;
- les livres sont des fiches ComicRust (dictionnaires aux noms de champs ComicRack) : les
  valeurs « proposées » (``ShadowSeries``…) sont déduites du nom du fichier comme le fait
  ComicRust ;
- **rien n'est déplacé ici** : un déplacement devient une demande de renommage
  (rappel ``renommer``), que ComicRust montre en aperçu puis applique (sans jamais écraser
  un fichier) ;
- modes « Copier » et images des fiches sans fichier, remplacement d'un fichier existant,
  suppression des dossiers vides : impossibles depuis un script ComicRust, ils sont
  signalés dans le rapport (voir ``library_organizer.py``) ;
- valeurs calculées (année de début, dernier numéro…) : sur les livres transmis au script
  (sélection ou bibliothèque), pas toute la bibliothèque ;
- textes en français.

Ce module ne dépend pas de ``comicrust`` : il est importé par ``library_organizer.py``
(même dossier) et peut servir seul (essais).
"""

import codecs
import copy
import datetime
import ntpath
import os
import posixpath
import re
import xml.etree.ElementTree as ET

# Version du format des profils (attribut ``Version`` de losettingsx.dat).
VERSION = 2.1
# Version de Library Organizer reprise.
VERSION_ORIGINALE = "2.1.13"

# Longueur maximale d'un chemin (ComicRack, programme .NET Framework, ne gère pas plus).
MAX_CHEMIN = 259


class Mode(object):
    Move = "Move"
    Copy = "Copy"
    Simulate = "Simulate"


# Nom affiché (règles, champs « vides ») → champ de la fiche (locommon.name_to_field).
NAME_TO_FIELD = {
    "Added Date": "AddedDate", "Age Rating": "AgeRating", "Alternate Count": "AlternateCount",
    "Alternate Number": "AlternateNumber", "Alternate Series": "AlternateSeries",
    "Black And White": "BlackAndWhite", "Characters": "Characters", "Colorist": "Colorist",
    "Counter": "Counter", "Count": "ShadowCount", "Cover Artist": "CoverArtist", "Day": "Day",
    "Editor": "Editor", "End Year": "EndYear", "End Month": "EndMonth",
    "File Format": "FileFormat", "File Name": "FileName", "File Path": "FilePath",
    "First Letter": "FirstLetter", "Format": "ShadowFormat", "Genre": "Genre",
    "Imprint": "Imprint", "Inker": "Inker", "Language": "LanguageISO", "Letterer": "Letterer",
    "Locations": "Locations", "Main Character Or Team": "MainCharacterOrTeam", "Manga": "Manga",
    "Month": "Month", "Notes": "Notes", "Number": "ShadowNumber", "Penciller": "Penciller",
    "Publisher": "Publisher", "Rating": "Rating", "Read Percentage": "ReadPercentage",
    "Released Date": "Released Date", "Review": "Review", "Scan Information": "ScanInformation",
    "Series": "ShadowSeries", "Series Complete": "SeriesComplete", "Series Group": "SeriesGroup",
    "Start Month": "StartMonth", "Start Year": "StartYear", "Story Arc": "StoryArc",
    "Tags": "Tags", "Teams": "Teams", "Title": "ShadowTitle", "Volume": "ShadowVolume",
    "Web": "Web", "Writer": "Writer", "Year": "ShadowYear",
}

FIELD_TO_NAME = {
    "AddedDate": "Added Date", "AgeRating": "Age Rating", "AlternateCount": "Alternate Count",
    "AlternateNumber": "Alternate Number", "AlternateSeries": "Alternate Series",
    "BlackAndWhite": "Black And White", "Characters": "Characters", "Colorist": "Colorist",
    "Counter": "Counter", "CoverArtist": "Cover Artist", "Day": "Day", "Editor": "Editor",
    "EndMonth": "End Month", "EndYear": "End Year", "FileFormat": "File Format",
    "FileName": "File Name", "FilePath": "File Path", "FirstLetter": "First Letter",
    "Genre": "Genre", "Imprint": "Imprint", "Inker": "Inker", "LanguageISO": "Language",
    "Letterer": "Letterer", "Locations": "Locations",
    "MainCharacterOrTeam": "Main Character Or Team", "Manga": "Manga", "Month": "Month",
    "Notes": "Notes", "Penciller": "Penciller", "Publisher": "Publisher", "Rating": "Rating",
    "ReadPercentage": "Read Percentage", "Read": "Read Percentage",
    "ReleasedDate": "Released Date", "Review": "Review", "ScanInformation": "Scan Information",
    "SeriesComplete": "Series Complete", "ShadowCount": "Count", "ShadowFormat": "Format",
    "ShadowNumber": "Number", "ShadowSeries": "Series", "SeriesGroup": "Series Group",
    "StoryArc": "Story Arc", "ShadowTitle": "Title", "ShadowVolume": "Volume",
    "ShadowYear": "Year", "StartMonth": "Start Month", "StartYear": "Start Year", "Tags": "Tags",
    "Teams": "Teams", "Web": "Web", "Writer": "Writer",
}

# Libellés français des noms de champs de l'original (l'original et ses fichiers gardent
# les noms anglais : ce sont eux qui sont enregistrés).
NOMS_FR = {
    "Added Date": "Date d'ajout", "Age Rating": "Classification", "Alternate Count": "Nombre (autre série)",
    "Alternate Number": "Numéro (autre série)", "Alternate Series": "Autre série",
    "Alternate Series Multi": "Autres séries (multiple)", "Black And White": "Noir et blanc",
    "Characters": "Personnages", "Colorist": "Coloriste", "Count": "Nombre de numéros",
    "Counter": "Compteur", "Cover Artist": "Couverture", "Custom": "Valeur personnalisée",
    "Day": "Jour", "Editor": "Directeur de publication", "End Month": "Mois de fin",
    "End Year": "Année de fin", "File Format": "Format du fichier", "File Name": "Nom du fichier",
    "File Path": "Chemin du fichier", "First Issue Number": "Premier numéro",
    "First Letter": "Première lettre", "Format": "Format", "Genre": "Genre",
    "Imprint": "Collection", "Inker": "Encreur", "Language": "Langue",
    "Last Issue Number": "Dernier numéro", "Letterer": "Lettreur", "Locations": "Lieux",
    "Main Character Or Team": "Personnage ou équipe principal", "Manga": "Manga",
    "Month": "Mois", "Month Number": "Mois (nombre)", "Notes": "Notes", "Number": "Numéro",
    "Penciller": "Dessinateur", "Publisher": "Éditeur", "Rating": "Note",
    "Community Rating": "Note de la communauté", "Read Percentage": "Pourcentage lu",
    "Released Date": "Date de parution", "Review": "Critique",
    "Scan Information": "Informations de scan", "Series": "Série",
    "Series Complete": "Série complète", "Series Group": "Groupe de séries",
    "Start Month": "Mois de début", "Start Year": "Année de début", "Story Arc": "Arc",
    "Tags": "Étiquettes", "Teams": "Équipes", "Title": "Titre", "Volume": "Volume",
    "Web": "Web", "Writer": "Scénariste", "Year": "Année",
}

# Champs proposés dans les règles (configformcontrols.MetadataExcludeRuleControl).
CHAMPS_REGLES = [
    "Age Rating", "Alternate Count", "Alternate Number", "Alternate Series", "Black And White",
    "Characters", "Count", "File Name", "File Path", "File Format", "Format", "Genre", "Imprint",
    "Language", "Locations", "Main Character Or Team", "Manga", "Month", "Number", "Notes",
    "Publisher", "Rating", "Read Percentage", "Review", "Series Complete", "Tags", "Teams",
    "Title", "Scan Information", "Series", "Series Group", "Start Month", "Start Year",
    "Story Arc", "Volume", "Web", "Year",
]
OPERATEURS_REGLES = ["contains", "does not contain", "greater than", "less than", "is", "is not"]
OPERATEURS_FR = {
    "contains": "contient", "does not contain": "ne contient pas", "greater than": "supérieur à",
    "less than": "inférieur à", "is": "est", "is not": "n'est pas",
}
# Champs Oui/Non des règles et leurs valeurs (« is » / « is not » seulement).
VALEURS_OUI_NON = {
    "Series Complete": ["Yes", "No", "Unknown"],
    "Black And White": ["Yes", "No", "Unknown"],
    "Manga": ["Yes", "Yes (Right to Left)", "No", "Unknown"],
}
VALEURS_OUI_NON_FR = {
    "oui": "Yes", "non": "No", "inconnu": "Unknown", "oui (droite à gauche)": "Yes (Right to Left)",
    "oui (de droite à gauche)": "Yes (Right to Left)",
}

# Champs « marquer en échec si vide » (configureform.failed_items).
CHAMPS_ECHEC = [
    "Age Rating", "Alternate Count", "Alternate Number", "Alternate Series", "Black And White",
    "Characters", "Colorist", "Count", "Cover Artist", "Editor", "Format", "Genre", "Imprint",
    "Inker", "Language", "Letterer", "Locations", "Main Character Or Team", "Manga", "Month",
    "Notes", "Number", "Penciller", "Publisher", "Rating", "Read Percentage", "Review",
    "Scan Information", "Series", "Series Complete", "Series Group", "Start Month", "Start Year",
    "Story Arc", "Tags", "Teams", "Title", "Volume", "Web", "Writer", "Year",
]
# Champs à valeur de substitution (configureform.empty_substitution_items).
CHAMPS_SUBSTITUTION = [
    "Age Rating", "Alternate Count", "Alternate Number", "Alternate Series", "Black And White",
    "Characters", "Colorist", "Count", "Cover Artist", "Editor", "Format", "First Letter",
    "Genre", "Imprint", "Inker", "Language", "Letterer", "Locations", "Main Character Or Team",
    "Manga", "Month", "Number", "Penciller", "Publisher", "Rating", "Read Percentage",
    "Scan Information", "Series", "Series Complete", "Series Group", "Start Month",
    "Start Year", "Story Arc", "Tags", "Teams", "Title", "Volume", "Writer", "Year",
]

CARACTERES_INTERDITS_DEFAUT = {"?": "", "/": "", "\\": "", "*": "", ":": " - ", "<": "[",
                               ">": "]", "|": "!", "\"": "'"}
MOIS_DEFAUT = {1: "January", 2: "February", 3: "March", 4: "April", 5: "May", 6: "June",
               7: "July", 8: "August", 9: "September", 10: "October", 11: "November",
               12: "December", 13: "Spring", 14: "Summer", 15: "Fall", 16: "Winter"}

MODELE_FICHIER_DEFAUT = "{<series>}{ Vol.<volume>}{ #<number2>}{ (of <count2>)}{ ({<month>, }<year>)}"
MODELE_DOSSIER_DEFAUT = "{<publisher>}\\{<imprint>}\\{<series>}{ (<startyear>{ <format>})}"


# ---------------------------------------------------------------------------------------
# Profils (losettings.Profile)
# ---------------------------------------------------------------------------------------

class ExcludeRule(object):
    """Règle champ / opérateur / valeur (locommon.ExcludeRule)."""

    def __init__(self, field, operator, value):
        self.field = field
        self.operator = operator
        self.value = value

    def book_should_be_moved(self, book, calculs):
        """1 si le livre correspond à la règle, 0 sinon (None si la règle est inutilisable)."""
        field = NAME_TO_FIELD.get(self.field)
        if field is None:
            return None
        if field in ("Manga", "SeriesComplete", "BlackAndWhite"):
            valeur = book.oui_non(field)
            attendu = "YesAndRightToLeft" if self.value == "Yes (Right to Left)" else self.value
            return self._calcul(valeur, attendu)
        if field in ("StartYear", "StartMonth"):
            debut = calculs.premier_livre(book)
            donnee = texte_net(debut.get("Month") if field == "StartMonth" else debut.get("ShadowYear"))
            return self._calcul(donnee, self.value)
        return self._calcul(texte_net(book.get(field)), self.value)

    def _calcul(self, field_data, value):
        op = self.operator
        if op == "is":
            return 1 if field_data == value else 0
        if op == "does not contain":
            return 1 if value not in field_data else 0
        if op == "contains":
            return 1 if value in field_data else 0
        if op == "is not":
            return 1 if value != field_data else 0
        if op == "greater than":
            try:
                return 1 if int(value) < int(field_data) else 0
            except ValueError:
                return 1 if value < field_data else 0
        if op == "less than":
            try:
                return 1 if int(value) > int(field_data) else 0
            except ValueError:
                return 1 if value > field_data else 0
        return None

    def to_json(self):
        return {"type": "regle", "champ": self.field, "operateur": self.operator, "valeur": self.value}

    def to_xml(self, parent):
        ET.SubElement(parent, "ExcludeRule", {"Field": self.field, "Operator": self.operator,
                                              "Value": self.value})


class ExcludeGroup(object):
    """Groupe de règles « Tous » / « Au moins un » (locommon.ExcludeGroup)."""

    def __init__(self, operator, rules=None):
        self.operator = operator
        self.rules = list(rules or [])

    def book_should_be_moved(self, book, calculs):
        count = total = 0
        for rule in self.rules:
            result = rule.book_should_be_moved(book, calculs)
            if result is None:
                continue
            count += result
            total += 1
        if total == 0:
            return None
        if self.operator == "Any":
            return 1 if count > 0 else 0
        return 1 if count == total else 0

    def to_json(self):
        return {"type": "groupe", "operateur": self.operator, "regles": [r.to_json() for r in self.rules]}

    def to_xml(self, parent):
        g = ET.SubElement(parent, "ExcludeGroup", {"Operator": self.operator})
        for r in self.rules:
            r.to_xml(g)


def regle_depuis_json(d):
    if d.get("type") == "groupe":
        return ExcludeGroup(d.get("operateur") or "Any", [regle_depuis_json(x) for x in d.get("regles") or []])
    return ExcludeRule(d.get("champ") or "Series", d.get("operateur") or "is", d.get("valeur") or "")


def _regle_depuis_xml(node):
    if node.tag == "ExcludeGroup":
        g = ExcludeGroup(node.get("Operator") or "Any")
        for n in node:
            r = _regle_depuis_xml(n)
            if r is not None:
                g.rules.append(r)
        return g
    if node.tag == "ExcludeRule":
        # Changement 1.7.17 → 2.0 : attribut « Text » devenu « Value ».
        valeur = node.get("Value")
        if valeur is None:
            valeur = node.get("Text") or ""
        return ExcludeRule(node.get("Field") or "", node.get("Operator") or "", valeur)
    return None


class Profile(object):
    """Réglages d'un profil (mêmes attributs que losettings.Profile, pour relire et
    réécrire le fichier ``losettingsx.dat`` / ``.lop`` de l'original)."""

    def __init__(self):
        self.Version = 0
        self.FolderTemplate = ""
        self.BaseFolder = ""
        self.FileTemplate = ""
        self.Name = ""
        self.EmptyFolder = ""
        self.EmptyData = {}
        self.Postfix = {}
        self.Prefix = {}
        self.Seperator = {}
        self.IllegalCharacters = dict(CARACTERES_INTERDITS_DEFAUT)
        self.Months = dict(MOIS_DEFAUT)
        self.TextBox = {}
        self.UseFolder = True
        self.UseFileName = True
        self.ExcludeFolders = []
        self.DontAskWhenMultiOne = True
        self.ExcludeRules = []
        self.ExcludeOperator = "Any"
        self.RemoveEmptyFolder = True
        self.ExcludedEmptyFolder = []
        self.MoveFileless = False
        self.FilelessFormat = ".jpg"
        self.ExcludeMode = "Do not"
        self.FailEmptyValues = False
        self.MoveFailed = False
        self.FailedFolder = ""
        self.FailedFields = []
        self.Mode = Mode.Move
        self.CopyMode = True
        self.AutoSpaceFields = True
        self.ReplaceMultipleSpaces = True
        self.CopyReadPercentage = True

    # Ordre d'écriture (celui de losettingsx.dat).
    _ORDRE = ["FolderTemplate", "BaseFolder", "FileTemplate", "EmptyFolder", "EmptyData", "Postfix",
              "Prefix", "Seperator", "IllegalCharacters", "Months", "TextBox", "UseFolder",
              "UseFileName", "ExcludeFolders", "DontAskWhenMultiOne", "ExcludeOperator",
              "RemoveEmptyFolder", "ExcludedEmptyFolder", "MoveFileless", "FilelessFormat",
              "ExcludeMode", "FailEmptyValues", "MoveFailed", "FailedFolder", "FailedFields",
              "Mode", "CopyMode", "AutoSpaceFields", "ReplaceMultipleSpaces", "CopyReadPercentage"]

    def duplicate(self):
        return copy.deepcopy(self)

    def update(self):
        """Mise à niveau d'un profil d'une version antérieure à 2.0 (losettings.update)."""
        if self.Version < 2.0:
            if self.Mode == "Test":
                self.Mode = Mode.Simulate
            remplacements = {"Language": "LanguageISO", "Format": "ShadowFormat", "Count": "ShadowCount",
                             "Number": "ShadowNumber", "Series": "ShadowSeries", "Title": "ShadowTitle",
                             "Volume": "ShadowVolume", "Year": "ShadowYear"}
            for key in list(self.EmptyData.keys()):
                if key in remplacements:
                    self.EmptyData[remplacements[key]] = self.EmptyData.pop(key)
            insertions = {"SeriesComplete": "Series Complete", "Read": "Read Percentage",
                          "FirstLetter": "First Letter", "AgeRating": "Age Rating",
                          "AlternateSeriesMulti": "Alternate Series Multi", "MonthNumber": "Month Number",
                          "AlternateNumber": "Alternate Number", "StartMonth": "Start Month",
                          "AlternateSeries": "Alternate Series", "ScanInformation": "Scan Information",
                          "StartYear": "Start Year", "AlternateCount": "Alternate Count"}
            for key, nouveau in insertions.items():
                for d in (self.TextBox, self.Prefix, self.Postfix, self.Seperator):
                    if key in d:
                        d[nouveau] = d.pop(key)
        self.Version = VERSION

    # --- JSON (réglages ComicRust) ---

    def to_json(self):
        d = {}
        for nom in ["Name"] + self._ORDRE:
            v = getattr(self, nom)
            if nom == "Months":
                v = {str(k): x for k, x in v.items()}
            elif isinstance(v, (dict, list)):
                v = copy.deepcopy(v)
            d[nom] = v
        d["Version"] = self.Version
        d["ExcludeRules"] = [r.to_json() for r in self.ExcludeRules]
        return d

    @classmethod
    def from_json(cls, d):
        p = cls()
        for nom in ["Name"] + cls._ORDRE:
            if nom not in d:
                continue
            defaut = getattr(p, nom)
            v = d[nom]
            if isinstance(defaut, bool):
                p.__dict__[nom] = bool(v)
            elif isinstance(defaut, str):
                p.__dict__[nom] = "" if v is None else str(v)
            elif isinstance(defaut, list):
                p.__dict__[nom] = [str(x) for x in (v or [])]
            elif isinstance(defaut, dict):
                if nom == "Months":
                    m = dict(MOIS_DEFAUT)
                    for k, x in (v or {}).items():
                        try:
                            m[int(k)] = str(x)
                        except ValueError:
                            pass
                    p.Months = m
                elif nom == "IllegalCharacters":
                    # Remplace entièrement : un caractère retiré dans la configuration
                    # ne doit pas revenir.
                    p.IllegalCharacters = {str(k): str(x) for k, x in (v or {}).items()}
                else:
                    p.__dict__[nom] = {str(k): str(x) for k, x in (v or {}).items()}
        try:
            p.Version = float(d.get("Version") or VERSION)
        except (TypeError, ValueError):
            p.Version = VERSION
        p.ExcludeRules = [regle_depuis_json(x) for x in d.get("ExcludeRules") or []]
        return p

    # --- XML (losettingsx.dat, .lop) ---

    def to_xml(self, parent=None):
        attrs = {"Name": self.Name, "Version": _nombre_net(self.Version)}
        e = ET.Element("Profile", attrs) if parent is None else ET.SubElement(parent, "Profile", attrs)
        for nom in self._ORDRE:
            v = getattr(self, nom)
            if isinstance(v, bool):
                ET.SubElement(e, nom).text = "true" if v else "false"
            elif isinstance(v, str):
                ET.SubElement(e, nom).text = v
            elif isinstance(v, dict):
                sous = ET.SubElement(e, nom)
                ecrire_vides = nom in ("IllegalCharacters", "Months")
                for k, x in v.items():
                    if x or ecrire_vides:
                        ET.SubElement(sous, "Item", {"Name": str(k), "Value": str(x)})
            elif isinstance(v, list):
                sous = ET.SubElement(e, nom)
                for x in v:
                    if x:
                        ET.SubElement(sous, "Item").text = x
        regles = ET.SubElement(e, "ExcludeRules", {"Operator": self.ExcludeOperator,
                                                   "ExcludeMode": self.ExcludeMode})
        for r in self.ExcludeRules:
            if r:
                r.to_xml(regles)
        return e

    def load_from_xml(self, node):
        """Relit un nœud ``<Profile>`` (ou ``<Setting>`` de la 1.7) ; faux en cas d'erreur."""
        try:
            self.Name = node.get("Name") or ""
            if node.get("Version") is not None:
                self.Version = float(node.get("Version").replace(",", "."))
            for nom in self._ORDRE:
                defaut = getattr(self, nom)
                enfant = node.find(nom)
                if isinstance(defaut, bool):
                    if enfant is not None:
                        self.__dict__[nom] = _booleen_net(enfant.text)
                elif isinstance(defaut, str):
                    if enfant is not None:
                        self.__dict__[nom] = "".join(enfant.itertext())
                elif isinstance(defaut, list):
                    items = node.findall(nom + "/Item")
                    if items:
                        self.__dict__[nom] = ["".join(i.itertext()) for i in items]
                elif isinstance(defaut, dict):
                    for item in node.findall(nom + "/Item"):
                        if len(item.attrib) == 2 and "Name" in item.attrib and "Value" in item.attrib:
                            if nom == "Months":
                                defaut[int(item.get("Name"))] = item.get("Value")
                            else:
                                defaut[item.get("Name")] = item.get("Value")
            regles = node.find("ExcludeRules")
            if regles is not None:
                self.ExcludeOperator = regles.get("Operator") or self.ExcludeOperator
                self.ExcludeMode = regles.get("ExcludeMode") or self.ExcludeMode
                for n in regles:
                    r = _regle_depuis_xml(n)
                    if r is not None:
                        self.ExcludeRules.append(r)
            self.update()
            return True
        except Exception as ex:  # comme l'original : le profil est ignoré
            self.erreur = str(ex)
            return False


def profil_par_defaut(nom="Default"):
    """Profil créé quand aucun n'existe (losettings.load_profiles)."""
    p = Profile()
    p.Name = nom
    p.FileTemplate = MODELE_FICHIER_DEFAUT
    p.FolderTemplate = MODELE_DOSSIER_DEFAUT
    p.Version = VERSION
    return p


def _booleen_net(texte):
    t = (texte or "").strip().lower()
    if t in ("true", "1"):
        return True
    if t in ("false", "0"):
        return False
    raise ValueError("booléen illisible : %r" % texte)


def _nombre_net(x):
    if isinstance(x, float) and x.is_integer():
        return str(int(x))
    return str(x)


def lire_profils_xml(donnees):
    """Profils d'un fichier de Library Organizer (``losettingsx.dat`` ou ``.lop`` exporté,
    format 2.x ``Profiles/Profile`` ou 1.7 ``Settings/Setting``).

    ``donnees`` : octets du fichier. Rend ``(profils, derniers, erreurs)`` : dictionnaire
    nom → Profile, liste des profils utilisés en dernier, messages d'erreur."""
    if donnees.startswith(codecs.BOM_UTF8):
        donnees = donnees[len(codecs.BOM_UTF8):]
    try:
        racine = ET.fromstring(donnees)
    except ET.ParseError as ex:
        return {}, [], ["fichier XML illisible : %s" % ex]
    if racine.tag in ("Profiles", "Settings"):
        noeuds = racine.findall("Profile" if racine.tag == "Profiles" else "Setting")
    elif racine.tag in ("Profile", "Setting"):
        noeuds = [racine]
    else:
        return {}, [], ["ce n'est pas un fichier de profils de Library Organizer (élément racine « %s »)"
                        % racine.tag]
    profils = {}
    erreurs = []
    for n in noeuds:
        p = Profile()
        p.Name = n.get("Name") or ""
        if not p.load_from_xml(n) or not p.Name:
            erreurs.append("profil « %s » illisible, ignoré%s" % (p.Name, " : " + getattr(p, "erreur", "")
                                                                 if getattr(p, "erreur", "") else ""))
            continue
        profils[p.Name] = p
    derniers = [x for x in (racine.get("LastUsed") or "").split(",") if x]
    return profils, derniers, erreurs


def ecrire_profils_xml(profils, derniers=None):
    """Octets d'un fichier ``Profiles`` (même format que losettingsx.dat)."""
    racine = ET.Element("Profiles")
    if derniers:
        racine.set("LastUsed", ",".join(derniers))
    for p in profils.values():
        p.to_xml(racine)
    return _xml_octets(racine)


def ecrire_profil_xml(profil):
    """Octets d'un profil exporté seul (racine ``Profile``, comme losettings.save_profile)."""
    return _xml_octets(profil.to_xml())


def _xml_octets(racine):
    if hasattr(ET, "indent"):
        ET.indent(racine, space="  ")
    corps = ET.tostring(racine, encoding="unicode")
    return codecs.BOM_UTF8 + ('<?xml version="1.0" encoding="utf-8"?>\n' + corps).encode("utf-8")


# ---------------------------------------------------------------------------------------
# Livres : vue « ComicRack » d'une fiche ComicRust
# ---------------------------------------------------------------------------------------

def _texte(v):
    return "" if v is None else str(v)


def texte_net(v):
    """``unicode(valeur)`` de l'original : entier → texte, réel entier sans « .0 »."""
    if v is None:
        return ""
    if isinstance(v, bool):
        return "True" if v else "False"
    if isinstance(v, float):
        return str(int(v)) if v.is_integer() else repr(v)
    return str(v)


def _entier(v, defaut=-1):
    if v is None or v == "":
        return defaut
    try:
        return int(v)
    except (TypeError, ValueError):
        try:
            return int(float(v))
        except (TypeError, ValueError):
            return defaut


def depuis_nom(chemin):
    """Série, numéro, volume, année déduits du nom du fichier (valeurs « proposées » de
    ComicRack, utilisées quand la fiche est vide ; même règle que ranger_fichiers.py)."""
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


def valeurs_personnalisees(store):
    """Décode ``CustomValuesStore`` (``,clé=valeur,…``, ``&#61;`` = ``=``, ``&#44;`` = ``,``)."""
    out = []
    for seg in (store or "").split(","):
        parts = seg.split("=")
        if len(parts) == 2:
            dec = lambda t: t.replace("&#61;", "=").replace("&#44;", ",")
            out.append((dec(parts[0]), dec(parts[1])))
    return out


def lire_date(texte):
    """Date ISO de la fiche (``2013-02-01T10:20:30``…) → ``datetime`` ou None."""
    m = re.match(r"\s*(\d{4})-(\d{1,2})-(\d{1,2})(?:[T ](\d{1,2}):(\d{2})(?::(\d{2}))?)?", str(texte or ""))
    if not m:
        return None
    try:
        return datetime.datetime(int(m.group(1)), int(m.group(2)), int(m.group(3)),
                                 int(m.group(4) or 0), int(m.group(5) or 0), int(m.group(6) or 0))
    except ValueError:
        return None


# Noms des langues (``LanguageAsText`` de ComicRack : nom de la culture .NET ; ici en
# français, comme sous un Windows français).
LANGUES = {
    "ar": "arabe", "br": "breton", "ca": "catalan", "cs": "tchèque", "da": "danois",
    "de": "allemand", "el": "grec", "en": "anglais", "eo": "espéranto", "es": "espagnol",
    "et": "estonien", "eu": "basque", "fi": "finnois", "fr": "français", "ga": "irlandais",
    "he": "hébreu", "hr": "croate", "hu": "hongrois", "id": "indonésien", "is": "islandais",
    "it": "italien", "ja": "japonais", "ko": "coréen", "la": "latin", "lt": "lituanien",
    "lv": "letton", "nb": "norvégien bokmål", "nl": "néerlandais", "no": "norvégien",
    "oc": "occitan", "pl": "polonais", "pt": "portugais", "ro": "roumain", "ru": "russe",
    "sk": "slovaque", "sl": "slovène", "sr": "serbe", "sv": "suédois", "th": "thaï",
    "tr": "turc", "uk": "ukrainien", "vi": "vietnamien", "zh": "chinois",
}


class Livre(object):
    """Un livre vu par Library Organizer : champs ComicRack (``ShadowSeries``,
    ``FilePath``, ``ReadPercentage``…) calculés depuis la fiche ComicRust ``data``
    (dictionnaire aux noms ComicRack). ``objet`` : le ``comicrust.Book`` correspondant."""

    def __init__(self, data, objet=None):
        self.data = dict(data or {})
        self.objet = objet
        self.id = self.data.get("Id")
        if self.data.get("EnableProposed") is False:
            self._proposees = {}
        else:
            self._proposees = depuis_nom(self.data.get("File"))

    # Chemin
    @property
    def FilePath(self):
        return _texte(self.data.get("File"))

    def module_chemins(self, *autres):
        return module_chemins(self.FilePath, *autres)

    @property
    def FileDirectory(self):
        f = self.FilePath
        return self.module_chemins().dirname(f) if f else ""

    @property
    def FileNameWithExtension(self):
        f = self.FilePath
        return self.module_chemins().basename(f) if f else ""

    @property
    def Caption(self):
        s = self.get("ShadowSeries") or "?"
        n = self.get("ShadowNumber")
        return s + (" #" + n if n else "")

    def nom_rapport(self):
        return self.FilePath or self.Caption

    def custom_value(self, cle):
        for k, v in valeurs_personnalisees(self.data.get("CustomValuesStore")):
            if k.lower() == cle.lower():
                return v
        return None

    def oui_non(self, field):
        """Valeur Oui/Non : ``Yes``, ``No``, ``YesAndRightToLeft`` ou ``Unknown``."""
        v = _texte(self.data.get(field)).strip()
        return v or "Unknown"

    def get(self, field):
        """Valeur d'un champ comme ComicRack la donne (entier ``-1`` = vide, texte ``""``
        = vide, ``datetime`` pour les dates)."""
        d = self.data
        p = self._proposees
        if field == "FilePath":
            return self.FilePath
        if field == "FileName":
            return os.path.splitext(self.FileNameWithExtension)[0]
        if field == "FileFormat":
            return os.path.splitext(self.FileNameWithExtension)[1].lstrip(".").upper()
        if field == "ShadowSeries":
            return _texte(d.get("Series")) or p.get("series", "")
        if field == "ShadowNumber":
            return _texte(d.get("Number")) or p.get("number", "")
        if field == "ShadowVolume":
            v = _entier(d.get("Volume"))
            return v if v != -1 else _entier(p.get("volume"))
        if field == "ShadowYear":
            v = _entier(d.get("Year"))
            return v if v != -1 else _entier(p.get("year"))
        if field == "ShadowCount":
            return _entier(d.get("Count"))
        if field == "ShadowTitle":
            return _texte(d.get("Title"))
        if field == "ShadowFormat":
            return _texte(d.get("Format"))
        if field in ("Month", "Day", "AlternateCount"):
            return _entier(d.get(field))
        if field in ("Manga", "SeriesComplete", "BlackAndWhite"):
            return self.oui_non(field)
        if field in ("Rating", "CommunityRating"):
            try:
                return float(d.get(field) or 0.0)
            except (TypeError, ValueError):
                return 0.0
        if field == "ReadPercentage":
            lu, total = _entier(d.get("LastPageRead"), 0), _entier(d.get("PageCount"), 0)
            if total <= 0 or lu <= 0:
                return 0
            return max(1, min(100, ((lu + 1) * 100) // total))
        if field == "AddedTime":
            return lire_date(d.get("Added"))
        if field == "ReleasedTime":
            return lire_date(d.get("Released"))
        if field == "LanguageAsText":
            iso = _texte(d.get("LanguageISO")).strip()
            return LANGUES.get(iso.lower().split("-")[0], iso) if iso else ""
        return _texte(d.get(field))

    def __repr__(self):
        return "<Livre %s %s>" % (self.id, self.FilePath)


def module_chemins(*chemins):
    """``ntpath`` pour un chemin Windows (lettre de lecteur, partage ``\\\\serveur``,
    barre oblique inversée) ou sous Windows, ``posixpath`` sinon."""
    if os.name == "nt":
        return ntpath
    for c in chemins:
        c = str(c or "")
        if "\\" in c or re.match(r"^[A-Za-z]:", c):
            return ntpath
    return posixpath


def cle_chemin(chemin, p):
    """Clé de comparaison de chemins (casse ignorée sous Windows, ``..`` résolus)."""
    if not chemin:
        return ""
    return p.normcase(p.normpath(chemin))


def combiner(p, a, b):
    """``Path.Combine`` de .NET : ``b`` vide → ``a`` ; ``b`` enraciné → ``b``."""
    if not b:
        return a
    if not a:
        return b
    if b.startswith(("\\", "/")) or (p is ntpath and ntpath.splitdrive(b)[0]):
        return b
    if a.endswith(("\\", "/")) or (p is ntpath and a.endswith(":")):
        return a + b
    return a + ("\\" if p is ntpath else "/") + b


def chemin_enracine(chemin, p):
    """Chemin complet (lecteur ou partage sous Windows, ``/`` ailleurs)."""
    if p is ntpath:
        lecteur = ntpath.splitdrive(chemin)[0]
        return bool(lecteur) and (lecteur.startswith("\\\\") or chemin[len(lecteur):].startswith(("\\", "/")))
    return chemin.startswith("/")


# ---------------------------------------------------------------------------------------
# Calculs sur la série (locommon.get_earliest_book / get_last_book)
# ---------------------------------------------------------------------------------------

class Calculs(object):
    """Premier et dernier numéro d'une série parmi **toute la bibliothèque**, comme
    l'original (``GetLibraryBooks()``). ``livres`` : liste, ou fonction sans argument qui la
    rend (appelée à la première demande seulement : une bibliothèque n'est lue que si un
    modèle utilise une valeur calculée)."""

    def __init__(self, livres):
        self._source = livres
        self._livres = None
        self._debut = {}
        self._fin = {}

    @property
    def livres(self):
        if self._livres is None:
            src = self._source
            self._livres = list(src() if callable(src) else (src or []))
        return self._livres

    @staticmethod
    def _cle(b):
        return b.get("Publisher") + b.get("ShadowSeries") + str(b.get("ShadowVolume"))

    def _meme_serie(self, livre):
        s, v, e = livre.get("ShadowSeries"), livre.get("ShadowVolume"), livre.get("Publisher")
        tous = self.livres if any(x is livre for x in self.livres) else self.livres + [livre]
        return [b for b in tous
                if b.get("ShadowSeries") == s and b.get("ShadowVolume") == v and b.get("Publisher") == e]

    def premier_livre(self, livre):
        index = self._cle(livre)
        if index in self._debut:
            return self._debut[index]
        debut = livre
        for b in self._meme_serie(livre):
            # Reprise fidèle de l'original (année vide = -1, mois vide = -1).
            if debut.get("ShadowYear") == -1 and b.get("ShadowYear") != 1:
                debut = b
            if b.get("ShadowYear") != -1 and b.get("ShadowYear") < debut.get("ShadowYear"):
                debut = b
            if b.get("ShadowYear") == debut.get("ShadowYear") and b.get("Month") != -1:
                if debut.get("Month") == -1:
                    debut = b
                elif b.get("Month") < debut.get("Month"):
                    debut = b
                elif b.get("Month") == debut.get("Month"):
                    if b.get("ShadowNumber") < debut.get("ShadowNumber"):
                        debut = b
        self._debut[index] = debut
        return debut

    def dernier_livre(self, livre):
        index = self._cle(livre)
        if index in self._fin:
            return self._fin[index]
        fin = livre
        for b in self._meme_serie(livre):
            if not fin.get("ShadowNumber").isdigit():
                fin = b
                continue
            if b.get("ShadowNumber").isdigit() and int(fin.get("ShadowNumber")) < int(b.get("ShadowNumber")):
                fin = b
        self._fin[index] = fin
        return fin

    def valeurs_serie(self, livre, field):
        """Toutes les valeurs d'un champ multiple dans la série (sans doublon)."""
        out = []
        for b in self._meme_serie(livre):
            for v in b.get(field).split(","):
                v = v.strip()
                if v and v not in out:
                    out.append(v)
        return out


# ---------------------------------------------------------------------------------------
# Dates (formats .NET, culture française)
# ---------------------------------------------------------------------------------------

MOIS_FR = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août",
           "septembre", "octobre", "novembre", "décembre"]
MOIS_FR_ABR = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.",
               "oct.", "nov.", "déc."]
JOURS_FR = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"]
JOURS_FR_ABR = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."]

FORMATS_STANDARD = {
    "d": "dd/MM/yyyy", "D": "dddd d MMMM yyyy", "f": "dddd d MMMM yyyy HH:mm",
    "F": "dddd d MMMM yyyy HH:mm:ss", "g": "dd/MM/yyyy HH:mm", "G": "dd/MM/yyyy HH:mm:ss",
    "M": "d MMMM", "m": "d MMMM", "Y": "MMMM yyyy", "y": "MMMM yyyy", "t": "HH:mm",
    "T": "HH:mm:ss", "s": "yyyy'-'MM'-'dd'T'HH':'mm':'ss", "u": "yyyy'-'MM'-'dd HH':'mm':'ss'Z'",
    "o": "yyyy'-'MM'-'dd'T'HH':'mm':'ss.fffffff", "O": "yyyy'-'MM'-'dd'T'HH':'mm':'ss.fffffff",
}

# Formats proposés dans la configuration (configformcontrols.InsertControlDateTime).
FORMATS_DATE = ["dd/MM/yy", "MM/dd/yy", "D", "M", "Y", "MMM d, yyy", "MMM dd, yyy",
                "MMMM d, yyy", "MMMM dd, yyy"]


def format_date(dt, fmt):
    """``DateTime.ToString(fmt)`` de .NET (culture française)."""
    if not fmt:
        fmt = "G"
    if len(fmt) == 1:
        fmt = FORMATS_STANDARD.get(fmt, fmt)
    out = []
    i = 0
    n = len(fmt)
    while i < n:
        c = fmt[i]
        if c in "'\"":
            j = fmt.find(c, i + 1)
            if j < 0:
                j = n
            out.append(fmt[i + 1:j])
            i = j + 1
            continue
        if c == "\\":
            if i + 1 < n:
                out.append(fmt[i + 1])
            i += 2
            continue
        if c == "%":
            i += 1
            continue
        j = i
        while j < n and fmt[j] == c:
            j += 1
        k = j - i
        if c == "d":
            out.append(str(dt.day) if k == 1 else "%02d" % dt.day if k == 2
                       else JOURS_FR_ABR[dt.weekday()] if k == 3 else JOURS_FR[dt.weekday()])
        elif c == "M":
            out.append(str(dt.month) if k == 1 else "%02d" % dt.month if k == 2
                       else MOIS_FR_ABR[dt.month - 1] if k == 3 else MOIS_FR[dt.month - 1])
        elif c == "y":
            out.append(str(dt.year % 100) if k == 1 else "%02d" % (dt.year % 100) if k == 2
                       else str(dt.year).zfill(k))
        elif c == "H":
            out.append(str(dt.hour) if k == 1 else "%02d" % dt.hour)
        elif c == "h":
            h = dt.hour % 12 or 12
            out.append(str(h) if k == 1 else "%02d" % h)
        elif c == "m":
            out.append(str(dt.minute) if k == 1 else "%02d" % dt.minute)
        elif c == "s":
            out.append(str(dt.second) if k == 1 else "%02d" % dt.second)
        elif c in "fF":
            out.append(("%06d" % dt.microsecond + "0")[:k])
        elif c == "t":
            pass  # pas d'indicateur AM/PM en français
        elif c == "z":
            out.append("+00" if k < 3 else "+00:00")
        elif c == "K":
            pass
        else:
            out.append(c * k)
        i = j
    return "".join(out)


# ---------------------------------------------------------------------------------------
# Modèles (lobookmover.PathMaker)
# ---------------------------------------------------------------------------------------

class ResultatSelection(object):
    """Choix de valeurs d'un champ multiple (loforms.MultiValueSelectionFormResult)."""

    def __init__(self, selection, every=False, folder=False, alwaysuse=False, dontask=False):
        self.Selection = list(selection)
        self.EveryIssue = every
        self.Folder = folder
        self.AlwaysUse = alwaysuse
        self.AlwaysUseDontAsk = dontask


class ValeursToujoursUtilisees(list):
    def __init__(self, do_not_ask=False, folder=False, values=None):
        list.__init__(self, values or [])
        self.do_not_ask = do_not_ask
        self.use_folder_seperator = folder


class Interface(object):
    """Questions posées pendant le calcul (par défaut, sans interface : comme si
    l'utilisateur fermait chaque fenêtre)."""

    def choisir_valeurs(self, valeurs, selection, champ, texte_livre, serie):
        """Fenêtre de choix des valeurs d'un champ multiple ; ``ResultatSelection`` ou None."""
        return None

    def raccourcir_chemin(self, chemin):
        """Chemin trop long : chemin raccourci par l'utilisateur, ou None (ignorer)."""
        return None

    def doublon(self, profil, livre, ancien, chemin, nom_renomme, restants):
        """Fichier déjà présent : ``("renommer"|"annuler", pour_tous)``."""
        return ("annuler", False)


class PathMaker(object):
    """Calcule le dossier et le nom d'un livre d'après les modèles d'un profil."""

    template_to_field = {
        "series": "ShadowSeries", "number": "ShadowNumber", "count": "ShadowCount", "Day": "Day",
        "ReleasedDate": "ReleasedTime", "AddedDate": "AddedTime", "EndYear": "EndYear",
        "EndMonth": "EndMonth", "EndMonth#": "EndMonth", "month": "Month", "month#": "Month",
        "year": "ShadowYear", "imprint": "Imprint", "publisher": "Publisher",
        "altSeries": "AlternateSeries", "altNumber": "AlternateNumber", "altCount": "AlternateCount",
        "volume": "ShadowVolume", "title": "ShadowTitle", "ageRating": "AgeRating",
        "language": "LanguageAsText", "format": "ShadowFormat", "startyear": "StartYear",
        "writer": "Writer", "tags": "Tags", "genre": "Genre", "characters": "Characters",
        "teams": "Teams", "scaninfo": "ScanInformation", "manga": "Manga",
        "seriesComplete": "SeriesComplete", "first": "FirstLetter", "read": "ReadPercentage",
        "counter": "Counter", "startmonth": "StartMonth", "startmonth#": "StartMonth",
        "colorist": "Colorist", "coverartist": "CoverArtist", "editor": "Editor", "inker": "Inker",
        "letterer": "Letterer", "locations": "Locations", "penciller": "Penciller",
        "storyarc": "StoryArc", "seriesgroup": "SeriesGroup", "maincharacter": "MainCharacterOrTeam",
        "firstissuenumber": "FirstIssueNumber", "lastissuenumber": "LastIssueNumber",
        "Rating": "Rating", "CommunityRating": "CommunityRating", "Custom": "Custom",
    }

    template_regex = re.compile(
        r"{(?P<prefix>[^{}<]*)<(?P<name>[^\d\s(>]*)(?P<args>\d*|(?:\([^){}]*\))*)>(?P<postfix>[^{}]*)}")

    yes_no_fields = ["Manga", "SeriesComplete"]

    def __init__(self, profile, calculs, interface=None):
        self.profile = profile
        self.calculs = calculs
        self.interface = interface or Interface()
        self._counter = None
        self.failed_fields = []
        self.failed = False
        self.book = None
        self.p = posixpath
        self._multi = {}
        self._toujours = {}

    # --- chemin complet ---

    def make_path(self, book, folder_template, file_template):
        """``(dossier, nom de fichier, échec)`` ; échec = champ obligatoire vide."""
        self.book = book
        self.p = book.module_chemins(self.profile.BaseFolder, self.profile.FailedFolder)
        self.failed = False
        self.failed_fields = []
        file_path = book.FileNameWithExtension
        if self.profile.UseFileName:
            file_path = self.make_file_name(file_template)
        folder_path = book.FileDirectory
        if self.profile.UseFolder:
            folder_path = self.make_folder_path(folder_template)
        if self.failed and not self.profile.MoveFailed:
            return book.FileDirectory, book.FileNameWithExtension, True
        return folder_path, file_path, self.failed

    def make_folder_path(self, template):
        folder_path = ""
        template = template.strip().strip("\\")
        if template:
            rough_path = self.insert_fields_into_template(template)
            for line in rough_path.split("\\"):
                if not line.strip():
                    line = self.profile.EmptyFolder
                line = self.replace_illegal_characters(line)
                line = line.strip(".")
                folder_path = combiner(self.p, folder_path, line.strip())
        if self.failed and self.profile.MoveFailed:
            folder_path = combiner(self.p, self.profile.FailedFolder, folder_path)
        else:
            folder_path = combiner(self.p, self.profile.BaseFolder, folder_path)
        if self.profile.ReplaceMultipleSpaces:
            folder_path = re.sub(r"\s\s+", " ", folder_path)
        return folder_path

    def make_file_name(self, template):
        file_name = self.insert_fields_into_template(template)
        file_name = file_name.strip()
        file_name = self.replace_illegal_characters(file_name)
        if not file_name:
            return ""
        extension = self.profile.FilelessFormat
        if self.book.FilePath:
            extension = self.p.splitext(self.book.FilePath)[1]
        if self.profile.ReplaceMultipleSpaces:
            file_name = re.sub(r"\s\s+", " ", file_name)
        return file_name + extension

    # --- jetons ---

    def insert_fields_into_template(self, template):
        """Remplace les jetons ``{préfixe<champ…>suffixe}``, les plus intérieurs d'abord,
        jusqu'à ce qu'il n'en reste que d'invalides (laissés tels quels)."""
        self.invalid = 0
        while self.template_regex.search(template):
            if self.invalid == len(self.template_regex.findall(template)):
                break
            self.invalid = 0
            template = self.template_regex.sub(self.insert_field, template)
        return template

    def insert_field(self, match):
        g = match.groupdict()
        conditional = False
        inversion = False
        inversion_args = ""
        conditional_args = ""
        name = g["name"]
        args = g["args"]

        # Inversion : <!champ>, <!champ(texte)>, <!champ(!regex)>
        if name.startswith("!"):
            inversion = True
            name = name.lstrip("!")
            if args:
                r = re.search(r"(\([^(]*\))$", args)
                if r is not None:
                    args = args[:-len(r.group(0))]
                    inversion_args = r.group(0)[1:-1]

        # Conditionnel : <?champ(texte)>, <?champ(!regex)>
        if name.startswith("?"):
            conditional = True
            name = name.lstrip("?")
            if not args:
                self.invalid += 1
                return match.group(0)
            r = re.search(r"(\([^(]*\))$", args)
            if r is None:
                self.invalid += 1
                return match.group(0)
            args = args[:-len(r.group(0))]
            conditional_args = r.group(0)[1:-1]

        if name not in self.template_to_field:
            self.invalid += 1
            return match.group(0)
        field = self.template_to_field[name]

        result = self.get_field_text(field, name, args)
        if result is None:
            self.invalid += 1
            return match.group(0)

        try:
            if conditional:
                if conditional_args.startswith("!"):
                    if conditional_args[1:] == "":
                        return ""
                    if re.match(conditional_args[1:], result) is not None:
                        return g["prefix"] + g["postfix"]
                    return ""
                if result == conditional_args:
                    return g["prefix"] + g["postfix"]
                return ""

            if inversion:
                if not inversion_args:
                    return g["prefix"] + g["postfix"] if not result else ""
                if inversion_args.startswith("!"):
                    if re.match(inversion_args[1:], result) is None:
                        return g["prefix"] + g["postfix"]
                    return ""
                return g["prefix"] + g["postfix"] if result != inversion_args else ""
        except re.error:
            self.invalid += 1
            return match.group(0)

        if not result:
            if self.profile.FailEmptyValues and field in self.profile.FailedFields:
                if field not in self.failed_fields:
                    self.failed_fields.append(field)
                self.failed = True
            if self.profile.EmptyData.get(field):
                return self.profile.EmptyData[field]
            return ""

        return g["prefix"] + result + g["postfix"]

    def get_field_text(self, field, template_name, args_match):
        """Texte d'un champ ou d'un calcul ; None si invalide (nombre d'arguments…)."""
        args = re.findall(r"\(([^)]*)\)", args_match)
        try:
            if field in ("StartYear", "StartMonth", "EndYear", "EndMonth"):
                return self.insert_start_value(field, args_match, template_name)
            elif field == "ReadPercentage" and len(args) == 3:
                return self.insert_read_percentage(args)
            elif field == "FirstLetter":
                if len(args) == 0:
                    return None
                if args[0] in NAME_TO_FIELD or args[0] in FIELD_TO_NAME:
                    return self.insert_first_letter(args[0])
                return None
            elif field == "Counter" and len(args) == 3:
                return self.insert_counter(args)
            elif field == "FirstIssueNumber":
                return self.insert_first_issue_number(args_match)
            elif field == "LastIssueNumber":
                return self.insert_last_issue_number(args_match)
            elif field == "Custom":
                if len(args) == 0:
                    return None
                return self.insert_custom_value(args[0])
            elif field in ("AddedTime", "ReleasedTime"):
                return self.insert_formated_datetime(args[0] if args else "", field)
            elif field in self.yes_no_fields and 0 < len(args) < 3:
                return self.insert_yes_no_field(field, args)
            elif len(args) == 2:
                return self.insert_multi_value_field(field, args)
            elif not args_match:
                return self.insert_text_field(field, template_name)
            elif args_match.isdigit():
                return self.insert_number_field(field, int(args_match))
            return None
        except (AttributeError, ValueError):
            return None

    def insert_text_field(self, field, template_name):
        if field == "Month" and not template_name.endswith("#"):
            return self.insert_month_as_name()
        text = self.book.get(field)
        if field in ("Manga", "SeriesComplete", "BlackAndWhite") and text == "Unknown":
            return ""
        if not text or text == -1:
            return ""
        return self.replace_illegal_characters(texte_net(text))

    def insert_number_field(self, field, padding):
        number = self.book.get(field)
        if number == -1 or number == "" or number is None:
            return ""
        if padding == 0:
            value = self.calculs.dernier_livre(self.book).get(field)
            padding = len(texte_net(value))
        return self.replace_illegal_characters(self.pad(number, padding))

    def insert_yes_no_field(self, field, args):
        text = args[0]
        no = len(args) == 2 and args[1] == "!"
        value = self.book.get(field)
        result = ""
        if not no and value in ("Yes", "YesAndRightToLeft"):
            result = text
        elif no and value == "No":
            result = text
        return self.replace_illegal_characters(result)

    def insert_read_percentage(self, args):
        text, operator, percent = args[0], args[1], args[2]
        lu = self.book.get("ReadPercentage")
        result = ""
        if operator == "=" and lu == int(percent):
            result = text
        elif operator == ">" and lu > int(percent):
            result = text
        elif operator == "<" and lu < int(percent):
            result = text
        return self.replace_illegal_characters(result)

    ARTICLES = re.compile(
        r"(?:(?:the|a|an|de|het|een|die|der|das|des|dem|der|ein|eines|einer|einen|la|le|l'|les|un|une|el|"
        r"las|los|las|un|una|unos|unas|o|os|um|uma|uns|umas|en|et|il|lo|uno|gli)\s+)?(?P<letter>.).+",
        re.I)

    def insert_first_letter(self, field):
        if field in NAME_TO_FIELD:
            field = NAME_TO_FIELD[field]
        prop = texte_net(self.book.get(field))
        m = self.ARTICLES.match(prop)
        result = m.group("letter").capitalize() if m else ""
        return self.replace_illegal_characters(result)

    def insert_counter(self, args):
        start = int(args[0])
        increment = int(args[1])
        pad = int(args[2]) if args[2] != "" else 0
        if self._counter is None:
            self._counter = start
        else:
            self._counter = self._counter + increment
        return self.pad(self._counter, pad)

    def insert_month_as_name(self):
        n = self.book.get("Month")
        if n in self.profile.Months:
            return self.replace_illegal_characters(self.profile.Months[n])
        return ""

    def insert_start_value(self, field, args, template_name):
        if field in ("StartYear", "EndYear"):
            return self.insert_start_year(field == "EndYear")
        return self.insert_start_month(args, template_name, field == "EndMonth")

    def insert_start_year(self, end=False):
        livre = self.calculs.dernier_livre(self.book) if end else self.calculs.premier_livre(self.book)
        year = livre.get("ShadowYear")
        if year == -1:
            return ""
        return self.replace_illegal_characters(texte_net(year))

    def insert_start_month(self, args, template_name, end=False):
        livre = self.calculs.dernier_livre(self.book) if end else self.calculs.premier_livre(self.book)
        month = livre.get("Month")
        if month == -1:
            return ""
        if template_name.endswith("#"):
            if args and args.isdigit():
                month = self.pad(month, int(args))
            return self.replace_illegal_characters(texte_net(month))
        if month in self.profile.Months:
            return self.replace_illegal_characters(self.profile.Months[month])
        return ""

    def insert_formated_datetime(self, time_format, field):
        dt = self.book.get(field)
        if dt is None:
            # Date absente : texte vide (ComicRack aurait écrit la date minimale 01/01/0001).
            return ""
        return format_date(dt, time_format)

    def insert_custom_value(self, key):
        r = self.book.custom_value(key)
        return "" if r is None else r

    def insert_first_issue_number(self, padding):
        number = self.calculs.premier_livre(self.book).get("ShadowNumber")
        if padding is not None and padding.isdigit():
            return self.replace_illegal_characters(self.pad(number, int(padding)))
        return self.replace_illegal_characters(number)

    def insert_last_issue_number(self, padding):
        number = self.calculs.dernier_livre(self.book).get("ShadowNumber")
        if padding is not None and padding.isdigit():
            return self.replace_illegal_characters(self.pad(number, int(padding)))
        return self.replace_illegal_characters(number)

    # --- champs multiples ---

    def insert_multi_value_field(self, field, args):
        seperator, mode = args[0], args[1]
        if mode == "series":
            return self.insert_multi_value_series(field, seperator)
        if mode == "issue":
            return self.insert_multi_value_issue(field, seperator)
        return None

    def _texte_livre(self, avec_numero):
        b = self.book
        t = b.get("ShadowSeries") + " vol. " + texte_net(b.get("ShadowVolume"))
        return t + " #" + b.get("ShadowNumber") if avec_numero else t

    def insert_multi_value_issue(self, field, seperator):
        field_dict = self._multi.setdefault(field, {})
        b = self.book
        index = (b.get("Publisher") + b.get("ShadowSeries") + texte_net(b.get("ShadowVolume"))
                 + b.get("ShadowNumber"))
        if index in field_dict:
            r = field_dict[index]
            return self.make_multi_value_issue_string(r.Selection, seperator, r.Folder)
        if not b.get(field).strip():
            field_dict[index] = ResultatSelection([])
            return ""
        always_used_values = self._toujours.setdefault(field, [])
        values = [v.strip() for v in b.get(field).split(",")]
        if len(values) == 1 and self.profile.DontAskWhenMultiOne:
            return self.replace_illegal_characters(values[0])
        selected_values = []
        for liste in sorted(always_used_values, key=len, reverse=True):
            if all(v in values for v in liste):
                if liste.do_not_ask:
                    return self.make_multi_value_issue_string(liste, seperator, liste.use_folder_seperator)
                selected_values = liste[:]
                break
        result = self.interface.choisir_valeurs(values, selected_values, field, self._texte_livre(True), False)
        if result is None:
            result = ResultatSelection([])
        field_dict[index] = result
        if len(result.Selection) == 0:
            return ""
        if result.AlwaysUse:
            always_used_values.append(ValeursToujoursUtilisees(result.AlwaysUseDontAsk, result.Folder,
                                                               result.Selection))
        return self.make_multi_value_issue_string(result.Selection, seperator, result.Folder)

    def make_multi_value_issue_string(self, values, seperator, usefolder):
        if usefolder:
            seperator = self.replace_illegal_characters(seperator) + "\\"
            return seperator.join(self.replace_illegal_characters(v) for v in values)
        return self.replace_illegal_characters(seperator.join(values))

    def insert_multi_value_series(self, field, seperator):
        field_dict = self._multi.setdefault(field, {})
        b = self.book
        index = b.get("Publisher") + b.get("ShadowSeries") + texte_net(b.get("ShadowVolume"))
        if index in field_dict:
            return self.make_multi_value_series_string(field_dict[index], seperator, field)
        values = self.calculs.valeurs_serie(b, field)
        if not values:
            field_dict[index] = ResultatSelection([])
            return ""
        result = self.interface.choisir_valeurs(values, [], field, self._texte_livre(False), True)
        if result is None:
            result = ResultatSelection([])
        field_dict[index] = result
        return self.make_multi_value_series_string(result, seperator, field)

    def make_multi_value_series_string(self, sel, seperator, field):
        if sel.Folder:
            seperator = self.replace_illegal_characters(seperator) + "\\"
            values = [self.replace_illegal_characters(v.strip()) for v in self.book.get(field).split(",")]
            selection = [self.replace_illegal_characters(v) for v in sel.Selection]
        else:
            values = [v.strip() for v in self.book.get(field).split(",")]
            selection = sel.Selection
        if sel.EveryIssue:
            result = seperator.join(selection)
        else:
            result = seperator.join(i for i in selection if i in values)
        if sel.Folder:
            return result
        return self.replace_illegal_characters(result)

    # --- utilitaires ---

    def replace_illegal_characters(self, text):
        for c in sorted(self.profile.IllegalCharacters.keys(), key=len, reverse=True):
            if c:
                text = text.replace(c, self.profile.IllegalCharacters[c])
        return text

    @staticmethod
    def pad(value, padding):
        """Complète la partie entière par des zéros (``7.1`` → ``007.1`` sur 3)."""
        remainder = ""
        try:
            number = float(value)
        except (TypeError, ValueError):
            return value
        value = texte_net(value) if not isinstance(value, str) else value
        if number >= 0:
            if "." in value:
                value, remainder = value.split(".", 1)
                remainder = "." + remainder
            return value.rjust(padding, "0") + remainder
        value = value[1:]
        if "." in value:
            value, remainder = value.split(".", 1)
            remainder = "." + remainder
        return "-" + value.rjust(padding, "0") + remainder


# ---------------------------------------------------------------------------------------
# Règles (locommon.check_metadata_rules / check_excluded_folders)
# ---------------------------------------------------------------------------------------

def check_metadata_rules(book, profile, calculs):
    """Vrai si le livre doit être traité par le profil."""
    count = total = 0
    for rule in profile.ExcludeRules:
        result = rule.book_should_be_moved(book, calculs)
        if result is None:
            continue
        count += result
        total += 1
    if total == 0:
        return True
    qualifies = (count > 0) if profile.ExcludeOperator == "Any" else (count == total)
    if profile.ExcludeMode == "Only":
        return qualifies
    return not qualifies


def check_excluded_folders(book_path, profile):
    """Faux si le livre est dans un dossier exclu (le texte du dossier figure dans son
    chemin, comme dans l'original)."""
    for path in profile.ExcludeFolders:
        if path and path in (book_path or ""):
            return False
    return True


# ---------------------------------------------------------------------------------------
# Journal (lologger.Logger) et annulation (locommon.UndoCollection)
# ---------------------------------------------------------------------------------------

class Logger(object):
    def __init__(self):
        self._log = []
        self._profile = ""
        self.header = ""

    def Add(self, action, path, message="", profile=""):
        self._log.append([profile or self._profile, str(action), str(path), str(message)])

    def SetProfile(self, profile):
        self._profile = profile

    def ToArray(self):
        return self._log

    def add_header(self, header_text):
        self.header += header_text

    def texte(self):
        """Contenu du fichier de journal (même forme que lologger.SaveLog)."""
        parts = ["Rapport de Library Organizer :\n\n" + self.header]
        for profil, action, chemin, message in self._log:
            parts.append("\n\n%s :\n%s : %s" % (profil, action, chemin))
            if message:
                parts.append("\nMessage : " + message)
        return "".join(parts) + "\n"


class UndoCollection(object):
    """Déplacements faits (demandés) : ancien chemin, nouveau chemin, profil."""

    def __init__(self):
        self._undo_paths = []
        self._current_paths = []
        self._profiles = []

    def __len__(self):
        return len(self._undo_paths)

    def append(self, undo_path, new_path, profile_name):
        if undo_path in self._current_paths:
            self._current_paths[self._current_paths.index(undo_path)] = new_path
        else:
            self._undo_paths.append(undo_path)
            self._current_paths.append(new_path)
            self._profiles.append(profile_name)

    def entries(self):
        return list(zip(self._profiles, self._current_paths, self._undo_paths))

    def to_json(self):
        """Forme enregistrée (JSON : aucun caractère à échapper, même « | » dans un nom
        de profil ou un chemin)."""
        return [{"profil": p, "nouveau": c, "ancien": a} for p, c, a in self.entries()]

    @classmethod
    def depuis_renommages(cls, renommages, profils=None):
        """Annulation des renommages **appliqués** par ComicRust (``applied_renames()`` :
        ``[{"from", "to"}]``). ``profils`` : rangement demandé (:meth:`to_json`), pour
        retrouver le profil de chaque déplacement (même ancien et nouveau chemin)."""
        index = {}
        for e in profils or []:
            p = module_chemins(e.get("ancien"), e.get("nouveau"))
            index[(cle_chemin(e.get("ancien"), p), cle_chemin(e.get("nouveau"), p))] = e.get("profil") or ""
        u = cls()
        for r in renommages or []:
            ancien, nouveau = r.get("from") or "", r.get("to") or ""
            if not ancien or not nouveau:
                continue
            p = module_chemins(ancien, nouveau)
            u.append(ancien, nouveau, index.get((cle_chemin(ancien, p), cle_chemin(nouveau, p)), ""))
        return u


# ---------------------------------------------------------------------------------------
# Rangement (lobookmover.BookMover)
# ---------------------------------------------------------------------------------------

class MoveResult(object):
    Success = 1
    Failed = 2
    Skipped = 3
    Duplicate = 4


class BookToMove(object):
    def __init__(self, book, path, index, failed_fields):
        self.book = book
        self.path = path
        self.profile_index = index
        self.failed_fields = failed_fields


def verbe_mode(mode, temps="passe"):
    if mode == Mode.Copy:
        return "copié (simulé : copie impossible dans ComicRust)" if temps == "passe" else "copie"
    if mode == Mode.Move:
        return "déplacement demandé" if temps == "passe" else "déplacement"
    return "déplacé (simulé)" if temps == "passe" else "déplacement (simulé)"


class ProfileReport(object):
    def __init__(self, total, name, mode):
        self.success = 0
        self.failed = 0
        self.skipped = 0
        self._total = total
        self._name = name
        self._mode = mode

    def get_report(self, cancelled=True):
        if cancelled:
            self.skipped = self._total - self.success - self.failed
        titre = {Mode.Move: "Déplacements demandés", Mode.Copy: "Copiés (simulé)",
                 Mode.Simulate: "Déplacés (simulé)"}.get(self._mode, "Traités")
        return "%s :\n%s : %s\tIgnorés : %s\tÉchecs : %s" % (self._name, titre, self.success,
                                                             self.skipped, self.failed)

    def to_json(self):
        return {"mode": self._mode, "succes": self.success, "ignores": self.skipped, "echecs": self.failed}


class BookMover(object):
    """Calcule les déplacements de livres pour une suite de profils et les demande.

    ``renommer(livre, chemin)`` : rappel qui met en attente le renommage (ComicRust).
    ``existe(chemin)`` : présence d'un fichier (``os.path.lexists`` par défaut).
    """

    def __init__(self, logger, interface=None, renommer=None, tous_les_livres=None,
                 annule=None, progression=None, existe=None):
        self.logger = logger
        self.interface = interface or Interface()
        self.renommer = renommer or (lambda livre, chemin: None)
        self.annule = annule or (lambda: False)
        self.progression = progression or (lambda fraction, texte: None)
        self.existe = existe or os.path.lexists
        # Toute la bibliothèque (liste, ou fonction appelée à la première demande).
        self._tous_source = tous_les_livres
        self._tous = None
        self.calculs = Calculs(lambda: self.tous)
        self.pathmaker = None
        self.failed_or_skipped = False
        self.HeldDuplicateBooks = []
        self.HeldDuplicateCount = 0
        self.CreatedPaths = []
        self.MovedBooks = set()
        self.undo_collection = UndoCollection()
        self.always_do_duplicate_action = False
        self.duplicate_action = None
        self.deplacements = []        # (livre, ancien chemin, nouveau chemin, profil)
        self.dossiers_retirer = []    # (ancien dossier, profil) pour les dossiers vides
        self.profile = None
        self.profile_reports = []
        self.annulation = False

    @property
    def tous(self):
        if self._tous is None:
            src = self._tous_source
            self._tous = list(src() if callable(src) else (src or []))
        return self._tous

    # Les livres déjà traités par l'original via le système de fichiers : ici, les
    # destinations demandées pendant cette exécution comptent comme occupées.
    def _occupe(self, chemin, p):
        return self.existe(chemin) or cle_chemin(chemin, p) in self.MovedBooks

    def _p(self, *chemins):
        return module_chemins(*chemins)

    def create_book_paths(self, books, profiles):
        books_and_paths = []
        self.profile_reports = [ProfileReport(len(books), p.Name, p.Mode) for p in profiles]
        self.pathmakers = [PathMaker(p, self.calculs, self.interface) for p in profiles]
        total = max(len(books), 1)
        for n, book in enumerate(books):
            if self.annule():
                self.annulation = True
                break
            self.progression(0.5 * n / total, book.nom_rapport())
            path = ""
            profile_index = None
            failed_fields = []
            for index, profile in enumerate(profiles):
                self.profile = profile
                self.pathmaker = self.pathmakers[index]
                self.logger.SetProfile(profile.Name)
                result = self.create_book_path(book)
                if result is MoveResult.Skipped:
                    self.profile_reports[index].skipped += 1
                    self.failed_or_skipped = True
                    continue
                if result is MoveResult.Failed:
                    self.profile_reports[index].failed += 1
                    self.failed_or_skipped = True
                    continue
                if profile.Mode == Mode.Copy:
                    books_and_paths.append(BookToMove(book, result, index, self.pathmaker.failed_fields))
                    continue
                if path:
                    self.profile_reports[profile_index].skipped += 1
                    self.logger.Add("Ignoré", book.nom_rapport(), "Le livre est déplacé par un profil suivant",
                                    profiles[profile_index].Name)
                    self.failed_or_skipped = True
                path = result
                profile_index = index
                failed_fields = self.pathmaker.failed_fields
            if path:
                self.profile = profiles[profile_index]
                self.logger.SetProfile(self.profile.Name)
                result = self.check_path_problems(book, path)
                if result is MoveResult.Skipped:
                    self.profile_reports[profile_index].skipped += 1
                    self.failed_or_skipped = True
                else:
                    books_and_paths.append(BookToMove(book, path, profile_index, failed_fields))
        return books_and_paths

    def create_book_path(self, book):
        nom = book.nom_rapport()
        if book.FilePath and not self.existe(book.FilePath):
            self.logger.Add("Échec", nom, "Le fichier n'existe pas")
            return MoveResult.Failed
        if not self.book_should_be_moved_with_rules(book):
            return MoveResult.Skipped
        if not book.FilePath:
            if not self.profile.MoveFileless:
                self.logger.Add("Ignoré", nom, "Livre sans fichier : la création d'images n'est pas activée")
                return MoveResult.Skipped
            self.logger.Add("Échec", nom, "Livre sans fichier : ComicRust ne peut pas créer l'image de la "
                            "vignette depuis un script")
            return MoveResult.Failed
        folder_path, file_name, failed = self.pathmaker.make_path(book, self.profile.FolderTemplate,
                                                                  self.profile.FileTemplate)
        p = self.pathmaker.p
        if failed:
            self.failed_or_skipped = True
            verbe = " est vide" if len(self.pathmaker.failed_fields) == 1 else " sont vides"
            if not self.profile.MoveFailed:
                self.logger.Add("Échec", nom, ",".join(_noms(self.pathmaker.failed_fields)) + verbe + ".")
                return MoveResult.Failed
        if not file_name:
            self.logger.Add("Échec", nom, "Le nom de fichier calculé est vide")
            return MoveResult.Failed
        full_path = combiner(p, folder_path, file_name)
        if not chemin_enracine(full_path, p):
            self.logger.Add("Échec", nom, "Le chemin calculé n'est pas complet (dossier de base vide ?) : "
                            + full_path)
            return MoveResult.Failed
        return full_path

    def book_should_be_moved_with_rules(self, book):
        if not check_excluded_folders(book.FilePath, self.profile):
            self.logger.Add("Ignoré", book.nom_rapport(), "Le livre est dans un dossier exclu")
            return False
        if not check_metadata_rules(book, self.profile, self.calculs):
            self.logger.Add("Ignoré", book.nom_rapport(), "Le livre est exclu par les règles")
            return False
        return True

    def check_path_problems(self, book, full_path):
        p = book.module_chemins(full_path)
        if full_path == book.FilePath or p.normpath(full_path) == p.normpath(book.FilePath):
            self.logger.Add("Ignoré", book.nom_rapport(), "Le livre est déjà à l'emplacement calculé")
            return MoveResult.Skipped
        if cle_chemin(full_path, p) == cle_chemin(book.FilePath, p):
            # Même chemin à la casse près : renommage pour corriger la casse.
            if self.profile.Mode == Mode.Simulate:
                self.logger.Add("Renommage (simulé)", book.nom_rapport(), "vers : " + full_path)
            elif self.profile.Mode == Mode.Move:
                self.renommer(book, full_path)
                self.deplacements.append((book, book.FilePath, full_path, self.profile.Name))
                self.logger.Add("Renommage demandé (casse)", book.nom_rapport(), "vers : " + full_path)
            self.logger.Add("Ignoré", book.nom_rapport(), "Le livre est déjà à l'emplacement calculé")
            return MoveResult.Skipped
        return None

    def process_books(self, books, profiles):
        books_to_move = self.create_book_paths(books, profiles)
        if self.annulation or not books_to_move:
            return self._rapport()
        total = len(books_to_move)
        for n, item in enumerate(books_to_move):
            if self.annule():
                self.annulation = True
                self.logger.Add("Annulé", "%d opérations" % (total - n), "Arrêt demandé par l'utilisateur")
                return self._rapport()
            self.progression(0.5 + 0.5 * n / total, item.book.nom_rapport())
            self.profile = profiles[item.profile_index]
            self.logger.SetProfile(self.profile.Name)
            result = self.process_book(item)
            if result is MoveResult.Duplicate:
                self.HeldDuplicateBooks.append(item)
            else:
                self._compter(item, result)
        self.HeldDuplicateCount = len(self.HeldDuplicateBooks)
        for item in self.HeldDuplicateBooks:
            if self.annule():
                self.annulation = True
                self.logger.Add("Annulé", "%d opérations" % self.HeldDuplicateCount,
                                "Arrêt demandé par l'utilisateur")
                return self._rapport()
            self.profile = profiles[item.profile_index]
            self.logger.SetProfile(self.profile.Name)
            result = self.process_duplicate_book(item)
            self.HeldDuplicateCount -= 1
            self._compter(item, result)
        return self._rapport()

    def _compter(self, item, result):
        r = self.profile_reports[item.profile_index]
        if result is MoveResult.Skipped:
            self.failed_or_skipped = True
            r.skipped += 1
        elif result is MoveResult.Failed:
            self.failed_or_skipped = True
            r.failed += 1
        elif result is MoveResult.Success:
            r.success += 1

    def _rapport(self):
        header = "\n\n".join(r.get_report(True) for r in self.profile_reports)
        self.logger.add_header(header)
        return header

    def process_book(self, item):
        book = item.book
        result, full_path = self.check_path_to_long(book, item.path)
        if result is not None:
            return result
        item.path = full_path
        p = book.module_chemins(full_path)
        if self._occupe(full_path, p):
            return MoveResult.Duplicate
        return self._terminer(item, full_path)

    def _terminer(self, item, full_path):
        book = item.book
        p = book.module_chemins(full_path)
        folder_path = p.dirname(full_path)
        result = self.create_folder(folder_path, p)
        if result is not MoveResult.Success:
            return result
        old_folder = book.FileDirectory
        result = self.move_book(book, full_path)
        if result is MoveResult.Success and self.profile.RemoveEmptyFolder and self.profile.Mode == Mode.Move:
            if old_folder:
                self.dossiers_retirer.append((old_folder, self.profile))
        if item.failed_fields and result is MoveResult.Success:
            verbe = " sont vides. " if len(item.failed_fields) > 1 else " est vide. "
            self.logger.Add("Échec", book.nom_rapport(), ",".join(_noms(item.failed_fields)) + verbe
                            + verbe_mode(self.profile.Mode) + " vers " + full_path)
            return MoveResult.Failed
        return result

    def process_duplicate_book(self, item):
        book = item.book
        full_path = item.path
        p = book.module_chemins(full_path)
        if self._occupe(full_path, p):
            oldbook = self.find_duplicate_book(full_path, p)
            rename_path = self.create_rename_path(full_path, p)
            rename_filename = p.basename(rename_path) if rename_path else ""
            if not self.always_do_duplicate_action:
                action, pour_tous = self.interface.doublon(self.profile, book, oldbook, full_path,
                                                           rename_filename, self.HeldDuplicateCount)
                self.duplicate_action = action
                if pour_tous:
                    self.always_do_duplicate_action = True
            if self.duplicate_action == "renommer" and rename_path:
                if len(rename_path) > MAX_CHEMIN:
                    court = self.interface.raccourcir_chemin(rename_path)
                    if court is None:
                        self.logger.Add("Ignoré", book.nom_rapport(),
                                        "Le chemin était trop long et l'utilisateur ne l'a pas raccourci")
                        return MoveResult.Skipped
                    rename_path = court
                item.path = rename_path
                return self.process_duplicate_book(item)
            self.logger.Add("Ignoré", book.nom_rapport(), "Un fichier existe déjà : " + full_path
                            + " (ComicRust n'écrase jamais un fichier)")
            return MoveResult.Skipped
        return self._terminer(item, full_path)

    def move_book(self, book, path):
        mode = self.profile.Mode
        if mode == Mode.Move:
            self.renommer(book, path)
            self.undo_collection.append(book.FilePath, path, self.profile.Name)
            self.deplacements.append((book, book.FilePath, path, self.profile.Name))
            self.logger.Add("Déplacement demandé", book.FilePath, "vers : " + path)
        else:
            self.logger.Add(verbe_mode(mode).capitalize(), book.FilePath, "vers : " + path)
        p = book.module_chemins(path)
        self.MovedBooks.add(cle_chemin(path, p))
        return MoveResult.Success

    def create_folder(self, folder_path, p):
        if not os.path.isdir(folder_path) and folder_path not in self.CreatedPaths:
            if self.profile.Mode != Mode.Move:
                self.logger.Add("Dossier créé (simulé)", folder_path)
            self.CreatedPaths.append(folder_path)
        return MoveResult.Success

    def check_path_to_long(self, book, full_path):
        if len(full_path) > MAX_CHEMIN:
            court = self.interface.raccourcir_chemin(full_path)
            if court is None:
                self.logger.Add("Ignoré", book.nom_rapport(),
                                "Le chemin calculé est trop long et l'utilisateur ne l'a pas raccourci")
                return MoveResult.Skipped, ""
            full_path = court
        return None, full_path

    def find_duplicate_book(self, path, p):
        cle = cle_chemin(path, p)
        for b in self.tous:
            if b.FilePath and cle_chemin(b.FilePath, p) == cle:
                return b
        return None

    def create_rename_path(self, path, p):
        """``nom (1).ext``, ``nom (2).ext``… (premier nom libre, 100 essais)."""
        base, extension = p.splitext(path)
        base = re.sub(r" \([0-9]\)$", "", base)
        for i in range(100):
            newpath = base + " (" + str(i + 1) + ")" + extension
            if self._occupe(newpath, p):
                continue
            return newpath
        return None


def _noms(fields):
    return [NOMS_FR.get(FIELD_TO_NAME.get(f, f), FIELD_TO_NAME.get(f, f)) for f in fields]


# ---------------------------------------------------------------------------------------
# Dossiers vides
# ---------------------------------------------------------------------------------------

def est_racine(chemin, p=None):
    """Racine d'un lecteur (``D:``, ``D:\\``), d'un partage (``\\\\serveur\\partage``) ou
    ``/`` : jamais proposée à la suppression."""
    p = p or module_chemins(chemin)
    if p is ntpath:
        _lecteur, reste = ntpath.splitdrive(chemin or "")
        return not reste.strip("\\/")
    return not (chemin or "").strip("/")


def normaliser_dossier(chemin, p=None):
    """Dossier sans barre finale (sauf une racine) et sans ``.``/``..`` ; ``""`` si vide."""
    chemin = (chemin or "").strip()
    if not chemin:
        return ""
    p = p or module_chemins(chemin)
    n = p.normpath(chemin)
    if est_racine(n, p):
        return n
    return n.rstrip("\\/")


def _sous(cle, base_cle, p):
    sep = "\\" if p is ntpath else "/"
    return bool(base_cle) and cle != base_cle and cle.startswith(base_cle.rstrip(sep) + sep)


def dossiers_qui_seront_vides(deplacements, retirer, listdir=os.listdir, isdir=os.path.isdir):
    """Dossiers qui seront vides une fois les déplacements appliqués, hors exceptions du
    profil (lobookmover.remove_empty_folders).

    Plus prudent que l'original, qui remontait sans limite : on ne remonte aux parents
    devenus vides **qu'à l'intérieur du dossier de base** du profil (``BaseFolder``, jamais
    proposé lui-même ni rien au-dessus) ; hors du dossier de base, seul le dossier qui
    contenait le livre est proposé. Une racine (lecteur, partage) ne l'est jamais. Chemins
    normalisés (pas de barre finale), sans doublon.

    ``deplacements`` : [(livre, ancien, nouveau, profil)] ; ``retirer`` : [(dossier, Profile)]."""
    partis = set()
    destinations = []
    for _livre, ancien, nouveau, _profil in deplacements:
        p = module_chemins(ancien, nouveau)
        partis.add(cle_chemin(ancien, p))
        destinations.append(cle_chemin(nouveau, p))
    vides = []
    vus = set()

    def devient_vide(dossier, p):
        try:
            noms = listdir(dossier)
        except OSError:
            return False
        for n in noms:
            c = cle_chemin(p.join(dossier, n), p)
            if c in partis or c in vus:
                continue
            return False
        return True

    for dossier, profil in retirer:
        p = module_chemins(dossier, profil.BaseFolder)
        exclus = {cle_chemin(x, p) for x in profil.ExcludedEmptyFolder if x}
        base = normaliser_dossier(profil.BaseFolder, p)
        base_cle = cle_chemin(base, p) if base else ""
        d = normaliser_dossier(dossier, p)
        while d:
            cle = cle_chemin(d, p)
            if est_racine(d, p) or (base_cle and cle == base_cle):
                break
            if cle in vus:
                # Déjà proposé : on continue vers le parent (dans le dossier de base).
                if not _sous(cle, base_cle, p):
                    break
                d = normaliser_dossier(p.dirname(d), p)
                continue
            if cle in exclus or not isdir(d):
                break
            sep = "\\" if p is ntpath else "/"
            if any(x == cle or x.startswith(cle.rstrip(sep) + sep) for x in destinations):
                break
            if not devient_vide(d, p):
                break
            vus.add(cle)
            vides.append(d)
            if not _sous(cle, base_cle, p):
                break  # hors du dossier de base : pas de remontée
            parent = normaliser_dossier(p.dirname(d), p)
            if not parent or cle_chemin(parent, p) == cle:
                break
            d = parent
    return vides


def nettoyer_liste_dossiers(dossiers):
    """Liste de dossiers vides à proposer : normalisés, sans doublon (casse ignorée sous
    Windows), sans racine."""
    out = []
    vus = set()
    for d in dossiers:
        p = module_chemins(d)
        n = normaliser_dossier(d, p)
        if not n or est_racine(n, p):
            continue
        cle = cle_chemin(n, p)
        if cle not in vus:
            vus.add(cle)
            out.append(n)
    return out


# ---------------------------------------------------------------------------------------
# Annulation du dernier rangement (lobookmover.UndoMover)
# ---------------------------------------------------------------------------------------

class UndoMover(BookMover):
    """Remet les livres du dernier rangement à leur ancien emplacement (demandes de
    renommage, comme pour un rangement). Un fichier qui n'est plus dans la bibliothèque
    ne peut pas être déplacé par ComicRust : il est signalé en échec."""

    def __init__(self, logger, undo_collection, profiles, **kw):
        BookMover.__init__(self, logger, **kw)
        self.undo = undo_collection
        self.profiles = profiles
        self.success = self.failed = self.skipped = 0

    def process_books(self):
        index = {}
        for b in self.tous:
            if b.FilePath:
                index[cle_chemin(b.FilePath, b.module_chemins())] = b
        entrees = self.undo.entries()
        tenus = []
        total = max(len(entrees), 1)
        for n, (profil, actuel, ancien) in enumerate(entrees):
            if self.annule():
                self.logger.Add("Annulé", "%d fichiers" % (len(entrees) - n), "Arrêt demandé par l'utilisateur")
                break
            self.progression(n / total, actuel)
            self.profile = self.profiles.get(profil) or Profile()
            self.logger.SetProfile(profil)
            p = module_chemins(actuel, ancien)
            livre = index.get(cle_chemin(actuel, p))
            r = self._un(livre, actuel, ancien, p)
            if r is MoveResult.Duplicate:
                tenus.append((livre, actuel, ancien, profil))
            else:
                self._compte(r)
        self.HeldDuplicateCount = len(tenus)
        for livre, actuel, ancien, profil in tenus:
            if self.annule():
                break
            self.profile = self.profiles.get(profil) or Profile()
            self.logger.SetProfile(profil)
            self._compte(self._doublon(livre, actuel, ancien, module_chemins(actuel, ancien)))
            self.HeldDuplicateCount -= 1
        texte = "Déplacements demandés : %d\tÉchecs : %d\tIgnorés : %d" % (self.success, self.failed, self.skipped)
        self.logger.add_header(texte)
        return texte

    def _compte(self, r):
        if r is MoveResult.Success:
            self.success += 1
        elif r is MoveResult.Failed:
            self.failed += 1
            self.failed_or_skipped = True
        elif r is MoveResult.Skipped:
            self.skipped += 1
            self.failed_or_skipped = True

    def _un(self, livre, actuel, ancien, p):
        if livre is None:
            if self.existe(actuel):
                self.logger.Add("Échec", actuel, "Ce fichier n'est pas dans la bibliothèque : ComicRust ne "
                                "déplace que les fichiers de ses livres")
            else:
                self.logger.Add("Échec", actuel, "Le fichier n'existe pas")
            return MoveResult.Failed
        if not self.existe(actuel):
            self.logger.Add("Échec", actuel, "Le fichier n'existe pas")
            return MoveResult.Failed
        if ancien == actuel or cle_chemin(ancien, p) == cle_chemin(actuel, p):
            self.logger.Add("Ignoré", actuel, "Le livre est déjà à l'emplacement calculé")
            return MoveResult.Skipped
        if self._occupe(ancien, p):
            return MoveResult.Duplicate
        return self._remettre(livre, actuel, ancien, p)

    def _remettre(self, livre, actuel, ancien, p):
        self.renommer(livre, ancien)
        self.MovedBooks.add(cle_chemin(ancien, p))
        self.deplacements.append((livre, actuel, ancien, self.profile.Name))
        self.logger.Add("Déplacement demandé", actuel, "vers : " + ancien)
        if self.profile.RemoveEmptyFolder:
            self.dossiers_retirer.append((p.dirname(actuel), self.profile))
        return MoveResult.Success

    def _doublon(self, livre, actuel, ancien, p):
        if not self._occupe(ancien, p):
            return self._remettre(livre, actuel, ancien, p)
        oldbook = self.find_duplicate_book(ancien, p)
        rename_path = self.create_rename_path(ancien, p)
        if not self.always_do_duplicate_action:
            action, pour_tous = self.interface.doublon(self.profile, livre, oldbook, ancien,
                                                       p.basename(rename_path) if rename_path else "",
                                                       self.HeldDuplicateCount)
            self.duplicate_action = action
            if pour_tous:
                self.always_do_duplicate_action = True
        if self.duplicate_action == "renommer" and rename_path:
            return self._doublon(livre, actuel, rename_path, p)
        self.logger.Add("Ignoré", actuel, "Un fichier existe déjà : " + ancien
                        + " (ComicRust n'écrase jamais un fichier)")
        return MoveResult.Skipped
