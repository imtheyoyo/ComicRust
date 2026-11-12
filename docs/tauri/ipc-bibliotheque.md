# Contrat IPC : bibliothèque (étape 1, `tauri-v0.2.0`)

Contrat entre le backend (`crates/comicrust-tauri`) et le front (`frontend/`). Toutes les commandes
sont asynchrones côté front (`invoke`), renvoient une promesse rejetée avec un **texte en français**
en cas d'erreur, et sérialisent en **camelCase**. Le front n'a aucune logique métier : recherche, tri
et regroupement sont faits par le backend.

## Types

```ts
type Mode = 'autonome' | 'comicrack';

interface EtatBiblio {
  mode: Mode;              // 'autonome' tant que rien n'est choisi
  nbLivres: number;
  premierDemarrage: boolean; // true : le front affiche l'assistant, rien n'est encore ouvert
  source: 'sqlite' | 'mysql' | 'xml'; // d'où viennent les livres (étape 6) ; 'sqlite' au premier démarrage
}

interface ChoixPremierDemarrage {
  mode: Mode;
  cheminIni?: string;      // mode 'comicrack' : ComicRack.ini (MySQL) ou ComicDb.xml ; absent = recherche auto
}

interface RequeteLivres {
  recherche?: string;      // mots séparés par des espaces, tous requis (série, titre, auteurs, éditeur, fichier…)
  tri: string;             // 'serie' | 'titre' | 'editeur' | 'parution' | 'ajout' | 'lecture' | 'note'
                           // | 'pages' | 'avancement' | 'taille' | 'fichier'
                           // synonymes acceptés : 'numero' (= 'serie', qui trie déjà série puis numéro
                           // en ordre naturel : 2 avant 10) et 'annee' (= 'parution'). Autre valeur : erreur.
  sensTri: 'asc' | 'desc';
  groupe?: 'aucun' | 'serie' | 'editeur'; // défaut 'aucun' ; les groupes se suivent, puis le tri joue dedans
  debut: number;           // rang du premier livre demandé (0 = début)
  nombre: number;          // taille de la fenêtre (plafonnée à 5000)
}

interface LivreResume {
  id: string;              // GUID du livre (texte, pas un nombre)
  titre: string;           // titre, sinon « Série #n », sinon nom du fichier
  serie?: string; numero?: string; editeur?: string; annee?: number;
  pages: number;           // 0 si inconnu
  lu: boolean;             // 100 % lu
  urlCouverture: string;   // URL directement utilisable dans <img src> (voir « Couvertures »)
}

interface PageLivres { total: number; livres: LivreResume[]; } // total = après recherche, avant fenêtre

interface LivreDetail {    // champs absents = non renseignés
  id: string; titre: string; serie?: string; numero?: string; volume?: number;
  annee?: number; mois?: number; jour?: number; editeur?: string; collection?: string;
  pages: number; lu: boolean; progression?: number;   // 0-100
  pageCourante?: number;   // dernière page lue (à partir de 0)
  // Panneau Détail : « Lu : x % (page n / total) ». `detail_livre` fait primer la lecture faite
  // dans ComicRust (data\avancement.json) sur la fiche / base ComicRack quand elle est plus avancée
  // (comme la version egui) : `progression` et `lu` en tiennent compte.
  pageLue?: number;        // page la plus loin lue, à partir de 1 (absente : jamais ouvert)
  sourceLecture?: 'comicrack' | 'comicrust';   // origine de `progression`
  // `note` : échelle ComicRack 0-5, demi-étoiles possibles ; absente = non noté (étoiles du panneau).
  scenariste?: string; dessinateur?: string; encreur?: string; coloriste?: string;
  lettreur?: string; artisteCouverture?: string; redacteur?: string; traducteur?: string;
  genre?: string; arcNarratif?: string; personnages?: string; equipes?: string; lieux?: string;
  etiquettes?: string; resume?: string; notes?: string; critique?: string;
  format?: string; langue?: string; classementAge?: string; noirEtBlanc?: string; manga?: string;
  note?: number; noteCommunaute?: number; isbn?: string; web?: string;
  tailleFichier?: number;  // octets
  dateAjout?: string; derniereLecture?: string;       // ISO 8601
  fichier?: string;        // chemin complet
  fichierPresent: boolean; // le fichier existe sur le disque à l'instant de l'appel
  manquant: boolean;       // marqué manquant par le dernier scan / par ComicRack
  urlCouverture: string;
}
```

