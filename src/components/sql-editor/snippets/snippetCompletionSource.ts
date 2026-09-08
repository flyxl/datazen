/**
 * Snippet completion source (§4.1).
 *
 * Coexists with the keyword / schema / function sources rather than replacing
 * them: relevance is expressed through `boost` (soft ordering) so snippets never
 * starve table or column suggestions. The single hard exclusion is a
 * dot-qualified position (`alias.`), where a snippet prefix is syntactically
 * impossible.
 */
import {
  snippetCompletion,
  type Completion,
  type CompletionSource,
} from '@codemirror/autocomplete';
import { inferSqlCompletionKind } from '../../../lib/sqlCompletionContext';
import { BUILTIN_SQL_SNIPPETS } from './builtinSnippets';
import type { SqlSnippetItem } from './types';

/** Snippet prefixes may contain `*` (e.g. `sel*`), so `\w` alone is not enough. */
const SNIPPET_PREFIX = /[A-Za-z_][A-Za-z0-9_*]*$/;

/** Statement-level snippets are the point of the feature; keep them visible. */
const BOOST_STATEMENT_CONTEXT = 5;
/** Inside a table or column position the user wants identifiers, not templates. */
const BOOST_IDENTIFIER_CONTEXT = -6;

export interface SnippetCompletionOptions {
  snippets?: readonly SqlSnippetItem[];
  /** Resolves `descriptionKey` to display text; defaults to echoing the key. */
  t?: (key: string) => string;
}

/** Build the `Completion[]` projection of a snippet library. */
export function buildSnippetCompletions(
  options: SnippetCompletionOptions = {},
): Array<{ item: SqlSnippetItem; completion: Completion }> {
  const snippets = options.snippets ?? BUILTIN_SQL_SNIPPETS;
  const translate = options.t ?? ((key: string) => key);
  return snippets.map((item) => ({
    item,
    completion: snippetCompletion(item.template, {
      label: item.prefix,
      detail: translate(item.descriptionKey),
      type: 'text',
    }),
  }));
}

export function createSnippetCompletionSource(
  options: SnippetCompletionOptions = {},
): CompletionSource {
  const entries = buildSnippetCompletions(options);

  return (context) => {
    const textBefore = context.state.sliceDoc(0, context.pos);

    // A snippet prefix can never follow a qualifier dot — the only 100%
    // mutually exclusive case, so hard-exclude it here.
    if (/\.\s*[\w$"'`]*$/.test(textBefore)) return null;

    const match = context.matchBefore(SNIPPET_PREFIX);
    if (!match && !context.explicit) return null;

    const kind = inferSqlCompletionKind(textBefore);
    const boost = kind === 'any' ? BOOST_STATEMENT_CONTEXT : BOOST_IDENTIFIER_CONTEXT;

    return {
      from: match?.from ?? context.pos,
      options: entries.map(({ completion }) => ({ ...completion, boost })),
      // `*` must stay in the pattern or typing `sel*` invalidates the list.
      validFor: /^[A-Za-z_][A-Za-z0-9_*]*$/,
    };
  };
}
