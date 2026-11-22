# Contrat IPC — regroupements et tableau (lot L5)

Écarts 9, 17 et 19 de `audit-parite.md`. Aucune nouvelle commande : le lot étend le contrat de
`lister_livres`, `lister_piles`, `groupes_vue`, `deplacer_groupes_liste`, `decaler_groupe_liste`
et de la section `vue` des réglages.

## Regroupements (`RequeteLivres.groupe`)

Neuf valeurs, dans l'ordre du menu egui (`GroupKey::ALL`) :

| `groupe`     | Libellé du menu     | Titre de groupe d'un livre (`Regroupement::titre`, `tri.rs`) |
|--------------|---------------------|---------------------------------------------------------------|
| `aucun`      | Aucun groupement    | —                                                             |
| `serie`      | Par série           | série affichée, `(sans série)`                                |
| `editeur`    | Par éditeur         | éditeur, `(sans éditeur)`                                     |
| `scenariste` | Par scénariste      | scénariste, `(sans scénariste)`                               |
| `annee`      | Par année           | année en texte, `(sans année)` (année 0 = absente)            |
| `arc`        | Par arc             | arc, `(sans arc)`                                             |
| `genre`      | Par genre           | genre, `(sans genre)`                                         |
| `format`     | Par format          | format, `(sans format)`                                       |
| `etat`       | Par état de lecture | `Non lu` (0 % ou inconnu), `En cours`, `Lu` (100 %)           |

Ordre des groupes (identique à `sorting::sort` d'egui) : titres comparés **en minuscules**, sens
inversé si le tri est décroissant (donc années croissantes, ou décroissantes avec `sensTri: desc` ;
`(sans année)` avant les chiffres). Avec le tri `ordre` d'une liste, les groupes suivent leur premier
livre dans la liste. Seule différence voulue avec egui : la série est la série **affichée** (déduite du
nom de fichier à défaut de fiche), comme les piles.

Valeur inconnue dans les réglages (`vue.groupe`, `vue.listes.<id>.groupe`) : ignorée.

## Titres de groupe renvoyés

- `LivreResume.groupe` : titre du groupe du livre (absent si `groupe = aucun`). Le front compare ces
  titres pour savoir où commence un groupe : c'est exactement la clé de `groupes_vue`.
- `Pile.groupe` : titre du groupe du **premier livre** de la pile (comme `stacked_rows` d'egui), présent
  pour tous les regroupements sauf `aucun` et `serie` (redondant avec les piles). Le front dessine un
  en-tête « titre — N livres » (N = somme des livres des piles consécutives du groupe) avant chaque
  groupe ; il n'est ni cliquable ni glissable.

## Listes de lecture

`deplacer_groupes_liste` et `decaler_groupe_liste` fonctionnent pour **tous** les regroupements : un
groupe est identifié par le titre de `groupes_vue` (ex. `1991`, `Non lu`, `(sans scénariste)`). Menu du
tableau / de la grille : « Monter / Descendre la série » (regroupement par série) ou « le groupe » (autres).
Message d'erreur sans regroupement : « vue non groupée : groupe la vue (par série, par exemple)… ».

## Colonnes du tableau

Champs ajoutés à `LivreResume` (tous optionnels) : `volume`, `mois`, `scenariste`, `note` (0 à 5),
`ajoute` (date ISO), `fichier`, `avancement` (% , absent si 0).

Les onze colonnes, dans l'ordre d'egui (`COLS` de `library_table.rs`) — `frontend/src/lib/colonnes.ts` :

| id           | Titre      | Format                                | Tri au clic (`ClefTri`) |
|--------------|------------|---------------------------------------|-------------------------|
| `serie`      | Série      | série                                 | `serie`                 |
| `numero`     | N°         | `v2 #3`, `v2`, `#3`                   | —                       |
| `titre`      | Titre      | titre (⚠ si fichier manquant)         | `titre`                 |
| `parution`   | Parution   | `03/1991`, `1991`                     | `parution`              |
| `editeur`    | Éditeur    | éditeur                               | —                       |
| `scenariste` | Scénariste | scénariste                            | —                       |
| `pages`      | Pages      | nombre                                | `pages`                 |
| `lu`         | Lu         | `45 %` (vide à 0)                     | `avancement`            |
| `note`       | Note       | `★★★☆☆` (vide sans note)              | `note`                  |
| `ajoute`     | Ajouté     | `jj/mm/aaaa`                          | `ajout`                 |
| `fichier`    | Fichier    | chemin complet                        | `fichier`               |

La colonne « # » (rang dans la liste) reste en tête dans une liste de lecture et n'est pas concernée par
les réglages. Le bouton `☰` (ou un clic droit sur l'en-tête) ouvre la liste des colonnes à cocher ;
au moins une colonne reste visible. Redimensionnement : poignée à droite de chaque en-tête (double-clic :
largeur d'egui). Les groupes autres que la série marquent leur première ligne par une étiquette `▸ titre`
(clic = sélectionner le groupe) ; le regroupement par série garde l'en-tête dans la colonne Série.

## Réglages : `vue.tableau`

```json
"vue": { "tableau": { "colonnes": ["serie", "titre", "note"], "largeurs": { "titre": 320 } } }
```

- `colonnes` : colonnes visibles, **toujours dans l'ordre canonique** ci-dessus ; absent = toutes.
  Identifiants inconnus et doublons ignorés ; liste sans identifiant valide = absent (jamais vide).
- `largeurs` : pixels par colonne, ramenés dans 30 à 2000 (entrées inconnues ou non numériques ignorées).
- Le sous-objet est **remplacé en entier** à chaque `enregistrer_preferences_vue` (comme `lecteur`) ; un
  objet vide `{}` efface la section (« tout afficher, largeurs d'origine »). Bloc séparé : il ne touche
  ni au tri, ni au groupe, ni aux vues des listes.
- Lecture tolérante champ par champ (fichier modifié à la main).
