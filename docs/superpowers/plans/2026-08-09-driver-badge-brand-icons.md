# Driver Badge Brand Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prefer per-`dbType` brand SVGs; when missing, show parent driver icon + bottom-right `shortLabel` instead of silently aliasing the whole badge to the parent.

**Architecture:** `resolve-drivers.mjs` stops writing alias URLs into `DRIVER_ICON_ENTRIES` and emits `DRIVER_ICON_PARENTS` only for protocol-reuse types that lack their own SVG. `DbTypeBadge` composites parent URL + shortLabel at runtime. Brand SVGs are added under each driver package when legally available.

**Tech Stack:** TypeScript/React, Vitest, Node `resolve-drivers.mjs`, SVG badge assets (24×24).

**Spec:** [docs/superpowers/specs/2026-08-09-driver-badge-brand-icons-design.md](../specs/2026-08-09-driver-badge-brand-icons-design.md)

## Global Constraints

- Git stubs: `generated.ts` must keep empty `DRIVER_ICON_ENTRIES` and empty `DRIVER_ICON_PARENTS`; never commit injected maps.
- Do not invent trademark lookalikes; if no brand mark, leave parent+shortLabel.
- Theme pack still wins for `db.<type>` URL resolution; overlay only when own type has no URL.

---

### Task 1: Generator + stub — `DRIVER_ICON_PARENTS`

**Files:**
- Modify: `scripts/resolve-drivers.mjs` (`DRIVER_ICON_ALIASES` → parent map; `generateFrontendRegistry`; stub template)
- Modify: `src/plugins/generated.ts` (stub export empty `DRIVER_ICON_PARENTS`)
- Modify: `src/lib/databaseTypes.ts` (`getDriverIconParents`)
- Modify: `src/lib/__tests__/driverIconMap.test.ts`
- Modify: `src/assets/db-icons/README.md` (one-line note on parents)

**Interfaces:**
- Produces: `export const DRIVER_ICON_PARENTS: Record<string, string>`
- Produces: `getDriverIconParents(): Record<string, string>`
- Consumes: disk `packages/drivers/*/ui/icons/{dbType}.svg`

- [ ] **Step 1: Update failing expectations in `driverIconMap.test.ts`**

Replace the alias path assertions with:

```ts
import { DRIVER_ICON_ENTRIES, DRIVER_ICON_PARENTS } from '../../plugins/generated';
import { getDriverIconMap, getDriverIconParents } from '../databaseTypes';

describe('getDriverIconParents', () => {
  it('returns generated DRIVER_ICON_PARENTS', () => {
    expect(getDriverIconParents()).toEqual({ ...DRIVER_ICON_PARENTS });
  });
});

describe('getDriverIconMap', () => {
  it('returns generated DRIVER_ICON_ENTRIES', () => {
    expect(getDriverIconMap()).toEqual({ ...DRIVER_ICON_ENTRIES });
  });

  it('includes basic driver icons from active build', () => {
    const map = getDriverIconMap();
    for (const key of BASIC_DRIVER_ICON_KEYS) {
      expect(map[key], key).toBeTruthy();
    }
  });

  it('does not silently alias reuse types onto parent SVG paths', () => {
    const map = getDriverIconMap();
    const parents = getDriverIconParents();
    // Either own brand file…
    if (map['db.doris']) {
      expect(map['db.doris']).toMatch(/doris/i);
      expect(parents.doris).toBeUndefined();
    } else {
      // …or parent mapping for composite badge
      expect(parents.doris).toBe('mysql');
      expect(map['db.doris']).toBeUndefined();
    }
    if (map['db.questdb']) {
      expect(map['db.questdb']).toMatch(/questdb/i);
      expect(parents.questdb).toBeUndefined();
    } else {
      expect(parents.questdb).toBe('postgresql');
      expect(map['db.questdb']).toBeUndefined();
    }
  });
});
```

- [ ] **Step 2: Run with inject to see current failure mode**

Run: `node scripts/resolve-drivers.mjs --drivers=basic && pnpm exec vitest run src/lib/__tests__/driverIconMap.test.ts`
Expected: FAIL on doris/questdb alias assertions (old test) or missing `DRIVER_ICON_PARENTS` after Step 1 edit.

Restore after: `pnpm drivers:restore` (or rely on later inject/restore).

- [ ] **Step 3: Implement generator**

In `resolve-drivers.mjs`:

