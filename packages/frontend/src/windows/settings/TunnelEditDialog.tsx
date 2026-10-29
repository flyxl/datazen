import { useEffect, useState } from 'react';
import { Label } from '@datazen/ui';
import { HttpProxyTunnelFields } from '../../components/connection/HttpProxyTunnelFields';
import { SshTunnelFields } from '../../components/connection/SshTunnelFields';
import { WebSocketTunnelFields } from '../../components/connection/WebSocketTunnelFields';
import {
  noopTabFill,
  type HttpProxyTunnelFieldsValue,
  type SshTunnelFieldsValue,
  type WebSocketTunnelFieldsValue,
} from '../../components/connection/tunnelFieldContracts';
import { Button } from '../../components/ui/Button';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { tunnelCommands } from '../../commands/tunnel';
import { pickPath } from '../../lib/pathPicker';
import { useI18n } from '../../hooks/useI18n';
import {
  SAVED_TUNNEL_KINDS,
  TUNNEL_DRAFT_PROBLEM_KEYS,
  draftToSavedTunnel,
  emptyTunnelDraft,
  savedTunnelToDraft,
  tunnelDraftProblem,
  type SavedTunnelKind,
  type SshDraft,
  type TunnelDraft,
} from '../../lib/tunnelDraft';
import { newTunnelId, useTunnelStore } from '../../stores/tunnelStore';
import type { SshAuthMethod } from '../../types';

export interface TunnelEditDialogProps {
  open: boolean;
  /** Entity to edit; `null` opens a blank create form. */
  tunnelId: string | null;
  onClose: () => void;
  /** Called after a successful create/update so the list can refresh. */
  onSaved: () => void;
}

/**
 * A dialog error is either a raw IPC message or a translation key resolved at
 * render time. Keeping keys unresolved means the fetch effect never has to
 * depend on the `t` function's identity (a caller-provided translator that
 * changes on every render would otherwise re-run the effect forever).
 */
type DialogError = { message: string } | { key: string };

function toDialogError(e: unknown, fallbackKey: string): DialogError {
  if (typeof e === 'string' && e) return { message: e };
  if (e instanceof Error && e.message) return { message: e.message };
  return { key: fallbackKey };
}

/**
 * Create/edit a saved tunnel.
 *
 * This is the only place that fetches a full entity (`get_tunnel`), because only
 * editing needs the decrypted credentials; every list/picker path stays on the
 * secret-free summaries (G9). The field bodies are the very same components the
 * connection form uses, driven through the narrow contracts in
 * `tunnelFieldContracts` — no second copy of the tunnel inputs.
 */
