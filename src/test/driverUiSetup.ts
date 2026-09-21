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
import '../locales';
import '../../packages/drivers/redis/ui/shared/meta';
import '../../packages/drivers/mongodb/ui/meta';
