import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { McpClientSection } from '../McpClientSection';
import { useAiStore } from '../../../stores/aiStore';
import { useSettingsStore } from '../../../stores/settingsStore';

vi.mock('../../../hooks/useLocaleDomains', () => ({ useLocaleDomains: () => true }));
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));

afterEach(cleanup);

describe('McpClientSection help', () => {
  beforeEach(() => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, mcpClientServers: [] },
    }));
    useAiStore.setState({
      mcpServers: [],
      mcpTools: [],
      mcpError: null,
      mcpServerErrors: {},
      mcpConnecting: false,
      loadMcpServers: vi.fn().mockResolvedValue(undefined),
      loadMcpTools: vi.fn().mockResolvedValue(undefined),
    });
  });

  it('moves the section description into help without hiding empty states or errors', () => {
    useAiStore.setState({ mcpError: 'Connection unavailable' });
    render(<McpClientSection />);

    expect(screen.queryByText('mcpClient.description')).not.toBeInTheDocument();
    expect(screen.getByText('mcpClient.noSavedConfigs')).toBeInTheDocument();
    expect(screen.getByText('mcpClient.noServers')).toBeInTheDocument();
    expect(screen.getByText('Connection unavailable')).toBeInTheDocument();
    fireEvent.focus(screen.getByRole('button', { name: 'mcpClient.title' }));
    expect(screen.getByRole('tooltip')).toHaveTextContent('mcpClient.description');
  });

  it('shows tool descriptions beside tool names as help and keeps qualified names visible', () => {
    useAiStore.setState({
      mcpServers: [{ serverId: 'demo', serverName: 'Demo server', toolsCount: 2 }],
      mcpTools: [
        {
          serverId: 'demo',
          serverName: 'Demo server',
          toolName: 'query',
          qualifiedName: 'mcp__demo__query',
          description: 'Query available records.',
          inputSchema: {},
        },
        {
          serverId: 'demo',
          serverName: 'Demo server',
          toolName: 'ping',
          qualifiedName: 'mcp__demo__ping',
          inputSchema: {},
        },
      ],
    });
    render(<McpClientSection />);
    const expand = screen.getByRole('button', { name: /Demo server/ });
    fireEvent.click(expand);

    expect(screen.getByText('mcp__demo__query')).toBeInTheDocument();
    expect(screen.getByText('mcp__demo__ping')).toBeInTheDocument();
    expect(screen.queryByText('Query available records.')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'ping' })).not.toBeInTheDocument();
    const help = screen.getByRole('button', { name: 'query' });
    expect(help.parentElement).toHaveTextContent('query');
    expect(help.closest('label')).toBeNull();
    expect(help.parentElement?.closest('button')).toBeNull();
    fireEvent.focus(help);
    expect(screen.getByRole('tooltip')).toHaveTextContent('Query available records.');
    fireEvent.click(help);
    expect(expand).toHaveAttribute('aria-expanded', 'true');
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });
});
