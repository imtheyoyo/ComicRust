// Logique d'affichage des formulaires de scripts (FormSpec), reprise de
// comicrust_core::scripting::forms : valeurs initiales, activation conditionnelle (`enabled_if`,
// héritée des conteneurs), validation (obligatoire, bornes, doublons). Le backend revalide et
// remet en forme la réponse (FormSpec::sanitize) : ceci ne sert qu'à guider l'utilisateur.
import type { ChoixBrut, ColonneTableScript, ControleScript, FormSpecScript } from './api';

export interface ChoixNormal { valeur: unknown; libelle: string }

/** Texte d'une valeur scalaire (comme `value_text`). */
export function texteValeur(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return v;
  return typeof v === 'object' ? JSON.stringify(v) : String(v);
}

/** Valeur « vraie » (comme `truthy`). */
export function vrai(v: unknown): boolean {
  if (v === null || v === undefined) return false;
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0;
  if (typeof v === 'string') return v.length > 0;
  if (Array.isArray(v)) return v.length > 0;
  return Object.keys(v as object).length > 0;
}

export function choixNormaux(liste: ChoixBrut[] | undefined): ChoixNormal[] {
  return (liste ?? []).map((c) => {
    if (Array.isArray(c)) return { valeur: c[0], libelle: texteValeur(c[1]) };
    if (c !== null && typeof c === 'object') {
      const o = c as { value?: unknown; label?: unknown };
      return { valeur: o.value ?? null, libelle: o.label !== undefined ? texteValeur(o.label) : texteValeur(o.value) };
    }
    return { valeur: c, libelle: texteValeur(c) };
  });
}

/** Rang du choix de valeur `v` (égalité, sinon même texte), -1 si absent. */
export function rangChoix(choix: ChoixNormal[], v: unknown): number {
  if (v === null || v === undefined) return -1;
  const i = choix.findIndex((c) => c.valeur === v);
  if (i >= 0) return i;
  const t = texteValeur(v);
  return choix.findIndex((c) => texteValeur(c.valeur) === t);
}

export const AVEC_VALEUR = new Set(['text', 'textarea', 'secret', 'checkbox', 'number', 'choice', 'radio', 'multichoice', 'list_edit', 'table', 'folder', 'file']);
export const estTexte = (c: ControleScript) => c.type === 'text' || c.type === 'textarea' || c.type === 'secret';
export const estSecret = (c: ControleScript) => c.type === 'secret' || (estTexte(c) && !!c.secret);

/** Colonnes d'un tableau éditable (titre seul ou objet). */
export function colonnesTable(c: ControleScript): Array<Required<Pick<ColonneTableScript, 'title' | 'type'>> & { choix: ChoixNormal[]; width?: number }> {
  return (c.columns ?? []).map((x) => {
    const o: ColonneTableScript = typeof x === 'string' ? { title: x } : (x as ColonneTableScript);
    return { title: texteValeur(o.title), type: o.type || 'text', choix: choixNormaux(o.choices), width: o.width };
  });
}

export function celluleVide(col: { type: string; choix: ChoixNormal[] }): unknown {
  if (col.type === 'checkbox') return false;
  if (col.type === 'number') return 0;
  if (col.type === 'choice') return col.choix[0]?.valeur ?? null;
  return '';
}

function arrondi(f: number, dec: number): number {
  const p = 10 ** Math.min(10, Math.max(0, dec));
  return Math.round(f * p) / p;
}

/** Valeur par défaut d'un contrôle (comme `default_value`). */
function defaut(c: ControleScript): unknown {
  switch (c.type) {
    case 'text': case 'textarea': case 'secret':
      return estSecret(c) ? '' : texteValeur(c.default);
    case 'checkbox':
      return vrai(c.default);
    case 'number': {
      let d = typeof c.default === 'number' ? c.default : Number(c.default ?? 0) || 0;
      if (typeof c.min === 'number') d = Math.max(d, c.min);
      if (typeof c.max === 'number') d = Math.min(d, c.max);
      return arrondi(d, c.decimals ?? 0);
    }
    case 'choice': case 'radio': {
      const ch = choixNormaux(c.choices);
      const i = rangChoix(ch, c.default);
      if (i >= 0) return ch[i].valeur;
      if (c.type === 'choice' && c.editable && c.default !== undefined && c.default !== null) return texteValeur(c.default);
      return c.type === 'radio' ? (ch[0]?.valeur ?? null) : null;
    }
    case 'multichoice': {
      const ch = choixNormaux(c.choices);
      const voulu = Array.isArray(c.default) ? c.default : [];
      return ch.filter((x) => voulu.some((w) => rangChoix([x], w) === 0)).map((x) => x.valeur);
    }
    case 'list_edit':
      return (Array.isArray(c.default) ? c.default : []).map(texteValeur);
    case 'table': {
      const cols = colonnesTable(c);
      return (Array.isArray(c.default) ? c.default : []).map((r) => cols.map((col, i) => (Array.isArray(r) && r[i] !== undefined ? r[i] : celluleVide(col))));
    }
    case 'folder': case 'file':
      return texteValeur(c.default);
    default:
      return null;
  }
}

