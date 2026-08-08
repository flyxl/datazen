import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowLeft,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  FileCode,
  Loader2,
  Save,
  Sparkles,
  Settings,
} from 'lucide-react';
import { Button } from '../ui/Button';
import { AiInput } from './AiInput';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { aiCommands } from '../../commands/ai';
import { cn } from '../../lib/cn';
import { openSettingsWindow } from '../../lib/windowManager';
import { extractWorkflowYaml, parseWorkflowYaml, validateWorkflowFields } from '../../lib/workflowYaml';
import { splitContextItems } from '../../lib/contextItems';
import type { AiChatMessage, ContextItem, WorkflowDefinition } from '../../types';
import { QuestionBlock } from './AiChatPanel';

interface WorkflowChatPanelProps {
  connections: { id: string; name: string; databaseType: string }[];
  onSaved?: () => void;
  onBack?: () => void;
}

export function WorkflowChatPanel({ connections, onSaved, onBack }: WorkflowChatPanelProps) {
  const { t } = useI18n();
  const workflowChat = useAiStore((s) => s.workflowChat);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const initChat = useAiStore((s) => s.initWorkflowChat);
  const sendMessage = useAiStore((s) => s.sendWorkflowChatMessage);
  const clearChat = useAiStore((s) => s.clearWorkflowChat);

  const [input, setInput] = useState('');
  const [selectedConnection, setSelectedConnection] = useState('');
  const [contextItems, setContextItems] = useState<ContextItem[]>([]);
  const [saving, setSaving] = useState(false);
  const [saveOk, setSaveOk] = useState(false);
  const [saveError, setSaveError] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!workflowChat) initChat();
  }, [workflowChat, initChat]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [workflowChat?.messages, workflowChat?.streamContent]);

  const handleSend = useCallback(() => {
    if (!input.trim() || workflowChat?.isStreaming) return;
    const conn = selectedConnection || undefined;
    const { contextFiles, contextTables } = splitContextItems(contextItems);
    void sendMessage({
      connectionId: conn,
      content: input.trim(),
      includeSchema: !!conn,
      contextFiles: contextFiles.length > 0 ? contextFiles : undefined,
      contextTables: contextTables.length > 0 ? contextTables : undefined,
    });
    setInput('');
    setContextItems([]);
  }, [input, workflowChat, sendMessage, selectedConnection, contextItems]);

  const handleNewChat = useCallback(() => {
    clearChat();
    initChat();
    setInput('');
    setSaveOk(false);
    setSaveError('');
  }, [clearChat, initChat]);

  const handleStop = useCallback(() => {
    const session = workflowChat;
    if (!session) return;
    const collected = session.streamContent || '';
    const reasoning = session.streamReasoning || '';
    const msgs = [...session.messages];
    if (collected || reasoning) {
      msgs.push({ role: 'assistant', content: collected, reasoning: reasoning || undefined });
    }
    useAiStore.setState({
      workflowChat: { ...session, messages: msgs, isStreaming: false, streamContent: '', streamReasoning: '', requestId: null },
    });
  }, [workflowChat]);

  const handleSaveYaml = useCallback(async (yaml: string) => {
    setSaving(true);
    setSaveError('');
    try {
      const parsed = parseWorkflowYaml(yaml);
      const missing = validateWorkflowFields(parsed);
      if (missing) {
        setSaveError(t('workflows.aiCreate.missingField', { field: missing }));
        setSaving(false);
        return;
      }
      const workflow = parsed as unknown as WorkflowDefinition;
      await aiCommands.workflowSave(workflow);
      setSaveOk(true);
      setSaving(false);
      onSaved?.();
      setTimeout(() => setSaveOk(false), 3000);
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : String(e));
      setSaving(false);
    }
  }, [t, onSaved]);

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

  const connectionOptions = [
    { value: '', label: t('workflows.aiCreate.noConnection') },
    ...connections.map((c) => ({ value: c.id, label: `${c.name} (${c.databaseType})` })),
  ];

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-3 py-1.5">
        {onBack && (
          <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={onBack} className="text-fg-muted hover:text-fg p-0.5 rounded">
            <ArrowLeft className="h-3.5 w-3.5" />
          </button>
        )}
        <Sparkles className="h-3.5 w-3.5 text-accent" />
        <span className="text-xs font-medium text-fg">{t('workflows.aiCreate.title')}</span>
        <div className="flex-1" />
        <Button variant="ghost" className="h-6 px-1.5 text-[11px]" onClick={handleNewChat}>
          {t('chat.clear')}
        </Button>
      </div>

      {/* Connection selector */}
      <div className="shrink-0 border-b border-edge px-3 py-1.5">
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-fg-muted shrink-0">{t('workflows.aiCreate.connection')}:</span>
          <Select
            value={selectedConnection}
            options={connectionOptions}
            onChange={setSelectedConnection}
            className="!h-6 !text-[11px] flex-1"
          />
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-3 py-2">
        {workflowChat?.messages.length === 0 && !workflowChat.isStreaming && (
          <div className="py-8 text-center text-xs text-fg-muted">
            {t('workflows.aiCreate.welcome')}
          </div>
        )}

        {workflowChat?.messages.map((msg, i) => (
          <WorkflowChatBubble
            key={i}
            message={msg}
            onSaveYaml={handleSaveYaml}
            saving={saving}
            saveOk={saveOk}
            saveError={saveError}
            t={t}
            onAnswerQuestions={(answers) => {
              void sendMessage({
                connectionId: selectedConnection || undefined,
                content: answers,
                includeSchema: !!selectedConnection,
              });
            }}
            isLastAssistant={
              msg.role === 'assistant' &&
              i === (workflowChat?.messages.length ?? 0) - 1 &&
              !workflowChat?.isStreaming
            }
          />
        ))}

        {workflowChat?.isStreaming && (workflowChat.streamContent || workflowChat.streamReasoning) && (
          <WorkflowChatBubble
            message={{
              role: 'assistant',
              content: workflowChat.streamContent,
              reasoning: workflowChat.streamReasoning || undefined,
            }}
            isStreaming
            t={t}
          />
        )}

        {workflowChat?.isStreaming && !workflowChat.streamContent && !workflowChat.streamReasoning && (
          <div className="flex items-center gap-2 py-2 text-xs text-fg-muted">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t('chat.thinking')}
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
          placeholder={t('workflows.aiCreate.placeholder')}
          rows={3}
          isLoading={workflowChat?.isStreaming}
          onStop={handleStop}
          connectionId={selectedConnection || undefined}
          contextItems={contextItems}
          onContextItemsChange={setContextItems}
        />
      </div>
    </div>
  );
}

