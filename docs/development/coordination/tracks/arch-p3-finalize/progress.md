# Track arch-p3-finalize: Git History Purification & Verification

> Status: **PASSED**
> Branch: `feat/sql-editor`
> LastHeartbeat: 2026-09-07T23:45:00+08:00
> Coordinator / Finalizer: Complete

---

## 任务目标与交付

1. **宿主仓库纯净性（Zero-Pro in DataZen）**：
   - 彻底移除了 `src/components/sql-editor/` 下的所有 Pro 增强业务代码（intentions、hover、join-completion、signature-help、paste-as-in、drop-caret）；
   - 移除了 Host 设置项（`editorInsertValueHints`、`editorTableHover`）、中英文本地化词条、Rust 后端 store 字段及相关单元测试；
   - 实现了基于 `@datazen/extension-points` 的 `sqlEditorProEP` 扩展点，宿主开箱自带纯净轻量的 Fallback 行为；
   - 外部商业/增强扩展已全部转移至独立 GitHub 私有仓库：`https://github.com/flyxl/datazen-extension-sql-editor-pro`。

2. **文档与规范收口**：
   - `docs/todo/sql-editor/prd.md`、`docs/todo/sql-editor/implementation-plan.md`、`docs/todo/sql-editor/README.md` 中的 Pro 特性已重构为扩展点描述，删除了多余的过时测试报告；
   - `AGENTS.md` 正式确立 **四维扩展体系 (Driver / Theme / EP / Workspace App)** 与公共设计系统 `@datazen/ui`；
   - 根目录 `LICENSE` 的 **DataZen Plugin, Driver & Extension Linking Exception** 更新，明确保护了 Driver、EP、Workspace App 的独立闭源许可豁免权；
   - `docs/todo/architecture-refine.md` 与 `architecture-refine-plan.md` 已标记为全量落地。

3. **全量回归验证**：
   - `npx vitest run`: 382 test files, 2697 tests 全部通过。
   - `npx vitest run --config vitest.drivers.config.ts`: 18 test files, 97 tests 全部通过。
   - `cargo test -p datazen --lib`: 全部通过（0 errors）。
   - `npx tsc --noEmit`: 0 errors。
