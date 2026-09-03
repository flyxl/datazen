/** Auto-split domain: backup (ja) */
const pack = {
  'backup.startRestore': '復元を開始…',
  'backup.restoring': '復元中…',
  'backup.restoreSuccess': '復元が完了しました',
  'backup.fileName': 'ファイル名',
  'backup.fileNameHint': 'クリックしてファイル名のパターンを変更します',
  'backup.searchConnection': '接続を検索…',
  'backup.searchDatabase': 'データベースを検索…',
  'backup.selectConnectionFirst': '最初に接続を選択してください',
  'backup.addOption': 'オプションを追加…',
  'backup.compressGzip': 'Gzipを使用してファイルを圧縮する',
  'backup.startBackup': 'バックアップを開始します…',
  'backup.inProgress': 'バックアップ中…',
  'backup.success': 'バックアップが完了しました',
  'backup.unsupportedType': 'この接続タイプはデータベースバックアップをサポートしていません',
  'backup.progressPreparing': 'バックアップを準備中…',
  'backup.progressObject': '{name} をダンプ中 ({current}/{total})',
  'backup.progressWriting': 'バックアップファイルを書き込み中…',
  'backup.restoreOverwriteConfirm':
    'ターゲットデータベース "{database}" にはすでに {count} オブジェクトがあります。続行すると既存のテーブル/ビューが削除され、バックアップが適用されます。上書きしますか？',
  'backup.restorePreparing': 'バックアップファイルを読み込み中…',
  'backup.restoreProgress': '{name} を復元中 ({current}/{total})',
  'backup.progressLog': '実行ログ',
  'backup.copyLog': 'ログをコピー',
  'backup.logCopied': 'Copied',
  'backup.logOmitted': '… {count} 行を省略しました（メモリ節約のためログを削減）',
} as const;
export default pack;
