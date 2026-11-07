import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { WebContextMenuHost } from '../WebContextMenu';
import { showWebContextMenu, useContextMenuStore } from '../../../stores/contextMenuStore';
import type { NativeMenuItemDef } from '../../../lib/nativeContextMenu';

/**
 * [tester] e2e-ops-menu-BUG-001 复测回归：closeAnyMenu 关闭派发机理。
 *
 * 被验证的 e2e 契约（e2e/specs/* 的 closeAnyMenu）：
 *  - document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
 *    → 事件冒泡到 WebContextMenu 的 window mousedown 监听器，
 *      e.target 为 Node（body）→ contains(body) === false → hide() 执行 → 菜单关闭；
 *  - window 派发（round-0 写法，BUG-001 根因）→ e.target === window 非 Node →
 *    rootRef.contains(window) 按 WebIDL 抛 TypeError → hide() 永不执行。
 */
const ONE_ITEM: NativeMenuItemDef[] = [
  { kind: 'item', id: 'a', label: 'A', action: () => undefined },
];

function submenuItems() {
  return [
    {
      kind: 'submenu' as const,
      id: 'more',
      label: 'More',
      items: [{ kind: 'item' as const, id: 'nested', label: 'Nested', action: () => undefined }],
    },
  ];
}

describe('[tester] closeAnyMenu close-dispatch contract (e2e-ops-menu-BUG-001)', () => {
  afterEach(() => {
    useContextMenuStore.getState().hide();
    cleanup();
  });

  it('test_tester_body_dispatch_closes_root_menu (e2e closeAnyMenu contract)', async () => {
    render(<WebContextMenuHost />);
    showWebContextMenu(ONE_ITEM, { x: 10, y: 10 });
    await screen.findByTestId('web-context-menu');

    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));

    await waitFor(() => expect(screen.queryByTestId('web-context-menu')).toBeNull(), {
      timeout: 500,
    });
  });

  it('test_tester_body_dispatch_closes_menu_with_open_submenu', async () => {
    render(<WebContextMenuHost />);
    showWebContextMenu(submenuItems(), { x: 10, y: 10 });
    await screen.findByTestId('web-context-menu');
    fireEvent.focus(screen.getByTestId('web-context-submenu-trigger-more'));
    await screen.findByTestId('web-context-submenu');

    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));

    await waitFor(() => expect(screen.queryByTestId('web-context-menu')).toBeNull(), {
      timeout: 500,
    });
    expect(screen.queryByTestId('web-context-submenu')).toBeNull();
  });

  it('test_tester_body_dispatch_without_open_menu_is_a_noop', async () => {
    render(<WebContextMenuHost />);
    expect(() =>
      document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true })),
    ).not.toThrow();
    expect(screen.queryByTestId('web-context-menu')).toBeNull();
  });

  it('test_tester_window_dispatch_throws_TypeError_and_does_not_close (BUG-001 mechanism)', async () => {
    // round-0 / BUG-001 的窗口派发：e.target === window 非 Node，
    // onDown 的 rootRef.contains(window) 抛 WebIDL TypeError → hide() 不执行。
    // 捕获 window error 事件取证并 preventDefault，避免其污染 vitest run。
    const captured: ErrorEvent[] = [];
    const onWindowError = (e: Event) => {
      captured.push(e as ErrorEvent);
      e.preventDefault();
    };
    window.addEventListener('error', onWindowError);
    try {
      render(<WebContextMenuHost />);
      showWebContextMenu(ONE_ITEM, { x: 10, y: 10 });
      const menu = await screen.findByTestId('web-context-menu');
      expect(menu).toBeTruthy();

      window.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));

      // hide() 未执行 → 菜单仍在 DOM（BUG-001 的可观测后果）
      expect(screen.queryByTestId('web-context-menu')).not.toBeNull();
      // 且 onDown 确实抛了 WebIDL TypeError（BUG-001 机理取证）。
      // 注意 jsdom realm 的 error 与测试 realm 跨 realm，不用 instanceof，
      // 按 name + WebIDL 消息断言。
      expect(captured.length).toBeGreaterThan(0);
      expect(captured[0]?.error?.name).toBe('TypeError');
      expect(String(captured[0]?.error)).toContain("not of type 'Node'");
    } finally {
      window.removeEventListener('error', onWindowError);
      useContextMenuStore.getState().hide();
    }
  });

  it('test_tester_direct_contains_window_throws_WebIDL_TypeError', async () => {
    render(<WebContextMenuHost />);
    showWebContextMenu(ONE_ITEM, { x: 10, y: 10 });
    const menu = await screen.findByTestId('web-context-menu');
    // 直接复现 onDown 第 175 行的表达式：contains 非 Node 参数按 WebIDL 抛错
    // （跨 realm，按消息断言而非 instanceof TypeError）
    expect(() => menu.contains(window as unknown as Node)).toThrow(
      /parameter 1 is not of type 'Node'/,
    );
    cleanup();
  });
});
