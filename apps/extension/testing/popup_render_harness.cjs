#!/usr/bin/env node

const fs = require('node:fs');
const path = require('node:path');
const { JSDOM } = require('jsdom');

const REPO_ROOT = path.resolve(__dirname, '..', '..', '..');
const POPUP_DIR = path.join(REPO_ROOT, 'apps', 'extension', 'src', 'popup');
const STATIC_DIR = path.join(REPO_ROOT, 'apps', 'extension', 'static');
const POPUP_HTML_PATH = path.join(POPUP_DIR, 'index.html');
const POPUP_MAIN_PATH = path.join(POPUP_DIR, 'main.js');
const CONTENT_MAIN_PATH = path.join(REPO_ROOT, 'apps', 'extension', 'src', 'content', 'main.js');
const DEFAULT_OUTPUT_DIR = path.join(REPO_ROOT, 'apps', 'extension', 'testing', 'artifacts', 'latest');
const PANEL_ID = 'roger-reviewer-panel';
const PANEL_HEADING_ID = 'roger-reviewer-heading';
const PANEL_SUBHEADING_ID = 'roger-reviewer-subheading';
const PANEL_BADGE_ID = 'roger-reviewer-attention-badge';
const PANEL_INFO_ID = 'roger-reviewer-info-text';
// No dedicated build-label element exists today; the build label is exposed
// through the info-toggle title, which collectPanelSummary falls back to.
const PANEL_BUILD_ID = 'roger-reviewer-build-label';
const PANEL_STATUS_ID = 'roger-reviewer-status';
const PANEL_INLINE_SLOT_ID = 'roger-reviewer-inline-slot';
const PANEL_RAIL_SLOT_ID = 'roger-reviewer-rail-slot';
const PANEL_MODAL_SLOT_ID = 'roger-reviewer-modal-slot';

const PR_URL = 'https://github.com/rust-lang/rust/pull/155408';

const SHARED_BUILD_LABEL = '0.1.0-dev+render-harness';
const SHARED_LAUNCH_RESPONSE = {
  ok: true,
  mode: 'native_messaging',
  message: 'Launch simulation dispatched for rust-lang/rust#155408.',
  attention_state: null,
};

const POPUP_SCENARIOS = {
  'non-pr': {
    activeTabUrl: 'https://github.com/rust-lang/rust/issues/155408',
    buildLabel: SHARED_BUILD_LABEL,
    statusResponse: null,
    launchResponse: {
      ok: true,
      mode: 'native_messaging',
      message: 'Launch simulation unavailable on non-PR tabs.',
      attention_state: null,
      finding_count: 0,
    },
  },
  'pr-idle': {
    activeTabUrl: PR_URL,
    buildLabel: SHARED_BUILD_LABEL,
    statusResponse: {
      ok: true,
      attention_state: null,
      finding_count: 0,
    },
    launchResponse: {
      ...SHARED_LAUNCH_RESPONSE,
      finding_count: 0,
    },
  },
  'pr-awaiting-approval': {
    activeTabUrl: PR_URL,
    buildLabel: SHARED_BUILD_LABEL,
    statusResponse: {
      ok: true,
      attention_state: 'awaiting_outbound_approval',
      finding_count: 0,
    },
    launchResponse: {
      ...SHARED_LAUNCH_RESPONSE,
      attention_state: 'awaiting_outbound_approval',
      freshness_label: 'fresh',
      message: 'Awaiting outbound approval in Roger.',
    },
  },
  'pr-findings-ready': {
    activeTabUrl: PR_URL,
    buildLabel: SHARED_BUILD_LABEL,
    statusResponse: {
      ok: true,
      attention_state: 'findings_ready',
      finding_count: 3,
      has_findings: true,
    },
    launchResponse: {
      ...SHARED_LAUNCH_RESPONSE,
      attention_state: 'findings_ready',
      finding_count: 3,
      has_findings: true,
      freshness_label: 'fresh',
      message: 'Findings ready in Roger.',
    },
  },
};

