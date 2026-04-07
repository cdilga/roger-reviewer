const PANEL_ID = 'roger-reviewer-panel';
const STATUS_ID = 'roger-reviewer-status';
const BADGE_ID = 'roger-reviewer-attention-badge';
const HEADING_ID = 'roger-reviewer-heading';
const INLINE_SLOT_ID = 'roger-reviewer-inline-slot';
const STYLE_ID = 'roger-reviewer-panel-style';
const INLINE_ANCHOR_SELECTORS = [
  '[class*="prc-PageHeader-Actions-"]',
  '[class*="PullRequestHeader-module__actionsAboveTitleOnNarrow__"]',
  '#partial-discussion-header .gh-header-actions',
  '.gh-header-actions',
];

const ACTIONS = [
  { id: 'start_review', label: 'Start' },
  { id: 'resume_review', label: 'Resume' },
  { id: 'show_findings', label: 'Findings' },
  { id: 'refresh_review', label: 'Refresh' },
];

const ATTENTION_STYLES = {
  awaiting_user_input: {
    label: 'Awaiting user input',
    background: 'var(--bgColor-attention-muted, #fff8c5)',
    color: 'var(--fgColor-attention, #9a6700)',
  },
  awaiting_outbound_approval: {
    label: 'Awaiting outbound approval',
    background: 'var(--bgColor-danger-emphasis, #cf222e)',
    color: 'var(--fgColor-onEmphasis, #ffffff)',
  },
  findings_ready: {
    label: 'Findings ready',
    background: 'var(--bgColor-success-muted, #dafbe1)',
    color: 'var(--fgColor-success, #1a7f37)',
  },
  refresh_recommended: {
    label: 'Refresh recommended',
    background: 'var(--bgColor-accent-muted, #ddf4ff)',
    color: 'var(--fgColor-accent, #0969da)',
  },
  review_failed: {
    label: 'Review failed',
    background: 'var(--bgColor-danger-emphasis, #cf222e)',
    color: 'var(--fgColor-onEmphasis, #ffffff)',
  },
};

function parsePullRequestContext() {
  if (typeof window === 'undefined') {
    return null;
  }

  const match = window.location.pathname.match(/^\/([^/]+)\/([^/]+)\/pull\/(\d+)/);
  if (!match) {
    return null;
  }

  return {
    owner: decodeURIComponent(match[1]),
    repo: decodeURIComponent(match[2]),
    pr_number: Number(match[3]),
  };
}

function setStatus(message, isError = false) {
  if (typeof document === 'undefined') {
    return;
  }

  const statusNode = document.getElementById(STATUS_ID);
  if (!statusNode) {
    return;
  }
  statusNode.textContent = message;
  statusNode.classList.remove('roger-panel-status--ok', 'roger-panel-status--error');
  statusNode.classList.add(isError ? 'roger-panel-status--error' : 'roger-panel-status--ok');
}

function clearAttentionBadge() {
  if (typeof document === 'undefined') {
    return;
  }

  const badge = document.getElementById(BADGE_ID);
  if (!badge) {
    return;
  }

  badge.textContent = '';
  badge.style.display = 'none';
}

function setAttentionBadge(attentionState, freshnessLabel) {
  if (typeof document === 'undefined') {
    return;
  }

  const badge = document.getElementById(BADGE_ID);
  if (!badge) {
    return;
  }

  const style = ATTENTION_STYLES[attentionState];
  if (!style) {
    clearAttentionBadge();
    return;
  }

  badge.textContent = freshnessLabel
    ? `${style.label} (${freshnessLabel})`
    : style.label;
  badge.style.display = 'inline-block';
  badge.style.background = style.background;
  badge.style.color = style.color;
}

