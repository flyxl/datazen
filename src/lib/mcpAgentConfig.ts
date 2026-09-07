import { useEffect, useState } from 'react';
import { settingsCommands } from '../commands/settings';

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

/** Formats CLI launch command (with quotes if executable path contains spaces). */
export function formatMcpCliCommand(command = 'datazen'): string {
  const trimmed = command.trim() || 'datazen';
  const hasQuotes =
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"));
  const quoted = trimmed.includes(' ') && !hasQuotes ? `"${trimmed}"` : trimmed;
  return `${quoted} --mcp`;
}

let cachedAppExecutablePath: string | null = null;

export async function getAppExecutablePath(): Promise<string> {
  if (cachedAppExecutablePath) return cachedAppExecutablePath;
  try {
    const p = await settingsCommands.getAppExecutablePath();
    if (p && typeof p === 'string' && p.trim().length > 0) {
      cachedAppExecutablePath = p.trim();
      return cachedAppExecutablePath;
    }
  } catch {
    // In non-Tauri or test environments, fallback safely to default CLI command
  }
  return 'datazen';
}

export function clearCachedAppExecutablePathForTest(): void {
  cachedAppExecutablePath = null;
}

export function useAppExecutablePath(): string {
  const [exePath, setExePath] = useState<string>(cachedAppExecutablePath ?? 'datazen');

  useEffect(() => {
    if (cachedAppExecutablePath) {
      setExePath(cachedAppExecutablePath);
      return;
    }
    let cancelled = false;
    void getAppExecutablePath().then((path) => {
      if (!cancelled) {
        setExePath(path);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return exePath;
}

/** Full JSON blob ready to paste into an mcpServers map wrapper. */
export function buildMcpAgentSnippet(target: McpAgentTarget, command = 'datazen'): McpAgentSnippet {
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
