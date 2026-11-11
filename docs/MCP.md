# Pilotage de ComicRust par une IA (serveur MCP)

Pour les tests de débogage, ComicRust peut être piloté par une IA (Claude Code, Claude
Desktop ou tout client [MCP](https://modelcontextprotocol.io)) : l'IA voit la fenêtre, clique,
saisit du texte, lit les erreurs, puis vérifie le résultat, sans intervention.

## Quel exécutable ?

| Exécutable          | Serveur MCP                                            |
|---------------------|--------------------------------------------------------|
| `ComicRust.exe`     | absent (le code n'est pas compilé)                     |
| `ComicRust-mcp.exe` | présent, **inactif tant que `--mcp` n'est pas donné**  |

`ComicRust-mcp.exe` est dans le paquet de chaque release (à côté de `ComicRust.exe`, même
dossier `data\`), et en artefact `ComicRust-mcp-windows-x64` de chaque exécution de la CI.
Compilation locale : `cargo build --release -p comicrust --features mcp`.

## Activation

```
ComicRust-mcp.exe --mcp                       écoute sur http://127.0.0.1:47801/mcp
ComicRust-mcp.exe --mcp=48000                 autre port
ComicRust-mcp.exe --mcp --mcp-token=SECRET    jeton exigé (ou variable MCP_TOKEN)
ComicRust-mcp.exe --mcp-stdio                 passerelle stdio (Claude Desktop, voir plus bas)
```

Les options `--mcp…` peuvent être combinées avec un fichier à ouvrir
(`ComicRust-mcp.exe --mcp "Z:\Thanos\Thanos Quest 01.cbz"`).

## Sécurité

- Écoute **uniquement en boucle locale** (127.0.0.1) : rien n'est accessible depuis le réseau.
- Requêtes venant d'une page web refusées (en-tête `Origin` étranger : protection contre le
  « DNS rebinding »).
- Jeton facultatif (`Authorization: Bearer …`), conseillé si d'autres personnes utilisent le PC.
- Aucun outil destructif : pas de suppression de fichiers, pas d'écriture dans la base de
  ComicRack. L'IA peut néanmoins cliquer partout dans l'interface comme un utilisateur :
  utiliser une bibliothèque de test, ou ComicRack en lecture seule.
- À réserver aux séances de test ; l'exécutable normal n'a pas ce serveur.

## Configuration du client

### Claude Code

```
claude mcp add --transport http comicrust http://127.0.0.1:47801/mcp
```

Avec un jeton :

```
claude mcp add --transport http comicrust http://127.0.0.1:47801/mcp --header "Authorization: Bearer SECRET"
```

ComicRust doit être lancé avec `--mcp` avant d'utiliser les outils.

### Claude Desktop

Dans `claude_desktop_config.json` (Réglages → Développeur → Modifier la configuration) :

```json
{
  "mcpServers": {
    "comicrust": {
      "command": "F:\\Projet REA par Claude\\ComicRust\\ComicRust-mcp.exe",
      "args": ["--mcp-stdio"]
    }
  }
}
```

La passerelle relaie les messages vers l'instance lancée avec `--mcp` ; si ComicRust n'est pas
ouvert, elle le démarre (avec `--mcp`). Pour un jeton : `"args": ["--mcp-stdio",
"--mcp-token=SECRET"]`.

## Outils

### Génériques (toute l'interface)

| Outil          | Rôle                                                                        |
|----------------|-----------------------------------------------------------------------------|
| `screenshot`   | capture PNG de la fenêtre ; 1 pixel = 1 point (coordonnées des clics)       |
| `ui_tree`      | widgets visibles : rôle, libellé, valeur, position, état (coché, désactivé…) |
| `click_widget` | clic au centre d'un widget trouvé par son libellé (ou son id)               |
| `wait_for`     | attend qu'un widget apparaisse (ou disparaisse)                             |
| `click`        | clic en (x, y) ; bouton droit, double clic, Ctrl/Maj                        |
| `hover`        | survol (bulles d'aide)                                                      |
| `drag`         | glisser-déposer                                                             |
| `scroll`       | molette                                                                     |
| `type_text`    | saisie dans le champ qui a le focus                                         |
| `press_key`    | touche ou raccourci (« ArrowRight », « Ctrl+O », « F11 », « Escape »)       |
| `wait`         | pause (ms)                                                                  |
| `get_logs`     | dernier rapport de plantage non encore affiché (`data\plantage.txt`)        |

### Propres à ComicRust

| Outil              | Rôle                                                                      |
|--------------------|---------------------------------------------------------------------------|
| `get_state`        | vue, livre ouvert (page / nombre de pages), bibliothèque (source, état, nombre de livres, liste et livre sélectionnés, recherche), thème, fenêtres ouvertes, erreur affichée |
| `list_books`       | livres de la bibliothèque (id, libellé, fichier), filtre = recherche rapide |
| `open_book`        | ouvre un livre (`id` de la bibliothèque ou `path`), page facultative      |
| `reader_goto_page` | page n (à partir de 1) ou `direction` next / prev                         |
| `close_book`       | ferme le livre (page mémorisée)                                           |
| `set_view`         | `library` ou `reader`                                                     |
| `list_lists`       | arbre des listes (id, nom, type)                                          |
| `select_list`      | affiche une liste (id ou nom), ou toute la bibliothèque                   |
| `set_search`       | texte de la recherche rapide                                              |
| `select_book`      | sélectionne un livre (panneau de détails)                                 |
| `close_dialog`     | ferme les fenêtres d'information (erreur, infos, à propos, bilans…)       |
| `get_errors`       | erreurs en cours et derniers rapports de plantage                         |
| `show_window`      | restaure la fenêtre réduite                                               |

## Méthode de test conseillée

1. `get_state` pour savoir où en est l'application.
2. Reproduire le scénario : de préférence avec les outils propres (`open_book`,
   `select_list`…), sinon par l'interface (`ui_tree` puis `click_widget`, `type_text`,
   `press_key`).
3. Vérifier : `screenshot` (rendu), `get_state` (valeurs), `get_errors` / `get_logs`.
4. Après un correctif : recompiler, relancer `ComicRust-mcp.exe --mcp`, rejouer le même
   scénario.

## Limites

- La fenêtre doit être affichée (pas réduite) pour les captures et les clics : `show_window`.
- Les vrais mouvements de souris au-dessus de la fenêtre se mêlent aux événements simulés :
  ne pas toucher la souris pendant une séance.
- Après une mise à jour intégrée, ComicRust redémarre sans `--mcp` : le relancer.
- Le code du serveur est dans le crate `crates/egui-mcp` (partagé avec PLaunch) ; les outils
  propres à ComicRust dans `crates/comicrust/src/mcp.rs`.
