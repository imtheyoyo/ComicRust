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

/** Fenêtres dont la fermeture a été demandée par le code (bouton Annuler, Fermer, Ouvrir…) :
 *  le filet de sécurité ne doit pas les rouvrir, même avec une saisie en cours. */
const fermeturesVoulues = new WeakSet<HTMLDialogElement>();

/** Ferme une fenêtre `use:modale` à la demande de l'utilisateur (bouton), modifications ou non. */
export function fermerModale(node: HTMLDialogElement | undefined): void {
  if (!node?.open) return;
  fermeturesVoulues.add(node);
  node.close();
}

/** Zones de travail au clavier (grille / tableau des livres, arbre des listes) : c'est là que
 *  Suppr, F2, flèches… agissent. Le focus y revient après un menu ou une fenêtre. */
const ZONE_TRAVAIL = '[data-testid="grille"], [data-testid="tableau"], [role="tree"]';
const horsCalque = (el: Element) => !el.closest('dialog, [role="menu"]');

/** Dernier élément focalisé hors fenêtre modale et hors menu. */
let dernierFocus: HTMLElement | null = null;
/** Dernier élément focalisé dans une zone de travail (recette K-02 : après « Ouvrir un fichier… »,
 *  le focus était rendu au bouton de la barre d'outils, où Suppr et F2 n'agissent pas). */
let dernierFocusTravail: HTMLElement | null = null;
if (typeof document !== 'undefined') {
  document.addEventListener('focusin', (e) => {
    const t = e.target;
    if (!(t instanceof HTMLElement) || t === document.body || !horsCalque(t)) return;
    dernierFocus = t;
    if (t.closest(ZONE_TRAVAIL)) dernierFocusTravail = t;
  });
}

/** Élément à refocaliser à la fermeture d'un menu ou d'une fenêtre ouvert depuis `origine` :
 *  - l'origine si elle est dans une zone de travail, ou si c'est un champ de saisie (recherche…) ;
 *  - sinon (bouton de barre d'outils, couverture non focalisable, corps de page) la dernière zone de
 *    travail utilisée, à défaut la grille / le tableau des livres affiché. */
export function cibleFocus(origine: Element | null): HTMLElement | null {
  const o = origine instanceof HTMLElement && origine !== document.body && origine.isConnected && horsCalque(origine) ? origine : null;
  if (o && (o.closest(ZONE_TRAVAIL) || o.matches('input, textarea, select, [contenteditable="true"]'))) return o;
  if (dernierFocusTravail?.isConnected) return dernierFocusTravail;
  const zone = [...document.querySelectorAll<HTMLElement>('[data-testid="grille"], [data-testid="tableau"]')].find(horsCalque);
  return zone ?? o;
}

/** Origine à retenir à l'ouverture d'un menu ou d'une fenêtre : l'élément actif s'il est hors
 *  calque, sinon le dernier élément focalisé (ouverture depuis un clic sur une zone non focalisable). */
export function origineFocus(): HTMLElement | null {
  const a = document.activeElement;
  return a instanceof HTMLElement && a !== document.body && horsCalque(a) ? a : dernierFocus;
}

/** Rend le focus (voir `cibleFocus`) une fois le DOM à jour, sauf si quelqu'un d'autre l'a pris
 *  entre-temps (champ ouvert par l'action choisie, autre fenêtre…) : seul un focus « perdu » (corps
 *  de page) ou resté sur l'origine hors zone de travail est déplacé. */
export function restaurerFocus(origine: HTMLElement | null): void {
  queueMicrotask(() => setTimeout(() => {
    const a = document.activeElement;
    const perdu = a === null || a === document.body || a === document.documentElement || (a === origine && !a.closest(ZONE_TRAVAIL));
    if (!perdu) return;
    const el = cibleFocus(origine);
    if (el && el !== a) el.focus({ preventScroll: true });
  }, 0));
}

/** Action Svelte : `<dialog use:modale={{ peutFermer, fermer }}>`. */
export function modale(node: HTMLDialogElement, options: OptionsModale = {}) {
  let opts = options;
  // Élément focalisé avant l'ouverture : rendu à la fermeture (sinon le focus reste sur <body>).
  let origine: HTMLElement | null = null;
  const memoriser = () => { if (!node.open) origine = origineFocus(); };
  // Ouverture par showModal() depuis n'importe où : on retient l'élément focalisé juste avant.
  const proto = node.showModal.bind(node);
  node.showModal = () => { memoriser(); proto(); };
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
    if (fermeturesVoulues.delete(node)) {
      opts.fermer?.();
      restaurerFocus(origine);
      origine = null;
      return;
    }
    // Le navigateur a fermé la fenêtre de lui-même (cancel non annulable), ou un formulaire
    // method="dialog" l'a fermée. Si on ne pouvait pas fermer : on la rouvre (rien n'est perdu).
    if (!peut()) {
      try { node.showModal(); } catch { /* déjà ouverte ou retirée du document */ }
      return;
    }
    opts.fermer?.();
    restaurerFocus(origine);
    origine = null;
  }

  window.addEventListener('keydown', clavier);
  node.addEventListener('cancel', cancel);
  node.addEventListener('close', close);
  if (opts.ouvrirAuMontage && !node.open) { memoriser(); node.showModal(); }

  return {
    update(o: OptionsModale) { opts = o; },
    destroy() {
      // Fenêtre retirée par son parent ({#if} remis à faux par `fermer`) sans événement `close` :
      // le focus partirait sur <body> (Suppr, F2 sans effet) ; on le rend comme à une fermeture.
      if (origine) restaurerFocus(origine);
      window.removeEventListener('keydown', clavier);
      node.removeEventListener('cancel', cancel);
      node.removeEventListener('close', close);
    },
  };
}
