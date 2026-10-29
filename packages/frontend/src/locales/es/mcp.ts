/** Auto-split domain: mcp (es) */
const pack = {
  'mcp.title': 'Servidor MCP',
  'mcp.description':
    'Ejecute DataZen como servidor MCP para que las herramientas externas de IA (Claude Desktop, Cursor, etc.) puedan acceder a sus bases de datos.',
  'mcp.enabled': 'Habilitar el servidor MCP',
  'mcp.enabledHint':
    'Desactivado de forma predeterminada. Se prefiere iniciar un proceso dedicado con --mcp para Claude Desktop / Cursor',
  'mcp.status': 'Estado',
  'mcp.running': 'En ejecución',
  'mcp.stopped': 'Detenido',
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
    'Es necesario reiniciar el servidor MCP para que los cambios de herramientas surtan efecto',
  'mcp.tools.applyHint':
    'Guarde para aplicar los cambios de herramientas de inmediato cuando el servidor MCP esté en ejecución',
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
    'Se aplica de inmediato cuando el servidor MCP está en ejecución (el modo integrado se recarga automáticamente)',
  'mcp.permission.applyHint':
    'Se aplica de inmediato cuando el servidor MCP está en ejecución (el modo integrado se recarga automáticamente)',
  'mcp.allowlist.title': 'Lista blanca de conexiones',
  'mcp.allowlist.description':
    'Solo las conexiones seleccionadas son visibles para los clientes MCP. Si no hay ninguna marcada, se deniega todo: permita explícitamente las conexiones antes de que las herramientas MCP puedan acceder a ellas.',
  'mcp.allowlist.empty': 'Todavía no hay conexiones guardadas.',
  'mcp.allowlist.restartHint':
    'Guarde para aplicar los cambios de la lista blanca de inmediato cuando el servidor MCP esté en ejecución',
  'mcp.allowlist.applyHint':
    'Guarde para aplicar los cambios de la lista blanca de inmediato cuando el servidor MCP esté en ejecución',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Copiar',
  'mcp.config.copied': 'Copiado',
  'mcp.config.pathHint': 'Ubicación típica: {path}',
  'mcp.config.commandHint':
    'Se completa con la ruta completa del binario de DataZen. Verifique la ruta del comando si mueve la aplicación.',
  'mcpClient.title': 'Servidores MCP externos',
  'mcpClient.description':
    'Conéctese a servidores MCP externos para ampliar las capacidades del asistente de IA.',
  'mcpClient.savedConfigs': 'Servidores guardados',
  'mcpClient.runtimeStatus': 'Servidores conectados',
  'mcpClient.addServer': 'Agregar servidor MCP',
  'mcpClient.serverName': 'Nombre',
  'mcpClient.command': 'Comando',
  'mcpClient.args': 'Argumentos',
  'mcpClient.save': 'Guardar',
  'mcpClient.saving': 'Guardando…',
  'mcpClient.edit': 'Editar',
  'mcpClient.delete': 'Eliminar',
  'mcpClient.enabled': 'Habilitado',
  'mcpClient.enabledForAi': 'Exponer al chat de IA',
  'mcpClient.invalidId': 'El ID solo puede contener letras, números, guiones bajos y guiones.',
  'mcpClient.duplicateId': 'Ya existe un servidor con este ID.',
  'mcpClient.connect': 'Conectar',
  'mcpClient.connecting': 'Conectando…',
  'mcpClient.disconnect': 'Desconectar',
  'mcpClient.tools': 'herramientas',
  'mcpClient.noSavedConfigs': 'No hay configuraciones de servidor MCP guardadas.',
  'mcpClient.noServers': 'No hay servidores MCP conectados.',
  'mcpClient.noTools': 'Este servidor no ha informado de ninguna herramienta.',
  'mcpClient.toolList': 'Herramientas',
  'mcpClient.reconnect': 'Reintentar',
  'mcpClient.connectFailed': 'Fallido',
  'mcpClient.envVars': 'Variables de entorno',
  'mcpClient.envKey': 'Nombre de la variable',
  'mcpClient.envValue': 'Valor',
  'mcpClient.addEnv': 'Agregar variable',
  'mcpClient.removeEnv': 'Eliminar variable',
  'mcpClient.noEnvVars': 'No hay variables de entorno configuradas.',
  'mcp.saved': 'Guardado',
} as const;
export default pack;
