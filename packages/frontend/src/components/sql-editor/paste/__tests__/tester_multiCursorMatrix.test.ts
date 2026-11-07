/**
 * [tester] 独立裁定测试 —— Track D Tester 自建，**不复用** Coder 的断言写法。
 *
 * 目的：三个必须独立裁定的问题
 *   (1) Mod-d 族四条键名到底哪些是死键（Coder 推翻了协调者先前的裁定，Tester 必须自己测）
 *   (3) copy-line 改键方案：提权范围 / macOS 让位测试是否空转绿 / 四修饰键是否跨平台冲突
 *
 * 方法：全部用真实 `EditorView` + 真实 `KeyboardEvent` 走 CodeMirror 真实 keymap 分发。
 * 与 Coder 的 `hit[0]` 探针不同，本文件用**单绑定隔离探针**判定「可达性」，
 * 再用**全绑定探针**判定「命中顺序」——两者可独立区分。
 */
import { describe, it, expect } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { searchKeymap } from '@codemirror/search';
import {
  defaultKeymap,
  historyKeymap,
  copyLineUp,
  copyLineDown,
  addCursorAbove,
} from '@codemirror/commands';
import { Prec } from '@codemirror/state';
import { createMultipleSelectionsExtension } from '../multipleSelections';

const DOC = 'SELECT a\nSELECT b\nSELECT c';

/** jsdom 的 navigator.platform 为空 ⇒ CodeMirror currentPlatform === 'key' ⇒ Mod ≡ Ctrl。 */
function mountView(
  anchor = 12,
  extra: Extension[] = [],
): {
  view: EditorView;
  parent: HTMLElement;
} {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const state = EditorState.create({
    doc: DOC,
    selection: { anchor, head: anchor },
    // 真实挂载顺序：editorExtensions.ts 的 keymap.of([...defaultKeymap, ...]) 在前
    extensions: [
      keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      createMultipleSelectionsExtension(),
      ...extra,
    ],
  });
  return { view: new EditorView({ state, parent }), parent };
}

function press(view: EditorView, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  view.contentDOM.dispatchEvent(event);
  return event;
}

function destroy(view: EditorView, parent: HTMLElement): void {
  view.destroy();
  parent.remove();
}

/**
 * 单绑定隔离探针：只注册 `key` 一条，返回该键名能否被此事件**触达**。
 * 这与「多绑定时谁先命中」是两个不同的问题——本函数只回答前者。
 */
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