const PANEL_SCENARIOS = {
  'pr-dark-rail': {
    activeTabUrl: PR_URL,
    buildLabel: SHARED_BUILD_LABEL,
    hostTheme: 'dark',
    placement: 'rail',
    statusResponse: {
      ok: true,
      mode: 'bounded_status',
      attention_state: null,
      message: 'Launch-only mode. Open Roger locally for authoritative detail.',
      finding_count: 0,
    },
    launchResponse: {
      ...SHARED_LAUNCH_RESPONSE,
      message: 'Start Review simulated for rust-lang/rust#155408.',
      attention_state: null,
    },
  },
  'pr-dark-awaiting-approval': {
    activeTabUrl: PR_URL,
    buildLabel: SHARED_BUILD_LABEL,
    hostTheme: 'dark',
    placement: 'rail',
    statusResponse: {
      ok: true,
      mode: 'bounded_status',
      attention_state: 'awaiting_outbound_approval',
      freshness_label: 'fresh',
      message: 'Awaiting outbound approval in Roger.',
      finding_count: 0,
    },
    launchResponse: {
      ...SHARED_LAUNCH_RESPONSE,
      attention_state: 'awaiting_outbound_approval',
      freshness_label: 'fresh',
      message: 'Resume review simulated for rust-lang/rust#155408.',
    },
  },
  'pr-dark-findings-ready': {
    activeTabUrl: PR_URL,
    buildLabel: SHARED_BUILD_LABEL,
    hostTheme: 'dark',
    placement: 'rail',
    statusResponse: {
      ok: true,
      mode: 'bounded_status',
      attention_state: 'findings_ready',
      freshness_label: 'fresh',
      message: 'Findings ready in Roger.',
      finding_count: 3,
      has_findings: true,
    },
    launchResponse: {
      ...SHARED_LAUNCH_RESPONSE,
      attention_state: 'findings_ready',
      freshness_label: 'fresh',
      message: 'Viewing findings for rust-lang/rust#155408.',
      finding_count: 3,
      has_findings: true,
    },
  },
  'pr-dark-inline': {
    activeTabUrl: PR_URL,
    buildLabel: SHARED_BUILD_LABEL,
    hostTheme: 'dark',
    placement: 'inline',
    statusResponse: {
      ok: true,
      mode: 'bounded_status',
      attention_state: null,
      message: 'Launch-only mode. Open Roger locally for authoritative detail.',
      finding_count: 0,
    },
    launchResponse: {
      ...SHARED_LAUNCH_RESPONSE,
    },
  },
};

const SURFACES = {
  panel: {
    defaultScenario: 'pr-dark-rail',
    scenarios: PANEL_SCENARIOS,
  },
  popup: {
    defaultScenario: 'pr-idle',
    scenarios: POPUP_SCENARIOS,
  },
};

function cloneJson(value) {
  if (value === null || value === undefined) {
    return value;
  }
  return JSON.parse(JSON.stringify(value));
}

function listSurfaceScenarios(surface) {
  const surfaceConfig = SURFACES[surface];
  if (!surfaceConfig) {
    throw new Error(`Unknown extension render harness surface "${surface}".`);
  }
  return Object.keys(surfaceConfig.scenarios);
}

function assertScenarioExists(surface, name) {
  const surfaceConfig = SURFACES[surface];
  if (!surfaceConfig) {
    throw new Error(`Unknown extension render harness surface "${surface}".`);
  }
  if (!Object.prototype.hasOwnProperty.call(surfaceConfig.scenarios, name)) {
    throw new Error(
      `Unknown ${surface} render harness scenario "${name}". Use --list-scenarios to inspect valid names.`
    );
  }
}

function dataUriForAsset(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  let mimeType = 'application/octet-stream';
  if (ext === '.svg') {
    mimeType = 'image/svg+xml';
  } else if (ext === '.png') {
    mimeType = 'image/png';
  }

  const raw = fs.readFileSync(filePath);
  return `data:${mimeType};base64,${raw.toString('base64')}`;
}

