# 宿主边界后续：实施方案

> **状态：已确认的实施方案，代码尚未按本文改动。**
> 确认日期：2026-10-29。基线：`main` 上 form-split 已落地（`apps/desktop`、`packages/frontend`、`modules` crate `datazen`）。
> 本文放在分支 `docs/path-facts-after-form-split`，不打开 PR，避免每次推送触发 CI。
> 合并前仍以代码为准；本文只约束这一组后续改动的切法。

已确认的决定：

1. 比对结果的跨进程租约可以去掉 SQLite，比较正文的落盘格式不变。
2. `connection_import` 里的 SQLite 只用于读取 **DBX** 的 `dbx.db`，不能拿掉。
3. 传输无关的命令体进 `datazen` crate；`datazen_desktop` 只留 Tauri 胶水。
4. 桌面 Cargo feature 列表写成显式全集，含今天被传递打开的 feature。
5. Rust 共享实现新建 `packages/driver-support`。现有 `@datazen/driver-sdk` 保持 TypeScript，不改名、不搬语言。
6. 隧道协议实现继续留在 `modules` 的 `tunnel` feature，桌面和 Web 共用。宿主只注入存储根、凭据和监听点。
7. `@datazen/frontend` 收成稳定入口；桌面专用文件挪到 `apps/desktop`。禁止新增 `@datazen/frontend/src/**` 深导入。

不做的事：

- 不把 `modules` 拆回多个业务 crate。
- 不把隧道实现搬进 `datazen_desktop`。
- 不把命令 DTO 或错误映射放进 `datazen_desktop` 让 Web 再抄一份。
- 不改比对结果的 manifest / `.rows` 格式。
- 本分支不实现下列阶段，只记录方案。

建议落地顺序：F0 文档与 feature 列表 → F1 租约 → F2 前端入口 → F3 `driver-support` → F4 命令体。F4 按功能切片，禁止一次搬完 `commands/`。

---

## 1. 比对库的排他事务

### 现在实际在锁什么

`modules/src/migration/data_sync/comparison_store/` 的比较正文 **不是** SQLite：

- 小于 `COMPARISON_MEMORY_LIMIT`（8 MiB）的结果留在内存。
- 更大的结果写到调用方给的存储根下：`manifest.json` + `table-*.rows`。翻页按偏移读，不反序列化整份比较。
- 存储根由 `ComparisonStorageRoot` 注入，并按 `OrganizationScope` 拒绝跨组织访问。

`rusqlite` 只出现在 `comparison_store/recovery.rs` 的 `OwnerLease`：进程活着时握着一个 SQLite 排他事务；进程崩溃后由操作系统放锁，下一个进程才能回收残留的 `comparison-*` 目录。`data_sync` feature 上的 `rusqlite` 依赖就是为了这把锁。`Cargo.toml` 里「comparison_store/plan_store 的 SQLite 侧实现」这句写宽了，`plan_store` 并不把计划存进 SQLite。

### 可替换方案

锁的要求只有三条：进程活着时互斥、进程崩溃后内核自动释放、本地磁盘即可（存储根不是网络盘）。

| 方案 | 崩溃后谁放锁 | 评价 |
|---|---|---|
| 维持 SQLite 排他事务 | SQLite/OS | 能用，但为了一把锁链入 `rusqlite`（bundled） |
| **推荐：对锁文件持有 `flock` / `LockFileEx`** | 内核，fd 关闭或进程死亡即释放 | 与现在的生命周期相同。Unix `flock`，Windows `LockFileEx`。可直接用 `fd-lock` 或等价封装，不必自己写 syscall。锁文件放在存储根的 `.owners/<owner-id>.lock`，`OwnerLease` 在进程内握着 `File`，字段 `_connection` 换成这个 `File` |
| 命名互斥量 / Unix 抽象套接字 | 内核 | 可行，但要两套 OS 对象，没有相对 flock 的收益 |
| PID 文件 + 探活 | 没有内核释放；靠读 `/proc` 或 `OpenProcess` | 不采用。PID 会复用，容器里误判 |

不采用「把比较正文改存进 SQLite」——那是换存储格式，不是换锁。

### 实施

