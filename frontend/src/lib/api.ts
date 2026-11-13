// Couche unique d'appel des commandes Tauri. Le front n'a aucune logique métier : il affiche ce que
// renvoient les commandes (crates/comicrust-tauri/src/app.rs). Les structures sont en camelCase
// (serde `rename_all = "camelCase"`), sauf `InfosApplication` et `Theme` (étape 0, snake_case).
//
// Hors de la fenêtre Tauri (navigateur, `npm run dev`), tous les appels sont routés vers
// `simule.ts` (MODE SIMULÉ : 5 000 livres factices) afin de développer et tester sans le backend.
import type { EventCallback, UnlistenFn } from '@tauri-apps/api/event';

// ───────────────────────────── Types du contrat IPC ─────────────────────────────

export interface InfosApplication {
  version: string;
  dossier_data: string;
  dossier_themes: string;
  dossier_webview: string;
}

export interface Theme {
  id: string;
  nom: string;
  livre: boolean;
}

export type ModeBiblio = 'autonome' | 'comicrack';

/** D'où viennent les livres : base SQLite, base MySQL (de ComicRust ou de ComicRack) ou ComicDb.xml. */
export type SourceBase = 'sqlite' | 'mysql' | 'xml';

export interface EtatBiblio {
  mode: ModeBiblio;
  nbLivres: number;
  premierDemarrage: boolean;
  source: SourceBase;
}

/** Connexion MySQL saisie (formulaire). Le mot de passe ne revient jamais du backend. */
export interface ParametresMysql {
  serveur: string;
  port: number;
  utilisateur: string;
  /** Absent : garder le mot de passe déjà enregistré pour cette connexion. */
  motDePasse?: string;
  base: string;
}

/** Connexion enregistrée, telle que relue : sans mot de passe, seulement « défini oui / non ». */
export interface ParametresMysqlPublic {
  serveur: string;
  port: number;
  utilisateur: string;
  motDePasseDefini: boolean;
  base: string;
}

export type TypeStore = 'sqlite' | 'mysql';

export interface ConnexionsMysql {
  /** Base de ComicRust choisie en mode autonome. */
  store: TypeStore;
  storeMysql?: ParametresMysqlPublic;
  /** Base MySQL de ComicRack saisie à la main (sans ComicRack.ini). */
  comicrackMysql?: ParametresMysqlPublic;
}

/** Ce que « Tester » vérifie : la base de ComicRust (autonome) ou celle de ComicRack. */
export type CibleTestMysql = 'autonome' | 'comicrack';

export interface ResultatTestMysql {
  nbLivres: number;
  /** « utilisateur@serveur:port/base », sans mot de passe. */
  description: string;
}

export interface ChoixMysql {
  mode: ModeBiblio;
  /** Mode autonome : `sqlite` (défaut) ou `mysql` (alors `mysql` est obligatoire). */
  store?: TypeStore;
  /** Autonome + `store: 'mysql'` : base de ComicRust. Mode `comicrack` : base de ComicRack saisie. */
  mysql?: ParametresMysql;
}

export interface ChoixPremierDemarrage {
  mode: ModeBiblio;
  /** Chemin du fichier ComicRack.ini (mode `comicrack` uniquement). */
  cheminIni?: string;
}

/** Clés de tri acceptées par `lister_livres` (docs/tauri/ipc-bibliotheque.md ; le backend trie). */
export type ClefTri =
  | 'serie' | 'titre' | 'parution' | 'ajout' | 'lecture' | 'note' | 'pages' | 'avancement' | 'taille' | 'fichier'
  /** « Ordre de la liste » (rang dans la liste affichée ; sans liste : ordre de la bibliothèque). */
  | 'ordre';
export type SensTri = 'asc' | 'desc';
export type Groupe = 'aucun' | 'serie' | 'editeur';

export interface RequeteLivres {
  recherche?: string;
  tri: ClefTri;
  sensTri: SensTri;
  groupe?: Groupe;
  /** Étape 5 : seulement les livres de cette liste (identifiant d'un NoeudListe). */
  liste?: string;
  /** Pile ouverte (vue empilée) : seulement les livres de cette série (titre d'une `Pile`). */
  serie?: string;
  debut: number;
  nombre: number;
}

/** Pile de la vue empilée par série (`lister_piles`). */
export interface Pile {
  /** Série affichée ; « (sans série) » pour les livres sans série. */
  serie: string;
  nbLivres: number;
  /** Livres lus à 100 %. */
  nbLus: number;
  /** Identifiants des livres de la pile, dans l'ordre de la vue. */
  ids: string[];
  /** Couverture du premier livre. */
  urlCouverture: string;
}

export interface ReponsePiles {
  totalLivres: number;
  piles: Pile[];
}

export interface LivreResume {
  id: string;
  titre: string;
  serie?: string;
  numero?: string;
  editeur?: string;
  annee?: number;
  pages: number;
  lu: boolean;
  /** À utiliser telle quelle : `cr-couv://<id>`, ou `http://cr-couv.localhost/<id>` sous Windows. */
  urlCouverture: string;
  /** Avec `liste` : rang du livre dans la liste (1 = premier), colonne « # ». */
  rang?: number;
}

export interface ReponseLivres {
  total: number;
  livres: LivreResume[];
}

/** Relecture en cours (`LivreDetail.relecture`). */
export interface Relecture {
  /** Page courante de la relecture, à partir de 1. */
  page: number;
  /** Nombre de pages du livre. */
  total: number;
  /** Avancement de la relecture, 1 à 99. */
  pourcentage: number;
}

/** Champs absents = non renseignés. Seuls les champs courants sont déclarés ici. */
export interface LivreDetail extends LivreResume {
  volume?: number;
  mois?: number;
  jour?: number;
  collection?: string;
  progression?: number;
  pageCourante?: number;
  /** Page la plus loin lue (à partir de 1) d'après `sourceLecture`. */
  pageLue?: number;
  /** Origine de `progression` : fiche / base ComicRack, ou lecture faite dans ComicRust (plus avancée). */
  sourceLecture?: 'comicrack' | 'comicrust';
  /** Relecture en cours d'un livre déjà lu (lecture ComicRust) : le livre reste `lu`, le panneau
   *  affiche aussi « relecture en cours : x % (page n / total) ». Absente : pas de relecture. */
  relecture?: Relecture;
  scenariste?: string;
  dessinateur?: string;
  encreur?: string;
  coloriste?: string;
  lettreur?: string;
  genre?: string;
  arcNarratif?: string;
  resume?: string;
  format?: string;
  langue?: string;
  note?: number;
  isbn?: string;
  tailleFichier?: number;
  dateAjout?: string;
  /** Chemin complet du fichier. */
  fichier?: string;
  fichierPresent: boolean;
  manquant: boolean;
}

/** Préférences d'affichage mémorisées dans data\comicrust-tauri.json (section `vue`).
 *  Champ absent = valeur par défaut du front. */
export interface PreferencesVue {
  tri?: ClefTri;
  sensTri?: SensTri;
  groupe?: Groupe;
  vue?: 'grille' | 'tableau';
  /** Vue empilée par série (grille seulement). */
  empiler?: boolean;
  /** Taille des couvertures (100 à 260 px). */
  zoom?: number;
  details?: boolean;
  /** Largeur du panneau de détails (220 à 800 px). */
  largeurDetails?: number;
  /** Largeur de la colonne des listes (180 à 500 px). */
  largeurListes?: number;
  /** Identifiant du thème (fichier CSS de data\themes). */
  theme?: string;
  /** Réglages du lecteur (sous-objet remplacé en entier à chaque enregistrement). */
  lecteur?: PreferencesLecteur;
  /** Vue propre à chaque liste (clé : identifiant). À l'enregistrement, fusion liste par liste ;
   *  `null` efface la vue d'une liste (liste supprimée). */
  listes?: Record<string, VueListe | null>;
}