const RUNTIME_ASSET_URIS = new Map([
  ['static/roger-mark.svg', dataUriForAsset(path.join(STATIC_DIR, 'roger-mark.svg'))],
  ['static/roger-wordmark.svg', dataUriForAsset(path.join(STATIC_DIR, 'roger-wordmark.svg'))],
]);

function rewriteStaticAssets(html) {
  const sharedIdentityCss = fs.readFileSync(path.join(STATIC_DIR, 'roger-identity.css'), 'utf8');
  let nextHtml = html.replace(
    /<link rel="stylesheet" href="\.\.\/\.\.\/static\/roger-identity\.css" \/>/u,
    `<style data-harness-source="roger-identity.css">\n${sharedIdentityCss}\n</style>`
  );

  nextHtml = nextHtml.replace(/\s*<script src="\.\/main\.js"><\/script>\s*/u, '\n');

  for (const [assetPath, dataUri] of RUNTIME_ASSET_URIS.entries()) {
    const relativeAsset = `../../${assetPath}`;
    nextHtml = nextHtml.split(relativeAsset).join(dataUri);
  }

  return nextHtml;
}

function loadPopupHtml() {
  return rewriteStaticAssets(fs.readFileSync(POPUP_HTML_PATH, 'utf8'));
}

function loadPopupMainSource() {
  return fs.readFileSync(POPUP_MAIN_PATH, 'utf8');
}

