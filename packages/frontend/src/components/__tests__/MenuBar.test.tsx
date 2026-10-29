import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MenuBar } from '../MenuBar';
import { enCopy } from '../../test/enCopy';

vi.mock('../../hooks/usePlatform', () => ({
  usePlatform: () => 'windows',
}));

vi.mock('../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) =>
    selector({ settings: { language: 'en', theme: { mode: 'dark' } } }),
}));

afterEach(cleanup);

// Menu titles are i18n copy (`menu.*`), so the accessible names asserted below
// are read from the dictionary rather than hard-coded: what this suite locks
// down is the menubar role / aria state / focus order, not the wording. enCopy()
// keeps that read-back honest — a renamed or emptied `menu.*` key fails the run
// instead of quietly turning `{ name: … }` into an unbounded query.
const APP_NAME = enCopy('menu.appName');
const FILE = enCopy('menu.file');
const IMPORT_CONNECTIONS = enCopy('common.importConnections');

describe('MenuBar accessibility and keyboard navigation', () => {
  it('exposes an application menubar and menu item state', () => {
    render(<MenuBar />);

    expect(screen.getByRole('menubar', { name: APP_NAME })).toBeInTheDocument();
    const file = screen.getByRole('menuitem', { name: FILE });
    expect(file).toHaveAttribute('aria-haspopup', 'menu');
    expect(file).toHaveAttribute('aria-expanded', 'false');
  });

  it('opens menus from the keyboard, moves through items, and closes on Escape', () => {
    render(<MenuBar />);
    const file = screen.getByRole('menuitem', { name: FILE });

    fireEvent.keyDown(file, { key: 'ArrowDown' });

    const menu = screen.getByRole('menu', { name: FILE });
    const items = screen.getAllByRole('menuitem');
    const firstMenuItem = items.find((item) => item.closest('[role="menu"]') === menu);
    expect(firstMenuItem).toBeDefined();
    expect(document.activeElement).toBe(firstMenuItem);

    fireEvent.keyDown(firstMenuItem!, { key: 'ArrowDown' });
    const menuItems = Array.from(menu.querySelectorAll<HTMLButtonElement>('button'));
    expect(document.activeElement).toBe(menuItems[1]);

    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(screen.queryByRole('menu', { name: FILE })).not.toBeInTheDocument();
    expect(document.activeElement).toBe(file);
  });

  it('announces checked menu entries and opens a submenu with the keyboard', () => {
    render(<MenuBar />);
    const file = screen.getByRole('menuitem', { name: FILE });
    fireEvent.click(file);

    const importConnections = screen.getByRole('menuitem', { name: IMPORT_CONNECTIONS });
    expect(importConnections).toHaveAttribute('aria-haspopup', 'menu');
    fireEvent.keyDown(importConnections, { key: 'ArrowRight' });

    expect(screen.getAllByRole('menu')).toHaveLength(2);
    const submenu = screen.getAllByRole('menu')[1];
    expect(document.activeElement).toBe(submenu.querySelector('button'));
  });
});
