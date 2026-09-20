import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { ConnectionAdvancedSettings } from '../ConnectionAdvancedSettings';
import type { ConnectionFormState } from '../useConnectionForm';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

afterEach(cleanup);

function createMockForm(overrides: Partial<ConnectionFormState> = {}): ConnectionFormState {
  return {
    showAdvanced: true,
    setShowAdvanced: vi.fn(),
    readOnly: false,
    setReadOnly: vi.fn(),
    driverReadOnly: false,
    supportsSSL: false,
    sslMode: 'prefer',
    sslOptions: [],
    setSslMode: vi.fn(),
    colorTag: '#3b82f6',
    setColorTag: vi.fn(),
    group: '',
    setGroup: vi.fn(),
    supportsSSH: true,
    tunnelKind: 'none',
    setTunnelKind: vi.fn(),
    sshEnabled: false,
    setSshEnabled: vi.fn(),
    sshHost: '',
    setSshHost: vi.fn(),
    sshPort: '22',
    setSshPort: vi.fn(),
    sshUsername: '',
    setSshUsername: vi.fn(),
    sshAuthMethod: 'password',
    setSshAuthMethod: vi.fn(),
    sshPassword: '',
    setSshPassword: vi.fn(),
    sshKeyPath: '',
    setSshKeyPath: vi.fn(),
    sshPassphrase: '',
    setSshPassphrase: vi.fn(),
    sshJumpEnabled: false,
    setSshJumpEnabled: vi.fn(),
    sshJumpHost: '',
    setSshJumpHost: vi.fn(),
    sshJumpPort: '22',
    setSshJumpPort: vi.fn(),
    sshJumpUsername: '',
    setSshJumpUsername: vi.fn(),
    sshJumpAuthMethod: 'password',
    setSshJumpAuthMethod: vi.fn(),
    sshJumpPassword: '',
    setSshJumpPassword: vi.fn(),
    sshJumpKeyPath: '',
    setSshJumpKeyPath: vi.fn(),
    sshJumpPassphrase: '',
    setSshJumpPassphrase: vi.fn(),
    httpProxyHost: '',
    setHttpProxyHost: vi.fn(),
    httpProxyPort: '8080',
    setHttpProxyPort: vi.fn(),
    httpProxyScheme: 'http',
    setHttpProxyScheme: vi.fn(),
    httpProxyUsername: '',
    setHttpProxyUsername: vi.fn(),
    httpProxyPassword: '',
    setHttpProxyPassword: vi.fn(),
    httpProxyTimeout: '30',
    setHttpProxyTimeout: vi.fn(),
    wsUrl: '',
    setWsUrl: vi.fn(),
    wsMode: 'datazen_v1',
    setWsMode: vi.fn(),
    wsAuthToken: '',
    setWsAuthToken: vi.fn(),
    wsTimeout: '30',
    setWsTimeout: vi.fn(),
    formVariant: 'standard',
    ...overrides,
  } as unknown as ConnectionFormState;
}

describe('ConnectionAdvancedSettings', () => {
  it('allows toggling read-only checkbox when driver is not read-only', () => {
    const setReadOnly = vi.fn();
    const form = createMockForm({ readOnly: false, driverReadOnly: false, setReadOnly });

    render(<ConnectionAdvancedSettings form={form} />);

    const checkbox = screen.getByRole('checkbox', { name: /newConn\.readOnly/i });
    expect(checkbox).not.toBeDisabled();
    expect(checkbox).not.toBeChecked();

    fireEvent.click(checkbox);
    expect(setReadOnly).toHaveBeenCalledWith(true);
  });

  it('disables read-only checkbox and shows locked hint when driver is read-only', () => {
    const setReadOnly = vi.fn();
    const form = createMockForm({ readOnly: true, driverReadOnly: true, setReadOnly });

    render(<ConnectionAdvancedSettings form={form} />);

    const checkbox = screen.getByRole('checkbox', { name: /newConn\.readOnly/i });
    expect(checkbox).toBeDisabled();
    expect(checkbox).toBeChecked();
    expect(screen.getByText('newConn.driverReadOnlyLocked')).toBeInTheDocument();

    fireEvent.click(checkbox);
    expect(setReadOnly).not.toHaveBeenCalled();
  });

  it('shows tunnel panel when supportsSSH and tunnelKind is none', () => {
    const form = createMockForm({ tunnelKind: 'none' });
    render(<ConnectionAdvancedSettings form={form} />);
    fireEvent.click(screen.getByTestId('new-conn-tunnel-toggle'));
    expect(screen.getByTestId('new-conn-tunnel-kind')).toBeInTheDocument();
  });

  it('renders HttpProxyTunnelFields when tunnelKind is httpProxy', () => {
    const form = createMockForm({ tunnelKind: 'httpProxy' });
    render(<ConnectionAdvancedSettings form={form} />);
    fireEvent.click(screen.getByTestId('new-conn-tunnel-toggle'));
    expect(screen.getByTestId('new-conn-http-proxy-fields')).toBeInTheDocument();
    expect(screen.queryByTestId('new-conn-ws-fields')).not.toBeInTheDocument();
  });

  it('renders WebSocketTunnelFields when tunnelKind is websocket', () => {
    const form = createMockForm({ tunnelKind: 'websocket' });
    render(<ConnectionAdvancedSettings form={form} />);
    fireEvent.click(screen.getByTestId('new-conn-tunnel-toggle'));
    expect(screen.getByTestId('new-conn-ws-fields')).toBeInTheDocument();
    expect(screen.queryByTestId('new-conn-http-proxy-fields')).not.toBeInTheDocument();
  });
});
