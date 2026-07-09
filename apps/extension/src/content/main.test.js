const test = require('node:test');
const assert = require('node:assert/strict');

const {
  appendGuidance,
  BRAND_CHIP_CLASS,
  BRIDGE_DISCONNECT_GUIDANCE,
  STATUS_MIRROR_SETTLE_TIMEOUT_MS,
  LAUNCH_CLIENT_SETTLE_TIMEOUT_MS,
  LAUNCH_CLIENT_TIMEOUT_GUIDANCE,
  formatLaunchSuccessStatus,
  describeLaunchProgress,
  handleLaunchProgressMessage,
  LISTING_STATUS_CLASS,
  requestStatusMirror,
  triggerLaunch,
  GITHUB_ACTION_BUTTON_CLASS,
  MODAL_FALLBACK_STATUS,
  MODAL_OPEN_BUTTON_LABEL,
  RESUME_ACTION_LABEL,
  applyActionModel,
  applyPanelModeStyles,
  clearStatus,
  createBrandChip,
  createPanel,
  deriveActionModel,
  mountInto,
  parsePullRequestContext,
  parsePullRequestListContext,
  isOpenOnlyListingQuery,
  parsePullTargetHref,
  extractPrListingRowTargets,
  PR_LISTING_ROUTE,
  LISTING_START_LABEL,
  LISTING_CONTROL_CLASS,
  LISTING_BUTTON_CLASS,
  listingControlIdForTarget,
  createListingRowControl,
  ensurePrListingRowControls,
  removeAllListingControls,
  refreshPrListingControls,
  pickInlineAnchorSelector,
  resolvePanelPlacement,
  setStatus,
  handleShowFindings,
  normalizeFindingsPayload,
  renderFindingsStaging,
  groupFindingsByTriage,
  createFindingItemNode,
  normalizeFindingsItem,
  FINDINGS_BODY_ID,
  FINDINGS_READONLY_CAPTION,
  FINDINGS_NOT_RELAYED_MESSAGE,
  FINDINGS_EMPTY_MESSAGE,
  handleTriageFinding,
  handleRequestClarification,
  handleReviseDraft,
  normalizeDraftsPayload,
  renderDraftsStaging,
  handleShowDrafts,
  createDraftsSection,
  DRAFTS_BODY_ID,
  renderSearchResults,
  handleSearch,
  createSearchSection,
  SEARCH_BODY_ID,
  renderTimeline,
  handleTimeline,
  createTimelineSection,
  TIMELINE_BODY_ID,
} = require('./main.js');

function createParent() {
  return {
    children: [],
    appendChild(node) {
      if (node.parentElement) {
        const oldIndex = node.parentElement.children.indexOf(node);
        if (oldIndex >= 0) {
          node.parentElement.children.splice(oldIndex, 1);
        }
      }
      this.children.push(node);
      node.parentElement = this;
      return node;
    },
    prepend(node) {
      if (node.parentElement) {
        const oldIndex = node.parentElement.children.indexOf(node);
        if (oldIndex >= 0) {
          node.parentElement.children.splice(oldIndex, 1);
        }
      }
      this.children.unshift(node);
      node.parentElement = this;
      return node;
    },
  };
}

function createTestElement(tagName) {
  const attributes = new Map();
  return {
    tagName: String(tagName).toUpperCase(),
    id: '',
    className: '',
    textContent: '',
    hidden: false,
    children: [],
    parentElement: null,
    dataset: {},
    style: {},
    querySelectorAll(selector) {
      if (selector === 'button[data-action]') {
        return findNodes(
          this,
          (node) => node.dataset && typeof node.dataset.action === 'string',
          []
        );
      }
      return [];
    },
    appendChild(node) {
      if (!node) {
        return null;
      }
      this.children.push(node);
      node.parentElement = this;
      return node;
    },
    addEventListener() {},
    setAttribute(name, value) {
      attributes.set(name, String(value));
    },
    getAttribute(name) {
      return attributes.has(name) ? attributes.get(name) : null;
    },
  };
}

function findNodes(root, predicate, output = []) {
  if (!root || typeof predicate !== 'function') {
    return output;
  }
  if (predicate(root)) {
    output.push(root);
  }
  if (Array.isArray(root.children)) {
    for (const child of root.children) {
      findNodes(child, predicate, output);
    }
  }
  return output;
}

test('pickInlineAnchorSelector prefers the first configured GitHub seam', () => {
  const selector = pickInlineAnchorSelector((candidate) =>
    candidate === '[class*="prc-PageHeader-Actions-"]' || candidate === '.gh-header-actions'
      ? {}
      : null
  );

  assert.equal(selector, '[class*="prc-PageHeader-Actions-"]');
});

test('resolvePanelPlacement falls back to modal mode when no bounded seam exists', () => {
  const fakeBody = {};
  const fakeDocument = {
    body: fakeBody,
    querySelector() {
      return null;
    },
  };

  const placement = resolvePanelPlacement(fakeDocument);
  assert.equal(placement.mode, 'modal');
  assert.equal(placement.mountNode, fakeBody);
});

test('resolvePanelPlacement prefers rail mode when both header and rail seams exist', () => {
  const inlineAnchor = {};
  const railAnchor = {
    querySelector() {
      return null;
    },
  };
  const fakeDocument = {
    body: {},
    querySelector(selector) {
      if (selector === '[class*="prc-PageHeader-Actions-"]') {
        return inlineAnchor;
      }
      if (selector === '[class*="Layout-sidebar"]') {
        return railAnchor;
      }
      return null;
    },
  };

  const placement = resolvePanelPlacement(fakeDocument);
  assert.equal(placement.mode, 'rail');
  assert.equal(placement.mountNode, railAnchor);
});

test('resolvePanelPlacement selects rail mode above reviewers when header seam is unavailable', () => {
  const reviewersCard = { id: 'reviewers' };
  const railAnchor = {
    querySelector(selector) {
      if (selector === '[aria-label="Reviewers"]') {
        return reviewersCard;
      }
      return null;
    },
  };

  const fakeDocument = {
    body: {},
    querySelector(selector) {
      if (selector === '[class*="Layout-sidebar"]') {
        return railAnchor;
      }
      return null;
    },
  };

  const placement = resolvePanelPlacement(fakeDocument);
  assert.equal(placement.mode, 'rail');
  assert.equal(placement.mountNode, railAnchor);
  assert.equal(placement.beforeNode, reviewersCard);
});

test('mountInto keeps placement idempotent and supports reinjection to a new parent', () => {
  const firstParent = createParent();
  const secondParent = createParent();
  const panel = { parentElement: null };

  assert.equal(mountInto(firstParent, panel), true);
  assert.equal(panel.parentElement, firstParent);
  assert.equal(firstParent.children.length, 1);

  assert.equal(mountInto(firstParent, panel), false);
  assert.equal(firstParent.children.length, 1);

  assert.equal(mountInto(secondParent, panel, { prepend: true }), true);
  assert.equal(panel.parentElement, secondParent);
  assert.equal(firstParent.children.length, 0);
  assert.equal(secondParent.children.length, 1);
});

test('appendGuidance keeps stale-state repair guidance visible on extension surfaces', () => {
  assert.equal(
    appendGuidance(
      'rr resume completed for octo/roger-reviewer#42',
      'Run `rr resume --session session-42` locally to reconcile stale state.'
    ),
    'rr resume completed for octo/roger-reviewer#42. Run `rr resume --session session-42` locally to reconcile stale state.'
  );
  assert.equal(appendGuidance('Launch intent dispatched.', ''), 'Launch intent dispatched.');
});

test('applyPanelModeStyles flips between inline, rail, and modal class contracts', () => {
  const classes = new Set();
  const panel = {
    classList: {
      toggle(className, enabled) {
        if (enabled) {
          classes.add(className);
        } else {
          classes.delete(className);
        }
      },
    },
  };

  applyPanelModeStyles(panel, 'inline');
  assert.equal(classes.has('roger-panel--inline'), true);
  assert.equal(classes.has('roger-panel--rail'), false);
  assert.equal(classes.has('roger-panel--modal'), false);
  assert.equal(classes.has('roger-panel--floating'), false);

  applyPanelModeStyles(panel, 'rail');
  assert.equal(classes.has('roger-panel--inline'), false);
  assert.equal(classes.has('roger-panel--rail'), true);
  assert.equal(classes.has('roger-panel--modal'), false);
  assert.equal(classes.has('roger-panel--floating'), false);

  applyPanelModeStyles(panel, 'modal');
  assert.equal(classes.has('roger-panel--inline'), false);
  assert.equal(classes.has('roger-panel--rail'), false);
  assert.equal(classes.has('roger-panel--modal'), true);
  assert.equal(classes.has('roger-panel--floating'), false);
});

test('GitHub header action host uses native GitHub/Primer button classes', () => {
  assert.match(GITHUB_ACTION_BUTTON_CLASS, /\bbtn\b/);
  assert.match(GITHUB_ACTION_BUTTON_CLASS, /\bbtn-sm\b/);
  assert.match(GITHUB_ACTION_BUTTON_CLASS, /\bButton\b/);
  assert.match(GITHUB_ACTION_BUTTON_CLASS, /\bButton--small\b/);
});

test('deriveActionModel keeps the default action set limited to launch, resume, and findings', () => {
  const model = deriveActionModel(null);
  assert.equal(model.primaryActionId, 'start_review');
  assert.equal(model.visibleActions.has('start_review'), true);
  assert.equal(model.visibleActions.has('resume_review'), true);
  assert.equal(model.visibleActions.has('show_findings'), false);
  assert.equal(model.visibleActions.has('refresh_review'), false);
});

test('deriveActionModel promotes Resume when refresh is recommended', () => {
  const model = deriveActionModel('refresh_recommended');
  assert.equal(model.primaryActionId, 'resume_review');
  assert.equal(model.visibleActions.has('show_findings'), false);
  assert.equal(model.visibleActions.has('refresh_review'), false);
  assert.equal(model.visibleActions.has('resume_review'), true);
});

test('deriveActionModel maps canonical attention states to expected primary actions', () => {
  const scenarios = [
    ['awaiting_user_input', 'resume_review', false],
    ['review_failed', 'resume_review', false],
    ['findings_ready', 'show_findings', true],
    ['awaiting_outbound_approval', 'resume_review', false],
    ['refresh_recommended', 'resume_review', false],
  ];

  for (const [attentionState, expectedPrimary, findingsVisible] of scenarios) {
    const model = deriveActionModel(attentionState);
    assert.equal(model.primaryActionId, expectedPrimary);
    assert.equal(model.visibleActions.has('show_findings'), findingsVisible);
    assert.equal(model.visibleActions.has('start_review'), true);
    assert.equal(model.visibleActions.has('resume_review'), true);
  }
});

test('deriveActionModel hides resume entirely when no local session exists', () => {
  const model = deriveActionModel(null, 0);
  assert.equal(model.primaryActionId, 'start_review');
  assert.equal(model.visibleActions.has('start_review'), true);
  assert.equal(model.visibleActions.has('resume_review'), false);
  assert.equal(model.visibleActions.has('show_findings'), false);
  assert.equal(model.sessionCount, 0);
});

test('deriveActionModel promotes resume to primary when one session exists', () => {
  const model = deriveActionModel(null, 1);
  assert.equal(model.primaryActionId, 'resume_review');
  assert.equal(model.visibleActions.has('start_review'), true);
  assert.equal(model.visibleActions.has('resume_review'), true);
  assert.equal(model.visibleActions.has('show_findings'), false);
  assert.equal(model.resumeLabel, RESUME_ACTION_LABEL);
});

test('deriveActionModel labels resume with the count when multiple sessions exist', () => {
  const model = deriveActionModel(null, 3);
  assert.equal(model.primaryActionId, 'resume_review');
  assert.equal(model.visibleActions.has('resume_review'), true);
  assert.equal(model.resumeLabel, `${RESUME_ACTION_LABEL} (3)`);
});

test('deriveActionModel keeps legacy both-buttons surface when session count is unknown', () => {
  for (const unknownCount of [null, undefined, -1, Number.NaN, 'two']) {
    const model = deriveActionModel(null, unknownCount);
    assert.equal(model.primaryActionId, 'start_review');
    assert.equal(model.visibleActions.has('start_review'), true);
    assert.equal(model.visibleActions.has('resume_review'), true);
    assert.equal(model.resumeLabel, RESUME_ACTION_LABEL);
    assert.equal(model.sessionCount, null);
  }
});

test('deriveActionModel still promotes fresh findings over session-derived resume primary', () => {
  const model = deriveActionModel('findings_ready', 2);
  assert.equal(model.primaryActionId, 'show_findings');
  assert.equal(model.visibleActions.has('show_findings'), true);
  assert.equal(model.visibleActions.has('resume_review'), true);
  assert.equal(model.resumeLabel, `${RESUME_ACTION_LABEL} (2)`);
});

test('deriveActionModel ignores resume-primary attention claims when zero sessions exist', () => {
  // Defensive truthfulness: an attention claim without any durable session
  // must not resurrect a resume button that has nothing to resume.
  const model = deriveActionModel('refresh_recommended', 0);
  assert.equal(model.visibleActions.has('resume_review'), false);
  assert.equal(model.primaryActionId, 'start_review');
});

test('applyActionModel toggles visibility and primary emphasis on action buttons', () => {
  const buttonStates = new Map();
  const makeButton = (actionId) => {
    const classes = new Set();
    const button = {
      dataset: { action: actionId },
      hidden: false,
      classList: {
        toggle(className, enabled) {
          if (enabled) {
            classes.add(className);
          } else {
            classes.delete(className);
          }
        },
      },
      setAttribute(name, value) {
        buttonStates.set(`${actionId}:${name}`, String(value));
      },
      hasClass(name) {
        return classes.has(name);
      },
    };
    return button;
  };

  const buttons = [
    makeButton('start_review'),
    makeButton('resume_review'),
    makeButton('show_findings'),
  ];
  const panel = {
    querySelectorAll(selector) {
      return selector === 'button[data-action]' ? buttons : [];
    },
  };

  applyActionModel(panel, null);
  assert.equal(buttons[0].hidden, false);
  assert.equal(buttons[0].hasClass('roger-panel-button--primary'), true);
  assert.equal(buttons[1].hidden, false);
  assert.equal(buttons[1].hasClass('roger-panel-button--secondary'), true);
  assert.equal(buttons[2].hidden, true);

  applyActionModel(panel, 'refresh_recommended');
  assert.equal(buttons[1].hidden, false);
  assert.equal(buttons[1].hasClass('roger-panel-button--primary'), true);
  assert.equal(buttonStates.get('resume_review:aria-hidden'), 'false');
  assert.equal(buttons[2].hidden, true);

  applyActionModel(panel, 'findings_ready');
  assert.equal(buttons[2].hidden, false);
  assert.equal(buttons[2].hasClass('roger-panel-button--primary'), true);
});

test('createBrandChip renders shared rr-brand-chip primitive', () => {
  const fakeDocument = {
    createElement(tagName) {
      return createTestElement(tagName);
    },
  };

  const chip = createBrandChip(fakeDocument);
  assert.equal(chip.tagName, 'SPAN');
  assert.equal(chip.className, BRAND_CHIP_CLASS);
  assert.match(chip.className, /\brr-brand-chip\b/);
  assert.equal(chip.getAttribute('aria-label'), 'Roger identity');
  assert.equal(chip.children.length, 2);
  const icon = chip.children[0];
  assert.equal(icon.tagName, 'IMG');
  assert.match(icon.className, /\broger-panel-brandicon\b/);
  // The mark must be an inline data: URI — GitHub's img-src CSP blocks
  // chrome-extension:// images injected into the page (regression: broken mark).
  assert.ok(
    icon.src.startsWith('data:image/svg+xml'),
    'brand mark must render from an inline data: URI'
  );
  assert.ok(
    !icon.src.includes('chrome-extension'),
    'brand mark must not depend on a chrome-extension:// resource URL'
  );
  assert.equal(chip.children[1].textContent, 'Roger');
});

