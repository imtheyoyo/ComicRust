// Notes de version (markdown brut de la release GitHub) découpées en blocs simples pour un
// affichage SÛR : le texte est rendu par les gabarits Svelte (échappé), jamais par innerHTML.
// Seuls les éléments courants des CHANGELOG sont reconnus : titres (#), listes (-, *, 1.),
// paragraphes, **gras**, `code` et [liens](https://…). Le reste (HTML compris) reste du texte.

export type Segment =
  | { type: 'texte'; texte: string }
  | { type: 'gras'; texte: string }
  | { type: 'code'; texte: string }
  /** `url` présente seulement pour une page du dépôt (ouvrable) ; sinon le lien est affiché
   *  comme du texte, suivi de son `adresse` si elle est en http(s). */
  | { type: 'lien'; texte: string; url?: string; adresse?: string };

export type Bloc =
  | { type: 'titre'; niveau: number; segments: Segment[] }
  | { type: 'liste'; ordonnee: boolean; elements: Segment[][] }
  | { type: 'paragraphe'; segments: Segment[] };

/** Adresse acceptée pour un lien cliquable : http ou https uniquement. */
export function urlSure(url: string): string | undefined {
  try {
    const u = new URL(url.trim());
    // Seules les pages du dépôt peuvent être ouvertes (filtre du backend, maj_ouvrir_page) :
    // un autre lien reste affiché en texte, avec son adresse, au lieu d'un clic sans effet.
    return u.protocol === 'https:' && u.href.startsWith('https://github.com/imtheyoyo/ComicRust/') ? u.href : undefined;
  } catch {
    return undefined;
  }
}

/** Découpe une ligne en segments (gras, code, liens). */
export function segments(ligne: string): Segment[] {
  const res: Segment[] = [];
  const motif = /\*\*(.+?)\*\*|__(.+?)__|`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)/g;
  let pos = 0;
  for (let m = motif.exec(ligne); m; m = motif.exec(ligne)) {
    if (m.index > pos) res.push({ type: 'texte', texte: ligne.slice(pos, m.index) });
    if (m[1] !== undefined || m[2] !== undefined) res.push({ type: 'gras', texte: m[1] ?? m[2] });
    else if (m[3] !== undefined) res.push({ type: 'code', texte: m[3] });
    else {
      const url = urlSure(m[5]);
      const adresse = !url && /^https?:\/\//i.test(m[5].trim()) ? m[5].trim() : undefined;
      res.push({ type: 'lien', texte: m[4], url, adresse });
    }
    pos = m.index + m[0].length;
  }
  if (pos < ligne.length) res.push({ type: 'texte', texte: ligne.slice(pos) });
  return res;
}

/** Blocs des notes de version. */
export function blocsNotes(markdown: string): Bloc[] {
  const blocs: Bloc[] = [];
  let paragraphe: string[] = [];
  let liste: { ordonnee: boolean; elements: Segment[][] } | null = null;
  const finir = () => {
    if (paragraphe.length) blocs.push({ type: 'paragraphe', segments: segments(paragraphe.join(' ')) });
    paragraphe = [];
    if (liste) blocs.push({ type: 'liste', ...liste });
    liste = null;
  };
  for (const brute of markdown.replace(/\r\n?/g, '\n').split('\n')) {
    const ligne = brute.trimEnd();
    const titre = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*$/.exec(ligne);
    const puce = /^\s*(?:([-*+])|(\d+)[.)])\s+(.*)$/.exec(ligne);
    if (!ligne.trim()) {
      finir();
    } else if (titre) {
      finir();
      blocs.push({ type: 'titre', niveau: titre[1].length, segments: segments(titre[2]) });
    } else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(ligne)) {
      finir(); // ligne de séparation
    } else if (puce) {
      const ordonnee = puce[2] !== undefined;
      if (paragraphe.length || (liste && liste.ordonnee !== ordonnee)) finir();
      liste ??= { ordonnee, elements: [] };
      liste.elements.push(segments(puce[3]));
    } else if (liste && /^\s{2,}\S/.test(brute)) {
      // Suite d'un élément de liste sur la ligne suivante (indentée).
      const dernier = liste.elements[liste.elements.length - 1];
      dernier.push({ type: 'texte', texte: ' ' }, ...segments(ligne.trim()));
    } else {
      if (liste) finir();
      paragraphe.push(ligne.trim());
    }
  }
  finir();
  return blocs;
}