/** Vue mémorisée d'une liste (tri, sens, groupe, grille / tableau), comme dans ComicRack. */
export interface VueListe {
  tri?: ClefTri;
  sensTri?: SensTri;
  groupe?: Groupe;
  vue?: 'grille' | 'tableau';
  /** Vue empilée par série de cette liste. */
  empiler?: boolean;
}

// ───────────────────────────── Lecteur (étape 2) ─────────────────────────────

export type ModeLecture = 'simple' | 'double' | 'defilement';
export type Ajustement = 'meilleur' | 'largeur' | 'hauteur' | 'reelle';

/** Réglages du lecteur, section `vue.lecteur` de data\comicrust-tauri.json. Champ absent = défaut.
 *  Le backend les garde tels quels (et ignore les champs inconnus ou invalides). */
export interface PreferencesLecteur {
  /** Défaut `simple`. */
  mode?: ModeLecture;
  /** Défaut `meilleur` (page entière). */
  ajustement?: Ajustement;
  /** Double page : la couverture (1re page) est affichée seule. Défaut `true`. */
  couvertureSeule?: boolean;
  /** Sens de lecture droite → gauche pour les livres non marqués manga. Défaut `false`
   *  (un livre marqué manga dans ses infos s'ouvre toujours en sens manga). */
  manga?: boolean;
  /** Bandeau de miniatures affiché. Défaut `false`. */
  miniatures?: boolean;
  /** Grossissement de la loupe (1,5 à 6). Défaut 2,5. */
  grossissementLoupe?: number;
  /** Quand la bibliothèque et ce poste n'ont pas retenu la même page (`pageRepriseComicrack`) :
   *  demander (défaut), prendre celle de la bibliothèque, ou celle de ComicRust (réglage
   *  « Reprise » de la version egui). */
  reprise?: ChoixReprise;
}

export type ChoixReprise = 'demander' | 'comicrack' | 'comicrust';

/** Type de page ComicRack (`FrontCover`, `Story`, `Advertisement`…), informatif. */
export interface PageInfo {
  /** Numéro de page du livre (rang dans la liste complète, pages supprimées comprises, 0 = première) :
   *  index des URL de pages, de `pageReprise` et de `enregistrer_avancement`. `pages[i].index === i`. */
  index: number;
  /** Image du fichier (rang dans l'ordre naturel des noms) ; diffère de `index` si réordonné. */
  image: number;
  /** Dimensions en pixels de l'image d'origine (avant rotation), si connues (sinon déduites au chargement). */
  largeur?: number;
  hauteur?: number;
  /** Page double (scan de deux pages côte à côte) : toujours affichée seule en double page. */
  double: boolean;
  type?: string;
  /** Page supprimée dans ComicRack : masquée par le lecteur. */
  supprimee: boolean;
  /** Rotation à appliquer à l'affichage, en degrés (0, 90, 180, 270). */
  rotation: number;
}

export interface LivreOuvert {
  /** Identifiant de la session de lecture, à rendre à fermer_livre / enregistrer_avancement. */
  jeton: string;
  /** GUID du livre ; absent pour un fichier hors bibliothèque (ouvrir_fichier). */
  id?: string;
  titre: string;
  /** Chemin complet du fichier ouvert. */
  fichier: string;
  /** Format (« eComic (ZIP) », « Document PDF », « Dossier d'images »…). */
  format: string;
  /** Nombre de pages du livre (pages supprimées comprises) = pages.length. */
  nbPages: number;
  /** Numéro de la page à afficher à l'ouverture (jamais une page supprimée) : celle de ComicRust. */
  pageReprise: number;
  /** Présent si la page retenue par la bibliothèque (ComicRack, ou base ComicRust) diffère de
   *  `pageReprise` : le lecteur propose le choix (réglage `reprise`). */
  pageRepriseComicrack?: number;
  /** Livre marqué manga (lecture droite → gauche). */
  manga: boolean;
  /** Base des URL des pages, utilisée telle quelle : `${urlBase}/${index}?l=${largeurMax}`. */
  urlBase: string;
  pages: PageInfo[];
}

export interface ResultatScan {
  ajoutes: number;
  misAJour: number;
  manquants: number;
}

export interface ErreurScan {
  message: string;
}

export interface ProgressionScan {
  fait: number;
  total: number;
}

// ───────────────────────────── Infos et édition (étape 3, docs/tauri/ipc-edition.md) ─────────────────────────────

/** Section `edition` de data\comicrust-tauri.json. */
export interface ReglagesEdition {
  /** Option « Lecture et écriture » du mode lié ComicRack (défaut false = lecture seule). */
  ecritureComicrack: boolean;
  /** Case « Écrire aussi les infos dans le fichier ». */
  ecrireDansFichier: boolean;
  /** Conversion : ranger l'original dans « _originaux ». */
  rangerOriginal: boolean;
  modeleNom?: string;
  dossierCible?: string;
}

export type TypeChamp =
  | 'texte' | 'texteLong' | 'liste' | 'entier' | 'note' | 'prix' | 'ouiNon' | 'manga' | 'booleen' | 'date' | 'langue';

export interface Choix { valeur: string; libelle: string }

export interface ChampFiche {
  /** Nom ComicRack (`Series`, `Writer`…) ou `CustomValue:<clé>`. */
  nom: string;
  libelle: string;
  type: TypeChamp;
  /** Texte à afficher et modifier ('' si vide ou si multiple). */
  valeur: string;
  /** Plusieurs livres aux valeurs différentes. */
  multiple: boolean;
  choix?: Choix[];
  min?: number;
  max?: number;
  /** Le champ propose les valeurs de la bibliothèque (proposer_valeurs). */
  suggestions: boolean;
  dansComicInfo: boolean;
  /** Texte d'aide du champ vide. */
  indication?: string;
}

export interface Onglet { nom: string; champs: string[] }

export interface LivreEdite {
  id: string;
  titre: string;
  fichier?: string;
  urlCouverture: string;
  pages: number;
  tailleFichier?: number;
  progression?: number;
  /** Infos pas encore écrites dans le fichier (ComicInfoIsDirty). */
  infosAEcrire: boolean;
  /** Le fichier reçoit les infos (CBZ/ZIP). */
  cbz: boolean;
}

export interface FicheEdition {
  ids: string[];
  multiple: boolean;
  livres: LivreEdite[];
  onglets: Onglet[];
  champs: ChampFiche[];
  /** Présent : consultation seulement, avec la marche à suivre. */
  lectureSeule?: string;
  ecrireDansFichierPossible: boolean;
  ecrireDansFichier: boolean;
  seuilConfirmation: number;
}

/** `valeur` '' : vider le champ. */
export interface ChampModifie { nom: string; valeur: string }
export interface ErreurChamp { nom: string; message: string }
/** `texte` : contenu du champ si on choisit la proposition. */
export interface Proposition { valeur: string; texte: string }

export interface DemandeModification {
  ids: string[];
  /** SEULEMENT les champs modifiés par l'utilisateur. */
  champs: ChampModifie[];
  /** Liste COMPLÈTE des pages (un seul livre), seulement si l'onglet Pages a été modifié. */
  pages?: PageModifiee[];
  ecrireDansFichier: boolean;
  confirme?: boolean;
}

