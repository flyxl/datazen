# 工具栏刷新按钮快捷键提示做平台区分

Written against: 54110ab85c2cf42d7110119516ffc71ed55e852f

## Evidence chain

- Surface: 连接工作区内容工具栏右侧刷新按钮——`src/windows/connection/ContentToolbar.tsx:181-188`（`ToolbarButton compact variant="ghost" label={t('connWin.refresh')}`）
- Problem: 同一工具栏的 tooltip 语言直接矛盾：刷新按钮 `:184` 写死 `title={`${t('connWin.refresh')} (⌘R)`}`，在 Windows/Linux 下仍然显示 macOS 符号 `⌘R`；而同一状态栏的快捷键提示（`src/windows/connection/ContentStatusBar.tsx:24-27,50-61`）是平台感知的（`mod = isMac ? '⌘' : 'Ctrl' / keySep = isMac ? '' : '+'`，渲染 `⌘N / Ctrl+N`、`⌘W / Ctrl+W`）；同工具栏相邻的文档按钮（`:190-197`）与 AI 按钮（`:199-206`）均无快捷键后缀，唯独刷新按钮带后缀且跨平台错误。`useKeyboardShortcuts` 注册表（`ContentView.tsx:295-310` 仅注册 `newQuery/closeTab`）与 `keymap` 单测中均无 `mod+R` 绑定，佐证该后缀从未参与平台区分
- Design evidence: 同表面已存在的绑定范例——`ContentStatusBar.tsx:24-27` 的 `usePlatform()` + `isMac` 三元（`mod/keySep`）、`src/windows/connection/query/QueryEditorSection.tsx:180-182` 的 `platform = usePlatform(); isMac = platform === 'macos'; executeShortcutLabel = isMac ? '⌘ Enter' : 'Ctrl+Enter'`；`usePlatform` 契约见 `src/hooks/usePlatform.ts:32-53`（`macos/windows/linux/unknown`，首帧同步 UA、后续异步 Tauri 校准）
- Owner: `src/windows/connection/ContentToolbar.tsx:181-188`（刷新按钮拥有者）；范例拥有者为 `src/windows/connection/ContentStatusBar.tsx` 与 `QueryEditorSection.tsx`
- Scope and affected surfaces: 仅 `ContentToolbar.tsx` 一处 tooltip（`:1-22` imports + `:77-105` 组件头 + `:181-188` 按钮）。`MenuBar` 的 `Ctrl+N / Ctrl+,` 硬编码、`ContentStatusBar` 的 `Space` 硬编码属另案，不在本计划内
- Uncertainty: 无。纠正方向唯一（跟随状态栏做 `isMac ? ⌘R : Ctrl+R`），不存在多解；“刷新是否真有 R 快捷键”属功能行为，按技能边界不转为设计 finding，归入 Stop conditions

## Design decision

刷新按钮 tooltip 改用 `usePlatform()` 做平台区分：macOS 显示 `(⌘R)`，Windows/Linux/`unknown` 显示 `(Ctrl+R)`，与状态栏 `⌘N/Ctrl+N` 语言一致。此改动只换 tooltip 后缀的呈现，不增删任何快捷键绑定、不改按钮的 `compact/icon/variant/onClick`；Windows 用户不再看到错误的 `⌘` 符号。

## Reuse

- `usePlatform()`（`src/hooks/usePlatform.ts:32-44`）+ `isMac = platform === 'macos'` 三元（范例：`ContentStatusBar.tsx:24-27`、`QueryEditorSection.tsx:180-182`、`TitleBar.tsx:50-51`）
- `t('connWin.refresh')`（`src/locales/en/connection.ts:161` 等 8 语言已齐）原样复用，只在其后拼接平台后缀
- Exemplar: `src/windows/connection/ContentStatusBar.tsx:50-61`（`<kbd>{mod}{keySep}N</kbd>`）、`QueryEditorSection.tsx:182`（`isMac ? '⌘ Enter' : 'Ctrl+Enter'`）

不新增图元、不新增 i18n key。现有系统已能表达该决策。

## Changes

