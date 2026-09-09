package com.datazen.jdbcagent;

import java.io.File;
import java.net.URL;
import java.net.URLClassLoader;
import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.Driver;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/**
 * sessionId → JDBC connection + open cursors.
 *
 * <p>When HikariCP is on the classpath (default agent build), connections are
 * borrowed from a shared pool keyed by url+user+jars. Otherwise falls back to
 * one dedicated connection per session.
 */
final class SessionManager {

  private final ConcurrentHashMap<String, Session> sessions = new ConcurrentHashMap<>();
  private final PoolRegistry pools = new PoolRegistry();

  String open(String requestLine) throws Exception {
    String url = JsonLite.extractString(requestLine, "url");
    if (url == null || url.isBlank()) {
      throw AgentException.connect("url is required");
    }
    String user = JsonLite.extractString(requestLine, "user");
    String password = JsonLite.extractString(requestLine, "password");
    String driverClass = JsonLite.extractString(requestLine, "driverClass");
    List<String> jars = JsonLite.extractStringArray(requestLine, "jars");
    Map<String, String> propsMap = JsonLite.extractStringMap(requestLine, "props");

    List<URL> urls = new ArrayList<>();
    for (String jar : jars) {
      File f = new File(jar);
      if (!f.isFile()) {
        throw AgentException.driver("JAR not found: " + jar);
      }
      urls.add(f.toURI().toURL());
    }

    ClassLoader parent = ClassLoader.getSystemClassLoader();
    URLClassLoader loader =
        urls.isEmpty() ? null : new URLClassLoader(urls.toArray(new URL[0]), parent);

    Properties props = new Properties();
    if (user != null) {
      props.setProperty("user", user);
    }
    if (password != null) {
      props.setProperty("password", password);
    }
    for (Map.Entry<String, String> e : propsMap.entrySet()) {
      props.setProperty(e.getKey(), e.getValue());
    }

    boolean usePool = PoolRegistry.hikariAvailable();
    if (propsMap != null && "false".equalsIgnoreCase(propsMap.getOrDefault("pool.enabled", "true"))) {
      usePool = false;
    }
    if ("false".equalsIgnoreCase(String.valueOf(JsonLite.extractString(requestLine, "poolEnabled")))) {
      usePool = false;
    }

    Connection conn;
    String poolKey = null;
    try {
      Driver driver = null;
      if (driverClass != null && !driverClass.isBlank() && loader != null) {
        Class<?> cls = Class.forName(driverClass, true, loader);
        Object inst = cls.getDeclaredConstructor().newInstance();
        if (!(inst instanceof Driver)) {
          throw AgentException.driver("class is not java.sql.Driver: " + driverClass);
        }
        driver = (Driver) inst;
      }

      if (usePool && driver != null) {
        poolKey = PoolRegistry.poolKey(url, user, jars);
        PoolRegistry.PoolSettings settings =
            PoolRegistry.PoolSettings.fromRequest(requestLine, propsMap);
        ClassLoader prev = Thread.currentThread().getContextClassLoader();
        try {
          if (loader != null) {
            Thread.currentThread().setContextClassLoader(loader);
          }
          conn = pools.borrow(poolKey, driver, loader, url, props, settings);
          loader = null; // owned by pool
        } finally {
          Thread.currentThread().setContextClassLoader(prev);
        }
      } else if (driver != null) {
        conn = driver.connect(url, props);
        if (conn == null) {
          throw AgentException.connect("Driver.connect returned null for url");
        }
      } else if (loader != null) {
        Thread.currentThread().setContextClassLoader(loader);
        conn = java.sql.DriverManager.getConnection(url, props);
      } else {
        conn = java.sql.DriverManager.getConnection(url, props);
      }
    } catch (AgentException e) {
      throw e;
    } catch (ClassNotFoundException e) {
      throw AgentException.driver("driver class not found: " + driverClass);
    } catch (SQLException e) {
      throw AgentException.connect(e.getMessage() != null ? e.getMessage() : "SQLException");
    } catch (Exception e) {
      throw AgentException.driver(e.getMessage() != null ? e.getMessage() : e.getClass().getName());
    }

    try {
      conn.setAutoCommit(true);
    } catch (SQLException e) {
      try {
        conn.close();
      } catch (SQLException ignored) {
      }
      if (poolKey != null) {
        pools.releaseRef(poolKey);
      }
      throw AgentException.connect(e.getMessage() != null ? e.getMessage() : "setAutoCommit failed");
    }
    String id = "s-" + UUID.randomUUID();
    sessions.put(id, new Session(id, conn, loader, poolKey));
    return "{\"sessionId\":" + JsonLite.quote(id) + ",\"pooled\":" + (poolKey != null) + "}";
  }

  void close(String sessionId) throws AgentException {
    if (sessionId == null) {
      throw AgentException.internal("sessionId required");
    }
    Session s = sessions.remove(sessionId);
    if (s != null) {
      s.close(pools);
    }
  }

