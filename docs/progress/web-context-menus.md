# Web Context Menu + 剩余 Data Sync

> 分支：`feat/web-context-menus`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus`  
> 基线：`main` @ `4572fb7`

## 分支清理（已完成）

已删除已合入 `main` 的分支与 worktree：

- `feat/data-sync-navicat`（local + origin）
- `agent/data-synchronization-prd`（local + origin）
- `.worktrees/data-sync-navicat`

保留：`main`、`origin/feat/driver-backup-restore`（未合入，进行中）。

## 功能清单

| ID | 功能 | 状态 | 单测 | 独立 QA | 覆盖率 | Commit |
|----|------|------|------|---------|--------|--------|
| F1 | Web 菜单定位 + 组件（二级菜单防截断） | done | 17 PASS | [F1 QA](c86bf24c-a11a-4a42-8d30-b310c3fdeb2c) PASS | lines 87.5% | `e983514` |
| F2 | MainWindow 连接/分组/空白右键改 Web 菜单 | done | 42 PASS | [F2 QA](86de97e9-365b-4ba8-82f1-0db550159c1a) PASS | mainWindowContextMenu.ts lines 100% | — |
| F3 | Host `showNativeContextMenu` 全部改为 Web | done | 78 PASS | [F3 QA](80d620d6-47c9-4b91-857d-ae5edf8cb0a4) PASS | nativeContextMenu.ts lines 100% | — |
| F4 | Redis 驱动右键改 Web 菜单 | done | 7 PASS | [F4 QA](268756f1-b0fb-4d4e-85b2-4af08031922b) PASS | redisKeyContextMenu.ts lines 100% | — |
| F5 | Data Sync：Cancel IPC + Apply / 行比较接线 | done | Rust 31 + FE 26 PASS | [F5 QA](43c82485-a192-40f6-a384-f8384d9f7767) PASS | jobs/exec/apply lines 83.97% | — |
| F6 | 架构 / AGENTS 文档 + merge main + push | done | docs | static | — | — |

提交时排除自动生成文件：`src/plugins/generated.ts`、`generated-locales.ts`、`src-tauri/src/plugin_init.rs`、`.plugin-features.json`（已 gitignore）。

## 测试约定

- 开发时写单元测试；验收由**新开**测试 agent 执行，禁止开发 agent 自充验收。
- 变更模块行覆盖率 ≥ 80%。
- 测试不通过 → 另开编码 agent 修复 → 再开新测试 agent。
- 通过后才 commit。