## Préférences d'affichage

```ts
// Section `vue` de data\comicrust-tauri.json. Champ absent = valeur par défaut du front.
interface PreferencesVue {
  tri?: ClefTri;                 // 'serie' | 'titre' | 'parution' | … (liste de RequeteLivres)
  sensTri?: 'asc' | 'desc';
  groupe?: 'aucun' | 'serie' | 'editeur';
  vue?: 'grille' | 'tableau';
  zoom?: number;                 // taille des couvertures, ramenée dans [100, 260]
  details?: boolean;             // panneau de détails affiché
  largeurDetails?: number;       // largeur du panneau, ramenée dans [220, 800]
  largeurListes?: number;        // largeur de la colonne des listes (barre latérale), ramenée dans [180, 500] ; défaut 250
  theme?: string;                // id du thème (fichier CSS de data\themes) ; disparu → thème par défaut
}
```

Mode portable strict : **rien dans le `localStorage` de la WebView** (les anciennes clés `comicrust.*`
de tauri-v0.2.0 sont reprises une fois puis effacées). Le front lit les préférences au démarrage,
**avant** le thème et le premier `lister_livres`, puis enregistre les changements après 400 ms sans
nouvelle modification (et aussitôt quand la page est masquée ou fermée). Mode simulé (navigateur) :
repli en mémoire ; `?prefs=<json>` simule un redémarrage.

Lecture tolérante : une valeur inconnue ou d'un mauvais type (fichier modifié à la main, version plus
récente) est ignorée champ par champ, sans rendre le fichier illisible. Les écritures (préférences, choix
du premier démarrage) passent par une lecture-modification-écriture sous verrou, dans un fichier
temporaire renommé ensuite : chacune ne change que sa partie.

## Commandes

