/** Auto-split domain: mcp (ru) */
const pack = {
  'mcp.title': 'MCP-сервер',
  'mcp.description':
    'Запустите DataZen как сервер MCP, чтобы внешние инструменты искусственного интеллекта (Claude Desktop, Cursor и т. д.) могли получить доступ к вашим базам данных.',
  'mcp.enabled': 'Включить MCP-сервер',
  'mcp.enabledHint':
    'По умолчанию выключено. Предпочитаю запускать выделенный процесс с --mcp для Claude Desktop/Cursor',
  'mcp.status': 'Статус',
  'mcp.running': 'Работает',
  'mcp.stopped': 'Остановлен',
  'mcp.start': 'Запустить MCP-сервер',
  'mcp.stop': 'Остановить',
  'mcp.transport': 'Транспорт',
  'mcp.usage':
    'Чтобы использовать DataZen в качестве сервера MCP с Claude Desktop или Cursor, добавьте следующую конфигурацию:',
  'mcp.tools': 'Открытые инструменты',
  'mcp.tools.description':
    'Выберите, какие инструменты предоставлять через MCP-сервер внешним клиентам ИИ.',
  'mcp.tools.enableAll': 'Включить все',
  'mcp.tools.disableAll': 'Отключить все',
  'mcp.tools.restartHint':
    'Сохраните, чтобы изменения инструментов вступили в силу сразу, когда MCP-сервер запущен',
  'mcp.tools.applyHint':
    'Сохраните, чтобы изменения инструментов вступили в силу сразу, когда MCP-сервер запущен',
  'mcp.toggleError': 'Не удалось запустить/остановить сервер MCP.',
  'mcp.permission.title': 'Режим доступа',
  'mcp.permission.readOnly': 'Только чтение',
  'mcp.permission.readOnlyHint': 'Только просмотр схемы; блокирует запросы и run_workflow',
  'mcp.permission.safeWrite': 'Безопасная запись (по умолчанию)',
  'mcp.permission.safeWriteHint':
    'Разрешает DML; блокирует DROP, TRUNCATE, ALTER, CREATE USER и подобные',
  'mcp.permission.highRiskWrite': 'Высокорисковая запись',
  'mcp.permission.highRiskWriteHint':
    'Без ограничений SQL; применяется только список запрещённых инструментов',
  'mcp.permission.restartHint':
    'Применяется сразу, когда MCP-сервер запущен (встроенный режим перезагружается автоматически)',
  'mcp.permission.applyHint':
    'Применяется сразу, когда MCP-сервер запущен (встроенный режим перезагружается автоматически)',
  'mcp.allowlist.title': 'Список разрешённых подключений',
  'mcp.allowlist.description':
    'Клиентам MCP видны только выбранные подключения. Если не отмечено ни одно, доступ запрещён для всех — явно разрешите подключения, прежде чем инструменты MCP смогут обращаться к ним.',
  'mcp.allowlist.empty': 'Сохранённых подключений пока нет.',
  'mcp.allowlist.restartHint':
    'Сохраните, чтобы изменения списка разрешений вступили в силу сразу, когда MCP-сервер запущен',
  'mcp.allowlist.applyHint':
    'Сохраните, чтобы изменения списка разрешений вступили в силу сразу, когда MCP-сервер запущен',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Копировать',
  'mcp.config.copied': 'Скопировано',
  'mcp.config.pathHint': 'Типичное расположение: {path}',
  'mcp.config.commandHint':
    'Указывается полный путь к исполняемому файлу DataZen. Проверьте путь к команде, если приложение было перемещено.',
  'mcpClient.title': 'Внешние MCP-серверы',
  'mcpClient.description':
    'Подключитесь к внешним серверам MCP, чтобы расширить возможности ИИ-помощника.',
  'mcpClient.savedConfigs': 'Сохранённые серверы',
  'mcpClient.runtimeStatus': 'Подключённые серверы',
  'mcpClient.addServer': 'Добавить MCP-сервер',
  'mcpClient.serverName': 'Имя',
  'mcpClient.command': 'Команда',
  'mcpClient.args': 'Аргументы',
  'mcpClient.save': 'Сохранить',
  'mcpClient.saving': 'Сохранение…',
  'mcpClient.edit': 'Изменить',
  'mcpClient.delete': 'Удалить',
  'mcpClient.enabled': 'Включено',
  'mcpClient.enabledForAi': 'Передать в ИИ-чат',
  'mcpClient.invalidId': 'ID может содержать только буквы, цифры, подчёркивания и дефисы.',
  'mcpClient.duplicateId': 'Сервер с таким ID уже существует.',
  'mcpClient.connect': 'Подключить',
  'mcpClient.connecting': 'Подключение…',
  'mcpClient.disconnect': 'Отключить',
  'mcpClient.tools': 'инструменты',
  'mcpClient.noSavedConfigs': 'Нет сохранённых конфигураций серверов MCP.',
  'mcpClient.noServers': 'Серверы MCP не подключены.',
  'mcpClient.noTools': 'Этот сервер не сообщил ни об одном инструменте.',
  'mcpClient.toolList': 'Инструменты',
  'mcpClient.reconnect': 'Повторить',
  'mcpClient.connectFailed': 'Не удалось',
  'mcpClient.envVars': 'Переменные окружения',
  'mcpClient.envKey': 'Имя переменной',
  'mcpClient.envValue': 'Значение',
  'mcpClient.addEnv': 'Добавить переменную',
  'mcpClient.removeEnv': 'Удалить переменную',
  'mcpClient.noEnvVars': 'Переменные окружения не настроены.',
  'mcp.saved': 'Сохранено',
} as const;
export default pack;
