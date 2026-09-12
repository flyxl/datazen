const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'Bienvenido a DataZen',
  'onboarding.sidebar.desc':
    '¿Ya tienes un cliente de base de datos? Importa tus conexiones en segundos — o empieza desde cero. Todo se queda en esta máquina.',
  'onboarding.sidebar.importTitle': 'Importación con un clic',
  'onboarding.sidebar.importDesc': 'DBeaver, DataGrip, Navicat, TablePlus.',
  'onboarding.sidebar.aiTitle': 'IA nativa',
  'onboarding.sidebar.aiDesc': 'Asistencia SQL con conocimiento del esquema.',
  'onboarding.sidebar.dashboardTitle': 'Paneles y Gráficos',
  'onboarding.sidebar.dashboardDesc':
    'Transforma los resultados de consultas en gráficos al instante, informes sin preocupaciones.',

  // S0 — Welcome
  'onboarding.s0.title': '¿Cómo te gustaría empezar?',
  'onboarding.s0.subtitle':
    'La mayoría importa desde el cliente que ya usan. Los demás métodos solo toman un minuto.',
  'onboarding.s0.importCard': 'Importar conexiones',
  'onboarding.s0.importCardDesc': 'Desde DBeaver, DataGrip, Navicat, TablePlus o un archivo',
  'onboarding.s0.importFastest': 'MÁS RÁPIDO',
  'onboarding.s0.importDetectedApp': '✓ Se encontró configuración de {app} en esta máquina',
  'onboarding.s0.manualCard': 'Crear conexión manualmente',
  'onboarding.s0.manualCardDesc': 'PostgreSQL, MySQL, SQLite, Redis y más',
  'onboarding.s0.sampleCard': 'Explorar con datos de ejemplo',
  'onboarding.s0.sampleCardDesc': 'Un playground SQLite local — sin configuración',

  // S1 — Connect
  'onboarding.s1.title': 'Crea tu primera conexión',
  'onboarding.s1.subtitle':
    'El mismo formulario que Nueva conexión — la misma validación, la misma prueba. <b>Puedes continuar después de guardar.</b>',
  'onboarding.s1.stepLabel': 'Paso 1 de 2',
  'onboarding.s1.driver': 'Controlador',
  'onboarding.s1.testBtn': 'Probar conexión',
  'onboarding.s1.testSuccess': 'Conectado',
  'onboarding.s1.testFail': 'Error de conexión',
  'onboarding.s1.importTitle': 'Importa tus conexiones',
  'onboarding.s1.importSubtitle':
    'Selecciona el cliente del que migras — o un archivo de conexión. Las conexiones llegan a tu espacio de trabajo, nada sale de esta máquina.',
  'onboarding.s1.importFileSource': 'Archivo',
  'onboarding.s1.importing': 'Importando…',
  'onboarding.s1.importDetecting': 'Buscando configuración del cliente…',
  'onboarding.s1.importSuccess': '✓ {count} conexiones importadas · {source}',
  'onboarding.s1.importUpdated': '✓ {count} conexiones actualizadas · {source}',
  'onboarding.s1.sampleTitle': 'Tu playground de ejemplo',
  'onboarding.s1.sampleSubtitle':
    'Una base de datos SQLite con una tabla demo_sales en inglés — sin servidor, sin credenciales, lista para consultar.',
  'onboarding.s1.samplePreparing': 'Preparando datos de ejemplo…',
  'onboarding.s1.sampleReady': 'Datos de ejemplo listos',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 regiones × 2 trimestres — conectado y esperando en tu espacio de trabajo.',
  'onboarding.s1.sampleConnection': 'Conexión: {name} · SQLite',
  'onboarding.s1.sampleFailed': 'No se pudieron preparar los datos de ejemplo',

  // S2 — AI
  'onboarding.s2.title': 'Configura la asistencia de IA',
  'onboarding.s2.subtitle':
    'DataZen usa tu propia clave API para generar SQL, explicar errores y analizar planes de consulta. Ollama funciona localmente y no necesita clave. Puedes omitir esto y configurarlo después.',
  'onboarding.s2.stepLabel': 'Paso 2 de 2',
  'onboarding.s2.optionalHint':
    'Opcional — puedes configurar la IA más tarde en Configuración → IA.',
  'onboarding.s2.securityNote':
    'Tu clave se cifra con AES-256-GCM y se almacena localmente. Las solicitudes van directamente a tu proveedor — nunca pasan por servidores DataZen. No existen.',

  // S3 — Done
  'onboarding.s3.title': 'Todo listo',
  'onboarding.s3.subtitle':
    'Tu espacio de trabajo está listo. DataZen abrirá tu conexión y podrás empezar a escribir consultas de inmediato.',
  'onboarding.s3.connLabel': 'Conexión',
  'onboarding.s3.aiLabel': 'Proveedor de IA',
  'onboarding.s3.storageLabel': 'Almacenamiento',
  'onboarding.s3.storageValue': 'Cifrado local · AES-256-GCM',
  'onboarding.s3.importedValue': '{count} conexiones importadas · {source}',
  'onboarding.s3.updatedValue': '{count} conexiones actualizadas · {source}',
  'onboarding.s3.aiNotConfigured': 'No configurado (opcional)',
  'onboarding.s3.notConfigured': 'No configurado (se puede agregar después)',
  'onboarding.s3.openBtn': 'Abrir DataZen',
  'onboarding.s3.nextHint':
    'Siguiente paso: la consulta está lista — presiona <b>Ejecutar</b> y luego <b>Agregar al panel</b>',

  // Common
  'onboarding.common.skip': 'Omitir configuración',
  'onboarding.common.back': 'Atrás',
  'onboarding.common.continue': 'Continuar',
  'onboarding.common.finish': 'Finalizar',
} as const;

export default onboarding;
