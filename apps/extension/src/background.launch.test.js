const test = require('node:test');
const assert = require('node:assert/strict');

const {
  handleLaunchMessage,
  handleStatusMessage,
  STATUS_PROBE_TIMEOUT_MS,
} = require('./background/main.js');

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

test('handleLaunchMessage preserves message and session id even without a fresh bounded mirror', async () => {
  // The panel renders a persistent success status line from message +
  // session_id, so the envelope must carry both even when the bridge omits
  // attention_state/generated_at and no bounded mirror can be normalized.
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage(_host, _intent, callback) {
        callback({
          ok: true,
          action: 'start_review',
          message: 'rr review completed for acme/widgets#42.',
          guidance: null,
          session_id: 'session-42',
        });
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

    assert.equal(response.ok, true);
    assert.equal(response.mode, 'native_messaging');
    assert.match(response.message, /rr review completed for acme\/widgets#42\./);
    assert.equal(response.session_id, 'session-42');
    assert.equal(response.attention_state, undefined);
  });
});

test('handleLaunchMessage failure envelopes keep message and guidance for persistent panel errors', async () => {
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage(_host, _intent, callback) {
        callback({
          ok: false,
          action: 'start_review',
          message: 'Roger bridge preflight failed.',
          guidance: 'Roger data directory not found. Run `rr init` to set up.',
          failure_kind: 'preflight_failed',
        });
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
    assert.equal(response.message, 'Roger bridge preflight failed.');
    assert.equal(
      response.guidance,
      'Roger data directory not found. Run `rr init` to set up.'
    );
  });
});

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

// --- connectNative Port stub for the Edge module-service-worker status leg ---
//
// The status probe no longer uses one-shot sendNativeMessage; it opens a
// long-lived connectNative Port so Edge's module service worker stays anchored
// until the host's single reply lands. This stub models the host contract
// (read one, write one, exit -> one onMessage then onDisconnect) and lets each
// test script the exact onMessage/onDisconnect ordering and timing.
function createNativePortStub({ behavior } = {}) {
  const messageListeners = [];
  const disconnectListeners = [];
  const port = {
    disconnected: false,
    postedMessages: [],
    onMessage: {
      addListener(fn) {
        messageListeners.push(fn);
      },
    },
    onDisconnect: {
      addListener(fn) {
        disconnectListeners.push(fn);
      },
    },
    disconnect() {
      if (this.disconnected) {
        return;
      }
      this.disconnected = true;
    },
    // Test-only drivers.
    emitMessage(response) {
      for (const fn of messageListeners) {
        fn(response);
      }
    },
    emitDisconnect() {
      for (const fn of disconnectListeners) {
        fn();
      }
    },
  };

  port.postMessage = function postMessage(message) {
    port.postedMessages.push(message);
    if (typeof behavior === 'function') {
      behavior(port, message);
    }
  };

  return port;
}

function chromeWithConnectNative(behavior) {
  let lastPort = null;
  const stub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      connectNative(_host) {
        lastPort = createNativePortStub({ behavior });
        return lastPort;
      },
    },
    getLastPort() {
      return lastPort;
    },
  };
  return stub;
}

test('status probe relays zero-session inventory as no_local_session (reply then disconnect)', async () => {
  const chromeStub = chromeWithConnectNative((port) => {
    // Host reads one, writes one, exits: message then disconnect.
    port.emitMessage({ ok: true, session_count: 0 });
    port.emitDisconnect();
  });

  await withChromeStub(chromeStub, async () => {
    const response = await handleStatusMessage({
      intent: { owner: 'acme', repo: 'widgets', pr_number: 42 },
    });

    assert.equal(response.ok, true);
    assert.equal(response.mode, 'no_local_session');
    assert.equal(response.session_count, 0);
    assert.equal(response.attention_state, undefined);

    const port = chromeStub.getLastPort();
    assert.equal(port.disconnected, true, 'port must be torn down after settling');
    assert.equal(port.postedMessages.length, 1);
    assert.deepEqual(port.postedMessages[0], {
      type: 'roger_bridge_status',
      owner: 'acme',
      repo: 'widgets',
      pr_number: 42,
    });
  });
});

test('status probe relays multi-session inventory without a fresh attention claim', async () => {
  const chromeStub = chromeWithConnectNative((port) => {
    port.emitMessage({
      ok: true,
      session_count: 2,
      sessions: [
        { session_id: 'session-1', provider: 'claude', attention_state: 'findings_ready' },
        { session_id: 'session-2', provider: 'codex', attention_state: 'review_failed' },
      ],
    });
    port.emitDisconnect();
  });

  await withChromeStub(chromeStub, async () => {
    const response = await handleStatusMessage({
      intent: { owner: 'acme', repo: 'widgets', pr_number: 42 },
    });

    assert.equal(response.ok, true);
    assert.equal(response.mode, 'session_inventory');
    assert.equal(response.session_count, 2);
    assert.equal(response.sessions.length, 2);
    assert.equal(response.attention_state, undefined);
  });
});

