# CI 与测试矩阵

> 与 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)、[release.yml](../../.github/workflows/release.yml) 及 [AGENTS.md](../../AGENTS.md) 测试约定配套。  
> 目标：PR 流水线快且稳定；驱动 SKU 组合可扩展；**`all` 预设不进 PR CI**。

## 1. 总览

| 层级 | PR CI（`ci.yml`） | Release（`release.yml`） | 本地 / 维护者 |
|------|-------------------|--------------------------|---------------|
| 驱动选型 | **`basic` 固定**（postgres, mysql, sqlite, redis） | Basic / All / Akulaku 三 SKU × 四平台 | 任意 `--drivers=` / `DATAZEN_DRIVERS` |
| Host 前端单测 | ✅ `pnpm test:unit` | 构建前 `pnpm build`（含 typecheck） | `pnpm test:unit` |
| TypeScript | ✅ `pnpm typecheck` | 同上 | `pnpm typecheck` |
| Host Rust lib | ✅ `cargo test -p datazen --lib`（basic features） | 完整 release 构建 | `cargo test -p datazen --lib` |
| driver-api | ✅ | 随构建链接 | `cargo test -p datazen-driver-api --lib` |
| ai-api | ✅ | 随构建链接 | `cargo test -p datazen-ai-api --lib` |
| Basic path 驱动 lib | ✅ 四 crate 并行 | Basic SKU 内嵌 | `cargo test -p datazen-driver-<id> --lib` |
| 可选 path 驱动 lib | ❌ | **All SKU** 构建时编译链接 | 改驱动 crate 时本地必跑 |
| Git 驱动（kiwi/superset） | ❌ | **Akulaku SKU**（需 Deploy Key） | 见 [ci-private-plugins.md](./ci-private-plugins.md) |
| Host E2E | ❌ | ❌（发版后手工 / R 阶段） | `pnpm e2e` / `pnpm e2e:minimal` |
| Host 契约矩阵 E2E | ❌ | ❌ | `pnpm e2e:contract:matrix` |
| 驱动专属 E2E | ❌ | ❌ | `packages/drivers/<id>/e2e/` |

**原则**

1. **Basic 必测**：每个 PR 与 `main` push 均跑 basic 四驱动 + Host 三件套（TS 单测 / Host lib / driver-api / ai-api）。
2. **All 不进 PR CI**：`resolve-drivers --drivers=all` 仅用于 Release **All** SKU 与本地全量验证，避免 PR 流水线编译全部 path 驱动。
3. **Path 轮转（维护者策略）**：可选 path 驱动（mongodb、clickhouse、duckdb、sqlserver、elasticsearch 等）**不在 PR CI 矩阵内**；修改某驱动 crate 时，作者须在 PR 说明中列出 `cargo test -p datazen-driver-<id> --lib`（及该 crate 内 UI 单测 / E2E）。发版 **All** SKU 是对全部 path 驱动的集成校验。
4. **契约矩阵**：Host Connection Contract（`e2e/contract/`）验证 PG/MySQL/SQLite 上同一套 Host UI journey；**不进 PR CI**，由维护者在合并前或 R 阶段跑 `pnpm e2e:contract:matrix`；fixtures 单测 `pnpm test:unit:e2e-contract` 可在本地或后续 CI 扩展中启用。

## 2. PR CI 步骤（与 workflow 对齐）

触发：`pull_request` → `main`、`push` → `main`、`workflow_dispatch`。

环境：`ubuntu-latest`；`DATAZEN_KEYRING=file`（无 OS 钥匙串）。

| 步骤 | 命令 / 动作 | 说明 |
|------|-------------|------|
| 依赖 | `pnpm install --frozen-lockfile` | Node **24**、pnpm **11**（与 workflow 一致） |
| 代码生成 | `node scripts/generate-builtin-locales.mjs` | `builtinLocales.ts` 为 gitignore codegen |
| 类型 | `pnpm typecheck` | `tsc --noEmit` |
| 守卫 | `check-managed-stubs.mjs`、`check-structure-editor-guardrails.mjs` | 防止误提交 inject 产物 |
| Host 单测 | `pnpm test:unit` | Vitest；`pretest:unit` 会 `--codegen-only --drivers=basic` |
| Site（条件） | `check-site-seo.mjs` | 仅当 diff 含 `site/` |
| 驱动解析 | `resolve-drivers.mjs --drivers=basic` | 写入 `.driver-features.json`、codegen |
| Rust | 见下表 | Rust **stable** |
| 清理 | `driver-file-stash.mjs restore` | 恢复被 inject 的 tracked 文件（`if: always()`） |
| ai-api | `cargo test -p datazen-ai-api --lib` | 在 restore **之后**执行（不依赖 inject 产物） |

Rust 测试顺序（与 `ci.yml` 一致）：

```bash
cargo test -p datazen-driver-api --lib
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen --lib --features "$FEATURES"
cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --lib
node scripts/driver-file-stash.mjs restore
cargo test -p datazen-ai-api --lib
```

## 3. 驱动预设与 SKU

| 预设 / SKU | Registry ids | PR CI | Release job |
|------------|--------------|-------|-------------|
| `basic`（默认） | postgres, mysql, sqlite, redis | ✅ | Basic 变体 |
| `all` | 全部 **path** 条目（不含 git 驱动） | ❌ | All 变体（`*-all` 后缀） |
| Akulaku 显式列表 | postgres,mysql,sqlite,redis,mongodb,kiwi,superset | ❌ | Akulaku 变体（`*-akulaku`）；`needs_git: true` |
| 自定义逗号列表 | 任意 registry id 组合 | ❌ | 仅本地 / 定制发版 |

详见 [optional-drivers.md](./optional-drivers.md)、[ci-private-plugins.md](./ci-private-plugins.md)。

## 4. 本地 PR 基线（与 CI 对齐）

贡献者在开 PR 前至少跑：

```bash
node scripts/generate-builtin-locales.mjs
pnpm typecheck
pnpm test:unit
node scripts/resolve-drivers.mjs --drivers=basic
cargo test -p datazen-driver-api --lib
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen --lib --features "$FEATURES"
cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --lib
node scripts/driver-file-stash.mjs restore
cargo test -p datazen-ai-api --lib
```

若改动 `site/`：`node scripts/check-site-seo.mjs`。

若改动可选 path 驱动：追加 `cargo test -p datazen-driver-<id> --lib` 与 `pnpm test:unit:drivers`（对应 crate 的 `ui/__tests__/`）。

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
- **Akulaku**：含 git 私有驱动；Secrets 在 GitHub Environment `release`。

## 7. 相关文档

- [e2e-testing.md](./e2e-testing.md) — WebDriver 构建与跑法
- [e2e-coverage.md](./e2e-coverage.md) — Host 路径覆盖矩阵
- [optional-drivers.md](./optional-drivers.md) — 可选 path 驱动说明
- [ci-private-plugins.md](./ci-private-plugins.md) — Git 驱动 Deploy Key
- [packaging.md](./packaging.md) — 发版渠道与 SKU 命名
