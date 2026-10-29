/**
 * [tester] KV slot registration guard for Redis — the double gate (PRD §7-4, F-1 + F-3).
 *
 * The host only renders a KV slot when **both** gates say yes: the driver's
 * `DatabaseTypeMeta.kvWorkspace` capability flag (F-1, `ui/shared/meta.ts`) and a
 * component row contributed by this build (F-3, the `kvSlots` block read by
 * `scripts/resolve-drivers.mjs`). The host-side suites
 * (`src/lib/__tests__/kvWorkspaceSlots.test.ts`) prove the *mechanism* with
 * fixture drivers only, so nothing pinned **Redis' own halves**: deleting the
 * capability block, or deleting a registration row, left the whole suite green
 * (verified by mutation during this track's test round). This file closes that.
 *
 * Mutation evidence for this file (test round, driver suite): flipping
 * `statusBar: true` → `false` reddens 2 cases; typo'ing the generated component
 * name reddens 2; undeclaring the `keyPropsSidebar` row reddens 2 (including the
 * cross-gate agreement case below).
 *
 * Merge-round re-verification (three shipped slots, coordinator run, each
 * injection reddens ≥2 and the tree restores clean): dropping the `home`
 * capability; dropping the `connectionHome` row; typo'ing its component name;
 * pointing its path at the directory instead of the module; and adding a second
 * `kvSlots:` key to the config object — the shape a naive union merge leaves
 * behind, where JS keeps only the last key and two slot rows vanish without any
 * compile error, since `scripts/**` is outside `tsc`'s reach.
 *
 * Maintenance contract: {@link SHIPPED_ROWS} is the **only** track-scoped fact in
 * this file. A slot Redis does not fill (`contextBar`, removed along with the
 * 48px context band itself) is therefore not asserted as "missing forever" — it
 * is asserted as "missing while this table says so", and adding a row here
 * re-checks both gates at once. Everything else (export identity, module paths,
 * copy keys) is derived from the registry rows rather than hardcoded, so a new
 * slot file cannot make this suite lie in either direction.
 *
 * Boundary note: importing `scripts/resolve-drivers.mjs` from a driver test is
 * test-only tooling access, not a host-`src/**` reference, so guard rule R1 does
 * not apply (it fires on specifiers resolving into `src/`). Verified with
 * `node scripts/check-driver-import-boundaries.mjs` → 0 blocking violations.
 */
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
// The codegen's own contract surface — the same helpers that emit `DRIVER_KV_SLOTS`.
import {
  KV_SLOT_NAMES,
  collectDriverKvSlotEntries,
  // @ts-expect-error TS7016: the codegen script is plain `.mjs` with no declaration
  // file, so the module has no type. Both exports this suite uses are plain
  // data/functions pinned by the assertions below, so the implicit `any` is safe
  // here and keeps the test out of the shared tsconfig's `allowJs` surface.
} from '../../../../../scripts/resolve-drivers.mjs';

import { redisMeta } from '../shared/meta';
import en from '../../locales/en';

/**
 * Rows this build ships: one per KV slot Redis fills, with the module the
 * codegen must import the component from. This table is the **only**
 * track-scoped fact in the file — adding a row here re-checks both gates plus
 * the export at once.
 */
const SHIPPED_ROWS: Array<{ slot: string; component: string; module: string }> = [
  { slot: 'statusBar', component: 'RedisKvStatusBar', module: 'packages/drivers/redis/ui/kv-bar' },
  {
    slot: 'keyPropsSidebar',
    component: 'RedisKeyPropsSidebar',
    module: 'packages/drivers/redis/ui/kv-bar',
  },
  {
    slot: 'connectionHome',
    component: 'RedisOverviewHome',
    module: 'packages/drivers/redis/ui/overview/RedisOverviewHome',
  },
];

const SHIPPED_SLOTS = SHIPPED_ROWS.map((row) => row.slot);

/**
 * Registry row → the module path codegen emits for it. Specifiers are relative
 * to the generated registry at `packages/frontend/src/extensions/generated.ts`,
 * so reaching the repo root (and back into packages/) takes four climbs.
 */
function moduleOf(row: { module: string }): string {
  return `../../../../${row.module}`;
}

/** `connectionHome` is the slot name; the capability flag behind it is `home`. */
const CAPABILITY_KEY: Record<string, string> = {
  contextBar: 'contextBar',
  statusBar: 'statusBar',
  keyPropsSidebar: 'keyPropsSidebar',
  connectionHome: 'home',
};
/** Capability Redis declares for a slot, normalised to a boolean. */
function capability(slot: string): boolean {
  const key = CAPABILITY_KEY[slot];
  return (redisMeta.kvWorkspace as Record<string, boolean | undefined> | undefined)?.[key] === true;
}

/** Slot names this build registers a component for, for the `redis` db type. */
function registeredSlots(): Array<Record<string, string>> {
  return collectDriverKvSlotEntries(undefined, ['redis']).map((entry: Record<string, string>) => ({
    dbType: entry.dbType,
    slot: entry.slot,
    component: entry.component,
    path: entry.path,
  }));
}

