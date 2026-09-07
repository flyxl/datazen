# Track arch-p1-ui: Common Design System (@datazen/ui)

> Status: **PASSED**
> Branch: `feature/arch-p1-ui`
> Base branch: `feat/sql-editor`
> LastHeartbeat: 2026-09-07T23:20:00+08:00

---

## Task Summary
1. 建立 `packages/ui/`：
   - 独立的 `package.json`（name: `@datazen/ui`），peerDependencies: react, react-dom。
   - 抽离通用原子组件：
     - `Button.tsx` (from `src/components/ui/Button.tsx`)
     - `Input.tsx` (from `src/components/ui/Input.tsx`)
     - `Select.tsx` (from `src/components/ui/Select.tsx`)
     - `Dialog.tsx` (from `src/components/ui/Dialog.tsx`)
     - `Tabs.tsx` (from `src/components/ui/Tabs.tsx`)
     - `Badge.tsx` (from `src/components/ui/Badge.tsx`)
     - `Label.tsx` (from `src/components/connection/shared.tsx`)
     - `cn.ts` (from `src/lib/cn.ts`)
2. 在 `src/components/ui/` 和 `src/lib/` 保留 re-export，以保证所有现有宿主代码无缝向后兼容。
3. 在 root `tsconfig.json` 与 `vite.config.ts` 中配置别名 `@datazen/ui` -> `packages/ui/src/index.ts`。
4. 编写 `packages/ui` 的基础渲染测试，确保通过。

---

## Phase: PASSED

### Implementation Commit
- Hash: `f3dab8f91`

### Test Commit
- Hash: `56c55301b`

### Changed Files
- `packages/ui/package.json`
- `packages/ui/tsconfig.json`
- `packages/ui/src/index.ts`
- `packages/ui/src/cn.ts`
- `packages/ui/src/tid.ts`
- `packages/ui/src/Button.tsx`
- `packages/ui/src/Input.tsx`
- `packages/ui/src/Select.tsx`
- `packages/ui/src/Dialog.tsx`
- `packages/ui/src/Tabs.tsx`
- `packages/ui/src/Badge.tsx`
- `packages/ui/src/Label.tsx`
- `packages/ui/src/__tests__/primitives.test.tsx`
- `src/components/ui/Button.tsx` (re-export)
- `src/components/ui/Input.tsx` (re-export)
- `src/components/ui/Select.tsx` (i18n wrapper)
- `src/components/ui/Dialog.tsx` (i18n wrapper)
- `src/components/ui/Tabs.tsx` (re-export)
- `src/components/ui/Badge.tsx` (re-export)
- `src/lib/cn.ts` (re-export)
- `src/components/connection/shared.tsx` (Label re-export)
- `tsconfig.json` (path alias)
- `vite.config.ts` (alias)
- `vitest.config.ts` (alias + include)

### Coder Self-Verification
| Suite | Result |
|-------|--------|
| `npx vitest run packages/ui` | 9 passed |
| `npx vitest run src/components/ui` | 50 passed |
| `npx tsc --noEmit` | pass |

### Independent Tester Verification (2026-09-07)
| Suite | Coder | Tester | Match |
|-------|-------|--------|-------|
| `npx vitest run packages/ui` | 9 passed | **9 passed** (1 file) | ✓ |
| `npx vitest run src/components/ui` | 50 passed | **50 passed** (8 files) | ✓ |
| `npx tsc --noEmit` | pass | **pass** | ✓ |

### Code Review Summary
- **`packages/ui/` 解耦**：Button / Input / Select / Dialog / Tabs / Badge / Label / cn 均无 Zustand Store、IPC、`useI18n` 或 `commands/` 引用；仅依赖 React、lucide-react、clsx、tailwind-merge。
- **宿主 re-export 链路**：
  - 直接 re-export：`Button`、`Input`、`Tabs`、`Badge`、`cn`、`Label`（via `shared.tsx`）。
  - 薄 i18n wrapper：`Select`（注入 `labels`）、`Dialog`（注入 `closeLabel`），符合设计预期。
- **审查备注（非 Bug）**：
  - `Button` 保留 `run` variant（SQL 执行语义 CSS token），属样式约定而非业务耦合。
  - `Select`（515 行）交互路径仅基础 open/listbox 单测覆盖；复杂 combobox/keyboard 路径留待后续补齐。
  - Vitest `--coverage` 全局阈值针对 `src/**`，未单独度量 `packages/ui/`；不影响本轨验收。

### E2E Registry
| Journey | Status | Notes |
|---------|--------|-------|
| 宿主 UI 组件 smoke（Button/Input/Select/Dialog 等） | 【留待 R 回归】 | 本轨为底层抽包，无 Host UI 交互路径变更；现有 `src/components/ui/__tests__/` 50 项单测已覆盖宿主 wrapper 兼容性。 |

### Notes
- `Select` / `Dialog` 在宿主保留薄 i18n wrapper（注入 `labels` / `closeLabel`），其余组件直接 re-export。
- `Select` 在 package 层使用 `labels` prop + 英文默认值，脱离宿主 `useI18n` 依赖。
