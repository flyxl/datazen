# New Connection Driver Icons + Sidebar UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Path 驱动自带 `ui/icons/*.svg` 经 `resolve-drivers` 注入 `DRIVER_ICON_ENTRIES`；新建连接侧栏用 `DbTypeBadge`、独立滚动与无 label 的搜索框。

**Architecture:** 图标与 `meta.ts` 同包；`scripts/resolve-drivers.mjs` 扫描 `ui/icons/{dbType}.svg`（缺失时查同驱动内别名文件），生成 `import … from '…?url'` + `DRIVER_ICON_ENTRIES`；`getDriverIconMap()` 仅返回该 map。新建连接窗口根容器 `overflow-hidden`，侧栏顶栏（标题 + 搜索）固定，列表区 `overflow-y-auto` + `DbTypeBadge`。

**Tech Stack:** Vite `?url` imports、Vitest、React、现有 `DbTypeBadge` / `Input` / i18n。

**Spec:** `docs/superpowers/specs/2026-08-09-new-connection-driver-icons-ui-design.md`

## Global Constraints

- 图标路径：`packages/drivers/{id}/ui/icons/{dbType}.svg`（Git 驱动：`.plugins/{id}/ui/icons/`）。
- 解析链不变：主题包 → `DRIVER_ICON_ENTRIES` → shortLabel 占位。
- 搜索：**无可见 label**；`placeholder` + `aria-label` 共用 `newConn.searchDrivers`。
- 匹配：`label` 与 `databaseType` id，不区分大小写子串。
- Host `src/assets/db-icons/*` 迁出后不再作为 `getDriverIconMap` 来源（可删或留 README 指向驱动包）。
- 复用协议类型默认**别名到父图标文件**（不必为每个协议兼容类型画独立 logo）。
- 分支 / worktree：`feat/new-connection-driver-icons-ui`（勿与 `main` 上无关脏改混提）。
- IPC / Overlay 全局策略不改；仅修新建连接布局。

## File map

| File | Responsibility |
|------|----------------|
| `packages/drivers/*/ui/icons/*.svg` | 各 `dbType` 默认角标（Simple Icons CC0 + 色底） |
| `scripts/resolve-drivers.mjs` | 发现 SVG、生成 icon imports + `DRIVER_ICON_ENTRIES` |
| `src/plugins/generated.ts` | 生成物（勿手改；跑 resolve 更新） |
| `src/lib/databaseTypes.ts` | `getDriverIconMap()` ← `DRIVER_ICON_ENTRIES` |
| `src/lib/filterDbTypes.ts` | 纯函数：按 query 过滤驱动列表 |
| `src/lib/__tests__/filterDbTypes.test.ts` | 搜索过滤单测 |
| `src/lib/__tests__/driverIconMap.test.ts` | 图标 map 键覆盖 |
| `src/windows/new-connection/NewConnectionWindow.tsx` | 布局滚动、搜索、`DbTypeBadge` |
| `src/locales/{zh-CN,en,…}.ts` | `newConn.searchDrivers` / `newConn.noDriversMatch` |
| `src/assets/db-icons/README.md` | 指向驱动包图标（可选删除旧 SVG） |

**Icon alias map（同驱动包内文件名，无独立 SVG 时使用）：**

| dbType | 使用文件 |
|--------|----------|
| `questdb`, `cloudberry` | `postgresql.svg` |
| `doris`, `starrocks`, `manticore`, `ob_oracle` | `mysql.svg` |

**必须存在独立 SVG 的 path dbType：**  
`postgresql`, `mysql`, `mariadb`, `sqlite`, `redis`, `mongodb`, `sqlserver`, `clickhouse`, `duckdb`, `elasticsearch`, `rqlite`, `turso`, `influxdb`, `victoriametrics`, `hbase`, `vector`

（Git：`kiwi` / `presto` / `trino` / `superset` — 有文件则生成；无则跳过，UI 走 shortLabel。）

---

### Task 1: 驱动包 SVG 资产

**Files:**
- Create: `packages/drivers/postgres/ui/icons/postgresql.svg`（从 `src/assets/db-icons/postgresql.svg` 复制）
- Create: `packages/drivers/mysql/ui/icons/mysql.svg`, `mariadb.svg`（从 host 复制）
- Create: `packages/drivers/sqlite/ui/icons/sqlite.svg`
- Create: `packages/drivers/redis/ui/icons/redis.svg`
- Create: `packages/drivers/{mongodb,sqlserver,clickhouse,duckdb,elasticsearch,rqlite,turso,influxdb,victoriametrics,hbase,vector}/ui/icons/{same}.svg`
- Modify: `src/assets/db-icons/README.md`（说明已迁移）
- Optional delete after Task 2 green: `src/assets/db-icons/*.svg`

