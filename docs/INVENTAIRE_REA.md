# Inventaire REA — ComicRack.exe

Analyse statique (REA, sans exécution) de `\\SERVEURNAS\Disque_DATA_1\_BDZ\ComicRack\ComicRack.exe`
et de sa bibliothèque métier `ComicRack.Engine.dll` — 20/10/2026.

## 1. Identification

| | ComicRack.exe | ComicRack.Engine.dll |
|---|---|---|
| SHA-256 | `6792394205e2787bce084b5e50e35cf8de8f941aa800878e5860982d5a0b94ab` | `558234b38906fa987737b4cffce13ff2ef178ae4a12e40aafe1de665afa97a91` |
| Taille | 2 813 952 o | 1 200 128 o |
| Format | PE32, CLI **IL-only**, AnyCPU | idem |
| Runtime | .NET Framework 4.8 (métadonnées v4.0.30319) | idem |
| Version | 1.0.9733.32469 | 1.0.9733.32467 |
| Signature | aucune (ni Authenticode ni strong name) | aucune |
| Taille du code | 431 types, 4 783 méthodes, 935 propriétés | 1 043 types, 6 504 méthodes |
| Point d'entrée | jeton `0x060002c0` (`Program.Main`) | — |

**C'est ComicRack Community Edition**, pas le ComicRack 0.9.178 d'origine : la chaîne
`https://github.com/maforget/ComicRackCE/commits/master.atom` (flux « ComicRack News ») et la
ressource `cYo.Projects.ComicRack.Viewer.CurrentCommit.txt` le prouvent. Le code source
décompilé de cette édition est public sur GitHub sous licence **GPL-2.0**.

### Pourquoi Ghidra n'apporte rien ici
Les deux binaires sont du **bytecode CIL pur** (flag `il-only`, aucun en-tête natif,
pas de ReadyToRun). Ghidra ne verrait qu'un stub x86 de 5 octets qui saute dans
`mscoree.dll`. Tout le comportement est dans les métadonnées .NET, que REA lit directement
(types, méthodes, corps IL, chaînes `#US`). Le seul code natif se trouve dans des DLL tierces
publiques (libwebp, libjxl, libheif, PDFium) qu'il n'y a pas lieu de rétro-ingénierer.
Pour lire le C# reconstitué, ILSpy/dnSpy — ou directement le dépôt ComicRackCE.

## 2. Imports

### Assemblys référencés par ComicRack.exe (14)
`mscorlib`, `System`, `System.Core`, `System.Xml`, `System.Drawing`, `System.Windows.Forms`
(UI WinForms), `cYo.Common`, `cYo.Common.Windows`, `cYo.Common.Presentation`,
`ComicRack.Engine`, `ComicRack.Engine.Display.Forms`, `ComicRack.Plugins`,
`Microsoft.WindowsAPICodePack.Shell` 1.1.5 (dialogues/miniatures Explorer),
**`IronPython` 2.7.0.40** (moteur de scripts/plugins `.py`).

### Assemblys référencés par ComicRack.Engine.dll (22) — les briques métier
| Bibliothèque | Rôle |
|---|---|
| ICSharpCode.SharpZipLib 1.4.2 | lecture/écriture ZIP → **CBZ** |
| SharpCompress 0.50.4 | **CBR (RAR)**, **CB7 (7z)**, TAR |
| PDFiumSharp 1.1.4 | rendu **PDF** |
| sharpPDF | export PDF |
| CSJ2K 3.0 | JPEG 2000 |
| LibHeifSharp 3.2 | HEIF/AVIF |
| MySqlConnector 2.0 | **base de données MySQL** (ton setup actuel) |
| System.Data | accès SQL générique |
| System.ServiceModel + IdentityModel | **WCF** : serveur de partage de bibliothèque / client Android |
| System.Runtime.Serialization, DataAnnotations | sérialisation XML/DataContract |
| Microsoft.Bcl.Numerics | calculs (imagerie) |

