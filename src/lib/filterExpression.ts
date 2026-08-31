import type { FilterCondition } from '../types';

/** Values accepted by the controlled filter expression language. */
export type FilterExpressionValue = string | number | boolean | null;

/** Columns can be passed as names or as the existing schema column shape. */
export type FilterExpressionColumn = string | { readonly name: string };

/** The deliberately small operator set supported by the text expression parser. */
export type FilterExpressionOperator =
  | 'eq'
  | 'ne'
  | 'gt'
  | 'gte'
  | 'lt'
  | 'lte'
  | 'isNull'
  | 'isNotNull';

export type FilterExpressionLogic = 'and' | 'or';

export interface FilterConditionExpression {
  type: 'condition';
  column: string;
  operator: FilterExpressionOperator;
  /** `null` is retained for IS NULL/IS NOT NULL so the AST is self-contained. */
  value: FilterExpressionValue;
}

export interface FilterLogicalExpression {
  type: 'logical';
  operator: FilterExpressionLogic;
  left: FilterExpression;
  right: FilterExpression;
}

export type FilterExpression = FilterConditionExpression | FilterLogicalExpression;

export type ParseResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: string; position: number };

export interface ParsedFilterApplication {
  expression: FilterExpression;
  conditions: FilterCondition[];
}

type ComparisonOperator = Extract<
  FilterExpressionOperator,
  'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte'
>;

type Token =
  | { type: 'identifier'; value: string; position: number }
  | { type: 'literal'; value: FilterExpressionValue; position: number }
  | { type: 'operator'; value: ComparisonOperator; position: number }
  | { type: 'keyword'; value: 'AND' | 'OR' | 'IS' | 'NOT'; position: number }
  | { type: 'leftParen' | 'rightParen'; position: number }
  | { type: 'eof'; position: number };

class FilterParseFailure extends Error {
  readonly position: number;

  constructor(message: string, position: number) {
    super(message);
    this.name = 'FilterParseFailure';
    this.position = position;
  }
}

function fail(message: string, position: number): never {
  throw new FilterParseFailure(message, position);
}

function isIdentifierStart(char: string | undefined): boolean {
  return char !== undefined && /[A-Za-z_]/.test(char);
}

function isIdentifierPart(char: string | undefined): boolean {
  return char !== undefined && /[A-Za-z0-9_]/.test(char);
}

function isWhitespace(char: string | undefined): boolean {
  return char !== undefined && /\s/.test(char);
}

function readString(input: string, start: number): { value: string; next: number } {
  let index = start + 1;
  let value = '';

  while (index < input.length) {
    const char = input[index];
    if (char === "'") {
      // SQL-style quote escaping: 'It''s'.
      if (input[index + 1] === "'") {
        value += "'";
        index += 2;
        continue;
      }
      return { value, next: index + 1 };
    }
    if (char === '\\') {
      const escaped = input[index + 1];
      if (escaped === undefined) fail('Unclosed string literal.', start);
      const escapedValue: Record<string, string> = {
        "'": "'",
        '\\': '\\',
        n: '\n',
        r: '\r',
        t: '\t',
      };
      if (!Object.prototype.hasOwnProperty.call(escapedValue, escaped)) {
        fail('Unsupported string escape.', index);
      }
      value += escapedValue[escaped];
      index += 2;
      continue;
    }
    if (char === '\n' || char === '\r') {
      fail('String literals cannot contain an unescaped line break.', index);
    }
    value += char;
    index += 1;
  }

  fail('Unclosed string literal.', start);
}

function readQuotedIdentifier(input: string, start: number): { value: string; next: number } {
  let index = start + 1;
  let value = '';

  while (index < input.length) {
    const char = input[index];
    if (char === '"') {
      // SQL-style quoted identifier escaping: "order""item".
      if (input[index + 1] === '"') {
        value += '"';
        index += 2;
        continue;
      }
      return { value, next: index + 1 };
    }
    if (char === '\n' || char === '\r') {
      fail('Quoted column names cannot contain an unescaped line break.', index);
    }
    value += char;
    index += 1;
  }

  fail('Unclosed quoted column name.', start);
}

