# 表结构编辑器 — 插件配置 + 驱动自报能力 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Host 表结构编辑器改为「唯一壳 + 驱动 UI 配置 + 驱动自报 caps/DDL plan」，P1 完成列与索引同草稿；T0（postgres/mysql/sqlite）完整，其余默认不支持；Mongo/Redis opt-out。

**Architecture:** `packages/driver-api` 新增 `StructureCapabilities` / `StructureChangeRequest` / `StructureChangePlan` 与 `DatabaseDriver` 默认方法；T0 驱动实现版本感知 caps + 方言 plan；Host 仅 IPC 消费 caps，禁止中央对照表；前端 `structureEditor` meta 驱动类型列表与字段显隐。

**Tech Stack:** Rust (`driver-api` + path drivers + Tauri commands)、React/TS (`TableStructureEditor`)、Vitest、cargo test、现有 `e2e/specs/table-structure.ts`。

**Spec:** [docs/superpowers/specs/2026-08-09-table-structure-editor-plugins-design.md](../specs/2026-08-09-table-structure-editor-plugins-design.md)

**开放项锁定（本计划）：**

| 项 | 决定 |
|----|------|
| Caps 缓存 | 每次打开结构编辑器拉取一次；编辑期间不缓存跨会话 |
| 执行 | 按 plan 语句**逐条**执行，失败即停并展示已执行条数；不伪称 MySQL 整批回滚 |
| PROTOCOL_VERSION | 增 trait 方法时 bump `1 → 2`，并同步检查依赖本协议的驱动/插件 |

---

## 文件结构（预期触点）

| 路径 | 职责 |
|------|------|
| `packages/driver-api/src/types.rs` | 新增 structure 相关类型 |
| `packages/driver-api/src/traits.rs` | `structure_capabilities` / `plan_structure_changes` 默认实现 |
| `packages/driver-api/src/lib.rs` | `PROTOCOL_VERSION = 2`；re-export |
| `packages/driver-api/src/reuse.rs` | `ReuseDriver` 转发新方法 |
| `packages/drivers/postgres/src/...` | caps（含 PG≥11 `indexInclude`）+ plan + 单元测试 |
| `packages/drivers/mysql/src/...` | caps + plan + 测试 |
| `packages/drivers/sqlite/src/...` | caps（`sqlite_rebuild`）+ plan + 测试 |
| `src-tauri/src/commands/schema.rs`（或新建 `structure.rs`） | IPC：`get_structure_capabilities`、`plan_table_structure_changes` |
| `src-tauri/src/lib.rs` / `main` 命令注册 | 注册上述命令 |
| `src/commands/database.ts`（或 `structure.ts`） | 前端 invoke 封装 |
| `src/lib/databaseMeta.ts` | `structureEditor?: StructureEditorUiConfig` |
| `src/lib/structureEditor/` | `types.ts`、`isControlEnabled.ts` + vitest |
| `packages/drivers/{postgres,mysql,sqlite}/ui/meta.ts`（或现有 meta 入口） | `structureEditor` 配置 |
| `src/windows/connection/TableStructureEditor.tsx` | 改为消费 caps + plan IPC；索引同草稿 |
| `src/windows/connection/IndexesView.tsx` | 薄封装或「去结构编辑器」引导；禁止分叉 SQL |
| 入口点（SchemaTree / 表头菜单等） | Mongo/Redis 隐藏「编辑表结构 / 新建表」 |
| `docs/...` / locales | 简短用户说明 + i18n keys |
| `e2e/specs/table-structure.ts` | 回归预览/保存仍可用 |

---

### Task 1: driver-api 类型 + trait 默认 + PROTOCOL_VERSION

**Files:**
- Modify: `packages/driver-api/src/types.rs`
- Modify: `packages/driver-api/src/traits.rs`
- Modify: `packages/driver-api/src/lib.rs`
- Modify: `packages/driver-api/src/reuse.rs`
- Test: `packages/driver-api` 内小型默认行为测试（可选 crate 内 `#[cfg(test)]`）

