/** Auto-split domain: mcp (pt-BR) */
const pack = {
  'mcp.title': 'Servidor MCP',
  'mcp.description':
    'Execute o DataZen como um servidor MCP para que ferramentas externas de IA (Claude Desktop, Cursor, etc.) possam acessar seus bancos de dados',
  'mcp.enabled': 'Habilitar servidor MCP',
  'mcp.enabledHint':
    'Desativado por padrão. Prefira iniciar um processo dedicado com --mcp para Claude Desktop/Cursor',
  'mcp.status': 'Status',
  'mcp.running': 'Em execução',
  'mcp.stopped': 'Parado',
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
    'Reinicie o servidor MCP (ou relance datazen --mcp) para que as alterações do modo de permissão entrem em vigor',
  'mcp.permission.applyHint':
    'Aplica imediatamente quando o MCP Server estiver em execução (o modo incorporado recarrega automaticamente)',
  'mcp.allowlist.title': 'Lista de permissões de conexão',
  'mcp.allowlist.description':
    'Apenas as conexões selecionadas são visíveis para clientes MCP. Desmarque todas para expor todas as conexões salvas.',
  'mcp.allowlist.empty': 'Nenhuma conexão salva ainda.',
  'mcp.allowlist.restartHint':
    'Reinicie o servidor MCP (ou relance datazen --mcp) para que as alterações da lista de permissões entrem em vigor',
  'mcp.allowlist.applyHint':
    'Salve para aplicar alterações da lista de permissões imediatamente quando o MCP Server estiver em execução',
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Copiar',
  'mcp.config.copied': 'Copiado',
  'mcp.config.pathHint': 'Localização típica: {path}',
  'mcp.config.commandHint':
    'Usa `datazen` no PATH. Para apps empacotados, substitua o comando pelo caminho absoluto do binário.',
  'mcpClient.title': 'Servidores MCP Externos',
  'mcpClient.description':
    'Conecte-se a servidores MCP externos para ampliar os recursos do assistente de IA.',
  'mcpClient.savedConfigs': 'Servidores salvos',
  'mcpClient.runtimeStatus': 'Servidores conectados',
  'mcpClient.addServer': 'Adicionar servidor MCP',
  'mcpClient.serverName': 'Nome',
  'mcpClient.command': 'Comando',
  'mcpClient.args': 'Argumentos',
  'mcpClient.save': 'Salvar',
  'mcpClient.saving': 'Salvando…',
  'mcpClient.edit': 'Editar',
  'mcpClient.delete': 'Excluir',
  'mcpClient.enabled': 'Habilitado',
  'mcpClient.enabledForAi': 'Expor para o bate-papo de IA',
  'mcpClient.invalidId': 'O ID pode conter apenas letras, números, sublinhados e hífens.',
  'mcpClient.duplicateId': 'Já existe um servidor com este ID.',
  'mcpClient.connect': 'Conectar',
  'mcpClient.connecting': 'Conectando…',
  'mcpClient.disconnect': 'Desconectar',
  'mcpClient.tools': 'ferramentas',
  'mcpClient.noSavedConfigs': 'Nenhuma configuração de servidor MCP salva.',
  'mcpClient.noServers': 'Nenhum servidor MCP conectado.',
  'mcpClient.noTools': 'Este servidor não reportou nenhuma ferramenta.',
  'mcpClient.toolList': 'Ferramentas',
  'mcpClient.reconnect': 'Tentar novamente',
  'mcpClient.connectFailed': 'Falhou',
  'mcpClient.envVars': 'Variáveis de ambiente',
  'mcpClient.envKey': 'Nome da variável',
  'mcpClient.envValue': 'Valor',
  'mcpClient.addEnv': 'Adicionar variável',
  'mcpClient.removeEnv': 'Remover variável',
  'mcpClient.noEnvVars': 'Nenhuma variável de ambiente configurada.',
  'mcp.saved': 'Salvo',
} as const;
export default pack;
