# Contrat IPC : transfert de la bibliothèque ComicRack (étape 6, `tauri-v0.7.0`, lot B)

Contrat entre le backend (`crates/comicrust-tauri`, module `transfert`) et le front (`frontend/`,
`DialogTransfert.svelte`). Mêmes règles que `ipc-bibliotheque.md` et `ipc-edition.md` : commandes
asynchrones (`invoke`), promesse rejetée avec un **texte en français**, JSON en **camelCase**,
**aucune logique métier côté front**. Comportement repris de la version egui
(`ui/transfer_window.rs`) ; la copie est celle du cœur :
`comicrust_core::store::Store::import_comicrack(src, opts, progress, cancel)`.

## Ce que fait le transfert

Copie dans la base autonome de ComicRust (SQLite `data\comicrust.db`, ou la base MySQL de ComicRust si elle est choisie dans les réglages) :

- les **livres** de ComicRack (fiche complète, lecture, notes, pages…) ;
- les **listes** (dossiers de listes, listes de lecture, listes intelligentes), si demandé ;
- les **dossiers de BD surveillés** (`<WatchFolders>`), si demandé.

| Source | Livres | Listes et dossiers |
|---|---|---|
| `mysql` : base MySQL de ComicRack | `comics` (lecture seule : `SELECT`) | `ComicDb.xml` (toujours dans ce fichier) |
| `comicdb` : `ComicDb.xml` (ComicRack sans MySQL) | `<Books>` du fichier | le même fichier |

**Idempotent** : relancer le transfert ne duplique rien. Un livre déjà transféré (même identifiant)
est **mis à jour** ; un livre déjà ajouté par l'analyse des dossiers (même fichier, autre
identifiant) est **remplacé** par la fiche ComicRack ; la lecture faite dans ComicRust entre-temps
et les fichiers convertis ou déplacés dans ComicRust sont gardés ; les listes sont remplacées (pas
ajoutées). Critère de recette : 5 323 livres, 47 listes, 0 doublon, y compris au second passage.

## Garanties

- **ComicRack n'est que lu** : aucune écriture dans sa base MySQL ni dans `ComicDb.xml` (tests :
  fichier identique à l'octet, fiches et compteurs MySQL inchangés).
- **Le mot de passe MySQL ne sort jamais du backend** : la connexion est relue dans
  `ComicRack.ini` (ou prise de la bibliothèque liée ouverte), gardée en mémoire le temps du
  transfert, **jamais** écrite (réglages, journaux), **jamais** renvoyée : l'aperçu ne donne que
  `utilisateur@hôte:port/base`, et toute erreur de la base est expurgée du mot de passe
  (`transfert::masquer`).
