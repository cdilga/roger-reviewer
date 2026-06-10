const PANEL_ID = 'roger-reviewer-panel';
const STATUS_ID = 'roger-reviewer-status';
const BADGE_ID = 'roger-reviewer-attention-badge';
const HEADING_ID = 'roger-reviewer-heading';
const SUBHEADING_ID = 'roger-reviewer-subheading';
const BRAND_CHIP_ID = 'roger-reviewer-brand-chip';
const INFO_TEXT_ID = 'roger-reviewer-info-text';
const INLINE_SLOT_ID = 'roger-reviewer-inline-slot';
const RAIL_SLOT_ID = 'roger-reviewer-rail-slot';
const MODAL_SLOT_ID = 'roger-reviewer-modal-slot';
const MODAL_DIALOG_ID = 'roger-reviewer-modal-dialog';
const MODAL_CONTENT_ID = 'roger-reviewer-modal-content';
const MODAL_OPEN_BUTTON_ID = 'roger-reviewer-modal-open';
const MODAL_CLOSE_BUTTON_ID = 'roger-reviewer-modal-close';
const STYLE_ID = 'roger-reviewer-panel-style';
const GITHUB_ACTION_BUTTON_CLASS = 'roger-panel-button btn btn-sm Button Button--small';
const BRAND_CHIP_CLASS = 'rr-brand-chip roger-panel-brand-chip';
const MODAL_OPEN_BUTTON_LABEL = 'Open Roger actions (fallback)';
const MODAL_FALLBACK_STATUS =
  'Page seams unavailable. Using in-page modal fallback. Toolbar popup remains manual backup.';
const INLINE_ANCHOR_SELECTORS = [
  '[class*="prc-PageHeader-Actions-"]',
  '[class*="PullRequestHeader-module__actionsAboveTitleOnNarrow__"]',
  '#partial-discussion-header .gh-header-actions',
  '.gh-header-actions',
];
const RAIL_ANCHOR_SELECTORS = [
  '[class*="Layout-sidebar"]',
  '#partial-discussion-sidebar',
  '.discussion-sidebar',
];
const RAIL_REVIEWERS_SELECTORS = [
  '[aria-label="Reviewers"]',
  '[data-testid="reviewers"]',
  '#reviewers',
  '.discussion-sidebar-item.sidebar-assignee',
];

const ACTIONS = [
  { id: 'start_review', label: 'Start Review in Roger' },
  { id: 'resume_review', label: 'Resume Existing Review' },
  { id: 'show_findings', label: 'View Findings' },
];
const FINDINGS_VISIBLE_ATTENTION_STATES = new Set(['findings_ready']);
const RESUME_PRIMARY_ATTENTION_STATES = new Set([
  'awaiting_user_input',
  'awaiting_outbound_approval',
  'refresh_recommended',
  'review_failed',
]);
const DEFAULT_INFO_MESSAGE =
  'Launch Roger locally from this pull request. For authoritative connection and review state, use Roger itself rather than this GitHub panel.';

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
    label: 'Resume review recommended',
    background: 'var(--bgColor-accent-muted, #ddf4ff)',
    color: 'var(--fgColor-accent, #0969da)',
  },
  review_failed: {
    label: 'Review failed',
    background: 'var(--bgColor-danger-emphasis, #cf222e)',
    color: 'var(--fgColor-onEmphasis, #ffffff)',
  },
};

function deriveActionModel(attentionState) {
  const visibleActions = new Set(['start_review', 'resume_review']);
  let primaryActionId = 'start_review';

  if (FINDINGS_VISIBLE_ATTENTION_STATES.has(attentionState)) {
    visibleActions.add('show_findings');
    primaryActionId = 'show_findings';
  } else if (RESUME_PRIMARY_ATTENTION_STATES.has(attentionState)) {
    primaryActionId = 'resume_review';
  }

  return {
    visibleActions,
    primaryActionId,
  };
}

function applyActionModel(panel, attentionState) {
  if (!panel || typeof panel.querySelectorAll !== 'function') {
    return deriveActionModel(attentionState);
  }

  const model = deriveActionModel(attentionState);
  for (const button of panel.querySelectorAll('button[data-action]')) {
    const actionId = button.dataset?.action;
    const isVisible = actionId ? model.visibleActions.has(actionId) : true;
    const isPrimary = actionId === model.primaryActionId;
    const isTertiary = actionId === 'show_findings' && isVisible && !isPrimary;
    button.hidden = !isVisible;
    button.classList?.toggle('roger-panel-button--primary', isPrimary && isVisible);
    button.classList?.toggle(
      'roger-panel-button--secondary',
      !isPrimary && !isTertiary && isVisible
    );
    button.classList?.toggle('roger-panel-button--tertiary', isTertiary);
    button.setAttribute?.('aria-hidden', isVisible ? 'false' : 'true');
  }

  return model;
}

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

