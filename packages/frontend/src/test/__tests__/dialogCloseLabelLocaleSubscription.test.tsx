/**
 * `Dialog`'s header close button follows the active locale on *every* render,
 * so an already-open dialog updates its accessible name when the user switches
 * language — with no remount and no prop change.
 *
 * Why this file exists: the subscription (`useI18n()` rather than a bare `t()`
 * in `Dialog.tsx`) is a deliberate behaviour change of the `wave2/dialog-i18n`
 * track, and it is the one part of that change with no rendering surface of its
 * own. A component that merely calls `t()` at render time is byte-identical to
 * one that subscribes until something *else* re-renders it, so what separates
 * the two implementations is a locale switch performed in place, on a dialog
 * that is already mounted and whose props never change.
 *
 * No invented copy: every string this file expects from the *registry* is read
 * back out of a shipped host dictionary (`en` / `zh-CN` / `ja`), so nothing
 * depends on wording invented for an assertion — which is exactly what the
 * English-only suites in this track could not do while `common.close` was the
 * literal `'Close'` in both `en` and the component. The only hand-written literal
 * this file ever compares against a rendered label is the `CUSTOM` sentinel, and
 * that reaches the component as a `closeLabel` argument rather than as a registry
 * read — the opposite case. The precondition case below re-checks that the
 * three copies still differ from each other, so a future copy change that
 * collapses them turns this suite red instead of quietly voiding its own
 * discriminating power.
 *
 * Boundary rule R2 (`scripts/check-driver-import-boundaries.mjs`) scans
 * `packages/**` for `setLocale(` and exempts exactly two paths —
 * `R2_FILE_CARVEOUTS` is a literal `Set` of `packages/ui/src/i18n.ts` and
 * `packages/ui/src/__tests__/i18n.test.tsx`, matched with `.has(rel)` rather than
 * a glob. `Dialog` is a `packages/ui` component and this suite calls
 * `setLocale`, which is why it is a host file.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import {
  Dialog,
  getLocale,
  getRegisteredTranslations,
  registerTranslations,
  setLocale,
} from '@datazen/ui';
// Production path: this module pushes the eager host dictionaries into the
// shared registry on import (`src/locales/index.ts`). Importing the real
// dictionaries is also why the registry already holds `en` / `zh-CN` below.
import '../../locales';
import enEager from '../../locales/en/eager';
import zhCNEager from '../../locales/zh-CN/eager';
import jaDictionary from '../../locales/ja';

const CLOSE_LABEL_KEY = 'common.close';

/** Eagerly loaded on every host boot; the second one needs no registration. */
const EAGER_ALT_LOCALE = 'zh-CN';
/** Optional locale pack: production reaches it through a lazy load. */
const LAZY_LOCALE = 'ja';

const EN_CLOSE = enEager[CLOSE_LABEL_KEY];
const ZH_CLOSE = zhCNEager[CLOSE_LABEL_KEY];
const JA_CLOSE = jaDictionary[CLOSE_LABEL_KEY];

/** The header X is the only button carrying an `aria-label` in these dialogs. */
function headerCloseNames(): string[] {
  return screen
    .getAllByRole('button')
    .filter((el) => el.getAttribute('aria-label') !== null)
    .map((el) => el.getAttribute('aria-label') as string);
}

const ENTRY_LOCALE = getLocale();

afterEach(() => {
  cleanup();
  setLocale(ENTRY_LOCALE);
});

describe('Dialog close label follows the locale of an already-open dialog', () => {
  it('has distinguishable shipped copy in every locale it switches between', () => {
    // The precondition, and the reason this file needs no probe wording. If two
    // of these ever become the same string the switch below stops
    // discriminating, so the suite says so loudly instead of passing quietly.
    expect(getRegisteredTranslations('en')[CLOSE_LABEL_KEY]).toBe(EN_CLOSE);
    expect(getRegisteredTranslations(EAGER_ALT_LOCALE)[CLOSE_LABEL_KEY]).toBe(ZH_CLOSE);
    expect(EN_CLOSE).not.toBe(ZH_CLOSE);
    expect(JA_CLOSE).not.toBe(EN_CLOSE);
    // The third pair. `ja` is reached through the lazy-pack path, so a copy
    // change that collapsed it onto `zh-CN` would leave the lazy case
    // indistinguishable from the eager one and go unnoticed by the two above.
    expect(JA_CLOSE).not.toBe(ZH_CLOSE);
  });

  it('relabels an open dialog in place when the locale changes, without remounting it', () => {
    // The whole contract. Reducing `useI18n()` to a bare `t` leaves the mounted
    // tree untouched by the switch (nothing subscribed), so the name stays the
    // English copy and this fails; so does restoring the pre-i18n hardcoded
    // literal, for the same reason plus one.
    render(
      <Dialog open title="Subscription" onClose={() => {}}>
        <p>body</p>
      </Dialog>,
    );
    const mountedButton = screen.getByRole('button');

    expect(headerCloseNames()).toEqual([EN_CLOSE]);

    act(() => setLocale(EAGER_ALT_LOCALE));

    expect(headerCloseNames()).toEqual([ZH_CLOSE]);
    // Same DOM node, i.e. the update came from a re-render of the mounted
    // dialog rather than from the caller re-rendering it.
    expect(screen.getByRole('button')).toBe(mountedButton);
  });

  it('leaves an explicit closeLabel prop untouched by the same switch', () => {
    // The control: the locale change above is not a blanket "everything on
    // screen changes" effect, it is `closeLabel` resolving through the
    // registry. With the prop supplied, the prop still wins.
    const CUSTOM = 'CUSTOM::NotLocalized';
    render(
      <Dialog open title="Subscription control" closeLabel={CUSTOM} onClose={() => {}}>
        <p>body</p>
      </Dialog>,
    );

    act(() => setLocale(EAGER_ALT_LOCALE));

    expect(headerCloseNames()).toEqual([CUSTOM]);
  });

  it('follows a locale registered the way an optional locale pack is loaded, and back', () => {
    // `ja` is a real shipped dictionary, not a probe: registering it here
    // mirrors `registerTranslations` from a lazily loaded pack, which is the
    // path a driver or extension locale takes. The return trip also rules out a
    // one-way update.
    registerTranslations({ [LAZY_LOCALE]: jaDictionary });
    render(
      <Dialog open title="Lazy locale" onClose={() => {}}>
        <p>body</p>
      </Dialog>,
    );

    act(() => setLocale(LAZY_LOCALE));
    expect(headerCloseNames()).toEqual([JA_CLOSE]);

    act(() => setLocale('en'));
    expect(headerCloseNames()).toEqual([EN_CLOSE]);
  });
});
