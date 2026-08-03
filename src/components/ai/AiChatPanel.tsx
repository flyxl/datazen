import { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Loader2, MessageSquare, Send, Settings, Sparkles, Trash2, Copy, Check, Wand2 } from 'lucide-react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';
import { openSettingsWindow } from '../../lib/windowManager';
import { WorkflowPanel } from './WorkflowPanel';
import type { AiChatMessage } from '../../types';

interface AiChatPanelProps {
  connectionId: string;
  database?: string;
  onInsertSql?: (sql: string) => void;
}

export function AiChatPanel({ connectionId, database, onInsertSql }: AiChatPanelProps) {
  const { t } = useI18n();
  const chatSession = useAiStore((s) => s.chatSession);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const initChat = useAiStore((s) => s.initChatSession);
  const sendMessage = useAiStore((s) => s.sendChatMessage);
  const clearChat = useAiStore((s) => s.clearChat);

  const [input, setInput] = useState('');
  const [tab, setTab] = useState<'chat' | 'workflows'>('chat');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!chatSession) {
      initChat();
    }
  }, [chatSession, initChat]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [chatSession?.messages, chatSession?.streamContent]);

  const handleSend = useCallback(() => {
    if (!input.trim() || chatSession?.isStreaming) return;
    void sendMessage({ connectionId, database, content: input.trim() });
    setInput('');
  }, [input, chatSession, sendMessage, connectionId]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  if (!isConfigured) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <div className="text-center text-xs text-fg-muted">
          <Sparkles className="mx-auto mb-2 h-5 w-5" />
          <p className="mb-3">{t('chat.notConfigured')}</p>
          <Button variant="primary" className="h-7 gap-1 px-3 text-xs" onClick={() => openSettingsWindow('ai')}>
            <Settings className="h-3.5 w-3.5" />
            {t('settings.ai.goToConfigure')}
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Header with tabs */}
      <div className="flex shrink-0 items-center justify-between border-b border-edge px-3 py-1.5">
        <div className="flex items-center gap-1">
          <button
            type="button"
            className={cn(
              'flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors',
              tab === 'chat' ? 'bg-accent/10 text-accent font-medium' : 'text-fg-muted hover:text-fg',
            )}
            onClick={() => setTab('chat')}
          >
            <MessageSquare className="h-3 w-3" />
            {t('chat.title')}
          </button>
          <button
            type="button"
            className={cn(
              'flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors',
              tab === 'workflows' ? 'bg-accent/10 text-accent font-medium' : 'text-fg-muted hover:text-fg',
            )}
            onClick={() => setTab('workflows')}
          >
            <Wand2 className="h-3 w-3" />
            {t('workflows.title')}
          </button>
        </div>
        {tab === 'chat' && (
          <Button
            variant="ghost"
            className="h-6 px-1.5 text-[11px]"
            onClick={clearChat}
            disabled={!chatSession?.messages.length}
          >
            <Trash2 className="h-3 w-3" />
          </Button>
        )}
      </div>

      {tab === 'workflows' ? (
        <div className="flex-1 overflow-y-auto px-3 py-2">
          <WorkflowPanel connectionId={connectionId} />
        </div>
      ) : (
        <>
          {/* Messages */}
          <div className="flex-1 overflow-y-auto px-3 py-2">
            {chatSession?.messages.length === 0 && !chatSession.isStreaming && (
              <div className="py-8 text-center text-xs text-fg-muted">
                {t('chat.welcome')}
              </div>
            )}

            {chatSession?.messages.map((msg, i) => (
              <ChatBubble key={i} message={msg} onInsertSql={onInsertSql} />
            ))}

            {chatSession?.isStreaming && (chatSession.streamContent || chatSession.streamReasoning) && (
              <ChatBubble
                message={{
                  role: 'assistant',
                  content: chatSession.streamContent,
                  reasoning: chatSession.streamReasoning || undefined,
                }}
                isStreaming
              />
            )}

            {chatSession?.isStreaming && !chatSession.streamContent && (
              <div className="flex items-center gap-2 py-2 text-xs text-fg-muted">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t('chat.thinking')}
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>

          {/* Input */}
          <div className="shrink-0 border-t border-edge p-2">
            <div className="flex gap-1.5">
              <textarea
                ref={inputRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={t('chat.placeholder')}
                rows={1}
                disabled={chatSession?.isStreaming}
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                className={cn(
                  'flex-1 resize-none rounded border border-edge bg-surface px-2 py-1.5',
                  'text-sm text-fg placeholder:text-fg-muted',
                  'focus:border-accent focus:outline-none',
                  'disabled:opacity-50',
                )}
              />
              <Button
                variant="primary"
                className="h-8 shrink-0 px-2"
                disabled={!input.trim() || chatSession?.isStreaming}
                onClick={handleSend}
              >
                <Send className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function ChatBubble({
  message,
  isStreaming,
  onInsertSql,
}: {
  message: AiChatMessage;
  isStreaming?: boolean;
  onInsertSql?: (sql: string) => void;
}) {
  const { t } = useI18n();
  const isUser = message.role === 'user';
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);
  const [reasoningOpen, setReasoningOpen] = useState(false);

  const codeBlocks = !isUser
    ? extractCodeBlocks(message.content)
    : [];

  const handleCopy = useCallback((code: string, idx: number) => {
    void navigator.clipboard.writeText(code);
    setCopiedIdx(idx);
    setTimeout(() => setCopiedIdx(null), 1500);
  }, []);

  return (
    <div className={cn('mb-3', isUser ? 'flex justify-end' : '')}>
      <div
        className={cn(
          'max-w-[90%] rounded-lg px-3 py-2 text-xs',
          isUser
            ? 'bg-blue-500/20 text-fg'
            : 'bg-surface-alt text-fg-secondary',
          isStreaming && !message.content && message.reasoning && 'animate-pulse',
        )}
      >
        {message.reasoning && (
          <div className="mb-2">
            <button
              type="button"
              className="flex items-center gap-1 text-[10px] text-fg-muted hover:text-fg transition-colors"
              onClick={() => setReasoningOpen(!reasoningOpen)}
            >
              {reasoningOpen
                ? <ChevronDown className="h-3 w-3" />
                : <ChevronRight className="h-3 w-3" />
              }
              <Sparkles className="h-2.5 w-2.5" />
              {t('chat.reasoning')}
            </button>
            {reasoningOpen && (
              <pre className="mt-1 whitespace-pre-wrap font-sans text-[11px] text-fg-muted border-l-2 border-fg-muted/20 pl-2 ml-1">
                {message.reasoning}
              </pre>
            )}
          </div>
        )}

        {message.content && (
          <pre className={cn('whitespace-pre-wrap font-sans', isStreaming && 'animate-pulse')}>{message.content}</pre>
        )}

        {!message.content && isStreaming && message.reasoning && (
          <div className="flex items-center gap-1 text-[10px] text-fg-muted mt-1">
            <Loader2 className="h-3 w-3 animate-spin" />
          </div>
        )}

        {codeBlocks.length > 0 && !isStreaming && (
          <div className="mt-2 flex flex-wrap gap-1">
            {codeBlocks.map((block, idx) => (
              <span key={idx} className="flex gap-0.5">
                <button
                  type="button"
                  className="rounded bg-surface px-1.5 py-0.5 text-[10px] text-fg-muted hover:text-fg"
                  onClick={() => handleCopy(block, idx)}
                >
                  {copiedIdx === idx ? (
                    <Check className="inline h-2.5 w-2.5" />
                  ) : (
                    <Copy className="inline h-2.5 w-2.5" />
                  )}
                </button>
                {onInsertSql && isSqlLike(block) && (
                  <button
                    type="button"
                    className="rounded bg-surface px-1.5 py-0.5 text-[10px] text-blue-400 hover:text-blue-300"
                    onClick={() => onInsertSql(block)}
                  >
                    {t('chat.insertSql')}
                  </button>
                )}
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function extractCodeBlocks(text: string): string[] {
  const regex = /```[\w]*\n([\s\S]*?)```/g;
  const blocks: string[] = [];
  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    blocks.push(match[1].trim());
  }
  return blocks;
}

function isSqlLike(code: string): boolean {
  const sqlKeywords = /^\s*(SELECT|INSERT|UPDATE|DELETE|CREATE|ALTER|DROP|WITH|EXPLAIN)\b/i;
  return sqlKeywords.test(code);
}
