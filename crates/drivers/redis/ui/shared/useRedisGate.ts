import { useCallback, type ReactNode } from 'react';
import { useI18n } from '@datazen/ui';
import { useBoundSettingsStore, useBoundConfirmDialog } from '@datazen/driver-sdk';
import { type DangerLevel, requiresConfirmation } from '../console/redisConsoleDanger';

/**
 * Gate level for a write path. `'write-op'` marks a workbench mutation that is
 * blocked by Safe Mode but never triggers a danger confirmation (that's Console-only).
 */
export type RedisGateLevel = DangerLevel | 'write-op';

/** Signature of the `gateWrite` callback returned by {@link useRedisGate}. */
export type GateWriteFn = (level: RedisGateLevel, command?: string) => Promise<boolean>;

export interface UseRedisGateResult {
  /**
   * Returns `true` when the caller may proceed, `false` when the action was
   * blocked (Safe Mode) or cancelled (danger confirmation). Reads `safeMode`
   * live at call time, matching the SQL execution gate.
   */
  gateWrite: (level: RedisGateLevel, command?: string) => Promise<boolean>;
  /** Dialog element to render once in the component tree. */
  gateDialog: ReactNode;
}

export function useRedisGate(): UseRedisGateResult {
  const { t } = useI18n();
  const [confirm, dialog] = useBoundConfirmDialog();

  const gateWrite = useCallback(
    async (level: RedisGateLevel, command?: string): Promise<boolean> => {
      const safeMode = useBoundSettingsStore.getState().settings.safeMode;
      const isWrite = level !== 'safe';

      if (safeMode && isWrite) {
        await confirm({
          title: t('settings.safeMode'),
          message: t('redis.safeMode.blocked'),
          confirmLabel: t('redis.safeMode.dismiss'),
          kind: 'info',
        });
        return false;
      }

      if (requiresConfirmation(level as DangerLevel)) {
        const cmd = command?.trim() || (level as string);
        return confirm({
          title: t('redis.danger.confirmTitle'),
          message: t('redis.danger.confirmMessage').replace('{command}', cmd),
          confirmLabel: t('redis.danger.confirm'),
          kind: 'warning',
          badge: level as string,
          codePreview: command,
        });
      }

      return true;
    },
    [confirm, t],
  );

  return { gateWrite, gateDialog: dialog };
}
