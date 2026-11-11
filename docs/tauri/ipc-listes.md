# Contrat IPC : listes ComicRack (étape 5, `tauri-v0.6.0`)

Contrat entre le backend (`crates/comicrust-tauri`, module `listes`) et le front (`frontend/`).
Mêmes règles que `ipc-bibliotheque.md` : commandes asynchrones (`invoke`), promesse rejetée avec un
**texte en français**, JSON en **camelCase**, **aucune logique métier côté front** : arbre, comptes,
places de dépôt, positions de déplacement, listes de départ possibles, validation des critères,
aperçu, correspondances `.cbl` : tout est calculé ici. Comportement repris de la version egui
(`library/lists_ops.rs`, `ui/lists_ui.rs`, `ui/smart_editor.rs`, `library/mouse.rs`,
`ui/library_panels.rs`) et de `comicrust-core` (`comic_lists`, `lists_edit`, `matcher_catalog`,
`smart`, `cbl`, `store::lists`).

## Où sont les listes

| Bibliothèque | Listes lues dans | Modifiables |
|---|---|---|
| Autonome (`data\comicrust.db`) | base de ComicRust (table `comicrust_lists`) | toujours |
| ComicRack MySQL | `ComicDb.xml` trouvé d'après `ComicRack.ini` (`find_comicdb`) | seulement avec `ecritureComicrack` **et ComicRack fermé** |
| ComicRack `ComicDb.xml` | ce fichier | seulement avec `ecritureComicrack` **et ComicRack fermé** |

- `ecritureComicrack` est l'option « Lecture et écriture » de `ipc-edition.md` (réglages de
  l'édition). Pour les **listes**, elle vaut aussi en mode `ComicDb.xml` seul (comme la version
  egui : `lists_writable`) ; les **fiches** de `ComicDb.xml`, elles, restent en lecture seule.
- `ComicDb.xml` n'est **jamais** écrit si ComicRack est lancé (il réécrit ce fichier en se fermant
  et toutes les 10 minutes) ni si sa présence ne peut pas être vérifiée ; le contrôle est fait
  avant l'opération **et** juste avant le remplacement du fichier. Écriture : seul
  `<ComicLists>` est remplacé (BOM, fins de ligne, livres, réglages gardés octet pour octet),
  relecture de contrôle avant écriture, sauvegarde `ComicDb.comicrust-AAAAMMJJ-HHMMSS.xml.bak`
  (les 10 dernières) et `ComicDb.comicrust-origine-….xml.bak` (la toute première, jamais
  supprimée), fichier temporaire puis remplacement atomique. Fichier modifié depuis sa lecture
  (par ComicRack) : refus, rien n'est écrit.
- Une liste non touchée reste identique **octet pour octet** ; un critère non modifié dans
  l'éditeur est recopié tel quel (valeurs « 4,5 », dates « 01/10/2019 », attributs inconnus).
- Base de ComicRust (SQLite ou MySQL) : refus si les listes ont été modifiées ailleurs depuis
  leur lecture (autre poste, transfert) ; rien n'est écrit.
- Une seule opération de modification à la fois (sinon erreur « un enregistrement des listes est
  déjà en cours »).
