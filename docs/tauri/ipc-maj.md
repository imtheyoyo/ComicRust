# Contrat IPC : mise à jour automatique (étape 9, `tauri-v0.8.0`, lots U et A)

Contrat entre le backend (`crates/comicrust-tauri`, module `maj`, lot U) et le front (`frontend/`, lot A).
Mêmes règles que `ipc-bibliotheque.md` : commandes `invoke`, promesse rejetée avec un **texte en français**,
JSON en **camelCase**, **aucune logique métier côté front**. Noyau dupliqué de la version egui
(`crates/comicrust/src/update/mod.rs`, non modifié).

## Résumé du contrat imposé

Commandes :

- `maj_etat() -> EtatMaj`
- `maj_verifier(manuel: bool) -> ()` : lance la vérification en arrière-plan ; résultat par l'événement.
- `maj_installer(tag: string) -> ()` : télécharge, vérifie le SHA-256, installe ; progression et résultat
  par événements.
- `maj_redemarrer() -> ()` : relance le même exe avec les mêmes arguments puis ferme l'appli.
- `maj_regler(verifierAuDemarrage?: bool, versionIgnoree?: string | null) -> EtatMaj`
- `maj_ouvrir_page(url: string) -> ()` *(précision du coordinateur)* : ouvre une page dans le navigateur
  par défaut ; seules les adresses commençant par `https://github.com/imtheyoyo/ComicRust/` sont acceptées.

Événements :

- `maj://statut` : `StatutMaj`
- `maj://progression` : `{ recu: number, total: number }` (≤ 10 par seconde)

Règles : une vérification automatique (`manuel = false`) n'émet `disponible` que si la version n'est pas
ignorée ; en cas d'erreur ou si à jour, elle reste silencieuse côté interface (le front n'affiche rien). Une
vérification manuelle affiche toujours le résultat.

## Types

```ts
type EtatMaj = {
  version: string;            // version en cours, ex. "0.8.0" (CARGO_PKG_VERSION de comicrust-tauri)
  versionComparee: string;    // version comparée aux releases (= version, ou COMICRUST_SIMULER_VERSION)
  verifierAuDemarrage: boolean; // réglage, vrai par défaut
  versionIgnoree: string | null; // tag ignoré par l'utilisateur, ex. "tauri-v0.8.1"
  statut: StatutMaj;          // statut courant (même forme que l'événement)
  installable: boolean;       // false hors Windows ou en développement : vérification seule
};

type StatutMaj = {
  etat: "inactif" | "verification" | "aJour" | "disponible" | "installation" | "installee" | "echec";
  manuel: boolean;
  release?: {                 // présent pour disponible, installation, installee (et echec d'installation)
    tag: string;              // "tauri-v0.8.1"
    nom: string;              // nom de la release (le tag si vide)
    notes: string;            // notes de version, markdown brut (corps de la release GitHub)
    urlPage: string;          // page de la release sur GitHub
    taille: number;           // taille du zip en octets (0 sans paquet)
    aPaquet: boolean;         // zip Windows ET son .sha256 présents : installation possible
  };
  erreur?: string;            // pour echec : message en français
  bilan?: { remplaces: number; restesOld: number }; // pour installee
};
```

Champs absents (`release`, `erreur`, `bilan`) : **non sérialisés** (pas de `null`).

