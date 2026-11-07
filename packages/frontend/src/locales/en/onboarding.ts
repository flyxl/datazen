const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'Welcome to DataZen',
  'onboarding.sidebar.desc':
    'Already have a database client? Bring your connections in seconds — or start fresh. Everything stays on this machine.',
  'onboarding.sidebar.importTitle': 'One-click import',
  'onboarding.sidebar.importDesc': 'DBeaver, DataGrip, Navicat, TablePlus.',
  'onboarding.sidebar.aiTitle': 'AI-native',
  'onboarding.sidebar.aiDesc': 'Schema-aware SQL assistance.',
  'onboarding.sidebar.dashboardTitle': 'Dashboards & Charts',
  'onboarding.sidebar.dashboardDesc': 'Visualise query results and share reports.',

  // S0 — Welcome
  'onboarding.s0.title': 'How would you like to start?',
  'onboarding.s0.subtitle':
    'Most people import from the client they already use. Everything else takes a minute.',
  'onboarding.s0.importCard': 'Import connections',
  'onboarding.s0.importCardDesc': 'From DBeaver, DataGrip, Navicat, TablePlus or a file',
  'onboarding.s0.importFastest': 'FASTEST',
  'onboarding.s0.importDetectedApp': '✓ Found {app} config on this machine',
  'onboarding.s0.manualCard': 'Create a connection manually',
  'onboarding.s0.manualCardDesc': 'PostgreSQL, MySQL, SQLite, Redis and more',
  'onboarding.s0.sampleCard': 'Explore with sample data',
  'onboarding.s0.sampleCardDesc': 'A local SQLite playground — nothing to configure',

  // S1 — Connect (entry specific: connection form / inline import / sample)
  'onboarding.s1.title': 'Create your first connection',
  'onboarding.s1.subtitle':
    'Same form as New Connection — same validation, same test. <b>Continue unlocks after saving.</b>',
  'onboarding.s1.stepLabel': 'Step 1 of 2',
  'onboarding.s1.driver': 'Driver',
  'onboarding.s1.testBtn': 'Test connection',
  'onboarding.s1.testSuccess': 'Connected',
  'onboarding.s1.testFail': 'Connection failed',
  'onboarding.s1.importTitle': 'Import your connections',
  'onboarding.s1.importSubtitle':
    'Pick the client you are migrating from — or a connection file. The connections land in your workspace, nothing leaves this machine.',
  'onboarding.s1.importFileSource': 'File',
  'onboarding.s1.importing': 'Importing…',
  'onboarding.s1.importDetecting': 'Looking for the client config…',
  'onboarding.s1.importSuccess': '✓ {count} connections imported · {source}',
  'onboarding.s1.importUpdated': '✓ {count} connections updated · {source}',
  'onboarding.s1.sampleTitle': 'Your sample playground',
  'onboarding.s1.sampleSubtitle':
    'A bundled SQLite database with an English demo_sales table — no server, no credentials, ready to query.',
  'onboarding.s1.samplePreparing': 'Preparing the sample dataset…',
  'onboarding.s1.sampleReady': 'Sample dataset ready',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 regions × 2 quarters — connected and waiting in your workspace.',
  'onboarding.s1.sampleConnection': 'Connection: {name} · SQLite',
  'onboarding.s1.sampleFailed': 'Could not prepare the sample dataset',

  // S2 — AI (always the second step)
  'onboarding.s2.title': 'Set up AI assistance',
  'onboarding.s2.subtitle':
    'DataZen uses your own API key to generate SQL, explain errors and analyse query plans. Ollama runs locally and needs no key. You can skip this and configure it later.',
  'onboarding.s2.stepLabel': 'Step 2 of 2',
  'onboarding.s2.optionalHint': 'Optional — you can configure AI later in Settings → AI.',
  'onboarding.s2.securityNote':
    "Your key is encrypted with AES-256-GCM and stored locally. Requests go directly to your provider — never through DataZen servers. There aren't any.",

  // S3 — Done
  'onboarding.s3.title': "You're all set",
  'onboarding.s3.subtitle':
    'Your workspace is ready. DataZen will open your connection and you can start writing queries right away.',
  'onboarding.s3.connLabel': 'Connection',
  'onboarding.s3.aiLabel': 'AI provider',
  'onboarding.s3.storageLabel': 'Storage',
  'onboarding.s3.storageValue': 'Encrypted locally · AES-256-GCM',
  'onboarding.s3.importedValue': '{count} connections imported · {source}',
  'onboarding.s3.updatedValue': '{count} connections updated · {source}',
  'onboarding.s3.aiNotConfigured': 'Not configured (optional)',
  'onboarding.s3.notConfigured': 'Not configured (can be added later)',
  'onboarding.s3.openBtn': 'Open DataZen',
  'onboarding.s3.nextHint':
    'Next: the query is ready — press <b>Execute</b> then <b>Add to Dashboard</b>',

  // Common
  'onboarding.common.skip': 'Skip setup',
  'onboarding.common.back': 'Back',
  'onboarding.common.continue': 'Continue',
  'onboarding.common.finish': 'Finish',
} as const;

export default onboarding;
