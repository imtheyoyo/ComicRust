# Contrat IPC : infos et édition (étape 3, `tauri-v0.4.0`)

Contrat entre le backend (`crates/comicrust-tauri`, module `edition`) et le front (`frontend/`).
Mêmes règles que `ipc-bibliotheque.md` : commandes asynchrones (`invoke`), promesse rejetée avec un
**texte en français** en cas d'erreur, JSON en **camelCase**, **aucune logique métier côté front**
(valeurs affichées, valeurs multiples, validation, suggestions, modèles de nom, conflits : tout est
calculé ici). Comportement repris de la version egui (`ui/info_editor.rs`, `library/info_form.rs`,
`library/files.rs`, `library/comicrack_write.rs`, `scripts/apply.rs`) et de `comicrust-core`
(`edit`, `writer`, `db::write`).

## Où sont écrites les modifications

| Bibliothèque | Fiches (`appliquer_modifications`, renommage) | Fichiers (ComicInfo.xml, conversion) |
|---|---|---|
| Autonome (SQLite `data\comicrust.db`) | base de ComicRust | oui ; la fiche suit (infos écrites, nouveau fichier) |
| ComicRack MySQL, **lecture seule** (défaut) | refusées (`lectureSeule`) | oui, **fichiers seulement** (rien dans ComicRack) ; `rangerOriginal` refusé |
| ComicRack MySQL, **lecture et écriture** | base MySQL de ComicRack (protocole `db/write.rs`) | oui ; la fiche ComicRack suit |
| ComicRack `ComicDb.xml` | refusées (ComicRack réécrit ce fichier lui-même) | oui, fichiers seulement ; `rangerOriginal` refusé |

Écriture MySQL (`comicrust_core::db::update_books`) : seuls les éléments touchés de la fiche sont
réécrits (le reste du XML recopié tel quel), compteur `changes.update_counter` de ComicRack
incrémenté, condition optimiste sur `comics.update_counter` (fiche modifiée entre-temps dans
ComicRack : **conflit** signalé, rien d'écrasé). `ComicInfoIsDirty` passe à vrai quand un champ du
ComicInfo change, à faux après l'écriture du fichier (seulement si la fiche n'a pas changé depuis).

