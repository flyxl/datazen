/**
 * Lightweight YAML syntax highlighting for the workflow YAML editor.
 *
 * Implemented as a CodeMirror 6 `StreamLanguage` (token names map straight to
 * `@lezer/highlight` tags, so `syntaxHighlighting(HighlightStyle.define(...))`
 * colors them) — no extra dependency required beyond what the app already ships.
 *
 * Scope is pragmatic: comments, mapping keys, quoted/plain scalars, numbers,
 * booleans/null, flow/list punctuation, anchors, and `${param}` interpolation.
 */
import { HighlightStyle, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { tags } from '@lezer/highlight';

interface YamlState {
  /** A mapping `key:` just ended on this line → trailing text is a value. */
  inValue: boolean;
}

const yamlParser = {
  startState: (): YamlState => ({ inValue: false }),
  copyState: (s: YamlState): YamlState => ({ ...s }),
  blankLine: () => {},
  token(this: unknown, stream: import('@codemirror/language').StringStream, state: YamlState) {
    stream.eatWhile(/[ \t]/);
    const ch = stream.peek();
    if (ch == null || ch === '\n') return null;
    if (stream.sol()) state.inValue = false; // values are per-line in simple mappings

    // comment
    if (ch === '#') {
      stream.skipToEnd();
      return 'comment';
    }

    // block scalar introducer (| or >) — rest belongs to a literal/flow scalar
    if (ch === '|' || ch === '>') {
      stream.next();
      state.inValue = true;
      return 'operator';
    }

    // flow structure / list markers
    if ('[]{} ,-'.includes(ch)) {
      stream.next();
      state.inValue = false;
      return 'operator';
    }

    // anchor / alias
    if (ch === '&' || ch === '*') {
      stream.next();
      stream.eatWhile(/[A-Za-z0-9_-]/);
      return 'keyword';
    }

    // quoted string (+ possible quoted key)
    if (ch === '"' || ch === "'") {
      const quote = ch as '"' | "'";
      stream.next();
      while (!stream.eol()) {
        const c = stream.next();
        if (c === quote) break;
        if (quote === '"' && c === '\\') stream.next();
      }
      if (stream.peek() === ':') {
        stream.next();
        state.inValue = true;
        return 'propertyName';
      }
      return 'string';
    }

    // ${param} interpolation (SQL parameter references)
    if (ch === '$') {
      while (!stream.eol()) {
        const c = stream.next();
        if (c === '}') break;
      }
      return 'keyword';
    }

    // plain identifier: key or scalar
    if (/[A-Za-z0-9_./-]/.test(ch)) {
      stream.eatWhile(/[A-Za-z0-9_./-]/);
      if (stream.peek() === ':') {
        stream.next();
        state.inValue = true;
        return 'propertyName';
      }
      // trailing plain scalar on this line (stop at inline comment)
      stream.eatWhile(/[^#\n]/);
      const text = stream.current().trim();
      if (!text) return null;
      if (/^(true|false|null|yes|no|on|off|~)$/i.test(text)) return 'atom';
      if (/^[+-]?\d+(\.\d+)?([eE][+-]?\d+)?$/.test(text)) return 'number';
      return state.inValue ? 'string' : 'variableName';
    }

    stream.next();
    state.inValue = false;
    return null;
  },
};

// Colors resolve from the app's `--cm-*` tokens (:root = light, .dark = dark),
// so syntax highlighting follows the active theme dynamically.
const yamlHighlight = HighlightStyle.define([
  { tag: tags.comment, color: 'var(--cm-comment)', fontStyle: 'italic' },
  { tag: tags.propertyName, color: 'var(--cm-keyword)' },
  { tag: tags.string, color: 'var(--cm-string)' },
  { tag: tags.number, color: 'var(--cm-number)' },
  { tag: tags.atom, color: 'var(--cm-operator)' },
  { tag: tags.keyword, color: 'var(--cm-punctuation)' },
  { tag: tags.variableName, color: 'var(--cm-foreground)' },
  { tag: tags.operator, color: 'var(--cm-punctuation)' },
]);

/** YAML language support for the workflow editor: syntax highlighting. */
export function langYaml(): Extension {
  return [StreamLanguage.define(yamlParser), syntaxHighlighting(yamlHighlight)];
}
