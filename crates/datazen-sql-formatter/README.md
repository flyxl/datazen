# DataZen native SQL formatter

A Rust implementation of the SQL formatting behavior expected by DataZen.
The reference is the original desktop renderer's pinned `sql-formatter`
15.8.2 package. Upstream portions adapted here are credited in
`third_party/sql-formatter-15.8.2-MIT.txt` under the MIT license.

The Native package does not include the upstream JavaScript implementation,
an interpreter, a subprocess, or a WebView for formatting. Formatting is
local compute only and never executes SQL. The API returns a typed error for
syntax or dialect coverage gaps and for bounded resource limits instead of
reporting a successful no-op. The exact supported scope is measured by
differential fixtures copied as data, not inferred from lexer recognition.

The pinned comparison covers representative SELECT, DML, DDL, transaction,
and comment handling across six dialects, not every statement accepted by
each database or client. Client-side delimiter changes and external or
streamed COPY targets are deliberately unsupported; never apply formatting
that rewrites a client command into a different command.

The Native host's normal Cargo build enables this formatter by default.
A constrained host build can use `--no-default-features` to omit it; the
Format command should then report that the host has no local formatter.
