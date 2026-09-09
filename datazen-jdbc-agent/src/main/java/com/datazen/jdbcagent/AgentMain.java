package com.datazen.jdbcagent;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.Locale;

/**
 * JDBC Agent entry: JSON-RPC 2.0 over stdio (one object per line).
 *
 * <p>stdout = protocol only; stderr = diagnostics. Driver {@code System.out}
 * is redirected to stderr at startup.
 */
public final class AgentMain {

  public static final String AGENT_VERSION = "0.1.0";
  public static final int PROTOCOL_VERSION = 1;

  private AgentMain() {}

  public static void main(String[] args) {
    // Prevent JDBC drivers from corrupting the protocol stream.
    System.setOut(System.err);

    PrintStream protocolOut =
        new PrintStream(new FileOutputStreamAdapter(FileDescriptor.out), true, StandardCharsets.UTF_8);
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
          // shutdown requested
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

  /** Minimal adapter so we can keep a dedicated protocol stdout after System.setOut. */
  private static final class FileOutputStreamAdapter extends java.io.OutputStream {
    private final java.io.FileOutputStream inner;

    FileOutputStreamAdapter(java.io.FileDescriptor fd) {
      this.inner = new java.io.FileOutputStream(fd);
    }

    @Override
    public void write(int b) throws java.io.IOException {
      inner.write(b);
    }

    @Override
    public void write(byte[] b, int off, int len) throws java.io.IOException {
      inner.write(b, off, len);
    }

    @Override
    public void flush() throws java.io.IOException {
      inner.flush();
    }
  }
}
