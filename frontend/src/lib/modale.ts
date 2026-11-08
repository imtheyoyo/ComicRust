// Boîte de dialogue modale (<dialog> natif) dont la fermeture par Échap est entièrement maîtrisée.
//
// Pourquoi : Chromium (donc WebView2 sous Windows) protège les pages contre les fenêtres qu'on ne
// peut pas fermer (« CloseWatcher ») : si l'événement `cancel` a été annulé (preventDefault) et
// qu'aucune action de l'utilisateur (clic, frappe) n'a eu lieu depuis, l'Échap suivant envoie un
// `cancel` NON annulable et le <dialog> se ferme tout seul. Le composant qui attendait d'appeler
// `onfermer` restait alors monté avec un état « ouvert » faux : la fenêtre ne se rouvrait plus
// (recette tauri-v0.4.0 I-03). Ici :
// - Échap est traité au `keydown` (preventDefault : le navigateur ne déclenche plus `cancel`) et
//   ferme seulement si `peutFermer()` ; un composant intérieur qui veut garder Échap pour lui
//   (liste de suggestions, aperçu…) appelle preventDefault ou stopPropagation. L'écoute se fait sur
//   `window` : les gestionnaires `onkeydown` de Svelte 5 sont délégués à la racine de l'application,
//   donc exécutés APRÈS un écouteur posé sur le <dialog> lui-même ;
// - filet de sécurité : si le <dialog> se ferme quand même sans qu'on l'ait demandé, on le rouvre
//   s'il ne doit pas se fermer (modifications en cours), sinon on appelle `fermer()` pour que
//   l'état d'ouverture du parent reste cohérent.

export interface OptionsModale {
  /** La fenêtre peut-elle être fermée par Échap (pas de modification, pas d'opération en cours) ? */
  peutFermer?: () => boolean;
  /** Fermeture demandée (Échap autorisé, ou fermeture imprévue) : le composant remet son état à zéro. */
  fermer?: () => void;
  /** Ouvrir la fenêtre dès le montage (fenêtres créées à la demande par un {#if}). */
  ouvrirAuMontage?: boolean;
}

/** Action Svelte : `<dialog use:modale={{ peutFermer, fermer }}>`. */
export function modale(node: HTMLDialogElement, options: OptionsModale = {}) {
  let opts = options;
  const peut = () => opts.peutFermer?.() ?? true;
  const fermer = () => {
    if (opts.fermer) opts.fermer();
    else if (node.open) node.close();
  };

  function clavier(e: KeyboardEvent) {
    if (e.key !== 'Escape' || e.defaultPrevented || !node.open) return;
    // Touche venue de la fenêtre (ou du corps de page, si le focus s'est perdu).
    const t = e.target;
    if (!(t instanceof Node) || !(node.contains(t) || t === document.body || t === document.documentElement)) return;
    e.preventDefault(); // pas de `cancel` natif, donc pas de fermeture forcée par le navigateur
    if (peut()) fermer();
  }

  function cancel(e: Event) {
    // Autres sources de `cancel` (bouton retour, etc.) : même règle qu'Échap.
    if (e.cancelable) e.preventDefault();
    if (e.cancelable && peut()) fermer();
  }

  function close() {
    // Le navigateur a fermé la fenêtre de lui-même (cancel non annulable), ou un formulaire
    // method="dialog" l'a fermée. Si on ne pouvait pas fermer : on la rouvre (rien n'est perdu).
    if (!peut()) {
      try { node.showModal(); } catch { /* déjà ouverte ou retirée du document */ }
      return;
    }
    opts.fermer?.();
  }

  window.addEventListener('keydown', clavier);
  node.addEventListener('cancel', cancel);
  node.addEventListener('close', close);
  if (opts.ouvrirAuMontage && !node.open) node.showModal();

  return {
    update(o: OptionsModale) { opts = o; },
    destroy() {
      window.removeEventListener('keydown', clavier);
      node.removeEventListener('cancel', cancel);
      node.removeEventListener('close', close);
    },
  };
}
