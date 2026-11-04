/**
 * Wiring proof for the driver side of the `Dialog` close-label localization
 * (track `wave2/dialog-i18n`).
 *
 * `@datazen/ui`'s `Dialog` used to hardcode `closeLabel = 'Close'`. The redis
 * driver is the only driver that renders bare `<Dialog>`s — eight of them, none
 * of which pass a `closeLabel` prop:
 *
 *   value-editors/DraftLeaveDialog.tsx        1  (this file's subject)
 *   key-browser/ImportExport.tsx              1
 *   key-browser/KeyWorkbenchDialogs.tsx       5
 *   observe/SlowlogPanel.tsx                  1
 *
 * The titles they render are already localized (`t('redis.*')`), so before the
 * fix a user running the app in e.g. `ja` saw a localized title next to an
 * English "Close".
 *
 * `DraftLeaveDialog` is the subject here because it is the only one of the eight
 * that mounts with no driver props, props bridges or store bindings — it is
 * driven purely by the `draftGuard` module singleton. The other seven are
 * covered by the same single `Dialog` code path, and
 * `dialogCloseLabelInventory.test.ts` pins that there is no ninth.
 *
 * English `common.close` is `'Close'`, byte-identical to the old hardcoded
 * literal, so an assertion against the shipped English copy proves nothing. The
 * probe below is registered into `en` (later registrations win per key — see
 * `registerTranslations` in `@datazen/ui/i18n.ts`) with wording that exists
 * nowhere else. `setLocale()` is deliberately NOT used: boundary rule R2 in
 * `scripts/check-driver-import-boundaries.mjs` allows it only in host `src/**`
 * and in `i18n.ts` / `i18n.test.tsx`, and driver tests are not allowed to call
 * it.
 *
 * The driver suite's `src/test/driverUiSetup.ts` loads the real host + driver
 * dictionaries, so `common.close` is genuinely present in the registry here —
 * this is the same state the running app is in, just with probe wording.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { registerTranslations, t } from '@datazen/ui';
import { DraftLeaveDialog } from '../value-editors/DraftLeaveDialog';
import { __resetDraftGuard, publishDraftDirty, requestDraftLeave } from '../shared/draftGuard';

const PROBE_CLOSE = 'PROBE::RedisDialogClose';
const LITERAL_DEFAULT = 'Close';

registerTranslations({ en: { 'common.close': PROBE_CLOSE } });

afterEach(() => {
  cleanup();
  __resetDraftGuard();
});

/** The header X is the only button in this dialog carrying an `aria-label`. */
function headerCloseNames(): string[] {
  return screen
    .getAllByRole('button')
    .filter((el) => el.getAttribute('aria-label') !== null)
    .map((el) => el.getAttribute('aria-label') as string);
}

/** Drives the real guard into the dirty ⇒ dialog-open state. */
function openDialog() {
  publishDraftDirty(true);
  render(<DraftLeaveDialog />);
  act(() => {
    // Dirty state makes the request hang and open the dialog, exactly as a
    // key switch in the running app does.
    void requestDraftLeave();
  });
}

describe('[tester] redis driver dialogs inherit the localized close label', () => {
  it('common.close is registered in this suite, and the probe overrides it', () => {
    // Precondition: without it the case below could pass for the wrong reason.
    expect(t('common.close')).toBe(PROBE_CLOSE);
    expect(t('common.close')).not.toBe(LITERAL_DEFAULT);
  });

  it('DraftLeaveDialog renders the registry copy instead of the hardcoded literal', () => {
    openDialog();

    // `role="dialog"` rather than the `testId` prop: `tid()` only emits
    // `data-testid` under `VITE_E2E=1` (see `packages/ui/src/tid.ts`), which no
    // unit suite sets — same locator the host Dialog suite uses.
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: PROBE_CLOSE })).toBeInTheDocument();
    expect(headerCloseNames()).toEqual([PROBE_CLOSE]);
    expect(headerCloseNames()).not.toContain(LITERAL_DEFAULT);
  });

  it('the localized close button still closes the dialog (onClose is wired)', () => {
    openDialog();
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    act(() => {
      screen.getByRole('button', { name: PROBE_CLOSE }).click();
    });

    // The journey does not stop at the label: clicking X must still run the
    // guard's cancel path and unmount the dialog.
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