- [ ] **Step 1: 写失败测试（默认 plan 不支持）**

在 `packages/driver-api` 或临时假驱动测试中断言：未覆盖驱动调用 `plan_structure_changes` 返回 `DriverError`（或约定的 `Unsupported`），且 `structure_capabilities` 全 false。

若当前 crate 无测试 harness，可先在 `src-tauri` 用 mock（Task 6）；本 Task 至少编译期定义类型。

- [ ] **Step 2: 实现类型（建议字段，serde `camelCase`）**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StructureCapabilities {
    pub create_table: bool,
    pub add_column: bool,
    pub drop_column: bool,
    pub rename_column: bool,
    pub alter_type: bool,
    pub alter_nullability: bool,
    pub alter_default: bool,
    pub alter_primary_key: bool,
    pub reorder_column: bool,
    pub comment: bool,
    pub create_index: bool,
    pub drop_index: bool,
    pub rebuild_index: bool,
    pub index_type: bool,
    pub index_include: bool,
    pub index_filter: bool,
    pub index_comment: bool,
    pub alter_strategy: AlterStrategy,
    pub dialect_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub index_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum AlterStrategy {
    #[default]
    None,
    Direct,
    SqliteRebuild,
}

// StructureColumnDraft / StructureIndexDraft / StructureChangeRequest / StructureChangePlan
// PlanStatement { sql, summary, risk } — risk 对齐 schema-diff: Additive|Destructive|Rewrite
```

`StructureChangeRequest` 需包含：`mode`（create|alter）、`schema`、`table`、`original`/`current` 列与索引（稳定 `id` 字段）。

- [ ] **Step 3: trait 默认实现**

```rust
async fn structure_capabilities(
    &self,
    _handle: &ConnectionHandle,
) -> Result<StructureCapabilities, DriverError> {
    Ok(StructureCapabilities {
        dialect_id: self.driver_type(),
        ..Default::default()
    })
}

async fn plan_structure_changes(
    &self,
    _handle: &ConnectionHandle,
    _request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    Err(DriverError::Unsupported(
        "table structure planning is not supported by this driver".into(),
    ))
}
```

确认 `DriverError` 已有合适变体；若无，用现有最接近变体并在错误文案中标明。

- [ ] **Step 4: `ReuseDriver` 转发**

在 `reuse.rs` 中 `async fn structure_capabilities` / `plan_structure_changes` 委托 `self.inner`。

- [ ] **Step 5: bump PROTOCOL_VERSION**

`packages/driver-api/src/lib.rs`：`PROTOCOL_VERSION` `1` → `2`。检索仓库内硬编码协议版本断言并更新。

- [ ] **Step 6: 验证**

```bash
cd packages/driver-api && cargo test
# 或从 workspace 根：
cargo test -p datazen-driver-api
```

Expected: PASS（crate 名以 `Cargo.toml` 为准）。

- [ ] **Step 7: Commit**

```bash
git add packages/driver-api
git commit -m "$(cat <<'EOF'
feat(driver-api): add structure capabilities/plan trait (protocol v2)

EOF
)"
```

---

### Task 2: PostgreSQL — caps（含版本）+ plan + 测试

**Files:**
- Create/Modify: `packages/drivers/postgres/src/structure.rs`（或等价模块）
- Modify: postgres `DatabaseDriver` impl
- Test: `packages/drivers/postgres/src/structure.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试 — PG 版本补丁**

```rust
#[test]
fn caps_index_include_false_on_pg10() {
    let caps = caps_for_version("10.23");
    assert!(!caps.index_include);
}

#[test]
fn caps_index_include_true_on_pg14() {
    let caps = caps_for_version("14.5");
    assert!(caps.index_include);
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p datazen-driver-postgres caps_index_include -- --nocapture
```

Expected: FAIL（函数未定义）。

- [ ] **Step 3: 实现 `caps_for_version` + `structure_capabilities`**

