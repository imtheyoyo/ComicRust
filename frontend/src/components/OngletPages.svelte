<script lang="ts">
  // Onglet « Pages » de la fenêtre « Infos du livre » (un seul livre), comme ui/info_pages.rs de la
  // version egui et l'onglet « Pages » de ComicRack (PagesView). Contrat : docs/tauri/ipc-pages.md.
  // - Vignettes (ou liste détaillée), sélection multiple (clic, Ctrl+clic, Maj+clic, flèches, Ctrl+A).
  // - Réordonner : glisser-déposer (événements pointeur : le glisser HTML5 est capté par Tauri sous
  //   Windows), boutons ◀ ▶ / Alt+← Alt+→, Mettre au début / à la fin (Ctrl+Alt+T / B).
  // - Type de page (Alt+Maj+A…K), rotation (Alt+Maj+6…9), position en double page, Supprimer /
  //   Restaurer (Suppr), favori (Ctrl+Maj+B / D), aperçu agrandi (Entrée, double-clic).
  // Toutes les transformations sont calculées par le backend (transformer_pages) ; l'enregistrement
  // se fait avec les champs (OK / Appliquer de la fenêtre, appliquer_modifications).
  import { onDestroy, tick } from 'svelte';
  import {
    fermerPages, lirePages, messageErreur, transformerPages, urlImagePage,
    type OperationPages, type PageLivre, type PagesLivre,
  } from '../lib/api';
  import MenuContextuel, { type EntreeMenu } from './MenuContextuel.svelte';

  interface Props {
    id: string;
    /** Chemin du fichier (copie de plusieurs pages : lignes « chemin | nom »). */
    fichier?: string;
    /** La fenêtre permet de modifier (pas d'enregistrement en cours, pas de confirmation). */
    actif: boolean;
    /** La liste a changé depuis lire_pages (● devant « Pages », OK / Appliquer). */
    modifie?: boolean;
  }
  let { id, fichier = '', actif, modifie = $bindable(false) }: Props = $props();

  const TAILLES = [{ h: 84, nom: 'Petites' }, { h: 124, nom: 'Moyennes' }, { h: 172, nom: 'Grandes' }];
  const CLE_PREFS = 'comicrust.ongletPages';
  const prefs = (() => {
    try { return JSON.parse(localStorage.getItem(CLE_PREFS) ?? '{}') as { vue?: string; taille?: number }; } catch { return {}; }
  })();

  let livre = $state<PagesLivre | null>(null);
  /** Liste reçue de lire_pages (référence de « modifié »). */
  let original = $state<PageLivre[]>([]);
  let pages = $state<PageLivre[]>([]);
  let chargement = $state(true);
  let erreur = $state('');
  let message = $state('');
  let occupe = $state(false);
  let selection = $state<Set<number>>(new Set());
  let ancre = $state<number | null>(null);
  /** Page qui a le « focus » clavier (image). */
  let curseur = $state<number | null>(null);
  let vue = $state<'vignettes' | 'details'>(prefs.vue === 'details' ? 'details' : 'vignettes');
  let taille = $state(Math.min(180, Math.max(64, prefs.taille ?? 124)));
  let menu = $state<{ x: number; y: number; entrees: EntreeMenu[] } | null>(null);
  let apercu = $state<number | null>(null);
  let favori = $state<{ image: number; texte: string } | null>(null);
  let liste = $state<HTMLDivElement>();
  let champFavori = $state<HTMLInputElement>();
  /** Glisser-déposer : départ, puis place d'insertion et trait à dessiner. */
  let glisse = $state<{ actif: boolean; x0: number; y0: number; image: number; mods: { ctrl: boolean; maj: boolean };
    cible: number | null; trait: { x: number; y: number; l: number; h: number } | null } | null>(null);
  let defilement: ReturnType<typeof setInterval> | undefined;
  let file: Promise<unknown> = Promise.resolve();

  $effect(() => {
    try { localStorage.setItem(CLE_PREFS, JSON.stringify({ vue, taille })); } catch { /* préférence locale seulement */ }
  });

  // Chargement pour le livre `id` (Précédent / Suivant : nouveau livre, séance remplacée).
  let idCharge = '';
  $effect(() => {
    const cible = id;
    if (cible !== idCharge) { idCharge = cible; void charger(); }
  });
  onDestroy(() => {
    clearInterval(defilement);
    if (livre) void fermerPages(livre.jeton).catch(() => {});
  });

  /** Lit (ou relit après « Appliquer ») les pages : nouvelle séance, vignettes relues du fichier. */
  export async function charger() {
    const ancien = livre?.jeton;
    const garder = livre?.id === id ? new Set(selection) : new Set<number>();
    if (livre && livre.id !== id) {
      // Autre livre (Précédent / Suivant) : rien de l'ancien ne doit pouvoir partir avec le nouveau.
      original = [];
      pages = [];
      livre = { ...livre, id, pages: [] };
    }
    chargement = true;
    erreur = '';
    try {
      const l = await lirePages(id, ancien);
      livre = l;
      original = l.pages;
      pages = l.pages;
      selection = new Set([...garder].filter((i) => i < l.nbImages));
      if (curseur !== null && curseur >= l.nbImages) curseur = null;
      apercu = null;
      favori = null;
    } catch (e) {
      erreur = messageErreur(e);
      livre = null;
      pages = [];
      original = [];
    } finally {
      chargement = false;
    }
  }

  /** Liste à enregistrer (complète, nouvel ordre), ou undefined si rien n'a changé ; attend la fin
   *  des opérations en file. */
  export async function pagesModifiees(): Promise<PageLivre[] | undefined> {
    await file;
    return modifie ? ($state.snapshot(pages) as PageLivre[]) : undefined;
  }

  const signature = (l: PageLivre[]) => l.map((p) => `${p.image}|${p.type}|${p.rotation}|${p.position}|${p.favori ?? ''}`).join('\n');
  $effect(() => { modifie = !!livre && signature(pages) !== signature(original); });

  // Pas de `occupe` ici : un bouton désactivé pendant l'opération perdrait le focus. Les opérations
  // sont mises en file (chacune part de la liste rendue par la précédente).
  const editable = $derived(!!livre?.modifiable && actif);
  const rangDe = $derived(new Map(pages.map((p, k) => [p.image, k])));
  /** Sélection dans l'ordre de la liste. */
  const choisies = $derived(pages.filter((p) => selection.has(p.image)));
  const imagesChoisies = $derived(choisies.map((p) => p.image));
  const nbChoisies = $derived(choisies.length);

  /** Valeur commune à la sélection (undefined : aucune ou différentes). */
  function commune<T>(f: (p: PageLivre) => T): T | undefined {
    if (choisies.length === 0) return undefined;
    const v = f(choisies[0]);
    return choisies.every((p) => f(p) === v) ? v : undefined;
  }

  /** `o` peut être une fonction évaluée à son tour dans la file (d'après la liste à jour). */
  async function operer(o: OperationPages | (() => OperationPages | null), apres?: () => void) {
    if (!editable || !livre) return;
    message = '';
    occupe = true;
    const avaitFocus = !!liste?.contains(document.activeElement);
    const suite = file.then(async () => {
      if (!editable) return;
      const operation = typeof o === 'function' ? o() : o;
      if (!operation) return;
      try {
        pages = await transformerPages($state.snapshot(pages) as PageLivre[], operation);
        // Une case déplacée perd le focus : il revient à la liste (raccourcis enchaînés).
        if (avaitFocus) await tick().then(() => { if (!liste?.contains(document.activeElement)) liste?.focus({ preventScroll: true }); });
        apres?.();
      } catch (e) {
        message = `⚠ ${messageErreur(e)}`;
      }
    });
    file = suite;
    await suite;
    if (file === suite) occupe = false;
  }

  const op = {
    type: (valeur: string) => operer({ op: 'type', images: imagesChoisies, valeur }),
    rotation: (valeur: number) => operer({ op: 'rotation', images: imagesChoisies, valeur }),
    position: (valeur: string) => operer({ op: 'position', images: imagesChoisies, valeur }),
    supprimer: () => operer({ op: 'basculerSupprimee', images: imagesChoisies }),
    debut: () => operer({ op: 'mettreAuDebut', images: imagesChoisies }, voirCurseur),
    fin: () => operer({ op: 'mettreALaFin', images: imagesChoisies }, voirCurseur),
    deplacer: (position: number) => operer({ op: 'deplacer', images: imagesChoisies, position }, voirCurseur),
    /** D'une place vers le début (-1) ou la fin (+1), la sélection gardée groupée. */
    decaler: (pas: -1 | 1) => operer(() => {
      if (choisies.length === 0) return null;
      const rangs = choisies.map((p) => rangDe.get(p.image)!);
      const pos = pas < 0 ? Math.max(0, Math.min(...rangs) - 1) : Math.min(pages.length, Math.max(...rangs) + 2);
      return { op: 'deplacer', images: imagesChoisies, position: pos };
    }, voirCurseur),
  };

  const peutAvant = $derived(nbChoisies > 0 && choisies.some((p, k) => rangDe.get(p.image) !== k));
  const peutApres = $derived(nbChoisies > 0 && choisies.some((p, k) => rangDe.get(p.image) !== pages.length - nbChoisies + k));

  // ───── Sélection ─────

  function cliquer(image: number, ctrl: boolean, maj: boolean) {
    if (maj) {
      const a = rangDe.get(ancre ?? image) ?? 0;
      const b = rangDe.get(image) ?? 0;
      const s = ctrl ? new Set(selection) : new Set<number>();
      for (const p of pages.slice(Math.min(a, b), Math.max(a, b) + 1)) s.add(p.image);
      selection = s;
    } else if (ctrl) {
      const s = new Set(selection);
      if (!s.delete(image)) s.add(image);
      selection = s;
      ancre = image;
    } else {
      selection = new Set([image]);
      ancre = image;
    }
    curseur = image;
    message = '';
  }

  const toutSelectionner = () => { selection = new Set(pages.map((p) => p.image)); };
  const inverser = () => { selection = new Set(pages.filter((p) => !selection.has(p.image)).map((p) => p.image)); };

  async function voirCurseur() {
    await tick();
    const img = curseur ?? choisies[0]?.image;
    if (img === undefined) return;
    liste?.querySelector<HTMLElement>(`[data-image="${img}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  /** Nombre de colonnes de la grille (pour ↑ ↓). */
  function colonnes(): number {
    if (vue === 'details' || !liste) return 1;
    const cases = [...liste.querySelectorAll<HTMLElement>('[data-image]')];
    if (cases.length < 2) return 1;
    const y = cases[0].offsetTop;
    const n = cases.findIndex((c) => c.offsetTop !== y);
    return n < 0 ? cases.length : n;
  }

  // ───── Clavier (raccourcis de ComicRack / egui) ─────

  /** Raccourcis de l'onglet (comme egui : actifs tant qu'aucun champ de saisie n'a le focus). */
  function clavier(e: KeyboardEvent) {
    if (!livre || pages.length === 0 || e.defaultPrevented) return;
    // Aperçu ou favori ouverts (même si le focus est resté sur la liste) : leur clavier d'abord.
    if (apercu !== null) { clavierApercu(e); return; }
    if (favori) { clavierFavori(e); return; }
    if ((e.target as HTMLElement).closest('input, select, textarea, [role="dialog"], [role="menu"]')) return;
    const dansListe = !!liste?.contains(e.target as Node);
    const ctrl = e.ctrlKey || e.metaKey;
    const code = e.code;
    // Alt+Maj+A…K : type ; Alt+Maj+6…9 : rotation.
    if (e.altKey && e.shiftKey && !ctrl) {
      const lettre = /^Key([A-Z])$/.exec(code)?.[1];
      const chiffre = /^(?:Digit|Numpad)([0-9])$/.exec(code)?.[1];
      const t = lettre && livre.types.find((c) => c.raccourci?.endsWith(`+${lettre}`));
      const r = chiffre && livre.rotations.find((c) => c.raccourci?.endsWith(`+${chiffre}`));
      if (t || r) {
        e.preventDefault();
        if (nbChoisies && t) void op.type(t.valeur);
        else if (nbChoisies && r) void op.rotation(Number(r.valeur));
        return;
      }
    }
    if (ctrl && e.altKey && (code === 'KeyT' || code === 'KeyB')) {
      e.preventDefault();
      if (nbChoisies) void (code === 'KeyT' ? op.debut() : op.fin());
      return;
    }
    if (ctrl && e.shiftKey && (code === 'KeyB' || code === 'KeyD')) {
      e.preventDefault();
      if (code === 'KeyB') demanderFavori();
      else if (nbChoisies === 1) void operer({ op: 'favori', image: imagesChoisies[0] });
      return;
    }
    if (e.altKey && !ctrl && (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      void op.decaler(e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 1);
      return;
    }
    if (ctrl && !e.altKey && e.key.toLowerCase() === 'a') { e.preventDefault(); toutSelectionner(); return; }
    if (ctrl && !e.altKey && e.key.toLowerCase() === 'c') { e.preventDefault(); void copier(); return; }
    if (e.key === 'Delete' && !ctrl && !e.altKey) { e.preventDefault(); if (nbChoisies) void op.supprimer(); return; }
    if (e.key === 'Enter' && dansListe && !ctrl && !e.altKey) { e.preventDefault(); ouvrirApercu(); return; }
    if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      e.preventDefault();
      const c = liste?.querySelector<HTMLElement>(`[data-image="${curseur ?? imagesChoisies[0] ?? pages[0].image}"]`)?.getBoundingClientRect();
      ouvrirMenu(c ? c.left + c.width / 2 : 100, c ? c.top + c.height / 2 : 100);
      return;
    }
    // Déplacement du curseur : dans la liste seulement (ailleurs, les flèches gardent leur rôle).
    if (!dansListe) return;
    const n = pages.length;
    const k = curseur !== null && rangDe.has(curseur) ? rangDe.get(curseur)! : -1;
    const cols = colonnes();
    let cible: number | null = null;
    switch (e.key) {
      case 'ArrowRight': cible = Math.min(n - 1, k + 1); break;
      case 'ArrowLeft': cible = Math.max(0, k - 1); break;
      case 'ArrowDown': cible = k < 0 ? 0 : Math.min(n - 1, k + cols); break;
      case 'ArrowUp': cible = Math.max(0, k - cols); break;
      case 'Home': cible = 0; break;
      case 'End': cible = n - 1; break;
      case 'PageDown': cible = Math.min(n - 1, Math.max(0, k) + cols * 3); break;
      case 'PageUp': cible = Math.max(0, k - cols * 3); break;
      case ' ':
        e.preventDefault();
        if (curseur !== null) cliquer(curseur, true, false);
        return;
      default: return;
    }
    e.preventDefault();
    const img = pages[cible].image;
    if (e.shiftKey) cliquer(img, ctrl, true);
    else if (ctrl) curseur = img; // Ctrl+flèche : déplace le curseur sans sélectionner (Espace coche)
    else cliquer(img, false, false);
    void voirCurseur();
  }

  // ───── Souris : clic, glisser-déposer ─────

  function appui(e: PointerEvent, image: number) {
    if (e.button !== 0) return;
    const ctrl = e.ctrlKey || e.metaKey;
    // Page non choisie, sans modificateur : choisie tout de suite (on peut la glisser aussitôt).
    if (!ctrl && !e.shiftKey && !selection.has(image)) cliquer(image, false, false);
    glisse = { actif: false, x0: e.clientX, y0: e.clientY, image, mods: { ctrl, maj: e.shiftKey }, cible: null, trait: null };
    liste?.setPointerCapture(e.pointerId);
    liste?.focus({ preventScroll: true });
  }

  function mouvement(e: PointerEvent) {
    if (!glisse) return;
    if (!glisse.actif) {
      if (Math.hypot(e.clientX - glisse.x0, e.clientY - glisse.y0) < 6 || !editable) return;
      glisse.actif = true;
      if (!selection.has(glisse.image)) cliquer(glisse.image, false, false);
      defilement = setInterval(defilerBords, 30);
    }
    dernierPointeur = { x: e.clientX, y: e.clientY };
    calculerCible(e.clientX, e.clientY);
  }

  let dernierPointeur = { x: 0, y: 0 };
  function defilerBords() {
    if (!liste || !glisse?.actif) return;
    const r = liste.getBoundingClientRect();
    const bord = 32;
    const y = dernierPointeur.y;
    if (y < r.top + bord) liste.scrollTop -= 12;
    else if (y > r.bottom - bord) liste.scrollTop += 12;
    else return;
    calculerCible(dernierPointeur.x, dernierPointeur.y);
  }

  /** Place d'insertion sous le pointeur : avant ou après la case survolée selon la moitié. */
  function calculerCible(x: number, y: number) {
    if (!glisse || !liste) return;
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-image]');
    const boite = liste.getBoundingClientRect();
    if (!el || !liste.contains(el)) {
      // Sous la dernière case : à la fin.
      const cases = liste.querySelectorAll<HTMLElement>('[data-image]');
      const derniere = cases[cases.length - 1];
      const rd = derniere?.getBoundingClientRect();
      if (rd && (y > rd.bottom || (y > rd.top && x > rd.right)) && y < boite.bottom) {
        glisse.cible = pages.length;
        glisse.trait = vue === 'details'
          ? { x: rd.left - boite.left, y: rd.bottom - boite.top + liste.scrollTop, l: rd.width, h: 3 }
          : { x: rd.right - boite.left + 2, y: rd.top - boite.top + liste.scrollTop, l: 3, h: rd.height };
      }
      return;
    }
    const r = el.getBoundingClientRect();
    const rang = rangDe.get(Number(el.dataset.image)) ?? 0;
    const apres = vue === 'details' ? y > r.top + r.height / 2 : x > r.left + r.width / 2;
    glisse.cible = rang + (apres ? 1 : 0);
    glisse.trait = vue === 'details'
      ? { x: r.left - boite.left, y: (apres ? r.bottom : r.top) - boite.top + liste.scrollTop - 1, l: r.width, h: 3 }
      : { x: (apres ? r.right + 2 : r.left - 5) - boite.left, y: r.top - boite.top + liste.scrollTop, l: 3, h: r.height };
  }

  function relache(e: PointerEvent) {
    const g = glisse;
    glisse = null;
    clearInterval(defilement);
    if (!g) return;
    if (liste?.hasPointerCapture(e.pointerId)) liste.releasePointerCapture(e.pointerId);
    if (g.actif) {
      if (g.cible !== null) void op.deplacer(g.cible);
      return;
    }
    // Simple clic ; double-clic (détecté ici : avec la capture du pointeur, `dblclick` arrive à la
    // liste et non à la case) : aperçu agrandi.
    const double = dernierClic?.image === g.image && e.timeStamp - dernierClic.t < 450 && !g.mods.ctrl && !g.mods.maj;
    dernierClic = double ? null : { image: g.image, t: e.timeStamp };
    if (g.mods.ctrl || g.mods.maj) cliquer(g.image, g.mods.ctrl, g.mods.maj);
    else cliquer(g.image, false, false);
    if (double) apercu = g.image;
  }
  let dernierClic: { image: number; t: number } | null = null;

  function annuleGlisse() {
    glisse = null;
    clearInterval(defilement);
  }

  function roulette(e: WheelEvent) {
    // Ctrl+molette : taille des vignettes (comme ComicRack).
    if (!(e.ctrlKey || e.metaKey) || vue !== 'vignettes') return;
    e.preventDefault();
    taille = Math.min(180, Math.max(64, taille - Math.sign(e.deltaY) * 12));
  }

  // ───── Menu contextuel ─────

  function menuSur(e: MouseEvent, image?: number) {
    e.preventDefault();
    if (image !== undefined && !selection.has(image)) cliquer(image, false, false);
    ouvrirMenu(e.clientX, e.clientY);
  }

  function ouvrirMenu(x: number, y: number) {
    if (!livre) return;
    const n = nbChoisies;
    const aucune = n === 0;
    const ro = !editable;
    const k = commune((p) => p.type);
    const r = commune((p) => p.rotation);
    const un = n === 1;
    const aFavori = un && !!choisies[0].favori;
    const entrees: EntreeMenu[] = [];
    if (!ro) {
      for (const t of livre.types) {
        entrees.push({ id: `type:${t.valeur}`, libelle: `${k === t.valeur ? '✓ ' : ''}Type : ${t.libelle}`, raccourci: t.raccourci, desactive: aucune });
      }
      livre.rotations.forEach((c, i) => {
        entrees.push({ id: `rotation:${c.valeur}`, libelle: `${r === Number(c.valeur) ? '✓ ' : ''}Rotation : ${c.libelle}`, raccourci: c.raccourci, desactive: aucune, separe: i === 0 });
      });
      entrees.push(
        { id: 'favori', libelle: 'Mettre en favoris…', raccourci: 'Ctrl+Maj+B', desactive: !un, separe: true },
        { id: 'sansFavori', libelle: 'Supprimer favoris', raccourci: 'Ctrl+Maj+D', desactive: !aFavori },
        { id: 'debut', libelle: 'Mettre au début', raccourci: 'Ctrl+Alt+T', desactive: aucune, separe: true },
        { id: 'fin', libelle: 'Mettre à la fin', raccourci: 'Ctrl+Alt+B', desactive: aucune },
        { id: 'origine', libelle: "Remettre dans l'ordre d'origine" },
      );
    }
    entrees.push(
      { id: 'apercu', libelle: '🔍 Aperçu agrandi', raccourci: 'Entrée', desactive: aucune, separe: true },
      { id: 'copier', libelle: '📋 Copier la page', raccourci: 'Ctrl+C', desactive: aucune },
      { id: 'tout', libelle: 'Tout sélectionner', raccourci: 'Ctrl+A', separe: true },
      { id: 'inverser', libelle: 'Inverser la sélection' },
      { id: 'rafraichir', libelle: '🔄 Rafraîchir' },
    );
    if (!ro) {
      const toutes = n > 0 && choisies.every((p) => p.supprimee);
      entrees.push({ id: 'supprimer', libelle: toutes ? '↺ Restaurer (ne plus marquer Supprimée)' : '✖ Marquer comme Supprimée', raccourci: 'Suppr', desactive: aucune, separe: true });
    }
    menu = { x, y, entrees };
  }

  function choixMenu(c: string) {
    const [quoi, valeur] = c.split(':');
    switch (quoi) {
      case 'type': void op.type(valeur); break;
      case 'rotation': void op.rotation(Number(valeur)); break;
      case 'favori': demanderFavori(); break;
      case 'sansFavori': void operer({ op: 'favori', image: imagesChoisies[0] }); break;
      case 'debut': void op.debut(); break;
      case 'fin': void op.fin(); break;
      case 'origine': void operer({ op: 'ordreOrigine' }); break;
      case 'apercu': ouvrirApercu(); break;
      case 'copier': void copier(); break;
      case 'tout': toutSelectionner(); break;
      case 'inverser': inverser(); break;
      case 'rafraichir': void charger(); break;
      case 'supprimer': void op.supprimer(); break;
      case 'ordreFichier': void operer({ op: 'ordreOrigine' }); break;
      case 'trierNom': void operer({ op: 'trierParNom', numerique: false }); break;
      case 'trierNumerique': void operer({ op: 'trierParNom', numerique: true }); break;
      case 'typesDefaut': void operer({ op: 'reinitialiserTypes' }); break;
      case 'toutReinitialiser': void operer({ op: 'toutReinitialiser' }); break;
    }
    if (quoi !== 'favori' && quoi !== 'apercu') liste?.focus({ preventScroll: true });
  }

  function menuReinitialiser(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: r.left, y: r.bottom + 2,
      entrees: [
        { id: 'ordreFichier', libelle: "Remettre dans l'ordre d'origine" },
        { id: 'trierNom', libelle: 'Trier par nom' },
        { id: 'trierNumerique', libelle: 'Trier par nom (numérique)' },
        { id: 'typesDefaut', libelle: 'Réinitialiser les types', separe: true },
        { id: 'toutReinitialiser', libelle: 'Tout réinitialiser (ordre, types, rotation, position, favoris)' },
      ],
    };
  }

  // ───── Favori, aperçu, copie ─────

  async function demanderFavori() {
    if (!editable || nbChoisies !== 1) return;
    const p = choisies[0];
    favori = { image: p.image, texte: p.favori || `Page ${(rangDe.get(p.image) ?? 0) + 1}` };
    await tick();
    champFavori?.focus();
    champFavori?.select();
  }

  function validerFavori(ok: boolean) {
    const f = favori;
    favori = null;
    liste?.focus({ preventScroll: true });
    if (ok && f && f.texte.trim()) void operer({ op: 'favori', image: f.image, texte: f.texte.trim() });
  }

  function ouvrirApercu() {
    const img = curseur !== null && selection.has(curseur) ? curseur : imagesChoisies[0] ?? curseur;
    if (img !== null && img !== undefined) apercu = img;
  }

  const pageApercu = $derived(apercu === null ? undefined : pages.find((p) => p.image === apercu));

  function clavierApercu(e: KeyboardEvent) {
    const k = apercu !== null ? rangDe.get(apercu) ?? 0 : 0;
    if (e.key === 'Escape' || e.key === 'Enter') { apercu = null; void tick().then(() => liste?.focus({ preventScroll: true })); }
    else if (e.key === 'ArrowRight' || e.key === 'PageDown' || e.key === ' ') apercu = pages[Math.min(pages.length - 1, k + 1)].image;
    else if (e.key === 'ArrowLeft' || e.key === 'PageUp') apercu = pages[Math.max(0, k - 1)].image;
    else if (e.key === 'Home') apercu = pages[0].image;
    else if (e.key === 'End') apercu = pages[pages.length - 1].image;
    else if (e.key === 'Delete' && editable) { const img = apercu!; void operer({ op: 'basculerSupprimee', images: [img] }); }
    else return;
    e.preventDefault();
    e.stopPropagation(); // Échap ferme l'aperçu, pas la fenêtre
    if (apercu !== null) { selection = new Set([apercu]); curseur = apercu; ancre = apercu; }
  }

  function clavierFavori(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); validerFavori(false); }
    else if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); validerFavori(true); }
  }

  async function copier() {
    if (!livre || nbChoisies === 0) return;
    try {
      if (nbChoisies > 1) {
        await navigator.clipboard.writeText(choisies.map((p) => `${fichier} | ${p.nom}`).join('\n'));
        message = `${nbChoisies} chemin(s) de page copié(s).`;
        return;
      }
      message = 'Copie de la page…';
      const reponse = await fetch(urlImagePage(livre, choisies[0], 0));
      const bitmap = await createImageBitmap(await reponse.blob());
      const toile = document.createElement('canvas');
      toile.width = bitmap.width;
      toile.height = bitmap.height;
      toile.getContext('2d')!.drawImage(bitmap, 0, 0);
      const png = await new Promise<Blob>((ok, ko) => toile.toBlob((b) => (b ? ok(b) : ko(new Error('image vide'))), 'image/png'));
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })]);
      message = `Page ${(rangDe.get(choisies[0].image) ?? 0) + 1} copiée dans le presse-papiers.`;
    } catch (e) {
      message = `Copie impossible : ${messageErreur(e)}`;
    }
  }

  // ───── Affichage ─────

  const libelleRotation = (r: number) => livre?.rotations.find((c) => Number(c.valeur) === r)?.libelle ?? `${r}°`;
  const libellePosition = (v: string) => livre?.positions.find((c) => c.valeur === v)?.libelle ?? v;
  const bulle = (p: PageLivre, k: number) =>
    `Page ${k + 1} — ${p.nom}\nType : ${p.libelleType}${p.rotation ? `\nRotation : ${libelleRotation(p.rotation)}` : ''}` +
    `${p.double ? '\nPage double' : ''}${p.position !== 'Default' ? `\nPosition en double page : ${libellePosition(p.position)}` : ''}` +
    `${p.favori ? `\nFavori : ${p.favori}` : ''}`;
  const largeurCase = $derived(Math.round(taille * 0.75 + 16));
  const typeCommun = $derived(commune((p) => p.type));
  const rotationCommune = $derived(commune((p) => p.rotation));
  const positionCommune = $derived(commune((p) => p.position));
  const toutesSupprimees = $derived(nbChoisies > 0 && choisies.every((p) => p.supprimee));
  const resumeSelection = $derived(
    nbChoisies === 0 ? 'Aucune page choisie'
      : nbChoisies === 1 ? `Page ${(rangDe.get(choisies[0].image) ?? 0) + 1} — ${choisies[0].nom}`
        : `${nbChoisies} pages choisies`,
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="onglet-pages" data-testid="onglet-pages" onkeydown={clavier}>
  <div class="entete">
    <span class="discret">
      {#if livre && !livre.modifiable}Pages du livre (consultation seulement) :
      {:else}Glisser-déposer, boutons ou menu contextuel (clic droit) pour changer l'ordre ou le type des pages :{/if}
    </span>
    <span class="espace"></span>
    <div class="groupe" role="group" aria-label="Affichage">
      <button class:actif={vue === 'vignettes'} aria-pressed={vue === 'vignettes'} onclick={() => (vue = 'vignettes')}>Vignettes</button>
      <button class:actif={vue === 'details'} aria-pressed={vue === 'details'} onclick={() => (vue = 'details')} data-testid="pages-details">Détails</button>
    </div>
    <select aria-label="Taille des vignettes (Ctrl+molette)" title="Taille des vignettes (Ctrl+molette)" disabled={vue !== 'vignettes'}
      value={TAILLES.reduce((a, t) => (Math.abs(t.h - taille) < Math.abs(a.h - taille) ? t : a)).h}
      onchange={(e) => (taille = Number(e.currentTarget.value))}>
      {#each TAILLES as t (t.h)}<option value={t.h}>{t.nom}</option>{/each}
    </select>
  </div>

  {#if livre?.lectureSeule}<p class="discret petit">🔒 {livre.lectureSeule}</p>{/if}

  {#if livre && livre.modifiable}
    <div class="outils" role="toolbar" aria-label="Pages choisies">
      <button disabled={!editable || !peutAvant} onclick={() => void op.decaler(-1)} title="Une place vers le début (Alt+←)" aria-label="Déplacer vers le début d'une place" data-testid="pages-avant">◀</button>
      <button disabled={!editable || !peutApres} onclick={() => void op.decaler(1)} title="Une place vers la fin (Alt+→)" aria-label="Déplacer vers la fin d'une place" data-testid="pages-apres">▶</button>
      <button disabled={!editable || !peutAvant} onclick={() => void op.debut()} title="Mettre au début (Ctrl+Alt+T)" data-testid="pages-debut">⇤ Au début</button>
      <button disabled={!editable || !peutApres} onclick={() => void op.fin()} title="Mettre à la fin (Ctrl+Alt+B)" data-testid="pages-fin">À la fin ⇥</button>
      <span class="separateur"></span>
      <label class="champ">Type
        <select disabled={!editable || nbChoisies === 0} value={typeCommun ?? ''} onchange={(e) => void op.type(e.currentTarget.value)} data-testid="pages-type">
          {#if typeCommun === undefined}<option value="" disabled>{nbChoisies > 1 ? '(valeurs multiples)' : '—'}</option>{/if}
          {#if typeCommun !== undefined && !livre.types.some((t) => t.valeur === typeCommun)}<option value={typeCommun}>{choisies[0]?.libelleType}</option>{/if}
          {#each livre.types as t (t.valeur)}<option value={t.valeur}>{t.libelle}</option>{/each}
        </select>
      </label>
      <label class="champ">Rotation
        <select disabled={!editable || nbChoisies === 0} value={rotationCommune === undefined ? '' : String(rotationCommune)} onchange={(e) => void op.rotation(Number(e.currentTarget.value))} data-testid="pages-rotation">
          {#if rotationCommune === undefined}<option value="" disabled>{nbChoisies > 1 ? '(multiples)' : '—'}</option>{/if}
          {#each livre.rotations as r (r.valeur)}<option value={r.valeur}>{r.libelle}</option>{/each}
        </select>
      </label>
      <label class="champ" title="Position de la page en affichage double page (Proche / Éloignée), comme le menu « Position de la page » de ComicRack">Double page
        <select disabled={!editable || nbChoisies === 0} value={positionCommune ?? ''} onchange={(e) => void op.position(e.currentTarget.value)} data-testid="pages-position">
          {#if positionCommune === undefined}<option value="" disabled>{nbChoisies > 1 ? '(multiples)' : '—'}</option>{/if}
          {#each livre.positions as c (c.valeur)}<option value={c.valeur}>{c.libelle}</option>{/each}
        </select>
      </label>
      <button disabled={!editable || nbChoisies === 0} onclick={() => void op.supprimer()} title="Marquer comme Supprimée / Restaurer (Suppr) : une page supprimée est masquée à la lecture" data-testid="pages-supprimer">
        {toutesSupprimees ? '↺ Restaurer' : '✖ Supprimer'}
      </button>
      <button disabled={!editable || nbChoisies !== 1} onclick={() => void demanderFavori()} title="Mettre en favoris (Ctrl+Maj+B)" aria-label="Favori">★</button>
      <button disabled={nbChoisies === 0} onclick={ouvrirApercu} title="Aperçu agrandi (Entrée, double-clic)" aria-label="Aperçu agrandi" data-testid="pages-apercu">🔍</button>
    </div>
  {/if}

  {#if erreur}
    <p class="erreur" role="alert">Fichier illisible : {erreur} <button class="mini" onclick={() => void charger()}>Réessayer</button></p>
  {:else if chargement && !livre}
    <p class="discret">Lecture des pages du fichier…</p>
  {:else if livre}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="liste" class:details={vue === 'details'} class:glisse={glisse?.actif} bind:this={liste}
      role="listbox" aria-multiselectable="true" aria-label="Pages du livre ({pages.length})" tabindex="0"
      aria-activedescendant={curseur !== null ? `page-${curseur}` : undefined}
      style:--h="{taille}px" style:--l="{largeurCase}px"
      onpointermove={mouvement} onpointerup={relache} onpointercancel={annuleGlisse} onwheel={roulette}
      oncontextmenu={(e) => menuSur(e)} onfocus={() => { if (curseur === null && pages.length) curseur = pages[0].image; }}
      data-testid="pages-liste">
      {#if vue === 'details'}
        <div class="ligne titre" aria-hidden="true">
          <span>N°</span><span>Image</span><span>Type</span><span>Rotation</span><span>Double page</span><span>Favori</span><span>Dimensions</span>
        </div>
      {/if}
      {#each pages as p, k (p.image)}
        {@const choisie = selection.has(p.image)}
        {#if vue === 'vignettes'}
          <div class="case" class:choisie class:curseur={curseur === p.image} class:supprimee={p.supprimee} id="page-{p.image}"
            role="option" tabindex="-1" aria-selected={choisie} aria-label="Page {k + 1}, {p.libelleType}{p.supprimee ? '' : ''}{p.favori ? ', favori' : ''}"
            title={glisse?.actif ? undefined : bulle(p, k)} data-image={p.image} data-rang={k}
            onpointerdown={(e) => appui(e, p.image)} oncontextmenu={(e) => { e.stopPropagation(); menuSur(e, p.image); }}>
            <span class="cadre">
              <img src={urlImagePage(livre, p)} alt="" loading="lazy" decoding="async" draggable="false"
                class:quart={p.rotation % 180 === 90} style:transform="translate(-50%, -50%) rotate({p.rotation}deg)" />
              {#if p.supprimee}<span class="croix" aria-hidden="true">✖</span>{/if}
              <span class="numero">{k + 1}</span>
              {#if p.double}<span class="badge double" title="Page double">⧉</span>{/if}
            </span>
            <span class="legende">{p.favori ? '★ ' : ''}{p.type !== 'Story' ? p.libelleType : ''}{p.position !== 'Default' ? ` · ${libellePosition(p.position)}` : ''}</span>
          </div>
        {:else}
          <div class="ligne" class:choisie class:curseur={curseur === p.image} class:supprimee={p.supprimee} id="page-{p.image}"
            role="option" tabindex="-1" aria-selected={choisie} title={glisse?.actif ? undefined : bulle(p, k)} data-image={p.image} data-rang={k}
            onpointerdown={(e) => appui(e, p.image)} oncontextmenu={(e) => { e.stopPropagation(); menuSur(e, p.image); }}>
            <span>{k + 1}</span><span class="nom">{p.nom}</span><span>{p.libelleType}</span><span>{p.rotation ? libelleRotation(p.rotation) : ''}</span>
            <span>{p.double ? '⧉ ' : ''}{p.position !== 'Default' ? libellePosition(p.position) : ''}</span><span class="nom">{p.favori ?? ''}</span>
            <span>{p.largeur && p.hauteur ? `${p.largeur} × ${p.hauteur}` : ''}</span>
          </div>
        {/if}
      {/each}
      {#if glisse?.actif && glisse.trait}
        <span class="trait" style:left="{glisse.trait.x}px" style:top="{glisse.trait.y}px" style:width="{glisse.trait.l}px" style:height="{glisse.trait.h}px" data-testid="pages-trait"></span>
      {/if}
    </div>
  {/if}

  <div class="pied">
    <span class="etat" aria-live="polite">
      {#if message}<span class:erreur={message.startsWith('⚠')}>{message}</span>
      {:else if modifie}<span class="touche">● Pages modifiées : enregistrées par OK ou Appliquer</span>
      {:else if livre}<span class="discret">{resumeSelection}</span>{/if}
    </span>
    <span class="espace"></span>
    {#if livre?.modifiable}
      <button disabled={!editable} onclick={() => void operer({ op: 'ordreOrigine' })} title="Remettre les pages dans l'ordre du fichier" data-testid="pages-reinitialiser">Réinitialiser</button>
      <button disabled={!editable} onclick={menuReinitialiser} title="Autres réinitialisations" aria-label="Autres réinitialisations" aria-haspopup="menu" data-testid="pages-reinitialiser-plus">▾</button>
    {/if}
  </div>

  {#if favori}
    <div class="sur-fenetre" role="presentation">
      <div class="boite" role="dialog" aria-modal="true" aria-labelledby="titre-favori" tabindex="-1" onkeydown={clavierFavori}>
        <h3 id="titre-favori">Favori</h3>
        <label for="champ-favori">Nom du favori de la page {(rangDe.get(favori.image) ?? 0) + 1} :</label>
        <input id="champ-favori" type="text" bind:this={champFavori} bind:value={favori.texte} data-testid="pages-favori" />
        <div class="boutons">
          <button class="principal" onclick={() => validerFavori(true)} disabled={!favori.texte.trim()}>OK</button>
          <button onclick={() => validerFavori(false)}>Annuler</button>
        </div>
      </div>
    </div>
  {/if}

  {#if pageApercu && livre}
    {@const k = rangDe.get(pageApercu.image) ?? 0}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="sur-fenetre apercu" role="dialog" aria-modal="true" aria-label="Aperçu de la page {k + 1}" tabindex="-1"
      onkeydown={clavierApercu} {@attach (n) => { n.focus(); }} data-testid="pages-apercu-grand">
      <div class="barre-apercu">
        <button onclick={() => (apercu = pages[Math.max(0, k - 1)].image)} disabled={k === 0} aria-label="Page précédente" title="Page précédente (←)">◀</button>
        <strong>Page {k + 1} / {pages.length}</strong>
        <span class="discret">{pageApercu.nom} · {pageApercu.libelleType}{pageApercu.rotation ? ` · ${libelleRotation(pageApercu.rotation)}` : ''}{pageApercu.double ? ' · page double' : ''}{pageApercu.favori ? ` · ★ ${pageApercu.favori}` : ''}</span>
        <button onclick={() => (apercu = pages[Math.min(pages.length - 1, k + 1)].image)} disabled={k === pages.length - 1} aria-label="Page suivante" title="Page suivante (→)">▶</button>
        <span class="espace"></span>
        <button onclick={() => { apercu = null; liste?.focus({ preventScroll: true }); }} aria-label="Fermer l'aperçu" title="Fermer (Échap)">✖</button>
      </div>
      <div class="image-apercu" class:quart={pageApercu.rotation % 180 === 90}>
        <img src={urlImagePage(livre, pageApercu, 1600)} alt="Page {k + 1}" draggable="false" class:supprimee={pageApercu.supprimee}
          style:transform="rotate({pageApercu.rotation}deg)" />
      </div>
    </div>
  {/if}

  {#if menu}
    <MenuContextuel x={menu.x} y={menu.y} entrees={menu.entrees} onchoix={choixMenu} onfermer={() => (menu = null)} />
  {/if}
</div>

<style>
  .onglet-pages { display: flex; flex-direction: column; gap: 6px; min-height: 0; height: 100%; position: relative; }
  .entete, .outils, .pied { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .espace { flex: 1; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; margin: 0; }
  .erreur { color: var(--cr-erreur); }
  .touche { color: var(--cr-accent); }
  .groupe { display: flex; }
  .groupe button:first-child { border-top-right-radius: 0; border-bottom-right-radius: 0; }
  .groupe button:last-child { border-top-left-radius: 0; border-bottom-left-radius: 0; margin-left: -1px; }
  button.actif { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  .outils button, .outils select { padding: 3px 8px; }
  .champ { display: inline-flex; align-items: center; gap: 4px; color: var(--cr-texte-discret); }
  .separateur { width: 1px; align-self: stretch; background: var(--cr-bordure); }
  .mini { padding: 2px 7px; }

  .liste {
    position: relative; flex: 1; min-height: 220px; overflow-y: auto; padding: 8px; outline: none;
    display: flex; flex-wrap: wrap; align-content: flex-start; gap: 8px;
    background: var(--cr-fond); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon);
    user-select: none; touch-action: none;
  }
  .liste:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 1px; }
  .liste.glisse { cursor: grabbing; }
  .case {
    position: relative; width: var(--l); height: calc(var(--h) + 24px); padding: 6px 8px 0; border-radius: var(--cr-rayon);
    display: flex; flex-direction: column; align-items: center; gap: 2px; border: 1px solid transparent; cursor: default;
  }
  .case:hover { background: var(--cr-fond-survol); }
  .case.choisie { background: color-mix(in srgb, var(--cr-accent) 28%, transparent); border-color: var(--cr-accent); }
  .case.curseur { outline: 1px dashed var(--cr-accent); outline-offset: 1px; }
  .cadre { position: relative; display: block; width: calc(var(--l) - 16px); height: calc(var(--h) - 2px); overflow: hidden; }
  .cadre img {
    position: absolute; left: 50%; top: 50%; width: calc(var(--l) - 16px); height: calc(var(--h) - 2px); object-fit: contain;
    filter: drop-shadow(0 0 1px var(--cr-texte-discret));
  }
  .cadre img.quart { width: calc(var(--h) - 2px); height: calc(var(--l) - 16px); }
  .supprimee .cadre img { opacity: 0.35; }
  .croix { position: absolute; inset: 0; display: grid; place-items: center; font-size: 28px; color: var(--cr-erreur); }
  .numero, .badge {
    position: absolute; top: 3px; right: 3px; padding: 0 4px; font-size: 11px; line-height: 15px; border-radius: 3px;
    background: color-mix(in srgb, var(--cr-texte) 80%, transparent); color: var(--cr-fond);
  }
  .badge.double { right: auto; left: 3px; }
  .legende { font-size: 11px; color: var(--cr-texte-discret); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .supprimee .legende { color: var(--cr-erreur); }

  .liste.details { flex-direction: column; flex-wrap: nowrap; gap: 0; padding: 0; }
  .ligne {
    display: grid; grid-template-columns: 44px minmax(120px, 2fr) minmax(110px, 1.4fr) 70px 100px minmax(80px, 1fr) 96px;
    gap: 8px; padding: 3px 8px; align-items: center; border-bottom: 1px solid color-mix(in srgb, var(--cr-bordure) 50%, transparent);
  }
  .ligne.titre { position: sticky; top: 0; z-index: 1; font-weight: 600; background: var(--cr-fond-panneau); }
  .ligne:not(.titre):hover { background: var(--cr-fond-survol); }
  .ligne.choisie { background: color-mix(in srgb, var(--cr-accent) 28%, transparent); }
  .ligne.curseur { outline: 1px dashed var(--cr-accent); outline-offset: -1px; }
  .ligne.supprimee { color: var(--cr-erreur); }
  .ligne .nom { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .trait { position: absolute; z-index: 2; background: var(--cr-accent); border-radius: 2px; pointer-events: none; }

  .sur-fenetre {
    position: absolute; inset: -4px; z-index: 20; display: grid; place-items: center;
    background: color-mix(in srgb, var(--cr-fond) 75%, transparent);
  }
  .boite {
    display: flex; flex-direction: column; gap: 8px; min-width: 320px; padding: 14px 16px;
    background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .boite h3 { margin: 0; font-size: 15px; }
  .boutons { display: flex; justify-content: flex-end; gap: 8px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  .apercu { display: flex; flex-direction: column; place-items: stretch; gap: 6px; padding: 6px; background: var(--cr-fond-panneau); outline: none; }
  .barre-apercu { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .image-apercu { flex: 1; min-height: 0; display: grid; place-items: center; overflow: hidden; container-type: size; }
  .image-apercu img { max-width: 100cqw; max-height: 100cqh; object-fit: contain; box-shadow: var(--cr-ombre); }
  .image-apercu.quart img { max-width: 100cqh; max-height: 100cqw; }
  .image-apercu img.supprimee { opacity: 0.4; }
</style>
