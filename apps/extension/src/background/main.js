const BRIDGE_HOST = 'com.roger_reviewer.bridge';
const EXTENSION_REGISTRATION_ACTION = 'register_extension_identity';
const SUPPORTED_ACTIONS = new Set([
  'start_review',
  'resume_review',
  'show_findings',
]);
const CANONICAL_ATTENTION_STATES = new Set([
  'awaiting_user_input',
  'awaiting_outbound_approval',
  'findings_ready',
  'refresh_recommended',
  'review_failed',
]);
const MAX_MIRROR_FRESHNESS_SECONDS = 300;
const MAX_SESSION_INVENTORY_ENTRIES = 5;
// Worker-side watchdog for the status probe. An open connectNative Port anchors
// Edge's module service worker so the host's one reply lands before teardown,
// but a hung/never-replying host could otherwise wedge the worker until the
// browser kills it. This bound force-settles to the honest launch-only degrade.
// MUST stay below the content-side settle timeout so the worker leg always
// resolves first (see STATUS_MIRROR_SETTLE_TIMEOUT_MS in content/main.js).
const STATUS_PROBE_TIMEOUT_MS = 4000;
// Worker-side watchdog for the LAUNCH dispatch. Like the status probe, launch
// uses a long-lived connectNative Port so Edge's module service worker stays
// anchored until the host's single launch-result reply lands (one-shot
// sendNativeMessage was torn down before the reply on Edge MV3 -> clicks
// silently no-oped). A launch (rr review/resume) can take real time, so this
// backstop is generous; it only force-settles a host that truly hangs.
const LAUNCH_DISPATCH_TIMEOUT_MS = 120000;
const BRIDGE_FAILURE_MODE_BY_KIND = Object.freeze({
  preflight_failed: 'bridge_preflight_failed',
  cli_spawn_failed: 'bridge_cli_spawn_failed',
  robot_schema_mismatch: 'bridge_robot_schema_mismatch',
  missing_session_id: 'bridge_missing_session_id',
  cli_outcome_not_safe: 'bridge_cli_outcome_not_safe',
});

function classifyBridgeFailureMode(response) {
  if (!response || typeof response !== 'object') {
    return 'native_bridge_failure';
  }

  const failureKind =
    typeof response.failure_kind === 'string' ? response.failure_kind : null;
  const launchOutcome =
    typeof response.launch_outcome === 'string' ? response.launch_outcome : null;

  if (failureKind === 'cli_outcome_not_safe' && launchOutcome) {
    return `bridge_cli_${launchOutcome}`;
  }

  return BRIDGE_FAILURE_MODE_BY_KIND[failureKind] || 'native_bridge_failure';
}

// Launch dispatch over a long-lived connectNative Port (NOT one-shot
// sendNativeMessage). On Edge's MV3 module service worker the one-shot reply
// callback was torn down before the native host replied, so panel launch
// clicks silently no-oped; an open port anchors the worker until the host's
// single launch-result envelope lands (read one, write one, exit -> one
// onMessage then onDisconnect). The resolved envelope shape is unchanged so
// handleLaunchMessage and the content panel behave identically.
function dispatchNative(intent) {
  return new Promise((resolve) => {
    let settled = false;
    let watchdog = null;
    let port = null;

    const settle = (value) => {
      if (settled) {
        return;
      }
      settled = true;
      if (watchdog !== null) {
        clearTimeout(watchdog);
        watchdog = null;
      }
      if (port) {
        try {
          port.disconnect();
        } catch {
          // Port may already be torn down; disconnecting twice is harmless.
        }
        port = null;
      }
      resolve(value);
    };

    const nativeError = (message) =>
      settle({ ok: false, mode: 'native_error', message });

    try {
      port = chrome.runtime.connectNative(BRIDGE_HOST);
    } catch (err) {
      resolve({
        ok: false,
        mode: 'native_error',
        message: (err && err.message) || 'connectNative is unavailable.',
      });
      return;
    }

    if (!port || typeof port.postMessage !== 'function') {
      resolve({
        ok: false,
        mode: 'native_error',
        message: 'Native messaging port is unavailable.',
      });
      return;
    }

    if (port.onMessage && typeof port.onMessage.addListener === 'function') {
      port.onMessage.addListener((response) => {
        if (!response || typeof response !== 'object') {
          settle({
            ok: false,
            mode: 'native_invalid_response',
            message: 'Bridge host returned an invalid response envelope.',
          });
          return;
        }
        const mirrored = normalizeBoundedStatus(response);
        settle({
          ok: Boolean(response.ok),
          mode: response.ok ? 'native_messaging' : classifyBridgeFailureMode(response),
          action: response.action,
          message: response.message || 'Bridge handled launch request.',
          guidance: response.guidance,
          session_id: response.session_id,
          failure_kind: response.failure_kind,
          launch_outcome: response.launch_outcome,
          ...(mirrored
            ? {
                attention_state: mirrored.attention_state,
                freshness_seconds: mirrored.freshness_seconds,
                freshness_label: mirrored.freshness_label,
              }
            : {}),
        });
      });
    }

    // Host died/unreachable or exited without writing a launch result: surface
    // it as a native_error so handleLaunchMessage degrades to the honest
    // "Native Messaging unavailable; launch blocked" guidance instead of hanging.
    if (port.onDisconnect && typeof port.onDisconnect.addListener === 'function') {
      port.onDisconnect.addListener(() => {
        const lastError = chrome.runtime?.lastError;
        nativeError(
          (lastError && lastError.message) ||
            'Native messaging host disconnected before returning a launch result.'
        );
      });
    }

    watchdog = setTimeout(() => {
      nativeError('Native messaging launch timed out before the host replied.');
    }, LAUNCH_DISPATCH_TIMEOUT_MS);

    try {
      port.postMessage(intent);
    } catch (err) {
      nativeError((err && err.message) || 'Failed to post launch intent to the native host.');
    }
  });
}