test('createPanel keeps GitHub button semantics while rendering Roger identity chip', () => {
  const fakeHead = createTestElement('head');
  const fakeDocument = {
    head: fakeHead,
    documentElement: createTestElement('html'),
    body: createTestElement('body'),
    createElement(tagName) {
      return createTestElement(tagName);
    },
    getElementById() {
      return null;
    },
  };

  const panel = createPanel(
    {
      owner: 'octo',
      repo: 'roger-reviewer',
      pr_number: 42,
    },
    fakeDocument
  );

  const identityChips = findNodes(
    panel,
    (node) => typeof node.className === 'string' && /\brr-brand-chip\b/.test(node.className)
  );
  assert.equal(identityChips.length, 1);

  const actionButtons = findNodes(
    panel,
    (node) => node.dataset && typeof node.dataset.action === 'string'
  );
  assert.equal(actionButtons.length, 3);
  const actionIds = actionButtons.map((button) => button.dataset.action).sort();
  assert.deepEqual(actionIds, ['resume_review', 'show_findings', 'start_review']);
  assert.equal(actionIds.includes('approve_outbound'), false);
  assert.equal(actionIds.includes('post_review'), false);
  for (const button of actionButtons) {
    assert.equal(button.className, GITHUB_ACTION_BUTTON_CLASS);
  }
  const findingsButton = actionButtons.find((button) => button.dataset.action === 'show_findings');
  assert.equal(findingsButton.hidden, true);
  const infoSummaries = findNodes(
    panel,
    (node) => typeof node.className === 'string' && node.className === 'roger-panel-info-toggle'
  );
  assert.equal(infoSummaries.length, 1);
  assert.match(infoSummaries[0].getAttribute('title'), /Extension build unavailable/i);
  const infoPanels = findNodes(
    panel,
    (node) => typeof node.className === 'string' && node.className === 'roger-panel-info-panel'
  );
  assert.equal(infoPanels.length, 1);
  assert.equal(infoPanels[0].children.length, 1);
  assert.match(infoPanels[0].children[0].textContent, /Launch Roger locally/i);
});

function renderPanelStyleSheet() {
  const fakeHead = createTestElement('head');
  const fakeDocument = {
    head: fakeHead,
    documentElement: createTestElement('html'),
    body: createTestElement('body'),
    createElement(tagName) {
      return createTestElement(tagName);
    },
    getElementById() {
      return null;
    },
  };

  createPanel({ owner: 'octo', repo: 'roger-reviewer', pr_number: 42 }, fakeDocument);

  const styleNode = findNodes(
    fakeHead,
    (node) => node.id === 'roger-reviewer-panel-style'
  )[0];
  assert.ok(styleNode, 'panel style node should be injected into the document head');
  return styleNode.textContent;
}

test('injected panel maps the dark host theme across every GitHub theme signal', () => {
  const css = renderPanelStyleSheet();

  // Explicit dark mode regardless of where GitHub puts the signal: the dark
  // surface remap must fire for :root[data-color-mode="dark"] (html), for any
  // ancestor carrying data-color-mode="dark" (body/wrapper), and for the panel
  // node itself carrying it. The previous mapping only matched the html-root
  // form, so a body-level signal (as real GitHub and the harness emit) left the
  // panel stuck on its washed-out light treatment — provable host-theme drift.
  assert.match(css, /:root\[data-color-mode="dark"\]\s+#roger-reviewer-panel/);
  assert.match(css, /\[data-color-mode="dark"\]\s+#roger-reviewer-panel/);
  assert.match(css, /#roger-reviewer-panel\[data-color-mode="dark"\]/);

  // GitHub "auto" theme resolves through prefers-color-scheme; the panel must
  // honour it instead of defaulting to the light surface in dark OS contexts.
  assert.match(css, /@media \(prefers-color-scheme: dark\)/);
  assert.match(css, /\[data-color-mode="auto"\]\s+#roger-reviewer-panel/);
  assert.match(css, /:root:not\(\[data-color-mode="light"\]\)\s+#roger-reviewer-panel/);
});

test('injected panel dark remap binds Roger surfaces to dark Primer variables', () => {
  const css = renderPanelStyleSheet();

  // The dark remap must resolve from the host's dark Primer tokens (so it feels
  // native), not from a bluffed light surface. Assert the Roger-owned mapping
  // points at the canonical Primer dark sources.
  assert.match(css, /--rr-panel-surface: var\(--overlay-bgColor, var\(--bgColor-default, #161b22\)\)/);
  assert.match(css, /--rr-panel-text: var\(--fgColor-default, #e6edf3\)/);
  assert.match(css, /--rr-panel-muted: var\(--fgColor-muted, #8b949e\)/);
  assert.match(css, /--rr-panel-border-strong: color-mix\(\s*in srgb,\s*var\(--borderColor-emphasis/);
});

test('injected panel uses a theme-aware metallic tint instead of literal white sheen', () => {
  const css = renderPanelStyleSheet();

  // The metallic Roger treatment must not blow surfaces toward pure white in
  // dark mode; the sheen is a token whose value flips per theme.
  assert.match(css, /--rr-panel-metal-tint: #ffffff/);
  assert.match(css, /--rr-panel-metal-tint: var\(--fgColor-default, #f0f6fc\)/);
  assert.match(css, /--rr-panel-metal-tint-strength: 7%/);

  // Button + chip + surface gradients consume the tint token, and no
  // surface/button sheen mix hardcodes `white` anymore (the accent primary
  // button keeps its branded blue gradient, which is intentional).
  assert.match(css, /var\(--rr-panel-metal-tint\) var\(--rr-panel-metal-tint-strength\)/);
  const surfaceWhiteMixes =
    css.match(/var\(--rr-panel-surface[^)]*\)\s+\d+%,\s*white/g) || [];
  assert.equal(
    surfaceWhiteMixes.length,
    0,
    'surface/button gradients must route their metallic sheen through the theme-aware tint token'
  );
});

test('injected panel title and metallic buttons stay legible in both themes', () => {
  const css = renderPanelStyleSheet();

  // The heading derives from --rr-panel-text, which is remapped to the host's
  // dark foreground in dark mode (above) and the light foreground at the base.
  assert.match(css, /\.roger-panel-heading\s*\{[^}]*color: var\(--rr-panel-text\)/);

  // Button hierarchy: a single full-width primary action with a branded blue
  // treatment, a metallic secondary that reads against the surface, and a
  // demoted tertiary for findings.
  assert.match(css, /\.roger-panel-button--primary\s*\{[\s\S]*?flex-basis: 100%/);
  assert.match(css, /\.roger-panel-button--primary\s*\{[\s\S]*?color: #ffffff/);
  assert.match(css, /\.roger-panel-button--secondary\s*\{[^}]*color: var\(--rr-panel-text\)/);
  assert.match(css, /\.roger-panel-button--tertiary\s*\{[^}]*color: var\(--rr-panel-muted\)/);
});

test('modal fallback copy keeps the in-page modal primary and popup manual-only', () => {
  assert.match(MODAL_OPEN_BUTTON_LABEL, /fallback/i);
  assert.match(MODAL_FALLBACK_STATUS, /modal fallback/i);
  assert.match(MODAL_FALLBACK_STATUS, /manual backup/i);
});

test('parsePullRequestContext extracts owner/repo/pr from PR URL path', () => {
  const originalWindow = global.window;
  global.window = {
    location: {
      pathname: '/octo-org/roger-reviewer/pull/42',
    },
  };

  try {
    assert.deepEqual(parsePullRequestContext(), {
      owner: 'octo-org',
      repo: 'roger-reviewer',
      pr_number: 42,
    });
  } finally {
    global.window = originalWindow;
  }
});

function createStatusClassList() {
  const classes = new Set();
  return {
    classes,
    add(...names) {
      for (const name of names) {
        classes.add(name);
      }
    },
    remove(...names) {
      for (const name of names) {
        classes.delete(name);
      }
    },
    contains(name) {
      return classes.has(name);
    },
  };
}

function makeActionButton(actionId, label) {
  const classes = new Set();
  return {
    dataset: { action: actionId },
    hidden: false,
    disabled: false,
    textContent: label,
    classList: {
      toggle(className, enabled) {
        if (enabled) {
          classes.add(className);
        } else {
          classes.delete(className);
        }
      },
      contains(name) {
        return classes.has(name);
      },
    },
    setAttribute() {},
  };
}

function makePanelDom({ inline = false } = {}) {
  const actionButtons = [
    makeActionButton('start_review', 'Start Review in Roger'),
    makeActionButton('resume_review', 'Resume Existing Review'),
    makeActionButton('show_findings', 'View Findings'),
  ];
  const panel = {
    classList: {
      contains(name) {
        return inline && name === 'roger-panel--inline';
      },
    },
    querySelectorAll(selector) {
      return selector === 'button[data-action]' ? actionButtons : [];
    },
  };
  const statusNode = {
    textContent: '',
    hidden: true,
    parentElement: panel,
    classList: createStatusClassList(),
  };
  const badge = { textContent: '', style: {} };
  const infoNode = { textContent: '' };
  const documentStub = {
    getElementById(id) {
      switch (id) {
        case 'roger-reviewer-panel':
          return panel;
        case 'roger-reviewer-status':
          return statusNode;
        case 'roger-reviewer-attention-badge':
          return badge;
        case 'roger-reviewer-info-text':
          return infoNode;
        default:
          return null;
      }
    },
  };

  return { actionButtons, badge, documentStub, infoNode, panel, statusNode };
}

function findActionButton(dom, actionId) {
  return dom.actionButtons.find((button) => button.dataset.action === actionId);
}

function makeChromeStub({ launchResponse, statusResponse, launchLastError = null } = {}) {
  const stub = {
    runtime: {
      lastError: null,
      sendMessage(payload, callback) {
        if (payload?.type === 'roger_bridge_launch') {
          if (launchLastError) {
            stub.runtime.lastError = { message: launchLastError };
            callback(undefined);
            stub.runtime.lastError = null;
            return;
          }
          callback(launchResponse);
          return;
        }
        if (payload?.type === 'roger_bridge_status') {
          callback(statusResponse);
        }
      },
    },
  };
  return stub;
}

function withPanelGlobals({ documentStub, chromeStub }, fn) {
  const previousDocument = global.document;
  const previousChrome = global.chrome;
  global.document = documentStub;
  global.chrome = chromeStub;
  try {
    return fn();
  } finally {
    global.document = previousDocument;
    global.chrome = previousChrome;
  }
}

const TEST_CONTEXT = { owner: 'acme', repo: 'widgets', pr_number: 42 };
const LAUNCH_ONLY_STATUS_RESPONSE = {
  ok: true,
  mode: 'launch_only',
  message:
    'Launch-only bridge mode. This browser surface can start Roger actions, but it does not own live local session state.',
  guidance: 'Open Roger locally (`rr status` or `rr findings`) for authoritative session state.',
};

test('launch failure renders a persistent error status with bridge message and guidance', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    launchResponse: {
      ok: false,
      mode: 'bridge_preflight_failed',
      action: 'start_review',
      message: 'Roger bridge preflight failed.',
      guidance: 'Roger data directory not found. Run `rr init` to set up.',
      failure_kind: 'preflight_failed',
    },
    statusResponse: LAUNCH_ONLY_STATUS_RESPONSE,
  });
  const button = { disabled: false, textContent: 'Start Review in Roger' };

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    triggerLaunch('start_review', TEST_CONTEXT, button);

    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /Roger bridge preflight failed\./);
    assert.match(dom.statusNode.textContent, /Run `rr init` to set up\./);
    assert.equal(dom.statusNode.classList.contains('roger-panel-status--error'), true);
    assert.equal(button.disabled, false);

    // A subsequent unrelated bounded-status re-render must not clear the error.
    requestStatusMirror(TEST_CONTEXT);
    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /Roger bridge preflight failed\./);
    assert.match(dom.statusNode.textContent, /Run `rr init` to set up\./);
  });
});

test('launch failure status keeps guidance lists readable', () => {
  assert.equal(
    appendGuidance('Launch blocked.', [
      'Run `rr extension setup --browser chrome`.',
      'Then run `rr extension doctor --browser chrome`.',
    ]),
    'Launch blocked. Run `rr extension setup --browser chrome`. Then run `rr extension doctor --browser chrome`.'
  );
});

test('bridge disconnect renders a persistent error status with setup guidance', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    launchLastError: 'Native host has exited.',
    statusResponse: LAUNCH_ONLY_STATUS_RESPONSE,
  });
  const button = { disabled: false, textContent: 'Start Review in Roger' };

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    triggerLaunch('start_review', TEST_CONTEXT, button);

    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /Bridge error: Native host has exited\./);
    assert.match(dom.statusNode.textContent, /rr extension setup/);
    assert.match(dom.statusNode.textContent, /rr extension doctor/);
    assert.equal(dom.statusNode.classList.contains('roger-panel-status--error'), true);

    requestStatusMirror(TEST_CONTEXT);
    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /rr extension setup/);
  });
});

test('missing bridge response renders a persistent error status with setup guidance', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    launchResponse: undefined,
    statusResponse: LAUNCH_ONLY_STATUS_RESPONSE,
  });
  const button = { disabled: false, textContent: 'Start Review in Roger' };

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    triggerLaunch('start_review', TEST_CONTEXT, button);

    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /No bridge response\./);
    assert.match(dom.statusNode.textContent, /rr extension setup/);
    assert.match(dom.statusNode.textContent, /rr extension doctor/);
    assert.equal(dom.statusNode.classList.contains('roger-panel-status--error'), true);
  });
});

test('launch fast-fails closed when sendMessage throws synchronously (extension context invalidated)', () => {
  const dom = makePanelDom();
  // sendMessage throwing synchronously means the response callback is never
  // registered; without the try/catch the panel would hang forever on
  // "Dispatching launch intent..." with the button disabled.
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage() {
        throw new Error('Extension context invalidated.');
      },
    },
  };
  const button = { disabled: true, textContent: 'Start Review in Roger' };

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    triggerLaunch('start_review', TEST_CONTEXT, button);

    // Panel must fast-fail closed with honest guidance and re-enable the button.
    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /Bridge dispatch failed: Extension context invalidated\./);
    assert.match(dom.statusNode.textContent, /rr extension setup/);
    assert.equal(dom.statusNode.classList.contains('roger-panel-status--error'), true);
    assert.equal(button.disabled, false);
    assert.equal(button.textContent, 'Start Review in Roger');
  });
});

test('launch fast-fails closed via the client watchdog when the response callback never fires', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const dom = makePanelDom();
  // Model an Edge MV3 worker teardown that silently drops the launch reply: the
  // callback is never invoked, so only the client-side watchdog can recover.
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage() {
        // Intentionally never calls the callback.
      },
    },
  };
  const button = { disabled: true, textContent: 'Start Review in Roger' };

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    triggerLaunch('start_review', TEST_CONTEXT, button);

    // Before the watchdog fires the panel is still dispatching (button held).
    assert.equal(button.disabled, true);
    assert.match(dom.statusNode.textContent, /Dispatching launch intent/);

    // Advance just past the client settle bound; the watchdog must fast-fail.
    t.mock.timers.tick(LAUNCH_CLIENT_SETTLE_TIMEOUT_MS + 1);

    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /Native Messaging unavailable; launch blocked\./);
    assert.match(dom.statusNode.textContent, /rr setup doctor --live/);
    assert.equal(dom.statusNode.classList.contains('roger-panel-status--error'), true);
    assert.equal(button.disabled, false);
    assert.equal(button.textContent, 'Start Review in Roger');
  });
});

test('client watchdog stays above the worker first-frame watchdog and points at setup doctor --live', () => {
  // The client watchdog must never pre-empt a live worker's own honest degrade
  // (LAUNCH_FIRST_FRAME_TIMEOUT_MS ~10000ms in background/main.js).
  assert.ok(LAUNCH_CLIENT_SETTLE_TIMEOUT_MS > 10000);
  assert.match(LAUNCH_CLIENT_TIMEOUT_GUIDANCE, /rr setup doctor --live/);
});

