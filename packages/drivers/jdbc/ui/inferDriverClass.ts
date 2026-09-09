/** Well-known JDBC URL prefixes → driver class (best-effort, optional). */
const PREFIX_TO_DRIVER: Array<{ prefix: string; driverClass: string; label: string }> = [
  { prefix: 'jdbc:h2:', driverClass: 'org.h2.Driver', label: 'H2' },
  { prefix: 'jdbc:mysql:', driverClass: 'com.mysql.cj.jdbc.Driver', label: 'MySQL' },
  { prefix: 'jdbc:mariadb:', driverClass: 'org.mariadb.jdbc.Driver', label: 'MariaDB' },
  { prefix: 'jdbc:postgresql:', driverClass: 'org.postgresql.Driver', label: 'PostgreSQL' },
  { prefix: 'jdbc:oracle:', driverClass: 'oracle.jdbc.OracleDriver', label: 'Oracle' },
  {
    prefix: 'jdbc:sqlserver:',
    driverClass: 'com.microsoft.sqlserver.jdbc.SQLServerDriver',
    label: 'SQL Server',
  },
  { prefix: 'jdbc:sqlite:', driverClass: 'org.sqlite.JDBC', label: 'SQLite' },
  { prefix: 'jdbc:db2:', driverClass: 'com.ibm.db.jdbc.DB2Driver', label: 'DB2' },
  { prefix: 'jdbc:hive2:', driverClass: 'org.apache.hive.jdbc.HiveDriver', label: 'Hive' },
  {
    prefix: 'jdbc:clickhouse:',
    driverClass: 'com.clickhouse.jdbc.ClickHouseDriver',
    label: 'ClickHouse',
  },
  { prefix: 'jdbc:presto:', driverClass: 'com.facebook.presto.jdbc.PrestoDriver', label: 'Presto' },
  { prefix: 'jdbc:trino:', driverClass: 'io.trino.jdbc.TrinoDriver', label: 'Trino' },
];

export type InferredJdbcDriver = {
  driverClass: string;
  label: string;
};

export function inferDriverClass(jdbcUrl: string): InferredJdbcDriver | undefined {
  const u = jdbcUrl.trim().toLowerCase();
  if (!u.startsWith('jdbc:')) return undefined;
  for (const row of PREFIX_TO_DRIVER) {
    if (u.startsWith(row.prefix)) {
      return { driverClass: row.driverClass, label: row.label };
    }
  }
  return undefined;
}

/** Logical product family for UI hints (not a native DataZen driver id). */
export function inferJdbcProductLabel(jdbcUrl: string): string | undefined {
  return inferDriverClass(jdbcUrl)?.label;
}

export const JDBC_DRIVER_PRESETS = PREFIX_TO_DRIVER;
