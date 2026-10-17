/**
 * Streaming, cancellation and attribution wiring for the AI store.
 *
 * Extracted from `stores/aiStore.ts` (which must stay reviewable and under
 * the repo's single-file size guidance) without changing the store's public
 * surface. The pure lifecycle rules live in `src/ai/streamRegistry` and
 * `src/ai/sessionCancel`; this module is only the Zustand adapter.
 */

import type { AiChatMessage, StreamChunkPayload, StreamErrorPayload } from '../../types';
import { extractQuestions, parseToolCallQuestions } from '../../lib/extractQuestions';
import { extractSqlFromResponse, extractSqlStreaming } from '../../lib/extractSql';
import { StreamRegistry } from '../../ai/streamRegistry';
import { SessionCanceller } from '../../ai/sessionCancel';
import { attributionOf, type AiTargetClaim, type CancelReceipt } from '../../ai/targetClaim';
import { aiCommands } from '../../commands/ai';
import { loadSessions, saveSessions, touchSession } from './sessions';
import type { AiStore } from './types';

type GetState = () => AiStore;
type SetState = (partial: Partial<AiStore>) => void;

export function findMcpToolName(toolCalls?: { name: string }[]): string | null {
  const mcpCall = toolCalls?.find((tc) => tc.name.startsWith('mcp/'));
  return mcpCall?.name ?? null;
}

// ── Session routing helpers (module-level, not in store) ──

/** Maps each active requestId to the session key it belongs to. */
const requestIdToKey: Record<string, string> = {};

export function registerRequestId(requestId: string, key: string): void {
  requestIdToKey[requestId] = key;
}

export function unregisterRequestId(requestId: string): string | undefined {
  const key = requestIdToKey[requestId];
  delete requestIdToKey[requestId];
  return key;
}

const streamRegistry = new StreamRegistry();
let sessionCanceller: SessionCanceller | null = null;

export function getStreamRegistry(): StreamRegistry {
  return streamRegistry;
}

function cancellerFor(get: GetState, set: SetState): SessionCanceller {
  if (!sessionCanceller) {
    sessionCanceller = new SessionCanceller({
      registry: streamRegistry,
      cancel: (requestId) => aiCommands.cancel(requestId),
      onCancelTimeout: (requestId) => handleCancelTimeout(get, set, requestId),
    });
  }
  return sessionCanceller;
}

/** Attribution metadata recorded with the session when a run starts. */
export function attributionFor(targetClaim?: AiTargetClaim) {
  return attributionOf(targetClaim);
}

/**
 * Ask the host to cancel the in-flight chat run.
 *
 * The receipt drives the UI: `marked` leaves the stream registry in
 * `cancelling` (the terminal chunk or the 8s fallback finishes it),
 * `alreadyFinished` needs no transition, and `forbidden` only warns.
 */
export async function cancelChat(
  get: GetState,
  set: SetState,
  requestId?: string,
): Promise<CancelReceipt | null> {
  const rid = requestId ?? get().chatSession?.requestId ?? null;
  if (!rid) return null;
  return cancellerFor(get, set).cancel(rid);
}

/**
 * 8s fallback: the backend never delivered its terminal chunk. Finalise the
 * visible session locally as cancelled so the stop button cannot spin forever.
 */
export function handleCancelTimeout(
  get: GetState,
  set: SetState,
  requestId: string,
): void {
  const { chatSession, workflowChat } = get();
  if (chatSession?.requestId === requestId) {
    set({ chatSession: finalizeCancelledSession(chatSession) });
    unregisterRequestId(requestId);
    return;
  }
  if (workflowChat?.requestId === requestId) {
    set({ workflowChat: finalizeCancelledSession(workflowChat) });
    unregisterRequestId(requestId);
  }
}

function finalizeCancelledSession<T extends NonNullable<AiStore['chatSession']>>(session: T): T {
  // The marker is the point: even a run cancelled before its first token gets
  // an assistant message, so the UI can say "Generation stopped." instead of
  // silently returning to the input with no trace of the cancelled run.
  const next: T = {
    ...session,
    isStreaming: false,
    streamContent: '',
    streamReasoning: '',
    streamMcpToolName: null,
    requestId: null,
    messages: [
      ...session.messages,
      {
        id: crypto.randomUUID(),
        role: 'assistant' as const,
        content: session.streamContent || '',
        reasoning: session.streamReasoning || undefined,
        cancelled: true,
      },
    ],
  };
  return next;
}

