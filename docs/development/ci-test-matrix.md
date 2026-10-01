# CI 与测试矩阵

> 与 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)、[release.yml](../../.github/workflows/release.yml) 及 [AGENTS.md](../../AGENTS.md) 测试约定配套。  
> 目标：PR 流水线快且稳定；驱动 SKU 组合可扩展；**`all` 预设不进 PR CI**。

## 1. 总览

| 层级 | PR CI（`ci.yml`） | Release（`release.yml`） | 本地 / 维护者 |
|------|-------------------|--------------------------|---------------|
| 驱动选型 | **`basic` 固定**（postgres, mysql, sqlite, redis） | Basic / All × 四平台 + Akulaku × 三平台（Windows / macOS，无 Linux） | 任意 `--drivers=` / `DATAZEN_DRIVERS` |
| Host 前端单测 | ✅ `pnpm test:unit` | 构建前 `pnpm build`（含 typecheck） | `pnpm test:unit` |
| 驱动 UI 单测 | ✅ `pnpm test:unit:drivers` | 构建前 `pnpm build`（含 typecheck） | `pnpm test:unit:drivers` |
| TypeScript | ✅ `pnpm typecheck` | 同上 | `pnpm typecheck` |
| Host Rust lib | ✅ `cargo test -p datazen --lib`（basic features） | 完整 release 构建 | `cargo test -p datazen --lib` |
| driver-api | ✅ | 随构建链接 | `cargo test -p datazen-driver-api --lib` |
| ai-api | ✅ | 随构建链接 | `cargo test -p datazen-ai-api --lib` |
| Basic path 驱动 lib | ✅ 四 crate 并行 | Basic SKU 内嵌 | `cargo test -p datazen-driver-<id> --lib` |
| 可选 path 驱动 lib | ❌ | **All SKU** 构建时编译链接 | 改驱动 crate 时本地必跑 |
| Git 驱动（kiwi/superset） | ❌ | **Akulaku SKU**（需 Deploy Key） | 见 [ci-private-drivers.md](./ci-private-drivers.md) |
| Host E2E | ❌ | ❌（发版后手工 / R 阶段） | `pnpm e2e` / `pnpm e2e:minimal` |
| Host 契约矩阵 E2E | ❌ | ❌ | `pnpm e2e:contract:matrix` |
| 驱动专属 E2E | ❌ | ❌ | `packages/drivers/<id>/e2e/` |

**原则**

1. **Basic 必测**：每个 PR 与 `main` push 均跑 basic 四驱动 + Host 三件套（TS 单测 / Host lib / driver-api / ai-api）。
2. **All 不进 PR CI**：`resolve-drivers --drivers=all` 仅用于 Release **All** SKU 与本地全量验证，避免 PR 流水线编译全部 path 驱动。
3. **Path 轮转（维护者策略）**：可选 path 驱动（mongodb、clickhouse、duckdb、sqlserver、elasticsearch 等）的 **Rust lib 不在 PR CI 矩阵内**；其 **UI 单测全部进 PR CI**（`pnpm test:unit:drivers` 按文件系统收集 `packages/drivers/*/ui/__tests__/`，与 `--drivers=` 选型无关，因此可选驱动同样被覆盖）。改 Rust 时作者仍须在 PR 说明中列出 `cargo test -p datazen-driver-<id> --lib`（及该 crate 内 E2E）。发版 **All** SKU 是对全部 path 驱动的集成校验。
4. **契约矩阵**：Host Connection Contract（`e2e/contract/`）验证 PG/MySQL/SQLite 上同一套 Host UI journey；**不进 PR CI**，由维护者在合并前或 R 阶段跑 `pnpm e2e:contract:matrix`；fixtures 单测 `pnpm test:unit:e2e-contract` 可在本地或后续 CI 扩展中启用。

## 2. PR CI 步骤（与 workflow 对齐）

