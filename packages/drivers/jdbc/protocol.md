# JDBC Agent Protocol (v1)

Host (`packages/drivers/jdbc`) ↔ Java Agent (`datazen-jdbc-agent`) over **stdio**.

- One JSON object per line (UTF-8 JSON-RPC 2.0).
- stdout = protocol only; stderr = logs.
- `protocolVersion`: **1**

## Methods

| Method | Params | Result |
|--------|--------|--------|
| `agent.hello` | `hostVersion`, `protocolVersion` | `agentVersion`, `protocolVersion`, `capabilities` |
| `agent.shutdown` | — | `ok` |
| `session.open` | `url`, `user`, `password`, `driverClass?`, `jars[]`, `props{}` | `sessionId` |
| `session.close` | `sessionId` | `ok` |
| `meta.databases` | `sessionId` | `{name}[]` |
| `meta.tables` | `sessionId`, `database?`, `schema?` | `{name,type}[]` |
| `meta.columns` | `sessionId`, `database?`, `schema?`, `table` | `{name,type,nullable}[]` |
| `query.execute` | `sessionId`, `sql`, `maxRows`, `fetchSize?` | `columns`, `rows`, `hasMore`, `cursorId?`, `executionId?`, `updateCount?` |
| `query.fetch` | `sessionId`, `cursorId`, `maxRows` | `rows`, `hasMore` |
| `query.close` | `sessionId`, `cursorId` | `ok` |
| `query.cancel` | `sessionId`, `executionId` | `ok` |
| `exec.update` | `sessionId`, `sql` | `updateCount` |
| `tx.begin` / `tx.commit` / `tx.rollback` | `sessionId` | `ok` |

## Host connection options

Stored on `ConnectionConfig.options`:

| Key | Meaning |
|-----|---------|
| `jdbcUrl` / `url` | JDBC URL (required) |
| `jars` | Absolute paths to vendor driver JARs |
| `driverClass` | e.g. `org.h2.Driver` |
| `props` | Extra string JDBC properties |

## Errors

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"...","data":{"category":"driver|connect|sql|cancel|internal"}}}
```

Never log passwords.
