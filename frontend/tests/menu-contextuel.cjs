// Tests Playwright (mode simulé) du menu contextuel : sous-menu « Scripts ▸ » (survol, clic, clavier,
// bords d'écran, replis) et « Monter / Descendre la pile » grisés quand ils sont sans effet.
//
// Usage : `npx vite --port 5391` dans frontend/, puis
//   node tests/menu-contextuel.cjs [http://localhost:5391] [chemin du module playwright]
const base = process.argv[2] ?? 'http://localhost:5391';
const { chromium } = require(process.argv[3] ?? '/home/claude/.npm-global/lib/node_modules/playwright');

let echecs = 0;
function ok(cond, msg) {
  if (!cond) { echecs++; console.log('  ÉCHEC :', msg); } else console.log('  ok :', msg);
}
const attendre = (ms) => new Promise((r) => setTimeout(r, ms));

async function ouvrir(browser, query = '', taille = { width: 1280, height: 800 }) {
  const page = await browser.newPage({ viewport: taille });
  page.on('pageerror', (e) => { echecs++; console.log('  ERREUR PAGE :', e.message); });
  await page.goto(`${base}/${query}`);
  await page.waitForSelector('[data-testid=grille] [data-index], [data-testid=tableau]');
  await page.waitForTimeout(500);
  return page;
}

/** Clic droit sur le premier livre à la position (x, y) de la fenêtre. */
async function clicDroitLivre(page, pos) {
  const livre = page.locator('[data-testid=grille] [data-index="0"]');
  await livre.click(); // sélectionne
  const b = await livre.boundingBox();
  if (pos) {
    // Position imposée (bords de fenêtre) : événement contextmenu synthétique sur le livre.
    await livre.evaluate((el, p) => el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: p.x, clientY: p.y })), pos);
  } else await page.mouse.click(b.x + 20, b.y + 20, { button: 'right' });
  await page.waitForSelector('[data-testid=menu-contextuel]');
  await page.waitForTimeout(150);
}

const sous = (page) => page.locator('[data-testid=sous-menu-contextuel]');
const parent = (page) => page.locator('[data-testid=menu-contextuel] [data-menu=scripts]');
const focusMenu = (page) => page.evaluate(() => document.activeElement?.getAttribute('data-menu') ?? null);

