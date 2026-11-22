// Inventaire des raccourcis clavier de ComicRust (fenêtre « À propos / Aide », touche F1).
// Relevé dans le code (lot L7) : VueBibliotheque.svelte (clavierGlobal), GrilleCouvertures / GrillePiles /
// VueTableau (navigation, Alt+↑/↓), Lecteur.svelte (clavier, molette), lib/fenetre.ts (F11, Échap, Ctrl+O),
// PanneauListes.svelte (arbre), OngletPages.svelte, dialogues. Une seule source : quand un lot ajoute
// un raccourci, il l'ajoute ici (ex. lot L6 : `I` infos du livre, fin de livre → livre suivant).

export interface Raccourci {
  touches: string;
  action: string;
}
export interface GroupeRaccourcis {
  titre: string;
  raccourcis: Raccourci[];
}

export const RACCOURCIS: GroupeRaccourcis[] = [
  {
    titre: 'Partout',
    raccourcis: [
      { touches: 'F1', action: 'Ouvrir cette aide' },
      { touches: 'F11', action: 'Plein écran de la fenêtre' },
      { touches: 'Échap', action: 'Quitter le plein écran, fermer une fenêtre ou un menu' },
      { touches: 'Ctrl+O', action: 'Ouvrir un fichier' },
      { touches: 'Ctrl+Maj+L', action: 'Afficher ou masquer le panneau des listes' },
    ],
  },
  {
    titre: 'Bibliothèque',
    raccourcis: [
      { touches: '/  ou  Ctrl+F', action: 'Aller au champ de recherche' },
      { touches: '↓ (dans la recherche)', action: 'Passer aux livres' },
      { touches: '← ↑ → ↓, Début, Fin, Pg↑, Pg↓', action: 'Se déplacer dans la grille ou le tableau' },
      { touches: 'Ctrl+clic, Maj+clic, Ctrl+A', action: 'Sélectionner plusieurs livres ou piles' },
      { touches: 'Entrée', action: 'Ouvrir le livre (ou la pile) courant' },
      { touches: 'Ctrl+I', action: 'Informations du livre sélectionné' },
      { touches: 'Menu, Maj+F10', action: 'Menu contextuel de la sélection' },
      { touches: 'Suppr', action: 'Retirer de la liste de lecture, ou de la bibliothèque (avec confirmation)' },
      { touches: 'Échap (pile ouverte)', action: 'Revenir aux piles' },
      { touches: 'Alt+↑ / Alt+↓', action: 'Monter / descendre la pile dans une liste de lecture' },
    ],
  },
  {
    titre: 'Panneau des listes',
    raccourcis: [
      { touches: '↑ ↓', action: 'Changer la liste affichée' },
      { touches: '→ / ←', action: 'Déplier, entrer / replier, remonter' },
      { touches: 'F2', action: 'Renommer la liste' },
      { touches: 'Suppr', action: 'Supprimer la liste' },
      { touches: 'Menu, Maj+F10', action: 'Menu de la liste' },
    ],
  },
  {
    titre: 'Lecteur',
    raccourcis: [
      { touches: '→ / ←', action: 'Page suivante / précédente (inversé en mode manga)' },
      { touches: 'Espace, Pg↓ / Retour arrière, Pg↑', action: 'Page suivante / précédente' },
      { touches: 'Début / Fin', action: 'Première / dernière page' },
      { touches: '↑ ↓', action: 'Faire défiler la page' },
      { touches: '1 2 3 4', action: 'Page entière, largeur, hauteur, taille réelle' },
      { touches: '+ / − / 0', action: 'Zoom avant / arrière / 100 %' },
      { touches: 'Ctrl+molette', action: 'Zoom sur le pointeur' },
      { touches: 'D', action: 'Double page' },
      { touches: 'V', action: 'Défilement vertical continu' },
      { touches: 'M', action: 'Sens manga' },
      { touches: 'T', action: 'Miniatures' },
      { touches: 'L', action: 'Loupe' },
      { touches: 'F ou F11', action: 'Plein écran' },
      { touches: 'Échap, Ctrl+W', action: "Fermer le lecteur (Échap quitte d'abord le plein écran)" },
      { touches: 'Clic gauche / droit, molette', action: 'Page suivante / précédente' },
    ],
  },
  {
    titre: 'Fenêtres et onglet Pages',
    raccourcis: [
      { touches: 'Ctrl+Entrée', action: 'Valider les informations du livre (OK)' },
      { touches: 'Alt+← → ↑ ↓ (onglet Pages)', action: 'Déplacer les pages sélectionnées' },
      { touches: 'Ctrl+A, Ctrl+C, Suppr, Entrée (onglet Pages)', action: 'Tout sélectionner, copier, supprimer, aperçu' },
    ],
  },
];