function detectBrowserLabel(userAgent = null) {
  const rawUserAgent =
    userAgent ||
    (typeof navigator !== 'undefined' && typeof navigator.userAgent === 'string'
      ? navigator.userAgent
      : '');
  const normalized = String(rawUserAgent).toLowerCase();

  if (normalized.includes('edg/')) {
    return 'edge';
  }
  if (normalized.includes('brave')) {
    return 'brave';
  }
  return 'chrome';
}

function buildRegistrationIntent(extensionId, browser) {
  return {
    action: EXTENSION_REGISTRATION_ACTION,
    owner: 'roger',
    repo: 'roger-reviewer',
    pr_number: 0,
    extension_id: extensionId,
    browser,
  };
}

async function registerRuntimeIdentity(dispatch = dispatchNative) {
  if (typeof chrome === 'undefined' || !chrome?.runtime?.id || typeof dispatch !== 'function') {
    return {
      ok: false,
      mode: 'registration_unavailable',
      message: 'Extension runtime identity registration unavailable in this context.',
    };
  }

  const extensionId = chrome.runtime.id;
  const browser = detectBrowserLabel();
  return dispatch(buildRegistrationIntent(extensionId, browser));
}

function nativeUnavailableGuidance(nativeResult) {
  const rawMessage =
    nativeResult?.message || nativeResult?.guidance || 'Native Messaging is unavailable.';
  const normalized = String(rawMessage).toLowerCase();

  if (normalized.includes('specified native messaging host not found')) {
    return [
      'Roger Native Messaging host is not registered for this browser yet.',
      'Run `rr extension setup --browser <edge|chrome|brave>` to install the host manifest.',
      'Then run `rr extension doctor --browser <edge|chrome|brave>` and reload the browser extension.',
      'If you already ran setup, make sure you are using the same Roger install and `RR_STORE_ROOT` that setup used.',
    ].join(' ');
  }

  if (normalized.includes('specified native messaging host is forbidden')) {
    return [
      'Roger Native Messaging host is registered but this browser profile is not allowed to access it yet.',
      'Rerun `rr extension setup --browser <edge|chrome|brave>` against the real browser host path, then run `rr extension doctor --browser <edge|chrome|brave>`.',
      'Confirm the discovered extension id matches the host manifest allowed origin.',
      'Then fully quit and relaunch the browser with the isolated rehearsal profile so the browser reloads the native host policy.',
      'If the host is still forbidden before the wrapper runs, this is a browser-side policy rejection and the next step is a sacrificial-profile/manual rehearsal, not GitHub posting.',
    ].join(' ');
  }

  return rawMessage;
}

function parseFreshnessSeconds(response) {
  if (typeof response.freshness_seconds === 'number' && Number.isFinite(response.freshness_seconds)) {
    return Math.max(0, Math.round(response.freshness_seconds));
  }

  if (typeof response.generated_at === 'string') {
    const generatedAt = Date.parse(response.generated_at);
    if (Number.isFinite(generatedAt)) {
      const deltaSeconds = (Date.now() - generatedAt) / 1000;
      return Math.max(0, Math.round(deltaSeconds));
    }
  }

  return null;
}