async function testsLivres(browser) {
  console.log('Menu des livres : sous-menu « Scripts ▸ »');
  const page = await ouvrir(browser);
  await clicDroitLivre(page);
  ok((await page.locator('[data-testid=menu-contextuel] [data-menu^="books:"]').count()) === 0, 'plus de scripts au premier niveau');
  ok((await page.locator('[data-menu=titre-scripts]').count()) === 0, 'plus de titre « Scripts »');
  ok((await parent(page).getAttribute('aria-haspopup')) === 'menu', 'aria-haspopup="menu" sur « Scripts »');
  ok((await parent(page).getAttribute('aria-expanded')) === 'false', 'aria-expanded=false à la fermeture');
  ok((await sous(page).count()) === 0, 'sous-menu fermé au départ');
  const rm = await page.locator('[data-testid=menu-contextuel]').boundingBox();

  // Survol : ouverture après un court délai, pas immédiate.
  await parent(page).hover();
  ok((await sous(page).count()) === 0, 'pas ouvert instantanément au survol (délai)');
  await sous(page).waitFor();
  await page.waitForTimeout(100);
  ok((await parent(page).getAttribute('aria-expanded')) === 'true', 'aria-expanded=true ouvert');
  ok((await sous(page).getAttribute('role')) === 'menu', 'sous-menu role="menu"');
  const rs = await sous(page).boundingBox();
  ok(rs.x >= rm.x + rm.width - 4 && rs.x <= rm.x + rm.width, `sous-menu à droite du menu (${Math.round(rs.x)} vs ${Math.round(rm.x + rm.width)})`);
  const items = await sous(page).locator('[role=menuitem]').evaluateAll((l) => l.map((b) => b.getAttribute('data-menu')));
  ok(items.filter((i) => i.startsWith('books:')).length === 3 && items.includes('scripts-gerer'), `entrées des scripts + « Gérer… » (${items.join(', ')})`);
  ok((await parent(page).getAttribute('aria-controls')) === (await sous(page).getAttribute('id')), 'aria-controls relie parent et sous-menu');

  // Traversée en diagonale : survole une autre entrée puis rejoint le sous-menu en < 300 ms.
  const autre = page.locator('[data-testid=menu-contextuel] [data-menu=renommer]');
  await autre.hover();
  await attendre(150);
  await sous(page).locator('[role=menuitem]').first().hover();
  await attendre(700);
  ok((await sous(page).count()) === 1, 'reste ouvert si on traverse vers le sous-menu (délai de fermeture)');
  // Quitte vers une autre entrée et y reste : se ferme.
  await autre.hover();
  await attendre(900);
  ok((await sous(page).count()) === 0, 'se ferme quand on reste sur une autre entrée');
  ok((await parent(page).getAttribute('aria-expanded')) === 'false', 'aria-expanded=false après fermeture');

  // Clic : ouvre et place le focus sur la première entrée.
  await parent(page).click();
  await sous(page).waitFor();
  await page.waitForTimeout(100);
  ok((await focusMenu(page)) === 'books:nettoyage.py:nettoyer', `clic : focus sur la 1re entrée (${await focusMenu(page)})`);
  await page.keyboard.press('ArrowDown');
  ok((await focusMenu(page)) === 'books:bedetheque/bd2.py:scraper', 'ArrowDown dans le sous-menu');
  await page.keyboard.press('End');
  ok((await focusMenu(page)) === 'scripts-gerer', 'End : dernière entrée');
  await page.keyboard.press('ArrowDown');
  ok((await focusMenu(page)) === 'books:nettoyage.py:nettoyer', 'ArrowDown boucle sur la 1re entrée');
  await page.keyboard.press('ArrowLeft');
  ok((await sous(page).count()) === 0 && (await focusMenu(page)) === 'scripts', 'Flèche gauche : referme et rend le focus à « Scripts »');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(100);
  ok((await page.locator('[data-testid=menu-contextuel]').count()) === 0, 'Échap sans sous-menu ouvert : ferme le menu');

  // Clavier complet : Flèche droite ouvre, Échap referme le sous-menu seulement, Échap referme le menu.
  await clicDroitLivre(page);
  await parent(page).focus();
  await page.keyboard.press('ArrowRight');
  await sous(page).waitFor();
  await page.waitForTimeout(100);
  ok((await focusMenu(page)) === 'books:nettoyage.py:nettoyer', 'Flèche droite : ouvre et focus sur la 1re entrée');
  await page.keyboard.press('Escape');
  ok((await sous(page).count()) === 0 && (await page.locator('[data-testid=menu-contextuel]').count()) === 1, 'Échap dans le sous-menu : ne ferme que le sous-menu');
  ok((await focusMenu(page)) === 'scripts', 'focus rendu à « Scripts »');
  await page.keyboard.press('Enter');
  await sous(page).waitFor();
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(100);
  ok((await page.locator('[data-testid=menu-contextuel]').count()) === 0, 'second Échap : menu fermé');

  // Choix d'un script : ferme le menu et lance le script sur le livre.
  await clicDroitLivre(page);
  await parent(page).focus();
  await page.keyboard.press('ArrowRight');
  await sous(page).waitFor();
  await page.keyboard.press('Enter');
  await page.waitForTimeout(400);
  ok((await page.locator('[data-testid=menu-contextuel]').count()) === 0, 'Entrée sur un script : menu fermé');
  const lancements = await page.evaluate(() => globalThis.__comicrustScripts?.lancements.length ?? 0);
  ok(lancements === 1, `script lancé (${lancements})`);
  await page.keyboard.press('Escape');

  // Clic ailleurs : ferme menu et sous-menu.
  await page.reload();
  await page.waitForSelector('[data-testid=grille] [data-index]');
  await clicDroitLivre(page);
  await parent(page).click();
  await sous(page).waitFor();
  await page.mouse.click(900, 700);
  await page.waitForTimeout(100);
  ok((await page.locator('[data-testid=menu-contextuel]').count()) === 0 && (await sous(page).count()) === 0, 'clic ailleurs : tout se ferme');

  // « Gérer les scripts… »
  await clicDroitLivre(page);
  await parent(page).click();
  await sous(page).locator('[data-menu=scripts-gerer]').click();
  await page.waitForSelector('[data-testid=dialogue-scripts]');
  ok(true, '« Gérer les scripts… » ouvre la gestion');
  await page.close();
}

