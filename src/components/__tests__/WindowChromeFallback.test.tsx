import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { WindowChromeFallback } from '../WindowChromeFallback';

vi.mock('../../hooks/usePlatform', () => ({
  usePlatform: () => 'macos',
}));

describe('WindowChromeFallback', () => {
  it('renders a macOS drag region so overlay windows can be moved before content loads', () => {
    const { container } = render(<WindowChromeFallback />);
    expect(container.querySelector('[data-testid="window-chrome-fallback"]')).toBeTruthy();
    expect(container.querySelector('[data-tauri-drag-region]')).toBeTruthy();
  });
});
