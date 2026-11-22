# IPC du lot L7 « Réglages, aide et configuration »

Écarts de l'audit de parité : 7 (assistant rejouable), 10 (À propos / aide), 13 (protocole de mesure),
15 (thème « Comme Windows »), 16 (panneau Listes masquable), 18 (« Copier les livres », « Relire »).
Code : `T/diagnostic.rs` (logique testée sans Tauri), `T/cmd_diagnostic.rs` (commandes, feature `app`),
`T/tests_diagnostic.rs`, `F/components/DialogAPropos.svelte`, `F/components/BoutonsRapport.svelte`,
`F/lib/raccourcis.ts`, `F/lib/simuleReglages.ts`.

## Commandes

| Commande | Arguments | Réponse |
|---|---|---|
| `infos_diagnostic` | — | `InfosDiagnostic` |
| `texte_diagnostic` | — | texte à copier (`string`) |
| `configuration_biblio` | — | `ConfigurationBiblio` (lecture seule, ne crée rien) |
| `relire_bibliotheque` | — | `EtatBiblio` (c'est `Biblio::ouvrir`, rappelable) |
| `rapport_livres` | `requete: RequeteLivres`, `titre: string` | rapport texte des livres affichés |

Les noms JSON sont en camelCase. Hors Tauri, `simuleReglages.ts` imite ces commandes
(`?pdfiumAbsent`, `?relireErreur`, `?baseSqliteAbsente`).

### `InfosDiagnostic`
`version`, `systeme` (« windows x86_64 »), `mode?`, `source` (`sqlite`/`mysql`/`xml`/vide), `nbLivres?`,
`dossierExe`, `dossierData`, `dossierThemes`, `dossierWebview`, `cheminIni?`, `baseSqlite`,
`baseSqlitePresente`, `storeMysql`, `comicrackMysql`, `pdfium: { charge, chemin?, erreur? }`, `depot`.

**Aucun secret** : ni mot de passe, ni identifiant, ni nom de serveur ou de base MySQL (seulement
« MySQL (paramètres non inclus) »). Testé (`diagnostic_sans_secret`) sur le texte, le JSON et
`ConfigurationBiblio`. L'état de PDFium vient de `comicrust_core::providers::pdfium_status()` ; le chemin est
cherché dans les mêmes dossiers que le cœur (`PDFIUM_DYNAMIC_LIB_PATH`, `lib\`, dossier de l'exécutable,
dossier courant), absent si PDFium vient du système.

### `ConfigurationBiblio`
`mode?`, `cheminIni?`, `storeMysql`, `comicrackMysql`, `baseSqlite`, `baseSqlitePresente`, `baseSqliteTaille?`.
Les connexions MySQL (sans mot de passe) viennent de `lire_connexions_mysql` (existant).

### `rapport_livres`
Même présentation que `shown_books_report` (egui) :
```
<liste ou « Bibliothèque »> — N livres (ComicRust vX.Y.Z)

Série #numéro (vVolume, année) — chemin du fichier
```
Respecte la recherche, le tri et la pile ouverte de la requête (`debut` et `nombre` sont ignorés).

## Assistant de configuration rejouable
Réglages ▸ « Changer de bibliothèque / de connexion… » ferme les Réglages et affiche `PremierDemarrage`
en mode `modification` (superposition plein écran ; l'application reste montée dessous, rendue `inert`).
- Prérempli avec `configuration_biblio` + `lire_connexions_mysql` (mode, `ComicRack.ini`, serveur/port/
  utilisateur/base ; mot de passe laissé vide = « enregistré, on le garde »).
- **Annuler** (bouton ou Échap) : rien n'est changé.
- **Confirmation** avant de remplacer la bibliothèque affichée ; mention « une base vide sera créée » si
  `data\comicrust.db` n'existe pas.
- Le changement réutilise les commandes existantes `configurer_premier_demarrage` et `configurer_mysql`
  (donc les garanties existantes : la nouvelle bibliothèque est ouverte **avant** d'enregistrer, un échec
  n'enregistre rien, refus pendant une analyse ou une édition, préférences d'affichage conservées, consentement
  « Lecture et écriture ComicRack » jamais reporté). **La base courante n'est jamais supprimée ni modifiée**
  (test `bascule_de_base_sans_perte_ni_suppression`).
- Rechargement propre : `App.svelte::bibliothequeRouverte` (celui du transfert) = section Bibliothèque, liste
  courante remise à zéro, listes rechargées, vue rafraîchie, analyse du démarrage et livres manquants relancés
  (équivalent de `select_list(None)` + `scan_after_load` + `load_library` d'egui).
- Bouton désactivé tant qu'un livre est ouvert dans le lecteur.
- Différence avec egui : pas de choix des dossiers de BD dans l'assistant (ils restent dans « Dossiers surveillés ») ;
  pas de `ComicDb.xml`/`Config.xml` séparés (le chemin de `ComicRack.ini` ou de `ComicDb.xml` suffit, comme au
  premier démarrage Tauri).

## Thème « Comme Windows »
Thème virtuel d'identifiant `systeme` (`THEME_SYSTEME`, `F/lib/theme.ts`), sans fichier : `themeReel` le
traduit en `moderne-clair` ou `moderne-sombre` d'après `prefers-color-scheme`, et la bascule du système est
suivie en direct (seulement quand `systeme` est le thème affiché, un aperçu d'un autre thème n'est pas
bousculé). L'identifiant est enregistré tel quel dans `vue.theme` (accepté par `id_theme_valide`). Aperçu,
Annuler et OK de `DialogReglages` fonctionnent comme pour les autres thèmes. Limite : un thème personnel
nommé `systeme.css` serait masqué par le thème virtuel.

## Jauges des listes
Préférence `vue.jaugesListes` (booléen, absent = activé) : case « Afficher les jauges des listes » dans Réglages ▸ Affichage des listes. Contrat, définitions et mise à jour : `ipc-jauges.md`.

## Panneau des listes masquable
Préférence `vue.listesVisibles` (booléen, absent = affiché). Bouton « Masquer les listes » (barre latérale) et
raccourci **Ctrl+Maj+L** ; masqué, la barre latérale devient une colonne de 48 px (Bibliothèque, Afficher les
listes, Dossiers, Réglages, Aide), pour que les fonctions restent accessibles (egui garde Réglages dans la barre
du haut). Pas de Ctrl+Alt (AltGr des claviers AZERTY).

## Rouvrir la dernière liste au démarrage
Réglages ▸ « Démarrage » ▸ « Rouvrir la dernière liste au démarrage » : **activé par défaut**. Désactivé, la
bibliothèque entière s'affiche à chaque démarrage (comportement d'avant).

Deux champs de la section `vue` de `data\comicrust-tauri.json`, sans nouvelle commande IPC (mêmes
`lire_preferences_vue` / `enregistrer_preferences_vue`) :

| Champ | Type | Sens |
|---|---|---|
| `rouvrirDerniereListe` | booléen | absent = activé ; seul `false` désactive |
| `derniereListe` | chaîne | identifiant (GUID, stable : ni index ni nom) de la dernière liste sélectionnée ; absent = bibliothèque entière. À l'enregistrement `""` l'efface ; le fichier ne contient jamais de chaîne vide |

- **Anciens réglages** (sans ces champs) : lus sans erreur, réglage activé, aucune liste à rouvrir ; rien n'est
  réécrit tant que l'utilisateur ne change pas de liste. Lecture tolérante : type invalide ou identifiant absurde
  (chemin, > 100 caractères) ignoré. Fusion champ par champ (enregistrer un autre réglage ne touche pas la liste).
- **Écriture** : à chaque changement de liste (liste de l'arbre, bouton « Bibliothèque », liste supprimée ou
  bibliothèque rouverte par un transfert / l'assistant), par l'enregistrement différé de `lib/prefs.ts`
  (400 ms, regroupé avec les autres préférences, écrit à la fermeture de la page) ; l'écriture du fichier se fait
  côté backend dans la commande IPC asynchrone, jamais sur le fil de l'interface. Elle est mémorisée même
  réglage désactivé, pour retrouver la liste en le réactivant. Elle est donc déjà en place après une fermeture
  brutale (plantage) ou une mise à jour / relance, à 400 ms près.
- **Restauration** (`App.svelte::restaurerDerniereListe`, au démarrage seulement) : une fois l'arbre des listes
  chargé, la liste est sélectionnée et ses dossiers parents dépliés. Elle ne dépend ni de la section ni du lecteur
  ni des Réglages ouverts à la fermeture : on revient à la bibliothèque, sur cette liste (vue, tri et regroupement
  de la liste, déjà mémorisés par `vue.listes.<id>`). **Aucun livre n'est rouvert** (ComicRust n'a pas de
  réglage « rouvrir le dernier livre »). La liste virtuelle « Livres manquants » n'est pas mémorisée.
- **Cas limites** : liste supprimée ou identifiant inconnu (autre bibliothèque, autre connexion) = bibliothèque
  entière, sans erreur, et la mémoire est effacée (pas de boucle) ; liste renommée = retrouvée (l'identifiant est
  stable) ; arbre illisible ou indisponible (ComicDb.xml absent, base hors ligne) = bibliothèque, mémoire gardée
  pour le démarrage suivant ; l'utilisateur qui choisit une liste ou une section pendant le chargement de l'arbre
  n'est pas écrasé. Changement de bibliothèque (assistant, transfert) : retour à la bibliothèque entière, mémoire
  effacée. Copie portable : le fichier `data\comicrust-tauri.json` suit le dossier, donc la dernière liste aussi.
- Tests : `reglages::tests::anciens_reglages_sans_derniere_liste`, `derniere_liste_ecriture_relecture_et_effacement`,
  `derniere_liste_lecture_tolerante` ; front simulé (`?persistance` conserve les préférences au rechargement,
  `?prefs={"derniereListe":"…"}` simule un fichier existant) vérifié sous Playwright.

## Aide / À propos
F1 (ou « Aide et à propos » dans la barre latérale) : version, système, PDFium (chargé ou raison), dossier data,
lien du dépôt (`maj_ouvrir_page`), raccourcis (inventaire dans `F/lib/raccourcis.ts` : **à compléter par les
lots qui ajoutent des touches**, ex. `I` du lecteur), « Copier les informations de diagnostic ».
