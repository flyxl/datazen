# datazen-ui

The shared Rune UI crate for DataZen's native desktop and WebAssembly targets.

## Current scope

This is the first integration slice: it pins Rune, provides one shared Rune view,
and exposes native and wasm entry points. It deliberately does not replace the
existing Tauri or React application yet.

## Build targets

- Native desktop: `cargo run -p datazen-ui --bin datazen-rune`
- WebAssembly: `wasm32-unknown-unknown` with the Rune `scripts/web-build.sh`
  helper and wasm-bindgen CLI version pinned by the Rune revision.

The Rune dependency is pinned by immutable Git revision in the workspace root.
