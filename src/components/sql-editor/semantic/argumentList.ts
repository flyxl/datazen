/**
 * Unified parser for comma-separated argument/value lists inside parentheses.
 * Shared between function signature help (§S4-B) and INSERT inlay hints (§S4-C).
 */

export interface ParenArgumentSlot {
  /** 0-based parameter/column index. */
  index: number;
  /** Document offset where the argument value starts (after '(' or ','). */
  pos: number;
  /** Document offset where the argument ends (before next ',' or ')' or end). */
  endPos: number;
  /** Trimmed text inside this slot if any. */
  text: string;
}

export interface ArgumentSlotsResult {
  slots: readonly ParenArgumentSlot[];
  isClosed: boolean;
  openParenOffset: number;
  closeParenOffset: number;
}

/**
 * Scan argument/value slots inside a parentheses group starting at openParenOffset.
 * Accurately handles:
 * - Single quotes ('...'), double quotes ("..."), backticks (`...`)
 * - Line comments (-- ...) and block comments (/* ... * /)
 * - Nested parentheses ((...))
 * - Incomplete parentheses groups (user currently typing before ')')
 */
export function scanArgumentSlots(
  source: string,
  openParenOffset: number,
  maxOffset?: number,
): ArgumentSlotsResult {
  const len = Math.min(source.length, maxOffset ?? source.length);
  const slots: ParenArgumentSlot[] = [];

  let depth = 0;
  let inSingleQuote = false;
  let inDoubleQuote = false;
  let inBacktick = false;
  let inLineComment = false;
  let inBlockComment = false;

  let currentSlotIndex = 0;
  let slotStart = openParenOffset + 1;
  // Skip leading whitespace for first slot
  while (slotStart < len && /\s/.test(source[slotStart]!)) {
    slotStart++;
  }

  let isClosed = false;
  let closeParenOffset = -1;

  let i = openParenOffset + 1;

  while (i < len) {
    const ch = source[i]!;
    const next = i + 1 < len ? source[i + 1]! : '';

    if (inLineComment) {
      if (ch === '\n') inLineComment = false;
      i++;
      continue;
    }
    if (inBlockComment) {
      if (ch === '*' && next === '/') {
        inBlockComment = false;
        i += 2;
        continue;
      }
      i++;
      continue;
    }

    if (inSingleQuote) {
      if (ch === "'" && next === "'") {
        i += 2;
        continue;
      }
      if (ch === "'") inSingleQuote = false;
      i++;
      continue;
    }
    if (inDoubleQuote) {
      if (ch === '"') inDoubleQuote = false;
      i++;
      continue;
    }
    if (inBacktick) {
      if (ch === '`') inBacktick = false;
      i++;
      continue;
    }

    // Entering quotes/comments
    if (ch === "'") {
      inSingleQuote = true;
      i++;
      continue;
    }
    if (ch === '"') {
      inDoubleQuote = true;
      i++;
      continue;
    }
    if (ch === '`') {
      inBacktick = true;
      i++;
      continue;
    }
    if (ch === '-' && next === '-') {
      inLineComment = true;
      i += 2;
      continue;
    }
    if (ch === '/' && next === '*') {
      inBlockComment = true;
      i += 2;
      continue;
    }

    // Nested parentheses
    if (ch === '(') {
      depth++;
      i++;
      continue;
    }
    if (ch === ')') {
      if (depth > 0) {
        depth--;
        i++;
        continue;
      }
      // Top-level closing paren
      isClosed = true;
      closeParenOffset = i;
      break;
    }

    // Semicolon stops top-level scan
    if (ch === ';' && depth === 0) {
      break;
    }

    // Top-level comma: finishes current slot and starts next slot
    if (ch === ',' && depth === 0) {
      const rawText = source.slice(slotStart, i).trim();
      slots.push({
        index: currentSlotIndex,
        pos: slotStart,
        endPos: i,
        text: rawText,
      });

      currentSlotIndex++;
      let nextSlotStart = i + 1;
      while (nextSlotStart < len && /\s/.test(source[nextSlotStart]!)) {
        nextSlotStart++;
      }
      slotStart = nextSlotStart;
      i++;
      continue;
    }

    i++;
  }

  // Trailing slot after last comma (or only slot if no commas)
  const slotEnd = isClosed ? closeParenOffset : i;
  if (slotStart <= slotEnd || currentSlotIndex === 0) {
    const rawText = source.slice(slotStart, slotEnd).trim();
    slots.push({
      index: currentSlotIndex,
      pos: slotStart,
      endPos: slotEnd,
      text: rawText,
    });
  }

  return {
    slots,
    isClosed,
    openParenOffset,
    closeParenOffset,
  };
}

/**
 * Determine which parameter slot the cursor is currently in.
 * Returns 0-based parameter index, or null if cursor is outside parentheses.
 */
export function getActiveArgumentIndexAtCursor(
  source: string,
  openParenOffset: number,
  cursor: number,
): number | null {
  if (cursor <= openParenOffset) return null;

  const res = scanArgumentSlots(source, openParenOffset);
  if (res.isClosed && cursor > res.closeParenOffset) {
    return null;
  }

  for (const slot of res.slots) {
    if (cursor >= slot.pos && cursor <= slot.endPos) {
      return slot.index;
    }
  }

  // If cursor is beyond the last known slot (e.g. typing at end of unclosed paren)
  if (res.slots.length > 0) {
    const last = res.slots[res.slots.length - 1]!;
    if (cursor >= last.pos) {
      return last.index;
    }
  }

  return 0;
}