test('launch success persists a truthful status line that the status mirror does not clear', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    launchResponse: {
      ok: true,
      mode: 'native_messaging',
      action: 'start_review',
      message: 'rr review completed for acme/widgets#42.',
      session_id: 'session-42',
    },
    statusResponse: LAUNCH_ONLY_STATUS_RESPONSE,
  });
  const button = { disabled: false, textContent: 'Start Review in Roger' };

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    // triggerLaunch internally chains requestStatusMirror after the generic
    // success path; the launch-only mirror envelope must not wipe the line.
    triggerLaunch('start_review', TEST_CONTEXT, button);

    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /rr review completed for acme\/widgets#42\./);
    assert.match(dom.statusNode.textContent, /session-42/);
    assert.equal(dom.statusNode.classList.contains('roger-panel-status--ok'), true);

    // A later unrelated bounded-status re-render must also leave it intact.
    requestStatusMirror(TEST_CONTEXT);
    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /rr review completed for acme\/widgets#42\./);
    assert.match(dom.statusNode.textContent, /session-42/);
  });
});

test('inline launch success status does not auto-clear on a timer', () => {
  const dom = makePanelDom({ inline: true });
  const chromeStub = makeChromeStub({
    launchResponse: {
      ok: true,
      mode: 'native_messaging',
      action: 'start_review',
      message: 'rr review completed for acme/widgets#42.',
      session_id: 'session-42',
    },
    statusResponse: LAUNCH_ONLY_STATUS_RESPONSE,
  });
  const button = { disabled: false, textContent: 'Start Review in Roger' };

  const scheduled = [];
  const previousSetTimeout = global.setTimeout;
  const previousClearTimeout = global.clearTimeout;
  global.setTimeout = (fn, ms) => {
    scheduled.push({ fn, ms });
    return scheduled.length;
  };
  global.clearTimeout = () => {};

  try {
    withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
      triggerLaunch('start_review', TEST_CONTEXT, button);

      // The only timer the success path may arm is the bounded-status mirror's
      // settle fallback, which must never touch the launch status line. Firing
      // every scheduled timer must leave the launch status fully intact.
      for (const entry of scheduled) {
        if (typeof entry.fn === 'function') {
          entry.fn();
        }
      }
    });
  } finally {
    global.setTimeout = previousSetTimeout;
    global.clearTimeout = previousClearTimeout;
  }

  assert.equal(dom.statusNode.hidden, false);
  assert.equal(dom.statusNode.classList.contains('roger-panel-status--inline-visible'), true);
  assert.match(
    dom.statusNode.textContent,
    /rr review completed for acme\/widgets#42\./,
    'launch status must stay visible after any mirror settle timer fires, never auto-clear'
  );
  assert.match(dom.statusNode.textContent, /session-42/);
});

test('formatLaunchSuccessStatus derives a truthful durable success line', () => {
  assert.equal(
    formatLaunchSuccessStatus({
      message: 'rr review completed for acme/widgets#42.',
      session_id: 'session-42',
    }),
    'rr review completed for acme/widgets#42. (session session-42)'
  );
  // Does not duplicate the session id when the bridge message already names it.
  assert.equal(
    formatLaunchSuccessStatus({
      message: 'Resume `rr status --session session-42` locally.',
      session_id: 'session-42',
    }),
    'Resume `rr status --session session-42` locally.'
  );
  assert.equal(formatLaunchSuccessStatus({}), 'Launch intent dispatched.');
});

test('disconnect guidance names rr extension setup and doctor', () => {
  assert.match(BRIDGE_DISCONNECT_GUIDANCE, /rr extension setup/);
  assert.match(BRIDGE_DISCONNECT_GUIDANCE, /rr extension doctor/);
});

test('setStatus toggles status classes for readable ok/error states', () => {
  const classes = new Set();
  const statusNode = {
    textContent: '',
    parentElement: {
      classList: {
        contains(name) {
          return name === 'roger-panel--inline';
        },
      },
    },
    classList: {
      remove(...names) {
        for (const name of names) {
          classes.delete(name);
        }
      },
      add(name) {
        classes.add(name);
      },
    },
  };

  const originalDocument = global.document;
  global.document = {
    getElementById(id) {
      return id === 'roger-reviewer-status' ? statusNode : null;
    },
  };

  try {
    setStatus('Idle status');
    assert.equal(statusNode.textContent, 'Idle status');
    assert.equal(statusNode.hidden, false);
    assert.equal(classes.has('roger-panel-status--ok'), true);
    assert.equal(classes.has('roger-panel-status--error'), false);

    setStatus('Fallback-only status', true);
    assert.equal(statusNode.textContent, 'Fallback-only status');
    assert.equal(classes.has('roger-panel-status--ok'), false);
    assert.equal(classes.has('roger-panel-status--error'), true);

    setStatus('Launch blocked', true, { revealInline: true });
    assert.equal(classes.has('roger-panel-status--inline-visible'), true);

    clearStatus();
    assert.equal(statusNode.hidden, true);
    assert.equal(statusNode.textContent, '');
  } finally {
    global.document = originalDocument;
  }
});

test('status mirror with zero-session inventory hides resume and invites a first review', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    statusResponse: {
      ok: true,
      mode: 'no_local_session',
      session_count: 0,
      message: 'No local Roger review session exists for this pull request yet.',
    },
  });

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    requestStatusMirror(TEST_CONTEXT);

    assert.equal(findActionButton(dom, 'start_review').hidden, false);
    assert.equal(
      findActionButton(dom, 'start_review').classList.contains('roger-panel-button--primary'),
      true
    );
    assert.equal(findActionButton(dom, 'resume_review').hidden, true);
    assert.equal(findActionButton(dom, 'show_findings').hidden, true);
    assert.match(dom.infoNode.textContent, /No Roger review exists for this PR yet — start one\./);
    // The mirror owns badge + info only; the action status line stays untouched.
    assert.equal(dom.statusNode.hidden, true);
    assert.equal(dom.statusNode.textContent, '');
  });
});

test('status mirror with single-session inventory promotes resume as primary', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    statusResponse: {
      ok: true,
      mode: 'session_inventory',
      session_count: 1,
      sessions: [{ session_id: 'session-1', provider: 'claude' }],
    },
  });

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    requestStatusMirror(TEST_CONTEXT);

    const resumeButton = findActionButton(dom, 'resume_review');
    assert.equal(resumeButton.hidden, false);
    assert.equal(resumeButton.classList.contains('roger-panel-button--primary'), true);
    assert.equal(resumeButton.textContent, RESUME_ACTION_LABEL);
    assert.equal(findActionButton(dom, 'start_review').hidden, false);
    assert.equal(findActionButton(dom, 'show_findings').hidden, true);
    assert.match(dom.infoNode.textContent, /1 local Roger review session for this PR\./);
    assert.match(dom.infoNode.textContent, /rr status/);
  });
});

test('status mirror with multi-session inventory labels resume with the count', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    statusResponse: {
      ok: true,
      mode: 'session_inventory',
      session_count: 3,
      sessions: [
        { session_id: 'session-1' },
        { session_id: 'session-2' },
        { session_id: 'session-3' },
      ],
    },
  });

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    requestStatusMirror(TEST_CONTEXT);

    const resumeButton = findActionButton(dom, 'resume_review');
    assert.equal(resumeButton.hidden, false);
    assert.equal(resumeButton.classList.contains('roger-panel-button--primary'), true);
    assert.equal(resumeButton.textContent, `${RESUME_ACTION_LABEL} (3)`);
    assert.match(dom.infoNode.textContent, /3 local Roger review sessions for this PR\./);
  });
});

test('status mirror with launch-only envelope keeps the legacy both-buttons surface', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({ statusResponse: LAUNCH_ONLY_STATUS_RESPONSE });

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    requestStatusMirror(TEST_CONTEXT);

    const resumeButton = findActionButton(dom, 'resume_review');
    assert.equal(resumeButton.hidden, false);
    assert.equal(resumeButton.textContent, RESUME_ACTION_LABEL);
    assert.equal(
      findActionButton(dom, 'start_review').classList.contains('roger-panel-button--primary'),
      true
    );
    assert.equal(resumeButton.classList.contains('roger-panel-button--primary'), false);
  });
});

test('bounded findings_ready status with inventory still promotes show_findings', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    statusResponse: {
      ok: true,
      mode: 'bounded_status',
      attention_state: 'findings_ready',
      freshness_seconds: 10,
      freshness_label: '10s old',
      session_count: 2,
      sessions: [{ session_id: 'session-1' }, { session_id: 'session-2' }],
      message: 'Mirroring bounded Roger attention state from local companion.',
    },
  });

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    requestStatusMirror(TEST_CONTEXT);

    const findingsButton = findActionButton(dom, 'show_findings');
    assert.equal(findingsButton.hidden, false);
    assert.equal(findingsButton.classList.contains('roger-panel-button--primary'), true);
    const resumeButton = findActionButton(dom, 'resume_review');
    assert.equal(resumeButton.hidden, false);
    assert.equal(resumeButton.textContent, `${RESUME_ACTION_LABEL} (2)`);
    assert.equal(dom.badge.textContent, 'Findings ready (10s old)');
  });
});

test('session inventory mirror never clears a persistent launch status line', () => {
  const dom = makePanelDom();
  const chromeStub = makeChromeStub({
    launchResponse: {
      ok: false,
      mode: 'bridge_preflight_failed',
      action: 'start_review',
      message: 'Roger bridge preflight failed.',
      guidance: 'Roger data directory not found. Run `rr init` to set up.',
      failure_kind: 'preflight_failed',
    },
    statusResponse: {
      ok: true,
      mode: 'no_local_session',
      session_count: 0,
    },
  });
  const button = { disabled: false, textContent: 'Start Review in Roger' };

  withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
    triggerLaunch('start_review', TEST_CONTEXT, button);

    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /Roger bridge preflight failed\./);

    // New truthful inventory modes must obey the same persistence rule.
    requestStatusMirror(TEST_CONTEXT);
    assert.equal(dom.statusNode.hidden, false);
    assert.match(dom.statusNode.textContent, /Roger bridge preflight failed\./);
    assert.equal(findActionButton(dom, 'resume_review').hidden, true);
  });
});

// The worker-side watchdog bound. The content settle timeout must exceed this
// so the worker leg always gets its full chance to resolve before the panel
// degrades. Imported directly so the assertion tracks the real source value.
const { STATUS_PROBE_TIMEOUT_MS } = require('../background/main.js');

test('content settle timeout strictly exceeds the worker watchdog so the worker leg resolves first', () => {
  assert.equal(typeof STATUS_MIRROR_SETTLE_TIMEOUT_MS, 'number');
  assert.equal(typeof STATUS_PROBE_TIMEOUT_MS, 'number');
  assert.ok(
    STATUS_MIRROR_SETTLE_TIMEOUT_MS > STATUS_PROBE_TIMEOUT_MS,
    'content settle timeout must outlast the worker watchdog'
  );
});

// A chrome stub whose status sendMessage callback never fires, modelling Edge
// tearing the module service worker down before the host reply lands.
function makeNeverSettlingStatusChromeStub() {
  return {
    runtime: {
      lastError: null,
      sendMessage(payload, _callback) {
        // Deliberately drop the status callback on the floor: the worker leg
        // never replies, so only the content-side timeout fallback can settle.
        if (payload?.type === 'roger_bridge_status') {
          return;
        }
      },
    },
  };
}

test('requestStatusMirror degrades to launch-only when the worker callback never fires', () => {
  const dom = makePanelDom();
  const chromeStub = makeNeverSettlingStatusChromeStub();

  // Fake timers: capture the scheduled settle fallback without real waiting.
  const scheduled = [];
  const previousSetTimeout = global.setTimeout;
  const previousClearTimeout = global.clearTimeout;
  global.setTimeout = (fn, ms) => {
    scheduled.push({ fn, ms });
    return scheduled.length;
  };
  global.clearTimeout = () => {};

  try {
    withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
      assert.doesNotThrow(() => requestStatusMirror(TEST_CONTEXT));

      // A bounded settle fallback must be armed at the content timeout.
      const settleFallback = scheduled.find(
        (entry) => entry.ms === STATUS_MIRROR_SETTLE_TIMEOUT_MS
      );
      assert.ok(settleFallback, 'a launch-only settle fallback must be scheduled');

      // Before the fallback fires, the panel has not yet been wrongly settled
      // by a non-existent worker reply; firing the fallback settles it.
      assert.doesNotThrow(() => settleFallback.fn());

      // Panel settled to launch-only: badge cleared, legacy both-buttons
      // surface retained (unknown inventory), info names launch-only.
      assert.equal(dom.badge.style.display, 'none');
      assert.equal(findActionButton(dom, 'start_review').hidden, false);
      assert.equal(findActionButton(dom, 'resume_review').hidden, false);
      assert.match(dom.infoNode.textContent, /Launch-only mode/i);

      // The status action line is owned by launch actions, not the mirror.
      assert.equal(dom.statusNode.hidden, true);
    });
  } finally {
    global.setTimeout = previousSetTimeout;
    global.clearTimeout = previousClearTimeout;
  }
});

test('requestStatusMirror settles exactly once: a late worker reply after the fallback is a no-op', () => {
  const dom = makePanelDom();

  // Capture the status callback so we can fire it AFTER the timeout fallback.
  let statusCallback = null;
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(payload, callback) {
        if (payload?.type === 'roger_bridge_status') {
          statusCallback = callback;
        }
      },
    },
  };

  const scheduled = [];
  const previousSetTimeout = global.setTimeout;
  const previousClearTimeout = global.clearTimeout;
  global.setTimeout = (fn, ms) => {
    scheduled.push({ fn, ms });
    return scheduled.length;
  };
  global.clearTimeout = () => {};

  try {
    withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
      requestStatusMirror(TEST_CONTEXT);

      const settleFallback = scheduled.find(
        (entry) => entry.ms === STATUS_MIRROR_SETTLE_TIMEOUT_MS
      );
      // Fallback settles first -> launch-only.
      settleFallback.fn();
      assert.match(dom.infoNode.textContent, /Launch-only mode/i);

      // A late worker reply (e.g. a revived worker) must NOT re-render the
      // panel: the mirror already settled exactly once.
      assert.doesNotThrow(() => {
        statusCallback({
          ok: true,
          mode: 'bounded_status',
          attention_state: 'findings_ready',
          freshness_seconds: 3,
          freshness_label: '3s old',
          session_count: 1,
          sessions: [{ session_id: 'session-1' }],
        });
      });

      // Badge stays cleared and info stays launch-only: the late reply lost.
      assert.equal(dom.badge.style.display, 'none');
      assert.match(dom.infoNode.textContent, /Launch-only mode/i);
    });
  } finally {
    global.setTimeout = previousSetTimeout;
    global.clearTimeout = previousClearTimeout;
  }
});

// ---------------------------------------------------------------------------
// PR-listing route + row-target discovery (bead ziv8.1)
// ---------------------------------------------------------------------------

function makeListingNode(tagName, props = {}) {
  const attributes = new Map();
  if (props.href) {
    attributes.set('href', props.href);
  }
  if (props.ariaLabel) {
    attributes.set('aria-label', props.ariaLabel);
  }
  if (props.testid) {
    attributes.set('data-testid', props.testid);
  }
  return {
    tagName: String(tagName).toUpperCase(),
    className: props.className || '',
    textContent: props.textContent || '',
    dataset: {},
    children: [],
    parentElement: null,
    getAttribute(name) {
      return attributes.has(name) ? attributes.get(name) : null;
    },
    setAttribute(name, value) {
      attributes.set(name, String(value));
    },
    appendChild(child) {
      this.children.push(child);
      child.parentElement = this;
      return child;
    },
  };
}

function makeListingDocument(roots) {
  const body = makeListingNode('BODY');
  for (const root of roots) {
    body.appendChild(root);
  }
  return {
    body,
    querySelectorAll(selector) {
      if (selector !== 'a[href]') {
        return [];
      }
      const out = [];
      const walk = (node) => {
        for (const child of node.children) {
          if (child.tagName === 'A' && child.getAttribute('href')) {
            out.push(child);
          }
          walk(child);
        }
      };
      walk(body);
      return out;
    },
  };
}

