/*
 * datazen.playground — sample extension bridge client.
 *
 * Zero-build vanilla JS mirroring the @datazen/ui-plugin-sdk semantics:
 * `plugin.ready` -> `host.ready` handshake, then reqId-correlated RPC against
 * the host postMessage router (src/lib/uiPluginBridge.ts). Every v1 API is
 * exercised from the page so manual/E2E testing covers the whole surface:
 * context.getConnections, context.getActiveConnection, command.invoke,
 * storage.get/set/remove, ui.notify and live theme.apply snapshots.
 */
(function () {
  'use strict';

  var API_VERSION = 2;
  var CHANNEL = 'ui-plugin';
  var HANDSHAKE_RETRIES = 10;
  var HANDSHAKE_RETRY_MS = 500;
  var REQUEST_TIMEOUT_MS = 15000;
  var COUNTER_KEY = 'playground.counter';

  var parentWindow = window.parent;
  var seq = 0;
  var pending = new Map();
  var handshakeTimer = null;
  var activeConnection = null; // { id, name, dbType } | null
  var firstConnectionId = null;

  function $(id) {
    return document.getElementById(id);
  }

  function set(id, text) {
    var node = $(id);
    if (node) node.textContent = String(text);
  }

  function setStatus(id, text, ok) {
    var node = $(id);
    if (!node) return;
    node.textContent = text;
    node.className = 'kv ' + (ok ? 'ok' : 'err');
  }

  function showError(scope, error) {
    var message = error && error.message ? error.message : String(error);
    console.error('[playground]', scope, message);
  }

  /* ------------------------------------------------------------------ RPC */

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
        set('pg-bridge-status', 'error: apiVersion mismatch (' + payload.apiVersion + ')');
        return;
      }
      set('pg-bridge-status', 'ready');
      renderTheme({
        dark: payload.dark === true,
        tokens: payload.tokens && typeof payload.tokens === 'object' ? payload.tokens : {},
      });
      loadConnections();
      loadCounter();
      return;
    }

    if (data.type === 'theme.apply') {
      var snap = data.payload || {};
      renderTheme({ dark: snap.dark === true, tokens: snap.tokens });
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

  /* --------------------------------------------------------------- theme */

  function renderTheme(state) {
    set('pg-dark-state', state.dark ? 'dark' : 'light');
    var tokens = state.tokens && typeof state.tokens === 'object' ? state.tokens : {};
    set('pg-token-count', Object.keys(tokens).length);

    var host = $('pg-token-chips');
    if (!host) return;
    host.textContent = '';
    Object.keys(tokens)
      .slice(0, 12)
      .forEach(function (name) {
        var chip = document.createElement('span');
        chip.className = 'chip';
        chip.textContent = name + ' = ' + tokens[name];
        chip.style.color = String(tokens[name]).match(/^#[0-9a-f]{3,8}$/i)
          ? String(tokens[name])
          : '';
        host.appendChild(chip);
      });
  }

  /* ------------------------------------------------------------ context */

  function loadConnections() {
    request('context.getActiveConnection')
      .then(function (result) {
        activeConnection = result && result.connection ? result.connection : null;
        set(
          'pg-active-conn',
          activeConnection ? activeConnection.name + ' (' + activeConnection.dbType + ')' : 'none',
        );
      })
      .catch(function (e) {
        showError('getActiveConnection', e);
        setStatus('pg-active-conn', 'error: ' + (e && e.message), false);
      });

    request('context.getConnections')
      .then(function (result) {
        var conns = result && Array.isArray(result.connections) ? result.connections : [];
        if (conns.length > 0) firstConnectionId = conns[0].id;
        var list = $('pg-conn-list');
        if (!list) return;
        list.textContent = '';
        conns.forEach(function (c) {
          var li = document.createElement('li');
          li.textContent = c.name + ' — ' + c.dbType + ' (' + c.id + ')';
          list.appendChild(li);
        });
        if (conns.length === 0) {
          var li = document.createElement('li');
          li.textContent = 'no saved connections';
          list.appendChild(li);
        }
      })
      .catch(function (e) {
        showError('getConnections', e);
        setStatus('pg-query-status', 'context error: ' + (e && e.message), false);
      });
  }

  /* ------------------------------------------------------------ command */

  function runQuery() {
    var sql = ($('pg-sql-input') && $('pg-sql-input').value) || 'SELECT 1';
    var configId = (activeConnection && activeConnection.id) || firstConnectionId;
    if (!configId) {
      set('pg-query-out', 'No connection available. Save one in DataZen and reload this tab.');
      setStatus('pg-query-status', 'no configId', false);
      return;
    }
    setStatus('pg-query-status', 'running…', true);
    request('command.invoke', { configId: configId, command: 'query', args: { sql: sql } })
      .then(function (result) {
        var rows = result && Array.isArray(result.rows) ? result.rows : null;
        var summary = rows
          ? rows.length +
            ' row(s), ' +
            (Array.isArray(result.columns) ? result.columns.length + ' column(s)' : '?')
          : 'ok';
        setStatus('pg-query-status', summary, true);
        var text = JSON.stringify(result, null, 2);
        set('pg-query-out', text.length > 4000 ? text.slice(0, 4000) + '\n… truncated' : text);
      })
      .catch(function (e) {
        showError('command.invoke', e);
        setStatus('pg-query-status', 'failed', false);
        set('pg-query-out', 'error: ' + (e && e.message ? e.message : String(e)));
      });
  }

  /* ------------------------------------------------------------ storage */

  function loadCounter() {
    request('storage.get', { key: COUNTER_KEY })
      .then(function (result) {
        var value = result && result.value != null ? Number(result.value) : 0;
        set('pg-counter', Number.isNaN(value) ? String(result.value) : String(value));
      })
      .catch(function (e) {
        showError('storage.get', e);
        set('pg-counter', 'err');
      });
  }

  function incrementCounter() {
    var current = Number($('pg-counter') ? $('pg-counter').textContent : 0);
    if (Number.isNaN(current)) current = 0;
    var next = current + 1;
    request('storage.set', { key: COUNTER_KEY, value: next })
      .then(function () {
        set('pg-counter', String(next));
      })
      .catch(function (e) {
        showError('storage.set', e);
        setStatus('pg-notify-status', 'storage.set failed: ' + (e && e.message), false);
      });
  }

  function clearCounter() {
    request('storage.remove', { key: COUNTER_KEY })
      .then(function () {
        set('pg-counter', '0');
      })
      .catch(function (e) {
        showError('storage.remove', e);
      });
  }

  /* -------------------------------------------------------------- notify */

  function sendNotify() {
    request('ui.notify', {
      title: 'Extension Playground',
      body: 'Hello from datazen.playground at ' + new Date().toLocaleTimeString(),
    })
      .then(function () {
        setStatus('pg-notify-status', 'sent ✓', true);
      })
      .catch(function (e) {
        showError('ui.notify', e);
        setStatus('pg-notify-status', 'rejected: ' + (e && e.message), false);
      });
  }

  /* ----------------------------------------------------------- handshake */

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
      set('pg-bridge-status', 'error: no host.ready');
      return;
    }
    stopHandshakeRetry();
    handshakeTimer = setTimeout(function () {
      sendPluginReady();
      scheduleHandshakeRetry(attempt + 1);
    }, HANDSHAKE_RETRY_MS);
  }

  function boot() {
    set('pg-bridge-status', 'connecting');

    $('pg-run-query').addEventListener('click', runQuery);
    $('pg-inc').addEventListener('click', incrementCounter);
    $('pg-clear').addEventListener('click', clearCounter);
    $('pg-notify').addEventListener('click', sendNotify);
    $('pg-sql-input').addEventListener('keydown', function (event) {
      if (event.key === 'Enter') runQuery();
    });

    window.addEventListener('message', onMessage);
    sendPluginReady();
    scheduleHandshakeRetry(1);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
