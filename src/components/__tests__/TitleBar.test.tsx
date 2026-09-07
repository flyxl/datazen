import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TitleBar } from '../TitleBar';
import { useUiStore } from '../../stores/uiStore';

const isFullscreenMock = vi.fn().mockResolvedValue(false);
const onResizedMock = vi.fn().mockResolvedValue(() => {});

vi.mock('../../hooks/usePlatform', () => ({
  usePlatform: () => 'macos',
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    isFullscreen: isFullscreenMock,
    onResized: onResizedMock,
  }),
}));

describe('TitleBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useUiStore.setState({ isFullscreen: false });
    // Simulate Tauri runtime
    (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__ =
      {};
  });

  afterEach(cleanup);

  it('renders pl-[78px] padding when window is not fullscreen on macOS', async () => {
    isFullscreenMock.mockResolvedValueOnce(false);
    render(
      <TitleBar title="Settings" leftContent={<button data-testid="settings-back">Back</button>} />,
    );

    const leftContainer = screen.getByTestId('settings-back').parentElement;
    expect(leftContainer).toHaveClass('pl-[78px]');
  });

  it('syncs fullscreen on mount and switches to pl-3 padding when fullscreen', async () => {
    isFullscreenMock.mockResolvedValueOnce(true);
    render(
      <TitleBar title="Settings" leftContent={<button data-testid="settings-back">Back</button>} />,
    );

    await waitFor(() => {
      const leftContainer = screen.getByTestId('settings-back').parentElement;
      expect(leftContainer).toHaveClass('pl-3');
      expect(leftContainer).not.toHaveClass('pl-[78px]');
    });
  });

  it('triggers syncFullscreen when leftContent changes on macOS', async () => {
    isFullscreenMock.mockResolvedValue(false);
    const { rerender } = render(<TitleBar title="Main" leftContent={null} />);

    // Now user navigates to settings, introducing leftContent
    isFullscreenMock.mockResolvedValueOnce(true);
    rerender(
      <TitleBar title="Settings" leftContent={<button data-testid="settings-back">Back</button>} />,
    );

    await waitFor(() => {
      const leftContainer = screen.getByTestId('settings-back').parentElement;
      expect(leftContainer).toHaveClass('pl-3');
    });
  });
});
