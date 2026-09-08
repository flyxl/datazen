/** Precomputed newline offsets for O(log n) line lookup. */
export type LineIndex = readonly number[];

export function buildLineIndex(source: string): LineIndex {
  const starts: number[] = [0];
  for (let i = 0; i < source.length; i += 1) {
    if (source.charCodeAt(i) === 10) starts.push(i + 1);
  }
  return starts;
}

export function lineAtOffset(lineStarts: LineIndex, offset: number): number {
  let lo = 0;
  let hi = lineStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (lineStarts[mid]! <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
}

export function isWhitespaceCode(code: number): boolean {
  return code === 32 || code === 9 || code === 10 || code === 13 || code === 12;
}
