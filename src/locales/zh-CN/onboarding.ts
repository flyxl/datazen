const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': '欢迎使用 DataZen',
  'onboarding.sidebar.desc':
    '已经有数据库客户端？几秒内导入连接——或从零开始。所有数据仅存储在本机。',
  'onboarding.sidebar.importTitle': '一键导入',
  'onboarding.sidebar.importDesc': 'DBeaver、DataGrip、Navicat、TablePlus。',
  'onboarding.sidebar.aiTitle': 'AI 原生',
  'onboarding.sidebar.aiDesc': '感知 Schema 的 SQL 辅助。',
  'onboarding.sidebar.dashboardTitle': '可视化仪表盘',
  'onboarding.sidebar.dashboardDesc': '查询结果秒变图表，报告无忧。',

  // S0 — Welcome
  'onboarding.s0.title': '你想如何开始？',
  'onboarding.s0.subtitle': '大多数人从已有客户端导入。其他方式也只需一分钟。',
  'onboarding.s0.importCard': '导入连接',
  'onboarding.s0.importCardDesc': '从 DBeaver、DataGrip、Navicat、TablePlus 或文件导入',
  'onboarding.s0.importFastest': '最快',
  'onboarding.s0.importDetectedApp': '✓ 检测到本机 {app} 配置',
  'onboarding.s0.manualCard': '手动创建连接',
  'onboarding.s0.manualCardDesc': 'PostgreSQL、MySQL、SQLite、Redis 等',
  'onboarding.s0.sampleCard': '浏览示例数据',
  'onboarding.s0.sampleCardDesc': '本地 SQLite 演示——无需任何配置',

  // S1 — Connect（按入口分流：连接表单 / 内联导入 / 示例数据）
  'onboarding.s1.title': '创建你的第一个连接',
  'onboarding.s1.subtitle':
    '与新建连接对话框相同的表单——相同的验证、相同的测试。<b>保存后即可继续。</b>',
  'onboarding.s1.stepLabel': '步骤 1 / 2',
  'onboarding.s1.driver': '驱动',
  'onboarding.s1.testBtn': '测试连接',
  'onboarding.s1.testSuccess': '已连接',
  'onboarding.s1.testFail': '连接失败',
  'onboarding.s1.importTitle': '导入你的连接',
  'onboarding.s1.importSubtitle':
    '选择你要迁移的客户端，或选一个连接文件。连接会直接进入你的工作区，数据不会离开本机。',
  'onboarding.s1.importFileSource': '文件',
  'onboarding.s1.importing': '导入中…',
  'onboarding.s1.importDetecting': '正在查找客户端配置…',
  'onboarding.s1.importSuccess': '✓ 已导入 {count} 个连接 · {source}',
  'onboarding.s1.importUpdated': '✓ 已更新 {count} 个连接 · {source}',
  'onboarding.s1.sampleTitle': '你的示例数据库',
  'onboarding.s1.sampleSubtitle':
    '内置 SQLite 数据库，包含英文 demo_sales 表——无需服务器、无需凭据，直接可查。',
  'onboarding.s1.samplePreparing': '正在准备示例数据…',
  'onboarding.s1.sampleReady': '示例数据已就绪',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 个区域 × 2 个季度——已连接，在你的工作区中等候。',
  'onboarding.s1.sampleConnection': '连接：{name} · SQLite',
  'onboarding.s1.sampleFailed': '示例数据准备失败',

  // S2 — AI（永远是第二步）
  'onboarding.s2.title': '设置 AI 辅助',
  'onboarding.s2.subtitle':
    'DataZen 使用你自己的 API 密钥来生成 SQL、解释错误和分析查询计划。Ollama 在本地运行，无需密钥。你可以跳过此步骤，稍后再配置。',
  'onboarding.s2.stepLabel': '步骤 2 / 2',
  'onboarding.s2.optionalHint': '可选——可稍后在“设置 → AI”中配置。',
  'onboarding.s2.securityNote':
    '你的密钥使用 AES-256-GCM 加密并存储在本地。请求直接发送到你的提供商——绝不经过 DataZen 服务器。我们没有服务器。',

  // S3 — Done
  'onboarding.s3.title': '一切就绪',
  'onboarding.s3.subtitle':
    '你的工作区已准备就绪。DataZen 将打开你的连接，你可以立即开始编写查询。',
  'onboarding.s3.connLabel': '连接',
  'onboarding.s3.aiLabel': 'AI 提供商',
  'onboarding.s3.storageLabel': '存储',
  'onboarding.s3.storageValue': '本地加密 · AES-256-GCM',
  'onboarding.s3.importedValue': '已导入 {count} 个连接 · {source}',
  'onboarding.s3.updatedValue': '已更新 {count} 个连接 · {source}',
  'onboarding.s3.aiNotConfigured': '未配置（可选）',
  'onboarding.s3.notConfigured': '未配置（可稍后添加）',
  'onboarding.s3.openBtn': '打开 DataZen',
  'onboarding.s3.nextHint': '下一步：查询已就绪 — 点击<b>执行</b>，然后<b>添加到仪表板</b>',

  // Common
  'onboarding.common.skip': '跳过设置',
  'onboarding.common.back': '返回',
  'onboarding.common.continue': '继续',
  'onboarding.common.finish': '完成',
} as const;

export default onboarding;