触发：`pull_request` → `main`、`push` → `main`、`workflow_dispatch`。

环境：`ubuntu-latest`；`DATAZEN_KEYRING=file`（无 OS 钥匙串）。

结构：`frontend` 与 `rust` 两个**并行 job**（互不依赖，墙钟时间取较大者），外加聚合 job `ci`（`needs: [frontend, rust]`）作为分支保护的 required status check。

### 2.1 frontend job（前端段）

| 步骤 | 命令 / 动作 | 说明 |
|------|-------------|------|
| 依赖 | `pnpm install --frozen-lockfile` | Node **24**、pnpm **11**（与 workflow 一致） |
| 代码生成 | `node scripts/generate-builtin-locales.mjs` | `builtinLocales.ts` 为 gitignore codegen |
| 类型 | `pnpm typecheck` | `tsc --noEmit` |
| 守卫 | `check-managed-stubs.mjs`、`check-structure-editor-guardrails.mjs` 等 | 防止误提交 inject 产物 |
| Host 单测 | `pnpm test:unit` | Vitest；`pretest:unit` 会 `--codegen-only --drivers=basic` |
| 驱动 UI 单测 | `pnpm test:unit:drivers` | `packages/drivers/*/ui/__tests__/`；与 Host 单测互不收集，两步都必须跑 |
| Site（条件） | `check-site-seo.mjs` | 仅当 diff 含 `site/`（`fetch-depth: 0` 仅此 job 需要） |

### 2.2 rust job（Rust 段，浅克隆）

| 步骤 | 命令 / 动作 | 说明 |
|------|-------------|------|
| 依赖 | `pnpm install --frozen-lockfile` | 注入脚本链 import `fflate`，仍需 node_modules |
| 驱动解析 | `resolve-drivers.mjs --drivers=basic` | 写入 `.driver-features.json`、codegen |
| Rust | 见下表 | Rust **stable** |
| 清理 | `driver-file-stash.mjs restore` | 恢复被 inject 的 tracked 文件（`if: always()`） |
| ai-api | `cargo test -p datazen-ai-api --lib` | 在 restore **之后**执行（不依赖 inject 产物） |

Rust 测试顺序（与 `ci.yml` 一致）：

```bash
# driver-api 与四个 path 驱动合为一次 cargo 调用（不依赖 --features）
cargo test --lib -p datazen-driver-api -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis
# datazen 需要 features 选择注入的驱动
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen --lib --features "$FEATURES"
# 恢复被 inject 的 tracked 文件，确保后续步骤工作在干净状态
node scripts/driver-file-stash.mjs restore
# ai-api 不依赖注入产物，放在 restore 之后
cargo test -p datazen-ai-api --lib
```

## 3. 驱动预设与 SKU

| 预设 / SKU | Registry ids | PR CI | Release job |
|------------|--------------|-------|-------------|
| `basic`（默认） | postgres, mysql, sqlite, redis | ✅ | Basic 变体 |
| `all` | 全部 **path** 条目（不含 git 驱动） | ❌ | All 变体（`*-all` 后缀） |
| Akulaku 显式列表 | postgres,mysql,sqlite,redis,mongodb,kiwi,superset | ❌ | Akulaku 变体（`*-akulaku`）；`needs_git: true` |
| 自定义逗号列表 | 任意 registry id 组合 | ❌ | 仅本地 / 定制发版 |

详见 [optional-drivers.md](./optional-drivers.md)、[ci-private-drivers.md](./ci-private-drivers.md)。

## 4. 本地 PR 基线（与 CI 对齐）

贡献者在开 PR 前至少跑：

```bash
node scripts/generate-builtin-locales.mjs
pnpm typecheck
pnpm test:unit
pnpm test:unit:drivers
node scripts/resolve-drivers.mjs --drivers=basic
cargo test --lib -p datazen-driver-api -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen --lib --features "$FEATURES"
node scripts/driver-file-stash.mjs restore
cargo test -p datazen-ai-api --lib
```

