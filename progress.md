# Issue 70

Status: READY_FOR_TEST

Implemented AI HTTP egress boundary independent of database NetworkProvider routes/tunnels.
All built-in provider constructors, ProtocolConfig and remote model helper default to public-only egress. Desktop bootstrap/model IPC explicitly select desktop private-network and environment-proxy support. Public EgressPolicy trait and with_http_client constructors allow deployment injection. Each request checks every resolved IP, pins checked addresses to the connection, disables automatic redirects, and disables environment proxies in restricted mode. Error text omits endpoint/credentials. Native external inventory providers remain trusted extensions and require deployment review.

Runtime coverage: local desktop success, default local/literal/DNS rejection, redirect target untouched, IPv4/IPv6/private/special/metadata classification, all three protocols complete/stream/probe/models blocked before reaching mock server.

Validation command: CARGO_TARGET_DIR=<system temp> cargo test -p datazen-ai-providers --lib
Exit: 0
Exact conclusion: test result: ok. 94 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.02s
HEAD before/after: d122d2a37e246067a163e2cc5fbd5403c02bfee7
Staged+working diff SHA256 before/after: 6bfff2840a0961bd33e9e45a4932d37edfe6e48835ae581c67012d5a48349df0
Full log path recorded in /tmp/datazen-ai70-log-path.
Rustfmt executed on modified Rust files; git diff --cached --check passes.

Integration verification still required: existing host tests/build for two desktop constructor call sites. No server adapter exists yet; architecture explicitly documents future service injection. Per-request pinned clients trade connection pool reuse for checked DNS consistency. No .env files read.

Delete this progress.md when accepting/merging the track.
