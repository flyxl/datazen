# DataZen 全项目代码审查报告与修改计划

> 来源：2026-08-07 只读全项目审查（会话 [项目审查](168e349c-8389-4a29-baf4-7fc5dfb0d5aa) 等）。  
> 本文件保存审查结论与分期计划，供后续修复对照。  
> **进度跟踪**见 [`progress-code-review-fix.md`](./progress-code-review-fix.md)。

## 总览

架构骨架（驱动 Registry、IPC 分层、插件编译期注入）整体合理；主要问题集中在：**密钥与备份安全、MCP 与 GUI 数据目录不一致、导入非原子、10 语系名不副实、CI/文档/可复现构建**。

---

## 修改计划（分 4 期）

### 一期 — 安全与数据完整性（优先）

| ID | 问题 | 建议改法 |
|----|------|----------|
| S1 | 应用数据 ZIP 导出含明文 `.key`，备份≈全盘密钥 | 导出排除 `.key` + 口令派生；或整包加密；至少显著警告 |
| S2 | 导入先 `clear` 再 copy，失败会掏空数据目录 | 暂存目录校验完整后原子 rename / 失败可回滚 |
| S3 | SSH 隧道 `check_server_key` 永远信任 | known_hosts / TOFU + 指纹确认 |
| S4 | SSH 密码/口令明文写在 `connections.json` | 与 DB 密码同样加密，或整文件加密 |
| S5 | `read_file`/`write_file`/`open_path` 路径过宽 | 对话框+IO 原子化在 Rust；路径 IPC 仅 webdriver |
| S6 | ZIP 炸弹无解压上限 | 未压缩总大小/膨胀比上限 |
| S7 | MCP `query` 文档写默认 100，代码 `None` 不限 | 默认 100 + 硬顶；与 GUI `queryResultLimit` 对齐 |

### 二期 — 正确性与产品一致性

| ID | 问题 | 建议改法 |
|----|------|----------|
| C1 | `run_mcp_stdio` 用 `com.datazen.app`，GUI 用 `com.tbeasy.datazen` | 统一 data dir helper |
| C2 | 8 个语系≈英文壳；zh-TW 备份文案简繁混用 | 真翻译或 UI 标 beta/隐藏；补“与 en 差异率”测试 |
| C3 | Pro 包含 kiwi+superset，Release 文案只写 Kiwi | 对齐矩阵、文案、产品定义（含是否收 olap） |
| C4 | 插件 `git clone` 未钉 ref | `plugins-registry.json` 加 commit/tag |
| C5 | 导入/导出成功用 error 对话框 | success/error 分流 |
| C6 | `connection_id` 语义混乱（配置 ID vs 运行时 ID） | IPC/MCP 统一约定并改名 |
| C7 | `rebuild_menu` 里 `block_on` | 改为 async 命令 |

### 三期 — 工程卫生与可维护性

| ID | 问题 | 建议改法 |
|----|------|----------|
| E1 | 无 PR CI（仅 release/pages） | 加 `ci.yml`：vitest + cargo test |
| E2 | `.gitignore` 粘连：`src-tauri/Cargo.lockwindows.png` | 拆成两行 |
| E3 | AGENTS 写 `tauri:dev` 无插件，实际默认 `all` | 改默认或改文档 |
| E4 | E2E 默认编全部插件 + 全量 bundle | `DATAZEN_PLUGINS=none`；更快 debug 路径 |
| E5 | Store 写文件非原子；备份整库进内存 | tmp+rename；流式/`pg_dump` |
| E6 | DB 能力用 `!== false` 默认开启 | Registry 显式 opt-in |
| E7 | 死代码：`query` window / 未用的 `getSystemUiLanguage` | 实现或删除并对齐 |
| E8 | 文档计数/语系过时 | 刷新 AGENTS + testing 文档 |

### 四期 — 体验与性能（可选，本次不执行）

| ID | 问题 | 建议改法 |
|----|------|----------|
| P1 | release `lto + codegen-units=1 + opt-level=s` 拖慢 Windows CI | 评估 `thin` LTO / `opt-level=3` |
| P2 | AI `max_tokens` 默认 20 万 | 降到合理默认 |
| P3 | 密钥仅存 Base64 `.key` 文件 | 迁 Keychain/DPAPI |
| P4 | SQL/NL 打 info 日志 | 降级 debug / 脱敏 |
| P5 | splash 过早隐藏、硬编码中英错误串 | 等 bootstrap；走 i18n |

---

## 建议落地顺序

1. **一期 S1–S7**（数据丢不了、密钥不裸奔、路径 IPC、MCP 限流）
2. **二期 C1–C7**（产品可信 + 可复现）
3. **三期 E1–E8**（工程卫生）

## S5 已落地说明（会话内先行）

生产路径已改为 Rust 侧 `*_with_dialog` 原子命令；`write_file` / `read_file` / `open_path` / 路径版备份导入导出等仅在 `webdriver` feature 下可用。详见 `docs/e2e-testing.md` 与 `e2e/specs/path-ipc-hardening.ts`。
