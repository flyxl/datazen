# 宿主边界后续：实施方案

> **状态：已确认的实施方案，代码尚未按本文改动。**
> 确认日期：2026-10-29。基线：`main` 上 form-split 已落地（`apps/desktop`、`packages/frontend`、`modules` crate `datazen`）。
> 2026-10-30 更新（`review/code-organization-2026-10` 分支）：F0、F1、F2 已在该分支
> 落地（F0 经 cargo tree 等价验证 26=26；F1 经 45/45 comparison_store + 244/244
> data_sync 测试验证；F2 含三处实施偏差，见 §6 末尾的实施记录）；F3 第一刀
> 已落地（`sql_split` + `sql_identifiers` 下沉 `driver-support`，见 §6 末尾实施记录；后续候选
> filters / keyset / dialect 已复核完毕，判决不搬，F3 收口，见 §6 复核结论）；
> F4a 第一刀已落地（`modules/src/connection/api.rs` 固定 api 模块写法，见 §6 末尾
> 实施记录）；F4b 第二刀已落地（`modules/src/table_browsing/api.rs` 收表读取
> 两用例与 `TargetCommandRunner` 端口，见 §6 末尾实施记录）；F4c sync 写路径刀
> 已落地（`data_sync::api` 收复核批次/键读/preview 装配，宿主 `statements.rs`/
> `select.rs` 删除、`apply.rs` 委托去重，见 §6 末尾实施记录）；F4c data_transfer
> 预览刀已落地（源范围闭包六处收口 + COUNT 装配/解析下沉，宿主 `preview.rs`
> 858→775 行只剩传输编排，见 §6 末尾实施记录）；F4c schema_diff 复核刀已落地
> （目录/快照/计划展开/复核消费/部署验证/单表对比六文件下沉 `schema_diff::api`，
> 宿主只剩端口与 JobRuntime 接线，见 §6 末尾实施记录）；F4c 残余复核与收口已落地
> （sync COUNT 装配下沉 `data_sync::api::count_table_rows`、result_write 确认读下沉
> `table_editing::result_edit::confirm_unchanged_read_sql`、data_transfer 复核无需新刀，
> `commands/` 生产代码零 SQL 拼装，见 §6 实施记录）。F4c 四刀加残余收口至此全部完成。
> 该分支同时合入代码组织评审（`code-organization-review.md`），两份计划的统一执行顺序见其 §4。
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
| `@datazen/frontend/shell` | 壳层专用的最小表面：`useI18n`、`usePlatform`、`useUiStore` 中 TitleBar 真正用到的选择子。不导出全部 store |
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
- 留在 `apps/desktop`：`updater.ts`、`TitleBar.tsx`、`WindowControls.tsx`、`assembly.ts`。TitleBar 改为从 `@datazen/frontend/shell` 取状态，不再从 `@datazen/frontend/src/stores/uiStore` 取。
- 留在 `packages/frontend`：`App`、stores、hooks、locales、components、`extensions/generated*.ts`（驱动 UI 注册表是共享的，由 `startSharedApp` 调用，桌面入口不再直接 import）。
- `registerWindowChrome` 仍然通过 `@datazen/extension-points`。不要把 TitleBar 搬进共享包去「消灭」最后一个桌面文件。

### 实施步骤

1. 在 `packages/frontend/src/bootstrap/startSharedApp.tsx` 放入从 `main.tsx` 移过来的共享启动代码，先让桌面改调它，行为不变（splash、locale、pro 扩展、store 预热逐项对照）。
2. 移动 `tauriEventCompat`，改调用点与测试。
3. 增加 `src/shell.ts`，只转出壳层用到的符号。TitleBar / WindowControls 改 import。
4. 改 `package.json` 的 `exports`：`.`、`./shell`、`./styles.css`。删掉 `./src/*`。
5. 全仓库搜索 `@datazen/frontend/src/`。生产代码和测试都改为上述三个说明符。`packages/frontend` 内部不准使用这个前缀。
6. 加 `scripts/check-frontend-public-entry.mjs`：`apps/`、`packages/`（排除 `packages/frontend`）、`e2e/` 中出现 `@datazen/frontend/src/` 即失败。接到现有的边界检查脚本旁，不新造 CI。

完成标准：`apps/desktop/src/main.tsx` 不再 import 任何 store 或 `App` 的深路径；`rg @datazen/frontend/src apps packages e2e` 无匹配。

### 实施记录（2026-10-30，`review/code-organization-2026-10` 分支）

F2 已落地，与上文三处偏差，均有买测：

