import { describe, expect, it } from 'vitest';
import { pickAnchorRow, scrollTopForAnchorRow } from '../fkNavigation';

const ROW_HEIGHT = 28;
const VIEWPORT = 375;

describe('pickAnchorRow', () => {
  it('keeps the active cell as the anchor while it is on screen', () => {
    const { anchorRow, anchorVisible } = pickAnchorRow({
      scrollTop: 140,
      rowHeight: ROW_HEIGHT,
      viewportHeight: VIEWPORT,
      activeRow: 6,
    });
    expect(anchorVisible).toBe(true);
    expect(anchorRow).toBe(6);
    expect(scrollTopForAnchorRow(anchorRow, ROW_HEIGHT)).toBe(168);
  });

  it('anchors on the first visible row when the active cell scrolled above the viewport', () => {
    // The grid keeps the active cell when the user scrolls by hand, so row 0 is
    // still "active" while the user is looking at row 4 and below. Restoring to
    // row 0 would throw away the position the jump started from.
    const { anchorRow, anchorVisible } = pickAnchorRow({
      scrollTop: 120,
      rowHeight: ROW_HEIGHT,
      viewportHeight: VIEWPORT,
      activeRow: 0,
    });
    expect(anchorVisible).toBe(false);
    expect(anchorRow).toBe(4);
    expect(scrollTopForAnchorRow(anchorRow, ROW_HEIGHT)).toBe(112);
  });

  it('anchors on the first visible row when the active cell is below the viewport', () => {
    const { anchorRow, anchorVisible } = pickAnchorRow({
      scrollTop: 0,
      rowHeight: ROW_HEIGHT,
      viewportHeight: VIEWPORT,
      activeRow: 40,
    });
    expect(anchorVisible).toBe(false);
    expect(anchorRow).toBe(0);
  });

  it('falls back to the first visible row when there is no active cell', () => {
    const { anchorRow, anchorVisible } = pickAnchorRow({
      scrollTop: 300,
      rowHeight: ROW_HEIGHT,
      viewportHeight: VIEWPORT,
      activeRow: null,
    });
    expect(anchorVisible).toBe(false);
    expect(anchorRow).toBe(10);
  });

  it('treats a zero row height or viewport as one pixel rather than dividing by zero', () => {
    const { anchorRow } = pickAnchorRow({
      scrollTop: 50,
      rowHeight: 0,
      viewportHeight: 0,
      activeRow: null,
    });
    expect(anchorRow).toBe(50);
  });
});
