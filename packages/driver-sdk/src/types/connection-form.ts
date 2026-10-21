/**
 * Connection form state contract shared by host and driver connection fields.
 *
 * Structural mirror of the host `useConnectionForm()` hook return value:
 * the hook annotates its return with this interface, so any drift between
 * host implementation and this contract fails host type-checking.
 */
import type { Dispatch, KeyboardEvent, RefObject, SetStateAction } from 'react';
import type { PathPicker } from '@datazen/ui';
import type {
  DatabaseType,
  SavedTunnel,
  SavedTunnelSummary,
  SshAuthMethod,
  SslMode,
  TunnelKind,
  TunnelSource,
} from '../../../../src/types';

export interface ConnectionFormState {
  /** Edit state controls the credential-preservation hint in the host form. */
  isEditing?: boolean;
  /** Clear stored secrets that are blank when this connection is saved. */
  clearStoredCredentials?: boolean;
  setClearStoredCredentials?: (value: boolean) => void;
  name: string;
  setName: Dispatch<SetStateAction<string>>;
  databaseType: DatabaseType;
  setDatabaseType: (newType: DatabaseType) => void;
  host: string;
  setHost: Dispatch<SetStateAction<string>>;
  port: string;
  setPort: Dispatch<SetStateAction<string>>;
  database: string;
  setDatabase: Dispatch<SetStateAction<string>>;
  schema: string;
  setSchema: Dispatch<SetStateAction<string>>;
  username: string;
  setUsername: Dispatch<SetStateAction<string>>;
  password: string;
  setPassword: Dispatch<SetStateAction<string>>;
  sslMode: SslMode;
  setSslMode: Dispatch<SetStateAction<SslMode>>;
  group: string;
  setGroup: (value: unknown) => void;
  colorTag: string;
  setColorTag: Dispatch<SetStateAction<string>>;
  readOnly: boolean;
  driverReadOnly: boolean;
  setReadOnly: (value: boolean) => void;

  /**
   * Tunnel source state machine (`none` / `saved` / `inline`). Single master
   * control: `tunnelId` is only cleared by an explicit transition, never as a
   * side effect of changing the tunnel type.
   */
  tunnelSource: TunnelSource;
  setTunnelSource: (source: TunnelSource) => void;
  /** Inline sub-selector value; read-only derived value while `saved`. */
  tunnelKind: TunnelKind;
  setTunnelKind: (kind: TunnelKind) => void;
  tunnelId: string | null;
  setTunnelId: (id: string | null) => void;
  /** Secret-free summaries from `tunnelStore` (app-wide, always fresh). */
  savedTunnels: SavedTunnelSummary[];
  /** Resolved summary for the current `tunnelId` (null when unset or missing). */
  savedTunnel: SavedTunnelSummary | null;
  /** True when `tunnelId` is set but no longer resolves to a saved tunnel. */
  tunnelRefMissing: boolean;
  /** True when the inline tunnel configuration passes tunnel-level validation. */
  tunnelInlineValid: boolean;
  tunnelBusy: boolean;
  tunnelError: string | null;
  /** Inline kind actually in effect (SSH with its toggle off counts as `none`). */
  effectiveTunnelKind: TunnelKind;
  /** `saved` → `inline`: fetch the entity and refill inline fields; never drops parameters. */
  unbindTunnel: () => Promise<void>;
  /** `inline` → `saved`: persist the current inline config; resolves to the entity or null. */
  saveAsTunnel: (name: string) => Promise<SavedTunnel | null>;
  /** Load an existing connection's tunnel reference without triggering an unbind. */
  hydrateTunnelRef: (id: string | null, kind: TunnelKind) => void;
  /** Reset the whole tunnel slice (database type switch). */
  resetTunnel: () => void;

