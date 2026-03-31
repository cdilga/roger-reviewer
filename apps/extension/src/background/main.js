const BRIDGE_HOST = 'com.roger_reviewer.bridge';
const SUPPORTED_ACTIONS = new Set([
  'start_review',
  'resume_review',
  'show_findings',
  'refresh_review',
]);

function buildCustomUrl(intent) {
  const base = `roger://launch/${encodeURIComponent(intent.owner)}/${encodeURIComponent(intent.repo)}/${intent.pr_number}`;
  const params = new URLSearchParams();
  params.set('action', intent.action);
  if (intent.instance) {
    params.set('instance', intent.instance);
  }
  return `${base}?${params.toString()}`;
}

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

      resolve({
        ok: Boolean(response.ok),
        mode: 'native_messaging',
        action: response.action,
        message: response.message || 'Bridge handled launch request.',
        guidance: response.guidance,
        session_id: response.session_id,
      });
    });
  });
}

async function launchViaCustomUrl(intent, fallbackReason) {
  const url = buildCustomUrl(intent);
  await chrome.tabs.create({ url });
  return {
    ok: true,
    mode: 'custom_url_fallback',
    action: intent.action,
    message:
      'Native Messaging unavailable; launched via custom URL fallback. Open Roger locally for authoritative status.',
    guidance: fallbackReason,
  };
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

  return launchViaCustomUrl(intent, nativeResult.message || nativeResult.guidance || null);
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
    sendResponse({
      ok: true,
      mode: 'launch_only',
      message: 'Launch-only mode. Live local status is not available in-extension.',
      guidance: 'Open Roger locally (`rr status`) for authoritative session state.',
    });
    return false;
  }

  return false;
});