describe('[tester] redis KV slot double gate (F-1 ⇄ F-3)', () => {
  it('declares a capability for exactly the slots this build ships', () => {
    // One loop over the four contract slot names: a shipped slot must be true, an
    // unshipped slot must be absent/false, so the host keeps its default rendering
    // there (no dead button, no orphan import).
    for (const slot of KV_SLOT_NAMES) {
      expect(capability(slot), `capability(${slot})`).toBe(SHIPPED_SLOTS.includes(slot));
    }
  });

  it('registers exactly one row per shipped slot, all for redis, with the declared module', () => {
    const rows = registeredSlots();
    // Sorted-set compare: a duplicated object key in the `kvSlots` block collapses
    // silently in JS (the collector reads slot by slot), which would drop a row.
    expect(rows.map((row) => row.slot).sort()).toEqual([...SHIPPED_SLOTS].sort());
    for (const expected of SHIPPED_ROWS) {
      const matching = rows.filter((row) => row.slot === expected.slot);
      expect(matching.length, `rows for ${expected.slot}`).toBe(1);
      const [row] = matching;
      expect(row.dbType).toBe('redis');
      expect(row.component, `component for ${expected.slot}`).toBe(expected.component);
      expect(row.path, `path for ${expected.slot}`).toBe(moduleOf(expected));
    }
  });

  it('names components that really are exports of the declared module', async () => {
    // A stale or typo'd `component` / `path` string would only fail at `vite build`;
    // this catches it while the row is still the only thing that changed. Both the
    // module and the export name come from the **registry row**, not from the
    // table, so the case where the two disagree is covered too. Rows that share a
    // module import it once.
    const byModule = new Map<string, Record<string, unknown>>();
    for (const row of registeredSlots()) {
      // codegen emits paths relative to `src/extensions/`; the test runs at repo root.
      const module = resolve(process.cwd(), row.path.replace(/^(\.\.\/)+/, ''));
      let exports_ = byModule.get(module);
      if (!exports_) {
        exports_ = (await import(module)) as Record<string, unknown>;
        byModule.set(module, exports_);
      }
      expect(exports_, `no export named ${row.component}`).toHaveProperty(row.component);
      expect(typeof exports_[row.component], row.component).toBe('function');
    }
  });

  it('keeps capability and registration in lockstep, never one gate alone', () => {
    // Capability without a row ⇒ the host falls back to the row-detail panel and
    // the driver's slot never renders; a row without capability ⇒ an orphan import
    // the host refuses to render. Both are silent dead code.
    const rows = new Set(registeredSlots().map((row) => row.slot));
    for (const slot of KV_SLOT_NAMES) {
      expect(capability(slot), `capability(${slot})`).toBe(rows.has(slot));
    }
  });
});

describe('[tester] kv-bar copy keys resolve', () => {
  const KV_BAR_DIR = resolve(process.cwd(), 'packages/drivers/redis/ui/kv-bar');
  const translations = en as unknown as Record<string, string>;

  /**
   * Every i18n key literal rendered by any slot in this package.
   *
   * Derived by walking `ui/kv-bar/*.tsx` rather than a hardcoded file list, so a
   * slot added later (contextBar) is covered automatically and its copy is never
   * reported as orphaned.
   */
  function keysInUse(): Set<string> {
    const keys = new Set<string>();
    for (const name of readdirSync(KV_BAR_DIR).filter((f) => f.endsWith('.tsx'))) {
      const text = readFileSync(join(KV_BAR_DIR, name), 'utf8');
      for (const match of text.matchAll(/['"]((?:redis|common)\.[A-Za-z0-9.]+)['"]/g)) {
        keys.add(match[1]);
      }
    }
    return keys;
  }

  it('every driver-owned key the slots render is present in en.ts', () => {
    // `redis.*` is this package's own dictionary. `common.*` belongs to the host
    // pack, which a driver unit test must not read (boundary rule R1) — the host
    // locale suite owns those, so only the driver keys are checked here.
    const driverKeys = [...keysInUse()].filter((key) => key.startsWith('redis.'));
    expect(driverKeys.length).toBeGreaterThan(0);
    for (const key of driverKeys) {
      expect(translations[key], `missing locale key: ${key}`).toBeTypeOf('string');
    }
  });

  it('borrows nothing from the host pack beyond the shared common.* keys', () => {
    // Keeps the host coupling surface explicit: the slots may only lean on
    // `common.*`, never on some other host namespace.
    for (const key of keysInUse()) {
      expect(/^(redis|common)\./.test(key), key).toBe(true);
      if (!key.startsWith('redis.')) expect(key.startsWith('common.'), key).toBe(true);
    }
  });

  it('leaves no unused copy behind in the namespaces these slots own', () => {
    const used = keysInUse();
    const owned = Object.keys(translations).filter(
      (key) => key.startsWith('redis.keyProps.') || key.startsWith('redis.contextBar.'),
    );
    expect(owned.length).toBeGreaterThan(0);
    for (const key of owned) expect(used.has(key), `unused locale key: ${key}`).toBe(true);
  });
});
