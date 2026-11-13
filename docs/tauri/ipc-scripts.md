# Contrat IPC : scripts Python (étape 7, `tauri-v0.9.0`, lots A et B)

Contrat entre le backend (`crates/comicrust-tauri`, module `scripts`, lot A) et le front (`frontend/`, lot B).
Mêmes règles que `ipc-bibliotheque.md` : commandes `invoke`, promesse rejetée avec un **texte en français**,
JSON en **camelCase** (sauf le contenu des formulaires et listes de choix, au format des scripts), **aucune
logique métier côté front**. Le moteur est celui du cœur (`comicrust_core::scripting`) ; le comportement est
celui de la version egui (`crates/comicrust/src/scripts/`, `ui/scripts_ui.rs`), dupliqué.

**Toutes les commandes sont asynchrones** : une question d'un script bloque le *fil du script* (jamais l'IPC)
jusqu'à `script_repondre` ou l'arrêt du script.

## Résumé du contrat imposé

Commandes :

- `scripts_lister() -> EtatScripts`
- `scripts_recharger() -> EtatScripts` : relit le dossier `scripts\`.
- `scripts_activer(id: string, actif: bool) -> EtatScripts`
- `scripts_auto(actif: bool) -> EtatScripts` : coupés, les automatismes en cours sont arrêtés et la file vidée.
- `script_lancer(id: string, hook: string, ids: string[]) -> string (runId)` ; refusé si un script interactif
  tourne déjà (ou applique ses modifications).
- `script_repondre(runId: string, reqId: number, valeur: any|null)`
- `script_vignettes_voulues(runId: string, reqId: number, rangs: number[])`
- `script_arreter(runId: string)` : annulation polie, puis arrêt forcé du processus Python après 3 s ; pendant
  une application, arrêt **entre deux livres** (écriture des ComicInfo.xml comprise), bilan `arrete: true`.
- `script_apercu(runId: string) -> Apercu`
- `script_appliquer(runId: string, cles: string[]) -> Bilan` : occupe le créneau interactif jusqu'au bilan (voir
  « Application en cours »).
- `script_hook_editeur(id: string, livreId: string, champs: object) -> string (runId)`
- `script_reglages_lire(id: string) -> object` / `script_reglages_ecrire(id: string, valeurs: object) -> object`

Événements : `script:journal`, `script:progression`, `script:question`, `script:formulaire`, `script:choix`,
`script:vignette`, `script:fin`, `script:applique`, `scripts:auto`, et `bibliotheque-modifiee` (existant).

Règles : en bibliothèque en lecture seule (ComicRack sans « Lecture et écriture », ComicDb.xml, base hors ligne,
configuration MySQL incomplète, aucune bibliothèque), l'aperçu s'affiche mais `script_appliquer` est refusé
(`lectureSeule: true`, `motif`). Les automatismes sont désactivés par défaut (`auto: false`).

## Types

```ts
type EtatScripts = {
  python: { chemin?: string; erreur?: string };   // erreur : Python embarqué introuvable (message complet)
  scripts: InfoScript[];
  avertissements: string[];   // en-têtes mal formés, fichiers illisibles : "rel/chemin.py:12 : message"
  auto: boolean;              // automatismes activés (faux par défaut)
  ecritureAutorisee: boolean;
  motifLectureSeule?: string;
  dossier: string;            // précision : dossier des scripts (à côté de l'exe)
  executionEnCours?: string;  // précision : runId du script interactif en cours (lancé ou en application)
};

