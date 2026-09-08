import { isKeyword } from './utils';
import type { SqlToken } from '../types';

export class TokenCursor {
  readonly tokens: readonly SqlToken[];
  pos = 0;

  constructor(tokens: readonly SqlToken[]) {
    this.tokens = tokens;
  }

  peek(offset = 0): SqlToken | undefined {
    return this.tokens[this.pos + offset];
  }

  advance(): SqlToken | undefined {
    const token = this.tokens[this.pos];
    if (token) this.pos += 1;
    return token;
  }

  atEnd(): boolean {
    return this.pos >= this.tokens.length;
  }

  skipWhile(predicate: (token: SqlToken) => boolean): void {
    while (!this.atEnd() && predicate(this.peek()!)) {
      this.advance();
    }
  }

  matchKeyword(keyword: string): boolean {
    const token = this.peek();
    if (!token || !isKeyword(token, keyword)) return false;
    this.advance();
    return true;
  }
}
