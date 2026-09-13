# 站点图标去 emoji 化（去掉 AI 味）

Written against: fdced28356017d73dfb8425e616bd8d308ab34b6

## Evidence chain

- Surface: `site/` 静态营销站全站（`index.html`、`features.html`、`ai.html`、`charts.html`、`workflow.html`、`databases.html`、`download.html`、`manual.html` 及其 `zh/` 镜像，共 18 个 HTML）+ 共享运行时 `site/assets/js/site.js` + 共享样式 `site/assets/css/site.css`
- Problem: 同一任务内图标语言直接矛盾——`site/index.html` 第 181/186/191/196 行已使用静态 `<i data-lucide="...">`（search / satellite-dish / repeat / sparkles），而其余页面同一 `.card .icon` 槽位仍写原始 emoji（`site/features.html:66 🤖`、`71 📊`、`76 ⚙️`、`83 🛰️`、`88 🔌`；`site/ai.html:47 🤖`、`230 🖥️`、`238 🔌`；`site/charts.html:47 📊`、`128 📈` 等；`site/databases.html:66 🐘`、`71 🐬`；`site/download.html:69 🍎`、`94 🪟`、`107 🐧`；`site/workflow.html:55 ⚙️`、`189 📦` 等；各 `zh/` 镜像一一对应）。`page-head h1` 更严重：`features.html:55 🧭`、`ai.html:47 🤖`、`charts.html:47 📊`、`workflow.html:55 ⚙️`、`databases.html:55 🔌`、`download.html:55 ⬇️`、`manual.html:81 📘`（+ `zh/` 对应行）全部带 emoji 前缀，而 `site/blog-architecture.html:58` 的 `h1` 为纯文本，证明纯文本 `h1` 是站内已存在的合法范例。`index.html:233-269` 的 `.uc-row .uc-ico`（💬🤖▶️✕🩺✓🐢🔍📈）与 `index.html:361-367` / `workflow.html:70-76` 的 `.flow-node`（🐘🐬🤖📤）同样是源码级 emoji。
- Design evidence: 站点自有 владельцев——`site/assets/css/site.css:623-649` 已为 `.card .icon svg` 定义 Lucide 笔画样式（`width:22px; stroke:currentColor; fill:none; stroke-width:1.8`）与品牌填充例外（`.card .icon .si svg {fill:currentColor; stroke:none}`）；`site/assets/js/site.js:477-518` 的 `EMOJI_TO_LUCIDE` 映射表 + `BRAND_SVGS`（apple/windows/linux）+ `replaceEmojiIcons()` + `loadLucide()` 证明「emoji 只是待替换的中间态，最终必须渲染为 Lucide SVG」是该表面的绑定契约。`replaceEmojiIcons` 只处理 `.card .icon` / `.uc-row .uc-ico` / `.flow-node`，从不处理 `page-head h1`，且依赖 `https://unpkg.com/lucide@0.460.0` CDN——CDN 失败或禁用 JS 时 emoji 原形毕露，与 `index.html` 静态 `data-lucide` 范例形成运行时矛盾。
- Owner: `site/assets/js/site.js`（`EMOJI_TO_LUCIDE`、`BRAND_SVGS`、`replaceEmojiIcons`、`loadLucide`）与 `site/assets/css/site.css`（`.card .icon svg`、`.uc-row .uc-ico svg`）
- Scope and affected surfaces: 上述 18 个 HTML 的三类位置——(a) `page-head h1` 前缀 emoji；(b) `.card .icon` 原始 emoji（含 OS 品牌 🍎🪟🐧）；(c) `.uc-row .uc-ico` 与 `.flow-node` 前缀 emoji。共享运行时 `site.js` 与样式 `site.css` 随之收敛。
- Uncertainty: 无。映射名以 `EMOJI_TO_LUCIDE` 表为准，不发明新图标语义；`🥧 pie-chart`、`🛟 life-buoy`、`📘` 等在表中有或无对应，下文给出确定性落点。

## Design decision

把 emoji 从源码层彻底移除，统一收敛到站内已有范例：正文图标一律用静态 `<i data-lucide="<EMOJI_TO_LUCIDE 中的精确名>">`（OS 三图标内联 `BRAND_SVGS` 的静态 SVG），`page-head h1` 一律纯文本（以 `blog-architecture.html` 为范例）。这消除「同一图标两种语言 + JS/CDN 运行时补丁」的根问题，是全站去 AI 味杠杆最高的单点改动；视觉身份（深色默认双主题、渐变 accent、卡片圆角）保持不变。