type InfoScript = {
  id: string;          // précision : identifiant du POINT D'ENTRÉE (#@Key, sinon "chemin/relatif.py:fonction")
  nom: string;         // #@Name
  description?: string;
  auteur?: string;     // jamais rempli pour l'instant (pas d'en-tête #@Author dans le cœur)
  hooks: string[];     // "Books"|"Library"|"Editor"|"ConfigScript"|"BookOpened"|"BookClosed"|"NewBooks"|"Startup"
  fichier: string;     // chemin relatif à scripts\ (séparateurs /)
  actif: boolean;      // choix de l'utilisateur, sinon en-tête #@Enabled
  image?: string;      // #@Image en URL data:image/(png|jpeg|gif|webp|bmp);base64 (≤ 512 Ko)
  reglages?: string;   // précision : id de la fonction ConfigScript du même fichier (bouton « Réglages »)
};
```

> Un fichier peut déclarer plusieurs fonctions (menu + `ConfigScript`, par exemple) : l'identifiant est donc
> celui du point d'entrée et non le seul chemin du fichier. Les réglages restent communs à tout le fichier.

### Lancement

- `script_lancer(id, "Books", ids)` : sélection (refusé si `ids` est vide) ;
- `script_lancer(id, "Library", [])` : `ids` vide = **toute la bibliothèque** ;
- `script_lancer(idConfig, "ConfigScript", [])` : réglages du script (possible sans bibliothèque ouverte) ;
- autre hook : refusé (les automatismes se déclenchent seuls ; `Editor` passe par `script_hook_editeur`).

Un script désactivé n'est pas lancé (sauf `ConfigScript`).

### Questions (`script:question`)

```ts
{ runId, reqId, genre: "ask"|"message", titre: string /* nom du script */, texte: string,
  question?: { type: "texte"|"choix"|"confirmation", defaut: any|null, choix: string[], secret: boolean } }
```

Précision : `question` (présent pour `genre: "ask"`) distingue `comicrust.ask` (texte), `ask_choice` (un des
`choix`) et `confirm` (oui/non). Réponse par `script_repondre` : texte → chaîne ; choix → la chaîne choisie ;
confirmation → booléen ; `null` → valeur par défaut. Une saisie `secret` n'a jamais de `defaut` (une réponse vide
garde la valeur actuelle côté script). Message : réponse `null` (« OK »).

### Formulaires (`script:formulaire`)

`{ runId, reqId, spec }`. `spec` est au **format des scripts** (`comicrust.form`), tel que validé et assaini par
`comicrust_core::scripting::forms` (le cœur le relit à l'identique) :

```ts
{ title, message, controls: Control[], buttons: string[], cancel: string|null /* libellé */, width, label_width,
  values: object /* valeurs initiales : défauts puis valeurs du script ; secret jamais pré-rempli */ }
Control = { id: number, type, key, label, help, enabled_if?: "clé"|"!clé"|"clé=v"|"clé!=v", width?, ...propres }
```

Types et champs propres : `text`/`secret` (`default`, `placeholder`, `required`, `rows`, `secret`, `inserts:
[{group, items:[{value,label}]}]`, `paste`, `hints: [[expression, texte]]`, `hint_none`), `checkbox`
(`default`), `number` (`default`, `min`, `max`, `step`, `decimals`), `choice` (`choices: [{value,label}]`,
`default`, `editable`, `required`), `radio` (`choices`, `default`, `horizontal`), `multichoice` (`choices`,
`default: value[]`, `height`, `required`), `list_edit` (`default: string[]`, `choices: string[]`, `unique`,
`required`, `height`), `table` (`columns: [{title, type: text|number|checkbox|choice, choices, width}]`,
`default: any[][]`, `height`), `folder`/`file` (`default`, `save`, `filters: [[nom, [ext…]]]`, `required`),
`label` (`text`, `style`), `separator`, `image` (`data`: **URL `data:`** au lieu du base64, `caption`,
`max_width`, `max_height`), `group` (`title`, `controls`), `columns` (`columns: Control[][]`, `widths`),
`tabs` (`tabs: [{title, controls}]`), `button` (`validate`).

**Réponse** (`script_repondre`) : l'objet des valeurs, avec la clé réservée **`"$bouton"`** = libellé du bouton
du bas, ou clé d'un bouton du formulaire (absente : premier bouton du bas). Le backend la retire et transmet
`{button, values}` au cœur, qui vérifie le bouton et assainit les valeurs. `null` = annulé (le script reçoit
`None`).

### Listes de choix (`script:choix`, `script:vignette`)

`{ runId, reqId, spec }`, `spec` au format de `comicrust.choose` :
`{ title, message, columns: string[], rows: string[][], images: (string|null)[] /* URL data:, vide = sans
vignettes */, multi, default: number[], filter, timeout, thumb_size, layout: "table"|"cards", lazy: number[],
on_demand: number[] }`.

Vignettes envoyées après coup (`lazy` : dès que possible ; `on_demand` : quand le front les demande) :
`script:vignette { runId, reqId, rang, image }` (`image` : URL `data:`, `""` = pas d'image). Le front signale les
lignes survolées ou choisies par `script_vignettes_voulues(runId, reqId, rangs)`.

**Réponse** : choix simple → un rang (ou `null`) ; `multi` → une liste de rangs ; `null` = annulé.

### Progression et journal

- `script:progression { runId, fraction?, texte?, detail?, image? }` : champs absents = inchangés ; `image` en
  URL `data:`, `""` = retirée.
- `script:journal { runId, genre: "info"|"sortie"|"erreur"|"systeme", texte }` : `sortie` = `print()` / stderr
  du script ; `info` (avertissements préfixés `[attention]`) ; `systeme` = messages de ComicRust (lancement,
  fin, bilan, arrêt). Les valeurs secrètes n'y apparaissent jamais.

### Fin, aperçu, application

```ts
// script:fin
{ runId, etat: "termine"|"annule"|"erreur", erreur?: string, nbModifs: number, champs?: object }