function readExtensionBuildLabel() {
  if (typeof chrome === 'undefined' || !chrome.runtime?.getManifest) {
    return '';
  }

  try {
    const manifest = chrome.runtime.getManifest();
    if (!manifest || typeof manifest !== 'object') {
      return '';
    }
    return manifest.version_name || manifest.version || '';
  } catch {
    return '';
  }
}

function readRuntimeAssetUrl(relativePath) {
  if (typeof chrome === 'undefined' || !chrome.runtime?.getURL) {
    return '';
  }

  try {
    return chrome.runtime.getURL(relativePath);
  } catch {
    return '';
  }
}

let inlineStatusResetTimer = null;

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

function setInfoMessage(message) {
  if (typeof document === 'undefined') {
    return;
  }

  const infoNode = document.getElementById(INFO_TEXT_ID);
  if (!infoNode) {
    return;
  }

  infoNode.textContent = message || DEFAULT_INFO_MESSAGE;
}

function clearStatus() {
  if (typeof document === 'undefined') {
    return;
  }

  const statusNode = document.getElementById(STATUS_ID);
  if (!statusNode) {
    return;
  }

  statusNode.hidden = true;
  statusNode.textContent = '';
  statusNode.classList.remove(
    'roger-panel-status--ok',
    'roger-panel-status--error',
    'roger-panel-status--inline-visible'
  );
}

function setStatus(message, isError = false, options = {}) {
  if (typeof document === 'undefined') {
    return;
  }

  const statusNode = document.getElementById(STATUS_ID);
  if (!statusNode) {
    return;
  }
  if (!message) {
    clearStatus();
    return;
  }

  statusNode.textContent = message;
  statusNode.hidden = false;
  statusNode.classList.remove(
    'roger-panel-status--ok',
    'roger-panel-status--error',
    'roger-panel-status--inline-visible'
  );
  statusNode.classList.add(isError ? 'roger-panel-status--error' : 'roger-panel-status--ok');

  const panel = statusNode.parentElement;
  if (panel?.classList?.contains('roger-panel--inline') && options.revealInline) {
    statusNode.classList.add('roger-panel-status--inline-visible');
    if (inlineStatusResetTimer !== null && typeof clearTimeout === 'function') {
      clearTimeout(inlineStatusResetTimer);
    }
    if (typeof setTimeout === 'function') {
      inlineStatusResetTimer = setTimeout(() => {
        statusNode.classList.remove('roger-panel-status--inline-visible');
        statusNode.hidden = true;
        inlineStatusResetTimer = null;
      }, 4500);
    }
  }
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
  const panel = typeof document !== 'undefined' ? document.getElementById(PANEL_ID) : null;

  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    lastAttentionState = null;
    if (panel) {
      applyActionModel(panel, lastAttentionState);
    }
    clearAttentionBadge();
    clearStatus();
    setInfoMessage('Launch-only mode. Open Roger locally (`rr status`) for authoritative detail.');
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
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        clearStatus();
        setInfoMessage('Launch-only mode. Open Roger locally (`rr status`) for authoritative detail.');
        return;
      }

      if (!response) {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        clearStatus();
        setInfoMessage('No bounded status response. Open Roger locally (`rr status`) for authoritative detail.');
        return;
      }

      if (!response.ok) {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        clearStatus();
        setInfoMessage(appendGuidance(response.message, response.guidance));
        return;
      }

      if (response.mode !== 'bounded_status' || !response.attention_state) {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        clearStatus();
        setInfoMessage(
          appendGuidance(
            response.message || 'Launch-only mode. Open Roger locally for authoritative detail.',
            response.guidance
          )
        );
        return;
      }

      lastAttentionState = response.attention_state;
      if (panel) {
        applyActionModel(panel, lastAttentionState);
      }
      setAttentionBadge(response.attention_state, response.freshness_label || null);
      clearStatus();
      setInfoMessage(
        appendGuidance(response.message || 'Mirroring bounded Roger status.', response.guidance)
      );
    }
  );
}

function pickSelector(selectors, querySelectorFn) {
  for (const selector of selectors) {
    if (querySelectorFn(selector)) {
      return selector;
    }
  }

  return null;
}