function requestStatusMirror(context) {
  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    clearAttentionBadge();
    setStatus('Launch-only mode. Open Roger locally for authoritative status.');
    return;
  }

  chrome.runtime.sendMessage(
    {
      type: 'roger_bridge_status',
      intent: {
        owner: context.owner,
        repo: context.repo,
        pr_number: context.pr_number,
      },
    },
    (response) => {
      if (chrome.runtime.lastError) {
        clearAttentionBadge();
        setStatus('Launch-only mode. Open Roger locally for authoritative status.');
        return;
      }

      if (!response) {
        clearAttentionBadge();
        setStatus('No status response. Open Roger locally for authoritative status.');
        return;
      }

      if (!response.ok) {
        clearAttentionBadge();
        const guidance = response.guidance ? ` ${response.guidance}` : '';
        setStatus(`${response.message}.${guidance}`.trim(), true);
        return;
      }

      if (response.mode !== 'bounded_status' || !response.attention_state) {
        clearAttentionBadge();
        setStatus(response.message || 'Launch-only mode. Open Roger locally for authoritative status.');
        return;
      }

      setAttentionBadge(response.attention_state, response.freshness_label || null);
      setStatus(response.message || 'Mirroring bounded Roger status.');
    }
  );
}

function pickInlineAnchorSelector(querySelectorFn) {
  for (const selector of INLINE_ANCHOR_SELECTORS) {
    if (querySelectorFn(selector)) {
      return selector;
    }
  }

  return null;
}

function findInlineAnchor(rootDocument) {
  if (!rootDocument?.querySelector) {
    return null;
  }

  const selector = pickInlineAnchorSelector((candidate) => rootDocument.querySelector(candidate));
  if (!selector) {
    return null;
  }

  return rootDocument.querySelector(selector);
}

function resolvePanelPlacement(rootDocument) {
  const anchor = findInlineAnchor(rootDocument);
  if (anchor) {
    return {
      mode: 'inline',
      mountNode: anchor,
    };
  }

  return {
    mode: 'floating',
    mountNode: rootDocument.body,
  };
}

function mountInto(parent, node, options = {}) {
  if (!parent || !node) {
    return false;
  }

  if (node.parentElement === parent) {
    return false;
  }

  if (options.prepend && typeof parent.prepend === 'function') {
    parent.prepend(node);
    return true;
  }

  parent.appendChild(node);
  return true;
}

function ensurePanelStyles(rootDocument) {
  if (rootDocument.getElementById(STYLE_ID)) {
    return;
  }

  const styleNode = rootDocument.createElement('style');
  styleNode.id = STYLE_ID;
  styleNode.textContent = `
#${PANEL_ID} {
  font-family: var(--fontStack-sansSerif, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif);
  background: var(--bgColor-muted, #f6f8fa);
  color: var(--fgColor-default, #1f2328);
  border: 1px solid var(--borderColor-default, #d0d7de);
  padding: 10px;
}

#${PANEL_ID}.roger-panel--floating {
  position: fixed;
  top: 88px;
  right: 24px;
  z-index: 9999;
  width: 260px;
  border-radius: 10px;
  box-shadow: 0 10px 24px rgba(27, 31, 35, 0.12);
}

#${PANEL_ID}.roger-panel--inline {
  position: static;
  width: 320px;
  max-width: 100%;
  margin-left: 8px;
  border-radius: 6px;
  box-shadow: none;
}

#${INLINE_SLOT_ID} {
  display: inline-flex;
  align-items: stretch;
  flex-shrink: 0;
  max-width: 100%;
}

#${PANEL_ID} .roger-panel-heading {
  margin: 0 0 10px 0;
  font-size: 13px;
  line-height: 1.3;
  font-weight: 600;
  color: var(--fgColor-default, #1f2328);
}

#${PANEL_ID} .roger-panel-badge {
  margin: 0 0 10px 0;
  font-size: 11px;
  font-weight: 600;
  border-radius: 999px;
  padding: 4px 8px;
  display: none;
}

#${PANEL_ID} .roger-panel-button-row {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

#${PANEL_ID} .roger-panel-button {
  border: 1px solid var(--button-default-borderColor-rest, var(--borderColor-default, #d0d7de));
  background: var(--button-default-bgColor-rest, var(--bgColor-default, #ffffff));
  color: var(--button-default-fgColor-rest, var(--fgColor-default, #1f2328));
  border-radius: 6px;
  padding: 6px 8px;
  font-size: 12px;
  line-height: 1.25;
  cursor: pointer;
}

#${PANEL_ID} .roger-panel-button:hover:not(:disabled) {
  background: var(--button-default-bgColor-hover, var(--bgColor-emphasis, #e9ecef));
  border-color: var(--button-default-borderColor-hover, var(--borderColor-emphasis, #8c959f));
}

#${PANEL_ID} .roger-panel-button:disabled {
  background: var(--button-default-bgColor-disabled, var(--bgColor-muted, #f6f8fa));
  border-color: var(--borderColor-muted, #d8dee4);
  color: var(--fgColor-muted, #656d76);
  cursor: not-allowed;
  opacity: 0.85;
}

#${PANEL_ID} .roger-panel-status {
  margin: 10px 0 0 0;
  font-size: 12px;
  line-height: 1.35;
}

#${PANEL_ID} .roger-panel-status--ok {
  color: var(--fgColor-success, #1a7f37);
}

#${PANEL_ID} .roger-panel-status--error {
  color: var(--fgColor-danger, #d1242f);
}
  `.trim();

  const styleHost = rootDocument.head || rootDocument.documentElement || rootDocument.body;
  if (styleHost) {
    styleHost.appendChild(styleNode);
  }
}

