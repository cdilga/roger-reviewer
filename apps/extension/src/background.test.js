const test = require('node:test');
const assert = require('node:assert/strict');

const {
  buildRegistrationIntent,
  detectBrowserLabel,
  MAX_MIRROR_FRESHNESS_SECONDS,
  MAX_SESSION_INVENTORY_ENTRIES,
  handleStatusMessage,
  handleLaunchMessage,
  dispatchNative,
  launchOnlyStatusEnvelope,
  normalizeBoundedStatus,
  normalizeStatusEnvelope,
  parseSessionInventory,
  registerRuntimeIdentity,
} = require('./background/main.js');

test('normalizeBoundedStatus returns bounded status for canonical state with freshness', () => {
  const response = normalizeBoundedStatus({
    ok: true,
    attention_state: 'awaiting_user_input',
    freshness_seconds: 12,
  });

  assert.ok(response);
  assert.equal(response.mode, 'bounded_status');
  assert.equal(response.attention_state, 'awaiting_user_input');
  assert.equal(response.freshness_seconds, 12);
  assert.equal(response.freshness_label, '12s old');
});

test('normalizeBoundedStatus rejects missing freshness indicator', () => {
  const response = normalizeBoundedStatus({
    ok: true,
    attention_state: 'findings_ready',
  });

  assert.equal(response, null);
});

test('normalizeBoundedStatus rejects stale mirror snapshots', () => {
  const response = normalizeBoundedStatus({
    ok: true,
    attention_state: 'findings_ready',
    freshness_seconds: MAX_MIRROR_FRESHNESS_SECONDS + 1,
  });

  assert.equal(response, null);
});

test('handleStatusMessage falls back to launch-only when status probe is unavailable', async () => {
  const response = await handleStatusMessage(
    {
      intent: {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      },
    },
    async () => null
  );

  assert.deepEqual(response, launchOnlyStatusEnvelope());
});

test('launchOnlyStatusEnvelope keeps no-status guidance honest and non-posting', () => {
  const response = launchOnlyStatusEnvelope();

  assert.equal(response.ok, true);
  assert.equal(response.mode, 'launch_only');
  assert.match(response.message, /launch-only bridge mode/i);
  assert.match(response.guidance, /rr status/);
  assert.match(response.guidance, /rr findings/);

  const combined = `${response.message} ${response.guidance}`.toLowerCase();
  assert.doesNotMatch(combined, /approval/);
  assert.doesNotMatch(combined, /ready to post/);
});

test('handleStatusMessage returns bounded status from companion-tier probe', async () => {
  const response = await handleStatusMessage(
    {
      intent: {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      },
    },
    async () =>
      normalizeBoundedStatus({
        ok: true,
        attention_state: 'refresh_recommended',
        freshness_seconds: 9,
      })
  );

  assert.ok(response);
  assert.equal(response.mode, 'bounded_status');
  assert.equal(response.attention_state, 'refresh_recommended');
});

test('normalizeBoundedStatus preserves repair guidance for stale mirrored state', () => {
  const response = normalizeBoundedStatus({
    ok: true,
    attention_state: 'refresh_recommended',
    freshness_seconds: 9,
    guidance: 'Run `rr resume --session session-42` locally before trusting stale findings.',
  });

  assert.ok(response);
  assert.equal(
    response.guidance,
    'Run `rr resume --session session-42` locally before trusting stale findings.'
  );
});

test('normalizeBoundedStatus passes durable session inventory through fresh bounded status', () => {
  const response = normalizeBoundedStatus({
    ok: true,
    attention_state: 'findings_ready',
    freshness_seconds: 30,
    session_count: 2,
    sessions: [
      {
        session_id: 'session-1',
        provider: 'claude',
        attention_state: 'findings_ready',
        updated_at: '2026-06-12T00:00:00Z',
      },
      { session_id: 'session-2', provider: 'codex' },
    ],
  });

  assert.ok(response);
  assert.equal(response.mode, 'bounded_status');
  assert.equal(response.attention_state, 'findings_ready');
  assert.equal(response.session_count, 2);
  assert.equal(response.sessions.length, 2);
  assert.equal(response.sessions[0].session_id, 'session-1');
  assert.equal(response.sessions[1].provider, 'codex');
});