若改动 `site/`：`node scripts/check-site-seo.mjs`。

若改动可选 path 驱动的 **Rust**：追加 `cargo test -p datazen-driver-<id> --lib`（该 crate 的 UI 单测已由 PR CI 的 `pnpm test:unit:drivers` 覆盖，无需再手工跑）。

若改动 Host UI 交互路径：同 PR 更新 E2E（见 [e2e-testing.md](./e2e-testing.md)）；全量 E2E 耗时长，**不要求**与 PR CI 同跑，但须在 PR test plan 说明。

## 5. E2E 与契约矩阵（CI 外）

| 命令 | 驱动 | 用途 | CI |
|------|------|------|-----|
| `pnpm e2e:minimal` | basic | 快速 Host E2E | ❌ |
| `pnpm e2e` | 当前 inject 选型 | 全量 Host E2E | ❌ |
| `pnpm e2e:contract:matrix` | PG + MySQL + SQLite | Host UI 契约 × 驱动 | ❌ |
| `pnpm test:unit:e2e-contract` | — | contract fixtures 单测 | ❌ |
| `packages/drivers/<id>/e2e/` | 单驱动 | 方言 / 专属 UI | ❌ |

契约 journey 列表见 [e2e-coverage.md](./e2e-coverage.md) §「Host Connection Contract × Driver」。

## 6. Release 流水线（摘要）

`release.yml` 在 tag `v*` 或手动 dispatch 时构建安装包；**不**替代 PR CI 的单测矩阵。

- **Basic**：四平台 × basic 驱动（与 PR CI 同套核心驱动，但做完整 `tauri build`）。
- **All**：四平台 × 全部 path 驱动（**不进 PR CI** 的集成验证点）。
- **Akulaku**：三平台（Windows / macOS）× 含 git 私有驱动；Secrets 在 GitHub Environment `release`。
- **更新通道（每个 SKU 各自独立）**：三个 SKU 都产出签名 updater 产物并各自发布清单
  `latest.json` / `latest-all.json` / `latest-akulaku.json`；构建时按 `matrix.variant`
  注入该 SKU 自己的 endpoint（`ci-tauri-build.mjs` 以 JSON Merge Patch 覆盖
  `plugins.updater.endpoints`）。Tauri updater 只按**平台**在清单里查条目、不认 SKU，
  因此共用一份清单就等于把变体更新成 Basic（丢掉 Basic 不含的驱动）。SKU 名单、清单名
  与平台集合的唯一来源是 `scripts/release-variants.mjs`；变体清单缺平台即失败，Basic
  仅告警。发布前用 `pnpm test:release-variants` 校验矩阵 / `tauri.conf.json` / 清单步骤 /
  打包模板四方一致（`scripts/check-release-variants.mjs`）。详见 [updater.md](./updater.md)。

### 6.1 driver union 预热：已撤销（实测）

**不要**在变体矩阵前加 `warm-driver-deps` 预热门。

曾存在这样一个 job：在 4 个 target 上各编一次 driver union（`all,kiwi,superset`）写进共享
rust-cache，11 个 variant job 通过 `needs` 等它完成。技术前提是成立的——driver crate 和
第三方依赖在所有 variant 中源码与 features 完全一致，只有宿主 `datazen` lib 随 driver
feature 集变化。但实测证明它一分钱不省：

| 指标 | 撤销前 `36286459429` | 撤销后 `36300831455` | 变化 |
| --- | --- | --- | --- |
| **总墙钟** | **34:47** | **56:12** | **+21:25 (+62%)** |
| runner-minutes（所有 job 之和） | 186:50 | 251:54 | +65:04 (+35%) |
| 11 个 build job 重活步骤合计 | 155:42 | 156:37 | +0:55 (+0.6%) |
| build 阶段最长 leg | 33:45 | 31:52 | −1:53 |

