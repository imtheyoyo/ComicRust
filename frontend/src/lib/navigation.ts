/** Navigation clavier commune à la grille et au tableau : renvoie le nouvel index, ou `null`
 *  si la touche n'est pas gérée. `colonnes` vaut 1 pour le tableau (flèches gauche/droite ignorées). */
export function cibleClavier(
  touche: string,
  courant: number,
  total: number,
  colonnes: number,
  lignesParPage: number,
): number | null {
  if (total <= 0) return null;
  if (courant < 0) {
    return ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(touche)
      ? 0
      : null;
  }
  let cible: number;
  switch (touche) {
    case 'ArrowRight': if (colonnes === 1) return null; cible = courant + 1; break;
    case 'ArrowLeft': if (colonnes === 1) return null; cible = courant - 1; break;
    case 'ArrowDown': cible = courant + colonnes; break;
    case 'ArrowUp': cible = courant - colonnes; break;
    case 'PageDown': cible = courant + colonnes * lignesParPage; break;
    case 'PageUp': cible = courant - colonnes * lignesParPage; break;
    case 'Home': cible = 0; break;
    case 'End': cible = total - 1; break;
    default: return null;
  }
  return Math.min(total - 1, Math.max(0, cible));
}