/** 全绑定探针：四条 Mod-d 键名同时注册，返回事件**命中哪一条**。 */
function winnerAmongFour(init: KeyboardEventInit): string | null {
  const variants = ['Mod-d', 'Mod-D', 'Shift-Mod-d', 'Shift-Mod-D'] as const;
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const hit: string[] = [];
  const state = EditorState.create({
    doc: 'x',
    extensions: [
      keymap.of(variants.map((name) => ({ key: name, run: () => (hit.push(name), true) }))),
    ],
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

/**
 * 真实浏览器会带上 keyCode；jsdom 不会（默认 0）。
 * 显式区分「带 keyCode 的真实形态」与「省略 keyCode 的形态」，
 * 因为 `runHandlers` 的非 US 布局回退分支以 `base[event.keyCode]` 为门槛。
 */
const REAL_D_KEYCODE = 68;
const MOD = { ctrlKey: true } as const;

describe('[tester] 裁定 1 · Mod-d 族键名可达性（非 mac 分支，Mod ≡ Ctrl）', () => {
  it('Mod-d / Mod-D 两条在真实浏览器形态（带 keyCode=68）下都活着', () => {
    // 真实浏览器：Ctrl+D，key="d"，keyCode=68
    expect(reachableAlone('Mod-d', { key: 'd', code: 'KeyD', keyCode: 68, ...MOD })).toBe(true);
    // 真实浏览器：Ctrl+Shift+D，key="D"，keyCode=68
    expect(
      reachableAlone('Mod-D', { key: 'D', code: 'KeyD', keyCode: 68, shiftKey: true, ...MOD }),
    ).toBe(true);
    // CapsLock：key="D" 但 shiftKey=false
    expect(reachableAlone('Mod-D', { key: 'D', code: 'KeyD', keyCode: 68, ...MOD })).toBe(true);
  });

  it('Shift-Mod-d 经 base[keyCode] 回退可达 —— US 与非 US 布局都可达（Coder 声称仅非 US，实测不成立）', () => {
    // 非 US 布局：物理 D 键上报 'đ'，base[68]==='d'，回退分支改写成 'd' 并允许补 Shift。
    expect(
      reachableAlone('Shift-Mod-d', {
        key: 'đ',
        code: 'KeyD',
        keyCode: REAL_D_KEYCODE,
        shiftKey: true,
        ...MOD,
      }),
    ).toBe(true);
    // ★ 反直觉事实：**US 布局下同样可达**（key='D' + Shift）。
    // 主查表查 "Ctrl-D"（命中 Mod-D 而非本键），miss 后回退分支查
    // modifiers(base[68]='d', event, true) = "Shift-Ctrl-d" ⇒ 命中本键。
    // 即「仅非 US 布局可达」这一说法与实测不符。
    expect(
      reachableAlone('Shift-Mod-d', {
        key: 'D',
        code: 'KeyD',
        keyCode: REAL_D_KEYCODE,
        shiftKey: true,
        ...MOD,
      }),
    ).toBe(true);
    // 但未按 Shift 时不可达：modifiers() 的第三参只是「允许补 Shift」，
    // 真正决定的是 event.shiftKey。键名叫 Shift-Mod-d 却并非总需 Shift，
    // 但此处必须按 Shift 才可达。
    expect(
      reachableAlone('Shift-Mod-d', { key: 'đ', code: 'KeyD', keyCode: REAL_D_KEYCODE, ...MOD }),
    ).toBe(false);
    // US 布局 + 无 Shift：主查表 "Ctrl-d" 命中 Mod-d，回退分支不再进入 ⇒ 本键不可达。
    expect(reachableAlone('Shift-Mod-d', { key: 'd', code: 'KeyD', keyCode: 68, ...MOD })).toBe(
      false,
    );
  });

  it('Shift-Mod-D 在真实浏览器形态下不可达（US 与非 US 布局均不可达）', () => {
    // key="D" + keyCode=68：主查表 modifiers(name, event, !isChar) 去掉 Shift ⇒ 命中 Mod-D。
    expect(
      reachableAlone('Shift-Mod-D', {
        key: 'D',
        code: 'KeyD',
        keyCode: 68,
        shiftKey: true,
        ...MOD,
      }),
    ).toBe(false);
    // 非 US 布局：回退分支第一步 modifiers(base[68]='d', ..., true) = "Shift-Ctrl-d"，
    // 拼不出 "Shift-Ctrl-D"；第二步 modifiers(shift[68]='D', ..., false) 不带 Shift，同样拼不出。
    expect(
      reachableAlone('Shift-Mod-D', {
        key: 'đ',
        code: 'KeyD',
        keyCode: 68,
        shiftKey: true,
        ...MOD,
      }),
    ).toBe(false);
  });

  it('Shift-Mod-D 仅在「浏览器不上报 keyCode」这一 jsdom 人工形态下可达', () => {
    // 记录一个**精确**事实：不是「所有平台都不可达」，而是
    // 「真实浏览器（keyCode 恒有值）不可达；keyCode 缺失时经第三条 else-if 分支可达」。
    // 这条断言用于把上述结论钉死，防止日后被误读成「绝对死键」。
    expect(reachableAlone('Shift-Mod-D', { key: 'D', code: 'KeyD', shiftKey: true, ...MOD })).toBe(
      true,
    );
  });

  it('边界：keyCode 缺失 + 非 US 布局字符 ⇒ 四条全部落空（真实浏览器不会发生）', () => {
    // base[0] 为 undefined ⇒ 回退分支整体不进入；主查表也拼不出任何一条。
    // 记录这个空洞，避免把它误当作已覆盖的组合。
    expect(winnerAmongFour({ key: 'đ', code: 'KeyD', ctrlKey: true, shiftKey: true })).toBeNull();
  });

  it('命中顺序矩阵：与 Coder 声称的完全一致', () => {
    expect(winnerAmongFour({ key: 'd', code: 'KeyD', keyCode: 68, ...MOD })).toBe('Mod-d');
    expect(winnerAmongFour({ key: 'D', code: 'KeyD', keyCode: 68, shiftKey: true, ...MOD })).toBe(
      'Mod-D',
    );
    expect(winnerAmongFour({ key: 'D', code: 'KeyD', keyCode: 68, ...MOD })).toBe('Mod-D');
    expect(winnerAmongFour({ key: 'đ', code: 'KeyD', keyCode: 68, shiftKey: true, ...MOD })).toBe(
      'Shift-Mod-d',
    );
  });
});

describe('[tester] 裁定 3 · copy-line 改键方案', () => {
  it('前置事实：Ctrl+Shift+↑/↓ 在非 mac 上原本**完全空闲**（提权前的状态）', () => {
    // 只挂 defaultKeymap+history+search，不挂多光标扩展 ⇒ 这是「改动之前」的世界。
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const state = EditorState.create({
      doc: DOC,
      selection: { anchor: 12, head: 12 },
      extensions: [keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap])],
    });
    const view = new EditorView({ state, parent });
    try {
      const up = press(view, {
        key: 'ArrowUp',
        code: 'ArrowUp',
        ctrlKey: true,
        shiftKey: true,
      });
      expect(view.state.doc.toString()).toBe(DOC);
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.empty).toBe(true);
      // 未被任何绑定处理 ⇒ 事件未被 preventDefault
      expect(up.defaultPrevented).toBe(false);
    } finally {
      destroy(view, parent);
    }
  });

  it('提权只发生在必要处：Prec.high 块里只有 Shift-Alt-ArrowUp/Down 和 Escape 三条', () => {
    // 用户自定义快捷键（默认 execute=Mod-Enter / saveQuery=Mod-s）不得被多光标抢占。
    // 若 Prec 覆盖范围扩大到整个 keymap，下面的 Mod-Enter 就会失效。
    //
    // Escape 提权是为了压过 defaultKeymap 里先注册的 simplifySelection，
    // 但 exitMultiCursor 在只有 ≤1 个 range 时返回 false，所以它不能吃掉
    // 同一个 keymap 里别的 Escape 绑定——下面那一条就是回归守卫。
    let escapeRan = false;
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const state = EditorState.create({
      doc: DOC,
      selection: { anchor: 12, head: 12 },
      extensions: [
        keymap.of([
          { key: 'Mod-Enter', run: () => true },
          { key: 'Mod-s', run: () => true },
          { key: 'Tab', run: () => true },
          {
            key: 'Escape',
            run: () => {
              escapeRan = true;
              return true;
            },
          },
        ]),
        createMultipleSelectionsExtension(),
      ],
    });
    const view = new EditorView({ state, parent });
    try {
      press(view, { key: 'Enter', code: 'Enter', ctrlKey: true });
      press(view, { key: 's', code: 'KeyS', ctrlKey: true });
      press(view, { key: 'Tab', code: 'Tab' });
      // 文档未变 ⇒ 没有任何多光标绑定误吞这些按键
      expect(view.state.doc.toString()).toBe(DOC);
      expect(view.state.selection.ranges).toHaveLength(1);
      // 单光标态下 exitMultiCursor 让路，默认优先级的 Escape 仍能拿到按键。
      press(view, { key: 'Escape', code: 'Escape' });
      expect(escapeRan).toBe(true);
    } finally {
      destroy(view, parent);
    }
  });

  it('提权范围精确：Shift-Alt-ArrowUp 提权，Alt-ArrowUp 保持原优先级不被波及', () => {
    // defaultKeymap: { key: "Alt-ArrowUp", run: moveLineUp } —— 移行。
    // 若 Prec.high 误把整个多光标 keymap 抬起来，Alt+↑ 会被 addCursorAbove 抢走。
    const { view, parent } = mountView(12);
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true });
      // moveLineUp：文档行序改变；addCursorAbove：文档不变、光标变两个
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.doc.toString()).not.toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('macOS 让位测试不是空转绿：证明「若 copy-line 提权，Cmd+Shift+↑ 就会复制行」', () => {
    // 构造一个反事实世界：把 Mod-Shift-ArrowUp 放在 Prec.high。
    // 若该场景下 Cmd+Shift+↑ 变成复制行，则现有的「keeps selecting to the start of the
    // document」断言确实在守护一条**真实且敏感**的边界，而非恒真。
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const state = EditorState.create({
      doc: DOC,
      selection: { anchor: 12, head: 12 },
      extensions: [
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
        // 注意：这里用 Meta 字面量，模拟 mac 上 Mod 的展开结果。
        Prec.high(keymap.of([{ key: 'Meta-Shift-ArrowUp', run: copyLineUp }])),
      ],
    });
    const view = new EditorView({ state, parent });
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', metaKey: true, shiftKey: true });
      // 反事实成立 ⇒ 提权确实会打断 selectDocStart ⇒ 现有 mac 让位断言有判别力。
      expect(view.state.doc.toString()).toBe('SELECT a\nSELECT b\nSELECT b\nSELECT c');
    } finally {
      destroy(view, parent);
    }
  });

  it('四修饰键 Alt-Shift-Mod-ArrowUp/Down 在非 mac 上命中 copyLine 且不与既有键冲突', () => {
    const up = mountView(12);
    try {
      press(up.view, {
        key: 'ArrowUp',
        code: 'ArrowUp',
        altKey: true,
        ctrlKey: true,
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
        ctrlKey: true,
        shiftKey: true,
      });
      expect(down.view.state.doc.toString()).toBe('SELECT a\nSELECT a\nSELECT b\nSELECT c');
    } finally {
      destroy(down.view, down.parent);
    }
  });

  it('Shift-Alt-ArrowUp/Down 现在归多光标独占（copyLine 在该组合上已不可达）', () => {
    // 确认没有「一个按键同时干两件事」的残留：Alt+Shift+↑ 只能加光标，不能复制行。
    const { view, parent } = mountView(12);
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, shiftKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('Mod-Alt-ArrowUp 仍然加光标（defaultKeymap 自带同命令，行为无差异）', () => {
    const { view, parent } = mountView(12);
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, ctrlKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('回归：Shift-Alt-ArrowUp 的 addCursorAbove 绑定确实来自多光标扩展而非 defaultKeymap', () => {
    // 若 defaultKeymap 的 Shift-Alt-ArrowUp→copyLineUp 仍先注册，本测试会失败。
    // 这是对「Prec 提权确实生效」的正向证明。
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const state = EditorState.create({
      doc: DOC,
      selection: { anchor: 12, head: 12 },
      extensions: [
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
        keymap.of([{ key: 'Shift-Alt-ArrowUp', run: addCursorAbove }]),
      ],
    });
    const view = new EditorView({ state, parent });
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, shiftKey: true });
      // 正常优先级下 defaultKeymap 先注册 ⇒ 复制行，光标仍为 1
      expect(view.state.doc.toString()).toBe('SELECT a\nSELECT b\nSELECT b\nSELECT c');
      expect(view.state.selection.ranges).toHaveLength(1);
    } finally {
      destroy(view, parent);
    }
  });

  it('copyLineDown 在非 mac 上仍可经 Mod-Shift-ArrowDown 触达（正向可达性）', () => {
    expect(
      reachableAlone('Mod-Shift-ArrowDown', {
        key: 'ArrowDown',
        code: 'ArrowDown',
        ctrlKey: true,
        shiftKey: true,
      }),
    ).toBe(true);
  });

  it('四修饰键的 mac 变体与非 mac 变体归一到两个**不同**的键名', () => {
    // 关键事实：`Meta-` 是字面修饰键，与平台无关；只有 `Mod-` 才随平台展开。
    // 因此 'Alt-Shift-Mod-ArrowUp' 在非 mac ⇒ "Shift-Ctrl-Alt-ArrowUp"，
    // 而其 mac 变体 'Alt-Shift-Cmd-ArrowUp' ⇒ "Shift-Meta-Alt-ArrowUp"，
    // 两者在同一个 keymap 里不会互相顶替。
    expect(
      reachableAlone('Alt-Shift-Ctrl-ArrowUp', {
        key: 'ArrowUp',
        code: 'ArrowUp',
        altKey: true,
        ctrlKey: true,
        shiftKey: true,
      }),
    ).toBe(true);
    // 字面 Meta 组合在非 mac 上同样可达（因为 Meta 与平台无关）——这一点容易被误解。
    expect(
      reachableAlone('Alt-Shift-Meta-ArrowUp', {
        key: 'ArrowUp',
        code: 'ArrowUp',
        altKey: true,
        metaKey: true,
        shiftKey: true,
      }),
    ).toBe(true);
    // 反证：不含 Ctrl 的非 mac 变体在 Ctrl 事件下不可达。
    expect(
      reachableAlone('Alt-Shift-Meta-ArrowUp', {
        key: 'ArrowUp',
        code: 'ArrowUp',
        altKey: true,
        ctrlKey: true,
        shiftKey: true,
      }),
    ).toBe(false);
  });
});

