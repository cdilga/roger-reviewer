const test = require('node:test');
const assert = require('node:assert/strict');

const {
  ACTIONS,
  NON_PR_SUBTITLE,
  PR_SUBTITLE,
  RESUME_PRIMARY_ATTENTION_STATES,
  buildStatusMessage,
  describeBuildInfo,
  deriveActionModel,
  buildLaunchMessage,
  buildPopupViewModel,
  describeLaunchResponse,
  parsePullRequestContextFromUrl,
  resolveAttentionState,
  resolveSessionCount,
  SUPPORTED_ACTIONS,
  routePopupAction,
  shortenSessionId,
  formatRelativeAge,
  resumeCommandForSession,
  copyTextToClipboard,
  summarizeFindings,
  extractSessions,
  newestSession,
  renderFindingsSummary,
  renderSessionsSummary,
  clearPopupResults,
} = require('./main.js');

test('parsePullRequestContextFromUrl extracts owner/repo/pr from GitHub PR URL', () => {
  const context = parsePullRequestContextFromUrl('https://github.com/octo/roger-reviewer/pull/42');

  assert.deepEqual(context, {
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 42,
  });
});

test('parsePullRequestContextFromUrl rejects non-PR URL', () => {
  const context = parsePullRequestContextFromUrl('https://github.com/octo/roger-reviewer/issues/42');

  assert.equal(context, null);
});

test('buildPopupViewModel returns non_pr guidance when active tab is not a pull request', () => {
  const viewModel = buildPopupViewModel('https://example.com/dashboard');

  assert.equal(viewModel.mode, 'non_pr');
  assert.equal(viewModel.context, null);
  assert.equal(viewModel.attentionState, null);
  assert.equal(viewModel.subtitle, NON_PR_SUBTITLE);
  assert.match(viewModel.subtitle, /manual backup/i);
  assert.match(viewModel.subtitle, /Open a GitHub pull request tab/i);
});

