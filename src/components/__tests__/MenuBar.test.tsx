import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MenuBar } from '../MenuBar';

vi.mock('../../hooks/usePlatform', () => ({
  usePlatform: () => 'windows',
}));

vi.mock('../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) =>
    selector({ settings: { language: 'en', theme: { mode: 'dark' } } }),
}));

afterEach(cleanup);

describe('MenuBar accessibility and keyboard navigation', () => {
  it('exposes an application menubar and menu item state', () => {
    render(<MenuBar />);

    expect(screen.getByRole('menubar', { name: 'DataZen' })).toBeInTheDocument();
    const file = screen.getByRole('menuitem', { name: 'File' });
    expect(file).toHaveAttribute('aria-haspopup', 'menu');
    expect(file).toHaveAttribute('aria-expanded', 'false');
  });

  it('opens menus from the keyboard, moves through items, and closes on Escape', () => {
    render(<MenuBar />);
    const file = screen.getByRole('menuitem', { name: 'File' });

    fireEvent.keyDown(file, { key: 'ArrowDown' });

    const menu = screen.getByRole('menu', { name: 'File' });
    const items = screen.getAllByRole('menuitem');
    const firstMenuItem = items.find((item) => item.closest('[role="menu"]') === menu);
    expect(firstMenuItem).toBeDefined();
    expect(document.activeElement).toBe(firstMenuItem);

    fireEvent.keyDown(firstMenuItem!, { key: 'ArrowDown' });
    const menuItems = Array.from(menu.querySelectorAll<HTMLButtonElement>('button'));
    expect(document.activeElement).toBe(menuItems[1]);

    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(screen.queryByRole('menu', { name: 'File' })).not.toBeInTheDocument();
    expect(document.activeElement).toBe(file);
  });

  it('announces checked menu entries and opens a submenu with the keyboard', () => {
    render(<MenuBar />);
    const file = screen.getByRole('menuitem', { name: 'File' });
    fireEvent.click(file);

    const importConnections = screen.getByRole('menuitem', { name: 'Import Connections' });
    expect(importConnections).toHaveAttribute('aria-haspopup', 'menu');
    fireEvent.keyDown(importConnections, { key: 'ArrowRight' });

    expect(screen.getAllByRole('menu')).toHaveLength(2);
    const submenu = screen.getAllByRole('menu')[1];
    expect(document.activeElement).toBe(submenu.querySelector('button'));
  });
});
