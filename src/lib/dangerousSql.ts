import { scanSql } from '../components/sql-editor/semantic/scanner';
import { SqlTokenKind } from '../components/sql-editor/semantic/tokens';
import type { SqlToken } from '../components/sql-editor/semantic/types';

/**
 * SQL risk classifier.
 *
 * Classifies each top-level statement into `read` / `mutation` / `unknown` and
 * produces high-risk findings (DROP / TRUNCATE / UPDATE||DELETE without a
 * top-level WHERE). Statement boundaries and lexical regions come from the
 * shared S2-A scanner so the frontend assessment and the Rust Host guard agree
 * on the same token stream (no duplicated lexer).
 */

export type SqlRiskClassification = 'read' | 'mutation' | 'unknown';

export type SqlRiskFindingType = 'drop' | 'truncate' | 'no-where';

export type SqlRiskFinding = {
  type: SqlRiskFindingType;
  /** Absolute offset range of the verb that triggered the finding. */
  from: number;
  to: number;
  statementIndex: number;
  verb: string;
};

export type SqlStatementRisk = {
  index: number;
  /** Main verb (uppercased), or null when the statement is not classifiable. */
  verb: string | null;
  classification: SqlRiskClassification;
  findings: SqlRiskFinding[];
  /**
   * True when a block comment (slash-star ... star-slash) contains a write verb.
   * This mirrors the Host guard's comment-hides-write-verb interception.
   */
  commentHidesWriteVerb: boolean;
  /** True when a top-level (depth 0) WHERE is present in this statement. */
  hasTopLevelWhere: boolean;
  /**
   * True when the verb is a strict write verb (the Host hard-blocks it under
   * readOnly/safeMode). SELECT INTO / EXPLAIN ANALYZE are "may write" but are
   * NOT strict write verbs, matching the Host guard.
   */
  strictWrite: boolean;
  /** Content range of the statement (surrounding whitespace trimmed). */
  range: { from: number; to: number };
};

export type SqlRiskAssessment = {
  statements: SqlStatementRisk[];
  /** Aggregate: mutation > unknown > read. */
  classification: SqlRiskClassification;
  findings: SqlRiskFinding[];
  hasHighRisk: boolean;
};

/** Main-clause verbs used to skip a leading `WITH` CTE (mirrors permission.rs). */
const MAIN_VERBS = new Set([
  'SELECT',
  'INSERT',
  'UPDATE',
  'DELETE',
  'MERGE',
  'UPSERT',
  'REPLACE',
  'CALL',
  'EXEC',
  'EXECUTE',
  'DO',
  'SHOW',
  'DESCRIBE',
  'DESC',
  'EXPLAIN',
  'SET',
  'USE',
  'BEGIN',
  'COMMIT',
  'ROLLBACK',
  'SAVEPOINT',
  'RELEASE',
  'PREPARE',
  'DEALLOCATE',
  'ANALYZE',
  'VACUUM',
  'COPY',
  'LOAD',
  'UNLOAD',
  'HANDLER',
  'OPTIMIZE',
  'REPAIR',
  'CHECKSUM',
  'CHECK',
  'FLUSH',
  'RESET',
  'KILL',
  'SHUTDOWN',
  'CREATE',
  'ALTER',
  'DROP',
  'TRUNCATE',
  'GRANT',
  'REVOKE',
  'RENAME',
  'COMMENT',
  'LOCK',
  'UNLOCK',
  'PURGE',
]);

/** Verbs the Host guard treats as mutating (mirrors safety.rs WRITE_VERBS). */
const WRITE_VERBS = new Set([
  'INSERT',
  'UPDATE',
  'DELETE',
  'MERGE',
  'UPSERT',
  'REPLACE',
  'CREATE',
  'ALTER',
  'DROP',
  'TRUNCATE',
  'GRANT',
  'REVOKE',
  'RENAME',
  'COPY',
  'LOAD',
  'UNLOAD',
  'CALL',
  'EXEC',
  'EXECUTE',
  'DO',
  'HANDLER',
  'OPTIMIZE',
  'REPAIR',
  'FLUSH',
  'RESET',
  'KILL',
  'SHUTDOWN',
  'PURGE',
  'VACUUM',
  'LOCK',
  'UNLOCK',
  'COMMENT',
]);