function applyPanelModeStyles(panel, mode) {
  panel.classList.toggle('roger-panel--inline', mode === 'inline');
  panel.classList.toggle('roger-panel--floating', mode !== 'inline');
}

function ensureInlineSlot(rootDocument, mountNode) {
  let inlineSlot = rootDocument.getElementById(INLINE_SLOT_ID);
  if (!inlineSlot) {
    inlineSlot = rootDocument.createElement('div');
    inlineSlot.id = INLINE_SLOT_ID;
  }

  mountInto(mountNode, inlineSlot, { prepend: true });
  return inlineSlot;
}

function createPanel(context, rootDocument) {
  ensurePanelStyles(rootDocument);

  const panel = rootDocument.createElement('section');
  panel.id = PANEL_ID;
  panel.className = 'roger-panel roger-panel--floating';

  const heading = rootDocument.createElement('h3');
  heading.id = HEADING_ID;
  heading.className = 'roger-panel-heading';
  heading.textContent = `Roger: ${context.owner}/${context.repo}#${context.pr_number}`;
  panel.appendChild(heading);

  const badge = rootDocument.createElement('p');
  badge.id = BADGE_ID;
  badge.className = 'roger-panel-badge';
  panel.appendChild(badge);

  const buttonRow = rootDocument.createElement('div');
  buttonRow.className = 'roger-panel-button-row';

  for (const action of ACTIONS) {
    const button = rootDocument.createElement('button');
    button.className = 'roger-panel-button';
    button.type = 'button';
    button.textContent = action.label;
    button.dataset.action = action.id;
    button.addEventListener('click', () => triggerLaunch(action.id, context, button));
    buttonRow.appendChild(button);
  }

  panel.appendChild(buttonRow);

  const status = rootDocument.createElement('p');
  status.id = STATUS_ID;
  status.className = 'roger-panel-status roger-panel-status--ok';
  status.textContent = 'Launch-only mode. Live status is shown in Roger locally.';
  panel.appendChild(status);

  return panel;
}

function updatePanelHeading(panel, context) {
  const heading = panel.querySelector(`#${HEADING_ID}`);
  if (!heading) {
    return;
  }

  heading.textContent = `Roger: ${context.owner}/${context.repo}#${context.pr_number}`;
}

function removePanel(rootDocument) {
  const panel = rootDocument.getElementById(PANEL_ID);
  if (panel) {
    panel.remove();
  }

  const inlineSlot = rootDocument.getElementById(INLINE_SLOT_ID);
  if (inlineSlot) {
    inlineSlot.remove();
  }
}

function ensurePanel(context, rootDocument) {
  let panel = rootDocument.getElementById(PANEL_ID);
  if (!panel) {
    panel = createPanel(context, rootDocument);
  }
  updatePanelHeading(panel, context);

  const placement = resolvePanelPlacement(rootDocument);
  if (placement.mode === 'inline') {
    const inlineSlot = ensureInlineSlot(rootDocument, placement.mountNode);
    mountInto(inlineSlot, panel);
  } else {
    const inlineSlot = rootDocument.getElementById(INLINE_SLOT_ID);
    if (inlineSlot) {
      inlineSlot.remove();
    }
    mountInto(rootDocument.body, panel);
  }

  applyPanelModeStyles(panel, placement.mode);
  return placement.mode;
}

