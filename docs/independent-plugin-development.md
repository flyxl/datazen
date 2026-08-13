# Independent Plugin Development

This guide describes how to develop a Datazen plugin in its own Git repository while using a local Datazen checkout as the build and debugging host.

The important point is that Datazen plugins are **compiled into Datazen at build time**. A plugin is not loaded from a `.so`, `.dylib`, or `.dll` at runtime. The recommended development setup is therefore two sibling repositories:

```text
workspace/
├── datazen/
└── datazen-driver-mydb/
```

The plugin repository remains independent, while the local Datazen repository is used to compile the plugin together with the application and to debug the complete Rust + frontend integration.

## 1. Prerequisites

Install the normal Datazen development prerequisites and make sure the Datazen repository can build successfully before adding your plugin.

You also need a checkout of your plugin repository next to the Datazen repository:

```text
~/workspace/
├── datazen/
└── datazen-driver-mydb/
```

The two repositories do not need to be merged into one Git repository and the plugin does not need to become a member of Datazen's Git repository.

## 2. Create the plugin repository

A driver plugin should be an independent Rust project. A typical layout is:

```text
datazen-driver-mydb/
├── Cargo.toml
├── Cargo.lock
├── src/
│   └── ...
├── ui/
│   └── ...
└── README.md
```

The exact Rust and frontend structure depends on the capabilities provided by the plugin. A plugin may contain only a Rust driver, or it may also provide frontend components such as database metadata, connection forms, connection views, settings, schema trees, SQL dialects, and related UI integration.

The plugin should depend on the public Datazen driver API rather than on Datazen application internals.

## 3. Register the local plugin in Datazen

During local development, add the plugin to Datazen's `drivers-registry.json` using a `path` source.

For example:

```json
{
  "mydb": {
    "source": "path",
    "path": "../datazen-driver-mydb",
    "feature": "plugin-mydb",
    "description": "MyDB driver"
  }
}
```

The path is relative to the Datazen repository.

The registry already uses this same `source: "path"` form for built-in drivers. It also supports `source: "git"` for independently hosted plugins. See the existing entries in `drivers-registry.json` for examples.

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

For a new plugin, editing `drivers-registry.json` directly is the simplest way to make the development setup explicit and reproducible inside your local checkout.

## 4. Build Datazen with the plugin

Datazen's driver selection is controlled by `--drivers` (or `DATAZEN_DRIVERS`). The resolver reads `drivers-registry.json`, resolves the requested driver IDs, generates the required build configuration, and injects the selected plugin dependencies/features into the Datazen build.

For your plugin:

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

You can also combine your plugin with the built-in drivers:

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
Datazen binary containing the plugin
```

The Rust plugin is therefore compiled and linked into the Datazen application. At runtime Datazen discovers the compiled driver through its normal driver registration mechanism; the plugin is not loaded through a platform-specific shared-library ABI.

## 5. Develop the Rust side

Make changes directly in the plugin repository:

```bash
cd ~/workspace/datazen-driver-mydb
```

Then rebuild/run Datazen from the Datazen repository:

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

Because the registry uses a local `path` dependency, Cargo builds the current source from `../datazen-driver-mydb`. There is no need to copy plugin source into `packages/` and no need to commit the plugin source to the Datazen repository.

The plugin's Rust implementation should use the Datazen driver API and register its driver using the registration mechanism provided by the API. This allows the compiled plugin to participate in Datazen's normal driver registry.

## 6. Develop the frontend side

A plugin may also contribute frontend code. The selected plugin's frontend integration is included by the Datazen frontend build together with the rest of the application.

This means frontend development should also be performed with the local Datazen checkout as the host:

```text
plugin repository
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

This is important for debugging because plugin UI code runs in the real Datazen application context rather than in a separate mock host. You can therefore debug the plugin together with Datazen's actual React/Tauri environment.

The current driver resolver generates `src/plugins/generated.ts` (gitignored) from the selected driver set. Frontend contributions are therefore part of the same build-time selection as the Rust driver. `pnpm install` / `pnpm build` run `--codegen-only` when those files are missing.

