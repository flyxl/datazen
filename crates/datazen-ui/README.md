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

The WebAssembly package builds without a page around it (H6):

~~~sh
rustup target add wasm32-unknown-unknown
cargo build -p datazen-ui --target wasm32-unknown-unknown
~~~

The artifact is `target/wasm32-unknown-unknown/debug/datazen_ui.wasm`
(a cdylib; the release profile plus wasm-opt shrinks it for shipping). The
web host is WebGPU-only: the optional WebGL backend is NOT compiled in —
wgpu's WebGL backend makes its hal instance `!Sync` on wasm32, which rune's
process-wide instance cache requires, so that feature breaks the wasm
build (rune's own web app ships WebGPU-only the same way). A standalone
static page that mounts the wasm is the next slice (H7); the React host's
index and routes stay untouched.

## Next slice

Add typed DataZen UI ports and connect Connections → connect → Schema → read-only table data to existing application use cases. Do not duplicate Driver, Runtime, credentials, permission policy or the HTTP MethodMap in this crate.

## The desktop process (H1)

`cargo run -p datazen-ui --bin datazen-rune` opens the shared Rune UI
with the in-memory workspace — no Tauri, no HTTP. The window needs a
display plus `libxkbcommon` (X11 also wants `libxkbcommon-x11`); the
connection list is the first page and is covered by the runtime tests.
