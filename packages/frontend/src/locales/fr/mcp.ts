/** Auto-split domain: mcp (fr) */
const pack = {
  'mcp.title': 'Serveur MCP',
  'mcp.description':
    "Exécutez DataZen en tant que serveur MCP afin que les outils d'IA externes (Claude Desktop, Cursor, etc.) puissent accéder à vos bases de données",
  'mcp.enabled': 'Activer le serveur MCP',
  'mcp.enabledHint':
    'Désactivé par défaut. Préférez lancer un processus dédié avec --mcp pour Claude Desktop / Cursor',
  'mcp.status': 'Statut',
  'mcp.running': "En cours d'exécution",
  'mcp.stopped': 'Arrêté',
  'mcp.start': 'Démarrer le serveur MCP',
  'mcp.stop': 'Arrêt',
  'mcp.transport': 'Transport',
  'mcp.usage':
    'Pour utiliser DataZen comme serveur MCP avec Claude Desktop ou Cursor, ajoutez la configuration suivante :',
  'mcp.tools': 'Outils exposés',
  'mcp.tools.description':
    'Choisissez les outils à exposer via le serveur MCP aux clients IA externes',
  'mcp.tools.enableAll': 'Activer tout',
  'mcp.tools.disableAll': 'Désactiver tout',
  'mcp.tools.restartHint':
    'Le serveur MCP doit être redémarré pour que les modifications apportées aux outils prennent effet',
  'mcp.tools.applyHint':
    "Enregistrez pour appliquer immédiatement les modifications d'outils lorsque le MCP Server est en cours d'exécution",
  'mcp.toggleError': 'Échec du démarrage/arrêt du serveur MCP',
  'mcp.permission.title': 'Mode de permission',
  'mcp.permission.readOnly': 'Lecture seule',
  'mcp.permission.readOnlyHint':
    'Introspection du schéma uniquement ; bloque query et run_workflow',
  'mcp.permission.safeWrite': 'Écriture sûre (par défaut)',
  'mcp.permission.safeWriteHint':
    'Autorise le DML ; bloque DROP, TRUNCATE, ALTER, CREATE USER et similaires',
  'mcp.permission.highRiskWrite': 'Écriture à haut risque',
  'mcp.permission.highRiskWriteHint':
    "Aucune restriction SQL ; seule la liste de blocage d'outils s'applique",
  'mcp.permission.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for permission mode changes to take effect',
  'mcp.permission.applyHint':
    "S'applique immédiatement lorsque le MCP Server est en cours d'exécution (le mode intégré se recharge automatiquement)",
  'mcp.allowlist.title': 'Liste blanche des connexions',
  'mcp.allowlist.description':
    'Seules les connexions sélectionnées sont visibles par les clients MCP. Décochez toutes les cases pour exposer toutes les connexions enregistrées.',
  'mcp.allowlist.empty': 'Aucune connexion enregistrée pour le moment.',
  'mcp.allowlist.restartHint':
    'Restart MCP Server (or relaunch datazen --mcp) for allowlist changes to take effect',
  'mcp.allowlist.applyHint':
    "Enregistrez pour appliquer immédiatement les modifications de la liste blanche lorsque le MCP Server est en cours d'exécution",
  'mcp.config.cursor': 'Cursor',
  'mcp.config.claude': 'Claude Desktop',
  'mcp.config.copy': 'Copier',
  'mcp.config.copied': 'Copié',
  'mcp.config.pathHint': 'Emplacement typique : {path}',
  'mcp.config.commandHint':
    'Utilise `datazen` dans le PATH. Pour les applications empaquetées, remplacez la commande par le chemin binaire absolu.',
  'mcpClient.title': 'Serveurs MCP externes',
  'mcpClient.description':
    "Connectez-vous à des serveurs MCP externes pour étendre les capacités de l'assistant AI.",
  'mcpClient.savedConfigs': 'Serveurs enregistrés',
  'mcpClient.runtimeStatus': 'Serveurs connectés',
  'mcpClient.addServer': 'Ajouter un serveur MCP',
  'mcpClient.serverName': 'Nom',
  'mcpClient.command': 'Commande',
  'mcpClient.args': 'Arguments',
  'mcpClient.save': 'Enregistrer',
  'mcpClient.saving': 'Enregistrement…',
  'mcpClient.edit': 'Modifier',
  'mcpClient.delete': 'Supprimer',
  'mcpClient.enabled': 'Activé',
  'mcpClient.enabledForAi': 'Exposer au chat IA',
  'mcpClient.invalidId':
    'L’ID ne peut contenir que des lettres, des chiffres, des tirets bas et des tirets.',
  'mcpClient.duplicateId': 'Un serveur avec cet ID existe déjà.',
  'mcpClient.connect': 'Connecter',
  'mcpClient.connecting': 'Connexion…',
  'mcpClient.disconnect': 'Déconnecter',
  'mcpClient.tools': 'Outils',
  'mcpClient.noSavedConfigs': 'Aucune configuration de serveur MCP enregistrée.',
  'mcpClient.noServers': 'Aucun serveur MCP connecté.',
  'mcpClient.noTools': 'Aucun outil signalé par ce serveur.',
  'mcpClient.toolList': 'Outils',
  'mcpClient.reconnect': 'Réessayer',
  'mcpClient.connectFailed': 'Échec',
  'mcpClient.envVars': 'Variables d’environnement',
  'mcpClient.envKey': 'Nom de la variable',
  'mcpClient.envValue': 'Valeur',
  'mcpClient.addEnv': 'Ajouter une variable',
  'mcpClient.removeEnv': 'Supprimer la variable',
  'mcpClient.noEnvVars': 'Aucune variable d’environnement configurée.',
  'mcp.saved': 'Enregistré',
} as const;
export default pack;
