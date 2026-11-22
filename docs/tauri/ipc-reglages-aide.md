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

## Panneau des listes masquable
Préférence `vue.listesVisibles` (booléen, absent = affiché). Bouton « Masquer les listes » (barre latérale) et
raccourci **Ctrl+Maj+L** ; masqué, la barre latérale devient une colonne de 48 px (Bibliothèque, Afficher les
listes, Dossiers, Réglages, Aide), pour que les fonctions restent accessibles (egui garde Réglages dans la barre
du haut). Pas de Ctrl+Alt (AltGr des claviers AZERTY).

## Aide / À propos
F1 (ou « Aide et à propos » dans la barre latérale) : version, système, PDFium (chargé ou raison), dossier data,
lien du dépôt (`maj_ouvrir_page`), raccourcis (inventaire dans `F/lib/raccourcis.ts` : **à compléter par les
lots qui ajoutent des touches**, ex. `I` du lecteur), « Copier les informations de diagnostic ».
