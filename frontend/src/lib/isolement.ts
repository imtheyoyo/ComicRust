// Calque plein écran qui garde le clavier pour lui (lecteur, relecture tauri-v1.0.2 B2).
//
// Le lecteur est une `div role="dialog" aria-modal="true"` posée au-dessus de la bibliothèque : sans
// plus, Tab donnait le focus aux boutons cachés dessous (bandeau de mise à jour, barre latérale,
// barre d'outils) et Entrée les activait sans que l'utilisateur les voie. `isoler` rend `inert`
// tout ce qui n'est pas sur le chemin de l'élément jusqu'à <body> (frères de chaque ancêtre), y
// compris ce qui apparaît ensuite à ces endroits (bandeau de mise à jour arrivé pendant la lecture).
// Les <dialog> restent actifs : une fenêtre ouverte depuis le calque (« Fichier introuvable » au
// passage au livre suivant…) doit rester utilisable ; fermées, elles ne prennent pas le focus.
// Compteur par élément : deux calques qui se chevauchent (un lecteur remplacé par un autre) ne se
// retirent pas mutuellement l'inertie ; un élément déjà inerte avant reste inerte.

const poses = new Map<HTMLElement, number>();

/** Rend inerte tout le document sauf `el` (et ses descendants). Renvoie la levée de l'isolement. */
export function isoler(el: HTMLElement): () => void {
  const touches: HTMLElement[] = [];
  const chemin = new Set<Element>();
  const parents: HTMLElement[] = [];
  for (let n: HTMLElement | null = el; n && n !== document.body; n = n.parentElement) {
    chemin.add(n);
    if (n.parentElement) parents.push(n.parentElement);
  }

  function poser(frere: Element) {
    if (chemin.has(frere) || !(frere instanceof HTMLElement) || ['SCRIPT', 'STYLE', 'DIALOG'].includes(frere.tagName)) return;
    if (touches.includes(frere)) return;
    const k = poses.get(frere);
    if (k === undefined) {
      if (frere.inert) return; // inerte par ailleurs (assistant ouvert…) : on n'y touche pas
      frere.inert = true;
      poses.set(frere, 1);
    } else poses.set(frere, k + 1);
    touches.push(frere);
  }

  for (const p of parents) for (const f of Array.from(p.children)) poser(f);
  const observateur = new MutationObserver((mutations) => {
    for (const m of mutations) m.addedNodes.forEach((n) => { if (n instanceof Element) poser(n); });
  });
  for (const p of parents) observateur.observe(p, { childList: true });

  let leve = false;
  return () => {
    if (leve) return;
    leve = true;
    observateur.disconnect();
    for (const f of touches) {
      const k = (poses.get(f) ?? 1) - 1;
      if (k <= 0) {
        poses.delete(f);
        f.inert = false;
      } else poses.set(f, k);
    }
  };
}
