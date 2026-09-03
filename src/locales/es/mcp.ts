/** Auto-split domain: mcp (es) */
const pack = {
  'mcp.title': 'Servidor MCP',
  'mcp.description':
    'Ejecute DataZen como servidor MCP para que las herramientas externas de IA (Claude Desktop, Cursor, etc.) puedan acceder a sus bases de datos.',
  'mcp.enabled': 'Habilitar el servidor MCP',
  'mcp.enabledHint':
    'Desactivado de forma predeterminada. Prefiere iniciar un proceso dedicado con --mcp para Claude Desktop/Cursor',
  'mcp.status': 'Estado',
  'mcp.running': 'Correr',
  'mcp.stopped': 'Interrumpido',
  'mcp.start': 'Iniciar el servidor MCP',
  'mcp.stop': 'Detener',
  'mcp.transport': 'Transporte',
  'mcp.usage':
    'Para usar DataZen como servidor MCP con Claude Desktop o Cursor, agregue la siguiente configuración:',
  'mcp.tools': 'Herramientas expuestas',
  'mcp.tools.description':
    'Elija qué herramientas exponer a través del servidor MCP a clientes de IA externos',
  'mcp.tools.enableAll': 'Habilitar todo',
  'mcp.tools.disableAll': 'Deshabilitar todo',
  'mcp.tools.restartHint':
    'Es necesario reiniciar MCP Server para que los cambios de herramientas surtan efecto',
  'mcp.tools.applyHint':
    'Guarda para aplicar cambios de herramientas de inmediato cuando el MCP Server esté en ejecución',
  'mcp.toggleError': 'No se pudo iniciar/detener el servidor MCP',
  'mcp.permission.title': 'Modo de permisos',
  'mcp.permission.readOnly': 'Solo lectura',
  'mcp.permission.readOnlyHint': 'Solo introspección de esquema; bloquea query y run_workflow',
  'mcp.permission.safeWrite': 'Escritura segura (predeterminado)',
  'mcp.permission.safeWriteHint':
    'Permite DML; bloquea DROP, TRUNCATE, ALTER, CREATE USER y similares',
  'mcp.permission.highRiskWrite': 'Escritura de alto riesgo',
  'mcp.permission.highRiskWriteHint':
    'Sin restricciones SQL; solo se aplica la lista de bloqueo de herramientas',
  'mcp.permission.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for permission mode changes to take effect',
  'mcp.permission.applyHint':
    'Se aplica de inmediato cuando el MCP Server está en ejecución (el modo integrado se recarga automáticamente)',
  'mcp.allowlist.title': 'Lista blanca de conexiones',
  'mcp.allowlist.description':
    'Solo las conexiones seleccionadas son visibles para los clientes MCP. Desmarca todas para exponer todas las conexiones guardadas.',
  'mcp.allowlist.empty': 'No hay conexiones guardadas aún.',
  'mcp.allowlist.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for allowlist changes to take effect',
  'mcp.allowlist.applyHint':
    'Guarda para aplicar cambios de la lista de permitidos de inmediato cuando el MCP Server esté en ejecución',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Copy',
  'mcp.config.copied': 'Copied',
  'mcp.config.pathHint': 'Ubicación típica: {path}',
  'mcp.config.commandHint':
    'Usa `datazen` en el PATH. Para aplicaciones empaquetadas, reemplaza el comando con la ruta absoluta del binario.',
  'mcpClient.title': 'Servidores MCP externos',
  'mcpClient.description':
    'Conéctese a servidores MCP externos para ampliar las capacidades del asistente de IA.',
  'mcpClient.savedConfigs': 'Saved Servers',
  'mcpClient.runtimeStatus': 'Connected Servers',
  'mcpClient.addServer': 'Agregar servidor MCP',
  'mcpClient.serverName': 'Nombre',
  'mcpClient.command': 'Dominio',
  'mcpClient.args': 'Argumentos',
  'mcpClient.save': 'Save',
  'mcpClient.saving': 'Saving…',
  'mcpClient.edit': 'Edit',
  'mcpClient.delete': 'Delete',
  'mcpClient.enabled': 'Enabled',
  'mcpClient.enabledForAi': 'Expose to AI Chat',
  'mcpClient.invalidId': 'ID may only contain letters, numbers, underscores, and hyphens.',
  'mcpClient.duplicateId': 'A server with this ID already exists.',
  'mcpClient.connect': 'Conectar',
  'mcpClient.connecting': 'Conectando…',
  'mcpClient.disconnect': 'Desconectar',
  'mcpClient.tools': 'herramientas',
  'mcpClient.noSavedConfigs': 'No saved MCP server configs.',
  'mcpClient.noServers': 'No hay servidores MCP conectados.',
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