// Build a list row container with one primary PR link, an optional review-state
// marker, and any number of extra (possibly foreign/non-PR) links.
function makeListingRow({ testid = 'issue-row', className, href, state, extraLinks = [] } = {}) {
  const row = makeListingNode('DIV', { testid, className });
  if (state) {
    row.appendChild(makeListingNode('SPAN', { ariaLabel: `${state} pull request` }));
  }
  if (href) {
    row.appendChild(makeListingNode('A', { href }));
  }
  for (const link of extraLinks) {
    row.appendChild(makeListingNode('A', { href: link }));
  }
  return row;
}

const LISTING_REPO_CONTEXT = {
  owner: 'octo',
  repo: 'roger-reviewer',
  route: PR_LISTING_ROUTE,
  openOnly: true,
};

test('parsePullRequestListContext recognizes repository PR list routes', () => {
  const base = parsePullRequestListContext({
    pathname: '/octo/roger-reviewer/pulls',
    search: '',
  });
  assert.equal(base.route, PR_LISTING_ROUTE);
  assert.equal(base.owner, 'octo');
  assert.equal(base.repo, 'roger-reviewer');
  assert.equal(base.openOnly, true);

  const filtered = parsePullRequestListContext({
    pathname: '/octo/roger-reviewer/pulls',
    search: '?q=is%3Apr+is%3Aopen',
  });
  assert.equal(filtered.route, PR_LISTING_ROUTE);
  assert.equal(filtered.openOnly, true);

  const closedFilter = parsePullRequestListContext({
    pathname: '/octo/roger-reviewer/pulls',
    search: '?q=is%3Apr+is%3Aclosed',
  });
  assert.equal(closedFilter.route, PR_LISTING_ROUTE);
  assert.equal(closedFilter.openOnly, false);
});

test('parsePullRequestListContext stays separate from the PR-detail route', () => {
  // Detail pages and other non-listing pages never resolve as a listing route.
  assert.equal(
    parsePullRequestListContext({ pathname: '/octo/roger-reviewer/pull/42', search: '' }),
    null
  );
  assert.equal(
    parsePullRequestListContext({ pathname: '/octo/roger-reviewer', search: '' }),
    null
  );
  assert.equal(
    parsePullRequestListContext({ pathname: '/octo/roger-reviewer/issues', search: '' }),
    null
  );
  assert.equal(parsePullRequestListContext({ pathname: '/', search: '' }), null);
  // The global cross-repo /pulls dashboard is out of scope.
  assert.equal(parsePullRequestListContext({ pathname: '/pulls', search: '' }), null);

  // SPA navigation: detail -> list flips discovery on without disturbing the
  // PR-detail route contract.
  const detailLoc = { pathname: '/octo/roger-reviewer/pull/42', search: '' };
  const listLoc = { pathname: '/octo/roger-reviewer/pulls', search: '' };
  assert.equal(parsePullRequestListContext(detailLoc), null);
  assert.equal(parsePullRequestListContext(listLoc).route, PR_LISTING_ROUTE);
});

test('isOpenOnlyListingQuery treats default and is:open as open-only', () => {
  assert.equal(isOpenOnlyListingQuery(''), true);
  assert.equal(isOpenOnlyListingQuery('?page=2'), true);
  assert.equal(isOpenOnlyListingQuery('?q=is%3Aopen'), true);
  assert.equal(isOpenOnlyListingQuery('?q=is%3Aclosed'), false);
  assert.equal(isOpenOnlyListingQuery('?q=is%3Amerged'), false);
  assert.equal(isOpenOnlyListingQuery('?q=is%3Apr+is%3Aall'), false);
  // A query with no explicit state qualifier can surface closed PRs, so it is
  // not treated as open-only.
  assert.equal(isOpenOnlyListingQuery('?q=is%3Apr+author%3Aocto'), false);
});

test('parsePullTargetHref normalizes repo PR links and rejects others', () => {
  assert.deepEqual(parsePullTargetHref('/octo/roger-reviewer/pull/7', 'octo', 'roger-reviewer'), {
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 7,
  });
  // Sub-page links normalize to the base PR target.
  assert.deepEqual(
    parsePullTargetHref('/octo/roger-reviewer/pull/7/files', 'octo', 'roger-reviewer'),
    { owner: 'octo', repo: 'roger-reviewer', pr_number: 7 }
  );
  // Absolute hrefs resolve the same way.
  assert.deepEqual(
    parsePullTargetHref('https://github.com/octo/roger-reviewer/pull/9', 'octo', 'roger-reviewer'),
    { owner: 'octo', repo: 'roger-reviewer', pr_number: 9 }
  );
  // Foreign repos and non-PR links are rejected.
  assert.equal(parsePullTargetHref('/other/repo/pull/3', 'octo', 'roger-reviewer'), null);
  assert.equal(parsePullTargetHref('/octo/roger-reviewer/issues/3', 'octo', 'roger-reviewer'), null);
  assert.equal(parsePullTargetHref('/octo/roger-reviewer/compare', 'octo', 'roger-reviewer'), null);
  assert.equal(parsePullTargetHref('/octo/roger-reviewer/pull/abc', 'octo', 'roger-reviewer'), null);
});

test('extractPrListingRowTargets dedups per row and ignores foreign/non-PR links', () => {
  const row1 = makeListingRow({
    href: '/octo/roger-reviewer/pull/10',
    state: 'open',
    extraLinks: ['/octo/roger-reviewer/pull/10', '/octo/roger-reviewer/pull/10/files'],
  });
  const row2 = makeListingRow({
    href: '/octo/roger-reviewer/pull/11',
    state: 'open',
    extraLinks: ['/other/repo/pull/99', '/octo/roger-reviewer/issues/5'],
  });
  const dom = makeListingDocument([row1, row2]);

  const targets = extractPrListingRowTargets(dom, LISTING_REPO_CONTEXT);
  assert.equal(targets.length, 2);
  assert.deepEqual(
    targets.map((t) => t.pr_number).sort((a, b) => a - b),
    [10, 11]
  );
  assert.equal(targets[0].row_node, row1);
  assert.equal(targets[1].row_node, row2);
  assert.equal(targets[0].owner, 'octo');
  assert.equal(targets[0].repo, 'roger-reviewer');
});

test('extractPrListingRowTargets recognizes classic js-issue-row containers', () => {
  const classicRow = makeListingNode('DIV', { className: 'js-issue-row' });
  classicRow.appendChild(
    makeListingNode('SPAN', { className: 'octicon octicon-git-pull-request color-fg-open' })
  );
  classicRow.appendChild(makeListingNode('A', { href: '/octo/roger-reviewer/pull/50' }));
  const dom = makeListingDocument([classicRow]);

  const targets = extractPrListingRowTargets(dom, LISTING_REPO_CONTEXT);
  assert.equal(targets.length, 1);
  assert.equal(targets[0].pr_number, 50);
  assert.equal(targets[0].row_node, classicRow);
});

test('extractPrListingRowTargets excludes closed and merged rows but keeps drafts', () => {
  const dom = makeListingDocument([
    makeListingRow({ href: '/octo/roger-reviewer/pull/1', state: 'open' }),
    makeListingRow({ href: '/octo/roger-reviewer/pull/2', state: 'closed' }),
    makeListingRow({ href: '/octo/roger-reviewer/pull/3', state: 'merged' }),
    makeListingRow({ href: '/octo/roger-reviewer/pull/4', state: 'draft' }),
  ]);

  const targets = extractPrListingRowTargets(dom, LISTING_REPO_CONTEXT);
  assert.deepEqual(
    targets.map((t) => t.pr_number).sort((a, b) => a - b),
    [1, 4]
  );
});

test('extractPrListingRowTargets includes stateless rows only on open-only routes', () => {
  const statelessRow = () => makeListingRow({ href: '/octo/roger-reviewer/pull/20' });

  const openOnly = extractPrListingRowTargets(
    makeListingDocument([statelessRow()]),
    LISTING_REPO_CONTEXT
  );
  assert.equal(openOnly.length, 1);

  const broad = extractPrListingRowTargets(makeListingDocument([statelessRow()]), {
    ...LISTING_REPO_CONTEXT,
    openOnly: false,
  });
  assert.equal(broad.length, 0);
});

test('extractPrListingRowTargets degrades when PR links are not inside a row container', () => {
  // A repository-level action bar link to a PR (no list-row container) must not
  // produce a misplaced target.
  const actionBar = makeListingNode('DIV', { className: 'gh-header-actions' });
  actionBar.appendChild(makeListingNode('A', { href: '/octo/roger-reviewer/pull/77' }));
  const dom = makeListingDocument([actionBar]);
  assert.deepEqual(extractPrListingRowTargets(dom, LISTING_REPO_CONTEXT), []);

  // Non-listing context and malformed documents degrade to no targets.
  assert.deepEqual(extractPrListingRowTargets(dom, null), []);
  assert.deepEqual(extractPrListingRowTargets(dom, { route: 'pr_detail' }), []);
  assert.deepEqual(extractPrListingRowTargets(null, LISTING_REPO_CONTEXT), []);
});

test('extractPrListingRowTargets is idempotent and side-effect free across rerenders', () => {
  const row1 = makeListingRow({ href: '/octo/roger-reviewer/pull/30', state: 'open' });
  const row2 = makeListingRow({ href: '/octo/roger-reviewer/pull/31', state: 'open' });
  const dom = makeListingDocument([row1, row2]);
  const childCountBefore = row1.children.length;

  const first = extractPrListingRowTargets(dom, LISTING_REPO_CONTEXT);
  const second = extractPrListingRowTargets(dom, LISTING_REPO_CONTEXT);

  assert.equal(first.length, 2);
  assert.equal(second.length, 2);
  assert.deepEqual(
    first.map((t) => t.pr_number),
    second.map((t) => t.pr_number)
  );
  assert.equal(first[0].row_node, second[0].row_node);
  // Extraction never mutates the DOM it reads.
  assert.equal(dom.body.children.length, 2);
  assert.equal(row1.children.length, childCountBefore);
});

test('extractPrListingRowTargets collapses duplicate rerendered rows for the same PR', () => {
  // PJAX/Turbo can transiently double-render a row; the unique-target contract
  // must keep one target rather than two.
  const dom = makeListingDocument([
    makeListingRow({ href: '/octo/roger-reviewer/pull/40', state: 'open' }),
    makeListingRow({ href: '/octo/roger-reviewer/pull/40', state: 'open' }),
  ]);

  const targets = extractPrListingRowTargets(dom, LISTING_REPO_CONTEXT);
  assert.equal(targets.length, 1);
  assert.equal(targets[0].pr_number, 40);
});

// ---------------------------------------------------------------------------
// PR-listing row control rendering (bead ziv8.2)
// ---------------------------------------------------------------------------

function makeRenderNode(tagName, props = {}) {
  const attrs = new Map();
  if (props.href) {
    attrs.set('href', props.href);
  }
  if (props.ariaLabel) {
    attrs.set('aria-label', props.ariaLabel);
  }
  if (props.testid) {
    attrs.set('data-testid', props.testid);
  }
  return {
    tagName: String(tagName).toUpperCase(),
    id: props.id || '',
    className: props.className || '',
    textContent: props.textContent || '',
    disabled: false,
    dataset: {},
    style: {},
    children: [],
    parentElement: null,
    _listeners: {},
    getAttribute(name) {
      return attrs.has(name) ? attrs.get(name) : null;
    },
    setAttribute(name, value) {
      attrs.set(name, String(value));
    },
    appendChild(child) {
      this.children.push(child);
      child.parentElement = this;
      return child;
    },
    remove() {
      const parent = this.parentElement;
      if (parent) {
        const index = parent.children.indexOf(this);
        if (index >= 0) {
          parent.children.splice(index, 1);
        }
        this.parentElement = null;
      }
    },
    addEventListener(type, fn) {
      if (!this._listeners[type]) {
        this._listeners[type] = [];
      }
      this._listeners[type].push(fn);
    },
    click() {
      for (const fn of this._listeners.click || []) {
        fn();
      }
    },
  };
}

function makeRenderDocument(rootNodes) {
  const head = makeRenderNode('HEAD');
  const body = makeRenderNode('BODY');
  for (const node of rootNodes) {
    body.appendChild(node);
  }
  const collectAll = () => {
    const out = [];
    const walk = (node) => {
      for (const child of node.children) {
        out.push(child);
        walk(child);
      }
    };
    walk(head);
    walk(body);
    return out;
  };
  return {
    head,
    body,
    createElement(tag) {
      return makeRenderNode(tag);
    },
    getElementById(id) {
      for (const node of collectAll()) {
        if (node.id === id) {
          return node;
        }
      }
      return null;
    },
    querySelectorAll(selector) {
      if (selector === 'a[href]') {
        return collectAll().filter((n) => n.tagName === 'A' && n.getAttribute('href'));
      }
      if (selector === `.${LISTING_CONTROL_CLASS}`) {
        return collectAll().filter(
          (n) =>
            typeof n.className === 'string' &&
            n.className.split(/\s+/).includes(LISTING_CONTROL_CLASS)
        );
      }
      return [];
    },
  };
}

function makeRenderRow({ testid = 'issue-row', href, state } = {}) {
  const row = makeRenderNode('DIV', { testid });
  if (state) {
    row.appendChild(makeRenderNode('SPAN', { ariaLabel: `${state} pull request` }));
  }
  if (href) {
    row.appendChild(makeRenderNode('A', { href }));
  }
  return row;
}

function listingButtonsIn(node, out = []) {
  for (const child of node.children) {
    if (child.tagName === 'BUTTON') {
      out.push(child);
    }
    listingButtonsIn(child, out);
  }
  return out;
}

