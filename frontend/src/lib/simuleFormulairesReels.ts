// MODE SIMULÉ (tauri-v0.9.1) : formulaires RÉELS des scripts d'exemple, tels que le backend les
// envoie au front (FormSpec du cœur passé par `scripts/formulaires.rs`, `formulaire_json`).
// Généré par le test `tests_formulaires` de comicrust-tauri (`COMICRUST_DUMP_FORMULAIRES=<dossier>`),
// seul le dossier des journaux (chemin temporaire du test) est remplacé.
// Non-régression de l'anomalie de la tauri-v0.9.0 : dans « Configurer BD2 », onglet « Données », la
// liste à cocher des champs (dans une colonne, avec des libellés de 360 px) n'était pas visible.
import type { FormSpecScript } from './api';

/** « Configurer BD2 » (bedetheque_scraper2.py, BDConfigForm) : trois onglets (Préférences, Données,
 *  Logging), bouton d'import, valeurs par défaut du script. */
export const SPEC_CONFIGURER_BD2: FormSpecScript = {
 "buttons": [
  "OK",
  "Annuler"
 ],
 "cancel": "Annuler",
 "controls": [
  {
   "help": "",
   "id": 0,
   "key": "",
   "label": "",
   "tabs": [
    {
     "controls": [
      {
       "controls": [
        {
         "choices": [
          {
           "label": "Français",
           "value": "FR"
          },
          {
           "label": "English",
           "value": "EN"
          }
         ],
         "default": "FR",
         "help": "Langue des fenêtres du script et du libellé « Info édition » ajouté au résumé.",
         "horizontal": true,
         "id": 2,
         "key": "LANGENFR",
         "label": "Langue du plugin",
         "type": "radio"
        },
        {
         "default": false,
         "help": "",
         "id": 3,
         "key": "FORMATARTICLES",
         "label": "Déplacer les articles principaux du nom des séries à la fin",
         "type": "checkbox"
        },
        {
         "default": "",
         "enabled_if": "FORMATARTICLES",
         "help": "",
         "hint_none": "",
         "hints": [],
         "id": 4,
         "inserts": [],
         "key": "ARTICLES",
         "label": "Articles principaux",
         "paste": false,
         "placeholder": "",
         "required": false,
         "rows": 1,
         "secret": false,
         "type": "text"
        },
        {
         "default": false,
         "help": "",
         "id": 5,
         "key": "TITLEIT",
         "label": "Convention Pascal Case pour les noms de séries et d'albums",
         "type": "checkbox"
        }
       ],
       "help": "",
       "id": 1,
       "key": "",
       "label": "",
       "title": "Langue et noms",
       "type": "group"
      },
      {
       "controls": [
        {
         "choices": [
          {
           "label": "Ignorer et passer à la suite",
           "value": "0"
          },
          {
           "label": "Demander à l'utilisateur de choisir",
           "value": "1"
          }
         ],
         "default": "0",
         "help": "",
         "horizontal": false,
         "id": 7,
         "key": "ChoixUtilisateur",
         "label": "",
         "type": "radio"
        },
        {
         "default": false,
         "help": "",
         "id": 8,
         "key": "AlwaysChooseSerie",
         "label": "Toujours choisir les séries manuellement",
         "type": "checkbox"
        },
        {
         "default": false,
         "help": "",
         "id": 9,
         "key": "ChoixAutoEdition",
         "label": "Choix automatique de l'édition des albums",
         "type": "checkbox"
        }
       ],
       "help": "",
       "id": 6,
       "key": "",
       "label": "",
       "title": "Décision lorsqu'une ambiguité de choix de série ou d'album se présente",
       "type": "group"
      },
      {
       "controls": [
        {
         "decimals": 0,
         "default": 7.0,
         "help": "",
         "id": 11,
         "key": "TIMEOUTS",
         "label": "Temps de pause entre 2 albums (en s)",
         "max": 999.0,
         "min": 1.0,
         "step": 1.0,
         "type": "number"
        },
        {
         "decimals": 0,
         "default": 1000.0,
         "help": "",
         "id": 12,
         "key": "TIMEOUT",
         "label": "Nombre maximal d'albums à traiter",
         "max": 1000000.0,
         "min": 1.0,
         "step": 1.0,
         "type": "number"
        },
        {
         "default": false,
         "help": "",
         "id": 13,
         "key": "CBRescrape",
         "label": "Exécuter un rescrape à partir des liens internet des albums",
         "type": "checkbox"
        },
        {
         "default": false,
         "help": "Module « cloudscraper » (et ses dépendances) à copier dans scripts\\lib.",
         "id": 14,
         "key": "use_cloudscraper",
         "label": "Utiliser cloudscraper (plus lent mais plus fiable)",
         "type": "checkbox"
        }
       ],
       "help": "",
       "id": 10,
       "key": "",
       "label": "",
       "title": "Traitement",
       "type": "group"
      },
      {
       "controls": [
        {
         "decimals": 0,
         "default": 150.0,
         "help": "Aussi réglable dans la fenêtre de choix (curseur « Taille »).",
         "id": 16,
         "key": "TailleVignettes",
         "label": "Hauteur des couvertures (pixels)",
         "max": 320.0,
         "min": 48.0,
         "step": 4.0,
         "type": "number"
        },
        {
         "default": false,
         "help": "Les résultats de recherche du site n'ont pas de couverture : la fenêtre s'ouvre tout de suite et la page des premières séries est lue pendant ce temps (une page à la fois, une seconde au moins entre deux pages ; les autres séries au survol). Couvertures gardées dans data\\bedetheque\\cache : une série déjà vue n'est pas relue.",
         "id": 17,
         "key": "VignettesSeries",
         "label": "Couvertures des séries : lire la page des premières séries proposées",
         "type": "checkbox"
        },
        {
         "decimals": 0,
         "default": 6.0,
         "enabled_if": "VignettesSeries",
         "help": "",
         "id": 18,
         "key": "NbVignettesSeries",
         "label": "Nombre de séries",
         "max": 12.0,
         "min": 1.0,
         "step": 1.0,
         "type": "number"
        }
       ],
       "help": "",
       "id": 15,
       "key": "",
       "label": "",
       "title": "Fenêtres de choix (ComicRust)",
       "type": "group"
      }
     ],
     "title": "Préférences"
    },
    {
     "controls": [
      {
       "columns": [
        [
         {
          "choices": [
           {
            "label": "Nom de la série",
            "value": "Series"
           },
           {
            "label": "Titre de l'album",
            "value": "Title"
           },
           {
            "label": "Scénariste(s) de l'album",
            "value": "Writer"
           },
           {
            "label": "Dessinateur(s) de l'album",
            "value": "Penciller"
           },
           {
            "label": "Coloriste(s) de l'album",
            "value": "Colorist"
           },
           {
            "label": "Lettreur(s) de l'album",
            "value": "Letterer"
           },
           {
            "label": "Encreur(s) de l'album",
            "value": "Inker"
           },
           {
            "label": "Artiste(s) de couverture de l'album",
            "value": "CoverArtist"
           },
           {
            "label": "Éditeur de la série",
            "value": "Publisher"
           },
           {
            "label": "Langue de la série",
            "value": "Language"
           },
           {
            "label": "Format de la série",
            "value": "Format"
           },
           {
            "label": "Évaluation de l'album",
            "value": "Rating"
           },
           {
            "label": "Date de publication de l'album",
            "value": "Printed"
           },
           {
            "label": "Marque d'éditeur de l'album (Collection)",
            "value": "Imprint"
           },
           {
            "label": "Résumé de la série et de l'album",
            "value": "Synopsys"
           },
           {
            "label": "Nombre total d'albums dans la série",
            "value": "Count"
           },
           {
            "label": "Annotations de l'album",
            "value": "Notes"
           },
           {
            "label": "Série complète",
            "value": "SerieStatus"
           },
           {
            "label": "Lien internet de l'album",
            "value": "Web"
           },
           {
            "label": "Genre de la série",
            "value": "Genre"
           },
           {
            "label": "Image de couverture de l'album",
            "value": "Cover"
           },
           {
            "label": "Numéro ISBN/EAN de l'album",
            "value": "ISBN"
           }
          ],
          "default": [],
          "height": 330.0,
          "help": "",
          "id": 20,
          "key": "Donnees",
          "label": "Champs à récupérer",
          "required": false,
          "type": "multichoice"
         }
        ],
        [
         {
          "help": "",
          "id": 21,
          "key": "",
          "label": "",
          "style": "bold",
          "text": "Options",
          "type": "label"
         },
         {
          "default": false,
          "help": "",
          "id": 22,
          "key": "ONESHOTFORMAT",
          "label": "Remplacer le format de l'album par \"One Shot\" si applicable",
          "type": "checkbox"
         },
         {
          "default": false,
          "help": "",
          "id": 23,
          "key": "CBDefault",
          "label": "Garder les valeurs proposées provenant du nom de fichier",
          "type": "checkbox"
         },
         {
          "default": false,
          "help": "",
          "id": 24,
          "key": "COUNTOF",
          "label": "Nombre total d'albums basé sur la numérotation des tomes",
          "type": "checkbox"
         },
         {
          "default": false,
          "help": "",
          "id": 25,
          "key": "COUNTFINIE",
          "label": "Nombre total d'albums seulement si la série est terminée",
          "type": "checkbox"
         },
         {
          "default": false,
          "help": "",
          "id": 26,
          "key": "ResumePremierTome",
          "label": "Résumé de la série sur le 1er tome seulement",
          "type": "checkbox"
         },
         {
          "default": false,
          "help": "",
          "id": 27,
          "key": "ShortWebLink",
          "label": "Utiliser des liens internet raccourcis pour les albums",
          "type": "checkbox"
         },
         {
          "default": false,
          "help": "",
          "id": 28,
          "key": "AcceptGenericArtists",
          "label": "Accepter les valeurs génériques d'artiste (<Collectif>, ...)",
          "type": "checkbox"
         },
         {
          "help": "",
          "id": 29,
          "key": "",
          "label": "",
          "type": "separator"
         },
         {
          "default": false,
          "help": "",
          "id": 30,
          "key": "ChampsVidesSeulement",
          "label": "Ne remplacer que les champs vides (ajout ComicRust)",
          "type": "checkbox"
         }
        ]
       ],
       "help": "",
       "id": 19,
       "key": "",
       "label": "",
       "type": "columns",
       "widths": []
      },
      {
       "controls": [
        {
         "default": "",
         "help": "« DEL » : vide les étiquettes",
         "hint_none": "",
         "hints": [],
         "id": 32,
         "inserts": [],
         "key": "TBTags",
         "label": "Étiquettes des albums",
         "paste": false,
         "placeholder": "vide = étiquettes inchangées ; DEL = les effacer",
         "required": false,
         "rows": 1,
         "secret": false,
         "type": "text"
        },
        {
         "default": "",
         "help": "",
         "hint_none": "",
         "hints": [],
         "id": 33,
         "inserts": [],
         "key": "SUBPATT",
         "label": "Chaîne de fin nom de série à supprimer",
         "paste": false,
         "placeholder": "",
         "required": false,
         "rows": 1,
         "secret": false,
         "type": "text"
        },
        {
         "decimals": 0,
         "default": 0.0,
         "help": "",
         "id": 34,
         "key": "PadNumber",
         "label": "Longueur des numéros d'album (complétée à gauche par 0)",
         "max": 9.0,
         "min": 0.0,
         "step": 1.0,
         "type": "number"
        }
       ],
       "help": "",
       "id": 31,
       "key": "",
       "label": "",
       "title": "",
       "type": "group"
      }
     ],
     "title": "Données"
    },
    {
     "controls": [
      {
       "controls": [
        {
         "default": false,
         "help": "",
         "id": 36,
         "key": "SHOWRENLOG",
         "label": "Afficher le rapport après le traitement",
         "type": "checkbox"
        },
        {
         "default": false,
         "help": "",
         "id": 37,
         "key": "DBGONOFF",
         "label": "Activer le log de debug",
         "type": "checkbox"
        },
        {
         "default": false,
         "help": "",
         "id": 38,
         "key": "SHOWDBGLOG",
         "label": "Afficher le log de debug après une erreur",
         "type": "checkbox"
        },
        {
         "decimals": 2,
         "default": 0.01,
         "help": "",
         "id": 39,
         "key": "RENLOGMAX",
         "label": "Taille maximale du fichier de rapport (en MO)",
         "max": 100.0,
         "min": 0.01,
         "step": 0.01,
         "type": "number"
        },
        {
         "decimals": 2,
         "default": 0.01,
         "help": "",
         "id": 40,
         "key": "DBGLOGMAX",
         "label": "Taille maximale du fichier de log de debug (en MO)",
         "max": 100.0,
         "min": 0.01,
         "step": 0.01,
         "type": "number"
        }
       ],
       "help": "",
       "id": 35,
       "key": "",
       "label": "",
       "title": "",
       "type": "group"
      },
      {
       "help": "",
       "id": 41,
       "key": "",
       "label": "",
       "style": "weak",
       "text": "Dossier : C:\\ComicRust\\data\\bedetheque\\journaux",
       "type": "label"
      }
     ],
     "title": "Logging"
    }
   ],
   "type": "tabs"
  },
  {
   "help": "App.Config du dossier du script dans ComicRack",
   "id": 42,
   "key": "importer",
   "label": "Importer les réglages de ComicRack…",
   "type": "button",
   "validate": false
  },
  {
   "help": "",
   "id": 43,
   "key": "",
   "label": "",
   "style": "small",
   "text": "Version 6.08 (c) 2021 kiwi13 & maforget — portage Python 3 pour ComicRust",
   "type": "label"
  }
 ],
 "label_width": 360.0,
 "message": "",
 "title": "Bedetheque Scraper 2",
 "values": {
  "ARTICLES": "Le,La,Les,L',The",
  "AcceptGenericArtists": false,
  "AlwaysChooseSerie": false,
  "CBDefault": false,
  "CBRescrape": false,
  "COUNTFINIE": true,
  "COUNTOF": true,
  "ChampsVidesSeulement": false,
  "ChoixAutoEdition": true,
  "ChoixUtilisateur": "1",
  "DBGLOGMAX": 0.01,
  "DBGONOFF": false,
  "Donnees": [
   "Series",
   "Title",
   "Writer",
   "Penciller",
   "Colorist",
   "Letterer",
   "Inker",
   "CoverArtist",
   "Publisher",
   "Language",
   "Format",
   "Rating",
   "Printed",
   "Imprint",
   "Synopsys",
   "Count",
   "Notes",
   "SerieStatus",
   "Web",
   "Genre",
   "Cover",
   "ISBN"
  ],
  "FORMATARTICLES": true,
  "LANGENFR": "FR",
  "NbVignettesSeries": 6,
  "ONESHOTFORMAT": false,
  "PadNumber": 0,
  "RENLOGMAX": 0.01,
  "ResumePremierTome": false,
  "SHOWDBGLOG": false,
  "SHOWRENLOG": false,
  "SUBPATT": " - - ",
  "ShortWebLink": false,
  "TBTags": "",
  "TIMEOUT": 1000,
  "TIMEOUTS": 7,
  "TITLEIT": true,
  "TailleVignettes": 150,
  "VignettesSeries": true,
  "use_cloudscraper": false
 },
 "width": 840.0
};