function buildPanelHostHtml(scenario) {
  const rootTheme = scenario.hostTheme === 'dark' ? 'dark' : 'light';
  const bodyAttributes = `data-color-mode="${rootTheme}" data-light-theme="light" data-dark-theme="dark_dimmed"`;
  const themeVariables =
    rootTheme === 'dark'
      ? `
          :root {
            color-scheme: dark;
            --bgColor-default: #0d1117;
            --bgColor-muted: #161b22;
            --bgColor-emphasis: #30363d;
            --overlay-bgColor: #161b22;
            --fgColor-default: #e6edf3;
            --fgColor-muted: #8b949e;
            --fgColor-danger: #ff7b72;
            --fgColor-onEmphasis: #ffffff;
            --fgColor-success: #3fb950;
            --fgColor-accent: #58a6ff;
            --borderColor-default: #30363d;
            --borderColor-muted: #21262d;
            --borderColor-emphasis: #6e7681;
            --button-default-bgColor-rest: #212830;
            --button-default-bgColor-hover: #30363d;
            --button-default-bgColor-disabled: #161b22;
            --button-default-borderColor-rest: #30363d;
            --button-default-borderColor-hover: #8b949e;
            --button-default-fgColor-rest: #e6edf3;
            --bgColor-danger-emphasis: #da3633;
            --bgColor-success-muted: rgba(63, 185, 80, 0.18);
            --bgColor-accent-muted: rgba(56, 139, 253, 0.18);
            --bgColor-attention-muted: rgba(210, 153, 34, 0.22);
          }
        `
      : `
          :root {
            color-scheme: light;
            --bgColor-default: #ffffff;
            --bgColor-muted: #f6f8fa;
            --bgColor-emphasis: #eaeef2;
            --overlay-bgColor: #ffffff;
            --fgColor-default: #1f2328;
            --fgColor-muted: #656d76;
            --fgColor-danger: #cf222e;
            --fgColor-onEmphasis: #ffffff;
            --fgColor-success: #1a7f37;
            --fgColor-accent: #0969da;
            --borderColor-default: #d0d7de;
            --borderColor-muted: #d8dee4;
            --borderColor-emphasis: #8c959f;
            --button-default-bgColor-rest: #ffffff;
            --button-default-bgColor-hover: #f3f4f6;
            --button-default-bgColor-disabled: #f6f8fa;
            --button-default-borderColor-rest: #d0d7de;
            --button-default-borderColor-hover: #8c959f;
            --button-default-fgColor-rest: #1f2328;
            --bgColor-danger-emphasis: #cf222e;
            --bgColor-success-muted: #dafbe1;
            --bgColor-accent-muted: #ddf4ff;
            --bgColor-attention-muted: #fff8c5;
          }
        `;

  const headerActions =
    scenario.placement === 'inline'
      ? `
          <div class="gh-header-actions">
            <button class="host-button">Ready for review</button>
            <button class="host-button">Review changes</button>
            <button class="host-button host-button--accent">Merge</button>
          </div>
        `
      : '';

  const sidebar =
    scenario.placement === 'rail'
      ? `
          <aside class="discussion-sidebar">
            <section aria-label="Reviewers" class="reviewers-card">
              <h2>Reviewers</h2>
              <p>lolbinarycat</p>
            </section>
            <section class="reviewers-card">
              <h2>Assignees</h2>
              <p>lolbinarycat</p>
            </section>
          </aside>
        `
      : '';

  return `
    <!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>Roger Panel Render Harness</title>
        <style>
          ${themeVariables}

          * {
            box-sizing: border-box;
          }

          body {
            margin: 0;
            min-height: 100vh;
            background:
              radial-gradient(circle at top left, rgba(88, 166, 255, 0.08), transparent 28%),
              linear-gradient(180deg, rgba(13, 17, 23, 0.98), rgba(17, 22, 28, 1));
            color: var(--fgColor-default);
            font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
          }

          .page-shell {
            max-width: 1280px;
            margin: 0 auto;
            padding: 32px 24px 56px;
            display: grid;
            grid-template-columns: minmax(0, 1fr) 320px;
            gap: 24px;
          }

          .page-shell--inline {
            grid-template-columns: minmax(0, 1fr);
          }

          .timeline-card,
          .discussion-sidebar,
          .reviewers-card {
            border: 1px solid var(--borderColor-default);
            border-radius: 12px;
            background: rgba(22, 27, 34, 0.86);
            box-shadow: 0 10px 24px rgba(0, 0, 0, 0.18);
          }

          .timeline-card {
            padding: 24px;
          }

          .discussion-sidebar {
            padding: 16px;
            display: grid;
            gap: 16px;
            align-self: start;
          }

          .reviewers-card {
            padding: 16px;
          }

          .reviewers-card h2,
          .timeline-card h1 {
            margin: 0 0 8px;
            font-size: 18px;
            line-height: 1.3;
          }

          .reviewers-card p,
          .timeline-card p {
            margin: 0;
            color: var(--fgColor-muted);
            line-height: 1.5;
          }

          .gh-header-actions {
            display: flex;
            justify-content: flex-end;
            gap: 8px;
            margin: 0 0 16px;
          }

          .host-button {
            border: 1px solid var(--borderColor-default);
            border-radius: 6px;
            padding: 6px 12px;
            background: #212830;
            color: var(--fgColor-default);
            font-size: 12px;
            line-height: 1.25;
            font-weight: 600;
          }

          .host-button--accent {
            background: #238636;
            border-color: #2ea043;
            color: #ffffff;
          }
        </style>
      </head>
      <body ${bodyAttributes}>
        <div class="page-shell ${scenario.placement === 'inline' ? 'page-shell--inline' : ''}">
          <main class="timeline-card">
            ${headerActions}
            <h1>Roger Reviewer Visual Harness</h1>
            <p>Dark GitHub-like host scaffold for local extension panel inspection.</p>
          </main>
          ${sidebar}
        </div>
      </body>
    </html>
  `;
}

