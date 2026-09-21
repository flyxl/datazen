# Independent Driver Development

This guide describes how to develop a Datazen **driver** in its own Git repository while using a local Datazen checkout as the build and debugging host.

The important point is that Datazen drivers are **compiled into Datazen at build time**. A driver is not loaded from a `.so`, `.dylib`, or `.dll` at runtime. The recommended development setup is therefore two sibling repositories:

```text
workspace/
├── datazen/
└── datazen-driver-mydb/
```

The driver repository remains independent, while the local Datazen repository is used to compile the driver together with the application and to debug the complete Rust + frontend integration.

## 1. Prerequisites

Install the normal Datazen development prerequisites and make sure the Datazen repository can build successfully before adding your driver.

You also need a checkout of your driver repository next to the Datazen repository:

```text
~/workspace/
├── datazen/
└── datazen-driver-mydb/
```

The two repositories do not need to be merged into one Git repository and the driver does not need to become a member of Datazen's Git repository.

## 2. Create the driver repository

A driver should be an independent Rust project. A typical layout is:

```text
datazen-driver-mydb/
├── Cargo.toml
├── Cargo.lock
├── src/
│   └── ...
├── ui/
│   └── ...
├── locales/            # frontend translations (see 6.3; only needed by drivers with UI)
└── README.md
```

The exact Rust and frontend structure depends on the capabilities provided by the driver. A driver may contain only a Rust driver, or it may also provide frontend components such as database metadata, connection forms, connection views, settings, schema trees, SQL dialects, and related UI integration.

The driver should depend on the public Datazen driver API rather than on Datazen application internals.

## 3. Register the local driver in Datazen

During local development, add the driver to Datazen's `drivers-registry.json` using a `path` source.

For example:

```json
{
  "mydb": {
    "source": "path",
    "path": "../datazen-driver-mydb",
    "feature": "driver-mydb",
    "description": "MyDB driver"
  }
}
```

The path is relative to the Datazen repository.

The registry already uses this same `source: "path"` form for built-in drivers. It also supports `source: "git"` for independently hosted drivers. See the existing entries in `drivers-registry.json` for examples.

> **Development workflow:** modifying `drivers-registry.json` locally is expected. The local change is simply part of your Datazen development checkout. It does not modify the GitHub `main` branch unless you commit it and submit a pull request and that pull request is merged.

### Optional local overrides

Datazen also supports `.drivers-dev.json` as a gitignored local override for an existing registry entry. This is useful when you want to keep the committed `drivers-registry.json` unchanged. For example:

```json
{
  "kiwi": {
    "source": "path",
    "path": "../datazen-driver-kiwi"
  }
}
```

For a new driver, editing `drivers-registry.json` directly is the simplest way to make the development setup explicit and reproducible inside your local checkout.

## 4. Build Datazen with the driver

Datazen's driver selection is controlled by `--drivers` (or `DATAZEN_DRIVERS`). The resolver reads `drivers-registry.json`, resolves the requested driver IDs, generates the required build configuration, and injects the selected driver dependencies/features into the Datazen build.

For your driver:

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

You can also combine your driver with the built-in drivers:

```bash
pnpm tauri:dev --drivers=basic,mydb
```

`basic` expands to the four core drivers (`postgres`, `mysql`, `sqlite`, and `redis`). `all` expands to all `source: "path"` drivers in the registry. An explicit comma-separated list can be used for custom driver sets.

For example:

```bash
pnpm tauri:dev --drivers=postgres,mysql,mydb
```

### What `--drivers` actually does

`--drivers` is a **build-time driver selection**, not a runtime dynamic-loader option.

Conceptually:

```text
--drivers=mydb
        │
        ▼
drivers-registry.json
        │
        ▼
resolve-drivers.mjs
        │
        ├── Cargo dependency
        ├── Cargo feature
        └── generated frontend registry
        │
        ▼
Datazen build
        │
        ▼
Datazen binary containing the driver
```

The Rust driver is therefore compiled and linked into the Datazen application. At runtime Datazen discovers the compiled driver through its normal driver registration mechanism; the driver is not loaded through a platform-specific shared-library ABI.

## 5. Develop the Rust side

Make changes directly in the driver repository:

```bash
cd ~/workspace/datazen-driver-mydb
```

Then rebuild/run Datazen from the Datazen repository:

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

