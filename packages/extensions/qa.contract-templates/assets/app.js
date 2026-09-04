/**
 * 测试环境合同模板中心 - 扩展页面核心逻辑
 * 遵循 DataZen 扩展规范与主题契约
 */
(function () {
  'use strict';

  var API_VERSION = 2;
  var CHANNEL = 'datazen-extension';
  var STORAGE_KEY_PREFS = 'qa_contract_prefs';
  var STORAGE_KEY_CUSTOM_TPLS = 'qa_contract_custom_tpls';
  var REQUEST_TIMEOUT_MS = 15000;

  var parentWindow = window.parent;
  var seq = 0;
  var pending = new Map();
  var handshakeTimer = null;

  var D = window.ContractTemplatesData;
  if (!D) {
    console.error('ContractTemplatesData not found!');
    return;
  }

  // 核心状态
  var state = {
    currentEnv: 'qa-01',
    currentTplId: 'tpl-saas-procurement',
    currentTab: 'preview', // 'preview' | 'variables' | 'source' | 'env_info'
    categoryFilter: 'all',
    searchQuery: '',
    highlightVariables: true,
    showWatermark: true,
    showSeal: true,
    variableValues: {}, // key -> value
    customTemplates: [],
    activeConnection: null,
    allConnections: [],
  };

  /* ------------------------------------------------------------------ DOM 工具 */
  function $(id) {
    return document.getElementById(id);
  }

  function escapeHtml(str) {
    return String(str || '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  /* ------------------------------------------------------------------ 桥接通信 (Bridge RPC) */
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

  function notify(title, body) {
    // 优先调用宿主原生通知
    request('ui.notify', { title: title, body: body || '' }).catch(function () {
      console.log('[notify fallback]', title, body);
    });
    // 同时在界面上显示轻量 Toast
    showToast(title + (body ? ' - ' + body : ''));
  }

  function showToast(msg) {
    var toast = $('toast');
    if (!toast) return;
    toast.textContent = msg;
    toast.classList.add('show');
    clearTimeout(toast._timer);
    toast._timer = setTimeout(function () {
      toast.classList.remove('show');
    }, 2800);
  }

  function applyHostTheme(snap) {
    var root = document.documentElement;
    var tokens = snap && snap.tokens && typeof snap.tokens === 'object' ? snap.tokens : {};
    Object.keys(tokens).forEach(function (name) {
      if (/^--/.test(name) && typeof tokens[name] === 'string') {
        root.style.setProperty(name, tokens[name]);
      }
    });
    var dark = !!(snap && snap.dark);
    root.classList.toggle('dark', dark);
    root.style.colorScheme = dark ? 'dark' : 'light';
  }

  function onMessage(event) {
    if (event.source !== parentWindow) return;
    var data = event.data;
    if (!data || typeof data !== 'object' || data.ch !== CHANNEL) return;

    if (data.type === 'host.ready') {
      stopHandshake();
      var payload = data.payload || {};
      applyHostTheme({
        dark: payload.dark === true,
        tokens: payload.tokens || {},
      });
      loadHostContext();
      loadStorageData();
      return;
    }

    if (data.type === 'theme.apply') {
      var p = data.payload || {};
      applyHostTheme({ dark: p.dark === true, tokens: p.tokens });
      return;
    }

    if ((data.ok === true || data.ok === false) && typeof data.reqId === 'string') {
      var entry = pending.get(data.reqId);
      if (!entry) return;
      pending.delete(data.reqId);
      clearTimeout(entry.timer);
      if (data.ok) {
        entry.resolve(data.payload || {});
      } else {
        var errPayload = data.payload || {};
        entry.reject(
          new Error((errPayload.code || 'E_INTERNAL') + ': ' + (errPayload.message || '')),
        );
      }
    }
  }

  function startHandshake() {
    var retries = 0;
    function ping() {
      retries += 1;
      parentWindow.postMessage(
        { ch: CHANNEL, type: 'plugin.ready', payload: { apiVersion: API_VERSION } },
        '*',
      );
      if (retries >= 15) {
        stopHandshake();
      }
    }
    ping();
    handshakeTimer = setInterval(ping, 500);
  }

  function stopHandshake() {
    if (handshakeTimer) {
      clearInterval(handshakeTimer);
      handshakeTimer = null;
    }
  }

  function loadHostContext() {
    request('context.getActiveConnection')
      .then(function (res) {
        state.activeConnection = res && res.connection ? res.connection : null;
        renderConnectionBadge();
      })
      .catch(function () {});

    request('context.getConnections')
      .then(function (res) {
        state.allConnections = (res && res.connections) || [];
      })
      .catch(function () {});
  }

  function renderConnectionBadge() {
    var el = $('active-conn-badge');
    if (!el) return;
    if (state.activeConnection) {
      el.textContent =
        '已连接: ' + state.activeConnection.name + ' (' + state.activeConnection.dbType + ')';
      el.title = '当前活动测试数据库连接: ' + state.activeConnection.id;
      el.className = 'badge badge-success';
    } else {
      el.textContent = '离线模式 (未挂接数据库)';
      el.title = '纯本地或沙箱渲染，无需实时数据库连接';
      el.className = 'badge badge-muted';
    }
  }

  /* ------------------------------------------------------------------ 持久化存储 (Storage) */
  function loadStorageData() {
    request('storage.get', { key: STORAGE_KEY_PREFS })
      .then(function (res) {
        var prefs = res && res.value;
        if (prefs && typeof prefs === 'object') {
          if (prefs.currentEnv && D.ENVIRONMENTS[prefs.currentEnv]) {
            state.currentEnv = prefs.currentEnv;
          }
          if (prefs.currentTplId) {
            state.currentTplId = prefs.currentTplId;
          }
          if (prefs.highlightVariables !== undefined) {
            state.highlightVariables = !!prefs.highlightVariables;
          }
          if (prefs.showWatermark !== undefined) {
            state.showWatermark = !!prefs.showWatermark;
          }
          if (prefs.showSeal !== undefined) {
            state.showSeal = !!prefs.showSeal;
          }
        }
        return request('storage.get', { key: STORAGE_KEY_CUSTOM_TPLS });
      })
      .then(function (res) {
        var custom = res && res.value;
        if (Array.isArray(custom)) {
          state.customTemplates = custom;
        }
        initUI();
      })
      .catch(function () {
        initUI();
      });
  }

  function savePrefs() {
    var prefs = {
      currentEnv: state.currentEnv,
      currentTplId: state.currentTplId,
      highlightVariables: state.highlightVariables,
      showWatermark: state.showWatermark,
      showSeal: state.showSeal,
    };
    request('storage.set', { key: STORAGE_KEY_PREFS, value: prefs }).catch(function () {});
  }

  /* ------------------------------------------------------------------ 业务逻辑与数据处理 */
  function getAllTemplates() {
    return D.TEMPLATES.concat(state.customTemplates);
  }

  function getCurrentTemplate() {
    var all = getAllTemplates();
    for (var i = 0; i < all.length; i++) {
      if (all[i].id === state.currentTplId) return all[i];
    }
    return all[0] || D.TEMPLATES[0];
  }

  function getCurrentEnvironment() {
    return D.ENVIRONMENTS[state.currentEnv] || D.ENVIRONMENTS['qa-01'];
  }

  function initVariablesForCurrentTemplate(forceReset) {
    var tpl = getCurrentTemplate();
    var env = getCurrentEnvironment();
    if (!tpl) return;

    if (forceReset || !state.variableValues || Object.keys(state.variableValues).length === 0) {
      state.variableValues = {};
      var vars = tpl.variables || [];
      for (var i = 0; i < vars.length; i++) {
        var v = vars[i];
        state.variableValues[v.key] = v.default;
      }
    }

    // 联动当前环境特有参数
    state.variableValues.party_a = env.partyA.name;
    state.variableValues.party_a_uscc = env.partyA.uscc;
    state.variableValues.env_code = env.code;
    state.variableValues.env_name = env.name;
    state.variableValues.env_badge = env.badge;

    // 如果是默认合同号，自动带有当前环境前缀
    if (state.variableValues.contract_no && forceReset) {
      state.variableValues.contract_no = D.generateRandomContractNo(
        env.code,
        tpl.code ? tpl.code.replace(/[^A-Z0-9]/g, '') : 'HT',
      );
    }
  }

  function renderEnvironmentSelect() {
    var sel = $('env-select');
    if (!sel) return;
    sel.innerHTML = '';
    Object.keys(D.ENVIRONMENTS).forEach(function (envKey) {
      var item = D.ENVIRONMENTS[envKey];
      var opt = document.createElement('option');
      opt.value = item.id;
      opt.textContent = item.badge + ' - ' + item.name;
      if (item.id === state.currentEnv) opt.selected = true;
      sel.appendChild(opt);
    });

    var envBadge = $('current-env-indicator');
    var curEnv = getCurrentEnvironment();
    if (envBadge && curEnv) {
      envBadge.textContent = curEnv.badge;
      envBadge.style.backgroundColor = curEnv.color;
      envBadge.style.color = '#ffffff';
    }
  }

  function renderCategoryTabs() {
    var container = $('category-tabs');
    if (!container) return;
    var categories = [
      { id: 'all', label: '全部模板' },
      { id: 'software', label: '软件订阅' },
      { id: 'development', label: '委托开发' },
      { id: 'confidential', label: '保密协议' },
      { id: 'infrastructure', label: '基础设施' },
      { id: 'hr', label: '人事聘用' },
      { id: 'license', label: '商业许可' },
    ];

    container.innerHTML = '';
    categories.forEach(function (cat) {
      var btn = document.createElement('button');
      btn.className = 'cat-pill' + (state.categoryFilter === cat.id ? ' active' : '');
      btn.textContent = cat.label;
      btn.onclick = function () {
        state.categoryFilter = cat.id;
        renderCategoryTabs();
        renderTemplateList();
      };
      container.appendChild(btn);
    });
  }

  function renderTemplateList() {
    var list = $('template-list');
    if (!list) return;
    list.innerHTML = '';

    var all = getAllTemplates();
    var filtered = all.filter(function (t) {
      if (state.categoryFilter !== 'all' && t.category !== state.categoryFilter) {
        return false;
      }
      if (state.searchQuery) {
        var q = state.searchQuery.toLowerCase();
        var matchTitle = (t.title || '').toLowerCase().indexOf(q) !== -1;
        var matchCode = (t.code || '').toLowerCase().indexOf(q) !== -1;
        var matchDesc = (t.description || '').toLowerCase().indexOf(q) !== -1;
        if (!matchTitle && !matchCode && !matchDesc) return false;
      }
      return true;
    });

    $('template-count').textContent = '(' + filtered.length + ')';

    if (filtered.length === 0) {
      var empty = document.createElement('div');
      empty.className = 'empty-state';
      empty.textContent = '未找到匹配的合同模板';
      list.appendChild(empty);
      return;
    }

    filtered.forEach(function (tpl) {
      var item = document.createElement('div');
      var isCurrent = tpl.id === state.currentTplId;
      item.className = 'tpl-item' + (isCurrent ? ' selected' : '');
      item.onclick = function () {
        if (state.currentTplId === tpl.id) return;
        state.currentTplId = tpl.id;
        initVariablesForCurrentTemplate(true);
        savePrefs();
        renderTemplateList();
        renderCurrentTemplateView();
      };

      var topRow = document.createElement('div');
      topRow.className = 'tpl-top-row';

      var codeBadge = document.createElement('span');
      codeBadge.className = 'badge badge-mono';
      codeBadge.textContent = tpl.code;

      var verBadge = document.createElement('span');
      verBadge.className = 'badge badge-subtle';
      verBadge.textContent = tpl.version;

      topRow.appendChild(codeBadge);
      topRow.appendChild(verBadge);

      var titleEl = document.createElement('div');
      titleEl.className = 'tpl-title';
      titleEl.textContent = tpl.title;

      var descEl = document.createElement('div');
      descEl.className = 'tpl-desc';
      descEl.textContent = tpl.description || '';

      var bottomRow = document.createElement('div');
      bottomRow.className = 'tpl-bottom-row';

      var catTag = document.createElement('span');
      catTag.className = 'tpl-tag';
      catTag.textContent = tpl.categoryName || tpl.category;

      var dateTag = document.createElement('span');
      dateTag.className = 'tpl-date';
      dateTag.textContent = tpl.updatedAt;

      bottomRow.appendChild(catTag);
      bottomRow.appendChild(dateTag);

      item.appendChild(topRow);
      item.appendChild(titleEl);
      item.appendChild(descEl);
      item.appendChild(bottomRow);

      list.appendChild(item);
    });
  }

  function renderCurrentTemplateView() {
    var tpl = getCurrentTemplate();
    if (!tpl) return;

    // 头部信息
    $('cur-tpl-title').textContent = tpl.title;
    $('cur-tpl-code').textContent = tpl.code;
    $('cur-tpl-version').textContent = tpl.version;
    $('cur-tpl-status').textContent = tpl.status || '已定稿';
    $('cur-tpl-category').textContent = tpl.categoryName || tpl.category;

    // 根据当前标签页渲染主内容
    renderTabNav();
    if (state.currentTab === 'preview') {
      renderContractPreview();
    } else if (state.currentTab === 'variables') {
      renderVariablesEditor();
    } else if (state.currentTab === 'source') {
      renderSourceView();
    } else if (state.currentTab === 'env_info') {
      renderEnvInfoView();
    }
  }

  function renderTabNav() {
    var tabs = ['preview', 'variables', 'source', 'env_info'];
    tabs.forEach(function (tabId) {
      var btn = $('tab-btn-' + tabId);
      var panel = $('tab-panel-' + tabId);
      if (btn) {
        btn.classList.toggle('active', state.currentTab === tabId);
      }
      if (panel) {
        panel.classList.toggle('active', state.currentTab === tabId);
      }
    });
  }

  /* ------------------------------------------------------------------ 预览渲染 */
  function renderContractPreview() {
    var panel = $('tab-panel-preview');
    if (!panel) return;

    var tpl = getCurrentTemplate();
    var env = getCurrentEnvironment();
    var raw = tpl.content || '';

    // 替换所有占位符
    var htmlContent = raw.replace(/\{\{([a-zA-Z0-9_]+)\}\}/g, function (match, key) {
      var val = state.variableValues[key];
      if (val === undefined || val === '') {
        val = '【' + key + ' 待填】';
      }
      if (state.highlightVariables) {
        return (
          '<span class="var-highlight" title="变量: ' + key + '">' + escapeHtml(val) + '</span>'
        );
      }
      return escapeHtml(val);
    });

    // 简单 Markdown 转 HTML
    var lines = htmlContent.split('\n');
    var parsedHtml = [];
    var inList = false;

    lines.forEach(function (line) {
      var trimmed = line.trim();
      if (!trimmed) {
        if (inList) {
          parsedHtml.push('</ul>');
          inList = false;
        }
        parsedHtml.push('<div class="spacer"></div>');
        return;
      }

      if (trimmed.indexOf('# ') === 0) {
        if (inList) {
          parsedHtml.push('</ul>');
          inList = false;
        }
        parsedHtml.push('<h1 class="contract-doc-title">' + trimmed.slice(2) + '</h1>');
      } else if (trimmed.indexOf('### ') === 0) {
        if (inList) {
          parsedHtml.push('</ul>');
          inList = false;
        }
        parsedHtml.push('<h3 class="contract-section-title">' + trimmed.slice(4) + '</h3>');
      } else if (trimmed.indexOf('---') === 0) {
        if (inList) {
          parsedHtml.push('</ul>');
          inList = false;
        }
        parsedHtml.push('<hr class="contract-divider"/>');
      } else if (trimmed.indexOf('- ') === 0) {
        if (!inList) {
          parsedHtml.push('<ul class="contract-list">');
          inList = true;
        }
        var liContent = formatBold(trimmed.slice(2));
        parsedHtml.push('<li>' + liContent + '</li>');
      } else if (/^\d+\.\s/.test(trimmed)) {
        if (inList) {
          parsedHtml.push('</ul>');
          inList = false;
        }
        parsedHtml.push('<p class="contract-clause">' + formatBold(trimmed) + '</p>');
      } else {
        if (inList) {
          parsedHtml.push('</ul>');
          inList = false;
        }
        parsedHtml.push('<p class="contract-p">' + formatBold(trimmed) + '</p>');
      }
    });

    if (inList) parsedHtml.push('</ul>');

    // 组装印章与签署区域
    var sealHtml = '';
    if (state.showSeal && env && env.seal) {
      sealHtml = [
        '<div class="contract-signing-section">',
        '  <div class="signing-col">',
        '    <div class="sign-title">甲方（采购/委托方）电子印章：</div>',
        '    <div class="mock-seal-box">',
        '      <div class="mock-seal">',
        '        <div class="seal-star">★</div>',
        '        <div class="seal-text">' + escapeHtml(env.seal.text) + '</div>',
        '        <div class="seal-type">' + escapeHtml(env.seal.type) + '</div>',
        '        <div class="seal-code">' + escapeHtml(env.seal.code) + '</div>',
        '      </div>',
        '    </div>',
        '    <div class="sign-date">签章核验状态：<span class="badge badge-success">测试环境数字验签通过</span></div>',
        '  </div>',
        '  <div class="signing-col">',
        '    <div class="sign-title">乙方（服务/受托方）签章：</div>',
        '    <div class="mock-seal-box">',
        '      <div class="mock-seal mock-seal-partyb">',
        '        <div class="seal-star">★</div>',
        '        <div class="seal-text">' +
          escapeHtml(state.variableValues.party_b || '模拟合作企业') +
          '</div>',
        '        <div class="seal-type">业务专用章 (模拟)</div>',
        '        <div class="seal-code">TEST-B-2026-0901</div>',
        '      </div>',
        '    </div>',
        '    <div class="sign-date">签署时间：' +
          escapeHtml(state.variableValues.effective_date || '2026-09-04') +
          '</div>',
        '  </div>',
        '</div>',
      ].join('\n');
    }

    var watermarkHtml = state.showWatermark
      ? '<div class="watermark-overlay" data-text="' +
        env.name +
        ' · 仅供测试联调 · 严禁商用"></div>'
      : '';

    panel.innerHTML = [
      '<div class="paper-container">',
      watermarkHtml,
      '  <div class="paper-page">',
      parsedHtml.join('\n'),
      sealHtml,
      '  </div>',
      '</div>',
    ].join('\n');
  }

  function formatBold(str) {
    return str.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  }

  /* ------------------------------------------------------------------ 变量编辑面板 */
  function renderVariablesEditor() {
    var panel = $('tab-panel-variables');
    if (!panel) return;

    var tpl = getCurrentTemplate();
    var vars = tpl.variables || [];

    var html = [
      '<div class="var-editor-header">',
      '  <div class="var-editor-title">当前模板占位符参数设置（实时影响预览）</div>',
      '  <div class="var-actions">',
      '    <button class="btn btn-primary" id="btn-mock-random">🎲 随机生成测试数据</button>',
      '    <button class="btn btn-secondary" id="btn-reset-vars">🔄 重置为默认</button>',
      '  </div>',
      '</div>',
      '<div class="var-form-grid">',
    ];

    vars.forEach(function (v) {
      var curVal =
        state.variableValues[v.key] !== undefined ? state.variableValues[v.key] : v.default;
      html.push('<div class="var-form-item">');
      html.push(
        '  <label class="var-label"><span class="var-key-tag">{{' +
          v.key +
          '}}</span> ' +
          escapeHtml(v.label) +
          '</label>',
      );

      if (v.type === 'select' && Array.isArray(v.options)) {
        html.push('  <select class="var-input" data-var-key="' + v.key + '">');
        v.options.forEach(function (opt) {
          var sel = opt === curVal ? ' selected' : '';
          html.push(
            '    <option value="' +
              escapeHtml(opt) +
              '"' +
              sel +
              '>' +
              escapeHtml(opt) +
              '</option>',
          );
        });
        html.push('  </select>');
      } else if (v.type === 'amount') {
        html.push('  <div class="input-with-action">');
        html.push(
          '    <input type="text" class="var-input" data-var-key="' +
            v.key +
            '" value="' +
            escapeHtml(curVal) +
            '"/>',
        );
        html.push(
          '    <button class="btn btn-sm btn-subtle btn-calc-amount" data-target-key="' +
            v.key +
            '" title="自动同步大写金额">转大写</button>',
        );
        html.push('  </div>');
      } else {
        html.push(
          '  <input type="' +
            (v.type === 'date' ? 'date' : 'text') +
            '" class="var-input" data-var-key="' +
            v.key +
            '" value="' +
            escapeHtml(curVal) +
            '"/>',
        );
      }
      html.push('</div>');
    });

    html.push('</div>');
    panel.innerHTML = html.join('\n');

    // 绑定表单输入事件
    panel.querySelectorAll('.var-input').forEach(function (input) {
      input.addEventListener('input', function (e) {
        var key = e.target.getAttribute('data-var-key');
        var val = e.target.value;
        state.variableValues[key] = val;

        // 若修改了 amount 且模板里有 amount_words，自动计算大写
        if (key === 'amount' && state.variableValues.amount_words !== undefined) {
          var words = D.amountToWords(val);
          state.variableValues.amount_words = words;
          var wordsInput = panel.querySelector('.var-input[data-var-key="amount_words"]');
          if (wordsInput) wordsInput.value = words;
        }
      });
    });

    panel.querySelectorAll('.btn-calc-amount').forEach(function (btn) {
      btn.addEventListener('click', function () {
        var tKey = btn.getAttribute('data-target-key');
        var val = state.variableValues[tKey];
        var words = D.amountToWords(val);
        if (words) {
          state.variableValues.amount_words = words;
          var wordsInput = panel.querySelector('.var-input[data-var-key="amount_words"]');
          if (wordsInput) wordsInput.value = words;
          notify('大写金额已更新', words);
        }
      });
    });

    var btnRandom = panel.querySelector('#btn-mock-random');
    if (btnRandom) {
      btnRandom.onclick = generateRandomTestData;
    }

    var btnReset = panel.querySelector('#btn-reset-vars');
    if (btnReset) {
      btnReset.onclick = function () {
        initVariablesForCurrentTemplate(true);
        renderVariablesEditor();
        notify('重置成功', '已恢复为模板默认测试数据');
      };
    }
  }

  function generateRandomTestData() {
    var tpl = getCurrentTemplate();
    var env = getCurrentEnvironment();
    if (!tpl) return;

    // 随机生成合同号
    state.variableValues.contract_no = D.generateRandomContractNo(
      env.code,
      tpl.code.replace(/[^A-Z0-9]/g, ''),
    );
    // 随机乙方公司
    state.variableValues.party_b = D.generateRandomCompanyName();
    state.variableValues.receiving_party = state.variableValues.party_b;
    // 随机联系人
    state.variableValues.contact_b = D.generateRandomName();
    state.variableValues.phone_b = D.generateRandomPhone();
    // 随机金额
    if (state.variableValues.amount !== undefined) {
      var randAmt = (Math.floor(50 + Math.random() * 450) * 1000).toFixed(2);
      state.variableValues.amount = randAmt;
      state.variableValues.amount_words = D.amountToWords(randAmt);
    }
    // 随机个人
    if (state.variableValues.employee_name !== undefined) {
      state.variableValues.employee_name = D.generateRandomName();
      state.variableValues.id_card =
        '330106199' + Math.floor(100000000 + Math.random() * 900000000) + 'X';
    }

    renderVariablesEditor();
    notify('生成成功', '已填充高仿真测试随机数据');
  }

  /* ------------------------------------------------------------------ 模板源码面板 */
  function renderSourceView() {
    var panel = $('tab-panel-source');
    if (!panel) return;
    var tpl = getCurrentTemplate();

    panel.innerHTML = [
      '<div class="source-view-header">',
      '  <div class="source-title">Markdown 原始模板内容（可直接复制接入测试代码）</div>',
      '  <div class="source-actions">',
      '    <button class="btn btn-secondary" id="btn-copy-src">📋 复制模板源码</button>',
      '    <button class="btn btn-secondary" id="btn-copy-json">📦 复制参数 JSON</button>',
      '  </div>',
      '</div>',
      '<pre class="code-block"><code id="code-content">' +
        escapeHtml(tpl.content) +
        '</code></pre>',
    ].join('\n');

    var btnCopySrc = panel.querySelector('#btn-copy-src');
    if (btnCopySrc) {
      btnCopySrc.onclick = function () {
        copyToClipboard(tpl.content, '模板 Markdown 源码已复制');
      };
    }

    var btnCopyJson = panel.querySelector('#btn-copy-json');
    if (btnCopyJson) {
      btnCopyJson.onclick = function () {
        var jsonStr = JSON.stringify(state.variableValues, null, 2);
        copyToClipboard(jsonStr, '测试参数 JSON 已复制');
      };
    }
  }

  /* ------------------------------------------------------------------ 环境信息面板 */
  function renderEnvInfoView() {
    var panel = $('tab-panel-env_info');
    if (!panel) return;
    var env = getCurrentEnvironment();

    panel.innerHTML = [
      '<div class="env-info-card">',
      '  <div class="env-info-title">测试环境主体与回调网关配置</div>',
      '  <div class="env-grid">',
      '    <div class="env-item"><b>当前环境：</b>' +
        escapeHtml(env.name) +
        ' (' +
        escapeHtml(env.code) +
        ')</div>',
      '    <div class="env-item"><b>签约甲方主体：</b>' + escapeHtml(env.partyA.name) + '</div>',
      '    <div class="env-item"><b>统一社会信用代码：</b><code>' +
        escapeHtml(env.partyA.uscc) +
        '</code></div>',
      '    <div class="env-item"><b>法定代表人：</b>' +
        escapeHtml(env.partyA.legalPerson) +
        '</div>',
      '    <div class="env-item"><b>对公结算银行：</b>' + escapeHtml(env.partyA.bank) + '</div>',
      '    <div class="env-item"><b>银行账号：</b><code>' +
        escapeHtml(env.partyA.account) +
        '</code></div>',
      '    <div class="env-item"><b>联系电话：</b>' + escapeHtml(env.partyA.phone) + '</div>',
      '    <div class="env-item"><b>注册经营地址：</b>' + escapeHtml(env.partyA.address) + '</div>',
      '    <div class="env-item"><b>电子印章编号：</b><code>' +
        escapeHtml(env.seal.code) +
        '</code> (' +
        escapeHtml(env.seal.type) +
        ')</div>',
      '    <div class="env-item env-item-full"><b>异步状态变更回调地址 (Mock Webhook)：</b><br/><code>' +
        escapeHtml(env.callbackUrl) +
        '</code></div>',
      '  </div>',
      '</div>',
      '<div class="env-db-section">',
      '  <div class="env-info-title">测试数据库连通性联调</div>',
      '  <p class="sub">可通过宿主当前活动连接检验测试库中的 contract_templates 表定义：</p>',
      '  <div class="row">',
      '    <button class="btn btn-primary" id="btn-test-db-query">执行测试库探测 (SELECT 1)</button>',
      '    <span id="db-probe-status" class="status-tip"></span>',
      '  </div>',
      '</div>',
    ].join('\n');

    var btnProbe = panel.querySelector('#btn-test-db-query');
    if (btnProbe) {
      btnProbe.onclick = testDbProbe;
    }
  }

  function testDbProbe() {
    var statusEl = $('db-probe-status');
    if (!state.activeConnection && state.allConnections.length === 0) {
      if (statusEl)
        statusEl.textContent = '提示：宿主未保存任何数据库连接，请先在 DataZen 连接测试库。';
      notify('未挂接数据库', '请先在 DataZen 中连接测试数据库');
      return;
    }

    var connId = state.activeConnection ? state.activeConnection.id : state.allConnections[0].id;
    if (statusEl) statusEl.textContent = '正在探测连接 ' + connId + ' ...';

    request('command.invoke', {
      connectionId: connId,
      command: 'query',
      args: { sql: 'SELECT 1 AS test_probe, NOW() AS test_time' },
    })
      .then(function (res) {
        if (statusEl) statusEl.textContent = '探测成功！测试库响应正常。';
        notify('测试数据库联通正常', '已成功执行连通性探测');
      })
      .catch(function (err) {
        // 部分数据库如 SQLite 不支持 NOW()，回退 SELECT 1
        return request('command.invoke', {
          connectionId: connId,
          command: 'query',
          args: { sql: 'SELECT 1' },
        }).then(function () {
          if (statusEl) statusEl.textContent = '探测成功！测试库响应正常 (SELECT 1)。';
          notify('测试数据库联通正常', '已成功连接');
        });
      })
      .catch(function (e) {
        if (statusEl) statusEl.textContent = '探测失败: ' + (e && e.message ? e.message : e);
      });
  }

  /* ------------------------------------------------------------------ 剪贴板与打印 */
  function copyToClipboard(text, successMsg) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard
        .writeText(text)
        .then(function () {
          notify('复制成功', successMsg || '');
        })
        .catch(function () {
          fallbackCopy(text, successMsg);
        });
    } else {
      fallbackCopy(text, successMsg);
    }
  }

  function fallbackCopy(text, successMsg) {
    var ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    try {
      document.execCommand('copy');
      notify('复制成功', successMsg || '');
    } catch (e) {
      notify('复制失败', '请手动选中复制');
    }
    document.body.removeChild(ta);
  }

  function getRenderedPlainText() {
    var tpl = getCurrentTemplate();
    if (!tpl) return '';
    return (tpl.content || '').replace(/\{\{([a-zA-Z0-9_]+)\}\}/g, function (m, k) {
      return state.variableValues[k] !== undefined ? state.variableValues[k] : m;
    });
  }

  /* ------------------------------------------------------------------ UI 交互绑定 */
  function initUI() {
    renderEnvironmentSelect();
    renderCategoryTabs();
    initVariablesForCurrentTemplate(false);
    renderTemplateList();
    renderCurrentTemplateView();

    // 环境选择器变更
    var envSelect = $('env-select');
    if (envSelect) {
      envSelect.addEventListener('change', function (e) {
        state.currentEnv = e.target.value;
        savePrefs();
        renderEnvironmentSelect();
        initVariablesForCurrentTemplate(true);
        renderCurrentTemplateView();
        notify('环境已切换', getCurrentEnvironment().name);
      });
    }

    // 搜索框
    var searchInput = $('search-input');
    if (searchInput) {
      searchInput.addEventListener('input', function (e) {
        state.searchQuery = e.target.value;
        renderTemplateList();
      });
    }

    // 标签页切换
    var tabs = ['preview', 'variables', 'source', 'env_info'];
    tabs.forEach(function (tabId) {
      var btn = $('tab-btn-' + tabId);
      if (btn) {
        btn.onclick = function () {
          state.currentTab = tabId;
          renderCurrentTemplateView();
        };
      }
    });

    // 视图选项：高亮变量
    var chkHighlight = $('chk-highlight');
    if (chkHighlight) {
      chkHighlight.checked = state.highlightVariables;
      chkHighlight.onchange = function () {
        state.highlightVariables = chkHighlight.checked;
        savePrefs();
        if (state.currentTab === 'preview') renderContractPreview();
      };
    }

    // 视图选项：水印
    var chkWatermark = $('chk-watermark');
    if (chkWatermark) {
      chkWatermark.checked = state.showWatermark;
      chkWatermark.onchange = function () {
        state.showWatermark = chkWatermark.checked;
        savePrefs();
        if (state.currentTab === 'preview') renderContractPreview();
      };
    }

    // 视图选项：印章
    var chkSeal = $('chk-seal');
    if (chkSeal) {
      chkSeal.checked = state.showSeal;
      chkSeal.onchange = function () {
        state.showSeal = chkSeal.checked;
        savePrefs();
        if (state.currentTab === 'preview') renderContractPreview();
      };
    }

    // 顶栏操作按钮
    var btnCopyRendered = $('btn-copy-rendered');
    if (btnCopyRendered) {
      btnCopyRendered.onclick = function () {
        copyToClipboard(getRenderedPlainText(), '渲染后合同全文已复制到剪贴板');
      };
    }

    var btnPrint = $('btn-print');
    if (btnPrint) {
      btnPrint.onclick = function () {
        window.print();
      };
    }

    // 快捷新建自定义模板
    var btnNewTpl = $('btn-new-tpl');
    if (btnNewTpl) {
      btnNewTpl.onclick = openNewTemplateDialog;
    }
  }

  function openNewTemplateDialog() {
    var title = prompt('请输入新测试合同模板名称：', '测试业务协议 (自定义)');
    if (!title) return;
    var code = 'TPL-CUSTOM-' + String(Math.floor(100 + Math.random() * 900));
    var newTpl = {
      id: 'custom-' + Date.now(),
      code: code,
      title: title,
      category: 'software',
      categoryName: '自定义模板',
      version: 'v1.0-draft',
      status: '草稿',
      description: '测试人员新增的自定义业务测试模板',
      updatedAt: '2026-09-04',
      variables: [
        { key: 'contract_no', label: '合同编号', default: code + '-2026', type: 'text' },
        {
          key: 'party_a',
          label: '甲方名称',
          default: getCurrentEnvironment().partyA.name,
          type: 'text',
        },
        { key: 'party_b', label: '乙方名称', default: '测试合作商有限公司', type: 'text' },
        { key: 'amount', label: '合同金额', default: '100000.00', type: 'amount' },
        { key: 'effective_date', label: '生效日期', default: '2026-09-04', type: 'date' },
      ],
      content: [
        '# ' + title,
        '',
        '**合同编号**：{{contract_no}}  ',
        '**生效日期**：{{effective_date}}  ',
        '',
        '---',
        '',
        '### 签约方',
        '- 甲方：{{party_a}}',
        '- 乙方：{{party_b}}',
        '',
        '### 条款约定',
        '1. 本合同为测试环境临时联调创建的自定义协议草稿。',
        '2. 合同总金额为人民币 {{amount}} 元。',
      ].join('\n'),
    };

    state.customTemplates.push(newTpl);
    state.currentTplId = newTpl.id;
    initVariablesForCurrentTemplate(true);
    request('storage.set', { key: STORAGE_KEY_CUSTOM_TPLS, value: state.customTemplates }).catch(
      function () {},
    );
    renderTemplateList();
    renderCurrentTemplateView();
    notify('创建成功', '自定义测试模板已保存');
  }

  /* ------------------------------------------------------------------ 初始化入口 */
  window.addEventListener('message', onMessage);
  startHandshake();

  // 若无宿主通信（如纯浏览器独立预览），延迟兜底初始化
  setTimeout(function () {
    if (!state.variableValues || Object.keys(state.variableValues).length === 0) {
      initUI();
    }
  }, 1200);
})();
