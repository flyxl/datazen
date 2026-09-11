const onboarding = {
  // S0 — 欢迎
  'onboarding.s0.title': '开始使用 DataZen',
  'onboarding.s0.subtitle':
    '连接数据库或浏览示例数据集，快速上手。',
  'onboarding.s0.importCard': '导入连接',
  'onboarding.s0.importCardDesc': '从 DBeaver、DataGrip、Navicat 或 TablePlus 导入配置。',
  'onboarding.s0.importFastest': '最快',
  'onboarding.s0.manualCard': '新建连接',
  'onboarding.s0.manualCardDesc': '从零开始配置一个数据库连接。',
  'onboarding.s0.sampleCard': '浏览示例数据',
  'onboarding.s0.sampleCardDesc': '打开内置演示数据集——无需任何配置。',

  // S1 — 连接
  'onboarding.s1.title': '连接数据库',
  'onboarding.s1.subtitle':
    '复用你之后会看到的同一个连接表单——随时可以修改。',
  'onboarding.s1.driver': '驱动',
  'onboarding.s1.host': '主机',
  'onboarding.s1.port': '端口',
  'onboarding.s1.user': '用户',
  'onboarding.s1.password': '密码',
  'onboarding.s1.database': '数据库',
  'onboarding.s1.testBtn': '测试连接',
  'onboarding.s1.testSuccess': '已连接',
  'onboarding.s1.testFail': '连接失败',

  // S2 — AI
  'onboarding.s2.title': '设置 AI 辅助',
  'onboarding.s2.subtitle':
    '可选——你可以稍后在设置中添加或更改。',
  'onboarding.s2.apiKey': 'API 密钥',
  'onboarding.s2.skipHint': '你可以跳过此步骤，稍后再配置 AI。',

  // S3 — 完成
  'onboarding.s3.title': '一切就绪',
  'onboarding.s3.subtitle':
    'DataZen 已准备好。你可以在设置中随时修改这些选项。',
  'onboarding.s3.connLabel': '连接',
  'onboarding.s3.aiLabel': 'AI 提供商',
  'onboarding.s3.storageLabel': '存储',
  'onboarding.s3.openBtn': '打开 DataZen',
  'onboarding.s3.nextHint':
    '下一步：运行预置查询，然后点击"添加到仪表板"',

  // 通用
  'onboarding.common.skip': '跳过设置',
  'onboarding.common.back': '返回',
  'onboarding.common.continue': '继续',
  'onboarding.common.finish': '完成',
} as const;

export default onboarding;