export type Operation = 'modification' | 'ecritureInfos' | 'conversion' | 'renommage';
export type Statut = 'ok' | 'inchange' | 'ignore' | 'erreur';

export interface ResultatLivre {
  id: string;
  titre: string;
  statut: Statut;
  message?: string;
  fichier?: string;
}

export interface BilanOperation {
  operation: Operation;
  /** Présent : rien n'a été fait, rappeler avec `confirme: true` après accord. */
  confirmationRequise?: { nbLivres: number; seuil: number; message: string };
  resume: string;
  complet: boolean;
  interrompu: boolean;
  livres: ResultatLivre[];
  /** Bilan à copier (une ligne par livre). */
  rapport: string;
}

export interface ProgressionEdition { operation: Operation; fait: number; total: number; livre: string }
/** `origine: 'externe'` : modification faite hors de ComicRust (ComicRack, autre poste), voir
 *  docs/tauri/ipc-surveillance.md ; absent : opération d'édition de ComicRust. */
export interface BibliothequeModifiee {
  ids: string[];
  origine?: 'externe';
  ajoutes?: number;
  modifies?: number;
  supprimes?: number;
}

// Surveillance de la base et état hors ligne (étape 6, docs/tauri/ipc-surveillance.md).
export type SourceSurveillee = 'comicrack-mysql' | 'comicrust' | 'comicrack-xml' | 'aucune';
export interface EtatBase {
  source: SourceSurveillee;
  /** La base est interrogée régulièrement (MySQL ComicRack, base de ComicRust). */
  surveillee: boolean;
  enLigne: boolean;
  /** Cause de l'échec (sans mot de passe), quand `enLigne` est faux. */
  message?: string;
  /** Échecs consécutifs depuis la perte de la connexion (0 : en ligne). */
  essais: number;
}

export interface ModeleNom { nom: string; modele: string }

export interface DemandeRenommage {
  ids: string[];
  modele: string;
  /** Absent ou vide : chaque fichier reste dans son dossier. */
  dossierCible?: string;
  apercu: boolean;
  confirme?: boolean;
}

export interface LigneRenommage {
  id: string;
  titre: string;
  de: string;
  vers: string;
  deNom: string;
  versNom: string;
  /** Présent : cette ligne ne sera pas appliquée. */
  probleme?: string;
  inchange: boolean;
}

export interface ResultatRenommage {
  lignes: LigneRenommage[];
  aRenommer: number;
  bilan?: BilanOperation;
}

// ───── Onglet Pages (étape 4, docs/tauri/ipc-pages.md) ─────

export type TypePage = 'FrontCover' | 'InnerCover' | 'Roundup' | 'Story' | 'Advertisement' | 'Editorial'
  | 'Letters' | 'Preview' | 'BackCover' | 'Other' | 'Deleted';

export interface ChoixPage { valeur: string; libelle: string; raccourci?: string }

/** Une page de la liste complète (ordre de lecture). */
export interface PageLivre {
  /** Rang dans l'ordre de lecture (recalculé par le backend) : `pages[i].index === i`. */
  index: number;
  /** Rang de l'image dans le fichier : identifiant STABLE de la page (sélection, opérations). */
  image: number;
  /** Nom de l'image dans l'archive. */
  nom: string;
  /** Valeur ComicRack (une TypePage, rarement des drapeaux combinés à renvoyer tels quels). */
  type: string;
  libelleType: string;
  supprimee: boolean;
  /** Page double (lecture seule, calculée comme ComicRack). */
  double: boolean;
  /** 0 | 90 | 180 | 270, à appliquer à l'affichage (la vignette n'est pas tournée). */
  rotation: number;
  /** 'Default' | 'Near' | 'Far'. */
  position: string;
  favori?: string | null;
  largeur?: number;
  hauteur?: number;
  urlVignette: string;
}

export interface PagesLivre {
  id: string;
  /** Séance de vignettes (protocole cr-page), à fermer par fermer_pages. */
  jeton: string;
  urlBase: string;
  largeurVignette: number;
  nbImages: number;
  /** Liste complète dans l'ordre de lecture ENREGISTRÉ, supprimées comprises. */
  pages: PageLivre[];
  modifiable: boolean;
  lectureSeule?: string;
  ecrireDansFichierPossible: boolean;
  types: ChoixPage[];
  /** valeur '0' | '90' | '180' | '270'. */
  rotations: ChoixPage[];
  positions: ChoixPage[];
}

/** Opération sur la liste, calculée par le backend (règles de ComicRack). */
export type OperationPages =
  | { op: 'deplacer'; images: number[]; position: number }
  | { op: 'mettreAuDebut'; images: number[] }
  | { op: 'mettreALaFin'; images: number[] }
  | { op: 'type'; images: number[]; valeur: string }
  | { op: 'basculerSupprimee'; images: number[] }
  | { op: 'rotation'; images: number[]; valeur: number }
  | { op: 'position'; images: number[]; valeur: string }
  | { op: 'favori'; image: number; texte?: string }
  | { op: 'ordreOrigine' }
  | { op: 'trierParNom'; numerique: boolean }
  | { op: 'reinitialiserTypes' }
  | { op: 'toutReinitialiser' };

/** Page envoyée à appliquer_modifications (un PageLivre convient tel quel). */
export interface PageModifiee {
  image: number;
  type?: string;
  supprimee?: boolean;
  rotation?: number;
  position?: string;
  favori?: string | null;
}

// ───── Listes ComicRack (étape 5, docs/tauri/ipc-listes.md) ─────

export type TypeListe = 'bibliotheque' | 'dossier' | 'lecture' | 'intelligente';
/** Dossier : réunion, intersection, aucun livre (rangement). */
export type Combinaison = 'ou' | 'et' | 'vide';

export interface EtatListes {
  source: 'comicrust' | 'comicdb' | 'aucune';
  /** ComicDb.xml (source 'comicdb'). */
  chemin?: string;
  /** Commandes de modification utilisables maintenant. */
  modifiable: boolean;
  /** Sinon, la raison (infobulle des commandes grisées, 🔒 sous le panneau). */
  lectureSeule?: string;
  /** ComicRack.exe détecté. */
  comicrackLance: boolean;
  /** Listes illisibles (ComicDb.xml introuvable, XML invalide…). */
  erreur?: string;
  /** Libellé de l'opération en cours. */
  enCours?: string;
  /** Change à chaque relecture des listes : arbre à recharger si différent. */
  version: number;
}

export interface NoeudListe {
  /** GUID (casse d'origine), à renvoyer tel quel. */
  id: string;
  nom: string;
  type: TypeListe;
  description?: string;
  /** Livres présents dans la bibliothèque, calculés par ComicRust. */
  nbLivres?: number;
  /** nbLivres absent : pourquoi (critère non pris en charge…). */
  inconnu?: string;
  /** Compte enregistré par ComicRack à sa dernière sauvegarde. */
  nbComicrack: number;
  combinaison?: Combinaison;
  /** Liste de lecture modifiable : dépôt de livres et « Ajouter à la liste ». */
  accepteLivres: boolean;
  peutMonter: boolean;
  peutDescendre: boolean;
  enfants?: NoeudListe[];
}

export interface ArbreListes {
  etat: EtatListes;
  /** Premier niveau, dans l'ordre de ComicRack (la « Bibliothèque » comprise si elle existe). */
  listes: NoeudListe[];
}