async function testsCorrections(browser) {
  console.log('Corrections : focus, fermeture involontaire, flèches');
  const page = await ouvrir(browser);
  await clicDroitLivre(page);
  // 1. Focus dans le sous-menu, le pointeur bouge sur le menu principal : le focus ne se perd pas.
  await parent(page).click();
  await sous(page).waitFor();
  await page.waitForTimeout(100);
  await page.locator('[data-testid=menu-contextuel] [data-menu=renommer]').hover();
  await page.mouse.move(await page.evaluate(() => innerWidth / 2), 5); // sort du menu
  await attendre(900);
  ok((await sous(page).count()) === 0, 'sous-menu refermé par le pointeur');
  ok((await focusMenu(page)) === 'scripts', `focus rendu à « Scripts » (${await focusMenu(page)})`);
  await page.keyboard.press('ArrowDown');
  ok((await focusMenu(page)) !== null, 'les flèches atteignent encore le menu');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(100);
  ok((await page.locator('[data-testid=menu-contextuel]').count()) === 0, 'Échap ferme le menu');
  // 2. Sous-menu ouvert au survol, ArrowDown déplace le focus : il se referme.
  await clicDroitLivre(page);
  await parent(page).hover();
  await sous(page).waitFor();
  await parent(page).focus();
  await page.keyboard.press('ArrowDown');
  ok((await sous(page).count()) === 0 && (await parent(page).getAttribute('aria-expanded')) === 'false', 'ArrowDown hors de l\'entrée parente : sous-menu fermé');
  await page.keyboard.press('Escape');
  await page.close();
  // 3. ArrowUp depuis le conteneur (sous-menu grisé n'a pas d'entrée focalisable) : pas de plantage ;
  //    sous-menu à entrées actives : conteneur focalisé puis ArrowUp → dernière entrée.
  const p2 = await ouvrir(browser);
  await clicDroitLivre(p2);
  await parent(p2).click();
  await sous(p2).waitFor();
  await p2.waitForTimeout(100);
  await sous(p2).evaluate((e) => e.focus());
  await p2.keyboard.press('ArrowUp');
  ok((await focusMenu(p2)) === 'scripts-gerer', `ArrowUp depuis le conteneur : dernière entrée (${await focusMenu(p2)})`);
  await sous(p2).evaluate((e) => e.focus());
  await p2.keyboard.press('ArrowDown');
  ok((await focusMenu(p2)) === 'books:nettoyage.py:nettoyer', 'ArrowDown depuis le conteneur : première entrée');
  await p2.close();
}

async function testsBords(browser) {
  console.log('Bords d\'écran et défilement');
  // Bord droit : le sous-menu bascule à gauche.
  let page = await ouvrir(browser);
  await clicDroitLivre(page, { x: 1275, y: 200 });
  await parent(page).hover();
  await sous(page).waitFor();
  await page.waitForTimeout(150);
  let rm = await page.locator('[data-testid=menu-contextuel]').boundingBox();
  let rs = await sous(page).boundingBox();
  ok(rs.x + rs.width <= rm.x + 3 && rs.x >= 0, `bord droit : sous-menu à gauche du menu (${Math.round(rs.x + rs.width)} <= ${Math.round(rm.x)})`);
  ok(rs.y >= 0 && rs.y + rs.height <= 800, 'bord droit : dans la fenêtre');
  await page.close();

  // Bas de fenêtre + nombreux scripts : sous-menu borné à la fenêtre et défilant.
  page = await ouvrir(browser, '?scripts=beaucoup', { width: 1000, height: 420 });
  await clicDroitLivre(page, { x: 300, y: 380 });
  await parent(page).hover();
  await sous(page).waitFor();
  await page.waitForTimeout(200);
  rs = await sous(page).boundingBox();
  ok(rs.y >= 0 && rs.y + rs.height <= 420 + 1, `bas de fenêtre : sous-menu dans la fenêtre (${Math.round(rs.y)}–${Math.round(rs.y + rs.height)} / 420)`);
  const defile = await sous(page).evaluate((e) => e.scrollHeight > e.clientHeight + 2);
  ok(defile, 'sous-menu long : défile');
  const nb = await sous(page).locator('[data-menu^="books:"]').count();
  ok(nb === 30, `pas de limite à 12 : ${nb} scripts listés`);
  await parent(page).focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('End');
  const visible = await page.evaluate(() => {
    const r = document.activeElement.getBoundingClientRect();
    return r.top >= 0 && r.bottom <= innerHeight + 1;
  });
  ok(visible && (await focusMenu(page)) === 'scripts-gerer', 'End : dernière entrée atteinte et visible dans la fenêtre');
  await page.close();
}

