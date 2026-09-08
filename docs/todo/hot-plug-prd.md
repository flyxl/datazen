# DataZen 插件全生命周期热插拔机制 (Hot-Plugging) PRD

| 文档版本 | 状态 | 适用架构 | 核心负责人 |
| :--- | :--- | :--- | :--- |
| v1.0.0 | Draft / Reviewing | DataZen 四维扩展架构 (Driver / Theme / EP / Workspace App) | DataZen 核心架构组 |

---

## 1. 业务背景与设计目标

### 1.1 现状与核心痛点
在传统的桌面客户端工具（如 DBeaver、早期的 VS Code 扩展等）中，安装、启用、升级或禁用插件往往需要：
1. **重启桌面客户端** 或 **整窗重新加载（Window Reload）**；
2. **丢失未保存的 SQL 草稿** 与多光标选区；
3. **中断正在执行的长耗时查询** 或破坏连接池活跃事务；
4. **重置工作区状态**（折叠目录、打开的表标签页、ER 图节点排布等）。

随着 DataZen 确立严格正交的四维可扩展架构（Driver、Theme、EP、Workspace App），用户对插件的使用预期转变为**像 Web 云端应用一样“即装即用、无感热拔、无缝降级”**。

### 1.2 目标与非目标
* **核心目标（P0）**：
  * **零重启应用**：安装、启用、禁用、卸载任意外观主题、工作区应用（Workspace App）或特权扩展点（Host EP），无需重启 Tauri 进程或刷新主窗口。
  * **状态零损毁**：编辑器在加载/卸载增强扩展（如 SQLEditor Pro）时，光标位置、编辑历史（Undo/Redo Stack）、当前文本内容与滚动高度必须 100% 保持，耗时控制在 **5ms 以内**。
  * **彻底的副作用回收**：扩展停用后，所有 DOM 装饰、事件监听、Blob URL 资产、通信桥接信道、定时器与内存 AST 模型必须完全清理，杜绝内存泄漏（Leak-Free）。
* **非目标（Out of Scope for v1）**：
  * **底层数据库驱动（Driver）热插拔**：当前驱动采用 Rust `inventory` 编译时链接注入机制，本期 PRD 重点聚焦 UI、Theme、Workspace App 与特权扩展点（EP）。驱动运行时热加载（WASM/子进程）作为 P2 架构演进储备。

---

## 2. 四维扩展体系热插拔设计矩阵

| 扩展维度 | 运行形态 | 热插机制 (Activate) | 热拔机制 (Deactivate) | 预期延迟 |
| :--- | :--- | :--- | :--- | :--- |
| **Theme (外观主题)** | 纯静态资产 (CSS / SVG / JSON) | Blob URL 映射 + 注入 `<style id="datazen-theme-pack">` + 广播 CSS 变量 | `resetPackState()` + 移除 `<style>` + 撤回 `URL.revokeObjectURL()` | < 16ms (一帧内) |
| **Workspace App (工作区应用)** | 独立沙箱 `<iframe>` (`datazen://{wappId}`) | 派发 `wapps:changed` + 动态更新导航菜单 + 按需挂载 iframe + 握手推流 | `closeByWapp(id)` 自动平滑关闭 Tab + 销毁 iframe + 断开通信桥 + 阻断 `datazen://` | < 50ms |
| **EP (特权扩展点 / SQLEditor Pro)** | 主进程特权内存执行 (In-Process JS/TS) | 动态模块载入 + `extensionRegistry.register()` + CodeMirror `Compartment.reconfigure` | 调用 `unregister()` + Compartment 重配为空 + 逆序执行 `subscriptions` 回收 | < 5ms |
| **Driver (数据库驱动)** | 编译时静态链接 (Rust + inventory) | *(编译时注入，保持现状)* | *(编译时注入，保持现状)* | N/A |

---

## 3. 特权扩展点 (Host EP) 热插拔架构设计

特权扩展（如 SQLEditor Pro、高级图表等）直接参与 CodeMirror 语法树分析、行号栏渲染和设置项贡献，是热插拔技术复杂度最高的部分。

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                       Host Extension Lifecycle Manager                      │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
            ┌──────────────────────────┴──────────────────────────┐
            ▼                                                     ▼
┌───────────────────────┐                             ┌───────────────────────┐
│  Hot-Plug (Activate)  │                             │ Hot-Unplug (Deactive) │
├───────────────────────┤                             ├───────────────────────┤
│ 1. 动态加载 ESM 包    │                             │ 1. 触发 deactivate()  │
│ 2. 注入 ExtensionCtx  │                             │ 2. 逆序释放 Subscriptions│
│ 3. 注册到 Registry    │                             │ 3. unregister(EP)     │
│ 4. useSyncExternalStore                             │ 4. Compartment 清空   │
│ 5. Compartment.reconf │                             │ 5. 设置项动态下线     │
└───────────────────────┘                             └───────────────────────┘
```

### 3.1 模块加载契约规范与 Context 设计
所有特权扩展模块必须实现统一的生命周期与资源持有规范：

```typescript
export interface Disposable {
  dispose(): void;
}

