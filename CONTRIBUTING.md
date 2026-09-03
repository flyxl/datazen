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

- Node.js ≥ 20
- pnpm ≥ 9
- Rust ≥ 1.77
- Tauri v2 system dependencies: https://v2.tauri.app/start/prerequisites/

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

Run all of the following (same as CI):

```bash
pnpm test:unit
cargo test -p datazen --lib
cargo test -p datazen-ai-api --lib
node scripts/check-site-seo.mjs
```

E2E requires a webdriver debug build — see [`docs/development/e2e-testing.md`](docs/development/e2e-testing.md). Do not use a bare `cargo build` as the E2E binary.

## Pull requests

1. Fork and create a branch from `main`.
2. Keep changes focused; match existing Rust / TypeScript style.
3. Run the baseline tests above.
4. Fill out the PR template (summary, test plan, checklist).
5. Link related issues when applicable.

### External contracts

Changes to MCP tools/resources, Tauri IPC shapes, driver/AI protocol versions, or persisted JSON keys may break downstream clients and plugins. Read [`docs/development/external-contract-policy.md`](docs/development/external-contract-policy.md) before merging contract-touching PRs.

Frontend IPC args use `snake_case` keys to match the Rust commands. Do not edit generated files (`src/plugins/generated.ts`, `src/plugins/generated-locales.ts`, `src-tauri/src/driver_init.rs`); they are gitignored and written by `pnpm install` / `resolve-drivers`.

## Plugins

External drivers use the compile-time plugin system (`drivers-registry.json`,
`register_driver!`). See [`docs/development/independent-driver-development.en.md`](docs/development/independent-driver-development.en.md)
or `AGENTS.md` for an overview.

## License

By contributing, you agree that your contributions will be licensed under the
same [GPL-3.0](LICENSE) license as the project.
