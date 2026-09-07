/** Lexical token kinds produced by the unified SQL scanner. */
export enum SqlTokenKind {
  Whitespace = 'whitespace',
  /** Statement delimiter outside literals/comments. */
  Semicolon = 'semicolon',
  LineComment = 'line_comment',
  BlockComment = 'block_comment',
  SingleQuoted = 'single_quoted',
  DoubleQuoted = 'double_quoted',
  BacktickQuoted = 'backtick_quoted',
  BracketQuoted = 'bracket_quoted',
  DollarQuoted = 'dollar_quoted',
  OpenParen = 'open_paren',
  CloseParen = 'close_paren',
  /** Any other contiguous run (identifiers, operators, numbers, etc.). */
  Other = 'other',
}

export function isCommentKind(kind: SqlTokenKind): boolean {
  return kind === SqlTokenKind.LineComment || kind === SqlTokenKind.BlockComment;
}

export function isStringKind(kind: SqlTokenKind): boolean {
  return (
    kind === SqlTokenKind.SingleQuoted ||
    kind === SqlTokenKind.DoubleQuoted ||
    kind === SqlTokenKind.BacktickQuoted ||
    kind === SqlTokenKind.BracketQuoted ||
    kind === SqlTokenKind.DollarQuoted
  );
}

export function isQuoteOrCommentKind(kind: SqlTokenKind): boolean {
  return isCommentKind(kind) || isStringKind(kind);
}
