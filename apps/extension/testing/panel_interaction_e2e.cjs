#!/usr/bin/env node
// Live panel-interaction E2E driver (real browser, real clicks, real native
// messaging) over the Chrome DevTools Protocol. This is the layer that contract
// and jsdom tests CANNOT cover: it drives genuine clicks against the loaded
// extension and asserts OBSERVABLE outcomes, so a launch that silently no-ops
// or a control that gives no visible feedback FAILS here.
//
// It exists because both failures shipped past green contract suites: the
// status relay (rr-s8wx) and the launch buttons (the one-shot sendNativeMessage
// twin) only broke against Edge's real MV3 service worker, which no stub models.
//
// Driven by scripts/extension/test_panel_interaction_e2e.sh, which launches the
// browser preloaded with the unpacked extension and passes the CDP port.
//
// Env:
//   RR_CDP_PORT_FILE  path to the browser profile DevToolsActivePort file (required unless RR_CDP_PORT)
//   RR_CDP_PORT       explicit CDP port (overrides the port file)
//   RR_PR_URL         PR page url to drive (default cdilga/roger-reviewer#2)
//   RR_SKIP_IDLE      "1" to skip the ~35s worker-idle regression (faster, weaker)
const fs = require('fs');
const path = require('path');

function loadPuppeteer() {
  for (const base of [__dirname, path.join(__dirname, '..', '..')]) {
    try {
      return require(require.resolve('puppeteer-core', { paths: [base] }));
    } catch {
      /* try next */
    }
  }
  console.error('FATAL: puppeteer-core not found. Run scripts/extension/test_panel_interaction_e2e.sh (it bootstraps the dep).');
  process.exit(3);
}
const puppeteer = loadPuppeteer();

const PR_URL = process.env.RR_PR_URL || 'https://github.com/cdilga/roger-reviewer/pull/2';
const SKIP_IDLE = process.env.RR_SKIP_IDLE === '1';
const IDLE_MS = 35000;

function resolvePort() {
  if (process.env.RR_CDP_PORT) return process.env.RR_CDP_PORT.trim();
  const f = process.env.RR_CDP_PORT_FILE;
  if (f && fs.existsSync(f)) return fs.readFileSync(f, 'utf8').split('\n')[0].trim();
  throw new Error('no CDP port: set RR_CDP_PORT or RR_CDP_PORT_FILE');
}