从 `get_server_info(handle)` 取 `server_version`；解析 major（失败 → 保守基线：`index_include = false`）。基线：`create_table/add/drop/rename/alter_*/create_index/drop_index/index_type/index_include(≥11)/index_filter/comment` 等按 PG 能力设 true；`reorder_column = false`（P1）；`alter_strategy = Direct`；`index_methods` 含 btree/hash 等（按版本过滤）。

- [ ] **Step 4: 写失败测试 — plan SQL 快照**

至少覆盖：`add_column`、`drop_column`、`rename_column`、`create_index`、`drop_index`。断言生成 SQL 子串或完整快照（注意引号 `"schema"."table"`）。

- [ ] **Step 5: 实现 `plan_structure_changes`**

意图 → 有序 `PlanStatement`；违反 caps → `Err`（明确文案），禁止静默生成。

- [ ] **Step 6: 跑测试**

```bash
cargo test -p datazen-driver-postgres structure
```

Expected: PASS。

- [ ] **Step 7: Commit**

```bash
git add packages/drivers/postgres
git commit -m "$(cat <<'EOF'
feat(postgres): structure_capabilities and plan_structure_changes

EOF
)"
```

---

### Task 3: MySQL — caps + plan + 测试

**Files:**
- Create/Modify: `packages/drivers/mysql/src/structure.rs`
- Modify: mysql driver impl
- Test: 同模块 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试** — `ADD COLUMN` / `DROP INDEX` / `MODIFY COLUMN` 等方言快照；caps 含 `comment`/`index_type` 等；`reorder_column` P1 可 false 或按 MySQL `MODIFY ... AFTER` 能力决定（若实现 AFTER 则 true，并在 plan 发出）。

- [ ] **Step 2: 跑测 FAIL → 实现 → PASS**

```bash
cargo test -p datazen-driver-mysql structure
```

- [ ] **Step 3: Commit**

```bash
git add packages/drivers/mysql
git commit -m "$(cat <<'EOF'
feat(mysql): structure_capabilities and plan_structure_changes

EOF
)"
```

---

### Task 4: SQLite — caps + plan + 测试

**Files:**
- Create/Modify: `packages/drivers/sqlite/src/structure.rs`
- Modify: sqlite driver impl

- [ ] **Step 1: 写失败测试** — `alter_strategy == SqliteRebuild`；不支持的原地改类型时 plan 风险为 `Rewrite` 或返回清晰错误（与产品诚实文案一致）；`add_column` 支持时生成 `ALTER TABLE ... ADD COLUMN`。

- [ ] **Step 2: FAIL → 实现 → PASS**

```bash
cargo test -p datazen-driver-sqlite structure
```

- [ ] **Step 3: Commit**

```bash
git add packages/drivers/sqlite
git commit -m "$(cat <<'EOF'
feat(sqlite): structure_capabilities and plan_structure_changes

EOF
)"
```

---

### Task 5: Host IPC 命令（无中央 caps 表）

**Files:**
- Modify or Create: `src-tauri/src/commands/schema.rs` / `structure.rs`
- Modify: 命令注册模块
- Test: `src-tauri` 内可测「命令委托 DriverRegistry」的薄逻辑（mock 困难时可依赖驱动集成测试 + 手动）

- [ ] **Step 1: 实现命令**

```rust
#[tauri::command]
pub async fn get_structure_capabilities(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<StructureCapabilities, CommandError> { /* resolve driver+handle → structure_capabilities */ }

#[tauri::command]
pub async fn plan_table_structure_changes(
    state: State<'_, AppState>,
    connection_id: String,
    request: StructureChangeRequest,
) -> Result<StructureChangePlan, CommandError> { /* → plan_structure_changes */ }
```

**禁止：** 任何 `match db_type` / `HashMap<DatabaseType, StructureCapabilities>` 作为 caps 来源。

- [ ] **Step 2: 注册 invoke handler**

- [ ] **Step 3: 编译**

```bash
cargo check -p datazen
```

Expected: 成功（需已 inject 驱动 features 的本地环境；或与 CI 一致的 `DATAZEN_DRIVERS=basic`）。

