const BRIDGE_HOST = 'com.roger_reviewer.bridge';
const SUPPORTED_ACTIONS = new Set([
  'start_review',
  'resume_review',
  'show_findings',
  'refresh_review',
]);
const CANONICAL_ATTENTION_STATES = new Set([
  'awaiting_user_input',
  'awaiting_outbound_approval',
  'findings_ready',
  'refresh_recommended',
  'review_failed',
]);
const MAX_MIRROR_FRESHNESS_SECONDS = 300;

function dispatchNative(intent) {
  return new Promise((resolve) => {
    chrome.runtime.sendNativeMessage(BRIDGE_HOST, intent, (response) => {
      if (chrome.runtime.lastError) {
        resolve({
          ok: false,
          mode: 'native_error',
          message: chrome.runtime.lastError.message,
        });
        return;
      }

      if (!response || typeof response !== 'object') {
        resolve({
          ok: false,
          mode: 'native_invalid_response',
          message: 'Bridge host returned an invalid response envelope.',
        });
        return;
      }

      const mirrored = normalizeBoundedStatus(response);
      resolve({
        ok: Boolean(response.ok),
        mode: 'native_messaging',
        action: response.action,
        message: response.message || 'Bridge handled launch request.',
        guidance: response.guidance,
        session_id: response.session_id,
        ...(mirrored
          ? {
              attention_state: mirrored.attention_state,
              freshness_seconds: mirrored.freshness_seconds,
              freshness_label: mirrored.freshness_label,
            }
          : {}),
      });
    });
  });
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

  return {
    ok: true,
    mode: 'bounded_status',
    attention_state: attentionState,
    freshness_seconds: freshnessSeconds,
    freshness_label: `${freshnessSeconds}s old`,
    message: 'Mirroring bounded Roger attention state from local companion.',
    guidance: 'Open Roger locally (`rr status`) for full authoritative detail.',
  };
}

function launchOnlyStatusEnvelope(reason = null) {
  return {
    ok: true,
    mode: 'launch_only',
    message: 'Launch-only mode. Live local status is not available in-extension.',
    guidance: reason || 'Open Roger locally (`rr status`) for authoritative session state.',
  };
}

function dispatchNativeStatus(intent) {
  return new Promise((resolve) => {
    chrome.runtime.sendNativeMessage(
      BRIDGE_HOST,
      {
        type: 'roger_bridge_status',
        owner: intent.owner,
        repo: intent.repo,
        pr_number: intent.pr_number,
      },
      (response) => {
        if (chrome.runtime.lastError) {
          resolve(null);
          return;
        }

        resolve(normalizeBoundedStatus(response));
      }
    );
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
      guidance: 'Supported actions: start_review, resume_review, show_findings, refresh_review.',
    };
  }

  const nativeResult = await dispatchNative(intent);
  if (nativeResult.ok) {
    return nativeResult;
  }

  return {
    ok: false,
    mode: 'native_unavailable',
    action: intent.action,
    message: 'Native Messaging unavailable; launch blocked.',
    guidance:
      nativeResult.message ||
      nativeResult.guidance ||
      'Run `rr extension setup --browser <edge|chrome|brave>` and rerun `rr extension doctor`.',
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

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    CANONICAL_ATTENTION_STATES,
    MAX_MIRROR_FRESHNESS_SECONDS,
    handleLaunchMessage,
    handleStatusMessage,
    launchOnlyStatusEnvelope,
    normalizeBoundedStatus,
    parseFreshnessSeconds,
  };
}
