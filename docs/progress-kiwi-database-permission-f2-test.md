# F2 测试报告：`databaseFieldType: 'domain'` + SchemaTree 多库路由

**分支**：`fix/kiwi-database-permission-load`  
**HEAD commit**：`50820b1` — fix(schema): only lock sidebar when configured DB is in list（F1）  
**测试时间**：2026-08-07  
**Agent 模式**：report-only（未改业务代码、未 commit）

> **说明**：F2 相关改动存在于**工作区未提交 diff**（`SchemaTree.tsx`、`databaseMeta.ts`、测试、`plugins-registry.json` 等）。Vitest 与静态核对均针对当前工作区执行。

## 测试目标（F2）

1. `databaseFieldType: 'domain'` 元数据（类型定义 + Kiwi 插件 meta）
2. `shouldUseMultiDatabaseTree`：`domain` 类型即使有 `initialDatabase` 也走多库树
3. `SchemaTree`：`domain` 时 strip `initialDatabase` 再交给 `MultiDatabaseSchemaTree`
4. Kiwi 插件 pin `b9cc1bdd2884ef9f95287c0b038664ec9f4f3598`

---

## 必跑：Vitest

```bash
cd /Users/flyxl/code/datazen
npx vitest run \
  src/windows/connection/schema-tree/__tests__/SchemaTree.test.tsx \
  src/lib/__tests__/databaseTypes.test.ts \
  src/stores/__tests__/schemaStore.test.ts
```

### 命令输出摘要

```
 RUN  v4.1.10 /Users/flyxl/code/datazen

 Test Files  3 passed (3)
      Tests  27 passed (27)
   Duration  1.17–1.30s
   Exit code: 0
```

### 与 F2 直接相关的用例

| 文件 | 用例 | 结果 |
|------|------|------|
| `SchemaTree.test.tsx` | `shouldUseMultiDatabaseTree ignores domain field as logical lock` | ✅ PASS |
| `SchemaTree.test.tsx` | mysql/pg 有/无 `initialDatabase` 路由（回归） | ✅ PASS（6 用例） |
| `databaseTypes.test.ts` | `kiwi has multi-database and fixed page size when plugin is loaded` | ⚠️ 见下方 |
| `schemaStore.test.ts` | `does not lock when configured database is absent from server list (e.g. Kiwi domain)` | ✅ PASS（F1 协同） |

**`databaseTypes.test.ts` 备注**：`DB_REGISTRY.kiwi` 在本工作区未注入（`src/plugins/generated.ts` 中 `PLUGIN_DB_ENTRIES` 为空，`.plugins/` 不存在）。该用例通过 `if (!DB_REGISTRY.kiwi) return` 早退，**未实际断言** Kiwi meta。Kiwi `databaseFieldType: 'domain'` 改由远程 pin 静态核对覆盖（见下）。

---

## 静态核对

### 1. `SchemaTree.tsx` — `shouldUseMultiDatabaseTree` + domain strip

**文件**：`src/windows/connection/schema-tree/SchemaTree.tsx`

| 检查项 | 预期 | 实际 | 结果 |
|--------|------|------|------|
| 导出 `shouldUseMultiDatabaseTree` | 存在 | L25–32 | ✅ |
| `databaseFieldType === 'domain'` → 返回 `true`（忽略 `initialDatabase`） | `return true` | L30 | ✅ |
| 非 domain：有 `initialDatabase` → `false` | `!initialDatabase?.trim()` | L31 | ✅ |
| 路由 Multi 树前 strip domain | `{ ...props, initialDatabase: undefined }` | L54–57 | ✅ |
| 注释说明 Kiwi domain 语义 | 有 | L18–24 | ✅ |

核心逻辑：

```typescript
if (meta.databaseFieldType === 'domain') return true;
// ...
const treeProps =
  meta?.databaseFieldType === 'domain'
    ? { ...props, initialDatabase: undefined }
    : props;
```

### 2. `databaseMeta.ts` — 类型扩展

**文件**：`src/lib/databaseMeta.ts`

| 检查项 | 结果 |
|--------|------|
| `databaseFieldType` 联合含 `'domain'` | ✅ |
| 注释说明 domain = 实例 host/domain，不参与 sidebar lock | ✅ |

### 3. Kiwi 插件 meta — `databaseFieldType: 'domain'`

| 来源 | 结果 |
|------|------|
| 本地 `.plugins/kiwi/ui/plugin-meta.ts` | ❌ 不可用（`.plugins/` 未生成） |
| 远程 `datazen-driver-kiwi` @ `b9cc1bd`（`git show …:ui/plugin-meta.ts`） | ✅ `databaseFieldType: 'domain'`，`hasMultiDatabase: true`，`defaultPageSize: 999` |

### 4. `plugins-registry.json` — Kiwi ref pin

**文件**：`plugins-registry.json`（工作区 diff，未 commit）

| 检查项 | 预期 | 实际 | 结果 |
|--------|------|------|------|
| kiwi `ref` | `b9cc1bdd2884ef9f95287c0b038664ec9f4f3598` | L21 匹配 | ✅ |
| 相对 HEAD commit | 已提交 | 仍为 `a17b12f…`（F2 pin 仅在工作区） | ⚠️ 未 commit |

---

## F2 行为矩阵（`shouldUseMultiDatabaseTree`）

| hasMultiDatabase | databaseFieldType | initialDatabase | 期望 | 测试覆盖 |
|------------------|-------------------|-----------------|------|----------|
| true | domain | `afi-ph-useraccount-dbreader.aku` | Multi 树 | ✅ 单元测试 |
| true | name | `datazen_test` | Standard 树 | ✅ 单元 + 集成 |
| true | name | undefined | Multi 树 | ✅ 单元 + 集成 |
| false | domain | any | Standard 树 | ✅ 单元测试 |

---

## 结论

| 维度 | 结论 |
|------|------|
| Vitest（3 文件 / 27 用例） | **PASS**（exit 0） |
| `SchemaTree` domain 路由逻辑 | **PASS** |
| `databaseMeta` `'domain'` 类型 | **PASS** |
| Kiwi 插件 meta @ `b9cc1bd` | **PASS**（远程核对） |
| Registry pin `b9cc1bd` | **PASS**（工作区；HEAD 未含此 pin） |
| 本地 plugin inject / `DB_REGISTRY.kiwi` 断言 | **SKIP**（无 `.plugins/`） |

### 总评：**PASS**（附注）

F2 核心实现与专用单元测试均通过；Kiwi 插件 meta 与 registry pin 在工作区 / 远程 ref 上核对一致。  
**待办（非本 agent 范围）**：F2 diff 尚未 commit；合并前需 `resolve-plugins` 注入 kiwi 后复跑 `databaseTypes.test.ts` 以激活 Kiwi meta 断言。

---

## 相关文件

- 实现：`src/windows/connection/schema-tree/SchemaTree.tsx`、`src/lib/databaseMeta.ts`
- 测试：`src/windows/connection/schema-tree/__tests__/SchemaTree.test.tsx`、`src/lib/__tests__/databaseTypes.test.ts`
- 插件：`plugins-registry.json` → kiwi ref `b9cc1bd`
- 进度主文档：[progress-kiwi-database-permission.md](./progress-kiwi-database-permission.md)
