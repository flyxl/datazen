import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { MainPage } from '../MainPage';

const fetchConnectionsMock = vi.fn().mockResolvedValue(undefined);
const fetchGroupsMock = vi.fn().mockResolvedValue(undefined);
const listenCrossWindowMock = vi.fn().mockResolvedValue(() => {});
const openNewConnectionWindowMock = vi.fn();

const storeState = {
  connections: [] as Array<{ id: string }>,
  connectionsLoaded: false,
  fetchConnections: fetchConnectionsMock,
  fetchGroups: fetchGroupsMock,
};

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (sel: (s: typeof storeState) => unknown) => sel(storeState),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  listenCrossWindow: (...args: unknown[]) => listenCrossWindowMock(...args),
}));

vi.mock('../../../lib/windowManager', () => ({
  openNewConnectionWindow: (...args: unknown[]) => openNewConnectionWindowMock(...args),
}));

vi.mock('../../connection/ConnectionPage', () => ({
  ConnectionPage: () => <div data-testid="connection-page-shell">connection shell</div>,
}));

vi.mock('../welcome/WelcomePage', () => ({
  WelcomePage: () => (
    <div data-testid="welcome-page">
      welcome
      <button
        type="button"
        data-testid="welcome-create-connection"
        onClick={() => openNewConnectionWindowMock()}
      >
        create
      </button>
    </div>
  ),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../../../components/MenuBar', () => ({
  MenuBar: () => <div data-testid="menu-bar">menu</div>,
}));

vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle">theme</div>,
}));

beforeEach(() => {
  vi.clearAllMocks();
  storeState.connections = [];
  storeState.connectionsLoaded = false;
});

afterEach(() => {
  cleanup();
});

describe('MainPage', () => {
  it('shows loading until connections are loaded', () => {
    render(<MainPage />);
    expect(screen.getByTestId('main-connections-loading')).toBeInTheDocument();
    expect(screen.queryByTestId('welcome-page')).not.toBeInTheDocument();
    expect(screen.queryByTestId('connection-page-shell')).not.toBeInTheDocument();
  });

  it('renders WelcomePage when loaded with no connections', () => {
    storeState.connectionsLoaded = true;

    render(<MainPage />);
    expect(screen.getByTestId('welcome-page')).toBeInTheDocument();
    expect(screen.queryByTestId('connection-page-shell')).not.toBeInTheDocument();
  });

  it('renders ConnectionPage when connections exist', () => {
    storeState.connectionsLoaded = true;
    storeState.connections = [{ id: 'conn-1' }];

    render(<MainPage />);
    expect(screen.getByTestId('connection-page-shell')).toBeInTheDocument();
    expect(screen.queryByTestId('welcome-page')).not.toBeInTheDocument();
  });

  it('welcome CTA calls openNewConnectionWindow', () => {
    storeState.connectionsLoaded = true;

    render(<MainPage />);
    fireEvent.click(screen.getByTestId('welcome-create-connection'));
    expect(openNewConnectionWindowMock).toHaveBeenCalledOnce();
  });

  it('fetches connections on mount and listens for cross-window updates', async () => {
    render(<MainPage />);
    expect(fetchConnectionsMock).toHaveBeenCalled();
    expect(fetchGroupsMock).toHaveBeenCalled();
    await waitFor(() =>
      expect(listenCrossWindowMock).toHaveBeenCalledWith(
        'datazen:connections-changed',
        expect.any(Function),
      ),
    );
  });
});
