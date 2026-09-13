# 站点卡片链接变体收敛（去掉内联样式 + 误用 media-card）

Written against: fdced28356017d73dfb8425e616bd8d308ab34b6

## Evidence chain

- Surface: `site/index.html:179-200`（`grid-4` 四链接卡）与 `site/index.html:230-274`（`grid-3` 三 `media-card`）+ `site/features.html:63-92`（`grid-3` 三卡 + `grid-2` 两卡）及其 `zh/` 镜像；共享样式 `site/assets/css/site.css:607-690`
- Problem: 同一 `.card` 宿主在同一任务内呈现三种互相矛盾的写法：(1) `index.html:180/185/190/195` 的 `<a class="card" style="display: block">` 把布局写进内联样式；(2) `index.html:230/245/259` 的 `<a class="card media-card" style="padding: 24px; display: block">` 以内联 `padding:24px` 直接覆盖 `.media-card { padding:0 }`（`site.css:669-674`），而这三张卡根本没有 `<img>`，误用为图片卡却用内联样式否决了图片卡的核心定义；(3) `features.html:65/70/75/82/87` 同样 `style="display: block"`（且标签换行写法与 `index.html` 不一致）。附带的可判定视觉后果：全局 `site.css:73-76` 规定 `a { color: var(--accent) }`，而 `.card h3` 未设颜色——`div.card h3` 继承 `var(--text)`，`<a class="card"> h3` 却继承青色 `var(--accent)`，链接卡标题与静态卡标题颜色不一致；且 `.card:hover` 未清 `text-decoration`，悬停时链接卡标题会出现下划线，与静态卡的「上浮 + 边框高亮」语言（`site.css:618-622`）矛盾。
- Design evidence: 宿主权威为 `site.css:607-667`——`.card` 定义 `padding:24px`、`border/border-radius/background` 与 hover 上浮；`.media-card` 定义「顶部图片 + `.body` + `.tag{margin-top:auto}`」的图片卡组合（`669-690`）；`.card .tag` 定义渐变文字链（`658-667`）。宿主从未定义「可点击卡片」变体，这正是内联 `display:block` 泛滥的原因，但证据同时要求：复用现有宿主，不从重复中凭空发明共享图元——除非证明现有系统表达不了。本例能证明：去掉内联样式后，`<a class="card">` 仍缺 `display:block`（网格拉伸）、`color:inherit`（标题颜色）、`hover{text-decoration:none}`（悬停语言）三条，现有选择器表达不了，必须引入恰好一个变体。
- Owner: `site/assets/css/site.css`（`.card`、`.media-card`、`.card .tag`）
- Scope and affected surfaces: `index.html`（7 处：4×`card` + 3×`card media-card`）、`zh/index.html`（7 处同构）、`features.html`（5 处）、`zh/features.html`（5 处），共 24 处内联 `style`（以 `rg 'style="[^"]*(display:\s*block|padding:\s*24px)' site` 计数为准）。`site.js`/`site.css` 其余部分不受影响。
- Uncertainty: 无。三张 `media-card` 无 `<img>` 是可直接观察到的误用，纠正方向唯一（退为普通卡），不存在多解。

## Design decision

引入恰好一个可复用变体 `a.card`（以属性而非新类名收敛，详见 Reuse），承载「可点击卡片」的三条布局/继承规则；同时把三张无图 `media-card` 退为普通 `.card`，删除全部 24 处卡片内联 `style`。此后「链接卡」与「静态卡」共享同一盒模型与 hover 语言，差异仅剩语义标签（`a` vs `div`），根除内联样式覆盖宿主的并行路径。卡片文案、圆角、阴影、渐变 `tag` 保持不变。

## Reuse

- `.card`（`site.css:608-622`）：盒模型、hover 上浮与边框高亮，原样继承
- `.card .tag`（`site.css:658-667`）：渐变文字链，原样继承；`index.html` 三张退回卡保留 `<span class="tag">…→</span>` 行
- `.media-card` / `.media-card .body`（`site.css:669-690`）：本次是纠正误用而非扩展——无图卡退出该组合，有图卡（未来如加截图）继续用它，不改其定义
- Exemplar: `site/index.html:179-200` 的 `grid-4` 结构（退变体后它即全站链接卡范例）

新图元论证：现有系统无任何选择器能同时表达「`a` 元素作为网格项拉伸 + 标题颜色回退 `--text` + 悬停无下划线」。新建一个属性选择器 `a.card`（归属 `site.css` 的 Cards 节，与 `.card:hover` 相邻），消费者为全站一切 `<a class="card">`；它不是从重复中抽象的通用工具类，而是宿主缺失的「链接态」——符合「复用宿主、仅补缺失一态」原则。

## Changes