/** Verbs the Host guard treats as read-only (mirrors safety.rs READ_VERBS). */
const READ_VERBS = new Set([
  'SELECT',
  'SHOW',
  'DESCRIBE',
  'DESC',
  'EXPLAIN',
  'PRAGMA',
  'USE',
  'WITH',
  'VALUES',
]);

const DESTRUCTIVE_VERBS = new Set(['DROP', 'TRUNCATE']);
const WHERE_NEEDS_VERBS = new Set(['UPDATE', 'DELETE']);

type SigTok = {
  kind: 'open' | 'close' | 'word';
  word?: string;
  from: number;
  to: number;
  depth: number;
};

/** Map fullwidth Latin (U+FF01–U+FF5E) to ASCII so ＤＲＯＰ → DROP. */
function normalizeFullwidth(sql: string): string {
  let out = '';
  for (const ch of sql) {
    const cp = ch.charCodeAt(0);
    out += cp >= 0xff01 && cp <= 0xff5e ? String.fromCharCode(cp - 0xfee0) : ch;
  }
  return out;
}

function splitStatements(
  source: string,
  tokens: readonly SqlToken[],
): { range: { from: number; to: number }; rawFrom: number; rawTo: number }[] {
  const raw: { from: number; to: number }[] = [];
  let start = 0;
  for (const tok of tokens) {
    if (tok.kind === SqlTokenKind.Semicolon && tok.parenDepth === 0) {
      raw.push({ from: start, to: tok.from });
      start = tok.to;
    }
  }
  raw.push({ from: start, to: source.length });

  const out = [];
  for (const r of raw) {
    let from = r.from;
    let to = r.to;
    while (from < to && /\s/.test(source[from])) from += 1;
    while (to > from && /\s/.test(source[to - 1])) to -= 1;
    if (from < to) {
      out.push({ range: { from, to }, rawFrom: r.from, rawTo: r.to });
    }
  }
  return out;
}

function buildSig(tokens: SqlToken[]): SigTok[] {
  const sig: SigTok[] = [];
  for (const tok of tokens) {
    if (tok.kind === SqlTokenKind.OpenParen) {
      sig.push({ kind: 'open', from: tok.from, to: tok.to, depth: tok.parenDepth });
    } else if (tok.kind === SqlTokenKind.CloseParen) {
      sig.push({ kind: 'close', from: tok.from, to: tok.to, depth: tok.parenDepth });
    } else if (tok.kind === SqlTokenKind.Other && /^[A-Za-z0-9_$]+$/.test(tok.text)) {
      sig.push({
        kind: 'word',
        word: tok.text.toUpperCase(),
        from: tok.from,
        to: tok.to,
        depth: tok.parenDepth,
      });
    }
  }
  return sig;
}

function isMainVerb(word: string): boolean {
  return MAIN_VERBS.has(word);
}

function skipWithClause(sig: SigTok[]): number | null {
  let depth = 0;
  for (let i = 1; i < sig.length; i += 1) {
    const tok = sig[i];
    if (tok.kind === 'open') depth += 1;
    else if (tok.kind === 'close') depth = Math.max(0, depth - 1);
    else if (depth === 0 && isMainVerb(tok.word as string)) return i;
  }
  return null;
}

function extractMainVerb(sig: SigTok[]): SigTok | null {
  if (sig.length === 0) return null;
  let idx = 0;
  if (sig[0].kind === 'word' && sig[0].word === 'WITH') {
    const skipped = skipWithClause(sig);
    if (skipped == null) return null;
    idx = skipped;
  }
  const tok = sig[idx];
  return tok.kind === 'word' ? tok : null;
}

function hasTopLevelWhere(tokens: SqlToken[]): boolean {
  for (const tok of tokens) {
    if (
      tok.kind === SqlTokenKind.Other &&
      tok.parenDepth === 0 &&
      /^[A-Za-z0-9_]+$/.test(tok.text) &&
      tok.text.toUpperCase() === 'WHERE'
    ) {
      return true;
    }
  }
  return false;
}

function commentContainsWriteVerb(tokens: SqlToken[]): boolean {
  for (const tok of tokens) {
    if (tok.kind !== SqlTokenKind.BlockComment) continue;
    const inner = tok.text.slice(2, -2);
    for (const word of inner.split(/\s+/)) {
      if (word && WRITE_VERBS.has(word.toUpperCase())) return true;
    }
  }
  return false;
}