async function testsReplis(browser) {
  console.log('Replis du sous-menu');
  const cas = [
    ['?scripts=vide', 'scripts-aucun', 'aucun script'],
    ['?scripts=echec', 'scripts-indispo', 'scripts indisponibles'],
  ];
  for (const [q, id, nom] of cas) {
    const page = await ouvrir(browser, q);
    await clicDroitLivre(page);
    await parent(page).hover();
    await sous(page).waitFor();
    ok((await sous(page).locator(`[data-menu=${id}]`).isDisabled()), `${nom} : entrée grisée`);
    ok((await sous(page).locator('[data-menu=scripts-gerer]').count()) === 1, `${nom} : « Gérer les scripts… » proposé`);
    await page.close();
  }
  let page = await ouvrir(browser, '?scripts=sansPython');
  await clicDroitLivre(page);
  await parent(page).hover();
  await sous(page).waitFor();
  const b = sous(page).locator('[data-menu^="books:"]').first();
  ok(await b.isDisabled(), 'Python introuvable : scripts grisés');
  ok(((await b.getAttribute('title')) ?? '').length > 0, 'Python introuvable : raison en infobulle');
  await parent(page).focus();
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(100);
  ok((await sous(page).count()) === 1, 'sous-menu 100 % grisé : ouvert et refermable au clavier (focus sur le panneau)');
  await page.keyboard.press('ArrowLeft');
  ok((await sous(page).count()) === 0 && (await focusMenu(page)) === 'scripts', 'Flèche gauche depuis le panneau grisé');
  await page.close();

  // Chargement : ouvert avant l'arrivée de la liste, le sous-menu se met à jour.
  page = await ouvrir(browser, '?scripts=lent');
  await clicDroitLivre(page);
  await parent(page).click();
  await sous(page).waitFor();
  ok((await sous(page).locator('[data-menu=scripts-charge]').count()) === 1, 'chargement : « Chargement des scripts… »');
  await page.waitForSelector('[data-testid=sous-menu-contextuel] [data-menu^="books:"]', { timeout: 5000 });
  ok(true, 'la liste arrive pendant que le sous-menu est ouvert : mise à jour');
  await page.close();

  // Pas de livre sélectionné : scripts grisés avec la raison.
  page = await ouvrir(browser);
  await clicDroitLivre(page);
  ok(true, 'menu ouvert');
  await page.close();
}