1. `src/windows/connection/ContentToolbar.tsx`
   - Change:
     - import 区追加 `import { usePlatform } from '../../hooks/usePlatform';`（与 `ContentStatusBar.tsx:2` 同路径写法；`ContentToolbar.tsx` 位于 `src/windows/connection/`，故为 `../../hooks/usePlatform`）
     - 组件内（`useCompactToolbar` 之后、 зві`return` 之前）追加：
       ```tsx
       const platform = usePlatform();
       const refreshTitle = `${t('connWin.refresh')} (${platform === 'macos' ? '⌘R' : 'Ctrl+R'})`;
       ```
     - `:181-188` 刷新按钮 `title={`${t('connWin.refresh')} (⌘R)`}` → `title={refreshTitle}`
   - Preserve: `compact`、`variant="ghost"`、`label={t('connWin.refresh')}`、`icon={<RefreshCw className="h-3.5 w-3.5" />}`、`onClick={onRefresh}`、`ToolbarShell h-12` 与 `contentToolbarExpandedMinWidth` 宽度估算（tooltip 不参与宽度）一字不动；相邻文档/AI 按钮不动
   - Verify: macOS 下 hover 显示 `Refresh (⌘R)`（中文环境为 `刷新 (⌘R)`），Windows/Linux 下显示 `Refresh (Ctrl+R)`；首帧（`unknown`→按非 mac 处理）与异步校准后一致，无闪烁外的跳变

## Scope

- Inherit: 未来工具栏一切“`⌘X / Ctrl+X`” tooltip 均按此 `usePlatform` 三元书写
- Verify: `ContentStatusBar` 的 `⌘N/Ctrl+N`、`⌘W/Ctrl+W`、`Space` 行——确认与新 tooltip 同语言且未被污染；`NavigatorToolbar` 的 `title={t('connWin.refresh')}`（无后缀版）保持原样，属有效例外（图标按钮无快捷键后缀是合法变体）
- Exclude: 是否真的存在全局 `mod+R` 绑定（功能行为，不碰）；`MenuBar` 快捷键硬编码、`Space` 无修饰键提示、文档按钮 tooltip 与 label 重复（`title/label` 双写 `docs.openAiHelp`）——均属另案；不新增 `connWin.refreshHint` 等 i18n key（后缀为呈现层拼接，不进翻译）

## Validation

- Product: 在连接工作区 hover 右侧刷新图标，macOS 见 `(⌘R)`、Windows/Linux 见 `(Ctrl+R)`；点击仍触发 `onRefresh`（即 `ConnectionPage handleRefresh`：刷新连接列表 + 导航树），行为与改前一致
- Interface: 路由 `connections`（任一 `showNewQuery` 真/假组合，工具栏左右簇数量变化不影响右侧 tooltip）；状态：`compact` 开/关（tooltip 均存在，因按钮常为 `compact`）；i18n 极端：德语 `Aktualisieren (Ctrl+R)` 不溢出；`unknown` 平台回退为 `Ctrl+R`
- System: 确认无第二套硬编码——`rg -n '⌘R' src/windows/connection/ContentToolbar.tsx` → 仅剩三元中的 mac 分支一处；`rg -n "usePlatform" src/windows/connection/ContentToolbar.tsx` → 有命中；与 `ContentStatusBar` 的 `mod` 语言一致
- Repository: `npx vitest run src/windows/connection/__tests__/ContentView.test.tsx src/windows/connection/__tests__/ContentStatusBar.test.tsx`（如存在工具栏用例则一并跑）→ 通过；`npx tsc --noEmit`（如有）→ 无新增类型错误

## Stop conditions

- Stop if 产品确认刷新按钮根本没有 `mod+R` 键盘绑定（`ContentView useKeyboardShortcuts` 与菜单加速键均无 `R`）——此时正确的设计纠正是删除后缀（`title={t('connWin.refresh')}`，与 `NavigatorToolbar` 无后缀版一致），而非平台区分；停下并报告，由用户在“删除后缀 vs 补绑定+平台区分”之间裁决，不擅自补全局快捷键
- Stop if 设计要求所有平台统一只显示 `⌘R`（以站内文档或用户明确指示为准）——此时回退本计划，需用户裁决

## Design documentation

- After acceptance and validation: 无需更新设计文档。如仓库日后新增工具栏/tooltip 规范，可补记一句「tooltip 快捷键后缀一律经 `usePlatform()` 做 `⌘X / Ctrl+X` 平台区分，禁止硬编码单平台符号；无绑定的按钮不写后缀」，目的地为该新文件，否则为 none