// Session EXISTENCE is durable local truth (unlike the freshness-bounded
// attention claim), so inventory fields are parsed and passed through
// independently of the 300s attention freshness window.
function parseSessionInventory(response) {
  if (!response || typeof response !== 'object') {
    return null;
  }

  const rawCount = response.session_count;
  if (typeof rawCount !== 'number' || !Number.isFinite(rawCount) || rawCount < 0) {
    return null;
  }

  const sessionCount = Math.floor(rawCount);
  const sessions = Array.isArray(response.sessions)
    ? response.sessions
        .filter(
          (entry) => entry && typeof entry === 'object' && typeof entry.session_id === 'string'
        )
        .slice(0, MAX_SESSION_INVENTORY_ENTRIES)
        .map((entry) => ({
          session_id: entry.session_id,
          ...(typeof entry.provider === 'string' ? { provider: entry.provider } : {}),
          ...(typeof entry.attention_state === 'string'
            ? { attention_state: entry.attention_state }
            : {}),
          ...(typeof entry.updated_at === 'string' ? { updated_at: entry.updated_at } : {}),
        }))
    : [];

  return { session_count: sessionCount, sessions };
}

function normalizeBoundedStatus(response) {
  if (!response || typeof response !== 'object') {
    return null;
  }

  const attentionState = response.attention_state;
  if (!CANONICAL_ATTENTION_STATES.has(attentionState)) {
    return null;
  }

  const freshnessSeconds = parseFreshnessSeconds(response);
  if (freshnessSeconds === null || freshnessSeconds > MAX_MIRROR_FRESHNESS_SECONDS) {
    return null;
  }

  const guidance =
    typeof response.guidance === 'string' && response.guidance.trim().length > 0
      ? response.guidance.trim()
      : null;
  const inventory = parseSessionInventory(response);

  return {
    ok: true,
    mode: 'bounded_status',
    attention_state: attentionState,
    freshness_seconds: freshnessSeconds,
    freshness_label: `${freshnessSeconds}s old`,
    message: 'Mirroring bounded Roger attention state from local companion.',
    guidance: 'Open Roger locally (`rr status`) for full authoritative detail.',
    ...(guidance ? { guidance } : {}),
    ...(inventory ? inventory : {}),
  };
}

function sessionInventoryStatusEnvelope(inventory) {
  if (inventory.session_count === 0) {
    return {
      ok: true,
      mode: 'no_local_session',
      session_count: 0,
      message: 'No local Roger review session exists for this pull request yet.',
      guidance: 'Start a review from this panel, or run `rr review` locally.',
    };
  }

  // Sessions exist but the attention claim is missing or stale: report the
  // durable inventory truth without bluffing any attention fields.
  return {
    ok: true,
    mode: 'session_inventory',
    session_count: inventory.session_count,
    sessions: inventory.sessions,
    message: `${inventory.session_count} local Roger review session(s) exist for this pull request; no fresh attention claim.`,
    guidance: 'Open Roger locally (`rr status`) for authoritative detail.',
  };
}

function normalizeStatusEnvelope(response) {
  const bounded = normalizeBoundedStatus(response);
  if (bounded) {
    return bounded;
  }

  const inventory = parseSessionInventory(response);
  if (inventory) {
    return sessionInventoryStatusEnvelope(inventory);
  }

  // Genuinely unknown (no/invalid native response): the caller falls back to
  // launchOnlyStatusEnvelope, the honest degraded case.
  return null;
}

function launchOnlyStatusEnvelope(reason = null) {
  return {
    ok: true,
    mode: 'launch_only',
    message:
      'Launch-only bridge mode. This browser surface can start Roger actions, but it does not own live local session state.',
    guidance:
      reason ||
      'Open Roger locally (`rr status` or `rr findings`) for authoritative session state.',
  };
}