- `bilan.remplaces` : fichiers installés (remplacés ou nouveaux, hors `data\`).
- `bilan.restesOld` : anciens fichiers renommés en `.old` (exe, `pdfium.dll`, `.md`, licences), supprimés au
  prochain démarrage.

## Commandes en détail

| Commande | Arguments | Réponse | Erreurs (promesse rejetée) |
|---|---|---|---|
| `maj_etat` | — | `EtatMaj` | jamais (réglages illisibles = valeurs par défaut) |
| `maj_verifier` | `manuel: bool` (absent = `true`) | `null` aussitôt | jamais ; sans effet pendant une vérification ou une installation |
| `maj_installer` | `tag: string` | `null` aussitôt (installation dans un fil) | non installable ; vérification/installation en cours ; déjà installée ; `tag` différent de la dernière release trouvée (« relancer la vérification ») ; release sans paquet |
| `maj_redemarrer` | — | (l'application se ferme) | relance impossible (l'application reste ouverte) |
| `maj_regler` | `verifierAuDemarrage?: bool`, `versionIgnoree?: string \| null` | `EtatMaj` à jour | type invalide ; écriture des réglages impossible |
| `maj_ouvrir_page` | `url: string` | `null` | adresse refusée (« adresse refusée : seules les pages https://github.com/imtheyoyo/ComicRust/… ») ; navigateur introuvable |

`maj_ouvrir_page` sert au lien de la release (`urlPage`) et aux liens des notes de version (aucun plugin
Tauri). Filtre : préfixe exact `https://github.com/imtheyoyo/ComicRust/` (casse comprise), 2 048 caractères
au plus, ni espace, ni caractère de contrôle, ni `" ' < > \ ^ \` | { }`. Les liens refusés (autre site) ne
doivent pas être cliquables côté front, ou afficher l'erreur. Ouverture : `explorer.exe <url>` sous Windows
(argument passé directement, sans interpréteur de commandes), `xdg-open` ailleurs.

**Une seule vérification à la fois** : la vérification automatique au démarrage est faite par le **backend
seul** (le front n'appelle pas `maj_verifier(false)` au démarrage). Un `maj_verifier` reçu pendant une
vérification en cours n'en lance pas une seconde : il s'y rattache ; s'il est manuel et que la vérification en
cours est automatique, celle-ci devient manuelle (événement `verification` avec `manuel: true`, puis résultat
toujours montré). Pendant une installation, `maj_verifier` est sans effet.

`maj_regler` : un champ **absent** ne change rien ; `versionIgnoree: null` (ou `""`) **efface** la version
ignorée. Le backend lit le JSON brut de l'appel pour distinguer absent et `null`.

Réglages : section `maj` de `data\comicrust-tauri.json`
(`{"maj": {"verifierAuDemarrage": false, "versionIgnoree": "tauri-v0.8.1"}}`), lecture tolérante (valeur
invalide ignorée), section omise si vide.

## Déroulement et événements

**Vérification manuelle** (`maj_verifier(true)`) :
`verification` → `disponible` (même si la version est ignorée) | `aJour` | `echec` (avec `erreur`),
tous avec `manuel: true`.

**Vérification automatique** (au démarrage si `verifierAuDemarrage`, en arrière-plan, sans bloquer
l'ouverture ; ou `maj_verifier(false)`) : **aucun événement** sauf `disponible` (`manuel: false`) quand la
version trouvée n'est pas la version ignorée (comparaison par numéro de version : `tauri-v0.8.1` = `0.8.1`).
Pendant la vérification, `maj_etat` rend `etat: "verification", manuel: false` (le front ne l'affiche pas) ;
ensuite, `inactif` si rien n'est proposé.

**Installation** (`maj_installer(tag)`) :
`installation` (`manuel: true`, `release`) → `maj://progression` répétés (au plus 10/s ; le dernier vaut
toujours `recu = total`) → `installee` (`bilan`) | `echec` (`erreur`, `release`).
`total` est la taille annoncée par GitHub (relevée à `recu` si plus petite).

**Redémarrage** (`maj_redemarrer`) : proposé après `installee`. L'application relance son exe avec les mêmes
arguments plus `--attendre-pid <pid>`, puis se ferme (`app.exit(0)` : la lecture des livres ouverts est
enregistrée comme à la fermeture normale).

## Côté backend (lot U)

- **Releases retenues** : tag commençant par `tauri-v` uniquement (les releases egui `vX.Y.Z` cohabitent dans
  `imtheyoyo/ComicRust`), brouillons ignorés, pré-releases acceptées, `per_page=50`. La plus récente plus
  récente que `versionComparee`.
- **Paquet** : asset `ComicRust-Tauri-{tag}-windows-x64.zip` et `…zip.sha256` (`release-tauri.yml`). Sans l'un
  des deux : `aPaquet: false`, installation refusée (le front propose la page de la release).
- **Réseau** : `ureq` 3 (certificats et proxy du système Windows), redirections 302 suivies à la main,
  délai global 600 s.
- **Téléchargement** dans `data\maj\` (`*.part` renommé à la fin), empreinte SHA-256 vérifiée **avant** toute
  modification ; extraction en chemins relatifs (`../` refusé) ; `ComicRust-Tauri.exe` obligatoire à la racine
  du paquet.
- **Remplacement** : chaque fichier existant devient `nom.old` (ou `nom.1.old`… si un `.old` existe déjà ; jamais
  un fichier écrasé), noté dans `data\maj\anciens.txt`. Le paquet contient `ComicRust-Tauri.exe`,
  `ComicRust-Tauri-mcp.exe`, `pdfium.dll`, `*.md`, `licences\`. **Rien sous `data\`** (réglages, thèmes, profil
  WebView2 `data\webview2`). Fichier verrouillé : `echec` avec un message clair ; les `.old` déjà créés restent
  notés ; si la copie d'un nouveau fichier échoue, l'ancien reprend sa place.
- **Démarrage** : `--attendre-pid <pid>` traité tout au début (attente de 10 s au plus de la fin de l'ancienne
  instance : profil WebView2 et port MCP libérés), puis nettoyage des `.old` listés et de `data\maj\` avant la
  création de la fenêtre. `--attendre-pid` n'est jamais retransmis (retiré des arguments d'une relance suivante).
- **Installable** : Windows, version livrée (`custom-protocol`, compilation optimisée). En développement ou hors
  Windows, la vérification fonctionne, l'installation est refusée.

## Recette et tests

- `COMICRUST_SIMULER_VERSION=0.7.0` : fait croire à une version plus ancienne (installation complète possible
  sans attendre la version suivante).
- `COMICRUST_MAJ_URL=http://127.0.0.1:8080/releases` : remplace l'adresse de l'API GitHub (faux serveur ; la
  réponse a le format de `GET /repos/{owner}/{repo}/releases`).
- Après une mise à jour, **relancer le client MCP** (Claude Desktop) : voir `docs/MCP-tauri.md`.
