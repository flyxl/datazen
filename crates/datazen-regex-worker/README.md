# Native query regular-expression worker

This component is currently an opt-in Linux x86_64 experiment. The host must
dispatch the fixed worker argument before app startup, run one supervisor off
the render thread, check every reply against the originating draft and
selection, and apply any edits as a single undoable transaction. This crate
does not own editor state, revisions, focus, or history.

The parent execs the already running trusted image via `/proc/self/exe`, with
empty environment, fixed working directory, closed inherited descriptors,
and a private Unix socket through standard input/output. Direct child death
follows the launching thread or an explicit kill/reap. The fresh image sets
Linux address-space, stack, CPU, descriptor, and core limits, disables dumpability,
and installs a default-deny seccomp filter before acknowledging readiness.
The document never leaves the parent until that acknowledgement arrives.
No shell, filesystem opens, network sockets, process creation, executable
launches, or JavaScript runtime are part of matching.

The deadline includes request preparation, the child handshake, matching,
reply transfer and parent validation. Once the deadline is observed, or an
invalid reply is observed, the parent kills and reaps the child without
depending on the engine's cooperation. The dedicated supervisor may still
block in kernel spawn/reap if the OS is stalled; no successful result is
returned after the deadline. Kernel availability and target ABI are explicit
requirements. There is no weaker in-process fallback or Windows/macOS
implementation.

The LF-only search window, wrapping rules, and bounded capture replacements
follow pinned `@codemirror/search` 6.6.0. Whole-word context follows
`@codemirror/state` 6.7.4 and `find-cluster-break` 1.0.4. Their MIT notices and the Unicode License V3 notice for the derived
Unicode 17 Alphabetic/Number ranges are included. The checked-in table
is byte-for-byte the already generated Native Query table from
`scripts/codemirror-whole-word-fixtures.mjs`. Matching uses pinned Regress 0.12.0 (pure Rust ECMAScript regexp).
The interface accepts only original-document UTF-8 scalar boundaries.
CR-bearing document, unrepresentable cursor edges, oversized requests or
results, and worker errors are no-edit results, not approximations.
The checked golden fixtures are not exhaustive ECMAScript conformance.

Linux verification, after normal Cargo prerequisites:

```sh
cargo test --locked -p datazen-regex-worker
cargo run --locked -p datazen-regex-worker --example worker_smoke
cargo run --locked -p datazen-regex-worker --example worker_goldens
```
