/** Auto-split domain: mcp (pt-BR) */
const pack = {
  'mcp.title': 'Servidor MCP',
  'mcp.description':
    'Execute o DataZen como um servidor MCP para que ferramentas externas de IA (Claude Desktop, Cursor, etc.) possam acessar seus bancos de dados',
  'mcp.enabled': 'Habilitar servidor MCP',
  'mcp.enabledHint':
    'Desativado por padrão. Prefira iniciar um processo dedicado com --mcp para Claude Desktop/Cursor',
  'mcp.status': 'Status',
  'mcp.running': 'Correndo',
  'mcp.stopped': 'Parou',
  'mcp.start': 'Iniciar servidor MCP',
  'mcp.stop': 'Parar',
  'mcp.transport': 'Transporte',
  'mcp.usage':
    'Para usar o DataZen como servidor MCP com Claude Desktop ou Cursor, adicione a seguinte configuração:',
  'mcp.tools': 'Ferramentas expostas',
  'mcp.tools.description':
    'Escolha quais ferramentas expor por meio do servidor MCP para clientes externos de IA',
  'mcp.tools.enableAll': 'Habilitar tudo',
  'mcp.tools.disableAll': 'Desativar tudo',
  'mcp.tools.restartHint':
    'O servidor MCP precisa ser reiniciado para que as alterações na ferramenta tenham efeito',
  'mcp.tools.applyHint':
    'Salve para aplicar alterações de ferramentas imediatamente quando o MCP Server estiver em execução',
  'mcp.toggleError': 'Falha ao iniciar/parar o servidor MCP',
  'mcp.permission.title': 'Modo de permissão',
  'mcp.permission.readOnly': 'Somente leitura',
  'mcp.permission.readOnlyHint': 'Apenas introspecção de esquema; bloqueia query e run_workflow',
  'mcp.permission.safeWrite': 'Escrita segura (padrão)',
  'mcp.permission.safeWriteHint':
    'Permite DML; bloqueia DROP, TRUNCATE, ALTER, CREATE USER e similares',
  'mcp.permission.highRiskWrite': 'Escrita de alto risco',
  'mcp.permission.highRiskWriteHint':
    'Sem restrições SQL; apenas a lista de bloqueio de ferramentas se aplica',
  'mcp.permission.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for permission mode changes to take effect',
  'mcp.permission.applyHint':
    'Aplica imediatamente quando o MCP Server estiver em execução (o modo incorporado recarrega automaticamente)',
  'mcp.allowlist.title': 'Lista de permissões de conexão',
  'mcp.allowlist.description':
    'Apenas as conexões selecionadas são visíveis para clientes MCP. Desmarque todas para expor todas as conexões salvas.',
  'mcp.allowlist.empty': 'Nenhuma conexão salva ainda.',
  'mcp.allowlist.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for allowlist changes to take effect',
  'mcp.allowlist.applyHint':
    'Salve para aplicar alterações da lista de permissões imediatamente quando o MCP Server estiver em execução',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Copy',
  'mcp.config.copied': 'Copied',
  'mcp.config.pathHint': 'Localização típica: {path}',
  'mcp.config.commandHint':
    'Usa `datazen` no PATH. Para apps empacotados, substitua o comando pelo caminho absoluto do binário.',
  'mcpClient.title': 'Servidores MCP Externos',
  'mcpClient.description':
    'Conecte-se a servidores MCP externos para ampliar os recursos do assistente de IA.',
  'mcpClient.savedConfigs': 'Saved Servers',
  'mcpClient.runtimeStatus': 'Connected Servers',
  'mcpClient.addServer': 'Adicionar servidor MCP',
  'mcpClient.serverName': 'Nome',
  'mcpClient.command': 'Comando',
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
  'mcpClient.tools': 'ferramentas',
  'mcpClient.noSavedConfigs': 'No saved MCP server configs.',
  'mcpClient.noServers': 'Nenhum servidor MCP conectado.',
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