  void closeAll() {
    for (String id : sessions.keySet()) {
      Session s = sessions.remove(id);
      if (s != null) {
        s.close(pools);
      }
    }
    pools.closeAll();
  }

  String metaDatabases(String line) throws Exception {
    Session s = require(line);
    DatabaseMetaData md = s.conn.getMetaData();
    List<String> names = new ArrayList<>();
    try (ResultSet rs = md.getCatalogs()) {
      while (rs.next()) {
        String n = rs.getString(1);
        if (n != null) {
          names.add(n);
        }
      }
    }
    if (names.isEmpty()) {
      try (ResultSet rs = md.getSchemas()) {
        while (rs.next()) {
          String n = rs.getString(1);
          if (n != null) {
            names.add(n);
          }
        }
      }
    }
    if (names.isEmpty()) {
      names.add("default");
    }
    return stringArrayObjects(names, "name");
  }

  String metaTables(String line) throws Exception {
    Session s = require(line);
    String database = JsonLite.extractString(line, "database");
    String schema = JsonLite.extractString(line, "schema");
    DatabaseMetaData md = s.conn.getMetaData();
    StringBuilder sb = new StringBuilder("[");
    boolean first = true;
    try (ResultSet rs =
        md.getTables(database, schema, "%", new String[] {"TABLE", "VIEW", "SYSTEM TABLE"})) {
      while (rs.next()) {
        if (!first) {
          sb.append(',');
        }
        first = false;
        String name = rs.getString("TABLE_NAME");
        String type = rs.getString("TABLE_TYPE");
        if (type == null) {
          type = "TABLE";
        }
        sb.append("{\"name\":")
            .append(JsonLite.quote(name))
            .append(",\"type\":")
            .append(JsonLite.quote(type))
            .append('}');
      }
    }
    sb.append(']');
    return sb.toString();
  }

  String metaColumns(String line) throws Exception {
    Session s = require(line);
    String database = JsonLite.extractString(line, "database");
    String schema = JsonLite.extractString(line, "schema");
    String table = JsonLite.extractString(line, "table");
    if (table == null || table.isBlank()) {
      throw AgentException.internal("table is required");
    }
    DatabaseMetaData md = s.conn.getMetaData();
    StringBuilder sb = new StringBuilder("[");
    boolean first = true;
    try (ResultSet rs = md.getColumns(database, schema, table, "%")) {
      while (rs.next()) {
        if (!first) {
          sb.append(',');
        }
        first = false;
        String name = rs.getString("COLUMN_NAME");
        String type = rs.getString("TYPE_NAME");
        int nullable = rs.getInt("NULLABLE");
        sb.append("{\"name\":")
            .append(JsonLite.quote(name))
            .append(",\"type\":")
            .append(JsonLite.quote(type != null ? type : "UNKNOWN"))
            .append(",\"nullable\":")
            .append(nullable != DatabaseMetaData.columnNoNulls)
            .append('}');
      }
    }
    sb.append(']');
    return sb.toString();
  }

  String queryExecute(String line) throws Exception {
    Session s = require(line);
    String sql = JsonLite.extractString(line, "sql");
    if (sql == null || sql.isBlank()) {
      throw AgentException.sql("sql is required");
    }
    int maxRows = JsonLite.extractInt(line, "maxRows", 1000);
    int fetchSize = JsonLite.extractInt(line, "fetchSize", 500);
    if (maxRows <= 0) {
      maxRows = 1000;
    }
    if (fetchSize <= 0) {
      fetchSize = 500;
    }

    Statement st = s.conn.createStatement();
    st.setFetchSize(fetchSize);
    String executionId = "e-" + UUID.randomUUID();
    s.statements.put(executionId, st);

    boolean hasResult;
    try {
      hasResult = st.execute(sql);
    } catch (SQLException e) {
      s.statements.remove(executionId);
      try {
        st.close();
      } catch (SQLException ignored) {
      }
      throw AgentException.sql(e.getMessage() != null ? e.getMessage() : "SQLException");
    }

    if (!hasResult) {
      int updated = st.getUpdateCount();
      s.statements.remove(executionId);
      try {
        st.close();
      } catch (SQLException ignored) {
      }
      return "{\"columns\":[],\"rows\":[],\"hasMore\":false,\"updateCount\":"
          + updated
          + ",\"executionId\":"
          + JsonLite.quote(executionId)
          + "}";
    }

    ResultSet rs = st.getResultSet();
    String columns = TypeCodec.columnsJson(rs.getMetaData());
    List<String> rows = TypeCodec.readRows(rs, maxRows);
    boolean hasMore = rows.size() >= maxRows;

    if (hasMore) {
      String cursorId = "c-" + UUID.randomUUID();
      s.cursors.put(cursorId, new Cursor(st, rs, executionId));
      return "{\"columns\":"
          + columns
          + ",\"rows\":"
          + TypeCodec.rowsJson(rows)
          + ",\"hasMore\":true,\"cursorId\":"
          + JsonLite.quote(cursorId)
          + ",\"executionId\":"
          + JsonLite.quote(executionId)
          + "}";
    }

    s.statements.remove(executionId);
    try {
      rs.close();
      st.close();
    } catch (SQLException ignored) {
    }
    return "{\"columns\":"
        + columns
        + ",\"rows\":"
        + TypeCodec.rowsJson(rows)
        + ",\"hasMore\":false,\"executionId\":"
        + JsonLite.quote(executionId)
        + "}";
  }

