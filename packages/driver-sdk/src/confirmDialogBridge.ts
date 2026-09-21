import type { ReactNode } from 'react';

/**
 * Confirm dialog bridge — driver-SDK capability injection.
 *
 * The imperative confirm dialog is owned by the host (`useConfirmDialog`
 * renders `ConfirmDialog`); the host binds the hook at module load
 * (schemaStoreBridge pattern) and drivers consume `useBoundConfirmDialog`.
 */

export type ConfirmDialogOptions = {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  kind?: 'warning' | 'info';
  /** Optional badge/tag displayed next to the title. */
  badge?: string;
  /** Optional SQL code preview section. */
  codePreview?: string;
  /** Optional longer description displayed below the message. */
  description?: string;
};

export type ConfirmDialogFn = (options: ConfirmDialogOptions) => Promise<boolean>;

/** Same contract as the host `useConfirmDialog`: [confirmFn, DialogElement]. */
export type BoundConfirmDialogHook = () => [ConfirmDialogFn, ReactNode];

let boundHook: BoundConfirmDialogHook | null = null;

export function bindConfirmDialog(hook: BoundConfirmDialogHook): void {
  boundHook = hook;
}

export function useBoundConfirmDialog(): [ConfirmDialogFn, ReactNode] {
  if (!boundHook) {
    throw new Error('ConfirmDialog has not been bound to driver-sdk yet.');
  }
  return boundHook();
}
