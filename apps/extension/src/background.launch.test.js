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
    assert.match(response.guidance, /host is not registered/i);
    assert.match(response.guidance, /rr extension setup --browser <edge\|chrome\|brave>/i);
    assert.match(response.guidance, /rr extension doctor --browser <edge\|chrome\|brave>/i);
    assert.match(response.guidance, /reload the browser extension/i);
    assert.match(response.guidance, /RR_STORE_ROOT/i);
    assert.equal(tabCreateCalled, false);
  });
});

for (const action of ['start_review', 'resume_review', 'show_findings', 'refresh_review']) {
  test(`handleLaunchMessage preserves native messaging success envelope for ${action}`, async () => {
    const chromeStub = {
      runtime: {
        lastError: null,
        onMessage: { addListener: () => {} },
        sendNativeMessage(_host, _intent, callback) {
          callback({
            ok: true,
            action,
            message: `Dispatching ${action} for acme/widgets#42`,
            guidance: null,
            session_id: null,
          });
        },
      },
    };

    await withChromeStub(chromeStub, async () => {
      const response = await handleLaunchMessage({
        intent: {
          action,
          owner: 'acme',
          repo: 'widgets',
          pr_number: 42,
        },
      });

      assert.equal(response.ok, true);
      assert.equal(response.mode, 'native_messaging');
      assert.equal(response.action, action);
      assert.match(response.message, new RegExp(`Dispatching ${action}`));
    });
  });
}
