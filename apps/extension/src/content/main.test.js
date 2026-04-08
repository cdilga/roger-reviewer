const test = require('node:test');
const assert = require('node:assert/strict');

const {
  BRAND_CHIP_CLASS,
  GITHUB_ACTION_BUTTON_CLASS,
  MODAL_FALLBACK_STATUS,
  MODAL_OPEN_BUTTON_LABEL,
  applyActionModel,
  applyPanelModeStyles,
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
    children: [],
    parentElement: null,
    dataset: {},
    style: {},
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

test('deriveActionModel hides Refresh by default and promotes Start as primary', () => {
  const model = deriveActionModel(null);
  assert.equal(model.primaryActionId, 'start_review');
  assert.equal(model.visibleActions.has('start_review'), true);
  assert.equal(model.visibleActions.has('resume_review'), true);
  assert.equal(model.visibleActions.has('show_findings'), true);
  assert.equal(model.visibleActions.has('refresh_review'), false);
});

test('deriveActionModel exposes Refresh and promotes it when refresh is recommended', () => {
  const model = deriveActionModel('refresh_recommended');
  assert.equal(model.primaryActionId, 'refresh_review');
  assert.equal(model.visibleActions.has('refresh_review'), true);
});

test('deriveActionModel maps canonical attention states to expected primary actions', () => {
  const scenarios = [
    ['awaiting_user_input', 'resume_review', false],
    ['review_failed', 'resume_review', false],
    ['findings_ready', 'show_findings', false],
    ['awaiting_outbound_approval', 'show_findings', false],
    ['refresh_recommended', 'refresh_review', true],
  ];

  for (const [attentionState, expectedPrimary, refreshVisible] of scenarios) {
    const model = deriveActionModel(attentionState);
    assert.equal(model.primaryActionId, expectedPrimary);
    assert.equal(model.visibleActions.has('refresh_review'), refreshVisible);
    assert.equal(model.visibleActions.has('start_review'), true);
    assert.equal(model.visibleActions.has('resume_review'), true);
    assert.equal(model.visibleActions.has('show_findings'), true);
  }
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
    makeButton('refresh_review'),
  ];
  const panel = {
    querySelectorAll(selector) {
      return selector === 'button[data-action]' ? buttons : [];
    },
  };

  applyActionModel(panel, null);
  assert.equal(buttons[0].hidden, false);
  assert.equal(buttons[0].hasClass('roger-panel-button--primary'), true);
  assert.equal(buttons[3].hidden, true);

  applyActionModel(panel, 'refresh_recommended');
  assert.equal(buttons[3].hidden, false);
  assert.equal(buttons[3].hasClass('roger-panel-button--primary'), true);
  assert.equal(buttonStates.get('refresh_review:aria-hidden'), 'false');
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
  assert.equal(chip.textContent, 'Roger');
  assert.equal(chip.getAttribute('aria-label'), 'Roger identity');
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
  assert.equal(actionButtons.length, 4);
  const actionIds = actionButtons.map((button) => button.dataset.action).sort();
  assert.deepEqual(actionIds, ['refresh_review', 'resume_review', 'show_findings', 'start_review']);
  assert.equal(actionIds.includes('approve_outbound'), false);
  assert.equal(actionIds.includes('post_review'), false);
  for (const button of actionButtons) {
    assert.equal(button.className, GITHUB_ACTION_BUTTON_CLASS);
  }
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
    assert.equal(classes.has('roger-panel-status--ok'), true);
    assert.equal(classes.has('roger-panel-status--error'), false);

    setStatus('Fallback-only status', true);
    assert.equal(statusNode.textContent, 'Fallback-only status');
    assert.equal(classes.has('roger-panel-status--ok'), false);
    assert.equal(classes.has('roger-panel-status--error'), true);

    setStatus('Launch blocked', true, { revealInline: true });
    assert.equal(classes.has('roger-panel-status--inline-visible'), true);
  } finally {
    global.document = originalDocument;
  }
});
