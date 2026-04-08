const test = require('node:test');
const assert = require('node:assert/strict');

const {
  buildRegistrationIntent,
  detectBrowserLabel,
  MAX_MIRROR_FRESHNESS_SECONDS,
  handleStatusMessage,
  launchOnlyStatusEnvelope,
  normalizeBoundedStatus,
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
