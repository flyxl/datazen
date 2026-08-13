import type {
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from '@codemirror/autocomplete';

/** Placeholder completion shown while a lazy namespace path is fetching. */
export function namespaceLoadingCompletionResult(
  loading: boolean,
  from: number,
  label: string,
): CompletionResult | null {
  if (!loading) return null;
  return {
    from,
    filter: false,
    options: [
      {
        label,
        type: 'class',
        boost: 99,
        apply: () => {
          /* do not insert the loading label */
        },
      },
    ],
  };
}

export function shouldShowNamespaceLoadingHint(
  loading: boolean,
  explicit: boolean,
  typedPrefix: boolean,
): boolean {
  if (!loading) return false;
  return explicit || typedPrefix;
}

export function namespaceLoadingCompletionSource(
  loading: boolean,
  label: string,
): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    const typed = context.matchBefore(/[\w."`]+/);
    const typedPrefix = Boolean(typed && typed.from < typed.to);
    if (!shouldShowNamespaceLoadingHint(loading, context.explicit, typedPrefix)) {
      return null;
    }
    const word = context.matchBefore(/[\w."`]*/);
    return namespaceLoadingCompletionResult(true, word?.from ?? context.pos, label);
  };
}
