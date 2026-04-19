const test = require('node:test');
const assert = require('node:assert/strict');

const {
  ACTIONS,
  NON_PR_SUBTITLE,
  PR_SUBTITLE,
  describeBuildInfo,
  buildLaunchMessage,
  buildPopupViewModel,
  describeLaunchResponse,
  parsePullRequestContextFromUrl,
  SUPPORTED_ACTIONS,
  routePopupAction,
} = require('./main.js');

test('parsePullRequestContextFromUrl extracts owner/repo/pr from GitHub PR URL', () => {
  const context = parsePullRequestContextFromUrl('https://github.com/octo/roger-reviewer/pull/42');

  assert.deepEqual(context, {
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 42,
  });
});

test('parsePullRequestContextFromUrl rejects non-PR URL', () => {
  const context = parsePullRequestContextFromUrl('https://github.com/octo/roger-reviewer/issues/42');

  assert.equal(context, null);
});

test('buildPopupViewModel returns non_pr guidance when active tab is not a pull request', () => {
  const viewModel = buildPopupViewModel('https://example.com/dashboard');

  assert.equal(viewModel.mode, 'non_pr');
  assert.equal(viewModel.context, null);
  assert.equal(viewModel.subtitle, NON_PR_SUBTITLE);
  assert.match(viewModel.subtitle, /manual backup/i);
  assert.match(viewModel.subtitle, /Open a GitHub pull request tab/i);
});

test('buildPopupViewModel returns PR context title and action subtitle on pull request tabs', () => {
  const viewModel = buildPopupViewModel('https://github.com/octo/roger-reviewer/pull/42');

  assert.equal(viewModel.mode, 'pr');
  assert.deepEqual(viewModel.context, {
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 42,
  });
  assert.equal(viewModel.subtitle, PR_SUBTITLE);
  assert.match(viewModel.title, /octo\/roger-reviewer#42/);
  assert.match(viewModel.subtitle, /manual backup controls/i);
  assert.match(viewModel.subtitle, /in-page Roger controls/i);
});

test('ACTIONS advertise explicit popup labels instead of generic verbs', () => {
  const labels = new Map(ACTIONS.map((action) => [action.id, action.label]));
  assert.equal(labels.get('start_review'), 'Start Review in Roger');
  assert.equal(labels.get('resume_review'), 'Resume Existing Review');
  assert.equal(labels.get('show_findings'), 'View Findings');
});

test('buildLaunchMessage emits canonical launch payload', () => {
  const message = buildLaunchMessage('start_review', {
    owner: 'octo',
    repo: 'roger-reviewer',
    pr_number: 42,
  });

  assert.deepEqual(message, {
    type: 'roger_bridge_launch',
    intent: {
      action: 'start_review',
      owner: 'octo',
      repo: 'roger-reviewer',
      pr_number: 42,
    },
  });
});

test('routePopupAction routes every documented action through dispatcher payload', async () => {
  const seenActions = [];

  for (const action of ACTIONS) {
    const result = await routePopupAction(
      action.id,
      {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      },
      async (payload) => {
        seenActions.push(payload.intent.action);
        return { ok: true, mode: 'native_messaging' };
      }
    );

    assert.equal(result.ok, true);
    assert.equal(result.mode, 'native_messaging');
  }

  assert.deepEqual(seenActions, ACTIONS.map((action) => action.id));
});

test('routePopupAction no longer exposes refresh_review as a supported action', () => {
  assert.equal(SUPPORTED_ACTIONS.has('refresh_review'), false);
  assert.throws(
    () =>
      buildLaunchMessage('refresh_review', {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      }),
    /Unsupported action/
  );
});

test('describeLaunchResponse appends repair guidance on successful launch responses', () => {
  const feedback = describeLaunchResponse({
    ok: true,
    mode: 'native_messaging',
    message: 'rr resume completed for octo/roger-reviewer#42',
    guidance: 'Run `rr resume --session session-42` locally to reconcile stale state.',
  });

  assert.equal(feedback.isError, false);
  assert.match(feedback.message, /rr resume completed/i);
  assert.match(feedback.message, /rr resume --session session-42/);
});

test('describeBuildInfo reports fallback text when version is unavailable', () => {
  assert.equal(describeBuildInfo(''), 'Extension build unavailable.');
  assert.equal(describeBuildInfo('0.1.0-dev+abc123'), 'Extension build 0.1.0-dev+abc123.');
});

test('buildLaunchMessage rejects unsupported actions', () => {
  assert.throws(
    () =>
      buildLaunchMessage('post_review', {
        owner: 'octo',
        repo: 'roger-reviewer',
        pr_number: 42,
      }),
    /Unsupported action/
  );
});