test('ensurePrListingRowControls renders one start control per eligible open row', () => {
  const openRow = makeRenderRow({ href: '/octo/roger-reviewer/pull/1', state: 'open' });
  const closedRow = makeRenderRow({ href: '/octo/roger-reviewer/pull/2', state: 'closed' });
  const draftRow = makeRenderRow({ href: '/octo/roger-reviewer/pull/3', state: 'draft' });
  const doc = makeRenderDocument([openRow, closedRow, draftRow]);

  const mounted = ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT);
  // open + draft are eligible; closed is not.
  assert.equal(mounted.length, 2);

  const controls = doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`);
  assert.equal(controls.length, 2);
  // Controls live inside the eligible rows, never outside a PR row.
  assert.equal(controls[0].parentElement, openRow);
  assert.equal(controls[1].parentElement, draftRow);
  assert.equal(
    closedRow.children.some(
      (c) => typeof c.className === 'string' && c.className.includes(LISTING_CONTROL_CLASS)
    ),
    false
  );
});

test('ensurePrListingRowControls is idempotent under rerenders', () => {
  const row = makeRenderRow({ href: '/octo/roger-reviewer/pull/5', state: 'open' });
  const doc = makeRenderDocument([row]);

  const first = ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT);
  const second = ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT);

  assert.equal(first.length, 1);
  assert.equal(second.length, 0);
  assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 1);
});

test('listing control exposes the full accessible name and only the start_review action', () => {
  const row = makeRenderRow({ href: '/octo/roger-reviewer/pull/7', state: 'open' });
  const doc = makeRenderDocument([row]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT);

  const controlId = listingControlIdForTarget({
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 7,
  });
  const control = doc.getElementById(controlId);
  assert.ok(control);
  const button = control.children[0];
  // Accessible name is the full label even if CSS ellipsizes the visible text.
  assert.equal(button.getAttribute('aria-label'), LISTING_START_LABEL);
  assert.equal(button.dataset.action, 'start_review');
  assert.equal(button.dataset.owner, 'octo');
  assert.equal(button.dataset.repo, 'roger-reviewer');
  assert.equal(button.dataset.prNumber, '7');

  // The listing surface renders only the start control: no resume / findings /
  // approval / posting actions.
  const buttons = listingButtonsIn(doc.body);
  assert.equal(buttons.length, 1);
  assert.equal(
    buttons.every((b) => b.dataset.action === 'start_review'),
    true
  );
});

test('clicking one row control marks pending state only on that row', () => {
  const rowA = makeRenderRow({ href: '/octo/roger-reviewer/pull/10', state: 'open' });
  const rowB = makeRenderRow({ href: '/octo/roger-reviewer/pull/11', state: 'open' });
  const doc = makeRenderDocument([rowA, rowB]);
  const launched = [];
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT, {
    onLaunch: (target) => launched.push(target.pr_number),
  });

  const btnA = doc.getElementById(
    listingControlIdForTarget({ owner: 'octo', repo: 'roger-reviewer', pr_number: 10 })
  ).children[0];
  const btnB = doc.getElementById(
    listingControlIdForTarget({ owner: 'octo', repo: 'roger-reviewer', pr_number: 11 })
  ).children[0];

  btnA.click();
  assert.equal(btnA.disabled, true);
  assert.equal(btnA.dataset.rrLaunching, 'true');
  // Row B is untouched: one row launching never rewrites the others.
  assert.equal(btnB.disabled, false);
  assert.equal(btnB.dataset.rrLaunching, undefined);
  assert.deepEqual(launched, [10]);
});

test('ensureListingStyles injects a single theme-aware style node', () => {
  const row = makeRenderRow({ href: '/octo/roger-reviewer/pull/30', state: 'open' });
  const doc = makeRenderDocument([row]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT);

  const styleNodes = doc.head.children.filter((n) => n.tagName === 'STYLE');
  assert.equal(styleNodes.length, 1);
  // Theme-aware: resolves from Primer button + accent tokens so light and dark
  // both stay legible.
  assert.match(styleNodes[0].textContent, /button-default-bgColor-rest/);
  assert.match(styleNodes[0].textContent, /fgColor-accent/);
  // Fit handling for narrow GitHub list widths.
  assert.match(styleNodes[0].textContent, /text-overflow:\s*ellipsis/);
  assert.match(styleNodes[0].textContent, /white-space:\s*nowrap/);
});

test('refreshPrListingControls injects on PR-list routes and clears off-route', () => {
  const row = makeRenderRow({ href: '/octo/roger-reviewer/pull/20', state: 'open' });
  const doc = makeRenderDocument([row]);
  const previousWindow = global.window;
  try {
    global.window = { location: { pathname: '/octo/roger-reviewer/pulls', search: '' } };
    refreshPrListingControls(doc);
    assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 1);

    // SPA navigation onto a PR-detail route is not a list route: controls clear.
    global.window = { location: { pathname: '/octo/roger-reviewer/pull/20', search: '' } };
    refreshPrListingControls(doc);
    assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 0);
  } finally {
    global.window = previousWindow;
  }
});

test('removeAllListingControls strips injected controls', () => {
  const row = makeRenderRow({ href: '/octo/roger-reviewer/pull/60', state: 'open' });
  const doc = makeRenderDocument([row]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT);
  assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 1);

  removeAllListingControls(doc);
  assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 0);
});

// ---------------------------------------------------------------------------
// PR-listing row launch dispatch through the bridge (bead ziv8.3)
// ---------------------------------------------------------------------------

function listingControlFor(doc, prNumber) {
  return doc.getElementById(
    listingControlIdForTarget({ owner: 'octo', repo: 'roger-reviewer', pr_number: prNumber })
  );
}

test('row launch sends the exact start_review payload for each PR row', () => {
  const sent = [];
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(msg, cb) {
        sent.push(msg);
        cb({ ok: true, mode: 'native_messaging', attention_state: 'awaiting_user_input' });
      },
    },
  };
  const doc = makeRenderDocument([
    makeRenderRow({ href: '/octo/roger-reviewer/pull/10', state: 'open' }),
    makeRenderRow({ href: '/octo/roger-reviewer/pull/11', state: 'open' }),
  ]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT, { chrome: chromeStub });

  listingControlFor(doc, 10).children[0].click();
  listingControlFor(doc, 11).children[0].click();

  assert.equal(sent.length, 2);
  // Only the existing start_review bridge action is ever used — no approval,
  // posting, or gh write path.
  assert.deepEqual(sent[0], {
    type: 'roger_bridge_launch',
    intent: { action: 'start_review', owner: 'octo', repo: 'roger-reviewer', pr_number: 10 },
  });
  assert.deepEqual(sent[1], {
    type: 'roger_bridge_launch',
    intent: { action: 'start_review', owner: 'octo', repo: 'roger-reviewer', pr_number: 11 },
  });
});

test('successful launch writes row-local status with target PR and session id', () => {
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(msg, cb) {
        cb({
          ok: true,
          mode: 'native_messaging',
          attention_state: 'awaiting_user_input',
          message: 'Launch intent dispatched.',
          session_id: 'sess-42',
        });
      },
    },
  };
  const doc = makeRenderDocument([
    makeRenderRow({ href: '/octo/roger-reviewer/pull/10', state: 'open' }),
    makeRenderRow({ href: '/octo/roger-reviewer/pull/11', state: 'open' }),
  ]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT, { chrome: chromeStub });

  const controlA = listingControlFor(doc, 10);
  controlA.children[0].click();
  const statusA = controlA.children[1];
  assert.equal(statusA.hidden, false);
  assert.equal(statusA.dataset.state, 'success');
  assert.match(statusA.textContent, /octo\/roger-reviewer#10/);
  assert.match(statusA.textContent, /sess-42/);
  // Button is restored after the launch settles.
  assert.equal(controlA.children[0].disabled, false);

  // Per-row isolation: row 11 was never touched.
  const statusB = listingControlFor(doc, 11).children[1];
  assert.equal(statusB.hidden, true);
  assert.equal(listingControlFor(doc, 11).children[0].disabled, false);
});

test('launch with no usable bridge surfaces a loud row-local error', () => {
  const doc = makeRenderDocument([makeRenderRow({ href: '/octo/roger-reviewer/pull/10', state: 'open' })]);
  // A chrome surface with no sendMessage is the "bridge unavailable" case.
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT, { chrome: { runtime: {} } });

  const control = listingControlFor(doc, 10);
  control.children[0].click();
  const status = control.children[1];
  assert.equal(status.dataset.state, 'error');
  assert.match(status.textContent, /Bridge unavailable/i);
});

test('bridge failure envelope shows a persistent row-local error (no auto-hide)', () => {
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(msg, cb) {
        cb({ ok: false, message: 'Native host missing.', guidance: 'Run `rr extension setup`.' });
      },
    },
  };
  const doc = makeRenderDocument([makeRenderRow({ href: '/octo/roger-reviewer/pull/12', state: 'open' })]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT, { chrome: chromeStub });

  const control = listingControlFor(doc, 12);
  control.children[0].click();
  const status = control.children[1];
  assert.equal(status.dataset.state, 'error');
  assert.match(status.textContent, /Native host missing/);
  assert.match(status.textContent, /rr extension setup/);
  // The error stays visible: there is no auto-hide timer on the row status.
  assert.equal(status.hidden, false);
});

test('chrome.runtime.lastError surfaces bridge disconnect guidance', () => {
  const chromeStub = {
    runtime: {
      lastError: { message: 'port closed' },
      sendMessage(msg, cb) {
        cb(undefined);
      },
    },
  };
  const doc = makeRenderDocument([makeRenderRow({ href: '/octo/roger-reviewer/pull/13', state: 'open' })]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT, { chrome: chromeStub });

  const control = listingControlFor(doc, 13);
  control.children[0].click();
  const status = control.children[1];
  assert.equal(status.dataset.state, 'error');
  assert.match(status.textContent, /port closed/);
  assert.match(status.textContent, /rr extension setup/);
});

test('custom_url_fallback launch reports a row-local fallback success', () => {
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(msg, cb) {
        cb({ ok: true, mode: 'custom_url_fallback' });
      },
    },
  };
  const doc = makeRenderDocument([makeRenderRow({ href: '/octo/roger-reviewer/pull/14', state: 'open' })]);
  ensurePrListingRowControls(doc, LISTING_REPO_CONTEXT, { chrome: chromeStub });

  const control = listingControlFor(doc, 14);
  control.children[0].click();
  const status = control.children[1];
  assert.equal(status.dataset.state, 'success');
  assert.match(status.textContent, /URL fallback/i);
});

// ---------------------------------------------------------------------------
// PR-listing review kickoff: realistic full-page fixture (bead ziv8.4)
//
// Deterministic fixture modeled on the product surface /cdilga/roger-reviewer/pulls:
// a repo header with a `New pull request` button, a filter/search subnav, a
// labels/milestones control bar, a stray repo-level PR shortcut link, and a list
// container with open + closed PR rows. Proves the full kickoff lane (route ->
// extract -> inject -> reinject -> dispatch) without live GitHub, and that Roger
// never disturbs repository-level chrome.
// ---------------------------------------------------------------------------

function appendChildNode(parent, tag, props) {
  return parent.appendChild(makeRenderNode(tag, props));
}

function hasListingControlWithin(node) {
  for (const child of node.children) {
    if (
      typeof child.className === 'string' &&
      child.className.split(/\s+/).includes(LISTING_CONTROL_CLASS)
    ) {
      return true;
    }
    if (hasListingControlWithin(child)) {
      return true;
    }
  }
  return false;
}

function makeListingPageFixture() {
  const owner = 'cdilga';
  const repo = 'roger-reviewer';

  // Repo-level header / toolbar Roger must not disturb.
  const header = makeRenderNode('DIV', { className: 'repo-header gh-header-actions' });
  appendChildNode(header, 'A', { href: `/${owner}/${repo}/compare`, textContent: 'New pull request' });
  // A stray repo-level link straight to a PR (e.g. a "latest PR" shortcut) that
  // is NOT inside a list row — must never receive a control.
  appendChildNode(header, 'A', { href: `/${owner}/${repo}/pull/999`, textContent: 'Latest PR' });

  // Filter / search subnav with same-page query links.
  const subnav = makeRenderNode('DIV', { className: 'subnav table-list-header' });
  appendChildNode(subnav, 'A', { href: `/${owner}/${repo}/pulls?q=is%3Apr+is%3Aopen`, textContent: 'Open' });
  appendChildNode(subnav, 'A', { href: `/${owner}/${repo}/pulls?q=is%3Apr+is%3Aclosed`, textContent: 'Closed' });
  appendChildNode(subnav, 'A', { href: `/${owner}/${repo}/labels`, textContent: 'Labels' });
  appendChildNode(subnav, 'A', { href: `/${owner}/${repo}/milestones`, textContent: 'Milestones' });

  // List container with open + closed PR rows.
  const list = makeRenderNode('DIV', { className: 'js-navigation-container' });
  const rowOpenA = makeRenderRow({ href: `/${owner}/${repo}/pull/101`, state: 'open' });
  const rowOpenB = makeRenderRow({ href: `/${owner}/${repo}/pull/102`, state: 'open' });
  const rowClosed = makeRenderRow({ href: `/${owner}/${repo}/pull/100`, state: 'closed' });
  list.appendChild(rowOpenA);
  list.appendChild(rowOpenB);
  list.appendChild(rowClosed);

  const doc = makeRenderDocument([header, subnav, list]);
  const context = parsePullRequestListContext({ pathname: `/${owner}/${repo}/pulls`, search: '' });
  return { doc, context, header, subnav, list, rowOpenA, rowOpenB, rowClosed };
}

function listingControlForOwner(doc, owner, repo, prNumber) {
  return doc.getElementById(listingControlIdForTarget({ owner, repo, pr_number: prNumber }));
}

test('ziv8.4 fixture: recognizes the list route and extracts only open PR row targets', () => {
  const { doc, context } = makeListingPageFixture();
  assert.equal(context.route, PR_LISTING_ROUTE);
  assert.equal(context.owner, 'cdilga');
  assert.equal(context.openOnly, true);

  const targets = extractPrListingRowTargets(doc, context);
  // Only the two open rows — never the closed row, the New PR / compare link,
  // the labels/milestones links, the same-page filter links, or the stray
  // repo-level PR shortcut.
  assert.deepEqual(
    targets.map((t) => t.pr_number).sort((a, b) => a - b),
    [101, 102]
  );
});

test('ziv8.4 fixture: injects controls only into open PR rows, never repo-level chrome', () => {
  const { doc, context, header, subnav, rowClosed } = makeListingPageFixture();
  const mounted = ensurePrListingRowControls(doc, context, { chrome: { runtime: {} } });
  assert.equal(mounted.length, 2);
  assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 2);

  // No control anywhere in the repo header (New pull request / stray PR link) or
  // the filter/labels/milestones subnav.
  assert.equal(hasListingControlWithin(header), false);
  assert.equal(hasListingControlWithin(subnav), false);
  // No control on the closed row.
  assert.equal(hasListingControlWithin(rowClosed), false);
  // Controls live inside the open rows.
  assert.ok(listingControlForOwner(doc, 'cdilga', 'roger-reviewer', 101));
  assert.ok(listingControlForOwner(doc, 'cdilga', 'roger-reviewer', 102));
});

test('ziv8.4 fixture: reinjection after Morph/SPA rerender does not duplicate controls', () => {
  const { doc, context, list } = makeListingPageFixture();
  ensurePrListingRowControls(doc, context, { chrome: { runtime: {} } });
  assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 2);

  // Idempotent rerun on the unchanged DOM.
  ensurePrListingRowControls(doc, context, { chrome: { runtime: {} } });
  assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 2);

  // Morph adds a duplicate row for PR 101 and a brand-new open row 103.
  list.appendChild(makeRenderRow({ href: '/cdilga/roger-reviewer/pull/101', state: 'open' }));
  list.appendChild(makeRenderRow({ href: '/cdilga/roger-reviewer/pull/103', state: 'open' }));
  ensurePrListingRowControls(doc, context, { chrome: { runtime: {} } });

  // 101 stays single (unique target), 102 + 103 present => 3 controls total.
  assert.equal(doc.querySelectorAll(`.${LISTING_CONTROL_CLASS}`).length, 3);
  assert.ok(listingControlForOwner(doc, 'cdilga', 'roger-reviewer', 103));
});

test('ziv8.4 fixture: exact bridge payloads for two row launches', () => {
  const { doc, context } = makeListingPageFixture();
  const sent = [];
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(msg, cb) {
        sent.push(msg);
        cb({ ok: true, mode: 'native_messaging', attention_state: 'awaiting_user_input', session_id: 's' + msg.intent.pr_number });
      },
    },
  };
  ensurePrListingRowControls(doc, context, { chrome: chromeStub });

  listingControlForOwner(doc, 'cdilga', 'roger-reviewer', 101).children[0].click();
  listingControlForOwner(doc, 'cdilga', 'roger-reviewer', 102).children[0].click();

  assert.deepEqual(sent, [
    { type: 'roger_bridge_launch', intent: { action: 'start_review', owner: 'cdilga', repo: 'roger-reviewer', pr_number: 101 } },
    { type: 'roger_bridge_launch', intent: { action: 'start_review', owner: 'cdilga', repo: 'roger-reviewer', pr_number: 102 } },
  ]);
});

test('ziv8.4 fixture: fail-closed rendering when the bridge is unavailable or errors', () => {
  // Bridge unavailable.
  const fixtureA = makeListingPageFixture();
  ensurePrListingRowControls(fixtureA.doc, fixtureA.context, { chrome: { runtime: {} } });
  const controlA = listingControlForOwner(fixtureA.doc, 'cdilga', 'roger-reviewer', 101);
  controlA.children[0].click();
  assert.equal(controlA.children[1].dataset.state, 'error');
  assert.match(controlA.children[1].textContent, /Bridge unavailable/i);

  // Error envelope from the bridge.
  const fixtureB = makeListingPageFixture();
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(msg, cb) {
        cb({ ok: false, message: 'Bridge preflight failed.', guidance: 'Run `rr extension doctor`.' });
      },
    },
  };
  ensurePrListingRowControls(fixtureB.doc, fixtureB.context, { chrome: chromeStub });
  const controlB = listingControlForOwner(fixtureB.doc, 'cdilga', 'roger-reviewer', 102);
  controlB.children[0].click();
  assert.equal(controlB.children[1].dataset.state, 'error');
  assert.match(controlB.children[1].textContent, /Bridge preflight failed/);
  assert.match(controlB.children[1].textContent, /rr extension doctor/);
});

// --- ack/progress protocol: launch-progress rendering -----------------------

test('describeLaunchProgress maps host stages to loud, human status lines', () => {
  assert.equal(describeLaunchProgress('host_started'), 'Roger host connected — running preflight…');
  assert.equal(describeLaunchProgress('preflight_ok'), 'Launching review…');
  // Unknown/future stages render as nothing rather than a raw enum token.
  assert.equal(describeLaunchProgress('something_new'), null);
  assert.equal(describeLaunchProgress(undefined), null);
});

test('handleLaunchProgressMessage renders progress into the PR-detail panel status', () => {
  const dom = makePanelDom();
  const previousWindow = global.window;
  const previousDocument = global.document;
  global.window = { location: { pathname: '/acme/widgets/pull/42', search: '' } };
  global.document = dom.documentStub;
  try {
    const handled = handleLaunchProgressMessage(
      {
        type: 'roger_bridge_launch_progress',
        stage: 'host_started',
        intent: { action: 'start_review', owner: 'acme', repo: 'widgets', pr_number: 42 },
      },
      dom.documentStub
    );
    assert.equal(handled, true);
    assert.equal(dom.statusNode.hidden, false);
    assert.equal(dom.statusNode.textContent, 'Roger host connected — running preflight…');
    assert.equal(dom.infoNode.textContent, 'Roger host connected — running preflight…');

    // A second stage supersedes the first in the same persistent line.
    handleLaunchProgressMessage(
      {
        type: 'roger_bridge_launch_progress',
        stage: 'preflight_ok',
        intent: { action: 'start_review', owner: 'acme', repo: 'widgets', pr_number: 42 },
      },
      dom.documentStub
    );
    assert.equal(dom.statusNode.textContent, 'Launching review…');
  } finally {
    global.window = previousWindow;
    global.document = previousDocument;
  }
});

test('handleLaunchProgressMessage ignores progress for a different PR', () => {
  const dom = makePanelDom();
  const previousWindow = global.window;
  const previousDocument = global.document;
  global.window = { location: { pathname: '/acme/widgets/pull/42', search: '' } };
  global.document = dom.documentStub;
  try {
    const handled = handleLaunchProgressMessage(
      {
        type: 'roger_bridge_launch_progress',
        stage: 'host_started',
        intent: { action: 'start_review', owner: 'acme', repo: 'widgets', pr_number: 999 },
      },
      dom.documentStub
    );
    assert.equal(handled, false);
    assert.equal(dom.statusNode.hidden, true);
    assert.equal(dom.statusNode.textContent, '');
  } finally {
    global.window = previousWindow;
    global.document = previousDocument;
  }
});

test('handleLaunchProgressMessage routes progress to the matching listing row status', () => {
  const { doc, context } = makeListingPageFixture();
  ensurePrListingRowControls(doc, context, { chrome: { runtime: {} } });
  const control = listingControlForOwner(doc, 'cdilga', 'roger-reviewer', 101);
  assert.ok(control, 'row control must be mounted for the test');

  const previousWindow = global.window;
  // Listing route: parsePullRequestContext returns null so the panel branch is
  // skipped and only the row is updated.
  global.window = { location: { pathname: '/cdilga/roger-reviewer/pulls', search: '' } };
  try {
    const handled = handleLaunchProgressMessage(
      {
        type: 'roger_bridge_launch_progress',
        stage: 'host_started',
        intent: { action: 'start_review', owner: 'cdilga', repo: 'roger-reviewer', pr_number: 101 },
      },
      doc
    );
    assert.equal(handled, true);
    const statusNode = control.children[1];
    assert.equal(statusNode.hidden, false);
    assert.equal(statusNode.textContent, 'Roger host connected — running preflight…');
    assert.equal(statusNode.dataset.state, 'pending');

    // A non-matching row is untouched.
    const other = listingControlForOwner(doc, 'cdilga', 'roger-reviewer', 102);
    assert.equal(other.children[1].hidden, true);
  } finally {
    global.window = previousWindow;
  }
});

test('status probe silent degrade now renders a visible mirror-unavailable note', () => {
  const dom = makePanelDom();
  const previousDocument = global.document;
  const previousChrome = global.chrome;
  global.document = dom.documentStub;
  delete global.chrome;
  try {
    requestStatusMirror(TEST_CONTEXT);
    assert.match(dom.infoNode.textContent, /Local status mirror unavailable/i);
  } finally {
    global.document = previousDocument;
    if (previousChrome === undefined) {
      delete global.chrome;
    } else {
      global.chrome = previousChrome;
    }
  }
});

test('manifest content_scripts cover both the PR-detail and PR-listing routes', () => {
  const fs = require('node:fs');
  const path = require('node:path');
  const manifestPath = path.join(__dirname, '..', '..', 'manifest.template.json');
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const matches = (manifest.content_scripts || []).flatMap((entry) => entry.matches || []);
  assert.ok(
    matches.some((m) => m.includes('/pull/')),
    'content_scripts must match the PR-detail route (…/pull/*)'
  );
  assert.ok(
    matches.some((m) => m.includes('/pulls')),
    'content_scripts must match the PR-listing route (…/pulls*) so row controls are not dead code'
  );
});

// ---------------------------------------------------------------------------
// Read-only findings staging (bead rr-self-exploration-review-workbench-
// recovery-tmmn.5). The renderer's item shape mirrors the real
// `rr findings --robot` projection (packages/cli/src/lib.rs handle_findings):
// finding_id, fingerprint, title, triage_state, outbound_state, evidence_count.
// ---------------------------------------------------------------------------

function findFindingsNodes(root, className) {
  return findNodes(
    root,
    (node) =>
      typeof node.className === 'string' &&
      node.className.split(/\s+/).includes(className)
  );
}

function findingsText(root, className) {
  return findFindingsNodes(root, className).map((node) => node.textContent);
}

function makeFindingsEnv() {
  const body = createTestElement('div');
  body.id = FINDINGS_BODY_ID;
  const statusNode = {
    textContent: '',
    hidden: true,
    parentElement: { classList: { contains: () => false } },
    classList: createStatusClassList(),
  };
  const documentStub = {
    createElement: (tag) => createTestElement(tag),
    getElementById(id) {
      if (id === FINDINGS_BODY_ID) {
        return body;
      }
      if (id === 'roger-reviewer-status') {
        return statusNode;
      }
      return null;
    },
  };
  return { body, statusNode, documentStub };
}

function makeFindingsChromeStub({ response, lastError = null } = {}) {
  const sent = [];
  const stub = {
    runtime: {
      lastError: null,
      sendMessage(payload, callback) {
        sent.push(payload);
        if (lastError) {
          stub.runtime.lastError = { message: lastError };
          callback(undefined);
          stub.runtime.lastError = null;
          return;
        }
        callback(response);
      },
    },
  };
  return { stub, sent };
}

const RELAYED_FINDINGS_RESPONSE = {
  ok: true,
  action: 'show_findings',
  session_id: 'session-42',
  attention_state: 'findings_ready',
  findings: {
    count: 3,
    warnings: ['reranker degraded to lexical fallback'],
    items: [
      {
        finding_id: 'f1',
        fingerprint: 'fp1',
        title: 'Unbounded retry loop in worker',
        triage_state: 'new',
        outbound_state: 'not_drafted',
        evidence_count: 2,
      },
      {
        finding_id: 'f2',
        fingerprint: 'fp2',
        title: 'Missing auth check on delete route',
        triage_state: 'new',
        outbound_state: 'drafted',
        evidence_count: 1,
      },
      {
        finding_id: 'f3',
        fingerprint: 'fp3',
        title: 'Typo in log message',
        triage_state: 'dismissed',
        outbound_state: 'not_drafted',
        evidence_count: 0,
      },
    ],
  },
};

test('normalizeFindingsPayload lifts a relayed findings envelope into normalized items', () => {
  const payload = normalizeFindingsPayload(RELAYED_FINDINGS_RESPONSE);
  assert.equal(payload.relayed, true);
  assert.equal(payload.count, 3);
  assert.equal(payload.items.length, 3);
  assert.deepEqual(payload.warnings, ['reranker degraded to lexical fallback']);
  const first = payload.items[0];
  assert.equal(first.finding_id, 'f1');
  assert.equal(first.title, 'Unbounded retry loop in worker');
  assert.equal(first.triage_state, 'new');
  assert.equal(first.outbound_state, 'not_drafted');
  assert.equal(first.evidence_count, 2);
});

test('normalizeFindingsPayload reports the honest not-relayed degrade when the bridge omits findings detail', () => {
  // The current BridgeResponse (packages/bridge/src/lib.rs) strips findings
  // items down to session_id + attention snapshot, so a real successful
  // show_findings response carries no items at all.
  const payload = normalizeFindingsPayload({
    ok: true,
    action: 'show_findings',
    session_id: 'session-42',
    attention_state: 'findings_ready',
    message: 'rr findings completed for acme/widgets#42',
    guidance: 'Open `rr tui` for the authoritative view.',
  });
  assert.equal(payload.relayed, false);
  assert.equal(payload.count, 0);
  assert.equal(payload.items.length, 0);
  assert.equal(payload.guidance, 'Open `rr tui` for the authoritative view.');
});

test('normalizeFindingsItem drops entries with neither title nor finding_id', () => {
  const payload = normalizeFindingsPayload({
    findings: { items: [{ triage_state: 'new' }, { finding_id: 'ok', title: 'Real' }] },
  });
  assert.equal(payload.items.length, 1);
  assert.equal(payload.items[0].finding_id, 'ok');
});

test('groupFindingsByTriage sorts new before dismissed and preserves in-group order', () => {
  const payload = normalizeFindingsPayload(RELAYED_FINDINGS_RESPONSE);
  const groups = groupFindingsByTriage(payload.items);
  assert.deepEqual(
    groups.map((group) => group.state),
    ['new', 'dismissed']
  );
  assert.equal(groups[0].items.length, 2);
  assert.deepEqual(
    groups[0].items.map((item) => item.finding_id),
    ['f1', 'f2']
  );
  assert.equal(groups[1].items.length, 1);
});

test('renderFindingsStaging renders a grouped read-only staged list with counts, badges, and caption', () => {
  const env = makeFindingsEnv();
  const payload = renderFindingsStaging(
    env.body,
    RELAYED_FINDINGS_RESPONSE,
    env.documentStub
  );

  assert.equal(payload.relayed, true);
  // Fixed read-only caption is always present.
  assert.deepEqual(findingsText(env.body, 'rr-findings-caption'), [FINDINGS_READONLY_CAPTION]);
  // Total counts, new group first.
  assert.deepEqual(findingsText(env.body, 'rr-findings-summary'), [
    '3 findings — 2 new, 1 dismissed',
  ]);
  assert.deepEqual(findingsText(env.body, 'rr-findings-group-title'), [
    'New (2)',
    'Dismissed (1)',
  ]);
  // Item titles rendered in group order (new leads).
  assert.deepEqual(findingsText(env.body, 'rr-findings-item-title'), [
    'Unbounded retry loop in worker',
    'Missing auth check on delete route',
    'Typo in log message',
  ]);
  // Triage + outbound badges present.
  const triageBadges = findingsText(env.body, 'rr-badge--triage');
  assert.ok(triageBadges.every((text) => text.startsWith('Triage: ')));
  const outboundBadges = findingsText(env.body, 'rr-badge--outbound');
  assert.ok(outboundBadges.includes('Outbound: Drafted'));
  assert.ok(outboundBadges.includes('Outbound: Not Drafted'));
  // Evidence-count anchor for findings that have evidence; none for the 0 case.
  const anchors = findingsText(env.body, 'rr-findings-anchor');
  assert.ok(anchors.includes('2 evidence locations'));
  assert.ok(anchors.includes('1 evidence location'));
  assert.equal(anchors.length, 2);
  // Envelope warnings surfaced, never swallowed.
  const warningNotes = findingsText(env.body, 'rr-findings-note--warning');
  assert.equal(warningNotes.length, 1);
  assert.match(warningNotes[0], /reranker degraded to lexical fallback/);
});

test('renderFindingsStaging renders the honest empty state with guidance and caption', () => {
  const env = makeFindingsEnv();
  renderFindingsStaging(
    env.body,
    { ok: true, findings: { count: 0, items: [], warnings: [] }, guidance: 'Open `rr tui` for detail.' },
    env.documentStub
  );

  assert.deepEqual(findingsText(env.body, 'rr-findings-caption'), [FINDINGS_READONLY_CAPTION]);
  const emptyNotes = findingsText(env.body, 'rr-findings-note--empty');
  assert.equal(emptyNotes.length, 1);
  assert.match(emptyNotes[0], new RegExp(FINDINGS_EMPTY_MESSAGE.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  assert.match(emptyNotes[0], /Open `rr tui` for detail\./);
  // No list content in the empty case.
  assert.equal(findingsText(env.body, 'rr-findings-item-title').length, 0);
});

test('renderFindingsStaging renders the honest not-relayed degrade when the bridge omits findings detail', () => {
  const env = makeFindingsEnv();
  renderFindingsStaging(
    env.body,
    { ok: true, action: 'show_findings', session_id: 's', guidance: 'Open `rr tui`.' },
    env.documentStub
  );

  assert.deepEqual(findingsText(env.body, 'rr-findings-caption'), [FINDINGS_READONLY_CAPTION]);
  const degradedNotes = findingsText(env.body, 'rr-findings-note--degraded');
  assert.equal(degradedNotes.length, 1);
  assert.match(
    degradedNotes[0],
    new RegExp(FINDINGS_NOT_RELAYED_MESSAGE.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
  );
  assert.match(degradedNotes[0], /Open `rr tui`\./);
});

test('handleShowFindings dispatches show_findings through the existing bridge launch path', () => {
  const env = makeFindingsEnv();
  const { stub, sent } = makeFindingsChromeStub({ response: RELAYED_FINDINGS_RESPONSE });
  const button = { textContent: 'Show local findings', disabled: false };

  withPanelGlobals({ documentStub: env.documentStub, chromeStub: stub }, () => {
    handleShowFindings(TEST_CONTEXT, button);
  });

  assert.equal(sent.length, 1);
  assert.equal(sent[0].type, 'roger_bridge_launch');
  assert.deepEqual(sent[0].intent, {
    action: 'show_findings',
    owner: 'acme',
    repo: 'widgets',
    pr_number: 42,
  });
  // Button restored after dispatch settles.
  assert.equal(button.disabled, false);
  assert.equal(button.textContent, 'Show local findings');
  // List rendered into the body, and a truthful status line written.
  assert.deepEqual(findingsText(env.body, 'rr-findings-summary'), [
    '3 findings — 2 new, 1 dismissed',
  ]);
  assert.equal(env.statusNode.hidden, false);
  assert.match(env.statusNode.textContent, /Loaded 3 local findings\./);
});

test('handleShowFindings surfaces bridge failure guidance loudly, never silently', () => {
  const env = makeFindingsEnv();
  const { stub } = makeFindingsChromeStub({
    response: {
      ok: false,
      mode: 'bridge_cli_outcome_not_safe',
      action: 'show_findings',
      message: 'rr findings reported bridge-unsafe outcome.',
      guidance: 'Open Roger locally and rerun `rr findings` for authoritative details.',
    },
  });
  const button = { textContent: 'Show local findings', disabled: false };

  withPanelGlobals({ documentStub: env.documentStub, chromeStub: stub }, () => {
    handleShowFindings(TEST_CONTEXT, button);
  });

  // Body carries a loud error note with message + guidance and the caption.
  assert.deepEqual(findingsText(env.body, 'rr-findings-caption'), [FINDINGS_READONLY_CAPTION]);
  const errorNotes = findingsText(env.body, 'rr-findings-note--error');
  assert.equal(errorNotes.length, 1);
  assert.match(errorNotes[0], /rr findings reported bridge-unsafe outcome\./);
  assert.match(errorNotes[0], /Open Roger locally and rerun `rr findings`/);
  // Persistent status line mirrors the same failure as an error.
  assert.equal(env.statusNode.hidden, false);
  assert.equal(env.statusNode.classList.contains('roger-panel-status--error'), true);
  assert.match(env.statusNode.textContent, /rr findings reported bridge-unsafe outcome\./);
});

test('handleShowFindings renders the honest not-relayed note for a stripped-down bridge success', () => {
  const env = makeFindingsEnv();
  const { stub } = makeFindingsChromeStub({
    response: {
      ok: true,
      action: 'show_findings',
      session_id: 'session-42',
      attention_state: 'findings_ready',
      message: 'rr findings completed for acme/widgets#42',
    },
  });
  const button = { textContent: 'Show local findings', disabled: false };

  withPanelGlobals({ documentStub: env.documentStub, chromeStub: stub }, () => {
    handleShowFindings(TEST_CONTEXT, button);
  });

  const degradedNotes = findingsText(env.body, 'rr-findings-note--degraded');
  assert.equal(degradedNotes.length, 1);
  assert.match(
    degradedNotes[0],
    new RegExp(FINDINGS_NOT_RELAYED_MESSAGE.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
  );
  assert.equal(env.statusNode.hidden, false);
  assert.match(
    env.statusNode.textContent,
    new RegExp(FINDINGS_NOT_RELAYED_MESSAGE.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
  );
});

// ---------------------------------------------------------------------------
// Session-candidates surface (bead rr-ext-session-candidates-surface-pnv0).
// ---------------------------------------------------------------------------

const {
  SESSIONS_SECTION_ID,
  SESSIONS_NOTICE_ID,
  SESSIONS_LIST_ID,
  shortenSessionId,
  formatRelativeAge,
  resumeCommandForSession,
  copyTextToClipboard,
  createCopyCommandBlock,
  normalizeSessionEntry,
  normalizeSessionEntries,
  parseAutoSelectWarning,
  handleSessionResume,
  renderSessionRow,
  renderSessionListInto,
  renderSessionInventoryIntoPanel,
  setSessionsNotice,
  updateSessionsSectionVisibility,
  isPickerRequiredResponse,
  STATUS_MIRROR_POLL_INTERVAL_MS,
  startStatusPolling,
  stopStatusPolling,
  isStatusPolling,
  handleStatusPollVisibilityChange,
} = require('./main.js');

function makeToggleClassList() {
  const classes = new Set();
  return {
    classes,
    add(...names) {
      for (const name of names) classes.add(name);
    },
    remove(...names) {
      for (const name of names) classes.delete(name);
    },
    toggle(name, enabled) {
      if (enabled) {
        classes.add(name);
      } else {
        classes.delete(name);
      }
    },
    contains(name) {
      return classes.has(name);
    },
  };
}

// A recording element factory: unlike createTestElement it stores event
// listeners so click() can fire them, which lets the session/copy/findings
// interaction paths be exercised end to end.
function makeRecordingEl(tagName, registry) {
  const attributes = new Map();
  const listeners = {};
  const el = {
    tagName: String(tagName).toUpperCase(),
    id: '',
    className: '',
    textContent: '',
    hidden: false,
    disabled: false,
    children: [],
    parentElement: null,
    dataset: {},
    style: {},
    classList: makeToggleClassList(),
    appendChild(node) {
      if (!node) {
        return null;
      }
      this.children.push(node);
      node.parentElement = this;
      return node;
    },
    prepend(node) {
      if (!node) {
        return null;
      }
      this.children.unshift(node);
      node.parentElement = this;
      return node;
    },
    remove() {
      if (this.parentElement) {
        const idx = this.parentElement.children.indexOf(this);
        if (idx >= 0) {
          this.parentElement.children.splice(idx, 1);
        }
        this.parentElement = null;
      }
    },
    replaceChildren() {
      for (const child of this.children) {
        child.parentElement = null;
      }
      this.children = [];
    },
    select() {},
    addEventListener(type, fn) {
      (listeners[type] = listeners[type] || []).push(fn);
    },
    click() {
      for (const fn of listeners.click || []) {
        fn({});
      }
    },
    setAttribute(name, value) {
      attributes.set(name, String(value));
    },
    getAttribute(name) {
      return attributes.has(name) ? attributes.get(name) : null;
    },
    querySelectorAll(selector) {
      if (selector === 'button[data-action]') {
        return findNodes(this, (node) => node.dataset && typeof node.dataset.action === 'string', []);
      }
      return [];
    },
  };
  if (registry) {
    registry.push(el);
  }
  return el;
}

// Minimal live document backed by a registry so getElementById resolves any
// element by its assigned id, and createElement yields recording elements.
function makeLiveDocument() {
  const registry = [];
  const head = makeRecordingEl('head', registry);
  return {
    registry,
    head,
    createElement(tag) {
      return makeRecordingEl(tag, registry);
    },
    getElementById(id) {
      for (let i = registry.length - 1; i >= 0; i -= 1) {
        if (registry[i].id === id) {
          return registry[i];
        }
      }
      return null;
    },
  };
}

function findByClass(root, className) {
  return findNodes(
    root,
    (node) =>
      typeof node.className === 'string' && node.className.split(/\s+/).includes(className)
  );
}

test('shortenSessionId collapses long ids and passes short ids through', () => {
  assert.equal(shortenSessionId('session-1700000000-1234-7'), 'session-17…1234-7');
  assert.equal(shortenSessionId('sess-9'), 'sess-9');
  assert.equal(shortenSessionId(''), '');
});

test('formatRelativeAge renders coarse ages from unix seconds and ISO strings', () => {
  const now = 2_000_000_000_000; // fixed ms
  assert.equal(formatRelativeAge(now / 1000, now), 'just now');
  assert.equal(formatRelativeAge(now / 1000 - 300, now), '5m ago');
  assert.equal(formatRelativeAge(now / 1000 - 7200, now), '2h ago');
  assert.equal(formatRelativeAge(now / 1000 - 172800, now), '2d ago');
  assert.equal(formatRelativeAge(new Date(now - 3600 * 1000).toISOString(), now), '1h ago');
  assert.equal(formatRelativeAge(null, now), null);
});

test('resumeCommandForSession renders the canonical rr open handoff', () => {
  assert.equal(resumeCommandForSession('sess-x'), 'rr open --session sess-x');
});

test('normalizeSessionEntry accepts inventory and candidate shapes', () => {
  assert.equal(normalizeSessionEntry({ nope: true }), null);
  const inv = normalizeSessionEntry({
    session_id: 'a',
    provider: 'opencode',
    attention_state: 'awaiting_user_input',
    updated_at: 1000,
  });
  assert.equal(inv.session_id, 'a');
  assert.equal(inv.provider, 'opencode');
  const cand = normalizeSessionEntry({ session_id: 'b', pull_request: 42, updated_at: '2026-01-01T00:00:00Z' });
  assert.equal(cand.session_id, 'b');
  assert.equal(cand.pull_request, 42);
  assert.equal(normalizeSessionEntries([{ session_id: 'a' }, { junk: 1 }, { session_id: 'c' }]).length, 2);
});

test('parseAutoSelectWarning extracts the auto-selected session and candidate count', () => {
  const parsed = parseAutoSelectWarning([
    'provider is tier b',
    'auto-selected session sess-7 from 3 candidates (pr_rank=1, binding_rank=2)',
  ]);
  assert.deepEqual(parsed, { sessionId: 'sess-7', total: 3 });
  assert.equal(parseAutoSelectWarning(['nothing here']), null);
});

test('renderSessionRow builds id, meta, resume button, and a copy handoff', () => {
  const doc = makeLiveDocument();
  const row = renderSessionRow(
    { session_id: 'sess-abc', provider: 'opencode', attention_state: 'awaiting_user_input', updated_at: 1000 },
    TEST_CONTEXT,
    doc
  );
  const idNode = findByClass(row, 'rr-session-id')[0];
  assert.ok(idNode);
  assert.equal(idNode.getAttribute('title'), 'sess-abc');
  const resume = findByClass(row, 'rr-session-resume')[0];
  assert.ok(resume);
  assert.equal(resume.dataset.action, 'resume_review');
  assert.equal(resume.dataset.sessionId, 'sess-abc');
  const meta = findByClass(row, 'rr-session-meta')[0];
  assert.match(meta.textContent, /Opencode/);
  assert.match(meta.textContent, /Awaiting User Input/);
  const code = findByClass(row, 'rr-copy-command')[0];
  assert.equal(code.textContent, 'rr open --session sess-abc');
});

test('handleSessionResume dispatches resume carrying the session id', () => {
  let received = null;
  handleSessionResume(
    { session_id: 'sess-target' },
    TEST_CONTEXT,
    { textContent: 'Resume' },
    { onResume: (session) => { received = session; } }
  );
  assert.equal(received.session_id, 'sess-target');
});

test('renderSessionInventoryIntoPanel renders inventory rows and reveals the section', () => {
  const doc = makeLiveDocument();
  const section = doc.createElement('section');
  section.id = SESSIONS_SECTION_ID;
  section.hidden = true;
  const notice = doc.createElement('p');
  notice.id = SESSIONS_NOTICE_ID;
  notice.hidden = true;
  const list = doc.createElement('ul');
  list.id = SESSIONS_LIST_ID;

  const count = renderSessionInventoryIntoPanel(
    [
      { session_id: 'a', provider: 'opencode', updated_at: 1000 },
      { session_id: 'b', provider: 'codex', updated_at: 2000 },
    ],
    TEST_CONTEXT,
    { document: doc }
  );
  assert.equal(count, 2);
  assert.equal(list.children.length, 2);
  assert.equal(section.hidden, false);

  // Empty inventory clears rows; section hides again when no notice is set.
  renderSessionInventoryIntoPanel([], TEST_CONTEXT, { document: doc });
  assert.equal(list.children.length, 0);
  assert.equal(section.hidden, true);
});

test('setSessionsNotice keeps the section visible on notice alone', () => {
  const doc = makeLiveDocument();
  const section = doc.createElement('section');
  section.id = SESSIONS_SECTION_ID;
  section.hidden = true;
  const notice = doc.createElement('p');
  notice.id = SESSIONS_NOTICE_ID;
  notice.hidden = true;
  const list = doc.createElement('ul');
  list.id = SESSIONS_LIST_ID;

  setSessionsNotice('Roger auto-selected session s1 of 3 — choose another below', doc);
  assert.equal(notice.hidden, false);
  assert.match(notice.textContent, /of 3/);
  assert.equal(section.hidden, false);

  setSessionsNotice(null, doc);
  assert.equal(notice.hidden, true);
  assert.equal(section.hidden, true);
});

test('copyTextToClipboard prefers navigator.clipboard and reports success', async () => {
  let written = null;
  const ok = await copyTextToClipboard('rr open --session s1', {
    navigator: { clipboard: { writeText: (text) => { written = text; return Promise.resolve(); } } },
  });
  assert.equal(ok, true);
  assert.equal(written, 'rr open --session s1');
});

test('copyTextToClipboard falls back to execCommand when clipboard is unavailable', async () => {
  const doc = makeLiveDocument();
  const body = doc.createElement('body');
  doc.body = body;
  let copied = false;
  doc.execCommand = (cmd) => {
    copied = cmd === 'copy';
    return true;
  };
  const ok = await copyTextToClipboard('rr open --session s1', { navigator: {}, document: doc });
  assert.equal(ok, true);
  assert.equal(copied, true);
});

test('createCopyCommandBlock renders a code block and a Copy button that copies', async () => {
  const doc = makeLiveDocument();
  let written = null;
  const block = createCopyCommandBlock(doc, 'sess-copy', {
    navigator: { clipboard: { writeText: (text) => { written = text; return Promise.resolve(); } } },
    feedbackMs: 0,
  });
  const code = findByClass(block, 'rr-copy-command')[0];
  assert.equal(code.textContent, 'rr open --session sess-copy');
  const button = findByClass(block, 'rr-copy-button')[0];
  assert.ok(button);
  button.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(written, 'rr open --session sess-copy');
});

test('isPickerRequiredResponse recognizes the typed picker signal and candidates', () => {
  assert.equal(isPickerRequiredResponse({ failure_kind: 'picker_required' }), true);
  assert.equal(isPickerRequiredResponse({ mode: 'bridge_picker_required' }), true);
  assert.equal(isPickerRequiredResponse({ candidates: [] }), true);
  assert.equal(isPickerRequiredResponse({ ok: false, mode: 'native_unavailable' }), false);
});

// Build a panel DOM including the sessions section nodes so triggerLaunch /
// requestStatusMirror render paths can be exercised end to end.
function makeSessionsPanelDom() {
  const doc = makeLiveDocument();
  const panel = doc.createElement('section');
  panel.id = 'roger-reviewer-panel';
  const buttonRow = doc.createElement('div');
  for (const action of ['start_review', 'resume_review', 'show_findings']) {
    const b = doc.createElement('button');
    b.dataset.action = action;
    buttonRow.appendChild(b);
  }
  panel.appendChild(buttonRow);
  const status = doc.createElement('p');
  status.id = 'roger-reviewer-status';
  status.hidden = true;
  status.classList = createStatusClassList();
  status.parentElement = panel;
  const badge = doc.createElement('p');
  badge.id = 'roger-reviewer-attention-badge';
  badge.style = {};
  const info = doc.createElement('p');
  info.id = 'roger-reviewer-info-text';
  const section = doc.createElement('section');
  section.id = SESSIONS_SECTION_ID;
  section.hidden = true;
  const notice = doc.createElement('p');
  notice.id = SESSIONS_NOTICE_ID;
  notice.hidden = true;
  const list = doc.createElement('ul');
  list.id = SESSIONS_LIST_ID;
  return { doc, panel, status, badge, info, section, notice, list };
}

test('per-session resume dispatch carries session_id in the launch intent', () => {
  const dom = makeSessionsPanelDom();
  const sent = [];
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(payload, callback) {
        sent.push(payload);
        if (payload?.type === 'roger_bridge_launch') {
          callback({ ok: true, mode: 'native_messaging', attention_state: 'awaiting_user_input', session_id: 'sess-r', freshness_label: '1s old' });
        }
      },
    },
  };
  const button = { disabled: false, textContent: 'Resume' };
  withPanelGlobals({ documentStub: dom.doc, chromeStub }, () => {
    triggerLaunch('resume_review', TEST_CONTEXT, button, { sessionId: 'sess-explicit' });
  });
  const launch = sent.find((p) => p.type === 'roger_bridge_launch');
  assert.equal(launch.intent.session_id, 'sess-explicit');
  assert.equal(launch.intent.action, 'resume_review');
});

test('picker_required launch response renders candidates as resume rows, not an error', () => {
  const dom = makeSessionsPanelDom();
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(payload, callback) {
        if (payload?.type === 'roger_bridge_launch') {
          callback({
            ok: false,
            mode: 'bridge_picker_required',
            failure_kind: 'picker_required',
            action: 'resume_review',
            message: 'Multiple Roger review sessions match — choose one to resume.',
            candidates: [
              { session_id: 'cand-a', provider: 'opencode', attention_state: 'awaiting_user_input', updated_at: 1000 },
              { session_id: 'cand-b', provider: 'codex', attention_state: 'refresh_recommended', updated_at: 2000 },
            ],
          });
        }
      },
    },
  };
  const button = { disabled: false, textContent: 'Resume Existing Review' };
  withPanelGlobals({ documentStub: dom.doc, chromeStub }, () => {
    triggerLaunch('resume_review', TEST_CONTEXT, button);
  });
  assert.equal(dom.list.children.length, 2);
  assert.equal(dom.section.hidden, false);
  // Not rendered as an error.
  assert.equal(dom.status.classList.contains('roger-panel-status--error'), false);
  assert.match(dom.notice.textContent, /choose one to resume/i);
});

test('auto-selected resume renders a visible choose-another notice', () => {
  const dom = makeSessionsPanelDom();
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(payload, callback) {
        if (payload?.type === 'roger_bridge_launch') {
          callback({
            ok: true,
            mode: 'native_messaging',
            action: 'resume_review',
            attention_state: 'awaiting_user_input',
            freshness_label: '1s old',
            session_id: 'sess-1',
            auto_selected_session: true,
            warnings: ['auto-selected session sess-1 from 3 candidates (pr_rank=1)'],
          });
        }
        if (payload?.type === 'roger_bridge_status') {
          callback({
            ok: true,
            mode: 'session_inventory',
            session_count: 3,
            sessions: [
              { session_id: 'sess-1', provider: 'opencode', updated_at: 3000 },
              { session_id: 'sess-2', provider: 'codex', updated_at: 2000 },
              { session_id: 'sess-3', provider: 'gemini', updated_at: 1000 },
            ],
          });
        }
      },
    },
  };
  const button = { disabled: false, textContent: 'Resume Existing Review' };
  withPanelGlobals({ documentStub: dom.doc, chromeStub }, () => {
    triggerLaunch('resume_review', TEST_CONTEXT, button);
  });
  assert.equal(dom.notice.hidden, false);
  assert.match(dom.notice.textContent, /auto-selected session sess-1 of 3/);
  // The inventory refresh populated the candidate list to choose from.
  assert.equal(dom.list.children.length, 3);
});

test('bounded status mirror renders the session inventory list', () => {
  const dom = makeSessionsPanelDom();
  const chromeStub = {
    runtime: {
      lastError: null,
      sendMessage(payload, callback) {
        if (payload?.type === 'roger_bridge_status') {
          callback({
            ok: true,
            mode: 'session_inventory',
            session_count: 2,
            sessions: [
              { session_id: 'inv-a', provider: 'opencode', updated_at: 1000 },
              { session_id: 'inv-b', provider: 'codex', updated_at: 2000 },
            ],
          });
        }
      },
    },
  };
  withPanelGlobals({ documentStub: dom.doc, chromeStub }, () => {
    requestStatusMirror(TEST_CONTEXT);
  });
  assert.equal(dom.list.children.length, 2);
  assert.equal(dom.section.hidden, false);
});

test('createPanel wires the primary View Findings action to the findings staging path', () => {
  const doc = makeLiveDocument();
  createPanel(TEST_CONTEXT, doc);
  const findingsButton = doc.registry.find(
    (el) => el.dataset && el.dataset.action === 'show_findings'
  );
  assert.ok(findingsButton);
  const chromeStub = makeFindingsChromeStub({ response: RELAYED_FINDINGS_RESPONSE });
  withPanelGlobals({ documentStub: doc, chromeStub: chromeStub.stub }, () => {
    findingsButton.click();
  });
  const body = doc.getElementById(FINDINGS_BODY_ID);
  const captions = findByClass(body, 'rr-findings-caption');
  assert.equal(captions.length, 1);
  assert.equal(captions[0].textContent, FINDINGS_READONLY_CAPTION);
  const summaries = findByClass(body, 'rr-findings-summary');
  assert.match(summaries[0].textContent, /3 findings/);
});

test('status re-poll starts while visible and stops when hidden', () => {
  const created = [];
  const cleared = [];
  const fakeSet = (fn) => {
    created.push(fn);
    return created.length;
  };
  const fakeClear = (id) => {
    cleared.push(id);
  };
  const visibleDoc = { visibilityState: 'visible' };
  const opts = { document: visibleDoc, setInterval: fakeSet, clearInterval: fakeClear, onTick: () => {} };

  startStatusPolling(TEST_CONTEXT, opts);
  assert.equal(isStatusPolling(), true);
  assert.equal(STATUS_MIRROR_POLL_INTERVAL_MS, 45000);

  handleStatusPollVisibilityChange({ document: { visibilityState: 'hidden' }, clearInterval: fakeClear });
  assert.equal(isStatusPolling(), false);
  assert.ok(cleared.length >= 1);

  // Restart on becoming visible again.
  handleStatusPollVisibilityChange(opts);
  assert.equal(isStatusPolling(), true);

  stopStatusPolling();
  assert.equal(isStatusPolling(), false);
});

test('status re-poll does not start while the tab is hidden', () => {
  const fakeSet = () => 1;
  const started = startStatusPolling(TEST_CONTEXT, {
    document: { visibilityState: 'hidden' },
    setInterval: fakeSet,
    clearInterval: () => {},
  });
  assert.equal(started, null);
  assert.equal(isStatusPolling(), false);
});

// ---------------------------------------------------------------------------
// Bounded local-parity surfaces (bead rr-surface-parity-epic-rfa2.3):
// triage buttons, clarification, draft edit-as-revision, search + timeline
// mirrors, and the approve/post HANDOFF (never a button).
// ---------------------------------------------------------------------------

function makeParityEnv() {
  const findingsBody = createTestElement('div');
  findingsBody.id = FINDINGS_BODY_ID;
  const draftsBody = createTestElement('div');
  draftsBody.id = DRAFTS_BODY_ID;
  const searchBody = createTestElement('div');
  searchBody.id = SEARCH_BODY_ID;
  const timelineBody = createTestElement('div');
  timelineBody.id = TIMELINE_BODY_ID;
  const statusNode = {
    textContent: '',
    hidden: true,
    parentElement: { classList: { contains: () => false } },
    classList: createStatusClassList(),
  };
  const documentStub = {
    createElement: (tag) => createTestElement(tag),
    getElementById(id) {
      if (id === FINDINGS_BODY_ID) return findingsBody;
      if (id === DRAFTS_BODY_ID) return draftsBody;
      if (id === SEARCH_BODY_ID) return searchBody;
      if (id === TIMELINE_BODY_ID) return timelineBody;
      if (id === 'roger-reviewer-status') return statusNode;
      return null;
    },
  };
  return { findingsBody, draftsBody, searchBody, timelineBody, statusNode, documentStub };
}

function nodesByClass(root, cls) {
  return findNodes(
    root,
    (node) => typeof node.className === 'string' && node.className.split(/\s+/).includes(cls)
  );
}

function textsByClass(root, cls) {
  return nodesByClass(root, cls).map((node) => node.textContent);
}

function buttonTexts(root) {
  return findNodes(root, (node) => node.tagName === 'BUTTON').map((node) => node.textContent);
}

const DRAFTED_ITEM_RAW = {
  finding_id: 'f2',
  title: 'Missing auth check on delete route',
  triage_state: 'confirmed',
  outbound_state: 'awaiting_approval',
  outbound_detail: { draft_id: 'draft-2', draft_batch_id: 'batch-9' },
  summary: 'The delete route skips the auth guard.',
  evidence_count: 1,
};

test('normalizeFindingsItem lifts outbound_detail draft_id + draft_batch_id', () => {
  const item = normalizeFindingsItem(DRAFTED_ITEM_RAW);
  assert.equal(item.draft_id, 'draft-2');
  assert.equal(item.draft_batch_id, 'batch-9');
  assert.equal(item.outbound_state, 'awaiting_approval');
});

test('createFindingItemNode renders elevated triage buttons + clarification + draft edit when context is provided', () => {
  const { documentStub } = makeParityEnv();
  const item = normalizeFindingsItem(DRAFTED_ITEM_RAW);
  const node = createFindingItemNode(item, documentStub, TEST_CONTEXT);

  const triageLabels = textsByClass(node, 'rr-triage-button');
  assert.deepEqual(triageLabels, ['Accept', 'Ignore', 'Needs follow-up', 'Resolve']);
  assert.equal(nodesByClass(node, 'rr-clarify-input').length, 1);
  assert.equal(textsByClass(node, 'rr-clarify-button')[0], 'Request clarification');
  assert.equal(nodesByClass(node, 'rr-draft-edit-input').length, 1);
  assert.equal(textsByClass(node, 'rr-draft-edit-save')[0], 'Save revision');
});

test('createFindingItemNode renders the approve HANDOFF command, never an approve/post button', () => {
  const { documentStub } = makeParityEnv();
  const item = normalizeFindingsItem(DRAFTED_ITEM_RAW);
  const node = createFindingItemNode(item, documentStub, TEST_CONTEXT);

  const commands = textsByClass(node, 'rr-copy-command');
  assert.ok(
    commands.includes('rr send approve --batch batch-9'),
    `expected copyable approve handoff, got ${JSON.stringify(commands)}`
  );
  // Deliberate asymmetry 1: no button may approve or post from the extension.
  const buttons = buttonTexts(node);
  assert.ok(
    !buttons.some((text) => /approve|post/i.test(String(text))),
    `no approve/post button may exist, got ${JSON.stringify(buttons)}`
  );
});

test('createFindingItemNode omits all mutation controls in the read-only mirror (no context)', () => {
  const { documentStub } = makeParityEnv();
  const item = normalizeFindingsItem(DRAFTED_ITEM_RAW);
  const node = createFindingItemNode(item, documentStub);
  assert.equal(nodesByClass(node, 'rr-triage-button').length, 0);
  assert.equal(nodesByClass(node, 'rr-clarify-input').length, 0);
  assert.equal(nodesByClass(node, 'rr-draft-edit-input').length, 0);
  assert.equal(nodesByClass(node, 'rr-copy-command').length, 0);
});

test('handleTriageFinding dispatches triage_finding then re-fetches findings', () => {
  const { documentStub } = makeParityEnv();
  const { stub, sent } = makeFindingsChromeStub({
    response: { ok: true, action: 'triage_finding', message: 'triaged', findings: { count: 0, items: [] } },
  });
  const button = { textContent: 'Accept', disabled: false };
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleTriageFinding(TEST_CONTEXT, 'f2', 'accepted', button);
  });

  assert.equal(sent[0].type, 'roger_bridge_launch');
  assert.deepEqual(sent[0].intent, {
    action: 'triage_finding',
    owner: 'acme',
    repo: 'widgets',
    pr_number: 42,
    finding_id: 'f2',
    state: 'accepted',
  });
  // Success re-fetches the findings mirror through the same launch path.
  assert.equal(sent.length, 2);
  assert.equal(sent[1].intent.action, 'show_findings');
  assert.equal(button.disabled, false);
});

test('handleReviseDraft dispatches revise_draft with draft_id + body and never posts', () => {
  const { documentStub } = makeParityEnv();
  const { stub, sent } = makeFindingsChromeStub({
    response: { ok: true, action: 'revise_draft', message: 'rr send edit recorded revision 2' },
  });
  const button = { textContent: 'Save revision', disabled: false };
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleReviseDraft(TEST_CONTEXT, 'draft-2', 'revised body text', button);
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].intent, {
    action: 'revise_draft',
    owner: 'acme',
    repo: 'widgets',
    pr_number: 42,
    draft_id: 'draft-2',
    body: 'revised body text',
  });
});

test('handleReviseDraft blocks an empty body without dispatching', () => {
  const { documentStub } = makeParityEnv();
  const { stub, sent } = makeFindingsChromeStub({ response: { ok: true } });
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleReviseDraft(TEST_CONTEXT, 'draft-2', '   ', null);
  });
  assert.equal(sent.length, 0);
});

test('handleRequestClarification dispatches request_clarification; empty body is blocked', () => {
  const { documentStub } = makeParityEnv();
  const { stub, sent } = makeFindingsChromeStub({
    response: { ok: true, action: 'request_clarification', message: 'recorded', clarification_ack: { clarification_id: 'c1' } },
  });
  const button = { textContent: 'Request clarification', disabled: false };
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleRequestClarification(TEST_CONTEXT, 'f2', '', button);
  });
  assert.equal(sent.length, 0, 'empty clarification must not dispatch');

  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleRequestClarification(TEST_CONTEXT, 'f2', 'why is this unsafe?', button);
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].intent, {
    action: 'request_clarification',
    owner: 'acme',
    repo: 'widgets',
    pr_number: 42,
    finding_id: 'f2',
    body: 'why is this unsafe?',
  });
});

test('renderDraftsStaging renders draft cards with edit controls + approve handoff, no approve button', () => {
  const { draftsBody, documentStub } = makeParityEnv();
  const response = {
    ok: true,
    drafts: { count: 1, items: [DRAFTED_ITEM_RAW], warnings: [] },
  };
  const payload = renderDraftsStaging(draftsBody, response, documentStub, TEST_CONTEXT);
  assert.equal(payload.items.length, 1);
  assert.equal(textsByClass(draftsBody, 'rr-drafts-item-title')[0], 'Missing auth check on delete route');
  assert.equal(nodesByClass(draftsBody, 'rr-draft-edit-input').length, 1);
  assert.ok(textsByClass(draftsBody, 'rr-copy-command').includes('rr send approve --batch batch-9'));
  assert.ok(!buttonTexts(draftsBody).some((text) => /approve|post/i.test(String(text))));
});

test('normalizeDraftsPayload drops findings that were never drafted', () => {
  const payload = normalizeDraftsPayload({
    drafts: {
      count: 2,
      items: [
        DRAFTED_ITEM_RAW,
        { finding_id: 'f1', title: 'Not drafted', outbound_state: 'not_drafted' },
      ],
    },
  });
  assert.equal(payload.items.length, 1);
  assert.equal(payload.items[0].finding_id, 'f2');
});

test('handleShowDrafts dispatches show_drafts and renders the drafts body', () => {
  const { documentStub } = makeParityEnv();
  const { stub, sent } = makeFindingsChromeStub({
    response: { ok: true, action: 'show_drafts', drafts: { count: 1, items: [DRAFTED_ITEM_RAW] } },
  });
  const button = { textContent: 'Show local drafts', disabled: false };
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleShowDrafts(TEST_CONTEXT, button);
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].intent, { action: 'show_drafts', owner: 'acme', repo: 'widgets', pr_number: 42 });
});

test('renderSearchResults renders relayed prior-review matches', () => {
  const { searchBody, documentStub } = makeParityEnv();
  const result = renderSearchResults(
    searchBody,
    { ok: true, search_results: { count: 2, matches: [{ title: 'prior finding A' }, { title: 'prior finding B' }] } },
    documentStub
  );
  assert.equal(result.items.length, 2);
  assert.deepEqual(textsByClass(searchBody, 'rr-search-item-title'), ['prior finding A', 'prior finding B']);
});

test('handleSearch dispatches search with the query; empty query is blocked', () => {
  const { documentStub } = makeParityEnv();
  const { stub, sent } = makeFindingsChromeStub({
    response: { ok: true, action: 'search', search_results: { count: 0, matches: [] } },
  });
  const button = { textContent: 'Search', disabled: false };
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleSearch(TEST_CONTEXT, '   ', button);
  });
  assert.equal(sent.length, 0, 'empty query must not dispatch');
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleSearch(TEST_CONTEXT, 'retry loop', button);
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].intent, {
    action: 'search',
    owner: 'acme',
    repo: 'widgets',
    pr_number: 42,
    query: 'retry loop',
  });
});

test('renderTimeline renders relayed events and handleTimeline dispatches timeline', () => {
  const { timelineBody, documentStub } = makeParityEnv();
  const result = renderTimeline(
    timelineBody,
    { ok: true, timeline: { events: [{ kind: 'run', at: '2026-07-09T00:00:00Z' }, { kind: 'draft' }] } },
    documentStub
  );
  assert.equal(result.items.length, 2);
  assert.equal(nodesByClass(timelineBody, 'rr-timeline-item').length, 2);

  const { stub, sent } = makeFindingsChromeStub({
    response: { ok: true, action: 'timeline', timeline: { events: [] } },
  });
  const button = { textContent: 'Show review timeline', disabled: false };
  withPanelGlobals({ documentStub, chromeStub: stub }, () => {
    handleTimeline(TEST_CONTEXT, button);
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].intent, { action: 'timeline', owner: 'acme', repo: 'widgets', pr_number: 42 });
});

test('createDraftsSection / createSearchSection / createTimelineSection build id-bearing sections', () => {
  const { documentStub } = makeParityEnv();
  const drafts = createDraftsSection(TEST_CONTEXT, documentStub);
  assert.equal(drafts.id, 'roger-reviewer-drafts');
  assert.ok(findNodes(drafts, (n) => n.id === DRAFTS_BODY_ID).length === 1);

  const search = createSearchSection(TEST_CONTEXT, documentStub);
  assert.equal(search.id, 'roger-reviewer-search');
  assert.ok(findNodes(search, (n) => n.id === SEARCH_BODY_ID).length === 1);

  const timeline = createTimelineSection(TEST_CONTEXT, documentStub);
  assert.equal(timeline.id, 'roger-reviewer-timeline');
  assert.ok(findNodes(timeline, (n) => n.id === TIMELINE_BODY_ID).length === 1);
});