function pickInlineAnchorSelector(querySelectorFn) {
  return pickSelector(INLINE_ANCHOR_SELECTORS, querySelectorFn);
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

function findRightRailPlacement(rootDocument) {
  if (!rootDocument?.querySelector) {
    return null;
  }

  const railSelector = pickSelector(RAIL_ANCHOR_SELECTORS, (candidate) =>
    rootDocument.querySelector(candidate)
  );
  const railNode = railSelector ? rootDocument.querySelector(railSelector) : null;
  if (!railNode) {
    return null;
  }

  let beforeNode = null;
  if (typeof railNode.querySelector === 'function') {
    const reviewersSelector = pickSelector(RAIL_REVIEWERS_SELECTORS, (candidate) =>
      railNode.querySelector(candidate)
    );
    beforeNode = reviewersSelector ? railNode.querySelector(reviewersSelector) : null;
  }

  return {
    mountNode: railNode,
    beforeNode,
  };
}

function resolvePanelPlacement(rootDocument) {
  const railPlacement = findRightRailPlacement(rootDocument);
  if (railPlacement?.mountNode) {
    return {
      mode: 'rail',
      mountNode: railPlacement.mountNode,
      beforeNode: railPlacement.beforeNode || null,
    };
  }

  const anchor = findInlineAnchor(rootDocument);
  if (anchor) {
    return {
      mode: 'inline',
      mountNode: anchor,
    };
  }

  return {
    mode: 'modal',
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
  --rr-brand-ink-900: #0d1117;
  --rr-brand-ink-700: #30363d;
  --rr-brand-ink-500: #57606a;
  --rr-brand-accent-700: #1f6feb;
  --rr-brand-accent-500: #2f81f7;
  --rr-brand-accent-300: #79c0ff;
  --rr-brand-glow-200: #d0d7de;
  --rr-brand-canvas-100: #f6f8fa;
  --rr-panel-surface: var(--overlay-bgColor, var(--bgColor-default, #ffffff));
  --rr-panel-surface-muted: var(--bgColor-muted, #f6f8fa);
  --rr-panel-surface-raised: color-mix(
    in srgb,
    var(--rr-panel-surface) 82%,
    var(--rr-panel-surface-muted) 18%
  );
  --rr-panel-border: color-mix(in srgb, var(--borderColor-default, #d0d7de) 86%, transparent);
  --rr-panel-border-strong: color-mix(
    in srgb,
    var(--borderColor-emphasis, #8c959f) 84%,
    transparent
  );
  --rr-panel-text: var(--fgColor-default, #1f2328);
  --rr-panel-muted: var(--fgColor-muted, #656d76);
  --rr-panel-highlight: rgba(255, 255, 255, 0.72);
  --rr-panel-shadow: rgba(15, 23, 42, 0.12);
  font-family: var(--fontStack-sansSerif, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif);
  background: var(--rr-panel-surface-muted);
  color: var(--rr-panel-text);
  border: 1px solid var(--rr-panel-border);
  padding: 12px;
}

:root[data-color-mode="dark"] #${PANEL_ID} {
  --rr-panel-border: rgba(240, 246, 252, 0.14);
  --rr-panel-border-strong: rgba(240, 246, 252, 0.22);
  --rr-panel-highlight: rgba(240, 246, 252, 0.08);
  --rr-panel-shadow: rgba(1, 4, 9, 0.42);
}

#${PANEL_ID}.roger-panel--inline {
  position: relative;
  width: auto;
  max-width: none;
  margin-left: 8px;
  border: 0;
  background: transparent;
  padding: 0;
  border-radius: 0;
  box-shadow: none;
}

#${PANEL_ID}.roger-panel--inline .roger-panel-heading,
#${PANEL_ID}.roger-panel--inline .roger-panel-subheading,
#${PANEL_ID}.roger-panel--inline .roger-panel-badge {
  display: none;
}

#${PANEL_ID}.roger-panel--inline .roger-panel-brandbar {
  margin: 0 6px 0 0;
  align-items: center;
}

#${INLINE_SLOT_ID} {
  display: inline-flex;
  align-items: stretch;
  flex-shrink: 0;
  max-width: 100%;
}

#${PANEL_ID}.roger-panel--rail,
#${PANEL_ID}.roger-panel--modal {
  position: static;
  width: 100%;
  max-width: 100%;
  margin: 0;
  border-radius: 14px;
  border-color: var(--rr-panel-border-strong);
  background:
    linear-gradient(
      180deg,
      color-mix(in srgb, var(--rr-panel-surface) 92%, white 8%) 0%,
      var(--rr-panel-surface-raised) 52%,
      color-mix(in srgb, var(--rr-panel-surface) 74%, var(--rr-panel-surface-muted) 26%) 100%
    ),
    linear-gradient(
      135deg,
      rgba(255, 255, 255, 0.14),
      rgba(255, 255, 255, 0) 38%,
      rgba(148, 163, 184, 0.18) 100%
    );
  box-shadow:
    inset 0 1px 0 var(--rr-panel-highlight),
    0 18px 38px var(--rr-panel-shadow);
}

#${RAIL_SLOT_ID} {
  display: block;
  width: 100%;
  margin: 0 0 16px 0;
}

#${MODAL_SLOT_ID} {
  margin: 8px 0 0 0;
}

#${MODAL_OPEN_BUTTON_ID} {
  border: 1px solid var(--button-default-borderColor-rest, var(--borderColor-default, #d0d7de));
  background: var(--button-default-bgColor-rest, var(--bgColor-default, #ffffff));
  color: var(--button-default-fgColor-rest, var(--fgColor-default, #1f2328));
  border-radius: 6px;
  padding: 6px 12px;
  font-size: 12px;
  line-height: 1.25;
  font-weight: 600;
  cursor: pointer;
}

#${MODAL_OPEN_BUTTON_ID}:hover {
  background: var(--button-default-bgColor-hover, var(--bgColor-emphasis, #e9ecef));
  border-color: var(--button-default-borderColor-hover, var(--borderColor-emphasis, #8c959f));
}

#${MODAL_DIALOG_ID} {
  border: 1px solid var(--borderColor-default, #d0d7de);
  border-radius: 10px;
  background: var(--bgColor-default, #ffffff);
  width: min(560px, calc(100vw - 40px));
  padding: 0;
}

#${MODAL_DIALOG_ID}::backdrop {
  background: rgba(27, 31, 35, 0.5);
}

#${MODAL_DIALOG_ID} .roger-panel-modal-frame {
  padding: 12px;
}

#${MODAL_DIALOG_ID} .roger-panel-modal-header {
  display: flex;
  justify-content: flex-end;
  margin: 0 0 8px 0;
}

#${MODAL_CLOSE_BUTTON_ID} {
  border: 1px solid var(--button-default-borderColor-rest, var(--borderColor-default, #d0d7de));
  background: var(--button-default-bgColor-rest, var(--bgColor-default, #ffffff));
  color: var(--button-default-fgColor-rest, var(--fgColor-default, #1f2328));
  border-radius: 6px;
  padding: 4px 10px;
  font-size: 12px;
  line-height: 1.25;
  cursor: pointer;
}

#${PANEL_ID} .roger-panel-heading {
  margin: 0;
  font-size: 23px;
  line-height: 1.15;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--rr-panel-text);
}

#${PANEL_ID} .roger-panel-subheading {
  margin: 4px 0 0 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--rr-panel-muted);
  font-family: var(
    --fontStack-monospace,
    ui-monospace,
    SFMono-Regular,
    SF Mono,
    Menlo,
    Consolas,
    monospace
  );
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

#${PANEL_ID} .roger-panel-brandbar {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 14px;
  margin: 0 0 14px 0;
}

#${PANEL_ID} .roger-panel-brandmark {
  display: inline-flex;
  align-items: flex-start;
  gap: 12px;
  flex: 1 1 auto;
  min-width: 0;
}

#${PANEL_ID} .roger-panel-heading-group {
  display: grid;
  gap: 2px;
  min-width: 0;
}

#${PANEL_ID} .rr-brand-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border-radius: 999px;
  border: 1px solid var(--rr-panel-border-strong);
  background: linear-gradient(
    120deg,
    color-mix(in srgb, var(--rr-panel-surface) 86%, white 14%) 0%,
    color-mix(in srgb, var(--rr-panel-surface-raised) 92%, white 8%) 100%
  );
  color: var(--rr-brand-ink-700);
  font-size: 11px;
  line-height: 1;
  font-weight: 600;
  letter-spacing: 0.01em;
  padding: 4px 9px;
  margin: 0;
  box-shadow: inset 0 1px 0 var(--rr-panel-highlight);
}

#${PANEL_ID} .roger-panel-brandicon {
  width: 16px;
  height: 16px;
  flex: 0 0 auto;
}

#${PANEL_ID}.roger-panel--inline .rr-brand-chip {
  padding: 3px 8px;
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
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: stretch;
}

#${PANEL_ID}.roger-panel--inline .roger-panel-button-row {
  display: inline-flex;
  flex-wrap: nowrap;
  gap: 6px;
}

#${PANEL_ID} .roger-panel-button {
  flex: 1 1 calc(50% - 4px);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-height: 36px;
  border: 1px solid var(--rr-panel-border-strong);
  background: linear-gradient(
    180deg,
    color-mix(
      in srgb,
      var(--button-default-bgColor-rest, var(--rr-panel-surface)) 84%,
      white 16%
    ),
    color-mix(
      in srgb,
      var(--button-default-bgColor-rest, var(--rr-panel-surface-raised)) 92%,
      var(--rr-panel-surface-muted) 8%
    )
  );
  color: var(--button-default-fgColor-rest, var(--rr-panel-text));
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 12px;
  line-height: 1.25;
  font-weight: 600;
  text-align: center;
  white-space: normal;
  cursor: pointer;
  box-shadow:
    inset 0 1px 0 var(--rr-panel-highlight),
    0 1px 2px rgba(15, 23, 42, 0.08);
  transition:
    border-color 120ms ease,
    background 120ms ease,
    box-shadow 120ms ease,
    transform 120ms ease;
}

#${PANEL_ID} .roger-panel-button:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--rr-panel-border-strong) 78%, var(--rr-brand-accent-300) 22%);
  background: linear-gradient(
    180deg,
    color-mix(
      in srgb,
      var(--button-default-bgColor-hover, var(--rr-panel-surface-raised)) 84%,
      white 16%
    ),
    color-mix(
      in srgb,
      var(--button-default-bgColor-hover, var(--rr-panel-surface-raised)) 92%,
      var(--rr-panel-surface-muted) 8%
    )
  );
  box-shadow:
    inset 0 1px 0 var(--rr-panel-highlight),
    0 6px 18px rgba(15, 23, 42, 0.12);
  transform: translateY(-1px);
}

#${PANEL_ID} .roger-panel-button:disabled {
  background: var(--button-default-bgColor-disabled, var(--bgColor-muted, #f6f8fa));
  border-color: var(--borderColor-muted, #d8dee4);
  color: var(--fgColor-muted, #656d76);
  cursor: not-allowed;
  opacity: 0.85;
}

#${PANEL_ID} .roger-panel-button[hidden] {
  display: none !important;
}

#${PANEL_ID} .roger-panel-button.roger-panel-button--primary {
  flex-basis: 100%;
  order: -1;
  border-color: color-mix(in srgb, var(--rr-brand-accent-700) 88%, black 12%);
  background:
    linear-gradient(
      180deg,
      color-mix(in srgb, var(--rr-brand-accent-500) 88%, white 12%),
      var(--rr-brand-accent-700)
    );
  color: #ffffff;
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.22),
    0 8px 22px rgba(31, 111, 235, 0.24);
}

#${PANEL_ID} .roger-panel-button.roger-panel-button--secondary {
  color: var(--rr-panel-text);
}

