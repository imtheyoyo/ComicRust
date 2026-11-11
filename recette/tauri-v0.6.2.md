# Recette ComicRust Tauri tauri-v0.6.2

| | |
|---|---|
| Version | tauri-v0.6.2 (pré-release corrective de l'étape 5 : **listes ComicRack**) |
| Paquet | `ComicRust-Tauri-tauri-v0.6.2-windows-x64.zip` |
| SHA-256 | `f7df767f857cdd68ac125b113aab757e7095965399245edd53b474bc9a235e4c` |
| Contenu | **tri, sens, groupement et vue mémorisés pour chaque liste** (remarque du 11/11) ; éditeur de listes intelligentes : critère sans marqueur `genre` accepté par le backend (E-01) ; **focus** rendu à la grille, au tableau ou à l'arbre après « Ouvrir un fichier », les fenêtres et les menus (K-02) ; « Ajouter à une liste… » propose la **dernière liste utilisée** même depuis la Bibliothèque (K-03) ; reprise des correctifs de la tauri-v0.6.1 (série du nom de fichier, focus, genre) |
| Origine | rapports du 11/11/2026 : [tauri-v0.6.0](rapports/tauri-v0.6.0-2026-11-11.txt) **NO-GO** (E-01, E-02 ; 10 non testés) et [tauri-v0.6.1](rapports/tauri-v0.6.1-2026-11-11.txt) **NO-GO** — **passée avec l'exe 0.6.0** (les deux copies contenaient encore `ComicRust-Tauri.exe` 0.6.0, vu par le MCP) : ses tests sont repris ici |

**Règle de passage :** tous les tests **[B]** (bloquants) doivent être OK. Un test **[M]** (majeur)
en échec est accepté seulement s'il est noté dans « Anomalies » avec un correctif prévu.
Les tests **[m]** (mineurs) n'empêchent pas le passage.

> ⚠ **Installer le nouveau programme dans les DEUX copies** (autonome et liée) : copier le contenu
> du zip **par-dessus** chaque dossier (`data\` conservé), puis vérifier la version (P-01). La
> section 6 peut réécrire `ComicDb.xml` : faire d'abord les sauvegardes PR-03 et PR-04 de la
> [recette tauri-v0.6.0](tauri-v0.6.0.md) (ComicRack fermé).

---

## 0. Déjà vérifié automatiquement (pour information)

| Vérification | Où | Résultat |
|---|---|---|
| **100 tests** du backend `comicrust-tauri` (97 + 3) : nœud de critère sans `genre` accepté (critère ou groupe deviné, marqueur toujours écrit en sortie) ; `Thanos.cbl` des échantillons rattaché avec les **noms réels** (`Silver Surfer 03.pdf`, `Silver Surfer 04 (1987).cbz`, `Warlock 09 (1976).cbz`…, sans fiche) ; vues par liste dans `comicrust-tauri.json` (lecture tolérante, fusion liste par liste, effacement) | `cargo test -p comicrust-tauri` | ✅ 100/100 |
| Front : `svelte-check` (185 fichiers, 0 erreur), `vite build` ; parcours simulés : focus après Échap (menu, Infos, Ouvrir un fichier, Ajouter à une liste), dernière liste reproposée depuis la Bibliothèque, vue par défaut d'une liste de lecture / intelligente, vue retrouvée, Bibliothèque inchangée, relue après redémarrage, oubliée à la suppression | local, Playwright (mode simulé) | ✅ |
| Liste « Lu » (bilan T-04 : ComicRust 0 / ComicRack 4) : même formule que ComicRack (`(LastPageRead+1)×100/PageCount`, Lu > 95, Non-lu < 10, en cours entre les deux) ; les trois chiffres relevés dans ComicRack donnent 5 329 livres pour 5 325 (listes disjointes) : voir N-02 | analyse du code | ℹ |
| Rustfmt, Clippy `-D warnings` | local + CI Windows | ✅ local ; CI à la publication |

**Non vérifiable ici** : la vraie fenêtre WebView2, ta vraie bibliothèque, ComicRack.

## 1. Installation

- [ ] **P-01 [B]** Dans **chacune** des deux copies, après la copie du zip : « ⚙ Réglages » indique **Version 0.6.2** — sinon l'ancien exe tourne encore (fermer toutes les fenêtres ComicRust et recopier). Noter la version affichée dans chaque copie : ……… / ………

## 2. Tri et groupement par liste (nouveau)

- [ ] **V-01 [B]** Ouvrir une liste de lecture jamais réglée : tri « # Ordre de la liste », croissant, sans groupe. La passer en tri **Série**, groupe **Par série**, vue **Tableau** ; ouvrir une autre liste puis revenir : les réglages sont retrouvés.
- [ ] **V-02 [B]** Revenir à « ▦ Bibliothèque » : elle garde **ses propres** tri, groupe et vue (pas ceux de la liste). Régler la Bibliothèque autrement, revenir à la liste : la liste n'a pas changé.
- [ ] **V-03 [B]** Fermer et relancer ComicRust : la Bibliothèque et chaque liste réglée retrouvent leur tri, leur sens, leur groupement et leur vue. `data\comicrust-tauri.json` contient une section `"listes"` dans `"vue"`.
- [ ] **V-04 [M]** Liste intelligente jamais réglée : elle prend le tri / groupe de la Bibliothèque ; une fois réglée, elle garde le sien. Le tri « # Ordre » n'est proposé que pour une liste de lecture.
- [ ] **V-05 [M]** Copie liée : même comportement ; `ComicDb.xml` n'est **pas** modifié par un changement de tri ou de groupe (date du fichier inchangée).
- [ ] **V-06 [m]** Supprimer une liste réglée : son entrée disparaît de `comicrust-tauri.json`.

## 3. Éditeur de listes intelligentes (repris)

- [ ] **E-01 [B]** Nouvelle liste intelligente, premier critère « Série contient » : saisir « Thanos » → « Aperçu : N livres trouvés ». Puis changer le **champ** (« Série » → « Nombre de pages »), saisir une valeur : l'aperçu affiche un nombre, **sans** message « missing field `genre` ». Répéter avec 3 champs différents, puis dans un **groupe imbriqué** ; « OK » crée la liste.
- [ ] **E-02 [B]** **Critères et opérateurs** : changer le champ (Série → Année, Note, Date d'ajout, Lu…) : opérateurs adaptés, valeur gardée si la nature est la même ; nombre « 3,5 » accepté et toujours « 3,5 » en rouvrant ; date `AAAA-MM-JJ` ; « dans les derniers … jours » ; case « Aa » (casse) sur le texte. Valeur invalide (Année « abc ») : erreur sous l'aperçu (« Critère … (Année) : « abc » n'est pas un nombre »), **OK et Appliquer grisés** (bulle « Corrige d'abord les erreurs signalées ») ; nom vide : grisés aussi.
- [ ] **E-03 [M]** Rouvrir la liste créée (✏ Modifier) : les critères changés sont enregistrés tels quels ; Ctrl+Entrée enregistre.

## 4. Série, numéro et volume déduits du nom de fichier (autonome, repris)

- [ ] **S-01 [B]** Un livre sans série en fiche, au nom du genre `Silver Surfer 04 (1987).cbz` ou `COSMIC COLLECTION #009 ….cbz`, s'affiche avec sa **série** et son **numéro** (plus « Sans série »). Noter un exemple : ………
- [ ] **S-02 [B]** Importer l'échantillon `Thanos.cbl` (« Analyser ») dans la copie autonome : **6 trouvés sur 7** (seul « Web Comic #1 » manque, volontairement). Noter trouvés / manquants : ……… / ………
- [ ] **S-03 [M]** Tri par série : les livres sans série en fiche sont classés avec leur série déduite, dans l'ordre naturel des numéros.
- [ ] **S-04 [M]** Une fiche qui a une série la **garde** ; « Infos du livre » d'un livre sans série : les champs restent **vides** (rien de déduit n'est écrit à l'enregistrement).
- [ ] **S-05 [m]** Export `.cbl` d'une liste contenant ces livres : série et numéro déduits présents dans le fichier exporté.

## 5. Clavier et listes de lecture (repris)

- [ ] **K-01 [B]** Liste de lecture ouverte, clic droit sur un livre ▸ « ⬆ Monter dans la liste » : **Suppr** retire ensuite le livre sélectionné, sans reclic ; dans l'arbre, **F2** renomme après un menu de l'arbre.
- [ ] **K-02 [B]** « Ouvrir un fichier… », « Infos du livre », « Renommer » puis **Échap** (chacun) : le focus revient sur la grille / le tableau ou l'arbre (flèches, Suppr, F2 répondent sans clic) ; la fenêtre se rouvre normalement ; Échap dans « Renommer » modifiée ne ferme pas (D-01).
- [ ] **K-03 [M]** Depuis la **Bibliothèque**, « Ajouter à une liste… » vers une liste X : la fois suivante, X est proposée (même depuis la Bibliothèque, même après un ajout par glisser-déposer sur l'arbre) ; au premier usage de la session, c'est la liste courante, sinon la dernière liste de lecture ouverte.
- [ ] **K-04 [m]** « Déplacer dans… » en thème **sombre** : la liste déroulante est lisible.
- [ ] **L-08 [B]** **Série entière** (egui v0.10.2) : ajouter à « Test » quelques livres de 3 séries, groupement « Par série », tri « # » croissant : les groupes se suivent dans l'ordre de leur **premier livre dans la liste** (pas par ordre alphabétique) ; clic sur l'**en-tête** d'une série = tous ses livres ; glisser cet en-tête entre deux autres séries : trait entre les groupes, au lâcher **toute la série** est à sa nouvelle place, dans son ordre. Ctrl+clic sur deux en-têtes puis glisser : elles bougent ensemble. Clic droit → « ⬆ Monter la série » / « ⬇ Descendre la série ».
- [ ] **L-09 [B]** Fermer et relancer la copie autonome : contenu et ordre de « Test » gardés ; aucun livre perdu (même compteur).

## 6. Écriture de `ComicDb.xml` (copie liée, non testée en tauri-v0.6.0)

**Sauvegardes PR-03 et PR-04 faites** (recette tauri-v0.6.0). ComicRack **fermé** pour les
écritures ; le relancer seulement pour vérifier.

- [ ] **W-01 [B]** **Lecture seule par défaut** (« Lecture et écriture » décochée dans ⚙ Réglages) : 🔒 à côté de « Listes » et sous le panneau « Lecture seule : active « Lecture et écriture » dans les réglages de l'édition pour modifier les listes de ComicRack (ComicRack fermé). » ; commandes de modification grisées (même bulle), glisser de livres refusé ; « Exporter en .cbl… » reste possible ; la date de `ComicDb.xml` ne change pas.
- [ ] **W-02 [B]** **Refus si ComicRack est lancé** : activer « Lecture et écriture », **ComicRack lancé** : en 3 s au plus, 🔒 « Ferme ComicRack pour modifier ses listes : il réécrit ComicDb.xml en se fermant (et toutes les 10 minutes) et effacerait les modifications. » ; commandes grisées, rien n'est écrit. Fermer ComicRack : en 3 s au plus, les commandes se dégrisent (sans relancer ComicRust).
- [ ] **W-03 [B]** ComicRack fermé : créer une liste de lecture « Test ComicRust » avec 3 livres (glisser) et la réordonner, créer un dossier, **dupliquer** une liste intelligente et changer une valeur de la **copie** ; ouvrir une autre liste intelligente et faire « OK » **sans rien changer**. À côté de `ComicDb.xml` : **`ComicDb.comicrust-origine-AAAAMMJJ-HHMMSS.xml.bak`** (créée à la première écriture, jamais supprimée) et des `ComicDb.comicrust-AAAAMMJJ-HHMMSS.xml.bak` (10 dernières).
- [ ] **W-04 [B]** **Listes non modifiées identiques** (avant de relancer ComicRack) : copier le `ComicDb.xml` actuel dans `apres\` et le comparer à `avant\` (PR-03) : seules diffèrent les lignes de « Test ComicRust », du dossier et de la copie modifiée ; **la liste ouverte puis validée sans changement est identique**, les autres listes et la partie livres aussi. Nombre de lignes différentes : ………
- [ ] **W-05 [B]** **Relancer ComicRack** : il démarre sans message d'erreur ; « Test ComicRust » (3 livres, dans l'ordre de ComicRust), le dossier et la copie modifiée y sont, avec le même nombre de livres que dans ComicRust ; tes autres listes sont intactes.
- [ ] **W-06 [M]** **Retour** : dans ComicRack, renommer « Test ComicRust » et lui ajouter un livre, fermer ComicRack : ComicRust (resté ouvert) **relit l'arbre tout seul** en quelques secondes (nouveau nom, compteur à 4).
- [ ] **W-07 [M]** Dans ComicRust, ajouter la liste importée (section 10, C-02) puis relancer ComicRack : elle y est, dans l'ordre du fichier `.cbl`.
- [ ] **W-08 [B]** Fin de section : supprimer les listes d'essai (dans ComicRust, ComicRack fermé) ; ComicRack rouvre ta bibliothèque et tes 47 listes intactes, aucune erreur ; au besoin, remettre `avant\ComicDb.xml` et la sauvegarde PR-04.

## 7. Non-régression

- [ ] **N-01 [B]** Arbre des listes de la copie liée identique à ComicRack (**47** listes) ; recherche « thanos » : **484**.
- [ ] **N-02 [M]** Liste « Divers ▸ Lu » : **ComicRack et ComicRust fermés puis relancés**, comparer les comptes de « Lu », « Non-lu » et « Lecture en cours » dans les deux (les trois listes ne se recouvrent pas : leur somme vaut le nombre de livres). En cas d'écart, noter les **titres** des livres de « Lu » dans ComicRack : ………

---

## Limites connues (ne pas compter comme anomalies)

- **Vue d'une liste dans ComicRack** : la vue que ComicRack enregistre pour chaque liste (`<Display>` dans `ComicDb.xml`) n'est pas reprise ; une liste jamais réglée dans ComicRust prend la vue par défaut (V-01, V-04). Les vues de ComicRust ne sont jamais écrites dans ComicRack.
- Les vues des sous-listes d'un **dossier supprimé** restent dans `comicrust-tauri.json`, sans effet.
- La fiche détail et la fenêtre d'édition montrent les valeurs **brutes** de la fiche (série vide) ; seules les vues, le tri, le regroupement et les `.cbl` utilisent la série du nom de fichier.
- ComicRust ne relit la base MySQL de ComicRack qu'au démarrage : une lecture faite dans ComicRack pendant que ComicRust est ouvert n'est visible qu'après relance.
- Les limites de la [tauri-v0.6.0](tauri-v0.6.0.md) restent valables.

## Anomalies

| N° test | Gravité | Description | Capture / fichier | Statut |
|---|---|---|---|---|
| | | | | |

## Bilan

| Catégorie | Total | OK | KO |
|---|---|---|---|
| Bloquants [B] | 19 | | |
| Majeurs [M] | 9 | | |
| Mineurs [m] | 3 | | |

Décision et remarques :
