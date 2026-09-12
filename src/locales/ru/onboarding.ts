const onboarding = {
  // Brand sidebar
  'onboarding.sidebar.heading': 'Добро пожаловать в DataZen',
  'onboarding.sidebar.desc':
    'Уже есть клиент для базы данных? Импортируйте подключения за секунды — или начните с нуля. Все данные остаются на этом устройстве.',
  'onboarding.sidebar.importTitle': 'Импорт в один клик',
  'onboarding.sidebar.importDesc': 'DBeaver, DataGrip, Navicat, TablePlus.',
  'onboarding.sidebar.aiTitle': 'ИИ-нативный',
  'onboarding.sidebar.aiDesc': 'Помощь SQL с учётом схемы.',
  'onboarding.sidebar.dashboardTitle': 'Панели и диаграммы',
  'onboarding.sidebar.dashboardDesc':
    'Результаты запросов мгновенно превращаются в графики, отчёты без забот.',

  // S0 — Welcome
  'onboarding.s0.title': 'Как вы хотите начать?',
  'onboarding.s0.subtitle':
    'Большинство импортируют из уже используемого клиента. Остальные способы занимают минуту.',
  'onboarding.s0.importCard': 'Импортировать подключения',
  'onboarding.s0.importCardDesc': 'Из DBeaver, DataGrip, Navicat, TablePlus или файла',
  'onboarding.s0.importFastest': 'БЫСТРЕЕЕ',
  'onboarding.s0.importDetectedApp': '✓ Найдена конфигурация {app} на этом устройстве',
  'onboarding.s0.manualCard': 'Создать подключение вручную',
  'onboarding.s0.manualCardDesc': 'PostgreSQL, MySQL, SQLite, Redis и другие',
  'onboarding.s0.sampleCard': 'Попробовать на примерах',
  'onboarding.s0.sampleCardDesc': 'Локальная песочница SQLite — настройка не требуется',

  // S1 — Connect
  'onboarding.s1.title': 'Создайте первое подключение',
  'onboarding.s1.subtitle':
    'Та же форма, что и в «Новое подключение» — та же валидация, тот же тест. <b>Можно продолжить после сохранения.</b>',
  'onboarding.s1.stepLabel': 'Шаг 1 из 2',
  'onboarding.s1.driver': 'Драйвер',
  'onboarding.s1.testBtn': 'Проверить подключение',
  'onboarding.s1.testSuccess': 'Подключено',
  'onboarding.s1.testFail': 'Ошибка подключения',
  'onboarding.s1.importTitle': 'Импортируйте подключения',
  'onboarding.s1.importSubtitle':
    'Выберите клиент-источник — или файл подключений. Подключения попадают в ваше рабочее пространство, ничего не покидает это устройство.',
  'onboarding.s1.importFileSource': 'Файл',
  'onboarding.s1.importing': 'Импорт…',
  'onboarding.s1.importDetecting': 'Поиск конфигурации клиента…',
  'onboarding.s1.importSuccess': '✓ Импортировано подключений: {count} · {source}',
  'onboarding.s1.importUpdated': '✓ Обновлено подключений: {count} · {source}',
  'onboarding.s1.sampleTitle': 'Ваша песочница с примерами',
  'onboarding.s1.sampleSubtitle':
    'Встроенная база SQLite с таблицей demo_sales на английском — без сервера, без учётных данных, готова к запросам.',
  'onboarding.s1.samplePreparing': 'Подготовка демо-данных…',
  'onboarding.s1.sampleReady': 'Демо-данные готовы',
  'onboarding.s1.sampleReadyDesc':
    'demo_sales(region, amount, quarter) · 4 региона × 2 квартала — подключено и ожидает в вашем рабочем пространстве.',
  'onboarding.s1.sampleConnection': 'Подключение: {name} · SQLite',
  'onboarding.s1.sampleFailed': 'Не удалось подготовить демо-данные',

  // S2 — AI
  'onboarding.s2.title': 'Настройте ИИ-ассистента',
  'onboarding.s2.subtitle':
    'DataZen использует ваш собственный API-ключ для генерации SQL, объяснения ошибок и анализа планов запросов. Ollama работает локально и не требует ключа. Вы можете пропустить это и настроить позже.',
  'onboarding.s2.stepLabel': 'Шаг 2 из 2',
  'onboarding.s2.optionalHint': 'Необязательно — можно настроить позже в Параметры → ИИ.',
  'onboarding.s2.securityNote':
    'Ваш ключ шифруется с помощью AES-256-GCM и хранится локально. Запросы идут напрямую к вашему провайдеру — никогда через серверы DataZen. Их просто не существует.',

  // S3 — Done
  'onboarding.s3.title': 'Все готово',
  'onboarding.s3.subtitle':
    'Ваше рабочее пространство готово. DataZen откроет подключение, и вы сможете сразу начать писать запросы.',
  'onboarding.s3.connLabel': 'Подключение',
  'onboarding.s3.aiLabel': 'Провайдер ИИ',
  'onboarding.s3.storageLabel': 'Хранилище',
  'onboarding.s3.storageValue': 'Локальное шифрование · AES-256-GCM',
  'onboarding.s3.importedValue': '{count} подключений импортировано · {source}',
  'onboarding.s3.updatedValue': '{count} подключений обновлено · {source}',
  'onboarding.s3.aiNotConfigured': 'Не настроено (необязательно)',
  'onboarding.s3.notConfigured': 'Не настроено (можно добавить позже)',
  'onboarding.s3.openBtn': 'Открыть DataZen',
  'onboarding.s3.nextHint':
    'Следующий шаг: запрос готов — нажмите <b>Выполнить</b>, затем <b>Добавить на панель</b>',

  // Common
  'onboarding.common.skip': 'Пропустить настройку',
  'onboarding.common.back': 'Назад',
  'onboarding.common.continue': 'Далее',
  'onboarding.common.finish': 'Готово',
} as const;

export default onboarding;