export type ExtensionSubscription = (() => void) | Disposable;

export interface ExtensionContext {
  /** 插件生命周期容器，存放所有需在卸载时释放的副作用 */
  subscriptions: ExtensionSubscription[];
  /** 扩展私有存储路径或全局数据 */
  extensionId: string;
}

export interface ExtensionModule {
  /** 激活入口：接收 context，完成扩展点注册与事件监听 */
  activate(context: ExtensionContext): void | Promise<void>;
  /** 停用入口：可选，执行模块级特定清理（如清空缓存池） */
  deactivate?(): void | Promise<void>;
}
```

### 3.2 动态加载与卸载器实现 (Module Loader)
宿主提供受控的动态模块加载管线，支持从本地沙箱资产目录或远程下载的包加载特权模块：

```typescript
export class HostExtensionLoader {
  private activeExtensions = new Map<string, {
    context: ExtensionContext;
    deactivate?: () => void | Promise<void>;
  }>();

  /** 动态热插扩展 */
  async load(extensionId: string, entryUrl: string): Promise<void> {
    if (this.activeExtensions.has(extensionId)) {
      await this.unload(extensionId);
    }

    const context: ExtensionContext = {
      extensionId,
      subscriptions: [],
    };

    // 1. 动态 ESM 加载
    const mod: ExtensionModule = await import(/* @vite-ignore */ entryUrl);
    
    // 2. 激活插件
    await mod.activate(context);

    // 3. 记录生命周期引用
    this.activeExtensions.set(extensionId, {
      context,
      deactivate: mod.deactivate,
    });
  }

  /** 动态热拔扩展 */
  async unload(extensionId: string): Promise<void> {
    const active = this.activeExtensions.get(extensionId);
    if (!active) return;

    try {
      // 1. 调用模块自定义 deactivate 钩子
      if (active.deactivate) {
        await active.deactivate();
      }
    } finally {
      // 2. 严格逆序释放所有 subscriptions (LIFO)
      for (let i = active.context.subscriptions.length - 1; i >= 0; i--) {
        const sub = active.context.subscriptions[i];
        try {
          if (typeof sub === 'function') sub();
          else sub.dispose();
        } catch (err) {
          console.error(`[ExtensionLoader] Error disposing resource:`, err);
        }
      }
      this.activeExtensions.delete(extensionId);
    }
  }
}
```

### 3.3 CodeMirror 6 深度集成：隔室热置换 (Compartment Reconfiguration)
为了实现编辑器功能的热装配与热卸载，**禁止在扩展更新时销毁重连 EditorView**。所有由特权扩展注入的 CodeMirror 能力（Gutter 运行按钮、Inlay Hints、Hover 悬浮、智能补全源、Keymap 绑定），均由独立隔室管控：

```typescript
// 编辑器内部维护独立隔室
const proStatementGutterCompartment = new Compartment();
const proIntentionCompartment = new Compartment();
const proHoverCompartment = new Compartment();
const proJoinCompletionCompartment = new Compartment();

export function updateEditorProCapabilities(view: EditorView, pro: SqlEditorProFeatures, enabled: boolean) {
  view.dispatch({
    effects: [
      // 启用时配置扩展列表，禁用时清空为 []
      proStatementGutterCompartment.reconfigure(
        enabled ? (pro.createStatementDecorations?.() ?? []) : []
      ),
      proIntentionCompartment.reconfigure(
        enabled ? (pro.createIntentionExtensions?.() ?? []) : []
      ),
      proHoverCompartment.reconfigure(
        enabled ? (pro.createHoverExtensions?.() ?? []) : []
      ),
    ],
  });
}
```

* **热拔效果**：
  * 处于活动状态的行号栏执行图标立即消失；
  * 内联展示的 INSERT 列名标签立即剔除；
  * 鼠标悬浮不再触发表结构解析；
  * 编辑器底层光标选区（Selection）、滚动条 Offset 与历史记录树保持不变。

---

## 4. Workspace App 沙箱插件热插拔规范

Workspace App 运行在独立沙箱 `<iframe>` 内，通过 `datazen://` 资产服务与 RPC 桥通信。

### 4.1 安装与动态就位 (Hot-Install)
1. 前端通过文件选择器或拖拽提交扩展包；
2. Rust 后端解压至 `.datazen-staging-{uuid}` 临时目录完成全量安全性扫描（Manifest 格式、MIME 白名单、无符号链接、无路径穿越）；
3. 原子重命名到 `{appData}/wapps/{wappId}/`；
4. 后端向所有前端窗口广播 `wapps:changed` 事件；
5. 前端 `extensionStore` 自动更新内存列表，侧边栏 `WorkspaceNavigator` 动态出现对应页面项。

