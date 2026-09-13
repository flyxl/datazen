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
        {
          href: 'https://github.com/flyxl/datazen/blob/main/CHANGELOG.md',
          label: 'Changelog',
          external: true,
        },
        { href: 'https://github.com/flyxl/datazen', label: 'GitHub', external: true },
      ],
      downloadCta: 'Download Free',
      downloadCtaShort: 'Download',
      menuAria: 'Menu',
      productMenuAria: 'Toggle product menu',
      footerAbout:
        'The AI-powered database workspace for developers. Query, debug, analyze and automate your databases from one lightweight desktop app. Licensed under GPLv3.',
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
      footerChangelog: 'Changelog',
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
        {
          href: 'https://github.com/flyxl/datazen/blob/main/CHANGELOG.md',
          label: '更新日志',
          external: true,
        },
        { href: 'https://github.com/flyxl/datazen', label: 'GitHub', external: true },
      ],
      downloadCta: '免费下载',
      downloadCtaShort: '下载',
      menuAria: '菜单',
      productMenuAria: '展开产品菜单',
      footerAbout:
        '面向开发者的 AI 数据库工作台：查询、排障、分析、自动化，都在一款轻量桌面应用里完成。GPLv3 协议开源。',
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
      footerChangelog: '更新日志',
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
      '<a class="nav-star" href="https://github.com/flyxl/datazen" target="_blank" rel="noopener" aria-label="GitHub Stars">' +
      '<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25Z"/></svg>' +
      '<span id="nav-star-count"></span>' +
      '</a>' +
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
      '<a class="btn btn-primary nav-cta" href="download.html"><span class="nav-cta-full">' +
      t.downloadCta +
      '</span><span class="nav-cta-short">' +
      t.downloadCtaShort +
      '</span>' +
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

  // ── Release downloads: resolve the matching installer from the latest release ──
  function initReleaseDownloads() {
    var buttons = document.querySelectorAll('.platform-download');
    if (!buttons.length) return;

    var ua = navigator.userAgent || '';
    var isArmMac =
      /Macintosh/.test(ua) && /arm|aarch64/i.test(navigator.userAgentData?.architecture || '');
    var platform = /Windows/.test(ua)
      ? 'windows'
      : /Linux/.test(ua) && !/Android/.test(ua)
        ? 'linux'
        : 'macos';
    var type = platform === 'windows' ? 'nsis' : platform === 'linux' ? 'AppImage' : 'dmg';
    var releaseUrl = 'https://api.github.com/repos/flyxl/datazen/releases/latest';

    fetch(releaseUrl)
      .then(function (response) {
        if (!response.ok) throw new Error('Release lookup failed');
        return response.json();
      })
      .then(function (release) {
        var assets = (release.assets || []).filter(function (asset) {
          return asset && asset.browser_download_url;
        });
        buttons.forEach(function (button) {
          if (button.dataset.platform === platform) {
            var buttonArch = button.dataset.arch || (isArmMac ? 'arm64' : 'x64');
            var platformPattern = new RegExp(platform + '-' + buttonArch, 'i');
            var extensionPattern =
              platform === 'windows'
                ? /\.exe$/i
                : platform === 'linux'
                  ? /\.AppImage$/i
                  : /\.dmg$/i;
            var candidates = assets.filter(function (asset) {
              return (
                platformPattern.test(asset.name) &&
                extensionPattern.test(asset.name) &&
                (platform !== 'windows' || !/portable/i.test(asset.name))
              );
            });
            var match = candidates.sort(function (left, right) {
              var leftVariant = /-(?:all|akulaku)(?:[-.])/i.test(left.name) ? 1 : 0;
              var rightVariant = /-(?:all|akulaku)(?:[-.])/i.test(right.name) ? 1 : 0;
              return leftVariant - rightVariant;
            })[0];
            if (!match) return;
            button.href = match.browser_download_url;
            button.target = '_self';
            button.removeAttribute('rel');
          }
        });
      })
      .catch(function () {});
  }

  // ── GitHub star badge ──
  function initStarBadge() {
    var navEl = document.getElementById('nav-star-count');
    var heroEl = document.getElementById('star-count');
    if (!navEl && !heroEl) return;
    fetch('https://api.github.com/repos/flyxl/datazen')
      .then(function (r) {
        return r.json();
      })
      .then(function (d) {
        if (d && d.stargazers_count != null) {
          var text = d.stargazers_count.toLocaleString();
          if (navEl) navEl.textContent = text;
          if (heroEl) heroEl.textContent = text;
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
      '<a href="https://github.com/flyxl/datazen/blob/main/CHANGELOG.md" target="_blank" rel="noopener">' +
      t.footerChangelog +
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

  // ── Icon library loader (Lucide for general, Simple Icons for brands) ──

  /** Emoji → Lucide icon name mapping */
  const EMOJI_TO_LUCIDE = {
    '⚡': 'zap',
    '🛡️': 'shield-check',
    '🔌': 'plug',
    '👤': 'user',
    '🧩': 'puzzle',
    '📖': 'book-open',
    '🤖': 'bot',
    '📊': 'bar-chart-3',
    '⚙️': 'settings',
    '🛰️': 'satellite-dish',
    '📈': 'trending-up',
    '🥧': 'pie-chart',
    '✨': 'sparkles',
    '📦': 'package',
    '🔁': 'repeat',
    '🛟': 'life-buoy',
    '🖥️': 'monitor',
    '🗄️': 'hard-drive',
    '📡': 'radio-tower',
    '🐢': 'gauge',
    '🔍': 'search',
    '🩺': 'stethoscope',
    '💬': 'message-circle',
    '▶️': 'play',
    '📤': 'send',
    '⬇️': 'download',
    '🐘': 'database',
    '🐬': 'droplets',
    '✕': 'x',
    '✓': 'check',
  };

  /** Brand logo SVGs (Simple Icons) — used for OS logos */
  const BRAND_SVGS = {
    apple:
      '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.8-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"/></svg>',
    windows:
      '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M3 12.5h8.5V22l-8.5-1v-8.5zm0-1h8.5V2l-8.5 1v8.5zm9.5 1h9V2.1l-9 1V12.5zm0 1v9.4l9-1V13.5h-9z"/></svg>',
    linux:
      '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M12.504 0c-.155 0-.315.008-.48.021-4.226.333-3.105 4.807-3.17 6.298-.076 1.092-.3 1.953-1.05 3.02-.885 1.051-2.127 2.75-2.716 4.521-.278.832-.41 1.684-.287 2.489a.424.424 0 0 0-.11.135c-.26.268-.45.6-.663.839-.199.199-.485.267-.797.4-.313.136-.658.269-.864.68-.09.189-.136.394-.132.602 0 .199.027.4.055.536.058.399.116.728.04.97-.249.68-.28 1.145-.106 1.484.174.334.535.47.94.601.81.2 1.91.135 2.774.6.926.466 1.866.67 2.616.47.526-.116.97-.464 1.208-.946.587-.003 1.23-.269 2.26-.334.699-.058 1.574.267 2.577.2.025.134.063.198.114.333l.003.003c.391.778 1.113 1.368 1.884 1.43.199.016.379-.035.551-.132a.97.97 0 0 0 .149-.104l.003.003c.371-.301.586-.733.603-1.216.016-.483-.138-.958-.454-1.33a1.548 1.548 0 0 0-.167-.164c.449-.194.827-.515 1.093-.934.333-.516.478-1.14.349-1.81-.136-.664-.6-.84-.942-1.004-.342-.165-.474-.232-.474-.597 0-.136.027-.284.082-.401.334-.664.186-1.2-.249-1.845-.665-.988-2.145-1.242-2.62-1.974-.475-.731-.235-1.694.365-2.891.464-.925.52-2.086.447-2.868-.073-.781-.27-1.13-.27-1.13s.587-.265 1.232-.466c.645-.199 1.072-.565 1.072-.565s.131-.467.138-.866c.04-.731-.195-1.2-.52-1.665-.324-.466-.658-.6-.658-.6s.267-.798.25-1.464c-.017-.664-.448-1.264-.953-1.664-.504-.399-.892-.532-.892-.532s-.065-.665-.399-1.265c-.334-.599-1.13-.798-1.73-.899-.6-.099-1.13.067-1.13.067S9.856.006 9.192 0c-.664-.006-1.264.067-1.264.067S6.667-.132 6.067.466c-.334.333-.534.865-.534.865s-.732.067-1.332.665c-.6.599-.866 1.53-.866 1.53s-.532.2-1.065.932c-.532.732-.665 1.864-.665 1.864s-.599.134-.866.866c-.265.732.067 1.797.067 1.797s-.533.665-.666 1.464c-.133.799.133 1.465.133 1.465s-.666.466-.866 1.265c-.199.799.134 1.664.134 1.664s-.467.532-.6 1.398c-.133.864.199 1.596.199 1.596l.132 3.997z"/></svg>',
  };

  /** Replace emoji text nodes inside .icon containers with Lucide SVGs */
  function replaceEmojiIcons() {
    document.querySelectorAll('.card .icon, .uc-row .uc-ico').forEach(function (el) {
      var text = el.textContent.trim();
      // Brand logos for OS
      if (text === '🍎' && BRAND_SVGS.apple) {
        el.innerHTML = '<span class="si">' + BRAND_SVGS.apple + '</span>';
        return;
      }
      if (text === '🪟' && BRAND_SVGS.windows) {
        el.innerHTML = '<span class="si">' + BRAND_SVGS.windows + '</span>';
        return;
      }
      if (text === '🐧' && BRAND_SVGS.linux) {
        el.innerHTML = '<span class="si">' + BRAND_SVGS.linux + '</span>';
        return;
      }
      // Lucide icons
      var lucideName = EMOJI_TO_LUCIDE[text];
      if (lucideName) {
        el.innerHTML = '<i data-lucide="' + lucideName + '"></i>';
      }
    });
    // Replace emojis in flow-node elements (prefix text before <small>)
    document.querySelectorAll('.flow-node').forEach(function (el) {
      var childNodes = Array.from(el.childNodes);
      for (var i = 0; i < childNodes.length; i++) {
        var node = childNodes[i];
        if (node.nodeType === 3) {
          // Text node — check for emoji prefix
          var t = node.textContent;
          for (var emoji in EMOJI_TO_LUCIDE) {
            if (t.indexOf(emoji) === 0) {
              var iconEl = document.createElement('i');
              iconEl.setAttribute('data-lucide', EMOJI_TO_LUCIDE[emoji]);
              iconEl.style.marginRight = '4px';
              el.insertBefore(iconEl, node);
              node.textContent = t.slice(emoji.length);
              break;
            }
          }
        }
      }
    });
    // Re-initialize Lucide icons if available
    if (typeof lucide !== 'undefined' && lucide.createIcons) {
      lucide.createIcons();
    }
  }

  /** Load Lucide from CDN */
  function loadLucide(callback) {
    if (typeof lucide !== 'undefined') {
      callback();
      return;
    }
    var script = document.createElement('script');
    script.src = 'https://unpkg.com/lucide@0.460.0/dist/umd/lucide.min.js';
    script.onload = callback;
    script.onerror = function () {
      console.warn('[icons] Failed to load Lucide CDN, emoji icons will remain');
    };
    document.head.appendChild(script);
  }

  // ORIGIN_PATH_HINT kept for documentation of Pages base; locale detection uses /zh/ only.
  void ORIGIN_PATH_HINT;

  document.addEventListener('DOMContentLoaded', function () {
    renderNav();
    renderFooter();
    initHeroDemo();
    initPlatformDetect();
    initReleaseDownloads();
    initStarBadge();
    initLightbox();
    loadLucide(replaceEmojiIcons);
  });
})();
