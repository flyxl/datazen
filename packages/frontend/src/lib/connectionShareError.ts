import type { I18nKey } from '../locales';

type TranslateFn = (key: I18nKey, params?: Record<string, string | number>) => string;

const FORMAT_LABELS = ['DataZen', 'TablePlus', 'Encrypted connection'] as const;

function extractFormatLabel(message: string): string | undefined {
  return FORMAT_LABELS.find((label) => message.includes(label));
}

/** Map backend connection import/export IPC strings to localized messages. */
export function translateConnectionShareError(
  message: string,
  t: TranslateFn,
  fallback: string,
): string {
  const trimmed = message.trim();
  if (!trimmed) return fallback;

  const format = extractFormatLabel(trimmed);

  const exact: Record<string, I18nKey> = {
    'Password is required': 'connShare.error.passwordRequired',
    'Password is required for encrypted connection import':
      'connShare.error.encryptedImportPasswordRequired',
    'Password is required for DBX encrypted import': 'connShare.error.dbxEncryptedPasswordRequired',
    'Password is required for DataZen encrypted import':
      'connShare.error.datazenEncryptedPasswordRequired',
    'Password is required for DataZen import': 'connShare.error.datazenPasswordRequired',
    'Import file is not valid UTF-8 text (for DataZen use .datazenconnection)':
      'connShare.error.invalidUtf8',
    'Unrecognized connection import format': 'connShare.error.unrecognizedFormat',
    'No connection files to import': 'connShare.error.noFiles',
    'Decryption failed: wrong password': 'connShare.error.decryptionWrongPassword',
    "Invalid import file: missing 'connections' field": 'connShare.error.missingConnectionsField',
    'missing connections': 'connShare.error.missingConnections',
    'DBX decryption failed: wrong passphrase': 'connShare.error.dbxWrongPassphrase',
    'DBX import: missing connections array': 'connShare.error.dbxMissingConnections',
  };

  const exactKey = exact[trimmed];
  if (exactKey) return t(exactKey);

  if (format) {
    if (trimmed === `Password is required for ${format} import`) {
      return t('connShare.error.formatPasswordRequired', { format });
    }
    if (trimmed === `Invalid ${format} file: too short`) {
      return t('connShare.error.invalidFileTooShort', { format });
    }
    if (trimmed === `Invalid ${format} file: expected password-based encryption`) {
      return t('connShare.error.invalidFileEncryptionType', { format });
    }
    if (trimmed.startsWith(`Invalid ${format} file: unsupported RNCryptor version`)) {
      return t('connShare.error.invalidFileRncryptorVersion', { format });
    }
    if (
      trimmed.startsWith(`${format} decryption failed: wrong password or corrupt file`) &&
      trimmed.includes('Legacy DataZen')
    ) {
      return t('connShare.error.decryptionFailedLegacyHint', { format });
    }
    if (trimmed === `${format} decryption failed: wrong password or corrupt file`) {
      return t('connShare.error.decryptionFailed', { format });
    }
    if (trimmed.startsWith(`Invalid ${format} JSON after decrypt:`)) {
      return t('connShare.error.invalidJsonAfterDecrypt', { format });
    }
    if (trimmed.startsWith(`${format} payload is not UTF-8:`)) {
      return t('connShare.error.payloadNotUtf8', { format });
    }
  }

  if (trimmed.startsWith('Failed to read ')) {
    return t('connShare.error.readFailed');
  }

  if (trimmed.startsWith('Unknown import source: ')) {
    return t('connShare.error.unknownImportSource');
  }

  return trimmed;
}

export function ipcConnectionShareError(e: unknown, t: TranslateFn, fallback: string): string {
  const raw =
    typeof e === 'string' && e.trim() ? e : e instanceof Error && e.message.trim() ? e.message : '';
  if (!raw) return fallback;
  return translateConnectionShareError(raw, t, fallback);
}

export interface ConnectionImportResult {
  imported: number;
  overwritten: number;
  groupsAdded: number;
  skipped?: string[];
}

export function formatConnectionImportSuccess(
  result: ConnectionImportResult,
  t: TranslateFn,
): string {
  const skipped = result.skipped?.length ?? 0;
  if (skipped > 0) {
    return t('connShare.importSuccessWithSkipped', {
      imported: result.imported,
      overwritten: result.overwritten,
      groupsAdded: result.groupsAdded,
      skipped,
    });
  }
  return t('connShare.importSuccess', {
    imported: result.imported,
    overwritten: result.overwritten,
    groupsAdded: result.groupsAdded,
  });
}
