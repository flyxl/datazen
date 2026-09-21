/**
 * Connection form state contract shared by host and driver connection fields.
 *
 * Structural mirror of the host `useConnectionForm()` hook return value:
 * the hook annotates its return with this interface, so any drift between
 * host implementation and this contract fails host type-checking.
 */
import type { Dispatch, KeyboardEvent, RefObject, SetStateAction } from 'react';
import type {
  DatabaseType,
  SavedTunnel,
  SshAuthMethod,
  SslMode,
  TunnelKind,
} from '../../../../src/types';

export interface ConnectionFormState {
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

  tunnelKind: TunnelKind;
  setTunnelKind: (kind: TunnelKind) => void;
  tunnelId: string | null;
  setTunnelId: (id: string | null) => void;
  savedTunnels: SavedTunnel[];

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
}
