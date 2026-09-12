import { useCallback, useEffect, useMemo, useState } from 'react';
import { connectionCommands } from '../../commands/connection';
import { useI18n } from '../../hooks/useI18n';
import { ipcConnectionShareError } from '../../lib/connectionShareError';
import { importFilePasswordPolicy } from '../../lib/importConnectionFile';
import type { ConnectionImportApp, ConnectionImportSource } from './ConnectionShareDialog';

/** Successful connection import statistics reported by the backend. */
export interface ConnectionImportResult {
  imported: number;
  overwritten: number;
  groupsAdded: number;
  skipped?: string[];
  sourceFormat?: string;
}

interface UseConnectionImportOptions {
  /** Where the connections come from (a DataZen/foreign file or another client). */
  source: ConnectionImportSource;
  /**
   * Reset internal state and skip path detection while false (dialogs pass
   * their `open` flag; always-mounted steps pass nothing).
   */
  enabled?: boolean;
  onImportSuccess?: (result: ConnectionImportResult) => void;
  onError?: (message: string) => void;
}

/**
 * Connection import flow shared by `ConnectionShareDialog` and the first-run
 * journey's inline import step: source pre-detection, file picking, the two
 * step (pick file → import with password) file flow and password policy.
 *
 * The hook owns no chrome, so callers can render it inside a dialog or inline.
 */
export function useConnectionImport({
  source,
  enabled = true,
  onImportSuccess,
  onError,
}: UseConnectionImportOptions) {
  const { t } = useI18n();
  const [password, setPassword] = useState('');
  const [dataPath, setDataPath] = useState('');
  const [pathFound, setPathFound] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);

  const appImport = source !== 'file';
  const importPasswordPolicy = selectedFile ? importFilePasswordPolicy(selectedFile) : null;

  // Detect the default config path of the chosen external client.
  useEffect(() => {
    if (!enabled) {
      setPassword('');
      setDataPath('');
      setPathFound(false);
      setLocalError(null);
      setSubmitting(false);
      setDetecting(false);
      setSelectedFile(null);
      return;
    }

    setLocalError(null);
    setSelectedFile(null);
    if (!appImport) {
      setDataPath('');
      setPathFound(false);
      setDetecting(false);
      return;
    }

    let cancelled = false;
    setDetecting(true);
    void connectionCommands
      .detectConnectionImportPath(source)
      .then((detected) => {
        if (cancelled) return;
        setDataPath(detected.path);
        setPathFound(detected.found);
      })
      .catch(() => {
        if (!cancelled) {
          setDataPath('');
          setPathFound(false);
        }
      })
      .finally(() => {
        if (!cancelled) setDetecting(false);
      });
    return () => {
      cancelled = true;
    };
  }, [appImport, enabled, source]);

  const replaceError = useCallback(
    (message: string) => {
      setLocalError(message);
      onError?.(message);
    },
    [onError],
  );

  const browsePath = useCallback(
    async (kind: 'file' | 'folder') => {
      if (!appImport) return;
      try {
        const picked = await connectionCommands.pickConnectionImportPathWithDialog(
          kind,
          source as ConnectionImportApp,
        );
        if (picked) {
          setDataPath(picked);
          setPathFound(true);
          setLocalError(null);
        }
      } catch (e) {
        replaceError(ipcConnectionShareError(e, t, t('common.importFailed')));
      }
    },
    [appImport, replaceError, source, t],
  );

  const pickImportFile = useCallback(async () => {
    setLocalError(null);
    setSubmitting(true);
    try {
      const picked = await connectionCommands.pickConnectionsImportFile();
      if (picked) setSelectedFile(picked);
      return picked;
    } catch (e) {
      replaceError(ipcConnectionShareError(e, t, t('common.importFailed')));
      return null;
    } finally {
      setSubmitting(false);
    }
  }, [replaceError, t]);

  /**
   * Run the import. In the file flow the first call only picks the file
   * (TablePlus-style: pick file first, then decrypt with the entered password).
   */
  const submit = useCallback(async () => {
    setLocalError(null);

    if (appImport && !dataPath.trim() && !pathFound) {
      setLocalError(t('connShare.pathRequired'));
      return;
    }

    setSubmitting(true);
    try {
      if (appImport) {
        const result = await connectionCommands.importConnectionsFromApp(
          source as ConnectionImportApp,
          password,
          dataPath,
        );
        onImportSuccess?.(result);
      } else {
        let file = selectedFile;
        if (!file) {
          file = (await connectionCommands.pickConnectionsImportFile()) ?? null;
          if (!file) return;
          setSelectedFile(file);
          return;
        }

        if (importFilePasswordPolicy(file) === 'required' && !password.trim()) {
          setLocalError(t('connShare.encryptedImportPasswordRequired'));
          return;
        }

        const result = await connectionCommands.importConnectionsAtPath(password, file);
        onImportSuccess?.(result);
      }
    } catch (e) {
      const message = ipcConnectionShareError(e, t, t('common.importFailed'));
      setLocalError(message);
      onError?.(message);
    } finally {
      setSubmitting(false);
    }
  }, [appImport, dataPath, onError, onImportSuccess, password, pathFound, selectedFile, source, t]);

  /** Label of the caller's primary action button. */
  const primaryActionLabel = useMemo(
    () =>
      !appImport && !selectedFile ? t('connShare.chooseImportFile') : t('connShare.importAction'),
    [appImport, selectedFile, t],
  );

  const busy = submitting || detecting;

  return {
    source,
    appImport,
    password,
    setPassword,
    dataPath,
    setDataPath,
    setPathFound,
    pathFound,
    detecting,
    submitting,
    busy,
    localError,
    setLocalError,
    selectedFile,
    importPasswordPolicy,
    primaryActionLabel,
    browsePath,
    pickImportFile,
    submit,
  };
}

export type ConnectionImportState = ReturnType<typeof useConnectionImport>;
