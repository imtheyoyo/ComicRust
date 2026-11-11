# Contrat IPC : surveillance de la base et comportement hors ligne (étape 6, `tauri-v0.7.0`, lot C)

Contrat entre le backend (`crates/comicrust-tauri`, module `surveillance`) et le front (`frontend/`).
Mêmes règles que `ipc-bibliotheque.md` : commandes asynchrones (`invoke`), promesse rejetée avec un
**texte en français**, JSON en **camelCase**, **aucune logique métier côté front**. Comportement repris de
la version egui (`library/watch.rs`, `ui/status_bar.rs`), dupliqué : `crates/comicrust` n'est pas modifié.

## Principe

Un fil de sondage (`comicrust-surveillance`) interroge la base de la bibliothèque ouverte **toutes les
5 secondes** (`PERIODE_SONDAGE`, la période suit la fin du sondage précédent) :

| Bibliothèque | Surveillant (`comicrust-core`) | Sondée ? |
|---|---|---|
| ComicRack MySQL | `db::Watcher` (compteurs de la table `changes`, vérification complète toutes les `FULL_CHECK`) | oui |
| Autonome (SQLite ou MySQL de ComicRust) | `store::StoreWatcher` (compteurs, idem) | oui |
| ComicRack `ComicDb.xml` | aucun (ComicRack réécrit ce fichier lui-même) | non, toujours « en ligne » |

- **Modifications extérieures** (faites dans ComicRack, ou par un autre poste) : appliquées à la bibliothèque
  affichée (`ComicDatabase::apply`, vue et listes invalidées, couvertures des livres modifiés refaites) puis
  signalées par `bibliotheque-modifiee`.
- **Écritures de ComicRust** (édition, renommage, lecture) : signalées par leurs sites d'écriture
  (`Biblio::signaler_rapport_comicrack`, `signaler_ecriture_autonome`, équivalents de `note_written` /
  `note_deleted` de la version egui) pour ne **pas** être renvoyées comme modifications extérieures. Un site
  non signalé provoque seulement une relecture sans effet de la fiche (un rafraîchissement).
