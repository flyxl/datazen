/**
 * i18n assembly for the path-driver UI suites (`vitest.drivers.config.ts`).
 *
 * Driver components read translations from the single shared `@datazen/ui`
 * registry and never import the host, so a driver test run has to register
 * dictionaries the same way the running app does:
 *   1. host eager dictionaries — loaded by the host app itself
 *      (`src/locales/index.ts` registers them on import);
 *   2. each driver pack — registered as a *side effect of the driver UI entry
 *      module* (`ui/shared/meta.ts` for redis, `ui/meta.ts` for mongodb),
 *      which is exactly the module `src/extensions/generated.ts` imports in
 *      the real app. The entry module in turn pulls `packages/drivers/<id>/
 *      locales/index.ts`.
 *
 * The harness deliberately goes through the meta entry and never imports a
 * driver `locales/index.ts` directly: mounting the pack on the real loading
 * path is what this track guarantees, and `ui/__tests__/localePackRegistration
 * .test.ts` per driver keeps that link permanently covered. Importing the
 * pack here instead would hide a broken/removed side-effect line in meta.
 *
 * Host-only test runs keep using `./setup.ts`; this file exists so the driver
 * suites get real strings without any `src/**` import inside `packages/drivers`.
 */
import { vi } from 'vitest';

/*
 * jsdom has no layout, so a scroll element measures 0×0 and the real
 * `useVirtualizer` correctly renders **no** rows. Every virtualized tree
 * therefore needs a stub, and thirteen Redis suites each carried their own copy.
 *
 * That duplication is not the interesting part — the *ordering* is. The imports
 * below reach `@datazen/ui`, which is where the shared `VirtualTree` shell
 * lives, and a setup file is evaluated before any suite's own `vi.mock` is
 * registered. A suite that mocked `@tanstack/react-virtual` therefore got the
 * mock for its own import while the shell — already evaluated above — kept the
 * real one, and the tree rendered an empty grid. `vi.mock` is hoisted above the
 * imports, so declaring it here closes that window: from the first module in
 * the graph, every consumer sees the stub.
 *
 * The stub hands back every row, which is what a small tree needs anyway, and
 * it is deliberately the same shape the per-suite copies use. Suites that
 * declare their own mock still win — theirs is registered later — so nothing
 * that already worked changes.
 */
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (opts: { count: number; estimateSize: () => number }) => {
    const size = opts.estimateSize();
    const items = Array.from({ length: opts.count }, (_, index) => ({
      index,
      key: index,
      start: index * size,
      size,
      lane: 0,
    }));
    return {
      getVirtualItems: () => items,
      getTotalSize: () => items.length * size,
      measureElement: () => undefined,
      scrollToOffset: () => undefined,
      scrollToIndex: () => undefined,
    };
  },
}));

import '../locales';
import '../../../../packages/drivers/redis/ui/shared/meta';
import '../../../../packages/drivers/mongodb/ui/meta';
