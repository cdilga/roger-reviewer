const ACTIONS = [
  { id: 'start_review', label: 'Start Review in Roger', hierarchy: 'primary' },
  { id: 'resume_review', label: 'Resume Existing Review', hierarchy: 'secondary' },
  { id: 'show_findings', label: 'View Findings', hierarchy: 'secondary' },
];
const NON_PR_SUBTITLE =
  'Open a GitHub pull request tab to enable manual backup launch actions.';
const PR_SUBTITLE =
  'Manual backup controls for this pull request. Prefer in-page Roger controls when available.';
const FINDINGS_VISIBLE_ATTENTION_STATES = new Set([
  'findings_ready',
  'awaiting_outbound_approval',
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
      showFindings: false,
    };
  }

  return {
    mode: 'pr',
    context,
    title: `Roger: ${context.owner}/${context.repo}#${context.pr_number}`,
    subtitle: PR_SUBTITLE,
    showFindings: true,
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

function appendGuidance(message, guidance) {
  const base = typeof message === 'string' ? message.trim() : '';
  const extra = typeof guidance === 'string' ? guidance.trim() : '';

  if (!base) {
    return extra;
  }
  if (!extra) {
    return base;
  }

  const normalizedBase = /[.!?]$/.test(base) ? base : `${base}.`;
  return `${normalizedBase} ${extra}`.trim();
}

function routePopupAction(action, context, dispatch) {
  if (typeof dispatch !== 'function') {
    throw new Error('Popup action dispatcher must be a function.');
  }

  return dispatch(buildLaunchMessage(action, context));
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

function describeLaunchResponse(response) {
  if (!response) {
    return {
      message: 'No launch response received. Open Roger locally and run the equivalent rr command.',
      isError: true,
      findingsKnownEmpty: null,
    };
  }

  if (!response.ok) {
    return {
      message: appendGuidance(response.message || 'Launch failed.', response.guidance),
      isError: true,
      findingsKnownEmpty: null,
    };
  }

  if (response.mode === 'custom_url_fallback') {
    return {
      message:
        response.message ||
        'Native bridge unavailable; launched via URL fallback. Open Roger locally for full status.',
      isError: false,
      findingsKnownEmpty: resolveFindingsKnownEmpty(response),
    };
  }

  return {
    message: appendGuidance(response.message || 'Launch intent dispatched.', response.guidance),
    isError: false,
    findingsKnownEmpty: resolveFindingsKnownEmpty(response),
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

function setFindingsButtonVisibility(visible) {
  const findingsButton = document.querySelector('button[data-action="show_findings"]');
  if (!findingsButton) {
    return;
  }
  findingsButton.hidden = !visible;
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
    if (feedback.findingsKnownEmpty === true) {
      setFindingsButtonVisibility(false);
    } else if (feedback.findingsKnownEmpty === false) {
      setFindingsButtonVisibility(true);
    }
  } catch (error) {
    setSubtitle(`Bridge error: ${String(error?.message || error)}`, true);
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
  setFindingsButtonVisibility(Boolean(viewModel.showFindings));

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

async function bootstrapPopup() {
  try {
    wireInfoAffordance();
    renderBuildLabel(readExtensionBuildLabel());
    const activeTab = await queryActiveTab();
    const viewModel = buildPopupViewModel(activeTab?.url || '');
    renderViewModel(viewModel);
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
    NON_PR_SUBTITLE,
    PR_SUBTITLE,
    FINDINGS_VISIBLE_ATTENTION_STATES,
    SUPPORTED_ACTIONS,
    buildLaunchMessage,
    buildPopupViewModel,
    describeLaunchResponse,
    describeBuildInfo,
    parsePullRequestContextFromUrl,
    readExtensionBuildLabel,
    renderBuildLabel,
    resolveFindingsKnownEmpty,
    routePopupAction,
  };
}