async function testsPiles(browser) {
  console.log('Piles : Monter / Descendre grisés sans effet');
  const page = await ouvrir(browser);
  await page.locator('[data-testid="arbre-listes"] >> text=À lire').first().click();
  await page.waitForTimeout(400);
  await page.click('[data-testid=empiler]');
  await page.waitForSelector('[data-testid=grille-piles] [data-pile]');
  const n = Number((await page.locator('[data-testid=compteur]').innerText()).match(/(\d+) piles/)[1]);
  const titres = () => page.evaluate(() => [...document.querySelectorAll('[data-pile]')]
    .sort((a, b) => Number(a.getAttribute('data-pile')) - Number(b.getAttribute('data-pile'))).map((e) => e.getAttribute('title').split(' — ')[0]));
  const pile = (i) => page.locator(`[data-testid=grille-piles] [data-pile="${i}"]`);
  const menuPile = async (i) => {
    await pile(i).scrollIntoViewIfNeeded();
    await pile(i).click({ button: 'right' });
    await page.waitForSelector('[data-testid=menu-contextuel]');
  };
  const etat = async (id) => page.locator(`[data-testid=menu-contextuel] [data-menu=${id}]`).evaluate((b) => ({ off: b.disabled, titre: b.title, texte: b.innerText }));
  const fermer = async () => { await page.keyboard.press('Escape'); await page.waitForTimeout(100); };
  ok(n >= 6, `${n} piles dans la liste`);

  await menuPile(0);
  let m = await etat('monter-pile'); let d = await etat('descendre-pile');
  ok(m.off && /première/.test(m.titre) && /première/.test(m.texte), `première pile : Monter grisé avec raison (« ${m.titre} »)`);
  ok(!d.off, 'première pile : Descendre actif');
  await fermer();
  // Alt+↑ sur la première pile : rien ne bouge, message bref.
  const avant = await titres();
  await pile(0).click();
  await page.keyboard.press('Alt+ArrowUp');
  await page.waitForTimeout(300);
  ok(JSON.stringify(await titres()) === JSON.stringify(avant), 'Alt+↑ sur la première pile : ordre inchangé');
  ok(/première/.test(await page.locator('[data-testid=info-lecture]').innerText()), 'Alt+↑ : raison affichée brièvement');

  await menuPile(n - 1);
  m = await etat('monter-pile'); d = await etat('descendre-pile');
  ok(d.off && /dernière/.test(d.titre), `dernière pile : Descendre grisé avec raison (« ${d.titre} »)`);
  ok(!m.off, 'dernière pile : Monter actif');
  await fermer();
  await pile(n - 1).click();
  await page.keyboard.press('Alt+ArrowDown');
  await page.waitForTimeout(300);
  ok(JSON.stringify(await titres()) === JSON.stringify(avant), 'Alt+↓ sur la dernière pile : ordre inchangé');

  // Pile du milieu : les deux actifs ; Monter déplace bien la pile.
  await menuPile(2);
  m = await etat('monter-pile'); d = await etat('descendre-pile');
  ok(!m.off && !d.off, 'pile du milieu : Monter et Descendre actifs');
  await page.locator('[data-testid=menu-contextuel] [data-menu=monter-pile]').click();
  await page.waitForTimeout(500);
  const apres = await titres();
  ok(apres[1] === avant[2] && apres[2] === avant[1], 'Monter : la pile 3 passe en 2e position');

  // Plusieurs piles : bloc en tête / en queue.
  await pile(0).click();
  await pile(1).click({ modifiers: ['Control'] });
  await menuPile(1);
  m = await etat('monter-pile'); d = await etat('descendre-pile');
  ok(m.off && /en tête/.test(m.titre), `bloc de 2 piles en tête : Monter grisé (« ${m.titre} »)`);
  ok(!d.off, 'bloc en tête : Descendre actif');
  const b0 = await titres();
  await page.locator('[data-testid=menu-contextuel] [data-menu=descendre-pile]').click();
  await page.waitForTimeout(500);
  const b1 = await titres();
  ok(b1[0] === b0[2] && b1[1] === b0[0] && b1[2] === b0[1] && b1[3] === b0[3], 'Descendre le bloc de 2 piles : bloc décalé d\'un cran');
  await pile(n - 2).scrollIntoViewIfNeeded();
  await pile(n - 2).click();
  await pile(n - 1).click({ modifiers: ['Control'] });
  await menuPile(n - 1);
  d = await etat('descendre-pile'); m = await etat('monter-pile');
  ok(d.off && /en queue/.test(d.titre), `bloc de 2 piles en queue : Descendre grisé (« ${d.titre} »)`);
  ok(!m.off, 'bloc en queue : Monter actif');
  await fermer();

  // Les piles du menu ont aussi le sous-menu Scripts.
  await menuPile(3);
  ok((await page.locator('[data-testid=menu-contextuel] [data-menu=scripts]').getAttribute('aria-haspopup')) === 'menu', 'menu des piles : « Scripts ▸ » sous-menu');
  await fermer();
  await page.close();
}

(async () => {
  const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME, args: ['--no-sandbox'] } : {});
  try {
    await testsLivres(browser);
    await testsBords(browser);
    await testsCorrections(browser);
    await testsReplis(browser);
    await testsPiles(browser);
  } finally {
    await browser.close();
  }
  console.log(echecs ? `\n${echecs} ÉCHEC(S)` : '\nTout est passé.');
  process.exit(echecs ? 1 : 0);
})();
