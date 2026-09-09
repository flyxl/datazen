import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { WelcomePage } from '../WelcomePage';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { sampleDataCommands } from '../../../commands/sampleData';
import { PENDING_CONNECTION_KEY } from '../../../lib/windowManager';

const openNewConnectionDialogMock = vi.fn();
const openConnectionShareDialogMock = vi.fn();
const fetchConnectionsMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/sampleData', () => ({
  sampleDataCommands: {
    initSampleDatabase: vi.fn(),
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: {
    getState: () => ({
      fetchConnections: fetchConnectionsMock,
    }),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  PENDING_CONNECTION_KEY: 'datazen:pending-connection',
  openNewConnectionDialog: (...args: unknown[]) => openNewConnectionDialogMock(...args),
}));

vi.mock('../../../lib/connectionShare', () => ({
  openConnectionShareDialog: (...args: unknown[]) => openConnectionShareDialogMock(...args),
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

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('WelcomePage', () => {
  beforeEach(() => {
    useOnboardingStore.getState().resetOnboarding();
    localStorage.clear();
  });

  it('renders app icon, feature overview, and connection CTAs', () => {
    render(<WelcomePage />);
    expect(screen.getByTestId('welcome-page')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-app-icon')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-app-icon')).toHaveAttribute('src', './logo.png');
    expect(screen.getByText('welcome.title')).toBeInTheDocument();
    expect(screen.getByText('welcome.feature.connections.title')).toBeInTheDocument();
    expect(screen.getByText('welcome.feature.dashboard.title')).toBeInTheDocument();
    expect(screen.getByText('welcome.feature.workflow.title')).toBeInTheDocument();
    expect(screen.getByText('common.aiAssistant')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-create-connection')).toHaveTextContent(
      'welcome.createConnection',
    );
    expect(screen.getByTestId('welcome-import-connection')).toHaveTextContent(
      'common.importConnections',
    );
    expect(screen.getByText('welcome.importConnectionHint')).toBeInTheDocument();
  });

  it('create-connection CTA opens new connection dialog and starts onboarding step 1', () => {
    render(<WelcomePage />);
    expect(useOnboardingStore.getState().status).toBe('not_started');
    fireEvent.click(screen.getByTestId('welcome-create-connection'));
    expect(openNewConnectionDialogMock).toHaveBeenCalledOnce();
    expect(useOnboardingStore.getState().status).toBe('active');
    expect(useOnboardingStore.getState().step).toBe(1);
  });

  it('import-connection CTA opens connection share dialog in import mode', () => {
    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-import-connection'));
    expect(openConnectionShareDialogMock).toHaveBeenCalledOnce();
    expect(openConnectionShareDialogMock).toHaveBeenCalledWith('import');
  });
});

describe('WelcomePage Onboarding', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    fetchConnectionsMock.mockResolvedValue(undefined);
    useOnboardingStore.getState().resetOnboarding();
  });

  it('renders guided steps banner and sample sqlite quick action', () => {
    render(<WelcomePage />);
    expect(screen.getByTestId('welcome-open-sample')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-create-connection')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-skip-onboarding')).toBeInTheDocument();
  });

  it('initiates sample database when clicking quick start and queues auto-connect', async () => {
    const mockConn = { id: 'sample_sqlite', name: 'Sample E-Commerce' };
    vi.mocked(sampleDataCommands.initSampleDatabase).mockResolvedValueOnce(mockConn);

    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-open-sample'));

    await waitFor(() => {
      expect(sampleDataCommands.initSampleDatabase).toHaveBeenCalled();
      expect(fetchConnectionsMock).toHaveBeenCalled();
      expect(useOnboardingStore.getState().status).toBe('active');
      expect(useOnboardingStore.getState().step).toBe(2);
      expect(useOnboardingStore.getState().sampleConnectionId).toBe('sample_sqlite');
      expect(localStorage.getItem(PENDING_CONNECTION_KEY)).toBe(
        JSON.stringify({ connectionId: 'sample_sqlite' }),
      );
    });
  });

  it('shows error dialog when sample database init fails', async () => {
    vi.mocked(sampleDataCommands.initSampleDatabase).mockRejectedValueOnce(
      new Error('sample init failed'),
    );

    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-open-sample'));

    await waitFor(() => {
      expect(screen.getByText('sample init failed')).toBeInTheDocument();
    });
    expect(useOnboardingStore.getState().status).toBe('not_started');
    expect(localStorage.getItem(PENDING_CONNECTION_KEY)).toBeNull();
  });

  it('marks onboarding as skipped when clicking skip link', () => {
    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-skip-onboarding'));
    expect(useOnboardingStore.getState().status).toBe('skipped');
  });
});
