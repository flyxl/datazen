/** Auto-split domain: backup (zh-TW) */
const pack = {
  'backup.startRestore': '開始還原…',
  'backup.restoring': '還原中…',
  'backup.restoreSuccess': '還原完成',
  'backup.fileName': '檔案名稱',
  'backup.fileNameHint': '點擊可修改檔案名稱模式',
  'backup.searchConnection': '搜尋連線…',
  'backup.searchDatabase': '搜尋資料庫…',
  'backup.selectConnectionFirst': '請先選擇連線',
  'backup.addOption': '新增選項…',
  'backup.compressGzip': '使用 Gzip 壓縮',
  'backup.startBackup': '開始備份',
  'backup.inProgress': '備份中…',
  'backup.success': '備份完成',
  'backup.unsupportedType': '此連線類型不支援資料庫備份',
  'backup.progressPreparing': '正在準備備份…',
  'backup.progressObject': '正在傾印 {name}（{current}/{total}）',
  'backup.progressWriting': '正在寫入備份檔案…',
  'backup.restoreOverwriteConfirm':
    '目標資料庫「{database}」已有 {count} 個物件。繼續將會刪除現有的資料表／檢視，然後套用備份。要覆寫嗎？',
  'backup.restorePreparing': '正在讀取備份檔案…',
  'backup.restoreProgress': '正在還原 {name}（{current}/{total}）',
  'backup.progressLog': '執行日誌',
  'backup.copyLog': '複製日誌',
  'backup.logCopied': '已複製',
  'backup.logOmitted': '… {count} 行日誌已省略（為節省記憶體已截斷）',
} as const;
export default pack;
