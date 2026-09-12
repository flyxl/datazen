const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': '歡迎使用 DataZen',
  'onboarding.sidebar.desc':
    '已經有資料庫客戶端？幾秒內匯入連線——或從零開始。所有資料僅儲存在本機。',
  'onboarding.sidebar.importTitle': '一鍵匯入',
  'onboarding.sidebar.importDesc': 'DBeaver、DataGrip、Navicat、TablePlus。',
  'onboarding.sidebar.aiTitle': 'AI 原生',
  'onboarding.sidebar.aiDesc': '感知 Schema 的 SQL 輔助。',
  'onboarding.sidebar.dashboardTitle': '視覺化儀表板',
  'onboarding.sidebar.dashboardDesc': '查詢結果秒變圖表，報告無憂。',

  // S0 — Welcome
  'onboarding.s0.title': '你想如何開始？',
  'onboarding.s0.subtitle': '大多數人從已有客戶端匯入。其他方式也只需一分鐘。',
  'onboarding.s0.importCard': '匯入連線',
  'onboarding.s0.importCardDesc': '從 DBeaver、DataGrip、Navicat、TablePlus 或檔案匯入',
  'onboarding.s0.importFastest': '最快',
  'onboarding.s0.importDetectedApp': '✓ 偵測到本機 {app} 設定',
  'onboarding.s0.manualCard': '手動建立連線',
  'onboarding.s0.manualCardDesc': 'PostgreSQL、MySQL、SQLite、Redis 等',
  'onboarding.s0.sampleCard': '瀏覽範例資料',
  'onboarding.s0.sampleCardDesc': '本機 SQLite 演示——無需任何設定',

  // S1 — Connect
  'onboarding.s1.title': '建立你的第一個連線',
  'onboarding.s1.subtitle':
    '與新建連線對話框相同的表單——相同的驗證、相同的測試。<b>儲存後即可繼續。</b>',
  'onboarding.s1.stepLabel': '步驟 1 / 2',
  'onboarding.s1.driver': '驅動',
  'onboarding.s1.testBtn': '測試連線',
  'onboarding.s1.testSuccess': '已連線',
  'onboarding.s1.testFail': '連線失敗',
  'onboarding.s1.importTitle': '匯入你的連線',
  'onboarding.s1.importSubtitle':
    '選擇你要遷移的客戶端，或選一個連線檔案。連線會直接進入你的工作區，資料不會離開本機。',
  'onboarding.s1.importFileSource': '檔案',
  'onboarding.s1.importing': '匯入中…',
  'onboarding.s1.importDetecting': '正在搜尋客戶端設定…',
  'onboarding.s1.importSuccess': '✓ 已匯入 {count} 個連線 · {source}',
  'onboarding.s1.importUpdated': '✓ 已更新 {count} 個連線 · {source}',
  'onboarding.s1.sampleTitle': '你的範例資料庫',
  'onboarding.s1.sampleSubtitle':
    '內建 SQLite 資料庫，包含英文 demo_sales 資料表——無需伺服器、無需憑證，直接可查。',
  'onboarding.s1.samplePreparing': '正在準備範例資料…',
  'onboarding.s1.sampleReady': '範例資料已就緒',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 個區域 × 2 個季度——已連線，在你的工作區中等候。',
  'onboarding.s1.sampleConnection': '連線：{name} · SQLite',
  'onboarding.s1.sampleFailed': '範例資料準備失敗',

  // S2 — AI
  'onboarding.s2.title': '設定 AI 輔助',
  'onboarding.s2.subtitle':
    'DataZen 使用你自己的 API 金鑰來生成 SQL、解釋錯誤和分析查詢計畫。Ollama 在本機執行，無需金鑰。你可以跳過此步驟，稍後再設定。',
  'onboarding.s2.stepLabel': '步驟 2 / 2',
  'onboarding.s2.optionalHint': '可選——可稍後在「設定 → AI」中設定。',
  'onboarding.s2.securityNote':
    '你的金鑰使用 AES-256-GCM 加密並儲存在本機。請求直接傳送到你的供應商——絕不經過 DataZen 伺服器。伺服器根本不存在。',

  // S3 — Done
  'onboarding.s3.title': '一切就緒',
  'onboarding.s3.subtitle':
    '你的工作區已準備就緒。DataZen 將開啟你的連線，你可以立即開始撰寫查詢。',
  'onboarding.s3.connLabel': '連線',
  'onboarding.s3.aiLabel': 'AI 供應商',
  'onboarding.s3.storageLabel': '儲存',
  'onboarding.s3.storageValue': '本機加密 · AES-256-GCM',
  'onboarding.s3.importedValue': '已匯入 {count} 個連線 · {source}',
  'onboarding.s3.updatedValue': '已更新 {count} 個連線 · {source}',
  'onboarding.s3.aiNotConfigured': '未設定（可選）',
  'onboarding.s3.notConfigured': '未設定（可稍後新增）',
  'onboarding.s3.openBtn': '開啟 DataZen',
  'onboarding.s3.nextHint': '下一步：查詢已就緒——點擊<b>執行</b>，然後<b>新增到儀表板</b>',

  // Common
  'onboarding.common.skip': '跳過設定',
  'onboarding.common.back': '返回',
  'onboarding.common.continue': '繼續',
  'onboarding.common.finish': '完成',
} as const;

export default onboarding;
