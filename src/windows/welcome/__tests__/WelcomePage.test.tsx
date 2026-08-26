import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, afterEach } from 'vitest';
import { WelcomePage } from '../WelcomePage';

const openNewConnectionDialogMock = vi.fn();
const openConnectionShareDialogMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/windowManager', () => ({
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

  it('create-connection CTA opens new connection dialog', () => {
    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-create-connection'));
    expect(openNewConnectionDialogMock).toHaveBeenCalledOnce();
  });

  it('import-connection CTA opens connection share dialog in import mode', () => {
    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-import-connection'));
    expect(openConnectionShareDialogMock).toHaveBeenCalledOnce();
    expect(openConnectionShareDialogMock).toHaveBeenCalledWith('import');
  });
});
