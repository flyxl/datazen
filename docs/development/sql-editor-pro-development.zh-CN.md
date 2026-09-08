# SQL Editor Pro 特权扩展开发指南

本文档介绍如何在 **DataZen** 项目中开发、改进和维护 **SQL Editor Pro**（`@datazen/extension-sql-editor-pro`）特权增强插件，阐明宿主与商业扩展之间的物理级解耦架构、本地开发调试流程、契约扩展规范以及独立的 Git 版本流转机制。

---

## 1. 架构定位与隔离原则 (Clean-Room Architecture)

DataZen 确立了严格的开源宿主与闭源商业特权扩展物理隔离策略：

```text
┌────────────────────────────────────────────────────────┐
│                   DataZen 宿主 (Host)                   │
│   • 仓库：github.com/flyxl/datazen (开源)               │
│   • 协议：GPL-3.0 + DataZen Linking Exception 豁免条款 │
│   • 源码：src/ (零 Pro 实现代码，Git 历史完全纯净)       │
│   • 契约：packages/extension-points/ (共享扩展点接口)    │
│   • 兜底：内置纯净基础功能 (Fallback)                   │
└──────────────────────────┬─────────────────────────────┘
                           │ 编译期装配 / 动态激活
                           ▼
┌────────────────────────────────────────────────────────┐
│           SQL Editor Pro 扩展 (Privileged Extension)   │
│   • 仓库：github.com/flyxl/datazen-extension-sql-editor-pro (私有闭源) │
│   • 本地路径：packages/pro-extensions/sql-editor-pro/  │
│   • 协议：商业私有专有许可证 (免除 GPL 传染)             │
│   • 职责：深度集成 CodeMirror，提供智能悬浮、函数提示、   │
│          智能 JOIN 补全、Intention 意图操作、Paste-as-IN │
└────────────────────────────────────────────────────────┘
```

### 核心隔离保障机制
1. **源码级隔离**：宿主 `src/` 中没有任何 `@datazen/extension-sql-editor-pro` 的静态源码依赖。
2. **构建期代码生成**：由 `scripts/resolve-pro.mjs` 动态写入 gitignored 的 `src/plugins/generated-pro.ts`：
   - 当 `--edition=pro` 时注入 `activate()` 并注册至 `sqlEditorProEP`；
   - 当 `--edition=community`（默认开源版）时输出空桩函数，宿主走纯净 Fallback。
3. **独立 Git 仓库**：`packages/pro-extensions/sql-editor-pro` 拥有独立的 `.git` 版本树，并被宿主根目录 `.gitignore` 全局忽略，绝对不会被意外提交进宿主仓库。

---

## 2. 本地工作区与目录结构

在 DataZen 根目录下，Pro 扩展位于 `packages/pro-extensions/sql-editor-pro`：

```text
packages/pro-extensions/sql-editor-pro/
├── .git/                        # 独立的私有 Git 仓库
├── package.json                 # 包元数据 (@datazen/extension-sql-editor-pro)
├── tsconfig.json                # 独立 TypeScript 编译配置
├── vitest.config.ts             # 独立的 Vitest 运行配置
├── AGENTS.md                    # 专属于该插件的 Agent 极简上下文开发规范
└── src/
    ├── index.ts                 # 扩展入口 activate(registry) 与导出
    ├── proFeatures.ts           # 汇聚装配所有 Pro 特性并向 sqlEditorProEP 注入
    ├── intentions/              # 智能意图 (星号展开、限定符增删、INSERT/函数内联提示)
    ├── hover/                   # 表结构悬浮卡片预览、Mod+Click 结构跳转
    ├── signature-help/          # 函数签名与参数高亮实时提示
    ├── join-completion/         # 基于外键依赖的智能 JOIN 补全
    ├── paste/                   # 智能粘贴 (Paste-as-IN) 与拖拽高亮 DropCaret
    ├── statement-gutter/        # 语句行号 Gutter 运行按钮与多语句高亮
    ├── bind-params/             # SQL 命名参数智能提取与绑定面板
    └── locales/                 # 独立的 Pro 多语言资源 (en.ts, zh-CN.ts)
```

