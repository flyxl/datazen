# Locales

Shipping pair (`en` / `zh-CN`) is split by **domain** so PRs stay small and low-frequency features can be code-split.

## Layout

```
locales/
  domains.ts           # EAGER_DOMAINS / LAZY_DOMAINS
  lazyPacks.ts         # dynamic import registry + ensureLocaleDomains()
  builtinLocales.ts    # eager runtime dictionaries
  fullLocales.ts       # full dict (tests / tooling only)
  zh-CN/  en/          # per-domain packs + index (full merge) + eager.ts
  zh-CN.ts  en.ts      # re-exports of full merge (back-compat)
  index.ts             # getTranslation / public API
  de.ts fr.ts …        # optional extra locales (monolith, unchanged)
```

## Lazy domains

| Domain       | Loaded when                          |
|--------------|--------------------------------------|
| sync         | Data Sync window                     |
| transfer     | Data Transfer window                 |
| schemaDiff   | Schema Diff window                   |
| workflows    | Workflow page                        |
| dashboard    | Dashboard panel                      |
| mcp          | Settings → MCP sections              |

```ts
import { useLocaleDomains } from '../hooks/useLocaleDomains';

// inside feature window:
useLocaleDomains(['sync']);
```

## Adding a key

1. Put it in the correct domain file under `zh-CN/<domain>.ts` and `en/<domain>.ts`.
2. Keep key prefixes stable (`sync.*`, `connWin.*`, …).
3. Run unit tests (`locales.test.ts` checks en/zh-CN parity on the full dict).
