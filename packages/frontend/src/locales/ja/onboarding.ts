const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'DataZen へようこそ',
  'onboarding.sidebar.desc':
    'データベースクライアントをお持ちですか？数秒で接続をインポート——またはゼロから始めましょう。すべてのデータはこのマシンに保存されます。',
  'onboarding.sidebar.importTitle': 'ワンクリックインポート',
  'onboarding.sidebar.importDesc': 'DBeaver、DataGrip、Navicat、TablePlus。',
  'onboarding.sidebar.aiTitle': 'AI ネイティブ',
  'onboarding.sidebar.aiDesc': 'スキーマ対応の SQL アシスタント。',
  'onboarding.sidebar.dashboardTitle': 'ダッシュボード＆チャート',
  'onboarding.sidebar.dashboardDesc': 'クエリ結果をすぐにチャートに変換、レポートも轻松に。',

  // S0 — Welcome
  'onboarding.s0.title': 'どのように始めますか？',
  'onboarding.s0.subtitle':
    'ほとんどの方は既存のクライアントからインポートします。それ以外も1分程度で完了します。',
  'onboarding.s0.importCard': '接続をインポート',
  'onboarding.s0.importCardDesc': 'DBeaver、DataGrip、Navicat、TablePlus またはファイルから',
  'onboarding.s0.importFastest': '最速',
  'onboarding.s0.importDetectedApp': '✓ このマシンに {app} の設定が見つかりました',
  'onboarding.s0.manualCard': '手動で接続を作成',
  'onboarding.s0.manualCardDesc': 'PostgreSQL、MySQL、SQLite、Redis など',
  'onboarding.s0.sampleCard': 'サンプルデータで体験',
  'onboarding.s0.sampleCardDesc': 'ローカル SQLite プレイグラウンド——設定不要',

  // S1 — Connect
  'onboarding.s1.title': '最初の接続を作成',
  'onboarding.s1.subtitle':
    '新規接続フォームと同じ——同じバリデーション、同じテスト。<b>保存後に続行できます。</b>',
  'onboarding.s1.stepLabel': 'ステップ 1 / 2',
  'onboarding.s1.driver': 'ドライバー',
  'onboarding.s1.testBtn': '接続テスト',
  'onboarding.s1.testSuccess': '接続済み',
  'onboarding.s1.testFail': '接続失敗',
  'onboarding.s1.importTitle': '接続をインポート',
  'onboarding.s1.importSubtitle':
    '移行元のクライアントを選択——または接続ファイルを選択。接続はワークスペースに直接导入され、データはこのマシンから外に出ません。',
  'onboarding.s1.importFileSource': 'ファイル',
  'onboarding.s1.importing': 'インポート中…',
  'onboarding.s1.importDetecting': 'クライアント設定を検索中…',
  'onboarding.s1.importSuccess': '✓ {count} 件の接続をインポートしました · {source}',
  'onboarding.s1.importUpdated': '✓ {count} 件の接続を更新しました · {source}',
  'onboarding.s1.sampleTitle': 'サンプルプレイグラウンド',
  'onboarding.s1.sampleSubtitle':
    'バンドルされた SQLite データベースに英語の demo_sales テーブル——サーバー不要、認証不要、すぐにクエリできます。',
  'onboarding.s1.samplePreparing': 'サンプルデータを準備中…',
  'onboarding.s1.sampleReady': 'サンプルデータの準備完了',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 地域 × 2 四半期——接続済み、ワークスペースで待機中。',
  'onboarding.s1.sampleConnection': '接続：{name} · SQLite',
  'onboarding.s1.sampleFailed': 'サンプルデータの準備に失敗しました',

  // S2 — AI
  'onboarding.s2.title': 'AI アシスタントを設定',
  'onboarding.s2.subtitle':
    'DataZen はご自身の API キーを使用して SQL を生成、エラーを説明、クエリプランを分析します。Ollama はローカルで動作し、キーは不要です。スキップして後から設定することもできます。',
  'onboarding.s2.stepLabel': 'ステップ 2 / 2',
  'onboarding.s2.optionalHint': 'オプション——後で設定 → AI から設定できます。',
  'onboarding.s2.securityNote':
    'キーは AES-256-GCM で暗号化され、ローカルに保存されます。リクエストはプロバイダに直接送信——DataZen サーバーを通ることはありません。サーバー自体が存在しないのです。',

  // S3 — Done
  'onboarding.s3.title': '準備完了',
  'onboarding.s3.subtitle':
    'ワークスペースの準備ができました。DataZen が接続を開き、すぐにクエリの作成を始められます。',
  'onboarding.s3.connLabel': '接続',
  'onboarding.s3.aiLabel': 'AI プロバイダ',
  'onboarding.s3.storageLabel': 'ストレージ',
  'onboarding.s3.storageValue': 'ローカル暗号化 · AES-256-GCM',
  'onboarding.s3.importedValue': '{count} 件の接続をインポート · {source}',
  'onboarding.s3.updatedValue': '{count} 件の接続を更新 · {source}',
  'onboarding.s3.aiNotConfigured': '未設定（オプション）',
  'onboarding.s3.notConfigured': '未設定（後から追加可能）',
  'onboarding.s3.openBtn': 'DataZen を開く',
  'onboarding.s3.nextHint':
    '下一步：クエリの準備ができました — <b>実行</b>を押してから<b>ダッシュボードに追加</b>',

  // Common
  'onboarding.common.skip': 'セットアップをスキップ',
  'onboarding.common.back': '戻る',
  'onboarding.common.continue': '続行',
  'onboarding.common.finish': '完了',
} as const;

export default onboarding;