1. Rename `DRIVER_ICON_ALIASES` → `DRIVER_ICON_PARENT` (same key→parentDbType values).
2. Change `resolveDriverIconImport` to only look for `${dbTypeId}.svg` (no parent file fallback).
3. In the dbTypes loop:
   - if own SVG resolved → `DRIVER_ICON_ENTRIES` as today
   - else if `DRIVER_ICON_PARENT[dt.id]` → push parent entry for `DRIVER_ICON_PARENTS`
4. Emit in generated template:

```ts
export const DRIVER_ICON_PARENTS: Record<string, string> = {
${parentEntryLines.join('\n')}
};
```

5. Stub mode / empty plugins: empty object for both maps.
6. Update canonical stub in `src/plugins/generated.ts` the same way (empty `DRIVER_ICON_PARENTS`).

`databaseTypes.ts`:

```ts
import { DRIVER_DB_ENTRIES, DRIVER_ICON_ENTRIES, DRIVER_ICON_PARENTS } from '../plugins/generated';

export function getDriverIconParents(): Record<string, string> {
  return { ...DRIVER_ICON_PARENTS };
}
```

- [ ] **Step 4: Verify**

Run:

```bash
node scripts/resolve-drivers.mjs --drivers=stub
# stub must include empty DRIVER_ICON_PARENTS
node scripts/resolve-drivers.mjs --drivers=basic
pnpm exec vitest run src/lib/__tests__/driverIconMap.test.ts
pnpm drivers:restore
```

Expected: PASS; with basic inject and no new SVGs yet, `parents.doris === 'mysql'` and no `map['db.doris']`.

- [ ] **Step 5: Commit**

```bash
git add scripts/resolve-drivers.mjs src/plugins/generated.ts src/lib/databaseTypes.ts \
  src/lib/__tests__/driverIconMap.test.ts src/assets/db-icons/README.md
git commit -m "$(cat <<'EOF'
feat(drivers): emit DRIVER_ICON_PARENTS instead of silent icon aliases

EOF
)"
```

---

### Task 2: `DbTypeBadge` parent + shortLabel composite

**Files:**
- Modify: `src/components/DbTypeBadge.tsx`
- Create: `src/components/__tests__/DbTypeBadge.test.tsx`

**Interfaces:**
- Consumes: `getDriverIconParents()`, `getDbIcon()`, `IconResolver.resolve`
- Produces: composite badge UI when own resolve is placeholder and parent URL exists

- [ ] **Step 1: Failing tests**

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DbTypeBadge } from '../DbTypeBadge';
import type { IconResolver } from '../../lib/iconResolver';

vi.mock('../../lib/databaseTypes', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/databaseTypes')>();
  return {
    ...actual,
    getDriverIconParents: () => ({ doris: 'mysql', questdb: 'postgresql' }),
    getDbIcon: (dbType: string) => {
      if (dbType === 'doris') return { label: 'Do', bg: 'bg-cyan-700' };
      if (dbType === 'questdb') return { label: 'Qd', bg: 'bg-rose-700' };
      return { label: 'DB', bg: 'bg-slate-600' };
    },
  };
});

function resolver(map: Record<string, string>): IconResolver {
  return {
    resolve(id) {
      const href = map[id];
      if (href) return { kind: 'url', href };
      return { kind: 'placeholder', label: '?', bgClass: 'bg-slate-600' };
    },
  };
}

describe('DbTypeBadge', () => {
  it('renders own brand url without shortLabel overlay', () => {
    const { container } = render(
      <DbTypeBadge
        databaseType="doris"
        resolver={resolver({ 'db.doris': '/doris.svg', 'db.mysql': '/mysql.svg' })}
      />,
    );
    expect(container.querySelector('img')?.getAttribute('src')).toBe('/doris.svg');
    expect(screen.queryByText('Do')).toBeNull();
  });

  it('composites parent icon with shortLabel when own url missing', () => {
    render(
      <DbTypeBadge
        databaseType="doris"
        resolver={resolver({ 'db.mysql': '/mysql.svg' })}
      />,
    );
    expect(screen.getByRole('img', { hidden: true }) /* or query img */ || document.querySelector('img')).toBeTruthy();
    expect(screen.getByText('Do')).toBeTruthy();
  });
});
```

Adjust queries to match implementation (`img` + text node). Prefer `container.querySelector('img')` + `getByText('Do')`.

- [ ] **Step 2: Run — expect FAIL**

`pnpm exec vitest run src/components/__tests__/DbTypeBadge.test.tsx`

- [ ] **Step 3: Implement `DbTypeBadge`**

When `resolved.kind !== 'url'`:
1. `parentId = getDriverIconParents()[databaseType]`
2. If parentId, `parent = resolver.resolve('db.' + parentId)`
3. If `parent.kind === 'url'`, render:

```tsx
<span className={cn('relative inline-flex shrink-0 ...', className)} style={dimensionStyle} aria-hidden>
  <img src={parent.href} alt="" className="h-full w-full rounded-lg object-contain shadow-sm" draggable={false} />
  <span
    className={cn(
      'absolute -bottom-0.5 -right-0.5 flex items-center justify-center rounded px-0.5 font-bold leading-none text-white shadow',
      getDbIcon(databaseType as DatabaseType).bg,
    )}
    style={{ fontSize: Math.min(11, Math.max(8, Math.round(size * 0.38))) }}
  >
    {getDbIcon(databaseType as DatabaseType).label}
  </span>
