// Tests Playwright (mode simulé) des correctifs front de la tauri-v1.0.3 (anomalies de la recette 1.0.2) :
// - lecture seule ComicRack : les entrées grisées des actions sur les FICHIERS (« Écrire les infos dans le
//   fichier », « Convertir en CBZ », « Renommer / déplacer… ») donnent la raison propre aux fichiers ;
// - piles réordonnables : déposer une pile dans le vide après la dernière pile (à sa droite ou sous la
//   dernière rangée) la place en fin de liste, avec le trait d'insertion en fin de liste.
//
// Usage : `npx vite --port 5391` dans frontend/, puis
//   node tests/correctifs-v1.0.3.cjs [http://localhost:5391] [chemin du module playwright]
// Navigateur : celui de Playwright, ou `CHROME=/usr/bin/google-chrome` pour un Chrome installé.
const base = process.argv[2] ?? 'http://localhost:5391';
const { chromium } = require(process.argv[3] ?? '/home/claude/.npm-global/lib/node_modules/playwright');

let echecs = 0;
function ok(cond, msg) {
  if (!cond) { echecs++; console.log('  ÉCHEC :', msg); } else console.log('  ok :', msg);
}

async function ouvrir(browser, query = '', taille = { width: 1400, height: 900 }) {
  const page = await browser.newPage({ viewport: taille });
  page.on('pageerror', (e) => { echecs++; console.log('  ERREUR PAGE :', e.message); });
  await page.goto(`${base}/${query}`);
  await page.waitForSelector('[data-testid=grille] [data-index], [data-testid=tableau]');
  await page.waitForTimeout(600);
  return page;
}

const FICHIERS = 'ni ses fiches ni les fichiers des livres';

async function testLectureSeuleFichiers(browser) {
  console.log('Lecture seule ComicRack : raison des actions sur les fichiers');
  const page = await ouvrir(browser, '?mode=comicrack');
  const livre = page.locator('[data-testid=grille] [data-index="0"]');
  await livre.click();
  await livre.click({ button: 'right' });
  await page.waitForSelector('[data-testid=menu-contextuel]');
  const etat = (id) => page.locator(`[data-testid=menu-contextuel] [data-menu=${id}]`).evaluate((b) => ({ off: b.disabled, titre: b.title }));
  for (const id of ['ecritureInfos', 'conversion', 'renommer']) {
    const e = await etat(id);
    ok(e.off && e.titre.includes(FICHIERS) && e.titre.includes('Lecture et écriture'), `« ${id} » grisé, raison des fichiers (« ${e.titre} »)`);
    ok(!/ne modifie pas ses fiches/.test(e.titre), `« ${id} » : plus la raison des seules fiches`);
  }
  // Les actions sur les fiches gardent leur raison (fiches).
  const lu = await etat('marquerLu');
  ok(lu.off && !lu.titre.includes(FICHIERS), `« Marquer comme lu » : raison des fiches (« ${lu.titre} »)`);
  await page.keyboard.press('Escape');
  await page.close();

  // Mode autonome : les actions sur les fichiers restent possibles.
  const p2 = await ouvrir(browser);
  await p2.locator('[data-testid=grille] [data-index="0"]').click({ button: 'right' });
  await p2.waitForSelector('[data-testid=menu-contextuel]');
  const r = await p2.locator('[data-testid=menu-contextuel] [data-menu=renommer]').evaluate((b) => b.disabled);
  ok(!r, 'mode autonome : « Renommer / déplacer… » actif');
  await p2.close();
}