1. **入口多一个 `./app`**：`startSharedApp` 没放进 `.`。`@datazen/frontend/src/lib/updater` 这类轻量消费者改走 `.` 后，如果 `.` 重导出 startSharedApp，会把整个应用图（App、stores、PRO 表）拖进桌面适配器的测试图——`updaterFlow.test` 的部分 `vi.mock`（generated.ts 只提供 variant 字段）在模块求值期撞上 `DB_REGISTRY` 展开直接炸。拆出 `./app` 后 `.` 保持轻量（contracts、能力位、updater 变体规则、windowKind、crossWindowBus），桌面适配器与它们的测试图不再背负应用图。
2. **测试镜像 `./testing/*`**：上文预期测试也只用三个说明符，实测 13 个接缝测试必须拿到内部件（断言 store 终态、vi.mock 内部接缝、注入状态）。新增 `"./testing/*": "./src/*"`，门禁双规则：`@datazen/frontend/src/` 全仓禁止、`testing/` 仅测试文件可用。vi.mock 按解析后文件拦截，镜像与包内相对导入命中同一模块，拦截链路不变。
3. **testing 导入带扩展名**：TS bundler 模式对 exports 通配目标不做扩展名推断，旧 `./src/*` 能解析靠的是 node_modules 物理路径回退（symlink 落在真实目录）；`testing/` 无物理目录，回退失效。仓库本就开了 `allowImportingTsExtensions`，测试镜像导入带上真实扩展名（`…/stores/uiStore.ts`）后 exports 成为唯一解析源——不需要复活 F6-c2 已退役的 tsconfig paths 别名表。

F3 第一刀已落地：

1. **切口选最小纯集**：全量盘点 `driver-api` 50+ 模块后，`sql_split`（683 行）与 `sql_identifiers`（97 行）是仅有的零 crate 依赖、零外部依赖模块；其余候选（filters/keyset/dialect/backup_dialect）都拖着 `Value` / `DatabaseDriver` / serde，强行下沉会把类型一起拖走，超出第一刀边界。
2. **模块级 re-export 保路径**：`driver-api` 的 `pub use datazen_driver_support::{sql_identifiers, sql_split}` 让下游 `datazen_driver_api::sql_split::…` 与 crate 内 `crate::sql_split::…` 均不变；lib.rs 顶层的 `SqlStatementScanner` / `Utf8ChunkDecoder` 聚合导出改指新源。函数体真正离开 `driver-api`，验收达成。
3. **driver-support 零依赖**：新 crate 无任何 dependencies（连 serde 都没有），驱动插件作者可单独取文本工具。单一消费者，按 P1 口径 path 依赖不上收 workspace 根表。
4. **验证**：`cargo check` 三级绿（driver-support / driver-api / modules），搬入的 14 个内嵌测试全绿，`PROTOCOL_VERSION`（=6）与 `MIN_PROTOCOL_VERSION`（=1）未动，前端零新依赖。

F3 后续候选复核结论（2026-10-30，F3 至此收口）：**filters / keyset / dialect 三候选均不搬，判决依据是方案自身的分批判据与消费者实测**——