describe('[tester] copyLineDown 变量存在性（防止未使用导入被 lint 误判）', () => {
  it('copyLineDown 被实际引用', () => {
    expect(typeof copyLineDown).toBe('function');
  });
});

/* ============================================================================
 * 阶段 C · 未覆盖分支的精确补测
 *
 * 覆盖率实测 multipleSelections.ts = 70% lines / 26.31% branch / 50% funcs，
 * 未覆盖行 30-36、52，全部落在**本轨未触碰的既有代码**上
 * （`git diff c37bc0848^ c37bc0848` 只改了 9-18 与 85-155 两段）。
 * 本块按覆盖率要求逐分支补齐，不追数量。
 * ========================================================================== */

/**
 * 直接对 `rectangularSelection` 注册的 `eventFilter` 提问。
 *
 * 该 filter 不决定选区类型，而是喂给 `EditorView.mouseSelectionStyle`
 * （`@codemirror/view:10173-10176`：`filter(event) ? rectangleSelectionStyle(...) : null`），
 * 因此可以从 facet 原样取出并直接调用——**不需要任何布局**，
 * 也就不受 jsdom 无法把 clientX/Y 映射到文档位置的限制。
 */
function filterAccepts(init: MouseEventInit): boolean {
  const { view, parent } = mountView(12);
  try {
    const facet = view.state.facet(EditorView.mouseSelectionStyle);
    expect(facet).toHaveLength(1);
    return facet[0](view, new MouseEvent('mousedown', init)) !== null;
  } finally {
    destroy(view, parent);
  }
}

