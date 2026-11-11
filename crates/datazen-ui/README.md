# datazen-ui

The shared Rune UI crate for DataZen native desktop and WebAssembly.

## Current implementation

The same Rust view tree is used by:

- the native desktop binary named datazen-rune;
- the WebAssembly entrypoint selected by the Web host with the query parameter ui=rune.

The shell has interactive navigation for Connections, SQL Editor, Data Browser, Schema, Workflows and Settings. These are migration placeholders, not yet wired to database use cases. Existing React and Tauri entrypoints remain the default.

## Pinned Rune source

Rune is included as a Git submodule at vendor/rune and pinned to:

2e4999f67064dd351e504bbdaaf71f0b601e2114

The submodule includes Rune's patched vendor/wgpu-hal source. DataZen applies that exact patch source at the workspace root because Cargo does not inherit patch.crates-io from a dependency's workspace.

After cloning DataZen:

~~~sh
git submodule update --init --recursive vendor/rune
~~~

## Toolchain

Rune pins Rust 1.99.1 in its upstream `rust-toolchain.toml`. The submodule is a dependency rather than a workspace member, so Rust does not automatically inherit that file. Install and select the pinned toolchain explicitly:

~~~sh
rustup toolchain install 1.99.1
RUSTUP_TOOLCHAIN=1.99.1 cargo run -p datazen-ui --bin datazen-rune
~~~

## Web preview

~~~sh
rustup target add wasm32-unknown-unknown
pnpm --filter datazen-web build:rune-ui
pnpm --filter datazen-web dev
~~~

Open the Web host at http://localhost:5174/?ui=rune. The Web target requires a supported browser and HTTPS or localhost. The optional WebGL backend is compiled in, but it is not yet treated as a supported fallback until browser/runtime verification is complete.

The Web build wrapper calls Rune's own wasm packager so its helper assets and embedded web fonts are copied together with the wasm-bindgen output. Vite copies the generated files under packages/web/public/rune-ui into the Web distribution.

## Next slice

Add typed DataZen UI ports and connect Connections → connect → Schema → read-only table data to existing application use cases. Do not duplicate Driver, Runtime, credentials, permission policy or the HTTP MethodMap in this crate.
