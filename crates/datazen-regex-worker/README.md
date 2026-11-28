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

Passive marks use a separately tagged, mark-only request and reply. The
request carries the visible LF UTF-8 byte range; the worker applies the
stock 250 UTF-16 unit margin at each edge. A wide multiline query can
speculatively reach the remainder of the line containing the right edge,
as in the pinned cursor. The host still clips painting to its own visible
slice. Split UTF-8 viewport edges, a UTF-16 margin between a surrogate pair,
and more than 1,000 matches return no partial marks. A passive response
cannot masquerade as a selection or edit response.

Linux verification, after normal Cargo prerequisites:

```sh
cargo test --locked -p datazen-regex-worker
cargo run --locked -p datazen-regex-worker --example worker_smoke
cargo run --locked -p datazen-regex-worker --example worker_goldens
cargo run --locked -p datazen-regex-worker --example worker_highlights
```

A known ECMAScript compatibility gap is refused rather than approximated:
if two alternatives declare the same capture name and the pattern refers to
that name via a named backreference, Regress 0.12 can match an empty reference
to the wrong alternative. This worker returns `UnsupportedPattern` for
that case before producing navigation, highlight, or edit data. Unique named
groups, numeric references, and duplicate names without a reference to the
duplicate are not refused by this check. Run the actual child regression with
`cargo run --locked -p datazen-regex-worker --example worker_compatibility`.
