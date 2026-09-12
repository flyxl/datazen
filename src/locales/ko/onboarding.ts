const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'DataZen에 오신 것을 환영합니다',
  'onboarding.sidebar.desc':
    '데이터베이스 클라이언트가 있으신가요? 몇 초 만에 연결을 가져오세요 — 아니면 새로 시작하세요. 모든 데이터는 이 컴퓨터에 저장됩니다.',
  'onboarding.sidebar.importTitle': '원클릭 가져오기',
  'onboarding.sidebar.importDesc': 'DBeaver, DataGrip, Navicat, TablePlus.',
  'onboarding.sidebar.aiTitle': 'AI 네이티브',
  'onboarding.sidebar.aiDesc': '스키마 인식 SQL 어시스턴트.',
  'onboarding.sidebar.dashboardTitle': '대시보드 & 차트',
  'onboarding.sidebar.dashboardDesc': '쿼리 결과를 바로 차트로, 리포트도 간편하게.',

  // S0 — Welcome
  'onboarding.s0.title': '어떻게 시작하시겠어요?',
  'onboarding.s0.subtitle':
    '대부분의 사용자는 기존 클라이언트에서 가져옵니다. 나머지 방법도 1분이면 충분합니다.',
  'onboarding.s0.importCard': '가져오기',
  'onboarding.s0.importCardDesc': 'DBeaver, DataGrip, Navicat, TablePlus 또는 파일에서',
  'onboarding.s0.importFastest': '최고',
  'onboarding.s0.importDetectedApp': '✓ 이 컴퓨터에서 {app} 설정을 찾았습니다',
  'onboarding.s0.manualCard': '수동으로 연결 만들기',
  'onboarding.s0.manualCardDesc': 'PostgreSQL, MySQL, SQLite, Redis 등',
  'onboarding.s0.sampleCard': '샘플 데이터로 체험',
  'onboarding.s0.sampleCardDesc': '로컬 SQLite 놀이터 — 설정 불필요',

  // S1 — Connect
  'onboarding.s1.title': '첫 번째 연결 만들기',
  'onboarding.s1.subtitle':
    '새 연결과 동일한 양식 — 동일한 유효성 검사, 동일한 테스트. <b>저장 후 계속할 수 있습니다.</b>',
  'onboarding.s1.stepLabel': '1단계 / 2',
  'onboarding.s1.driver': '드라이버',
  'onboarding.s1.testBtn': '연결 테스트',
  'onboarding.s1.testSuccess': '연결됨',
  'onboarding.s1.testFail': '연결 실패',
  'onboarding.s1.importTitle': '연결 가져오기',
  'onboarding.s1.importSubtitle':
    '마이그레이션할 클라이언트 선택 — 또는 연결 파일을 선택하세요. 연결은 워크스페이스에 직접 저장되며, 데이터는 이 컴퓨터를 벗어나지 않습니다.',
  'onboarding.s1.importFileSource': '파일',
  'onboarding.s1.importing': '가져오는 중…',
  'onboarding.s1.importDetecting': '클라이언트 설정을 검색 중…',
  'onboarding.s1.importSuccess': '✓ {count}개 연결 가져옴 · {source}',
  'onboarding.s1.importUpdated': '✓ {count}개 연결 업데이트됨 · {source}',
  'onboarding.s1.sampleTitle': '샘플 놀이터',
  'onboarding.s1.sampleSubtitle':
    '번들 SQLite 데이터베이스에 영어 demo_sales 테이블 — 서버 불필요, 인증 불필요, 바로 쿼리 가능.',
  'onboarding.s1.samplePreparing': '샘플 데이터 준비 중…',
  'onboarding.s1.sampleReady': '샘플 데이터 준비 완료',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4개 지역 × 2개 분기 — 연결 완료, 워크스페이스에서 대기 중.',
  'onboarding.s1.sampleConnection': '연결: {name} · SQLite',
  'onboarding.s1.sampleFailed': '샘플 데이터 준비 실패',

  // S2 — AI
  'onboarding.s2.title': 'AI 어시스턴스 설정',
  'onboarding.s2.subtitle':
    'DataZen은 자체 API 키를 사용하여 SQL을 생성하고, 오류를 설명하고, 쿼리 플랜을 분석합니다. Ollama는 로컬에서 실행되며 키가 필요 없습니다. 건너뛰고 나중에 설정할 수도 있습니다.',
  'onboarding.s2.stepLabel': '2단계 / 2',
  'onboarding.s2.optionalHint': '선택사항 — 나중에 설정 → AI에서 구성할 수 있습니다.',
  'onboarding.s2.securityNote':
    '키는 AES-256-GCM으로 암호화되어 로컬에 저장됩니다. 요청은DataProvider에 직접 전송 — DataZen 서버를 거치지 않습니다. 서버 자체가 없으니까요.',

  // S3 — Done
  'onboarding.s3.title': '모두 준비 완료',
  'onboarding.s3.subtitle':
    '워크스페이스가 준비되었습니다. DataZen이 연결을 열고 바로 쿼리를 작성할 수 있습니다.',
  'onboarding.s3.connLabel': '연결',
  'onboarding.s3.aiLabel': 'AI 공급자',
  'onboarding.s3.storageLabel': '저장소',
  'onboarding.s3.storageValue': '로컬 암호화 · AES-256-GCM',
  'onboarding.s3.importedValue': '{count}개 연결 가져옴 · {source}',
  'onboarding.s3.updatedValue': '{count}개 연결 업데이트됨 · {source}',
  'onboarding.s3.aiNotConfigured': '미설정 (선택사항)',
  'onboarding.s3.notConfigured': '미설정 (나중에 추가 가능)',
  'onboarding.s3.openBtn': 'DataZen 열기',
  'onboarding.s3.nextHint':
    '다음 단계: 쿼리가 준비되었습니다 — <b>실행</b>을 누른 후 <b>대시보드에 추가</b>',

  // Common
  'onboarding.common.skip': '설정 건너뛰기',
  'onboarding.common.back': '뒤로',
  'onboarding.common.continue': '계속',
  'onboarding.common.finish': '완료',
} as const;

export default onboarding;
