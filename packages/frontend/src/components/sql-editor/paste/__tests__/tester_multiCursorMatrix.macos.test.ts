/**
 * [tester] 独立裁定测试 —— macOS 分支（navigator.platform = MacIntel ⇒ Mod ≡ Cmd）。
 *
 * 与 Coder 的 `multipleSelections.macos.test.ts` 并列，互为独立复核。
 * 平台桩的必要性：@codemirror/view 的 `browser.mac` 在**模块求值期**由
 * `navigator.platform` 决定（dist/index.js:83 附近），`currentPlatform`（:9017）
 * 再据此把 `Mod` 展开为 `Meta` 或 `Ctrl`。jsdom 的 navigator.platform 为空。
 * `vi.hoisted` 会被提升到所有 import 之前，因此桩能赶在模块首次求值前生效。
 */
import { describe, it, expect, afterAll, vi } from 'vitest';
import type { Extension } from '@codemirror/state';
import type { EditorView as EditorViewType } from '@codemirror/view';

const platformBackup = vi.hoisted(() => {
  const own = Object.getOwnPropertyDescriptor(navigator, 'platform');
  Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true });
  return own;
});

afterAll(() => {
  if (platformBackup) {
    Object.defineProperty(navigator, 'platform', platformBackup);
  } else {
    Reflect.deleteProperty(navigator, 'platform');
  }
});

const { EditorState } = await import('@codemirror/state');
const { EditorView, keymap } = await import('@codemirror/view');
const { searchKeymap } = await import('@codemirror/search');
const { defaultKeymap, historyKeymap } = await import('@codemirror/commands');
const { createMultipleSelectionsExtension } = await import('../multipleSelections');

const DOC = 'SELECT a\nSELECT b\nSELECT c';
const MOD = { metaKey: true } as const;

function mountView(anchor = 12): { view: EditorViewType; parent: HTMLElement } {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const state = EditorState.create({
    doc: DOC,
    selection: { anchor, head: anchor },
    extensions: [
      keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      createMultipleSelectionsExtension(),
    ] as Extension[],
  });
  return { view: new EditorView({ state, parent }), parent };
}

function press(view: EditorViewType, init: KeyboardEventInit): void {
  view.contentDOM.dispatchEvent(
    new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }),
  );
}

function destroy(view: EditorViewType, parent: HTMLElement): void {
  view.destroy();
  parent.remove();
}

function reachableAlone(key: string, init: KeyboardEventInit): boolean {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const hit: string[] = [];
  const state = EditorState.create({
    doc: 'x',
    extensions: [keymap.of([{ key, run: () => (hit.push(key), true) }])],
  });
  const view = new EditorView({ state, parent });
  try {
    press(view, init);
    return hit.length > 0;
  } finally {
    view.destroy();
    parent.remove();
  }
}

function winnerAmongFour(init: KeyboardEventInit): string | null {
  const variants = ['Mod-d', 'Mod-D', 'Shift-Mod-d', 'Shift-Mod-D'] as const;
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const hit: string[] = [];
  const state = EditorState.create({
    doc: 'x',
    extensions: [keymap.of(variants.map((n) => ({ key: n, run: () => (hit.push(n), true) })))],
  });
  const view = new EditorView({ state, parent });
  try {
    press(view, init);
    return hit[0] ?? null;
  } finally {
    view.destroy();
    parent.remove();
  }
}

describe('[tester] 裁定 1 · Mod-d 族（macOS 分支，Mod ≡ Cmd）', () => {
  it('平台桩生效：Mod 走 Meta，不走 Ctrl', () => {
    expect(reachableAlone('Mod-d', { key: 'd', code: 'KeyD', ...MOD })).toBe(true);
    expect(reachableAlone('Mod-d', { key: 'd', code: 'KeyD', ctrlKey: true })).toBe(false);
    expect(reachableAlone('Cmd-d', { key: 'd', code: 'KeyD', ...MOD })).toBe(true);
  });

  it('Mod-d / Mod-D 在 mac 上都活着（真实浏览器形态，带 keyCode=68）', () => {
    expect(reachableAlone('Mod-d', { key: 'd', code: 'KeyD', keyCode: 68, ...MOD })).toBe(true);
    expect(
      reachableAlone('Mod-D', { key: 'D', code: 'KeyD', keyCode: 68, shiftKey: true, ...MOD }),
    ).toBe(true);
  });

  it('mac 上 Shift-Mod-d 是「被 Mod-D 遮蔽」而非结构性不可达；Shift-Mod-D 才真不可达', () => {
    // w3c-keyname@2.2.8 的 ignoreKey 规则：mac && metaKey && shiftKey && !ctrl && !alt
    // ⇒ 忽略 event.key，改读 shift[event.keyCode]（shift[68]==='D'）。
    // 后果：主查表查 "Meta-D" 命中 Mod-D，带 Shift 前缀的键名只在主查表 miss 后才有机会。
    // ★ 实测：主查表 hit 后回退分支根本不进入，但**单绑定隔离**下 Shift-Mod-d 仍可达
    //   —— 因为主查表 miss（该键单独注册时 "Meta-D" 无对应绑定）时，回退分支
    //   modifiers(base[68]='d', event, true) = "Shift-Meta-d" 正好命中它。
    // 因此正确表述是「在真实四绑定 keymap 中被 Mod-D 遮蔽」，而非「结构上不可达」。
    expect(
      reachableAlone('Shift-Mod-d', {
        key: 'd',
        code: 'KeyD',
        keyCode: 68,
        shiftKey: true,
        ...MOD,
      }),
    ).toBe(true);
    expect(
      reachableAlone('Shift-Mod-d', {
        key: 'đ',
        code: 'KeyD',
        keyCode: 68,
        shiftKey: true,
        ...MOD,
      }),
    ).toBe(true);

    // Shift-Mod-D 在**真实浏览器形态**（keyCode=68）下真正不可达：
    // 回退两步分别拼出 "Shift-Meta-d" 与（不带 Shift 的）"Meta-D"，都拼不出 "Shift-Meta-D"。
    expect(
      reachableAlone('Shift-Mod-D', {
        key: 'D',
        code: 'KeyD',
        keyCode: 68,
        shiftKey: true,
        ...MOD,
      }),
    ).toBe(false);

    // 与非 mac 分支完全一致的例外：keyCode 缺失时 base[0] 为 undefined，回退分支不进入，
    // 第三条 `else if (isChar && event.shiftKey && modifiers(name, event, true))` 直接命中。
    // 这是 jsdom 人工形态，真实浏览器不会产生。
    expect(reachableAlone('Shift-Mod-D', { key: 'D', code: 'KeyD', shiftKey: true, ...MOD })).toBe(
      true,
    );
  });

  it('mac 边界：keyCode 缺失 + 非 US 布局字符 ⇒ 四条全部落空', () => {
    expect(winnerAmongFour({ key: 'đ', code: 'KeyD', shiftKey: true, ...MOD })).toBeNull();
  });

  it('mac 命中顺序矩阵', () => {
    expect(winnerAmongFour({ key: 'd', code: 'KeyD', keyCode: 68, ...MOD })).toBe('Mod-d');
    expect(winnerAmongFour({ key: 'D', code: 'KeyD', keyCode: 68, shiftKey: true, ...MOD })).toBe(
      'Mod-D',
    );
    expect(winnerAmongFour({ key: 'đ', code: 'KeyD', keyCode: 68, shiftKey: true, ...MOD })).toBe(
      'Mod-D',
    );
  });
});

