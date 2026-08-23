(function () {
  const ORIGIN_PATH_HINT = '/datazen';

  const STR = {
    en: {
      nav: [
        { href: 'index.html', label: 'Home' },
        { href: 'features.html', label: 'Features' },
        { href: 'ai.html', label: 'AI' },
        { href: 'charts.html', label: 'Charts' },
        { href: 'workflow.html', label: 'Workflow' },
        { href: 'databases.html', label: 'Databases' },
        { href: 'docs.html', label: 'User Guide' },
        { href: 'manual.html', label: 'Manual' },
        { href: 'download.html', label: 'Download' },
      ],
      downloadCta: 'Download',
      menuAria: 'Menu',
      footerAbout:
        'Lightweight, free, open-source cross-platform desktop database client. Built with Tauri v2 + Rust + React. Licensed under GPLv3.',
      footerProduct: 'Product',
      footerLinks: 'Links',
      footerFeatures: 'Features',
      footerAi: 'AI Assistant',
      footerCharts: 'Charts',
      footerWorkflow: 'Workflow',
      footerDocs: 'User Guide',
      footerManual: 'Manual',
      footerDatabases: 'Databases',
      footerGithub: 'GitHub',
      footerReleases: 'Releases',
      footerIssues: 'Issues',
      footerContact: 'Contact',
      langSwitchLabel: '中',
      langSwitchAria: 'Switch to Chinese',
    },
    zh: {
      nav: [
        { href: 'index.html', label: '首页' },
        { href: 'features.html', label: '功能总览' },
        { href: 'ai.html', label: 'AI 助手' },
        { href: 'charts.html', label: '图表可视化' },
        { href: 'workflow.html', label: 'Workflow' },
        { href: 'databases.html', label: '数据库' },
        { href: 'docs.html', label: '使用说明' },
        { href: 'manual.html', label: '使用手册' },
        { href: 'download.html', label: '下载' },
      ],
      downloadCta: '下载',
      menuAria: '菜单',
      footerAbout:
        '轻量、免费、开源的跨平台桌面数据库客户端。基于 Tauri v2 + Rust + React 构建，GPLv3 协议开源。',
      footerProduct: '产品',
      footerLinks: '链接',
      footerFeatures: '功能总览',
      footerAi: 'AI 助手',
      footerCharts: '图表可视化',
      footerWorkflow: 'Workflow',
      footerDocs: '使用说明',
      footerManual: '使用手册',
      footerDatabases: '数据库支持',
      footerGithub: 'GitHub 仓库',
      footerReleases: '下载中心',
      footerIssues: '反馈 Issue',
      footerContact: '联系作者',
      langSwitchLabel: 'EN',
      langSwitchAria: '切换到英文',
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
    const links = t.nav
      .map(function (n) {
        const active = n.href === current ? ' class="active"' : '';
        return '<a href="' + n.href + '"' + active + '>' + n.label + '</a>';
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
      '<a class="nav-lang" href="' +
      langHref +
      '" aria-label="' +
      t.langSwitchAria +
      '">' +
      t.langSwitchLabel +
      '</a>' +
      '<a class="btn btn-primary nav-cta" href="download.html">' +
      t.downloadCta +
      '</a>' +
      '</div></nav>';
    const toggle = host.querySelector('.nav-toggle');
    toggle.addEventListener('click', function () {
      host.querySelector('.nav-links').classList.toggle('open');
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
      '<a href="docs.html">' +
      t.footerDocs +
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
      '<div class="copy">© 2026 DataZen · GPLv3 License · macOS / Windows</div>' +
      '</div></footer>';
  }

  // ORIGIN_PATH_HINT kept for documentation of Pages base; locale detection uses /zh/ only.
  void ORIGIN_PATH_HINT;

  document.addEventListener('DOMContentLoaded', function () {
    renderNav();
    renderFooter();
  });
})();
