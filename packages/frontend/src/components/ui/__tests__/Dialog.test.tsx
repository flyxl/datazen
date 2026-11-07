import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { getLocale, setLocale } from '@datazen/ui';
import { registerLocale, unregisterLocale } from '../../../locales';
import { Dialog } from '../Dialog';
import { enCopy } from '../../../test/enCopy';

// This suite renders the *host* wrapper (`src/components/ui/Dialog.tsx`), which
// injects `t('common.close')` into `@datazen/ui`'s `closeLabel`. The accessible
// name of the close button is therefore i18n copy, so the locators below read it
// back from the same dictionary through enCopy() instead of pinning `'Close'`
// (原则六 第 3 类锚点; the "library default, exempt" reading in the first
// revision of this track was wrong — see redis-assert-policy BUG-001).

afterEach(cleanup);

describe('Dialog', () => {
  it('renders title and children when open', () => {
    render(
      <Dialog open title="Test Dialog" onClose={() => {}}>
        <p>Dialog body</p>
      </Dialog>,
    );
    expect(screen.getByText('Test Dialog')).toBeInTheDocument();
    expect(screen.getByText('Dialog body')).toBeInTheDocument();
  });

  it('does not render when closed', () => {
    const { container } = render(
      <Dialog open={false} title="Hidden" onClose={() => {}}>
        <p>Hidden body</p>
      </Dialog>,
    );
    expect(container.innerHTML).toBe('');
  });

  it('does not close when clicking the backdrop', () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Backdrop test" onClose={onClose}>
        <p>Content</p>
      </Dialog>,
    );
    const backdrop = document.querySelector('[aria-hidden="true"]');
    expect(backdrop).toBeTruthy();
    fireEvent.click(backdrop!);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes when the header close button is clicked', () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Close button" onClose={onClose}>
        <p>Content</p>
      </Dialog>,
    );
    fireEvent.click(screen.getByRole('button', { name: enCopy('common.close') }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('closes on Escape', () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Escape test" onClose={onClose}>
        <p>Content</p>
      </Dialog>,
    );
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('focuses the dialog on open and restores the opener on close', () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <>
        <button type="button">Open</button>
        <Dialog open={false} title="Focus test" onClose={onClose}>
          <input aria-label="Dialog input" />
        </Dialog>
      </>,
    );
    const opener = screen.getByRole('button', { name: 'Open' });
    opener.focus();
    rerender(
      <>
        <button type="button">Open</button>
        <Dialog open title="Focus test" onClose={onClose}>
          <input aria-label="Dialog input" />
        </Dialog>
      </>,
    );
    expect(screen.getByRole('button', { name: enCopy('common.close') })).toHaveFocus();
    rerender(
      <>
        <button type="button">Open</button>
        <Dialog open={false} title="Focus test" onClose={onClose}>
          <input aria-label="Dialog input" />
        </Dialog>
      </>,
    );
    expect(screen.getByRole('button', { name: 'Open' })).toHaveFocus();
  });

  it('wraps Tab focus within the dialog', () => {
    render(
      <Dialog open title="Tab test" onClose={() => {}} footer={<button type="button">Last</button>}>
        <p>Content</p>
      </Dialog>,
    );
    const dialog = screen.getByRole('dialog');
    const closeButton = screen.getByRole('button', { name: enCopy('common.close') });
    screen.getByRole('button', { name: 'Last' }).focus();
    fireEvent.keyDown(dialog, { key: 'Tab' });
    expect(closeButton).toHaveFocus();
  });

  it('takes the close button label from i18n, not from the library default', () => {
    // The two locators above read `common.close` back from the dictionary, which
    // is copy-change-proof but *not* wiring-proof: the shipped en wording and the
    // pre-i18n `closeLabel = 'Close'` literal that `@datazen/ui`'s `Dialog` still
    // falls back to for an unregistered `common.close` are the same string, so
    // dropping the wrapper's `t()` injection would leave them green.
    // This case closes that hole with a wording only this test owns (原则六 第 2
    // 类：测试自造数据): if the host wrapper ever stopped feeding
    // `t('common.close')` into `closeLabel`, the probe name never renders.
    const PROBE_LOCALE = 'zz-assert-probe';
    const PROBE_LABEL = 'Zqx dialog close probe';
    registerLocale(PROBE_LOCALE, 'Assert probe', { 'common.close': PROBE_LABEL });
    const previous = getLocale();
    setLocale(PROBE_LOCALE);
    try {
      render(
        <Dialog open title="Probe label" onClose={() => {}}>
          <p>Content</p>
        </Dialog>,
      );
      expect(screen.getByRole('button', { name: PROBE_LABEL })).toBeInTheDocument();
    } finally {
      setLocale(previous);
      unregisterLocale(PROBE_LOCALE);
    }
  });
});
