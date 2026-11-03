# Recette ComicRust Tauri tauri-v0.3.0

| | |
|---|---|
| Version | tauri-v0.3.0 (pré-release de la version Tauri 2 : étape 2, **le lecteur**) |
| Paquet | `ComicRust-Tauri-tauri-v0.3.0-windows-x64.zip` |
| SHA-256 | `063be3dfa73b63a9ef98c8d438a8ab395d4121da57be3c18f3176ac633e3f517` |
| Contenu | **lecteur plein écran** ouvert depuis la bibliothèque (double-clic, Entrée, « Lire ») et par « Ouvrir un fichier… » ; formats CBZ/CBR/CB7/CBT/PDF/DjVu/images/HEIF/AVIF/dossier/WebComic ; page simple, double page, **défilement continu (V)**, manga ; ajustements, zoom, loupe, plein écran ; miniatures et curseur ; ordre, pages supprimées et rotation de la fiche ; préchargement ; reprise de lecture et fenêtre « Reprendre la lecture » ; avancement (base ComicRust en autonome, `data\avancement.json` en mode lié) ; réglages du lecteur mémorisés |
| Origine | étape 2 de [`docs/tauri/PLAN.md`](../docs/tauri/PLAN.md), contrat [`docs/tauri/ipc-lecteur.md`](../docs/tauri/ipc-lecteur.md) ; tauri-v0.2.1 **GO** le 03/11/2026 ([rapport](rapports/tauri-v0.2.1-2026-11-03.txt)). Tests repris des recettes lecteur de la version egui ([v0.2.0](v0.2.0.md), [v0.2.1](v0.2.1.md), [v0.6.0](v0.6.0.md), [v0.9.1](v0.9.1.md)) |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> **ComicRack n'est jamais modifié** : en mode « lié », la base (MySQL ou `ComicDb.xml`) est lue en
> lecture seule et **la lecture n'y est pas écrite** (elle reste dans `data\avancement.json`).
> Comme pour la tauri-v0.2.1, garder **deux copies** : une **autonome** et une **liée** à ComicRack.
>
> **Échantillons** : `F:\Projet REA par Claude\REA ComicRack\ComicRust-echantillons` (appelés
> « échantillons » ci-dessous ; même contenu que [`recette/echantillons`](echantillons)).
> Les fichiers hors bibliothèque s'ouvrent par **« Ouvrir un fichier… »** (barre de la
> bibliothèque) en collant leur chemin.
>
> **Valeurs de référence** : « thanos » **484** livres en tauri-v0.2.1 (ComicRack : 483) ; la
> bibliothèque liée compte **5 325** livres.

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| 53 tests du backend `comicrust-tauri` (33 de la tauri-v0.2.1 + 20) : CBZ hors bibliothèque (pages d'origine servies telles quelles, pages réduites), adresse `cr-page` et jeton inconnu (404), **ordre, pages supprimées et rotation de la fiche**, ordre du `ComicInfo.xml` sans fiche, image seule qui ouvre son dossier, **avancement écrit une fois par séance** dans la base en mode autonome, **mode ComicRack en lecture seule** (mémoire locale seulement), cache borné, préchargement des pages suivantes, réponse 504 après le délai (NAS muet), livre réécrit pendant la lecture rouvert, 5ᵉ livre ouvert (le plus ancien fermé), contrat JSON camelCase, page tournée et réduite ; `avancement.json` (page la plus loin gardée, fichier illisible mis de côté) ; réglages `vue.lecteur` (valeurs conservées, validées, invalides ignorées) | `cargo test` (`comicrust-tauri`) | ✅ 53/53 |
| Front : `svelte-check` (0 erreur, 0 avertissement), `vite build` | local + CI Windows (`ci-tauri.yml`) | ✅ |
| Parcours simulé dans un navigateur (Playwright, mode simulé sans Tauri) : ouverture par double-clic, → (page 2), **D** (planches 1, 2-3, 4-5, 6, puis 7 seule (page double)), **V** (défilement), **M**, **3**, **+** (125 %), **T** (miniatures), Échap ferme le lecteur ; question « Reprendre la lecture » (`?repriseDiff`) et choix « lue dans ComicRust (ce poste) » ; « Ouvrir un fichier… » ; aucune erreur JavaScript | Playwright (mode simulé) | ✅ |
| Clippy `-D warnings`, tests, compilation de l'application complète (`--features app,custom-protocol`) | CI Windows (`ci-tauri.yml`) | ✅ verte |

**Non vérifiable ici** : la vraie fenêtre WebView2 et le plein écran, le rendu des vrais fichiers
(PDFium, DjVu, HEIF/AVIF sous Windows), la fluidité sur le NAS, le verrouillage des fichiers par
Windows, le NAS débranché, la base ComicRack réelle (sections 1 à 12).

## 1. Préparation

- [ ] **P-01 [B]** Vérifier l'empreinte : `Get-FileHash .\ComicRust-Tauri-tauri-v0.3.0-windows-x64.zip` → identique au SHA-256 ci-dessus. Le zip contient `ComicRust-Tauri.exe`, `pdfium.dll`, `README.md`, `RECETTE.md` et `licences\`.
- [ ] **P-02 [B]** Copier le contenu du zip **par-dessus** les deux copies tauri-v0.2.1 (dossiers `data\` conservés) et lancer chacune : pas d'assistant, même mode (Autonome / Lié), mêmes livres, mêmes préférences d'affichage (vue, tri, thème) qu'en tauri-v0.2.1.
- [ ] **P-03 [M]** Copie autonome : ajouter le dossier des **échantillons** comme dossier surveillé et lancer un scan : les échantillons lisibles apparaissent dans la bibliothèque (le fichier corrompu peut manquer ou être signalé).

## 2. Ouverture du lecteur

- [ ] **O-01 [B]** Depuis la bibliothèque (copie autonome) : **double-clic** sur une couverture, puis **Entrée** sur un livre sélectionné (grille et tableau), puis bouton **« Lire »** du panneau de détails : chaque fois le lecteur s'ouvre en plein fenêtre (« Ouverture du livre… » puis la page), titre du livre en haut, « Page n / N ».
- [ ] **O-02 [B]** **Échap** (ou ✖ « Fermer le lecteur (Échap) », ou **Ctrl+W**) : retour à la bibliothèque, **même livre sélectionné**, même position de défilement ; les flèches déplacent à nouveau la sélection (le focus est revenu à la liste).
- [ ] **O-03 [B]** **« Ouvrir un fichier… »** : la fenêtre « Ouvrir un fichier » s'ouvre (« Chemin d'une BD (CBZ, CBR, CB7, PDF…), d'un dossier d'images ou d'une image : ») ; « Ouvrir » est grisé tant que le champ est vide ; coller le chemin de `Thanos Quest 01 (1990).cbz` des échantillons et valider par **Entrée** : le livre s'ouvre, titre « Thanos Quest 01 (1990) » (nom du fichier), « Page 1 / 11 ». « Annuler » ferme la fenêtre sans rien ouvrir.
- [ ] **O-04 [M]** Chemin **entre guillemets** (Explorateur : Maj+clic droit → « Copier en tant que chemin d'accès ») : accepté tel quel, le livre s'ouvre.
- [ ] **O-05 [M]** Chemin d'**une image seule** (`Dossier d'images\page001.jpg`) : le dossier s'ouvre **à cette image** (« Page 2 / 3 »).
- [ ] **O-06 [B]** `Fichier corrompu.cbz` : message « Impossible d'ouvrir ce livre : … » (texte en français) et bouton « Fermer » qui ramène à la bibliothèque ; pas de plantage. Même chose pour un chemin qui n'existe pas.
- [ ] **O-07 [m]** Un fichier non BD (un `.txt` renommé en `.cbz`) : message d'erreur, pas de plantage.
- [ ] **O-08 [M]** Copie liée : un livre de la bibliothèque ComicRack s'ouvre par double-clic, Entrée et « Lire », comme en O-01.

## 3. Formats (échantillons et vraies BD)

Pour chaque fichier : la première page s'affiche en moins de 2 s, le nombre de pages est le bon,
→ passe à la page suivante. Le format est visible dans le panneau de détails (« Format ») quand le
livre est dans la bibliothèque.

- [ ] **F-01 [B]** `Thanos Quest 01 (1990).cbz` : 11 pages (« Page 1 / 11 »), pages dans l'ordre.
- [ ] **F-02 [B]** `Silver Surfer 04 (1987).cbr` : 11 pages.
- [ ] **F-03 [B]** `Warlock 09 (1976).cb7` : 11 pages ; `Warlock 09 (1976) - copie.7z` : idem.
- [ ] **F-04 [M]** `Infinity Gauntlet 02 (1991).cbt` : 11 pages.
- [ ] **F-05 [B]** `Silver Surfer 03.pdf` : 4 pages, rendu net ; en **taille réelle (4)** le texte reste net.
- [ ] **F-06 [M]** `Captain Marvel 25 (1973).djvu` : 1 page lisible.
- [ ] **F-07 [M]** `Dossier d'images` (chemin du dossier) : 3 pages.
- [ ] **F-08 [M]** `Faux CBR - en realite ZIP.cbr` : s'ouvre normalement (reconnu par sa signature).
- [ ] **F-09 [B]** `Images HEIF AVIF.cbz` : 4 pages affichées (p02 et p04 en HEIC, p03 en AVIF), nettes, sans blocs de couleur parasites, miniatures comprises (T).
- [ ] **F-10 [B]** `Web Comic 01.cbw` : 6 pages (images du dossier `web`).
- [ ] **F-11 [M]** `Web Comic index 02.cbw` : 5 pages, pas de « publicité » (page bleue) ; `Web Comic navigation 03.cbw` : 4 pages, la 2ᵉ est une double page (seule en mode double page).
- [ ] **F-12 [m]** `Web Comic composition 04.cbw` : 3 pages, deux planches côte à côte sur fond noir, la première **à droite** ; `xkcd en ligne 05.cbw` (Internet) : derniers épisodes de xkcd. Nombre de pages : ………
- [ ] **F-13 [B]** Vraies BD de la collection : un CBZ, un CBR **RAR5** et un CBR **RAR4** : pages dans le bon ordre (page 2 avant page 10), couleurs correctes.
- [ ] **F-14 [M]** Vraie BD CB7 (LZMA2 solide) : s'ouvre ; temps d'ouverture : ……… s. Vrai PDF : toutes les pages lisibles.
- [ ] **F-15 [M]** BD contenant des pages **WebP** : affichées.
- [ ] **F-16 [m]** BD contenant des pages **JPEG XL** (`.jxl`) ou un vrai DjVu : affichées. *(« Sans objet » si tu n'en as pas.)*

## 4. Modes d'affichage

Sur `Thanos Quest 01 (1990).cbz` sauf mention contraire (la **page 7** est une double page scannée, large).

- [ ] **M-01 [B]** **▯ Page** : une page à la fois ; « Page n / 11 ».
- [ ] **M-02 [B]** **D** (ou **▯▯ Double**) puis Début : planches **1** (couverture seule) | **2-3** | **4-5** | **6** | **7** (large, seule) | **8-9** | **10-11** ; le haut indique « Pages 2-3 / 11 »… ; aucune page sautée.
- [ ] **M-03 [M]** Double page + ← depuis la fin : exactement les mêmes planches à l'envers (10-11, 8-9, 7, 6, 4-5, 2-3, 1), sans décalage.
- [ ] **M-04 [M]** Double page, bouton **Couv. seule** désactivé : la couverture est appariée (1-2, 3-4…) ; la page 7 reste seule.
- [ ] **M-05 [B]** **V** (ou **⇕ Défilement**) : les pages se suivent dans une colonne continue, défilement à la molette, aux flèches ↓ ↑, Espace / Maj+Espace et Page suiv. / Page préc. ; le numéro « Page n » suit la page en haut de l'écran ; **V** de nouveau : retour en page simple **à la même page**.
- [ ] **M-06 [M]** Défilement continu sur une vraie BD de plus de 100 pages : aller à la fin au curseur puis remonter : pas de trou blanc durable, pas de saut de position quand les images arrivent.
- [ ] **M-07 [B]** **M** (ou **⇐ Manga**) en double page : la page de **gauche** est la suivante ; → recule, ← avance ; les boutons ◀ ▶ et le curseur sont inversés.
- [ ] **M-08 [M]** Un livre marqué **Manga (droite à gauche)** dans sa fiche (ComicRack ou `ComicInfo.xml`) s'ouvre directement en sens manga.

## 5. Ajustements, zoom, loupe, plein écran

- [ ] **J-01 [B]** Touches **1 2 3 4** (ou la liste « Page entière (1) / Largeur (2) / Hauteur (3) / Taille réelle (4) ») : page entière, largeur (barre de défilement verticale), hauteur, taille réelle ; la liste suit la touche.
- [ ] **J-02 [B]** **+ − 0** (et boutons − / % / +) : zoom avant, arrière, retour à 100 % ; le pourcentage suit. **Ctrl+molette** : zoom **autour du pointeur** (le point visé reste sous la souris).
- [ ] **J-03 [M]** Page zoomée (ou en largeur) : **glisser** à la souris déplace la page ; relâcher ne change pas de page.
- [ ] **J-04 [M]** **L** (ou 🔍) : une loupe ronde suit le pointeur et grossit la page ; le curseur ×2,5 règle le grossissement (1,5 à 6) ; **L** de nouveau la retire.
- [ ] **J-05 [B]** **F** ou **F11** (ou ⛶) : vrai plein écran Windows (barre des tâches masquée) ; **Échap** quitte le plein écran **sans fermer** le lecteur ; un second Échap ferme le lecteur.
- [ ] **J-06 [m]** Redimensionner ou agrandir la fenêtre : la page se réajuste aussitôt, sans flou durable.

## 6. Clavier et souris

- [ ] **C-01 [B]** → / Espace / Page suiv. : page (planche) suivante ; ← / Retour arrière / Maj+Espace / Page préc. : précédente ; **Début** / **Fin** : première / dernière page.
- [ ] **C-02 [M]** ↓ / ↑ en largeur (2) : la page défile d'un petit pas sans changer de page.
- [ ] **C-03 [M]** Maintenir → sur une grosse BD : pas de blocage ni de plantage, la page finale s'affiche.
- [ ] **C-04 [B]** Clic sur le **tiers droit** de la page → suivante ; **tiers gauche** → précédente ; **centre** → rien ; **clic droit** → précédente.
- [ ] **C-05 [M]** **Molette** en page entière : une page par cran, sans sauts multiples ; en largeur (page plus haute que l'écran) : la molette fait défiler la page.
- [ ] **C-06 [M]** Barres du haut et du bas : masquées (avec le curseur) après 2 à 3 s sans bouger la souris, réapparaissent au mouvement ; restent visibles tant que la souris est dessus ou qu'un bouton a le focus (Tab).

## 7. Miniatures et curseur

- [ ] **T-01 [B]** **T** (ou ▦) : bandeau de miniatures en bas ; la miniature courante est surlignée et le bandeau la suit quand on tourne les pages ; un clic sur une miniature va à cette page ; **T** le masque.
- [ ] **T-02 [B]** Curseur du bas (◀ ▶ et glissière) : glisser jusqu'à la page 8 → la page 8 s'affiche ; ◀ ▶ = précédente / suivante.
- [ ] **T-03 [m]** Les miniatures se remplissent sans ralentir la lecture (sur une BD du NAS) ; en manga, le bandeau va de droite à gauche.

## 8. Ordre, pages supprimées et rotation de la fiche

Copie liée. Dans **ComicRack**, sur un livre de test (CBZ d'une dizaine de pages) : vue des pages,
**déplacer** une page (ex. la 3 après la 6), marquer une page **« Supprimée »** (type de page) et
**tourner** une page de 90°. Fermer ComicRack puis relancer la copie liée de ComicRust.

- [ ] **E-01 [B]** Le livre s'ouvre dans **l'ordre de ComicRack** (la page déplacée à sa nouvelle place), comme dans le lecteur de ComicRack.
- [ ] **E-02 [B]** La page supprimée **n'apparaît pas** (ni en lecture, ni dans les miniatures) ; le total « / N » compte une page de moins.
- [ ] **E-03 [M]** La page tournée s'affiche tournée (miniature comprise) ; en double page, une page tournée en paysage est seule.
- [ ] **E-04 [M]** « Ouvrir un fichier… » sur le **même** fichier (chemin de la bibliothèque) : même ordre, mêmes pages masquées et même reprise qu'en ouvrant depuis la bibliothèque.
- [ ] **E-05 [m]** Le même fichier copié **hors de la bibliothèque** (autre dossier) et ouvert par « Ouvrir un fichier… » : ordre du `ComicInfo.xml` du fichier (sinon ordre naturel des noms), aucune page masquée.

## 9. Préchargement et fluidité (CBZ du NAS)

Sur un vrai CBZ du NAS (`\\SERVEURNAS\…`), de préférence gros (> 300 pages ou > 500 Mo).

- [ ] **V-01 [B]** Ouverture depuis la bibliothèque : première page affichée en ……… s (attendu : quelques secondes au plus).
- [ ] **V-02 [B]** Après une ou deux secondes sur une page, → affiche la suivante **sans attente visible** (pages préparées à l'avance) ; ← aussi ; en double page, les planches suivantes sont prêtes de même.
- [ ] **V-03 [M]** Feuilleter 30 pages d'affilée à un rythme de lecture rapide : pas de saccade ni de « Chargement… » durable ; la fenêtre reste réactive (barres, Échap).
- [ ] **V-04 [M]** Mémoire après avoir parcouru le gros livre puis ouvert 5 livres différents (ComicRust-Tauri + WebView2, Gestionnaire des tâches) : ……… Mo ; elle ne grimpe pas sans fin.

## 10. Reprise de lecture

- [ ] **R-01 [B]** Copie autonome : lire jusqu'à la page 6, fermer le lecteur, **fermer l'application**, relancer, rouvrir le même livre : reprise **à la page 6**.
- [ ] **R-02 [B]** Copie liée : un livre lu dans **ComicRack** jusqu'à une page X et jamais ouvert dans ComicRust s'ouvre **à la page X** de ComicRack, sans question. Page X : ………
- [ ] **R-03 [B]** Copie liée : lire ce livre jusqu'à une autre page Y dans ComicRust, fermer, rouvrir : la fenêtre **« Reprendre la lecture »** apparaît, avec le titre du livre, « La bibliothèque et ce poste n'ont pas retenu la même page : », et les choix « 📖 Page X — lue dans la bibliothèque », « 📖 Page Y — lue dans ComicRust (ce poste) », « ⏮ Depuis le début », la case « Toujours faire ce choix » et « Annuler ». Le premier bouton a le focus ; Tab et Entrée fonctionnent.
- [ ] **R-04 [B]** Chaque choix ouvre à la bonne page : page X, page Y, page 1. *(Rouvrir le livre entre deux essais.)*
- [ ] **R-05 [M]** « Annuler » (ou Échap) : le lecteur se ferme sans afficher de page ; rouvrir le livre repose la question.
- [ ] **R-06 [M]** Cocher **« Toujours faire ce choix »** puis « lue dans ComicRust (ce poste) » : aux ouvertures suivantes, plus de question, reprise directe à la page de ce poste ; `data\comicrust-tauri.json` contient `"reprise": "comicrust"` dans `vue.lecteur`. Retirer cette ligne (application fermée) : la question revient.
- [ ] **R-07 [m]** Reprise sur une page devenue « Supprimée » dans la fiche : le livre s'ouvre à la page visible suivante.

## 11. Avancement de lecture

- [ ] **W-01 [B]** Copie autonome : lire quelques pages d'un livre non lu, fermer le lecteur : sans relancer, le panneau de détails indique « Lecture N % » ; aller jusqu'à la **dernière page** d'un livre puis fermer : le livre est marqué **✓ Lu** (couverture et panneau).
- [ ] **W-02 [B]** Copie autonome, **une écriture par séance** : noter la date de modification de `data\comicrust.db`, sans scan en cours, tourner des pages pendant une minute sans aller à la fin : la date **ne change pas** ; fermer le lecteur : elle change (une seule fois). Rouvrir et refermer le livre : une nouvelle écriture (nouvelle séance).
- [ ] **W-03 [M]** `data\avancement.json` (les deux copies) : une entrée par fichier lu (chemin complet) avec `page`, `pageMax` et `pages`, mise à jour pendant la lecture (environ une seconde après chaque page) ; revenir en arrière ne fait pas reculer `pageMax`.
- [ ] **W-04 [B]** Copie liée, **rien n'est écrit dans ComicRack** : lire quelques pages d'un livre dans ComicRust (ComicRack lancé ou non) et fermer : dans ComicRack, la page de lecture, la date d'ouverture et l'état « lu » de ce livre **n'ont pas changé** ; `ComicDb.xml` (ou la base MySQL) n'est pas modifié ; seule `data\avancement.json` de la copie liée a changé.
- [ ] **W-05 [M]** Copie liée : la lecture faite dans ComicRust est retrouvée au prochain lancement (reprise à la page de ce poste, ou question R-03 si ComicRack a une autre page).
- [ ] **W-06 [m]** Fermer **l'application** (croix de la fenêtre) avec un livre ouvert dans le lecteur : la page est gardée (reprise au relancement) et, en autonome, la lecture est écrite dans la base.

## 12. Réglages, fichiers, réseau et mode portable

- [ ] **G-01 [B]** Réglages du lecteur mémorisés : choisir **Double**, **Largeur (2)**, désactiver **Couv. seule**, afficher les **miniatures**, loupe **×4**, **Manga** (sur un livre non manga) ; fermer le lecteur et l'application, relancer, ouvrir un **autre** livre : tous ces réglages sont repris. Le zoom revient à 100 % et la loupe est retirée (non mémorisés, attendu).
- [ ] **G-02 [M]** `data\comicrust-tauri.json` : la section `vue` contient un sous-objet `lecteur` (`mode`, `ajustement`, `couvertureSeule`, `miniatures`, `grossissementLoupe`, `manga`) ; `mode` et les autres préférences de la vue sont inchangés.
- [ ] **G-03 [m]** Application fermée, mettre une valeur absurde dans `vue.lecteur` (ex. `"mode": "n_importe_quoi"`, `"grossissementLoupe": 99`) : au lancement, aucune erreur ; la valeur fautive revient à sa valeur par défaut (ou est bornée à 6), les autres réglages sont conservés.
- [ ] **X-01 [B]** **Fichier jamais verrouillé** : pendant la lecture d'une **copie** de `Thanos Quest 01 (1990).cbz` (disque local), la **renommer** puis la **déplacer** dans un autre dossier avec l'Explorateur : Windows accepte **sans** message « fichier ouvert dans une autre application » ; le lecteur reste utilisable (les pages déjà prêtes s'affichent, les autres peuvent indiquer « ⚠ Page n illisible »). Remettre le fichier : rouvert, il se lit normalement.
- [ ] **X-02 [M]** Même essai sur un CBZ **du NAS** : renommage accepté pendant la lecture.
- [ ] **X-03 [B]** **NAS débranché pendant la lecture** (câble réseau ou Wi-Fi coupé) d'une BD du NAS : la fenêtre **ne gèle pas** (barres, Échap, bibliothèque utilisables) ; les pages non préparées affichent « ⚠ Page n illisible » au plus tard après une trentaine de secondes ; Échap ferme le lecteur. Rebrancher : le livre rouvert se lit normalement.
- [ ] **X-04 [M]** NAS débranché, ouvrir une BD du NAS : message « Impossible d'ouvrir ce livre : … » au plus tard après 40 s environ, sans gel de la fenêtre. Durée : ……… s.
- [ ] **X-05 [B]** **Mode portable** : après tous ces tests, `data\` contient `avancement.json`, `comicrust-tauri.json`, `webview2\` (et `comicrust.db`, `couvertures\` en autonome) ; **aucun** dossier `ComicRust` ni `fr.imtheyoyo.comicrust` dans `%APPDATA%` ni `%LOCALAPPDATA%` ; aucun fichier ajouté à côté des BD lues.

## 13. Non-régression courte (tauri-v0.2.1)

- [ ] **N-01 [B]** Copie liée : recherche « thanos » → **484** livres (comme en tauri-v0.2.1) ; Échap vide la recherche. Nombre : ………
- [ ] **N-02 [B]** Mémorisation de la vue : changer le tri, le groupe, la vue (Tableau) et le thème, fermer, relancer : tout revient tel quel ; ouvrir et fermer un livre ne change pas ces préférences.
- [ ] **N-03 [M]** Grille : couvertures affichées (pas d'image cassée), défilement continu sans trou ; tableau trié par clic sur les en-têtes ; tri Série en ordre naturel (#2 avant #10).

---

## Limites connues (ne pas compter comme anomalies)

- **Mode lié : rien n'est écrit dans ComicRack** (page, date, état « lu ») jusqu'à l'étape 6 ; la
  lecture reste dans `data\avancement.json` de la copie liée (voir W-04).
- **Annuler la question de reprise compte quand même une ouverture** : en autonome, le livre a été
  ouvert puis fermé (date et nombre d'ouvertures mis à jour dans la base de ComicRust).
- **Pas de sélecteur de fichiers natif** : « Ouvrir un fichier… » demande de coller ou taper le
  chemin ; pas de glisser-déposer de fichier dans la fenêtre ni d'ouverture en ligne de commande.
- **La touche V et le défilement continu n'existent pas dans la version egui** : pas de
  comparaison possible avec elle pour M-05 et M-06. Dans ce lecteur, **L** est la loupe (dans la
  version egui, L affichait le panneau de liste).
- Le zoom et la loupe ne sont pas mémorisés (G-01).
- Le format n'est pas affiché dans le lecteur (seulement dans le panneau de détails de la
  bibliothèque) ; pas d'infos du livre (touche I de la version egui) : étape 3.
- Pas de listes de lecture `.cbl` ni d'enchaînement avec le livre suivant en fin de livre : étape 5.
- Pas de fichiers récents.
- Une page signalée « illisible » (NAS débranché, fichier renommé) le reste jusqu'à la réouverture
  du livre.
- Au plus 4 livres gardés ouverts en mémoire : le plus ancien est fermé (lecture enregistrée).
- WebView2 doit être présent (Windows 11 : intégré). Il n'est pas embarqué dans le zip.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 41 | 41 | 0 |
| Majeurs [M] | 33 | 32 | 1 (M-08 non testé) |
| Mineurs [m] | 9 | 9 | 0 |

Décision et remarques :
GO — recette validée le 03/11/2026 : 82 OK sur 83, bloquants tous OK (rapport : `recette/rapports/tauri-v0.3.0-2026-11-03.txt`). M-08 non testé, reporté dans tauri-v0.4.0. E-03 : la miniature d'une page tournée sort de son cadre dans le bandeau → corrigé en tauri-v0.4.0.