function WorkflowChatBubble({
  message,
  isStreaming,
  onSaveYaml,
  saving,
  saveOk,
  saveError,
  t,
  onAnswerQuestions,
  isLastAssistant,
}: {
  message: AiChatMessage;
  isStreaming?: boolean;
  onSaveYaml?: (yaml: string) => void;
  saving?: boolean;
  saveOk?: boolean;
  saveError?: string;
  t: ReturnType<typeof useI18n>['t'];
  onAnswerQuestions?: (formatted: string) => void;
  isLastAssistant?: boolean;
}) {
  const isUser = message.role === 'user';
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);
  const [reasoningOpen, setReasoningOpen] = useState(false);
  const [previewYaml, setPreviewYaml] = useState<string | null>(null);

  const yamlBlocks = !isUser && !isStreaming ? extractWorkflowYaml(message.content) : [];

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
              type="button" onMouseDown={(e) => e.preventDefault()}
              className="flex items-center gap-1 text-[10px] text-fg-muted hover:text-fg transition-colors"
              onClick={() => setReasoningOpen(!reasoningOpen)}
            >
              {reasoningOpen ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
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
          <pre className={cn('whitespace-pre-wrap font-sans', isStreaming && 'animate-pulse')}>
            {message.content}
          </pre>
        )}

        {!message.content && isStreaming && message.reasoning && (
          <div className="flex items-center gap-1 text-[10px] text-fg-muted mt-1">
            <Loader2 className="h-3 w-3 animate-spin" />
          </div>
        )}

        {/* YAML actions */}
        {yamlBlocks.length > 0 && !isStreaming && (
          <div className="mt-2 space-y-2">
            {yamlBlocks.map((yaml, idx) => (
              <div key={idx} className="rounded border border-accent/30 bg-accent/5 p-2">
                <div className="flex items-center gap-1.5 mb-1">
                  <FileCode className="h-3 w-3 text-accent" />
                  <span className="text-[10px] font-medium text-accent">{t('workflows.aiCreate.yamlDetected')}</span>
                </div>
                <div className="flex gap-1">
                  <button
                    type="button" onMouseDown={(e) => e.preventDefault()}
                    className="rounded bg-surface px-1.5 py-0.5 text-[10px] text-fg-muted hover:text-fg"
                    onClick={() => handleCopy(yaml, idx)}
                  >
                    {copiedIdx === idx ? <Check className="inline h-2.5 w-2.5" /> : <Copy className="inline h-2.5 w-2.5" />}
                  </button>
                  <button
                    type="button" onMouseDown={(e) => e.preventDefault()}
                    className="rounded bg-surface px-1.5 py-0.5 text-[10px] text-fg-muted hover:text-fg"
                    onClick={() => setPreviewYaml(previewYaml === yaml ? null : yaml)}
                  >
                    {t('workflows.aiCreate.preview')}
                  </button>
                  {onSaveYaml && (
                    <button
                      type="button" onMouseDown={(e) => e.preventDefault()}
                      className={cn(
                        'flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px]',
                        saveOk ? 'text-green-500' : 'text-accent hover:text-accent/80',
                      )}
                      onClick={() => onSaveYaml(yaml)}
                      disabled={saving}
                    >
                      {saving ? (
                        <Loader2 className="inline h-2.5 w-2.5 animate-spin" />
                      ) : saveOk ? (
                        <Check className="inline h-2.5 w-2.5" />
                      ) : (
                        <Save className="inline h-2.5 w-2.5" />
                      )}
                      {saving ? t('workflows.aiCreate.saving') : saveOk ? t('workflows.aiCreate.saved') : t('workflows.aiCreate.save')}
                    </button>
                  )}
                </div>
                {previewYaml === yaml && (
                  <pre className="mt-2 rounded bg-surface p-2 text-[11px] font-mono text-fg-secondary max-h-60 overflow-auto whitespace-pre-wrap">
                    {yaml}
                  </pre>
                )}
                {saveError && (
                  <p className="mt-1 text-[10px] text-red-400">{saveError}</p>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {!isUser && !isStreaming && message.questions && message.questions.length > 0 && isLastAssistant && onAnswerQuestions && (
        <QuestionBlock
          questions={message.questions}
          onSubmit={onAnswerQuestions}
        />
      )}
    </div>
  );
}