#${PANEL_ID} .roger-panel-button.roger-panel-button--tertiary {
  color: var(--rr-panel-muted);
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--rr-panel-surface-raised) 86%, white 14%),
    color-mix(in srgb, var(--rr-panel-surface-raised) 94%, var(--rr-panel-surface-muted) 6%)
  );
}

#${PANEL_ID} .roger-panel-button.roger-panel-button--primary:hover:not(:disabled) {
  border-color: var(--rr-brand-accent-700);
  background:
    linear-gradient(
      180deg,
      color-mix(in srgb, var(--rr-brand-accent-500) 78%, black 22%),
      color-mix(in srgb, var(--rr-brand-accent-700) 76%, black 24%)
    );
}

#${PANEL_ID}.roger-panel--inline .roger-panel-button {
  flex: 0 1 auto;
  padding: 0 12px;
  min-height: 28px;
  white-space: nowrap;
}

#${PANEL_ID} .roger-panel-status {
  display: block;
  margin: 10px 0 0 0;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid var(--rr-panel-border);
  background: color-mix(in srgb, var(--rr-panel-surface) 92%, var(--rr-panel-surface-muted) 8%);
  font-size: 12px;
  line-height: 1.35;
  color: var(--rr-panel-muted);
}

#${PANEL_ID}.roger-panel--inline .roger-panel-status {
  display: none;
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 20;
  min-width: 240px;
  max-width: min(360px, calc(100vw - 32px));
  margin: 0;
  padding: 8px 10px;
  border: 1px solid var(--borderColor-default, #d0d7de);
  border-radius: 10px;
  background: var(--rr-panel-surface, var(--overlay-bgColor, #ffffff));
  box-shadow: var(--shadow-small, 0 3px 12px rgba(31, 35, 40, 0.12));
}

#${PANEL_ID}.roger-panel--inline .roger-panel-status.roger-panel-status--inline-visible {
  display: block;
}

#${PANEL_ID} .roger-panel-info {
  position: relative;
  margin: 0;
  flex: 0 0 auto;
}