describe('[tester] 裁定 3 · macOS 键位边界', () => {
  it('Cmd+Shift+↑ 让位给 selectDocStart（不抢原生手势）', () => {
    const { view, parent } = mountView(20);
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', metaKey: true, shiftKey: true });
      expect(view.state.selection.main.empty).toBe(false);
      expect(view.state.selection.main.from).toBe(0);
      expect(view.state.selection.main.to).toBe(20);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('Cmd+Shift+↓ 让位给 selectDocEnd', () => {
    const { view, parent } = mountView(4);
    try {
      press(view, { key: 'ArrowDown', code: 'ArrowDown', metaKey: true, shiftKey: true });
      expect(view.state.selection.main.empty).toBe(false);
      expect(view.state.selection.main.from).toBe(4);
      expect(view.state.selection.main.to).toBe(DOC.length);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('四修饰键 Alt-Shift-Mod-ArrowUp/Down（Cmd+Opt+Shift+↑↓）在 mac 上复制行', () => {
    const up = mountView(12);
    try {
      press(up.view, {
        key: 'ArrowUp',
        code: 'ArrowUp',
        altKey: true,
        metaKey: true,
        shiftKey: true,
      });
      expect(up.view.state.doc.toString()).toBe('SELECT a\nSELECT b\nSELECT b\nSELECT c');
    } finally {
      destroy(up.view, up.parent);
    }

    const down = mountView(4);
    try {
      press(down.view, {
        key: 'ArrowDown',
        code: 'ArrowDown',
        altKey: true,
        metaKey: true,
        shiftKey: true,
      });
      expect(down.view.state.doc.toString()).toBe('SELECT a\nSELECT a\nSELECT b\nSELECT c');
    } finally {
      destroy(down.view, down.parent);
    }
  });

  it('四修饰键在 mac 上不与 standardKeymap 的 Ctrl-Shift-Arrow（翻页）冲突', () => {
    // standardKeymap 有 { mac: "Ctrl-ArrowUp", run: cursorPageUp, shift: selectPageUp }，
    // 与 Alt-Shift-Meta-ArrowUp 是不同键名，两者可共存。
    expect(
      reachableAlone('Ctrl-Shift-ArrowUp', {
        key: 'ArrowUp',
        code: 'ArrowUp',
        ctrlKey: true,
        shiftKey: true,
      }),
    ).toBe(true);
    expect(
      reachableAlone('Alt-Shift-Meta-ArrowUp', {
        key: 'ArrowUp',
        code: 'ArrowUp',
        altKey: true,
        metaKey: true,
        shiftKey: true,
      }),
    ).toBe(true);
  });

  it('Option+Shift+↑ 在 mac 上归多光标（不复制行）', () => {
    const { view, parent } = mountView(12);
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, shiftKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('Cmd+D 端到端：连续三次命中三个 occurrence', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const doc = 'SELECT name, age, name FROM users WHERE name = 1';
    const state = EditorState.create({
      doc,
      selection: { anchor: 9, head: 9 },
      extensions: [
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
        createMultipleSelectionsExtension(),
      ] as Extension[],
    });
    const view = new EditorView({ state, parent });
    try {
      press(view, { key: 'd', code: 'KeyD', ...MOD });
      expect(view.state.selection.ranges).toHaveLength(1);
      press(view, { key: 'd', code: 'KeyD', ...MOD });
      expect(view.state.selection.ranges).toHaveLength(2);
      press(view, { key: 'D', code: 'KeyD', shiftKey: true, ...MOD });
      expect(view.state.selection.ranges).toHaveLength(3);
    } finally {
      destroy(view, parent);
    }
  });
});
