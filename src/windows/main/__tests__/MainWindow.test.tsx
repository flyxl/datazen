import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MainWindow } from '../MainWindow';

vi.mock('../../connection/ConnectionWindow', () => ({
  ConnectionWindow: () => <div data-testid="connection-window-shell">connection shell</div>,
}));

describe('MainWindow', () => {
  it('renders the unified connection workspace', () => {
    render(<MainWindow />);
    expect(screen.getByTestId('connection-window-shell')).toHaveTextContent('connection shell');
  });
});
