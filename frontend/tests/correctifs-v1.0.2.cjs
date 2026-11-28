// Tests Playwright (mode simulé) des correctifs front de la tauri-v1.0.2 (relecture du lot front) :
// B1 sélection suivie par identifiant après relecture, B2 focus piégé dans le lecteur, B3 « Sélectionner
// tout le groupe » en un appel, B4 compteur de la barre latérale, B5 relecture sans squelettes, B6 arbre
// des listes jamais écrasé par une réponse périmée, B7 mois hors 1..12, B8 démarrages concurrents des
// manquants, B9 Précédent / Suivant bloqués pendant le chargement, B10 rôles ARIA.
//
// Usage : `npx vite --port 5391` dans frontend/, puis
//   node tests/correctifs-v1.0.2.cjs [http://localhost:5391] [chemin du module playwright]
// Navigateur : celui de Playwright, ou `CHROME=/usr/bin/google-chrome` pour un Chrome installé.
const base = process.argv[2] ?? 'http://localhost:5391';
const { chromium } = require(process.argv[3] ?? '/home/claude/.npm-global/lib/node_modules/playwright');

let echecs = 0;
function ok(cond, msg) {
  if (!cond) { echecs++; console.log('  ÉCHEC :', msg); } else console.log('  ok :', msg);
}

async function ouvrir(browser, query = '?n=5000') {
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  page.on('pageerror', (e) => { echecs++; console.log('  ERREUR PAGE :', e.message); });
  await page.goto(`${base}/${query}`);
  await page.waitForSelector('[data-testid=grille] [data-index]');
  await page.waitForTimeout(500);
  return page;
}
const titreDetails = (page) => page.locator('[data-testid=details] h2').innerText();
const titreCellule = (page, i) => page.locator(`[data-testid=grille] [data-index="${i}"] .titre`).innerText();

async function menuLivre(page, sel, entree) {
  await page.locator(sel).click({ button: 'right' });
  await page.waitForSelector('[data-testid=menu-contextuel]');
  await page.locator(`[data-testid=menu-contextuel] [data-menu=${entree}]`).click();
}

async function testB1Marquer(browser) {
  console.log('B1 : « Marquer comme lu » dans une vue triée par avancement');
  const page = await ouvrir(browser);
  await page.selectOption('[data-testid=tri]', 'avancement');
  await page.waitForTimeout(800);
  await page.locator('[data-testid=grille] [data-index="2"]').click();
  await page.waitForTimeout(300);
  const avant = await titreDetails(page);
  await menuLivre(page, '[data-testid=grille] [data-index="2"]', 'marquerLu');
  await page.waitForTimeout(1500);
  ok((await titreDetails(page)) === avant, `le livre courant reste « ${avant} » (et non le livre arrivé à son rang)`);
  // Ctrl+A puis relecture : toujours tout sélectionné.
  await page.locator('[data-testid=grille]').focus();
  await page.keyboard.press('Control+a');
  await page.evaluate(() => window.__comicrustSimule.emettre('bibliotheque-modifiee', { origine: 'externe', ids: [] }));
  await page.waitForTimeout(1000);
  ok(/5 000 sélectionnés|5 000 sélectionnés/.test(await page.locator('[data-testid=compteur]').innerText()), 'Ctrl+A reste « tout » après relecture');
  await page.close();
}

async function testB1Infos(browser) {
  console.log('B1 : Infos du livre ▸ Suivant après changement de série');
  const page = await ouvrir(browser);
  const t = [];
  for (let i = 0; i < 4; i++) t.push(await titreCellule(page, i));
  await page.locator('[data-testid=grille] [data-index="1"]').click();
  await page.keyboard.press('Control+i');
  await page.waitForSelector('[data-testid=dialogue-infos] [data-champ="Series"]');
  await page.waitForTimeout(400);
  await page.locator('[data-testid=dialogue-infos] [data-champ="Series"]').first().fill('ZZZ fin de liste');
  await page.waitForTimeout(500);
  await page.getByRole('button', { name: 'Suivant ▶' }).click();
  await page.waitForTimeout(1500);
  const fenetre = (await page.locator('#titre-infos').innerText()).replace(/^Infos — /, '');
  const vue = await titreDetails(page);
  ok(fenetre === t[2], `Suivant ouvre le livre qui suivait (« ${fenetre} » / attendu « ${t[2]} »)`);
  ok(fenetre === vue, `fenêtre et vue sur le même livre (« ${fenetre} » / « ${vue} »)`);
  // Le livre modifié est parti en fin de vue : Précédent va au livre qui précède MAINTENANT.
  await page.getByRole('button', { name: '◀ Précédent' }).click();
  await page.waitForTimeout(1200);
  const prec = (await page.locator('#titre-infos').innerText()).replace(/^Infos — /, '');
  ok(prec === t[0] && prec === (await titreDetails(page)), `Précédent : livre précédent dans la vue, fenêtre et vue d'accord (« ${prec} »)`);
  await page.close();
}