1. `OwnerLease` 改为持有锁文件的 `File`（`fd-lock`）。获取失败的语义与现在的 `DatabaseBusy` / `DatabaseLocked` 对齐：别人还活着，不回收。
2. 回收路径改为：能抢到锁才删除对应 `comparison-*` 目录，然后立刻放锁或关闭 fd。
3. 删掉 `recovery.rs` 对 `rusqlite` 的依赖。若 `data_sync` 不再使用 `rusqlite`，从该 feature 去掉 `dep:rusqlite`。
4. 更新 `modules/Cargo.toml` 里 `data_sync` 的注释，写明 SQLite 租约已替换。
5. 保留并改写 `recovery` / `tester_recovery` 测试：制造「锁被持有」与「锁已关闭」两种夹具，不再打开 SQLite。

`connection_import` 的 `rusqlite` 与此无关，见下一节，不要在 F1 里删。

---

## 2. `connection_import` 的「别人」是谁

`modules/src/connection_import/mod.rs` 的导入格式是：

| `ImportFormat` | 来源 | 文件 | 是否 SQLite |
|---|---|---|---|
| `DataZen` | DataZen 自己的导出 | JSON / `.datazenconnection` | 否 |
| `DBeaver` | DBeaver | `data-sources.json`，同目录可选 `credentials-config.json` | 否 |
| `Navicat` | Navicat | Navicat 导出 | 否 |
| `DataGrip` | DataGrip | DataGrip 导出 | 否 |
| `TablePlus` | TablePlus | `.tableplusconnection` 或 `Connections.plist` | 否 |
| `DbxEncrypted` / `DbxPlain` | **DBX** | 活库 `dbx.db` | **是** |

「别人」指用户机器上已有的这些数据库客户端，不是任意第三方 SQLite 文件。只有 DBX 把连接和密钥放在 SQLite 里（表 `connections.config_json`、`connection_secrets`）。`parse_sqlite` 先把 `dbx.db` 连带 `-wal`/`-shm` 复制到临时目录，再用 `rusqlite` 只读打开，避免碰到正在使用的活库。

因此：

- F1 可以去掉 `data_sync` 的 `rusqlite`。
- `connection_import` feature 继续依赖 `rusqlite`，直到不再支持导入 DBX。不要用通用 SQL 解析器替换这段读取。

---

## 3. 命令体：桌面与 Web 共用

`apps/desktop/src-tauri/src/commands/` 里体积大的是编排（同步执行、传输预览、schema diff job），不是 DTO 映射。DTO 和错误映射若留在 `datazen_desktop`，Web 宿主会再写一遍。

三层，依赖只向下：

```text
datazen_desktop 或未来 apps/web
  传输胶水（Tauri command / HTTP handler）、窗口、对话框、钥匙串、托盘
        │  只调用
        ▼
datazen crate 内各 feature 的 api 模块
  入参 DTO、RequestContext、调用 application、返回 ApiError
        │
        ▼
该 feature 的 application / domain
```

约定：

- `api` 模块放在对应 feature 下，例如 `modules/src/connection/api.rs`，不要新建一个包揽全部命令的 crate，也不要做全局 `host_api` 上帝模块。
- `api` 不依赖 Tauri、Axum、`AppState`。入参是 `RequestContext`（或现有 `OrganizationScope`）加上已经构造好的端口/服务。
- 序列化形状与今天的 IPC 一致：前端 camelCase，Rust snake_case + `#[serde(rename_all = "camelCase")]`。能复用 `datazen-driver-api` 里已有 DTO 的不要再定义一份。
- 错误用 `modules` 已有的 `ApiError` / feature error。桌面把它们映射成 `CommandError`，Web 以后映射成 HTTP 状态。映射表只许有一份桌面版、一份 Web 版，业务错误文本在 `api` 层定稿。
- 桌面专有能力留在宿主：系统对话框、托盘、钥匙串、`datazen://`、窗口句柄。这些函数没有 `api` 孪生。
- 隧道仍然是 `modules::tunnel`。宿主胶水只负责把存储根和凭据送进已有端口。

`get_connections` 这类已经在调 `connection` 服务的命令作为第一刀模板，目标形态：

```rust
// modules::connection::api
pub async fn list_connections(ctx: &RequestContext, svc: &ConnectionService) -> Result<Vec<ConnectionConfig>, ApiError>;

// apps/desktop command
#[tauri::command]
pub async fn get_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, CommandError> {
    let ctx = state.request_context()?;
    let svc = require_connection_service(&state)?;
    connection::api::list_connections(&ctx, svc).await.map_err(to_command_error)
}
```

切片顺序：

