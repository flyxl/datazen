import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { Loader2, Radio, Send, X } from 'lucide-react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Button } from '../../../../src/components/ui/Button';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { redisCommandInvoke } from './redisInvoke';

export interface PubSubPanelProps {
  connectionId: string;
}

interface PubSubMessage {
  id: string;
  ts: number;
  channel: string;
  payload: string;
  subscriptionId: string;
}

interface ActiveSubscription {
  id: string;
  channels: string[];
  patterns: string[];
}

interface PubSubMessageEvent {
  connectionId: string;
  subscriptionId: string;
  channel: string;
  payload: string;
  ts: number;
}

const MAX_MESSAGES = 500;

function parseNameList(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString();
}

export function PubSubPanel({ connectionId }: PubSubPanelProps) {
  const { t } = useI18n();
  const [channelsInput, setChannelsInput] = useState('');
  const [patternsInput, setPatternsInput] = useState('');
  const [publishChannel, setPublishChannel] = useState('');
  const [publishMessage, setPublishMessage] = useState('');
  const [subscriptions, setSubscriptions] = useState<ActiveSubscription[]>([]);
  const [messages, setMessages] = useState<PubSubMessage[]>([]);
  const [subscribing, setSubscribing] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastReceivers, setLastReceivers] = useState<number | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messageSeq = useRef(0);
  const subscriptionsRef = useRef(subscriptions);
  subscriptionsRef.current = subscriptions;

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [messages, scrollToBottom]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    void (async () => {
      try {
        unlisten = await listen<PubSubMessageEvent>('redis-pubsub-message', (event) => {
          const payload = event.payload;
          if (payload.connectionId !== connectionId) return;
          messageSeq.current += 1;
          setMessages((prev) => {
            const next = [
              ...prev,
              {
                id: `${payload.ts}-${messageSeq.current}`,
                ts: payload.ts,
                channel: payload.channel,
                payload: payload.payload,
                subscriptionId: payload.subscriptionId,
              },
            ];
            return next.length > MAX_MESSAGES ? next.slice(-MAX_MESSAGES) : next;
          });
        });
        if (cancelled) unlisten();
      } catch {
        // Not in Tauri runtime (e.g. unit test env)
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [connectionId]);

  useEffect(() => {
    return () => {
      for (const sub of subscriptionsRef.current) {
        void redisCommandInvoke('redis', 'pubsub_unsubscribe', {
          connectionId,
          subscriptionId: sub.id,
        }).catch(() => {});
      }
    };
  }, [connectionId]);

  const handleSubscribe = useCallback(async () => {
    const channels = parseNameList(channelsInput);
    const patterns = parseNameList(patternsInput);
    if (channels.length === 0 && patterns.length === 0) {
      setError(t('redis.pubsubNeedTarget'));
      return;
    }

    setSubscribing(true);
    setError(null);
    try {
      const subscriptionId = await redisCommandInvoke<string>('redis', 'pubsub_subscribe', {
        connectionId,
        channels,
        patterns,
      });
      setSubscriptions((prev) => [
        ...prev,
        { id: subscriptionId, channels, patterns },
      ]);
      setChannelsInput('');
      setPatternsInput('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubscribing(false);
    }
  }, [channelsInput, patternsInput, connectionId, t]);

  const handleUnsubscribe = useCallback(async (subscriptionId: string) => {
    setError(null);
    try {
      await redisCommandInvoke('redis', 'pubsub_unsubscribe', {
        connectionId,
        subscriptionId,
      });
      setSubscriptions((prev) => prev.filter((s) => s.id !== subscriptionId));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [connectionId]);

  const handlePublish = useCallback(async () => {
    const channel = publishChannel.trim();
    if (!channel) {
      setError(t('redis.pubsubChannelRequired'));
      return;
    }

    setPublishing(true);
    setError(null);
    try {
      const receivers = await redisCommandInvoke<number>('redis', 'pubsub_publish', {
        connectionId,
        channel,
        message: publishMessage,
      });
      setLastReceivers(receivers);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPublishing(false);
    }
  }, [connectionId, publishChannel, publishMessage, t]);

  const handleClearMessages = useCallback(() => {
    setMessages([]);
    setLastReceivers(null);
  }, []);

  return (
    <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
      <div className="flex min-h-0 w-full shrink-0 flex-col border-b border-edge lg:w-[360px] lg:border-b-0 lg:border-r">
        <div className="border-b border-edge bg-surface-alt px-4 py-3">
          <h3 className="text-sm font-medium text-fg">{t('redis.pubsubSubscribe')}</h3>
          <p className="mt-1 text-xs text-fg-muted">{t('redis.pubsubClusterNote')}</p>
        </div>

        <div className="space-y-3 px-4 py-3">
          <label className="block text-xs text-fg-secondary">
            {t('redis.pubsubChannels')}
            <textarea
              className="mt-1 w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg outline-none focus:border-accent"
              rows={3}
              placeholder={t('redis.pubsubChannelsPlaceholder')}
              value={channelsInput}
              onChange={(e) => setChannelsInput(e.target.value)}
            />
          </label>

          <label className="block text-xs text-fg-secondary">
            {t('redis.pubsubPatterns')}
            <textarea
              className="mt-1 w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg outline-none focus:border-accent"
              rows={2}
              placeholder={t('redis.pubsubPatternsPlaceholder')}
              value={patternsInput}
              onChange={(e) => setPatternsInput(e.target.value)}
            />
          </label>

          <Button
            variant="primary"
            className="h-8 w-full gap-1 text-xs"
            onClick={() => void handleSubscribe()}
            disabled={subscribing}
          >
            {subscribing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Radio className="h-3.5 w-3.5" />
            )}
            {t('redis.pubsubSubscribeAction')}
          </Button>
        </div>

        <div className="border-t border-edge px-4 py-3">
          <h3 className="text-sm font-medium text-fg">{t('redis.pubsubPublish')}</h3>
          <div className="mt-3 space-y-2">
            <input
              className="w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg outline-none focus:border-accent"
              placeholder={t('redis.pubsubPublishChannelPlaceholder')}
              value={publishChannel}
              onChange={(e) => setPublishChannel(e.target.value)}
            />
            <textarea
              className="w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg outline-none focus:border-accent"
              rows={3}
              placeholder={t('redis.pubsubPublishMessagePlaceholder')}
              value={publishMessage}
              onChange={(e) => setPublishMessage(e.target.value)}
            />
            <Button
              variant="secondary"
              className="h-8 w-full gap-1 text-xs"
              onClick={() => void handlePublish()}
              disabled={publishing}
            >
              {publishing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Send className="h-3.5 w-3.5" />
              )}
              {t('redis.pubsubPublishAction')}
            </Button>
            {lastReceivers !== null && (
              <p className="text-xs text-fg-muted">
                {t('redis.pubsubReceivers', { count: lastReceivers })}
              </p>
            )}
          </div>
        </div>

        {subscriptions.length > 0 && (
          <div className="min-h-0 flex-1 overflow-auto border-t border-edge px-4 py-3">
            <h3 className="text-xs font-medium uppercase tracking-wide text-fg-secondary">
              {t('redis.pubsubActiveSubscriptions')}
            </h3>
            <ul className="mt-2 space-y-2">
              {subscriptions.map((sub) => (
                <li
                  key={sub.id}
                  className="rounded-md border border-edge bg-surface px-3 py-2 text-xs"
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0 font-mono text-fg-secondary">
                      {sub.channels.length > 0 && (
                        <div>{t('redis.pubsubChannels')}: {sub.channels.join(', ')}</div>
                      )}
                      {sub.patterns.length > 0 && (
                        <div>{t('redis.pubsubPatterns')}: {sub.patterns.join(', ')}</div>
                      )}
                    </div>
                    <button
                      type="button"
                      className="shrink-0 rounded p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
                      title={t('redis.pubsubUnsubscribe')}
                      onClick={() => void handleUnsubscribe(sub.id)}
                    >
                      <X className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4 py-2">
          <span className="text-sm font-medium text-fg">{t('redis.pubsubMessages')}</span>
          <span className="text-xs text-fg-muted">({messages.length})</span>
          <div className="flex-1" />
          <Button
            variant="secondary"
            className="h-7 px-2 text-xs"
            onClick={handleClearMessages}
            disabled={messages.length === 0}
          >
            {t('redis.pubsubClearMessages')}
          </Button>
        </div>

        {error && (
          <div className="border-b border-danger/20 bg-danger/10 px-4 py-2 text-sm text-danger">
            {error}
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-auto">
          {messages.length === 0 ? (
            <div className="flex h-full items-center justify-center p-8 text-sm text-fg-muted">
              {t('redis.pubsubEmpty')}
            </div>
          ) : (
            <table className="w-full border-collapse text-[13px]">
              <thead className="sticky top-0 bg-surface-alt">
                <tr className="border-b border-edge text-left text-xs text-fg-secondary">
                  <th className="px-4 py-2 font-medium">{t('redis.pubsubTime')}</th>
                  <th className="px-4 py-2 font-medium">{t('redis.pubsubChannel')}</th>
                  <th className="px-4 py-2 font-medium">{t('redis.value')}</th>
                </tr>
              </thead>
              <tbody>
                {messages.map((msg) => (
                  <tr key={msg.id} className="border-b border-edge/60 align-top">
                    <td className="whitespace-nowrap px-4 py-2 font-mono text-xs text-fg-muted">
                      {formatTime(msg.ts)}
                    </td>
                    <td className="max-w-[180px] px-4 py-2 font-mono text-xs text-fg">
                      {msg.channel}
                    </td>
                    <td className="px-4 py-2 font-mono text-xs text-fg">
                      <span className={cn('break-all whitespace-pre-wrap')}>{msg.payload}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <div ref={messagesEndRef} />
        </div>
      </div>
    </div>
  );
}
