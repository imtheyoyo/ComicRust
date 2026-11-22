# Protocole de mesure : performances et mémoire (5 000+ livres)

Écart 13 de l'audit de parité (« mémoire WebView2 non mesurée »). **Ce document décrit le protocole ; il n'a
pas été exécuté** (pas de Windows ni de WebView2 dans l'environnement de développement). Les résultats se
reportent dans la recette de la version, avec la machine utilisée.

## 1. Cadre
- **Jeu de données** : une bibliothèque de 5 000 livres au moins (idéalement 5 323, celle de la recette), dont
  ≥ 30 % en CBZ/CBR sur un disque réseau (NAS), quelques PDF, des séries de 100+ numéros, des listes
  intelligentes. Noter : nombre de livres, de séries, de listes, taille de `data\couvertures`.
- **Machine** : modèle, CPU, RAM, disque (SSD/NAS), version de Windows 11, version de WebView2
  (`edge://version`), version de ComicRust (À propos ▸ « Copier les informations de diagnostic »).
- **Conditions** : application neuve, installation portable, autres programmes lourds fermés, démarrage à froid
  (cache de couvertures vidé) puis à chaud (cache plein). Chaque mesure : 3 passes, retenir médiane et pire cas.
- **Comparer** : même bibliothèque ouverte dans ComicRack CE et dans la version egui (`ComicRust.exe`).

## 2. Outils
| Besoin | Outil |
|---|---|
| Mémoire par processus | Gestionnaire des tâches ▸ Détails (`ComicRust-Tauri.exe`, `msedgewebview2.exe` ×N : **additionner**), ou PowerShell : `Get-Process ComicRust-Tauri,msedgewebview2 \| Measure-Object WorkingSet64,PrivateMemorySize64 -Sum` |
| Mémoire JS de la page | DevTools de WebView2 (lancer avec `--mcp` ou activer F12) ▸ Mémoire ▸ instantané du tas ; ▸ Performance ▸ cases « Mémoire » |
| Temps de rendu, défilement | DevTools ▸ Performance (enregistrer 10 s de défilement) : images/s, tâches > 50 ms |
| Temps d'ouverture | Horloge : de la création du processus à l'affichage de la première couverture (`Get-Process … StartTime` + capture vidéo, ou `performance.now()` dans la console après `ouvrir_bibliotheque`) |
| Disque / réseau | Moniteur de ressources ▸ Disque (lectures de l'exécutable), compteur du NAS |

## 3. Scénarios et mesures
Pour chacun, relever **temps**, **mémoire totale** (ComicRust + WebView2) avant/après, **fluidité** (images/s).

| # | Scénario | À relever | Seuil indicatif |
|---|---|---|---|
| P1 | Démarrage à froid → bibliothèque affichée | temps jusqu'à `lister_livres` rendu ; mémoire au repos | < 3 s ; < 400 Mo |
| P2 | Démarrage à chaud | idem | < 2 s |
| P3 | Défilement continu de la grille (5 000 couvertures) de haut en bas, 3 tailles de couvertures | images/s ; mémoire au bas de la grille | ≥ 45 i/s ; croissance < 150 Mo |
| P4 | Passage grille ↔ tableau ↔ piles (empiler par série) | temps de bascule | < 500 ms |
| P5 | Recherche rapide (frappe de 5 lettres), tri sur chaque clé, regroupement par série | latence ressentie, temps `lister_livres` | < 300 ms |
| P6 | Ouvrir un livre de 200 pages (CBZ local, NAS, PDF) et tourner 100 pages | temps de la 1re page ; temps moyen par page ; mémoire du lecteur ; mémoire après fermeture | 1re page < 1 s (local) ; retour à ±50 Mo après fermeture |
| P7 | Mode défilement continu sur un livre de 200 pages | mémoire max ; absence de saccades | borne < 300 Mo |
| P8 | Analyse d'un dossier de 5 000 fichiers (nouveaux) | durée, débit, mémoire ; l'interface reste réactive | interface fluide |
| P9 | Vérification des livres manquants (5 000 fichiers, NAS) | durée ; arrêt possible | — |
| P10 | « ⟳ Relire » et changement de bibliothèque (assistant rejoué) | durée ; mémoire avant/après (fuite si elle monte à chaque passage) | stable après 5 passages |
| P11 | Fuites : 30 min d'usage varié (listes, édition, lecteur, thèmes), puis mémoire au repos | dérive de mémoire | < 10 % de dérive |
| P12 | Bibliothèque MySQL (réseau) : ouverture, scan de changements externes (surveillance) | durée d'ouverture ; charge réseau périodique | — |

## 4. Diagnostic en cas de dérive
1. Séparer **processus Rust** (`ComicRust-Tauri.exe`) et **WebView2** : si Rust grossit, suspecter le cache de
   couvertures, la vue en cache (`Biblio::vue`) ou les livres ouverts du lecteur ; si WebView2 grossit, le tas
   JS (instantané du tas, filtrer sur `LivreResume`, `Blob`, `ImageBitmap`).
2. Répéter le scénario 5 fois ; une croissance linéaire = fuite, un palier = cache.
3. Joindre les « informations de diagnostic », la machine, la taille de `data\`, les captures de DevTools.

## 5. Rapport
Tableau machine × scénario (médiane / pire cas), comparaison ComicRack CE et egui, verdict par seuil, liste
des régressions à corriger. À coller dans la recette (section « Performances ») et à reporter dans
`docs/tauri/audit-parite.md` (ligne « Couvertures, performance »).
