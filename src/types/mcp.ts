/** MCP client/server configuration types. */

export interface McpServerConfig {
  id: string;
  name: string;
  transport: 'stdio' | 'sse' | 'streamable_http';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  enabled?: boolean;
}

export interface McpToolInfo {
  serverId: string;
  toolName: string;
  qualifiedName: string;
  description?: string;
  inputSchema: Record<string, unknown>;
}