const results = [];
const record = (name, ok, detail) => {
  results.push({ name, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}: ${name}${detail ? ` — ${detail}` : ''}`);
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Is an element genuinely visible to a human (not just present in the DOM)?
const VIS = (sel) => `(() => {
  const el = document.querySelector(${JSON.stringify(sel)});
  if (!el) return { exists:false };
  const cs = getComputedStyle(el); const r = el.getBoundingClientRect();
  return { exists:true, text:(el.textContent||'').trim().slice(0,120),
    visible: cs.display!=='none' && cs.visibility!=='hidden' && parseFloat(cs.opacity)>0 && r.height>0 && r.width>0 };
})()`;

async function clickAction(page, action) {
  const rect = await page.evaluate((a) => {
    const b = [...document.querySelectorAll('#roger-reviewer-panel button[data-action]')]
      .find((x) => x.dataset.action === a && !x.hidden);
    if (!b) return null;
    const r = b.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  }, action);
  if (!rect) return false;
  await page.mouse.click(rect.x, rect.y);
  return true;
}

const launchSettled = (s) =>
  !s.btnDisabled && s.btnText !== '…' &&
  /completed|session|launched|blocked|unavailable|error|failed/i.test(`${s.status} ${s.info}`);

const snapLaunch = (page) => page.evaluate(() => {
  const panel = document.getElementById('roger-reviewer-panel');
  const status = document.getElementById('roger-reviewer-status');
  const info = document.getElementById('roger-reviewer-info-text');
  const b = panel?.querySelector('button[data-action]:not([hidden])');
  const scs = status ? getComputedStyle(status) : null;
  const sr = status ? status.getBoundingClientRect() : null;
  return {
    btnText: (b?.textContent || '').trim(),
    btnDisabled: !!b?.disabled,
    status: (status?.textContent || '').trim().slice(0, 160),
    info: (info?.textContent || '').trim().slice(0, 160),
    statusVisible: !!scs && scs.display !== 'none' && scs.visibility !== 'hidden' &&
      parseFloat(scs.opacity) > 0 && sr.height > 0,
  };
});

(async () => {
  const port = resolvePort();
  const browser = await puppeteer.connect({ browserURL: `http://127.0.0.1:${port}`, defaultViewport: null });
  const pages = await browser.pages();
  let page = pages.find((p) => p.url().includes('/pull/'));
  if (!page) { page = pages.find((p) => p.url().includes('github.com')) || pages[0]; await page.goto(PR_URL, { waitUntil: 'networkidle2' }).catch(() => {}); }
  await page.bringToFront();
  await sleep(2500);

  // 1) Panel present + brand mark actually loaded (not a broken image).
  const mark = await page.evaluate(() => {
    const panel = document.getElementById('roger-reviewer-panel');
    const icon = document.querySelector('.roger-panel-brandicon');
    return { panel: !!panel, markLoaded: icon ? (icon.complete && icon.naturalWidth > 0) : false };
  });
  record('panel injected on PR page', mark.panel);
  record('brand mark renders (naturalWidth>0, not broken image)', mark.markLoaded);
  if (!mark.panel) { await finish(browser); return; }

  // 2) Each launch button click SETTLES with VISIBLE feedback (no silent no-op).
  for (const action of ['start_review', 'resume_review']) {
    const clicked = await clickAction(page, action);
    if (!clicked) { record(`${action}: button present + clickable`, false, 'no visible button'); continue; }
    await sleep(4500);
    const s = await snapLaunch(page);
    record(`${action}: click settles (not stuck on …)`, launchSettled(s), `btn="${s.btnText}"`);
    record(`${action}: feedback is VISUALLY VISIBLE to a user`, s.statusVisible && launchSettled(s),
      s.status.slice(0, 80));
  }

  // 3) Worker-idle regression (the exact condition that broke one-shot
  //    sendNativeMessage): after the MV3 worker idles, a click must STILL settle.
  if (!SKIP_IDLE) {
    console.log(`--- waiting ${IDLE_MS / 1000}s for the service worker to idle (rr-s8wx-class regression guard) ---`);
    await sleep(IDLE_MS);
    await clickAction(page, 'start_review');
    await sleep(5000);
    const s = await snapLaunch(page);
    record('launch still settles after service-worker idle', launchSettled(s), s.status.slice(0, 80));
  } else {
    console.log('--- RR_SKIP_IDLE=1: skipping the worker-idle regression (weaker run) ---');
  }

  // 4) The "i" details control toggles and reveals visible info.
  const iRect = await page.evaluate(() => {
    const s = document.querySelector('#roger-reviewer-panel details.roger-panel-info summary');
    if (!s) return null;
    const r = s.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  });
  if (iRect) {
    const before = await page.evaluate(() => document.querySelector('#roger-reviewer-panel details.roger-panel-info')?.open);
    await page.mouse.click(iRect.x, iRect.y);
    await sleep(500);
    const after = await page.evaluate(() => document.querySelector('#roger-reviewer-panel details.roger-panel-info')?.open);
    const panelVis = await page.evaluate(`(${VIS('#roger-reviewer-panel .roger-panel-info-panel')})`);
    record('"i" button toggles and reveals visible info', before === false && after === true && panelVis.visible);
  } else {
    record('"i" button present', false, 'no info summary');
  }

  await page.screenshot({ path: path.join(__dirname, 'panel_interaction_e2e.png') }).catch(() => {});
  await finish(browser);
})().catch((e) => { console.error('ERR', e.message); process.exit(2); });

async function finish(browser) {
  await browser.disconnect().catch(() => {});
  const failed = results.filter((r) => !r.ok);
  console.log(`\npanel_interaction_e2e: ${failed.length === 0 ? 'PASS' : 'FAIL'} (${results.length - failed.length}/${results.length} checks)`);
  process.exit(failed.length === 0 ? 0 : 1);
}