</span>
```

Else keep existing placeholder branch.

- [ ] **Step 4: Run tests PASS**

`pnpm exec vitest run src/components/__tests__/DbTypeBadge.test.tsx src/lib/__tests__/driverIconMap.test.ts`

- [ ] **Step 5: Commit**

```bash
git add src/components/DbTypeBadge.tsx src/components/__tests__/DbTypeBadge.test.tsx
git commit -m "$(cat <<'EOF'
feat(ui): composite parent driver icon with shortLabel badge

EOF
)"
```

---

### Task 3: Brand SVG assets for reuse types

**Files:**
- Create: `packages/drivers/postgres/ui/icons/questdb.svg`
- Create: `packages/drivers/postgres/ui/icons/cloudberry.svg`
- Create: `packages/drivers/mysql/ui/icons/doris.svg`
- Create: `packages/drivers/mysql/ui/icons/starrocks.svg`
- Create: `packages/drivers/mysql/ui/icons/manticore.svg`
- Create: `packages/drivers/mysql/ui/icons/ob_oracle.svg`
- Modify: `docs/superpowers/specs/2026-08-09-new-connection-driver-icons-ui-design.md` (note superseded reuse policy → link new spec)

**Interfaces:**
- After inject, these types appear in `DRIVER_ICON_ENTRIES` and **not** in `DRIVER_ICON_PARENTS`.

- [ ] **Step 1: Source marks**

Prefer official brand SVG marks; wrap in the same structure as `mariadb.svg` / `postgresql.svg` (rounded rect + centered white/light path). If a mark cannot be obtained legally, skip that file (parent overlay remains).

- [ ] **Step 2: Add files + verify inject**

```bash
node scripts/resolve-drivers.mjs --drivers=basic
node -e "const g=require('fs').readFileSync('src/plugins/generated.ts','utf8'); console.log(/db\\.doris/.test(g), /DRIVER_ICON_PARENTS[\\s\\S]*doris/.test(g));"
pnpm exec vitest run src/lib/__tests__/driverIconMap.test.ts
pnpm drivers:restore
```

Expected: `db.doris` in entries; `doris` not in parents (if SVG added).

- [ ] **Step 3: Commit**

```bash
git add packages/drivers/postgres/ui/icons packages/drivers/mysql/ui/icons \
  docs/superpowers/specs/2026-08-09-new-connection-driver-icons-ui-design.md
git commit -m "$(cat <<'EOF'
feat(drivers): add brand badge SVGs for protocol-reuse db types

EOF
)"
```

---

### Task 4: Spec status + final verification

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-driver-badge-brand-icons-design.md` (状态 → 已实现；链到本 plan)

- [ ] **Step 1: Mark spec implemented, link plan**
- [ ] **Step 2: Final unit tests**

```bash
pnpm exec vitest run src/lib/__tests__/driverIconMap.test.ts src/components/__tests__/DbTypeBadge.test.tsx src/lib/__tests__/iconResolver.test.ts
node scripts/check-managed-stubs.mjs
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-08-09-driver-badge-brand-icons-design.md \
  docs/superpowers/plans/2026-08-09-driver-badge-brand-icons.md
git commit -m "$(cat <<'EOF'
docs: mark driver badge brand icons design implemented

EOF
)"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| No silent alias into `DRIVER_ICON_ENTRIES` | 1 |
| `DRIVER_ICON_PARENTS` generation rules | 1 |
| `getDriverIconParents` | 1 |
| Runtime parent + shortLabel | 2 |
| Brand SVGs when available | 3 |
| Stub / CI stub safety | 1, 4 |
| Docs update | 1, 3, 4 |
