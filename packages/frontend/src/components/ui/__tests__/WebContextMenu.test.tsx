import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebContextMenuHost } from '../WebContextMenu';
import { showWebContextMenu, useContextMenuStore } from '../../../stores/contextMenuStore';

describe('WebContextMenuHost', () => {
  afterEach(() => {
    useContextMenuStore.getState().hide();
    cleanup();
  });

  it('renders nothing when closed', () => {
    render(<WebContextMenuHost />);
    expect(screen.queryByTestId('web-context-menu')).toBeNull();
  });

  it('shows items and runs actions', async () => {
    const action = vi.fn();
    render(<WebContextMenuHost />);
    showWebContextMenu(
      [
        { kind: 'item', id: 'run', label: 'Run', action },
        { kind: 'separator' },
        { kind: 'item', id: 'off', label: 'Off', enabled: false, action: () => undefined },
      ],
      { x: 20, y: 30 },
    );
    const menu = await screen.findByTestId('web-context-menu');
    expect(menu).toBeTruthy();
    expect(screen.getByTestId('web-context-item-off')).toBeDisabled();
    fireEvent.click(screen.getByTestId('web-context-item-run'));
    expect(action).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId('web-context-menu')).toBeNull();
  });

  it('opens a submenu when its trigger receives keyboard focus', async () => {
    const nested = vi.fn();
    render(<WebContextMenuHost />);
    showWebContextMenu(
      [
        {
          kind: 'submenu',
          id: 'more',
          label: 'More',
          items: [{ kind: 'item', id: 'nested', label: 'Nested', action: nested }],
        },
      ],
      { x: 20, y: 30 },
    );
    await screen.findByTestId('web-context-menu');
    fireEvent.focus(screen.getByTestId('web-context-submenu-trigger-more'));
    expect(await screen.findByTestId('web-context-submenu')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('web-context-item-nested'));
    expect(nested).toHaveBeenCalledOnce();
  });

  it('opens a submenu on hover and does not clip near the right edge', async () => {
    vi.spyOn(window, 'innerWidth', 'get').mockReturnValue(400);
    vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(300);
    const nested = vi.fn();
    render(<WebContextMenuHost />);
    showWebContextMenu(
      [
        {
          kind: 'submenu',
          id: 'more',
          label: 'More',
          items: [{ kind: 'item', id: 'nested', label: 'Nested', action: nested }],
        },
      ],
      { x: 360, y: 20 },
    );
    await screen.findByTestId('web-context-menu');
    fireEvent.mouseEnter(screen.getByTestId('web-context-submenu-trigger-more'));
    const sub = await screen.findByTestId('web-context-submenu');
    await waitFor(() => {
      const left = Number.parseFloat((sub as HTMLElement).style.left || '0');
      expect(left + sub.getBoundingClientRect().width).toBeLessThanOrEqual(400);
    });
    fireEvent.click(screen.getByTestId('web-context-item-nested'));
    expect(nested).toHaveBeenCalledTimes(1);
  });

  it('closes on Escape', async () => {
    render(<WebContextMenuHost />);
    showWebContextMenu([{ kind: 'item', id: 'a', label: 'A', action: () => undefined }], {
      x: 10,
      y: 10,
    });
    await screen.findByTestId('web-context-menu');
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('web-context-menu')).toBeNull();
  });

  it.each(['blur', 'resize'] as const)('closes on window %s', async (eventName) => {
    render(<WebContextMenuHost />);
    showWebContextMenu([{ kind: 'item', id: 'a', label: 'A', action: () => undefined }], {
      x: 10,
      y: 10,
    });
    await screen.findByTestId('web-context-menu');

    fireEvent(window, new Event(eventName));
    expect(screen.queryByTestId('web-context-menu')).toBeNull();
  });
});
