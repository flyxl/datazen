# datazen-jdbc-agent

External Java Agent for DataZen generic JDBC (monorepo path `datazen-jdbc-agent/`).

- **Java:** 17+
- **Protocol:** JSON-RPC 2.0 over stdio — [`packages/drivers/jdbc/protocol.md`](../packages/drivers/jdbc/protocol.md)
- **Deps:** HikariCP 5.1 + SLF4J nop (downloaded by `build.sh` into `lib/`, packed into the fat jar).
  Vendor JDBC JARs are still supplied at `session.open` via `jars[]` (URLClassLoader).

## Build

```bash
./datazen-jdbc-agent/build.sh
# → datazen-jdbc-agent/build/datazen-jdbc-agent.jar
```

Requires network once to cache HikariCP / SLF4J under `lib/`.

## Connection pool

When HikariCP is present (default build), sessions **borrow** connections from a pool keyed by `url + user + jars`:

| Setting | Source | Default |
|---------|--------|--------|
| `maximumPoolSize` | top-level or `props.maximumPoolSize` / `props.pool.maximumPoolSize` | 5 (cap 32) |
| `minimumIdle` | top-level or props | 0 |
| `pool.connectionTimeoutMs` | props | 30000 |
| `pool.idleTimeoutMs` | props | 600000 |
| `pool.maxLifetimeMs` | props | 1800000 |

Disable pooling for a session:

```json
{ "poolEnabled": false }
// or props: { "pool.enabled": "false" }
```

`session.open` result includes `"pooled": true|false`.

Without Hikari on the classpath, the agent falls back to one dedicated connection per session.

## Run (smoke)

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"agent.hello","params":{"hostVersion":"dev","protocolVersion":1}}' \
  | java -jar datazen-jdbc-agent/build/datazen-jdbc-agent.jar
```

## Methods

`agent.hello` / `agent.shutdown` / `session.*` / `meta.*` / `query.*` / `exec.update` / `tx.*`