- [ ] **Step 4: Commit**

```bash
git add src-tauri
git commit -m "$(cat <<'EOF'
feat(ipc): get_structure_capabilities and plan_table_structure_changes

EOF
)"
```

---

### Task 6: 前端类型、commands、`isControlEnabled`

**Files:**
- Create: `src/lib/structureEditor/types.ts`
- Create: `src/lib/structureEditor/isControlEnabled.ts`
- Create: `src/lib/structureEditor/isControlEnabled.test.ts`
- Modify: `src/commands/database.ts`（或新建 `structure.ts`）
- Modify: `src/lib/databaseMeta.ts` — 增加 `structureEditor?`

- [ ] **Step 1: 写失败测试**

```ts
import { describe, it, expect } from 'vitest';
import { isControlEnabled } from './isControlEnabled';

describe('isControlEnabled', () => {
  it('returns false when cap is false', () => {
    expect(isControlEnabled({ renameColumn: false } as StructureCapabilities, 'renameColumn')).toBe(false);
  });
  it('returns true when cap is true', () => {
    expect(isControlEnabled({ renameColumn: true } as StructureCapabilities, 'renameColumn')).toBe(true);
  });
});
```

- [ ] **Step 2: 跑测 FAIL → 实现 → PASS**

```bash
npx vitest run src/lib/structureEditor/isControlEnabled.test.ts
```

- [ ] **Step 3: 封装 IPC**

```ts
export function getStructureCapabilities(connectionId: string) {
  return invoke<StructureCapabilities>('get_structure_capabilities', { connection_id: connectionId });
}
export function planTableStructureChanges(connectionId: string, request: StructureChangeRequest) {
  return invoke<StructureChangePlan>('plan_table_structure_changes', {
    connection_id: connectionId,
    request,
  });
}
```

- [ ] **Step 4: 扩展 `DatabaseTypeMeta`**

```ts
structureEditor?: {
  enabled?: boolean;
  columnTypes: { value: string; label: string }[];
  defaultColumnType: string;
  fields: {
    comment?: boolean;
    charset?: boolean;
    collation?: boolean;
    unsigned?: boolean;
    length?: boolean;
  };
  indexMethods: string[];
};
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/structureEditor src/lib/databaseMeta.ts src/commands
git commit -m "$(cat <<'EOF'
feat(frontend): structure editor types, IPC, and control helper

EOF
)"
```

---

### Task 7: T0 驱动前端 `structureEditor` meta

**Files:**
- Modify: postgres/mysql/sqlite 驱动 UI meta（路径以仓库现有 `ui/meta.ts` / `databaseMeta` 贡献为准）
- Modify: mongodb/redis meta — `structureEditor: { enabled: false }` 或省略 + `supportsSQL` 已为 false

- [ ] **Step 1: 为 postgres/mysql/sqlite 填写类型列表与 fields**

从现有 `TableStructureEditor` 硬编码类型列表迁出到对应 meta（Host 删除硬编码默认 PG 列表，改为「无 config → 只读提示 / 禁用新建」）。

- [ ] **Step 2: 确认 Mongo/Redis 无入口配置**

- [ ] **Step 3: Commit**

```bash
git add packages/drivers src/lib
git commit -m "$(cat <<'EOF'
feat(drivers): export structureEditor UI config for T0 SQL drivers

EOF
)"
```

---

### Task 8: 重构 `TableStructureEditor` 壳

**Files:**
- Modify: `src/windows/connection/TableStructureEditor.tsx`（可拆子组件到同目录 `structure/`）
- Modify: 打开编辑器的入口（确认传入 `connectionId`）

- [ ] **Step 1: 打开时并行加载** schema + `getStructureCapabilities` + meta.`structureEditor`

- [ ] **Step 2: 草稿模型** 含列 + 索引；UI 按 `fields` 显隐；`isControlEnabled` 禁用控件 + i18n 短原因

- [ ] **Step 3: `reorderColumn === false` 时禁用拖拽**

- [ ] **Step 4: 预览** 调用 `planTableStructureChanges`；展示 `sql/summary/risk`

