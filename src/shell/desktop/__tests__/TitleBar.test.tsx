import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TitleBar } from '../TitleBar';
import { useUiStore } from '../../../stores/uiStore';
import { configurePlatform } from '../../../platform/contracts';

const isFullscreenMock = vi.fn().mockResolvedValue(false);
const onResizedMock = vi.fn().mockResolvedValue(() => {});

// Form-split F2: TitleBar imports the OS hook and the form constant from the
// desktop zone; the form is compile-time, so the env module gets a getter
// mock that this suite pins to the desktop branch.
vi.mock('../../../platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return true;
  },
}));

vi.mock('../../../hooks/usePlatform', () => ({
  usePlatform: () => 'macos',
  getPlatformSync: () => 'macos',
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
    // uiStore.syncFullscreen reads the platform port — point its fullscreen
    // query at the same mock the Tauri window module used to hit directly.
    configurePlatform({
      services: {} as never,
      windows: {
        openSingletonWindow: () => undefined,
        openMultiInstanceWindow: () => undefined,
        focusMainWindow: async () => undefined,
        hasOpenChildWindows: async () => false,
        windowChrome: {
          isFullscreen: isFullscreenMock,
          onFullscreenChanged: async () => () => undefined,
          setBackgroundRootColor: async () => undefined,
          onCloseRequested: async () => () => undefined,
          minimizeCurrent: async () => undefined,
          closeCurrent: async () => undefined,
        },
      },
      events: {
        emitCrossWindow: async () => undefined,
        listenCrossWindow: async () => () => undefined,
      },
    });
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
    // Deterministic across both mount effects (listener sync + leftContent
    // sync): a persistent implementation instead of a one-shot, so the
    // 250/500ms retries cannot race the once-queue.
    isFullscreenMock.mockResolvedValue(true);
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
    isFullscreenMock.mockResolvedValue(true);
    rerender(
      <TitleBar title="Settings" leftContent={<button data-testid="settings-back">Back</button>} />,
    );

    await waitFor(() => {
      const leftContainer = screen.getByTestId('settings-back').parentElement;
      expect(leftContainer).toHaveClass('pl-3');
    });
  });
});
