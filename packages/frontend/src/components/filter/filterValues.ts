/**
 * The value shapes a filter condition carries.
 *
 * A condition's value is one shape per operator, but older drafts (and the
 * back end's comma separated compatibility form) carry the same information
 * in a string. The panel reads both forms, so the parsing lives in **one**
 * place: the editor and the summary must never disagree about what a value
 * says.
 */
import type { Value } from '../../types';

/** The value of a set operator (`in` / `notIn`) as a list of strings. */
export function toTokens(value: Value | undefined): string[] {
  if (Array.isArray(value)) return value.map((entry) => (entry == null ? '' : String(entry)));
  if (typeof value === 'string' && value !== '') {
    return value
      .split(',')
      .map((part) => part.trim())
      .filter((part) => part !== '');
  }
  return [];
}

/** The value of a range operator (`between` / `notBetween`) as its two bounds. */
export function toPair(value: Value | undefined): [string, string] {
  if (Array.isArray(value)) {
    const [lower = '', upper = ''] = value;
    return [lower == null ? '' : String(lower), upper == null ? '' : String(upper)];
  }
  if (typeof value === 'string' && value.includes(',')) {
    const separator = value.indexOf(',');
    return [value.slice(0, separator).trim(), value.slice(separator + 1).trim()];
  }
  return [value == null ? '' : String(value), ''];
}
