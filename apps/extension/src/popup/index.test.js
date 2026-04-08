const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const popupHtmlPath = path.join(__dirname, 'index.html');
const popupHtml = fs.readFileSync(popupHtmlPath, 'utf8');
const extensionRoot = path.join(__dirname, '..', '..');
const staticRoot = path.join(extensionRoot, 'static');

test('popup shell includes Roger identity assets and brand header', () => {
  assert.match(popupHtml, /class="brand-shell"/);
  assert.match(popupHtml, /class="brand-mark"/);
  assert.match(popupHtml, /class="brand-wordmark"/);
  assert.match(popupHtml, /roger-mark\.svg/);
  assert.match(popupHtml, /roger-wordmark\.svg/);

  assert.equal(fs.existsSync(path.join(staticRoot, 'roger-mark.svg')), true);
  assert.equal(fs.existsSync(path.join(staticRoot, 'roger-wordmark.svg')), true);
});

test('popup shell imports shared extension identity tokens', () => {
  assert.match(popupHtml, /roger-identity\.css/);
  assert.match(popupHtml, /--rr-popup-accent/);
  assert.match(popupHtml, /--rr-popup-canvas/);

  const identityCssPath = path.join(staticRoot, 'roger-identity.css');
  assert.equal(fs.existsSync(identityCssPath), true);
  const identityCss = fs.readFileSync(identityCssPath, 'utf8');
  assert.match(identityCss, /--rr-brand-accent-500/);
  assert.match(identityCss, /--rr-brand-ink-900/);
});

test('popup copy preserves manual-backup guidance in branded shell', () => {
  assert.match(popupHtml, /Local-first review continuity/i);
  assert.match(popupHtml, /id="popup-title"/);
  assert.match(popupHtml, /id="popup-subtitle"/);
});
