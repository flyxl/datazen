import { useCallback, useEffect, useRef, useState } from 'react';
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  Loader2,
  MessageSquare,
  Settings,
  Sparkles,
  Trash2,
  Wand2,
  Send,
} from 'lucide-react';
import { Button } from '../ui/Button';
import { AiInput } from './AiInput';
import { AiMessageContent } from './AiMessageContent';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';
import { openDocsWindow, openSettingsWindow } from '../../lib/windowManager';
import { WorkflowPanel } from './WorkflowPanel';
import { splitContextItems } from '../../lib/contextItems';
import type { AiChatMessage, AiQuestion, ContextItem } from '../../types';

interface AiChatPanelProps {
  dbSessionId: string;
  database?: string;
  sqlDialect?: string;
  onInsertSql?: (sql: string) => void;
}

function formatMcpToolDisplayName(qualifiedName: string): string {
  const parts = qualifiedName.split('/');
  return parts.length >= 3 ? parts.slice(2).join('/') : qualifiedName;
}

export function AiChatPanel({ dbSessionId, database, sqlDialect, onInsertSql }: AiChatPanelProps) {
  const { t } = useI18n();
  const chatSession = useAiStore((s) => s.chatSession);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const initChat = useAiStore((s) => s.initChatSession);
  const sendMessage = useAiStore((s) => s.sendChatMessage);
  const clearChat = useAiStore((s) => s.clearChat);

  const [input, setInput] = useState('');
  const [tab, setTab] = useState<'chat' | 'workflows'>('chat');
  const [contextItems, setContextItems] = useState<ContextItem[]>([]);
  const messagesEndRef = useRef<HTMLDivElement>(null);

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
    const { contextFiles, contextTables } = splitContextItems(contextItems);
    void sendMessage({
      dbSessionId,
      database,
      content: input.trim(),
      contextFiles: contextFiles.length > 0 ? contextFiles : undefined,
      contextTables: contextTables.length > 0 ? contextTables : undefined,
    });
    setInput('');
    setContextItems([]);
  }, [input, chatSession, sendMessage, dbSessionId, database, contextItems]);

  if (!isConfigured) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <div className="text-center text-xs text-fg-muted">
          <Sparkles className="mx-auto mb-2 h-5 w-5" />
          <p className="mb-3">{t('common.aiNotConfigured')}</p>
          <Button
            variant="primary"
            className="h-7 gap-1 px-3 text-xs"
            onClick={() => openSettingsWindow('ai')}
          >
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
            onMouseDown={(e) => e.preventDefault()}
            className={cn(
              'flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors',
              tab === 'chat'
                ? 'bg-accent/10 text-accent font-medium'
                : 'text-fg-muted hover:text-fg',
            )}
            onClick={() => setTab('chat')}
          >
            <MessageSquare className="h-3 w-3" />
            {t('common.aiAssistant')}
          </button>
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            className={cn(
              'flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors',
              tab === 'workflows'
                ? 'bg-accent/10 text-accent font-medium'
                : 'text-fg-muted hover:text-fg',
            )}
            onClick={() => setTab('workflows')}
          >
            <Wand2 className="h-3 w-3" />
            {t('workflows.title')}
          </button>
        </div>
        <div className="flex items-center gap-0.5">
          {tab === 'chat' && (
            <>
              <Button
                variant="ghost"
                className="h-6 px-1.5 text-[11px]"
                title={t('docs.openAiHelp')}
                onClick={() => openDocsWindow('context')}
              >
                <BookOpen className="h-3 w-3" />
              </Button>
              <Button
                variant="ghost"
                className="h-6 px-1.5 text-[11px]"
                onClick={clearChat}
                disabled={!chatSession?.messages.length}
              >
                <Trash2 className="h-3 w-3" />
              </Button>
            </>
          )}
          {tab === 'workflows' && (
            <Button
              variant="ghost"
              className="h-6 px-1.5 text-[11px]"
              title={t('docs.openWorkflowHelp')}
              onClick={() => openDocsWindow('workflows')}
            >
              <BookOpen className="h-3 w-3" />
            </Button>
          )}
        </div>
      </div>

      {tab === 'workflows' ? (
        <div className="flex-1 overflow-y-auto px-3 py-2">
          <WorkflowPanel dbSessionId={dbSessionId} />
        </div>
      ) : (
        <>
          {/* Messages */}
          <div className="flex-1 overflow-y-auto px-3 py-2">
            {chatSession?.messages.length === 0 && !chatSession.isStreaming && (
              <div className="py-8 text-center text-xs text-fg-muted">{t('chat.welcome')}</div>
            )}

            {chatSession?.messages.map((msg, i) => (
              <ChatBubble
                key={i}
                message={msg}
                sqlDialect={sqlDialect}
                onInsertSql={onInsertSql}
                onAnswerQuestions={(answers) => {
                  void sendMessage({ dbSessionId, database, content: answers });
                }}
                isLastAssistant={
                  msg.role === 'assistant' &&
                  i === chatSession.messages.length - 1 &&
                  !chatSession.isStreaming
                }
              />
            ))}

            {chatSession?.isStreaming &&
              (chatSession.streamContent || chatSession.streamReasoning) && (
                <ChatBubble
                  message={{
                    role: 'assistant',
                    content: chatSession.streamContent,
                    reasoning: chatSession.streamReasoning || undefined,
                  }}
                  sqlDialect={sqlDialect}
                  isStreaming
                />
              )}

            {chatSession?.isStreaming && !chatSession.streamContent && (
              <div className="flex items-center gap-2 py-2 text-xs text-fg-muted">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {chatSession.streamMcpToolName
                  ? t('chat.callingMcpTool', {
                      name: formatMcpToolDisplayName(chatSession.streamMcpToolName),
                    })
                  : t('chat.thinking')}
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>

          {/* Input */}
          <div className="shrink-0 border-t border-edge p-2">
            <AiInput
              value={input}
              onChange={setInput}
              onSubmit={handleSend}
              placeholder={t('chat.placeholder')}
              disabled={chatSession?.isStreaming}
              isLoading={chatSession?.isStreaming}
              dbSessionId={dbSessionId}
              database={database}
              contextItems={contextItems}
              onContextItemsChange={setContextItems}
            />
          </div>
        </>
      )}
    </div>
  );
}

