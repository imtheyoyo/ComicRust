# Contrat IPC : livres manquants (lot L3)

Contrat entre le backend (`crates/comicrust-tauri`, modules `manquants`, `biblio_manquants`, `cmd_manquants`) et le
front (`frontend/`, `lib/manquants.svelte.ts`, `DialogManquants.svelte`, `BandeauManquants.svelte`). Mêmes règles que
`ipc-bibliotheque.md` : commandes `invoke`, promesse rejetée avec un **texte en français**, JSON en **camelCase**,
**aucune logique métier côté front**. La détection et ses garde-fous sont ceux du cœur
(`comicrust_core::missing`), comme la version egui (`ui/missing_ui.rs`, `library/missing.rs`).

Toutes les commandes qui parcourent des disques sont `async` (un NAS éteint ne fige jamais la fenêtre). Le suivi
d'une opération longue se fait par **relevés** de `etat_manquants` (le front le relit toutes les 400 ms), sans
événement dédié.

## Détection : quand un livre est « manquant »

- Marque `missing` de la fiche (`est_manquant`). Dans la base **ComicRust** (SQLite / MySQL autonome) elle est
  écrite par `apply_scan_seen`. Dans une bibliothèque **ComicRack** elle n'est **jamais écrite** dans la base de
  ComicRack : elle vit en mémoire et est perdue au rechargement (le front relance donc une vérification à l'ouverture).
- **Volume injoignable ≠ fichier manquant.** Un livre dont le volume (lecteur, partage réseau) ne répond pas n'est
  jamais marqué : `sondeFichier` renvoie `volumeInjoignable` (avec `racine`), `verifier` le range dans
  `injoignables`.
- **Disparition massive** : par volume et par sous-dossier de premier niveau, au moins 200 livres disparus d'un
  coup, ou au moins 20 livres et plus de la moitié de ceux qui y étaient présents, sont `bloques` (non marqués,
  avertissement : partage partiellement monté). Voir `comicrust_core::mass_guard`.
- Un résultat de vérification ne vaut que pour le chemin vérifié : si la fiche a changé pendant la vérification,
  la marque n'est pas posée (jamais de marque sur une fiche modifiée entre-temps).

## Règles d'écriture

Mêmes règles que les listes et l'édition (`Edition::cible`) : base ComicRust = modifiable ; ComicRack MySQL
seulement avec « Lecture et écriture » et une configuration complète ; ComicDb.xml = lecture seule ; base hors
ligne = refus (`motif_hors_ligne`). La raison est exposée par `EtatManquants.raisonLectureSeule` ; les commandes
d'écriture (`proposer_fichier`, `chercher_manquants`, `appliquer_relocalisations`, `preparer_retrait`,
`retirer_livres`) la renvoient comme erreur. Aucune écriture, en lecture seule, ni de la base ni des fichiers.
`EtatManquants.raisonFichiers` donne la même décision avec le texte des actions sur les **fichiers** des livres
(« Écrire les infos dans le fichier », « Convertir en CBZ », « Renommer / déplacer… ») : le texte du refus de
`Cible::fichiers_modifiables` (`edition::raison_fichiers`), affiché sur ces entrées grisées.

## Commandes

- `etat_manquants() -> EtatManquants`
- `verifier_manquants() -> BilanVerification` : vérifie tous les livres. Une seule vérification à la fois (erreur
  sinon). Pose les marques en mémoire, les persiste si la base est modifiable.
- `arreter_manquants()` : arrête la vérification et la recherche en cours (`interrompu: true` dans le bilan).
- `sonder_livre(id) -> SondeLivre` : examine le fichier avant l'ouverture ; marque ou démarque le livre
  (`introuvable`), jamais sur `volumeInjoignable`.
- `proposer_fichier(id, chemin) -> Proposition` : « Retrouver le fichier » avec un chemin saisi ; vérifie
  l'existence, détecte un doublon (fichier déjà utilisé par une autre fiche) et la ressemblance.
