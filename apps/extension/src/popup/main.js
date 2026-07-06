const ACTIONS = [
  { id: 'start_review', label: 'Start Review in Roger', hierarchy: 'primary' },
  { id: 'resume_review', label: 'Resume Existing Review', hierarchy: 'secondary' },
  { id: 'show_findings', label: 'View Findings', hierarchy: 'secondary' },
];
const NON_PR_SUBTITLE =
  'Open a GitHub pull request tab to enable manual backup launch actions.';
const PR_SUBTITLE =
  'Manual backup controls for this pull request. Prefer in-page Roger controls when available.';
const FINDINGS_VISIBLE_ATTENTION_STATES = new Set(['findings_ready']);
const RESUME_PRIMARY_ATTENTION_STATES = new Set([
  'awaiting_user_input',
  'awaiting_outbound_approval',
  'refresh_recommended',
  'review_failed',
]);

const SUPPORTED_ACTIONS = new Set(ACTIONS.map((action) => action.id));
const ACTION_LABELS = new Map(ACTIONS.map((action) => [action.id, action.label]));

function parsePullRequestContextFromUrl(rawUrl) {
  if (typeof rawUrl !== 'string' || rawUrl.length === 0) {
    return null;
  }

  let parsedUrl;
  try {
    parsedUrl = new URL(rawUrl);
  } catch {
    return null;
  }

  if (parsedUrl.hostname !== 'github.com') {
    return null;
  }

  const match = parsedUrl.pathname.match(/^\/([^/]+)\/([^/]+)\/pull\/(\d+)(?:\/|$)/);
  if (!match) {
    return null;
  }

  return {
    owner: decodeURIComponent(match[1]),
    repo: decodeURIComponent(match[2]),
    pr_number: Number(match[3]),
  };
}

function buildPopupViewModel(rawUrl) {
  const context = parsePullRequestContextFromUrl(rawUrl);
  if (!context) {
    return {
      mode: 'non_pr',
      context: null,
      title: 'Roger Reviewer',
      subtitle: NON_PR_SUBTITLE,
      attentionState: null,
    };
  }

  return {
    mode: 'pr',
    context,
    title: `Roger: ${context.owner}/${context.repo}#${context.pr_number}`,
    subtitle: PR_SUBTITLE,
    attentionState: null,
  };
}

function buildLaunchMessage(action, context) {
  if (!SUPPORTED_ACTIONS.has(action)) {
    throw new Error(`Unsupported action: ${String(action)}`);
  }

  if (
    !context ||
    typeof context.owner !== 'string' ||
    typeof context.repo !== 'string' ||
    typeof context.pr_number !== 'number'
  ) {
    throw new Error('Missing pull request context for launch action.');
  }

  return {
    type: 'roger_bridge_launch',
    intent: {
      action,
      owner: context.owner,
      repo: context.repo,
      pr_number: context.pr_number,
    },
  };
}

function buildStatusMessage(context) {
  if (
    !context ||
    typeof context.owner !== 'string' ||
    typeof context.repo !== 'string' ||
    typeof context.pr_number !== 'number'
  ) {
    throw new Error('Missing pull request context for status request.');
  }

  return {
    type: 'roger_bridge_status',
    intent: {
      owner: context.owner,
      repo: context.repo,
      pr_number: context.pr_number,
    },
  };
}

const BRIDGE_DISCONNECT_GUIDANCE =
  'Native bridge unreachable. Run `rr extension setup --browser <edge|chrome|brave>`, then `rr extension doctor --browser <edge|chrome|brave>`, and reload this page.';

function normalizeGuidanceText(guidance) {
  if (Array.isArray(guidance)) {
    return guidance
      .filter((entry) => typeof entry === 'string' && entry.trim().length > 0)
      .map((entry) => entry.trim())
      .join(' ');
  }
  return typeof guidance === 'string' ? guidance.trim() : '';
}