function buildChromeMock(window, scenario, transcript) {
  let lastError = null;

  const runtime = {
    getManifest() {
      const version = scenario.buildLabel || '';
      return {
        name: 'Roger Reviewer',
        version,
        version_name: version,
      };
    },
    getURL(relativePath) {
      return RUNTIME_ASSET_URIS.get(relativePath) || `chrome-extension://roger-harness/${relativePath}`;
    },
    get lastError() {
      return lastError;
    },
    sendMessage(payload, callback) {
      transcript.runtimeMessages.push(cloneJson(payload));

      let response;
      if (payload?.type === 'roger_bridge_status') {
        transcript.statusRequests.push(cloneJson(payload));
        response = cloneJson(scenario.statusResponse);
      } else if (payload?.type === 'roger_bridge_launch') {
        transcript.launchRequests.push(cloneJson(payload));
        response = cloneJson(scenario.launchResponse);
      } else {
        response = {
          ok: false,
          message: `Harness received unsupported runtime payload ${String(payload?.type || 'unknown')}.`,
        };
      }

      window.setTimeout(() => {
        lastError = response && response.__lastError ? { message: response.__lastError } : null;
        callback(lastError ? undefined : response ?? null);
        lastError = null;
      }, 0);
    },
  };

  const tabs = {
    query(queryInfo, callback) {
      transcript.tabQueries.push(cloneJson(queryInfo));
      window.setTimeout(() => {
        callback([
          {
            id: 1,
            active: true,
            url: scenario.activeTabUrl,
          },
        ]);
      }, 0);
    },
  };

  return { runtime, tabs };
}

function installNodeGlobals(window, chrome) {
  const previous = {
    HTMLElement: global.HTMLElement,
    HTMLDialogElement: global.HTMLDialogElement,
    Node: global.Node,
    MutationObserver: global.MutationObserver,
    chrome: global.chrome,
    document: global.document,
    navigator: global.navigator,
    requestAnimationFrame: global.requestAnimationFrame,
    window: global.window,
  };

  global.window = window;
  global.document = window.document;
  global.chrome = chrome;
  global.navigator = window.navigator;
  global.Node = window.Node;
  global.HTMLElement = window.HTMLElement;
  global.HTMLDialogElement = window.HTMLDialogElement;
  global.MutationObserver = class {
    observe() {}
    disconnect() {}
  };
  global.requestAnimationFrame = (callback) => window.setTimeout(callback, 0);

  if (window.HTMLDialogElement && window.HTMLDialogElement.prototype) {
    const proto = window.HTMLDialogElement.prototype;
    if (typeof proto.showModal !== 'function') {
      proto.showModal = function showModal() {
        this.open = true;
        this.setAttribute('open', 'open');
      };
    }
    if (typeof proto.close !== 'function') {
      proto.close = function close() {
        this.open = false;
        this.removeAttribute('open');
      };
    }
  }

  return () => {
    global.window = previous.window;
    global.document = previous.document;
    global.chrome = previous.chrome;
    global.navigator = previous.navigator;
    global.Node = previous.Node;
    global.HTMLElement = previous.HTMLElement;
    global.HTMLDialogElement = previous.HTMLDialogElement;
    global.MutationObserver = previous.MutationObserver;
    global.requestAnimationFrame = previous.requestAnimationFrame;
  };
}

async function flushDom(window, cycles = 4) {
  for (let index = 0; index < cycles; index += 1) {
    await Promise.resolve();
    await new Promise((resolve) => window.setTimeout(resolve, 0));
  }
}

function collectButtonSummary(window, rootNode) {
  return Array.from(rootNode.querySelectorAll('button[data-action]')).map((button) => {
    const computed = window.getComputedStyle(button);
    return {
      action: button.dataset.action || '',
      label: button.textContent.trim(),
      hidden: Boolean(button.hidden),
      disabled: Boolean(button.disabled),
      className: button.className,
      styles: {
        backgroundColor: computed.backgroundColor,
        backgroundImage: computed.backgroundImage,
        borderColor: computed.borderTopColor,
        borderRadius: computed.borderRadius,
        boxShadow: computed.boxShadow,
        color: computed.color,
        fontWeight: computed.fontWeight,
      },
    };
  });
}

