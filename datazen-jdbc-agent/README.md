# datazen-jdbc-agent

External Java Agent for DataZen generic JDBC connections.

- **Java:** 17+ (21 recommended for CI)
- **Protocol:** JSON-RPC 2.0 over stdio (one JSON object per line)
- **Contract:** [`packages/drivers/jdbc/protocol.md`](../packages/drivers/jdbc/protocol.md)
- **Plan:** [`docs/todo/jdbc-agent-implementation-plan.md`](../docs/todo/jdbc-agent-implementation-plan.md)

## Phase 0 status

Implements `agent.hello` only. Session / query / meta land in later phases.

## Build & run (JDK 17+)

```bash
mkdir -p build/classes
javac -encoding UTF-8 -source 17 -target 17 -d build/classes \
  src/main/java/com/datazen/jdbcagent/AgentMain.java

jar cfe build/datazen-jdbc-agent.jar com.datazen.jdbcagent.AgentMain \
  -C build/classes .

echo '{"jsonrpc":"2.0","id":1,"method":"agent.hello","params":{"hostVersion":"dev","protocolVersion":1}}' \
  | java -jar build/datazen-jdbc-agent.jar
```

Expected stdout line contains `"protocolVersion":1` and `"agentVersion":"0.1.0"`.

## Design notes

- stdout = protocol only; stderr = logs
- Do not bundle vendor JDBC JARs; Host passes jar paths in `session.open` (Phase 2)
- Single shared Agent process per DataZen app (Host-side policy)