function readNumber(input: string, start: number): { value: number; next: number } | null {
  const match = input
    .slice(start)
    .match(/^[+-]?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/);
  if (!match) return null;

  const raw = match[0];
  const value = Number(raw);
  if (!Number.isFinite(value)) fail('Numeric literal is out of range.', start);
  return { value, next: start + raw.length };
}

function tokenize(input: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;

  while (index < input.length) {
    const char = input[index];
    if (isWhitespace(char)) {
      index += 1;
      continue;
    }

    if (char === "'") {
      const string = readString(input, index);
      tokens.push({ type: 'literal', value: string.value, position: index });
      index = string.next;
      continue;
    }

    if (char === '"') {
      const identifier = readQuotedIdentifier(input, index);
      tokens.push({ type: 'identifier', value: identifier.value, position: index });
      index = identifier.next;
      continue;
    }

    if (char === '(') {
      tokens.push({ type: 'leftParen', position: index });
      index += 1;
      continue;
    }
    if (char === ')') {
      tokens.push({ type: 'rightParen', position: index });
      index += 1;
      continue;
    }

    if (char === '=' || char === '!' || char === '>' || char === '<') {
      const next = input[index + 1];
      if (char === '=' && next === '=') fail('Only = is supported for equality.', index);
      if (char === '!' && next !== '=') fail('Only != is supported.', index);
      if (char === '<' && next === '>') fail('Only the supported comparison operators are allowed.', index);
      const operator =
        char === '='
          ? 'eq'
          : char === '!'
            ? 'ne'
            : char === '>'
              ? next === '='
                ? 'gte'
                : 'gt'
              : next === '='
                ? 'lte'
                : 'lt';
      tokens.push({
        type: 'operator',
        value: operator,
        position: index,
      });
      index += next === '=' ? 2 : 1;
      continue;
    }

    if (char === '+' || char === '-' || /\d/.test(char)) {
      const number = readNumber(input, index);
      if (!number) fail('Expected a numeric literal.', index);
      tokens.push({ type: 'literal', value: number.value, position: index });
      index = number.next;
      continue;
    }

    if (isIdentifierStart(char)) {
      const start = index;
      index += 1;
      while (isIdentifierPart(input[index])) index += 1;
      const word = input.slice(start, index);
      const upper = word.toUpperCase();
      if (upper === 'TRUE' || upper === 'FALSE') {
        tokens.push({ type: 'literal', value: upper === 'TRUE', position: start });
      } else if (upper === 'NULL') {
        tokens.push({ type: 'literal', value: null, position: start });
      } else if (upper === 'AND' || upper === 'OR' || upper === 'IS' || upper === 'NOT') {
        tokens.push({ type: 'keyword', value: upper, position: start });
      } else {
        tokens.push({ type: 'identifier', value: word, position: start });
      }
      continue;
    }

    fail('Unsupported token in filter expression.', index);
  }

  tokens.push({ type: 'eof', position: input.length });
  return tokens;
}

class FilterParser {
  private index = 0;

  constructor(
    private readonly tokens: Token[],
    private readonly columns: ReadonlySet<string>,
  ) {}

  parse(): FilterExpression {
    const expression = this.parseOr();
    const token = this.current();
    if (token.type !== 'eof') {
      fail('Unexpected token after the filter expression.', token.position);
    }
    return expression;
  }

  private parseOr(): FilterExpression {
    let expression = this.parseAnd();
    while (this.matchesKeyword('OR')) {
      expression = {
        type: 'logical',
        operator: 'or',
        left: expression,
        right: this.parseAnd(),
      };
    }
    return expression;
  }

  private parseAnd(): FilterExpression {
    let expression = this.parsePrimary();
    while (this.matchesKeyword('AND')) {
      expression = {
        type: 'logical',
        operator: 'and',
        left: expression,
        right: this.parsePrimary(),
      };
    }
    return expression;
  }

