# Track arch-p2-pro: Extract Enhanced SQLEditor Features to Private Repo

> Status: **PASSED**
> Branch: `feature/arch-p2-pro`
> Base branch: `feat/sql-editor`
> LastHeartbeat: 2026-09-07T23:31:30+08:00
> Coder Commit: `fb0cfa7ed`
> Tester Commit: `d24cfc3ee`

---

## Task Summary
1. 将增强 SQLEditor 功能独立迁移至私有仓库 `https://github.com/flyxl/datazen-extension-sql-pro`：
   - 本地克隆或初始化该独立 npm 模块：`@datazen/extension-sql-pro`
   - 转移并封装如下 Pro 级特权增强功能：
     - `src/intentions/`（Star expansion, insert hint, function hint, qualifier actions, analyze intentions）
     - `src/hover/`（Table hover tooltip, definition navigation Mod+Click）
     - `src/signature-help/`（函数签名提示与 parameter inlay）
     - `src/join-completion/`（外键关联智能补全）
     - `src/paste/`（Paste as IN, Drop caret）
   - 实现 `@datazen/extension-points` 的 `ExtensionPoint` 激活入口（`activate(registry)`）
   - 提交并推送至 `flyxl/datazen-extension-sql-pro` 私有仓库的 `main` 分支。
2. 在 datazen 仓库的工作树中：
   - 移除 `src/components/sql-editor/` 下上述增强目录与测试；
   - 在 `editorExtensions.ts` 和 `SqlEditor.tsx` 中保留干净的 Fallback 扩展点（基础语法、基本补全、Gutter 运行、参数绑定均完整保留）；
   - 确保 `datazen` 仓库自身无任何 Pro 业务逻辑，但可通过 `@datazen/extension-points` 随时插拔 Pro 扩展；
   - 验证宿主测试通过。

---

## Deliverables

| Item | Status |
|------|--------|
| Private repo `@datazen/extension-sql-pro` pushed to `main` | Done (`02de84e`) |
| `sqlEditorProEP` in `packages/extension-points` | Done |
| Host pro modules removed; fallback wired | Done |
| Self-test vitest `src/components/sql-editor` | **350 passed** |
| Self-test `npx tsc --noEmit` | **0 errors** |

---

## Tester Verification (2026-09-07, fresh instance re-run)

### Code Review
- ✅ 私有仓库 `flyxl/datazen-extension-sql-pro` 存在（private），`main` 含完整 Pro 模块（intentions/hover/signature-help/join-completion/paste）及 `activate()` 入口（`src/index.ts` → `proFeatures.ts`）。
- ✅ 宿主已移除 `intentions/`、`hover/`、`signature-help/`、`join-completion/` 目录及 Pro 测试；`paste/` 保留薄委托层（`createPasteExtensions` / `contextMenuItems` 经 `sqlEditorProEP` 转发，Fallback 返回空/null）。
- ✅ `packages/extension-points/src/sqlEditorProEP.ts` 契约清晰，Fallback 全 no-op。
- ℹ️ `editorExtensions.ts` 中 completion/intention/hover 均通过 `extensionRegistry.get(sqlEditorProEP)` 委托，Fallback 下 JOIN/signature/intention/hover 为空扩展。

### Independent Re-run (Coder `fb0cfa7ed`)
| Suite | Coder Self-Report | Tester Measured (fresh instance) |
|-------|-------------------|----------------------------------|
| `npx vitest run src/components/sql-editor` | 350 passed | **21 files, 350 passed** (5.20s) |
| `npx tsc --noEmit` | 0 errors | **0 errors** |

### Coverage (changed modules, targeted)
| File | Lines |
|------|-------|
| `packages/extension-points/src/sqlEditorProEP.ts` | 44% (fallback fns not unit-tested; exercised via paste/contextMenu fallback test) |
| `src/components/sql-editor/paste/*` | 85% |
| `src/components/sql-editor/editorExtensions.ts` | 0% direct (outside vitest Option-C gate; sql-editor suite green) |

### E2E Registry
| Journey | Status | Notes |
|---------|--------|-------|
| SQLEditor basic completion + gutter (Fallback) | 【留待 R 回归】 | Pro 未注册时行为由 350 单测覆盖；Host E2E 无 Pro 路径变更 |
| Pro extension activate + intentions/hover | 【留待 R 回归】 | 需私有 npm 包安装 + license；不在 Fallback 验收范围 |

---

## Notes
- 基础补全仍依赖宿主内 `src/lib/sqlFunctionRegistry.ts` 与 `metadata/findRelation.ts`（非 Pro 专属，schema 补全共用）。
- Pro 激活：宿主启动时调用 `@datazen/extension-sql-pro` 的 `activate(extensionRegistry)` 即可注入增强实现。
