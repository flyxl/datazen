import type { KeyboardEvent } from 'react';
import type { PathPicker } from '@datazen/ui';
import type { SshAuthMethod } from '../../types';

/**
 * Controlled contracts for the three tunnel field components.
 *
 * They are structural subsets of `ConnectionFormState`, so the connection form
 * keeps passing its whole state object unchanged, while a second consumer (the
 * settings tunnel editor) can drive the very same fields from a lightweight
 * adapter without casting through `unknown`.
 *
 * Setters take plain values on purpose: every call site inside the field
 * components assigns an input value, and `Dispatch<SetStateAction<string>>`
 * (the form's type) is assignable to `(value: string) => void`.
 */

export interface SshTunnelFieldsValue {
  /** Drivers that cannot carry an SSH tunnel hide the whole block. */
  supportsSSH: boolean;
  sshEnabled: boolean;
  setSshEnabled: (value: boolean) => void;
  sshHost: string;
  setSshHost: (value: string) => void;
  sshPort: string;
  setSshPort: (value: string) => void;
  sshUsername: string;
  setSshUsername: (value: string) => void;
  sshAuthMethod: SshAuthMethod;
  setSshAuthMethod: (value: SshAuthMethod) => void;
  sshPassword: string;
  setSshPassword: (value: string) => void;
  sshKeyPath: string;
  setSshKeyPath: (value: string) => void;
  sshPassphrase: string;
  setSshPassphrase: (value: string) => void;
  sshJumpEnabled: boolean;
  setSshJumpEnabled: (value: boolean) => void;
  sshJumpHost: string;
  setSshJumpHost: (value: string) => void;
  sshJumpPort: string;
  setSshJumpPort: (value: string) => void;
  sshJumpUsername: string;
  setSshJumpUsername: (value: string) => void;
  sshJumpAuthMethod: SshAuthMethod;
  setSshJumpAuthMethod: (value: SshAuthMethod) => void;
  sshJumpPassword: string;
  setSshJumpPassword: (value: string) => void;
  sshJumpKeyPath: string;
  setSshJumpKeyPath: (value: string) => void;
  sshJumpPassphrase: string;
  setSshJumpPassphrase: (value: string) => void;
  /** Tab-to-fill factory; consumers without clipboard fill pass a no-op. */
  tabFill: (setter: (value: string) => void) => (e: KeyboardEvent<HTMLInputElement>) => void;
  /**
   * Opens the host's native path picker for the key-path fields. `PathInput`
   * is a pure view, so every host of these fields must supply one.
   */
  pickPath: PathPicker;
}

export interface HttpProxyTunnelFieldsValue {
  httpProxyHost: string;
  setHttpProxyHost: (value: string) => void;
  httpProxyPort: string;
  setHttpProxyPort: (value: string) => void;
  httpProxyScheme: 'http' | 'https';
  setHttpProxyScheme: (value: 'http' | 'https') => void;
  httpProxyUsername: string;
  setHttpProxyUsername: (value: string) => void;
  httpProxyPassword: string;
  setHttpProxyPassword: (value: string) => void;
  httpProxyTimeout: string;
  setHttpProxyTimeout: (value: string) => void;
}

export interface WebSocketTunnelFieldsValue {
  wsUrl: string;
  setWsUrl: (value: string) => void;
  wsMode: 'datazen_v1' | 'raw_binary';
  setWsMode: (value: 'datazen_v1' | 'raw_binary') => void;
  wsAuthToken: string;
  setWsAuthToken: (value: string) => void;
  wsTimeout: string;
  setWsTimeout: (value: string) => void;
}

/** A `tabFill` that never fills anything, for consumers without clipboard history. */
export const noopTabFill: SshTunnelFieldsValue['tabFill'] = () => () => {};