### 规范依赖与零宿主内部引用（Zero @host/*）
Pro 扩展遵循彻底的纯净解耦规范，**严禁使用任何 `@host/*` 别名**，所需公共能力均由权威公共包导出：
- `@datazen/extension-points`：SQL 编辑器契约（`SqlEditorProps`, `SqlSchema` 等）、语义模型（`buildSemanticModel`, `scanSql`, `getDialectAdapter` 等）以及多语言跨宿主桥梁；
- `@datazen/ui`：宿主共享设计系统（Button, Dialog, Input, Select, cn 等纯 React 视图组件）；
- `@codemirror/*`：CodeMirror 6 官方核心库。
- 本地词条：`src/locales/` 维护自己的翻译，通过 `@datazen/extension-points` 的 `createExtensionI18n` 复用宿主当前的语言设置与切换调度。

---

## 3. 准备工作与代码检出

如果是新克隆的宿主工作区，可以通过以下方式准备 Pro 扩展：

### 自动克隆/同步
```bash
# 执行解析脚本，会自动拉取私有仓库到 packages/pro-extensions/sql-editor-pro
node scripts/resolve-pro.mjs --edition=pro
```

> **注意**：首次拉取私有仓库需要本地具有访问 `flyxl/datazen-extension-sql-editor-pro` 的 GitHub SSH 权限。如果已有本地副本，也可通过环境变量指定：
> ```bash
> export DATAZEN_PRO_PATH="/path/to/datazen-extension-sql-editor-pro"
> ```

---

## 4. 日常开发与实时调试

启动 Pro 版桌面端开发环境：

```bash
# 启动 Pro 模式（默认使用 basic 核心驱动：PG, MySQL, SQLite, Redis）
pnpm tauri:dev:pro

# 或者显式指定驱动列表进行调试（如加入 MongoDB 或 ClickHouse）
pnpm tauri:dev --edition=pro --drivers=postgres,mysql,mongodb
```

### 极速 HMR 热更新
- Vite 会自动监控 `packages/pro-extensions/sql-editor-pro/src/` 中的任何文件修改。
- 只要保存文件，桌面应用内部的 SQL 编辑器即刻**毫秒级热更新**，**无需重启前端，也无需重新编译 Rust 后端**。

---

## 5. 功能改进开发流程

### 场景一：纯插件内部特性改进（不修改扩展点契约）
*例如：优化表悬浮卡片展示字段注释、改进 JOIN 关联字段推断评分、增加新的 SQL 函数签名、扩展 Intention 意图等。*

1. **修改代码**：
   在 `packages/pro-extensions/sql-editor-pro/src/` 对应子模块中直接编写功能。
2. **本地调试**：
   在打开的 `pnpm tauri:dev:pro` 窗口中即时验证效果。
3. **补充/更新单元测试**：
   在模块同级 `__tests__/` 目录中增加测试用例。
4. **运行全量 Pro 测试**：
   ```bash
   pnpm test:pro
   ```
   *（所有 17 个测试文件、167+ 项测试通过方可提交）*
5. **在私有仓库内提交并推送**：
   ```bash
   cd packages/pro-extensions/sql-editor-pro
   git status
   git add .
   git commit -m "feat(hover): support column comments in table hover preview"
   git push origin main
   ```

---

### 场景二：新增或修改宿主扩展点契约（跨宿主与插件变更）
*例如：要在 SQL 编辑器顶部添加一个 Pro 专用的性能分析抽屉，或新增一种编辑器事件监听插槽。*

1. **第一步：在宿主中扩展特权契约**：
   在宿主 `packages/extension-points/src/sqlEditorProEP.ts` 中扩展 `SqlEditorProFeatures` 接口定义，例如：
   ```typescript
   export interface SqlEditorProFeatures {
     // ... 现有能力
     /** 新增：高级 SQL 性能分析面板提供者 */
     createProfilerPanel?: (ctx: SqlEditorContext) => React.ReactNode;
   }
   ```
2. **第二步：在宿主中编写消费与 Fallback**：
   在宿主组件（如 `src/components/sql-editor/SqlEditor.tsx`）中通过 `useExtension(sqlEditorProEP)` 消费该能力，并编写当插件未激活时的安全兜底逻辑：
   ```typescript
   const proFeatures = useExtension(sqlEditorProEP);
   // proFeatures?.createProfilerPanel ? proFeatures.createProfilerPanel(ctx) : null
   ```