Because the registry uses a local `path` dependency, Cargo builds the current source from `../datazen-driver-mydb`. There is no need to copy driver source into `packages/` and no need to commit the driver source to the Datazen repository.

The driver's Rust implementation should use the Datazen driver API and register its driver using the registration mechanism provided by the API. This allows the compiled driver to participate in Datazen's normal driver registry.

## 6. Develop the frontend side

A driver may also contribute frontend components. The selected driver's frontend integration is included by the Datazen frontend build together with the rest of the application.

This means frontend development should also be performed with the local Datazen checkout as the host:

```text
driver repository
      │
      ├── Rust implementation
      │
      └── frontend implementation
               │
               ▼
        Datazen frontend build
               │
               ▼
          Datazen application
```

This is important for debugging because driver UI code runs in the real Datazen application context rather than in a separate mock host. You can therefore debug the driver together with Datazen's actual React/Tauri environment.

The current driver resolver generates `src/extensions/generated.ts` (gitignored) from the selected driver set. Frontend contributions are therefore part of the same build-time selection as the Rust driver. `pnpm install` / `pnpm build` run `--codegen-only` when those files are missing.

When adding frontend functionality, follow the structure and conventions used by existing external drivers in the registry, such as Kiwi, OLAP, and Superset, and **strictly obey the dependency boundary contract below** (full specification: [Driver ↔ Host Dependency Boundaries](driver-api-dependency-boundary.md), "Part 2").

### 6.1 Frontend dependency boundary (the allowed import surface)

Driver frontend code may **only** import:

| Source | Provides |
| --- | --- |
| `@datazen/ui` | Base components (`Button` / `Input` / `Select` / `Dialog` / `Tabs` / `Badge` / `Label` / `Slider` / `TemporalValueInput` / `PathInput`), `cn`, the i18n runtime (`t` / `useI18n`, …) |
| `@datazen/driver-sdk` | Metadata/dialect contracts such as `DatabaseTypeMeta`, sunk-down shared types (`ConnectionFormState`, `KeyEntry`, `NativeMenuItemDef`, `ConnectionViewProps`, …), Command IPC wrappers (`driverCommands` / `fileCommands`), pure helpers, the context-menu API, and the injection bridges `useBound*` (see 6.2) |
| `@datazen/extension-points` | EP contract types only; ordinary database drivers normally do not need it |
| npm dependencies | Third-party packages declared by the driver repository itself (e.g. `react`) |

**Any relative import into the host's `src/**` is forbidden** (`../../../src/...` at any nesting depth, including `src/hooks`, `src/stores`, `src/lib`, `src/types`, `src/components`, `src/locales`). Counter-example (❌ violating) and correct form (✅):

```ts
// ❌ Counter-example: importing from the host
import { cn } from '../../../../../src/lib/cn';
// ✅ Correct: always go through the shared packages
import { cn, useI18n } from '@datazen/ui';
```

The bare specifiers resolve to `packages/*/src/index.ts` both in the host build (root `tsconfig.json` paths + `vite.config.ts` alias) and in driver unit tests (`vitest.drivers.config.ts` alias, run via `pnpm test:unit:drivers`); an independent driver repository simply adds `packages/ui` and `packages/driver-sdk` as local path dependencies.

### 6.2 Consuming host capabilities: sinking down and injection bridges

When driver UI needs a capability that lives on the host side, use one of these patterns (decision table and full bridge inventory: contract document sections 2.2 / 2.3):

