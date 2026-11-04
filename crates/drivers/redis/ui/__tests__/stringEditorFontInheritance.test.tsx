/**
 * The font-size contract for the Redis string save-error banner.
 *
 * The regression this exists to catch
 * -----------------------------------
 * The `boxed` variant carries `text-xs`, while the hand-written bar this
 * component absorbed carried **no** font-size class at all. Those look like a
 * difference and are not: the banner renders inside `KeyEditors`, whose root is
 *
 *     <div className="space-y-3 text-xs">          // KeyEditors.tsx
 *
 * so the absorbed bar inherited `text-xs` from that ancestor anyway. A
 * previous revision of this call site "fixed" a supposed 16px→12px shrink by
 * adding `text-base`, which silently made the message **33% larger** than base.
 *
 * That mistake is invisible to any test that renders `StringEditor` on its own,
 * because the `text-xs` ancestor is then missing. It is only observable by
 * rendering the **real tree** and walking the actual ancestor chain — so that
 * is what this test does. Reading the stylesheets is not enough: `text-xs` is a
 * class on a DOM node, it never appears in `src/styles/*.css`.
 *
 * If this test ever needs changing, re-derive it by rendering the real
 * `KeyDetailEditor` and printing the ancestor chain — not by reasoning about
 * which stylesheet sets a font size.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

const setString = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeSetString: (...args: unknown[]) => setString(...args),
  invokeGetKeyRaw: vi.fn().mockResolvedValue(null),
  invokeDecodeValue: vi.fn().mockResolvedValue(null),
}));

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);

import type { KeyDetail } from '../shared/types';
import { KeyDetailEditor } from '../value-editors/KeyEditors';

const FONT_SIZE = /(?:^|\s)text-(?:xs|sm|base|lg|xl|[2-9]xl)(?:\s|$)/;

/** Every ancestor that declares a font size, nearest first. */
function fontSizeAncestors(el: Element): string[] {
  const out: string[] = [];
  let node: Element | null = el.parentElement;
  while (node) {
    const cls = node.getAttribute('class') ?? '';
    if (FONT_SIZE.test(cls)) out.push(`${node.tagName.toLowerCase()}: ${cls.trim()}`);
    node = node.parentElement;
  }
  return out;
}

function ownFontSizeClass(el: Element): string | null {
  const m = FONT_SIZE.exec(` ${el.getAttribute('class') ?? ''} `);
  return m ? m[0].trim() : null;
}

async function renderWithSaveError() {
  setString.mockRejectedValue(new Error('nope'));
  const detail: KeyDetail = { key: 'k', keyType: 'string', ttl: -1, value: 'hello' };
  render(
    <KeyDetailEditor
      dbSessionId="s"
      dbIndex={0}
      detail={detail}
      modules={[]}
      onRefresh={() => {}}
    />,
  );
  fireEvent.change(await screen.findByTestId('redis-string-input'), {
    target: { value: 'changed' },
  });
  fireEvent.click(await screen.findByTestId('redis-string-save'));
  return screen.findByTestId('redis-string-save-error');
}

describe('StringEditor save-error banner: inherited font size', () => {
  afterEach(cleanup);

  it('has a text-xs ancestor in the real KeyEditors tree', async () => {
    const banner = await renderWithSaveError();
    // Guards the premise of the next test: if the tree ever stops going
    // through KeyEditors, the conclusion below stops holding.
    expect(fontSizeAncestors(banner)[0]).toBe('div: space-y-3 text-xs');
  });

  it('agrees with the font size it inherits, so its size equals base', async () => {
    const banner = await renderWithSaveError();
    const own = ownFontSizeClass(banner);
    const nearest = fontSizeAncestors(banner)[0];

    expect(own).toBe('text-xs');
    // The invariant, stated generally: an override is only harmless when it
    // agrees with what it overrides. `text-base` (the bug) and any future
    // `text-lg` both fail here even though the ancestor check above still
    // passes, which is why both assertions are needed.
    expect(own).toMatch(FONT_SIZE);
    expect(nearest).toContain(own ?? 'text-xs');
  });
});
