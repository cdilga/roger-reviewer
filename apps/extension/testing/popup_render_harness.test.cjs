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
  assert.match(result.summary.infoText, /1 local Roger review session for this PR/i);
  assert.match(result.summary.infoText, /authoritative detail/i);
  assert.deepEqual(result.summary.visibleActions, ['start_review', 'resume_review']);
  assert.deepEqual(result.summary.transcript.statusRequests[0].intent, {
    owner: 'rust-lang',
    repo: 'rust',
    pr_number: 155408,
  });
  assert.match(result.summary.panelStyleSheetExcerpt, /roger-panel--rail/);
  // One durable local session exists, so resuming is the primary intent.
  const resumeButton = result.summary.buttons.find((button) => button.action === 'resume_review');
  assert.ok(resumeButton);
  assert.match(resumeButton.className, /roger-panel-button--primary/);
  assert.equal(resumeButton.label, 'Resume Existing Review');
  const startButton = result.summary.buttons.find((button) => button.action === 'start_review');
  assert.match(startButton.className, /roger-panel-button--secondary/);
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

test('panel render harness mounts the injected card in light rail mode above reviewers', async () => {
  const result = await runPanelScenario('pr-light-rail');

  assert.equal(result.summary.surface, 'panel');
  assert.equal(result.summary.panelMode, 'rail');
  assert.equal(result.summary.mountedUnderId, 'roger-reviewer-rail-slot');
  assert.equal(result.summary.hostTheme, 'light');
  assert.deepEqual(result.summary.visibleActions, ['start_review', 'resume_review']);
  const resumeButton = result.summary.buttons.find((button) => button.action === 'resume_review');
  assert.ok(resumeButton);
  assert.match(resumeButton.className, /roger-panel-button--primary/);
});

// Host-theme fidelity probe: the injected stylesheet must carry the full
// matrix of GitHub dark-theme signals AND route its metallic sheen through the
// theme-aware tint token, in BOTH the light and dark host probes (the
// stylesheet is theme-agnostic markup; the host signal selects which block
// wins at paint time). This is the automated proof the mapping is host-theme
// driven, not a one-off contrast tweak. Live light/dark screenshots are a
// separate later phase.
for (const scenario of ['pr-light-rail', 'pr-dark-rail']) {
  test(`panel theme mapping covers every GitHub dark signal in the ${scenario} host probe`, async () => {
    const result = await runPanelScenario(scenario);
    const css = result.summary.panelStyleSheetExcerpt;

    assert.match(css, /:root\[data-color-mode="dark"\] #roger-reviewer-panel/);
    assert.match(css, /\[data-color-mode="dark"\] #roger-reviewer-panel/);
    assert.match(css, /#roger-reviewer-panel\[data-color-mode="dark"\]/);
    assert.match(css, /@media \(prefers-color-scheme: dark\)/);
    assert.match(css, /\[data-color-mode="auto"\] #roger-reviewer-panel/);

    // Dark remap binds to canonical dark Primer variables.
    assert.match(css, /--rr-panel-text: var\(--fgColor-default, #e6edf3\)/);
    assert.match(css, /--rr-panel-surface: var\(--overlay-bgColor, var\(--bgColor-default, #161b22\)\)/);

    // Theme-aware metallic tint, no literal-white surface/button sheen mixes.
    assert.match(css, /--rr-panel-metal-tint: var\(--fgColor-default, #f0f6fc\)/);
    const surfaceWhiteMixes = css.match(/var\(--rr-panel-surface[^)]*\)\s+\d+%,\s*white/g) || [];
    assert.equal(surfaceWhiteMixes.length, 0);
  });
}

test('popup render harness remains available as a secondary local surface', async () => {
  const result = await runPopupScenario('pr-idle');

  assert.equal(result.summary.surface, 'popup');
  assert.match(result.summary.title, /rust-lang\/rust#155408/);
  // The idle scenario truthfully reports zero durable local sessions
  // (session_count: 0), so there is no review to resume and no resume button.
  assert.deepEqual(result.summary.visibleActions, ['start_review']);
});
