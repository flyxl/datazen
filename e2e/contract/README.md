# Host Connection Contract

Driver-agnostic Host UI/IPC journeys parameterized by {@link DriverFixtureDefinition}.

| Path | Role |
|------|------|
| `fixtures.ts` | Fixture definitions, dialect seed SQL, capability gating (unit-tested) |
| `journeys/` | Reusable journey runners (F2+) |
| `matrix*.ts` | WDIO entry that opens each driver window and runs journeys (F2+) |

Run unit gate:

```bash
pnpm test:unit:e2e-contract:coverage
```
