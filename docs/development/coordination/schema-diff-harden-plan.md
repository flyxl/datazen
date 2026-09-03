# Schema Migration Hardening — 实施计划

> 对应 PRD: `docs/todo/schema-differ-harden-prd.md`

## Track sd-drv: Driver 渲染完整性 + Capability 一致性

### 目标
1. CreateTable/AddColumn DDL 渲染完整 metadata（DEFAULT, COMMENT, AUTO_INCREMENT）
2. 三个 driver 的 Capabilities 与 Renderer 完全一致
3. PK migration: PG DropPrimaryKey, MySQL Add/DropPK 实现
4. 所有支持的 op 提供 rollback SQL（能提供则提供，不能则 None）
5. SQLite: Capabilities 标记不支持的 op 为 false（无 rebuild 本阶段不实现 rebuild）

### 验收标准
- [ ] PG CreateTable 渲染包含 DEFAULT、COMMENT
- [ ] MySQL CreateTable 渲染包含 DEFAULT、COMMENT、AUTO_INCREMENT
- [ ] SQLite CreateTable 渲染包含 DEFAULT
- [ ] PG DropPrimaryKey 有 Renderer（使用 information_schema 获取 constraint 名称的注释或参数）
- [ ] MySQL AddPrimaryKey/DropPrimaryKey 有 Renderer
- [ ] 三个 driver Capabilities.supports(op) == true 当且仅当 Renderer 能成功处理
- [ ] SQLite Capabilities 中不支持的 op 标记 false
- [ ] MigrationColumn 有 `default_value` 和 `comment` 字段
- [ ] 每个新增/修改的 Renderer 有对应单元测试
- [ ] `cargo test -p datazen-driver-postgres` 通过
- [ ] `cargo test -p datazen-driver-mysql` 通过
- [ ] `cargo test -p datazen-driver-sqlite` 通过

### 关键文件
- `packages/driver-api/src/schema_migration.rs`
- `packages/drivers/postgres/src/migration.rs`
- `packages/drivers/mysql/src/migration.rs`
- `packages/drivers/sqlite/src/migration.rs`
- `src-tauri/src/schema_diff/operations.rs`（to_driver_api 桥接）

---

## Track sd-plan: Plan 安全

### 目标
1. Type mapper 失败 → PlanRequirement::Unsupported，op 不进入 executable statements
2. Type narrowing (VARCHAR(255)→VARCHAR(100)) → StatementRisk::Rewrite 或 Destructive
3. 激活已有的 is_type_narrowing / is_narrowing_nullability / extract_len

### 验收标准
- [ ] plan.rs 中 type mapper 的 `if let Ok(ty)` 改为 match，Err 时推 Unsupported
- [ ] is_type_narrowing 被 plan 流程调用
- [ ] VARCHAR(255)→VARCHAR(100) 被标记为 Rewrite
- [ ] INT→SMALLINT 被标记为 Rewrite
- [ ] 所有变更有对应单元测试
- [ ] `cargo test -p datazen --lib` 中 schema_diff 测试通过

### 关键文件
- `src-tauri/src/schema_diff/plan.rs`

---

## Track sd-ui: Backfill UI + Requirements 展示

### 目标
1. SchemaDiffPlan TS 类型加 requirements 字段
2. Plan 面板渲染 Backfill/Unsupported requirements
3. 简单 Backfill 提示（无需完整 Configure/Run/Check workflow — 按 PRD 收口建议）
4. Plan Review 展示 risk 标签、warnings、rollback completeness

### 验收标准
- [ ] `src/commands/schemaDiff.ts` 的 SchemaDiffPlan 类型包含 requirements
- [ ] SchemaDiffPlanPanel 展示 requirements（⚠ Backfill required / ❌ Unsupported）
- [ ] Plan 面板展示 rollback completeness（Available / Partial）
- [ ] i18n en.ts 和 zh-CN.ts 有对应 key
- [ ] `npx vitest run` 前端测试通过
- [ ] `npx tsc --noEmit` 通过

### 关键文件
- `src/commands/schemaDiff.ts`
- `src/windows/schema-diff/SchemaDiffPlanPanel.tsx`
- `src/windows/schema-diff/SchemaDiffWindow.tsx`
- `src/locales/en.ts`
- `src/locales/zh-CN.ts`

---

## Track sd-ir: IR/Compare/Dependencies 完善

### 目标
1. PK 变更不再被 skip（ir.rs 的 `ColumnChange::PrimaryKey => continue` 改为生成 op）
2. Index diff 增强：属性变更（columns 改变）→ DropIndex + CreateIndex
3. Dependency ordering 增加稳定 tie-break（同桶内按 table+op key 排序）

### 验收标准
- [ ] PK 从 col1→col2 变更时生成 DropPrimaryKey + AddPrimaryKey
- [ ] Index columns 变更时生成 DropIndex + CreateIndex
- [ ] 同桶内 op 按 `table_name + key()` 字母序排序
- [ ] 相同输入两次生成的 Plan 完全一致
- [ ] 每项变更有对应单元测试
- [ ] `cargo test -p datazen --lib` 中 schema_diff 测试通过

### 关键文件
- `src-tauri/src/schema_diff/ir.rs`
- `src-tauri/src/schema_diff/compare.rs`
- `src-tauri/src/schema_diff/dependencies.rs`
