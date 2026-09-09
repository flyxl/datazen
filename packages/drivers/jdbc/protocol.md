# JDBC Agent Protocol (v1)

Host (`packages/drivers/jdbc`) <-> Java Agent (`datazen-jdbc-agent`) over **stdio**.

- One JSON object per line on stdin (requests) and stdout (responses).
- stderr is diagnostics only - never mix protocol frames into stderr.
- Encoding: UTF-8 JSON-RPC 2.0.

## Version

| Field | Value |
|-------|-------|
| `protocolVersion` | `1` |
| Agent package version | independent semver (e.g. `0.1.0`) |

Negotiate via `agent.hello`. Host must reject agents with incompatible major protocol version.

## Methods

| Method | Status |
|--------|--------|
| `agent.hello` | Phase 1 (Host + Agent) |
| `agent.shutdown` | Phase 1 |
| `session.open` / `session.close` | Phase 2 |
| `meta.databases` / `meta.tables` / `meta.columns` | Phase 2 |
| `query.execute` / `query.fetch` / `query.close` / `query.cancel` | Phase 2 |
| `exec.update` | Phase 2 |
| `tx.begin` / `tx.commit` / `tx.rollback` | Phase 2 |

## Hello example

```json
{"jsonrpc":"2.0","id":1,"method":"agent.hello","params":{"hostVersion":"0.0.0","protocolVersion":1}}
{"jsonrpc":"2.0","id":1,"result":{"agentVersion":"0.1.0","protocolVersion":1,"capabilities":["jdbc","session","query.stream","tx"]}}
```

## Host behavior (Phase 1)

- Lazy start on first `ensure_running` / `rpc`
- Single shared Agent process
- stdout reader demux by response `id`
- One automatic restart after unexpected exit; second failure surfaces error
- `shutdown` sends `agent.shutdown` then kills the child

## Errors

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"...","data":{"category":"driver|connect|sql|cancel|internal"}}}
```

Never include passwords in `message` or logs.
