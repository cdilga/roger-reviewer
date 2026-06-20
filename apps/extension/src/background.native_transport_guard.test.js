const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

// Guard against the bug class behind rr-s8wx (status relay) AND the panel
// launch buttons: one-shot `chrome.runtime.sendNativeMessage` is torn down with
// Edge's MV3 module service worker before the native reply lands, so the
// content -> background -> native round trip silently never settles. Every
// reply-expecting native dispatch in the background worker must use a
// long-lived `chrome.runtime.connectNative` Port (which anchors the worker
// until the host's single reply arrives).
//
// This is the cheap static guard that would have failed the moment the launch
// path was left on the fragile call. If a deliberate one-shot use is ever
// genuinely needed, update this test with the rationale rather than deleting it.
const backgroundSource = fs.readFileSync(
  path.join(__dirname, 'background', 'main.js'),
  'utf8'
);

test('background worker does not use one-shot sendNativeMessage (Edge MV3 teardown hazard)', () => {
  // Match an actual CALL (`sendNativeMessage(`), not mentions in the comments
  // that explain why the pattern is banned.
  const calls = backgroundSource.match(/\bsendNativeMessage\s*\(/g) || [];
  assert.deepEqual(
    calls,
    [],
    'background/main.js must dispatch native messages over a long-lived connectNative Port, ' +
      'not one-shot chrome.runtime.sendNativeMessage (torn down before reply on Edge MV3).'
  );
});

test('background worker dispatches native messages via connectNative Port', () => {
  assert.ok(
    backgroundSource.includes('connectNative'),
    'background/main.js should open a connectNative Port for native dispatch'
  );
});
