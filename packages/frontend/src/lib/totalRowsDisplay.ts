import type { CountStrategy } from '@datazen/backend-client';

/**
 * The three shapes a row count can take in the interface.
 *
 * "Exact" and "estimated" carry a number; "unknown" does not, because there
 * is no number to carry. The estimate is a separate shape on purpose: an
 * estimate must never be rendered like a fact.
 */
export type TotalRowsView =
  | { kind: 'exact'; value: number }
  | { kind: 'estimated'; value: number }
  | { kind: 'unknown' };

/**
 * Normalise a (kind, value) pair into the view the interface renders.
 *
 * A missing kind means "exact": a backend that does not know about the three
 * states counted exactly, and its count was exact. The value is only read
 * for the two shapes that carry one — the unknown view has no number, and
 * reading one would put a number on the screen that nobody has.
 */
export function totalRowsView(
  kind: CountStrategy | undefined | null,
  totalRows: number,
): TotalRowsView {
  if (kind === 'estimated') return { kind: 'estimated', value: totalRows };
  if (kind === 'unsupported') return { kind: 'unknown' };
  return { kind: 'exact', value: totalRows };
}

/**
 * The compact form of a row count: 1234 → '1,234', 1_200_000 → '1.2M'.
 *
 * The tilde is **not** part of the number: the caller adds it, so a test can
 * assert the number and the "about" symbol separately — and so an estimate
 * can never be rendered in the same shape as a fact.
 */
export function formatRowCount(value: number): string {
  if (!Number.isFinite(value)) return '0';
  const count = Math.max(0, Math.floor(value));
  if (count < 1_000_000) return count.toLocaleString('en-US');
  if (count < 1_000_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  return `${(count / 1_000_000_000).toFixed(1)}B`;
}

/**
 * Whether the grid may offer a next page.
 *
 * The unknown view has no total, and the only honest evidence that more rows
 * may follow is a **full** page: a half-filled page is the end of the data as
 * far as anybody knows. The other two views carry a number — an estimate is a
 * bound and may be too small, so it never closes the grid, while an exact
 * total that is not bigger than the rows already shown means the data ends
 * here.
 */
export function canPageForward(
  view: TotalRowsView,
  rowCount: number,
  pageSize: number,
): boolean {
  if (view.kind === 'unknown') {
    return pageSize > 0 && rowCount >= pageSize;
  }
  if (view.kind === 'estimated') {
    return true;
  }
  return view.value > rowCount;
}