- **Une opération à la fois** : le transfert réserve la bibliothèque (comme l'édition). L'analyse
  des dossiers est refusée pendant le transfert ; le transfert est refusé pendant une analyse.
- Une panique du travail est ramenée à une erreur lisible (`erreur interne pendant le transfert`).

## Commandes

| Commande | Arguments | Résultat |
|---|---|---|
| `apercu_transfert` | – | `ApercuTransfert` : ce qui est proposé (rien de lourd n'est lu). |
| `lancer_transfert` | `{demande: DemandeTransfert}` | `ResumeTransfert` **à la fin** du transfert (la promesse reste en attente pendant la copie ; la progression arrive par événements). Rejet : erreur lisible. |
| `annuler_transfert` | – | rien. Demande l'arrêt entre deux lots de livres (jamais au milieu d'un lot). |

```ts
type SourceTransfert = 'mysql' | 'comicdb';

interface ApercuTransfert {
  mysql?: string;              // base MySQL de ComicRack trouvée : 'utilisateur@hôte:port/base', SANS mot de passe
  sourceParDefaut: SourceTransfert;   // 'mysql' si une base MySQL est trouvée, sinon 'comicdb'
  comicdb: string;             // ComicDb.xml proposé ('' : introuvable, à indiquer)
  comicdbPresent: boolean;
  destination: string;         // base de ComicRust, texte à afficher (ex. 'SQLite …\data\comicrust.db')
  destinationExiste: boolean;
  nbLivresDestination?: number;       // livres déjà dans la base de destination
  mode: 'autonome' | 'comicrack';     // bibliothèque actuellement affichée
  bloque?: string;             // présent : le transfert ne peut pas démarrer (analyse en cours…)
}

interface DemandeTransfert {
  source: SourceTransfert;
  comicdb?: string;            // livres (source comicdb) ou listes et dossiers (source mysql) ; absent : celui de l'aperçu
  listes: boolean;             // défaut côté backend : true
  dossiers: boolean;           // défaut côté backend : true
  basculer: boolean;           // défaut côté backend : true (utiliser ensuite ComicRust en autonome)
}

interface ResumeTransfert {
  livresSource: number;        // fiches lues dans ComicRack
  ajoutes: number;
  remplaces: number;           // déjà ajoutés par l'analyse des dossiers (même fichier) : doublon évité
  misAJour: number;            // déjà transférés (transfert relancé) : doublon évité
  ecartes: number;             // fiches sans identifiant, ou identifiant en double dans ComicRack
  doublonsEvites: number;      // remplaces + misAJour
  listes: number;              // listes enregistrées (tous niveaux)
  dossiers: number;            // dossiers surveillés ajoutés
  cheminsRamenes: number;      // chemins ramenés sous un dossier surveillé équivalent
  fichiersGardes: number;      // fichiers convertis ou déplacés dans ComicRust, gardés
  interrompu: boolean;         // arrêté : lots déjà écrits gardés, ni listes ni dossiers, pas de bascule
  avertissements: string[];    // non bloquants (tous ; le front en montre 10)
  nbLivresDestination?: number;
  doublonsRestants?: number;   // livres de la base dont le fichier est celui d'un autre livre (cible : 0)
  resume: string;              // phrase de bilan prête à afficher
  bascule: boolean;            // la bibliothèque affichée est maintenant la base autonome
  etat?: EtatBiblio;           // présent : la bibliothèque a été rouverte, le front met à jour son état
}
```

## Événement

| Événement | Charge | Quand |
|---|---|---|
| `transfert-progression` | `{phase: string, fait: number, total: number}` | toutes les 200 ms pendant `lancer_transfert`. |

`phase` : « préparation… », « lecture de la base MySQL de ComicRack… » (`fait` / `total` : fiches
lues), « lecture de ComicDb.xml… », « écriture dans la base de ComicRust (livres, puis listes et
dossiers)… » (livres écrits), « vérification de la base de ComicRust… ». `total` vaut 0 tant
qu'il n'est pas connu (barre indéterminée).

## Après le transfert

- Transfert **complet** et `basculer: true` : la section `mode` des réglages passe à `autonome`
  (le chemin de `ComicRack.ini` est conservé) et la bibliothèque est rouverte ; `etat` est rempli et
  `bascule` vaut `true`.
- Transfert **interrompu** : pas de bascule. Si la bibliothèque affichée est déjà la base autonome,
  elle est rechargée (on voit les livres déjà écrits) et `etat` est rempli.
- Si la réouverture échoue, le transfert reste réussi : un avertissement l'explique et `etat` est
  absent.

## Erreurs courantes (rejet de `lancer_transfert`)

- `aucune base MySQL de ComicRack configurée : …` : source `mysql` sans `DataSource=mysql:…`.
- `indique le ComicDb.xml de ComicRack` : source `comicdb` sans chemin.
- `<chemin> : <cause>` : `ComicDb.xml` illisible (source `comicdb`). Avec la source `mysql`, un
  `ComicDb.xml` illisible n'est qu'un avertissement (« ni listes ni dossiers »).
- `ce ComicDb.xml ne contient aucun livre : …` / `la base MySQL de ComicRack ne contient aucun livre`.
- `connexion à <utilisateur@hôte:port/base> impossible : …` (mot de passe expurgé).
- `analyse des dossiers en cours : attends sa fin` / `une opération est déjà en cours`.

## Tests

`crates/comicrust-tauri/src/tests_transfert.rs` : SQLite + `ComicDb.xml` de test (fixture
`fixtures/listes/ComicDb_anonyme.xml`, 47 listes ; 5 323 livres fabriqués) : 5 323 / 47 / 0 doublon,
second passage idempotent, source inchangée, doublon évité (même fichier), arrêt, options,
erreurs, une seule opération, bascule en autonome, mot de passe jamais renvoyé. Le test MySQL réel
(`mysql_comicrack_lecture_seule`) n'est exécuté que si `COMICRUST_TEST_MYSQL` est définie (base de
test uniquement, jamais `comickrack_demo`) ; il crée et supprime les tables `comics` et `changes`
(verrou commun avec le test d'édition).

## Mode simulé du front

`frontend/src/lib/simuleTransfert.ts` imite le contrat hors de Tauri. Paramètres d'adresse :
`?sansMysql` (aucune base MySQL trouvée), `?transfertErreur` (le transfert échoue),
`?transfertBloque` (analyse en cours).
