import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MainPage } from '../MainPage';

vi.mock('../../connection/ConnectionPage', () => ({
  ConnectionPage: () => <div data-testid="connection-page-shell">connection shell</div>,
}));

describe('MainPage', () => {
  it('renders the unified connection workspace', () => {
    render(<MainPage />);
    expect(screen.getByTestId('connection-page-shell')).toHaveTextContent('connection shell');
  });
});
