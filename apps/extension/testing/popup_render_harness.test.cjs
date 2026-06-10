const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const { runPanelScenario, runPopupScenario } = require('./popup_render_harness.cjs');

function makeOutputDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'roger-extension-render-harness-'));
}

test('panel render harness mounts the injected card in dark rail mode above reviewers', async () => {
  const outputDir = makeOutputDir();
  const result = await runPanelScenario('pr-dark-rail', { outputDir });

  assert.equal(result.summary.surface, 'panel');
  assert.equal(result.summary.panelMode, 'rail');
  assert.equal(result.summary.mountedUnderId, 'roger-reviewer-rail-slot');
  assert.equal(result.summary.hostTheme, 'dark');
  assert.equal(result.summary.title, 'Roger Reviewer');
  assert.equal(result.summary.subtitle, 'rust-lang/rust#155408');
  assert.match(result.summary.buildLabel, /Extension build 0\.1\.0-dev\+render-harness\./i);
  assert.equal(result.summary.infoToggleLabel, 'i');
  assert.match(result.summary.infoTooltip, /Extension build 0\.1\.0-dev\+render-harness\./i);
  assert.match(result.summary.infoText, /authoritative detail/i);
  assert.deepEqual(result.summary.visibleActions, ['start_review', 'resume_review']);
  assert.deepEqual(result.summary.transcript.statusRequests[0].intent, {
    owner: 'rust-lang',
    repo: 'rust',
    pr_number: 155408,
  });
  assert.match(result.summary.panelStyleSheetExcerpt, /roger-panel--rail/);
  assert.match(result.summary.buttons[0].className, /roger-panel-button--primary/);
  assert.ok(fs.existsSync(result.htmlPath));
  assert.ok(fs.existsSync(result.summaryPath));
});

test('panel render harness promotes findings in the dark rail card when bounded status says findings-ready', async () => {
  const result = await runPanelScenario('pr-dark-findings-ready');
  const findingsButton = result.summary.buttons.find((button) => button.action === 'show_findings');

  assert.ok(findingsButton);
  assert.equal(findingsButton.hidden, false);
  assert.match(findingsButton.className, /roger-panel-button--primary/);
  assert.equal(result.summary.badgeText, 'Findings ready (fresh)');
  assert.deepEqual(result.summary.visibleActions, ['start_review', 'resume_review', 'show_findings']);
});

test('popup render harness remains available as a secondary local surface', async () => {
  const result = await runPopupScenario('pr-idle');

  assert.equal(result.summary.surface, 'popup');
  assert.match(result.summary.title, /rust-lang\/rust#155408/);
  assert.deepEqual(result.summary.visibleActions, ['start_review', 'resume_review']);
});
