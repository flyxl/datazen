const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'Bem-vindo ao DataZen',
  'onboarding.sidebar.desc':
    'Já tem um cliente de banco de dados? Importe suas conexões em segundos — ou comece do zero. Tudo fica nesta máquina.',
  'onboarding.sidebar.importTitle': 'Importação com um clique',
  'onboarding.sidebar.importDesc': 'DBeaver, DataGrip, Navicat, TablePlus.',
  'onboarding.sidebar.aiTitle': 'IA nativa',
  'onboarding.sidebar.aiDesc': 'Assistência SQL com conhecimento do schema.',
  'onboarding.sidebar.dashboardTitle': 'Painéis e Gráficos',
  'onboarding.sidebar.dashboardDesc':
    'Transforme resultados de consultas em gráficos na hora, relatórios sem complicação.',

  // S0 — Welcome
  'onboarding.s0.title': 'Como você gostaria de começar?',
  'onboarding.s0.subtitle':
    'A maioria importa do cliente que já usa. Os outros métodos levam apenas um minuto.',
  'onboarding.s0.importCard': 'Importar conexões',
  'onboarding.s0.importCardDesc': 'Do DBeaver, DataGrip, Navicat, TablePlus ou de um arquivo',
  'onboarding.s0.importFastest': 'MAIS RÁPIDO',
  'onboarding.s0.importDetectedApp': '✓ Configuração do {app} encontrada nesta máquina',
  'onboarding.s0.manualCard': 'Criar conexão manualmente',
  'onboarding.s0.manualCardDesc': 'PostgreSQL, MySQL, SQLite, Redis e mais',
  'onboarding.s0.sampleCard': 'Explorar com dados de exemplo',
  'onboarding.s0.sampleCardDesc': 'Um playground SQLite local — nada para configurar',

  // S1 — Connect
  'onboarding.s1.title': 'Crie sua primeira conexão',
  'onboarding.s1.subtitle':
    'O mesmo formulário da Nova conexão — a mesma validação, o mesmo teste. <b>Você pode continuar após salvar.</b>',
  'onboarding.s1.stepLabel': 'Passo 1 de 2',
  'onboarding.s1.driver': 'Driver',
  'onboarding.s1.testBtn': 'Testar conexão',
  'onboarding.s1.testSuccess': 'Conectado',
  'onboarding.s1.testFail': 'Falha na conexão',
  'onboarding.s1.importTitle': 'Importe suas conexões',
  'onboarding.s1.importSubtitle':
    'Selecione o cliente de origem — ou um arquivo de conexão. As conexões vão direto para seu workspace, nada sai desta máquina.',
  'onboarding.s1.importFileSource': 'Arquivo',
  'onboarding.s1.importing': 'Importando…',
  'onboarding.s1.importDetecting': 'Procurando configuração do cliente…',
  'onboarding.s1.importSuccess': '✓ {count} conexões importadas · {source}',
  'onboarding.s1.importUpdated': '✓ {count} conexões atualizadas · {source}',
  'onboarding.s1.sampleTitle': 'Seu playground de exemplo',
  'onboarding.s1.sampleSubtitle':
    'Um banco SQLite com uma tabela demo_sales em inglês — sem servidor, sem credenciais, pronto para consultar.',
  'onboarding.s1.samplePreparing': 'Preparando dados de exemplo…',
  'onboarding.s1.sampleReady': 'Dados de exemplo prontos',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 regiões × 2 trimestres — conectado e aguardando no seu workspace.',
  'onboarding.s1.sampleConnection': 'Conexão: {name} · SQLite',
  'onboarding.s1.sampleFailed': 'Não foi possível preparar os dados de exemplo',

  // S2 — AI
  'onboarding.s2.title': 'Configure a assistência de IA',
  'onboarding.s2.subtitle':
    'O DataZen usa sua própria chave de API para gerar SQL, explicar erros e analisar planos de consulta. O Ollama roda localmente e não precisa de chave. Você pode pular e configurar depois.',
  'onboarding.s2.stepLabel': 'Passo 2 de 2',
  'onboarding.s2.optionalHint':
    'Opcional — você pode configurar a IA depois em Configurações → IA.',
  'onboarding.s2.securityNote':
    'Sua chave é criptografada com AES-256-GCM e armazenada localmente. As requisições vão direto ao seu provedor — nunca passam pelos servidores DataZen. Não existem.',

  // S3 — Done
  'onboarding.s3.title': 'Tudo pronto',
  'onboarding.s3.subtitle':
    'Seu workspace está pronto. O DataZen abrirá sua conexão e você pode começar a escrever consultas imediatamente.',
  'onboarding.s3.connLabel': 'Conexão',
  'onboarding.s3.aiLabel': 'Provedor de IA',
  'onboarding.s3.storageLabel': 'Armazenamento',
  'onboarding.s3.storageValue': 'Criptografado localmente · AES-256-GCM',
  'onboarding.s3.importedValue': '{count} conexões importadas · {source}',
  'onboarding.s3.updatedValue': '{count} conexões atualizadas · {source}',
  'onboarding.s3.aiNotConfigured': 'Não configurado (opcional)',
  'onboarding.s3.notConfigured': 'Não configurado (pode ser adicionado depois)',
  'onboarding.s3.openBtn': 'Abrir DataZen',
  'onboarding.s3.nextHint':
    'Próximo passo: a consulta está pronta — pressione <b>Executar</b> e depois <b>Adicionar ao painel</b>',

  // Common
  'onboarding.common.skip': 'Pular configuração',
  'onboarding.common.back': 'Voltar',
  'onboarding.common.continue': 'Continuar',
  'onboarding.common.finish': 'Concluir',
} as const;

export default onboarding;
