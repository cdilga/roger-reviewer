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

const RESUME_ACTION_LABEL = 'Resume Existing Review';
const ACTIONS = [
  { id: 'start_review', label: 'Start Review in Roger' },
  { id: 'resume_review', label: RESUME_ACTION_LABEL },
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

// Session existence is durable local truth from the status probe:
// - 0 sessions: there is nothing to resume, so no resume button at all.
// - >=1 sessions: an existing review makes resuming the likeliest intent.
// - null/undefined (launch-only/degraded bridge): we genuinely do not know,
//   so keep the legacy both-buttons surface; launch failures are loud.
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

  const resumeLabel =
    knownSessionCount !== null && knownSessionCount > 1
      ? `${RESUME_ACTION_LABEL} (${knownSessionCount})`
      : RESUME_ACTION_LABEL;

  return {
    visibleActions,
    primaryActionId,
    resumeLabel,
    sessionCount: knownSessionCount,
  };
}

function applyActionModel(panel, attentionState, sessionCount = null) {
  if (!panel || typeof panel.querySelectorAll !== 'function') {
    return deriveActionModel(attentionState, sessionCount);
  }

  const model = deriveActionModel(attentionState, sessionCount);
  for (const button of panel.querySelectorAll('button[data-action]')) {
    const actionId = button.dataset?.action;
    const isVisible = actionId ? model.visibleActions.has(actionId) : true;
    const isPrimary = actionId === model.primaryActionId;
    const isTertiary = actionId === 'show_findings' && isVisible && !isPrimary;
    button.hidden = !isVisible;
    if (actionId === 'resume_review' && button.textContent !== model.resumeLabel) {
      button.textContent = model.resumeLabel;
    }
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

// ---------------------------------------------------------------------------
// GitHub PR-listing route + row-target discovery (bead ziv8.1).
//
// This block is intentionally discovery-only: it recognizes repository PR-list
// routes and extracts row-level PR targets as a pure, testable contract. It
// does NOT render any controls (that is ziv8.2) and is deliberately kept
// separate from the PR-detail panel path above so `/owner/repo/pull/<n>`
// behavior does not regress.
// ---------------------------------------------------------------------------

const PR_LISTING_ROUTE = 'pr_listing';

// Row containers Roger is willing to anchor a row target onto. Anchored on
// stable list-item containers rather than cosmetic text wrappers. If a
// `/pull/<n>` link is not inside one of these within a bounded climb, Roger
// degrades to no target rather than risk attaching to a repository-level
// action bar, breadcrumb, or unrelated chrome.
const PR_LISTING_ROW_TESTIDS = new Set(['list-view-item', 'issue-row']);
const PR_LISTING_ROW_MAX_CLIMB = 12;

// Roger's PR list view defaults to GitHub's implicit `is:open` scope. A `q`
// query that explicitly broadens to closed/merged/all is treated as not
// open-only; an explicit `is:open` (or no query at all) is open-only.
function isOpenOnlyListingQuery(search) {
  if (typeof search !== 'string' || search.trim() === '') {
    return true;
  }

  let params;
  try {
    params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  } catch {
    return false;
  }

  if (!params.has('q')) {
    return true;
  }

  const q = (params.get('q') || '').toLowerCase();
  if (q.trim() === '') {
    return true;
  }
  if (/\bis:closed\b/.test(q) || /\bis:merged\b/.test(q) || /\bis:all\b/.test(q)) {
    return false;
  }
  if (/\bis:open\b/.test(q)) {
    return true;
  }
  // A query with no explicit state qualifier can still surface closed PRs on
  // GitHub, so stay conservative and do not treat it as open-only.
  return false;
}

// Recognize repository PR-list routes such as `/owner/repo/pulls` and its
// filtered variants. Returns null for PR-detail pages, the global `/pulls`
// dashboard, and every non-listing page. Accepts an optional location-like
// object so SPA navigation can be exercised deterministically in tests.
function parsePullRequestListContext(locationLike) {
  const loc =
    locationLike || (typeof window !== 'undefined' ? window.location : null);
  if (!loc || typeof loc.pathname !== 'string') {
    return null;
  }

  const match = loc.pathname.match(/^\/([^/]+)\/([^/]+)\/pulls\/?$/);
  if (!match) {
    return null;
  }

  return {
    owner: decodeURIComponent(match[1]),
    repo: decodeURIComponent(match[2]),
    route: PR_LISTING_ROUTE,
    openOnly: isOpenOnlyListingQuery(loc.search || ''),
  };
}

// Parse a candidate href into a `/owner/repo/pull/<n>` target, but only when it
// belongs to the listing's current repository. Absolute and relative hrefs and
// sub-pages (`/pull/<n>/files`) are all normalized to the base PR target;
// non-PR links (issues, compare, other repos) return null.
function parsePullTargetHref(href, owner, repo) {
  if (typeof href !== 'string') {
    return null;
  }

  let path = href;
  const schemeMatch = href.match(/^https?:\/\/[^/]+(\/.*)$/i);
  if (schemeMatch) {
    path = schemeMatch[1];
  }

  const match = path.match(/^\/([^/]+)\/([^/]+)\/pull\/(\d+)(?:[/?#].*)?$/);
  if (!match) {
    return null;
  }

  const hrefOwner = decodeURIComponent(match[1]);
  const hrefRepo = decodeURIComponent(match[2]);
  if (owner && repo && (hrefOwner !== owner || hrefRepo !== repo)) {
    return null;
  }

  const prNumber = Number(match[3]);
  if (!Number.isInteger(prNumber) || prNumber <= 0) {
    return null;
  }

  return { owner: hrefOwner, repo: hrefRepo, pr_number: prNumber };
}

function readNodeClassName(node) {
  if (!node) {
    return '';
  }
  if (typeof node.className === 'string') {
    return node.className;
  }
  if (typeof node.getAttribute === 'function') {
    return node.getAttribute('class') || '';
  }
  return '';
}

function readNodeTestId(node) {
  if (!node) {
    return '';
  }
  if (typeof node.getAttribute === 'function') {
    const attr = node.getAttribute('data-testid');
    if (attr) {
      return attr;
    }
  }
  if (node.dataset && typeof node.dataset.testid === 'string') {
    return node.dataset.testid;
  }
  return '';
}

function isPrListingRowNode(node) {
  if (!node || typeof node !== 'object') {
    return false;
  }

  const testId = readNodeTestId(node);
  if (testId && PR_LISTING_ROW_TESTIDS.has(testId)) {
    return true;
  }

  const className = readNodeClassName(node);
  if (/\bjs-issue-row\b/.test(className)) {
    return true;
  }

  const tag = (node.tagName || '').toUpperCase();
  if (tag === 'LI' && /\bListItem\b/.test(className)) {
    return true;
  }

  return false;
}

// Climb from a `/pull/<n>` link to its enclosing list-row container, bounded so
// a stray link never resolves to distant page chrome.
function findPrListingRowNode(anchor) {
  let node = anchor;
  let depth = 0;
  while (node && depth < PR_LISTING_ROW_MAX_CLIMB) {
    if (isPrListingRowNode(node)) {
      return node;
    }
    node = node.parentElement || null;
    depth += 1;
  }
  return null;
}

function* walkElementDescendants(node) {
  const children = node && node.children ? Array.from(node.children) : [];
  for (const child of children) {
    yield child;
    yield* walkElementDescendants(child);
  }
}

// Read an open/closed/merged/draft signal off a single node from GitHub's
// state markers (Primer color classes, aria-labels, or the react state label).
function readNodeReviewState(node) {
  if (!node || typeof node !== 'object') {
    return null;
  }

  const className = readNodeClassName(node).toLowerCase();
  if (/\bstate--merged\b/.test(className) || /\bcolor-fg-done\b/.test(className)) {
    return 'merged';
  }
  if (/\bstate--closed\b/.test(className) || /\bcolor-fg-closed\b/.test(className)) {
    return 'closed';
  }
  if (/\bstate--open\b/.test(className) || /\bcolor-fg-open\b/.test(className)) {
    return 'open';
  }

  const label =
    typeof node.getAttribute === 'function'
      ? (node.getAttribute('aria-label') || '').toLowerCase().trim()
      : '';
  if (label) {
    if (label.includes('merged')) {
      return 'merged';
    }
    if (label.includes('closed')) {
      return 'closed';
    }
    if (label.includes('draft')) {
      return 'draft';
    }
    if (label.includes('open')) {
      return 'open';
    }
  }

  const testId = readNodeTestId(node).toLowerCase();
  if (testId === 'issue-pr-state-label' || testId === 'pr-state') {
    const text = (node.textContent || '').toLowerCase();
    if (text.includes('merged')) {
      return 'merged';
    }
    if (text.includes('closed')) {
      return 'closed';
    }
    if (text.includes('draft')) {
      return 'draft';
    }
    if (text.includes('open')) {
      return 'open';
    }
  }

  return null;
}

// Resolve a row's review state. Closed/merged signals win over open if both are
// present so Roger never treats a closed row as eligible.
function detectRowReviewState(row) {
  let found = readNodeReviewState(row);
  if (found === 'closed' || found === 'merged') {
    return found;
  }
  for (const node of walkElementDescendants(row)) {
    const state = readNodeReviewState(node);
    if (state === 'closed' || state === 'merged') {
      return state;
    }
    if (state && !found) {
      found = state;
    }
  }
  return found;
}

// Draft PRs are an open sub-state, so they remain eligible. Indeterminate rows
// (no detectable state marker) are eligible only when the listing route is
// explicitly open-only.
function isRowOpenEligible(row, openOnly) {
  const state = detectRowReviewState(row);
  if (state === 'closed' || state === 'merged') {
    return false;
  }
  if (state === 'open' || state === 'draft') {
    return true;
  }
  return openOnly === true;
}

// Extract unique, open-eligible row-level PR targets from a rendered PR list.
// Pure and side-effect free: it reads the DOM and returns
// `{ owner, repo, pr_number, row_node }` records. Repeated calls on an
// unchanged DOM return the same target set (idempotent by construction), and it
// returns [] for unrecognized DOM or a non-listing context rather than
// attaching anything.
function extractPrListingRowTargets(rootDocument, listingContext) {
  if (!rootDocument || typeof rootDocument.querySelectorAll !== 'function') {
    return [];
  }
  if (!listingContext || listingContext.route !== PR_LISTING_ROUTE) {
    return [];
  }

  const { owner, repo, openOnly } = listingContext;
  const anchors = rootDocument.querySelectorAll('a[href]');
  const targets = [];
  const seenRows = new Set();
  const seenKeys = new Set();

  for (const anchor of anchors) {
    const href =
      typeof anchor.getAttribute === 'function' ? anchor.getAttribute('href') : null;
    const parsed = parsePullTargetHref(href, owner, repo);
    if (!parsed) {
      continue;
    }

    const row = findPrListingRowNode(anchor);
    if (!row) {
      // Degrade: a PR link outside any recognized row container (action bar,
      // breadcrumb, unknown DOM) gets no target rather than a misplaced one.
      continue;
    }
    if (seenRows.has(row)) {
      // Collapse duplicate `/pull/<n>` links inside the same row to one target.
      continue;
    }
    seenRows.add(row);

    if (!isRowOpenEligible(row, openOnly)) {
      continue;
    }

    const key = `${parsed.owner}/${parsed.repo}#${parsed.pr_number}`;
    if (seenKeys.has(key)) {
      continue;
    }
    seenKeys.add(key);

    targets.push({
      owner: parsed.owner,
      repo: parsed.repo,
      pr_number: parsed.pr_number,
      row_node: row,
    });
  }

  return targets;
}

// ---------------------------------------------------------------------------
// GitHub PR-listing row controls (bead ziv8.2).
//
// Renders one additive, GitHub-native "Start Review in Roger" control per
// discovered open PR row. Deliberately does NOT reuse the single global
// PR-detail PANEL_ID node (a list page holds many PR targets), keeps per-row UI
// state local, and stays idempotent under Turbo/PJAX/Morph rerenders. Outbound
// dispatch through the bridge is bead ziv8.3; here the control owns rendering,
// placement, accessibility, and a safe row-local launching affordance only.
// ---------------------------------------------------------------------------

const LISTING_STYLE_ID = 'roger-reviewer-listing-style';
const LISTING_CONTROL_CLASS = 'rr-listing-control';
const LISTING_BUTTON_CLASS = 'rr-listing-button';
const LISTING_STATUS_CLASS = 'rr-listing-status';
const LISTING_ROW_MOUNTED_ATTR = 'data-rr-listing-mounted';
const LISTING_START_LABEL = 'Start Review in Roger';
// The button always carries the full accessible name via aria-label. The
// visible text may be ellipsized by CSS at narrow GitHub list widths, but the
// accessible name stays "Start Review in Roger".
const LISTING_GITHUB_BUTTON_CLASS = 'btn btn-sm';

function listingControlIdForTarget(target) {
  return `roger-reviewer-listing-${target.owner}-${target.repo}-${target.pr_number}`.replace(
    /[^a-zA-Z0-9_-]/g,
    '-'
  );
}

function ensureListingStyles(rootDocument) {
  if (!rootDocument || typeof rootDocument.createElement !== 'function') {
    return;
  }
  if (
    typeof rootDocument.getElementById === 'function' &&
    rootDocument.getElementById(LISTING_STYLE_ID)
  ) {
    return;
  }

  const styleNode = rootDocument.createElement('style');
  styleNode.id = LISTING_STYLE_ID;
  styleNode.textContent = `
.${LISTING_CONTROL_CLASS} {
  display: inline-flex;
  align-items: center;
  margin-left: 8px;
  vertical-align: middle;
  max-width: 100%;
}

.${LISTING_BUTTON_CLASS} {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  min-height: 28px;
  padding: 0 10px;
  border: 1px solid var(--button-default-borderColor-rest, var(--borderColor-default, #d0d7de));
  border-radius: 6px;
  background: var(--button-default-bgColor-rest, var(--bgColor-default, #ffffff));
  color: var(--button-default-fgColor-rest, var(--fgColor-default, #1f2328));
  font-size: 12px;
  line-height: 1.25;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: pointer;
}

.${LISTING_BUTTON_CLASS}:hover:not(:disabled) {
  background: var(--button-default-bgColor-hover, var(--bgColor-muted, #f3f4f6));
  border-color: var(--button-default-borderColor-hover, var(--borderColor-emphasis, #8c959f));
}

.${LISTING_BUTTON_CLASS}:disabled {
  cursor: not-allowed;
  opacity: 0.7;
}

.${LISTING_BUTTON_CLASS} .${LISTING_BUTTON_CLASS}-accent {
  color: var(--fgColor-accent, #0969da);
  font-weight: 700;
}

.${LISTING_STATUS_CLASS} {
  display: inline-block;
  margin-left: 6px;
  max-width: 320px;
  font-size: 11px;
  line-height: 1.3;
  color: var(--fgColor-muted, #656d76);
  white-space: normal;
  vertical-align: middle;
}

.${LISTING_STATUS_CLASS}[data-state="error"] {
  color: var(--fgColor-danger, #d1242f);
}

.${LISTING_STATUS_CLASS}[data-state="success"] {
  color: var(--fgColor-success, #1a7f37);
}
  `.trim();

  const styleHost = rootDocument.head || rootDocument.documentElement || rootDocument.body;
  if (styleHost && typeof styleHost.appendChild === 'function') {
    styleHost.appendChild(styleNode);
  }
}

// Row-local status writer. Touches ONLY the passed row status node so a launch
// in one row never rewrites the PR-detail panel or other rows. Status is
// deliberately persistent (no auto-hide timer) so launch failures stay visible
// until the next action supersedes them.
function setListingRowStatus(statusNode, message, state) {
  if (!statusNode) {
    return;
  }
  if (!message) {
    statusNode.hidden = true;
    statusNode.textContent = '';
    statusNode.dataset.state = '';
    return;
  }
  statusNode.hidden = false;
  statusNode.textContent = message;
  statusNode.dataset.state = state || 'info';
}

// Dispatch a row launch through Roger's existing daemonless bridge using the
// already-supported `start_review` action. Keeps failure handling as loud as
// the PR-detail triggerLaunch() path (native host missing, forbidden origin,
// invalid/blocked response, missing session id) but writes only this row's
// button + status. The chrome surface is injectable for tests.
function dispatchListingRowLaunch(target, button, statusNode, options = {}) {
  const chromeApi = options.chrome || (typeof chrome !== 'undefined' ? chrome : null);

  const restoreButton = () => {
    if (button) {
      button.disabled = false;
      button.dataset.rrLaunching = 'false';
    }
  };

  if (!chromeApi || !chromeApi.runtime || typeof chromeApi.runtime.sendMessage !== 'function') {
    restoreButton();
    setListingRowStatus(
      statusNode,
      'Bridge unavailable in browser context. Open Roger locally and run `rr` manually.',
      'error'
    );
    return;
  }

  setListingRowStatus(
    statusNode,
    `Dispatching review for ${target.owner}/${target.repo}#${target.pr_number}…`,
    'pending'
  );

  chromeApi.runtime.sendMessage(
    {
      type: 'roger_bridge_launch',
      intent: {
        action: 'start_review',
        owner: target.owner,
        repo: target.repo,
        pr_number: target.pr_number,
      },
    },
    (response) => {
      restoreButton();

      if (chromeApi.runtime.lastError) {
        setListingRowStatus(
          statusNode,
          appendGuidance(
            `Bridge error: ${chromeApi.runtime.lastError.message}`,
            BRIDGE_DISCONNECT_GUIDANCE
          ),
          'error'
        );
        return;
      }

      if (!response) {
        setListingRowStatus(
          statusNode,
          appendGuidance('No bridge response.', BRIDGE_DISCONNECT_GUIDANCE),
          'error'
        );
        return;
      }

      if (!response.ok) {
        setListingRowStatus(statusNode, appendGuidance(response.message, response.guidance), 'error');
        return;
      }

      if (response.mode === 'custom_url_fallback') {
        setListingRowStatus(
          statusNode,
          'Launched via URL fallback. Open Roger locally for authoritative status.',
          'success'
        );
        return;
      }

      // Success (native_messaging or generic ok). Anchor the row status on this
      // row's target and include the returned session id when present.
      const base = formatLaunchSuccessStatus(response);
      const targetTag = `${target.owner}/${target.repo}#${target.pr_number}`;
      const message = base.includes(`#${target.pr_number}`) ? base : `${targetTag}: ${base}`;
      setListingRowStatus(statusNode, message, 'success');
    }
  );
}

// Row-local launch affordance. Disables and marks ONLY this row's button while a
// launch is in flight so a single row launching never rewrites other rows. By
// default it dispatches through the bridge; tests/other callers can override
// with options.onLaunch.
function handleListingRowLaunch(target, button, statusNode, options = {}) {
  if (button) {
    button.disabled = true;
    button.dataset.rrLaunching = 'true';
  }
  if (typeof options.onLaunch === 'function') {
    options.onLaunch(target, button, statusNode);
    return;
  }
  dispatchListingRowLaunch(target, button, statusNode, options);
}

function createListingRowControl(target, rootDocument, options = {}) {
  const control = rootDocument.createElement('span');
  control.id = listingControlIdForTarget(target);
  control.className = LISTING_CONTROL_CLASS;

  const button = rootDocument.createElement('button');
  button.type = 'button';
  button.className = `${LISTING_BUTTON_CLASS} ${LISTING_GITHUB_BUTTON_CLASS}`;
  button.textContent = LISTING_START_LABEL;
  // Accessible name is always the full label even if CSS ellipsizes the text.
  button.setAttribute('aria-label', LISTING_START_LABEL);
  button.dataset.action = 'start_review';
  button.dataset.owner = target.owner;
  button.dataset.repo = target.repo;
  button.dataset.prNumber = String(target.pr_number);

  const statusNode = rootDocument.createElement('span');
  statusNode.className = LISTING_STATUS_CLASS;
  statusNode.hidden = true;

  if (typeof button.addEventListener === 'function') {
    button.addEventListener('click', () =>
      handleListingRowLaunch(target, button, statusNode, options)
    );
  }

  control.appendChild(button);
  control.appendChild(statusNode);
  return control;
}

function rowHasListingControl(row, controlId) {
  if (row && row.id === controlId) {
    return true;
  }
  for (const node of walkElementDescendants(row)) {
    if (node && node.id === controlId) {
      return true;
    }
  }
  return false;
}

// Inject exactly one Roger start control into each discovered open PR row.
// Idempotent: an already-mounted row is left untouched, so repeated calls after
// DOM mutation or SPA navigation never duplicate controls.
function ensurePrListingRowControls(rootDocument, listingContext, options = {}) {
  const targets = extractPrListingRowTargets(rootDocument, listingContext);
  if (targets.length === 0) {
    return [];
  }

  ensureListingStyles(rootDocument);

  const mounted = [];
  for (const target of targets) {
    const row = target.row_node;
    if (!row || typeof rootDocument.createElement !== 'function') {
      continue;
    }

    const controlId = listingControlIdForTarget(target);
    if (rowHasListingControl(row, controlId)) {
      continue;
    }

    const control = createListingRowControl(target, rootDocument, options);
    if (typeof row.appendChild === 'function') {
      row.appendChild(control);
      if (typeof row.setAttribute === 'function') {
        row.setAttribute(LISTING_ROW_MOUNTED_ATTR, controlId);
      }
      mounted.push(control);
    }
  }

  return mounted;
}

// Remove all listing controls (used when navigating away from a PR list, e.g.
// onto a PR-detail page) so stale controls never linger on a non-listing route.
function removeAllListingControls(rootDocument) {
  if (!rootDocument || typeof rootDocument.querySelectorAll !== 'function') {
    return;
  }
  const controls = rootDocument.querySelectorAll(`.${LISTING_CONTROL_CLASS}`);
  for (const control of controls) {
    if (control && typeof control.remove === 'function') {
      control.remove();
    }
  }
}

// Bridge the listing-control lifecycle into the existing navigation refresh.
// On a PR-list route it injects row controls; on any other route it clears
// them. Kept fully separate from the PR-detail panel path.
function refreshPrListingControls(rootDocument, options = {}) {
  const listingContext = parsePullRequestListContext();
  if (!listingContext) {
    removeAllListingControls(rootDocument);
    return;
  }
  ensurePrListingRowControls(rootDocument, listingContext, options);
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

const BRIDGE_DISCONNECT_GUIDANCE =
  'Native bridge unreachable. Run `rr extension setup --browser <edge|chrome|brave>`, then `rr extension doctor --browser <edge|chrome|brave>`, and reload this page.';

// Content-side settle bound for the bounded status mirror. On Edge the
// content->background->native leg can fail to settle if the module service
// worker callback never fires (worker torn down before the host reply lands).
// This fallback guarantees the panel always degrades to launch-only within
// bounded time instead of hanging un-settled. It MUST exceed the worker-side
// watchdog (STATUS_PROBE_TIMEOUT_MS ~4000ms in background/main.js) so the
// worker leg is always given its full chance to resolve first.
const STATUS_MIRROR_SETTLE_TIMEOUT_MS = 5000;

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

  // Status messages are deliberately persistent: they stay visible until the
  // next action or a newer truthful state supersedes them. An earlier
  // auto-hide timer here made launch failures silently disappear after 4.5s.
  const panel = statusNode.parentElement;
  if (panel?.classList?.contains('roger-panel--inline') && options.revealInline) {
    statusNode.classList.add('roger-panel-status--inline-visible');
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

// Bounded status mirroring updates the badge, action model, and info text.
// It deliberately never touches the action status line: that line is owned by
// launch actions and must stay visible until the next action supersedes it.
// (A clearStatus() here used to wipe fresh launch results back to null.)
function requestStatusMirror(context) {
  const panel = typeof document !== 'undefined' ? document.getElementById(PANEL_ID) : null;

  const applyMirroredModel = (attentionState, sessionCount) => {
    lastAttentionState = attentionState;
    lastSessionCount = normalizeSessionCount(sessionCount);
    if (panel) {
      applyActionModel(panel, lastAttentionState, lastSessionCount);
    }
  };

  // One-shot settle guard. The worker leg can settle the panel from its
  // callback, OR — when that callback never fires because Edge tore the module
  // service worker down before the host reply landed — the timeout fallback
  // settles it to launch-only. Whichever fires first wins; the loser is a
  // no-op. This guarantees the panel is always settled within bounded time and
  // is never left hanging.
  let settled = false;
  let settleTimer = null;
  const settleOnce = (apply) => {
    if (settled) {
      return;
    }
    settled = true;
    if (settleTimer !== null && typeof clearTimeout === 'function') {
      clearTimeout(settleTimer);
      settleTimer = null;
    }
    apply();
  };

  // Reused launch-only degrade branch: the panel can still start Roger actions,
  // it just does not own live local session state. The default message is a
  // VISIBLE one-line "mirror unavailable" note — the status probe used to
  // degrade silently (showing nothing), which hid a real broken-mirror signal.
  const degradeToLaunchOnly = (
    message = 'Local status mirror unavailable — launch-only mode. Open Roger locally (`rr status`) for authoritative detail.'
  ) => {
    applyMirroredModel(null, null);
    clearAttentionBadge();
    setInfoMessage(message);
  };

  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    settleOnce(() => degradeToLaunchOnly());
    return;
  }

  // Bounded settle fallback: if the worker callback never fires, degrade to
  // launch-only so the panel never hangs un-settled. Must outlast the
  // worker-side watchdog (STATUS_PROBE_TIMEOUT_MS) so the worker leg resolves
  // first when it can.
  if (typeof setTimeout === 'function') {
    settleTimer = setTimeout(() => {
      settleOnce(() => degradeToLaunchOnly());
    }, STATUS_MIRROR_SETTLE_TIMEOUT_MS);
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
      settleOnce(() => {
        if (chrome.runtime.lastError) {
          degradeToLaunchOnly();
          return;
        }

        if (!response) {
          degradeToLaunchOnly(
            'No bounded status response. Open Roger locally (`rr status`) for authoritative detail.'
          );
          return;
        }

        if (!response.ok) {
          applyMirroredModel(null, null);
          clearAttentionBadge();
          setInfoMessage(appendGuidance(response.message, response.guidance));
          return;
        }

        // Durable local truth: no review session exists for this PR, so there
        // is nothing to resume and the panel must not pretend otherwise.
        if (response.mode === 'no_local_session') {
          applyMirroredModel(null, 0);
          clearAttentionBadge();
          setInfoMessage('No Roger review exists for this PR yet — start one.');
          return;
        }

        // Sessions exist but no fresh attention claim: surface the truthful
        // inventory without bluffing findings, drafts, or attention state.
        if (
          response.mode === 'session_inventory' &&
          normalizeSessionCount(response.session_count) !== null
        ) {
          applyMirroredModel(null, response.session_count);
          clearAttentionBadge();
          const count = normalizeSessionCount(response.session_count);
          setInfoMessage(
            `${count} local Roger review session${count === 1 ? '' : 's'} for this PR. ` +
              'Open Roger locally (`rr status`) for authoritative detail.'
          );
          return;
        }

        if (response.mode !== 'bounded_status' || !response.attention_state) {
          applyMirroredModel(null, null);
          clearAttentionBadge();
          setInfoMessage(
            appendGuidance(
              response.message || 'Launch-only mode. Open Roger locally for authoritative detail.',
              response.guidance
            )
          );
          return;
        }

        applyMirroredModel(response.attention_state, response.session_count);
        setAttentionBadge(response.attention_state, response.freshness_label || null);
        setInfoMessage(
          appendGuidance(response.message || 'Mirroring bounded Roger status.', response.guidance)
        );
      });
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

// Roger-owned dark-surface remap. GitHub/Primer ship light defaults on
// --bgColor-*/--fgColor-*/--borderColor-*; in dark host themes those tokens
// already flip, but the panel previously layered hardcoded `white` mixes and
// flat highlight rgba on top, which washed the surface, title, and buttons out.
// This block restates Roger's own surface/border/highlight/shadow tokens for
// dark so they resolve from the host's dark Primer values instead of bluffing
// a light treatment. It is emitted under every host-theme dark signal below.
const PANEL_DARK_SURFACE_VARS = `
  --rr-panel-surface: var(--overlay-bgColor, var(--bgColor-default, #161b22));
  --rr-panel-surface-muted: var(--bgColor-muted, #21262d);
  --rr-panel-surface-raised: color-mix(
    in srgb,
    var(--rr-panel-surface) 78%,
    var(--bgColor-emphasis, #30363d) 22%
  );
  --rr-panel-border: color-mix(
    in srgb,
    var(--borderColor-default, #30363d) 82%,
    rgba(240, 246, 252, 0.16)
  );
  --rr-panel-border-strong: color-mix(
    in srgb,
    var(--borderColor-emphasis, #6e7681) 78%,
    rgba(240, 246, 252, 0.22)
  );
  --rr-panel-text: var(--fgColor-default, #e6edf3);
  --rr-panel-muted: var(--fgColor-muted, #8b949e);
  --rr-brand-ink-700: var(--fgColor-default, #e6edf3);
  --rr-panel-highlight: rgba(240, 246, 252, 0.08);
  --rr-panel-shadow: rgba(1, 4, 9, 0.42);
  --rr-panel-metal-tint: var(--fgColor-default, #f0f6fc);
  --rr-panel-metal-tint-strength: 7%;
`;

// Each entry is a selector under which the Roger panel must adopt the dark
// surface remap. We deliberately cover the full matrix of how GitHub signals an
// active dark theme so the panel feels native regardless of placement of the
// host signal:
//   - data-color-mode="dark" on :root (html) — GitHub's most common explicit case
//   - data-color-mode="dark" on any ancestor (body/wrapper) — harness + some hosts
//   - data-color-mode="auto" + prefers-color-scheme: dark — GitHub "auto" theme
//   - bare prefers-color-scheme: dark — degraded host with no explicit mode attr
function buildDarkThemeBlocks(panelSelector) {
  const surfaceRule = (selector) => `${selector} {${PANEL_DARK_SURFACE_VARS}}`;

  const explicitDark = [
    surfaceRule(`:root[data-color-mode="dark"] ${panelSelector}`),
    surfaceRule(`[data-color-mode="dark"] ${panelSelector}`),
    surfaceRule(`${panelSelector}[data-color-mode="dark"]`),
  ].join('\n');

  const autoDark = `
@media (prefers-color-scheme: dark) {
  ${surfaceRule(`:root[data-color-mode="auto"] ${panelSelector}`)}
  ${surfaceRule(`[data-color-mode="auto"] ${panelSelector}`)}
  ${surfaceRule(`:root:not([data-color-mode="light"]) ${panelSelector}`)}
}`;

  return `${explicitDark}\n${autoDark}`;
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
  /* Metallic sheen tint mixed into button/chip gradients. Light themes tolerate
     a pure-white sheen; dark themes must use a restrained light overlay so the
     surface does not blow out toward white and wash the light foreground text.
     The dark remap restates this token. */
  --rr-panel-metal-tint: #ffffff;
  --rr-panel-metal-tint-strength: 16%;
  font-family: var(--fontStack-sansSerif, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif);
  background: var(--rr-panel-surface-muted);
  color: var(--rr-panel-text);
  border: 1px solid var(--rr-panel-border);
  padding: 12px;
}

${buildDarkThemeBlocks(`#${PANEL_ID}`)}

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
      color-mix(in srgb, var(--rr-panel-surface) 92%, var(--rr-panel-metal-tint) 8%) 0%,
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
    color-mix(in srgb, var(--rr-panel-surface) 86%, var(--rr-panel-metal-tint) 14%) 0%,
    color-mix(in srgb, var(--rr-panel-surface-raised) 92%, var(--rr-panel-metal-tint) 8%) 100%
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
      var(--button-default-bgColor-rest, var(--rr-panel-surface))
        calc(100% - var(--rr-panel-metal-tint-strength)),
      var(--rr-panel-metal-tint) var(--rr-panel-metal-tint-strength)
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
      var(--button-default-bgColor-hover, var(--rr-panel-surface-raised))
        calc(100% - var(--rr-panel-metal-tint-strength)),
      var(--rr-panel-metal-tint) var(--rr-panel-metal-tint-strength)
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
    color-mix(in srgb, var(--rr-panel-surface-raised) 86%, var(--rr-panel-metal-tint) 14%),
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
    color-mix(in srgb, var(--rr-panel-surface-raised) 88%, var(--rr-panel-metal-tint) 12%),
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
    color-mix(in srgb, var(--rr-panel-surface) 92%, var(--rr-panel-metal-tint) 8%),
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

// The Roger mark is rendered as an inline data: URI, NOT a chrome-extension://
// web-accessible resource. GitHub's strict img-src CSP blocks chrome-extension:
// images injected into the page, which made the mark render as a broken image.
// A data:image/svg+xml URI is permitted by GitHub's img-src and needs no
// resource load. Keep ROGER_MARK_SVG in sync with static/roger-mark.svg — a
// content test guards against drift.
const ROGER_MARK_SVG =
  '<svg width="96" height="96" viewBox="0 0 96 96" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">' +
  '<title id="title">Roger walkie mark</title>' +
  '<desc id="desc">Metallic walkie-talkie icon with radio pulse accent.</desc>' +
  '<defs>' +
  '<linearGradient id="plate" x1="12" y1="10" x2="84" y2="86" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#F6F8FA" /><stop offset="0.48" stop-color="#D0D7DE" /><stop offset="1" stop-color="#8C959F" /></linearGradient>' +
  '<linearGradient id="body" x1="27" y1="17" x2="69" y2="80" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#6E7781" /><stop offset="0.54" stop-color="#4A535D" /><stop offset="1" stop-color="#2D333B" /></linearGradient>' +
  '<linearGradient id="screen" x1="34" y1="24" x2="62" y2="42" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#0D1117" /><stop offset="1" stop-color="#30363D" /></linearGradient>' +
  '<radialGradient id="pulse" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(67 22) rotate(135) scale(8 8)"><stop offset="0" stop-color="#58A6FF" /><stop offset="1" stop-color="#1F6FEB" /></radialGradient>' +
  '</defs>' +
  '<rect x="8" y="8" width="80" height="80" rx="20" fill="url(#plate)" stroke="#6E7781" stroke-width="1.5" />' +
  '<rect x="26" y="18" width="44" height="60" rx="11" fill="url(#body)" stroke="#A3ADB8" stroke-opacity="0.45" />' +
  '<rect x="34" y="25" width="28" height="16" rx="4" fill="url(#screen)" />' +
  '<rect x="45" y="11" width="6" height="6" rx="1.5" fill="#AFB8C1" />' +
  '<path d="M48 11V4" stroke="#57606A" stroke-width="2.2" stroke-linecap="round" />' +
  '<circle cx="61.5" cy="22.5" r="6.5" fill="url(#pulse)" />' +
  '<path d="M73 16C76.7 19.5 76.7 25.5 73 29" stroke="#1F6FEB" stroke-opacity="0.75" stroke-width="2" stroke-linecap="round" />' +
  '<path d="M76.5 12.5C82.3 18.2 82.3 27.8 76.5 33.5" stroke="#58A6FF" stroke-opacity="0.5" stroke-width="1.8" stroke-linecap="round" />' +
  '<circle cx="39" cy="48.5" r="3.2" fill="#C9D1D9" />' +
  '<circle cx="57" cy="48.5" r="3.2" fill="#C9D1D9" />' +
  '<g stroke="#C9D1D9" stroke-width="2" stroke-linecap="round"><path d="M35 58H61" /><path d="M35 63H61" /><path d="M35 68H61" /></g>' +
  '</svg>';
const ROGER_MARK_DATA_URI = `data:image/svg+xml;utf8,${encodeURIComponent(ROGER_MARK_SVG)}`;

function createBrandChip(rootDocument) {
  const chip = rootDocument.createElement('span');
  chip.id = BRAND_CHIP_ID;
  chip.className = BRAND_CHIP_CLASS;
  chip.setAttribute('aria-label', 'Roger identity');
  const icon = rootDocument.createElement('img');
  icon.className = 'roger-panel-brandicon';
  icon.src = ROGER_MARK_DATA_URI;
  icon.alt = '';
  icon.setAttribute('aria-hidden', 'true');
  chip.appendChild(icon);
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
  applyActionModel(panel, lastAttentionState, lastSessionCount);

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
  applyActionModel(panel, lastAttentionState, lastSessionCount);

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
// null means "unknown inventory" (launch-only/degraded), which keeps the
// legacy both-buttons surface; 0 means durable truth that no session exists.
let lastSessionCount = null;
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
    lastSessionCount = null;
    // On non-detail routes (including PR-list pages) the listing controls own
    // their own lifecycle, separate from the detail panel.
    refreshPrListingControls(rootDocument);
    return;
  }

  ensurePanel(context, rootDocument);

  const nextKey = contextKey(context);
  if (lastContextKey !== nextKey) {
    lastContextKey = nextKey;
    lastAttentionState = null;
    lastSessionCount = null;
    const panel = rootDocument.getElementById(PANEL_ID);
    if (panel) {
      applyActionModel(panel, lastAttentionState, lastSessionCount);
    }
    clearStatus();
    setInfoMessage(DEFAULT_INFO_MESSAGE);
    requestStatusMirror(context);
  }

  // A PR-detail page is never a PR-list route, so this clears any stale listing
  // controls without touching the detail panel.
  refreshPrListingControls(rootDocument);
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
  registerLaunchProgressListener(document);
}

// Map a host launch-progress stage to the persistent one-liner Roger renders
// while a launch is in flight. Unknown stages return null (rendered as nothing)
// so a future stage never surfaces a raw enum token to the user.
function describeLaunchProgress(stage) {
  if (stage === 'host_started') {
    return 'Roger host connected — running preflight…';
  }
  if (stage === 'preflight_ok') {
    return 'Launching review…';
  }
  return null;
}

// Render a host launch-progress frame into the persistent status areas of
// whichever surface the progressing launch belongs to. The PR-detail panel is
// updated only when the frame's target matches the visible PR; a matching
// listing row (if present) gets its own row-local status. Best-effort and
// side-effect scoped: it never resolves any launch promise or clears a result.
function handleLaunchProgressMessage(message, rootDocument) {
  const doc =
    rootDocument || (typeof document !== 'undefined' ? document : null);
  if (!message || message.type !== 'roger_bridge_launch_progress' || !doc) {
    return false;
  }

  const text = describeLaunchProgress(message.stage);
  if (!text) {
    return false;
  }

  const intent = message.intent && typeof message.intent === 'object' ? message.intent : {};
  let rendered = false;

  // PR-detail panel: only when the progressing target is the visible PR.
  const context = parsePullRequestContext();
  if (
    context &&
    context.owner === intent.owner &&
    context.repo === intent.repo &&
    Number(context.pr_number) === Number(intent.pr_number)
  ) {
    setStatus(text, false, { revealInline: true });
    setInfoMessage(text);
    rendered = true;
  }

  // Listing row: route to the matching row's status node when one is mounted.
  if (
    typeof intent.owner === 'string' &&
    typeof intent.repo === 'string' &&
    typeof doc.getElementById === 'function'
  ) {
    const controlId = listingControlIdForTarget({
      owner: intent.owner,
      repo: intent.repo,
      pr_number: intent.pr_number,
    });
    const control = doc.getElementById(controlId);
    const statusNode = findListingStatusNode(control);
    if (statusNode) {
      setListingRowStatus(statusNode, text, 'pending');
      rendered = true;
    }
  }

  return rendered;
}

// Locate a listing control's status node, preferring the real-DOM querySelector
// and falling back to a direct child scan (keeps this robust across DOM shims).
function findListingStatusNode(control) {
  if (!control) {
    return null;
  }
  if (typeof control.querySelector === 'function') {
    const viaQuery = control.querySelector(`.${LISTING_STATUS_CLASS}`);
    if (viaQuery) {
      return viaQuery;
    }
  }
  const children = Array.isArray(control.children) ? control.children : [];
  for (const child of children) {
    const className = typeof child.className === 'string' ? child.className : '';
    if (className.split(/\s+/).includes(LISTING_STATUS_CLASS)) {
      return child;
    }
  }
  return null;
}

// Listen for the background worker's launch-progress fan-out (delivered over
// chrome.tabs.sendMessage to this content script). Registered once at bootstrap.
function registerLaunchProgressListener(rootDocument) {
  if (typeof chrome === 'undefined' || !chrome.runtime?.onMessage?.addListener) {
    return;
  }
  chrome.runtime.onMessage.addListener((message) => {
    handleLaunchProgressMessage(message, rootDocument);
    return false;
  });
}

function formatLaunchSuccessStatus(response) {
  const base = appendGuidance(
    response?.message || 'Launch intent dispatched.',
    response?.guidance
  );
  const sessionId =
    typeof response?.session_id === 'string' && response.session_id.trim().length > 0
      ? response.session_id.trim()
      : null;

  if (sessionId && !base.includes(sessionId)) {
    return `${base} (session ${sessionId})`;
  }
  return base;
}

function triggerLaunch(action, context, button) {
  const panel = typeof document !== 'undefined' ? document.getElementById(PANEL_ID) : null;

  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    lastAttentionState = null;
    if (panel) {
      applyActionModel(panel, lastAttentionState, lastSessionCount);
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
          applyActionModel(panel, lastAttentionState, lastSessionCount);
        }
        clearAttentionBadge();
        const disconnectStatus = appendGuidance(
          `Bridge error: ${chrome.runtime.lastError.message}`,
          BRIDGE_DISCONNECT_GUIDANCE
        );
        setInfoMessage(disconnectStatus);
        setStatus(disconnectStatus, true, { revealInline: true });
        return;
      }

      if (!response) {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState, lastSessionCount);
        }
        clearAttentionBadge();
        const noResponseStatus = appendGuidance(
          'No bridge response.',
          BRIDGE_DISCONNECT_GUIDANCE
        );
        setInfoMessage(noResponseStatus);
        setStatus(noResponseStatus, true, { revealInline: true });
        return;
      }

      if (!response.ok) {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState, lastSessionCount);
        }
        clearAttentionBadge();
        setInfoMessage(appendGuidance(response.message, response.guidance));
        setStatus(appendGuidance(response.message, response.guidance), true, { revealInline: true });
        return;
      }

      if (response.mode === 'custom_url_fallback') {
        lastAttentionState = null;
        if (panel) {
          applyActionModel(panel, lastAttentionState, lastSessionCount);
        }
        clearAttentionBadge();
        setInfoMessage('Launched via URL fallback. Open Roger locally for authoritative status.');
        setStatus('Launched via URL fallback. Open Roger locally for authoritative status.', false, { revealInline: true });
        return;
      }

      if (response.mode === 'native_messaging' && response.attention_state) {
        lastAttentionState = response.attention_state;
        if (typeof response.session_id === 'string' && response.session_id.trim().length > 0) {
          // A canonical session id is durable proof at least one session exists.
          lastSessionCount =
            typeof lastSessionCount === 'number' && lastSessionCount >= 1 ? lastSessionCount : 1;
        }
        if (panel) {
          applyActionModel(panel, lastAttentionState, lastSessionCount);
        }
        setAttentionBadge(response.attention_state, response.freshness_label || null);
        const successStatus = formatLaunchSuccessStatus(response);
        setInfoMessage(successStatus);
        setStatus(successStatus, false, { revealInline: true });
        return;
      }

      lastAttentionState = null;
      if (panel) {
        applyActionModel(panel, lastAttentionState, lastSessionCount);
      }
      const successStatus = formatLaunchSuccessStatus(response);
      setInfoMessage(successStatus);
      setStatus(successStatus, false, { revealInline: true });
      // Refresh the bounded badge/action model; this must not clear the
      // launch result status line we just rendered.
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
    BRIDGE_DISCONNECT_GUIDANCE,
    STATUS_MIRROR_SETTLE_TIMEOUT_MS,
    formatLaunchSuccessStatus,
    describeLaunchProgress,
    handleLaunchProgressMessage,
    registerLaunchProgressListener,
    requestStatusMirror,
    triggerLaunch,
    GITHUB_ACTION_BUTTON_CLASS,
    INLINE_ANCHOR_SELECTORS,
    MODAL_FALLBACK_STATUS,
    MODAL_OPEN_BUTTON_LABEL,
    RESUME_ACTION_LABEL,
    applyActionModel,
    applyPanelModeStyles,
    createBrandChip,
    createPanel,
    deriveActionModel,
    ensurePanel,
    findInlineAnchor,
    mountInto,
    normalizeSessionCount,
    parsePullRequestContext,
    parsePullRequestListContext,
    isOpenOnlyListingQuery,
    parsePullTargetHref,
    extractPrListingRowTargets,
    PR_LISTING_ROUTE,
    LISTING_START_LABEL,
    LISTING_CONTROL_CLASS,
    LISTING_BUTTON_CLASS,
    LISTING_STATUS_CLASS,
    listingControlIdForTarget,
    createListingRowControl,
    ensurePrListingRowControls,
    removeAllListingControls,
    refreshPrListingControls,
    dispatchListingRowLaunch,
    setListingRowStatus,
    pickInlineAnchorSelector,
    readExtensionBuildLabel,
    readRuntimeAssetUrl,
    refreshPanelForCurrentPage,
    resolvePanelPlacement,
    clearStatus,
    setStatus,
  };
}