async function testB2(browser) {
  console.log('B2 : focus piégé dans le lecteur');
  const page = await ouvrir(browser);
  await page.locator('[data-testid=grille] [data-index="0"]').dblclick();
  await page.waitForSelector('[data-testid=lecteur] [data-testid=scene]');
  // Le bandeau de mise à jour (vérification en arrière-plan) peut apparaître PENDANT la lecture.
  await page.waitForSelector('[data-testid=bandeau-maj-installer]', { timeout: 5000 }).catch(() => {});
  await page.waitForTimeout(300);
  let hors = 0;
  for (let k = 0; k < 40; k++) {
    await page.keyboard.press('Tab');
    if (!(await page.evaluate(() => !!document.activeElement?.closest('[data-testid=lecteur]') || document.activeElement === document.body))) hors++;
  }
  ok(hors === 0, `aucun focus hors du lecteur en 40 Tab (${hors})`);
  const detailsAvant = await page.locator('[data-testid=details]').count();
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=lecteur]', { state: 'detached' });
  await page.waitForTimeout(300);
  ok((await page.locator('[data-testid=details]').count()) === detailsAvant, 'aucun bouton caché activé');
  ok((await page.evaluate(() => document.querySelectorAll('[inert]').length)) === 0, 'plus rien d\'inerte une fois le lecteur fermé');
  await page.close();
}

async function testB3(browser) {
  console.log('B3 : « Sélectionner tout l\'éditeur » loin du début du groupe (100 000 livres)');
  const page = await ouvrir(browser, '?n=100000');
  await page.selectOption('[data-testid=groupe]', 'editeur');
  await page.waitForTimeout(1000);
  await page.getByRole('button', { name: '☰ Tableau' }).click();
  await page.waitForSelector('[data-testid=tableau] [data-index]');
  await page.locator('[data-testid=tableau]').evaluate((z) => (z.scrollTop = 9000 * 40));
  await page.waitForTimeout(1200);
  const ligne = page.locator('[data-testid=tableau] [data-index="9005"]');
  await ligne.click();
  const t0 = Date.now();
  await menuLivre(page, '[data-testid=tableau] [data-index="9005"]', 'groupe');
  await page.waitForFunction(() => /sélectionnés/.test(document.querySelector('[data-testid=compteur]').textContent), null, { timeout: 120000 });
  const duree = Date.now() - t0;
  const texte = await page.locator('[data-testid=compteur]').innerText();
  ok(duree < 3000, `sélection en ${duree} ms (avant : 71 s)`);
  ok(/9 669 sélectionnés|9 669 sélectionnés/.test(texte), `tout le groupe (${texte.trim()})`);
  await page.close();
}

async function testB4B5(browser) {
  console.log('B4 / B5 : compteur avec une pile ouverte ; relecture sans squelettes');
  const page = await ouvrir(browser);
  // B5 : défilé à 60 %, « Marquer comme lu » : la zone visible ne repasse jamais en squelettes.
  await page.locator('[data-testid=grille]').evaluate((z) => (z.scrollTop = z.scrollHeight * 0.6));
  await page.waitForTimeout(800);
  const cell = page.locator('[data-testid=grille] [data-index]').nth(8);
  await cell.click();
  await page.evaluate(() => {
    window.__maxSq = 0;
    const z = document.querySelector('[data-testid=grille]');
    new MutationObserver(() => { window.__maxSq = Math.max(window.__maxSq, z.querySelectorAll('.squelette').length); })
      .observe(z, { subtree: true, childList: true });
  });
  await cell.click({ button: 'right' });
  await page.waitForSelector('[data-testid=menu-contextuel]');
  await page.locator('[data-testid=menu-contextuel] [data-menu=marquerLu]').click();
  await page.waitForTimeout(1500);
  ok((await page.evaluate(() => window.__maxSq)) === 0, `aucun squelette pendant la relecture (${await page.evaluate(() => window.__maxSq)})`);
  // B4
  const nb = () => page.evaluate(() => window.__comicrust_etat().nbLivres);
  ok((await nb()) === 5000, 'compteur : 5 000 au départ');
  await page.locator('[data-testid=empiler]').click();
  await page.waitForTimeout(800);
  await page.locator('[data-testid=grille-piles]').focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid=barre-pile]');
  await page.waitForTimeout(800);
  ok((await nb()) === 5000, `pile ouverte : la barre latérale garde 5 000 (${await nb()})`);
  await page.close();
}

