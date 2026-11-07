import { useEffect, useRef } from 'react';
import { applyMatchedClipboard, matchConnectionClipboard } from '../../lib/connectionClipboard';
import type { ConnectionFormState } from './useConnectionForm';

export interface UseConnectionClipboardFillOptions {
  enabled: boolean;
  /**
   * Read the clipboard once when the form mounts (default `true`).
   *
   * Hosts that mount the form *without* a user gesture must pass `false`: on
   * macOS an automatic `clipboard.readText()` goes through the paste permission
   * path and can stall the renderer (observed as a frozen connection form on
   * the first-run journey, and as a hanging WebDriver click in E2E). Explicit
   * pastes keep working either way — the `paste` listener is always attached.
   */
  autoRead?: boolean;
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
    if (options.autoRead !== false) {
      void (async () => {
        try {
          const text = await navigator.clipboard.readText();
          if (cancelled || appliedRef.current) return;
          applyText(text, false);
        } catch {
          /* clipboard permission / empty */
        }
      })();
    }

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
