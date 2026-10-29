/** Auto-split domain: mcp (de) */
const pack = {
  'mcp.title': 'MCP-Server',
  'mcp.description':
    'Führen Sie DataZen als MCP-Server aus, damit externe KI-Tools (Claude Desktop, Cursor usw.) auf Ihre Datenbanken zugreifen können',
  'mcp.enabled': 'MCP-Server aktivieren',
  'mcp.enabledHint':
    'Standardmäßig deaktiviert. Starten Sie lieber einen dedizierten Prozess mit --mcp für Claude Desktop/Cursor',
  'mcp.status': 'Status',
  'mcp.running': 'Läuft',
  'mcp.stopped': 'Angehalten',
  'mcp.saved': 'Gespeichert',
  'mcp.start': 'MCP-Server starten',
  'mcp.stop': 'Stoppen',
  'mcp.transport': 'Transport',
  'mcp.usage':
    'Um DataZen als MCP-Server mit Claude Desktop oder Cursor zu verwenden, fügen Sie die folgende Konfiguration hinzu:',
  'mcp.tools': 'Freigegebene Tools',
  'mcp.tools.description':
    'Wählen Sie aus, welche Tools über den MCP-Server externen KI-Clients zugänglich gemacht werden sollen',
  'mcp.tools.enableAll': 'Alle aktivieren',
  'mcp.tools.disableAll': 'Alle deaktivieren',
  'mcp.tools.restartHint':
    'Speichern, um Tool-Änderungen sofort anzuwenden, wenn der MCP-Server läuft',
  'mcp.tools.applyHint':
    'Speichern, um Tool-Änderungen sofort anzuwenden, wenn der MCP-Server läuft',
  'mcp.toggleError': 'Der MCP-Server konnte nicht gestartet/gestoppt werden',
  'mcp.permission.title': 'Berechtigungsmodus',
  'mcp.permission.readOnly': 'Nur lesen',
  'mcp.permission.readOnlyHint': 'Nur Schema-Introspektion; blockiert query und run_workflow',
  'mcp.permission.safeWrite': 'Sicheres Schreiben (Standard)',
  'mcp.permission.safeWriteHint': 'Erlaubt DML; blockiert DROP, TRUNCATE, ALTER, CREATE USER u. Ä.',
  'mcp.permission.highRiskWrite': 'Hochriskantes Schreiben',
  'mcp.permission.highRiskWriteHint': 'Keine SQL-Einschränkungen; nur die Tool-Sperre gilt',
  'mcp.permission.restartHint':
    'Wird sofort angewendet, wenn der MCP-Server läuft (eingebetteter Modus lädt automatisch neu)',
  'mcp.permission.applyHint':
    'Wird sofort angewendet, wenn der MCP-Server läuft (eingebetteter Modus lädt automatisch neu)',
  'mcp.allowlist.title': 'Verbindungs-Allowlist',
  'mcp.allowlist.description':
    'Nur ausgewählte Verbindungen sind für MCP-Clients sichtbar. Ist kein Häkchen gesetzt, wird der Zugriff vollständig verweigert — erlauben Sie die benötigten Verbindungen ausdrücklich, damit MCP-Tools darauf zugreifen können.',
  'mcp.allowlist.empty': 'Noch keine gespeicherten Verbindungen.',
  'mcp.allowlist.restartHint':
    'Speichern, um Allowlist-Änderungen sofort anzuwenden, wenn der MCP-Server läuft',
  'mcp.allowlist.applyHint':
    'Speichern, um Allowlist-Änderungen sofort anzuwenden, wenn der MCP-Server läuft',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Kopieren',
  'mcp.config.copied': 'Kopiert',
  'mcp.config.pathHint': 'Typischer Speicherort: {path}',
  'mcp.config.commandHint':
    'Wird mit dem vollständigen Binärpfad von DataZen befüllt. Prüfen Sie den Befehlspfad, falls die Anwendung verschoben wurde.',
  'mcpClient.title': 'Externe MCP-Server',
  'mcpClient.description':
    'Stellen Sie eine Verbindung zu externen MCP-Servern her, um die Funktionen des KI-Assistenten zu erweitern.',
  'mcpClient.savedConfigs': 'Gespeicherte Server',
  'mcpClient.runtimeStatus': 'Verbundene Server',
  'mcpClient.addServer': 'MCP-Server hinzufügen',
  'mcpClient.serverName': 'Name',
  'mcpClient.command': 'Befehl',
  'mcpClient.args': 'Argumente',
  'mcpClient.save': 'Speichern',
  'mcpClient.saving': 'Wird gespeichert…',
  'mcpClient.edit': 'Bearbeiten',
  'mcpClient.delete': 'Löschen',
  'mcpClient.enabled': 'Aktiviert',
  'mcpClient.enabledForAi': 'Für den KI-Chat freigeben',
  'mcpClient.invalidId':
    'Die ID darf nur Buchstaben, Zahlen, Unterstriche und Bindestriche enthalten.',
  'mcpClient.duplicateId': 'Ein Server mit dieser ID existiert bereits.',
  'mcpClient.connect': 'Verbinden',
  'mcpClient.connecting': 'Verbinden…',
  'mcpClient.disconnect': 'Trennen',
  'mcpClient.tools': 'Tools',
  'mcpClient.noSavedConfigs': 'Keine gespeicherten MCP-Server-Konfigurationen.',
  'mcpClient.noServers': 'Keine MCP-Server verbunden.',
  'mcpClient.noTools': 'Dieser Server meldet keine Tools.',
  'mcpClient.toolList': 'Tools',
  'mcpClient.reconnect': 'Erneut versuchen',
  'mcpClient.connectFailed': 'Fehlgeschlagen',
  'mcpClient.envVars': 'Umgebungsvariablen',
  'mcpClient.envKey': 'Variablenname',
  'mcpClient.envValue': 'Wert',
  'mcpClient.addEnv': 'Variable hinzufügen',
  'mcpClient.removeEnv': 'Variable entfernen',
  'mcpClient.noEnvVars': 'Keine Umgebungsvariablen konfiguriert.',
} as const;
export default pack;