test('buildPopupViewModel returns PR context title and action subtitle on pull request tabs', () => {
  const viewModel = buildPopupViewModel('https://github.com/octo/roger-reviewer/pull/42');

  assert.equal(viewModel.mode, 'pr');
  assert.deepEqual(viewModel.context, {
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 42,
  });
  assert.equal(viewModel.attentionState, null);
  assert.equal(viewModel.subtitle, PR_SUBTITLE);
  assert.match(viewModel.title, /octo\/roger-reviewer#42/);
  assert.match(viewModel.subtitle, /manual backup controls/i);
  assert.match(viewModel.subtitle, /in-page Roger controls/i);
});

test('buildStatusMessage emits canonical bounded-status payload', () => {
  assert.deepEqual(
    buildStatusMessage({
      owner: 'octo',
      repo: 'roger-reviewer',
      pr_number: 42,
    }),
    {
      type: 'roger_bridge_status',
      intent: {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      },
    }
  );
});

test('ACTIONS advertise explicit popup labels instead of generic verbs', () => {
  const labels = new Map(ACTIONS.map((action) => [action.id, action.label]));
  assert.equal(labels.get('start_review'), 'Start Review in Roger');
  assert.equal(labels.get('resume_review'), 'Resume Existing Review');
  assert.equal(labels.get('show_findings'), 'View Findings');
});

test('deriveActionModel hides findings until bounded status justifies it', () => {
  const model = deriveActionModel(null);
  assert.equal(model.primaryActionId, 'start_review');
  assert.equal(model.visibleActions.has('show_findings'), false);
  assert.equal(model.visibleActions.has('resume_review'), true);
});

test('deriveActionModel promotes findings only for findings-ready state', () => {
  const findingsModel = deriveActionModel('findings_ready');
  assert.equal(findingsModel.primaryActionId, 'show_findings');
  assert.equal(findingsModel.visibleActions.has('show_findings'), true);

  const approvalModel = deriveActionModel('awaiting_outbound_approval');
  assert.equal(RESUME_PRIMARY_ATTENTION_STATES.has('awaiting_outbound_approval'), true);
  assert.equal(approvalModel.primaryActionId, 'resume_review');
  assert.equal(approvalModel.visibleActions.has('show_findings'), false);
});

test('popup deriveActionModel hides resume when no local session exists', () => {
  const model = deriveActionModel(null, 0);
  assert.equal(model.primaryActionId, 'start_review');
  assert.equal(model.visibleActions.has('start_review'), true);
  assert.equal(model.visibleActions.has('resume_review'), false);
  assert.equal(model.visibleActions.has('show_findings'), false);
});

test('popup deriveActionModel promotes resume when sessions exist and labels counts', () => {
  const single = deriveActionModel(null, 1);
  assert.equal(single.primaryActionId, 'resume_review');
  assert.equal(single.visibleActions.has('resume_review'), true);
  assert.equal(single.resumeLabel, 'Resume Existing Review');

  const multiple = deriveActionModel(null, 4);
  assert.equal(multiple.primaryActionId, 'resume_review');
  assert.equal(multiple.resumeLabel, 'Resume Existing Review (4)');

  const findings = deriveActionModel('findings_ready', 2);
  assert.equal(findings.primaryActionId, 'show_findings');
  assert.equal(findings.visibleActions.has('resume_review'), true);
  assert.equal(findings.resumeLabel, 'Resume Existing Review (2)');
});

test('popup deriveActionModel keeps legacy both-buttons model when count is unknown', () => {
  for (const unknownCount of [null, undefined, -3, Number.NaN]) {
    const model = deriveActionModel(null, unknownCount);
    assert.equal(model.primaryActionId, 'start_review');
    assert.equal(model.visibleActions.has('resume_review'), true);
    assert.equal(model.resumeLabel, 'Resume Existing Review');
  }
});

test('resolveSessionCount only trusts finite non-negative numeric counts', () => {
  assert.equal(resolveSessionCount({ session_count: 0 }), 0);
  assert.equal(resolveSessionCount({ session_count: 3 }), 3);
  assert.equal(resolveSessionCount({ session_count: -1 }), null);
  assert.equal(resolveSessionCount({ session_count: 'two' }), null);
  assert.equal(resolveSessionCount({}), null);
  assert.equal(resolveSessionCount(null), null);
});

test('buildLaunchMessage emits canonical launch payload', () => {
  const message = buildLaunchMessage('start_review', {
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 42,
  });

  assert.deepEqual(message, {
    type: 'roger_bridge_launch',
    intent: {
      action: 'start_review',
      owner: 'octo',
      repo: 'roger-reviewer',
      pr_number: 42,
    },
  });
});

test('routePopupAction routes every documented action through dispatcher payload', async () => {
  const seenActions = [];

  for (const action of ACTIONS) {
    const result = await routePopupAction(
      action.id,
      {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      },
      async (payload) => {
        seenActions.push(payload.intent.action);
        return { ok: true, mode: 'native_messaging' };
      }
    );

    assert.equal(result.ok, true);
    assert.equal(result.mode, 'native_messaging');
  }

  assert.deepEqual(seenActions, ACTIONS.map((action) => action.id));
});

test('routePopupAction no longer exposes refresh_review as a supported action', () => {
  assert.equal(SUPPORTED_ACTIONS.has('refresh_review'), false);
  assert.throws(
    () =>
      buildLaunchMessage('refresh_review', {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      }),
    /Unsupported action/
  );
});

test('describeLaunchResponse appends repair guidance on successful launch responses', () => {
  const feedback = describeLaunchResponse({
    ok: true,
    mode: 'native_messaging',
    message: 'rr resume completed for octo/roger-reviewer#42',
    guidance: 'Run `rr resume --session session-42` locally to reconcile stale state.',
  });

  assert.equal(feedback.isError, false);
  assert.match(feedback.message, /rr resume completed/i);
  assert.match(feedback.message, /rr resume --session session-42/);
});

test('resolveAttentionState only elevates findings action for findings-ready responses', () => {
  assert.equal(resolveAttentionState({ attention_state: 'findings_ready' }), 'findings_ready');
  assert.equal(
    resolveAttentionState({ attention_state: 'awaiting_outbound_approval' }),
    'awaiting_outbound_approval'
  );
  assert.equal(resolveAttentionState({ finding_count: 2 }), 'findings_ready');
  assert.equal(resolveAttentionState({ finding_count: 0 }), null);
});

test('describeBuildInfo reports fallback text when version is unavailable', () => {
  assert.equal(describeBuildInfo(''), 'Extension build unavailable.');
  assert.equal(describeBuildInfo('0.1.0-dev+abc123'), 'Extension build 0.1.0-dev+abc123.');
});

test('buildLaunchMessage rejects unsupported actions', () => {
  assert.throws(
    () =>
      buildLaunchMessage('post_review', {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      }),
    /Unsupported action/
  );
});

// ---------------------------------------------------------------------------
// Findings + session summaries (bead rr-ext-session-candidates-surface-pnv0).
// ---------------------------------------------------------------------------

test('summarizeFindings reads count + up to three titles from a findings mirror', () => {
  const summary = summarizeFindings({
    ok: true,
    findings: {
      count: 5,
      items: [
        { title: 'Unbounded retry loop' },
        { title: 'Missing auth check' },
        { title: 'Race in cache write' },
        { title: 'Typo in log' },
      ],
    },
  });
  assert.equal(summary.count, 5);
  assert.deepEqual(summary.titles, ['Unbounded retry loop', 'Missing auth check', 'Race in cache write']);
  assert.equal(summarizeFindings({ ok: true }), null);
});

test('extractSessions normalizes the session inventory array', () => {
  const sessions = extractSessions({
    sessions: [
      { session_id: 'a', provider: 'opencode', updated_at: 1000 },
      { junk: true },
      { session_id: 'b', updated_at: '2026-01-01T00:00:00Z' },
    ],
  });
  assert.equal(sessions.length, 2);
  assert.equal(sessions[0].session_id, 'a');
  assert.equal(sessions[1].session_id, 'b');
  assert.deepEqual(extractSessions({}), []);
});

test('newestSession picks the most recently updated session', () => {
  const newest = newestSession([
    { session_id: 'old', updated_at: 1000 },
    { session_id: 'new', updated_at: 3000 },
    { session_id: 'mid', updated_at: 2000 },
  ]);
  assert.equal(newest.session_id, 'new');
  assert.equal(newestSession([]), null);
});

test('shortenSessionId and resumeCommandForSession build the compact handoff', () => {
  assert.equal(shortenSessionId('session-1700000000-1234-7'), 'session-17…1234-7');
  assert.equal(resumeCommandForSession('sess-x'), 'rr open --session sess-x');
});

test('formatRelativeAge renders coarse ages', () => {
  const now = 2_000_000_000_000;
  assert.equal(formatRelativeAge(now / 1000 - 300, now), '5m ago');
  assert.equal(formatRelativeAge(null, now), null);
});

test('copyTextToClipboard uses navigator.clipboard when present', async () => {
  let written = null;
  const ok = await copyTextToClipboard('rr open --session s1', {
    navigator: { clipboard: { writeText: (t) => { written = t; return Promise.resolve(); } } },
  });
  assert.equal(ok, true);
  assert.equal(written, 'rr open --session s1');
});

// Minimal DOM stubs for the render helpers, which target #popup-results.
function makePopupEl(tag) {
  return {
    tagName: String(tag).toUpperCase(),
    textContent: '',
    children: [],
    attributes: {},
    _listeners: {},
    appendChild(node) {
      this.children.push(node);
      return node;
    },
    setAttribute(name, value) {
      this.attributes[name] = value;
    },
    addEventListener(type, fn) {
      (this._listeners[type] = this._listeners[type] || []).push(fn);
    },
    click() {
      for (const fn of this._listeners.click || []) fn({});
    },
  };
}

function withPopupDocument(fn) {
  const results = makePopupEl('div');
  const previous = global.document;
  global.document = {
    createElement: (tag) => makePopupEl(tag),
    getElementById: (id) => (id === 'popup-results' ? results : null),
  };
  try {
    return fn(results);
  } finally {
    global.document = previous;
  }
}

function collectText(node, acc = []) {
  if (node.textContent) {
    acc.push(node.textContent);
  }
  for (const child of node.children || []) {
    collectText(child, acc);
  }
  return acc;
}

test('renderFindingsSummary renders a count heading and titles into the results area', () => {
  withPopupDocument((results) => {
    clearPopupResults();
    renderFindingsSummary({ count: 2, titles: ['Unbounded retry loop', 'Missing auth check'] });
    const text = collectText(results).join(' | ');
    assert.match(text, /Findings \(2\)/);
    assert.match(text, /Unbounded retry loop/);
    assert.match(text, /Missing auth check/);
  });
});

test('renderSessionsSummary lists sessions and a copy button for the newest session', async () => {
  await withPopupDocument(async (results) => {
    let written = null;
    clearPopupResults();
    renderSessionsSummary(
      [
        { session_id: 'sess-old', updated_at: 1000 },
        { session_id: 'sess-new', updated_at: 3000 },
      ],
      { navigator: { clipboard: { writeText: (t) => { written = t; return Promise.resolve(); } } } }
    );
    const text = collectText(results).join(' | ');
    assert.match(text, /Local sessions \(2\)/);
    assert.match(text, /rr open --session sess-new/);

    // The copy button copies the newest session's handoff command.
    const buttons = [];
    const walk = (n) => {
      if (n.tagName === 'BUTTON') buttons.push(n);
      for (const c of n.children || []) walk(c);
    };
    walk(results);
    assert.equal(buttons.length, 1);
    buttons[0].click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(written, 'rr open --session sess-new');
  });
});
