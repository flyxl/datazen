(function () {
  const ORIGIN_PATH_HINT = '/datazen';

  const STR = {
    en: {
      nav: [
        {
          label: 'Product',
          href: 'features.html',
          items: [
            { href: 'ai.html', label: 'AI Assistant' },
            { href: 'charts.html', label: 'Charts' },
            { href: 'workflow.html', label: 'Workflows' },
            { href: 'features.html#mcp', label: 'MCP Server / Client' },
            { href: 'databases.html', label: 'Databases' },
          ],
        },
        { href: 'index.html#why', label: 'Why DataZen' },
        { href: 'manual.html', label: 'Docs' },
        { href: 'https://github.com/flyxl/datazen', label: 'GitHub', external: true },
      ],
      downloadCta: 'Download Free',
      menuAria: 'Menu',
      productMenuAria: 'Toggle product menu',
      footerAbout:
        'The AI-powered database workspace for developers. Query, debug, analyze and automate your databases — locally, from one lightweight desktop app. Licensed under GPLv3.',
      footerProduct: 'Product',
      footerLinks: 'Links',
      footerFeatures: 'All features',
      footerAi: 'AI Assistant',
      footerCharts: 'Charts',
      footerWorkflow: 'Workflows',
      footerMcp: 'MCP Server / Client',
      footerManual: 'Docs',
      footerDatabases: 'Databases',
      footerGithub: 'GitHub',
      footerReleases: 'Releases',
      footerIssues: 'Issues',
      footerContact: 'Contact',
      langSwitchLabel: '中',
      langSwitchAria: 'Switch to Chinese',
      themeToggleAria: 'Toggle light / dark theme',
    },
    zh: {
      nav: [
        {
          label: '产品',
          href: 'features.html',
          items: [
            { href: 'ai.html', label: 'AI 助手' },
            { href: 'charts.html', label: '图表可视化' },
            { href: 'workflow.html', label: 'Workflow 自动化' },
            { href: 'features.html#mcp', label: 'MCP Server / Client' },
            { href: 'databases.html', label: '数据库支持' },
          ],
        },
        { href: 'index.html#why', label: '为什么是 DataZen' },
        { href: 'manual.html', label: '使用手册' },
        { href: 'https://github.com/flyxl/datazen', label: 'GitHub', external: true },
      ],
      downloadCta: '免费下载',
      menuAria: '菜单',
      productMenuAria: '展开产品菜单',
      footerAbout:
        '面向开发者的 AI 数据库工作台：查询、排障、分析、自动化，都在一款轻量桌面应用里本地完成。GPLv3 协议开源。',
      footerProduct: '产品',
      footerLinks: '链接',
      footerFeatures: '功能总览',
      footerAi: 'AI 助手',
      footerCharts: '图表可视化',
      footerWorkflow: 'Workflow 自动化',
      footerMcp: 'MCP Server / Client',
      footerManual: '使用手册',
      footerDatabases: '数据库支持',
      footerGithub: 'GitHub 仓库',
      footerReleases: '下载中心',
      footerIssues: '反馈 Issue',
      footerContact: '联系作者',
      langSwitchLabel: 'EN',
      langSwitchAria: '切换到英文',
      themeToggleAria: '切换亮色 / 暗色主题',
    },
  };

  function detectLocale() {
    const path = location.pathname;
    return /\/zh(\/|$)/.test(path) ? 'zh' : 'en';
  }

  function assetBase(locale) {
    return locale === 'zh' ? '../assets/' : 'assets/';
  }

  function currentFile() {
    const raw = location.pathname.split('/').pop() || '';
    if (!raw || raw.indexOf('.') === -1) return 'index.html';
    return raw;
  }

  function counterpartHref(locale) {
    const file = currentFile();
    if (locale === 'en') {
      return file === 'index.html' ? 'zh/' : 'zh/' + file;
    }
    return file === 'index.html' ? '../' : '../' + file;
  }

  function renderNav() {
    const host = document.getElementById('site-header');
    if (!host) return;
    const locale = detectLocale();
    const t = STR[locale];
    const assets = assetBase(locale);
    const current = currentFile();

    function isActive(href) {
      return href.split('#')[0] === current;
    }

    const links = t.nav
      .map(function (n) {
        if (n.items) {
          const groupActive = n.items.some(function (it) {
            return isActive(it.href);
          });
          const items = n.items
            .map(function (it) {
              return (
                '<a href="' +
                it.href +
                '"' +
                (isActive(it.href) ? ' class="active"' : '') +
                '>' +
                it.label +
                '</a>'
              );
            })
            .join('');
          return (
            '<div class="nav-drop' +
            (groupActive ? ' active' : '') +
            '">' +
            '<a class="nav-drop-label" href="' +
            n.href +
            '">' +
            n.label +
            '</a>' +
            '<button class="nav-drop-caret" type="button" aria-label="' +
            t.productMenuAria +
            '" aria-expanded="false">▾</button>' +
            '<div class="nav-drop-menu">' +
            items +
            '</div>' +
            '</div>'
          );
        }
        const active = isActive(n.href) && n.href.indexOf('#') === -1 ? ' class="active"' : '';
        const ext = n.external ? ' target="_blank" rel="noopener"' : '';
        return '<a href="' + n.href + '"' + ext + active + '>' + n.label + '</a>';
      })
      .join('');

    const langHref = counterpartHref(locale);
    host.innerHTML =
      '<nav class="nav"><div class="wrap nav-inner">' +
      '<a class="nav-logo" href="index.html"><img src="' +
      assets +
      'logo.png" alt="DataZen">DataZen</a>' +
      '<button class="nav-toggle" aria-label="' +
      t.menuAria +
      '">☰</button>' +
      '<div class="nav-links">' +
      links +
      '</div>' +
      '<div class="nav-tools">' +
      '<button class="theme-toggle" type="button" data-theme-toggle aria-label="' +
      t.themeToggleAria +
      '" title="' +
      t.themeToggleAria +
      '"><span data-theme-icon>🌙</span></button>' +
      '<a class="nav-lang" href="' +
      langHref +
      '" aria-label="' +
      t.langSwitchAria +
      '">' +
      t.langSwitchLabel +
      '</a>' +
      '</div>' +
      '<a class="btn btn-primary nav-cta" href="download.html">' +
      t.downloadCta +
      '</a>' +
      '</div></nav>';

    const toggle = host.querySelector('.nav-toggle');
    toggle.addEventListener('click', function () {
      host.querySelector('.nav-links').classList.toggle('open');
    });
    host.querySelectorAll('.nav-drop-caret').forEach(function (btn) {
      btn.addEventListener('click', function (e) {
        e.preventDefault();
        const drop = btn.closest('.nav-drop');
        const open = drop.classList.toggle('open');
        btn.setAttribute('aria-expanded', open ? 'true' : 'false');
      });
    });
    const themeBtn = host.querySelector('[data-theme-toggle]');
    themeBtn.addEventListener('click', toggleTheme);
    syncThemeIcon(host.querySelector('[data-theme-icon]'));
  }

  // ── Theme: dark default, persisted in localStorage, applied pre-paint ──

  const THEME_KEY = 'dz-theme';

  function currentTheme() {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === 'light' || saved === 'dark') return saved;
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  function applyTheme(theme) {
    document.documentElement.setAttribute('data-theme', theme);
  }

  function syncThemeIcon(el) {
    if (!el) return;
    el.textContent = document.documentElement.getAttribute('data-theme') === 'light' ? '☀️' : '🌙';
  }

  function toggleTheme() {
    const next = currentTheme() === 'dark' ? 'light' : 'dark';
    localStorage.setItem(THEME_KEY, next);
    applyTheme(next);
    document.querySelectorAll('[data-theme-icon]').forEach(syncThemeIcon);
  }

  // Set the attribute as early as possible to avoid theme flash.
  applyTheme(currentTheme());

  // Follow system theme when user hasn't manually chosen one.
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function (e) {
    if (localStorage.getItem(THEME_KEY)) return;
    applyTheme(e.matches ? 'dark' : 'light');
    document.querySelectorAll('[data-theme-icon]').forEach(syncThemeIcon);
  });

  // ── Hero demo: light up the NL → SQL → result → chart pipeline in a loop ──

  function initHeroDemo() {
    const demo = document.querySelector('[data-demo]');
    if (!demo) return;
    const stages = Array.prototype.slice.call(demo.querySelectorAll('.demo-stage'));
    if (!stages.length) return;

    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      stages.forEach(function (s) {
        s.classList.add('active');
      });
      return;
    }

    let i = 1;
    setInterval(function () {
      i = i > stages.length ? 1 : i;
      stages.forEach(function (s, idx) {
        s.classList.toggle('active', idx < i);
      });
      i += 1;
    }, 1500);
  }

  // ── Platform detection: hero download button shows OS-specific label ──
  function initPlatformDetect() {
    var btn = document.getElementById('hero-download');
    if (!btn) return;
    var ua = navigator.userAgent || '';
    var isMac = /Macintosh|Mac OS X/.test(ua);
    var isWin = /Windows/.test(ua);
    var isLinux = /Linux/.test(ua) && !/Android/.test(ua);
    if (isMac) btn.textContent = 'Download for macOS';
    else if (isWin) btn.textContent = 'Download for Windows';
    else if (isLinux) btn.textContent = 'Download for Linux';

    // macOS notarization note
    if (isMac) {
      var note = document.getElementById('hero-notarize-note');
      if (note) note.style.display = '';
    }
  }

  // ── GitHub star badge ──
  function initStarBadge() {
    var el = document.getElementById('star-count');
    if (!el) return;
    fetch('https://api.github.com/repos/flyxl/datazen')
      .then(function (r) {
        return r.json();
      })
      .then(function (d) {
        if (d && d.stargazers_count != null) {
          el.textContent = d.stargazers_count.toLocaleString();
        }
      })
      .catch(function () {});
  }

  // ── Gallery lightbox ──
  function initLightbox() {
    var overlay = document.createElement('div');
    overlay.className = 'lightbox-overlay';
    overlay.innerHTML = '<img src="" alt="" /><div class="lightbox-caption"></div>';
    document.body.appendChild(overlay);
    var lbImg = overlay.querySelector('img');
    var lbCap = overlay.querySelector('.lightbox-caption');

    function close() {
      overlay.classList.remove('active');
      document.body.style.overflow = '';
    }
    overlay.addEventListener('click', close);
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') close();
    });

    document.querySelectorAll('.gallery-img').forEach(function (img) {
      img.addEventListener('click', function () {
        lbImg.src = img.src;
        lbImg.alt = img.alt;
        lbCap.textContent = img.getAttribute('data-caption') || '';
        overlay.classList.add('active');
        document.body.style.overflow = 'hidden';
      });
    });
  }

  function renderFooter() {
    const host = document.getElementById('site-footer');
    if (!host) return;
    const t = STR[detectLocale()];
    host.innerHTML =
      '<footer class="footer"><div class="wrap">' +
      '<div class="footer-grid">' +
      '<div><h4>DataZen</h4><p>' +
      t.footerAbout +
      '</p></div>' +
      '<div><h4>' +
      t.footerProduct +
      '</h4>' +
      '<a href="features.html">' +
      t.footerFeatures +
      '</a>' +
      '<a href="ai.html">' +
      t.footerAi +
      '</a>' +
      '<a href="charts.html">' +
      t.footerCharts +
      '</a>' +
      '<a href="workflow.html">' +
      t.footerWorkflow +
      '</a>' +
      '<a href="features.html#mcp">' +
      t.footerMcp +
      '</a>' +
      '<a href="manual.html">' +
      t.footerManual +
      '</a>' +
      '<a href="databases.html">' +
      t.footerDatabases +
      '</a></div>' +
      '<div><h4>' +
      t.footerLinks +
      '</h4>' +
      '<a href="https://github.com/flyxl/datazen" target="_blank" rel="noopener">' +
      t.footerGithub +
      '</a>' +
      '<a href="https://github.com/flyxl/datazen/releases" target="_blank" rel="noopener">' +
      t.footerReleases +
      '</a>' +
      '<a href="https://github.com/flyxl/datazen/issues" target="_blank" rel="noopener">' +
      t.footerIssues +
      '</a>' +
      '<a href="mailto:wuxiaolongklws@gmail.com">' +
      t.footerContact +
      '</a></div>' +
      '</div>' +
      '<div class="copy">© 2026 DataZen · GPLv3 License · macOS / Windows / Linux</div>' +
      '</div></footer>';
  }

  // ORIGIN_PATH_HINT kept for documentation of Pages base; locale detection uses /zh/ only.
  void ORIGIN_PATH_HINT;

  document.addEventListener('DOMContentLoaded', function () {
    renderNav();
    renderFooter();
    initHeroDemo();
    initPlatformDetect();
    initStarBadge();
    initLightbox();
  });
})();