- `chercher_manquants(ids, dossier) -> ResultatRecherche` : « Chercher dans un dossier ». `ids` vide : tous les
  livres manquants ; un seul identifiant : ce livre (même présent) ; plusieurs : leurs seuls manquants.
  Comparaison par nom de fichier (puis nom sans extension).
- `appliquer_relocalisations(demandes) -> BilanOperation` : met à jour `file` des fiches. Une demande dont la
  fiche n'a plus l'`ancien` chemin est ignorée.
- `preparer_retrait(ids) -> PreparationRetrait` : légendes et vérification des fichiers (fenêtre de confirmation).
- `retirer_livres(ids, confirme) -> BilanOperation` : supprime les **fiches** (jamais les fichiers). Le backend
  relance la vérification et refuse sans `confirme` quand elle est exigée (voir plus bas) : le front ne peut pas
  contourner la confirmation.

## Types

```ts
EtatManquants { nbManquants, verification?: {fait,total}, recherche?: {dossier,fichiers,comparaison},
                derniere?: BilanVerification, raisonLectureSeule?: string, raisonFichiers?: string }
BilanVerification { resume, manquants, presents, injoignables: {racine,livres}[],
                    bloques: {racine,livres,disparus}[], indetermines, interrompu, concluant, avertissement? }
SondeLivre { id, titre, fichier, etat: 'present'|'introuvable'|'volumeInjoignable', racine?, avertissement? }
Proposition { id, legende, ancien, nouveau, correspondance, sure, autres, doublon?, coche }
ResultatRecherche { dossier, propositions, nonRetrouves, fichiersExamines, erreurs, interrompu, info? }
DemandeRelocalisation { id, ancien, nouveau }
BilanOperation { nb, info, erreur? }
PreparationRetrait { ids, legendes, autres, bilan: BilanVerification, confirmationRequise, comicrack }
```

`LivreResume.manquant?: boolean` (absent : faux) ; `LivreDetail.manquant` ; `LivreDetail.fichierPresent` reste
l'état du disque : un livre `fichierPresent: false` sans `manquant` est **inaccessible** (volume débranché), pas
manquant.

## Liste virtuelle « Livres manquants »

Identifiant `@manquants` (constante `ID_LISTE_MANQUANTS`), acceptée comme `liste` dans `lister_livres`,
`lister_piles`, `groupes_vue`… (résolue par `membres_liste`). Elle n'existe pas dans l'arbre des listes : le front
l'affiche en bas du panneau Listes, toujours visible (discrète à 0) pour que la vérification manuelle reste
accessible. Pas de colonne « # » (aucun ordre propre).

## Confirmation de « Retirer »

`confirmationRequise` (et exigence côté `retirer_livres`) si : un volume est injoignable, une disparition massive
est bloquée, des livres sont indéterminés, des fichiers sont **encore présents**, ou plus de 100 livres.
`comicrack: true` : bibliothèque ComicRack, la fenêtre demande de fermer ComicRack (fiches modifiées dans la base
partagée). Retirer ne supprime jamais un fichier.

## Événement

`bibliotheque-modifiee` (existant) avec `origine: "manquants"` : `{ ids, origine, supprimes }`. Le front relit la
vue (sans perdre défilement ni sélection), les comptes des listes et l'état des manquants.

## Point d'extension : dialogue natif

Aucun plugin dialog n'est utilisé : les chemins sont saisis. `lib/manquants.svelte.ts` exporte
`selecteurs = { fichier?, dossier? }` (fonctions `(initial) => Promise<string | null>`) : renseignées par un lot
futur, elles font apparaître un bouton « Parcourir… » dans « Retrouver le fichier » et « Chercher dans un
dossier ». Aucun autre changement n'est nécessaire.

## Mode simulé

`lib/simuleManquants.ts`. Paramètres : `?manquants=volume` (livres 3 et 4 sur un volume injoignable),
`?manquants=lectureSeule`, `?manquants=massif` (disparition massive bloquée). `window.__comicrustSimule.manquants`
expose `volumeAbsent`, `retires`, `marques` et les `appels` reçus.
