# Contributing to DataZen

Thanks for your interest in contributing. This guide covers the basics for
working on the main repository. For deeper architecture notes, see
[`AGENTS.md`](AGENTS.md) and [`docs/architecture/`](docs/architecture/).

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

- Bug reports and feature ideas via [Issues](https://github.com/flyxl/datazen/issues)
  (use the templates)
- Questions and discussion via [Discussions](https://github.com/flyxl/datazen/discussions)
- Pull requests for fixes and improvements
- Security issues: see [SECURITY.md](SECURITY.md) (do **not** open a public issue)

## Development setup

### Prerequisites

Use the same toolchain as CI when possible ([ci-test-matrix.md](docs/development/ci-test-matrix.md)):

| Tool | Recommended | Minimum |
|------|-------------|---------|
| Node.js | **24** | 20 |
| pnpm | **11** | 9 |
| Rust | **stable** | 1.77 |
| Tauri v2 | — | [system deps](https://v2.tauri.app/start/prerequisites/) |

### Run the app

```bash
pnpm install
pnpm tauri:dev
```

Plugin selection examples:

```bash
pnpm tauri:dev --drivers=basic
pnpm tauri:dev --drivers=basic,kiwi
```

### Tests (PR baseline)

Run the same checks as [`.github/workflows/ci.yml`](.github/workflows/ci.yml) (full command list in [ci-test-matrix.md](docs/development/ci-test-matrix.md)):

```bash
node scripts/generate-builtin-locales.mjs
pnpm typecheck
pnpm test:unit
node scripts/resolve-drivers.mjs --drivers=basic
cargo test -p datazen-driver-api --lib
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen --lib --features "$FEATURES"
cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --lib
cargo test -p datazen-ai-api --lib
node scripts/driver-file-stash.mjs restore
```

If you changed `site/`: `node scripts/check-site-seo.mjs`.

If you changed an **optional path driver** (not in basic SKU): also run `cargo test -p datazen-driver-<id> --lib` and driver UI tests under `packages/drivers/<id>/ui/__tests__/`.

E2E is **not** in PR CI but is required when you change Host UI paths — see [`e2e-testing.md`](docs/development/e2e-testing.md). Do not use a bare `cargo build` as the E2E binary.

### Frontend boundaries (windows & stores)

- **Main workspace vs sub-windows**: Connection / Settings / Workflow / Dashboard live in the `main` OS window; Backup, Data Sync, Schema Diff, and Data Transfer are separate sub-windows. See [windows.md](docs/architecture/windows.md).
- **Store scope**: Persistent config uses `connectionId` (`connectionStore`); live DB sessions use `dbSessionId` (`activeConnectionStore`, never persisted). Sub-windows must not mutate `panelStore`; use `crossWindowBus` for cross-window notifications. Details: [windows.md §6](docs/architecture/windows.md#6-窗口边界与-store-职责), [naming.md](docs/architecture/naming.md), [state.md](docs/architecture/frontend/state.md).

## Pull requests

1. Fork and create a branch from `main`.
2. Keep changes focused; match existing Rust / TypeScript style.
3. Run the baseline tests above.
4. Fill out the PR template (summary, test plan, checklist).
5. Link related issues when applicable.

Frontend IPC args use `snake_case` keys to match the Rust commands. Do not edit generated files (`src/plugins/generated.ts`, `src/plugins/generated-locales.ts`, `src-tauri/src/driver_init.rs`); they are gitignored and written by `pnpm install` / `resolve-drivers`.

## Plugins

External drivers use the compile-time plugin system (`drivers-registry.json`,
`register_driver!`). See [`docs/development/independent-driver-development.en.md`](docs/development/independent-driver-development.en.md)
or `AGENTS.md` for an overview.

## License

By contributing, you agree that your contributions will be licensed under the
same [GPL-3.0](LICENSE) license as the project.