Lecture (avancement) en mode **lecture et écriture** : à la fermeture du livre (ou à la fin du livre,
ou à la fermeture de l'application), `CurrentPage`, `LastPageRead` (jamais reculée), `Opened`
(heure d'ouverture), `OpenCount + 1`, `NewPages = 0` et `PageCount` s'il manque sont écrits dans la
fiche ComicRack, comme la version egui. La lecture reste aussi gardée dans `data\avancement.json`.

## Réglages (`data\comicrust-tauri.json`, section `edition`)

```ts
interface ReglagesEdition {
  ecritureComicrack: boolean;  // option « Lecture et écriture » du mode lié (défaut false = lecture seule)
  ecrireDansFichier: boolean;  // case « Écrire aussi les infos dans le fichier » (défaut false)
  rangerOriginal: boolean;     // conversion : ranger l'original dans « _originaux » (défaut false)
  modeleNom?: string;          // dernier modèle de nom utilisé
  dossierCible?: string;       // dernier dossier cible du renommage
}
```

| Commande | Arguments | Résultat |
|---|---|---|
| `lire_reglages_edition` | – | `ReglagesEdition` |
| `enregistrer_reglages_edition` | `{reglages: Partial<ReglagesEdition>}` | `ReglagesEdition` à jour (**fusion** : seuls les champs fournis changent). |

`ecritureComicrack` n'a d'effet qu'en mode ComicRack **MySQL** (sans effet pour `ComicDb.xml`, qui
reste en lecture seule pour les fiches, comme la version egui). Il est pris en compte à l'opération
suivante (pas besoin de rouvrir la bibliothèque). Le mot de passe MySQL n'est jamais écrit ici.

## Éditeur d'infos

```ts
type TypeChamp = 'texte' | 'texteLong' | 'liste' | 'entier' | 'note' | 'prix'
               | 'ouiNon' | 'manga' | 'booleen' | 'date' | 'langue';

interface Choix { valeur: string; libelle: string; }   // listes déroulantes (ouiNon, manga, booleen)

interface ChampFiche {
  nom: string;            // nom ComicRack ('Series', 'Writer'…) ou 'CustomValue:<clé>'
  libelle: string;        // libellé français (« Série », « Valeur perso. … » pour l'onglet Personnalisé : la clé)
  type: TypeChamp;
  valeur: string;         // texte à afficher et modifier ('' si vide ou si multiple)
  multiple: boolean;      // plusieurs livres aux valeurs différentes : afficher « (valeurs multiples) »
  choix?: Choix[];        // ouiNon : ''/No/Yes ; manga : ''/No/Yes/YesAndRightToLeft ; booleen : Oui/Non
  min?: number; max?: number;  // entier
  suggestions: boolean;   // le champ propose les valeurs de la bibliothèque (proposer_valeurs)
  dansComicInfo: boolean; // champ du ComicInfo.xml (le modifier rend la fiche « à écrire dans le fichier »)
  indication?: string;    // texte d'aide du champ vide : 'JJ/MM/AAAA', 'valeurs séparées par des virgules'…
}

interface Onglet { nom: string; champs: string[]; }   // noms de ChampFiche, dans l'ordre

interface LivreEdite {
  id: string; titre: string; fichier?: string; urlCouverture: string;
  pages: number; tailleFichier?: number; progression?: number;
  infosAEcrire: boolean;  // ComicInfoIsDirty : infos pas encore écrites dans le fichier
  cbz: boolean;           // le fichier reçoit les infos (CBZ/ZIP)
}

interface FicheEdition {
  ids: string[];
  multiple: boolean;
  livres: LivreEdite[];   // dans l'ordre des ids (livres inconnus ignorés)
  onglets: Onglet[];      // Résumé, Détails, Intrigue et notes, Catalogue, Personnalisé (valeurs perso.)
  champs: ChampFiche[];   // tous les champs, valeurs personnalisées comprises (clés de tous les livres)
  lectureSeule?: string;  // présent : consultation seulement, avec la marche à suivre
  ecrireDansFichierPossible: boolean; // au moins un livre en CBZ
  ecrireDansFichier: boolean;         // valeur mémorisée de la case (réglage)
  seuilConfirmation: number;          // au-delà, les opérations demandent confirmation (voir « Garde-fou »)
}

interface ChampModifie { nom: string; valeur: string; }  // valeur '' : vider le champ
interface ErreurChamp { nom: string; message: string; }  // message sans le libellé
interface Proposition { valeur: string; texte: string; } // texte : contenu du champ si on la choisit
```

Onglets et champs (identiques à la version egui, d'après `ComicBookDialog` de ComicRack) :

- **Résumé** : Series, Number, Count, Volume, Title, AlternateSeries, AlternateNumber, AlternateCount,
  Year, Month, Day, Writer, Penciller, Inker, Colorist, Letterer, CoverArtist, Editor, Translator, Rating
- **Détails** : Publisher, Imprint, Genre, Format, AgeRating, LanguageISO, BlackAndWhite, Manga,
  SeriesComplete, StoryArc, SeriesGroup, Tags, CommunityRating, Web, PreferredFrontCover, EnableProposed
- **Intrigue et notes** : Characters, Teams, MainCharacterOrTeam, Locations, ScanInformation, Summary, Notes, Review
- **Catalogue** : BookStore, BookOwner, BookAge, BookCondition, BookCollectionStatus, BookPrice,
  BookLocation, ISBN, Released, Checked, BookNotes
- **Personnalisé** : `CustomValue:<clé>` (toutes les clés des livres, sans casse, triées)

Formats affichés et saisis : note `4,5`, date `17/05/2024`, booléen `Oui`/`Non`, « Unknown » = vide.
Les valeurs saisies sont validées et normalisées par `comicrust_core::edit` (mêmes règles que la
version egui) : un champ vide **vide** la fiche.

| Commande | Arguments | Résultat |
|---|---|---|
| `lire_fiches` | `{ids: string[]}` | `FicheEdition`. Erreur si aucun livre connu. Un livre : valeurs du livre. Plusieurs : valeur commune, sinon `multiple:true` et `valeur:''`. |
| `proposer_valeurs` | `{champ, texte, tout?: boolean, max?: number}` | `Proposition[]`. Valeurs déjà présentes dans la bibliothèque (une valeur de liste à part, sans doublon de casse, ordre naturel) plus les valeurs usuelles (Public, Oui/Non…). `texte` : saisie actuelle ; pour un champ liste, seule la dernière valeur (après la dernière virgule) est complétée. Sans `tout` : celles qui **commencent** par la saisie puis celles qui la **contiennent**, au plus `max` (défaut 8), rien si la saisie est vide ou déjà complète. `tout:true` (bouton ▾) : toutes celles qui contiennent la saisie (sans limite). Champ sans suggestions : `[]`. |
| `verifier_champs` | `{champs: ChampModifie[]}` | `ErreurChamp[]` (vide : tout est accepté). À appeler pendant la saisie (après ~300 ms) pour afficher l'erreur sous le champ ; « OK / Appliquer » doivent être désactivés tant qu'il y en a. Le texte « (valeurs multiples) » est toujours refusé (garde-fou : jamais écrit tel quel dans une fiche). |
| `appliquer_modifications` | `{demande: DemandeModification}` | `BilanOperation`. Voir ci-dessous. |
| `ajouter_valeur_perso` | `{cle: string, existantes: string[]}` | `ChampFiche` vide de la nouvelle valeur personnalisée. Erreur lisible si la clé est vide, contient `,` `=` ou un caractère de contrôle, ou existe déjà (sans casse) parmi `existantes`. |

```ts
interface DemandeModification {
  ids: string[];
  champs: ChampModifie[];       // SEULEMENT les champs modifiés par l'utilisateur
  ecrireDansFichier: boolean;   // écrire aussi ComicInfo.xml dans les CBZ (mémorisé dans les réglages)
  confirme?: boolean;           // garde-fou : true après la confirmation de l'utilisateur
}
```

Règles du front (comme la fenêtre egui) :

- un livre : un champ est « modifié » si son texte diffère de `valeur` ; plusieurs livres : un champ
  `multiple` est modifié s'il est saisi (non vide) ou **vidé explicitement** (bouton ✖ « vider pour
  tous les livres », envoyé avec `valeur:''`) ; effacer une saisie sur un champ multiple = non modifié ;
- bouton ↺ : revenir à `valeur` (plus envoyé) ;
- « OK » ferme après le bilan, « Appliquer » garde la fenêtre et rappelle `lire_fiches` ;
- Précédent / Suivant (un seul livre) : `appliquer_modifications` d'abord s'il y a des modifications,
  puis `lire_fiches` du voisin (l'ordre est celui de la vue affichée, connu du front).

`appliquer_modifications` : refusée (erreur) en lecture seule, si une valeur est refusée
(`verifier_champs` non vide : message qui liste les champs) ou si une opération est déjà en cours.
Ordre : 1. fiches (base de la bibliothèque) ; 2. si `ecrireDansFichier`, `ComicInfo.xml` des livres
en CBZ, d'après la fiche **relue après l'écriture** (les autres formats sont « ignorés » dans le bilan).
Rien n'est écrit pour un champ non envoyé ; un champ envoyé égal à la valeur actuelle n'écrit rien
(« déjà à jour »).

## Fichiers

| Commande | Arguments | Résultat |
|---|---|---|
| `ecrire_infos_fichiers` | `{ids, confirme?}` | `BilanOperation`. Écrit la fiche dans le `ComicInfo.xml` des CBZ (« Écrire les infos dans le fichier »). Autres formats : ignorés. Fiches relues dans MySQL au moment d'écrire en mode ComicRack MySQL. |
| `convertir_cbr_cbz` | `{ids, rangerOriginal?: boolean, confirme?}` | `BilanOperation`. CBR/RAR, CB7/7Z, CBT/TAR → CBZ à côté de l'original (pages recopiées telles quelles, ComicInfo.xml d'après la fiche). L'original reste en place, ou est rangé dans `_originaux` (même dossier, nom libre `a (2).cbr`) si `rangerOriginal` (défaut : réglage). Autonome / ComicRack en écriture : la fiche pointe ensuite sur le CBZ. L'original n'est rangé qu'**après** la mise à jour de la fiche (fiche non mise à jour : original laissé à sa place, erreur dans le bilan). Fiche illisible (base ComicRack injoignable) : pas de conversion, erreur. Erreur si `rangerOriginal` en lecture seule. |

Écriture d'un CBZ (`comicrust_core::writer`) : archive réécrite dans un fichier temporaire du même
dossier (pages recopiées sans recompression), vérifiée, puis **remplacement atomique** de l'original
(nouvelles tentatives pendant ~3 s si le fichier est momentanément refusé : antivirus, partage SMB,
ComicRack). Aucun fichier n'est gardé ouvert ; les livres ouverts dans le lecteur relisent le fichier
à la page suivante. Couvertures en cache et pages en cache du livre sont oubliées après l'écriture :
l'`urlCouverture` d'un livre dont la couverture a pu changer reçoit un suffixe `?v=<n>` (la WebView
garde une heure l'image d'une même URL) ; le front l'utilise toujours telle quelle.

| `ouvrir_dossier_fichier` | `{id}` | `void`. Montre le fichier dans son dossier : Windows `explorer.exe /select,"chemin"` (fichier sélectionné) ; macOS `open -R` ; ailleurs `xdg-open` du dossier. Fichier absent mais dossier présent : le dossier est ouvert. Erreur lisible : livre inconnu, sans fichier, dossier introuvable (contrôles limités à ~2 s : partage réseau qui ne répond pas). Permis en lecture seule. Menu contextuel : « Ouvrir le dossier du fichier » (un seul livre). |

## Renommage et déplacement

```ts
interface ModeleNom { nom: string; modele: string; }

interface DemandeRenommage {
  ids: string[];
  modele: string;          // syntaxe ci-dessous ; '/' crée des sous-dossiers
  dossierCible?: string;   // absent ou vide : chaque fichier reste dans son dossier (simple renommage)
  apercu: boolean;         // true : rien n'est fait, seules les lignes sont rendues
  confirme?: boolean;
}

interface LigneRenommage {
  id: string; titre: string;
  de: string;  vers: string;        // chemins complets (vers : normalisé comme il sera écrit dans la fiche)
  deNom: string; versNom: string;   // à afficher : noms seuls si le dossier ne change pas, sinon chemin complet de vers
  probleme?: string;                // présent : cette ligne ne sera pas appliquée
  inchange: boolean;                // déjà à cet emplacement (rien à faire)
}

interface ResultatRenommage {
  lignes: LigneRenommage[];
  aRenommer: number;                // lignes applicables
  bilan?: BilanOperation;           // apercu:false seulement
}
```

| Commande | Arguments | Résultat |
|---|---|---|
| `modeles_de_nom` | – | `ModeleNom[]` : « Série v2 #12 (2024) » `{series}[ v{volume}][ #{number}][ ({year})]` (défaut), « Export ComicRack » `[{format} ][{series}][ {volume}][ #{number}][ ({year}[/{month}])]`, « Éditeur / Série / Série #012 » `{publisher}/{series}/{series}[ #{number:000}]`. |
| `renommer_deplacer` | `{demande: DemandeRenommage}` | `ResultatRenommage`. Lecture seule : erreur. Le modèle et le dossier sont mémorisés (réglages) quand `apercu:false`. |

Syntaxe des modèles (`comicrust_core::edit::files`, format d'export de ComicRack) : `{champ}` (sans
casse : `series`, `number`, `volume`, `year`, `month`, `day`, `title`, `count`, `format`, `publisher`,
`imprint`, `writer`, `genre`, `language`, `filename` = nom actuel sans extension, et tout champ du
catalogue) ; `{{clé}}` valeur personnalisée ; `{number:000}` zéros ; `[ … ]` partie omise si un champ
qu'elle contient est vide ; `\x` caractère tel quel ; `/` sous-dossier. Série, numéro, volume, année,
nombre et format prennent la valeur proposée (déduite du nom du fichier) si la fiche est vide.
L'extension actuelle est gardée. Caractères interdits de Windows remplacés (`:` → ` -`, `/ \ |` → `-`…).

Problèmes détectés à l'aperçu (ligne non appliquée, **jamais d'écrasement**) : livre sans fichier,
fichier introuvable, nom vide d'après le modèle, chemin refusé sous Windows (caractères, noms réservés,
plus de 259 caractères), destination déjà existante, même destination que « autre livre ».
Application : déplacement sans remplacement (autre volume : copie vérifiée puis suppression), dossiers
créés ; si la fiche ne peut pas être mise à jour, le fichier est **remis à sa place**.