3. **第三步：在 Pro 插件中实现新增契约**：
   在 `packages/pro-extensions/sql-editor-pro/src/proFeatures.ts` 中注入实现。
4. **第四步：双重验证**：
   ```bash
   pnpm test:unit # 验证宿主回归（Fallback 在社区版下依然完整正常）
   pnpm test:pro  # 验证 Pro 扩展全部能力通过
   ```
5. **第五步：分别独立提交两座仓库**：
   - 宿主仓库：`git commit` 提交契约与 Fallback（保持纯净，绝不含实现）；
   - 私有仓库：`cd packages/pro-extensions/sql-editor-pro && git commit && git push` 提交商业实现。

---

### 场景三：添加声明式设置项与自定义 UI
Pro 扩展支持通过声明式协议在宿主“设置 -> 查询编辑器”中动态注入设置项，宿主会自动渲染并自动完成加密持久化（无需修改 Rust 后端代码）：

在 `packages/pro-extensions/sql-editor-pro/src/proFeatures.ts` 的 `contributes.settings` 增加配置声明：

```typescript
contributes: {
  settings: [
    {
      id: 'sql-editor-pro.general',
      title: 'SQL Editor Pro 增强设置',
      items: [
        {
          id: 'editorTableHover',
          type: 'boolean',
          label: '启用表结构悬浮卡片预览',
          description: '悬停在表名或别名上方时显示列定义和外键',
          defaultValue: true,
        },
        // 支持自定义 React UI 渲染组件（例如自定义滑块、复合拾色器等）
        {
          id: 'editorCustomMetric',
          type: 'custom',
          renderItem: (ctx) => <MyCustomSettingControl {...ctx} />,
        },
      ],
    },
  ],
}
```

---

## 6. 测试与质量规范

请严格遵循项目防回归核心原则（详见 [docs/development/interaction-and-testing-principles.md](interaction-and-testing-principles.md)）：

1. **连续旅程测试（Journey Test）**：
   编辑器交互逻辑（如自动补全、JOIN 推荐、快捷键意图）禁止只测试合法完整语句，必须模拟键盘击键全过程（包括打到一半的残缺态），断言每一步的状态机跃迁与退出。
2. **测试专用命令**：
   | 命令 | 说明 | 范围 |
   | :--- | :--- | :--- |
   | `pnpm test:pro` | 运行 SQL Editor Pro 专属全部单元测试 | `packages/pro-extensions/sql-editor-pro` |
   | `pnpm test:unit` | 运行宿主核心单元测试（确保社区版 Fallback 完好） | `src/` 与共享 SDK |
   | `pnpm test:scripts` | 验证驱动/Pro 构建装配脚本测试 | `scripts/__tests__/` |

---

## 7. 本地打包验证与生产发布

### 本地编译发布包验证
在准备发版或合入大功能前，在本地进行一次真实打包验证：

```bash
# 1. 快速编译 Pro 版测试（仅包含 4 大核心数据库驱动）
pnpm tauri:build:pro:minimal

# 2. 全量驱动 Pro 版打包
pnpm tauri:build:pro
```

打包完成后，检查 `target/release/bundle/` 下生成的安装包，启动应用验证以下 Pro 特性均正常激活：
- [x] 表名/别名悬浮预览卡片
- [x] 函数括号内参数签名提示
- [x] 外键智能 JOIN 补全
- [x] 智能 Intention 操作（Alt+Enter / Option+Enter）
- [x] 参数绑定浮层与历史记录
- [x] Gutter 行内执行指示器

验证完毕后，执行工作区还原：
```bash
pnpm pro:restore
```

### GitHub Actions 自动化发布
仓库的 `.github/workflows/release.yml` 已配置自动构建 Pro 版。CI 环境通过 `PRO_DEPLOY_KEY`（或 `SQL_EDITOR_PRO_DEPLOY_KEY`）密钥在打包阶段自动将 `flyxl/datazen-extension-sql-editor-pro` 源码拉入编译管线，无需人工干预。
