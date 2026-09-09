package com.datazen.jdbcagent;

import java.io.PrintStream;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Dispatches JSON-RPC methods. Hand-rolled JSON to stay dependency-free. */
final class JsonRpcLoop {

  private final SessionManager sessions;
  private final PrintStream out;
  private final PrintStream log;

  JsonRpcLoop(SessionManager sessions, PrintStream out, PrintStream log) {
    this.sessions = sessions;
    this.out = out;
    this.log = log;
  }

  /** @return true if agent should exit */
  boolean handleLine(String line) {
    String id = JsonLite.extractRaw(line, "id");
    String method = JsonLite.extractString(line, "method");
    if (method == null) {
      writeError(id, -32600, "Invalid Request: missing method", "internal");
      return false;
    }
    try {
      switch (method) {
        case "agent.hello" -> writeHello(id);
        case "agent.shutdown" -> {
          writeResult(id, "{\"ok\":true}");
          return true;
        }
        case "session.open" -> writeResult(id, sessions.open(line));
        case "session.close" -> {
          sessions.close(JsonLite.extractString(line, "sessionId"));
          writeResult(id, "{\"ok\":true}");
        }
        case "meta.databases" -> writeResult(id, sessions.metaDatabases(line));
        case "meta.tables" -> writeResult(id, sessions.metaTables(line));
        case "meta.columns" -> writeResult(id, sessions.metaColumns(line));
        case "query.execute" -> writeResult(id, sessions.queryExecute(line));
        case "query.fetch" -> writeResult(id, sessions.queryFetch(line));
        case "query.close" -> {
          sessions.queryClose(line);
          writeResult(id, "{\"ok\":true}");
        }
        case "query.cancel" -> {
          sessions.queryCancel(line);
          writeResult(id, "{\"ok\":true}");
        }
        case "exec.update" -> writeResult(id, sessions.execUpdate(line));
        case "tx.begin" -> {
          sessions.txBegin(line);
          writeResult(id, "{\"ok\":true}");
        }
        case "tx.commit" -> {
          sessions.txCommit(line);
          writeResult(id, "{\"ok\":true}");
        }
        case "tx.rollback" -> {
          sessions.txRollback(line);
          writeResult(id, "{\"ok\":true}");
        }
        default -> writeError(
            id, -32601, "Method not found: " + method, "internal");
      }
    } catch (AgentException e) {
      writeError(id, e.code, e.getMessage(), e.category);
    } catch (Exception e) {
      log.println("[jdbc-agent] " + method + ": " + e);
      writeError(id, -32000, safeMsg(e), "internal");
    }
    return false;
  }

  private void writeHello(String id) {
    writeResult(
        id,
        "{"
            + "\"agentVersion\":\""
            + AgentMain.AGENT_VERSION
            + "\","
            + "\"protocolVersion\":"
            + AgentMain.PROTOCOL_VERSION
            + ","
            + "\"capabilities\":[\"jdbc\",\"session\",\"query.stream\",\"tx\"]"
            + "}");
  }

  void writeResult(String id, String resultJson) {
    String idField = id == null ? "null" : id;
    out.println("{\"jsonrpc\":\"2.0\",\"id\":" + idField + ",\"result\":" + resultJson + "}");
    out.flush();
  }

  void writeError(String id, int code, String message, String category) {
    String idField = id == null ? "null" : id;
    out.println(
        "{\"jsonrpc\":\"2.0\",\"id\":"
            + idField
            + ",\"error\":{"
            + "\"code\":"
            + code
            + ",\"message\":\""
            + JsonLite.escape(message)
            + "\","
            + "\"data\":{\"category\":\""
            + category
            + "\"}}}");
    out.flush();
  }

  private static String safeMsg(Exception e) {
    String m = e.getMessage();
    if (m == null || m.isEmpty()) {
      return e.getClass().getSimpleName();
    }
    // Redact common password query params.
    return m.replaceAll("(?i)(password=)[^&;\\s]+", "$1***");
  }
}