#${PANEL_ID} .roger-panel-info summary {
  list-style: none;
}

#${PANEL_ID} .roger-panel-info summary::-webkit-details-marker {
  display: none;
}

#${PANEL_ID} .roger-panel-info-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: 1px solid var(--rr-panel-border-strong);
  border-radius: 999px;
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--rr-panel-surface-raised) 88%, white 12%),
    color-mix(in srgb, var(--rr-panel-surface) 92%, var(--rr-panel-surface-muted) 8%)
  );
  color: var(--rr-panel-text);
  font-size: 13px;
  line-height: 1;
  font-weight: 700;
  cursor: pointer;
  user-select: none;
  box-shadow:
    inset 0 1px 0 var(--rr-panel-highlight),
    0 1px 2px rgba(15, 23, 42, 0.08);
}

#${PANEL_ID} .roger-panel-info[open] .roger-panel-info-toggle {
  border-color: color-mix(in srgb, var(--rr-panel-border-strong) 72%, var(--rr-brand-accent-300) 28%);
  box-shadow:
    inset 0 1px 0 var(--rr-panel-highlight),
    0 0 0 3px color-mix(in srgb, var(--rr-brand-accent-300) 18%, transparent);
}

#${PANEL_ID} .roger-panel-info-panel {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  z-index: 30;
  display: none;
  width: min(320px, calc(100vw - 48px));
  margin: 0;
  padding: 11px 12px;
  border: 1px solid var(--rr-panel-border-strong);
  border-radius: 12px;
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--rr-panel-surface) 92%, white 8%),
    color-mix(in srgb, var(--rr-panel-surface-raised) 94%, var(--rr-panel-surface-muted) 6%)
  );
  box-shadow: 0 18px 36px var(--rr-panel-shadow);
}

