import type { SslMode } from '../types';

/** Parsed clipboard payload that the new-connection form can apply. */
export interface ConnectionClipboardFill {
  host?: string;
  port?: string;
  database?: string;
  username?: string;
  password?: string;
  schema?: string;
  name?: string;
  sslMode?: SslMode;
  options?: Record<string, unknown>;
  /** Expand Advanced when TLS / plugin advanced fields were filled. */
  expandAdvanced?: boolean;
}

export type ConnectionClipboardParser = (text: string) => ConnectionClipboardFill | null;

export interface MatchedConnectionClipboard {
  databaseType: string;
  fill: ConnectionClipboardFill;
}
