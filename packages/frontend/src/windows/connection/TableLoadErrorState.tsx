/**
 * The full-surface error state of a table that has no columns yet, extracted from
 * `TableView.tsx` (DB-03 pushed that file over its size ceiling). Markup moved verbatim.
 *
 * It is its own component because the condition it answers — "the first load failed, there
 * is nothing to keep on screen" — is distinct from the banner that sits above a table whose
 * *reload* failed and whose chrome must stay usable.
 */
import { CopyableError } from '../../components/ui/CopyableError';

export function TableLoadErrorState({
  error,
  retryLabel,
  onRetry,
}: {
  error: string;
  retryLabel: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="max-w-lg text-center">
        <CopyableError message={error} copyButton className="text-sm text-red-400" />
        <button
          type="button"
          className="mt-2 text-xs text-accent hover:underline"
          onClick={onRetry}
        >
          {retryLabel}
        </button>
      </div>
    </div>
  );
}