// Edge's MV3 module service worker can tear down before a one-shot
// sendNativeMessage reply lands, so the content->background->native status
// relay never settles. A long-lived connectNative Port keeps the worker alive
// until the host replies (the host reads one message, writes one, exits — that
// maps to exactly one onMessage followed by onDisconnect). We settle EXACTLY
// ONCE behind a guard, then disconnect, and a worker-side watchdog force-settles
// to launch-only so a hung host can never wedge the worker.
function dispatchNativeStatus(intent) {
  return new Promise((resolve) => {
    let settled = false;
    let watchdog = null;
    let port = null;

    const settle = (value) => {
      if (settled) {
        return;
      }
      settled = true;
      if (watchdog !== null) {
        clearTimeout(watchdog);
        watchdog = null;
      }
      if (port) {
        try {
          port.disconnect();
        } catch {
          // Port may already be torn down; disconnecting twice is harmless.
        }
        port = null;
      }
      resolve(value);
    };

    try {
      port = chrome.runtime.connectNative(BRIDGE_HOST);
    } catch {
      // connectNative is unavailable or threw synchronously: honest degrade.
      resolve(null);
      return;
    }

    if (!port || typeof port.postMessage !== 'function') {
      resolve(null);
      return;
    }

    // Host replied: normalize its single envelope and tear the port down.
    if (port.onMessage && typeof port.onMessage.addListener === 'function') {
      port.onMessage.addListener((response) => {
        settle(normalizeStatusEnvelope(response));
      });
    }

    // Host died/unreachable or exited after its single write: if we have not
    // already settled from a message, degrade to launch-only honestly.
    if (port.onDisconnect && typeof port.onDisconnect.addListener === 'function') {
      port.onDisconnect.addListener(() => {
        settle(null);
      });
    }

    // A hung host that neither replies nor disconnects must not wedge the
    // worker: force a launch-only settle (which also disconnects the port).
    watchdog = setTimeout(() => {
      settle(null);
    }, STATUS_PROBE_TIMEOUT_MS);

    try {
      port.postMessage({
        type: 'roger_bridge_status',
        owner: intent.owner,
        repo: intent.repo,
        pr_number: intent.pr_number,
      });
    } catch {
      settle(null);
    }
  });
}

async function handleLaunchMessage(payload) {
  const intent = payload?.intent;
  if (!intent || typeof intent !== 'object') {
    return {
      ok: false,
      mode: 'invalid_request',
      message: 'Missing launch intent payload.',
      guidance: 'Reload the GitHub PR page and retry Roger launch.',
    };
  }

  if (!SUPPORTED_ACTIONS.has(intent.action)) {
    return {
      ok: false,
      mode: 'invalid_request',
      message: `Unsupported action: ${String(intent.action)}`,
      guidance: 'Supported actions: start_review, resume_review, show_findings.',
    };
  }

  const nativeResult = await dispatchNative(intent);
  if (nativeResult.ok) {
    return nativeResult;
  }

  if (
    nativeResult.mode !== 'native_error' &&
    nativeResult.mode !== 'native_invalid_response'
  ) {
    return nativeResult;
  }

  return {
    ok: false,
    mode: 'native_unavailable',
    action: intent.action,
    message: 'Native Messaging unavailable; launch blocked.',
    guidance: nativeUnavailableGuidance(nativeResult),
  };
}

async function handleStatusMessage(payload, statusProbe = dispatchNativeStatus) {
  const intent = payload?.intent;
  if (!intent || typeof intent !== 'object') {
    return {
      ok: false,
      mode: 'invalid_request',
      message: 'Missing status intent payload.',
      guidance: 'Reload the GitHub PR page and retry Roger status check.',
    };
  }

  const mirrored = await statusProbe(intent);
  return mirrored || launchOnlyStatusEnvelope();
}

function registerRuntimeHandlers() {
  if (typeof chrome === 'undefined' || !chrome?.runtime?.onMessage) {
    return;
  }

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (message?.type === 'roger_bridge_launch') {
      handleLaunchMessage(message)
        .then((response) => sendResponse(response))
        .catch((error) => {
          sendResponse({
            ok: false,
            mode: 'unexpected_error',
            message: `Bridge dispatch failed: ${String(error)}`,
            guidance: 'Open Roger locally and run the equivalent rr command.',
          });
        });
      return true;
    }

    if (message?.type === 'roger_bridge_status') {
      handleStatusMessage(message)
        .then((response) => sendResponse(response))
        .catch(() => sendResponse(launchOnlyStatusEnvelope()))
      ;
      return true;
    }

    return false;
  });
}

registerRuntimeHandlers();
registerRuntimeIdentity().catch(() => null);

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    CANONICAL_ATTENTION_STATES,
    MAX_MIRROR_FRESHNESS_SECONDS,
    MAX_SESSION_INVENTORY_ENTRIES,
    STATUS_PROBE_TIMEOUT_MS,
    buildRegistrationIntent,
    detectBrowserLabel,
    dispatchNativeStatus,
    handleLaunchMessage,
    handleStatusMessage,
    launchOnlyStatusEnvelope,
    nativeUnavailableGuidance,
    normalizeBoundedStatus,
    normalizeStatusEnvelope,
    parseFreshnessSeconds,
    parseSessionInventory,
    sessionInventoryStatusEnvelope,
    registerRuntimeIdentity,
  };
}
