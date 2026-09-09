package com.datazen.jdbcagent;

import java.io.PrintWriter;
import java.sql.Connection;
import java.sql.Driver;
import java.sql.SQLException;
import java.sql.SQLFeatureNotSupportedException;
import java.util.Properties;
import java.util.logging.Logger;
import javax.sql.DataSource;

/**
 * Minimal {@link DataSource} that obtains connections from a {@link Driver} loaded
 * via a session-specific {@link java.net.URLClassLoader}. Used as HikariCP's
 * underlying data source so vendor JARs need not sit on the agent classpath.
 */
final class DriverDataSource implements DataSource {

  private final Driver driver;
  private final String url;
  private final Properties props;

  DriverDataSource(Driver driver, String url, Properties props) {
    this.driver = driver;
    this.url = url;
    this.props = props;
  }

  @Override
  public Connection getConnection() throws SQLException {
    Connection c = driver.connect(url, props);
    if (c == null) {
      throw new SQLException("Driver.connect returned null for " + url);
    }
    return c;
  }

  @Override
  public Connection getConnection(String username, String password) throws SQLException {
    Properties p = new Properties(props);
    if (username != null) {
      p.setProperty("user", username);
    }
    if (password != null) {
      p.setProperty("password", password);
    }
    Connection c = driver.connect(url, p);
    if (c == null) {
      throw new SQLException("Driver.connect returned null for " + url);
    }
    return c;
  }

  @Override
  public PrintWriter getLogWriter() {
    return null;
  }

  @Override
  public void setLogWriter(PrintWriter out) {}

  @Override
  public void setLoginTimeout(int seconds) {}

  @Override
  public int getLoginTimeout() {
    return 0;
  }

  @Override
  public Logger getParentLogger() throws SQLFeatureNotSupportedException {
    throw new SQLFeatureNotSupportedException();
  }

  @Override
  public <T> T unwrap(Class<T> iface) throws SQLException {
    if (iface.isInstance(this)) {
      return iface.cast(this);
    }
    throw new SQLException("not a wrapper for " + iface);
  }

  @Override
  public boolean isWrapperFor(Class<?> iface) {
    return iface.isInstance(this);
  }
}
