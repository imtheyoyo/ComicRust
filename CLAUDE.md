# ComicRust — consignes de développement

- Projet : lecteur/gestionnaire de BD en Rust, portage de ComicRack CE. Dépôt imtheyoyo/ComicRust.
- Langue : code commenté et documentation en français.
- Chaque release produit une version **Windows 11 x64** (workflow `release.yml`).
- **Chaque version a un document de recette `recette/vX.Y.Z.md`** (tests manuels à valider avant
  de passer à l'étape suivante). Le rédiger avant de lancer la release ; la CI refuse de publier
  sans lui. Voir `recette/README.md` pour le format (gravités [B]/[M]/[m], section « déjà vérifié
  automatiquement », non-régression, limites connues, bilan GO/NO-GO).
- **Chaque version a sa section dans `CHANGELOG.md`** (`## [vX.Y.Z] — date`, en français : Nouveautés /
  Corrections / Recette). Elle devient les notes de la release GitHub ; la CI refuse de publier sans elle.
- Pour chaque recette, générer et publier la page à cocher avec `recette/outil/generer_page.py`
  (une page par version). Yoyo colle le rapport copié depuis cette page : s'en servir pour la
  version suivante (correctifs + non-régression).
- Ne pas commencer l'étape suivante de la feuille de route tant que la recette de la version
  courante n'est pas validée (GO) par Yoyo.
- Avant de publier : `cargo clippy --all-targets -- -D warnings` et `cargo test --all`
  (avec `PDFIUM_DYNAMIC_LIB_PATH` pour les tests PDF).
