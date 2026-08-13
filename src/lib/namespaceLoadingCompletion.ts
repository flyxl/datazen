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

export function namespaceLoadingCompletionSource(
  loading: boolean,
  label: string,
): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    if (!loading) return null;
    const word = context.matchBefore(/[\w."`]*/);
    return namespaceLoadingCompletionResult(true, word?.from ?? context.pos, label);
  };
}