function appendGuidance(message, guidance) {
  const base = typeof message === 'string' ? message.trim() : '';
  const extra = normalizeGuidanceText(guidance);

  if (!base) {
    return extra;
  }
  if (!extra) {
    return base;
  }

  const normalizedBase = /[.!?]$/.test(base) ? base : `${base}.`;
  return `${normalizedBase} ${extra}`.trim();
}

// Map a host launch-progress stage to the one-liner the popup shows in its
// subtitle while a launch is in flight (mirrors the in-page panel wording).
function describeLaunchProgress(stage) {
  if (stage === 'host_started') {
    return 'Roger host connected — running preflight…';
  }
  if (stage === 'preflight_ok') {
    return 'Launching review…';
  }
  return null;
}

function routePopupAction(action, context, dispatch) {
  if (typeof dispatch !== 'function') {
    throw new Error('Popup action dispatcher must be a function.');
  }

  return dispatch(buildLaunchMessage(action, context));
}

function normalizeSessionCount(sessionCount) {
  if (
    typeof sessionCount !== 'number' ||
    !Number.isFinite(sessionCount) ||
    sessionCount < 0
  ) {
    return null;
  }
  return Math.floor(sessionCount);
}

function resolveSessionCount(response) {
  if (!response || typeof response !== 'object') {
    return null;
  }
  return normalizeSessionCount(response.session_count);
}

// Session existence is durable local truth from the status probe:
// - 0 sessions: there is nothing to resume, so no resume button at all.
// - >=1 sessions: an existing review makes resuming the likeliest intent.
// - null/undefined (unknown inventory): keep the legacy both-buttons surface.
function deriveActionModel(attentionState, sessionCount = null) {
  const knownSessionCount = normalizeSessionCount(sessionCount);
  const visibleActions = new Set(['start_review']);
  let primaryActionId = 'start_review';

  if (knownSessionCount === null || knownSessionCount >= 1) {
    visibleActions.add('resume_review');
  }
  if (knownSessionCount !== null && knownSessionCount >= 1) {
    primaryActionId = 'resume_review';
  }

  if (FINDINGS_VISIBLE_ATTENTION_STATES.has(attentionState)) {
    visibleActions.add('show_findings');
    primaryActionId = 'show_findings';
  } else if (
    RESUME_PRIMARY_ATTENTION_STATES.has(attentionState) &&
    visibleActions.has('resume_review')
  ) {
    primaryActionId = 'resume_review';
  }

  const resumeBaseLabel = ACTION_LABELS.get('resume_review');
  const resumeLabel =
    knownSessionCount !== null && knownSessionCount > 1
      ? `${resumeBaseLabel} (${knownSessionCount})`
      : resumeBaseLabel;

  return {
    visibleActions,
    primaryActionId,
    resumeLabel,
    sessionCount: knownSessionCount,
  };
}

function resolveFindingsKnownEmpty(response) {
  if (!response || typeof response !== 'object') {
    return null;
  }

  if (typeof response.finding_count === 'number') {
    return response.finding_count <= 0;
  }
  if (typeof response.has_findings === 'boolean') {
    return !response.has_findings;
  }
  if (typeof response.attention_state === 'string') {
    return !FINDINGS_VISIBLE_ATTENTION_STATES.has(response.attention_state);
  }
  return null;
}

function resolveAttentionState(response) {
  if (!response || typeof response !== 'object') {
    return null;
  }

  if (typeof response.attention_state === 'string') {
    return response.attention_state;
  }

  return resolveFindingsKnownEmpty(response) === false ? 'findings_ready' : null;
}