export interface ResultatListes {
  arbre: ArbreListes;
  /** Liste à sélectionner (null : toute la bibliothèque) ; absent : inchangée. */
  selection?: string | null;
  /** Liste à passer aussitôt en renommage. */
  renommer?: string;
  /** Liste créée (ses dossiers parents sont à déplier). */
  creee?: string;
  /** Bilan à montrer sous le panneau. */
  info: string;
  sauvegarde?: string;
  avertissements: string[];
}

export type PlaceListe = 'avant' | 'dans' | 'apres';

export interface DemandeNouvelleListe {
  type: 'lecture' | 'dossier';
  nom?: string;
  parent?: string;
  position?: number;
  livres?: string[];
  /** Juste après cette liste dans son dossier. */
  apresListe?: string;
}

export interface InfosSuppression { nom: string; sousListes: number; dependantes: string[] }
export interface InfosConversion { nom: string; nbLivres: number }
export interface DestinationListe { id: string | null; nom: string; profondeur: number; actuel: boolean }
export interface GroupeVue { titre: string; nombre: number }

export type ModeCriteres = 'tous' | 'auMoinsUn';
export type NatureValeur = 'texte' | 'nombre' | 'date' | 'ouiNon' | 'manga' | 'activation' | 'valeurPerso' | 'expression';

export interface Operateur { op: number; libelle: string; valeurs: 0 | 1 | 2; unite?: string }
export interface TypeCritere {
  /** xsi:type (« ComicBookSeriesMatcher »). */
  type: string;
  libelle: string;
  nature: NatureValeur;
  unite?: string;
  statSerie: boolean;
  /** ComicRust sait le calculer. */
  evalue: boolean;
  /** Case « casse » utile (IgnoreCase). */
  texte: boolean;
  operateurs: Operateur[];
}
export interface CatalogueCriteres {
  categories: Array<{ nom: string; criteres: TypeCritere[] }>;
  defaut: { type: string; op: number };
}

export interface Critere {
  genre: 'critere';
  non: boolean;
  type: string;
  op: number;
  /** valeurPerso : valeur = nom de la valeur, valeur2 = texte cherché. */
  valeur: string;
  valeur2: string;
  ignorerCasse: boolean;
  /** Fourni par lire_criteres : À RENVOYER TEL QUEL. */
  origine?: string;
  /** false : critère absent du catalogue, affiché en lecture seule et gardé tel quel. */
  modifiable: boolean;
  libelle?: string;
}
export interface GroupeCriteres {
  genre: 'groupe';
  non: boolean;
  mode: ModeCriteres;
  noeuds: NoeudCritere[];
  origine?: string;
}
export type NoeudCritere = Critere | GroupeCriteres;

export interface Limite { valeur: string; unite: 'livres' | 'mo' | 'go'; selection: 'hasard' | 'position' | 'serie' }
export interface CriteresListe {
  nom: string;
  description: string;
  mode: ModeCriteres;
  noeuds: NoeudCritere[];
  depart?: string | null;
  pasDansDepart: boolean;
  limite?: Limite | null;
  /** Livres retirés à la main dans ComicRack (lecture). */
  retires: number;
  reinitialiserRetires?: boolean;
}
export interface ListeDepart { id: string; nom: string; profondeur: number; type: TypeListe }
export interface EditeurListe {
  id?: string;
  criteres: CriteresListe;
  listesDepart: ListeDepart[];
  /** Présent : consultation seulement. */
  lectureSeule?: string;
}
export interface Apercu { nbLivres?: number; inconnu?: string; erreurs: string[] }

export interface ManquantCbl { rang: number; serie: string; numero: string; volume?: number; annee?: number }
export interface RapportCbl {
  nom: string;
  total: number;
  trouves: number;
  parIdentifiant: number;
  parSerie: number;
  doublons: number;
  manquants: ManquantCbl[];
  /** Bilan copiable. */
  texte: string;
}

// ───────────────────────────── Mise à jour (tauri-v0.8.0) ─────────────────────────────
// Contrat : docs/tauri/ipc-maj.md. Le backend vérifie les releases GitHub, télécharge le paquet,
// vérifie son SHA-256 et l'installe ; le front n'affiche que l'état reçu.

/** Release proposée (`StatutMaj.release`). */
export interface ReleaseMaj {
  /** Étiquette GitHub, ex. « tauri-v0.8.1 ». */
  tag: string;
  nom: string;
  /** Notes de version en markdown brut (rendues en texte simple, jamais en HTML). */
  notes: string;
  urlPage: string;
  /** Taille du paquet en octets. */
  taille: number;
  /** La release a un paquet Windows installable. */
  aPaquet: boolean;
}

export type EtatVerificationMaj = 'inactif' | 'verification' | 'aJour' | 'disponible' | 'installation' | 'installee' | 'echec';

/** Charge de l'événement `maj://statut` (et champ `statut` de `EtatMaj`). */
export interface StatutMaj {
  etat: EtatVerificationMaj;
  /** Vérification demandée par l'utilisateur (sinon automatique, au démarrage). */
  manuel: boolean;
  release?: ReleaseMaj;
  erreur?: string;
  bilan?: BilanMaj;
}

/** Bilan d'une installation réussie (`StatutMaj.bilan`, état `installee`). */
export interface BilanMaj {
  /** Fichiers installés (remplacés ou nouveaux, hors `data\`). */
  remplaces: number;
  /** Anciens fichiers renommés en `.old`, supprimés au prochain démarrage. */
  restesOld: number;
  /** Scripts de `scripts\` modifiés par l'utilisateur et gardés : `.nouveau` posés à côté. */
  scriptsNouveaux: number;
  /** Chemins des `.nouveau` posés (absent si aucun). */
  cheminsNouveaux?: string[];
  /** Scripts livrés supprimés par l'utilisateur, non réinstallés (absent si aucun). */
  scriptsNonReinstalles?: string[];
}

export interface EtatMaj {
  /** Version installée, ex. « 0.8.0 ». */
  version: string;
  /** Version comparée aux releases (différente en mode recette). */
  versionComparee: string;
  verifierAuDemarrage: boolean;
  versionIgnoree: string | null;
  statut: StatutMaj;
  /** Faux hors Windows ou en développement : l'installation est impossible. */
  installable: boolean;
}

/** Charge de l'événement `maj://progression` (au plus 10 par seconde). */
export interface ProgressionMaj {
  recu: number;
  total: number;
}

// ───────────────────────────── Aiguillage Tauri / simulé ─────────────────────────────

/** `true` dans la fenêtre Tauri ; `false` dans un navigateur (mode simulé). */
export const dansTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function appeler<T>(commande: string, args?: Record<string, unknown>): Promise<T> {
  if (dansTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(commande, args);
  }
  const { appelerSimule } = await import('./simule');
  return (await appelerSimule(commande, args ?? {})) as T;
}

/** Abonnement à un événement du backend (ou de la simulation). Renvoie la fonction de désabonnement. */
export async function ecouter<T>(nom: string, rappel: (charge: T) => void): Promise<UnlistenFn> {
  if (dansTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    const cb: EventCallback<T> = (e) => rappel(e.payload);
    return listen<T>(nom, cb);
  }
  const { ecouterSimule } = await import('./simule');
  return ecouterSimule(nom, rappel as (c: unknown) => void);
}

// ───────────────────────────── Transfert ComicRack → ComicRust (étape 6) ─────────────────────────────
// Contrat : docs/tauri/ipc-transfert.md. Le mot de passe MySQL n'est jamais renvoyé par le backend.

