import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { McpSettingsSection } from '../McpSettingsSection';
import { settingsCommands } from '../../../commands/settings';
import { aiCommands } from '../../../commands/ai';
import { clearCachedAppExecutablePathForTest } from '../../../lib/mcpAgentConfig';
import { useSettingsStore } from '../../../stores/settingsStore';

afterEach(cleanup);

vi.mock('../../../hooks/useLocaleDomains', () => ({
  useLocaleDomains: () => true,
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string>) => {
      if (params?.path) return `${key}:${params.path}`;
      return key;
    },
  }),
}));

describe('McpSettingsSection', () => {
  beforeEach(() => {
    clearCachedAppExecutablePathForTest();
    vi.restoreAllMocks();
    vi.spyOn(aiCommands, 'mcpListAllTools').mockResolvedValue([]);
    vi.spyOn(aiCommands, 'mcpGetStatus').mockResolvedValue({ running: false, transport: 'stdio' });
  });

  it('renders and copies snippet with full executable path', async () => {
    vi.spyOn(settingsCommands, 'getAppExecutablePath').mockResolvedValue(
      '/Applications/DataZen.app/Contents/MacOS/datazen',
    );
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: { writeText: writeTextSpy },
    });

    render(<McpSettingsSection />);

    await waitFor(() => {
      expect(
        screen.getByText((content) =>
          content.includes('/Applications/DataZen.app/Contents/MacOS/datazen'),
        ),
      ).toBeInTheDocument();
    });

    const copyBtn = screen.getByText('mcp.config.copy');
    fireEvent.click(copyBtn);

    expect(writeTextSpy).toHaveBeenCalled();
    const copiedJson = JSON.parse(writeTextSpy.mock.calls[0][0]);
    expect(copiedJson.mcpServers.datazen.command).toBe(
      '/Applications/DataZen.app/Contents/MacOS/datazen',
    );
    expect(copiedJson.mcpServers.datazen.args).toEqual(['--mcp']);
  });

  it('switches targets and retains full executable path', async () => {
    vi.spyOn(settingsCommands, 'getAppExecutablePath').mockResolvedValue(
      '/Applications/DataZen.app/Contents/MacOS/datazen',
    );

    render(<McpSettingsSection />);

    await waitFor(() => {
      expect(
        screen.getByText((content) =>
          content.includes('/Applications/DataZen.app/Contents/MacOS/datazen'),
        ),
      ).toBeInTheDocument();
    });

    const claudeBtn = screen.getByText('mcp.config.claude');
    fireEvent.click(claudeBtn);

    await waitFor(() => {
      expect(screen.getByText(/claude_desktop_config/)).toBeInTheDocument();
    });
  });
});