/** Enfants d'un conteneur, dans l'ordre. */
export function enfants(c: ControleScript): ControleScript[] {
  if (c.type === 'group') return c.controls ?? [];
  if (c.type === 'columns') return (c.columns ?? []).flatMap((col) => (Array.isArray(col) ? (col as ControleScript[]) : []));
  if (c.type === 'tabs') return (c.tabs ?? []).flatMap((t) => t.controls ?? []);
  return [];
}

function parcourir(liste: ControleScript[], valeurs: Record<string, unknown>, parentActif: boolean, f: (c: ControleScript, actif: boolean) => void) {
  for (const c of liste) {
    const actif = parentActif && (!c.enabled_if || condition(c.enabled_if, valeurs));
    f(c, actif);
    parcourir(enfants(c), valeurs, actif, f);
  }
}

/** Condition `enabled_if` : « clé », « !clé », « clé=valeur », « clé!=valeur ». */
export function condition(texte: string, valeurs: Record<string, unknown>): boolean {
  const t = texte.trim();
  let cle: string;
  let negation = false;
  let egal: string | undefined;
  const k = t.indexOf('!=');
  if (k >= 0) { cle = t.slice(0, k).trim(); negation = true; egal = t.slice(k + 2).trim(); }
  else if (t.includes('=')) { const j = t.indexOf('='); cle = t.slice(0, j).trim(); egal = t.slice(j + 1).trim(); }
  else if (t.startsWith('!')) { cle = t.slice(1).trim(); negation = true; }
  else cle = t;
  const v = valeurs[cle];
  const r = egal !== undefined ? texteValeur(v) === egal : vrai(v);
  return r !== negation;
}

/** Valeurs initiales : défauts remplacés par `values` du script (jamais pour un secret). */
export function valeursInitiales(spec: FormSpecScript): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  parcourir(spec.controls ?? [], {}, true, (c) => {
    if (!c.key || !AVEC_VALEUR.has(c.type)) return;
    const donne = spec.values?.[c.key];
    out[c.key] = donne !== undefined && !estSecret(c) ? convertir(c, donne) ?? defaut(c) : defaut(c);
  });
  return out;
}

/** Conversion d'une valeur donnée par le script dans la forme du contrôle (sinon undefined). */
function convertir(c: ControleScript, v: unknown): unknown {
  switch (c.type) {
    case 'checkbox': return vrai(v);
    case 'number': {
      const f = typeof v === 'number' ? v : Number(String(v).replace(',', '.'));
      return Number.isFinite(f) ? arrondi(f, c.decimals ?? 0) : undefined;
    }
    case 'choice': case 'radio': {
      const ch = choixNormaux(c.choices);
      const i = rangChoix(ch, v);
      if (i >= 0) return ch[i].valeur;
      if (v === null) return null;
      return c.type === 'choice' && c.editable ? texteValeur(v) : undefined;
    }
    case 'multichoice': {
      if (!Array.isArray(v)) return undefined;
      const ch = choixNormaux(c.choices);
      return ch.filter((x) => v.some((w) => rangChoix([x], w) === 0)).map((x) => x.valeur);
    }
    case 'list_edit': return Array.isArray(v) ? v.filter((x) => x !== null).map(texteValeur) : undefined;
    case 'table': {
      if (!Array.isArray(v)) return undefined;
      const cols = colonnesTable(c);
      return v.map((r) => cols.map((col, i) => (Array.isArray(r) && r[i] !== undefined ? r[i] : celluleVide(col))));
    }
    default: return typeof v === 'object' && v !== null ? undefined : texteValeur(v);
  }
}

/** Clés des contrôles activés. */
export function clesActives(spec: FormSpecScript, valeurs: Record<string, unknown>): Set<string> {
  const out = new Set<string>();
  parcourir(spec.controls ?? [], valeurs, true, (c, actif) => { if (actif && c.key) out.add(c.key); });
  return out;
}