export type SourceTransfert = 'mysql' | 'comicdb';

export interface ApercuTransfert {
  /** Base MySQL de ComicRack trouvée (`utilisateur@hôte:port/base`, sans mot de passe). */
  mysql?: string;
  sourceParDefaut: SourceTransfert;
  /** `ComicDb.xml` proposé ('' : introuvable). */
  comicdb: string;
  comicdbPresent: boolean;
  destination: string;
  destinationExiste: boolean;
  nbLivresDestination?: number;
  mode: ModeBiblio;
  /** Présent : le transfert ne peut pas démarrer maintenant. */
  bloque?: string;
}

export interface DemandeTransfert {
  source: SourceTransfert;
  comicdb?: string;
  listes: boolean;
  dossiers: boolean;
  basculer: boolean;
}

export interface ResumeTransfert {
  livresSource: number;
  ajoutes: number;
  remplaces: number;
  misAJour: number;
  ecartes: number;
  doublonsEvites: number;
  listes: number;
  dossiers: number;
  cheminsRamenes: number;
  fichiersGardes: number;
  interrompu: boolean;
  avertissements: string[];
  nbLivresDestination?: number;
  doublonsRestants?: number;
  /** Phrase de bilan prête à afficher. */
  resume: string;
  bascule: boolean;
  /** Présent : la bibliothèque a été rouverte (le front met à jour son état). */
  etat?: EtatBiblio;
}

export interface ProgressionTransfert { phase: string; fait: number; total: number }

// ───────────────────────────── Commandes ─────────────────────────────

export const infos = () => appeler<InfosApplication>('infos');
export const listerThemes = () => appeler<Theme[]>('lister_themes');
export const chargerTheme = (id: string) => appeler<string>('charger_theme', { id });

export const lirePreferencesVue = () => appeler<PreferencesVue>('lire_preferences_vue');
/** Fusionne les champs fournis ; renvoie l'ensemble à jour. */
export const enregistrerPreferencesVue = (preferences: PreferencesVue) =>
  appeler<PreferencesVue>('enregistrer_preferences_vue', { preferences });

export const ouvrirBibliotheque = () => appeler<EtatBiblio>('ouvrir_bibliotheque');
/** État de la connexion à la base ; les changements arrivent par l'événement `base-etat`. */
export const etatBase = () => appeler<EtatBase>('etat_base');
/** Bouton « Réessayer » : sondage immédiat, renvoie l'état à jour. */
export const reessayerBase = () => appeler<EtatBase>('reessayer_base');
export const configurerPremierDemarrage = (choix: ChoixPremierDemarrage) =>
  appeler<EtatBiblio>('configurer_premier_demarrage', { choix });

/** Teste une connexion MySQL saisie ; rejette avec un message lisible (sans mot de passe). */
export const testerConnexionMysql = (cible: CibleTestMysql, parametres: ParametresMysql) =>
  appeler<ResultatTestMysql>('test_connexion_mysql', { cible, parametres });
export const lireConnexionsMysql = () => appeler<ConnexionsMysql>('lire_connexions_mysql');
/** Premier démarrage avec MySQL (base de ComicRust ou base ComicRack saisie) ; rien n'est enregistré si l'ouverture échoue. */
export const configurerMysql = (choix: ChoixMysql) => appeler<EtatBiblio>('configurer_mysql', { choix });

export const listerLivres = (requete: RequeteLivres) => appeler<ReponseLivres>('lister_livres', { requete });
export const listerPiles = (requete: RequeteLivres) => appeler<ReponsePiles>('lister_piles', { requete });
export const detailLivre = (id: string) => appeler<LivreDetail>('detail_livre', { id });

export const listerDossiers = () => appeler<string[]>('lister_dossiers');
/** Renvoie la liste à jour des dossiers surveillés. */
export const ajouterDossier = (chemin: string) => appeler<string[]>('ajouter_dossier', { chemin });
export const retirerDossier = (chemin: string) => appeler<string[]>('retirer_dossier', { chemin });
export const lancerScan = () => appeler<void>('lancer_scan');

export const ouvrirLivre = (id: string) => appeler<LivreOuvert>('ouvrir_livre', { id });
/** Ouvre un fichier (ou un dossier d'images, ou une image : son dossier) hors bibliothèque. */
export const ouvrirFichier = (chemin: string) => appeler<LivreOuvert>('ouvrir_fichier', { chemin });

/** Livre à ouvrir dans le lecteur : un livre de la bibliothèque ou un fichier. */
export type SourceLecteur = { id: string } | { chemin: string };
export const fermerLivre = (jeton: string) => appeler<void>('fermer_livre', { jeton });
/** `page` : index dans le fichier de la page affichée ; `termine` : la dernière page a été vue. */
export const enregistrerAvancement = (jeton: string, page: number, termine: boolean) =>
  appeler<void>('enregistrer_avancement', { jeton, page, termine });

// Infos et édition (étape 3).
export const lireReglagesEdition = () => appeler<ReglagesEdition>('lire_reglages_edition');
export const enregistrerReglagesEdition = (reglages: Partial<ReglagesEdition>) =>
  appeler<ReglagesEdition>('enregistrer_reglages_edition', { reglages });
export const lireFiches = (ids: string[]) => appeler<FicheEdition>('lire_fiches', { ids });
export const proposerValeurs = (champ: string, texte: string, tout = false, max?: number) =>
  appeler<Proposition[]>('proposer_valeurs', { champ, texte, tout, max });
export const verifierChamps = (champs: ChampModifie[]) => appeler<ErreurChamp[]>('verifier_champs', { champs });
export const appliquerModifications = (demande: DemandeModification) =>
  appeler<BilanOperation>('appliquer_modifications', { demande });
export const ajouterValeurPerso = (cle: string, existantes: string[]) =>
  appeler<ChampFiche>('ajouter_valeur_perso', { cle, existantes });
export const ecrireInfosFichiers = (ids: string[], confirme?: boolean) =>
  appeler<BilanOperation>('ecrire_infos_fichiers', { ids, confirme });
export const convertirCbrCbz = (ids: string[], rangerOriginal?: boolean, confirme?: boolean) =>
  appeler<BilanOperation>('convertir_cbr_cbz', { ids, rangerOriginal, confirme });
export const modelesDeNom = () => appeler<ModeleNom[]>('modeles_de_nom');
export const renommerDeplacer = (demande: DemandeRenommage) =>
  appeler<ResultatRenommage>('renommer_deplacer', { demande });
export const annulerOperation = () => appeler<void>('annuler_operation');

// Transfert de la bibliothèque ComicRack (étape 6, docs/tauri/ipc-transfert.md).
export const apercuTransfert = () => appeler<ApercuTransfert>('apercu_transfert');
export const lancerTransfert = (demande: DemandeTransfert) => appeler<ResumeTransfert>('lancer_transfert', { demande });
export const annulerTransfert = () => appeler<void>('annuler_transfert');
export const lirePages = (id: string, remplace?: string) => appeler<PagesLivre>('lire_pages', { id, remplace });
export const fermerPages = (jeton: string) => appeler<void>('fermer_pages', { jeton });
export const transformerPages = (pages: PageLivre[], operation: OperationPages) =>
  appeler<PageLivre[]>('transformer_pages', { pages, operation });