#${PANEL_ID} .roger-panel-info[open] .roger-panel-info-panel {
  display: block;
}

#${PANEL_ID} .roger-panel-info-panel p {
  margin: 0;
  font-size: 11px;
  line-height: 1.5;
  color: var(--rr-panel-muted);
}

#${PANEL_ID}.roger-panel--rail .roger-panel-button-row,
#${PANEL_ID}.roger-panel--modal .roger-panel-button-row {
  margin-top: 4px;
}

#${PANEL_ID}.roger-panel--rail .roger-panel-button,
#${PANEL_ID}.roger-panel--modal .roger-panel-button {
  width: 100%;
}

#${PANEL_ID} .roger-panel-status--ok {
  color: var(--rr-panel-muted);
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
  panel.classList.toggle('roger-panel--rail', mode === 'rail');
  panel.classList.toggle('roger-panel--modal', mode === 'modal');
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

function ensureRailSlot(rootDocument, mountNode, beforeNode) {
  let railSlot = rootDocument.getElementById(RAIL_SLOT_ID);
  if (!railSlot) {
    railSlot = rootDocument.createElement('div');
    railSlot.id = RAIL_SLOT_ID;
  }

  if (
    beforeNode &&
    beforeNode.parentElement === mountNode &&
    typeof mountNode.insertBefore === 'function'
  ) {
    mountNode.insertBefore(railSlot, beforeNode);
    return railSlot;
  }

  mountInto(mountNode, railSlot, { prepend: true });
  return railSlot;
}

function openModalDialog(dialog) {
  if (!dialog) {
    return;
  }

  if (typeof dialog.showModal === 'function') {
    if (!dialog.open) {
      dialog.showModal();
    }
    return;
  }

  dialog.setAttribute('open', 'open');
}

function closeModalDialog(dialog) {
  if (!dialog) {
    return;
  }

  if (typeof dialog.close === 'function') {
    if (dialog.open) {
      dialog.close();
    }
    return;
  }

  dialog.removeAttribute('open');
}

function ensureModalSlot(rootDocument) {
  let modalSlot = rootDocument.getElementById(MODAL_SLOT_ID);
  if (!modalSlot) {
    modalSlot = rootDocument.createElement('div');
    modalSlot.id = MODAL_SLOT_ID;

    const openButton = rootDocument.createElement('button');
    openButton.id = MODAL_OPEN_BUTTON_ID;
    openButton.type = 'button';
    openButton.textContent = MODAL_OPEN_BUTTON_LABEL;

    const dialog = rootDocument.createElement('dialog');
    dialog.id = MODAL_DIALOG_ID;

    const frame = rootDocument.createElement('div');
    frame.className = 'roger-panel-modal-frame';

    const modalHeader = rootDocument.createElement('div');
    modalHeader.className = 'roger-panel-modal-header';

    const closeButton = rootDocument.createElement('button');
    closeButton.id = MODAL_CLOSE_BUTTON_ID;
    closeButton.type = 'button';
    closeButton.textContent = 'Close';

    const modalContent = rootDocument.createElement('div');
    modalContent.id = MODAL_CONTENT_ID;

    closeButton.addEventListener('click', () => closeModalDialog(dialog));
    openButton.addEventListener('click', () => openModalDialog(dialog));
    dialog.addEventListener('cancel', (event) => {
      if (event?.preventDefault) {
        event.preventDefault();
      }
      closeModalDialog(dialog);
    });

    modalHeader.appendChild(closeButton);
    frame.appendChild(modalHeader);
    frame.appendChild(modalContent);
    dialog.appendChild(frame);

    modalSlot.appendChild(openButton);
    modalSlot.appendChild(dialog);
  }

  const slotHost = rootDocument.body || rootDocument.documentElement;
  mountInto(slotHost, modalSlot, { prepend: true });

  return {
    slot: modalSlot,
    content: rootDocument.getElementById(MODAL_CONTENT_ID),
  };
}

function removeSlot(rootDocument, slotId) {
  const slot = rootDocument.getElementById(slotId);
  if (slot) {
    slot.remove();
  }
}

