# Contrat IPC : fenêtre et lancement (lot L2)

Écarts 4, 6 et 11 de `audit-parite.md`. Code : `src/fenetre.rs` et `src/plantage.rs` (logique pure,
testée), `src/cmd_fenetre.rs` (Tauri, fonction `app`), front `lib/fenetre.ts`,
`components/DepotFichiers.svelte`, `components/DialogPlantage.svelte`.

## Réglages : section `fenetre` de `data\comicrust-tauri.json`
`{ x, y, largeur, hauteur, maximisee }` en pixels physiques ; géométrie « normale » (jamais celle d'une
fenêtre maximisée, réduite ou en plein écran). Section absente tant que rien n'est mémorisé. Lecture
tolérante (champ invalide ou hors bornes ignoré).

- **Restauration** (`restaurer_et_afficher`) : fenêtre créée masquée, taille bornée (800×500 au plus
  petit, taille du plus grand écran au plus grand), position gardée seulement si sa barre de titre
  recouvre un écran existant (100×16 px), sinon fenêtre centrée ; maximisée si elle l'était.
- **Sauvegarde** (`evenement_fenetre`) : redimensionnement et déplacement, au plus une écriture par
  seconde ; écriture finale à la fermeture.

## Commandes
| Commande | Arguments | Résultat |
|---|---|---|
| `definir_titre_fenetre` | `titre: string \| null` | titre « <titre> — ComicRust » ou « ComicRust » |
| `lancement_initial` | | `{ chemin, genre } \| null`, **une seule fois** |
| `classer_chemin` | `chemin` | `'livre' \| 'dossier' \| 'liste' \| 'absent'` |
| `lire_plantage` | | `{ texte, resume, fichier } \| null` |
| `acquitter_plantage` | | supprime `data\plantage.txt` |

## Ligne de commande
`ComicRust-Tauri.exe [fichier | dossier | liste.cbl] [--mcp] [--attendre-pid N]` : le premier argument qui
n'est pas une option est le chemin (rendu absolu). Les options `--…` sont ignorées. Pas de
single-instance (la version egui n'en a pas non plus).

## Front
- Dépôt (`onDragDropEvent`, permission `core:event` de `core:default`) : premier chemin seulement.
  `.cbl` : import dans les listes ; fichier : lecteur ; dossier : choix lecture comme livre / ajout aux
  dossiers et analyse. Le chemin du lancement est traité de même, une fois la bibliothèque ouverte.
- F11 (plein écran, permissions `set-fullscreen` et `is-fullscreen`), Échap pour en sortir, Ctrl+O.
- **Point d'extension du lot « dialogue natif »** : `definirDialogueFichier(() => Promise<string | null>)`
  dans `lib/fenetre.ts` ; tant qu'il n'est pas branché, Ctrl+O ouvre la saisie du chemin existante.
- Le lecteur annonce son titre (`definirLivreOuvert`) et sa présence (`signalerLecteur`).

## Plantage
Crochet de panique installé au démarrage : `data\plantage.txt` (version, date, fil, message, pile) ; deux
paniques de la même session sont mises bout à bout. Au lancement suivant, la fenêtre « s'est fermé de
façon inattendue » propose « Copier » ; la fermer supprime le fichier. Recette :
`COMICRUST_SIMULER_PLANTAGE=1` provoque une panique au démarrage (comme egui). Limite : une panique
rattrapée (fil de travail) laisse aussi un rapport.

## Mode simulé
`?ouvrir=<chemin>`, `?plantage`, événements `comicrust-simule-depot` / `comicrust-simule-survol`,
traces dans `window.__comicrustSimule.fenetre` (`simuleFenetre.ts`).
