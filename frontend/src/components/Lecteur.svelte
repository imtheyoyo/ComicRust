<script lang="ts">
  // Lecteur plein écran (étape 2). Ouvre le livre (ouvrir_livre), affiche les pages servies par le
  // backend à `${urlBase}/${index}?l=${largeur}` et renvoie l'avancement (enregistrer_avancement).
  // Modes : page simple, double page (pages doubles seules, couverture seule), défilement vertical
  // continu ; sens manga ; ajustements ; zoom ; loupe. Clavier et souris comme la version egui
  // (crates/comicrust/src/input.rs et ui/viewer.rs), Échap ferme le lecteur.
  import { onMount, tick, untrack } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import {
    enregistrerAvancement, fermerLivre, messageErreur, ouvrirFichier, ouvrirLivre, pleinEcran, urlPage,
    type Ajustement, type ChoixReprise, type LivreOuvert, type ModeLecture, type PageInfo, type SourceLecteur,
  } from '../lib/api';
  import {
    borneZoom, debutPlanche, DIMENSIONS_DEFAUT, dimensionsAffichees, echelleAjustement, largeurDemandee,
    pagesVisibles, planche, positionDe, precedente, suivante, type ContexteNavigation, type Dimensions,
  } from '../lib/lecteur';
  import { enregistrerReglagesLecteur, reglagesLecteur } from '../lib/prefs';

  interface Props {
    /** Livre de la bibliothèque (`{id}` = LivreResume.id) ou fichier hors bibliothèque (`{chemin}`). */
    source: SourceLecteur;
    onfermer: () => void;
  }
  let { source, onfermer }: Props = $props();

  /** Délai avant l'envoi de l'avancement après un changement de page (ms). */
  const DELAI_AVANCEMENT = 800;
  /** Délai d'inactivité de la souris avant de masquer les barres (ms). */
  const DELAI_BARRES = 2500;
  /** Délai minimal entre deux changements de page à la molette (ms, comme egui). */
  const DELAI_MOLETTE = 180;
  /** Planches préchargées après / avant la planche courante. */
  const PRECHARGE_APRES = 3;
  const PRECHARGE_AVANT = 1;
  /** Défilement continu : espace entre les pages et pages chargées autour de la zone visible. */
  const ECART = 8;
  const MARGE_DEFILEMENT = 3;
  const TAILLE_LOUPE = 260;

  // ── Réglages mémorisés (data\comicrust-tauri.json, section vue.lecteur) ──
  const memo = reglagesLecteur();
  let mode = $state<ModeLecture>(memo.mode ?? 'simple');
  let ajustement = $state<Ajustement>(memo.ajustement ?? 'meilleur');
  let couvertureSeule = $state(memo.couvertureSeule ?? true);
  let miniatures = $state(memo.miniatures ?? false);
  let grossissement = $state(memo.grossissementLoupe ?? 2.5);

  $effect(() => {
    enregistrerReglagesLecteur({ mode, ajustement, couvertureSeule, miniatures, grossissementLoupe: grossissement });
  });

  // ── Livre ──
  let livre = $state<LivreOuvert | null>(null);
  let erreur = $state('');
  let manga = $state(false);
  let position = $state(0);
  let zoom = $state(1);
  /** Dimensions réelles des images chargées (avant rotation), par index de page. */
  const dimsChargees = new SvelteMap<number, Dimensions>();
  const pagesEnErreur = new SvelteSet<number>();

  const visibles = $derived(livre ? pagesVisibles(livre.pages, livre.nbPages) : []);
  const infos = $derived(new Map<number, PageInfo>((livre?.pages ?? []).map((p) => [p.index, p])));
  const rotation = (index: number) => infos.get(index)?.rotation ?? 0;

  function dimsBrutes(index: number): Dimensions {
    const d = dimsChargees.get(index);
    if (d) return d;
    const p = infos.get(index);
    if (p?.largeur && p?.hauteur) return { largeur: p.largeur, hauteur: p.hauteur };
    return p?.double ? { largeur: DIMENSIONS_DEFAUT.largeur * 2, hauteur: DIMENSIONS_DEFAUT.hauteur } : DIMENSIONS_DEFAUT;
  }
  const dimsDe = (index: number) => dimensionsAffichees(dimsBrutes(index), rotation(index));

  const ctx = $derived<ContexteNavigation>({
    nb: visibles.length,
    double: mode === 'double',
    couvertureSeule,
    large: (k: number) => {
      const index = visibles[k];
      if (index === undefined) return false;
      if (infos.get(index)?.double) return true;
      const d = dimsDe(index);
      return d.largeur > d.hauteur;
    },
  });

  const positions = $derived(mode === 'defilement' ? [position] : planche(ctx, position));
  const ordreAffichage = $derived(manga ? [...positions].reverse() : positions);

  // ── Zone d'affichage ──
  let scene = $state<HTMLDivElement>();
  let racine = $state<HTMLDivElement>();
  let sceneL = $state(1000);
  let sceneH = $state(700);
  let largeurFenetre = $state(1280);
  const largeurL = $derived(
    largeurDemandee(largeurFenetre, zoom, ajustement, typeof devicePixelRatio === 'number' ? devicePixelRatio : 1),
  );
  const url = (index: number, l = largeurL) => (livre ? urlPage(livre, index, l) : '');

  // Modes page : planche ramenée à une hauteur commune, puis ajustée (viewer.rs).
  const disposition = $derived.by(() => {
    const tailles = ordreAffichage.map((k) => dimsDe(visibles[k]));
    const refH = Math.max(1, ...tailles.map((t) => t.hauteur));
    const ramenees = tailles.map((t) => ({ largeur: (t.largeur * refH) / t.hauteur, hauteur: refH }));
    const total = { largeur: ramenees.reduce((s, t) => s + t.largeur, 0), hauteur: refH };
    const echelle = echelleAjustement(ajustement, total, { largeur: sceneL, hauteur: sceneH }) * zoom;
    return {
      pages: ordreAffichage.map((k, i) => ({
        position: k, index: visibles[k], largeur: ramenees[i].largeur * echelle, hauteur: ramenees[i].hauteur * echelle,
      })),
      largeur: total.largeur * echelle,
      hauteur: total.hauteur * echelle,
    };
  });
  const defilable = $derived(
    mode === 'defilement' || disposition.largeur > sceneL + 1 || disposition.hauteur > sceneH + 1,
  );

  // Défilement continu : hauteur de chaque page et décalage cumulé.
  const colonne = $derived.by(() => {
    if (mode !== 'defilement') return { tailles: [] as Dimensions[], hauts: [] as number[], total: 0, largeur: 0 };
    const tailles = visibles.map((index) => {
      const d = dimsDe(index);
      const s = echelleAjustement(ajustement, d, { largeur: sceneL, hauteur: sceneH }) * zoom;
      return { largeur: d.largeur * s, hauteur: d.hauteur * s };
    });
    const hauts: number[] = [];
    let y = 0;
    for (const t of tailles) {
      hauts.push(y);
      y += t.hauteur + ECART;
    }
    return { tailles, hauts, total: Math.max(0, y - ECART), largeur: Math.max(0, ...tailles.map((t) => t.largeur)) };
  });
  let hautScene = $state(0);
  /** Page affichée en haut et décalage dans celle-ci : conservés quand les hauteurs changent. */
  let ancre = { position: 0, decalage: 0 };

  function positionAuHaut(y: number): number {
    const h = colonne.hauts;
    let a = 0;
    let b = h.length - 1;
    while (a < b) {
      const m = (a + b + 1) >> 1;
      if (h[m] <= y) a = m;
      else b = m - 1;
    }
    return a;
  }
  const plageDefilement = $derived.by(() => {
    if (mode !== 'defilement' || visibles.length === 0) return { a: 0, b: -1 };
    return {
      a: Math.max(0, positionAuHaut(hautScene) - MARGE_DEFILEMENT),
      b: Math.min(visibles.length - 1, positionAuHaut(hautScene + sceneH) + MARGE_DEFILEMENT),
    };
  });

  $effect(() => {
    // Hauteurs recalculées (image chargée, zoom, taille de fenêtre) : on garde la page du haut.
    const hauts = colonne.hauts;
    if (mode !== 'defilement' || !scene || hauts.length === 0) return;
    untrack(() => {
      const cible = (hauts[ancre.position] ?? 0) + ancre.decalage;
      if (Math.abs(scene!.scrollTop - cible) > 1) scene!.scrollTop = cible;
    });
  });

  function surDefilement() {
    if (!scene || mode !== 'defilement') return;
    hautScene = scene.scrollTop;
    const p = positionAuHaut(hautScene);
    ancre = { position: p, decalage: hautScene - (colonne.hauts[p] ?? 0) };
    const lue = positionAuHaut(hautScene + sceneH * 0.25);
    if (lue !== position) position = lue;
  }

  // ── Navigation ──
  async function aller(p: number) {
    if (visibles.length === 0) return;
    position = Math.min(Math.max(0, p), visibles.length - 1);
    if (mode === 'defilement') {
      ancre = { position, decalage: 0 };
      if (scene) scene.scrollTop = colonne.hauts[position] ?? 0;
      return;
    }
    await tick();
    if (scene) {
      scene.scrollTop = 0;
      scene.scrollLeft = manga ? scene.scrollWidth : 0;
    }
  }
  function avancer() {
    const s = mode === 'defilement' ? (position + 1 < visibles.length ? position + 1 : null) : suivante(ctx, position);
    if (s !== null) void aller(s);
  }
  function reculer() {
    const s = mode === 'defilement' ? (position > 0 ? position - 1 : null) : precedente(ctx, position);
    if (s !== null) void aller(s);
  }
  const allerA = (k: number) => void aller(mode === 'defilement' ? k : debutPlanche(ctx, k));

  function changerMode(m: ModeLecture) {
    if (m === mode) return;
    const p = position;
    mode = m;
    void tick().then(() => aller(m === 'defilement' ? p : debutPlanche(ctx, p)));
  }
  function changerAjustement(a: Ajustement) {
    ajustement = a;
    zoom = 1;
    if (mode !== 'defilement') void aller(position);
  }
  function basculerManga() {
    manga = !manga;
    if (!livre?.manga) enregistrerReglagesLecteur({ manga });
  }

  // ── Zoom ──
  async function zoomer(nouveau: number, px?: number, py?: number) {
    const z = borneZoom(nouveau);
    if (!scene || z === zoom) return;
    const r = z / zoom;
    const x = px ?? sceneL / 2;
    const y = py ?? sceneH / 2;
    const gx = scene.scrollLeft + x;
    const gy = scene.scrollTop + y;
    zoom = z;
    if (mode === 'defilement') return; // l'ancre garde la page du haut
    await tick();
    scene.scrollLeft = gx * r - x;
    scene.scrollTop = gy * r - y;
  }

  // ── Avancement ──
  let minuterieAvancement: ReturnType<typeof setTimeout> | undefined;
  let dernierEnvoi = '';
  const etatAvancement = $derived.by(() => {
    if (!livre || visibles.length === 0) return null;
    const termine =
      mode === 'defilement'
        ? hautScene + sceneH >= colonne.total - 2 || position === visibles.length - 1
        : positions.includes(visibles.length - 1);
    return { page: visibles[position], termine };
  });
  $effect(() => {
    const a = etatAvancement;
    if (!a) return;
    clearTimeout(minuterieAvancement);
    minuterieAvancement = setTimeout(envoyerAvancement, DELAI_AVANCEMENT);
  });
  let termineVu = false;
  async function envoyerAvancement(): Promise<void> {
    clearTimeout(minuterieAvancement);
    const a = etatAvancement;
    if (!livre || !a || questionReprise) return;
    termineVu ||= a.termine; // une fois la fin vue, le livre reste « lu » pour cette session
    const clef = `${a.page}/${termineVu}`;
    if (clef === dernierEnvoi) return;
    dernierEnvoi = clef;
    try {
      await enregistrerAvancement(livre.jeton, a.page, termineVu);
    } catch (e) {
      console.warn('avancement non enregistré', e);
    }
  }

  let fermeture = false;
  async function fermer() {
    if (fermeture) return;
    fermeture = true;
    if (pleinEcranActif) void basculerPleinEcran();
    const l = livre;
    if (l) {
      await envoyerAvancement();
      try {
        await fermerLivre(l.jeton);
      } catch (e) {
        console.warn('fermeture du livre', e);
      }
    }
    onfermer();
  }

  // ── Plein écran ──
  let pleinEcranActif = $state(false);
  async function basculerPleinEcran() {
    const voulu = !pleinEcranActif;
    try {
      await pleinEcran(voulu);
      pleinEcranActif = voulu;
    } catch (e) {
      console.warn('plein écran', e);
    }
  }

  // ── Barres auto-masquées ──
  let barres = $state(true);
  let surBarre = false;
  let minuterieBarres: ReturnType<typeof setTimeout> | undefined;
  function montrerBarres() {
    barres = true;
    clearTimeout(minuterieBarres);
    minuterieBarres = setTimeout(() => {
      const focus = document.activeElement;
      if (!surBarre && !(focus instanceof HTMLSelectElement)) barres = false;
    }, DELAI_BARRES);
  }

  /** Barres : restent visibles tant que la souris ou le focus clavier y est. Écouteurs posés par
   *  attachement (une barre d'outils ARIA n'est pas elle-même focalisable). */
  function survolBarre(el: HTMLElement) {
    const entre = () => (surBarre = true);
    const sort = () => (surBarre = false);
    el.addEventListener('pointerenter', entre);
    el.addEventListener('pointerleave', sort);
    el.addEventListener('focusin', montrerBarres);
    return () => {
      el.removeEventListener('pointerenter', entre);
      el.removeEventListener('pointerleave', sort);
      el.removeEventListener('focusin', montrerBarres);
    };
  }

  // ── Loupe ──
  let loupe = $state(false);
  let loupeVue = $state<{ x: number; y: number; index: number; l: number; h: number; rx: number; ry: number } | null>(null);
  function suivreLoupe(e: PointerEvent) {
    if (!loupe) return;
    const cible = (document.elementsFromPoint(e.clientX, e.clientY) as HTMLElement[]).find((el) => el.dataset?.page);
    if (!cible) {
      loupeVue = null;
      return;
    }
    const r = cible.getBoundingClientRect();
    loupeVue = {
      x: e.clientX, y: e.clientY, index: Number(cible.dataset.page), l: r.width, h: r.height,
      rx: (e.clientX - r.left) / r.width, ry: (e.clientY - r.top) / r.height,
    };
  }

  // ── Souris : clic (tiers gauche/droit), clic droit, glisser quand zoomé ──
  let appui: { x: number; y: number; gauche: number; haut: number; deplace: boolean; id: number } | null = null;
  let glisse = $state(false);
  function pointeurBas(e: PointerEvent) {
    if (e.button !== 0 || !scene) return;
    appui = { x: e.clientX, y: e.clientY, gauche: scene.scrollLeft, haut: scene.scrollTop, deplace: false, id: e.pointerId };
  }
  function pointeurBouge(e: PointerEvent) {
    montrerBarres();
    suivreLoupe(e);
    if (!appui || !scene || e.pointerId !== appui.id) return;
    const dx = e.clientX - appui.x;
    const dy = e.clientY - appui.y;
    if (!appui.deplace && Math.hypot(dx, dy) > 4 && defilable) {
      appui.deplace = true;
      scene.setPointerCapture(e.pointerId);
    }
    if (appui.deplace) {
      glisse = true;
      scene.scrollLeft = appui.gauche - dx;
      scene.scrollTop = appui.haut - dy;
    }
  }
  function pointeurHaut(e: PointerEvent) {
    if (!appui || e.pointerId !== appui.id) return;
    const { deplace } = appui;
    appui = null;
    glisse = false;
    if (scene?.hasPointerCapture(e.pointerId)) scene.releasePointerCapture(e.pointerId);
    if (deplace || !scene) return;
    // Tiers gauche / droit de la vue : page précédente / suivante (inversé en manga), centre : rien.
    const r = scene.getBoundingClientRect();
    const rel = (e.clientX - r.left) / r.width;
    const inverse = manga && mode !== 'defilement';
    if (rel > 0.67) inverse ? reculer() : avancer();
    else if (rel < 0.33) inverse ? avancer() : reculer();
  }
  function menuContextuel(e: MouseEvent) {
    e.preventDefault();
    reculer();
  }

  let derniereMolette = 0;
  function molette(e: WheelEvent) {
    if (e.ctrlKey) {
      e.preventDefault();
      const r = scene!.getBoundingClientRect();
      void zoomer(zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1), e.clientX - r.left, e.clientY - r.top);
      return;
    }
    if (mode === 'defilement' || defilable) return; // défilement natif
    e.preventDefault();
    if (e.deltaY === 0 || performance.now() - derniereMolette < DELAI_MOLETTE) return;
    derniereMolette = performance.now();
    if (e.deltaY > 0) avancer();
    else reculer();
  }
  /** Molette non passive (preventDefault pour Ctrl+molette et le changement de page). */
  function ecouterMolette(el: HTMLElement) {
    el.addEventListener('wheel', molette, { passive: false });
    return () => el.removeEventListener('wheel', molette);
  }

  // ── Clavier (input.rs) ──
  function clavier(e: KeyboardEvent) {
    if ((!livre || questionReprise) && e.key !== 'Escape') return;
    const cible = e.target as HTMLElement | null;
    if (cible?.closest('input, select, textarea') && e.key !== 'Escape') return;
    if (e.altKey || ((e.ctrlKey || e.metaKey) && !['w', 'W'].includes(e.key))) return;
    const continu = mode === 'defilement';
    const pas = (f: number) => scene?.scrollBy({ top: f * sceneH });
    switch (e.key) {
      case 'ArrowRight': if (manga && !continu) reculer(); else avancer(); break;
      case 'ArrowLeft': if (manga && !continu) avancer(); else reculer(); break;
      case 'ArrowDown': pas(0.15); break;
      case 'ArrowUp': pas(-0.15); break;
      case ' ':
        if (continu) pas(e.shiftKey ? -0.9 : 0.9);
        else if (e.shiftKey) reculer();
        else avancer();
        break;
      case 'PageDown': if (continu) pas(0.9); else avancer(); break;
      case 'PageUp': if (continu) pas(-0.9); else reculer(); break;
      case 'Backspace': reculer(); break;
      case 'Home': void aller(0); break;
      case 'End': allerA(visibles.length - 1); break;
      case 'Escape': if (pleinEcranActif) void basculerPleinEcran(); else void fermer(); break;
      case 'w': case 'W': if (e.ctrlKey || e.metaKey) void fermer(); else return; break;
      case 'F11': case 'f': case 'F': void basculerPleinEcran(); break;
      case '1': changerAjustement('meilleur'); break;
      case '2': changerAjustement('largeur'); break;
      case '3': changerAjustement('hauteur'); break;
      case '4': changerAjustement('reelle'); break;
      case 'd': case 'D': changerMode(mode === 'double' ? 'simple' : 'double'); break;
      case 'v': case 'V': changerMode(mode === 'defilement' ? 'simple' : 'defilement'); break;
      case 'm': case 'M': basculerManga(); break;
      case 't': case 'T': miniatures = !miniatures; break;
      case 'l': case 'L': loupe = !loupe; if (!loupe) loupeVue = null; break;
      case '+': case '=': void zoomer(zoom * 1.25); break;
      case '-': void zoomer(zoom / 1.25); break;
      case '0': void zoomer(1); break;
      default: return;
    }
    e.preventDefault();
  }

  // ── Préchargement : planches voisines décodées hors écran ──
  const precharges = $derived.by(() => {
    if (!livre || visibles.length === 0) return [] as number[];
    const liste: number[] = [];
    if (mode === 'defilement') return liste; // la colonne charge déjà ses voisines
    let p: number | null = position;
    for (let k = 0; k < PRECHARGE_APRES && p !== null; k++) {
      p = suivante(ctx, p);
      if (p !== null) for (const q of planche(ctx, p)) liste.push(visibles[q]);
    }
    p = position;
    for (let k = 0; k < PRECHARGE_AVANT && p !== null; k++) {
      p = precedente(ctx, p);
      if (p !== null) for (const q of planche(ctx, p)) liste.push(visibles[q]);
    }
    return liste;
  });

  /** Suit le chargement d'une image : dimensions réelles (détection des pages doubles) et erreurs. */
  function suivreImage(index: number) {
    return (img: HTMLImageElement) => {
      const charge = () => {
        if (img.naturalWidth > 0) {
          const d = { largeur: img.naturalWidth, hauteur: img.naturalHeight };
          const avant = dimsChargees.get(index);
          if (!avant || avant.largeur !== d.largeur || avant.hauteur !== d.hauteur) {
            // Seul le rapport largeur/hauteur compte (l'image peut être réduite par ?l=).
            const connue = dimsBrutes(index);
            if (Math.abs(connue.largeur / connue.hauteur - d.largeur / d.hauteur) > 0.01) dimsChargees.set(index, d);
          }
          pagesEnErreur.delete(index);
        }
      };
      const echec = () => pagesEnErreur.add(index);
      if (img.complete && img.naturalWidth > 0) charge();
      img.addEventListener('load', charge);
      img.addEventListener('error', echec);
      return () => {
        img.removeEventListener('load', charge);
        img.removeEventListener('error', echec);
      };
    };
  }
  function decoder(img: HTMLImageElement) {
    img.decode?.().catch(() => { /* erreur gérée par suivreImage */ });
  }

  // ── Miniatures ──
  let bandeau = $state<HTMLDivElement>();
  $effect(() => {
    const p = position;
    if (!miniatures || !bandeau) return;
    const m = bandeau.querySelector<HTMLElement>(`[data-miniature="${p}"]`);
    if (!m) return;
    // Pas de scrollIntoView : il fait défiler TOUS les ancêtres, y compris le cadre du lecteur
    // (overflow masqué mais défilable par script). Bandeau bas rétracté (translateY(100%)), la
    // miniature est sous la fenêtre : tout le lecteur remontait de la hauteur du bandeau et le
    // haut de la page était coupé (recette tauri-v0.7.1, anomalie lecteur). On ne fait défiler
    // que le bandeau, horizontalement, pour centrer la miniature.
    const b = bandeau.getBoundingClientRect();
    const r = m.getBoundingClientRect();
    bandeau.scrollLeft += r.left + r.width / 2 - (b.left + b.width / 2);
  });

  /** Garde-fou : le cadre du lecteur ne doit jamais défiler (focus d'un élément d'un bandeau
   *  rétracté, etc.) ; seule la scène défile. */
  function cadreFixe() {
    if (racine && (racine.scrollTop !== 0 || racine.scrollLeft !== 0)) {
      racine.scrollTop = 0;
      racine.scrollLeft = 0;
    }
  }

  // ── Reprise : la bibliothèque et ce poste n'ont pas retenu la même page (windows.rs,
  // `resume_window` de la version egui) ──
  let questionReprise = $state<{ bibliotheque: number; poste: number } | null>(null);
  let toujoursCeChoix = $state(false);
  let boutonReprise = $state<HTMLButtonElement>();

  /** Affiche le livre à partir de la page `index` (numéro de page du livre). */
  async function demarrer(index: number) {
    if (!livre) return;
    const vis = pagesVisibles(livre.pages, livre.nbPages);
    dernierEnvoi = `${vis[positionDe(vis, index)]}/false`;
    await tick();
    void aller(mode === 'defilement' ? positionDe(vis, index) : debutPlanche(ctx, positionDe(vis, index)));
    racine?.focus();
  }

  function choisirReprise(index: number, choix?: Exclude<ChoixReprise, 'demander'>) {
    questionReprise = null;
    if (toujoursCeChoix && choix) enregistrerReglagesLecteur({ reprise: choix });
    void demarrer(index);
  }

  // ── Ouverture ──
  onMount(() => {
    racine?.focus();
    montrerBarres();
    let vivant = true;
    ('id' in source ? ouvrirLivre(source.id) : ouvrirFichier(source.chemin))
      .then(async (l) => {
        if (!vivant) {
          void fermerLivre(l.jeton).catch(() => {});
          return;
        }
        manga = l.manga || (memo.manga ?? false);
        livre = l;
        const autre = l.pageRepriseComicrack;
        if (autre === undefined || autre === l.pageReprise) return demarrer(l.pageReprise);
        const reprise = memo.reprise ?? 'demander';
        if (reprise === 'comicrack') return demarrer(autre);
        if (reprise === 'comicrust') return demarrer(l.pageReprise);
        // Rien n'est envoyé tant que la page n'est pas choisie (page de départ inconnue).
        dernierEnvoi = `${l.pageReprise}/false`;
        questionReprise = { bibliotheque: autre, poste: l.pageReprise };
        await tick();
        boutonReprise?.focus();
      })
      .catch((e) => (erreur = messageErreur(e)));
    const quitter = () => void envoyerAvancement();
    const pleinEcranDoc = () => {
      if (!document.fullscreenElement && pleinEcranActif && !('__TAURI_INTERNALS__' in window)) pleinEcranActif = false;
    };
    window.addEventListener('pagehide', quitter);
    document.addEventListener('fullscreenchange', pleinEcranDoc);
    return () => {
      vivant = false;
      clearTimeout(minuterieBarres);
      clearTimeout(minuterieAvancement);
      window.removeEventListener('pagehide', quitter);
      document.removeEventListener('fullscreenchange', pleinEcranDoc);
    };
  });

  const libellePage = $derived.by(() => {
    if (visibles.length === 0) return '';
    const nums = [...positions].sort((a, b) => a - b).map((k) => k + 1);
    return `${nums.length > 1 ? 'Pages' : 'Page'} ${nums.join('-')} / ${visibles.length}`;
  });
  const AJUSTEMENTS: Array<{ id: Ajustement; titre: string }> = [
    { id: 'meilleur', titre: 'Page entière (1)' },
    { id: 'largeur', titre: 'Largeur (2)' },
    { id: 'hauteur', titre: 'Hauteur (3)' },
    { id: 'reelle', titre: 'Taille réelle (4)' },
  ];
