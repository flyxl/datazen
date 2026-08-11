import { useEffect, useRef } from 'react';
import {
  applyMatchedClipboard,
  matchConnectionClipboard,
} from '../../lib/connectionClipboard';
import type { ConnectionFormState } from './useConnectionForm';

export interface UseConnectionClipboardFillOptions {
  enabled: boolean;
  availableTypes?: string[] | null;
  onApplied?: (databaseType: string) => void;
}

export function useConnectionClipboardFill(
  form: ConnectionFormState,
  options: UseConnectionClipboardFillOptions,
): void {
  const formRef = useRef(form);
  formRef.current = form;
  const appliedRef = useRef(false);
  const onAppliedRef = useRef(options.onApplied);
  onAppliedRef.current = options.onApplied;
  const availableTypes = options.availableTypes ?? undefined;

  useEffect(() => {
    if (!options.enabled || appliedRef.current) return;

    const applyText = (text: string, fromPasswordField: boolean): boolean => {
      const current = formRef.current;
      if (current.name.trim() || current.password) return false;
      const matched = matchConnectionClipboard(text, availableTypes ?? undefined);
      if (!matched) return false;
      if (fromPasswordField && !text.includes('://')) return false;
      applyMatchedClipboard(current, matched);
      appliedRef.current = true;
      onAppliedRef.current?.(matched.databaseType);
      return true;
    };

    let cancelled = false;
    void (async () => {
      try {
        const text = await navigator.clipboard.readText();
        if (cancelled || appliedRef.current) return;
        applyText(text, false);
      } catch {
        /* clipboard permission / empty */
      }
    })();

    const onPaste = (event: ClipboardEvent) => {
      if (appliedRef.current) return;
      const text = event.clipboardData?.getData('text/plain') ?? '';
      const target = event.target;
      const fromPasswordField =
        target instanceof HTMLElement && Boolean(target.closest('input[type="password"]'));
      if (applyText(text, fromPasswordField)) {
        event.preventDefault();
      }
    };
    window.addEventListener('paste', onPaste);
    return () => {
      cancelled = true;
      window.removeEventListener('paste', onPaste);
    };
  }, [options.enabled, availableTypes]);
}