## Garde-fou des opérations en masse

Au-delà de `seuilConfirmation` livres (**100**, le seuil de confirmation des retraits en masse de la
version egui), `appliquer_modifications`, `ecrire_infos_fichiers`, `convertir_cbr_cbz` et
`renommer_deplacer` (hors aperçu) **ne font rien** et rendent un `BilanOperation` avec
`confirmationRequise` ; le front affiche le message et rappelle avec `confirme:true`.

## Bilan, progression, annulation

```ts
type Operation = 'modification' | 'ecritureInfos' | 'conversion' | 'renommage';
type Statut = 'ok' | 'inchange' | 'ignore' | 'erreur';

interface ResultatLivre {
  id: string; titre: string;
  statut: Statut;            // ok : modifié / écrit / converti / renommé ; inchange : déjà à jour ;
                             // ignore : format qui ne convient pas, sans fichier ; erreur : voir message
  message?: string;          // détail lisible (erreur, « conflit : fiche modifiée dans ComicRack »…)
  fichier?: string;          // nouveau chemin (conversion, renommage)
}

interface BilanOperation {
  operation: Operation;
  confirmationRequise?: { nbLivres: number; seuil: number; message: string }; // rien n'a été fait
  resume: string;            // une ligne : « 3 livre(s) modifié(s), 1 déjà à jour, 1 erreur(s) »
  complet: boolean;          // tout a abouti
  interrompu: boolean;       // annuler_operation pendant le lot
  livres: ResultatLivre[];
  rapport: string;           // bilan à copier (texte, une ligne par livre)
}
```