- [ ] **Step 5: 执行** 逐条 `execute_query`（或现有执行 API）；失败停止；成功后刷新 schema

- [ ] **Step 6: 删除 Host 内 PG 风格 `generateSQL` / 硬编码类型表**（迁移到驱动 plan）

- [ ] **Step 7: 手动或 vitest 覆盖关键 draft→request 映射 helper（建议抽纯函数并测）

- [ ] **Step 8: Commit**

```bash
git add src/windows/connection
git commit -m "$(cat <<'EOF'
refactor(ui): TableStructureEditor consumes driver caps and plan IPC

EOF
)"
```

---

### Task 9: IndexesView 收敛 + opt-out 入口

**Files:**
- Modify: `src/windows/connection/IndexesView.tsx`
- Modify: Schema tree / 表菜单中「编辑表结构」「新建表」可见性

- [ ] **Step 1: IndexesView** — 只读列表 + CTA「在表结构中编辑」，或内部调用同一 `plan_*`（禁止第二套 SQL 字符串拼接）

- [ ] **Step 2: 入口守卫**

```ts
function canOpenStructureEditor(meta: DatabaseTypeMeta): boolean {
  if (meta.isKeyValue || meta.connectionView === 'document') return false;
  if (meta.structureEditor?.enabled === false) return false;
  if (!meta.supportsSQL) return false;
  return true;
}
```

- [ ] **Step 3: Commit**

```bash
git add src
git commit -m "$(cat <<'EOF'
fix(ui): hide structure editor for non-table drivers; unify indexes path

EOF
)"
```

---

### Task 10: i18n、文档、护栏、e2e

**Files:**
- Modify: `src/locales/en.ts`、`zh-CN.ts`（及项目要求的同步语系）
- Modify: 使用说明章节（若已有连接/表结构相关章则追加；否则短段落）
- Modify: `docs/competitive-comparison-dbx.md`（如需一句指向新架构）
- Modify: `e2e/specs/table-structure.ts`
- Optional: `scripts/` 或 CI comment — `rg` 护栏禁止 Host `capabilityByType` / `structure_capabilities_by`

- [ ] **Step 1: 新增 i18n keys**（caps 禁用原因、执行非原子提示、opt-out 文案）

- [ ] **Step 2: e2e** — PG 预览仍出现 SQL；保存路径不回归

- [ ] **Step 3: 护栏检查**

```bash
rg -n "capabilityByType|structure_capabilities_by|StructureCapabilities\s*\{[^}]*postgres" src src-tauri || true
```

Expected: 无 Host 中央表命中。

- [ ] **Step 4: 回归命令**

```bash
npx vitest run src/lib/structureEditor
cargo test -p datazen-driver-postgres structure
cargo test -p datazen-driver-mysql structure
cargo test -p datazen-driver-sqlite structure
cargo test -p datazen --lib
```

- [ ] **Step 5: Commit**

```bash
git add src/locales docs e2e scripts
git commit -m "$(cat <<'EOF'
docs(i18n): structure editor dialect limits and e2e/guardrails

EOF
)"
```

---

## 自检（计划作者）

- [x] 规格中每个 P1 目标均有对应 Task
- [x] 类型/trait/IPC/UI/T0 驱动/opt-out 均有文件路径
- [x] 关键逻辑含 TDD（版本 caps、plan SQL、`isControlEnabled`）
- [x] 无「实施时再决定」的架构分叉；开放项已锁定
- [x] 明确禁止 Host caps 对照表
- [x] `PROTOCOL_VERSION` bump 写入 Task 1

---

## 执行方式（完成后由用户选择）

1. **Subagent-Driven（推荐）** — 每 Task 派生子代理，Task 间复查  
2. **Inline** — 本会话按 Task 顺序执行  

---

**完成后定义：** T0 三驱动 plan 测试绿；Host 无中央 caps 表；结构编辑器列+索引同草稿走 IPC；Mongo/Redis 无误导入口；现有 table-structure e2e 不红。
