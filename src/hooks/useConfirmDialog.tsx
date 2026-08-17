import { useCallback, useState } from 'react';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';

interface ConfirmOptions {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  kind?: 'warning' | 'info';
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

/**
 * Imperative web-based confirm dialog.
 * Returns [confirmFn, DialogElement]. Render `DialogElement` once in your tree.
 */
export function useConfirmDialog(): [ConfirmFn, React.ReactNode] {
  const [state, setState] = useState<{
    options: ConfirmOptions;
    resolve: (value: boolean) => void;
  } | null>(null);

  const confirm: ConfirmFn = useCallback(
    (options) =>
      new Promise<boolean>((resolve) => {
        setState({ options, resolve });
      }),
    [],
  );

  const handleClose = useCallback(
    (result: boolean) => {
      state?.resolve(result);
      setState(null);
    },
    [state],
  );

  const dialog = state ? (
    <ConfirmDialog
      open
      title={state.options.title}
      message={state.options.message}
      confirmLabel={state.options.confirmLabel}
      cancelLabel={state.options.cancelLabel}
      kind={state.options.kind}
      onConfirm={() => handleClose(true)}
      onCancel={() => handleClose(false)}
    />
  ) : null;

  return [confirm, dialog];
}
