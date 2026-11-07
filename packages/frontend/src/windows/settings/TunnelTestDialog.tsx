import { useEffect, useState } from 'react';
import { Label } from '@datazen/ui';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { Input } from '../../components/ui/Input';
import { tunnelCommands } from '../../commands/tunnel';
import { useI18n } from '../../hooks/useI18n';
import type { SavedTunnelSummary } from '../../types';

export interface TunnelTestDialogProps {
  open: boolean;
  tunnel: SavedTunnelSummary | null;
  onClose: () => void;
}

const DEFAULT_TARGET_HOST = '127.0.0.1';

function messageOf(e: unknown): string | null {
  if (typeof e === 'string' && e) return e;
  if (e instanceof Error && e.message) return e.message;
  return null;
}

function isValidPort(value: string): boolean {
  const port = Number(value);
  return value.trim().length > 0 && Number.isInteger(port) && port > 0 && port <= 65535;
}

/**
 * Live probe for a saved tunnel.
 *
 * The target host/port is a required input, not a decoration: the probe sends a
 * CONNECT handshake to that target (http proxy) or opens a channel to it
 * (websocket `datazen_v1`). The scope note states honestly what a pass proves —
 * an SSH probe only reaches the jump host, `raw_binary` only reaches the relay.
 */
export function TunnelTestDialog({ open, tunnel, onClose }: TunnelTestDialogProps) {
  const { t } = useI18n();
  const [host, setHost] = useState(DEFAULT_TARGET_HOST);
  const [port, setPort] = useState('');
  const [running, setRunning] = useState(false);
  const [elapsedMs, setElapsedMs] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setHost(DEFAULT_TARGET_HOST);
    setPort('');
    setRunning(false);
    setElapsedMs(null);
    setError(null);
  }, [open, tunnel?.id]);

  const tunnelId = tunnel?.id ?? null;
  const hostMissing = host.trim().length === 0;
  const portMissing = !isValidPort(port);
  const invalid = hostMissing || portMissing;

  const handleRun = async () => {
    if (!tunnelId || invalid || running) return;
    setRunning(true);
    setElapsedMs(null);
    setError(null);
    try {
      const ms = await tunnelCommands.testTunnel(tunnelId, host.trim(), Number(port));
      setElapsedMs(ms);
    } catch (e) {
      setError(messageOf(e) ?? t('common.operationFailed'));
    } finally {
      setRunning(false);
    }
  };

  const scopeKey =
    tunnel?.kind === 'ssh'
      ? 'settings.tunnels.test.scope.ssh'
      : tunnel?.kind === 'httpProxy'
        ? 'settings.tunnels.test.scope.httpProxy'
        : 'settings.tunnels.test.scope.websocket';

  return (
    <Dialog
      open={open}
      title={t('settings.tunnels.test.title')}
      description={t('settings.tunnels.test.description', { name: tunnel?.name ?? '' })}
      onClose={running ? () => {} : onClose}
      testId="tunnel-test-dialog"
      footer={
        <>
          <Button
            variant="secondary"
            onClick={onClose}
            disabled={running}
            data-testid="tunnel-test-close"
          >
            {t('common.close')}
          </Button>
          <Button
            onClick={() => void handleRun()}
            disabled={invalid || running}
            data-testid="tunnel-test-run"
          >
            {running ? t('settings.tunnels.test.running') : t('settings.tunnels.test.run')}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div>
            <Label required>{t('settings.tunnels.test.targetHost')}</Label>
            <Input
              value={host}
              onChange={(e) => setHost(e.target.value)}
              placeholder={DEFAULT_TARGET_HOST}
              data-testid="tunnel-test-host"
            />
          </div>
          <div>
            <Label required>{t('settings.tunnels.test.targetPort')}</Label>
            <Input
              value={port}
              onChange={(e) => setPort(e.target.value)}
              placeholder="5432"
              inputMode="numeric"
              data-testid="tunnel-test-port"
            />
          </div>
        </div>

        <p className="text-[11px] text-fg-muted" data-testid="tunnel-test-scope">
          {t(scopeKey)}
        </p>
        <p className="text-[11px] text-fg-muted" data-testid="tunnel-test-scope-general">
          {t('settings.tunnels.test.scope.general')}
        </p>

        {elapsedMs !== null ? (
          <p className="text-xs text-green-500" role="status" data-testid="tunnel-test-success">
            {t('settings.tunnels.test.success', { ms: elapsedMs })}
          </p>
        ) : null}
        {error ? (
          <ErrorBanner as="p" data-testid="tunnel-test-error">
            {t('settings.tunnels.test.failed', { error })}
          </ErrorBanner>
        ) : null}
        {hostMissing || portMissing ? (
          <p className="text-[11px] text-fg-muted" data-testid="tunnel-test-target-hint">
            {hostMissing
              ? t('settings.tunnels.test.hostRequired')
              : t('settings.tunnels.test.portRequired')}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
