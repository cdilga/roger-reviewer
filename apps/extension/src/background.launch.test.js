const test = require('node:test');
const assert = require('node:assert/strict');

const { handleLaunchMessage } = require('./background/main.js');

function withChromeStub(stub, fn) {
  const previousChrome = global.chrome;
  global.chrome = stub;
  return Promise.resolve()
    .then(fn)
    .finally(() => {
      if (previousChrome === undefined) {
        delete global.chrome;
      } else {
        global.chrome = previousChrome;
      }
    });
}

test('handleLaunchMessage fails closed when Native Messaging is unavailable', async () => {
  let tabCreateCalled = false;
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage(_host, _intent, callback) {
        this.lastError = { message: 'Specified native messaging host not found.' };
        callback(undefined);
        this.lastError = null;
      },
    },
    tabs: {
      async create() {
        tabCreateCalled = true;
      },
    },
  };

  await withChromeStub(chromeStub, async () => {
    const response = await handleLaunchMessage({
      intent: {
        action: 'start_review',
        owner: 'acme',
        repo: 'widgets',
        pr_number: 42,
      },
    });

    assert.equal(response.ok, false);
    assert.equal(response.mode, 'native_unavailable');
    assert.equal(response.action, 'start_review');
    assert.match(response.message, /launch blocked/i);
    assert.match(response.guidance, /native messaging host not found/i);
    assert.equal(tabCreateCalled, false);
  });
});

test('handleLaunchMessage preserves native messaging success envelope', async () => {
  const chromeStub = {
    runtime: {
      lastError: null,
      onMessage: { addListener: () => {} },
      sendNativeMessage(_host, _intent, callback) {
        callback({
          ok: true,
          action: 'resume_review',
          message: 'Dispatching resume_review for acme/widgets#42',
          guidance: null,
          session_id: null,
        });
      },
    },
  };

  await withChromeStub(chromeStub, async () => {
    const response = await handleLaunchMessage({
      intent: {
        action: 'resume_review',
        owner: 'acme',
        repo: 'widgets',
        pr_number: 42,
      },
    });

    assert.equal(response.ok, true);
    assert.equal(response.mode, 'native_messaging');
    assert.equal(response.action, 'resume_review');
  });
});
