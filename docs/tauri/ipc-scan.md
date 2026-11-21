# Contrat IPC — Scan et dossiers (lot L4)

Code : `crates/comicrust-tauri/src/cmd_scan.rs` (commandes), `biblio.rs` (analyse, arrêt, bilan),
`reglages.rs` (section `analyse`), `surveillance.rs` (option de suivi). Référence egui : `scan_after_load`
(`events.rs`), `scan_window` (`ui/windows.rs`), `auto_refresh` (`app.rs`, `toolbar.rs`).

## Commandes

| Commande | Arguments | Retour |
|---|---|---|
| `arreter_scan` | — | `bool` : vrai si une analyse était en cours (arrêt demandé) |
| `lancer_scan_auto` | — | `bool` : vrai si lancée ; faux sans erreur si désactivée, mode lié, hors ligne ou sans dossier |
| `lire_reglages_analyse` | — | `{ auDemarrage, suivreComicrack }` (défauts : `true`, `true`) |
| `enregistrer_reglages_analyse` | `reglages` (champs partiels) | réglages à jour |

`lancer_scan` (existant) est inchangé.

## Événements

`scan-progression` et `scan-termine` inchangés. Nouveau `scan-bilan`, émis juste avant `scan-termine` :
`{ resume, dossiers, fichiers, arretee, erreurs: [{chemin, message}] (500 max), erreursTotal, texte }`.
`arretee` vrai : analyse arrêtée par l'utilisateur ; les livres lus avant l'arrêt sont enregistrés et la
bibliothèque rechargée, `scan-termine` suit normalement.

## Réglages (`data\comicrust-tauri.json`, section `analyse`)

- `auDemarrage` (défaut vrai) : analyse au chargement de la bibliothèque (front : `SuiviScan.lancerAuto`,
  appelée à l'ouverture, après le premier démarrage et après un transfert). Jamais en mode lié, hors
  ligne, sans dossier.
- `suivreComicrack` (défaut vrai) : sondage de la base MySQL de ComicRack. Désactivé : plus de sondage
  (ni détection hors ligne) ; relu à chaque sondage (effet en 5 s). La base de ComicRust reste toujours
  surveillée (comme egui : `auto_refresh || standalone`). `etat_base.surveillee` en tient compte.
- La reprise de lecture (Demander / ComicRack / ComicRust) reste `vue.lecteur.reprise` ; le bloc
  « Analyse et suivi » de Réglages permet d'y revenir.

## Mode simulé

`simuleScan.ts` : `?scanAuto` (active l'analyse du démarrage), `?scanErreurs=N`, `?scanLent`.