三条原因：

1. **`needs` 是 job 级硬屏障。** 预热的 23:48 完全串行地加在关键路径上，没有任何东西与之
   并行。`+23:48 − 1:53 = +21:55`，与实测的 `+21:25` 吻合：build 阶段本身的长度几乎没变，
   预热是**净增**而非替换。
2. **预热要暖的依赖闭包本来就是热的。** 两个 run 里 11 个 build job 的
   `Cache Rust compilation` 恢复耗时**全部 > 5s**（命中；miss 约 1~2s）——`swatinem/rust-cache`
   自 `9eb095728` 起已在工作。预热没有把任何一个 miss 变成 hit，因为没有 miss 可转。
3. **真正占时间的部分预热在结构上碰不到。** 本地实测单个 variant 在热缓存之上的边际成本是
   **lib codegen 5m29s + fat LTO 链接 12m07s ≈ 17m36s**。lib 按各自的 `--features driver-*`
   编译；链接要把该 variant 的 `dist/` 嵌进二进制，且 fat LTO 要对所有 rlib 重跑一遍全程序
   优化。两者都是 variant 特有的，预热只能命中「第三方依赖 + driver crate」这一段，而那一段
   本就已经 100% 命中。

> **教训**：评估任何缓存/预热优化之前，先量被优化那一侧在优化**之前**的 cache 恢复耗时。
> 用「应该会 miss」代替「量一下是不是 miss」，会做出一个 21 分钟的负优化。

撤销预热时必须**同时**去掉 build job 上的 `save-if: false`：预热曾是该 cache key 的唯一写入者，
留着这行就变成无人写入，条目会在 7 天闲置后被 GitHub 逐出，之后每次发版都静默退回冷编译。

### 6.2 union 类型检查：保留

`union-typecheck` 是独立的一个 job，对 union（`all,kiwi,superset`，即 basic / all / akulaku
的超集）做**一次** `tsc --noEmit`。11 个 variant job 通过 workflow 级
`DATAZEN_CI_TYPECHECK_ONCE=1` 跳过自己的 tsc，改走 `pnpm build:bundle`（codegen + Vite）。

单列一个 job 的理由是它与 target 无关：一条 runner 就能覆盖全部 4 个
`(platform, target)` 组合，放在预热里则要么重复 4 次，要么落在最慢那条的关键路径上。

**它不拖慢整条 run。** 把 tsc 放回 11 个 variant job 看似「去掉一个串行前缀」，但那样每个
job 都长 `T_tsc`，而结束时间取最长那条 leg：墙钟同样 +`T_tsc`，runner 时间反而多
`10 × T_tsc`，且覆盖面从 union 缩回各自的 variant。保留是严格更优。唯一前提是它必须继续
`needs` 进 `build`——否则等于给发版摘掉了类型检查。

### 6.3 不要用环境变量加回 thin LTO

没有 `--profile` 不代表改不动：`tauri build` 会把环境传给它的 cargo 子进程，而 cargo 认
`CARGO_PROFILE_RELEASE_LTO=thin` / `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`（已在隔离工程上
验证 rustc 确实收到 `-C lto=thin`）。这是一条真实存在的逃生口，本仓库**主动选择不走**：
`[profile.release]` 的 `opt-level = "z"` + fat LTO 是为压体积设的，thin LTO 会让发版二进制
变大且幅度未实测。需要时先量体积。

## 7. 相关文档

- [e2e-testing.md](./e2e-testing.md) — WebDriver 构建与跑法
- [e2e-coverage.md](./e2e-coverage.md) — Host 路径覆盖矩阵
- [optional-drivers.md](./optional-drivers.md) — 可选 path 驱动说明
- [ci-private-drivers.md](./ci-private-drivers.md) — Git 驱动 Deploy Key
- [packaging.md](./packaging.md) — 发版渠道与 SKU 命名