function createBrandChip(rootDocument) {
  const chip = rootDocument.createElement('span');
  chip.id = BRAND_CHIP_ID;
  chip.className = BRAND_CHIP_CLASS;
  chip.setAttribute('aria-label', 'Roger identity');
  const iconUrl = readRuntimeAssetUrl('static/roger-mark.svg');
  if (iconUrl) {
    const icon = rootDocument.createElement('img');
    icon.className = 'roger-panel-brandicon';
    icon.src = iconUrl;
    icon.alt = '';
    icon.setAttribute('aria-hidden', 'true');
    chip.appendChild(icon);
  }
  const label = rootDocument.createElement('span');
  label.className = 'roger-panel-brandlabel';
  label.textContent = 'Roger';
  chip.appendChild(label);
  return chip;
}

function createPanel(context, rootDocument) {
  ensurePanelStyles(rootDocument);

  const panel = rootDocument.createElement('section');
  panel.id = PANEL_ID;
  panel.className = 'roger-panel roger-panel--inline';

  const brandBar = rootDocument.createElement('div');
  brandBar.className = 'roger-panel-brandbar';

  const brandMark = rootDocument.createElement('div');
  brandMark.className = 'roger-panel-brandmark';
  brandMark.appendChild(createBrandChip(rootDocument));

  const headingGroup = rootDocument.createElement('div');
  headingGroup.className = 'roger-panel-heading-group';

  const heading = rootDocument.createElement('h3');
  heading.id = HEADING_ID;
  heading.className = 'roger-panel-heading';
  heading.textContent = 'Roger Reviewer';

  const subheading = rootDocument.createElement('p');
  subheading.id = SUBHEADING_ID;
  subheading.className = 'roger-panel-subheading';
  subheading.textContent = `${context.owner}/${context.repo}#${context.pr_number}`;

  headingGroup.appendChild(heading);
  headingGroup.appendChild(subheading);
  brandMark.appendChild(headingGroup);
  brandBar.appendChild(brandMark);

  const badge = rootDocument.createElement('p');
  badge.id = BADGE_ID;
  badge.className = 'roger-panel-badge';

  const info = rootDocument.createElement('details');
  info.className = 'roger-panel-info';

  const infoToggle = rootDocument.createElement('summary');
  infoToggle.className = 'roger-panel-info-toggle';
  infoToggle.textContent = 'i';
  infoToggle.setAttribute('aria-label', 'Roger launch details');

  const infoPanel = rootDocument.createElement('div');
  infoPanel.className = 'roger-panel-info-panel';
  infoPanel.setAttribute('role', 'note');

  const buildLabel = readExtensionBuildLabel();
  infoToggle.setAttribute(
    'title',
    buildLabel
      ? `Roger launch details. Extension build ${buildLabel}.`
      : 'Roger launch details. Extension build unavailable.'
  );

  const infoText = rootDocument.createElement('p');
  infoText.id = INFO_TEXT_ID;
  infoText.textContent = DEFAULT_INFO_MESSAGE;

  infoPanel.appendChild(infoText);
  info.appendChild(infoToggle);
  info.appendChild(infoPanel);

  brandBar.appendChild(info);
  panel.appendChild(brandBar);
  panel.appendChild(badge);

  const buttonRow = rootDocument.createElement('div');
  buttonRow.className = 'roger-panel-button-row';

  for (const action of ACTIONS) {
    const button = rootDocument.createElement('button');
    button.className = GITHUB_ACTION_BUTTON_CLASS;
    button.type = 'button';
    button.textContent = action.label;
    button.dataset.action = action.id;
    button.addEventListener('click', () => triggerLaunch(action.id, context, button));
    buttonRow.appendChild(button);
  }

  panel.appendChild(buttonRow);
  applyActionModel(panel, lastAttentionState);

  const status = rootDocument.createElement('p');
  status.id = STATUS_ID;
  status.className = 'roger-panel-status roger-panel-status--ok';
  status.hidden = true;
  panel.appendChild(status);

  return panel;
}

function updatePanelHeading(panel, context) {
  const heading = panel.querySelector(`#${HEADING_ID}`);
  const subheading = panel.querySelector(`#${SUBHEADING_ID}`);
  if (!heading || !subheading) {
    return;
  }

  heading.textContent = 'Roger Reviewer';
  subheading.textContent = `${context.owner}/${context.repo}#${context.pr_number}`;
}