**Interfaces:**
- Produces: 磁盘上上述路径的 SVG 文件（供 Task 2 扫描）
- Consumes: 无

- [ ] **Step 1: 复制已有 5 个 Host 图标到驱动包**

```bash
mkdir -p packages/drivers/postgres/ui/icons \
  packages/drivers/mysql/ui/icons \
  packages/drivers/sqlite/ui/icons \
  packages/drivers/redis/ui/icons
cp src/assets/db-icons/postgresql.svg packages/drivers/postgres/ui/icons/postgresql.svg
cp src/assets/db-icons/mysql.svg packages/drivers/mysql/ui/icons/mysql.svg
cp src/assets/db-icons/mariadb.svg packages/drivers/mysql/ui/icons/mariadb.svg
cp src/assets/db-icons/sqlite.svg packages/drivers/sqlite/ui/icons/sqlite.svg
cp src/assets/db-icons/redis.svg packages/drivers/redis/ui/icons/redis.svg
```

- [ ] **Step 2: 为其余 path 驱动添加品牌角标 SVG**

对每个缺失的 `{driver}/ui/icons/{dbType}.svg`：从 [Simple Icons](https://simpleicons.org/)（CC0）取对应 slug 的 path，套用与现有 `src/assets/db-icons/README.md` 相同的「圆角色底 + 居中白色/对比色 mark」结构（参考 `mysql.svg` / `postgresql.svg` 的 viewBox 与圆角 rect）。品牌色可用各 `meta.ts` 的视觉色近似。

Slug 提示（若站点命名不同则选最接近的官方 mark）：

| dbType | Simple Icons 近似 slug |
|--------|------------------------|
| mongodb | mongodb |
| sqlserver | microsoftsqlserver |
| clickhouse | clickhouse |
| duckdb | （无则用简化鸭/六边形几何 mark，仍放色底） |
| elasticsearch | elasticsearch |
| rqlite | （无则几何 mark） |
| turso | （无则几何 mark） |
| influxdb | influxdb |
| victoriametrics | （无则几何 mark） |
| hbase | apachehbase 或几何 |
| vector | （通用向量几何 mark） |

不要求像素级复刻官网；目标是 24–36px 可辨。

- [ ] **Step 3: 更新 README 指向**

`src/assets/db-icons/README.md` 改为说明默认图标在 `packages/drivers/*/ui/icons/`，由 `resolve-drivers` 注入。

- [ ] **Step 4: Commit**

```bash
git add packages/drivers/*/ui/icons src/assets/db-icons/README.md
git commit -m "$(cat <<'EOF'
feat(drivers): add per-driver db badge SVG icons

EOF
)"
```

---

### Task 2: Codegen `DRIVER_ICON_ENTRIES` + `getDriverIconMap`

**Files:**
- Modify: `scripts/resolve-drivers.mjs` (`generateFrontendRegistry`)
- Modify: `src/plugins/generated.ts`（跑脚本再生）
- Modify: `src/lib/databaseTypes.ts`（去掉硬编码 host imports）
- Modify: `src/lib/__tests__/driverIconMap.test.ts`
- Optional delete: `src/assets/db-icons/*.svg`（若不再被引用）

**Interfaces:**
- Consumes: Task 1 的 SVG 路径
- Produces:
  - `export const DRIVER_ICON_ENTRIES: Record<string, string>` in `generated.ts`
  - `getDriverIconMap(): IconSourceMap` → `{ ...DRIVER_ICON_ENTRIES }`

- [ ] **Step 1: 扩展 `driverIconMap` 测试（先失败）**

替换 `src/lib/__tests__/driverIconMap.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { getDriverIconMap } from '../databaseTypes';

describe('getDriverIconMap', () => {
  it('exposes db.* keys from generated DRIVER_ICON_ENTRIES', () => {
    const map = getDriverIconMap();
    expect(map['db.postgresql']).toMatch(/postgresql/i);
    expect(map['db.mysql']).toBeTruthy();
    expect(map['db.mariadb']).toBeTruthy();
    expect(map['db.redis']).toBeTruthy();
    expect(map['db.mongodb']).toBeTruthy();
    expect(map['db.clickhouse']).toBeTruthy();
    // protocol reuse → parent icon file
    expect(map['db.doris']).toBeTruthy();
    expect(map['db.questdb']).toBeTruthy();
  });
});
```

Run: `npx vitest run src/lib/__tests__/driverIconMap.test.ts`  
Expected: FAIL（尚无 `db.mongodb` 等，或仍只有 5 个硬编码键）

- [ ] **Step 2: 在 `resolve-drivers.mjs` 增加图标发现与生成**

在 `BASIC_PATH_FRONTEND` / 文件顶部附近加入别名表与解析辅助（路径相对于 repo `ROOT`）：

```js
const DRIVER_ICON_ALIASES = {
  questdb: 'postgresql',
  cloudberry: 'postgresql',
  doris: 'mysql',
  starrocks: 'mysql',
  manticore: 'mysql',
  ob_oracle: 'mysql',
};

function driverUiDirFromMetaPath(metaPath) {
  // metaPath like '../../packages/drivers/postgres/ui/meta' (from src/plugins)
  const absMetaTs = resolve(ROOT, 'src/plugins', `${metaPath}.ts`);
  return dirname(absMetaTs);
}

function resolveDriverIconImport(metaPath, dbTypeId) {
  const uiDir = driverUiDirFromMetaPath(metaPath);
  const candidates = [dbTypeId, DRIVER_ICON_ALIASES[dbTypeId]].filter(Boolean);
  for (const name of candidates) {
    const abs = join(uiDir, 'icons', `${name}.svg`);
    if (existsSync(abs)) {
      // import path relative to src/plugins/generated.ts
      const relFromPlugins = relative(resolve(ROOT, 'src/plugins'), abs).replaceAll('\\', '/');
      const importPath = relFromPlugins.startsWith('.') ? relFromPlugins : `./${relFromPlugins}`;
      return { abs, importPath: `${importPath}?url`, fileKey: name };
    }
  }
  return null;
}
```

在 `generateFrontendRegistry` 循环内（meta import 之后）收集图标：

```js
const iconImportLines = [];
const iconEntryLines = [];
const iconImportByAbs = new Map(); // abs -> binding name

// inside for (const id of plugins) { ... for (const dt of cfg.dbTypes) {
const resolved = resolveDriverIconImport(cfg.metaPath, dt.id);
if (resolved) {
  let binding = iconImportByAbs.get(resolved.abs);
  if (!binding) {
    binding = `driverIcon_${resolved.fileKey.replace(/[^a-zA-Z0-9]/g, '_')}`;
    // ensure unique if collision
    let n = binding;
    let i = 2;
    while ([...iconImportByAbs.values()].includes(n)) {
      n = `${binding}_${i++}`;
    }
    binding = n;
    iconImportByAbs.set(resolved.abs, binding);
    iconImportLines.push(`import ${binding} from '${resolved.importPath}';`);
  }
  iconEntryLines.push(`  'db.${dt.id}': ${binding},`);
}
// } }
```

将 `iconImportLines` 拼进生成文件顶部（与 meta imports 一起），并在 `DRIVER_DB_ENTRIES` 后输出：

```js
/** Default driver badge icon URLs keyed by semantic id (`db.<type>`). */
export const DRIVER_ICON_ENTRIES: Record<string, string> = {
${iconEntryLines.join('\n')}
};
```

确保 `import { existsSync } from 'fs'` / `dirname` / `relative` / `join` 已从 `node:fs` / `node:path` 引入（脚本若已有则复用）。

- [ ] **Step 3: 再生 generated + 改 `getDriverIconMap`**

```bash
node scripts/resolve-drivers.mjs
# 或项目惯用：DATAZEN_DRIVERS=all node scripts/resolve-drivers.mjs
# 以当前仓库默认选型为准，需包含 mongodb/clickhouse 等 path 驱动
```

`src/lib/databaseTypes.ts`：

```ts
import { DRIVER_DB_ENTRIES, DRIVER_ICON_ENTRIES } from '../plugins/generated';
// 删除 postgresqlIconUrl 等五条 host ?url import

export function getDriverIconMap(): IconSourceMap {
  return { ...DRIVER_ICON_ENTRIES };
}
```

确认无其它文件仍 import `src/assets/db-icons/*.svg`；若无引用可删除这些 SVG。

- [ ] **Step 4: 跑测试通过**

```bash
npx vitest run src/lib/__tests__/driverIconMap.test.ts
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/resolve-drivers.mjs src/plugins/generated.ts src/lib/databaseTypes.ts \
  src/lib/__tests__/driverIconMap.test.ts src/assets/db-icons
git commit -m "$(cat <<'EOF'
feat(drivers): generate DRIVER_ICON_ENTRIES from package SVGs

EOF
)"
```

---

### Task 3: 搜索过滤纯函数 + i18n

**Files:**
- Create: `src/lib/filterDbTypes.ts`
- Create: `src/lib/__tests__/filterDbTypes.test.ts`
- Modify: `src/locales/zh-CN.ts`, `en.ts`, and remaining locale files (`de`, `es`, `fr`, `ja`, `ko`, `pt-BR`, `ru`, `zh-TW`) — 在 `'newConn.selectDbType'` 附近插入两键

**Interfaces:**
- Produces:
  - `export function filterDbTypesByQuery<T extends { value: string; label: string }>(items: T[], query: string): T[]`
  - i18n: `newConn.searchDrivers`, `newConn.noDriversMatch`

- [ ] **Step 1: 写失败测试**

`src/lib/__tests__/filterDbTypes.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { filterDbTypesByQuery } from '../filterDbTypes';

const items = [
  { value: 'postgresql', label: 'PostgreSQL' },
  { value: 'mysql', label: 'MySQL' },
  { value: 'mongodb', label: 'MongoDB' },
];

describe('filterDbTypesByQuery', () => {
  it('returns all when query empty/whitespace', () => {
    expect(filterDbTypesByQuery(items, '')).toEqual(items);
    expect(filterDbTypesByQuery(items, '  ')).toEqual(items);
  });

  it('matches label case-insensitively', () => {
    expect(filterDbTypesByQuery(items, 'mongo')).toEqual([items[2]]);
  });

  it('matches databaseType id', () => {
    expect(filterDbTypesByQuery(items, 'SQL')).toEqual([items[0], items[1]]);
  });
});
```

Run: `npx vitest run src/lib/__tests__/filterDbTypes.test.ts`  
Expected: FAIL（module missing）

- [ ] **Step 2: 实现**

`src/lib/filterDbTypes.ts`:

```ts
export function filterDbTypesByQuery<T extends { value: string; label: string }>(
  items: T[],
  query: string,
): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter(
    (item) =>
      item.label.toLowerCase().includes(q) || item.value.toLowerCase().includes(q),
  );
}
```

Run 同上 → PASS

- [ ] **Step 3: i18n keys**

`zh-CN.ts`（`newConn.selectDbType` 后）：

```ts
  'newConn.searchDrivers': '搜索驱动…',
  'newConn.noDriversMatch': '无匹配驱动',
```

`en.ts`:

```ts
  'newConn.searchDrivers': 'Search drivers…',
  'newConn.noDriversMatch': 'No matching drivers',
```

其余语系：可暂用英文文案或简短本地化，但**必须**加入同名 key（`TranslationKey` 以 `zh-CN` 为准）。

- [ ] **Step 4: Commit**

```bash
git add src/lib/filterDbTypes.ts src/lib/__tests__/filterDbTypes.test.ts src/locales
git commit -m "$(cat <<'EOF'
feat(new-conn): add driver list filter helper and i18n

EOF
)"
```

---

### Task 4: 新建连接侧栏布局、搜索、DbTypeBadge

**Files:**
- Modify: `src/windows/new-connection/NewConnectionWindow.tsx`

**Interfaces:**
- Consumes: `filterDbTypesByQuery`, `DbTypeBadge`, `Input`, i18n keys from Task 3；图标来自 Task 2 解析链

- [ ] **Step 1: 改布局与列表（完整目标结构）**

将根/侧栏改为（保留现有 store/form 逻辑；删除 `lucide-react` 的 `Database` import）：

```tsx
import { useEffect, useMemo, useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getUrlParam } from '../../lib/windowKind';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { filterDbTypesByQuery } from '../../lib/filterDbTypes';
import { connectionCommands } from '../../commands/connection';
import { ConnectionFormBody } from '../../components/connection/ConnectionFormBody';
import { useConnectionForm } from '../../components/connection/useConnectionForm';
import type { DatabaseType } from '../../types';

// ... ALL_DB_TYPES, closeWindow 不变 ...

export function NewConnectionWindow() {
  // ... existing hooks ...
  const [driverQuery, setDriverQuery] = useState('');

  const dbTypes = useMemo(() => {
    const available = !availableDrivers
      ? ALL_DB_TYPES
      : ALL_DB_TYPES.filter((db) => availableDrivers.includes(db.value));
    return filterDbTypesByQuery(available, driverQuery);
  }, [availableDrivers, driverQuery]);

  return (
    <div className="flex h-screen min-h-0 flex-col overflow-hidden bg-surface-alt text-fg">
      <TitleBar title={editId ? t('newConn.editTitle') : t('newConn.title')} />

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[220px] shrink-0 min-h-0 flex-col border-r border-edge bg-surface">
          <div className="shrink-0 space-y-2 p-4 pb-2">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              {t('newConn.selectDbType')}
            </div>
            <Input
              value={driverQuery}
              onChange={(e) => setDriverQuery(e.target.value)}
              placeholder={t('newConn.searchDrivers')}
              aria-label={t('newConn.searchDrivers')}
              className="h-8 text-sm"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
            <div className="flex flex-col gap-0.5">
              {dbTypes.length === 0 ? (
                <div className="px-2.5 py-2 text-sm text-fg-muted">
                  {t('newConn.noDriversMatch')}
                </div>
              ) : (
                dbTypes.map((db) => (
                  <button
                    key={db.value}
                    type="button"
                    onClick={() => form.handleDatabaseTypeChange(db.value)}
                    className={cn(
                      'flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors select-none',
                      form.databaseType === db.value
                        ? 'bg-surface-raised text-fg'
                        : 'text-fg-secondary hover:bg-surface-alt hover:text-fg',
                    )}
                  >
                    <DbTypeBadge databaseType={db.value} size={24} />
                    <div className="font-medium">{db.label}</div>
                  </button>
                ))
              )}
            </div>
          </div>
        </aside>

        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto p-6">
            {/* connectionConfig + ConnectionFormBody 不变 */}
          </div>
          <footer className="flex shrink-0 ...">{/* 不变 */}</footer>
        </main>
      </div>
    </div>
  );
}
```

要点核对：
- 根：`overflow-hidden`（防整页滚到 TitleBar 下）。
- aside：`min-h-0 flex-col`；仅列表容器 `overflow-y-auto`。
- 搜索：无额外 label 节点。
- 图标：`DbTypeBadge`，不用 Lucide `Database`。

- [ ] **Step 2: 类型检查 / 单测**

```bash
npx vitest run src/lib/__tests__/filterDbTypes.test.ts src/lib/__tests__/driverIconMap.test.ts
pnpm exec tsc --noEmit -p tsconfig.json
# 若仓库惯用其它检查命令，以 CI 为准；至少保证上述 vitest 通过
```

Expected: PASS / 无本改动引入的 TS 错误

- [ ] **Step 3: 手工核对清单（macOS Overlay）**

1. `pnpm tauri:dev`（或当前 worktree 等价命令）打开新建连接。
2. 侧栏可独立滚动；右侧表单独立滚动；窗口本身不整体垂直滚。
3. traffic lights 背后无列表文字/图标透出。
4. 各驱动显示品牌角标（有 SVG 时）；协议复用类型显示父图标。
5. 搜索「mongo」只剩 MongoDB；清空恢复；乱输显示「无匹配驱动」。
6. 搜索框上方无「搜索」类 label，仅 placeholder。

- [ ] **Step 4: Commit**

```bash
git add src/windows/new-connection/NewConnectionWindow.tsx
git commit -m "$(cat <<'EOF'
feat(new-conn): sidebar scroll, driver search, and DbTypeBadge

EOF
)"
```

---

### Task 5: 文档收尾

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-new-connection-driver-icons-ui-design.md`（状态 → 已实现；指向本 plan）
- Modify: `AGENTS.md` 仅当「驱动图标路径」需一笔带过时（可选短句：`packages/drivers/*/ui/icons` + `DRIVER_ICON_ENTRIES`）

- [ ] **Step 1: 更新 spec 状态与 plan 链接**
- [ ] **Step 2: Commit**

```bash
git commit -m "$(cat <<'EOF'
docs: mark new-connection driver icons design implemented

EOF
)"
```

---

## Spec coverage checklist

| Spec 要求 | Task |
|-----------|------|
| 驱动包 SVG + 方案 A | 1–2 |
| `DRIVER_ICON_ENTRIES` / `getDriverIconMap` | 2 |
| 协议复用别名 | 1–2（`DRIVER_ICON_ALIASES`） |
| `DbTypeBadge` 列表 | 4 |
| 侧栏独立滚动 / 整页不滚 / traffic lights | 4 |
| 搜索无 label、仅 placeholder | 3–4 |
| i18n keys | 3 |
| 单元测试图标 + 过滤 | 2–3 |

## Self-review notes

- 无 TBD；SVG 内容以「复制现有 + Simple Icons 色底」为可执行说明，非空占位。
- `filterDbTypesByQuery` 与窗口组件签名一致（`value` + `label`）。
- generated 文件必须经脚本再生，禁止手填长期分叉。