## Reuse

- `EMOJI_TO_LUCIDE` 映射（`site/assets/js/site.js:477-508`）：`⚡→zap`、`🛡️→shield-check`、`🔌→plug`、`👤→user`、`🧩→puzzle`、`📖→book-open`、`🤖→bot`、`📊→bar-chart-3`、`⚙️→settings`、`🛰️→satellite-dish`、`📈→trending-up`、`🥧→pie-chart`、`✨→sparkles`、`📦→package`、`🔁→repeat`、`🛟→life-buoy`、`🖥️→monitor`、`🗄️→hard-drive`、`📡→radio-tower`、`🐢→gauge`、`🔍→search`、`🩺→stethoscope`、`💬→message-circle`、`▶️→play`、`📤→send`、`⬇️→download`、`🐘→database`、`🐬→droplets`、`✕→x`、`✓→check`
- `BRAND_SVGS`（`site/assets/js/site.js:511-518`）：`🍎→apple`、`🪟→windows`、`🐧→linux`，以 `<span class="si">SVG</span>` 静态内联
- `.card .icon svg` / `.card .icon .si svg` / `.uc-row .uc-ico svg` 样式（`site/assets/css/site.css:634-649`、`1484-1492`）
- Exemplar: `site/index.html:181`（`<div class="icon" aria-hidden="true"><i data-lucide="search"></i></div>`）与 `site/blog-architecture.html:58`（纯文本 `h1`）

不新增图元。现有系统已能表达全部决策；`site.js` 的运行时替换逻辑反而是应被收敛的并行路径。

## Changes

1. `site/*.html` 与 `site/zh/*.html`（共 18 个文件，`blog-architecture.html` 双语除外——其 `h1` 已合规，仅改其卡片/流程节点如有）
   - Change:
     - (a) `page-head h1`：删除行首 emoji 及后续空格，只保留文字（例 `🤖 AI Assistant` → `AI Assistant`；`🧭 Features` → `Features`；`📘 DataZen User Manual` → `DataZen User Manual`；中文页同理）。不加图标、不改标题文案。
     - (b) `.card .icon`：把 `<div class="icon">EMOJI</div>` 替换为 `<div class="icon" aria-hidden="true"><i data-lucide="<映射名>"></i></div>`，映射名严格按上表（例 `🤖→bot`、`📊→bar-chart-3`、`⚙️→settings`、`🛰️→satellite-dish`、`🐘→database`、`🐬→droplets`、`🗄️→hard-drive`、`⚡→zap`、`📡→radio-tower`、`📈→trending-up`、`🥧→pie-chart`、`✨→sparkles`、`📦→package`、`🔁→repeat`、`🛟→life-buoy`、`🖥️→monitor`、`🩺→stethoscope`）。`download.html` 的 `🍎/🪟/🐧` 替换为 `<div class="icon" aria-hidden="true"><span class="si">` + `BRAND_SVGS` 中对应 SVG 原文 + `</span></div>`。
     - (c) `.uc-row .uc-ico`（`index.html:233-269` + `zh/index.html:226-258`）：把 `<span class="uc-ico">EMOJI</span>` 替换为 `<span class="uc-ico"><i data-lucide="<映射名>"></i></span>`（`💬→message-circle`、`🤖→bot`、`▶️→play`、`✕→x`、`🩺→stethoscope`、`✓→check`、`🐢→gauge`、`🔍→search`、`📈→trending-up`）。
     - (d) `.flow-node`（`index.html:361-367`、`workflow.html:70-76` 及其 `zh/` 镜像）：删除行首 emoji 文本，改为前缀 `<i data-lucide="<映射名>"></i>`（`🐘→database`、`🐬→droplets`、`🤖→bot`、`📤→send`），不保留 emoji 字符、不写行内 `style`。
   - Preserve: 所有文案、链接、版式、主题 token 不变；`index.html` 四张 grid-4 卡片已合规，原样保留作全站范例。
   - Verify: `rg -n '[💬🤖▶️✕🩺✓🐢🔍📈🐘🐬📤🛰️✨⚡🛡️🔌👤🧩📖📊⚙️📦🔁🛟🖥️🗄️📡🍎🪟🐧🧭⬇️🥧]' site --glob '*.html'` 无命中；页面图标全部为 Lucide 笔画风格。

