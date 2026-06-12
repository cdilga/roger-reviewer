const test = require('node:test');
const assert = require('node:assert/strict');

const {
  appendGuidance,
  BRAND_CHIP_CLASS,
  BRIDGE_DISCONNECT_GUIDANCE,
  formatLaunchSuccessStatus,
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
  pickInlineAnchorSelector,
  resolvePanelPlacement,
  setStatus,
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
  assert.equal(chip.children.length, 1);
  assert.equal(chip.children[0].textContent, 'Roger');
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
  global.setTimeout = (...args) => {
    scheduled.push(args);
    return 0;
  };

  try {
    withPanelGlobals({ documentStub: dom.documentStub, chromeStub }, () => {
      triggerLaunch('start_review', TEST_CONTEXT, button);
    });
  } finally {
    global.setTimeout = previousSetTimeout;
  }

  assert.equal(dom.statusNode.hidden, false);
  assert.equal(dom.statusNode.classList.contains('roger-panel-status--inline-visible'), true);
  assert.equal(
    scheduled.length,
    0,
    'launch status must stay visible until the next action, never auto-clear on a timer'
  );
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
