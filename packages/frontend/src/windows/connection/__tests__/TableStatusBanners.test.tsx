/**
 * Permanent guard for the extraction of the panel status banners out of
 * `TableView` (DB-06 track, GC budget).
 *
 * The golden file is the **pre-split** markup: it was captured from a verbatim
 * copy of `TableView` at commit 78197e830 (the temporary probe rendered the old
 * and the new component with the same props and compared the DOM byte for
 * byte — all ten scenarios came out `identical: true`). The probe is deleted;
 * this file is what keeps the conclusion alive.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
// Side effect: registers the eager host dictionaries in the shared i18n engine.
// `CopyableError` lives in @datazen/ui and looks its own label up there.
import '../../../locales';
import { TableStatusBanners } from '../TableStatusBanners';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const GOLDEN = readFileSync(
  resolve(__dirname, '__fixtures__/table-status-banners.golden.html'),
  'utf-8',
);

afterEach(cleanup);

describe('TableStatusBanners', () => {
  it('renders the pre-split banner markup byte for byte', () => {
    const { container } = render(
      <TableStatusBanners
        error="syntax error near filter clause"
        hasWork
        insertNotice={null}
        onRetry={vi.fn()}
      />,
    );

    expect(container.innerHTML).toBe(GOLDEN);
  });

  it('renders nothing when there is no error and no notice', () => {
    const { container } = render(
      <TableStatusBanners error={null} hasWork={false} insertNotice={null} onRetry={vi.fn()} />,
    );

    expect(container.innerHTML).toBe('');
  });

  it('shows the "changes are still staged" hint only when there is staged work', () => {
    const { container: withWork } = render(
      <TableStatusBanners
        error="boom"
        hasWork
        insertNotice={null}
        onRetry={vi.fn()}
      />,
    );
    expect(withWork.textContent).toContain('tableData.errorPendingHint');

    cleanup();

    const { container: withoutWork } = render(
      <TableStatusBanners
        error="boom"
        hasWork={false}
        insertNotice={null}
        onRetry={vi.fn()}
      />,
    );
    expect(withoutWork.textContent).not.toContain('tableData.errorPendingHint');
  });

  it('keeps the insert notice on its own live region', () => {
    const { container } = render(
      <TableStatusBanners
        error={null}
        hasWork={false}
        insertNotice="the new row could not be located"
        onRetry={vi.fn()}
      />,
    );

    const notice = container.querySelector('[data-testid="table-insert-degraded"]');
    expect(notice).not.toBeNull();
    expect(notice?.getAttribute('role')).toBe('status');
    expect(notice?.getAttribute('aria-live')).toBe('polite');
    expect(notice?.textContent).toBe('the new row could not be located');
  });
});