| Commande | Arguments | Résultat |
|---|---|---|
| `annuler_operation` | – | `void`. Demande l'arrêt du lot en cours **entre deux livres** (jamais au milieu d'un fichier ou d'un déplacement). |

Une seule opération d'édition à la fois (sinon erreur « une opération est déjà en cours »). Une
opération d'édition et l'**analyse des dossiers** s'excluent (erreur dans les deux sens), de même que
le changement de bibliothèque pendant une opération : un fichier renommé pendant l'analyse serait
sinon ajouté comme un nouveau livre. Après `annuler_operation`, les livres non traités figurent dans
le bilan en `ignore` avec le message « non fait : arrêt demandé ».

Événements (`listen` côté front) :

- `edition-progression` : `{operation: Operation, fait: number, total: number, livre: string}` au plus
  ~10 fois par seconde pendant un lot (`livre` : titre du livre en cours).
- `bibliotheque-modifiee` : `{ids: string[]}` après toute opération qui a changé des fiches (fiches
  relues après écriture et reportées dans la bibliothèque affichée) ; le front recharge `lister_livres`
  et, si besoin, `detail_livre` / `lire_fiches`.

## Écarts avec la version egui (assumés)

- La version egui n'a pas de fenêtre « Renommer » (le renommage passe par les scripts `ranger_fichiers.py`
  / Library Organizer, avec aperçu). Ici : commande dédiée avec les mêmes modèles du cœur, le même aperçu
  et les mêmes refus de conflit que l'aperçu des scripts (pas de suffixe « (2) » automatique).
- La version egui ne demande pas de confirmation pour modifier beaucoup de fiches : le garde-fou
  ci-dessus est propre à Tauri (seuil de la confirmation des retraits en masse).
- Lecture seule (ComicRack) : la version egui garde une table « converti → CBZ » dans ses réglages pour
  ranger l'original ; ici `rangerOriginal` est refusé en lecture seule (l'original reste en place).
- Écritures « Fichiers » non reportées dans ComicRack (base injoignable, conflit) : la version egui les
  garde dans ses réglages et les refait au lancement suivant ; ici elles sont signalées en erreur dans le
  bilan (le fichier, lui, est bien écrit ; relancer « Écrire les infos » refait le report).
- Onglet « Pages » et scripts `Editor` : étapes 4 et 7.