function collectPopupSummary(window, document, scenarioName, transcript) {
  const buttons = collectButtonSummary(window, document);
  return {
    surface: 'popup',
    scenario: scenarioName,
    buildLabel: document.getElementById('popup-build-info')?.textContent.trim() || '',
    infoToggleLabel: document.getElementById('popup-info-toggle')?.textContent.trim() || '',
    subtitle: document.getElementById('popup-subtitle')?.textContent.trim() || '',
    subtitleIsError: document
      .getElementById('popup-subtitle')
      ?.classList.contains('status-error') || false,
    title: document.getElementById('popup-title')?.textContent.trim() || '',
    visibleActions: buttons.filter((button) => !button.hidden).map((button) => button.action),
    buttons,
    transcript,
  };
}

function resolvePanelMode(document) {
  if (document.getElementById(PANEL_RAIL_SLOT_ID)) {
    return 'rail';
  }
  if (document.getElementById(PANEL_INLINE_SLOT_ID)) {
    return 'inline';
  }
  if (document.getElementById(PANEL_MODAL_SLOT_ID)) {
    return 'modal';
  }
  return null;
}

function collectPanelSummary(window, document, scenarioName, scenario, transcript) {
  const panel = document.getElementById(PANEL_ID);
  if (!panel) {
    throw new Error(`Panel scenario "${scenarioName}" did not mount ${PANEL_ID}.`);
  }

  const infoToggle = panel.querySelector('.roger-panel-info-toggle');
  const styleNode = document.getElementById('roger-reviewer-panel-style');
  const panelComputed = window.getComputedStyle(panel);
  const bodyComputed = window.getComputedStyle(document.body);
  const badge = document.getElementById(PANEL_BADGE_ID);
  const badgeComputed = badge ? window.getComputedStyle(badge) : null;
  const brandChip = panel.querySelector('.rr-brand-chip');
  const brandChipComputed = brandChip ? window.getComputedStyle(brandChip) : null;
  const buttons = collectButtonSummary(window, panel);

  return {
    surface: 'panel',
    scenario: scenarioName,
    hostTheme: scenario.hostTheme,
    panelMode: resolvePanelMode(document),
    panelClasses: panel.className,
    mountedUnderId: panel.parentElement?.id || null,
    title: document.getElementById(PANEL_HEADING_ID)?.textContent.trim() || '',
    subtitle: document.getElementById(PANEL_SUBHEADING_ID)?.textContent.trim() || '',
    buildLabel:
      document.getElementById(PANEL_BUILD_ID)?.textContent.trim() ||
      infoToggle?.getAttribute('title') ||
      '',
    infoToggleLabel: infoToggle?.textContent.trim() || '',
    infoTooltip: infoToggle?.getAttribute('title') || '',
    infoText: document.getElementById(PANEL_INFO_ID)?.textContent.trim() || '',
    statusText: document.getElementById(PANEL_STATUS_ID)?.textContent.trim() || '',
    statusHidden: document.getElementById(PANEL_STATUS_ID)?.hidden ?? true,
    badgeText: badge?.textContent.trim() || '',
    // The mode-specific rules (e.g. .roger-panel--rail) sit deep in the
    // injected stylesheet, so the excerpt must cover the full sheet.
    panelStyleSheetExcerpt: styleNode?.textContent.slice(0, 32000) || '',
    panelStyles: {
      backgroundColor: panelComputed.backgroundColor,
      backgroundImage: panelComputed.backgroundImage,
      borderColor: panelComputed.borderTopColor,
      boxShadow: panelComputed.boxShadow,
      color: panelComputed.color,
    },
    hostStyles: {
      backgroundColor: bodyComputed.backgroundColor,
      backgroundImage: bodyComputed.backgroundImage,
      color: bodyComputed.color,
    },
    badgeStyles: badgeComputed
      ? {
          backgroundColor: badgeComputed.backgroundColor,
          color: badgeComputed.color,
        }
      : null,
    brandChipStyles: brandChipComputed
      ? {
          backgroundColor: brandChipComputed.backgroundColor,
          backgroundImage: brandChipComputed.backgroundImage,
          borderColor: brandChipComputed.borderTopColor,
          color: brandChipComputed.color,
        }
      : null,
    visibleActions: buttons.filter((button) => !button.hidden).map((button) => button.action),
    buttons,
    transcript,
  };
}