test('status probe still mirrors fresh bounded attention with inventory attached', async () => {
  const chromeStub = chromeWithConnectNative((port) => {
    port.emitMessage({
      ok: true,
      attention_state: 'findings_ready',
      freshness_seconds: 12,
      session_count: 1,
      sessions: [{ session_id: 'session-1', provider: 'claude' }],
    });
    port.emitDisconnect();
  });

  await withChromeStub(chromeStub, async () => {
    const response = await handleStatusMessage({
      intent: { owner: 'acme', repo: 'widgets', pr_number: 42 },
    });

    assert.equal(response.ok, true);
    assert.equal(response.mode, 'bounded_status');
    assert.equal(response.attention_state, 'findings_ready');
    assert.equal(response.session_count, 1);
    assert.equal(response.sessions.length, 1);
  });
});

test('status probe degrades to launch-only when the host disconnects without a message', async () => {
  // Host died/unreachable: the port disconnects before any reply lands.
  const chromeStub = chromeWithConnectNative((port) => {
    port.emitDisconnect();
  });

  await withChromeStub(chromeStub, async () => {
    const response = await handleStatusMessage({
      intent: { owner: 'acme', repo: 'widgets', pr_number: 42 },
    });

    assert.equal(response.ok, true);
    assert.equal(response.mode, 'launch_only');
    assert.equal(response.session_count, undefined);
    assert.equal(response.attention_state, undefined);
  });
});

test('status probe watchdog settles launch-only when a hung host never replies', async () => {
  // Host neither replies nor disconnects: the worker-side watchdog must
  // force-settle to launch-only and tear the port down so the worker is freed.
  const chromeStub = chromeWithConnectNative(() => {
    // Intentionally silent: no emitMessage, no emitDisconnect.
  });

  const realSetTimeout = global.setTimeout;
  const realClearTimeout = global.clearTimeout;
  let scheduledWatchdog = null;
  global.setTimeout = (fn, ms) => {
    scheduledWatchdog = { fn, ms };
    return 1;
  };
  global.clearTimeout = () => {};

  try {
    await withChromeStub(chromeStub, async () => {
      const pending = handleStatusMessage({
        intent: { owner: 'acme', repo: 'widgets', pr_number: 42 },
      });

      assert.ok(scheduledWatchdog, 'a watchdog timer must be armed');
      assert.equal(scheduledWatchdog.ms, STATUS_PROBE_TIMEOUT_MS);

      // Fire the watchdog as the browser would after the bound elapses.
      scheduledWatchdog.fn();

      const response = await pending;
      assert.equal(response.ok, true);
      assert.equal(response.mode, 'launch_only');

      const port = chromeStub.getLastPort();
      assert.equal(port.disconnected, true, 'watchdog must tear the hung port down');
    });
  } finally {
    global.setTimeout = realSetTimeout;
    global.clearTimeout = realClearTimeout;
  }
});

test('status probe settles exactly once and never double-settles or leaks the port', async () => {
  // Reply lands first (settle #1), then a late disconnect and a late watchdog
  // fire must be no-ops: single settle, single disconnect, no leaked port.
  let messageDriver = null;
  const chromeStub = chromeWithConnectNative((port) => {
    messageDriver = port;
  });

  const realSetTimeout = global.setTimeout;
  const realClearTimeout = global.clearTimeout;
  let scheduledWatchdog = null;
  let clearedWatchdog = false;
  global.setTimeout = (fn) => {
    scheduledWatchdog = fn;
    return 7;
  };
  global.clearTimeout = () => {
    clearedWatchdog = true;
  };

  try {
    await withChromeStub(chromeStub, async () => {
      const pending = handleStatusMessage({
        intent: { owner: 'acme', repo: 'widgets', pr_number: 42 },
      });

      // First settle: a real bounded reply.
      messageDriver.emitMessage({
        ok: true,
        attention_state: 'findings_ready',
        freshness_seconds: 5,
        session_count: 1,
        sessions: [{ session_id: 'session-1' }],
      });

      const response = await pending;
      assert.equal(response.mode, 'bounded_status');
      assert.equal(response.attention_state, 'findings_ready');

      const port = chromeStub.getLastPort();
      assert.equal(port.disconnected, true);
      assert.equal(clearedWatchdog, true, 'the watchdog must be cleared on settle');

      // Late stragglers must not throw and must not change the resolved value.
      let disconnectCountBefore = port.disconnected;
      port.emitDisconnect();
      if (typeof scheduledWatchdog === 'function') {
        scheduledWatchdog();
      }
      // Idempotent: a second emit/disconnect/watchdog leaves the port settled.
      assert.equal(port.disconnected, disconnectCountBefore);

      // The promise has only one resolution; awaiting again yields the same value.
      const again = await pending;
      assert.equal(again.mode, 'bounded_status');
    });
  } finally {
    global.setTimeout = realSetTimeout;
    global.clearTimeout = realClearTimeout;
  }
});

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