type Apercu = { lignes: LigneApercu[]; conflits: string[]; inchanges: number /* précision */;
                lectureSeule: boolean; motif?: string };
type LigneApercu = { cle: string; livreId: string; titre: string; champ: string;
                     libelle: string /* précision : libellé français du champ */;
                     avant: string; apres: string; genre: "fiche"|"renommage"|"comicinfo"; conflit?: string };
type Bilan = { fiches: number; renommages: number; comicinfo: number; erreurs: string[]; arrete: boolean;
               resume: string /* précision */ };
```

- L'aperçu est calculé à la fin du script (même arrêté ou en erreur : ce qui a été demandé avant), sur les
  fiches actuelles. Clés : `<livreId>|fiche|<Champ>`, `<livreId>|renommage`, `<livreId>|comicinfo`. Une ligne
  avec `conflit` (valeur refusée, destination occupée, fichier introuvable, non-CBZ…) n'est jamais appliquée.
- `script_appliquer(runId, cles)` applique les lignes choisies, dans l'ordre : fiches (`edit::apply_edits`),
  renommages / déplacements (`edit::rename_or_move`, fichier libéré par le lecteur, arrêt entre deux livres),
  puis ComicInfo.xml (écriture des infos existante). Écritures signalées à la surveillance ; la bibliothèque
  affichée est mise à jour et `bibliotheque-modifiee { ids }` est émis ; puis `script:applique { runId, bilan }`.
  Une exécution ne s'applique qu'une fois (l'aperçu est ensuite vide). Les renommages faits sont enregistrés
  dans `data\renommages\` (relus par `comicrust.applied_renames()`).
- Lecture seule : `script_apercu` rend `lectureSeule: true` et `motif` ; `script_appliquer` est rejeté avec ce
  motif.

#### Application en cours

- `script_appliquer` **reprend le créneau interactif** (libéré à la fin du script) et le garde jusqu'au bilan :
  pendant toute l'application, `executionEnCours` vaut ce `runId`, `script_lancer` / `script_hook_editeur` sont
  refusés (« Un script est déjà en cours… ») et un second `script_appliquer` aussi (« application déjà en
  cours »). Refusé si un autre script interactif tourne. Le créneau est libéré **avant** `script:applique`.
- Avancement : `script:progression { runId, texte }` (écriture des ComicInfo.xml : `"ComicInfo.xml 2/5 Titre"`).
- Arrêt : `script_arreter(runId)` pendant l'application → arrêt entre deux livres (jamais au milieu d'un
  renommage ou d'une écriture), l'écriture des ComicInfo.xml en cours est annulée (`Edition::annuler`) ;
  journal `systeme` « Arrêt demandé : l'application s'arrête après le livre en cours… » ; le bilan rendu (et
  `script:applique`) porte `arrete: true` et l'erreur « N opération(s) non faite(s) : arrêt demandé avant leur
  tour ».
- Front : pendant l'application, la fenêtre affiche « Application en cours… » et un bouton **Arrêter** ; ni
  Annuler, ni Fermer, ni Échap avant le bilan.

### Hook Editor (fenêtre Infos)

`script_hook_editeur(id, livreId, champs)` : `champs` = valeurs actuelles de la fenêtre (nom ComicRack → texte,
non enregistrées) ; le script voit la fiche avec ces valeurs. À la fin, `script:fin.champs` = champs proposés
(nom → texte tel qu'affiché par la fenêtre Infos) ; `"$ecrireDansFichier": true` si le script a demandé
`write_info`. Rien n'est enregistré ; renommage et autres livres ignorés (signalés dans `script:journal`).

### Réglages des scripts

`script_reglages_lire(id)` : réglages du fichier du script ; une valeur secrète définie vaut **`"••••"`** (vide
si non définie ou illisible), jamais la valeur en clair. Clé réservée **`"$secrets"`** : liste des clés secrètes
(une clé marquée secrète sans valeur apparaît avec `""`), pour que le front affiche **toujours** ces champs en
mot de passe (bouton œil pour voir la saisie, jamais la valeur enregistrée). `script_reglages_ecrire(id,
valeurs)` fusionne `valeurs` (la clé `"$secrets"` est ignorée) ; un secret renvoyé inchangé (`"••••"`) **garde
l'ancienne valeur chiffrée**. Rend les réglages masqués (précision).

Stockage : `data\comicrust-tauri.json`, section `scripts` (lecture tolérante) :
`{ auto?: bool, desactives: string[], actives: string[], valeurs: { "<fichier relatif>": { …, "<clé>": {"$secret":
"dpapi:…"} } } }`. Secrets chiffrés par `comicrust_core::secret` (DPAPI sous Windows ; ailleurs ils ne sont pas
conservés).

### Automatismes (`scripts:auto`)

`Startup` (première ouverture de la bibliothèque de la session), `BookOpened` / `BookClosed` (livre de la
bibliothèque ouvert / fermé dans le lecteur), `NewBooks` (analyse des dossiers qui a ajouté des livres). Seulement
si `auto` est vrai et le script actif. Un automatisme à la fois par hook (50 en attente au plus), **aucune
question** (valeurs par défaut, formulaires `None`), 300 s au plus, modifications appliquées au fil de l'eau.
Bibliothèque en lecture seule ou Python absent : non lancés (dit dans le journal).

Pour un automatisme, **seul `scripts:auto { script, hook, ligne }`** est émis (plus `bibliotheque-modifiee` après
une application) : pas de `script:journal`, `script:fin`… (le front ignore les runId inconnus).

### Fermeture

`RunEvent::ExitRequested` / `Exit` (`Scripts::fermer`) :

- plus aucun lancement ni application (`script_appliquer`, application au fil de l'eau d'un automatisme) ;
- scripts et automatismes annulés (arrêt forcé après le délai de grâce), file des automatismes vidée ;
- applications en cours — celle d'un script du menu **et** celles des automatismes (`apply_live`, comptées
  elles aussi) — arrêtées **entre deux livres** ; `Edition::annuler()` est appelé dans tous les cas (écriture des
  ComicInfo.xml) ;
- attente de 8 s au plus (`ATTENTE_FERMETURE`) pour les scripts ; tant qu'une application tourne, attente de la
  fin du livre en cours (un fichier déplacé a toujours sa fiche à jour), avec une borne de sécurité de
  **5 minutes** (`ATTENTE_FERMETURE_APPLICATION` : de quoi finir le déplacement d'un gros fichier vers un partage
  réseau lent ; au-delà, l'opération est jugée bloquée et ComicRust se ferme quand même).

### Divers

- Le module `comicrust.py` est écrit dans `data\python\` ; l'interpréteur est `COMICRUST_PYTHON`, puis
  `python\python.exe` à côté de l'exe (Python embarqué), puis (hors Windows) `python3`.
- `comicrust.open_url` : adresse vérifiée par `check_url` du cœur (http/https avec hôte), ouverte comme
  `maj::ouvrir_page`. `comicrust.clipboard_set` : presse-papiers du système (`clip.exe` sous Windows). Sous
  Windows, `clip.exe` et `explorer.exe` sont lancés avec `CREATE_NO_WINDOW` (aucune console qui clignote).
- **Images** : tout ce que le backend envoie (icône `#@Image`, contrôle `image`, `images` et `script:vignette`
  des listes de choix, `script:progression.image`) est une URL `data:image/(png|jpeg|gif|webp|bmp);base64,…`
  (format reconnu d'après les octets). Le front (`imageScriptSure`) **refuse tout le reste** : SVG (peut porter du
  script), `cr-…://`, `http://cr-….localhost/`, base64 brut.
- Automatismes : un automatisme qui ne peut pas partir (lecture seule, base ou Python indisponible, fil
  impossible à créer) libère toujours son hook et n'est jamais attendu par la fermeture.