export function TunnelEditDialog({ open, tunnelId, onClose, onSaved }: TunnelEditDialogProps) {
  const { t } = useI18n();
  const createTunnel = useTunnelStore((s) => s.create);
  const updateTunnel = useTunnelStore((s) => s.update);

  const [draft, setDraft] = useState<TunnelDraft>(() => emptyTunnelDraft());
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [error, setError] = useState<DialogError | null>(null);

  // Opening always starts from a clean form: blank when creating, the stored
  // entity when editing (fetched here, never carried over from the list).
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setError(null);
    setSaving(false);
    setDirty(false);
    if (!tunnelId) {
      setDraft(emptyTunnelDraft());
      setLoading(false);
      return;
    }
    setLoading(true);
    void tunnelCommands
      .getTunnel(tunnelId)
      .then((entity) => {
        if (cancelled) return;
        if (!entity) {
          setError({ key: 'settings.tunnels.editor.loadFailed' });
          setDraft(emptyTunnelDraft());
          return;
        }
        setDraft(savedTunnelToDraft(entity));
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(toDialogError(e, 'settings.tunnels.editor.loadFailed'));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, tunnelId]);

  const patchSsh = (partial: Partial<SshDraft>) => {
    setDirty(true);
    setDraft((d) => ({ ...d, ssh: { ...d.ssh, ...partial } }));
  };
  const patchJump = (partial: Partial<SshDraft['jump']>) => {
    setDirty(true);
    setDraft((d) => ({ ...d, ssh: { ...d.ssh, jump: { ...d.ssh.jump, ...partial } } }));
  };
  const patchHttpProxy = (partial: Partial<TunnelDraft['httpProxy']>) => {
    setDirty(true);
    setDraft((d) => ({ ...d, httpProxy: { ...d.httpProxy, ...partial } }));
  };
  const patchWebSocket = (partial: Partial<TunnelDraft['websocket']>) => {
    setDirty(true);
    setDraft((d) => ({ ...d, websocket: { ...d.websocket, ...partial } }));
  };

  const sshForm: SshTunnelFieldsValue = {
    supportsSSH: true,
    // The kind selector above owns the choice, so the reused fields render
    // without their own enable checkbox (a second toggle would be dead here).
    sshEnabled: true,
    setSshEnabled: () => {},
    sshHost: draft.ssh.host,
    setSshHost: (v) => patchSsh({ host: v }),
    sshPort: draft.ssh.port,
    setSshPort: (v) => patchSsh({ port: v }),
    sshUsername: draft.ssh.username,
    setSshUsername: (v) => patchSsh({ username: v }),
    sshAuthMethod: draft.ssh.authMethod,
    setSshAuthMethod: (v: SshAuthMethod) => patchSsh({ authMethod: v }),
    sshPassword: draft.ssh.password,
    setSshPassword: (v) => patchSsh({ password: v }),
    sshKeyPath: draft.ssh.keyPath,
    setSshKeyPath: (v) => patchSsh({ keyPath: v }),
    sshPassphrase: draft.ssh.passphrase,
    setSshPassphrase: (v) => patchSsh({ passphrase: v }),
    sshJumpEnabled: draft.ssh.jump.enabled,
    setSshJumpEnabled: (v) => patchJump({ enabled: v }),
    sshJumpHost: draft.ssh.jump.host,
    setSshJumpHost: (v) => patchJump({ host: v }),
    sshJumpPort: draft.ssh.jump.port,
    setSshJumpPort: (v) => patchJump({ port: v }),
    sshJumpUsername: draft.ssh.jump.username,
    setSshJumpUsername: (v) => patchJump({ username: v }),
    sshJumpAuthMethod: draft.ssh.jump.authMethod,
    setSshJumpAuthMethod: (v: SshAuthMethod) => patchJump({ authMethod: v }),
    sshJumpPassword: draft.ssh.jump.password,
    setSshJumpPassword: (v) => patchJump({ password: v }),
    sshJumpKeyPath: draft.ssh.jump.keyPath,
    setSshJumpKeyPath: (v) => patchJump({ keyPath: v }),
    sshJumpPassphrase: draft.ssh.jump.passphrase,
    setSshJumpPassphrase: (v) => patchJump({ passphrase: v }),
    tabFill: noopTabFill,
    // The key-path inputs are `PathInput`s, which ask the host for a picker.
    pickPath,
  };

  const httpProxyForm: HttpProxyTunnelFieldsValue = {
    httpProxyHost: draft.httpProxy.host,
    setHttpProxyHost: (v) => patchHttpProxy({ host: v }),
    httpProxyPort: draft.httpProxy.port,
    setHttpProxyPort: (v) => patchHttpProxy({ port: v }),
    httpProxyScheme: draft.httpProxy.scheme,
    setHttpProxyScheme: (v) => patchHttpProxy({ scheme: v }),
    httpProxyUsername: draft.httpProxy.username,
    setHttpProxyUsername: (v) => patchHttpProxy({ username: v }),
    httpProxyPassword: draft.httpProxy.password,
    setHttpProxyPassword: (v) => patchHttpProxy({ password: v }),
    httpProxyTimeout: draft.httpProxy.timeout,
    setHttpProxyTimeout: (v) => patchHttpProxy({ timeout: v }),
  };

  const webSocketForm: WebSocketTunnelFieldsValue = {
    wsUrl: draft.websocket.url,
    setWsUrl: (v) => patchWebSocket({ url: v }),
    wsMode: draft.websocket.mode,
    setWsMode: (v) => patchWebSocket({ mode: v }),
    wsAuthToken: draft.websocket.authToken,
    setWsAuthToken: (v) => patchWebSocket({ authToken: v }),
    wsTimeout: draft.websocket.timeout,
    setWsTimeout: (v) => patchWebSocket({ timeout: v }),
  };

  const problem = tunnelDraftProblem(draft);
  const pending = loading || saving;
  const hint = dirty && problem ? t(TUNNEL_DRAFT_PROBLEM_KEYS[problem]) : null;
  const errorText = error ? ('key' in error ? t(error.key) : error.message) : null;

  const handleSave = async () => {
    if (problem || pending) return;
    setSaving(true);
    setError(null);
    const entity = draftToSavedTunnel(draft, draft.id ?? newTunnelId());
    try {
      if (draft.id) {
        await updateTunnel(entity);
      } else {
        await createTunnel(entity);
      }
      onSaved();
      onClose();
    } catch (e) {
      setError(toDialogError(e, 'settings.tunnels.editor.saveFailed'));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      title={t(
        draft.id ? 'settings.tunnels.editor.editTitle' : 'settings.tunnels.editor.createTitle',
      )}
      description={t('settings.tunnels.editor.description')}
      onClose={onClose}
      testId="tunnel-edit-dialog"
      className="max-w-2xl"
      footer={
        <>
          <Button
            variant="secondary"
            onClick={onClose}
            disabled={saving}
            data-testid="tunnel-edit-cancel"
          >
            {t('common.cancel')}
          </Button>
          <Button
            onClick={() => void handleSave()}
            disabled={problem !== null || pending}
            data-testid="tunnel-edit-save"
          >
            {saving ? t('common.loading') : t('common.save')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div>
            <Label required>{t('settings.tunnels.editor.name')}</Label>
            <Input
              value={draft.name}
              onChange={(e) => {
                setDirty(true);
                setDraft((d) => ({ ...d, name: e.target.value }));
              }}
              placeholder={t('settings.tunnels.editor.namePlaceholder')}
              data-testid="tunnel-edit-name"
            />
          </div>
          <div>
            <Label required>{t('settings.tunnels.editor.kind')}</Label>
            <Select
              value={draft.kind}
              options={SAVED_TUNNEL_KINDS.map((kind) => ({
                value: kind,
                label: t(`settings.tunnels.kind.${kind}`),
              }))}
              onChange={(v) => {
                setDirty(true);
                setDraft((d) => ({ ...d, kind: v as SavedTunnelKind }));
              }}
              triggerDataAttrs={{ 'data-testid': 'tunnel-edit-kind' }}
            />
          </div>
        </div>

        {loading ? (
          <p className="text-xs text-fg-muted">{t('common.loading')}</p>
        ) : (
          <>
            {draft.kind === 'ssh' && <SshTunnelFields form={sshForm} showEnableToggle={false} />}
            {draft.kind === 'httpProxy' && <HttpProxyTunnelFields form={httpProxyForm} />}
            {draft.kind === 'websocket' && <WebSocketTunnelFields form={webSocketForm} />}
          </>
        )}

        {hint ? (
          <ErrorBanner as="p" data-testid="tunnel-edit-hint">
            {hint}
          </ErrorBanner>
        ) : null}
        {errorText ? (
          <ErrorBanner as="p" data-testid="tunnel-edit-error">
            {errorText}
          </ErrorBanner>
        ) : null}
      </div>
    </Dialog>
  );
}
