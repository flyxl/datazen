# JDBC (external Agent) guide

DataZen can talk to databases that only ship a **JDBC** driver by running a small **out-of-process Java Agent**. The main app stays pure Rust/Tauri; no in-process JVM.

## Requirements

1. **JRE/JDK 17+** on the machine (`java` on `PATH` or `JAVA_HOME`).
2. Built agent jar: `datazen-jdbc-agent/build/datazen-jdbc-agent.jar` (see build below).
3. Feature enabled: include `jdbc` in `DATAZEN_DRIVERS` (registry id `jdbc`).
4. Vendor **JDBC JAR**(s) you obtain yourself (not redistributed by DataZen).

## Build the agent (monorepo)

```bash
chmod +x datazen-jdbc-agent/build.sh
./datazen-jdbc-agent/build.sh
```

Point the Host at the jar via `AgentLaunchConfig.agent_jar` (default `datazen-jdbc-agent.jar` in CWD) or copy the jar next to the app and configure Settings when UI lands.

## Connection fields

Use database type **JDBC**. Put advanced fields in connection `options`:

```json
{
  "jdbcUrl": "jdbc:h2:mem:demo;DB_CLOSE_DELAY=-1",
  "driverClass": "org.h2.Driver",
  "jars": ["/absolute/path/to/h2.jar"],
  "props": {}
}
```

Username / password use the normal connection fields.

## Capabilities vs native drivers

| Feature | JDBC Agent MVP |
|---------|----------------|
| Connect / query / execute | Yes |
| Object tree (tables/columns) | Best-effort via `DatabaseMetaData` |
| EXPLAIN / migration renderer | No |
| Schema diff / backup polish | Limited |
| Streaming large results | Batched `query.fetch`; Host may materialize |

Prefer **native** DataZen drivers when available.

## Security

- Only load JARs you trust.
- Agent speaks on stdio to the local Host only.
- Passwords stay in memory for the session; not written to agent logs.

## Architecture pointer

See [drivers architecture](../architecture/backend/drivers.md) (external Agent section) and [implementation plan](../todo/jdbc-agent-implementation-plan.md).
