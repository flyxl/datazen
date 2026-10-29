import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  buildMcpAgentSnippet,
  buildMcpServerEntry,
  clearCachedAppExecutablePathForTest,
  formatMcpCliCommand,
  getAppExecutablePath,
} from '../mcpAgentConfig';
import { settingsCommands } from '../../commands/settings';

describe('mcpAgentConfig', () => {
  beforeEach(() => {
    clearCachedAppExecutablePathForTest();
    vi.restoreAllMocks();
  });

  it('builds a stdio entry with --mcp', () => {
    expect(buildMcpServerEntry('/Applications/DataZen.app/Contents/MacOS/datazen')).toEqual({
      command: '/Applications/DataZen.app/Contents/MacOS/datazen',
      args: ['--mcp'],
    });
  });

  it('formats cli command with quotes when path contains spaces', () => {
    expect(formatMcpCliCommand()).toBe('datazen --mcp');
    expect(formatMcpCliCommand('datazen')).toBe('datazen --mcp');
    expect(formatMcpCliCommand('/Applications/DataZen.app/Contents/MacOS/datazen')).toBe(
      '/Applications/DataZen.app/Contents/MacOS/datazen --mcp',
    );
    expect(formatMcpCliCommand('/Applications/Data Zen.app/Contents/MacOS/datazen')).toBe(
      '"/Applications/Data Zen.app/Contents/MacOS/datazen" --mcp',
    );
    expect(formatMcpCliCommand('"/Applications/Data Zen.app/Contents/MacOS/datazen"')).toBe(
      '"/Applications/Data Zen.app/Contents/MacOS/datazen" --mcp',
    );
  });

  it('wraps cursor and claude snippets with custom executable command', () => {
    const cursor = buildMcpAgentSnippet(
      'cursor',
      '/Applications/DataZen.app/Contents/MacOS/datazen',
    );
    expect(cursor.target).toBe('cursor');
    expect(cursor.configPathHint).toContain('mcp.json');
    expect(JSON.parse(cursor.json)).toEqual({
      mcpServers: {
        datazen: {
          command: '/Applications/DataZen.app/Contents/MacOS/datazen',
          args: ['--mcp'],
        },
      },
    });

    const claude = buildMcpAgentSnippet('claude', 'datazen');
    expect(claude.configPathHint).toContain('claude_desktop_config');
    expect(claude.json).toContain('"--mcp"');
  });

  it('resolves app executable path from IPC and caches result', async () => {
    const getAppExecutablePathSpy = vi
      .spyOn(settingsCommands, 'getAppExecutablePath')
      .mockResolvedValue('/Applications/DataZen.app/Contents/MacOS/datazen');

    const path1 = await getAppExecutablePath();
    expect(path1).toBe('/Applications/DataZen.app/Contents/MacOS/datazen');

    const path2 = await getAppExecutablePath();
    expect(path2).toBe('/Applications/DataZen.app/Contents/MacOS/datazen');
    expect(getAppExecutablePathSpy).toHaveBeenCalledTimes(1);
  });

  it('falls back to "datazen" when IPC fails or returns empty', async () => {
    vi.spyOn(settingsCommands, 'getAppExecutablePath').mockRejectedValue(
      new Error('IPC unavailable'),
    );

    const fallback = await getAppExecutablePath();
    expect(fallback).toBe('datazen');
  });
});