/** Detect "may write" statements the Host does not hard-block (SELECT INTO / EXPLAIN ANALYZE). */
function isMayWrite(verb: string, sig: SigTok[]): boolean {
  if (verb === 'SELECT') {
    return sig.some((t) => t.kind === 'word' && t.word === 'INTO' && t.depth === 0);
  }
  if (verb === 'EXPLAIN') {
    for (let i = 0; i < sig.length; i += 1) {
      if (sig[i].kind === 'word' && sig[i].word === 'EXPLAIN') {
        const next = sig[i + 1];
        if (next && next.kind === 'word' && next.word === 'ANALYZE') return true;
      }
    }
  }
  return false;
}

function classifyVerb(
  verb: string | null,
  strictWrite: boolean,
  mayWrite: boolean,
): SqlRiskClassification {
  if (strictWrite || mayWrite) return 'mutation';
  if (verb && READ_VERBS.has(verb)) return 'read';
  return 'unknown';
}

function buildFindings(
  verb: string | null,
  verbTok: SigTok | null,
  hasWhere: boolean,
  statementIndex: number,
): SqlRiskFinding[] {
  if (!verb || !verbTok) return [];
  const findings: SqlRiskFinding[] = [];
  if (verb && DESTRUCTIVE_VERBS.has(verb)) {
    findings.push({
      type: verb === 'DROP' ? 'drop' : 'truncate',
      from: verbTok.from,
      to: verbTok.to,
      statementIndex,
      verb,
    });
  } else if (WHERE_NEEDS_VERBS.has(verb) && !hasWhere) {
    findings.push({ type: 'no-where', from: verbTok.from, to: verbTok.to, statementIndex, verb });
  }
  return findings;
}

function aggregateClassification(statements: SqlStatementRisk[]): SqlRiskClassification {
  if (statements.some((s) => s.classification === 'mutation')) return 'mutation';
  if (statements.some((s) => s.classification === 'unknown')) return 'unknown';
  return 'read';
}

/** Classify every statement in a multi-statement SQL string, aggregating all findings. */
export function classifyRisk(sql: string): SqlRiskAssessment {
  const normalized = normalizeFullwidth(sql);
  const { tokens } = scanSql(normalized, { includeOther: true, includeText: true });
  const statements = splitStatements(normalized, tokens).map((stmt, index) => {
    const stmtTokens = tokens.filter((t) => t.from >= stmt.rawFrom && t.to <= stmt.rawTo);
    const sig = buildSig(stmtTokens);
    const verbTok = extractMainVerb(sig);
    const verb = verbTok && verbTok.word != null ? verbTok.word : null;
    const strictWrite = verb ? WRITE_VERBS.has(verb) : false;
    const hasWhere = hasTopLevelWhere(stmtTokens);
    const commentHidesWriteVerb = commentContainsWriteVerb(stmtTokens);
    const mayWrite = verb ? isMayWrite(verb, sig) : false;
    const classification = classifyVerb(verb, strictWrite, mayWrite);
    const findings = buildFindings(verb, verbTok, hasWhere, index);
    return {
      index,
      verb,
      classification,
      findings,
      commentHidesWriteVerb,
      hasTopLevelWhere: hasWhere,
      strictWrite,
      range: stmt.range,
    };
  });
  const findings = statements.flatMap((s) => s.findings);
  return {
    statements,
    classification: aggregateClassification(statements),
    findings,
    hasHighRisk: findings.length > 0,
  };
}

/**
 * True when a statement's main verb is DROP or TRUNCATE (back-compat helper).
 * Behaviour is unchanged from the original heuristic but now shares the scanner
 * and the classifier's drop/truncate findings.
 */
export function isDangerousWriteStatement(stmt: string): boolean {
  return classifyRisk(stmt).statements.some((s) =>
    s.findings.some((f) => f.type === 'drop' || f.type === 'truncate'),
  );
}

/** True when any statement in the script is DROP or TRUNCATE (back-compat helper). */
export function sqlContainsDangerousWrite(sql: string): boolean {
  return isDangerousWriteStatement(sql);
}