- **Pas de sondage pendant une analyse des dossiers ni une opération d'édition** (elles rechargent ou
  reportent elles-mêmes la base) ; la bibliothèque rechargée (`ouvrir`, fin d'analyse) recrée le
  surveillant d'après la nouvelle base.
- Le sondage **n'écrit jamais** dans la base, ne charge pas la bibliothèque entière, et rouvre sa connexion
  à chaque essai (MySQL redémarré : reprise sans relancer ComicRust).

## Hors ligne

Un échec de sondage (serveur arrêté, réseau coupé, base SQLite disparue, accès refusé) passe l'état à
**hors ligne** ; un sondage réussi le remet **en ligne** et les modifications survenues entre-temps sont
relues dans la foulée (les compteurs vus sont conservés).

Tant que la base est hors ligne :

- la **consultation continue** (la bibliothèque est en mémoire : liste, détail, lecteur) ;
- les **écritures dans une base sont refusées** avec le message `MOTIF_HORS_LIGNE`, **avant** toute tentative
  (rien n'est écrit à moitié, aucune fiche ni fichier ne diverge) :
  `appliquer_modifications`, `renommer_deplacer`, `ecrire_infos_fichiers`, `convertir_cbr_cbz`, les
  opérations sur les listes de la base de ComicRust. `lire_fiches` et `lire_pages` renvoient la même raison
  dans `lectureSeule` (fiche affichée en consultation) ;
- les **fichiers seulement** (ComicRack en lecture seule, qui n'écrit déjà rien dans la base) restent
  modifiables : le message de lecture seule habituel est conservé, pas de faux « base injoignable » ;
- la **lecture** (page courante) reste mémorisée dans `data\avancement.json` ; l'écriture dans la fiche
  ComicRack (option « Lecture et écriture ») n'est pas tentée / échoue sans perte, et l'erreur est lisible ;
- les **saisies du front sont conservées** (formulaire Infos grisé, pas fermé) et redeviennent enregistrables
  au retour de la base, sans rien resaisir.

Texte (`MOTIF_HORS_LIGNE`) : « Base injoignable : modifications impossibles pour le moment. ComicRust réessaie
automatiquement ; rien n'est perdu (la lecture reste mémorisée sur ce poste). »

### Configuration MySQL saisie à la main

La parité d'écriture ne dépend pas de l'origine de la configuration (`ComicRack.ini` ou saisie dans les
réglages) : `Biblio::source()` donne la même `DbConfig` et l'écriture suit le même chemin
(`db::update_books`, protocole des compteurs). Une configuration **incomplète** (serveur, utilisateur ou
base vide, ce que `DbConfig::is_complete` contrôle dans l'assistant egui) est refusée avant toute connexion
avec `CONFIG_MYSQL_INCOMPLETE`. Un serveur injoignable donne
« base ComicRack inaccessible, fiches non modifiées : <cause> » (édition) ou
« lecture non enregistrée dans la base de ComicRack (gardée dans ComicRust) : … » (lecture).

### Mot de passe

Jamais dans l'état, les événements, les messages ni les journaux : les causes d'échec passent par
`masquer_secret` (le mot de passe de la configuration est remplacé par `***`), `DbConfig` n'affiche que `***`
en `Debug`, et aucune commande ne renvoie la configuration MySQL. Les tests n'utilisent que des mots de passe
factices.

## Commandes

```ts
type SourceBase = 'comicrack-mysql' | 'comicrust' | 'comicrack-xml' | 'aucune';

interface EtatBase {
  source: SourceBase;
  surveillee: boolean;   // vrai pour comicrack-mysql et comicrust
  enLigne: boolean;
  message?: string;      // cause de l'échec (sans mot de passe), si enLigne est faux
  essais: number;        // échecs consécutifs depuis la perte (0 : en ligne)
}
```

| Commande | Arguments | Résultat | Rôle |
|---|---|---|---|
| `etat_base` | — | `EtatBase` | état courant (affichage initial) |
| `reessayer_base` | — | `EtatBase` | bouton « Réessayer » : sondage immédiat (peut durer jusqu'à 8 s, délai de connexion MySQL), événements émis comme d'habitude, état à jour renvoyé |

## Événements

| Événement | Charge | Quand |
|---|---|---|
| `base-etat` | `EtatBase` | au premier sondage, puis **à chaque transition** (en ligne → hors ligne, retour, changement de cause) ; pas à chaque essai raté (`essais` se lit par `etat_base`) |
| `bibliotheque-modifiee` | `{ ids: string[], origine: "externe", ajoutes: number, modifies: number, supprimes: number }` | modifications extérieures appliquées. `ids` : livres ajoutés ou modifiés (les supprimés ne sont comptés que dans `supprimes`). **Même événement que l'édition** (`{ ids }`) : la vue se rafraîchit déjà sans perdre défilement ni sélection ; `origine` le distingue. Le front recharge en plus l'arbre des listes (comptes) |

Ordre : `base-etat` (bascule) avant `bibliotheque-modifiee` dans un même sondage.

## Front

- `lib/connexion.svelte.ts` : `connexion` (`enLigne`, `message`, `essais`, `reessai`, `revenue`,
  `motifEcriture`, `reessayer()`), démarré par `App.svelte`.
- `components/BandeauBase.svelte` : bandeau « ⚠ Base injoignable, nouvel essai… » (cause en info-bulle,
  bouton **Réessayer**, `data-testid="bandeau-hors-ligne"`), puis « ✔ La base répond de nouveau » pendant 4 s.
- Écritures désactivées hors ligne : formulaire Infos (grisé, bandeau d'explication, saisies gardées),
  « Renommer » ; le reste est refusé par le backend avec le message lisible. L'état des listes
  (`etat_listes.lectureSeule`) porte la même raison.
- Mode simulé (`lib/simuleBase.ts`) : `?horsLigne` démarre hors ligne ; `window.__comicrustBase.perdre()`,
  `.rendre()` (la base « répond » au prochain essai ou clic sur Réessayer).

## Tests (`src/tests_surveillance.rs`)

SQLite autonome, sans serveur : premier sondage, modifications extérieures (ajout, modification,
suppression), écritures propres non renotifiées (et témoin non signalé), suppression signalée, pas de
sondage pendant une édition, rechargement, bascule hors ligne / reprise automatique (un seul événement par
transition), « Réessayer », fil de sondage, refus d'écriture hors ligne puis reprise, configuration MySQL
saisie à la main (injoignable : port fermé local ; incomplète), absence du mot de passe, `ComicDb.xml`.
MySQL réel : `mysql_comicrack_sondage_et_hors_ligne`, seulement si `COMICRUST_TEST_MYSQL` est définie (base
de test, **jamais** `comickrack_demo`).

## Points ouverts

- La lecture faite hors ligne en mode « Lecture et écriture » reste dans `data\avancement.json` ; elle n'est
  pas encore reportée dans la fiche ComicRack au retour de la base (`sort_local_progress` / migration de la
  version egui).
- Les écritures de la base faites par la conversion / l'écriture des infos en mode autonome ne sont pas
  signalées (pas de compteur renvoyé par `set_comic_info_written`) : une relecture sans effet au sondage suivant.
- Listes de ComicRack (`ComicDb.xml`) : leur relecture périodique reste celle de `listes.rs`.
