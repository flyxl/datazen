/** Auto-split domain: mcp (de) */
const pack = {
  'mcp.title': 'MCP-Server',
  'mcp.description':
    'Führen Sie DataZen als MCP-Server aus, damit externe KI-Tools (Claude Desktop, Cursor usw.) auf Ihre Datenbanken zugreifen können',
  'mcp.enabled': 'Aktivieren Sie den MCP-Server',
  'mcp.enabledHint':
    'Standardmäßig deaktiviert. Starten Sie lieber einen dedizierten Prozess mit --mcp für Claude Desktop/Cursor',
  'mcp.status': 'Status',
  'mcp.running': 'Läuft',
  'mcp.stopped': 'Angehalten',
  'mcp.start': 'Starten Sie den MCP-Server',
  'mcp.stop': 'Stoppen',
  'mcp.transport': 'Transport',
  'mcp.usage':
    'Um DataZen als MCP-Server mit Claude Desktop oder Cursor zu verwenden, fügen Sie die folgende Konfiguration hinzu:',
  'mcp.tools': 'Freigelegte Werkzeuge',
  'mcp.tools.description':
    'Wählen Sie aus, welche Tools über MCP Server externen KI-Clients zugänglich gemacht werden sollen',
  'mcp.tools.enableAll': 'Alle aktivieren',
  'mcp.tools.disableAll': 'Alle deaktivieren',
  'mcp.tools.restartHint':
    'Damit die Tool-Änderungen wirksam werden, muss der MCP-Server neu gestartet werden',
  'mcp.tools.applyHint':
    'Speichern, um Tool-Änderungen sofort anzuwenden, wenn der MCP Server läuft',
  'mcp.toggleError': 'Der MCP-Server konnte nicht gestartet/gestoppt werden',
  'mcp.permission.title': 'Berechtigungsmodus',
  'mcp.permission.readOnly': 'Nur lesen',
  'mcp.permission.readOnlyHint': 'Nur Schema-Introspektion; blockiert query und run_workflow',
  'mcp.permission.safeWrite': 'Sicheres Schreiben (Standard)',
  'mcp.permission.safeWriteHint': 'Erlaubt DML; blockiert DROP, TRUNCATE, ALTER, CREATE USER u. Ä.',
  'mcp.permission.highRiskWrite': 'Hochriskantes Schreiben',
  'mcp.permission.highRiskWriteHint': 'Keine SQL-Einschränkungen; nur die Tool-Sperre gilt',
  'mcp.permission.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for permission mode changes to take effect',
  'mcp.permission.applyHint':
    'Wird sofort angewendet, wenn der MCP Server läuft (eingebetteter Modus lädt automatisch neu)',
  'mcp.allowlist.title': 'Verbindungs-Whitelist',
  'mcp.allowlist.description':
    'Nur ausgewählte Verbindungen sind für MCP-Clients sichtbar. Entfernen Sie alle Häkchen, um alle gespeicherten Verbindungen freizugeben.',
  'mcp.allowlist.empty': 'Noch keine gespeicherten Verbindungen.',
  'mcp.allowlist.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for allowlist changes to take effect',
  'mcp.allowlist.applyHint':
    'Speichern, um Allowlist-Änderungen sofort anzuwenden, wenn der MCP Server läuft',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Copy',
  'mcp.config.copied': 'Copied',
  'mcp.config.pathHint': 'Typischer Speicherort: {path}',
  'mcp.config.commandHint':
    'Verwendet `datazen` im PATH. Für gepackte Apps ersetzen Sie den Befehl durch den absoluten Binärpfad.',
  'mcpClient.title': 'Externe MCP-Server',
  'mcpClient.description':
    'Stellen Sie eine Verbindung zu externen MCP-Servern her, um die Funktionen des KI-Assistenten zu erweitern.',
  'mcpClient.savedConfigs': 'Saved Servers',
  'mcpClient.runtimeStatus': 'Connected Servers',
  'mcpClient.addServer': 'MCP-Server hinzufügen',
  'mcpClient.serverName': 'Name',
  'mcpClient.command': 'Befehl',
  'mcpClient.args': 'Argumente',
  'mcpClient.save': 'Save',
  'mcpClient.saving': 'Saving…',
  'mcpClient.edit': 'Edit',
  'mcpClient.delete': 'Delete',
  'mcpClient.enabled': 'Enabled',
  'mcpClient.enabledForAi': 'Expose to AI Chat',
  'mcpClient.invalidId': 'ID may only contain letters, numbers, underscores, and hyphens.',
  'mcpClient.duplicateId': 'A server with this ID already exists.',
  'mcpClient.connect': 'Verbinden',
  'mcpClient.connecting': 'Verbinden…',
  'mcpClient.disconnect': 'Trennen',
  'mcpClient.tools': 'Werkzeuge',
  'mcpClient.noSavedConfigs': 'No saved MCP server configs.',
  'mcpClient.noServers': 'Keine MCP-Server verbunden.',
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
