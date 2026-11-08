// MODE SIMULÉ (étape 4) : faux backend de l'onglet Pages (docs/tauri/ipc-pages.md), utilisé seulement
// hors de Tauri. Il imite comicrust-core/src/pages.rs (règles de ComicRack : déplacement, « Mettre à
// la fin » d'une couverture, réinitialisations, types) sur les livres factices de simule.ts, pour
// développer et tester le front. La logique réelle est dans le backend.
import type { ChoixPage, OperationPages, PageLivre, PageModifiee, PagesLivre } from './api';
import type { LivreSimule } from './simuleEdition';

const TYPES: ChoixPage[] = [
  ['FrontCover', 'Couverture avant', 'A'], ['InnerCover', 'Couverture intérieure', 'B'], ['Roundup', 'Résumé', 'C'],
  ['Story', 'Histoire', 'D'], ['Advertisement', 'Publicité', 'E'], ['Editorial', 'Éditorial', 'F'],
  ['Letters', 'Courrier des lecteurs', 'G'], ['Preview', 'Aperçu', 'H'], ['BackCover', 'Couverture arrière', 'I'],
  ['Other', 'Autre', 'J'], ['Deleted', 'Supprimée', 'K'],
].map(([valeur, libelle, raccourci]) => ({ valeur, libelle, raccourci: `Alt+Maj+${raccourci}` }));
const ROTATIONS: ChoixPage[] = [['0', 'Aucune', '6'], ['90', '90°', '7'], ['180', '180°', '8'], ['270', '270°', '9']]
  .map(([valeur, libelle, raccourci]) => ({ valeur, libelle, raccourci: `Alt+Maj+${raccourci}` }));
const POSITIONS: ChoixPage[] = [['Default', 'Par défaut'], ['Near', 'Proche'], ['Far', 'Éloignée']]
  .map(([valeur, libelle]) => ({ valeur, libelle }));

const libelle = (t: string) => t.split(',').map((x) => TYPES.find((c) => c.valeur === x.trim())?.libelle ?? x.trim()).join(', ');

/** Attributs enregistrés d'une page (ce que garde la fiche). */
interface PageFiche { image: number; type: string; rotation: number; position: string; favori?: string }

export interface ContextePages {
  livres: LivreSimule[];
  /** Raison de la lecture seule, ou undefined. */
  lectureSeule: () => string | undefined;
  cbz: (l: LivreSimule) => boolean;
  pause: (ms: number) => Promise<unknown>;
}