// Listes ComicRack (étape 5, docs/tauri/ipc-listes.md).
export const etatListes = () => appeler<EtatListes>('etat_listes');
export const arbreListes = () => appeler<ArbreListes>('arbre_listes');
export const bilanListes = () => appeler<string>('bilan_listes');
export const creerListe = (demande: DemandeNouvelleListe) => appeler<ResultatListes>('creer_liste', { demande });
export const renommerListe = (id: string, nom: string) => appeler<ResultatListes>('renommer_liste', { id, nom });
export const modifierListe = (id: string, champs: { nom?: string; description?: string; combinaison?: Combinaison }) =>
  appeler<ResultatListes>('modifier_liste', { id, ...champs });
export const dupliquerListe = (id: string) => appeler<ResultatListes>('dupliquer_liste', { id });
export const infosSuppression = (id: string) => appeler<InfosSuppression>('infos_suppression', { id });
export const supprimerListe = (id: string, listeAffichee?: string) => appeler<ResultatListes>('supprimer_liste', { id, listeAffichee });
export const destinationsListe = (id: string) => appeler<DestinationListe[]>('destinations_liste', { id });
export const deplacerListe = (id: string, parent: string | null, position?: number) =>
  appeler<ResultatListes>('deplacer_liste', { id, parent, position });
export const decalerListe = (id: string, haut: boolean) => appeler<ResultatListes>('decaler_liste', { id, haut });
export const placeDepotListe = (glissee: string, cible: string, place: PlaceListe) =>
  appeler<boolean>('place_depot_liste', { glissee, cible, place });
export const deposerListe = (glissee: string, cible: string, place: PlaceListe) =>
  appeler<ResultatListes>('deposer_liste', { glissee, cible, place });
export const infosConversion = (id: string) => appeler<InfosConversion>('infos_conversion', { id });
export const convertirEnLecture = (id: string) => appeler<ResultatListes>('convertir_en_lecture', { id });
export const ajouterLivresListe = (id: string, livres: string[], position?: number) =>
  appeler<ResultatListes>('ajouter_livres_liste', { id, livres, position });
export const depotLivresPossible = (id: string, listeAffichee?: string) =>
  appeler<string | null>('depot_livres_possible', { id, listeAffichee });
export const retirerLivresListe = (id: string, livres: string[]) => appeler<ResultatListes>('retirer_livres_liste', { id, livres });
export const decalerLivresListe = (id: string, livres: string[], haut: boolean) =>
  appeler<ResultatListes>('decaler_livres_liste', { id, livres, haut });
/** `insertion` : rang d'insertion dans la vue décrite par `requete` (0 : avant le premier, total : après le dernier). */
export const deplacerLivresListe = (id: string, livres: string[], requete: RequeteLivres, insertion: number) =>
  appeler<ResultatListes>('deplacer_livres_liste', { id, livres, requete, insertion });
/** `insertion` : frontière entre groupes (0 : avant le premier groupe). */
export const deplacerGroupesListe = (id: string, requete: RequeteLivres, groupes: string[], insertion: number) =>
  appeler<ResultatListes>('deplacer_groupes_liste', { id, requete, groupes, insertion });
export const decalerGroupeListe = (id: string, requete: RequeteLivres, groupe: string, haut: boolean) =>
  appeler<ResultatListes>('decaler_groupe_liste', { id, requete, groupe, haut });
export const groupesVue = (requete: RequeteLivres) => appeler<GroupeVue[]>('groupes_vue', { requete });
export const catalogueCriteres = () => appeler<CatalogueCriteres>('catalogue_criteres');
export const nouveauCritere = (typeCritere: string, ancien?: Critere) => appeler<Critere>('nouveau_critere', { typeCritere, ancien });
export const lireCriteres = (id?: string) => appeler<EditeurListe>('lire_criteres', { id });
export const apercuCriteres = (criteres: CriteresListe, id?: string) => appeler<Apercu>('apercu_criteres', { id, criteres });
export const enregistrerCriteres = (criteres: CriteresListe, place: { id?: string; parent?: string; position?: number; apresListe?: string }) =>
  appeler<ResultatListes>('enregistrer_criteres', { criteres, ...place });
export const analyserCbl = (chemin: string) => appeler<RapportCbl>('analyser_cbl', { chemin });
export const importerCbl = (chemin: string, place: { nom?: string; parent?: string; position?: number; apresListe?: string } = {}) =>
  appeler<{ resultat: ResultatListes; rapport: RapportCbl }>('importer_cbl', { chemin, ...place });
export const exporterCbl = (id: string, chemin: string) => appeler<{ chemin: string; livres: number }>('exporter_cbl', { id, chemin });

/** URL d'une vignette, ou de l'image à la largeur `largeur` (0 = taille d'origine) pour l'aperçu
 *  agrandi et la copie, à partir de `urlVignette`. En mode simulé, image SVG générée. */
export function urlImagePage(livre: PagesLivre, p: PageLivre, largeur?: number): string {
  if (!dansTauri() && livre.urlBase.startsWith('simule:')) {
    const faux = { jeton: livre.jeton, titre: livre.id, pages: [{ ...p, index: p.image, rotation: 0 }] } as unknown as LivreOuvert;
    return imageSimulee(faux, p.image);
  }
  if (largeur === undefined) return p.urlVignette;
  const base = p.urlVignette.replace(/\?.*$/, '');
  return largeur > 0 ? `${base}?l=${largeur}` : base;
}

/** Montre le fichier du livre dans son dossier (Explorateur de Windows, fichier sélectionné). */
export const ouvrirDossierFichier = (id: string) => appeler<void>('ouvrir_dossier_fichier', { id });

// Mise à jour (docs/tauri/ipc-maj.md) : résultats et progression par `maj://statut` et
// `maj://progression` (voir `ecouterMaj`).
export const majEtat = () => appeler<EtatMaj>('maj_etat');
/** Lance la vérification en arrière-plan ; le résultat arrive par `maj://statut`. */
export const majVerifier = (manuel: boolean) => appeler<void>('maj_verifier', { manuel });
/** Télécharge, vérifie (SHA-256) et installe la release `tag` ; progression et résultat par événements. */
export const majInstaller = (tag: string) => appeler<void>('maj_installer', { tag });
/** Relance ComicRust (même exécutable, mêmes arguments) puis ferme l'application. */
export const majRedemarrer = () => appeler<void>('maj_redemarrer');
/** Change les réglages fournis (`versionIgnoree: null` : plus aucune version ignorée). */
export const majRegler = (reglages: { verifierAuDemarrage?: boolean; versionIgnoree?: string | null }) =>
  appeler<EtatMaj>('maj_regler', reglages);
/** Ouvre une page de la release dans le navigateur (le backend n'accepte que
 *  `https://github.com/imtheyoyo/ComicRust/…`). Hors Tauri : nouvel onglet. */
export async function majOuvrirPage(url: string): Promise<void> {
  if (!dansTauri()) {
    window.open(url, '_blank', 'noopener,noreferrer');
    return;
  }
  await appeler<void>('maj_ouvrir_page', { url });
}
/** Abonnement aux deux événements de la mise à jour ; renvoie la fonction de désabonnement. */
export async function ecouterMaj(
  surStatut: (s: StatutMaj) => void,
  surProgression: (p: ProgressionMaj) => void,
): Promise<UnlistenFn> {
  const [a, b] = await Promise.all([
    ecouter<StatutMaj>('maj://statut', surStatut),
    ecouter<ProgressionMaj>('maj://progression', surProgression),
  ]);
  return () => { a(); b(); };
}

