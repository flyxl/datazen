package com.datazen.jdbcagent;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;

/**
 * Phase 0 JDBC Agent: stdio JSON-RPC loop with {@code agent.hello} only.
 *
 * <p>No third-party JSON library - minimal hand parsing for hello/shutdown so
 * the jar stays dependency-free until Phase 2 (session/query).
 */
public final class AgentMain {

  public static final String AGENT_VERSION = "0.1.0";
  public static final int PROTOCOL_VERSION = 1;

  private AgentMain() {}

  public static void main(String[] args) {
    PrintStream protocolOut = System.out;
    PrintStream log = System.err;

    try {
      BufferedReader in =
          new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
      String line;
      while ((line = in.readLine()) != null) {
        line = line.trim();
        if (line.isEmpty()) {
          continue;
        }
        if (!line.startsWith("{")) {
          log.println("[jdbc-agent] skip non-json line");
          continue;
        }
        handleLine(line, protocolOut, log);
      }
    } catch (Exception e) {
      log.println("[jdbc-agent] fatal: " + e.getMessage());
      System.exit(1);
    }
  }

  static void handleLine(String line, PrintStream out, PrintStream log) {
    String id = extractJsonValue(line, "id");
    String method = extractJsonString(line, "method");
    if (method == null) {
      writeError(out, id, -32600, "Invalid Request: missing method", "internal");
      return;
    }
    switch (method) {
      case "agent.hello" -> writeHello(out, id);
      case "agent.shutdown" -> {
        writeResult(out, id, "{\"ok\":true}");
        System.exit(0);
      }
      default -> writeError(
          out,
          id,
          -32601,
          "Method not found: " + method + " (Phase 0 agent implements agent.hello only)",
          "internal");
    }
  }

  static void writeHello(PrintStream out, String id) {
    String result =
        "{"
            + "\"agentVersion\":\""
            + AGENT_VERSION
            + "\","
            + "\"protocolVersion\":"
            + PROTOCOL_VERSION
            + ","
            + "\"capabilities\":[\"jdbc\",\"session\",\"query.stream\",\"tx\"]"
            + "}";
    writeResult(out, id, result);
  }

  static void writeResult(PrintStream out, String id, String resultJson) {
    String idField = id == null ? "null" : id;
    out.println(
        "{\"jsonrpc\":\"2.0\",\"id\":" + idField + ",\"result\":" + resultJson + "}");
    out.flush();
  }

  static void writeError(
      PrintStream out, String id, int code, String message, String category) {
    String idField = id == null ? "null" : id;
    String safe = message.replace("\\", "\\\\").replace("\"", "\\\"");
    out.println(
        "{\"jsonrpc\":\"2.0\",\"id\":"
            + idField
            + ",\"error\":{"
            + "\"code\":"
            + code
            + ",\"message\":\""
            + safe
            + "\","
            + "\"data\":{\"category\":\""
            + category
            + "\"}}}");
    out.flush();
  }

  static String extractJsonValue(String json, String key) {
    String pattern = "\"" + key + "\"";
    int i = json.indexOf(pattern);
    if (i < 0) {
      return null;
    }
    int colon = json.indexOf(':', i + pattern.length());
    if (colon < 0) {
      return null;
    }
    int j = colon + 1;
    while (j < json.length() && Character.isWhitespace(json.charAt(j))) {
      j++;
    }
    if (j >= json.length()) {
      return null;
    }
    if (json.charAt(j) == '"') {
      int end = json.indexOf('"', j + 1);
      if (end < 0) {
        return null;
      }
      return "\"" + json.substring(j + 1, end) + "\"";
    }
    int end = j;
    while (end < json.length()) {
      char c = json.charAt(end);
      if (c == ',' || c == '}' || Character.isWhitespace(c)) {
        break;
      }
      end++;
    }
    return json.substring(j, end);
  }

  static String extractJsonString(String json, String key) {
    String pattern = "\"" + key + "\"";
    int i = json.indexOf(pattern);
    if (i < 0) {
      return null;
    }
    int colon = json.indexOf(':', i + pattern.length());
    if (colon < 0) {
      return null;
    }
    int q1 = json.indexOf('"', colon + 1);
    if (q1 < 0) {
      return null;
    }
    int q2 = json.indexOf('"', q1 + 1);
    if (q2 < 0) {
      return null;
    }
    return json.substring(q1 + 1, q2);
  }
}
