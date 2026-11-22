# Dialogue natif de fichiers (lot L8)

Écart 5 de l'audit de parité : la version egui utilise `rfd` (Ouvrir, dossier, Enregistrer sous) ; la
version Tauri saisissait les chemins. Ce lot ajoute le plugin officiel `tauri-plugin-dialog`.

## Aucune commande IPC nouvelle

Le front appelle directement les commandes du plugin (`invoke('plugin:dialog|open' | 'plugin:dialog|save')`) ;
aucun code Rust métier n'est ajouté.

- `crates/comicrust-tauri/Cargo.toml` : `tauri-plugin-dialog = { version = "2", optional = true }`, activé par la
  fonction `app` (résolu en 2.8.4 avec tauri 2.12.5 ; il apporte `rfd` 0.17, déjà dans `Cargo.lock`, et `tauri-plugin-fs`).
- `src/app.rs` : `.plugin(tauri_plugin_dialog::init())` en tête du `Builder`.
- `capabilities/default.json` : `dialog:allow-open` et `dialog:allow-save`. `dialog:default` n'est PAS utilisé : il
  ajouterait `allow-message` (boîtes de message), inutile (le front n'emploie ni `alert` ni `confirm`).

## Front : `frontend/src/lib/dialogue.ts`

| Fonction | Rôle |
|---|---|
| `choisirFichier({ titre, cheminInitial, filtres })` | « Ouvrir » : chemin choisi ou `null` |
| `choisirDossier({ titre, cheminInitial })` | choix d'un dossier |
| `enregistrerSous({ titre, cheminInitial, filtres })` | « Enregistrer sous » (le système confirme l'écrasement) |
| `dialogueDisponible()` | vrai dans Tauri (ou avec un dialogue factice de test) |
| `installerDialogues()` | appelée par `main.ts` : branche `definirDialogueFichier` (Ctrl+O, bouton « Ouvrir un fichier… ») et `selecteurs.fichier/dossier` (livres manquants) |

Filtres : `{ nom, extensions[] }`, extensions sans point. `cheminNormal()` retire le préfixe `\\?\` (et `\\?\UNC\`) ;
le plugin le fait déjà (`FilePath::simplified`), c'est une garde de plus.

### Mode simulé
Hors Tauri, pas de dialogue : `dialogueDisponible()` est faux, aucun bouton « Parcourir… » n'apparaît et Ctrl+O ouvre la
saisie du chemin (comportement précédent). Les tests Playwright posent `window.__comicrustDialogueSimule =
async (genre, options) => chemin | null` (`genre` : `'fichier' | 'dossier' | 'enregistrer'`) avant le chargement.

### Points branchés
- Ctrl+O et bouton « Ouvrir un fichier… » (`fenetre.ts`, `demanderOuvertureFichier`).
- Dossiers surveillés : « Parcourir… » choisit un dossier et l'ajoute aussitôt.
- Listes : import `.cbl` (choix puis analyse) et export `.cbl` (« Enregistrer sous », puis export).
- Transfert : « Parcourir… » pour `ComicDb.xml`.
- Scripts : champs `file` / `folder` (et `save: true` → « Enregistrer sous »), filtres du formulaire repris (`filtresChamp`).
- Livres manquants : « Retrouver le fichier » et « Chercher dans un dossier » (`selecteurs`).
- Premier démarrage : « Parcourir… » pour `ComicRack.ini` (il n'y a pas de choix de dossier dans cet écran).

## Mode portable
Le plugin n'écrit aucun fichier de lui-même (ni cache ni réglage). Seules les boîtes de dialogue du système (Windows)
peuvent mémoriser « dernier dossier utilisé » dans le registre de l'utilisateur, hors du contrôle de l'application.

## Limites
`app.rs` (feature `app`) ne compile pas sur Linux : la compilation du plugin et le comportement réel des boîtes
Windows sont à vérifier dans la recette sous Windows 11 (CI).