/** URL d'une page. `largeurMax` : largeur souhaitée en pixels physiques (0 = taille d'origine ;
 *  le backend ne grossit jamais l'image). En mode simulé, image SVG générée. */
export function urlPage(livre: LivreOuvert, index: number, largeurMax: number): string {
  if (!dansTauri() && livre.urlBase.startsWith('simule:')) return imageSimulee(livre, index);
  return `${livre.urlBase}/${index}?l=${largeurMax}`;
}

const imagesSimulees = new Map<string, string>();
/** Page factice du mode simulé (SVG aux dimensions de la page, numéro bien visible). */
function imageSimulee(livre: LivreOuvert, index: number): string {
  const clef = livre.jeton + '/' + index;
  let u = imagesSimulees.get(clef);
  if (u) return u;
  const p = livre.pages.find((x) => x.index === index);
  const double = p?.double ?? false;
  const l = p?.largeur ?? (double ? 2000 : 1000);
  const h = p?.hauteur ?? 1500;
  let teinte = 0;
  for (const c of livre.titre) teinte = (teinte * 31 + c.charCodeAt(0)) % 360;
  teinte = (teinte + index * 23) % 360;
  const cases = [];
  const marge = 40;
  const lignes = 3;
  const hc = (h - marge * (lignes + 1)) / lignes;
  for (let r = 0; r < lignes; r++) {
    const y = marge + r * (hc + marge);
    const nb = 1 + ((index + r) % 2);
    const lc = (l - marge * (nb + 1)) / nb;
    for (let k = 0; k < nb; k++) {
      cases.push(`<rect x="${marge + k * (lc + marge)}" y="${y}" width="${lc}" height="${hc}" fill="hsl(${(teinte + 40 * (r + k)) % 360} 45% 72%)" stroke="#111" stroke-width="6"/>`);
    }
  }
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${l}" height="${h}" viewBox="0 0 ${l} ${h}">` +
    `<rect width="${l}" height="${h}" fill="#fbf8f0"/>${cases.join('')}` +
    `<circle cx="${l / 2}" cy="${h / 2}" r="${Math.min(l, h) / 6}" fill="hsl(${teinte} 70% 45%)" opacity=".85"/>` +
    `<text x="${l / 2}" y="${h / 2 + 50}" text-anchor="middle" font-family="sans-serif" font-size="150" font-weight="700" fill="#fff">${index + 1}</text>` +
    `<text x="${l - 30}" y="${h - 14}" text-anchor="end" font-family="sans-serif" font-size="28" fill="#333">${double ? 'double page · ' : ''}page ${index + 1}</text>` +
    `<text x="30" y="${h - 14}" font-family="sans-serif" font-size="28" fill="#333">◀ début</text></svg>`;
  u = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  imagesSimulees.set(clef, u);
  return u;
}

/** Plein écran de la fenêtre : API de fenêtre Tauri (permission `core:window:allow-set-fullscreen`),
 *  sinon plein écran du document (navigateur, ou permission absente). */
export async function pleinEcran(actif: boolean): Promise<void> {
  if (dansTauri()) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().setFullscreen(actif);
      return;
    } catch (e) {
      console.warn('plein écran Tauri indisponible, repli sur le document', e);
    }
  }
  if (actif && !document.fullscreenElement) await document.documentElement.requestFullscreen?.();
  else if (!actif && document.fullscreenElement) await document.exitFullscreen?.();
}

/** Message lisible pour une erreur d'IPC : le backend rejette avec un texte français (qui peut
 *  contenir un chemin) ; on tolère aussi Error, objet `{message}` ou valeur absente. */
export function messageErreur(e: unknown): string {
  if (typeof e === 'string' && e.trim()) return e;
  if (e instanceof Error && e.message) return e.message;
  if (e && typeof e === 'object' && 'message' in e && typeof (e as { message: unknown }).message === 'string') {
    return (e as { message: string }).message || 'Erreur inconnue.';
  }
  return 'Erreur inconnue.';
}

// ───────────────────────────── Scripts Python (tauri-v0.9.0) ─────────────────────────────
// Contrat : docs/tauri/ipc-scripts.md. Tout texte venant d'un script (nom, journal, libellés,
// valeurs) est affiché comme du texte (jamais d'innerHTML) ; les images ne sont acceptées que
// sous forme d'URL `data:image/(png|jpeg|gif|webp|bmp);base64,…` (voir `imageScriptSure`).

export type HookScript = 'Books' | 'Library' | 'Editor' | 'ConfigScript' | 'BookOpened' | 'BookClosed' | 'NewBooks' | 'Startup';

export interface InfoScript {
  /** Identifiant du point d'entrée (`#@Key`, sinon `chemin/relatif.py:fonction`) : opaque pour le
   *  front, à rendre tel quel au backend. Un fichier peut déclarer plusieurs points d'entrée. */
  id: string;
  nom: string;
  description?: string;
  auteur?: string;
  hooks: HookScript[];
  /** Fichier, chemin relatif au dossier scripts\ (séparateurs /). */
  fichier: string;
  actif: boolean;
  /** Icône du script (#@Image) en URL data: (≤ 512 Ko). */
  image?: string;
  /** Id du point d'entrée ConfigScript du même fichier (bouton « Configurer… »). */
  reglages?: string;
}

export interface EtatScripts {
  python: { chemin?: string; erreur?: string };
  scripts: InfoScript[];
  avertissements: string[];
  /** Automatismes (BookOpened, NewBooks…) : désactivés par défaut. */
  auto: boolean;
  ecritureAutorisee: boolean;
  motifLectureSeule?: string;
  /** Dossier des scripts (à côté de l'exe). */
  dossier: string;
  /** runId du script interactif en cours (lancé ou en application), s'il y en a un. */
  executionEnCours?: string;
}

export type GenreModif = 'fiche' | 'renommage' | 'comicinfo';

export interface LigneApercu {
  cle: string;
  livreId: string;
  titre: string;
  /** Nom ComicRack du champ (ou « Fichier », « ComicInfo.xml »). */
  champ: string;
  /** Libellé français du champ, à afficher. */
  libelle: string;
  avant: string;
  apres: string;
  genre: GenreModif;
  conflit?: string;
}

export interface ApercuScript {
  lignes: LigneApercu[];
  conflits: string[];
  /** Valeurs demandées déjà identiques à la fiche (ignorées). */
  inchanges: number;
  lectureSeule: boolean;
  motif?: string;
}

export interface BilanScript {
  fiches: number;
  renommages: number;
  comicinfo: number;
  erreurs: string[];
  arrete: boolean;
  /** Résumé en français, rédigé par le backend. */
  resume: string;
}

export type GenreJournal = 'info' | 'sortie' | 'erreur' | 'systeme';
export interface EvJournalScript { runId: string; genre: GenreJournal; texte: string }
export interface EvProgressionScript { runId: string; fraction?: number; texte?: string; detail?: string; image?: string }
/** Nature d'une question `ask` : `comicrust.ask` (texte), `ask_choice` (un des `choix`), `confirm` (oui/non). */
export type TypeQuestionScript = 'texte' | 'choix' | 'confirmation';
export interface DetailQuestionScript {
  type: TypeQuestionScript;
  /** Valeur par défaut (jamais pour une saisie secrète). */
  defaut: unknown;
  choix: string[];
  /** Saisie masquée (clé d'API…) : une réponse vide garde la valeur actuelle côté script. */
  secret: boolean;
}
/** `titre` : nom du script. `question` : présent pour `genre: "ask"`. */
export interface EvQuestionScript { runId: string; reqId: number; genre: 'ask' | 'message'; titre?: string; texte: string; question?: DetailQuestionScript }
export interface EvFormulaireScript { runId: string; reqId: number; spec: FormSpecScript }
export interface EvChoixScript { runId: string; reqId: number; spec: ChooseSpecScript }
export interface EvVignetteScript { runId: string; reqId: number; rang: number; image: string }
export type EtatFinScript = 'termine' | 'annule' | 'erreur';
export interface EvFinScript { runId: string; etat: EtatFinScript; erreur?: string; nbModifs: number; champs?: Record<string, unknown> }
export interface EvAppliqueScript { runId: string; bilan: BilanScript }
export interface EvAutoScript { script: string; hook: string; ligne: string }

