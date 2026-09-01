import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WindowControls } from '../WindowControls';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe('WindowControls accessibility', () => {
  it('uses localized labels for each window action', () => {
    render(<WindowControls />);

    expect(screen.getByRole('button', { name: 'menu.minimize' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'menu.zoom' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'menu.closeWindow' })).toBeInTheDocument();
  });
});
