(function () {
  const NAV = [
    { href: "index.html", label: "首页" },
    { href: "features.html", label: "功能总览" },
    { href: "ai.html", label: "AI 助手" },
    { href: "charts.html", label: "图表可视化" },
    { href: "workflow.html", label: "Workflow" },
    { href: "databases.html", label: "数据库" },
    { href: "download.html", label: "下载" },
  ];

  function base() {
    // Pages live in docs/ at site root; assets are relative.
    const path = location.pathname;
    const inSubdir = path.endsWith("/") || path.split("/").pop().indexOf(".") === -1;
    return inSubdir ? "" : "";
  }

  function renderNav() {
    const host = document.getElementById("site-header");
    if (!host) return;
    const current = location.pathname.split("/").pop() || "index.html";
    const links = NAV.map(function (n) {
      const active = n.href === current ? ' class="active"' : "";
      return '<a href="' + n.href + '"' + active + ">" + n.label + "</a>";
    }).join("");
    host.innerHTML =
      '<nav class="nav"><div class="wrap nav-inner">' +
      '<a class="nav-logo" href="index.html"><img src="assets/logo.png" alt="DataZen">DataZen</a>' +
      '<button class="nav-toggle" aria-label="菜单">☰</button>' +
      '<div class="nav-links">' + links + "</div>" +
      '<a class="btn btn-primary nav-cta" href="download.html">下载</a>' +
      "</div></nav>";
    const toggle = host.querySelector(".nav-toggle");
    toggle.addEventListener("click", function () {
      host.querySelector(".nav-links").classList.toggle("open");
    });
  }

  function renderFooter() {
    const host = document.getElementById("site-footer");
    if (!host) return;
    host.innerHTML =
      '<footer class="footer"><div class="wrap">' +
      '<div class="footer-grid">' +
      "<div><h4>DataZen</h4><p>轻量、免费、开源的跨平台桌面数据库客户端。基于 Tauri v2 + Rust + React 构建，GPLv3 协议开源。</p></div>" +
      "<div><h4>产品</h4>" +
      '<a href="features.html">功能总览</a>' +
      '<a href="ai.html">AI 助手</a>' +
      '<a href="charts.html">图表可视化</a>' +
      '<a href="workflow.html">Workflow</a>' +
      '<a href="databases.html">数据库支持</a></div>' +
      "<div><h4>链接</h4>" +
      '<a href="https://github.com/flyxl/datazen" target="_blank" rel="noopener">GitHub 仓库</a>' +
      '<a href="https://github.com/flyxl/datazen/releases" target="_blank" rel="noopener">下载中心</a>' +
      '<a href="https://github.com/flyxl/datazen/issues" target="_blank" rel="noopener">反馈 Issue</a>' +
      '<a href="mailto:wuxiaolongklws@gmail.com">联系作者</a></div>' +
      "</div>" +
      '<div class="copy">© 2026 DataZen · GPLv3 License · macOS / Windows</div>' +
      "</div></footer>";
  }

  document.addEventListener("DOMContentLoaded", function () {
    renderNav();
    renderFooter();
  });
})();
