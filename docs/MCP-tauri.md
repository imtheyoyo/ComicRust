# Pilotage de ComicRust-Tauri par une IA (MCP)

`ComicRust-Tauri-mcp.exe` est une variante de la version Tauri qui embarque un serveur
[MCP](https://modelcontextprotocol.io) : une IA (Claude Code, Claude Desktop…) peut voir la
fenêtre, lire les éléments de la page, cliquer, taper au clavier, lire la console JavaScript
et interroger le backend en lecture seule. C'est un outil de **test et de débogage**.

`ComicRust-Tauri.exe` reste inchangé : le serveur n'y est pas compilé. La variante egui a son
propre serveur (`ComicRust-mcp.exe`, voir `docs/MCP.md` sur la branche principale) : mêmes noms
d'outils, ports différents, les deux peuvent tourner en même temps.

## Démarrage

Lancer `ComicRust-Tauri-mcp.exe --mcp` (dans le dossier portable, à côté de `data\`). Le serveur
écoute sur `http://127.0.0.1:47802/mcp`.

| Option | Effet |
|---|---|
| `--mcp` | active le serveur sur le port 47802 |
| `--mcp=PORT` | autre port |
| `--mcp-token=JETON` | exige `Authorization: Bearer JETON` (ou variable `MCP_TOKEN`) |
| `--mcp-stdio` | passerelle stdio (voir Claude Desktop) |

Sans `--mcp`, la variante se comporte exactement comme `ComicRust-Tauri.exe`.

### Deux copies en même temps (autonome et liée)

Chaque copie doit écouter sur **son propre port**, sinon la seconde ne démarre pas son serveur et
les deux clients MCP parlent à la même instance. Par exemple :

- copie autonome : `ComicRust-Tauri-mcp.exe --mcp` (port 47802) ;
- copie liée : `ComicRust-Tauri-mcp.exe --mcp=47803`.

Puis un client par port (`claude mcp add --transport http comicrust-tauri-lie
http://127.0.0.1:47803/mcp`, ou dans Claude Desktop `"args": ["--mcp-stdio", "--mcp=47803"]`). La commande `invoke infos` indique le dossier `data` de l'instance
pilotée : c'est le moyen de vérifier qu'on parle à la bonne copie.

## Brancher un client

Claude Code :

```powershell
claude mcp add --transport http comicrust-tauri http://127.0.0.1:47802/mcp
```

Claude Desktop (`claude_desktop_config.json`, Réglages › Développeur › Modifier la config) :

```json
{
  "mcpServers": {
    "comicrust-tauri": {
      "command": "F:\\Chemin\\ComicRust-Tauri\\ComicRust-Tauri-mcp.exe",
      "args": ["--mcp-stdio"]
    }
  }
}
```

La passerelle relaie les messages vers l'instance en cours et lance `--mcp` si besoin.

### Relance et port occupé (passerelle `--mcp-stdio`)

- **Relance** : dès qu'une requête trouve le port fermé (application fermée entre-temps, plantée…),
  la passerelle relance l'application avec `--mcp` et attend jusqu'à 30 s qu'elle ouvre son port.
  Ce n'est plus limité à une seule relance par session. Après un démarrage raté, elle attend 20 s
  avant de réessayer (pas de relance en boucle) et rappelle la cause dans la réponse.
- **Port occupé par un autre programme** (il accepte la connexion mais ne répond pas comme le
  serveur MCP : erreur de lecture, HTTP 404…) : **aucune relance**, et l'échec n'est plus silencieux.
  Un avertissement `pont MCP ComicRust-Tauri : le port 47802 est occupé…` est écrit sur la **sortie
  d'erreur de la passerelle** (visible dans les journaux du client, p. ex. Claude Desktop ›
  Réglages › Développeur › journaux) et renvoyé dans la **réponse d'erreur JSON-RPC** que le client
  affiche. Remède : fermer l'autre programme, ou choisir un autre port des deux côtés
  (`--mcp=47803` dans `args`, et la même adresse dans le client).
- **Application lancée mais port non ouvert** (application arrêtée aussitôt, ou port refusé à son
  serveur) : même avertissement, avec le statut de sortie si l'application s'est arrêtée.
- **Côté application** (`--mcp=PORT` avec un port déjà pris) : le serveur ne démarre pas ; en plus de
  la ligne sur la sortie d'erreur, le **titre de la fenêtre** affiche `⚠ serveur MCP : port … indisponible`
  (sans console sous Windows, la sortie d'erreur n'est vue de personne).

### Après une mise à jour automatique

La mise à jour intégrée (`docs/tauri/ipc-maj.md`) remplace aussi `ComicRust-Tauri-mcp.exe` (l'ancien
devient `ComicRust-Tauri-mcp.exe.old`, supprimé au démarrage suivant) puis relance l'application avec
les mêmes arguments (`--mcp=47803` conservé) et `--attendre-pid <pid>` : la nouvelle instance attend
jusqu'à 10 s que l'ancienne ait libéré le port MCP et le profil WebView2.

- **Relancer le client MCP** (quitter puis rouvrir Claude Desktop, ou recharger le serveur MCP dans
  Claude Code) : la passerelle `--mcp-stdio` lancée par le client tourne toujours sur l'**ancien**
  binaire (le `.old`), qu'elle garde verrouillé, et parle encore au nouveau serveur sans le savoir.
  Après la relance du client, la passerelle utilise la nouvelle version et le `.old` sera supprimé au
  démarrage suivant de ComicRust.
- Si la passerelle trouve le port fermé pendant le redémarrage, elle peut relancer elle-même
  l'application : la seconde instance trouve alors le port pris (titre `⚠ serveur MCP : port …
  indisponible`) ; fermer cette fenêtre en trop.

## Outils

| Outil | Rôle |
|---|---|
| `screenshot` | capture PNG ; 1 pixel = 1 pixel CSS, coordonnées utilisables pour `click` |
| `ui_tree` | éléments visibles : rôle, libellé, valeur, position, état, id |
| `get_state` | section, mode, nombre de livres, liste courante, dialogues, alertes, focus, erreurs JS récentes |
| `click_widget`, `wait_for` | par libellé, sélecteur CSS ou id de `ui_tree` |
| `click`, `hover`, `drag`, `scroll` | souris (vrais événements Chromium) |
| `type_text`, `press_key` | clavier (« Enter », « Ctrl+A », « F5 »…) |
| `wait` | pause |
| `get_logs` | console de la page et erreurs JavaScript (depuis le chargement) |
| `eval_js` | évalue du JavaScript dans la page (sans restriction : débogage) |
| `invoke` | commande du backend **en lecture seule** (`infos`, `lister_livres`, `detail_livre`, `arbre_listes`…) |
| `reload_page` | recharge l'interface web |

Les noms et arguments sont ceux de la version egui ; seules les unités diffèrent (pixels CSS ici,
points egui là-bas).

## Sécurité

- Écoute sur `127.0.0.1` uniquement ; requêtes venant d'une page web refusées (en-tête `Origin`).
- Jeton facultatif.
- `invoke` refuse les commandes qui écrivent (listes, fichiers, base ComicRack). `eval_js` et les
  clics peuvent en revanche tout faire, comme un utilisateur : tester sur une bibliothèque de test.
- À n'utiliser que pendant les tests.

## Fonctionnement

`crates/comicrust-tauri/src/mcp/` : serveur HTTP local (transport MCP « Streamable HTTP »,
réponses JSON) dont les outils passent par le protocole DevTools de WebView2
(`ICoreWebView2::CallDevToolsProtocolMethod`) : `Page.captureScreenshot`,
`Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`, `Input.insertText`, `Runtime.evaluate`.
Le script `inject.js` (ajouté au chargement de la page) garde le journal de la console et
construit l'arbre des éléments. Le front expose son état d'affichage par
`window.__comicrust_etat()` (lecture seule). Windows uniquement (WebView2).
