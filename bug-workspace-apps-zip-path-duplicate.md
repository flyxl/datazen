# BUG · workspace-apps 复制了宿主的 zip 路径校验

**状态**：开放 · Wave-3 必修 · **两份副本必须同一个提交内一起改**

## 症状

zip-slip 防护逻辑在全仓有**两份逐字节相同**的实现。

## 位置

| 位置 | 内容 |
| --- | --- |
| `packages/workspace-apps/src/zip_path.rs:21` | `MAX_COMPRESSION_RATIO: u64 = 100` |
| `packages/workspace-apps/src/zip_path.rs:24` | `validate_zip_entry_path` |
| `packages/workspace-apps/src/zip_path.rs:66` | `validate_theme_zip_path` |
| `src-tauri/src/app_data_archive.rs:159` | `pub fn validate_zip_entry_path` |
| `src-tauri/src/app_data_archive.rs:203` | `MAX_COMPRESSION_RATIO` |

## 证据

逐行读过 `zip_path.rs:24-64` 与 `app_data_archive.rs:159-199`：**41 行逐字节相同**
（反斜杠归一、`//` 前缀、`C:/` 盘符、`ParentDir`、`RootDir | Prefix(_)` 全部一致）。
两份文件各自的文档注释（`zip_path.rs:4`、`:10`）都已承认宿主那份的存在。

## 影响

安全相关的路径校验出现双源。任一处单边修改 ⇒ 一侧仍按旧规则放行。

## 消费者

- 宿主：`app_data_archive.rs:219/:356/:1051/:1081`，测试 `:689-693` `:865` `:867-868` `:873-875` `:1022`
- 宿主外部调用：`src-tauri/src/commands/wapps/storage.rs:77`
- B1：`install.rs:23/:135/:293`、`manifest.rs:332`，测试 `:92` `:122`

## 修法

合并进 `platform-infra::path_safety`（预置提交 `179c6c8b6` 建了该文件但**内容留空推迟**，即为此留的坑），
两份调用点一并切过去。

## 关闭条件

全仓 `validate_zip_entry_path` / `validate_theme_zip_path` / `MAX_COMPRESSION_RATIO` 的**定义**只剩 `platform-infra` 一处；
`cargo test -p datazen --lib` 与 `cargo test -p datazen-workspace-apps` 全绿。