test('normalizeStatusEnvelope reports no_local_session for zero-session probes', () => {
  const response = normalizeStatusEnvelope({ ok: true, session_count: 0 });

  assert.ok(response);
  assert.equal(response.ok, true);
  assert.equal(response.mode, 'no_local_session');
  assert.equal(response.session_count, 0);
  assert.equal(response.attention_state, undefined);
  assert.equal(response.freshness_seconds, undefined);
  assert.match(response.message, /no local roger review session/i);
});

test('normalizeStatusEnvelope reports session_inventory when attention claim is stale', () => {
  const response = normalizeStatusEnvelope({
    ok: true,
    attention_state: 'findings_ready',
    freshness_seconds: MAX_MIRROR_FRESHNESS_SECONDS + 1,
    session_count: 2,
    sessions: [
      { session_id: 'session-1', provider: 'claude', attention_state: 'findings_ready' },
      { session_id: 'session-2', provider: 'codex', attention_state: 'awaiting_user_input' },
    ],
  });

  assert.ok(response);
  assert.equal(response.ok, true);
  assert.equal(response.mode, 'session_inventory');
  assert.equal(response.session_count, 2);
  assert.equal(response.sessions.length, 2);
  // No top-level attention claim may survive: it is stale, not fresh truth.
  assert.equal(response.attention_state, undefined);
  assert.equal(response.freshness_seconds, undefined);
  assert.match(response.message, /no fresh attention claim/i);
  const combined = `${response.message} ${response.guidance}`.toLowerCase();
  assert.doesNotMatch(combined, /ready to post/);
});

test('normalizeStatusEnvelope keeps genuinely-unknown responses null for launch-only fallback', () => {
  assert.equal(normalizeStatusEnvelope(null), null);
  assert.equal(normalizeStatusEnvelope({ ok: true }), null);
  assert.equal(normalizeStatusEnvelope({ ok: true, session_count: -1 }), null);
  assert.equal(normalizeStatusEnvelope({ ok: true, session_count: 'two' }), null);
});

test('parseSessionInventory sanitizes malformed entries and caps the session list', () => {
  const inventory = parseSessionInventory({
    session_count: 9,
    sessions: [
      { session_id: 'session-1' },
      { provider: 'claude' },
      'session-3',
      null,
      { session_id: 'session-4' },
      { session_id: 'session-5' },
      { session_id: 'session-6' },
      { session_id: 'session-7' },
      { session_id: 'session-8' },
    ],
  });

  assert.equal(inventory.session_count, 9);
  assert.ok(inventory.sessions.length <= MAX_SESSION_INVENTORY_ENTRIES);
  for (const session of inventory.sessions) {
    assert.equal(typeof session.session_id, 'string');
  }
});

test('handleStatusMessage surfaces durable session inventory modes from the probe', async () => {
  const intentPayload = {
    intent: { owner: 'octo', repo: 'roger-reviewer', pr_number: 42 },
  };

  const emptyResponse = await handleStatusMessage(intentPayload, async () =>
    normalizeStatusEnvelope({ ok: true, session_count: 0 })
  );
  assert.equal(emptyResponse.mode, 'no_local_session');
  assert.equal(emptyResponse.session_count, 0);

  const inventoryResponse = await handleStatusMessage(intentPayload, async () =>
    normalizeStatusEnvelope({
      ok: true,
      session_count: 3,
      sessions: [
        { session_id: 'session-1' },
        { session_id: 'session-2' },
        { session_id: 'session-3' },
      ],
    })
  );
  assert.equal(inventoryResponse.mode, 'session_inventory');
  assert.equal(inventoryResponse.session_count, 3);
  assert.equal(inventoryResponse.sessions.length, 3);
});

test('handleStatusMessage rejects malformed status request payload', async () => {
  const response = await handleStatusMessage({}, async () => null);

  assert.equal(response.ok, false);
  assert.equal(response.mode, 'invalid_request');
});

test('detectBrowserLabel maps known user-agent signatures', () => {
  assert.equal(detectBrowserLabel('Mozilla/5.0 Edg/124.0.0.0'), 'edge');
  assert.equal(
    detectBrowserLabel('Mozilla/5.0 Chrome/124.0.0.0 Safari/537.36 Brave/124'),
    'brave'
  );
  assert.equal(detectBrowserLabel('Mozilla/5.0 Chrome/124.0.0.0'), 'chrome');
});