When adding frontend functionality, follow the structure and conventions used by existing external plugins in the registry, such as Kiwi, OLAP, and Superset.

## 7. Iterative development loop

The normal development loop is:

```text
1. Edit plugin source
       ↓
2. Start/restart Datazen with --drivers=mydb
       ↓
3. Datazen resolves the local path plugin
       ↓
4. Rust + frontend are compiled into Datazen
       ↓
5. Debug the plugin in the real Datazen application
       ↓
6. Repeat
```

For Rust changes, Cargo recompiles the affected plugin code. For frontend changes, the normal Datazen frontend development tooling can be used to debug the resulting UI.

## 8. Test the plugin independently

The plugin repository should keep its own tests and CI. At minimum, test the driver implementation independently of the Datazen application where practical. **Do not add driver-specific tests to the Host** (`src-tauri/`, `src/`, `e2e/specs/`). Path drivers in this monorepo follow the same rule: tests live under `packages/drivers/<id>/` (`#[cfg(test)]`, `tests/`, `ui/__tests__/`, `e2e/`). See [AGENTS.md](../AGENTS.md)「驱动测试落点」.

For example:

```bash
cd ~/workspace/datazen-driver-mydb
cargo test
```

If the plugin has frontend code, run the frontend project's normal test/type-check/build commands as defined by that plugin repository.

These tests verify the plugin itself. Running Datazen with `--drivers=mydb` verifies the integration between the plugin and the current Datazen source tree.

## 9. Before publishing the plugin

A plugin should be validated in both forms:

### Local integration

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

This verifies that the current plugin source can be compiled into and used by Datazen.

### Independent plugin build/test

```bash
cd ~/workspace/datazen-driver-mydb
cargo test
```

Run the plugin repository's frontend checks as well when applicable.

## 10. Switching from local development to a Git dependency

After the plugin is published, Datazen can consume it from its independent Git repository.

During development:

```json
{
  "mydb": {
    "source": "path",
    "path": "../datazen-driver-mydb",
    "feature": "plugin-mydb"
  }
}
```

For a committed Datazen registry entry, the plugin can instead be pinned to a Git revision:

```json
{
  "mydb": {
    "source": "git",
    "git": "https://github.com/example/datazen-driver-mydb.git",
    "ref": "<commit-sha>",
    "feature": "plugin-mydb"
  }
}
```

Pinning a commit makes the Datazen build reproducible and avoids silently changing the plugin version used by a Datazen build.

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
publish plugin repository
        │
        ▼
Datazen registry PR
        │
        ▼
source = git
ref = <pinned commit>
```

## 11. Submitting the Datazen registry change

The plugin repository and Datazen repository remain separate Git repositories.

When the plugin is ready for inclusion in Datazen:

1. Push the plugin repository and publish the required revision.
2. Create a branch in the Datazen repository.
3. Change the plugin's registry entry from the local `path` source to the plugin's Git repository and pinned revision.
4. Run the Datazen build/tests with the plugin selected.
5. Open a pull request against Datazen.
6. The Datazen repository owners review and merge the registry change.

A local `drivers-registry.json` change used during development does not affect GitHub `main` by itself. Only a merged pull request changes the shared registry.

## 12. Recommended repository layout

A complete independent-plugin development workspace should look like:

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

The two repositories are independent, but the local filesystem layout allows Datazen's existing `source: "path"` and `--drivers` mechanisms to compile the plugin into the application.

## 13. Summary

The recommended development model is:

- Keep the plugin in its **own Git repository**.
- Keep the plugin checkout **next to the Datazen checkout**.
- During development, register the plugin in Datazen with `source: "path"`.
- Run Datazen with `pnpm tauri:dev --drivers=<plugin-id>`.
- Let Datazen compile the plugin into the application at build time.
- Use the real Datazen application to debug both Rust integration and frontend UI.
- Keep plugin tests and CI in the plugin repository.
- When ready to publish, change the Datazen registry entry to a pinned `source: "git"` revision through a pull request.

This model keeps plugin source code independent while preserving Datazen's compile-time integration model and avoids the ABI/versioning problems of runtime Rust dynamic-library loading.