function ChatBubble({
  message,
  isStreaming,
  sqlDialect,
  onInsertSql,
  onAnswerQuestions,
  isLastAssistant,
}: {
  message: AiChatMessage;
  isStreaming?: boolean;
  sqlDialect?: string;
  onInsertSql?: (sql: string) => void;
  onAnswerQuestions?: (formatted: string) => void;
  isLastAssistant?: boolean;
}) {
  const { t } = useI18n();
  const isUser = message.role === 'user';
  const [reasoningOpen, setReasoningOpen] = useState(false);

  const hasQuestions = !isUser && !isStreaming && message.questions && message.questions.length > 0;

  return (
    <div className={cn('mb-3', isUser ? 'flex justify-end' : '')}>
      <div
        className={cn(
          'max-w-[90%] rounded-lg px-3 py-2 text-xs',
          isUser ? 'bg-blue-500/20 text-fg' : 'bg-surface-alt text-fg-secondary',
          isStreaming && !message.content && message.reasoning && 'animate-pulse',
        )}
      >
        {message.reasoning && (
          <div className="mb-2">
            <button
              type="button"
              onMouseDown={(e) => e.preventDefault()}
              className="flex items-center gap-1 text-[10px] text-fg-muted hover:text-fg transition-colors"
              onClick={() => setReasoningOpen(!reasoningOpen)}
            >
              {reasoningOpen ? (
                <ChevronDown className="h-3 w-3" />
              ) : (
                <ChevronRight className="h-3 w-3" />
              )}
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

        {message.content &&
          (isUser ? (
            <pre className="whitespace-pre-wrap font-sans">{message.content}</pre>
          ) : (
            <AiMessageContent
              content={message.content}
              sqlDialect={sqlDialect}
              onInsertSql={onInsertSql}
              isStreaming={isStreaming}
            />
          ))}

        {!message.content && isStreaming && message.reasoning && (
          <div className="flex items-center gap-1 text-[10px] text-fg-muted mt-1">
            <Loader2 className="h-3 w-3 animate-spin" />
          </div>
        )}
      </div>

      {hasQuestions && isLastAssistant && onAnswerQuestions && (
        <QuestionBlock questions={message.questions!} onSubmit={onAnswerQuestions} />
      )}
    </div>
  );
}

export function QuestionBlock({
  questions,
  onSubmit,
}: {
  questions: AiQuestion[];
  onSubmit: (formatted: string) => void;
}) {
  const { t } = useI18n();
  const [answers, setAnswers] = useState<Record<string, string | string[]>>(() => {
    const initial: Record<string, string | string[]> = {};
    for (const q of questions) {
      initial[q.id] = q.allowMultiple ? [] : '';
    }
    return initial;
  });
  const [submitted, setSubmitted] = useState(false);

  const toggleOption = useCallback(
    (questionId: string, optionId: string, allowMultiple?: boolean) => {
      setAnswers((prev) => {
        if (allowMultiple) {
          const current = (prev[questionId] as string[]) || [];
          const next = current.includes(optionId)
            ? current.filter((id) => id !== optionId)
            : [...current, optionId];
          return { ...prev, [questionId]: next };
        }
        return { ...prev, [questionId]: prev[questionId] === optionId ? '' : optionId };
      });
    },
    [],
  );

  const setTextAnswer = useCallback((questionId: string, text: string) => {
    setAnswers((prev) => ({ ...prev, [questionId]: text }));
  }, []);

  const handleSubmit = useCallback(() => {
    const lines: string[] = [];
    for (const q of questions) {
      const answer = answers[q.id];
      if (q.allowMultiple && Array.isArray(answer) && answer.length > 0) {
        const labels = answer.map((aid) => {
          const opt = q.options.find((o) => o.id === aid);
          return opt ? opt.label : aid;
        });
        lines.push(`${q.prompt}\n${labels.join(', ')}`);
      } else if (typeof answer === 'string' && answer.trim()) {
        const opt = q.options.find((o) => o.id === answer);
        lines.push(`${q.prompt}\n${opt ? opt.label : answer}`);
      }
    }
    if (lines.length > 0) {
      setSubmitted(true);
      onSubmit(lines.join('\n\n'));
    }
  }, [questions, answers, onSubmit]);

  if (submitted) return null;

  return (
    <div className="mt-2 space-y-3 rounded-lg border border-accent/20 bg-accent/5 px-3 py-2.5">
      {questions.map((q) => (
        <div key={q.id}>
          <p className="text-xs font-medium text-fg mb-1.5">{q.prompt}</p>

          {q.options.length > 0 && (
            <div className="flex flex-wrap gap-1.5 mb-1.5">
              {q.options.map((opt) => {
                const selected = q.allowMultiple
                  ? ((answers[q.id] as string[]) || []).includes(opt.id)
                  : answers[q.id] === opt.id;
                return (
                  <button
                    key={opt.id}
                    type="button"
                    onMouseDown={(e) => e.preventDefault()}
                    className={cn(
                      'rounded-md border px-2.5 py-1 text-[11px] transition-colors',
                      selected
                        ? 'border-accent bg-accent/15 text-accent font-medium'
                        : 'border-edge bg-surface text-fg-muted hover:text-fg hover:border-fg-muted',
                    )}
                    onClick={() => toggleOption(q.id, opt.id, q.allowMultiple)}
                  >
                    {opt.label}
                  </button>
                );
              })}
            </div>
          )}

          <input
            type="text"
            className="w-full h-7 rounded border border-edge bg-surface px-2 text-xs text-fg outline-none focus:border-accent placeholder:text-fg-muted/50"
            placeholder={t('chat.questions.customAnswer')}
            value={
              typeof answers[q.id] === 'string' && !q.options.find((o) => o.id === answers[q.id])
                ? (answers[q.id] as string)
                : ''
            }
            onChange={(e) => setTextAnswer(q.id, e.target.value)}
          />
        </div>
      ))}

      <Button variant="primary" className="h-7 text-xs gap-1" onClick={handleSubmit}>
        <Send className="h-3 w-3" />
        {t('chat.questions.submit')}
      </Button>
    </div>
  );
}