let lastContextKey = null;
let refreshScheduled = false;

function contextKey(context) {
  if (!context) {
    return null;
  }

  return `${context.owner}/${context.repo}#${context.pr_number}`;
}

function refreshPanelForCurrentPage(rootDocument) {
  const context = parsePullRequestContext();
  if (!context) {
    removePanel(rootDocument);
    lastContextKey = null;
    return;
  }

  ensurePanel(context, rootDocument);

  const nextKey = contextKey(context);
  if (lastContextKey !== nextKey) {
    lastContextKey = nextKey;
    requestStatusMirror(context);
  }
}

function scheduleRefresh(rootDocument) {
  if (refreshScheduled) {
    return;
  }

  refreshScheduled = true;
  const run = () => {
    refreshScheduled = false;
    refreshPanelForCurrentPage(rootDocument);
  };

  if (typeof requestAnimationFrame === 'function') {
    requestAnimationFrame(run);
    return;
  }

  setTimeout(run, 0);
}

function registerNavigationHooks(rootDocument) {
  if (typeof window === 'undefined') {
    return;
  }

  const onPotentialNavigation = () => scheduleRefresh(rootDocument);
  window.addEventListener('turbo:load', onPotentialNavigation);
  window.addEventListener('pjax:end', onPotentialNavigation);
  window.addEventListener('popstate', onPotentialNavigation);

  if (typeof MutationObserver !== 'undefined' && rootDocument.body) {
    const observer = new MutationObserver(() => onPotentialNavigation());
    observer.observe(rootDocument.body, {
      childList: true,
      subtree: true,
    });
  }
}

function bootstrapRogerPanel() {
  if (typeof document === 'undefined') {
    return;
  }

  refreshPanelForCurrentPage(document);
  registerNavigationHooks(document);
}

function triggerLaunch(action, context, button) {
  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    clearAttentionBadge();
    setStatus('Bridge unavailable in browser context. Open Roger locally and run rr manually.', true);
    return;
  }

  const previousText = button.textContent;
  button.disabled = true;
  button.textContent = '…';
  setStatus('Dispatching launch intent...');

  chrome.runtime.sendMessage(
    {
      type: 'roger_bridge_launch',
      intent: {
        action,
        owner: context.owner,
        repo: context.repo,
        pr_number: context.pr_number,
      },
    },
    (response) => {
      button.disabled = false;
      button.textContent = previousText;

      if (chrome.runtime.lastError) {
        clearAttentionBadge();
        setStatus(`Bridge error: ${chrome.runtime.lastError.message}`, true);
        return;
      }

      if (!response) {
        clearAttentionBadge();
        setStatus('No bridge response. Open Roger locally and run rr manually.', true);
        return;
      }

      if (!response.ok) {
        clearAttentionBadge();
        const guidance = response.guidance ? ` ${response.guidance}` : '';
        setStatus(`${response.message}.${guidance}`.trim(), true);
        return;
      }

      if (response.mode === 'custom_url_fallback') {
        clearAttentionBadge();
        setStatus('Launched via URL fallback. Open Roger locally for authoritative status.');
        return;
      }

      if (response.mode === 'native_messaging' && response.attention_state) {
        setAttentionBadge(response.attention_state, response.freshness_label || null);
        setStatus(response.message || 'Launch intent dispatched.');
        return;
      }

      setStatus(response.message || 'Launch intent dispatched.');
      requestStatusMirror(context);
    }
  );
}

if (typeof window !== 'undefined' && typeof document !== 'undefined') {
  bootstrapRogerPanel();
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    INLINE_ANCHOR_SELECTORS,
    applyPanelModeStyles,
    ensurePanel,
    findInlineAnchor,
    mountInto,
    parsePullRequestContext,
    pickInlineAnchorSelector,
    refreshPanelForCurrentPage,
    resolvePanelPlacement,
    setStatus,
  };
}
