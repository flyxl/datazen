/** Auto-split domain: backup (ko) */
const pack = {
  'backup.startRestore': '복원 시작…',
  'backup.restoring': '복원 중…',
  'backup.restoreSuccess': '복원이 완료되었습니다',
  'backup.fileName': '파일 이름',
  'backup.fileNameHint': '파일 이름 패턴을 변경하려면 클릭하세요.',
  'backup.searchConnection': '연결 검색…',
  'backup.searchDatabase': '데이터베이스 검색…',
  'backup.selectConnectionFirst': '먼저 연결을 선택하세요',
  'backup.addOption': '옵션 추가…',
  'backup.compressGzip': 'Gzip을 사용하여 파일 압축',
  'backup.startBackup': '백업 시작…',
  'backup.inProgress': '백업 중…',
  'backup.success': '백업 완료',
  'backup.unsupportedType': '이 연결 유형은 데이터베이스 백업을 지원하지 않습니다',
  'backup.progressPreparing': '백업 준비 중…',
  'backup.progressObject': '{name} 덤프 중 ({current}/{total})',
  'backup.progressWriting': '백업 파일 쓰기 중…',
  'backup.restoreOverwriteConfirm':
    '대상 데이터베이스 "{database}"에 이미 {count}개의 객체가 있습니다. 계속하면 기존 테이블/뷰가 삭제된 후 백업이 적용됩니다. 덮어쓰시겠습니까?',
  'backup.restorePreparing': '백업 파일 읽기 중…',
  'backup.restoreProgress': '{name} 복원 중 ({current}/{total})',
  'backup.progressLog': '실행 로그',
  'backup.copyLog': '로그 복사',
  'backup.logCopied': 'Copied',
  'backup.logOmitted': '… {count}줄 생략됨(메모리 절약을 위해 로그 축소)',
} as const;
export default pack;
