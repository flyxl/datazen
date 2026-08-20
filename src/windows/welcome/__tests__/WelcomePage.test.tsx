import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, afterEach } from 'vitest';
import { WelcomePage } from '../WelcomePage';

const openNewConnectionWindowMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/windowManager', () => ({
  openNewConnectionWindow: (...args: unknown[]) => openNewConnectionWindowMock(...args),
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
});

describe('WelcomePage', () => {
  it('renders feature overview and create-connection CTA', () => {
    render(<WelcomePage />);
    expect(screen.getByTestId('welcome-page')).toBeInTheDocument();
    expect(screen.getByText('welcome.title')).toBeInTheDocument();
    expect(screen.getByText('welcome.feature.connections.title')).toBeInTheDocument();
    expect(screen.getByText('welcome.feature.dashboard.title')).toBeInTheDocument();
    expect(screen.getByText('welcome.feature.workflow.title')).toBeInTheDocument();
    expect(screen.getByText('welcome.feature.ai.title')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-create-connection')).toHaveTextContent(
      'welcome.createConnection',
    );
  });

  it('create-connection CTA opens new connection window', () => {
    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-create-connection'));
    expect(openNewConnectionWindowMock).toHaveBeenCalledOnce();
  });
});