### 4.2 禁用与热卸载 (Hot-Uninstall / Hot-Disable)
1. **自动平滑关闭活动 Tab**：
   前端执行 `useWorkspaceTabsStore.getState().closeByWapp(wappId)`。遵循“右邻居优先、次选左邻居”的焦点保护机制，如果用户正在当前应用内，平滑激活相邻 Tab；
2. **切断通信桥与信封拦截**：
   * 宿主 `ExtensionPageShell` 卸载对应的 `<iframe>` DOM 节点；
   * `extensionBridge` 触发 `detach()`，注销 `window.addEventListener('message')` 监听器；
   * 正在进行中的长耗时 RPC 请求立即返回 `BRIDGE_ERROR.PLUGIN_DISABLED`；
3. **协议级资产阻断**：
   Rust 端的 `datazen://` 资产服务分发器检测到插件被禁用/卸载，任何后续对于 `datazen://{wappId}/...` 的网络/静态资源请求直接响应 `HTTP 403 Forbidden`。

---

## 5. 外观主题 (Theme) 热插拔规范

### 5.1 激活与切换 (Hot-Apply)
1. 读取主题包内 `tokens.css`，解析内部相对路径的字体与背景图；
2. 通过 `URL.createObjectURL(blob)` 映射为运行时有效资源；
3. 创建/更新 `<style id="datazen-theme-pack">`，应用主界面颜色变量；
4. 广播 `datazen:theme-pack-changed` 事件，遍历当前所有打开的沙箱 iframe 并推送 `theme.apply` 快照。

### 5.2 卸载与重置 (Hot-Reset)
1. 移除 `<style id="datazen-theme-pack">` 标签；
2. 逐一遍历并调用 `URL.revokeObjectURL()` 释放内存 Blob；
3. 恢复宿主内置暗色/亮色默认 ThemeToken；
4. 全过程无白屏、无重刷。

---

## 6. 设置中心热响应设计 (Settings Reactivity)

在 `packages/extension-points` 中声明的扩展设置项（包含标准表单项与自定义 UI）：
1. **热插时**：`ExtensionRegistry` 收到实现注册后，通过 `useSyncExternalStore` 触发 React 重新渲染，`SettingsContent` 的对应分区动态出现该插件的配置组与自定义 UI 组件；
2. **热拔时**：`unregister()` 触发后，设置界面立即隐藏该配置卡片，不留无用占位符；
3. **数据持久化安全**：用户的配置项存储在 `settings.pluginSettings[wappId]` 中。即使插件被禁用或卸载，配置数据依然安全保存在配置文件中；下次重新启用该扩展时，配置选项自动恢复。

---

## 7. 异常边界与容灾防护机制 (Error Boundary & Resilience)

特权代码在主进程中执行，一旦发生未捕获异常，绝不能拖垮整个 DataZen。

### 7.1 编辑器安全熔断 (Safe Fallback Guard)
所有通过扩展点调用的装饰函数，外层包裹 `SafeCompartmentWrapper`：
```typescript
try {
  return pro.createStatementDecorations?.(opts) ?? [];
} catch (err) {
  console.error('[SQLEditorPro] Decorator crashed, auto-falling back to core:', err);
  // 记录错误并动态卸载该插件，保障编辑基本功能不中断
  extensionRegistry.unregister(sqlEditorProEP);
  return [];
}
```

### 7.2 沙箱 iframe 崩溃看门狗 (Watchdog)
* **握手超时保护**：iframe 加载超过 10 秒未发出 `plugin.ready` 信号，界面自动切换至“应用加载失败，点击重载”提示卡；
* **内存与限流保护**：单个插件桥接限制最大并发请求数 $\le 20$，超时时间 30 秒，防止恶意或死循环脚本耗尽前端资源。

---

## 8. 测试与质量验收矩阵

| 测试类型 | 测试用例 | 验证方式 | 预期标准 |
| :--- | :--- | :--- | :--- |
| **单元测试** | `extensionPoints.hotplug.test.ts` | 连续注册、反注册特权扩展点 | 状态无缝回退默认基础版，通知订阅者执行 |
| **单测 (React)** | `SettingsContent.hotplug.test.tsx` | 动态注册带自定义 UI 的配置项并立即反注册 | DOM 动态出现后立即消失，数据无破坏 |
| **旅程测试** | `editorCompartment.hotplug.test.ts` | 在用户输入 SQL 过程中动态切换 Pro 开关 | 键入不中断，光标不变，Gutter 动态去留 |
| **内存测试** | `memoryLeak.test.ts` | 循环执行 100 次“加载-卸载”循环 | DOM 节点数与 JS Heap 内存无持续增长 |
| **E2E 测试** | `e2e/specs/hotplug.spec.ts` | 通过 UI 动态安装 zip、使用功能、停用插件 | 全程无窗口重载，相关 Tab 自动关闭 |
