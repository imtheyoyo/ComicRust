#!/usr/bin/env python3
"""Transforme une recette Markdown (recette/vX.Y.Z.md) en page à cocher.

    python3 recette/outil/generer_page.py recette/v0.2.0.md sortie.html

La page reprend les sections, les tests (« - [ ] **ID [B|M|m]** texte »), le tableau
« déjà vérifié automatiquement » et les limites connues. Les coches sont gardées dans
le navigateur (localStorage), une clé par version.
"""
import json
import re
import sys
from pathlib import Path

ITEM = re.compile(r"^- \[[ xX]\] \*\*(?P<id>[A-Z]+-\d+)(?: \[(?P<sev>[BMm])\])?\*\* (?P<text>.+)$")
REPO_BLOB = "https://github.com/imtheyoyo/ComicRust/blob/main/recette/"


def parse(md: str) -> dict:
    lines = md.splitlines()
    data = {"title": "", "version": "", "meta": [], "rule": "", "auto": [], "auto_intro": "",
            "sections": [], "limits": []}
    section = None
    mode = "head"
    para: list[str] = []

    def flush_para():
        nonlocal para
        text = " ".join(p.strip() for p in para).strip()
        para = []
        if not text:
            return
        if mode == "head" and text.startswith("**Règle"):
            data["rule"] = text
        elif mode == "auto":
            data["auto_intro"] = text
        elif mode == "section" and section is not None and not section["items"]:
            section["intro"] = (section["intro"] + " " + text).strip()

    for line in lines:
        if line.startswith("# "):
            data["title"] = line[2:].strip()
            m = re.search(r"v\d+\.\d+\.\d+", data["title"])
            data["version"] = m.group(0) if m else data["title"]
            continue
        if line.startswith("## "):
            flush_para()
            heading = line[3:].strip()
            if heading.startswith("0."):
                mode = "auto"
            elif heading.startswith("Limites"):
                mode = "limits"
            elif heading.startswith(("Anomalies", "Bilan")):
                mode = "end"
            else:
                mode = "section"
                section = {"title": re.sub(r"^\d+\.\s*", "", heading), "intro": "", "items": []}
                data["sections"].append(section)
            continue
        if mode == "end":
            continue
        if line.startswith("|"):
            flush_para()
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if all(set(c) <= set("-: ") for c in cells):
                continue
            if mode == "head" and len(cells) == 2 and cells[0] and cells[0] not in ("Testeur", "Date", "Poste") \
                    and "Décision" not in cells[0]:
                data["meta"].append(cells)
            elif mode == "auto" and len(cells) == 3 and cells[0] != "Vérification":
                data["auto"].append(cells)
            continue
        m = ITEM.match(line)
        if m and mode == "section":
            flush_para()
            section["items"].append({"id": m["id"], "sev": m["sev"] or "M", "text": m["text"]})
            continue
        if mode == "limits" and line.startswith("- "):
            data["limits"].append(line[2:].strip())
            continue
        if line.strip() in ("", "---"):
            flush_para()
        else:
            para.append(line)
    flush_para()
    return data


def main():
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    data = parse(src.read_text(encoding="utf-8"))
    data["repo_base"] = REPO_BLOB
    n = sum(len(s["items"]) for s in data["sections"])
    template = (Path(__file__).parent / "page_modele.html").read_text(encoding="utf-8")
    page = template.replace("/*__DONNEES__*/null", json.dumps(data, ensure_ascii=False))
    page = page.replace("__TITRE__", f"Recette ComicRust {data['version']}")
    out.write_text(page, encoding="utf-8")
    print(f"{out} : {len(data['sections'])} sections, {n} tests")


if __name__ == "__main__":
    main()