export function creerPagesSimulees(ctx: ContextePages) {
  /** Listes enregistrées (fiche), par livre. */
  const enregistrees = new Map<string, PageFiche[]>();
  /** Séances de vignettes ouvertes (jeton → livre) ; consultées par les tests. */
  const seances = new Map<string, string>();
  let numero = 0;

  const livre = (id: string) => {
    const l = ctx.livres[Number(id) - 1];
    if (!l) throw new Error('Livre introuvable.');
    return l;
  };
  const nomImage = (l: LivreSimule, i: number) => `${String(i + 1).padStart(3, '0')}.jpg`;
  const dims = (i: number) => {
    const double = i >= 6 && (i - 6) % 9 === 0;
    return i % 13 === 4 ? {} : { largeur: double ? 2000 : 1000, hauteur: 1500 };
  };

  /** Fiche d'origine des livres factices (mêmes particularités que le lecteur simulé). */
  function origine(l: LivreSimule): PageFiche[] {
    return Array.from({ length: l.pages }, (_, i) => ({
      image: i,
      type: i === 0 ? 'FrontCover' : i === 4 && Number(l.id) % 2 === 0 ? 'Deleted' : 'Story',
      rotation: l.id === '1' && i === 11 ? 180 : l.id === '3' ? ({ 1: 90, 2: 270 } as Record<number, number>)[i] ?? 0 : 0,
      position: 'Default',
    }));
  }
  const fiche = (l: LivreSimule) => enregistrees.get(l.id) ?? origine(l);

  function complete(l: LivreSimule, jeton: string, p: PageFiche, index: number): PageLivre {
    const type = p.type || 'Story';
    const d = dims(p.image);
    return {
      index, image: p.image, nom: nomImage(l, p.image), type, libelleType: libelle(type),
      supprimee: type.split(',').some((x) => x.trim() === 'Deleted'),
      double: !!d.largeur && d.largeur > d.hauteur!, rotation: p.rotation, position: p.position || 'Default',
      favori: p.favori || undefined, ...d, urlVignette: `simule://vignette/${jeton}/${p.image}?l=192`,
    };
  }
  const versFiche = (p: PageLivre | PageModifiee): PageFiche => ({
    image: p.image,
    type: p.supprimee === true ? 'Deleted' : p.supprimee === false && p.type === 'Deleted' ? 'Story' : p.type || 'Story',
    rotation: p.rotation ?? 0, position: p.position ?? 'Default', favori: p.favori || undefined,
  });

  function deplacer(l: PageFiche[], position: number, images: number[]): PageFiche[] {
    let pos = Math.min(position, l.length);
    const bouges = l.filter((p) => images.includes(p.image));
    const reste: PageFiche[] = [];
    l.forEach((p, k) => {
      if (images.includes(p.image)) { if (k < position) pos--; } else reste.push(p);
    });
    reste.splice(pos, 0, ...bouges);
    return reste;
  }

  function transformer(l: PageFiche[], o: OperationPages, noms: (i: number) => string): PageFiche[] {
    const chaque = (images: number[], f: (p: PageFiche) => Partial<PageFiche>) =>
      l.map((p) => (images.includes(p.image) ? { ...p, ...f(p) } : p));
    switch (o.op) {
      case 'deplacer': return deplacer(l, o.position, o.images);
      case 'mettreAuDebut': return deplacer(l, 0, o.images);
      case 'mettreALaFin': {
        const sel = l.filter((p) => o.images.includes(p.image));
        if (sel.length && sel.every((p) => p.type === 'FrontCover')) {
          l = chaque(o.images, () => ({ type: 'Other' }));
          if (sel.length < l.length) l[sel.length] = { ...l[sel.length], type: 'FrontCover' };
        }
        return deplacer(l, l.length, o.images);
      }
      case 'type': return chaque(o.images, () => ({ type: o.valeur }));
      case 'rotation': return chaque(o.images, () => ({ rotation: o.valeur }));
      case 'position': return chaque(o.images, () => ({ position: o.valeur }));
      case 'basculerSupprimee': return chaque(o.images, (p) => ({ type: p.type === 'Deleted' ? 'Story' : 'Deleted' }));
      case 'favori': return chaque([o.image], () => ({ favori: o.texte?.trim() || undefined }));
      case 'ordreOrigine': return [...l].sort((a, b) => a.image - b.image);
      case 'trierParNom': {
        const c = new Intl.Collator('fr', { numeric: o.numerique, sensitivity: 'base' });
        return [...l].sort((a, b) => (o.numerique ? c.compare(noms(a.image), noms(b.image)) : noms(a.image) < noms(b.image) ? -1 : noms(a.image) > noms(b.image) ? 1 : 0) || a.image - b.image);
      }
      case 'reinitialiserTypes': return l.map((p, k) => ({ ...p, type: k === 0 ? 'FrontCover' : 'Story' }));
      case 'toutReinitialiser':
        return [...l].sort((a, b) => a.image - b.image).map((p, k) => ({ image: p.image, type: k === 0 ? 'FrontCover' : 'Story', rotation: 0, position: 'Default' }));
    }
  }

  return {
    /** Ordre de lecture enregistré d'un livre (lecteur simulé), ou undefined s'il n'a pas changé. */
    ordre: (id: string) => enregistrees.get(id),
    seances,
    /** appliquer_modifications avec `pages` : vrai si la liste a changé. Erreur si incohérente. */
    enregistrer(id: string, pages: PageModifiee[]): boolean {
      const l = livre(id);
      const vues = new Set(pages.map((p) => p.image));
      if (pages.length !== l.pages || vues.size !== l.pages || pages.some((p) => p.image < 0 || p.image >= l.pages)) {
        throw new Error(`liste de pages incohérente : chaque image du fichier (0 à ${l.pages - 1}) doit y être une fois et une seule`);
      }
      const nouvelle = pages.map(versFiche);
      const avant = fiche(l);
      if (JSON.stringify(nouvelle) === JSON.stringify(avant)) return false;
      enregistrees.set(l.id, nouvelle);
      return true;
    },
    commande(commande: string, args: Record<string, unknown>): unknown {
      switch (commande) {
        case 'lire_pages': {
          const l = livre(String(args.id));
          if (!l.fichierPresent) throw new Error(`Fichier introuvable : ${l.fichier}`);
          if (args.remplace) seances.delete(String(args.remplace));
          const jeton = `v${++numero}`;
          seances.set(jeton, l.id);
          const ro = ctx.lectureSeule();
          const r: PagesLivre = {
            id: l.id, jeton, urlBase: `simule://vignette/${jeton}`, largeurVignette: 192, nbImages: l.pages,
            pages: fiche(l).map((p, k) => complete(l, jeton, p, k)), modifiable: !ro, lectureSeule: ro,
            ecrireDansFichierPossible: ctx.cbz(l), types: TYPES, rotations: ROTATIONS, positions: POSITIONS,
          };
          return ctx.pause(80).then(() => r);
        }
        case 'fermer_pages':
          seances.delete(String(args.jeton));
          return null;
        case 'transformer_pages': {
          const pages = args.pages as PageLivre[];
          if (pages.length === 0) return [];
          const jeton = pages[0].urlVignette.split('/')[3];
          const l = livre(seances.get(jeton) ?? '1');
          const res = transformer(pages.map(versFiche), args.operation as OperationPages, (i) => nomImage(l, i));
          return res.map((p, k) => complete(l, jeton, p, k));
        }
        default:
          return undefined;
      }
    },
  };
}