function removePanel(rootDocument) {
  const panel = rootDocument.getElementById(PANEL_ID);
  if (panel) {
    panel.remove();
  }

  removeSlot(rootDocument, INLINE_SLOT_ID);
  removeSlot(rootDocument, RAIL_SLOT_ID);
  removeSlot(rootDocument, MODAL_SLOT_ID);
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
    removeSlot(rootDocument, RAIL_SLOT_ID);
    removeSlot(rootDocument, MODAL_SLOT_ID);
  } else if (placement.mode === 'rail') {
    const railSlot = ensureRailSlot(rootDocument, placement.mountNode, placement.beforeNode || null);
    mountInto(railSlot, panel);
    removeSlot(rootDocument, INLINE_SLOT_ID);
    removeSlot(rootDocument, MODAL_SLOT_ID);
  } else {
    const modalPlacement = ensureModalSlot(rootDocument);
    if (modalPlacement.content) {
      mountInto(modalPlacement.content, panel);
    }
    removeSlot(rootDocument, INLINE_SLOT_ID);
    removeSlot(rootDocument, RAIL_SLOT_ID);
  }

  applyPanelModeStyles(panel, placement.mode);
  applyActionModel(panel, lastAttentionState);

  if (placement.mode === 'modal' && lastPanelMode !== 'modal') {
    const dialog = rootDocument.getElementById(MODAL_DIALOG_ID);
    openModalDialog(dialog);
    clearStatus();
    setInfoMessage(MODAL_FALLBACK_STATUS);
  }

  if (placement.mode !== 'modal' && lastPanelMode === 'modal') {
    const dialog = rootDocument.getElementById(MODAL_DIALOG_ID);
    closeModalDialog(dialog);
  }

  lastPanelMode = placement.mode;
  return placement.mode;
}

let lastContextKey = null;
let lastPanelMode = null;
let lastAttentionState = null;
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
    lastPanelMode = null;
    lastAttentionState = null;
    return;
  }

  ensurePanel(context, rootDocument);

  const nextKey = contextKey(context);
  if (lastContextKey !== nextKey) {
    lastContextKey = nextKey;
    lastAttentionState = null;
    const panel = rootDocument.getElementById(PANEL_ID);
    if (panel) {
      applyActionModel(panel, lastAttentionState);
    }
    clearStatus();
    setInfoMessage(DEFAULT_INFO_MESSAGE);
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
  const panel = typeof document !== 'undefined' ? document.getElementById(PANEL_ID) : null;

  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    lastAttentionState = null;
    if (panel) {
      applyActionModel(panel, lastAttentionState);
    }
    clearAttentionBadge();
    setInfoMessage('Bridge unavailable in browser context. Open Roger locally and run `rr` manually.');
    setStatus('Bridge unavailable in browser context. Open Roger locally and run rr manually.', true, { revealInline: true });
    return;
  }

  const previousText = button.textContent;
  button.disabled = true;
  button.textContent = '…';
  setStatus('Dispatching launch intent...', false, { revealInline: true });

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
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        setInfoMessage(`Bridge error: ${chrome.runtime.lastError.message}`);
        setStatus(`Bridge error: ${chrome.runtime.lastError.message}`, true, { revealInline: true });
        return;
      }

      if (!response) {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        setInfoMessage('No bridge response. Open Roger locally and run `rr` manually.');
        setStatus('No bridge response. Open Roger locally and run rr manually.', true, { revealInline: true });
        return;
      }

      if (!response.ok) {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        setInfoMessage(appendGuidance(response.message, response.guidance));
        setStatus(appendGuidance(response.message, response.guidance), true, { revealInline: true });
        return;
      }

      if (response.mode === 'custom_url_fallback') {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        clearAttentionBadge();
        setInfoMessage('Launched via URL fallback. Open Roger locally for authoritative status.');
        setStatus('Launched via URL fallback. Open Roger locally for authoritative status.', false, { revealInline: true });
        return;
      }

      if (response.mode === 'native_messaging' && response.attention_state) {
        lastAttentionState = response.attention_state;
        if (panel) {
          applyActionModel(panel, lastAttentionState);
        }
        setAttentionBadge(response.attention_state, response.freshness_label || null);
        setInfoMessage(appendGuidance(response.message || 'Launch intent dispatched.', response.guidance));
        setStatus(
          appendGuidance(response.message || 'Launch intent dispatched.', response.guidance),
          false,
          { revealInline: true }
        );
        return;
      }

      lastAttentionState = null;
      if (panel) {
        applyActionModel(panel, lastAttentionState);
      }
      setInfoMessage(appendGuidance(response.message || 'Launch intent dispatched.', response.guidance));
      setStatus(
        appendGuidance(response.message || 'Launch intent dispatched.', response.guidance),
        false,
        { revealInline: true }
      );
      requestStatusMirror(context);
    }
  );
}

if (typeof window !== 'undefined' && typeof document !== 'undefined') {
  bootstrapRogerPanel();
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    appendGuidance,
    BRAND_CHIP_CLASS,
    GITHUB_ACTION_BUTTON_CLASS,
    INLINE_ANCHOR_SELECTORS,
    MODAL_FALLBACK_STATUS,
    MODAL_OPEN_BUTTON_LABEL,
    applyActionModel,
    applyPanelModeStyles,
    createBrandChip,
    createPanel,
    deriveActionModel,
    ensurePanel,
    findInlineAnchor,
    mountInto,
    parsePullRequestContext,
    pickInlineAnchorSelector,
    readExtensionBuildLabel,
    readRuntimeAssetUrl,
    refreshPanelForCurrentPage,
    resolvePanelPlacement,
    clearStatus,
    setStatus,
  };
}