  sshEnabled: boolean;
  setSshEnabled: (v: boolean) => void;
  sshHost: string;
  setSshHost: Dispatch<SetStateAction<string>>;
  sshPort: string;
  setSshPort: Dispatch<SetStateAction<string>>;
  sshUsername: string;
  setSshUsername: Dispatch<SetStateAction<string>>;
  sshAuthMethod: SshAuthMethod;
  setSshAuthMethod: Dispatch<SetStateAction<SshAuthMethod>>;
  sshPassword: string;
  setSshPassword: Dispatch<SetStateAction<string>>;
  sshKeyPath: string;
  setSshKeyPath: Dispatch<SetStateAction<string>>;
  sshPassphrase: string;
  setSshPassphrase: Dispatch<SetStateAction<string>>;
  sshJumpEnabled: boolean;
  setSshJumpEnabled: Dispatch<SetStateAction<boolean>>;
  sshJumpHost: string;
  setSshJumpHost: Dispatch<SetStateAction<string>>;
  sshJumpPort: string;
  setSshJumpPort: Dispatch<SetStateAction<string>>;
  sshJumpUsername: string;
  setSshJumpUsername: Dispatch<SetStateAction<string>>;
  sshJumpAuthMethod: SshAuthMethod;
  setSshJumpAuthMethod: Dispatch<SetStateAction<SshAuthMethod>>;
  sshJumpPassword: string;
  setSshJumpPassword: Dispatch<SetStateAction<string>>;
  sshJumpKeyPath: string;
  setSshJumpKeyPath: Dispatch<SetStateAction<string>>;
  sshJumpPassphrase: string;
  setSshJumpPassphrase: Dispatch<SetStateAction<string>>;

  httpProxyHost: string;
  setHttpProxyHost: Dispatch<SetStateAction<string>>;
  httpProxyPort: string;
  setHttpProxyPort: Dispatch<SetStateAction<string>>;
  httpProxyScheme: 'http' | 'https';
  setHttpProxyScheme: Dispatch<SetStateAction<'http' | 'https'>>;
  httpProxyUsername: string;
  setHttpProxyUsername: Dispatch<SetStateAction<string>>;
  httpProxyPassword: string;
  setHttpProxyPassword: Dispatch<SetStateAction<string>>;
  httpProxyTimeout: string;
  setHttpProxyTimeout: Dispatch<SetStateAction<string>>;

  wsUrl: string;
  setWsUrl: Dispatch<SetStateAction<string>>;
  wsMode: 'datazen_v1' | 'raw_binary';
  setWsMode: Dispatch<SetStateAction<'datazen_v1' | 'raw_binary'>>;
  wsAuthToken: string;
  setWsAuthToken: Dispatch<SetStateAction<string>>;
  wsTimeout: string;
  setWsTimeout: Dispatch<SetStateAction<string>>;

  formVariant: string;
  hasUsername: boolean;
  supportsSSL: boolean;
  supportsSSH: boolean;
  sslOptions: Array<{ value: string; label: string }>;

  handleDatabaseTypeChange: (newType: DatabaseType) => void;
  onTest: () => Promise<void>;
  onSave: () => Promise<void>;
  testing: boolean;
  testOk: string | null;
  setTestOk: Dispatch<SetStateAction<string | null>>;
  testErr: string | null;
  setTestErr: Dispatch<SetStateAction<string | null>>;
  testResultRef: RefObject<HTMLDivElement>;
  showAdvanced: boolean;
  setShowAdvanced: Dispatch<SetStateAction<boolean>>;
  tabFill: (setter: (v: string) => void) => (e: KeyboardEvent<HTMLInputElement>) => void;
  validationErrors: Record<string, string>;
  validate: () => boolean;
  options: Record<string, unknown>;
  setOptions: (
    next: Record<string, unknown> | ((prev: Record<string, unknown>) => Record<string, unknown>),
  ) => void;

  /**
   * Opens the host's native path picker. Driver connection fields render
   * `@datazen/ui`'s `PathInput`, which is a pure view: it never calls a host
   * API itself, so the host passes the picker in through this contract.
   */
  pickPath: PathPicker;
}
