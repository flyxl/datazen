# Host Connection Contract

Driver-agnostic Host UI/IPC journeys parameterized by {@link DriverFixtureDefinition}.

| Path | Role |
|------|------|
| `fixtures.ts` | Fixture definitions, dialect seed SQL, capability gating (unit-tested) |
| `journeys/plan.ts` | Journey planning + pure assertion helpers (unit-tested) |
| `journeys/run-core.ts` | WDIO runners for HC-DATA / HC-FILTER / HC-QUERY |
| `open-fixture.ts` | Open PG / MySQL / SQLite connection windows |
| `../specs/host-contract-matrix.ts` | Matrix: each driver window × planned journeys |

Run unit gate:

```bash
pnpm test:unit:e2e-contract:coverage
# or
npx vitest run --coverage --config vitest.e2e-contract.config.ts
```

Run matrix E2E (requires webdriver debug build):

```bash
pnpm e2e:contract:matrix
pnpm e2e:contract:pg
```