1. `connection`（已经较薄，用来固定 `api` 模块的写法、错误映射和测试位置）。
2. 只读查询与 schema / 表浏览。
3. export、backup。
4. 最后才是 `commands/sync`、`commands/data_transfer`、`commands/schema_diff`。每刀只搬一个用例（例如 preview 或 cancel），搬完删宿主里的算法副本。禁止只复制不删除。

验收：宿主生产代码里不再出现 SQL 拼装、行对比、计划展开。`commands` 下单个生产文件以「一个传输函数大约十几行」为完成标准，测试可以仍放在宿主，但断言的是映射和权限，不是算法。算法测试留在 `modules`。

---

## 4. 桌面 feature 显式全集

今天 `apps/desktop/src-tauri/Cargo.toml` 的 `datazen` features 没有写出 `jobs`、`workflow`、`mcp`、`common`，但它们被 `schema_diff` / `data_sync` / `data_transfer` / `ai` 传递打开。读清单的人会以为 MCP 没编进桌面。

F0 把传递依赖改成显式列出，并在注释中写「桌面二进制启用全部已落地业务 feature；未列出的 feature 不得被桌面意外打开」。至少补上：

`common`、`jobs`、`workflow`、`mcp`

不改 feature 的含义，不借此删减桌面能力。改完后 `cargo tree -p datazen_desktop -i datazen`（或等价的 `--features` 核对）应与改前的启用集合一致。

---

## 5. `driver-api` 与 `driver-sdk` 怎么分

两边语言不同，不要共用一个 crate 名。

| 路径 | 包名 | 语言 | 内容 |
|---|---|---|---|
| `packages/driver-api` | `datazen-driver-api` | Rust | **只留** trait、跨驱动 DTO、`PROTOCOL_VERSION`。Host 与每个 driver crate 都依赖它 |
| `packages/driver-support`（新建） | `datazen-driver-support` | Rust | 共享 **实现**：已稳定的类型解析/格式化辅助、流式读取骨架、备份分块里与方言无关的部分。只被 `packages/drivers/*` 依赖，不被前端依赖 |
| `packages/driver-sdk` | `@datazen/driver-sdk` | TypeScript | 维持现状：metadata、SQL dialect、schema bridge、driver command 的前端契约。peer 依赖 React。不放入 Rust 代码 |

`driver-support` 没有 `package.json`。Cargo workspace `members` 用现有的 `packages/drivers/*` 不够覆盖它，要在根 `Cargo.toml` 的 `members` 里加 `packages/driver-support`。

搬迁规则：

- 第一批只搬已经被两个以上驱动复制、且不含方言 SQL 的函数（优先 `parse_type_parts` / `format_type` 一类纯函数，若它们今天不在 trait 定义里）。
- trait 方法和 `PROTOCOL_VERSION` 留在 `driver-api`。搬迁不得顺手改协议。
- 只有一个驱动使用的代码留在该驱动里。
- 前端继续只 import `@datazen/driver-sdk`，不新增指向 Rust 包的绑定。

---

## 6. 深导入：具体改法

### 问题

`packages/frontend/package.json` 的 `"."` 指向 `src/index.ts`，而该文件只导出 `DESKTOP_CAPABILITIES`、`WEB_CAPABILITIES`、`IS_DESKTOP_BUILD`。

桌面真实入口不走这个文件。[apps/desktop/src/main.tsx](../../apps/desktop/src/main.tsx) 直接：

```ts
import App from '@datazen/frontend/src/App';
import { installTauriEventUnlistenRaceWorkaround } from '@datazen/frontend/src/lib/tauriEventCompat';
import { useSchemaStore } from '@datazen/frontend/src/stores/schemaStore';
```

`exports` 还有 `"./src/*": "./src/*.js"`。源码是 `.ts`/`.tsx`，仓库里没有这些 `.js`。只有 Vite 的 TypeScript 解析能工作。包内文件互相引用必须继续用相对路径，那不是问题。

另外 `tauriEventCompat` 是桌面补丁，放在共享包里等于公开 API 的一部分。

### 目标入口

改完后 `apps/desktop` 只能使用下面三个说明符（测试 mock 也一样）：

| 说明符 | 导出 |
|---|---|
| `@datazen/frontend` | `startSharedApp`、平台端口与能力位（`configurePlatform`、`requirePlatform`、契约类型、`DESKTOP_CAPABILITIES`、`WEB_CAPABILITIES`、`IS_DESKTOP_BUILD`） |
| `@datazen/frontend/chrome` | 壳层专用的最小表面：`useI18n`、`usePlatform`、`useUiStore` 中 TitleBar 真正用到的选择子。不导出全部 store |
| `@datazen/frontend/styles.css` | `src/styles/globals.css` |

