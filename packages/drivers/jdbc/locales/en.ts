const jdbcEn = {
  'jdbc.form.capabilityHint':
    'Generic JDBC via external Java agent — limited features vs native drivers (no EXPLAIN / migration renderer).',
  'jdbc.form.jdbcUrl': 'JDBC URL',
  'jdbc.form.urlRequired': 'JDBC URL is required',
  'jdbc.form.urlMustStartWithJdbc': 'URL must start with jdbc:',
  'jdbc.form.driverClass': 'Driver class',
  'jdbc.form.driverClassHint': 'Optional if the JAR registers via SPI; recommended for reliability.',
  'jdbc.form.jars': 'JDBC driver JARs',
  'jdbc.form.jarsRequired': 'At least one driver JAR path is required',
  'jdbc.form.jarsHint': 'One absolute path per line. JARs are not redistributed by DataZen.',
  'jdbc.form.agentJar': 'Agent JAR (optional override)',
  'jdbc.form.agentJarHint': 'Defaults to Settings / datazen-jdbc-agent.jar on the agent search path.',
} as const;

export type JdbcTranslationKey = keyof typeof jdbcEn;
export default jdbcEn;