function describeLaunchResponse(response) {
  if (!response) {
    return {
      message: 'No launch response received. Open Roger locally and run the equivalent rr command.',
      isError: true,
      findingsKnownEmpty: null,
      attentionState: null,
    };
  }

  if (!response.ok) {
    return {
      message: appendGuidance(response.message || 'Launch failed.', response.guidance),
      isError: true,
      findingsKnownEmpty: null,
      attentionState: null,
    };
  }

  if (response.mode === 'custom_url_fallback') {
    return {
      message:
        response.message ||
        'Native bridge unavailable; launched via URL fallback. Open Roger locally for full status.',
      isError: false,
      findingsKnownEmpty: resolveFindingsKnownEmpty(response),
      attentionState: resolveAttentionState(response),
    };
  }

  return {
    message: appendGuidance(response.message || 'Launch intent dispatched.', response.guidance),
    isError: false,
    findingsKnownEmpty: resolveFindingsKnownEmpty(response),
    attentionState: resolveAttentionState(response),
  };
}

function queryActiveTab(queryTabs = null) {
  const queryFn =
    queryTabs ||
    ((callback) => chrome.tabs.query({ active: true, lastFocusedWindow: true }, callback));

  return new Promise((resolve, reject) => {
    queryFn((tabs) => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
        return;
      }
      resolve(Array.isArray(tabs) ? tabs[0] : null);
    });
  });
}

function sendRuntimeMessage(payload) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendMessage(payload, (response) => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
        return;
      }
      resolve(response || null);
    });
  });
}

function readExtensionBuildLabel(manifestProvider = null) {
  const provider = manifestProvider || (() => chrome.runtime.getManifest());
  try {
    const manifest = provider();
    if (!manifest || typeof manifest !== 'object') {
      return '';
    }
    return manifest.version_name || manifest.version || '';
  } catch {
    return '';
  }
}

function describeBuildInfo(label) {
  if (!label) {
    return 'Extension build unavailable.';
  }
  return `Extension build ${label}.`;
}

function renderBuildLabel(label) {
  const buildNode = document.getElementById('popup-build-info');
  if (!buildNode) {
    return;
  }
  buildNode.textContent = describeBuildInfo(label);
}

function setSubtitle(text, isError = false) {
  const subtitle = document.getElementById('popup-subtitle');
  if (!subtitle) {
    return;
  }

  subtitle.textContent = text;
  subtitle.classList.toggle('status-error', isError);
}

function setButtonsDisabled(disabled) {
  const buttons = document.querySelectorAll('button[data-action]');
  for (const button of buttons) {
    if (button.hidden) {
      continue;
    }
    button.disabled = disabled;
  }
}

function applyActionModel(attentionState, sessionCount = null) {
  const model = deriveActionModel(attentionState, sessionCount);
  const buttons = document.querySelectorAll('button[data-action]');
  for (const button of buttons) {
    const action = button.dataset.action;
    const isVisible = model.visibleActions.has(action);
    const isPrimary = action === model.primaryActionId;
    const isTertiary = action === 'show_findings' && isVisible && !isPrimary;
    button.hidden = !isVisible;
    if (action === 'resume_review' && button.textContent !== model.resumeLabel) {
      button.textContent = model.resumeLabel;
    }
    button.classList.toggle('action-primary', isPrimary && isVisible);
    button.classList.toggle('action-secondary', !isPrimary && !isTertiary && isVisible);
    button.classList.toggle('action-tertiary', isTertiary);
  }
  return model;
}

function wireInfoAffordance() {
  const details = document.getElementById('popup-info');
  const toggle = document.getElementById('popup-info-toggle');
  if (!toggle || !details) {
    return;
  }

  const syncToggleLabel = () => {
    toggle.textContent = details.open ? 'Hide Info' : 'Build and fallback details';
  };

  syncToggleLabel();
  details.addEventListener('toggle', syncToggleLabel);
}

async function handleActionClick(action, context, button) {
  const previousLabel = button.textContent;
  button.disabled = true;
  button.textContent = 'Launching…';
  setSubtitle('Sending launch request…');

  try {
    const response = await routePopupAction(action, context, sendRuntimeMessage);
    const feedback = describeLaunchResponse(response);
    setSubtitle(feedback.message, feedback.isError);
    applyActionModel(feedback.attentionState, resolveSessionCount(response));
  } catch (error) {
    setSubtitle(
      appendGuidance(`Bridge error: ${String(error?.message || error)}`, BRIDGE_DISCONNECT_GUIDANCE),
      true
    );
  } finally {
    button.disabled = false;
    button.textContent = previousLabel;
  }
}

