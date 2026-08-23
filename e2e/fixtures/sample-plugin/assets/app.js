/*
 * Sample plugin bridge client (F9 E2E fixture).
 *
 * Zero-build vanilla JS mirroring the @datazen/extension-sdk bridge semantics:
 * `plugin.ready` -> `host.ready` handshake, then reqId-correlated RPC against
 * the host postMessage router (src/lib/extensionBridge.ts). Kept dependency-free
 * on purpose so the fixture is a plain static directory.
 */
(function () {
  'use strict';

  var API_VERSION = 2;
  var CHANNEL = 'datazen-extension';
  var HANDSHAKE_RETRIES = 10;
  var HANDSHAKE_RETRY_MS = 500;
  var REQUEST_TIMEOUT_MS = 15000;
  var STORAGE_KEY = 'e2e-marker';
  var STORAGE_VALUE = 'ok';

  // BUG-F9-02 workaround: safaridriver cannot automate elements inside the
  // opaque-origin plugin iframe, so probe outcomes are persisted through the
  // bridge storage.set RPC and asserted from disk by plugins.spec.ts.
  var PROBE_PREFIX = 'probe.';

  var parentWindow = window.parent;
  var seq = 0;
  var pending = new Map();
  var handshakeTimer = null;

  function el(testId) {
    return document.querySelector('[data-testid="' + testId + '"]');
  }

  function set(testId, text) {
    var node = el(testId);
    if (node) node.textContent = String(text);
  }

  function fail(testId, error) {
    set(testId, 'error: ' + (error && error.message ? error.message : String(error)));
  }

  function persistProbe(key, value) {
    request('storage.set', { key: PROBE_PREFIX + key, value: value }).catch(function () {
      /* best-effort; spec polls the file and reports the last persisted value */
    });
  }

  // BUG-F9-04 ②: every error path must land both in a DOM error element and
  // in the probe.error storage key — never escape as an unhandled rejection
  // (an earlier revision crashed here calling done('ERR ' + ...) with done
  // bound to a path string).
  function persistError(scope, error) {
    var message = error && error.message ? error.message : String(error);
    set('probe-error', scope + ': ' + message);
    persistProbe('error', scope + ': ' + message);
  }

  function renderContext(ctx) {
    var dark = ctx.dark ? 'dark' : 'light';
    applyHostTheme(ctx); // theme-consistency spec: page MUST consume host tokens
    set('dark-state', dark);
    persistProbe('dark', dark);
    var count = ctx.tokens && typeof ctx.tokens === 'object' ? Object.keys(ctx.tokens).length : 0;
    set('token-count', count);
  }

  // Theme-consistency contract (packages/extensions/README.md): write the
  // host's --c-*/--dt-* tokens onto :root, toggle the `dark` class and keep
  // native controls/scrollbars in sync via color-scheme.
  function applyHostTheme(ctx) {
    var root = document.documentElement;
    var tokens = ctx && ctx.tokens && typeof ctx.tokens === 'object' ? ctx.tokens : {};
    Object.keys(tokens).forEach(function (name) {
      if (/^--/.test(name) && typeof tokens[name] === 'string') {
        root.style.setProperty(name, tokens[name]);
      }
    });
    var dark = !!(ctx && ctx.dark);
    root.classList.toggle('dark', dark);
    root.style.colorScheme = dark ? 'dark' : 'light';
  }

  function request(type, payload) {
    seq += 1;
    var reqId = 'req-' + seq;
    return new Promise(function (resolve, reject) {
      var timer = setTimeout(function () {
        pending.delete(reqId);
        reject(new Error(type + ' timed out'));
      }, REQUEST_TIMEOUT_MS);
      pending.set(reqId, { resolve: resolve, reject: reject, timer: timer });
      var envelope = { ch: CHANNEL, type: type, reqId: reqId, target: 'host' };
      if (payload !== undefined) envelope.payload = payload;
      parentWindow.postMessage(envelope, '*');
    });
  }

  function onMessage(event) {
    if (event.source !== parentWindow) return; // anti-spoofing: parent frames only
    var data = event.data;
    if (!data || typeof data !== 'object' || data.ch !== CHANNEL) return;

    if (data.type === 'host.ready') {
      stopHandshakeRetry();
      var payload = data.payload || {};
      if (payload.apiVersion !== API_VERSION) {
        set('bridge-status', 'error: apiVersion mismatch (' + payload.apiVersion + ')');
        persistError('apiVersion', 'host reported ' + payload.apiVersion);
        return;
      }
      renderContext({
        dark: payload.dark === true,
        tokens: payload.tokens && typeof payload.tokens === 'object' ? payload.tokens : {},
      });
      set('bridge-status', 'ready');
      persistProbe('bridge', 'ok');
      runChecks();
      return;
    }

    // Live theme snapshots (dark toggle / theme pack switch) keep mode+count fresh.
    if (data.type === 'theme.apply') {
      var snap = data.payload || {};
      renderContext({ dark: snap.dark === true, tokens: snap.tokens });
      return;
    }

    if ((data.ok === true || data.ok === false) && typeof data.reqId === 'string') {
      var entry = pending.get(data.reqId);
      if (!entry) return; // late / duplicate / foreign response — ignore
      pending.delete(data.reqId);
      clearTimeout(entry.timer);
      if (data.ok) {
        entry.resolve(data.payload || {});
      } else {
        var p = data.payload || {};
        entry.reject(new Error((p.code || 'E_INTERNAL') + ': ' + (p.message || '')));
      }
    }
  }

  // Journey 2 / M2 acceptance: one real command.invoke round-trip against the
  // first saved connection. `ok:<rows>` proves the full plugin → bridge →
  // execute_driver_command path; `err:…` keeps the failure observable (the
  // spec treats unreachable-DB outcomes as environment-gated).
  function runQueryProbe(conns) {
    if (!conns || conns.length === 0) {
      persistProbe('query', 'err:no-conn');
      return;
    }
    request('command.invoke', {
      configId: conns[0].id,
      command: 'query',
      args: { sql: 'SELECT 1 AS one' },
    })
      .then(function (result) {
        // Bridge envelope {result: <CommandResult.data>}; query data is the
        // multi-statement wrapper {results:[{columns, rows,…}], totalTimeMs}.
        var payload = result && result.result ? result.result : result;
        var stmts = payload && Array.isArray(payload.results) ? payload.results : null;
        var first = stmts && stmts[0];
        var rows =
          first && Array.isArray(first.rows)
            ? first.rows.length
            : payload && Array.isArray(payload.rows)
              ? payload.rows.length
              : -1;
        set('query-probe', 'ok');
        persistProbe('query', 'ok:' + rows + 'rows');
      })
      .catch(function (e) {
        var message = e && e.message ? e.message : String(e);
        set('query-probe', 'error');
        persistProbe('query', 'err:' + message.slice(0, 120));
      });
  }

  function runChecks() {
    // Journey 2: context.getConnections renders the visible connection count.
    request('context.getConnections')
      .then(function (result) {
        var conns = result && Array.isArray(result.connections) ? result.connections : [];
        set('conn-count', conns.length);
        persistProbe('connCount', String(conns.length));
        runQueryProbe(conns);
      })
      .catch(function (e) {
        fail('conn-count', e);
        persistProbe('connCount', 'error: ' + (e && e.message ? e.message : String(e)));
        persistProbe('query', 'err:no-conn');
      });

    // Journey 2: storage.set -> storage.get round-trip proves the RPC bridge.
    request('storage.set', { key: STORAGE_KEY, value: STORAGE_VALUE })
      .then(function () {
        return request('storage.get', { key: STORAGE_KEY });
      })
      .then(function (result) {
        set('storage-roundtrip', result && result.value === STORAGE_VALUE ? 'ok' : 'mismatch');
        if (!result || result.value !== STORAGE_VALUE) {
          persistError('storage-roundtrip', new Error('mismatch'));
        }
      })
      .catch(function (e) {
        fail('storage-roundtrip', e);
        persistError('storage-roundtrip', e);
      });
  }

  function sendPluginReady() {
    parentWindow.postMessage(
      { ch: CHANNEL, type: 'plugin.ready', target: 'host', payload: { apiVersion: API_VERSION } },
      '*',
    );
  }

  function stopHandshakeRetry() {
    if (handshakeTimer !== null) {
      clearTimeout(handshakeTimer);
      handshakeTimer = null;
    }
  }

  function scheduleHandshakeRetry(attempt) {
    if (attempt >= HANDSHAKE_RETRIES) {
      set('bridge-status', 'error: no host.ready');
      persistError('handshake', new Error('no host.ready after retries'));
      return;
    }
    stopHandshakeRetry();
    handshakeTimer = setTimeout(function () {
      sendPluginReady();
      scheduleHandshakeRetry(attempt + 1);
    }, HANDSHAKE_RETRY_MS);
  }

  window.addEventListener('message', onMessage);
  // Last-resort net: unexpected script errors or rejections are persisted so
  // the spec can read them from disk; neither handler can itself throw.
  window.addEventListener('error', function (event) {
    persistError('window', (event && event.message) || 'unknown script error');
  });
  window.addEventListener('unhandledrejection', function (event) {
    var reason = event && event.reason;
    persistError('rejection', reason && reason.message ? reason.message : String(reason));
  });

  function boot() {
    set('bridge-status', 'connecting');
    sendPluginReady();
    scheduleHandshakeRetry(1);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
