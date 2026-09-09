package com.datazen.jdbcagent;

import com.zaxxer.hikari.HikariConfig;
import com.zaxxer.hikari.HikariDataSource;
import java.net.URLClassLoader;
import java.sql.Connection;
import java.sql.Driver;
import java.sql.SQLException;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Shared Hikari pools per connection identity (url + user + jars).
 *
 * <p>Vendor drivers stay on a per-pool {@link URLClassLoader}; Hikari borrows via
 * {@link DriverDataSource}. Sessions call {@link Connection#close()} to return to the pool.
 */
final class PoolRegistry {

  private final ConcurrentHashMap<String, PoolEntry> pools = new ConcurrentHashMap<>();

  static boolean hikariAvailable() {
    try {
      Class.forName("com.zaxxer.hikari.HikariDataSource");
      return true;
    } catch (ClassNotFoundException e) {
      return false;
    }
  }

  static String poolKey(String url, String user, List<String> jars) {
    StringBuilder sb = new StringBuilder(url == null ? "" : url);
    sb.append('\0').append(user == null ? "" : user);
    if (jars != null) {
      for (String j : jars) {
        sb.append('\0').append(j);
      }
    }
    return sb.toString();
  }

  /** Borrow a pooled connection; caller must {@link Connection#close()} when done. */
  Connection borrow(
      String key,
      Driver driver,
      URLClassLoader loader,
      String url,
      Properties props,
      PoolSettings settings)
      throws SQLException {
    PoolEntry entry =
        pools.compute(
            key,
            (k, existing) -> {
              if (existing != null && !existing.ds.isClosed()) {
                existing.refCount++;
                return existing;
              }
              HikariConfig cfg = new HikariConfig();
              cfg.setDataSource(new DriverDataSource(driver, url, props));
              cfg.setPoolName("datazen-jdbc-" + Integer.toHexString(k.hashCode()));
              cfg.setMaximumPoolSize(settings.maximumPoolSize);
              cfg.setMinimumIdle(settings.minimumIdle);
              cfg.setConnectionTimeout(settings.connectionTimeoutMs);
              cfg.setIdleTimeout(settings.idleTimeoutMs);
              cfg.setMaxLifetime(settings.maxLifetimeMs);
              cfg.setAutoCommit(true);
              cfg.setInitializationFailTimeout(10_000);
              HikariDataSource ds = new HikariDataSource(cfg);
              PoolEntry created = new PoolEntry(ds, loader);
              created.refCount = 1;
              return created;
            });
    try {
      return entry.ds.getConnection();
    } catch (SQLException e) {
      releaseRef(key);
      throw e;
    }
  }

  void releaseRef(String key) {
    pools.computeIfPresent(
        key,
        (k, entry) -> {
          entry.refCount--;
          if (entry.refCount <= 0) {
            try {
              entry.ds.close();
            } catch (Exception ignored) {
            }
            if (entry.loader != null) {
              try {
                entry.loader.close();
              } catch (Exception ignored) {
              }
            }
            return null;
          }
          return entry;
        });
  }

  void closeAll() {
    for (String key : pools.keySet()) {
      PoolEntry e = pools.remove(key);
      if (e != null) {
        try {
          e.ds.close();
        } catch (Exception ignored) {
        }
        if (e.loader != null) {
          try {
            e.loader.close();
          } catch (Exception ignored) {
          }
        }
      }
    }
  }

  static final class PoolSettings {
    int maximumPoolSize = 5;
    int minimumIdle = 0;
    long connectionTimeoutMs = 30_000;
    long idleTimeoutMs = 600_000;
    long maxLifetimeMs = 1_800_000;

    static PoolSettings fromRequest(String requestLine, Map<String, String> propsMap) {
      PoolSettings s = new PoolSettings();
      if (propsMap != null) {
        applyInt(propsMap, "pool.maximumPoolSize", v -> s.maximumPoolSize = v);
        applyInt(propsMap, "maximumPoolSize", v -> s.maximumPoolSize = v);
        applyInt(propsMap, "pool.minimumIdle", v -> s.minimumIdle = v);
        applyLong(propsMap, "pool.connectionTimeoutMs", v -> s.connectionTimeoutMs = v);
        applyLong(propsMap, "pool.idleTimeoutMs", v -> s.idleTimeoutMs = v);
        applyLong(propsMap, "pool.maxLifetimeMs", v -> s.maxLifetimeMs = v);
      }
      int max = JsonLite.extractInt(requestLine, "maximumPoolSize", -1);
      if (max > 0) {
        s.maximumPoolSize = max;
      }
      int minIdle = JsonLite.extractInt(requestLine, "minimumIdle", -1);
      if (minIdle >= 0) {
        s.minimumIdle = minIdle;
      }
      if (s.maximumPoolSize < 1) {
        s.maximumPoolSize = 1;
      }
      if (s.maximumPoolSize > 32) {
        s.maximumPoolSize = 32;
      }
      if (s.minimumIdle > s.maximumPoolSize) {
        s.minimumIdle = s.maximumPoolSize;
      }
      return s;
    }

    private static void applyInt(Map<String, String> map, String key, java.util.function.IntConsumer c) {
      String v = map.get(key);
      if (v == null) {
        return;
      }
      try {
        c.accept(Integer.parseInt(v.trim()));
      } catch (NumberFormatException ignored) {
      }
    }

    private static void applyLong(
        Map<String, String> map, String key, java.util.function.LongConsumer c) {
      String v = map.get(key);
      if (v == null) {
        return;
      }
      try {
        c.accept(Long.parseLong(v.trim()));
      } catch (NumberFormatException ignored) {
      }
    }
  }

  private static final class PoolEntry {
    final HikariDataSource ds;
    final URLClassLoader loader;
    int refCount;

    PoolEntry(HikariDataSource ds, URLClassLoader loader) {
      this.ds = ds;
      this.loader = loader;
    }
  }
}
