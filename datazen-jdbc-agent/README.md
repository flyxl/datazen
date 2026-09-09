# datazen-jdbc-agent

External Java Agent for DataZen generic JDBC (monorepo path `datazen-jdbc-agent/`).

- **Java:** 17+
- **Protocol:** JSON-RPC 2.0 over stdio — [`packages/drivers/jdbc/protocol.md`](../packages/drivers/jdbc/protocol.md)
- **Deps:** none (pure JDK). Vendor JDBC JARs supplied at `session.open` via `jars[]`.

## Build

```bash
./datazen-jdbc-agent/build.sh
# → datazen-jdbc-agent/build/datazen-jdbc-agent.jar
```

## Run (smoke)

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"agent.hello","params":{"hostVersion":"dev","protocolVersion":1}}' \
  | java -jar datazen-jdbc-agent/build/datazen-jdbc-agent.jar
```

With H2 (download h2.jar yourself):

```bash
# session.open with jars:["/path/h2.jar"], driverClass:"org.h2.Driver",
# url:"jdbc:h2:mem:test;DB_CLOSE_DELAY=-1", user:"sa", password:""
```

## Methods

`agent.hello` / `agent.shutdown` / `session.*` / `meta.*` / `query.*` / `exec.update` / `tx.*`
