# 代码审查修复进度（一期～三期）

> 对照计划：[`code-review-2026-08-07-full.md`](./code-review-2026-08-07-full.md)  
> 分支：`fix/code-review-phase-1-3`  
> 流程：开发（含单元测试）→ **独立测试 Agent** 出 E2E/结果 → 失败则编码 Agent 修复 → 通过后提交。

## 总表

| ID | 标题 | 开发 | 单元测试 | 测试 Agent | 提交 |
|----|------|------|----------|------------|------|
| S1 | ZIP 导出排除 `.key` | ✅ | ✅ 18 lib | ✅ ADB-008 + unit | ✅ 见下 |
| S2 | 导入原子化 / 可回滚 | ✅ | ✅ 19 lib | ✅ unit+E2E | ✅ 见下 |
| S3 | SSH known_hosts / TOFU | ✅ | ✅ 11 ssh_tunnel | ✅ unit | ✅ |
| S4 | SSH 密码加密存储 | ✅ | ✅ store:: 5 | ✅ unit | ✅ |
| S5 | 路径 IPC 收紧 | ✅ | ✅ | ✅ path-ipc E2E | ✅ |
| S6 | ZIP 炸弹防护 | ✅ | ✅ 24 lib | ✅ unit+E2E | ✅ |
| S7 | MCP query 默认/硬顶 | ✅ | ✅ resolve_query_limit | ✅ unit | ✅ |
| C1 | MCP/GUI data dir 统一 | ⬜ | ⬜ | ⬜ | ⬜ |
| C2 | 语系真翻译或 beta | ⬜ | ⬜ | ⬜ | ⬜ |
| C3 | Pro 文案与插件矩阵对齐 | ⬜ | ⬜ | ⬜ | ⬜ |
| C4 | 插件钉 commit/tag | ⬜ | ⬜ | ⬜ | ⬜ |
| C5 | 成功/错误对话框分流 | ⬜ | ⬜ | ⬜ | ⬜ |
| C6 | connection_id 语义统一 | ⬜ | ⬜ | ⬜ | ⬜ |
| C7 | rebuild_menu 去 block_on | ⬜ | ⬜ | ⬜ | ⬜ |
| E1 | PR CI（vitest + cargo test） | ⬜ | ⬜ | ⬜ | ⬜ |
| E2 | `.gitignore` 粘连拆分 | ⬜ | ⬜ | ⬜ | ⬜ |
| E3 | AGENTS / 插件默认对齐 | ⬜ | ⬜ | ⬜ | ⬜ |
| E4 | E2E 快速构建路径 | ⬜ | ⬜ | ⬜ | ⬜ |
| E5 | Store 原子写 | ⬜ | ⬜ | ⬜ | ⬜ |
| E6 | DB 能力显式 opt-in | ⬜ | ⬜ | ⬜ | ⬜ |
| E7 | 死代码清理 | ⬜ | ⬜ | ⬜ | ⬜ |
| E8 | 文档刷新 | ⬜ | ⬜ | ⬜ | ⬜ |

图例：⬜ 未开始 · 🟡 进行中 · ✅ 完成 · ❌ 失败待修

## 详细记录

### S1 — ZIP 导出排除 `.key`

- **实现**：`should_exclude` 排除 `.key`；导入时若 ZIP 无 `.key` 则保留本机密钥；legacy ZIP 含 `.key` 仍可恢复
- **单元测试**：`cargo test -p datazen --lib app_data_archive` — 18 passed
- **测试 Agent**：初测 PARTIAL（旧 webdriver 二进制）；修复后复测 **PASS**（unit 18/18，`app-data-backup` 8/8 含 ADB-008）
- **Bug**：无（初测「ZIP 仍含 .key」为过期二进制，非源码缺陷）
- **提交**：本提交

### S5 — 路径 IPC 收紧

- **实现**：`*_with_dialog` 原子命令；路径 IPC 门控 `webdriver`；`open_log_dir` / `open_workflows_dir` / `open_context_dir`
- **单元测试**：`commands::file` / `adb` / `config` / `backup` 门控与校验
- **测试 Agent 结果**：`path-ipc-hardening` 6/6、`app-data-backup` 7/7→8/8 通过
- **Bug**：无
- **提交**：本提交

### S2 — 导入原子化 / 可回滚

- **实现**：staging → sibling prepared → `rename` 交换；失败回滚；移除就地 `clear_dir_contents`
- **单元测试**：19 passed（含 `import_swap_failure_restores_original_data_dir`）
- **测试 Agent**：PASS（unit + app-data-backup 8/8）
- **Bug**：无
- **提交**：本提交

### S6 — ZIP 炸弹防护

- **实现**：512MiB 未压缩上限、压缩比 100、条目数 10 万；预扫描 + 流式限流
- **测试 Agent**：PASS（24 unit + app-data-backup 8/8）
- **提交**：本系列提交

### S7 — MCP query 默认/硬顶

- **实现**：`resolve_query_limit`：None→100，硬顶 50000；工具描述同步
- **测试 Agent**：PASS（3 unit）
- **提交**：本系列提交

### S3 — SSH known_hosts / TOFU

- **实现**：`ssh_known_hosts.json` TOFU；指纹不匹配拒绝
- **测试 Agent**：PASS（11 unit）；E2E 定为 UNIT-COVERED
- **已知低风险**：无 UI 指纹确认；损坏 known_hosts 会重新 TOFU

### S4 — SSH 密码加密存储

- **实现**：persist/load 加密/解密 `ssh_tunnel.password` / `passphrase`
- **测试 Agent**：PASS（store 5 tests）
- **已知低风险**：旧明文 SSH 字段无迁移路径（解密失败则清空）