function clearFreshModuleCache(modulePath) {
  delete require.cache[require.resolve(modulePath)];
}

async function runPopupScenario(scenarioName, options = {}) {
  assertScenarioExists('popup', scenarioName);
  const scenario = cloneJson(POPUP_SCENARIOS[scenarioName]);
  const dom = new JSDOM(loadPopupHtml(), {
    pretendToBeVisual: true,
    runScripts: 'outside-only',
    url: 'https://roger.testing/popup/index.html',
  });

  const { window } = dom;
  const transcript = {
    launchRequests: [],
    runtimeMessages: [],
    statusRequests: [],
    tabQueries: [],
  };

  const chrome = buildChromeMock(window, scenario, transcript);
  window.chrome = chrome;
  window.globalThis.chrome = chrome;
  window.module = { exports: {} };
  window.exports = window.module.exports;
  window.eval(loadPopupMainSource());
  await flushDom(window);

  if (options.clickAction) {
    const button = window.document.querySelector(`button[data-action="${options.clickAction}"]`);
    if (!button) {
      throw new Error(`Popup scenario "${scenarioName}" does not expose action "${options.clickAction}".`);
    }
    if (button.hidden || button.disabled) {
      throw new Error(`Popup action "${options.clickAction}" is not clickable in scenario "${scenarioName}".`);
    }
    button.click();
    await flushDom(window);
  }

  const summary = collectPopupSummary(window, window.document, scenarioName, transcript);
  let htmlPath = null;
  let summaryPath = null;

  if (options.outputDir) {
    fs.mkdirSync(options.outputDir, { recursive: true });
    htmlPath = path.join(options.outputDir, `popup-${scenarioName}.rendered.html`);
    summaryPath = path.join(options.outputDir, `popup-${scenarioName}.summary.json`);
    fs.writeFileSync(htmlPath, `${dom.serialize()}\n`, 'utf8');
    fs.writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, 'utf8');
  }

  window.close();

  return {
    htmlPath,
    summary,
    summaryPath,
  };
}

async function runPanelScenario(scenarioName, options = {}) {
  assertScenarioExists('panel', scenarioName);
  const scenario = cloneJson(PANEL_SCENARIOS[scenarioName]);
  const dom = new JSDOM(buildPanelHostHtml(scenario), {
    pretendToBeVisual: true,
    url: scenario.activeTabUrl,
  });

  const { window } = dom;
  const transcript = {
    launchRequests: [],
    runtimeMessages: [],
    statusRequests: [],
    tabQueries: [],
  };
  const chrome = buildChromeMock(window, scenario, transcript);

  const restoreGlobals = installNodeGlobals(window, chrome);
  let contentModule;

  try {
    clearFreshModuleCache(CONTENT_MAIN_PATH);
    contentModule = require(CONTENT_MAIN_PATH);
    contentModule.refreshPanelForCurrentPage(window.document);
    await flushDom(window);

    if (options.clickAction) {
      const button = window.document.querySelector(
        `#${PANEL_ID} button[data-action="${options.clickAction}"]`
      );
      if (!button) {
        throw new Error(`Panel scenario "${scenarioName}" does not expose action "${options.clickAction}".`);
      }
      if (button.hidden || button.disabled) {
        throw new Error(`Panel action "${options.clickAction}" is not clickable in scenario "${scenarioName}".`);
      }
      button.click();
      await flushDom(window);
    }

    const summary = collectPanelSummary(window, window.document, scenarioName, scenario, transcript);
    let htmlPath = null;
    let summaryPath = null;

    if (options.outputDir) {
      fs.mkdirSync(options.outputDir, { recursive: true });
      htmlPath = path.join(options.outputDir, `panel-${scenarioName}.rendered.html`);
      summaryPath = path.join(options.outputDir, `panel-${scenarioName}.summary.json`);
      fs.writeFileSync(htmlPath, `${dom.serialize()}\n`, 'utf8');
      fs.writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, 'utf8');
    }

    return {
      htmlPath,
      summary,
      summaryPath,
    };
  } finally {
    restoreGlobals();
    clearFreshModuleCache(CONTENT_MAIN_PATH);
    window.close();
  }
}