  String queryFetch(String line) throws Exception {
    Session s = require(line);
    String cursorId = JsonLite.extractString(line, "cursorId");
    int maxRows = JsonLite.extractInt(line, "maxRows", 1000);
    Cursor c = s.cursors.get(cursorId);
    if (c == null) {
      throw AgentException.internal("unknown cursorId");
    }
    List<String> rows = TypeCodec.readRows(c.rs, maxRows);
    boolean hasMore = rows.size() >= maxRows;
    if (!hasMore) {
      s.cursors.remove(cursorId);
      s.statements.remove(c.executionId);
      try {
        c.rs.close();
        c.st.close();
      } catch (SQLException ignored) {
      }
    }
    return "{\"rows\":" + TypeCodec.rowsJson(rows) + ",\"hasMore\":" + hasMore + "}";
  }

  void queryClose(String line) throws Exception {
    Session s = require(line);
    String cursorId = JsonLite.extractString(line, "cursorId");
    Cursor c = s.cursors.remove(cursorId);
    if (c != null) {
      s.statements.remove(c.executionId);
      try {
        c.rs.close();
        c.st.close();
      } catch (SQLException ignored) {
      }
    }
  }

  void queryCancel(String line) throws Exception {
    Session s = require(line);
    String executionId = JsonLite.extractString(line, "executionId");
    Statement st = s.statements.get(executionId);
    if (st != null) {
      try {
        st.cancel();
      } catch (SQLException e) {
        throw AgentException.cancel(e.getMessage() != null ? e.getMessage() : "cancel failed");
      }
    }
  }

  String execUpdate(String line) throws Exception {
    Session s = require(line);
    String sql = JsonLite.extractString(line, "sql");
    if (sql == null) {
      throw AgentException.sql("sql is required");
    }
    try (Statement st = s.conn.createStatement()) {
      int n = st.executeUpdate(sql);
      return "{\"updateCount\":" + n + "}";
    } catch (SQLException e) {
      throw AgentException.sql(e.getMessage() != null ? e.getMessage() : "SQLException");
    }
  }

  void txBegin(String line) throws Exception {
    Session s = require(line);
    s.conn.setAutoCommit(false);
  }

  void txCommit(String line) throws Exception {
    Session s = require(line);
    s.conn.commit();
    s.conn.setAutoCommit(true);
  }

  void txRollback(String line) throws Exception {
    Session s = require(line);
    s.conn.rollback();
    s.conn.setAutoCommit(true);
  }

  private Session require(String line) throws AgentException {
    String sessionId = JsonLite.extractString(line, "sessionId");
    if (sessionId == null) {
      throw AgentException.internal("sessionId required");
    }
    Session s = sessions.get(sessionId);
    if (s == null) {
      throw AgentException.internal("unknown sessionId");
    }
    return s;
  }

  private static String stringArrayObjects(List<String> names, String field) {
    StringBuilder sb = new StringBuilder("[");
    for (int i = 0; i < names.size(); i++) {
      if (i > 0) {
        sb.append(',');
      }
      sb.append('{')
          .append(JsonLite.quote(field))
          .append(':')
          .append(JsonLite.quote(names.get(i)))
          .append('}');
    }
    sb.append(']');
    return sb.toString();
  }

  private static final class Session {
    final String id;
    final Connection conn;
    /** Non-null only for non-pooled sessions (owns the loader). */
    final URLClassLoader loader;
    /** Non-null when borrowed from {@link PoolRegistry}. */
    final String poolKey;
    final ConcurrentHashMap<String, Cursor> cursors = new ConcurrentHashMap<>();
    final ConcurrentHashMap<String, Statement> statements = new ConcurrentHashMap<>();

    Session(String id, Connection conn, URLClassLoader loader, String poolKey) {
      this.id = id;
      this.conn = conn;
      this.loader = loader;
      this.poolKey = poolKey;
    }

    void close(PoolRegistry pools) {
      for (Cursor c : cursors.values()) {
        try {
          c.rs.close();
        } catch (Exception ignored) {
        }
        try {
          c.st.close();
        } catch (Exception ignored) {
        }
      }
      cursors.clear();
      statements.clear();
      try {
        conn.close();
      } catch (Exception ignored) {
      }
      if (poolKey != null) {
        pools.releaseRef(poolKey);
      } else if (loader != null) {
        try {
          loader.close();
        } catch (Exception ignored) {
        }
      }
    }
  }

  private static final class Cursor {
    final Statement st;
    final ResultSet rs;
    final String executionId;

    Cursor(Statement st, ResultSet rs, String executionId) {
      this.st = st;
      this.rs = rs;
      this.executionId = executionId;
    }
  }
}
