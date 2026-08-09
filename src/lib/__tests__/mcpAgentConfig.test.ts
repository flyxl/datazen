import { describe, expect, it } from 'vitest';
import { buildMcpAgentSnippet, buildMcpServerEntry } from '../mcpAgentConfig';

describe('mcpAgentConfig', () => {
  it('builds a stdio entry with --mcp', () => {
    expect(buildMcpServerEntry('/Applications/DataZen.app/Contents/MacOS/datazen')).toEqual({
      command: '/Applications/DataZen.app/Contents/MacOS/datazen',
      args: ['--mcp'],
    });
  });

  it('wraps cursor and claude snippets', () => {
    const cursor = buildMcpAgentSnippet('cursor', 'datazen');
    expect(cursor.target).toBe('cursor');
    expect(cursor.configPathHint).toContain('mcp.json');
    expect(JSON.parse(cursor.json)).toEqual({
      mcpServers: { datazen: { command: 'datazen', args: ['--mcp'] } },
    });

    const claude = buildMcpAgentSnippet('claude');
    expect(claude.configPathHint).toContain('claude_desktop_config');
    expect(claude.json).toContain('"--mcp"');
  });
});