async function testB6B7B8(browser) {
  console.log('B6 / B7 / B8 : réponses périmées de l\'arbre, mois, démarrages concurrents');
  const page = await ouvrir(browser);
  const r = await page.evaluate(async () => {
    const s = window.__comicrustSimule;
    const { EtatListes } = await import('/src/lib/listes.svelte.ts');
    const { creerListe } = await import('/src/lib/api.ts');
    const l = new EtatListes();
    await l.charger();
    s.retardsApres.arbre_listes = [500]; // arbre calculé AVANT la création, reçu APRÈS
    const lent = l.charger();
    await new Promise((res) => setTimeout(res, 60));
    const cree = await l.executer('Création', () => creerListe({ type: 'lecture', nom: 'Liste B6', livres: [] }));
    await lent;
    const trouve = (n) => (n ?? []).some((x) => x.nom === 'Liste B6' || trouve(x.enfants));
    const b6 = !!cree && trouve(l.arbre?.listes);
    const { parution } = await import('/src/lib/colonnes.ts');
    const b7 = [parution({ annee: 1991, mois: 13 }), parution({ annee: 1991, mois: 3 }), parution({ annee: 1991, mois: -1 })];
    const { manquants } = await import('/src/lib/manquants.svelte.ts');
    const avant = s.ecoutes['bibliotheque-modifiee'] ?? 0;
    await Promise.all([manquants.demarrer(false), manquants.demarrer(false)]);
    const apres = s.ecoutes['bibliotheque-modifiee'] ?? 0;
    return { b6, b7, avant, apres };
  });
  ok(r.b6, 'B6 : la liste créée reste dans l\'arbre malgré la réponse périmée');
  ok(JSON.stringify(r.b7) === JSON.stringify(['1991', '03/1991', '1991']), `B7 : mois hors 1..12 ignoré (${r.b7.join(', ')})`);
  ok(r.apres === r.avant, `B8 : deux démarrages = une seule écoute (${r.avant} → ${r.apres})`);
  await page.close();
}

async function testB9(browser) {
  console.log('B9 : double clic sur Suivant');
  const page = await ouvrir(browser);
  await page.locator('[data-testid=grille] [data-index="0"]').click();
  await page.keyboard.press('Control+i');
  await page.waitForSelector('[data-testid=dialogue-infos] [data-champ]');
  await page.waitForTimeout(500);
  const t1 = await titreCellule(page, 1);
  await page.evaluate(() => { window.__comicrustSimule.retardsApres.lire_fiches = [400, 0]; });
  const suivant = page.getByRole('button', { name: 'Suivant ▶' });
  await suivant.click();
  await page.waitForTimeout(80);
  ok(await suivant.isDisabled(), 'Suivant grisé pendant le chargement du voisin');
  await suivant.click({ force: true });
  await page.waitForTimeout(1500);
  const fenetre = (await page.locator('#titre-infos').innerText()).replace(/^Infos — /, '');
  ok(fenetre === t1 && fenetre === (await titreDetails(page)), `un seul livre d'avance, fenêtre et vue d'accord (« ${fenetre} »)`);
  await page.close();
}

async function testB10(browser) {
  console.log('B10 : ARIA de la grille et du tableau');
  const page = await ouvrir(browser);
  await page.locator('[data-testid=grille] [data-index="0"]').click();
  await page.locator('[data-testid=grille]').evaluate((z) => (z.scrollTop = z.scrollHeight / 2));
  await page.waitForTimeout(400);
  const ref = await page.locator('[data-testid=grille]').getAttribute('aria-activedescendant');
  ok(ref === null || (await page.locator(`#${ref}`).count()) === 1, `aria-activedescendant jamais vers un élément absent (${ref})`);
  await page.getByRole('button', { name: '☰ Tableau' }).click();
  await page.waitForSelector('[data-testid=tableau] [data-index]');
  const r = await page.evaluate(() => {
    const lignes = [...document.querySelectorAll('[data-testid=tableau] [role=row]')];
    const enfantsSansRole = lignes.flatMap((l) => [...l.children].filter((c) => !['gridcell', 'columnheader'].includes(c.getAttribute('role'))));
    const entetesBoutons = document.querySelectorAll('[data-testid=tableau] button[role=columnheader]').length;
    return { n: enfantsSansRole.length, entetesBoutons };
  });
  ok(r.n === 0 && r.entetesBoutons === 0, `enfants des lignes : cellules ou en-têtes (${r.n} autres, ${r.entetesBoutons} boutons en-têtes)`);
  await page.close();
}

(async () => {
  const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME, args: ['--no-sandbox'] } : {});
  try {
    await testB1Marquer(browser);
    await testB1Infos(browser);
    await testB2(browser);
    await testB3(browser);
    await testB4B5(browser);
    await testB6B7B8(browser);
    await testB9(browser);
    await testB10(browser);
  } finally {
    await browser.close();
  }
  console.log(echecs ? `\n${echecs} ÉCHEC(S)` : '\nTout est passé.');
  process.exit(echecs ? 1 : 0);
})();