| Commande | Arguments | Résultat |
|---|---|---|
| `lire_preferences_vue` | – | `PreferencesVue` (`{}` si rien n'est enregistré). Erreur si `comicrust-tauri.json` est illisible (le front prend alors les valeurs par défaut). |
| `enregistrer_preferences_vue` | `{preferences: PreferencesVue}` | `PreferencesVue` à jour. **Fusion** : seuls les champs fournis changent ; valeurs invalides ignorées. |
| `ouvrir_bibliotheque` | – | `EtatBiblio`. Lit `data\comicrust-tauri.json` ; si un mode est choisi, ouvre la base (autonome : SQLite `data\comicrust.db` ; comicrack : MySQL du `ComicRack.ini` ou `ComicDb.xml`, **lecture seule**). Sans choix : `{mode:'autonome', nbLivres:0, premierDemarrage:true}`. Rappelable pour recharger. |
| `configurer_premier_demarrage` | `{choix: ChoixPremierDemarrage}` | `EtatBiblio` (avec `premierDemarrage:false`). Autonome : crée la base. Comicrack : teste la connexion (erreur lisible si MySQL injoignable) ; rien n'est copié ni écrit dans ComicRack. |
| `lister_livres` | `{requete: RequeteLivres}` | `PageLivres` |
| `detail_livre` | `{id}` | `LivreDetail` |
| `lister_dossiers` | – | `string[]` (dossiers surveillés, triés ; vide en mode comicrack) |
| `ajouter_dossier` | `{chemin}` | `string[]` (liste à jour). Erreur si le dossier n'existe pas ou en mode comicrack. Doublon ignoré. |
| `retirer_dossier` | `{chemin}` | `string[]` (liste à jour). Les livres déjà en base restent affichés. |
| `lancer_scan` | – | `void`, rend la main aussitôt. Erreur si un scan tourne déjà, si aucun dossier, ou en mode comicrack. |

## Événements (`listen` côté front)

- `scan-progression` : `{fait: number, total: number}` environ 5 fois par seconde. `total` grandit pendant le parcours.
- `scan-termine` : `{ajoutes: number, misAJour: number, manquants: number}`. Le front recharge ensuite `lister_livres`.
- `scan-erreur` (extension au contrat) : `{message: string}` si le scan échoue (base illisible…) ; il remplace alors `scan-termine`.

## Couvertures

Protocole de ressources `cr-couv`, id du livre pour chemin. L'URL réellement chargeable dépend de la
plateforme de la WebView ; **le backend la fournit toute faite dans `urlCouverture`** :

- Windows (WebView2) : `http://cr-couv.localhost/<id>`
- ailleurs : `cr-couv://localhost/<id>`

Le front utilise donc `urlCouverture` tel quel (pas de reconstruction). Le serveur répond en JPEG
(miniature 360 px de haut, cache `data\couvertures\<id>.jpg`, générée à la demande, 4 fils de travail) ;
404 si le fichier est absent ou illisible (le front affiche un visuel de remplacement). Aucun base64 dans l'IPC.
La CSP (`tauri.conf.json`) autorise ces deux origines dans `img-src`.

## Notes

- Livres « manquants » (fichier disparu) : conservés dans la liste (`manquant:true` dans le détail) ;
  la couverture reste servie depuis le cache disque.
- Tri : valeurs vides toujours en dernier ; avec un groupe, les groupes se suivent par ordre alphabétique
  (inversé si `desc`), « (sans série) » / « (sans éditeur) » pour les valeurs vides.
- Mode `comicrack` : lecture seule, rien n'est copié ni écrit ; avec un `ComicRack.ini`, le mot de passe MySQL
  y reste (jamais dans `comicrust-tauri.json` en clair, ni dans les messages d'erreur). Connexion saisie à la
  main : voir « Connexion MySQL » ci-dessous.
- Réglages : `data\comicrust-tauri.json` (`{mode, cheminIni, storeMysql, comicrackMysql, vue, …}`), distinct du `comicrust.ron` de la version egui ;
  la base autonome est le même fichier `data\comicrust.db` que la version egui.
- `ouvrir_bibliotheque` et `configurer_premier_demarrage` répondent « analyse en cours… » (erreur) tant qu'un scan tourne.
- `fichierPresent` : vérifié avec un délai de 2 s ; un partage réseau muet donne `false`.
- Couvertures : cache disque lu tout de suite ; seules les absentes sont produites (tâches récentes d'abord,
  abandon au-delà de 30 s d'attente, 25 s max par lecture). 200 : `Cache-Control: private, max-age=3600` ;
  404 : `no-store`, mais l'échec est mémorisé 3 minutes côté Rust (rechargé après un scan).

## Connexion MySQL (étape 6, `tauri-v0.7.0`, lot A)

Formulaire (serveur, port, utilisateur, mot de passe masqué, base) + bouton « Tester », pour (i) la base
MySQL de ComicRack saisie à la main (sans `ComicRack.ini`) et (ii) la base de ComicRust en mode autonome
(SQLite par défaut, ou MySQL). Code : `crates/comicrust-tauri/src/connexion.rs`.

```ts
interface ParametresMysql {
  serveur: string; port: number; utilisateur: string; base: string;
  motDePasse?: string;     // absent = garder le mot de passe déjà enregistré ; '' = mot de passe vide
}
interface ParametresMysqlPublic {   // ce que le front relit : JAMAIS le mot de passe
  serveur: string; port: number; utilisateur: string; base: string;
  motDePasseDefini: boolean;
}
interface ConnexionsMysql {
  store: 'sqlite' | 'mysql';        // base de ComicRust (autonome)
  storeMysql?: ParametresMysqlPublic;
  comicrackMysql?: ParametresMysqlPublic; // source ComicRack saisie
}
interface ResultatTestMysql { nbLivres: number; description: string } // « utilisateur@serveur:port/base »
interface ChoixMysql {
  mode: Mode;
  store?: 'sqlite' | 'mysql';       // autonome ; défaut 'sqlite'
  mysql?: ParametresMysql;          // autonome + store 'mysql' : base de ComicRust ; comicrack : base de ComicRack
}
```

| Commande | Arguments | Résultat |
|---|---|---|
| `test_connexion_mysql` | `{cible: 'autonome' \| 'comicrack', parametres}` | `ResultatTestMysql`. `comicrack` : accès + table `comics` (`comicrust_core::db::test_connection`), rien n'est écrit. `autonome` : accès à la base de ComicRust (`Store::test`) ; **le schéma de ComicRust y est créé** s'il manque, comme à « Continuer » : à tester sur une base destinée à ComicRust. Erreur lisible (sans mot de passe) si injoignable ou refusé ; délai de connexion 8 s. |
| `lire_connexions_mysql` | – | `ConnexionsMysql` (sans mot de passe). |
| `configurer_mysql` | `{choix: ChoixMysql}` | `EtatBiblio` (`source: 'mysql'`, ou `'sqlite'` si `store` absent / `'sqlite'`). Même règle que `configurer_premier_demarrage` : le choix est ouvert d'abord, **un choix qui échoue n'est pas enregistré**. Erreur « paramètres MySQL manquants » si `mysql` manque quand il est requis. |

`configurer_premier_demarrage` reste inchangée (autonome SQLite, ou `ComicRack.ini` / `ComicDb.xml`).
Choisir un mode efface les connexions des autres choix (passer en SQLite supprime `storeMysql`).

### Persistance et mot de passe

`data\comicrust-tauri.json` gagne deux sections facultatives (anciens fichiers sans elles : lus sans
changement) :

```json
{ "storeMysql":     { "host": "…", "port": 3306, "user": "…", "password": "dpapi:<hex>", "database": "…" },
  "comicrackMysql": { "…": "même forme" } }
```

- `storeMysql` : base de ComicRust en MySQL (absente = SQLite `data\comicrust.db`). `comicrackMysql` :
  source ComicRack saisie, utilisée quand `cheminIni` est absent.
- Le mot de passe est chiffré par `comicrust_core::secret` (DPAPI, lié au compte Windows). **Jamais en clair**
  dans le fichier, les journaux, les erreurs (remplacé par `***` si une couche basse le répétait) ni dans l'IPC.
  Hors Windows, aucun coffre : il n'est pas gardé (à ressaisir). Fichier copié sur un autre PC / compte :
  mot de passe vide, l'ouverture échoue avec l'indication « saisis-le à nouveau ».
- Lecture tolérante : entrée qui n'est pas un objet ou sans serveur / utilisateur / base = ignorée ; port
  hors bornes = 3306 ; mot de passe illisible = vide.
- `ouvrir_bibliotheque` : autonome → `StoreConfig::MySql` si `storeMysql`, sinon SQLite ; comicrack sans
  `cheminIni` → `comicrackMysql` (lecture seule, ou écriture si l'option « Lecture et écriture » est cochée).

### Tests

Unitaires sans serveur (port 1 de la boucle locale pour l'échec). Tests sur un vrai MySQL : variable
d'environnement `COMICRUST_TEST_MYSQL` (chaîne de connexion d'une base de test, jamais `comickrack_demo`) ;
ignorés si elle est absente ou si le serveur est injoignable.