1. `site/assets/css/site.css`（Cards 节，紧随 `.card:hover`）
   - Change: 追加三条且仅三条：
     ```css
     a.card { display: block; color: var(--text); }
     a.card:hover { text-decoration: none; }
     a.card h3 { color: var(--text); }
     ```
     （首条解决网格拉伸与标题继承青色问题；次条统一悬停语言；第三条为防御性明确——即使全局 `a` 色变更，卡标题仍与 `div.card` 一致。）
   - Preserve: `.card`、`.media-card`、`.card .tag` 原定义一字不动；不加 `.card-link` 新类名（用元素+类属性收敛，调用点改动最小）。
   - Verify: `grid-4` 链接卡标题颜色与 `Why DataZen` 区 `div.card` 标题一致；悬停只有上浮 + 边框高亮，无下划线。

2. `site/index.html`
   - Change:
     - 第 180/185/190/195 行：`<a class="card" href="…" style="display: block">` → `<a class="card" href="…">`（删内联 `style`，`display:block` 改由 `a.card` 提供）。
     - 第 230/245/259 行：`<a class="card media-card" href="ai.html" style="padding: 24px; display: block">` → `<a class="card" href="ai.html">`（去 `media-card`、去内联 `padding/display`；卡内 `.uc-row` 与 `<span class="tag">` 结构原样保留）。
   - Preserve: 四卡/三卡的图标（_ICON 计划已接管，此处不动图标名_）、标题、描述、`href` 目标不变；`grid-4`/`grid-3` 容器类不变。
   - Verify: 三张退回卡的内边距与 `Why DataZen` 六张 `div.card` 完全一致（均为宿主 `padding:24px`），不再是图片卡的 `padding:0` 被内联否决态。

3. `site/zh/index.html`、`site/features.html`、`site/zh/features.html`
   - Change: 同规则批量收敛——删除所有 `<a class="card" … style="display: block"…>` 中的 `style` 属性（`features.html:65/70/75/82/87` 含换行写法 `style="display: block"\n          >`，一并清理为 `>`）；`zh/index.html` 三张 `card media-card + padding:24px` 同 `index.html` 退为 `<a class="card">`。
   - Preserve: 中英文文案、`href`（含 `features.html#mcp`、`#mcp` 锚点）、`grid-3`/`grid-2` 结构不变。
   - Verify: `rg -n 'class="card media-card"' site --glob '*.html'` 无命中；`rg -n '<a class="card"[^>]*style=' site --glob '*.html'` 无命中。

## Scope

- Inherit: 全站未来一切 `<a class="card">` 自动获得链接态（无需再写内联样式）；`zh/` 镜像与英文页同构继承。
- Verify: `databases.html`、`charts.html`、`workflow.html` 的 `div.card` 静态卡确认未被 `a.card` 规则污染（属性选择器天然隔离）；`860px/600px` 断点下链接卡与静态卡同行高；键盘 Tab 焦点下 `a.card` 可见性（沿用浏览器默认 outline，本计划不自定义焦点环）。
- Exclude: 图标去 emoji 化（另一计划）；`kicker` 渐变、`stats-band b` 渐变、`compare` 表格、`cta-band`、`flow` 箭头 `→`（文本箭头属内容而非内联样式，不碰）；不统一 `features.html` 与 `index.html` 的标签换行风格（仅删属性，不重排版式）。

## Validation

- Product: 打开 `index.html`，四张场景卡与三张 AI 场景卡点击区域铺满整个卡片，标题为正文色、悬停无下划线只有上浮；与 `Why DataZen` 静态卡并排对比，内边距、圆角、阴影一致。
- Interface: 路由 `index/features` × 中英双语；状态：默认 / hover / 键盘 focus / `600px` 单列；内容极端：最长卡（MCP Server / Client 英文两行标题）在 `grid-2` 与 `grid-4` 下不错位。
- System: 确认无第二套卡片路径——`rg -n 'style="' site --glob '*.html'` 剩余命中不得含 `display: block` 或 `padding: 24px`；`rg -n 'media-card' site --glob '*.html'` 无命中（CSS 定义保留供未来有图卡使用）；`rg -n 'a\.card' site/assets/css/site.css` 恰好一处。
- Repository: `rg -n 'class="card media-card"|style="[^"]*display:\s*block|style="[^"]*padding:\s*24px' site --glob '*.html'` → 无输出；`npx prettier --check site/index.html site/features.html site/zh/index.html site/zh/features.html site/assets/css/site.css`（如仓库有 prettier 配置则通过，否则跳过并注明）→ 通过或等价无 diff 噪音；`python3 -m http.server` 本地预览点击全部卡片链接无 404。

## Stop conditions

- Stop if 发现某张 `media-card` 实际预期配图（截图缺失而非误用）——此时停下并报告该卡，不擅自补图或保留误用类。
- Stop if 全局 `a { color }` 另有产品级设计契约要求链接卡标题保持青色（以站内文档或用户明确指示为准）——此时 `a.card h3` 第三条规则需回退，需用户裁决。

## Design documentation

- After acceptance and validation: 无需更新设计文档。如仓库日后新增 `site/DESIGN.md`，可补记一句「可点击卡片用 `<a class="card">`（`a.card` 变体），禁止卡片内联 `style`；无图卡禁止 `media-card`」，目的地为该新文件，否则为 none。