  private parsePrimary(): FilterExpression {
    if (this.matches('leftParen')) {
      const expression = this.parseOr();
      this.expect('rightParen', 'Expected closing parenthesis.');
      return expression;
    }
    return this.parseCondition();
  }

  private parseCondition(): FilterConditionExpression {
    const column = this.expect('identifier', 'Expected an allowed column name.');
    if (!this.columns.has(column.value)) {
      fail(`Unknown filter column: ${column.value}.`, column.position);
    }

    const operator = this.parseOperator();
    if (operator === 'isNull' || operator === 'isNotNull') {
      return { type: 'condition', column: column.value, operator, value: null };
    }

    const value = this.current();
    if (value.type !== 'literal') {
      fail('Expected a string, number, boolean, or NULL value.', value.position);
    }
    this.index += 1;
    return { type: 'condition', column: column.value, operator, value: value.value };
  }

  private parseOperator(): FilterExpressionOperator {
    const token = this.current();
    if (token.type === 'operator') {
      this.index += 1;
      return token.value;
    }
    if (this.matchesKeyword('IS')) {
      const not = this.matchesKeyword('NOT');
      this.expectNull('Expected NULL after IS or IS NOT.');
      return not ? 'isNotNull' : 'isNull';
    }
    fail('Expected one of =, !=, >, >=, <, <=, IS NULL, or IS NOT NULL.', token.position);
  }

  private current(): Token {
    return this.tokens[this.index] ?? { type: 'eof', position: 0 };
  }

  private matches<T extends Token['type']>(type: T): boolean {
    if (this.current().type !== type) return false;
    this.index += 1;
    return true;
  }

  private matchesKeyword(keyword: 'AND' | 'OR' | 'IS' | 'NOT'): boolean {
    const token = this.current();
    if (token.type !== 'keyword' || token.value !== keyword) return false;
    this.index += 1;
    return true;
  }

  private expect<T extends Token['type']>(type: T, message: string): Extract<Token, { type: T }> {
    const token = this.current();
    if (token.type !== type) fail(message, token.position);
    this.index += 1;
    return token as Extract<Token, { type: T }>;
  }

  private expectNull(message: string): void {
    const token = this.current();
    if (token.type !== 'literal' || token.value !== null) fail(message, token.position);
    this.index += 1;
  }
}

function columnNames(columns: readonly FilterExpressionColumn[]): ReadonlySet<string> {
  return new Set(columns.map((column) => (typeof column === 'string' ? column : column.name)));
}

/**
 * Parse the intentionally controlled filter language. The result contains no SQL
 * text; all literal values remain data for the caller's parameterised path.
 */
export function parseFilterExpression(
  input: string,
  columns: readonly FilterExpressionColumn[],
): ParseResult<FilterExpression> {
  if (typeof input !== 'string') {
    return { ok: false, error: 'Filter expression must be text.', position: 0 };
  }

  try {
    const expression = new FilterParser(tokenize(input), columnNames(columns)).parse();
    return { ok: true, value: expression };
  } catch (error) {
    if (error instanceof FilterParseFailure) {
      return { ok: false, error: error.message, position: error.position };
    }
    throw error;
  }
}

/**
 * Adapt an AST to the existing structured filter condition API. This deliberately
 * returns conditions only; callers that need mixed AND/OR semantics keep the AST.
 */
export function filterExpressionToConditions(expression: FilterExpression): FilterCondition[] {
  if (expression.type === 'condition') {
    return [
      {
        column: expression.column,
        operator: expression.operator,
        value: expression.value,
      },
    ];
  }
  return [
    ...filterExpressionToConditions(expression.left),
    ...filterExpressionToConditions(expression.right),
  ];
}

/** Parse and prepare the structured payload used by an Apply callback. */
export function parseFilterForApply(
  input: string,
  columns: readonly FilterExpressionColumn[],
): ParseResult<ParsedFilterApplication> {
  const parsed = parseFilterExpression(input, columns);
  if (!parsed.ok) return parsed;
  return {
    ok: true,
    value: {
      expression: parsed.value,
      conditions: filterExpressionToConditions(parsed.value),
    },
  };
}
