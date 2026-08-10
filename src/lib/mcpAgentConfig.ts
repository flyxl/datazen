/** Build MCP client config snippets for Cursor / Claude Desktop. */

export type McpAgentTarget = 'cursor' | 'claude';

export interface McpAgentSnippet {
  target: McpAgentTarget;
  /** Where users typically place this JSON fragment. */
  configPathHint: string;
  /** Pretty-printed JSON for mcpServers entry / full wrapper. */
  json: string;
}

export function buildMcpServerEntry(command: string): Record<string, unknown> {
  return {
    command,
    args: ['--mcp'],
  };
}

/** Full JSON blob ready to paste into an mcpServers map wrapper. */
export function buildMcpAgentSnippet(
  target: McpAgentTarget,
  command = 'datazen',
): McpAgentSnippet {
  const entry = buildMcpServerEntry(command);
  const wrapper = {
    mcpServers: {
      datazen: entry,
    },
  };
  const configPathHint =
    target === 'cursor'
      ? '.cursor/mcp.json (project) or Cursor Settings → MCP'
      : 'claude_desktop_config.json (Claude Desktop app support directory)';
  return {
    target,
    configPathHint,
    json: JSON.stringify(wrapper, null, 2),
  };
}

export const MCP_AGENT_TARGETS: McpAgentTarget[] = ['cursor', 'claude'];
