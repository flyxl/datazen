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
pnpm tauri:dev --plugins=none
pnpm tauri:dev --plugins=kiwi
```

### Tests (PR baseline)

```bash
pnpm test:unit
cargo test -p datazen --lib
```

Optional site SEO structural check (marketing site under `site/`):

```bash
node scripts/check-site-seo.mjs
```

E2E requires a webdriver debug build — see [`docs/e2e-testing.md`](docs/e2e-testing.md). Do not use a bare `cargo build` as the E2E binary.

## Pull requests

1. Fork and create a branch from `main`.
2. Keep changes focused; match existing Rust / TypeScript style.
3. Run the baseline tests above.
4. Fill out the PR template (summary, test plan, checklist).
5. Link related issues when applicable.

Frontend IPC args use `snake_case` keys to match the Rust commands. Avoid editing generated files (`src/plugins/generated.ts`, `src-tauri/src/plugin_init.rs`).

## Plugins

External drivers use the compile-time plugin system (`plugins-registry.json`,
`register_driver!`). See [`docs/plugin-development.md`](docs/plugin-development.md)
if present, or `AGENTS.md` for an overview.

## License

By contributing, you agree that your contributions will be licensed under the
same [GPL-3.0](LICENSE) license as the project.
