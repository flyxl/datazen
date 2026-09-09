package com.datazen.jdbcagent;

import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;

/**
 * JDBC Agent entry: JSON-RPC 2.0 over stdio (one object per line).
 *
 * <p>stdout = protocol only; stderr = diagnostics. Driver {@code System.out}
 * is redirected to stderr at startup so JDBC drivers cannot corrupt the wire.
 */
public final class AgentMain {

  public static final String AGENT_VERSION = "0.1.0";
  public static final int PROTOCOL_VERSION = 1;

  private AgentMain() {}

  public static void main(String[] args) {
    // Keep a dedicated stream on the real stdout FD before hijacking System.out.
    PrintStream protocolOut =
        new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
    System.setOut(System.err);

    PrintStream log = System.err;
    SessionManager sessions = new SessionManager();
    JsonRpcLoop loop = new JsonRpcLoop(sessions, protocolOut, log);

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
        if (loop.handleLine(line)) {
          break;
        }
      }
    } catch (Exception e) {
      log.println("[jdbc-agent] fatal: " + e.getMessage());
      System.exit(1);
    } finally {
      sessions.closeAll();
    }
  }
}
