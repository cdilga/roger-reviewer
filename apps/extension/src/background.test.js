const test = require('node:test');
const assert = require('node:assert/strict');

const {
  MAX_MIRROR_FRESHNESS_SECONDS,
  handleStatusMessage,
  launchOnlyStatusEnvelope,
  normalizeBoundedStatus,
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