### P/Invoke (appels natifs)
- **exe** → `user32.dll` : `SetProcessDPIAware`, `SendMessage` ×2 (c'est tout).
- **Engine** → 38 imports :
  - `libheif` : `heif_check_filetype`
  - `jxl.dll` : `JxlDecoderCreate/SetInput/ProcessInput/GetBasicInfo/SetImageOutBuffer…`,
    `JxlEncoderCreate/AddImageFrame/AddJPEGFrame/StoreJPEGMetadata…` (décodage **et**
    conversion JPEG→JXL sans perte)
  - `Resources\x86|x64\libwebp.dll` : `WebPGetInfo`, `WebPDecodeBGRAInto`, `WebPEncode(Lossless)BGR(A)`
    (variantes 32/64 bits choisies à l'exécution)

## 3. Ressources (52)
Formulaires WinForms embarqués : `MainForm`, `ReaderForm`, `ScriptOutputForm`, vues
`ComicBrowserControl/Form/View`, `ComicExplorerView`, `ComicListLibraryBrowser`,
`ComicListFolderFilesBrowser`, `ComicPagesView`, `QuickOpenView`, `RemoteConnectionView`,
`SmallComicPreview` ; ~30 dialogues (`ComicBookDialog`, `SmartListDialog`,
`SmartListQueryDialog`, `MatcherEditor`, `ExportComicsDialog`, `DevicesEditDialog`,
`OpenRemoteDialog`, `PreferencesDialog`, `TasksDialog`…). `Properties.Resources` pèse 1 Mo
(icônes, images). `CurrentCommit.txt` (41 o) = hash git du build.

## 4. Chaînes notables

**Séquence de démarrage (`Program`)** — elle décrit littéralement l'architecture :
`Splash Thread` → `Loading Database` / `OpenDatabase` → `LoadCustomSettings` →
`Resources\Icons\Publishers*.zip`, `AgeRatings*.zip`, `Formats*.zip`, `Special*.zip`
(packs d'icônes) → `DefaultLists.txt` → `InitCache` « Initialize Disk Caches » →
`ReadNewsFeed` → `CreateMainWindow` → `Starting Network` → `Remote Server Started/Stopped`.
Arrêt : `SaveAllData` « Saving all Data to Disk… ».

**Configuration / environnement** : `Config.xml`, `NewsFeeds.xml`, `UseLocalSettings`
(mode portable), variables `APPEXE`, `APPPATH`, `APPDATA`, `USERPATH`,
`ComicRackSingleInstance` (mutex instance unique), `-restart -waitpid`,
`Resources\Textures\Backgrounds|Papers`, `Help\ComicRack Introduction.djvu`, format `eComic`.

**Listes de lecture** : `.cbl`, « ComicRack Reading List », `FormatReadingList`.

**Modèle `ComicBook` (Engine)** : `Id`, `FilePath`, `FileSize`, `FileIsMissing`,
`FileModifiedTime`, `FileCreationTime`, `AddedTime`, `ReleasedTime`, `OpenedTime`,
`OpenedCount`, `CurrentPage`, `LastPageRead`, `Rating`, `ColorAdjustment`, `Checked`,
`SeriesComplete`, `EnableProposed`, `CustomThumbnailKey`, `CustomValuesStore`,
`BookPrice/Age/Condition/Store/Owner/CollectionStatus/Notes/Location`, `ISBN`,
`ComicInfoIsDirty`, `ComicBookIsDirty`.

**Modèle `ComicInfo`** (= schéma de `ComicInfo.xml`) : `Title, Series, Number, Count, Volume,
AlternateSeries/Number/Count, StoryArc, SeriesGroup, Summary, Notes, Review, Year, Month, Day,
Writer, Penciller, Inker, Colorist, Letterer, CoverArtist, Editor, Translator, Publisher,
Imprint, Genre, Tags, Web, PageCount, LanguageISO, Format, AgeRating, BlackAndWhite, Manga,
Characters, Teams, MainCharacterOrTeam, Locations, ScanInformation, CommunityRating, Pages`.
Multi-valeurs séparées par `", "`.

**Règles métier visibles dans les chaînes** :
- normalisation de série : regex `\bv(ol(ume)?)?\.?\s?\d+\b\s*` puis `[^a-z0-9]|\bthe\b|\band\b`
  (sert à rapprocher « The Avengers Vol. 3 » et « Avengers ») ;
- gabarit d'affichage `[{alternateseries}][ #{alternatenumber}]`, jetons `{(?<name>[a-z]+)}` ;
- tolérance de la faute `Advertisment` à côté de `Advertisement` (type de page) ;
- `._` : fichiers ressources macOS ignorés dans les archives ;
- regex de plage d'années `\((?<startYear>\d{4})(?:_(?<startMonth>\d{2}))?-(?<endYear>\d{4})…\)`.

## 5. Fonctions principales (classe `Program`, 72 méthodes)

| Méthode | Rôle |
|---|---|
| `Main` (0x060002c0) | init, instance unique, splash, boucle WinForms |
| `get_ExtendedSettings`, `get_UseLocalSettings`, `get_Settings` | ligne de commande + `Config.xml`, mode portable |
| `get_Database`, `get_BookFactory` | bibliothèque (XML/SQLite/MySQL) et fabrique de livres |
| `get_CacheManager`, `get_ImagePool`, `get_InternetCache` | caches disque de pages/miniatures |
| `get_QueueManager`, `get_Scanner` | file de tâches de fond, scan des dossiers surveillés |
| `get_NetworkManager` | serveur/partage WCF |
| `get_BackupManager` | sauvegardes de la base (nouveauté CE) |
| `GetAvailable/UsedComicBookMatchers`, `CreateComicBookMatchersMenu` | moteur de **listes intelligentes** |
| `ShowComicOpenDialog` | ouverture (filtres de formats dont `.cbl`) |
| `MakeBooksImage` | génération d'images de couvertures groupées |
| `LoadDefaultBackground/PaperTextures` | textures du lecteur |
| `StartupProgress`, `RefreshAllWindows`, `ForAllForms`, `AskQuestion` | plomberie UI |

Limite rencontrée : l'inspection exhaustive des membres de l'exe dépasse le budget de
représentation de REA (64 Mo) ; l'analyse a donc été faite par types ciblés
(`Program`, `ComicInfo`, `ComicBook`, `ComicPageInfo`).

## 6. Comment ComicRack fonctionne

```
ComicRack.exe (WinForms)        ── UI : navigateur, lecteur, dialogues, IronPython
  └─ ComicRack.Plugins          ── API exposée aux scripts .py (ComicRack.App, ComicBook…)
  └─ ComicRack.Engine.Display.Forms ── rendu des pages (zoom, double page, textures)
  └─ ComicRack.Engine           ── cœur
       ├─ ComicBook / ComicInfo / ComicPageInfo   modèle
       ├─ Providers               CBZ · CBR · CB7 · PDF · DjVu · dossier · eComic
       │                          + décodeurs WebP/JXL/HEIF/JP2
       ├─ ComicLibrary            ComicDB.xml (défaut) | SQLite | MySQL
       ├─ ComicListItems          dossiers, listes, listes intelligentes (matchers), CBL
       ├─ Cache                   pages/miniatures sur disque
       ├─ Queue / Scanner         scan des dossiers, mise à jour des fichiers
       └─ Network (WCF)           partage de bibliothèque, client Android
  └─ cYo.Common*                ── utilitaires (XML, threads, imagerie, Win32)
```

Cycle de vie d'un livre : le **scanner** trouve un fichier → la **fabrique** choisit le
**fournisseur** selon l'extension/signature → lecture de `ComicInfo.xml` dans l'archive →
création d'un `ComicBook` (GUID) dans la **bibliothèque** → miniature mise en **cache** →
les **listes intelligentes** (matchers sur les champs) se réévaluent → à la lecture, le
fournisseur sert les pages triées naturellement, la position (`CurrentPage`,
`LastPageRead`) est enregistrée, et les modifications de métadonnées peuvent être
réécrites dans l'archive (`ComicInfoIsDirty`).
