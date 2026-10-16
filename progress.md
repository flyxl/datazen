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

## Independent acceptance corrections

Fixed proxy-only DNS regression: desktop validates HTTP(S)/userinfo URL, then lets reqwest and trusted environment proxy resolve target. Restricted mode retains complete IP validation/pinning/no_proxy.
Restored remote model listing 10-second connect timeout and 30-second overall deadline including DNS and response body.
Added subprocess-isolated real proxy regression and real hanging-socket 30-second timeout regression in packages/ai-providers/tests/egress_regressions.rs.

Command: CARGO_TARGET_DIR=<system temp> cargo test -p datazen-ai-providers
Exit: 0
Exact conclusions:
- test result: ok. 94 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.02s
- test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 32.02s
- doc tests: 0 passed, 0 failed
HEAD before/after: 60cc80df7637b5f48b3bdc41a7cc0b10a79f3e8c
Staged+working diff SHA256 before/after: 69e91afcf2c92aa6a4447098b11538b1d28f55ddf4d1d4c9b68a123ecdffee57
Status: READY_FOR_TEST (requires fresh independent acceptance).

## IPv6 documentation-prefix correction

Independent acceptance found 3fff::/20 accepted by PublicInternet. Added exact 20-bit rejection plus first/last/in-range and adjacent out-of-range boundary regression. No change to 5f00::/16 or other prefix behavior.
Command: CARGO_TARGET_DIR=<system temp> cargo test -p datazen-ai-providers
Exit: 0
Exact conclusions:
- test result: ok. 95 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.03s
- test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 32.02s
HEAD before/after: 2bcb4d00aeef8927a862f2f3889a601a4e95382c
Staged+working diff SHA256 before/after: f4b4cfe91339d98c3e721465b14f2a611c1f77b42538da9ea49bdffbed37f645
Status: READY_FOR_TEST
