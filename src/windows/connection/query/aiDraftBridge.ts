/**
 * Bridge contracts for the AI Chat Draft feature (Track S3-B2).
 *
 * ContentView acts as the sole coordinator that owns the pending draft.
 * Callbacks flow: ContentView → PanelContentRenderer → QueryPanel → (deep).
 * AiChatPanel receives the draft via props and calls `onDraftConsumed` after
 * the textarea is written.
 *
 * Rules:
 * - No message is sent, no history is cleared, no second chat store is created.
 * - AI-not-configured pages never consume the draft.
 * - Streaming: draft can be prefilled but must NOT be sent.
 * - Conflict (existing non-empty textarea): non-modal replace/append/cancel choice.
 */

import type { TableContextInput, TableSqlActionSpec } from '../../../lib/tableSqlActions';

export type AiChatDraftSource = 'query-error';

export interface AiChatDraftRequest {
  /** Unique id for the draft (crypto.randomUUID). */
  requestId: string;
  source: AiChatDraftSource;
  panelId: string;
  connectionId: string;
  dbSessionId: string;
  database?: string;
  schema?: string;
  contextFingerprint: string;
  /** The pre-built prompt / context content to fill into the chat input. */
  content: string;
  /** Always `true` for now; signals AiChatPanel should focus the input. */
  focus: true;
}

/**
 * Callbacks that deep components (e.g. QueryErrorPanel) may invoke to request
 * navigation or an AI chat draft. ContentView owns the implementations.
 */
export interface ContentViewCallbacks {
  /** Open a relation (table/view) data or structure tab. */
  openRelation: (
    name: string,
    schema: string | null,
    database: string,
    subTab?: 'data' | 'structure' | 'ddl',
    targetColumn?: string,
  ) => void;
  /** Request an AI chat draft to be prefilled into the AiChatPanel. */
  openAiChatDraft: (request: AiChatDraftRequest) => void;
  /**
   * Open a query tab with the SQL draft of a table action (DB-16's
   * "Edit in SQL" uses it to hand the no-primary-key `SELECT … WHERE`
   * template to the editor). Absent on mounts that cannot open one.
   */
  openSqlEditorForTable?: (input: TableContextInput, spec: TableSqlActionSpec) => void;
}
