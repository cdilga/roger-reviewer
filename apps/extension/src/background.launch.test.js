const test = require('node:test');
const assert = require('node:assert/strict');

const { handleLaunchMessage } = require('./background/main.js');

function withChromeStub(stub, fn) {
  const previousChrome = global.chrome;
  global.chrome = stub;
  return Promise.resolve()
    .then(fn)
    .finally(() => {
      if (previousChrome === undefined) {
        delete global.chrome;
      } else {
        global.chrome = previousChrome;
      }
    });
}

test('handleLaunchMessage fails closed when Native Messaging is unavailable', async () => {
  let tabCreateCalled = false;
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage(_host, _intent, callback) {
        this.lastError = { message: 'Specified native messaging host not found.' };
        callback(undefined);
        this.lastError = null;
      },
    },
    tabs: {
      async create() {
        tabCreateCalled = true;
      },
    },
  };

  await withChromeStub(chromeStub, async () => {
    const response = await handleLaunchMessage({
      intent: {
        action: 'start_review',
        owner: 'acme',
        repo: 'widgets',
        pr_number: 42,
      },
    });

    assert.equal(response.ok, false);
    assert.equal(response.mode, 'native_unavailable');
    assert.equal(response.action, 'start_review');
    assert.match(response.message, /launch blocked/i);
    assert.match(response.guidance, /host is not registered/i);
    assert.match(response.guidance, /rr extension setup --browser <edge\|chrome\|brave>/i);
    assert.match(response.guidance, /rr extension doctor --browser <edge\|chrome\|brave>/i);
    assert.match(response.guidance, /reload the browser extension/i);
    assert.match(response.guidance, /RR_STORE_ROOT/i);
    assert.equal(tabCreateCalled, false);
  });
});

test('handleLaunchMessage surfaces forbidden native host access as browser-policy guidance', async () => {
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage(_host, _intent, callback) {
        this.lastError = {
          message: 'Access to the specified native messaging host is forbidden.',
        };
        callback(undefined);
        this.lastError = null;
      },
    },
  };

  await withChromeStub(chromeStub, async () => {
    const response = await handleLaunchMessage({
      intent: {
        action: 'start_review',
        owner: 'acme',
        repo: 'widgets',
        pr_number: 42,
      },
    });

    assert.equal(response.ok, false);
    assert.equal(response.mode, 'native_unavailable');
    assert.equal(response.action, 'start_review');
    assert.match(response.message, /launch blocked/i);
    assert.match(response.guidance, /registered but this browser profile is not allowed/i);
    assert.match(response.guidance, /rr extension setup --browser <edge\|chrome\|brave>/i);
    assert.match(response.guidance, /rr extension doctor --browser <edge\|chrome\|brave>/i);
    assert.match(response.guidance, /extension id matches the host manifest allowed origin/i);
    assert.match(response.guidance, /fully quit and relaunch the browser/i);
    assert.match(response.guidance, /browser-side policy rejection/i);
    assert.match(response.guidance, /sacrificial-profile\/manual rehearsal/i);
  });
});

for (const action of ['start_review', 'resume_review', 'show_findings']) {
  test(`handleLaunchMessage preserves native messaging success envelope for ${action}`, async () => {
    const generatedAt = new Date(Date.now() - 2_000).toISOString();
    const chromeStub = {
      runtime: {
        lastError: null,
        onMessage: { addListener: () => {} },
        sendNativeMessage(_host, _intent, callback) {
          callback({
            ok: true,
            action,
            message: `Dispatching ${action} for acme/widgets#42`,
            guidance: null,
            session_id: `session-${action}`,
            attention_state: 'awaiting_user_input',
            generated_at: generatedAt,
            status: {
              schema_id: 'rr.robot.status.v1',
              outcome: 'complete',
              generated_at: generatedAt,
              session_id: `session-${action}`,
              attention_state: 'awaiting_user_input',
            },
          });
        },
      },
    };

    await withChromeStub(chromeStub, async () => {
      const response = await handleLaunchMessage({
        intent: {
          action,
          owner: 'acme',
          repo: 'widgets',
          pr_number: 42,
        },
      });

      assert.equal(response.ok, true);
      assert.equal(response.mode, 'native_messaging');
      assert.equal(response.action, action);
      assert.match(response.message, new RegExp(`Dispatching ${action}`));
      assert.equal(response.session_id, `session-${action}`);
      assert.equal(response.attention_state, 'awaiting_user_input');
      assert.equal(response.launch_outcome, undefined);
      assert.equal(typeof response.freshness_seconds, 'number');
      assert.match(response.freshness_label, /old$/);
    });
  });
}