async function testPilesFinDeListe(browser) {
  console.log('Piles : dépôt dans le vide après la dernière pile = fin de liste');
  for (const ou of ['droite', 'dessous']) {
    // « dessous » : fenêtre haute, pour qu'il reste du vide sous la dernière rangée.
    const page = await ouvrir(browser, '', ou === 'dessous' ? { width: 1400, height: 2400 } : undefined);
    await page.locator('[data-testid="arbre-listes"] >> text=À lire').first().click();
    await page.waitForTimeout(400);
    await page.click('[data-testid=empiler]');
    await page.waitForSelector('[data-testid=grille-piles] [data-pile]');
    await page.waitForTimeout(300);
    const n = await page.locator('[data-testid=grille-piles] [data-pile]').count();
    const titres = () => page.evaluate(() => [...document.querySelectorAll('[data-pile]')]
      .sort((a, b) => Number(a.getAttribute('data-pile')) - Number(b.getAttribute('data-pile'))).map((e) => e.getAttribute('title').split(' — ')[0]));
    const avant = await titres();
    // Fin de la grille à l'écran ; la pile glissée est une des dernières affichées (même écran).
    await page.locator('[data-testid=grille-piles]').evaluate((z) => { z.scrollTop = z.scrollHeight; });
    await page.waitForTimeout(300);
    const src = Math.max(0, n - 4);
    const premiere = await page.locator(`[data-testid=grille-piles] [data-pile="${src}"]`).boundingBox();
    const derniere = await page.locator(`[data-testid=grille-piles] [data-pile="${n - 1}"]`).boundingBox();
    const zone = await page.locator('[data-testid=grille-piles]').boundingBox();
    let cible;
    if (ou === 'droite') {
      cible = { x: derniere.x + derniere.width + 60, y: derniere.y + derniere.height / 2 };
      if (cible.x > zone.x + zone.width - 10) {
        console.log('  (dernière rangée pleine : cas « à droite » sans objet)');
        await page.close();
        continue;
      }
    } else {
      cible = { x: derniere.x + derniere.width / 2, y: Math.min(derniere.y + derniere.height + 50, zone.y + zone.height - 40) };
      if (cible.y <= derniere.y + derniere.height) {
        console.log('  (pas de vide sous la dernière rangée : cas « dessous » sans objet)');
        await page.close();
        continue;
      }
    }
    await page.mouse.move(premiere.x + premiere.width / 2, premiere.y + premiere.height / 2);
    await page.mouse.down();
    await page.mouse.move(premiere.x + premiere.width / 2 + 20, premiere.y + premiere.height / 2 + 10, { steps: 3 });
    await page.mouse.move(cible.x, cible.y, { steps: 10 });
    await page.waitForTimeout(150);
    const trait = await page.locator('[data-testid=trait-insertion]').boundingBox().catch(() => null);
    ok(!!trait && Math.abs(trait.x - (derniere.x + derniere.width + 5)) < 4 && Math.abs(trait.y - derniere.y) < 4,
      `${ou} : trait d'insertion après la dernière pile (${trait ? `${Math.round(trait.x)},${Math.round(trait.y)}` : 'absent'})`);
    ok(/Déplacer ici/.test(await page.locator('body').innerText()), `${ou} : badge « Déplacer ici »`);
    await page.mouse.up();
    await page.waitForTimeout(800);
    const apres = await titres();
    const attendu = [...avant.slice(0, src), ...avant.slice(src + 1), avant[src]];
    ok(JSON.stringify(apres) === JSON.stringify(attendu),
      `${ou} : la pile ${src + 1} passe en fin de liste (${avant[src]} → ${apres.indexOf(avant[src]) + 1}/${apres.length})`);
    await page.close();
  }

  // Le vide entre deux piles (hors fin de liste) ne fait toujours rien.
  const page = await ouvrir(browser);
  await page.locator('[data-testid="arbre-listes"] >> text=À lire').first().click();
  await page.waitForTimeout(400);
  await page.click('[data-testid=empiler]');
  await page.waitForSelector('[data-testid=grille-piles] [data-pile]');
  await page.waitForTimeout(300);
  const p0 = await page.locator('[data-testid=grille-piles] [data-pile="0"]').boundingBox();
  const p1 = await page.locator('[data-testid=grille-piles] [data-pile="1"]').boundingBox();
  if (Math.abs(p0.y - p1.y) < 2) {
    await page.mouse.move(p0.x + p0.width / 2, p0.y + p0.height / 2);
    await page.mouse.down();
    await page.mouse.move(p0.x + p0.width / 2 + 20, p0.y + p0.height / 2, { steps: 3 });
    await page.mouse.move((p0.x + p0.width + p1.x) / 2, p0.y + p0.height / 2, { steps: 5 });
    await page.waitForTimeout(150);
    ok((await page.locator('[data-testid=trait-insertion]').count()) === 0, 'entre deux piles (dans l\'espace) : pas de fin de liste');
    await page.mouse.up();
  }
  await page.close();
}

async function testLivresFinDeListe(browser) {
  console.log('Livres (liste de lecture, ordre de la liste) : dépôt dans le vide après le dernier livre');
  const page = await ouvrir(browser, '', { width: 1400, height: 2400 });
  // « Ma lecture » : 12 livres, il reste du vide sous la dernière rangée.
  await page.locator('[data-liste-id="f0000000-0000-4000-8000-00000000fb01"] .triangle').click();
  await page.locator('[data-liste-id="l0000000-0000-4000-8000-00malecture"]').click();
  await page.waitForTimeout(800);
  const titres = () => page.evaluate(() => [...document.querySelectorAll('[data-testid=grille] [data-index]')]
    .sort((a, b) => Number(a.getAttribute('data-index')) - Number(b.getAttribute('data-index'))).map((e) => e.querySelector('.titre')?.textContent));
  const avant = await titres();
  const n = avant.length;
  const der = await page.locator(`[data-testid=grille] [data-index="${n - 1}"]`).boundingBox();
  const zone = await page.locator('[data-testid=grille]').boundingBox();
  if (!der || der.y + der.height + 60 > zone.y + zone.height) {
    console.log('  (liste trop longue pour avoir du vide après le dernier livre : sans objet)');
    await page.close();
    return;
  }
  const src = await page.locator('[data-testid=grille] [data-index="0"]').boundingBox();
  await page.mouse.move(src.x + src.width / 2, src.y + src.height / 2);
  await page.mouse.down();
  await page.mouse.move(src.x + src.width / 2 + 20, src.y + src.height / 2 + 10, { steps: 3 });
  await page.mouse.move(der.x + der.width / 2, der.y + der.height + 50, { steps: 10 });
  await page.waitForTimeout(150);
  ok((await page.locator('[data-testid=trait-insertion]').count()) === 1, 'trait d\'insertion en fin de liste');
  await page.mouse.up();
  await page.waitForTimeout(800);
  const apres = await titres();
  ok(apres[n - 1] === avant[0] && apres[0] === avant[1], `le premier livre passe en fin de liste (${avant[0]})`);
  await page.close();
}

(async () => {
  const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME, args: ['--no-sandbox'] } : {});
  try {
    await testLectureSeuleFichiers(browser);
    await testPilesFinDeListe(browser);
    await testLivresFinDeListe(browser);
  } finally {
    await browser.close();
  }
  console.log(echecs ? `\n${echecs} ÉCHEC(S)` : '\nTout est passé.');
  process.exit(echecs ? 1 : 0);
})();
