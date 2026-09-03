/** Auto-split domain: mcp (ru) */
const pack = {
  'mcp.title': 'MCP-сервер',
  'mcp.description':
    'Запустите DataZen как сервер MCP, чтобы внешние инструменты искусственного интеллекта (Claude Desktop, Cursor и т. д.) могли получить доступ к вашим базам данных.',
  'mcp.enabled': 'Включить MCP-сервер',
  'mcp.enabledHint':
    'По умолчанию выключено. Предпочитаю запускать выделенный процесс с --mcp для Claude Desktop/Cursor',
  'mcp.status': 'Статус',
  'mcp.running': 'Бег',
  'mcp.stopped': 'Остановлено',
  'mcp.start': 'Запустить MCP-сервер',
  'mcp.stop': 'Останавливаться',
  'mcp.transport': 'Транспорт',
  'mcp.usage':
    'Чтобы использовать DataZen в качестве сервера MCP с Claude Desktop или Cursor, добавьте следующую конфигурацию:',
  'mcp.tools': 'Открытые инструменты',
  'mcp.tools.description':
    'Выберите, какие инструменты предоставлять через MCP Server внешним клиентам AI.',
  'mcp.tools.enableAll': 'Включить все',
  'mcp.tools.disableAll': 'Отключить все',
  'mcp.tools.restartHint':
    'Сервер MCP необходимо перезапустить, чтобы изменения инструмента вступили в силу.',
  'mcp.tools.applyHint':
    'Сохраните, чтобы применить изменения инструментов сразу, когда MCP Server запущен',
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
    'Restart MCP Server (or relaunch datazen --mcp) for permission mode changes to take effect',
  'mcp.permission.applyHint':
    'Применяется сразу, когда MCP Server запущен (встроенный режим перезагружается автоматически)',
  'mcp.allowlist.title': 'Белый список подключений',
  'mcp.allowlist.description':
    'Только выбранные подключения видны клиентам MCP. Снимите все флажки, чтобы показать все сохранённые подключения.',
  'mcp.allowlist.empty': 'Сохранённых подключений пока нет.',
  'mcp.allowlist.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for allowlist changes to take effect',
  'mcp.allowlist.applyHint':
    'Сохраните, чтобы применить изменения списка разрешений сразу, когда MCP Server запущен',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Copy',
  'mcp.config.copied': 'Copied',
  'mcp.config.pathHint': 'Типичное расположение: {path}',
  'mcp.config.commandHint':
    'Использует `datazen` из PATH. Для упакованных приложений замените команду на абсолютный путь к бинарному файлу.',
  'mcpClient.title': 'Внешние MCP-серверы',
  'mcpClient.description':
    'Подключитесь к внешним серверам MCP, чтобы расширить возможности AI-помощника.',
  'mcpClient.savedConfigs': 'Saved Servers',
  'mcpClient.runtimeStatus': 'Connected Servers',
  'mcpClient.addServer': 'Добавить MCP-сервер',
  'mcpClient.serverName': 'Имя',
  'mcpClient.command': 'Команда',
  'mcpClient.args': 'Аргументы',
  'mcpClient.save': 'Save',
  'mcpClient.saving': 'Saving…',
  'mcpClient.edit': 'Edit',
  'mcpClient.delete': 'Delete',
  'mcpClient.enabled': 'Enabled',
  'mcpClient.enabledForAi': 'Expose to AI Chat',
  'mcpClient.invalidId': 'ID may only contain letters, numbers, underscores, and hyphens.',
  'mcpClient.duplicateId': 'A server with this ID already exists.',
  'mcpClient.connect': 'Соединять',
  'mcpClient.connecting': 'Подключение…',
  'mcpClient.disconnect': 'Отключить',
  'mcpClient.tools': 'инструменты',
  'mcpClient.noSavedConfigs': 'No saved MCP server configs.',
  'mcpClient.noServers': 'Серверы MCP не подключены.',
  'mcpClient.noTools': 'No tools reported by this server.',
  'mcpClient.toolList': 'Tools',
  'mcpClient.reconnect': 'Retry',
  'mcpClient.connectFailed': 'Failed',
  'mcpClient.envVars': 'Environment Variables',
  'mcpClient.envKey': 'Variable name',
  'mcpClient.envValue': 'Value',
  'mcpClient.addEnv': 'Add variable',
  'mcpClient.removeEnv': 'Remove variable',
  'mcpClient.noEnvVars': 'No environment variables configured.',
} as const;
export default pack;
