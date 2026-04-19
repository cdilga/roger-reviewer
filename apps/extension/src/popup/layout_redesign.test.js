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
