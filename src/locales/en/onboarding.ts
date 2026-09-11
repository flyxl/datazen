const onboarding = {
  // S0 — Welcome
  'onboarding.s0.title': 'Get started with DataZen',
  'onboarding.s0.subtitle':
    'Connect to a database or explore a sample to get started.',
  'onboarding.s0.importCard': 'Import connections',
  'onboarding.s0.importCardDesc': 'Bring in configs from DBeaver, DataGrip, Navicat or TablePlus.',
  'onboarding.s0.importFastest': 'FASTEST',
  'onboarding.s0.manualCard': 'Create connection',
  'onboarding.s0.manualCardDesc': 'Set up a database connection from scratch.',
  'onboarding.s0.sampleCard': 'Explore sample data',
  'onboarding.s0.sampleCardDesc': 'Dive into a bundled demo dataset — no setup required.',

  // S1 — Connect
  'onboarding.s1.title': 'Connect to a database',
  'onboarding.s1.subtitle':
    'Reuses the same connection form you\'ll see later — nothing is locked in.',
  'onboarding.s1.driver': 'Driver',
  'onboarding.s1.host': 'Host',
  'onboarding.s1.port': 'Port',
  'onboarding.s1.user': 'User',
  'onboarding.s1.password': 'Password',
  'onboarding.s1.database': 'Database',
  'onboarding.s1.testBtn': 'Test connection',
  'onboarding.s1.testSuccess': 'Connected',
  'onboarding.s1.testFail': 'Connection failed',

  // S2 — AI
  'onboarding.s2.title': 'Set up AI assistance',
  'onboarding.s2.subtitle':
    'Optional — you can add or change this later in Settings.',
  'onboarding.s2.apiKey': 'API key',
  'onboarding.s2.skipHint': 'You can skip this and configure AI later.',

  // S3 — Done
  'onboarding.s3.title': 'You\'re all set',
  'onboarding.s3.subtitle':
    'DataZen is ready. You can change any of these in Settings.',
  'onboarding.s3.connLabel': 'Connection',
  'onboarding.s3.aiLabel': 'AI provider',
  'onboarding.s3.storageLabel': 'Storage',
  'onboarding.s3.openBtn': 'Open DataZen',
  'onboarding.s3.nextHint':
    'Next: run the prebuilt query and press Add to Dashboard',

  // Common
  'onboarding.common.skip': 'Skip setup',
  'onboarding.common.back': 'Back',
  'onboarding.common.continue': 'Continue',
  'onboarding.common.finish': 'Finish',
} as const;

export default onboarding;