/** Apply a stream chunk, dropping late/duplicate chunks through the registry. */
export function handleStreamChunk(
  get: GetState,
  set: SetState,
  payload: StreamChunkPayload,
): void {
  const { nl2sql, chatSession, workflowChat } = get();

  // ── NL2SQL routing (not part of the chat lifecycle registry) ──
  if (payload.requestId === nl2sql.requestId) {
    const accumulated = payload.content
      ? nl2sql.streamingSql + payload.content
      : nl2sql.streamingSql;
    const streamingPreview = extractSqlStreaming(accumulated);
    set({
      nl2sql: {
        ...nl2sql,
        streamingSql: accumulated,
        streamingPreview,
        generatedSql: payload.done ? extractSqlFromResponse(accumulated) : nl2sql.generatedSql,
        isGenerating: payload.done ? false : nl2sql.isGenerating,
      },
    });
    if (payload.done) {
      unregisterRequestId(payload.requestId);
    }
    return;
  }

  const targetSession =
    chatSession && payload.requestId === chatSession.requestId
      ? 'chatSession'
      : workflowChat && payload.requestId === workflowChat.requestId
        ? 'workflowChat'
        : null;

  if (!targetSession) return;

  // Generation / cancel / terminal gates. A dropped chunk must not touch the
  // rendered text; the terminal chunk itself is still accepted at most once.
  if (streamRegistry.has(payload.requestId)) {
    const applied = streamRegistry.applyChunk(payload.requestId, {
      done: payload.done,
      cancelled: payload.cancelled,
    });
    if (!applied.accepted) return;
  }

  const session = (targetSession === 'chatSession' ? chatSession : workflowChat)!;
  const newContent = (session.streamContent || '') + (payload.content || '');
  const newReasoning = (session.streamReasoning || '') + (payload.reasoning || '');
  const mcpToolName = findMcpToolName(payload.toolCalls) ?? session.streamMcpToolName ?? null;
  if (payload.done) {
    unregisterRequestId(payload.requestId);
    const { cleanContent, questions: xmlQuestions } = extractQuestions(newContent);
    const toolCalls =
      payload.toolCalls && payload.toolCalls.length > 0 ? payload.toolCalls : undefined;

    let questions = xmlQuestions.length > 0 ? xmlQuestions : undefined;
    if (!questions && toolCalls) {
      const tcQuestions = parseToolCallQuestions(toolCalls);
      if (tcQuestions.length > 0) questions = tcQuestions;
    }

    const assistantMessage: AiChatMessage = {
      role: 'assistant',
      content: cleanContent,
      reasoning: newReasoning || undefined,
      questions,
      toolCalls,
      cancelled: payload.cancelled === true,
    };
    const nextSession = {
      ...session,
      messages: [...session.messages, assistantMessage],
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      streamMcpToolName: null,
      requestId: null,
    };
    set({ [targetSession]: nextSession });

    // Persist chat history to localStorage
    if (targetSession === 'chatSession') {
      const key = nextSession.sessionKey;
      const sessions = loadSessions();
      const updated = touchSession(sessions, key);
      updated[key] = {
        ...updated[key],
        messages: nextSession.messages.map((m) => ({
          role: m.role as 'user' | 'assistant' | 'tool' | 'system',
          content: m.content,
          reasoning: m.reasoning,
          toolCalls: m.toolCalls,
          toolCallId: m.toolCallId,
          cancelled: m.cancelled,
        })),
        lastAccess: Date.now(),
      };
      saveSessions(updated);
    }
  } else {
    set({
      [targetSession]: {
        ...session,
        streamContent: newContent,
        streamReasoning: newReasoning,
        streamMcpToolName: mcpToolName,
      },
    });
  }
}

/** Out-of-band stream error: terminal once, idempotent on repeats. */
export function handleStreamError(
  get: GetState,
  set: SetState,
  payload: StreamErrorPayload,
): void {
  const { nl2sql, chatSession, workflowChat } = get();
  if (payload.requestId === nl2sql.requestId) {
    unregisterRequestId(payload.requestId);
    set({
      nl2sql: { ...nl2sql, isGenerating: false },
      nl2sqlError: payload.error,
    });
  }

  const errTarget =
    chatSession && payload.requestId === chatSession.requestId
      ? 'chatSession'
      : workflowChat && payload.requestId === workflowChat.requestId
        ? 'workflowChat'
        : null;
  if (!errTarget) return;

  if (streamRegistry.has(payload.requestId)) {
    const applied = streamRegistry.applyError(payload.requestId);
    if (!applied.accepted) return;
  }

  unregisterRequestId(payload.requestId);
  const session = (errTarget === 'chatSession' ? chatSession : workflowChat)!;
  set({
    [errTarget]: {
      ...session,
      isStreaming: false,
      streamContent: '',
      streamReasoning: '',
      streamMcpToolName: null,
      requestId: null,
      messages: [...session.messages, { role: 'assistant', content: `Error: ${payload.error}` }],
    },
  });
}
