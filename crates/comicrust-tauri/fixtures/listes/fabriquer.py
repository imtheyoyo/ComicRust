#!/usr/bin/env python3
"""Fabrique la fixture anonymisée des listes de ComicRack (étape 5 Tauri, tauri-v0.6.0).

Usage : python3 -I fabriquer.py <ComicDb.xml réel de ComicRack> [dossier de sortie]

Entrée : le vrai ComicDb.xml de Yoyo (livres dans MySQL : <Books /> vide, 47 listes).
Sorties (dans le dossier de ce script par défaut) :
- ComicDb_anonyme.xml : même fichier (octets, fins de ligne CRLF, ordre, identifiants, critères)
  sans les réglages d'affichage <Display> (volumineux, sans intérêt pour les listes), avec les
  chemins personnels remplacés (dossiers surveillés, liste noire vidée, chemins de critères) ;
- listes_reference.txt : l'arbre attendu (« profondeur|type|nom|BookCount|identifiant »), lu par
  ElementTree, indépendamment du code Rust : référence de « arbre identique à ComicRack ».
"""
import os
import re
import sys
import xml.etree.ElementTree as ET

XSI = "{http://www.w3.org/2001/XMLSchema-instance}type"
TYPES = {
    "ComicLibraryListItem": "bibliotheque",
    "ComicListItemFolder": "dossier",
    "ComicIdListItem": "lecture",
    "ComicSmartListItem": "intelligente",
}


def anonymiser(texte: str) -> str:
    # Réglages d'affichage : retirés (avec l'indentation qui les précède).
    texte = re.sub(r"\r?\n[ \t]*<Display>.*?</Display>", "", texte, flags=re.S)
    # Chemins personnels.
    texte = re.sub(r"(<WatchFolder Folder=\")[^\"]*(\")", r"\1\\\\NAS\\BD\2", texte)
    texte = re.sub(r"<BlackList>.*?</BlackList>", "<BlackList />", texte, flags=re.S)
    texte = texte.replace("E:\\_BACKUP\\BandesDessinees\\Nouveau dossier\\", "E:\\_SAUVEGARDE\\BD\\Nouveau dossier\\")
    texte = texte.replace("Compil by Yo Thanos", "Compil Thanos")
    texte = re.sub(r"\\\\SERVEURNAS\\[^<\"]*", r"\\\\NAS\\BD", texte)
    return texte


def reference(chemin: str) -> list:
    racine = ET.parse(chemin).getroot()
    lignes = []

    def parcourir(items, prof):
        for it in items.findall("Item"):
            lignes.append(
                f"{prof}|{TYPES[it.get(XSI)]}|{it.get('Name')}|{it.findtext('BookCount') or '0'}|{it.get('Id')}"
            )
            sous = it.find("Items")
            if sous is not None:
                parcourir(sous, prof + 1)

    parcourir(racine.find("ComicLists"), 0)
    return lignes


def main():
    source = sys.argv[1]
    sortie = sys.argv[2] if len(sys.argv) > 2 else os.path.dirname(os.path.abspath(__file__))
    with open(source, "rb") as f:
        texte = f.read().decode("utf-8")
    anonyme = anonymiser(texte)
    xml = os.path.join(sortie, "ComicDb_anonyme.xml")
    with open(xml, "wb") as f:
        f.write(anonyme.encode("utf-8"))
    lignes = reference(xml)
    with open(os.path.join(sortie, "listes_reference.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lignes) + "\n")
    print(f"{len(lignes)} listes, {len(anonyme)} caractères")


if __name__ == "__main__":
    main()