`exports` 的值写成真实存在的源文件（`.ts` / `.tsx` / `.css`），删除 `./src/*` → `.js` 这条。

`startSharedApp()` 吃掉今天 `main.tsx` 里所有共享启动步骤：启动计时、加载 `App`、全局 CSS、splash、滚动锁、图标、locale、`generated-pro` 初始化、schema/table/panel 等 store 的预热。这些 import 变成 `packages/frontend` 内部的相对路径。

桌面 `main.tsx` 收敛成：

```ts
import { startSharedApp } from '@datazen/frontend';
import { installTauriEventUnlistenRaceWorkaround } from './platform/desktop/tauriEventCompat';
import { maybeCheckOnStartup } from './platform/desktop/updater';
import { assembleDesktopPlatform } from './shell/desktop/assembly';

installTauriEventUnlistenRaceWorkaround();
assembleDesktopPlatform();
maybeCheckOnStartup();
startSharedApp();
```

### 文件去留

- 挪到 `apps/desktop/src/platform/desktop/tauriEventCompat.ts`：`packages/frontend/src/lib/tauriEventCompat.ts`。共享包不再提到 Tauri unlisten 竞态。
- 留在 `apps/desktop`：`updater.ts`、`TitleBar.tsx`、`WindowControls.tsx`、`assembly.ts`。TitleBar 改为从 `@datazen/frontend/chrome` 取状态，不再从 `@datazen/frontend/src/stores/uiStore` 取。
- 留在 `packages/frontend`：`App`、stores、hooks、locales、components、`extensions/generated*.ts`（驱动 UI 注册表是共享的，由 `startSharedApp` 调用，桌面入口不再直接 import）。
- `registerWindowChrome` 仍然通过 `@datazen/extension-points`。不要把 TitleBar 搬进共享包去「消灭」最后一个桌面文件。

### 实施步骤

1. 在 `packages/frontend/src/bootstrap/startSharedApp.tsx` 放入从 `main.tsx` 移过来的共享启动代码，先让桌面改调它，行为不变（splash、locale、pro 扩展、store 预热逐项对照）。
2. 移动 `tauriEventCompat`，改调用点与测试。
3. 增加 `src/chrome.ts`，只转出壳层用到的符号。TitleBar / WindowControls 改 import。
4. 改 `package.json` 的 `exports`：`.`、`./chrome`、`./styles.css`。删掉 `./src/*`。
5. 全仓库搜索 `@datazen/frontend/src/`。生产代码和测试都改为上述三个说明符。`packages/frontend` 内部不准使用这个前缀。
6. 加 `scripts/check-frontend-public-entry.mjs`：`apps/`、`packages/`（排除 `packages/frontend`）、`e2e/` 中出现 `@datazen/frontend/src/` 即失败。接到现有的边界检查脚本旁，不新造 CI。

完成标准：`apps/desktop/src/main.tsx` 不再 import 任何 store 或 `App` 的深路径；`rg @datazen/frontend/src apps packages e2e` 无匹配。

---

## 7. 分 PR 与验收

每个 PR 都要能单独编译、有调用方、不提交空 `api` 模块。

| 顺序 | 内容 | 验收 |
|---|---|---|
| F0 | 本文 + 桌面 feature 显式全集（可与本文同 PR，也可紧随其后） | feature 启用集合与改前一致；文档路径与仓库一致 |
| F1 | 比对租约改为 flock | `data_sync` 不再依赖 `rusqlite`；恢复测试覆盖「锁住不回收 / 放锁后回收」；比较正文格式不变 |
| F2 | 前端稳定入口与深导入门禁 | 第 6 节完成标准 |
| F3 | 新建 `driver-support`，搬第一批纯函数 | `PROTOCOL_VERSION` 不变；`driver-api` 不再包含已搬走的函数体；前端无新依赖 |
| F4a | `connection::api` + 桌面胶水变薄 | IPC 载荷与错误码不变；宿主测试只覆盖映射 |
| F4b… | 按第 3 节顺序逐个用例下沉 | 每刀删除宿主中的对应实现 |

Web 宿主仍然不在这组 PR 里创建。`api` 模块不引用 Axum。隧道实现不迁出 `modules`。