test('handleLaunchMessage keeps degraded bridge launch outcome explicit', async () => {
  const generatedAt = new Date(Date.now() - 1_000).toISOString();
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage(_host, _intent, callback) {
        callback({
          ok: true,
          action: 'resume_review',
          message:
            'rr resume completed in degraded mode for acme/widgets#42. Open Roger locally with `rr status --session session-resume` for authoritative detail.',
          guidance: null,
          session_id: 'session-resume',
          launch_outcome: 'degraded',
          attention_state: 'review_failed',
          generated_at: generatedAt,
          status: {
            schema_id: 'rr.robot.status.v1',
            outcome: 'complete',
            generated_at: generatedAt,
            session_id: 'session-resume',
            attention_state: 'review_failed',
          },
        });
      },
    },
  };

  await withChromeStub(chromeStub, async () => {
    const response = await handleLaunchMessage({
      intent: {
        action: 'resume_review',
        owner: 'acme',
        repo: 'widgets',
        pr_number: 42,
      },
    });

    assert.equal(response.ok, true);
    assert.equal(response.mode, 'native_messaging');
    assert.equal(response.launch_outcome, 'degraded');
    assert.equal(response.attention_state, 'review_failed');
    assert.match(response.message, /degraded mode/i);
    assert.match(response.message, /rr status --session session-resume/);
  });
});

for (const [label, bridgeResponse, expectedMode] of [
  [
    'preflight failure',
    {
      ok: false,
      action: 'start_review',
      message: 'Roger bridge preflight failed.',
      guidance: 'Roger data directory not found. Run `rr init` to set up.',
      failure_kind: 'preflight_failed',
    },
    'bridge_preflight_failed',
  ],
  [
    'CLI spawn failure',
    {
      ok: false,
      action: 'start_review',
      message: 'Failed to invoke rr review through Roger bridge.',
      guidance: 'Run `rr doctor` to inspect local setup, then retry `rr review --repo acme/widgets --pr 42 --robot --robot-format json`.',
      failure_kind: 'cli_spawn_failed',
    },
    'bridge_cli_spawn_failed',
  ],
  [
    'robot schema mismatch',
    {
      ok: false,
      action: 'show_findings',
      message: 'rr findings returned a non-canonical --robot payload.',
      guidance: 'Open Roger locally and rerun `rr findings --repo acme/widgets --pr 42 --robot --robot-format json` for authoritative details.',
      failure_kind: 'robot_schema_mismatch',
    },
    'bridge_robot_schema_mismatch',
  ],
  [
    'missing canonical session id',
    {
      ok: false,
      action: 'resume_review',
      message: 'rr resume completed without a canonical Roger session id.',
      guidance: 'Open Roger locally and rerun `rr resume --repo acme/widgets --pr 42 --robot --robot-format json` for authoritative recovery.',
      failure_kind: 'missing_session_id',
    },
    'bridge_missing_session_id',
  ],
  [
    'blocked CLI outcome',
    {
      ok: false,
      action: 'start_review',
      message: "rr review reported bridge-unsafe outcome 'blocked'.",
      guidance: 'Repair actions: rr review --repo acme/widgets --pr 42',
      failure_kind: 'cli_outcome_not_safe',
      launch_outcome: 'blocked',
    },
    'bridge_cli_blocked',
  ],
]) {
  test(`handleLaunchMessage preserves ${label} distinctly`, async () => {
    const chromeStub = {
      runtime: {
        lastError: null,
        onMessage: { addListener: () => {} },
        sendNativeMessage(_host, _intent, callback) {
          callback(bridgeResponse);
        },
      },
    };

    await withChromeStub(chromeStub, async () => {
      const response = await handleLaunchMessage({
        intent: {
          action: bridgeResponse.action,
          owner: 'acme',
          repo: 'widgets',
          pr_number: 42,
        },
      });

      assert.equal(response.ok, false);
      assert.equal(response.mode, expectedMode);
      assert.equal(response.failure_kind, bridgeResponse.failure_kind);
      assert.equal(response.launch_outcome, bridgeResponse.launch_outcome);
      assert.match(response.message, /\S/);
      assert.match(response.guidance, /rr /);
    });
  });
}

test('handleLaunchMessage rejects refresh_review as a browser action', async () => {
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage() {
        throw new Error('native dispatch should not be reached');
      },
    },
  };

  await withChromeStub(chromeStub, async () => {
    const response = await handleLaunchMessage({
      intent: {
        action: 'refresh_review',
        owner: 'acme',
        repo: 'widgets',
        pr_number: 42,
      },
    });

    assert.equal(response.ok, false);
    assert.equal(response.mode, 'invalid_request');
    assert.match(response.guidance, /Supported actions:/);
  });
});
