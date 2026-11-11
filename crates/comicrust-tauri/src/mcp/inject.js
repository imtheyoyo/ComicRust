// Script de pilotage MCP injecté dans la page (variante ComicRust-Tauri-mcp uniquement).
// Idempotent : il est ajouté au chargement de la page et réévalué avant chaque outil.
// - journal : copie des messages de la console et des erreurs JavaScript ;
// - arbre : éléments visibles (rôle, libellé, valeur, position en pixels CSS, état) ;
// - recherche d'un élément par libellé, rôle, sélecteur CSS ou id.
(() => {
  if (window.__mcp) return;
  const journal = [];
  const texte = (a) => {
    if (a instanceof Error) return a.stack || a.message;
    if (a && typeof a === 'object') {
      try { return JSON.stringify(a); } catch { return String(a); }
    }
    return String(a);
  };
  const noter = (niveau, args) => {
    const heure = new Date().toISOString().slice(11, 23);
    journal.push(`${heure} [${niveau}] ${args.map(texte).join(' ')}`);
    if (journal.length > 1000) journal.shift();
  };
  for (const n of ['log', 'info', 'warn', 'error', 'debug']) {
    const original = console[n].bind(console);
    console[n] = (...args) => { noter(n, args); original(...args); };
  }
  addEventListener('error', (e) => noter('exception', [`${e.message} @ ${e.filename}:${e.lineno}`]));
  addEventListener('unhandledrejection', (e) => noter('promesse rejetée', [e.reason]));

  const INTERACTIF = 'a[href],button,input,select,textarea,summary,[role=button],[role=link],' +
    '[role=menuitem],[role=menuitemcheckbox],[role=tab],[role=option],[role=checkbox],[role=radio],' +
    '[role=switch],[role=treeitem],[role=slider],[role=gridcell],[tabindex]:not([tabindex="-1"]),' +
    '[contenteditable=""],[contenteditable=true],[draggable=true]';
  const STRUCTURE = 'h1,h2,h3,h4,img[alt],dialog,[role=dialog],[role=alert],[role=status],' +
    '[role=heading],[role=tooltip],progress,[role=progressbar]';

  const ids = new WeakMap();
  const parId = new Map();
  let suivant = 1;
  const idDe = (el) => {
    let id = ids.get(el);
    if (!id) {
      id = (suivant++).toString(36);
      ids.set(el, id);
    }
    parId.set(id, new WeakRef(el));
    return id;
  };

  const zone = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return null;
    if (r.bottom <= 0 || r.right <= 0 || r.top >= innerHeight || r.left >= innerWidth) return null;
    const s = getComputedStyle(el);
    if (s.visibility === 'hidden' || s.display === 'none' || Number(s.opacity) === 0) return null;
    return r;
  };
  const ROLES = {
    A: 'link', BUTTON: 'button', SELECT: 'combobox', TEXTAREA: 'textbox', H1: 'heading', H2: 'heading',
    H3: 'heading', H4: 'heading', IMG: 'img', DIALOG: 'dialog', SUMMARY: 'button', PROGRESS: 'progressbar',
  };
  const role = (el) => el.getAttribute('role') || ROLES[el.tagName] || (el.tagName === 'INPUT'
    ? ({ checkbox: 'checkbox', radio: 'radio', range: 'slider', button: 'button', submit: 'button' }[el.type] || 'textbox')
    : el.tagName.toLowerCase());
  const propre = (s) => (s || '').replace(/\s+/g, ' ').trim();
  /** Texte d'un élément sans celui des listes déroulantes et champs qu'il contient. */
  const texteVisible = (el) => {
    if (!el.querySelector('select,textarea,option')) return el.innerText || el.textContent;
    const morceaux = [];
    const parcours = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let n = parcours.nextNode(); n; n = parcours.nextNode()) {
      if (!n.parentElement.closest('select,textarea,option,script,style')) morceaux.push(n.textContent);
    }
    return morceaux.join(' ');
  };
  const libelle = (el) => {
    const l = el.getAttribute('aria-label') || (el.labels && el.labels[0] && texteVisible(el.labels[0])) ||
      el.getAttribute('alt') || el.getAttribute('title');
    if (l) return propre(l);
    if (['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)) return propre(el.getAttribute('placeholder'));
    return propre(texteVisible(el)).slice(0, 200);
  };
  const valeur = (el) => {
    if (el.tagName === 'SELECT') return el.options[el.selectedIndex]?.text || '';
    // Un champ mot de passe n'est jamais renvoyé au client MCP.
    if (el.tagName === 'INPUT' && el.type === 'password') return el.value ? '***' : '';
    if ((el.tagName === 'INPUT' && !['checkbox', 'radio', 'button', 'submit'].includes(el.type)) || el.tagName === 'TEXTAREA') return el.value;
    if (el.getAttribute('aria-valuenow')) return el.getAttribute('aria-valuenow');
    return '';
  };
  const etats = (el) => {
    const f = [];
    if (el.matches(INTERACTIF)) f.push('cliquable');
    if (el.disabled || el.getAttribute('aria-disabled') === 'true') f.push('désactivé');
    const coche = el.type === 'checkbox' || el.type === 'radio' ? el.checked : el.getAttribute('aria-checked');
    if (coche === true || coche === 'true') f.push('coché');
    else if (coche === false || coche === 'false') f.push('décoché');
    if (el.getAttribute('aria-selected') === 'true' || el.getAttribute('aria-current') && el.getAttribute('aria-current') !== 'false') f.push('sélectionné');
    if (el.getAttribute('aria-expanded') === 'true') f.push('ouvert');
    if (el.getAttribute('aria-expanded') === 'false') f.push('fermé');
    if (document.activeElement === el) f.push('focus');
    return f;
  };

  /** Éléments utiles visibles, dans l'ordre du document. */
  const elements = (tous) => {
    const sortie = [];
    const marquer = new Set();
    for (const el of document.body.querySelectorAll('*')) {
      const r = zone(el);
      if (!r) continue;
      let garde = el.matches(INTERACTIF) || el.matches(STRUCTURE);
      if (!garde) {
        // Texte « feuille » hors d'un élément interactif (compteurs, titres, messages…).
        const propreTexte = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
        garde = propreTexte && !el.closest(INTERACTIF) || (tous && el.children.length === 0);
      }
      if (!garde) continue;
      let profondeur = 0;
      for (let p = el.parentElement; p; p = p.parentElement) if (marquer.has(p)) profondeur++;
      marquer.add(el);
      sortie.push({ el, r, profondeur });
    }
    return sortie;
  };
  const ligne = ({ el, r, profondeur }) => {
    let s = '  '.repeat(Math.min(profondeur, 12)) + role(el);
    const l = libelle(el);
    const v = valeur(el);
    if (l) s += ` "${l.slice(0, 80)}"`;
    if (v) s += ` = "${String(v).slice(0, 80)}"`;
    s += ` @(${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}×${Math.round(r.height)})`;
    const f = etats(el);
    if (f.length) s += ` [${f.join(', ')}]`;
    return s + ` id=${idDe(el)}`;
  };

  const chercher = (q) => {
    if (q.id) {
      const el = parId.get(String(q.id))?.deref();
      if (!el || !el.isConnected) throw new Error(`aucun élément avec l'id ${q.id} (relancer ui_tree)`);
      return el;
    }
    let candidats;
    if (q.selector) {
      candidats = [...document.querySelectorAll(q.selector)].filter((e) => zone(e) || q.cache);
    } else {
      if (q.label === undefined) throw new Error('indiquer « label », « selector » ou « id »');
      const cible = propre(q.label).toLowerCase();
      const r = q.role ? String(q.role).toLowerCase() : null;
      candidats = elements(false)
        .map((x) => x.el)
        .filter((e) => !r || role(e).toLowerCase() === r)
        .map((e) => {
          const l = libelle(e).toLowerCase();
          const v = String(valeur(e)).toLowerCase();
          const exact = l === cible || (!l && v === cible);
          return { e, exact, ok: exact || l.includes(cible) || v.includes(cible) };
        })
        .filter((x) => x.ok)
        .sort((a, b) => (b.exact - a.exact) || (b.e.matches(INTERACTIF) - a.e.matches(INTERACTIF)))
        .map((x) => x.e);
    }
    const el = candidats[q.index || 0];
    if (!el) {
      const noms = elements(false).filter((x) => x.el.matches(INTERACTIF)).map((x) => libelle(x.el))
        .filter(Boolean).slice(0, 25).map((n) => `« ${n.slice(0, 40)} »`);
      throw new Error(`aucun élément « ${q.label ?? q.selector} »` +
        (noms.length ? `. Éléments cliquables visibles : ${noms.join(', ')}` : ''));
    }
    return el;
  };

  window.__mcp = {
    journal: (n, filtre) => {
      const f = (filtre || '').toLowerCase();
      return journal.filter((l) => !f || l.toLowerCase().includes(f)).slice(-n).join('\n');
    },
    arbre: (filtre, tous) => {
      const f = (filtre || '').toLowerCase();
      const lignes = elements(tous).map(ligne).filter((l) => !f || l.toLowerCase().includes(f));
      return `${lignes.length} élément(s) — coordonnées en pixels CSS (mêmes unités que click et screenshot).\n` +
        lignes.join('\n');
    },
    /** Centre d'un élément (amené à l'écran si besoin), pour un clic. */
    viser: (q) => {
      const el = chercher(q);
      let r = el.getBoundingClientRect();
      if (r.top < 0 || r.bottom > innerHeight || r.left < 0 || r.right > innerWidth) {
        el.scrollIntoView({ block: 'center', inline: 'center' });
        r = el.getBoundingClientRect();
      }
      return { x: r.left + r.width / 2, y: r.top + r.height / 2, desc: `${role(el)} « ${libelle(el).slice(0, 60)} »` };
    },
    present: (q) => {
      try { chercher(q); return true; } catch { return false; }
    },
    etat: () => {
      const dialogues = [...document.querySelectorAll('dialog[open],[role=dialog],[aria-modal=true]')]
        .filter((d) => zone(d))
        .map((d) => propre(d.getAttribute('aria-label') || d.querySelector('h1,h2,h3')?.innerText || libelle(d)).slice(0, 80));
      const alertes = [...document.querySelectorAll('[role=alert],[role=status]')]
        .filter((d) => zone(d)).map((d) => libelle(d)).filter(Boolean);
      const a = document.activeElement;
      return {
        url: location.href,
        titre: document.title,
        application: typeof window.__comicrust_etat === 'function' ? window.__comicrust_etat() : null,
        dialogues,
        alertes,
        focus: a && a !== document.body ? `${role(a)} « ${libelle(a).slice(0, 60)} »` : null,
        fenetre: { largeur: innerWidth, hauteur: innerHeight, ratio_pixels: devicePixelRatio },
        erreurs_recentes: journal.filter((l) => /\[(error|exception|promesse rejetée)\]/.test(l)).slice(-5),
      };
    },
  };
})();