- Contrôle final avant toute écriture (les deux sources) : dans les listes créées ou modifiées,
  chaque identifiant (liste, livres d'une liste de lecture, livres retirés à la main) doit être un
  GUID `8-4-4-4-12` (`XmlSerializer` lit des `Guid` : une seule valeur invalide et ComicRack ne
  chargerait plus `ComicDb.xml`) ; sinon refus « … GUID attendu », rien n'est écrit.
- Opération sans effet (livres déjà présents…) : rien n'est réécrit, aucune sauvegarde créée.
- Les listes sont relues automatiquement quand `ComicDb.xml` change (ComicRack l'a réécrit) :
  `etat_listes` et `arbre_listes` comparent la date et la taille du fichier à chaque appel.

## Types

```ts
type TypeListe = 'bibliotheque' | 'dossier' | 'lecture' | 'intelligente';
type Combinaison = 'ou' | 'et' | 'vide';   // dossier : réunion, intersection, aucun livre (rangement)

interface EtatListes {
  source: 'comicrust' | 'comicdb' | 'aucune';
  chemin?: string;           // ComicDb.xml (source 'comicdb')
  modifiable: boolean;       // commandes de modification utilisables maintenant
  lectureSeule?: string;     // sinon, la raison (infobulle des commandes grisées, 🔒 sous le panneau)
  comicrackLance: boolean;   // ComicRack.exe détecté (vérifié au plus toutes les 3 s ; toujours false hors Windows)
  erreur?: string;           // listes illisibles (ComicDb.xml introuvable, XML invalide…)
  enCours?: string;          // libellé de l'opération en cours (« Suppression de « X » »)
  version: number;           // change à chaque relecture des listes : arbre à recharger si différent
}

interface NoeudListe {
  id: string;                // GUID (casse d'origine) ; à renvoyer tel quel
  nom: string;
  type: TypeListe;
  description?: string;
  nbLivres?: number;         // livres présents dans la bibliothèque, calculés par ComicRust
  inconnu?: string;          // nbLivres absent : pourquoi (critère non pris en charge…)
  nbComicrack: number;       // compte enregistré par ComicRack à sa dernière sauvegarde (BookCount, 0 si absent)
  combinaison?: Combinaison; // dossiers
  accepteLivres: boolean;    // liste de lecture modifiable : dépôt de livres et « Ajouter à la liste »
  peutMonter: boolean;       // « ⬆ Monter » possible (pas déjà en tête de son dossier)
  peutDescendre: boolean;
  enfants?: NoeudListe[];    // dossiers
}

interface ArbreListes {
  etat: EtatListes;
  listes: NoeudListe[];      // premier niveau, dans l'ordre de ComicRack (la « Bibliothèque » comprise si elle existe)
}

interface ResultatListes {
  arbre: ArbreListes;        // listes relues après l'enregistrement : à afficher aussitôt
  selection?: string | null; // liste à sélectionner (null : toute la bibliothèque) ; absent : inchangée
  renommer?: string;         // liste à passer aussitôt en renommage (liste ou dossier qui vient d'être créé)
  creee?: string;            // identifiant de la liste créée (ses dossiers parents sont à déplier)
  info: string;              // bilan à montrer sous le panneau (« ✔ 3 livres ajoutés à « Test » »)
  sauvegarde?: string;       // nom du fichier .bak de ComicDb.xml créé
  avertissements: string[];  // problèmes sans conséquence (ancienne sauvegarde non supprimée…)
}
```

## Arbre et sélection

| Commande | Arguments | Résultat |
|---|---|---|
| `etat_listes` | – | `EtatListes`. Léger : à appeler toutes les 3 s environ pour griser / dégriser (ComicRack lancé ou fermé) et recharger l'arbre si `version` a changé. |
| `arbre_listes` | – | `ArbreListes`. Comptes calculés sur la bibliothèque ouverte (listes intelligentes comprises, résultat gardé en cache jusqu'au prochain changement de la base, des listes ou du jour). |
| `bilan_listes` | – | `string` : pour chaque liste, « ComicRust n / ComicRack m », lignes différentes marquées « ≠ » (bouton « 📋 Copier le bilan des listes », comparaison avec ComicRack des recettes v0.5.x). |

Infobulle d'une liste (comme egui) : description, « ComicRust : n livres » (ou « pas encore
calculable » + `inconnu`), « ComicRack (dernier enregistrement) : m livres ».

**Sélection d'une liste** : `lister_livres` reçoit deux ajouts (`ipc-bibliotheque.md`) :

```ts
interface RequeteLivres {
  // … champs existants …
  liste?: string;          // identifiant de la liste affichée ; absent : toute la bibliothèque
  tri: string;             // + 'ordre' : « Ordre de la liste » (rang dans la liste ; synonymes 'liste', 'position')
}
interface LivreResume {
  // … champs existants …
  rang?: number;           // avec `liste` : rang du livre dans la liste (1 = premier), colonne « # »
}
```

- Liste de lecture : livres **dans l'ordre de la liste** ; liste intelligente : contenu calculé ;
  dossier : réunion / intersection de ses sous-listes ; « Bibliothèque » : tous les livres.
  Liste au contenu inconnu : 0 livre. Identifiant inconnu : erreur « liste introuvable ».
- Tri `'ordre'` : par rang dans la liste. **Avec un groupe**, les groupes se suivent dans l'ordre
  de leur **premier livre dans la liste** (pas par ordre alphabétique) : déplacer une série dans la
  liste la déplace à l'écran (v0.10.2). Sans `liste`, `'ordre'` = ordre de la bibliothèque.
- À la sélection d'une **liste de lecture**, le front propose le tri `'ordre'` croissant (comme
  ComicRack). Le réordonnancement (glisser, Monter/Descendre) n'est possible que dans une liste de
  lecture modifiable triée `'ordre'` **croissant** ; sinon le front affiche « Trie la vue dans
  l'ordre de la liste (« # » croissant) pour réordonner ».
- Une liste supprimée pendant qu'elle est affichée (`supprimer_liste` avec `listeAffichee`) :
  `ResultatListes.selection = null`.

## Gestion de l'arbre

| Commande | Arguments | Résultat |
|---|---|---|
| `creer_liste` | `{demande: {type: 'lecture' \| 'dossier', nom?: string, parent?: string, position?: number, livres?: string[], apresListe?: string}}` | `ResultatListes` (`selection`, `renommer`, `creee` = la nouvelle liste). Nom par défaut « Nouvelle liste » / « Nouveau dossier ». Emplacement : `parent` + `position` (fin si absent) ; ou `apresListe` = juste après cette liste dans son dossier (ComicRack : après la liste sélectionnée). `livres` : liste de lecture créée avec ces livres (« Nouvelle liste de lecture… » du menu des livres ; doublons retirés). |
| `renommer_liste` | `{id, nom}` | `ResultatListes`. Nom vide refusé ; caractères interdits en XML retirés. |
| `modifier_liste` | `{id, nom?, description?, combinaison?: Combinaison}` | `ResultatListes`. Fenêtre « Modifier… » (liste de lecture, dossier). Seuls les champs fournis **et différents** sont écrits. Description vide = retirée. |
| `dupliquer_liste` | `{id}` | `ResultatListes` (« Copie de X » juste après, sous-listes comprises, nouveaux identifiants ; sélectionnée). La « Bibliothèque » ne peut pas être copiée. |
| `infos_suppression` | `{id}` | `{nom: string, sousListes: number, dependantes: string[]}` pour la fenêtre de confirmation : « Ce dossier contient n sous-listes : elles seront supprimées avec lui », « Ces listes intelligentes partent de cette liste ; elles partiront de toute la bibliothèque : … », « Les livres eux-mêmes ne sont pas supprimés. » |
| `supprimer_liste` | `{id, listeAffichee?: string}` | `ResultatListes` (`selection: null` si `listeAffichee` a disparu avec elle). La « Bibliothèque » ne peut pas être supprimée. |
| `destinations_liste` | `{id}` | `{id: string \| null, nom: string, profondeur: number, actuel: boolean}[]` : menu « Déplacer dans » (« (premier niveau) » avec `id:null`, puis les dossiers sauf la liste et ses sous-dossiers ; `actuel` : son dossier actuel, à griser). |
| `deplacer_liste` | `{id, parent: string \| null, position?: number}` | `ResultatListes`. `position` : rang parmi les nouvelles voisines (fin si absent). Impossible dans elle-même ou un descendant. |
| `decaler_liste` | `{id, haut: boolean}` | `ResultatListes`. « ⬆ Monter » / « ⬇ Descendre » d'un cran dans son dossier (voir `peutMonter`). |
| `place_depot_liste` | `{glissee, cible, place: 'avant' \| 'dans' \| 'apres'}` | `boolean` : dépôt possible (trait de couleur normale) ou non (trait d'erreur). Zones : dossier = quart haut `avant`, quart bas `apres`, milieu `dans` ; autre liste = moitié haute `avant`, basse `apres`. |
| `deposer_liste` | `{glissee, cible, place}` | `ResultatListes`. Erreur lisible si impossible (sur elle-même, dans sa propre branche, « dans » une liste qui n'est pas un dossier). |
| `convertir_en_lecture` | `{id}` | `ResultatListes`. Liste intelligente → liste de lecture contenant ses livres **actuels** (calculés ici), identifiant, nom, description et affichage gardés ; les listes qui en partent continuent de fonctionner. Contenu inconnu : erreur « … ne peut pas être convertie : ComicRust ne connaît pas son contenu (…) ». |
| `infos_conversion` | `{id}` | `{nom: string, nbLivres: number}` pour la confirmation : « « X » devient une liste de lecture contenant ses n livres actuels ; ses critères sont abandonnés. » |

Libellés des commandes et menus (comme egui) : « Nouveau ▸ » (📄 Nouvelle liste de lecture,
🔍 Nouvelle liste intelligente…, 📁 Nouveau dossier ; sur un dossier : dedans, à la fin ; sur une
liste : juste après elle), « ✏ Modifier… » / « ✏ Modifier les critères… » (ouvrable en lecture
seule pour consulter les critères), « Renommer (F2) », « ⎘ Dupliquer », « 🗑 Supprimer… »,
« 📄 Convertir en liste de lecture… », « ⬆ Monter », « ⬇ Descendre », « Déplacer dans ▸ ».

## Livres et listes de lecture

| Commande | Arguments | Résultat |
|---|---|---|
| `ajouter_livres_liste` | `{id, livres: string[], position?: number}` | `ResultatListes` (« n livres ajoutés à « X » (m déjà dans la liste) »). Fin de liste si `position` absent. Livres déjà présents ignorés. Menu « ➕ Ajouter n livres à la liste ▸ » (arbre des listes de lecture : `accepteLivres`) et dépôt de livres sur une liste du panneau. |
| `depot_livres_possible` | `{id, listeAffichee?: string}` | `string \| null` : `null` = dépôt accepté (liste mise en évidence), sinon la raison (curseur « interdit ») : « La bibliothèque contient déjà tous les livres », « Dossier : dépose les livres sur une liste de lecture », « Liste intelligente : son contenu dépend de ses critères », « Les livres sont déjà dans cette liste », ou la raison de la lecture seule. |
| `retirer_livres_liste` | `{id, livres}` | `ResultatListes` (« n livres retirés de « X » »). Touche **Suppr** dans une liste de lecture : retire de la liste, **ne supprime aucun fichier**. |
| `decaler_livres_liste` | `{id, livres, haut: boolean}` | `ResultatListes`. « ⬆ Monter / ⬇ Descendre dans la liste » : les livres choisis, ensemble, d'un cran. Rien à faire (déjà en tête) : `info` vide, rien d'écrit. |
| `deplacer_livres_liste` | `{id, livres, requete: RequeteLivres, insertion: number}` | `ResultatListes`. Glisser-déposer **dans la vue** : `insertion` = rang d'insertion dans la vue décrite par `requete` (0 = avant le premier livre affiché, `total` = après le dernier). Le backend calcule le repère (premier livre non déplacé à partir de ce rang, sinon le dernier avant lui) puis la position dans la liste : les livres déplacés gardent leur ordre. Dépôt sans effet : rien d'écrit. |
| `deplacer_groupes_liste` | `{id, requete: RequeteLivres, groupes: string[], insertion: number}` | `ResultatListes`. **Séries entières** (v0.10.1/v0.10.2) : vue groupée (`requete.groupe`), clic sur l'en-tête d'un groupe = tous ses livres ; Ctrl/Maj pour plusieurs groupes. `groupes` : titres des groupes déplacés tels qu'affichés (« (sans série) » compris) ; `insertion` : frontière entre groupes (0 = avant le premier groupe, nombre de groupes = après le dernier). Les livres des groupes choisis se placent devant le premier livre du groupe visé (sinon en fin de liste), dans leur ordre. Aucun livre perdu. |
| `decaler_groupe_liste` | `{id, requete, groupe: string, haut: boolean}` | `ResultatListes`. Clic droit sur un en-tête : « Monter / Descendre la série ». |
| `groupes_vue` | `{requete: RequeteLivres}` | `{titre: string, nombre: number}[]` : groupes de la vue dans l'ordre (pour placer le trait de dépôt entre deux groupes sans tout charger). |

`requete` doit être la requête de la vue affichée (même `liste`, `recherche`, `tri: 'ordre'`,
`sensTri: 'asc'`, `groupe`) : le backend refait la même vue pour traduire rangs et groupes en
livres. Refus si la liste n'est pas une liste de lecture, si la vue n'est pas triée dans l'ordre de
la liste croissant, ou en lecture seule.

## Éditeur de listes intelligentes

Fenêtre « Liste intelligente » (modèle `SmartListDialog` de ComicRack, comme egui) : nom,
description, « Correspond à [tous ▾ / au moins un ▾] des critères suivants, dans : [liste de
départ ▾] » + case « pas dans cette liste », une ligne par critère (case **Non**, champ,
opérateur, valeur(s), unité, case « casse » pour le texte, menu ☰ : nouveau critère, nouveau
groupe, monter, descendre, supprimer ; ✖), groupes imbriqués (leur « tous / au moins un » et leur
case Non), limite (nombre de livres / Mo / Go ; au hasard / par position / par série), livres
retirés à la main dans ComicRack (« n livres retirés à la main » + « Réinitialiser »), aperçu
du nombre de livres trouvés.

```ts
type ModeCriteres = 'tous' | 'auMoinsUn';
type NatureValeur = 'texte' | 'nombre' | 'date' | 'ouiNon' | 'manga' | 'activation'
                  | 'valeurPerso' | 'expression';

interface Operateur { op: number; libelle: string; valeurs: 0 | 1 | 2; unite?: string } // unite : 'jours'
interface TypeCritere {
  type: string;            // xsi:type (« ComicBookSeriesMatcher »)
  libelle: string;         // « Série »
  nature: NatureValeur;
  unite?: string;          // unité de la valeur (« Mo »…)
  statSerie: boolean;      // statistique de série (catégorie à part)
  evalue: boolean;         // ComicRust sait le calculer (sinon l'aperçu dira « pas encore pris en charge »)
  texte: boolean;          // case « casse » utile (IgnoreCase)
  operateurs: Operateur[];
}
interface CatalogueCriteres {
  categories: {nom: string; criteres: TypeCritere[]}[]; // Texte, Nombres, Dates, Oui / Non, Statistiques de série, Modules de ComicRack ; libellés triés
  defaut: {type: string; op: number};                    // nouveau critère : « Série contient »
}

type NoeudCritere = Critere | GroupeCriteres;
interface Critere {
  genre: 'critere';
  non: boolean;
  type: string; op: number;
  valeur: string; valeur2: string;  // valeurPerso : valeur = nom de la valeur, valeur2 = texte cherché
  ignorerCasse: boolean;
  origine?: string;        // fourni par lire_criteres (chemin « 0/2/1 ») : À RENVOYER TEL QUEL
  modifiable: boolean;     // false : critère absent du catalogue, affiché en lecture seule et gardé tel quel
  libelle?: string;        // critère non modifiable : son libellé
}
interface GroupeCriteres {
  genre: 'groupe';
  non: boolean; mode: ModeCriteres;
  noeuds: NoeudCritere[];
  origine?: string;        // idem : attributs d'origine du groupe (Collapsed…) gardés
}
interface Limite { valeur: string; unite: 'livres' | 'mo' | 'go'; selection: 'hasard' | 'position' | 'serie' }
interface CriteresListe {
  nom: string; description: string;
  mode: ModeCriteres;
  noeuds: NoeudCritere[];
  depart?: string | null;      // liste de départ (null : toute la bibliothèque)
  pasDansDepart: boolean;
  limite?: Limite | null;      // null : pas de limite
  retires: number;             // livres retirés à la main dans ComicRack (lecture)
  reinitialiserRetires?: boolean; // « Réinitialiser » : les remettre dans la liste
}
interface ListeDepart { id: string; nom: string; profondeur: number; type: TypeListe }
interface EditeurListe {
  id?: string;                 // absent : nouvelle liste
  criteres: CriteresListe;
  listesDepart: ListeDepart[]; // sans elle-même ni celles qui dépendent d'elle (pas de boucle), sans la « Bibliothèque »
  lectureSeule?: string;       // présent : consultation seulement (boutons OK / Appliquer grisés)
}
interface Apercu { nbLivres?: number; inconnu?: string; erreurs: string[] }
```

| Commande | Arguments | Résultat |
|---|---|---|
| `catalogue_criteres` | – | `CatalogueCriteres` (`matcher_catalog` de comicrust-core, libellés français de ComicRack corrigés). |
| `nouveau_critere` | `{typeCritere: string, ancien?: Critere}` | `Critere` : changement de champ ; opérateur et valeurs gardés si la nature de la valeur est la même, sinon opérateur 0 et valeurs vides. Sans `ancien` : critère vide de ce type. |
| `lire_criteres` | `{id?: string}` | `EditeurListe`. Sans `id` : nouvelle liste (« Nouvelle liste intelligente », un critère « Série contient » vide). Dates affichées `AAAA-MM-JJ`. |
| `apercu_criteres` | `{id?: string, criteres: CriteresListe}` | `Apercu` : erreurs de saisie (« Critère 2 (Année) : « abc » n'est pas un nombre ») ou nombre de livres trouvés (comme la liste une fois enregistrée : liste de départ, limite, sous-groupes). À appeler 250 ms après la dernière modification. OK / Appliquer grisés tant que `erreurs` n'est pas vide. |
| `enregistrer_criteres` | `{id?: string, criteres: CriteresListe, parent?: string, position?: number, apresListe?: string}` | `ResultatListes` (nouvelle liste : `creee`, `selection`). « OK » ferme après succès ; « Appliquer » garde la fenêtre (le front rappelle `lire_criteres` avec `creee`/`id`). Seul ce qui a changé est réécrit (nom, description, critères). Refus si la liste de départ crée une boucle. |

Valeurs saisies (contrôlées ici, comme egui) : nombre (virgule ou point ; écrit avec le séparateur
décimal du poste, relu par ComicRack), note de 0 à 5, date `AAAA-MM-JJ`, nombre de jours
(« dans les derniers … jours »), expression régulière (parenthèses / crochets équilibrés), nom de
valeur personnalisée obligatoire, limite = entier positif.

## Import / export `.cbl`

```ts
interface ManquantCbl { rang: number; serie: string; numero: string; volume?: number; annee?: number }
interface RapportCbl {
  nom: string;               // <Name> de la liste, sinon le nom du fichier
  total: number;
  trouves: number;
  parIdentifiant: number;    // retrouvés par <Id> (GUID ComicRack)
  parSerie: number;          // retrouvés par série + numéro (+ volume)
  doublons: number;          // entrées qui désignent un livre déjà retenu
  manquants: ManquantCbl[];
  texte: string;             // bilan copiable (« Liste « X » : 318 / 321 numéros trouvés » + manquants)
}
```

| Commande | Arguments | Résultat |
|---|---|---|
| `analyser_cbl` | `{chemin}` | `RapportCbl` sans rien créer (aperçu avant import). Fichier de plus de 32 Mo refusé (« trop gros ») ; entités d'un DOCTYPE jamais développées ; `manquants` limité aux 1 000 premiers (le nombre exact est `total - trouves`, le `texte` finit par « … et n autres manquants »). |
| `importer_cbl` | `{chemin, nom?: string, parent?: string, position?: number, apresListe?: string}` | `{resultat: ResultatListes, rapport: RapportCbl}` : nouvelle liste de lecture (nom = `nom`, sinon celui du `.cbl`) avec les livres trouvés, dans l'ordre du fichier. Correspondance : `<Id>` dans la bibliothèque, sinon série + numéro (normalisés : casse, articles, zéros) + volume si les deux le connaissent. Listes en lecture seule : refus. |
| `exporter_cbl` | `{id, chemin}` | `{chemin: string, livres: number}` : écrit le `.cbl` (format ComicRack : `Series`, `Number`, `Volume`, `Year`, `Format`, `<Id>`) des livres de la liste, dans l'ordre affiché par `'ordre'`. Possible en lecture seule (aucune liste modifiée). Fichier écrit dans un temporaire puis renommé ; extension `.cbl` ajoutée si absente. |

## Erreurs courantes (textes à afficher tels quels)

- « Ferme ComicRack pour modifier ses listes : il réécrit ComicDb.xml en se fermant (et toutes les
  10 minutes) et effacerait les modifications. »
- « Impossible de vérifier si ComicRack est lancé (…) : par prudence, ses listes ne sont pas
  modifiées. »
- « Lecture seule : active « Lecture et écriture » dans Réglages ▸ Bibliothèque ComicRack pour
  modifier les listes de ComicRack (ComicRack fermé). »
- « … a été modifié depuis sa lecture (par ComicRack ?) : recharge les listes puis recommence »
- « Les listes ont été modifiées ailleurs (autre poste ou transfert depuis ComicRack) … »
- « un enregistrement des listes est déjà en cours »

Le front préfixe les erreurs d'une opération par « <opération> : rien n'a été enregistré. ».

## Simulation (recette hors Windows)

Variable d'environnement `COMICRUST_SIMULER_COMICRACK` (n'importe quelle valeur) : ComicRack est
considéré comme lancé (comme la version egui).
