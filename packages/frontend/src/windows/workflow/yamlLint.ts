/**
 * Lint for the workflow YAML editor.
 *
 * Currently focuses on the highest-value check for workflow SQL steps:
 * **parameter reference pairing** — every `${param}` must have a closing `}`.
 * A param reference left unclosed (`draft SQL `${id` / `${id}${name`) is the
 * most common silent failure in hand-written workflow YAML, so we surface it
 * as a CodeMirror lint diagnostic at the offending `${`.
 *
 * Conservative by design: a stray `}` with no open `${` is ignored (SQL blocks
 * can legitimately contain `}`, e.g. in string literals or JSON casts), which
 * keeps us from flagging real SQL. Comment lines are skipped.
 */
import { linter, type Diagnostic } from '@codemirror/lint';
import type { EditorView } from '@codemirror/view';

const TOKEN_RE = /\$\{|\}/g;

/** Map a doc offset to its line's start and whether `#` precedes it on the line (comment). */
function lineCommentAhead(doc: string, pos: number): boolean {
  const lineStart = doc.lastIndexOf('\n', pos - 1) + 1;
  for (let i = lineStart; i < pos; i++) {
    if (doc.charCodeAt(i) === 35 /* # */) return true;
  }
  return false;
}

export function paramRefDiagnostics(view: EditorView): Diagnostic[] {
  const doc = view.state.doc.toString();
  const diags: Diagnostic[] = [];
  const openers: number[] = []; // offsets of `${` awaiting a closer

  TOKEN_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TOKEN_RE.exec(doc)) !== null) {
    if (lineCommentAhead(doc, m.index)) continue;
    if (m[0] === '${') {
      openers.push(m.index);
    } else if (openers.length > 0) {
      openers.pop();
    }
    // stray `}` (no open `${`) → ignore (may be legitimate SQL)
  }

  for (const open of openers) {
    const from = Math.min(open, doc.length);
    diags.push({
      from,
      to: Math.max(from + 1, Math.min(open + 2, doc.length)),
      severity: 'error',
      message: 'Unclosed parameter reference — missing "}"',
      source: 'workflow-yaml',
    });
  }
  return diags;
}

/** Lint extension for `@codemirror/lint` in the workflow YAML editor. */
export function yamlParamLint() {
  return linter(paramRefDiagnostics, { delay: 300 });
}