function renderViewModel(viewModel) {
  const title = document.getElementById('popup-title');
  if (title) {
    title.textContent = viewModel.title;
  }
  setSubtitle(viewModel.subtitle);
  applyActionModel(viewModel.attentionState || null);

  if (viewModel.mode !== 'pr' || !viewModel.context) {
    setButtonsDisabled(true);
    return;
  }

  const buttons = document.querySelectorAll('button[data-action]');
  for (const button of buttons) {
    const action = button.dataset.action;
    if (!SUPPORTED_ACTIONS.has(action)) {
      button.disabled = true;
      continue;
    }

    button.textContent = ACTION_LABELS.get(action) || action;
    button.disabled = false;
    button.addEventListener('click', () => handleActionClick(action, viewModel.context, button));
  }
}

async function syncPopupActionModel(context) {
  if (!context) {
    return null;
  }

  try {
    const response = await sendRuntimeMessage(buildStatusMessage(context));
    const attentionState = resolveAttentionState(response);
    applyActionModel(attentionState, resolveSessionCount(response));
    return attentionState;
  } catch {
    applyActionModel(null, null);
    return null;
  }
}

// Render the background worker's launch-progress fan-out (delivered to the
// popup over the runtime broadcast, since the popup is not a tab) into the
// subtitle. Best-effort: a dropped frame never affects the final launch result.
function handleLaunchProgressMessage(message) {
  if (!message || message.type !== 'roger_bridge_launch_progress') {
    return false;
  }
  const text = describeLaunchProgress(message.stage);
  if (!text) {
    return false;
  }
  setSubtitle(text);
  return true;
}

function registerLaunchProgressListener() {
  if (typeof chrome === 'undefined' || !chrome.runtime?.onMessage?.addListener) {
    return;
  }
  chrome.runtime.onMessage.addListener((message) => {
    handleLaunchProgressMessage(message);
    return false;
  });
}

async function bootstrapPopup() {
  try {
    wireInfoAffordance();
    registerLaunchProgressListener();
    renderBuildLabel(readExtensionBuildLabel());
    const activeTab = await queryActiveTab();
    const viewModel = buildPopupViewModel(activeTab?.url || '');
    renderViewModel(viewModel);
    if (viewModel.mode === 'pr' && viewModel.context) {
      await syncPopupActionModel(viewModel.context);
    }
  } catch (error) {
    setButtonsDisabled(true);
    setSubtitle(`Unable to read active tab: ${String(error?.message || error)}`, true);
  }
}

if (typeof document !== 'undefined' && typeof chrome !== 'undefined') {
  bootstrapPopup();
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    ACTIONS,
    BRIDGE_DISCONNECT_GUIDANCE,
    appendGuidance,
    describeLaunchProgress,
    handleLaunchProgressMessage,
    registerLaunchProgressListener,
    NON_PR_SUBTITLE,
    PR_SUBTITLE,
    FINDINGS_VISIBLE_ATTENTION_STATES,
    RESUME_PRIMARY_ATTENTION_STATES,
    SUPPORTED_ACTIONS,
    applyActionModel,
    buildLaunchMessage,
    buildStatusMessage,
    buildPopupViewModel,
    describeLaunchResponse,
    describeBuildInfo,
    deriveActionModel,
    normalizeSessionCount,
    parsePullRequestContextFromUrl,
    readExtensionBuildLabel,
    renderBuildLabel,
    resolveAttentionState,
    resolveSessionCount,
    resolveFindingsKnownEmpty,
    routePopupAction,
    syncPopupActionModel,
  };
}