</script>

<svelte:window onkeydown={clavier} bind:innerWidth={largeurFenetre} />

{#snippet image(index: number, largeur: number, hauteur: number, l: number)}
  {@const r = rotation(index)}
  {@const quart = ((r % 180) + 180) % 180 === 90}
  <img
    src={url(index, l)}
    alt="Page {index + 1}"
    draggable="false"
    decoding="async"
    style:width="{quart ? hauteur : largeur}px"
    style:height="{quart ? largeur : hauteur}px"
    style:transform={r ? `translate(-50%, -50%) rotate(${r}deg)` : 'translate(-50%, -50%)'}
    {@attach suivreImage(index)}
  />
{/snippet}

{#snippet page(index: number, largeur: number, hauteur: number)}
  <div class="page" data-page={index} style:width="{largeur}px" style:height="{hauteur}px">
    {#if pagesEnErreur.has(index)}
      <span class="illisible">⚠ Page {index + 1} illisible</span>
    {:else}
      <span class="attente">Chargement…</span>
      {@render image(index, largeur, hauteur, largeurL)}
    {/if}
  </div>
{/snippet}

<div
  class="lecteur"
  class:sans-curseur={!barres && !loupe}
  bind:this={racine}
  tabindex="-1"
  role="dialog"
  aria-modal="true"
  aria-label="Lecteur{livre ? ' : ' + livre.titre : ''}"
  data-testid="lecteur"
  data-mode={mode}
  data-manga={manga}
  data-position={position}
  onscroll={cadreFixe}
>
  {#if erreur}
    <div class="message erreur" role="alert">
      <p>Impossible d'ouvrir ce livre : {erreur}</p>
      <button onclick={() => void fermer()}>Fermer</button>
    </div>
  {:else if !livre}
    <p class="message">Ouverture du livre…</p>
  {:else if visibles.length === 0}
    <div class="message">
      <p>Ce livre ne contient aucune page lisible.</p>
      <button onclick={() => void fermer()}>Fermer</button>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="scene"
      class:defilable
      class:glisse
      class:loupe-active={loupe}
      bind:this={scene}
      bind:clientWidth={sceneL}
      bind:clientHeight={sceneH}
      onpointerdown={pointeurBas}
      onpointermove={pointeurBouge}
      onpointerup={pointeurHaut}
      onpointercancel={() => { appui = null; glisse = false; }}
      onpointerleave={() => (loupeVue = null)}
      oncontextmenu={menuContextuel}
      onscroll={surDefilement}
      {@attach ecouterMolette}
      data-testid="scene"
    >
      {#if mode === 'defilement'}
        <div class="colonne" style:height="{colonne.total}px" style:width="{Math.max(colonne.largeur, sceneL)}px">
          {#each visibles as index, k (index)}
            {@const t = colonne.tailles[k]}
            <div class="case" style:top="{colonne.hauts[k]}px" style:width="{t.largeur}px" style:height="{t.hauteur}px">
              {#if k >= plageDefilement.a && k <= plageDefilement.b}
                {@render page(index, t.largeur, t.hauteur)}
              {:else}
                <div class="page vide" style:width="{t.largeur}px" style:height="{t.hauteur}px"></div>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <div class="plan" style:width="{Math.max(disposition.largeur, sceneL)}px" style:height="{Math.max(disposition.hauteur, sceneH)}px">
          <div class="planche" style:width="{disposition.largeur}px" style:height="{disposition.hauteur}px">
            {#each disposition.pages as p (p.index)}
              {@render page(p.index, p.largeur, p.hauteur)}
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <div class="prechargement" aria-hidden="true">
      {#each precharges as index (index)}
        <img src={url(index)} alt="" decoding="async" {@attach suivreImage(index)} {@attach decoder} />
      {/each}
    </div>

    {#if loupe && loupeVue}
      {@const g = grossissement}
      <div
        class="loupe"
        style:left="{loupeVue.x - TAILLE_LOUPE / 2}px"
        style:top="{loupeVue.y - TAILLE_LOUPE / 2}px"
        style:width="{TAILLE_LOUPE}px"
        style:height="{TAILLE_LOUPE}px"
        data-testid="loupe"
      >
        <div
          class="page"
          style:width="{loupeVue.l * g}px"
          style:height="{loupeVue.h * g}px"
          style:left="{TAILLE_LOUPE / 2 - loupeVue.rx * loupeVue.l * g}px"
          style:top="{TAILLE_LOUPE / 2 - loupeVue.ry * loupeVue.h * g}px"
        >
          {@render image(loupeVue.index, loupeVue.l * g, loupeVue.h * g, largeurL)}
        </div>
      </div>
    {/if}
  {/if}

  {#if questionReprise && livre}
    {@const q = questionReprise}
    <div class="reprise" role="alertdialog" aria-modal="true" aria-labelledby="titre-reprise" aria-describedby="texte-reprise" data-testid="question-reprise">
      <h2 id="titre-reprise">Reprendre la lecture</h2>
      <p class="nom">{livre.titre}</p>
      <p id="texte-reprise">La bibliothèque et ce poste n'ont pas retenu la même page :</p>
      <button bind:this={boutonReprise} onclick={() => choisirReprise(q.bibliotheque, 'comicrack')} data-testid="reprise-bibliotheque">📖 Page {q.bibliotheque + 1} — lue dans la bibliothèque</button>
      <button onclick={() => choisirReprise(q.poste, 'comicrust')} data-testid="reprise-poste">📖 Page {q.poste + 1} — lue dans ComicRust (ce poste)</button>
      <button onclick={() => choisirReprise(0)}>⏮ Depuis le début</button>
      <label><input type="checkbox" bind:checked={toujoursCeChoix} /> Toujours faire ce choix</label>
      <button onclick={() => void fermer()}>Annuler</button>
    </div>
  {/if}

  <header
    class="barre haut"
    class:cachee={!barres}
    {@attach survolBarre}
    role="toolbar"
    aria-label="Outils du lecteur"
  >
    <button onclick={() => void fermer()} title="Fermer le lecteur (Échap)" aria-label="Fermer le lecteur" data-testid="fermer-lecteur">✖</button>
    <span class="titre" title={livre?.titre}>{livre?.titre ?? ''}</span>
    <span class="numero" data-testid="numero-page">{libellePage}</span>
    <span class="espace"></span>
    <div class="groupe-boutons" role="group" aria-label="Mode d'affichage">
      <button aria-pressed={mode === 'simple'} class:actif={mode === 'simple'} onclick={() => changerMode('simple')} title="Page simple">▯ Page</button>
      <button aria-pressed={mode === 'double'} class:actif={mode === 'double'} onclick={() => changerMode('double')} title="Double page (D)">▯▯ Double</button>
      <button aria-pressed={mode === 'defilement'} class:actif={mode === 'defilement'} onclick={() => changerMode('defilement')} title="Défilement vertical continu (V)">⇕ Défilement</button>
    </div>
    {#if mode === 'double'}
      <button aria-pressed={couvertureSeule} class:actif={couvertureSeule} onclick={() => (couvertureSeule = !couvertureSeule)} title="Couverture affichée seule en double page">Couv. seule</button>
    {/if}
    <select value={ajustement} onchange={(e) => changerAjustement(e.currentTarget.value as Ajustement)} aria-label="Ajustement" data-testid="ajustement">
      {#each AJUSTEMENTS as a (a.id)}<option value={a.id}>{a.titre}</option>{/each}
    </select>
    <button aria-pressed={manga} class:actif={manga} onclick={basculerManga} title="Sens manga, de droite à gauche (M)">⇐ Manga</button>
    <div class="groupe-boutons" role="group" aria-label="Zoom">
      <button onclick={() => void zoomer(zoom / 1.25)} title="Zoom arrière (−, Ctrl+molette)" aria-label="Zoom arrière">−</button>
      <button onclick={() => void zoomer(1)} title="Zoom 100 % (0)" data-testid="zoom">{Math.round(zoom * 100)} %</button>
      <button onclick={() => void zoomer(zoom * 1.25)} title="Zoom avant (+, Ctrl+molette)" aria-label="Zoom avant">+</button>
    </div>
    <button aria-pressed={loupe} class:actif={loupe} onclick={() => { loupe = !loupe; loupeVue = null; }} title="Loupe (L)" aria-label="Loupe">🔍</button>
    {#if loupe}
      <label class="outil" title="Grossissement de la loupe">×{grossissement}<input type="range" min="1.5" max="6" step="0.5" bind:value={grossissement} aria-label="Grossissement de la loupe" /></label>
    {/if}
    <button aria-pressed={miniatures} class:actif={miniatures} onclick={() => (miniatures = !miniatures)} title="Miniatures (T)" aria-label="Miniatures">▦</button>
    <button aria-pressed={pleinEcranActif} class:actif={pleinEcranActif} onclick={() => void basculerPleinEcran()} title="Plein écran (F, F11)" aria-label="Plein écran">⛶</button>
  </header>

  {#if livre && visibles.length > 0}
    <footer class="barre bas" class:cachee={!barres} {@attach survolBarre}>
      {#if miniatures}
        <div class="miniatures" bind:this={bandeau} dir={manga ? 'rtl' : 'ltr'} data-testid="miniatures">
          {#each visibles as index, k (index)}
            <button
              class="miniature"
              class:courante={positions.includes(k)}
              data-miniature={k}
              onclick={() => allerA(k)}
              title="Page {k + 1}"
              aria-label="Aller à la page {k + 1}"
              aria-current={positions.includes(k) ? 'page' : undefined}
            >
              <!-- Cadre fixe : une page tournée de 90/270° garde ses proportions (largeur et
                   hauteur échangées avant rotation) sans déborder (recette tauri-v0.3.0 E-03). -->
              <span class="cadre-miniature">
                <img src={url(index, 256)} alt="" loading="lazy" decoding="async" draggable="false"
                  class:quart={rotation(index) % 180 === 90}
                  style:transform="translate(-50%, -50%) rotate({rotation(index)}deg)" />
              </span>
              <span>{k + 1}</span>
            </button>
          {/each}
        </div>
      {/if}
      <div class="curseur">
        <button onclick={() => (manga && mode !== 'defilement' ? avancer() : reculer())} title={manga ? 'Page suivante' : 'Page précédente'} aria-label={manga ? 'Page suivante' : 'Page précédente'}>◀</button>
        <input
          type="range"
          min="1"
          max={visibles.length}
          value={position + 1}
          dir={manga && mode !== 'defilement' ? 'rtl' : 'ltr'}
          oninput={(e) => allerA(Number(e.currentTarget.value) - 1)}
          aria-label="Page"
          aria-valuetext={libellePage}
          data-testid="curseur-page"
        />
        <button onclick={() => (manga && mode !== 'defilement' ? reculer() : avancer())} title={manga ? 'Page précédente' : 'Page suivante'} aria-label={manga ? 'Page précédente' : 'Page suivante'}>▶</button>
      </div>
    </footer>
  {/if}
</div>

<style>
  /* Fond du lecteur : variable facultative --cr-fond-lecteur (repli sur --cr-fond). */
  /* overflow: clip (et non hidden) : le cadre n'est pas un conteneur défilable, aucun
     scrollIntoView ni focus ne peut le décaler (repli hidden + garde-fou cadreFixe). */
  .lecteur {
    position: fixed; inset: 0; z-index: 50; overflow: hidden; overflow: clip; outline: none;
    background: var(--cr-fond-lecteur, var(--cr-fond)); color: var(--cr-texte);
  }
  .lecteur.sans-curseur .scene { cursor: none; }
  .message { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 12px; color: var(--cr-texte-discret); margin: 0; }
  .message.erreur { color: var(--cr-erreur); }
  .reprise {
    position: absolute; z-index: 3; left: 50%; top: 50%; transform: translate(-50%, -50%);
    display: grid; gap: 8px; min-width: min(420px, calc(100vw - 32px)); padding: 18px 20px;
    background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon);
    box-shadow: 0 8px 32px rgb(0 0 0 / 0.4);
  }
  .reprise h2 { margin: 0; font-size: 18px; }
  .reprise p { margin: 0; }
  .reprise .nom { font-weight: 600; }
  .reprise button { text-align: left; }
  .reprise label { display: flex; align-items: center; gap: 6px; color: var(--cr-texte-discret); }
  .scene {
    position: absolute; inset: 0; overflow: auto; user-select: none; touch-action: none;
    scrollbar-color: var(--cr-bordure) transparent;
  }
  .scene.defilable { cursor: grab; }
  .scene.glisse { cursor: grabbing; }
  .scene.loupe-active { cursor: crosshair; }
  .plan { display: flex; align-items: center; justify-content: center; }
  .planche { display: flex; flex: none; }
  .colonne { position: relative; }
  .case { position: absolute; left: 50%; transform: translateX(-50%); }
  .page {
    position: relative; flex: none; overflow: hidden;
    background: color-mix(in srgb, var(--cr-fond-panneau) 60%, transparent);
  }
  .page.vide { background: color-mix(in srgb, var(--cr-fond-panneau) 35%, transparent); }
  .page img { position: absolute; left: 50%; top: 50%; display: block; max-width: none; }
  .attente, .illisible {
    position: absolute; inset: 0; display: grid; place-content: center; font-size: 15px; color: var(--cr-texte-discret);
  }
  .illisible { color: var(--cr-erreur); border: 1px solid var(--cr-erreur); margin: 8px; border-radius: var(--cr-rayon); }
  .prechargement { position: absolute; left: -10000px; top: 0; width: 1px; height: 1px; overflow: hidden; pointer-events: none; }
  .prechargement img { width: 1px; height: 1px; }
  .loupe {
    position: fixed; z-index: 3; overflow: hidden; pointer-events: none; border-radius: 50%;
    border: 3px solid var(--cr-accent); box-shadow: var(--cr-ombre); background: var(--cr-fond);
  }
  .loupe .page { position: absolute; background: none; }

  .barre {
    position: absolute; left: 0; right: 0; z-index: 2; display: flex; align-items: center; gap: 8px; padding: 6px 10px;
    background: color-mix(in srgb, var(--cr-fond-panneau) 92%, transparent); color: var(--cr-texte);
    box-shadow: var(--cr-ombre); transition: opacity 0.2s, transform 0.2s;
  }
  .barre.haut { top: 0; flex-wrap: wrap; border-bottom: 1px solid var(--cr-bordure); }
  .barre.bas { bottom: 0; flex-direction: column; align-items: stretch; border-top: 1px solid var(--cr-bordure); }
  .barre.haut.cachee { opacity: 0; transform: translateY(-100%); pointer-events: none; }
  .barre.bas.cachee { opacity: 0; transform: translateY(100%); pointer-events: none; }
  .titre { flex: 0 1 auto; min-width: 60px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .numero { color: var(--cr-texte-discret); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .espace { flex: 1 0 0; }
  .barre.haut button, .barre.haut select { padding: 4px 9px; }
  .outil { display: flex; align-items: center; gap: 4px; color: var(--cr-texte-discret); }
  .outil input { width: 80px; accent-color: var(--cr-accent); }
  .groupe-boutons { display: flex; }
  .groupe-boutons button { border-radius: 0; margin-left: -1px; }
  .groupe-boutons button:first-child { border-radius: var(--cr-rayon) 0 0 var(--cr-rayon); margin-left: 0; }
  .groupe-boutons button:last-child { border-radius: 0 var(--cr-rayon) var(--cr-rayon) 0; }
  button.actif { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  button.actif:hover:not(:disabled) { background: var(--cr-accent); }
  .curseur { display: flex; align-items: center; gap: 8px; }
  .curseur input { flex: 1; accent-color: var(--cr-accent); }
  .miniatures { display: flex; gap: 6px; overflow-x: auto; padding: 2px 2px 6px; scrollbar-color: var(--cr-bordure) transparent; }
  .miniature {
    flex: none; width: 64px; padding: 2px; display: flex; flex-direction: column; align-items: center; gap: 2px;
    background: var(--cr-fond); font-size: 11px; color: var(--cr-texte-discret);
  }
  .cadre-miniature { position: relative; display: block; width: 58px; height: 84px; overflow: hidden; background: var(--cr-fond-survol); }
  .cadre-miniature img { position: absolute; left: 50%; top: 50%; width: 58px; height: 84px; object-fit: contain; }
  .cadre-miniature img.quart { width: 84px; height: 58px; }
  .miniature.courante { border-color: var(--cr-accent); outline: 2px solid var(--cr-accent); color: var(--cr-texte); }
</style>