test('buildRegistrationIntent emits bridge registration envelope', () => {
  const intent = buildRegistrationIntent('abcdefghijklmnopabcdefghijklmnop', 'chrome');

  assert.equal(intent.action, 'register_extension_identity');
  assert.equal(intent.owner, 'roger');
  assert.equal(intent.repo, 'roger-reviewer');
  assert.equal(intent.pr_number, 0);
  assert.equal(intent.extension_id, 'abcdefghijklmnopabcdefghijklmnop');
  assert.equal(intent.browser, 'chrome');
});

test('registerRuntimeIdentity dispatches runtime id to native bridge', async () => {
  const previousChrome = global.chrome;
  global.chrome = {
    runtime: {
      id: 'abcdefghijklmnopabcdefghijklmnop',
    },
  };

  try {
    let dispatchedIntent = null;
    const response = await registerRuntimeIdentity(async (intent) => {
      dispatchedIntent = intent;
      return { ok: true, action: intent.action, message: 'registered' };
    });

    assert.equal(response.ok, true);
    assert.equal(dispatchedIntent.action, 'register_extension_identity');
    assert.equal(dispatchedIntent.extension_id, 'abcdefghijklmnopabcdefghijklmnop');
    assert.equal(dispatchedIntent.pr_number, 0);
  } finally {
    if (previousChrome === undefined) {
      delete global.chrome;
    } else {
      global.chrome = previousChrome;
    }
  }
});

// ---------------------------------------------------------------------------
// Bounded local-parity actions (bead rr-surface-parity-epic-rfa2.3): new
// actions pass the SUPPORTED_ACTIONS gate, and dispatchNative relays the new
// bounded response fields to the content script.
// ---------------------------------------------------------------------------

test('handleLaunchMessage accepts the new bounded local-parity actions', async () => {
  const actions = [
    'triage_finding',
    'show_drafts',
    'revise_draft',
    'request_clarification',
    'search',
    'timeline',
  ];
  for (const action of actions) {
    const result = await handleLaunchMessage(
      { intent: { action, owner: 'acme', repo: 'widgets', pr_number: 42 } },
      null,
      { dispatch: async () => ({ ok: true, mode: 'native_messaging', action, message: 'ok' }) }
    );
    assert.equal(result.ok, true, `${action} should pass the supported-action gate`);
    assert.notEqual(result.mode, 'invalid_request');
  }
});

test('handleLaunchMessage still rejects an unsupported action with the updated guidance', async () => {
  const result = await handleLaunchMessage(
    { intent: { action: 'delete_repo', owner: 'acme', repo: 'widgets', pr_number: 42 } },
    null,
    { dispatch: async () => ({ ok: true }) }
  );
  assert.equal(result.ok, false);
  assert.equal(result.mode, 'invalid_request');
  assert.match(result.guidance, /triage_finding/);
});

function fakeNativePort(frame) {
  const listeners = [];
  return {
    onMessage: { addListener: (fn) => listeners.push(fn) },
    onDisconnect: { addListener: () => {} },
    postMessage() {
      // Deliver the single final frame on the next tick, mimicking the host.
      for (const fn of listeners) {
        fn(frame);
      }
    },
    disconnect() {},
  };
}

test('dispatchNative relays the new bounded fields (findings/drafts/search_results/timeline/clarification_ack)', async () => {
  const frame = {
    ok: true,
    action: 'show_drafts',
    message: 'relayed',
    findings: { items: [{ finding_id: 'f1' }] },
    drafts: { items: [{ finding_id: 'f2', outbound_detail: { draft_id: 'd1' } }] },
    search_results: { matches: [{ title: 'prior' }] },
    timeline: { events: [{ kind: 'run' }] },
    clarification_ack: { clarification_id: 'c1' },
  };
  const previousChrome = global.chrome;
  global.chrome = { runtime: { connectNative: () => fakeNativePort(frame), lastError: null } };
  try {
    const result = await dispatchNative(
      { action: 'show_drafts', owner: 'acme', repo: 'widgets', pr_number: 42 },
      () => {}
    );
    assert.equal(result.ok, true);
    assert.deepEqual(result.drafts, frame.drafts);
    assert.deepEqual(result.findings, frame.findings);
    assert.deepEqual(result.search_results, frame.search_results);
    assert.deepEqual(result.timeline, frame.timeline);
    assert.deepEqual(result.clarification_ack, frame.clarification_ack);
  } finally {
    global.chrome = previousChrome;
  }
});
