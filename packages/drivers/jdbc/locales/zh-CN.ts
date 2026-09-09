const jdbcZhCN = {
  'jdbc.form.capabilityHint':
    '通过外部 Java Agent 的通用 JDBC — 能力弱于原生驱动（无 EXPLAIN / migration renderer）。',
  'jdbc.form.inferredProduct': '识别为：{{product}}。',
  'jdbc.form.jdbcUrl': 'JDBC URL',
  'jdbc.form.urlRequired': '请填写 JDBC URL',
  'jdbc.form.urlMustStartWithJdbc': 'URL 必须以 jdbc: 开头',
  'jdbc.form.driverClass': '驱动类名',
  'jdbc.form.driverClassHint':
    '若 JAR 已通过 SPI 注册可留空；建议填写。URL 变更且类名为空时会自动推断。',
  'jdbc.form.useInferred': '使用推断类名',
  'jdbc.form.jars': 'JDBC 驱动 JAR',
  'jdbc.form.jarsRequired': '至少需要一个驱动 JAR 路径',
  'jdbc.form.jarsHint': '每行一个绝对路径，或点「浏览」多选。DataZen 不会分发厂商 JAR。',
  'jdbc.form.browseJars': '浏览 JAR…',
  'jdbc.form.agentJar': 'Agent JAR（可选覆盖）',
  'jdbc.form.agentJarHint': '默认使用设置项或搜索路径中的 datazen-jdbc-agent.jar。',
} as const;

export default jdbcZhCN;
