const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const popupHtmlPath = path.join(__dirname, 'index.html');
const popupHtml = fs.readFileSync(popupHtmlPath, 'utf8');

test('popup redesign exposes primary/secondary hierarchy with demoted findings action', () => {
  assert.match(popupHtml, /class="action-primary"[^>]*data-action="start_review"/);
  assert.match(popupHtml, /class="action-secondary"[^>]*data-action="resume_review"/);
  assert.match(popupHtml, /class="action-tertiary"[^>]*data-action="show_findings"/);
  assert.match(popupHtml, /Start Review in Roger/);
  assert.match(popupHtml, /Resume Existing Review/);
  assert.match(popupHtml, /View Findings/);
});

test('popup redesign moves build details into a persistent info affordance', () => {
  assert.match(popupHtml, /id="popup-info-toggle"/);
  assert.match(popupHtml, /id="popup-build-info"/);
  assert.doesNotMatch(popupHtml, /id="popup-build"/);
});

test('popup follows the OS/browser theme preference for light and dark', () => {
  // The popup is a standalone document with no GitHub host, so the faithful
  // theme signal is the OS/browser preference. It must opt into both schemes
  // and ship a dark remap of the Roger-owned popup tokens.
  assert.match(popupHtml, /color-scheme: light dark/);
  assert.match(popupHtml, /@media \(prefers-color-scheme: dark\)/);
});

test('popup light theme keeps Roger-owned surface tokens (no hardcoded white card)', () => {
  // Surfaces route through tokens so the dark remap can flow through; the card
  // and button backgrounds must not be hardcoded light values anymore.
  assert.match(popupHtml, /--rr-popup-surface: #ffffff/);
  assert.match(popupHtml, /\.card\s*\{[^}]*background: var\(--rr-popup-surface\)/);
  assert.match(popupHtml, /button\s*\{[^}]*background: var\(--rr-popup-surface-muted\)/);
  assert.match(popupHtml, /\.action-secondary\s*\{[^}]*background: var\(--rr-popup-surface\)/);
});

test('popup dark theme remaps ink, surface, accent, and border tokens for legibility', () => {
  const darkBlock = popupHtml.match(
    /@media \(prefers-color-scheme: dark\)\s*\{\s*:root\s*\{([\s\S]*?)\}\s*\}/
  );
  assert.ok(darkBlock, 'popup should define a dark :root token remap');
  const darkTokens = darkBlock[1];
  assert.match(darkTokens, /--rr-popup-ink: #e6edf3/);
  assert.match(darkTokens, /--rr-popup-muted: #8b949e/);
  assert.match(darkTokens, /--rr-popup-surface: #161b22/);
  assert.match(darkTokens, /--rr-popup-surface-muted: #21262d/);
  assert.match(darkTokens, /--rr-popup-accent: #58a6ff/);
  assert.match(darkTokens, /--rr-popup-border: #30363d/);
});

test('popup error status stays legible in dark theme', () => {
  // A pure light-mode danger red is too dark on a dark surface; the popup
  // raises it to a Primer dark danger foreground under prefers-color-scheme.
  assert.match(
    popupHtml,
    /@media \(prefers-color-scheme: dark\)\s*\{\s*\.status-error\s*\{\s*color: #ff7b72/
  );
});
