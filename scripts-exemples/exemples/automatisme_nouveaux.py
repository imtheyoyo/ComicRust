# -*- coding: utf-8 -*-
"""Automatisme sur les nouveaux livres (script d'exemple fourni avec ComicRust).

    ┌──────────────────────────────────────────────────────────────────────────────┐
    │ DÉSACTIVÉ PAR DÉFAUT (en-tête « #@Enabled false »). Pour l'activer :         │
    │ 📜 Scripts ▸ Gérer les scripts… ▸ cocher « Nouveaux livres : étiquette et    │
    │ série depuis le nom » (et cocher « Exécuter les automatismes »).             │
    └──────────────────────────────────────────────────────────────────────────────┘

Une fois activé, après chaque analyse des dossiers qui ajoute des livres (hook
``NewBooks``), pour chaque nouveau livre :

- ajoute une étiquette (champ Tags), « à lire » par défaut, sans doublon ;
- complète la série, le numéro et l'année **vides** depuis le nom du fichier
  (« Thanos Quest 01 (1990).cbr » → série « Thanos Quest », numéro « 1 », année 1990).

C'est un automatisme : pas de question ni d'aperçu, les modifications sont appliquées au
fil de l'eau (elles restent visibles et annulables dans la fiche). Seules les fiches sont
modifiées : aucun fichier n'est touché.
"""

import os
import re

import comicrust

DEFAUTS = {"etiquette": "à lire", "completer": True}


def _reglage(nom):
    v = comicrust.settings().get(nom)
    return DEFAUTS[nom] if v is None else v


#@Name Nouveaux livres : réglages
#@Hook ConfigScript
#@Key nouveaux-livres-reglages
#@Description Étiquette ajoutée aux nouveaux livres, complément depuis le nom du fichier.
def reglages():
    s = comicrust.settings()
    s["etiquette"] = (comicrust.ask("Étiquette ajoutée (vide = aucune) :", _reglage("etiquette")) or "").strip()
    s["completer"] = comicrust.confirm(
        "Compléter série, numéro et année vides depuis le nom du fichier ?", bool(_reglage("completer")))
    s.save()


def ajouter_etiquette(tags, etiquette):
    """« a, b » + « c » → « a, b, c » (rien si déjà présente, sans casse)."""
    liste = [t.strip() for t in re.split(r"[,;]", tags or "") if t.strip()]
    if etiquette.casefold() not in (t.casefold() for t in liste):
        liste.append(etiquette)
    return ", ".join(liste)


def depuis_nom(chemin):
    """Série, numéro et année devinés depuis le nom du fichier (clés absentes si rien de sûr)."""
    info = {}
    nom = os.path.splitext(os.path.basename(str(chemin or "").replace("\\", "/")))[0]
    m = re.search(r"[(\[]((?:19|20)\d{2})(?:[-./]\d{1,2})*[)\]]", nom)
    if m:
        info["Year"] = int(m.group(1))
    reste = " ".join(re.sub(r"\s*[(\[][^)\]]*[)\]]", " ", nom).replace("_", " ").split())
    m = re.match(r"^(?P<s>.+?)(?:\s*[-–]\s*|\s+)(?:(?:tome|t|n°|no\.?)\s*|#\s*)?"
                 r"(?P<n>\d+(?:[.,]\d+)?|½)(?:\s*[-–:]\s*.+?)?\s*$", reste, re.IGNORECASE)
    if m:
        info["Series"] = m.group("s").strip(" -–")
        info["Number"] = re.sub(r"^0+(?=\d)", "", m.group("n"))
    return info


def _vide(v):
    if v is None:
        return True
    if isinstance(v, str):
        return not v.strip()
    return isinstance(v, (int, float)) and not isinstance(v, bool) and v < 0


#@Name Nouveaux livres : étiquette et série depuis le nom
#@Hook NewBooks
#@Key nouveaux-livres
#@Description Après une analyse : étiquette « à lire » et série/numéro/année depuis le nom
#@Description du fichier. Désactivé par défaut : à cocher dans « Gérer les scripts ».
#@Enabled false
def nouveaux(books):
    etiquette = (_reglage("etiquette") or "").strip()
    completer = bool(_reglage("completer"))
    n = 0
    for book in books:
        if comicrust.cancelled():
            break
        if etiquette:
            book.set("Tags", ajouter_etiquette(book.tags, etiquette))
        if completer and book.file:
            info = depuis_nom(book.file)
            serie_ok = _vide(book.series) or info.get("Series", "").casefold() == book.series.casefold()
            for champ, valeur in info.items():
                if champ == "Number" and not serie_ok:
                    continue  # numéro d'une autre série : on n'y touche pas
                if _vide(book.get(champ)):
                    book.set(champ, valeur)
        n += 1
    comicrust.log("%d nouveau(x) livre(s) traité(s)." % n)
    return n