async function runSurfaceScenario(surface, scenarioName, options = {}) {
  if (surface === 'panel') {
    return runPanelScenario(scenarioName, options);
  }
  if (surface === 'popup') {
    return runPopupScenario(scenarioName, options);
  }
  throw new Error(`Unknown extension render harness surface "${surface}".`);
}

function parseArgs(argv) {
  const options = {
    all: false,
    clickAction: null,
    json: false,
    listScenarios: false,
    outputDir: null,
    scenario: null,
    surface: 'panel',
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--all') {
      options.all = true;
      continue;
    }
    if (arg === '--json') {
      options.json = true;
      continue;
    }
    if (arg === '--list-scenarios') {
      options.listScenarios = true;
      continue;
    }
    if (arg === '--surface') {
      options.surface = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith('--surface=')) {
      options.surface = arg.slice('--surface='.length);
      continue;
    }
    if (arg === '--scenario') {
      options.scenario = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith('--scenario=')) {
      options.scenario = arg.slice('--scenario='.length);
      continue;
    }
    if (arg === '--output-dir') {
      options.outputDir = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith('--output-dir=')) {
      options.outputDir = arg.slice('--output-dir='.length);
      continue;
    }
    if (arg === '--click') {
      options.clickAction = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith('--click=')) {
      options.clickAction = arg.slice('--click='.length);
      continue;
    }
    throw new Error(`Unknown extension render harness argument: ${arg}`);
  }

  return options;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const surfaceConfig = SURFACES[options.surface];
  if (!surfaceConfig) {
    throw new Error(`Unknown extension render harness surface "${options.surface}".`);
  }

  if (options.listScenarios) {
    for (const surface of Object.keys(SURFACES)) {
      for (const scenario of listSurfaceScenarios(surface)) {
        process.stdout.write(`${surface}:${scenario}\n`);
      }
    }
    return;
  }

  const scenarioNames = options.all
    ? listSurfaceScenarios(options.surface)
    : [options.scenario || surfaceConfig.defaultScenario];
  const outputDir = options.outputDir || DEFAULT_OUTPUT_DIR;
  const results = [];

  for (const scenarioName of scenarioNames) {
    results.push(
      await runSurfaceScenario(options.surface, scenarioName, {
        clickAction: options.clickAction,
        outputDir,
      })
    );
  }

  if (options.json) {
    process.stdout.write(`${JSON.stringify(results, null, 2)}\n`);
    return;
  }

  process.stdout.write(
    `Rendered ${results.length} ${options.surface} scenario(s) into ${outputDir}\n`
  );
  for (const result of results) {
    process.stdout.write(`- ${result.summary.surface}:${result.summary.scenario}\n`);
    process.stdout.write(`  title: ${result.summary.title}\n`);
    process.stdout.write(`  subtitle: ${result.summary.subtitle}\n`);
    if (result.summary.surface === 'panel') {
      process.stdout.write(`  panel mode: ${result.summary.panelMode}\n`);
    }
    process.stdout.write(`  visible actions: ${result.summary.visibleActions.join(', ') || 'none'}\n`);
    if (result.htmlPath) {
      process.stdout.write(`  html: ${result.htmlPath}\n`);
    }
    if (result.summaryPath) {
      process.stdout.write(`  summary: ${result.summaryPath}\n`);
    }
  }
}

if (require.main === module) {
  main().catch((error) => {
    console.error(`extension render harness failed: ${String(error?.stack || error)}`);
    process.exitCode = 1;
  });
}

module.exports = {
  DEFAULT_OUTPUT_DIR,
  PANEL_SCENARIOS,
  POPUP_SCENARIOS,
  SURFACES,
  buildPanelHostHtml,
  loadPopupHtml,
  parseArgs,
  runPanelScenario,
  runPopupScenario,
  runSurfaceScenario,
};
