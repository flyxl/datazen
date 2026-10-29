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

  it('shows explanations only in tooltips and keeps permission help outside radio labels', async () => {
    vi.spyOn(settingsCommands, 'getAppExecutablePath').mockResolvedValue('/usr/bin/datazen');
    vi.spyOn(aiCommands, 'mcpListAllTools').mockResolvedValue(['query']);
    const updateSettings = vi.fn();
    render(
      <McpSettingsSection
        settings={useSettingsStore.getState().settings}
        onSettingsChange={updateSettings}
      />,
    );

    await screen.findByText('query');
    expect(screen.getByText('mcp.stopped')).toBeInTheDocument();
    expect(screen.getByText('mcp.allowlist.empty')).toBeInTheDocument();
    expect(screen.getByText(/mcp.config.pathHint:/)).toBeInTheDocument();

    for (const [label, description] of [
      ['mcp.title', 'mcp.description'],
      ['mcp.enabled', 'mcp.enabledHint'],
      ['mcp.permission.title', 'mcp.permission.applyHint'],
      ['mcp.allowlist.title', 'mcp.allowlist.description mcp.allowlist.applyHint'],
      ['mcp.tools', 'mcp.tools.description mcp.tools.applyHint'],
      ['mcp.permission.readOnly', 'mcp.permission.readOnlyHint'],
      ['mcp.permission.safeWrite', 'mcp.permission.safeWriteHint'],
      ['mcp.permission.highRiskWrite', 'mcp.permission.highRiskWriteHint'],
    ]) {
      expect(screen.queryByText(description)).not.toBeInTheDocument();
      const help = screen.getByRole('button', { name: label });
      expect(help.closest('label')).toBeNull();
      expect(help.parentElement?.closest('button')).toBeNull();
      fireEvent.focus(help);
      expect(screen.getByRole('tooltip')).toHaveTextContent(description);
      fireEvent.keyDown(document, { key: 'Escape' });
    }

    const configHelp = screen
      .getAllByRole('button', { name: 'mcp.config.cursor' })
      .find((button) => button.textContent === '?');
    expect(configHelp).toBeDefined();
    fireEvent.focus(configHelp!);
    expect(screen.getByRole('tooltip')).toHaveTextContent('mcp.usage mcp.config.commandHint');
    fireEvent.click(configHelp!);
    expect(updateSettings).not.toHaveBeenCalled();
    fireEvent.keyDown(document, { key: 'Escape' });

    fireEvent.click(screen.getByRole('radio', { name: 'mcp.permission.readOnly' }));
    expect(updateSettings).toHaveBeenCalledWith({ mcpPermissionMode: 'read_only' });
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
