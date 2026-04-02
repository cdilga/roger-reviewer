const PANEL_ID = 'roger-reviewer-panel';
const STATUS_ID = 'roger-reviewer-status';
const BADGE_ID = 'roger-reviewer-attention-badge';

const ACTIONS = [
  { id: 'start_review', label: 'Start' },
  { id: 'resume_review', label: 'Resume' },
  { id: 'show_findings', label: 'Findings' },
  { id: 'refresh_review', label: 'Refresh' },
];

const ATTENTION_STYLES = {
  awaiting_user_input: { label: 'Awaiting user input', background: '#fef3c7', color: '#92400e' },
  awaiting_outbound_approval: {
    label: 'Awaiting outbound approval',
    background: '#ffe4e6',
    color: '#9f1239',
  },
  findings_ready: { label: 'Findings ready', background: '#dcfce7', color: '#166534' },
  refresh_recommended: { label: 'Refresh recommended', background: '#e0f2fe', color: '#0c4a6e' },
  review_failed: { label: 'Review failed', background: '#fee2e2', color: '#991b1b' },
};

function parsePullRequestContext() {
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
  const statusNode = document.getElementById(STATUS_ID);
  if (!statusNode) {
    return;
  }
  statusNode.textContent = message;
  statusNode.style.color = isError ? '#b42318' : '#0f5132';
}

function clearAttentionBadge() {
  const badge = document.getElementById(BADGE_ID);
  if (!badge) {
    return;
  }

  badge.textContent = '';
  badge.style.display = 'none';
}

function setAttentionBadge(attentionState, freshnessLabel) {
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

function renderPanel(context) {
  if (document.getElementById(PANEL_ID)) {
    return;
  }

  const panel = document.createElement('section');
  panel.id = PANEL_ID;
  panel.style.position = 'fixed';
  panel.style.top = '88px';
  panel.style.right = '24px';
  panel.style.zIndex = '9999';
  panel.style.width = '260px';
  panel.style.background = '#ffffff';
  panel.style.border = '1px solid #d0d7de';
  panel.style.borderRadius = '10px';
  panel.style.boxShadow = '0 10px 24px rgba(27,31,35,0.12)';
  panel.style.padding = '12px';
  panel.style.fontFamily = 'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';

  const heading = document.createElement('h3');
  heading.textContent = `Roger: ${context.owner}/${context.repo}#${context.pr_number}`;
  heading.style.margin = '0 0 10px 0';
  heading.style.fontSize = '13px';
  heading.style.lineHeight = '1.3';
  heading.style.fontWeight = '600';
  panel.appendChild(heading);

  const badge = document.createElement('p');
  badge.id = BADGE_ID;
  badge.style.margin = '0 0 10px 0';
  badge.style.fontSize = '11px';
  badge.style.fontWeight = '600';
  badge.style.borderRadius = '999px';
  badge.style.padding = '4px 8px';
  badge.style.display = 'none';
  panel.appendChild(badge);

  const buttonRow = document.createElement('div');
  buttonRow.style.display = 'grid';
  buttonRow.style.gridTemplateColumns = 'repeat(2, minmax(0, 1fr))';
  buttonRow.style.gap = '8px';

  for (const action of ACTIONS) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = action.label;
    button.dataset.action = action.id;
    button.style.border = '1px solid #d0d7de';
    button.style.background = '#f6f8fa';
    button.style.borderRadius = '6px';
    button.style.padding = '6px 8px';
    button.style.fontSize = '12px';
    button.style.cursor = 'pointer';
    button.addEventListener('click', () => triggerLaunch(action.id, context, button));
    buttonRow.appendChild(button);
  }

  panel.appendChild(buttonRow);

  const status = document.createElement('p');
  status.id = STATUS_ID;
  status.textContent = 'Launch-only mode. Live status is shown in Roger locally.';
  status.style.margin = '10px 0 0 0';
  status.style.fontSize = '12px';
  status.style.lineHeight = '1.35';
  status.style.color = '#57606a';
  panel.appendChild(status);

  document.body.appendChild(panel);
}

function triggerLaunch(action, context, button) {
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

(function bootstrapRogerPanel() {
  const context = parsePullRequestContext();
  if (!context) {
    return;
  }

  renderPanel(context);
  requestStatusMirror(context);
})();