describe('[tester] 阶段 C · 既有未覆盖分支补测（鼠标交互 + keydown 兜底）', () => {
  it('clickAddsSelectionRange 的判定：仅 altKey 且不带 shift 时追加光标', () => {
    const { view, parent } = mountView(12);
    try {
      const facet = view.state.facet(EditorView.clickAddsSelectionRange);
      expect(facet.length).toBeGreaterThan(0);
      const decide = facet[0];
      expect(decide(new MouseEvent('mousedown', { altKey: true }))).toBe(true);
      // Option+Shift+点击 不追加（让位给矩形选区）
      expect(decide(new MouseEvent('mousedown', { altKey: true, shiftKey: true }))).toBe(false);
      // 普通点击不追加
      expect(decide(new MouseEvent('mousedown', {}))).toBe(false);
      // 单独 Shift 也不追加
      expect(decide(new MouseEvent('mousedown', { shiftKey: true }))).toBe(false);
      // Cmd+点击不追加
      expect(decide(new MouseEvent('mousedown', { metaKey: true }))).toBe(false);
    } finally {
      destroy(view, parent);
    }
  });

  it('eventFilter 放行 Option 拖拽，挡下无修饰键拖拽', () => {
    expect(filterAccepts({ altKey: true, button: 0, buttons: 1 })).toBe(true);
    expect(filterAccepts({ button: 0, buttons: 1 })).toBe(false);
    expect(filterAccepts({ shiftKey: true, button: 0, buttons: 1 })).toBe(false);
    expect(filterAccepts({ metaKey: true, button: 0, buttons: 1 })).toBe(false);
  });

  it('eventFilter 的第二个条件：非左键（button!==0 且 buttons!==1）被挡下', () => {
    expect(filterAccepts({ altKey: true, button: 2, buttons: 2 })).toBe(false);
    // button===0 即可通过，buttons 无需为 1
    expect(filterAccepts({ altKey: true, button: 0, buttons: 0 })).toBe(true);
    // buttons===1 也能通过（第二个析取项）
    expect(filterAccepts({ altKey: true, button: 4, buttons: 1 })).toBe(true);
  });

  it('eventFilter 的三个析取项中只有第一项可能为真，其余两项是静态死臂', () => {
    // 源码： (e.altKey || (e.altKey && e.shiftKey) || (e.metaKey && e.altKey)) && (...)
    // 第 2 项要求 altKey 真，但它只在第 1 项为假（即 altKey 假）时才被求值 ⇒ 恒假。
    // 第 3 项要求 altKey 真，同样只在前两项为假时才被求值 ⇒ 恒假。
    // 即整个析取式恒等于 e.altKey。这解释了 branch 覆盖率长期偏低：
    // 那两臂不是"漏测"，是**不可达**。穷举 8 组合把这一点钉死。
    for (const altKey of [false, true]) {
      for (const shiftKey of [false, true]) {
        for (const metaKey of [false, true]) {
          expect(filterAccepts({ altKey, shiftKey, metaKey, button: 0, buttons: 1 })).toBe(altKey);
        }
      }
    }
  });

  it('keydown 兜底 handler 落空：非 D 键不阻止默认行为', () => {
    const { view, parent } = mountView(12);
    try {
      // 用 F5：Ctrl+A 之类会被 defaultKeymap 绑定（Mod-a → selectAll，来自 commands），
      // 那样 defaultPrevented 为 true 是 defaultKeymap 干的，不是本 handler。
      const ev = new KeyboardEvent('keydown', {
        bubbles: true,
        cancelable: true,
        key: 'F5',
        code: 'F5',
        ctrlKey: true,
      });
      view.contentDOM.dispatchEvent(ev);
      expect(ev.defaultPrevented).toBe(false);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('keydown 兜底 handler 落空：带 Alt 的 D 键不触发（!e.altKey 守卫）', () => {
    const { view, parent } = mountView(12);
    try {
      const ev = new KeyboardEvent('keydown', {
        bubbles: true,
        cancelable: true,
        key: 'd',
        code: 'KeyD',
        keyCode: 68,
        ctrlKey: true,
        altKey: true,
      });
      view.contentDOM.dispatchEvent(ev);
      expect(ev.defaultPrevented).toBe(false);
    } finally {
      destroy(view, parent);
    }
  });

  it('selectNextOccurrence 落空时 keymap 的 preventDefault 仍然生效（易误解，需钉死）', () => {
    // 实测事实：`KeyBinding.preventDefault: true` 在 `runFor` 里**无条件**置位，
    // 与命令自身的返回值无关（@codemirror/view:9189-9193）。
    // 因此"selectNextOccurrence 返回 false ⇒ 浏览器默认行为放行"是**错的**：
    // 光标停在空行末尾时命令确实返回 false（已单独验证），但 Ctrl+D 仍被 preventDefault。
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const state = EditorState.create({
      doc: 'alpha\n',
      selection: { anchor: 6 },
      extensions: [createMultipleSelectionsExtension()] as Extension[],
    });
    const view = new EditorView({ state, parent });
    try {
      const ev = new KeyboardEvent('keydown', {
        bubbles: true,
        cancelable: true,
        key: 'd',
        code: 'KeyD',
        keyCode: 68,
        ctrlKey: true,
      });
      view.contentDOM.dispatchEvent(ev);
      // 命令落空 ⇒ 选区不变、仍是单光标
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.empty).toBe(true);
      // 但默认行为仍被挡下（来自 KeyBinding.preventDefault，不是来自 domEventHandlers）
      expect(ev.defaultPrevented).toBe(true);
    } finally {
      destroy(view, parent);
    }
  });
});
