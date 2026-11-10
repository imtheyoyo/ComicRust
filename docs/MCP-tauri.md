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