/** Choix d'une liste : `"texte"`, `[valeur, "libellé"]` ou `{value, label}` (forms.rs, parse_choices). */
export type ChoixBrut = string | number | boolean | [unknown, unknown] | { value?: unknown; label?: unknown };

/** Contrôle d'un formulaire de script, tel que décrit par le script et vérifié par
 *  comicrust_core::scripting::forms (noms de champs du JSON des scripts). Les champs dépendent
 *  de `type` : text, textarea, secret, checkbox, number, choice, radio, multichoice, list_edit,
 *  table, folder, file, label, separator, image, group, columns, tabs, button. */
export interface ControleScript {
  /** Numéro unique ajouté par le backend. */
  id?: number;
  type: string;
  key?: string;
  label?: string;
  help?: string;
  enabled_if?: string;
  width?: number;
  default?: unknown;
  placeholder?: string;
  required?: boolean;
  rows?: number;
  secret?: boolean;
  inserts?: Array<ChoixBrut | { group?: string; items: ChoixBrut[] }>;
  paste?: boolean;
  hints?: Array<[string, string]>;
  hint_none?: string;
  min?: number;
  max?: number;
  step?: number;
  decimals?: number;
  choices?: ChoixBrut[];
  editable?: boolean;
  horizontal?: boolean;
  height?: number;
  unique?: boolean;
  /** Tableau : colonnes ; conteneur « columns » : listes de contrôles. */
  columns?: unknown[];
  widths?: Array<number | null>;
  filters?: Array<[string, string | string[]]>;
  save?: boolean;
  text?: string;
  style?: string;
  /** Image : URL data:/cr-… (ou base64 brut, converti en data:). */
  data?: string;
  caption?: string;
  max_width?: number;
  max_height?: number;
  title?: string;
  controls?: ControleScript[];
  tabs?: Array<{ title?: string; controls?: ControleScript[] }>;
  validate?: boolean;
}

export interface ColonneTableScript { title?: string; type?: string; choices?: ChoixBrut[]; width?: number }

export interface FormSpecScript {
  title?: string;
  message?: string;
  controls?: ControleScript[];
  buttons?: string[];
  /** Libellé du bouton d'annulation (null : aucun ; absent : le dernier s'il y en a plusieurs). */
  cancel?: string | null;
  width?: number;
  label_width?: number;
  values?: Record<string, unknown>;
}

export interface ChooseSpecScript {
  title?: string;
  message?: string;
  columns?: string[];
  rows?: Array<unknown[] | string>;
  /** Vignettes données avec la liste (URL data:/cr-…, base64 brut toléré, null = sans). */
  images?: Array<string | null>;
  /** Rangs dont la vignette arrive après coup (dès que possible / à la demande). */
  lazy?: number[];
  on_demand?: number[];
  multi?: boolean;
  default?: number | number[];
  filter?: boolean;
  timeout?: number;
  thumb_size?: number;
  layout?: 'table' | 'cards' | '';
}

/** Clé réservée ajoutée à l'objet des valeurs d'un formulaire : bouton cliqué (libellé d'un
 *  bouton du bas ou clé d'un bouton du formulaire). Le cœur retire les clés inconnues. */
export const CLE_BOUTON_FORMULAIRE = '$bouton';

/** Image venant d'un script affichable sans risque : seulement une URL
 *  `data:image/(png|jpeg|gif|webp|bmp);base64,…` (forme produite par le backend, qui reconnaît le
 *  format d'après les octets). Tout le reste est refusé : SVG (peut porter du script), autres
 *  protocoles, base64 brut. */
export function imageScriptSure(v: unknown): string | undefined {
  if (typeof v !== 'string') return undefined;
  return /^data:image\/(png|jpeg|gif|webp|bmp);base64,[A-Za-z0-9+/]+={0,2}$/.test(v) ? v : undefined;
}

export const scriptsLister = () => appeler<EtatScripts>('scripts_lister');
export const scriptsRecharger = () => appeler<EtatScripts>('scripts_recharger');
export const scriptsActiver = (id: string, actif: boolean) => appeler<EtatScripts>('scripts_activer', { id, actif });
export const scriptsAuto = (actif: boolean) => appeler<EtatScripts>('scripts_auto', { actif });
/** Lance un script ; rend l'identifiant de l'exécution. `Books` : sélection (non vide) ;
 *  `Library` : `ids` vide = toute la bibliothèque ; `ConfigScript` : `[]` (id du point d'entrée
 *  ConfigScript, voir `InfoScript.reglages`). Refusé si un script interactif tourne. */
export const scriptLancer = (id: string, hook: HookScript, ids: string[]) => appeler<string>('script_lancer', { id, hook, ids });
/** Réponse : question texte → chaîne, question choix → la chaîne choisie, confirmation →
 *  booléen (null → valeur par défaut) ; message → null ; formulaire → objet des valeurs avec
 *  `$bouton` (null = annulé) ; liste de choix → rang (liste de rangs si multiple) ou null. */
export const scriptRepondre = (runId: string, reqId: number, valeur: unknown) =>
  appeler<void>('script_repondre', { runId, reqId, valeur: valeur === undefined ? null : valeur });
export const scriptVignettesVoulues = (runId: string, reqId: number, rangs: number[]) =>
  appeler<void>('script_vignettes_voulues', { runId, reqId, rangs });
export const scriptArreter = (runId: string) => appeler<void>('script_arreter', { runId });
export const scriptApercu = (runId: string) => appeler<ApercuScript>('script_apercu', { runId });
export const scriptAppliquer = (runId: string, cles: string[]) => appeler<BilanScript>('script_appliquer', { runId, cles });
export const scriptHookEditeur = (id: string, livreId: string, champs: Record<string, string>) =>
  appeler<string>('script_hook_editeur', { id, livreId, champs });
export const scriptReglagesLire = (id: string) => appeler<Record<string, unknown>>('script_reglages_lire', { id });
/** Fusionne les réglages ; rend les réglages enregistrés (secrets masqués). */
export const scriptReglagesEcrire = (id: string, valeurs: Record<string, unknown>) =>
  appeler<Record<string, unknown>>('script_reglages_ecrire', { id, valeurs });
/** Clé ajoutée par le backend à `script:fin.champs` (hook Editor) : le script a demandé
 *  `write_info` (case « Écrire aussi les infos dans le fichier » de la fenêtre Infos). */
export const CLE_ECRIRE_DANS_FICHIER = '$ecrireDansFichier';
/** Valeur d'un secret défini, renvoyée masquée par le backend. */
export const SECRET_MASQUE = '••••';
/** Clé réservée des réglages lus : liste des clés secrètes (champ toujours masqué, même quand le
 *  secret n'est pas défini ou illisible). Jamais renvoyée au backend (qui l'ignorerait). */
export const CLE_SECRETS = '$secrets';