2. `site/assets/js/site.js`
   - Change: 保留 `loadLucide(replaceEmojiIcons)` 调用结构，但把 `replaceEmojiIcons` 收敛为仅执行 `lucide.createIcons()` 的静态渲染函数——删除 `EMOJI_TO_LUCIDE` 分支、`BRAND_SVGS` 分支与 `.flow-node` 文本节点扫描三段 emoji 补丁逻辑（或整函数简化为 `if (window.lucide) lucide.createIcons()`）。`EMOJI_TO_LUCIDE`/`BRAND_SVGS` 常量随之删除，不再引用 `unpkg` 失败时的 emoji 回退注释以外的逻辑。
   - Preserve: `renderNav`、`renderFooter`、主题切换、平台识别、下载解析、star badge、lightbox 逻辑一字不动；Lucide CDN 加载方式不变。
   - Verify: 禁用网络 CDN 时页面不再出现 emoji 图标（静态 `<i data-lucide>` 原位留空而非 emoji），联网时图标正常渲染；`rg -n 'EMOJI_TO_LUCIDE|BRAND_SVGS|replaceEmojiIcons' site/assets/js/site.js` 仅剩 `createIcons` 调用。

3. `site/assets/css/site.css`
   - Change: 仅补一处 `.flow-node` 内静态图标对齐：`.flow-node > i[data-lucide], .flow-node > svg.lucide { width:14px; height:14px; margin-right:6px; vertical-align:-2px; }`，替代原 JS 写入的 `marginRight:4px` 行内样式。`.card .icon` 与 `.uc-row .uc-ico` 的 svg 尺寸规则已存在，不新增。
   - Preserve: 全部 token（`--bg/--surface/--accent/--gradient` 等）、双主题、卡片 hover、栅格断点不变。
   - Verify: Workflow/MCP 流程节点图标与文字基线对齐，中英文双版一致。

## Scope

- Inherit: 全站 18 个 HTML 自动继承新图标语言；`zh/` 镜像与英文页同规则映射。
- Verify: `download.html` 三 OS 品牌图标（内联 SVG 在浅色主题下的对比度）、`charts.html` 四图标（`trending-up/bar-chart-3/pie-chart/sparkles`）在 `grid-3` 中的视觉一致性、`manual.html` 纯文本 `h1` 是否需补面包屑以外的层级（不需要，仅确认）。
- Exclude: 文案重写与 AI 话术治理（`AI-powered` 标题、`Learn about MCP →` 等）属 copy 决策，本计划不碰；`kicker` 渐变文本、`stats-band`、`compare` 表格属另一 finding/计划；不更换 Lucide 版本（保持 `0.460.0`），不自托管图标字体。

## Validation

- Product: 关闭 JS 或断网打开 `features.html`，卡片图标位不再出现 emoji；联网时全站图标为统一 Lucide 笔画风格，`h1` 为纯文本。
- Interface: 覆盖路由 `index/features/ai/charts/workflow/databases/download/manual` × 中英双语；状态：CDN 成功 / CDN 失败 / `prefers-reduced-motion`；视口 `1280/900/600px` 确认 `.icon` 42px 容器内 SVG 居中；内容极端：`download.html` 三品牌 SVG 在深浅主题下均清晰。
- System: 确认没有第二套图标路径——`site.js` 不再含 emoji 字符扫描，`site.css` 不新增图标字体；复用检查 `rg -n 'data-lucide' site --glob '*.html' | wc -l` 应显著上升且无 emoji 回退。
- Repository: `rg -n 'style="[^"]*display:\s*block' site --glob '*.html'` → 仅剩卡片链接计划应处理的命中（本计划不引入新的）；`rg -n 'EMOJI|🍎|🤖|📊|⚙️' site/assets/js/site.js` → 无命中（注释中的说明文字除外）；`python3 -m http.server` 本地预览全站点击一遍无 404。

## Stop conditions

- Stop if Lucide `0.460.0` 缺失某个映射名对应的图标（以 `lucide.createIcons()` 渲染后缺图标为准）——此时停下并报告缺失名，不自行发明替代图标或升级 CDN 版本。
- Stop if 发现某 emoji 承载语义区分（例 🐘/🐬 需换成官方 DB logo 而非 `database/droplets` 通用图标）——此属品牌资产决策，需用户另行指定，不在本计划内扩展。

## Design documentation

- After acceptance and validation: 无需更新设计文档。站内图标契约已由 `site.js`/`site.css` 代码拥有；如仓库日后新增 `site/DESIGN.md`，可补记一句「站点图标一律静态 `data-lucide`，`h1` 纯文本，禁止源码级 emoji」，目的地为该新文件，否则为 none。
