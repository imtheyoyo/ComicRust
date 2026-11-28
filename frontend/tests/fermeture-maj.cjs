// Test Playwright (mode simulé) du correctif B1 de la relecture tauri-v1.0.2 : fenêtre fermée pendant
// l'installation d'une mise à jour → événement `maj://fermeture-differee` → la fenêtre « Mise à jour »
// s'ouvre avec « Installation en cours : ComicRust se fermera à la fin ».
//
// Usage : `npx vite --port 5392` dans frontend/, puis
//   node tests/fermeture-maj.cjs [http://localhost:5392] [chemin du module playwright]
const base = process.argv[2] ?? 'http://localhost:5392';
const { chromium } = require(process.argv[3] ?? '/home/claude/.npm-global/lib/node_modules/playwright');

(async () => {
  const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME } : {});
  let echecs = 0;
  const ok = (c, m) => { if (!c) { echecs++; console.log('  ÉCHEC :', m); } else console.log('  ok :', m); };
  const page = await browser.newPage();
  page.on('pageerror', (e) => { echecs++; console.log('  ERREUR PAGE :', e.message); });
  await page.goto(`${base}/?n=50`);
  await page.waitForSelector('[data-testid=grille] [data-index]');
  await page.waitForTimeout(300);
  console.log('B1 : fermeture pendant l\'installation');
  ok((await page.locator('[data-testid=dialog-maj]').count()) === 0, 'fenêtre « Mise à jour » fermée au départ');
  await page.evaluate(() => {
    const s = window.__comicrustSimule;
    const release = { tag: 'tauri-v9.9.9', nom: 'tauri-v9.9.9', notes: '', urlPage: 'https://github.com/imtheyoyo/ComicRust/releases', taille: 1000, aPaquet: true };
    s.emettre('maj://statut', { etat: 'installation', manuel: true, release });
    s.emettre('maj://fermeture-differee', null);
  });
  await page.waitForSelector('[data-testid=maj-fermeture-differee]', { timeout: 3000 }).catch(() => {});
  const texte = await page.locator('[data-testid=maj-fermeture-differee]').innerText().catch(() => '');
  ok(texte.includes('Installation en cours : ComicRust se fermera à la fin'), `message affiché (« ${texte} »)`);
  await browser.close();
  console.log(echecs ? `${echecs} échec(s)` : 'tout est bon');
  process.exit(echecs ? 1 : 0);
})();