/** Contrôles désactivés (y compris conteneurs sans clé), repérés par l'objet du contrôle. */
export function controlesInactifs(spec: FormSpecScript, valeurs: Record<string, unknown>): Set<ControleScript> {
  const out = new Set<ControleScript>();
  parcourir(spec.controls ?? [], valeurs, true, (c, actif) => { if (!actif) out.add(c); });
  return out;
}

const nombre = (f: number) => (Number.isInteger(f) ? String(f) : String(f).replace('.', ','));

/** Erreurs de saisie (contrôles activés seulement), clé → message (comme `validate`). */
export function valider(spec: FormSpecScript, valeurs: Record<string, unknown>): Record<string, string> {
  const err: Record<string, string> = {};
  parcourir(spec.controls ?? [], valeurs, true, (c, actif) => {
    if (!actif || !c.key || !AVEC_VALEUR.has(c.type)) return;
    const v = valeurs[c.key];
    if (estTexte(c)) {
      if (c.required && !estSecret(c) && !texteValeur(v).trim()) err[c.key] = 'champ obligatoire';
    } else if (c.type === 'folder' || c.type === 'file') {
      if (c.required && !texteValeur(v).trim()) err[c.key] = 'chemin obligatoire';
    } else if (c.type === 'number') {
      if (typeof v !== 'number' || !Number.isFinite(v)) err[c.key] = 'nombre attendu';
      else if (typeof c.min === 'number' && v < c.min) err[c.key] = `au moins ${nombre(c.min)}`;
      else if (typeof c.max === 'number' && v > c.max) err[c.key] = `au plus ${nombre(c.max)}`;
    } else if (c.type === 'choice') {
      if (c.required && !texteValeur(v).trim()) err[c.key] = 'choix obligatoire';
    } else if (c.type === 'multichoice') {
      if (c.required && !vrai(v)) err[c.key] = 'cocher au moins un élément';
    } else if (c.type === 'list_edit') {
      const l = Array.isArray(v) ? v.map(texteValeur) : [];
      if (c.required && l.length === 0) err[c.key] = 'liste vide';
      else if (c.unique !== false) {
        const vus = new Set<string>();
        const d = l.find((x) => { const k = x.toLowerCase(); if (vus.has(k)) return true; vus.add(k); return false; });
        if (d !== undefined) err[c.key] = `« ${d} » en double`;
      }
    }
  });
  return err;
}

/** Boutons du bas (OK / Annuler par défaut) et rang du bouton d'annulation. */
export function boutons(spec: FormSpecScript): { liste: string[]; annuler: number } {
  let liste = (spec.buttons ?? []).map(texteValeur).filter((b) => b.trim());
  if (liste.length === 0) liste = ['OK', 'Annuler'];
  let annuler: number;
  if (spec.cancel === undefined) annuler = liste.length > 1 ? liste.length - 1 : -1;
  else if (spec.cancel === null) annuler = -1;
  else annuler = liste.indexOf(texteValeur(spec.cancel));
  return { liste, annuler };
}

/** Indication sous un champ texte (comme `text_hint`) : [correspond, texte] ou null. */
export function indication(c: ControleScript, texte: string): [boolean, string] | null {
  const t = texte.trim();
  const hints = c.hints ?? [];
  if (!t || (hints.length === 0 && !c.hint_none)) return null;
  for (const [motif, txt] of hints) {
    try {
      if (new RegExp(motif).test(t)) return [true, texteValeur(txt)];
    } catch {
      /* expression non comprise par le moteur JS : ignorée (le backend l'a acceptée) */
    }
  }
  return c.hint_none ? [false, c.hint_none] : null;
}

/** Groupes du menu « Insérer » (groupes titrés et choix hors groupe). */
export function groupesInsertion(c: ControleScript): Array<{ titre: string; items: ChoixNormal[] }> {
  const out: Array<{ titre: string; items: ChoixNormal[] }> = [];
  for (const brut of c.inserts ?? []) {
    if (brut && typeof brut === 'object' && !Array.isArray(brut) && 'items' in brut) {
      const g = brut as { group?: string; items: ChoixBrut[] };
      out.push({ titre: texteValeur(g.group), items: choixNormaux(g.items) });
    } else {
      const items = choixNormaux([brut as ChoixBrut]);
      const der = out[out.length - 1];
      if (der && der.titre === '') der.items.push(...items);
      else out.push({ titre: '', items });
    }
  }
  return out.filter((g) => g.items.length > 0);
}