1. **filters（371 行）**：全文件为 IPC 契约 DTO（`FilterConnector` / `FilterCondition` / `FilterOperator` 含 arity 分类、`SortCondition`），WHERE 构建器早已在 modules（`data_sync/filter.rs`、`data_transfer/filter.rs`）。DTO 正是 §5「driver-api 只留 trait、跨驱动 DTO、PROTOCOL_VERSION」的正确内容；消费者全在 modules（4 域）+ 宿主间接，零驱动消费者，搬去 driver-support 反而违反「只被 packages/drivers/* 依赖」。
2. **dialect（275 行）**：`SqlDialect` 是驱动方拥有的 SQL 文本契约 trait（§5「只留 trait」的正确内容），`LegacySqlDialect` 是该 trait 的单向迁移适配器（文档注释明示「old trait keeps the only implementation while call sites move」）——trait 配套适配器随 trait 走，消费者同样全在 modules，零驱动消费者。
3. **keyset（486 行）**：内容是方言 SQL 装配（`row_value_seek_predicate` / `lexicographic_seek_predicate` / `build_keyset_select_sql*`），被 §5 第一批判据「不含方言 SQL」直接排除；消费者为 3 个驱动（trait 默认实现回调）+ modules `table_browsing`，下沉会让 modules 直依 driver-support，违反「只被 packages/drivers/* 依赖」。
4. **结构性障碍（即便放宽上述判据仍在）**：keyset/filters 需要 `Value` / `DatabaseDriver`，driver-support 必须反向依赖 driver-api，与既有 `pub use datazen_driver_support::{sql_identifiers, sql_split}` 桥接 re-export 构成 cargo 环依赖；拆桥需改 11 处 `datazen_driver_api::sql_{split,identifiers}` 消费者并放弃第一刀的零依赖卖点（「驱动插件作者可单独取文本工具」）。收益（driver-api 减 ~1.1k 行）不抵三项判据违反 + 桥接拆除波及。
5. **残余路径**：若未来 driver-api 瘦身仍要推进，正确切口不是 driver-support，而是把「只被 modules 消费的实现」迁 modules（如 `LegacySqlDialect` 若 `SqlDialect` 迁移完成后的退役路径），或等 P3 feature 图修剪时一并评估。记入路线图，不在本组 PR 强推。

F4c schema_diff 复核刀已落地（F4c 收官刀：目录/快照/计划展开/复核消费/部署验证/单表对比整体下沉）：

1. **切口**：宿主 `commands/schema_diff/` 的算法面六文件下沉后**删除**——`dependency_catalog.rs`（356 行，目标依赖目录读取与完整性校验）、`object_scope.rs`（390 行，SQL Server 对象范围门）、`unified_plan/catalog.rs`（256 行，统一计划目录装配）、`unified_plan/revalidation.rs`（114 行，源快照复核）与两份算法测试 `tests/{catalog,selectors}.rs`；`unified_plan.rs` 632→116、`plan_commands.rs` 404→106、`compare_commands.rs` 179→71、`deploy_commands.rs` 466→344。modules 侧新建 `migration/schema_diff/api.rs`（2369 行）收全部用例编排：`prepare_table_plan` / `prepare_unified_plan`（计划展开）、`consume_and_verify_reviewed_plan` / `revalidate_source_snapshot`（复核消费与部署前双端验证）、`compare_table_schemas`（单表列级对比 + DDL 预览）、`list/fetch_schema_objects` 与目录/快照读取族、`validate_target_dependency_catalog`、SQL Server 范围门两函数。
2. **端口**：`SchemaDiffAdapterHost`（sync 适配器对解析，桌面实现挂 `state.sync_adapters` 注册表，错误文本与下沉前逐字节一致）与 `ComparisonPreviewHost`（DDL 渲染与全类型读取由 data_transfer 提供，宿主薄转发 `transfer::full_types` / `transfer::ddl`）按 F4b `TargetCommandRunner` 先例落宿主；`SchemaDiffEndpoint`（driver + handle + config 借用集）为下沉侧端点入参，由宿主 `runtime_session_context` 构造。`reviewed_repository.rs`（569 行）保留宿主——reviewed-plan 存储端口的桌面单租户实现，非算法；`job.rs`（JobRuntime 接线，P5 Wave-1 轨）与历史记账、只读/破坏性门同样留宿主。
3. **错误路线**：`SchemaDiffApiError::{Driver,Validation,Internal}` 三变体与桌面此前内联构造的 `CommandError` 一一对应（`Driver` 的 `NotSupported` → `Validation` 分叉在 api 侧完成），桌面 `From<SchemaDiffApiError> for CommandError` 是唯一一份桌面映射表，Web 宿主以后自持 HTTP 版。验证序列的错误顺序与文本逐字节不变。
4. **sync 域顺带归位**：`commands/sync/compare.rs` 的 IR 对比算法（`format_ir_type` / `ir_column_snapshot` / `diff_table_schemas_ir`，105 行）下沉 `modules::schema_diff::compare`，宿主文件只剩 COUNT 装配；宿主 `sync/tests.rs` 里断言这些函数的 6 个算法测试（4 IR + 2 raw diff）同批迁 modules `compare.rs` 内嵌测试，`sync/tests.rs` 1105→896 只剩传输/集成测试。
5. **测试**：算法测试 7 个（目录读取、选择器解析、范围门，原 `tests/{catalog,selectors}.rs`）逐字迁 `api_tests.rs`（MockDriver 取 driver-api 公开测试替身，桌面 `crate::testing::mock_driver` 即其再导出）；IR 对比 6 个迁 `compare.rs`。modules `--features schema_diff` 1133/1133 绿（原 1120 + 迁移 13）；宿主保留的 `tests/plan_deploy.rs` 断言经宿主装配的映射与集成，零改动。
6. **验收达成**：宿主 schema_diff 域（含 `schema_diff.rs` 与测试）6,108→3,612 行，生产代码不再含 SQL 拼装、目录读取、计划展开与快照复核（残留 SQL 字符串仅在测试断言/fixture）；连同 sync 域顺带归位，宿主侧净删 2,705 行。
7. **验证**：modules check 绿 + `--features schema_diff` 测试 1133/1133；rustfmt 全部改过文件通过；`check-module-layers` / `check-ddd-layers` 门禁绿；`pnpm typecheck` 绿（tsrs 三配置）；前端单元套件本刀为纯 Rust 改动，沙箱内抽样运行（含全部 scripts/__tests__ 门禁测试）0 失败，全量套件与桌面 crate 同口径交 CI 验证（沙箱 ~10 分钟回收长进程，既有限制）。

F4c 残余复核与收口（sync / data_transfer / result_write 三处，2026-10-30）：

1. **sync 域复核——剩余体积为宿主正确内容，一处残余下沉**：算法面已全部离开宿主（写路径 SQL 在 `data_sync::api`，IR 对比在 `schema_diff::compare`，keyset 装配在 modules 构建器、`keyset_source.rs` 是活连接分页端口，分类在 `classify_tables`、`inspect.rs` 只做装饰）；剩余 ~6.9k 行为 LiveExecutor 端口、租约/JobRuntime 接线、合同测试（`jobs_contract` / `jobs_cancel_contract` / `filter_validation_tests`）与 E2E 故障注入——§3 模式下宿主正确内容，无需新刀。唯一残余 COUNT 装配（`compare.rs` 内联 `SELECT COUNT(*)`，消费者 sync `tasks.rs` + data_transfer `inspect.rs` 两处）下沉 `data_sync::api::count_table_rows`：限定名装配（mysql 反引号分叉）+ 宽容解析（非负整数/有限浮点/数字字符串、读不出回退 0）原语义迁移，驱动错误原样透传（宿主 `cmd_err` 映射字节不变），宿主 `compare.rs` 43→24 行只剩薄委托，解析测试随迁 modules（`count_tests`）。
2. **data_transfer 域复核——无需新刀**：生产文件（preview / exec / execution / inspect / job_api / mod）经扫描零 SQL 拼装（预览刀收口后），分类已在 `data_transfer::inspect_tables`；剩余 ~5.7k 行为 Job 生命周期（受理/取消/恢复）、claimed-plan 执行与检查点终结编排、适配器接线与合同测试——宿主正确内容。
3. **result_write 确认读（复核新发现的残余，DB-14 域）**：MySQL 改动行语义的确认 SELECT 内联装配（`format!("SELECT … WHERE …")` + 谓词循环，~25 行）下沉 `table_editing::result_edit::confirm_unchanged_read_sql`——该模块本就拥有结果网格写回规则（「host owns no rules」是其文档注释的明文承诺），UPDATE 装配此前已走 `build_update_statement` trait 契约，本刀补齐读回半边。宿主保留 `qualify_sql_target` 限定与运行时执行/裁决解析；占位符错误经既有 `From<TableEditingError>` 的 `Driver → Driver` 臂映射，字节不变。装配面新增 1 测（MockDriver 钉死标识符引用、占位符编号、多键谓词 AND 序、None 身份值落 NULL 参数）。
4. **验收达成（§3 全树口径）**：`commands/` 生产代码零 SQL 拼装——最终扫描仅测试 fixture/断言残留（`result_source_tests` / `exec.rs` 测试段等）。modules `--features schema_diff,data_sync,table-editing` 并跑 1448/1448 绿；rustfmt 全部改过文件通过。

F4c data_transfer 预览刀已落地（源范围闭包四路收口 + COUNT 装配/解析下沉）：

1. **切口**：`data_transfer/preview.rs`（宿主 858 行）的算法面是同一套源范围装配闭包的四份宿主拷贝（占位符经 `LegacySqlDialect` 转发、列类型从 schema 现查）——SQL 文件路径校验环、SQL 文件 write-plans 环、库目标校验环、库目标 typed 占位符渲染环；同一闭包集在 modules 侧还有两份（`execute/dispatcher.rs`、`job/handler.rs`）。收进 `recordset::build_source_scope_for_driver` 后六处共用一份；预览行数估计的 COUNT 装配（限定名 + WHERE 尾巴 + 参数顺序）与单格解析/限幅一并下沉（`count_scoped_rows_sql` / `parse_scoped_row_count`），执行留在宿主（租约与驱动错误映射在彼）。宿主 `preview.rs` 858→775 行，剩余为会话上下文、适配器注册表、schema 装载调用与计划签发——与 F4b 保留的宿主传输机器同类，非算法。
2. **family 来源分叉（保住原来源）**：预览命令 / SQL 文件路径 / 执行 dispatcher 的 family 取驱动自报 `driver_type()`（`for_driver`）；Job handler 的 family 取端点配置类型（适配器注册表用它做键，桌面上游两者一致但来源不同）——`for_family` 显式收参，`for_driver` 是它的驱动自报特化。tuple 范围门（`tuple_ordering_supported`）与拒绝文本都消费该字符串，换成统一来源会在分叉时改变判定；一个测试用「驱动自报 fixture 拒绝、显式 postgresql 放行」钉死分叉。
3. **错误路线**：闭包内 `TransferError::unsupported(error.to_string())` 原样保留——`DriverError::Unsupported` 的 Display 带 `Unsupported: ` 前缀，转发的是整串（测试钉死该前缀存活）；调用点各自的错误装饰（裸 Validation / `From<TransferError>` / `source driver cannot execute…` / `cannot render typed source scope preview: …` 前缀）留在宿主，`TransferError::Validation` 的 Display 即裸文本，IPC 字节不变。
4. **测试**：新增 6 个——闭包版与驱动版同一断言面（等价性钉死）、过滤列类型经 schema 现查转发进占位符、family 来源分叉、占位符拒绝映射、COUNT 限定名三族（PG 无/有 schema、MySQL database）与 WHERE 尾巴、COUNT 解析四种拒绝文本 + 整数/字符串载体 + 限幅取小。data_transfer 闭包 1212/1212 绿（原 1206 + 新 6）；modules 27 feature 全集 2573/2573 绿。
5. **验证**：`data_transfer` feature check 绿、27 feature 全集 check 绿（对齐宿主腿）；rustfmt 全部改过文件通过；`check-module-layers` / `check-ddd-layers` 门禁绿；桌面 crate 沙箱缺 GTK/WebKit 无法本地编译（既有限制，CI 验证），宿主文件经 rustfmt 解析 + 人工类型复查。

F4c sync 写路径刀已落地（覆盖两个已探明切口 + 宿主重复装配消除）：

1. **切口**：`sync/host/statements.rs`（复核批次 SQL 生成，180 行）与 `sync/host/select.rs`（§5.3 键读 SELECT，184 行）整体下沉后**删除**；`apply.rs` 的 `generate_data_sync_sql_impl`（ui-preview 命令）循环体与 statements.rs 是同一套装配序列的两份宿主拷贝（sqlserver 预检 → 投影复验 → preview 字面量 → qualify → identity 盖章），一并改为委托 modules 同一份装配——`exec.rs` 的 plan-preview 路径经同一函数自动受益。宿主 `apply.rs` 1177→682 行，`host/mod.rs` 增端口实现。
2. **端口与上下文**：`BatchSqlHost`（单方法 `write_sql_context(target_table)`）按 F4b `TargetCommandRunner` 先例落宿主实现——`relation_meta` → `target_slot` → source/target 适配器，查找顺序与失败文本不变。装配入参 `WriteSqlContext`（借用）收 family/库名/schema/表 schema/双适配器/方言/identity 开关位；复核批次用例 `generate_reviewed_batch_sql` 收端口，preview 用例直呼共享核心 `generate_write_sql_for_table`。
3. **门禁先于端口（顺序契约）**：options 校验、关系匹配、未复核行拒绝、空批次短路都在 api 内**先于**任何端口调用执行——下沉前这些校验先于宿主 `relation_meta`/`target_slot` 查询，下沉后以 `UntouchedHost`（端口即 panic）3 个测试钉死该顺序。租约生命周期不变：`target_slot()` 返回 `Slot` 克隆，`HostDataSync.slots` 仍持有 `Arc<LiveConnectionLease>`，局部克隆提前 drop 不释放租约。
4. **错误路线**：全部走 `DataSyncError::validation`，经既有 `From<DataSyncError> for CommandError` 映射表转 IPC——`Validation` 的 Display 即裸文本，错误字节不变；预检/身份校验/投影漂移的错误文本逐字迁移。
5. **一处顺序重排（可达输入上等价）**：ui-preview 循环里 sqlserver 预检原在投影查找之前，委托后统一在 modules 装配内（投影查找之后）执行。`ChangeSet::from_comparison` 是 `comparison.tables` 的严格过滤器，投影查找在可达输入上不会 miss，两序行为一致。
6. **测试迁移与新增**：宿主 7 个内联测试（预检×2、身份×2、投影漂移、options 覆盖）与键读 3 个测试原样迁 modules；新增门禁序 3 测（`UntouchedHost`）与装配 happy path 2 测（哑适配器 + mock 驱动）。data_sync 闭包 258/258 绿（原 244 + 新 14），modules 全量 1210/1210 绿。
7. **验证**：`data_sync` feature check 绿；桌面宿主 feature 联合（27 feature 全集对齐宿主腿）check 绿；裸 `connection` / `services` / `connection_import` 单 feature 失败为**存量问题**（stash 后基线复现，与评审 F-5 同类，归 P3 feature 图修剪）；桌面 crate 沙箱缺 GTK/WebKit 无法本地编译（既有限制，CI 验证），宿主文件经 rustfmt 解析 + 人工类型复查。

F4b 第二刀已落地：

1. **切口为表读取两用例**（方案切片顺序第 2 步「只读查询与 schema / 表浏览」的主体）：`read_table_page`（分页读，含计数与行同批、估计失败降级重读、不可用估计回退精确 `COUNT(*)`）与 `preview_table_read`（DB-15 预览）；两者共享的 `assemble_statements`（DB-15 单点拼装）、`fetch_table_schema`、`statement_row_sets`（流式/一次性双形态结果合并）一并下沉。`query.rs` 的 execute_query 本就只是 driver_command 传输信封、`schema.rs` 的 estimate_table_rows 算法已在 modules 的 QueryExecutor——都不携带算法，本刀不动。
2. **端口走消费者定义（R1）**：`TargetCommandRunner`（目标/会话上执行一条命令并收集输出）不命名 runtime 门面；桌面实现 `DesktopTargetRunner` 持 `&PlatformAdapter + &RuntimeConnectionUseCases`，保留目标解析、提交令牌、终态轮询与产物分块收集——这些是传输机器不是算法，验收标准（宿主不再出现 SQL 拼装/行合并/计划展开）不涉及其内容。写链路 D4-d 的 `commit_changes` 已立 `Arc<dyn ConnectionUseCases>` 先例，本刀同源。
3. **错误走 `TableReadError` 而非 feature error 折叠**：用例层校验/构建失败 `Validation`/`Internal` 与桌面原内联 `CommandError` 构造一一对应；端口失败走 `Runtime(ApiError)` **原样透传**。此处不能复用 `From<ApiError> for FeatureError` 的折叠桶：`ResourceBusy` 在桌面映射表里是独立可处置 IPC 类别，折叠成 `Internal` 会改错误外观。桌面 `From<TableReadError>` 是唯一一份桌面映射表。
4. **宿主保留三处输入校验的重复**（上下文、页大小、过滤器，与写链路 `commit_changes` 同款）：宿主副本保持本传输的错误顺序稳定（profile 查找前拦下坏请求），api 侧重检因「第二传输直呼时也必须成立」。两个副本都不是死代码：宿主副本先择错，api 副本对宿主路径永远通过。
5. **验收达成**：宿主 `table_target.rs` 874→270 行，生产代码不再含 SQL 拼装、行合并、计数计划展开；DTO（`TableReadRequest` 等）随用例进 modules，宿主 re-export 保住 `table_target::` 路径，`data.rs` 命令与 `target_read_tests.rs`（20+ 用例）零改动。
6. **验证**：table-browsing 闭包 `cargo test --lib` 1125/1125 绿（含 api 新增 12 测：拼装三计划、双形态合并、端口失败透传、无宿主类型闸门）；全 feature 无选择器 `cargo check`（对齐宿主腿 27 feature）绿；桌面 crate 沙箱缺 GTK/WebKit 无法本地编译（既有限制，CI 验证），宿主文件经 rustfmt 解析 + 人工类型复查。
7. **feature 表随刀显式化**：table-browsing 增列 async-trait/tracing/driver-api/execution/mcp/table-editing——请求 DTO 本就嵌 `RowChangeTableContext`（table-editing 词汇）、读调用本就依赖 mcp 的目标注入策略，此前靠 schema→execution→connection 传递闭包掩盖；全部是既有激活集的子集，无解析变化。

F4a 第一刀已落地：

1. **错误路线选 feature error 而非新增 ApiErrorCode**：`as_str` 的协议字面量与连接规格 §13 的 code 列逐字绑定，新增 `Internal` 变体等于发明协议取值，违反仓库的规格纪律（「不发明取值」）。因此 `api` 函数返回 `ConnectionError`（方案明文允许「ApiError / feature error」二选一），业务错误文本在 application 层早已定稿；桌面 `conn_cmd` 就是那唯一一份桌面映射表，Web 宿主以后自持 HTTP 版。IPC 错误码与消息**逐字节不变**（同类型流过同一映射）。
2. **切口为 CRUD + 测试配置解析**：`list/get/save/duplicate/delete/reorder` 六个用例 + `resolve_test_config`（原 test_connection 的密钥合并逻辑下沉）；会话编排（connect 的 open_dedicated_session）、隧道拨号、注册表查询留宿主——它们编排宿主服务，本就不是 DTO 映射。
3. **宿主命令变薄**：8 处调用点改为 `connection_api::*` 委托；「保存后退休旧会话」钩子（PR #116 语义）留在宿主命令层，与方案「桌面专有能力留在宿主」一致。
4. **测试位置**：api 契约测试（脱敏投影、None 语义、密钥合并、复制去时间戳）在 `modules` 内嵌；宿主测试（原 17 个）不动，断言的仍是映射与集成。
5. **验证**：`cargo check --features connection,execution` 绿；modules 1487/1487 全绿；桌面 crate 因沙箱缺 GTK/WebKit 无法本地编译（既有限制，CI 验证），改动经 rustfmt 语法解析 + 人工类型复查（Arc→& 经显式 `&service` 解引用传递）。
6. **顺带发现（既有问题，非本刀引入）**：裸 `--features connection` 编译不过——`connection/dto.rs` 无条件重导出 `crate::execution::*`，而 `execution` 不在 `connection` 的 feature 列表里。与评审 F-5（裸 connection_import 编译失败）同类，归 P3 feature 图修剪。

---

## 7. 分 PR 与验收

每个 PR 都要能单独编译、有调用方、不提交空 `api` 模块。

| 顺序 | 内容 | 验收 |
|---|---|---|
| F0 | 本文 + 桌面 feature 显式全集（可与本文同 PR，也可紧随其后） | feature 启用集合与改前一致；文档路径与仓库一致 |
| F1 | 比对租约改为 flock | `data_sync` 不再依赖 `rusqlite`；恢复测试覆盖「锁住不回收 / 放锁后回收」；比较正文格式不变 |
| F2 | 前端稳定入口与深导入门禁 | 第 6 节完成标准 |
| F3 | 新建 `driver-support`，搬第一批纯函数（第一刀已落地：`sql_split` + `sql_identifiers`；后续候选 filters / keyset / dialect 已复核：三候选均不满足方案分批判据，判决不搬，见 §6 复核结论，F3 收口） | `PROTOCOL_VERSION` 不变；`driver-api` 不再包含已搬走的函数体；前端无新依赖（已达成） |
| F4a | `connection::api` + 桌面胶水变薄（第一刀已落地：CRUD + `resolve_test_config`；会话/隧道编排按方案留宿主） | IPC 载荷与错误码不变；宿主测试只覆盖映射 |
| F4b | 表读取两用例（第二刀已落地：`table_browsing::api` 的 `read_table_page`/`preview_table_read` + `TargetCommandRunner` 端口；query/schema 薄命令不动） | IPC 载荷与错误码不变；宿主不再含 SQL 拼装/行合并/计划展开 |
| ~~export、backup~~ | **复核结论（2026-10-30）：无需 F4 刀**——D6 item 5 已把它们落到目标形态（算法在 `modules::export`/`modules::backup`，宿主 `export.rs`/`backup.rs` 只剩端口实现 + 对话框/事件绑定），本方案写就时未计入 | 宿主不再含算法（已达成） |
| F4c… | sync / data_transfer / schema_diff，按第 3 节顺序逐个用例下沉（**sync 刀已落地**：`host/statements.rs`、`host/select.rs` 下沉 `data_sync::api`，`apply.rs` 循环体委托同一装配去重，`BatchSqlHost` 端口单方法落宿主；**data_transfer 预览刀已落地**：源范围闭包四宿主拷贝 + 两 modules 拷贝收进 `recordset::build_source_scope_for_driver/_for_family`（Job handler 的 family 来源=端点配置类型，保住分叉），COUNT 装配/解析下沉 `count_scoped_rows_sql`/`parse_scoped_row_count`，执行留宿主租约侧，宿主 `preview.rs` 858→775 只剩传输编排；**schema_diff 复核刀已落地**：目录/快照/计划展开/复核消费/部署验证/单表对比六文件下沉 `schema_diff::api`，`SchemaDiffAdapterHost`/`ComparisonPreviewHost` 端口落宿主，sync 域 IR 对比算法顺带归位 modules，宿主 schema_diff 域 6,108→3,612 行；**残余收口已落地**：sync COUNT 装配下沉 `count_table_rows`、result_write 确认读下沉 `confirm_unchanged_read_sql`，`commands/` 生产代码零 SQL 拼装；modules 侧 `migration::{data_sync,data_transfer,schema_diff}` 已有 4 万行对应域，下沉是接合不是新建） | 每刀删除宿主中的对应实现（sync 刀已达成：宿主不再含写路径 SQL 拼装与键读 SELECT；data_transfer 预览刀已达成：宿主 `preview.rs` 不再含源范围 SQL 装配与 COUNT 装配/解析；schema_diff 复核刀已达成：宿主生产代码不再含目录读取、计划展开、快照复核与 IR 对比算法；残余收口已达成：`commands/` 生产代码零 SQL 拼装，全树口径验收达成） |

Web 宿主仍然不在这组 PR 里创建。`api` 模块不引用 Axum。隧道实现不迁出 `modules`。

---

## 8. 跨形态一致性收敛（M 系列）

> 立项依据（用户指令，2026-10-31）：**AI 的 DB 工具是 web 与 desktop 共同需要的能力——实现方式可以不同，基础能力必须一致；MCP 同理，差异只允许是 Server 形态（stdio 子进程 / 内嵌 duplex / streamable HTTP + 服务端身份），其余完全一致；runtime 读取路径不允许「同一能力、两条适配」。**
> 基线核对：`528f10e`（2026-10-31）。本节回答「runtime 适配层为什么存在 web/desktop 不一致」，并给出收敛切片。

### 8.1 现状核对（已核实事实）

**AI 侧——已符合原则，一处缺口。**

| 层 | 位置 | 形态中立性 |
|---|---|---|
| 工具定义（5 个只读工具） | `modules/src/ai/application/tools.rs::db_tool_definitions()` | ✅ 共享 |
| 工具执行 | `modules/src/ai/application/db_tools.rs`，经 `DatabaseToolHost` 端口（`ai/application/ports.rs`） | ✅ 端口化 |
| 工具循环 + 守卫 | `modules/src/ai/application/tool_loop.rs` + `AiToolLoopHost` 端口 | ✅ 共享 |
| schema 上下文 | `modules/src/ai/application/schema_context.rs::RuntimeSchemaContextSource`，只吃 `SessionConnectionLookup` + `ResourceExecutionHost` 两个端口 | ✅ 端口化 |
| 桌面装配 | `AppState` 实现 `DatabaseToolHost`（`services/feature_ports.rs`）、`AiRequestHost` 按 claim 分流（`commands/ai/grant.rs`）、`services/schema_context_source.rs` 注入两端口 | ✅ 属于「形态实现」 |
| **缺口：MCP 工具面** | `commands/ai/chat.rs` 的 `mcp_tool_definitions` / `execute_mcp_tool` 直接绑桌面 `state.mcp_client_manager`（stdio 子进程管理器） | ❌ 无端口，web 落地时不可实现 → M3 |

**MCP 侧——不符合原则。** DDD 目标架构（`ddd-refactor/01-target-architecture.md` §3.3）明文「`mcp/` 工具业务分发；传输和登录在宿主」，现状：

| 层 | 位置 | 行数 |
|---|---|---|
| 已共享 | `modules/src/mcp/`：`domain/permission.rs`（三档权限）、`domain/allowlist.rs`、`application/command_policy.rs`（命令准备，主消费者其实是 driver_command 适配器）、`application/describe.rs` | 751 |
| 宿主私有 | `src-tauri/src/mcp/`：13 个工具实现（`MCP_ALL_TOOLS`：10 无状态 + 3 有状态；tools.rs，直连 AppState / `RuntimeCommandSource` / `WorkflowExecutor`）、有状态会话协议（`session*.rs`：open/use/close、TTL、cleanup worker 与 RollbackAndClose 退避）、`tool_help.rs`、`resources.rs`、`prompts.rs`、rmcp handler/server、`client.rs`（MCP Client）、stdio token 认证与 golden 契约 | ~4,200 |

后果：web 宿主落地时要重写 13 个工具 + 有状态会话语义 + Resources/Prompts——违反「差异应只有 Server 形态不同」。

**runtime 读取路径——不一致的实体。**「在持久化连接 profile 上执行只读元数据命令」这件事存在两条路径：

1. **AI 的路径（共享）**：`RuntimeSchemaContextSource`（modules）→ `ResourceExecutionHost` 端口 → 桌面注入 `RuntimeCommandSource`。
2. **MCP 的路径（桌面私有）**：`src-tauri/src/mcp/runtime_schema.rs::read_target_command` → 直呼桌面结构体 `RuntimeCommandSource::read_target_command`（构造参数 `DesktopRepository` + `PlatformEntry`，均为桌面类型）。

`RuntimeCommandSource` 起源是 Wapp 只读宿主（错误文案 `"Wapps may invoke read-only driver commands"`、step id `"wapp-command"` 仍是原文），后为 MCP 复用，又被 `schema_context_source.rs` 注入为 AI 源的 `ResourceExecutionHost` 实现——即它在系统里的合法角色是**桌面的端口实现**，但它同时被 MCP 当**直连捷径**使用。

### 8.2 为什么不一致（根因，非设计分歧）

1. **时序**：桌面先行。MCP Server 出生在 Tauri 宿主内（rmcp stdio 传输 + AppState 接线是其出生需求），当时 `modules` crate 尚不存在（D 系列整合前布局为 `src-tauri` + `packages/*`）。
2. **抽取按消费者逐个推进**：AI 在 D6 item 4（2026-10-26）完成端口化归位；MCP 的抽取停在纯策略层（permission/allowlist/describe/command_policy）——工具本体有状态、碰 Runtime，抽取成本高一档，尚未排到。
3. **没有测试钉位置**：golden 契约（`contract.rs` / `mcp_external_contract.json`）只钉外部 schema 形状，不钉实现位置；F-14 分层门禁只保证 modules 不反向依赖宿主，检测不到「本该在 modules 的业务留在宿主」。

结论：这是**未完成的迁移**，不是被允许的形态差异。§4.7/§4.8 定义的允许差异是「同一端口、另一适配器」（`ProfileRepository`、`SessionDirectory`、`EventSink`、`Clock` 等）；`RuntimeCommandSource` 对 MCP 而言不是端口实现，是绕开端口的桌面直连。

### 8.3 收敛切片（每刀独立 PR）

| 刀 | 内容 | 验收 |
|---|---|---|
| **M4（已落地，2026-10-31）** | 元数据读取统一：`read_target_command`/`driver_for_connection` 用例化进 `modules/src/workflow/application/runtime_reads.rs`（吃 `ResourceExecutionHost` 端口；`ReadOnlyForcedHost` 只读强制随之归共享语义），姊妹用例 `connection_schema_context`（token 预算的表结构装配）落 `modules/src/ai/application/schema_context.rs`；宿主 `RuntimeCommandSource` 删去 inherent 捷径方法，回归端口实现 + 导出装配；宿主 `mcp/runtime_schema.rs` 删除，`DataZenMcpServer` 新增 `resource_host: Arc<dyn ResourceExecutionHost>` 字段由 `services::runtime_command_source::resource_host()` 装配；Wapp `driver.rs` 同步换轨。**归属与方案原文的差异**：原计划落 `modules/src/mcp/application`，实施发现 feature 环（`workflow` 已依赖 `mcp`、`ai` 已依赖 `workflow`，`mcp` 不能反向依赖两者），故命令用例落 workflow、上下文装配落 ai——依赖方向无环，调用面不变 | 宿主 `mcp/` 不再引用 `RuntimeCommandSource`（仅 server.rs 装配行经 `services::resource_host()` 间接构造，类型不再出现在 `mcp/` 内）；MCP 元数据工具错误文本不变（profile/driver/禁用/未知命令四类文案逐字保留；唯一改写的访问级拒绝文案原为 Wapp 专属不可达路径）；step id `"wapp-command"` → `"read-only-command"`（仅日志/审计，不入 MCP 输出）；modules 侧新增 6 个行为钉子测试 |
| **M3（已落地，2026-10-31）** | AI 的 MCP 工具面端口化：`McpToolBroker` 消费者端口（`tools_for_ai` / `call_tool`，返回预格式化文本——rmcp 结果格式化是传输形状留实现侧）+ `McpBrokeredTool` 中立结构落 `modules/src/ai/application/ports.rs`；装配用例（qualified 命名规则、`ToolDefinition` 映射、tool loop 的 `String` 执行契约与错误文案）落 `modules/src/ai/application/mcp_tools.rs`；桌面实现 = `AppState`（`services/feature_ports.rs`：`enabled_for_ai` 过滤读设置存储、stdio 子进程管理器、spawn 白名单与 token 校验留桌面）；`commands/ai/chat.rs` 两函数变薄壳，桌面 `mcp_qualified_name` 变共享规则转发（单一规则源，client 测试零改动） | AI 工具循环测试不感知形态（`AiToolLoopHost` 的 `String` 契约未动）；桌面 MCP client 既有测试零改动；错误文案 "MCP tool error ({qualified}): …" 逐字保留；共享层新增 5 个用例测试 |
| **M1（最大刀）** | MCP 工具分发归位：新建 `modules/src/mcp/application/tools/`（工具目录 name/description/input schema + 分发 + 无状态工具用例），实现改吃共享端口（`ConnectionRepository`、`ResourceExecutionHost`、workflow application 契约）；`tool_help.rs` 随迁；宿主 `tools.rs` 变 rmcp 薄绑定——`#[tool]` 签名保留，函数体一行委托 | golden 契约 `mcp_external_contract.json` 逐字节不变；宿主不再含工具业务；`--features mcp` 测试绿 |
| **M2** | 有状态会话语义归位：TTL、归属校验、cleanup 策略（100ms–30s 退避、`resourceRelease=Confirmed` 才删记录）进 `modules/src/mcp/application/session/`；宿主保留定时 worker 与 rmcp 传输生命周期驱动 | 桌面行为不变（`session_cleanup` 既有测试全绿，语义测试迁 modules） |

顺序理由：M4 消除双路径并给 M1 铺路；M3 独立且小；M1 是主体；M2 独立于 M1 可后置。

### 8.4 文档修正（随本节一并完成）

- `mcp.md` §1.1：删去不成立的「共享的工具/资源/提示词逻辑位于 `modules/src/mcp/`」，改记真实分层并指向本节；`packages/kernel/src/sql_guard/` 路径修正为 `modules/src/execution/domain/sql_guard/`。
- `ai.md` §1.11：`query_db` 不存在于工具面（实际为 5 个只读工具），已修正；MCP 工具面缺口标注 M3。