- **Pure functions / IPC wrappers**: **move** the implementation down into `@datazen/driver-sdk` (one single implementation in the whole repository; copying is forbidden). After a sink-down, the old host path keeps only a **thin re-export** (a re-export pointing at the single SDK implementation, never a second implementation) **when legacy host consumers still import it**, and is **moved away entirely, leaving no empty shell, when no consumer remains**, with its consumers switching to importing the SDK directly (three-way rule: contract document sections 2.2 / 2.5). Thin-shell precedents: `src/lib/cn.ts` (the whole file is the single line `export { cn } from '@datazen/ui';`) → `@datazen/ui`, `src/lib/nativeContextMenu.ts:7-15` → SDK `nativeContextMenu`, `src/commands/driver.ts:6-11` → SDK `ipc/driverCommands`, `src/commands/file.ts:2/9` → **merged re-export** with the SDK `ipc/fileCommands` (the host keeps extra host-only commands). Move-away-without-a-shell precedents: `driverSettings`, `resolveEditorFontFamily` — the only implementations are `packages/driver-sdk/src/driverSettings.ts` and `packages/driver-sdk/src/resolveEditorFontFamily.ts`; the old host paths `src/lib/driverSettings.ts` / `src/lib/resolveEditorFontFamily.ts` no longer exist after `92a039383` moved them away, and the host consumers `src/windows/settings/DriverSettingsSection.tsx:3` and `src/components/sql-editor/editorExtensions.ts:36-38` import from `@datazen/driver-sdk` directly.
- **Runtime state from a host zustand store / React hook**: use the capability injection bridge — driver code consumes the `useBoundX()` accessor from `@datazen/driver-sdk`, while the host calls `bindX()` at module load, right where its own store/hook is defined. Existing bridges: `useBoundSettingsStore`, `useBoundConnectionStore`, `useBoundConfirmDialog`, `useBoundSchemaStore`, and `showNativeContextMenu` (via `bindContextMenuBridge`). Driver code **never calls** `bindX`; consuming an unbound bridge throws during development (`'<X> has not been bound to driver-sdk yet.'`).

```tsx
// Driver UI example (matches the live code in packages/drivers/redis/ui)
import { useBoundSettingsStore, useBoundConfirmDialog } from '@datazen/driver-sdk';

const safeMode = useBoundSettingsStore((s) => s.settings.safeMode); // reactive subscription in a component
// imperative reads on event/async paths: useBoundSettingsStore.getState().settings.safeMode
const [confirm, dialog] = useBoundConfirmDialog(); // render `dialog` once; `confirm` returns Promise<boolean>
```

When a new host capability is required, **do not import it from the host**; sink it down or add a bridge following the procedure in contract document section 2.5. The ban also covers the host's **thin re-export shells** (`src/lib/cn.ts`, `src/commands/driver.ts`, …): they exist only to keep legacy host imports stable, so driver code always imports the package name instead. This is now enforced statically by the Wave 4 guard — `pnpm test:boundaries` (rule R1 resolves every specifier literal inside driver packages and rejects anything climbing into host `src/`; rule R2 blocks `setLocale` calls anywhere under `packages/**`, exempting only the i18n runtime definition file and its own unit test), see contract document section 2.6.

### 6.3 i18n: one runtime and locale self-registration

- `@datazen/ui` is the **one and only** i18n implementation of the whole application (lookup / `{param}` interpolation / `en` fallback); there is no bridge and no second engine. Driver code uniformly does `import { useI18n } from '@datazen/ui'` (non-React paths such as form validators receive `t` as a parameter per the SDK contract, see `DriverFormValidator`).
- **Only the host calls `setLocale`** (the language preference is persisted and synchronized by the host settingsStore); any `setLocale` call in driver production code is a violation.
- **Translations are provided and self-registered by the driver package**: dictionaries live in `locales/` (one file per language, keys carrying the driver's own prefix such as `redis.*` / `mongo.*`); the pure side-effect module `locales/index.ts` statically imports **every** language dictionary in that directory and calls `registerTranslations` once (the registered set does not shrink to match the host's wired optional-language set — the asymmetry is the intended end state, see contract document section 2.4.3); it is hooked up via a single side-effect import pointing at this package's `locales/` directory in the driver UI entry module (the first UI module actually imported by `generated.ts`), and **the relative depth depends on where the entry lives**: an entry at `ui/meta.ts` (e.g. mongodb) writes `import '../locales';`, an entry at `ui/shared/meta.ts` (e.g. redis, two levels deep) writes `import '../../locales';`. **This self-registration chain has landed with Wave 3 (`i18n-drivers`)** (the host-side `DRIVER_LOCALES` aggregation codegen was deleted in the same batch and no longer appears in production code); add new driver translations through your own `locales/` self-registration and never through a host aggregation step.
- Driver-side `t()` keys are plain `string`s — there is no compile-time `I18nKey` checking; translation completeness is enforced by `node scripts/i18n-sync-check.mjs`, which scans both the host `src/locales/` and every driver package's `locales/` (a driver pack missing `locales/index.ts`, or an index that forgot to import a language file, fails as a structural issue). During development only edit your package's `en.ts` (the single source of truth).

## 7. Iterative development loop

The normal development loop is:

```text
1. Edit driver source
       ↓
2. Start/restart Datazen with --drivers=mydb
       ↓
3. Datazen resolves the local path driver
       ↓
4. Rust + frontend are compiled into Datazen
       ↓
5. Debug the driver in the real Datazen application
       ↓
6. Repeat
```

For Rust changes, Cargo recompiles the affected driver code. For frontend changes, the normal Datazen frontend development tooling can be used to debug the resulting UI.

## 8. Test the driver independently

The driver repository should keep its own tests and CI. At minimum, test the driver implementation independently of the Datazen application where practical. **Do not add driver-specific tests to the Host** (`src-tauri/`, `src/`, `e2e/specs/`). Path drivers in this monorepo follow the same rule: tests live under `packages/drivers/<id>/` (`#[cfg(test)]`, `tests/`, `ui/__tests__/`, `e2e/`). See [AGENTS.md](../../AGENTS.md)「驱动测试落点」.

For example:

```bash
cd ~/workspace/datazen-driver-mydb
cargo test
```

If the driver has frontend code, run the frontend project's normal test/type-check/build commands as defined by that driver repository.

These tests verify the driver itself. Running Datazen with `--drivers=mydb` verifies the integration between the driver and the current Datazen source tree.

## 9. Before publishing the driver

A driver should be validated in both forms:

### Local integration

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

This verifies that the current driver source can be compiled into and used by Datazen.

### Independent driver build/test

```bash
cd ~/workspace/datazen-driver-mydb
cargo test
```

Run the driver repository's frontend checks as well when applicable.

## 10. Switching from local development to a Git dependency

After the driver is published, Datazen can consume it from its independent Git repository.

During development:

```json
{
  "mydb": {
    "source": "path",
    "path": "../datazen-driver-mydb",
    "feature": "driver-mydb"
  }
}
```

For a committed Datazen registry entry, the driver can instead be pinned to a Git revision:

```json
{
  "mydb": {
    "source": "git",
    "git": "https://github.com/example/datazen-driver-mydb.git",
    "ref": "<commit-sha>",
    "feature": "driver-mydb"
  }
}
```

Pinning a commit makes the Datazen build reproducible and avoids silently changing the driver version used by a Datazen build.

The transition is therefore:

```text
local development
source = path
path = ../datazen-driver-mydb
        │
        ▼
validate integration
        │
        ▼
publish driver repository
        │
        ▼
Datazen registry PR
        │
        ▼
source = git
ref = <pinned commit>
```

## 11. Submitting the Datazen registry change

The driver repository and Datazen repository remain separate Git repositories.

When the driver is ready for inclusion in Datazen:

1. Push the driver repository and publish the required revision.
2. Create a branch in the Datazen repository.
3. Change the driver's registry entry from the local `path` source to the driver's Git repository and pinned revision.
4. Run the Datazen build/tests with the driver selected.
5. Open a pull request against Datazen.
6. The Datazen repository owners review and merge the registry change.

A local `drivers-registry.json` change used during development does not affect GitHub `main` by itself. Only a merged pull request changes the shared registry.

## 12. Recommended repository layout

A complete independent-driver development workspace should look like:

```text
~/workspace/
├── datazen/
│   ├── drivers-registry.json
│   ├── scripts/
│   ├── src/
│   ├── src-tauri/
│   └── ...
│
└── datazen-driver-mydb/
    ├── Cargo.toml
    ├── Cargo.lock
    ├── src/
    ├── ui/
    └── ...
```

The two repositories are independent, but the local filesystem layout allows Datazen's existing `source: "path"` and `--drivers` mechanisms to compile the driver into the application.

## 13. Summary

The recommended development model is:

- Keep the driver in its **own Git repository**.
- Keep the driver checkout **next to the Datazen checkout**.
- During development, register the driver in Datazen with `source: "path"`.
- Run Datazen with `pnpm tauri:dev --drivers=<driver-id>`.
- Let Datazen compile the driver into the application at build time.
- Use the real Datazen application to debug both Rust integration and frontend UI.
- Depend on `@datazen/ui` / `@datazen/driver-sdk` only on the frontend (no imports of host `src/**`); consume host capabilities via sink-down or `bind*`/`useBound*` bridges; i18n goes through the single runtime with self-registered locales.
- Keep driver tests and CI in the driver repository.
- When ready to publish, change the Datazen registry entry to a pinned `source: "git"` revision through a pull request.

This model keeps driver source code independent while preserving Datazen's compile-time integration model and avoids the ABI/versioning problems of runtime Rust dynamic-library loading.
