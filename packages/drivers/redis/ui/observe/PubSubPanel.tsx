import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Loader2, Radio, Search, Send, X } from 'lucide-react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Button, cn } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import { redisCommandInvoke } from '../shared/redisInvoke';
import { useRedisGate } from '../shared/useRedisGate';

export interface PubSubPanelProps {
  dbSessionId: string;
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
  dbSessionId: string;
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

export function PubSubPanel({ dbSessionId }: PubSubPanelProps) {
  const { t } = useI18n();
  const { gateWrite, gateDialog } = useRedisGate();
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
  const [searchText, setSearchText] = useState('');
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
          if (payload.dbSessionId !== dbSessionId) return;
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
  }, [dbSessionId]);

  useEffect(() => {
    return () => {
      for (const sub of subscriptionsRef.current) {
        void redisCommandInvoke('redis', 'pubsub_unsubscribe', {
          dbSessionId,
          subscriptionId: sub.id,
        }).catch(() => {});
      }
    };
  }, [dbSessionId]);

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
        dbSessionId,
        channels,
        patterns,
      });
      setSubscriptions((prev) => [...prev, { id: subscriptionId, channels, patterns }]);
      setChannelsInput('');
      setPatternsInput('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubscribing(false);
    }
  }, [channelsInput, patternsInput, dbSessionId, t]);

  const handleUnsubscribe = useCallback(
    async (subscriptionId: string) => {
      setError(null);
      try {
        await redisCommandInvoke('redis', 'pubsub_unsubscribe', {
          dbSessionId,
          subscriptionId,
        });
        setSubscriptions((prev) => prev.filter((s) => s.id !== subscriptionId));
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [dbSessionId],
  );

  const handlePublish = useCallback(async () => {
    const channel = publishChannel.trim();
    if (!channel) {
      setError(t('redis.pubsubChannelRequired'));
      return;
    }
    if (!(await gateWrite('write-op', `PUBLISH ${channel} ...`))) return;

    setPublishing(true);
    setError(null);
    try {
      const receivers = await redisCommandInvoke<number>('redis', 'pubsub_publish', {
        dbSessionId,
        channel,
        message: publishMessage,
      });
      setLastReceivers(receivers);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPublishing(false);
    }
  }, [dbSessionId, publishChannel, publishMessage, t, gateWrite]);

  const handleClearMessages = useCallback(() => {
    setMessages([]);
    setLastReceivers(null);
  }, []);

  const filteredMessages = useMemo(() => {
    if (!searchText.trim()) return messages;
    const query = searchText.toLowerCase();
    return messages.filter(
      (msg) =>
        msg.channel.toLowerCase().includes(query) || msg.payload.toLowerCase().includes(query),
    );
  }, [messages, searchText]);

  const channelStats = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const msg of messages) {
      counts[msg.channel] = (counts[msg.channel] || 0) + 1;
    }
    return Object.entries(counts)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5);
  }, [messages]);

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
              data-testid="redis-pubsub-channels"
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
              data-testid="redis-pubsub-patterns"
              value={patternsInput}
              onChange={(e) => setPatternsInput(e.target.value)}
            />
          </label>

          <Button
            variant="primary"
            className="h-8 w-full gap-1 text-xs"
            data-testid="redis-pubsub-subscribe"
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
              data-testid="redis-pubsub-publish-channel"
              value={publishChannel}
              onChange={(e) => setPublishChannel(e.target.value)}
            />
            <textarea
              className="w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg outline-none focus:border-accent"
              rows={3}
              placeholder={t('redis.pubsubPublishMessagePlaceholder')}
              data-testid="redis-pubsub-publish-message"
              value={publishMessage}
              onChange={(e) => setPublishMessage(e.target.value)}
            />
            <Button
              variant="secondary"
              className="h-8 w-full gap-1 text-xs"
              data-testid="redis-pubsub-publish"
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
                        <div className="flex items-center gap-1.5">
                          <span className="inline-flex shrink-0 items-center rounded bg-accent/15 px-1.5 py-0.5 text-[10px] font-medium text-accent">
                            {t('redis.pubsubTypeChannel')}
                          </span>
                          <span className="truncate">{sub.channels.join(', ')}</span>
                        </div>
                      )}
                      {sub.patterns.length > 0 && (
                        <div className="mt-1 flex items-center gap-1.5">
                          <span className="inline-flex shrink-0 items-center rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning">
                            {t('redis.pubsubTypePattern')}
                          </span>
                          <span className="truncate">{sub.patterns.join(', ')}</span>
                        </div>
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
          <span className="text-xs text-fg-muted">
            {t('redis.pubsubMessageCount', { count: messages.length })}
          </span>
          <div className="flex-1" />
          <div className="relative">
            <Search className="absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-fg-muted" />
            <input
              className="h-7 w-40 rounded-md border border-edge bg-surface pl-7 pr-2 text-xs text-fg outline-none focus:border-accent"
              placeholder={t('redis.pubsubSearchPlaceholder')}
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
            />
          </div>
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

        <div className="min-h-0 flex-1 overflow-auto" data-testid="redis-pubsub-messages">
          {filteredMessages.length === 0 ? (
            <div className="flex h-full items-center justify-center p-8 text-sm text-fg-muted">
              {messages.length === 0 ? t('redis.pubsubEmpty') : t('redis.pubsubSearchPlaceholder')}
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
                {filteredMessages.map((msg) => (
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

        {channelStats.length > 0 && (
          <div className="flex shrink-0 flex-wrap items-center gap-3 border-t border-edge bg-surface-alt px-4 py-1.5 text-[11px] text-fg-muted">
            <span className="font-medium text-fg-secondary">{t('redis.pubsubTopChannels')}</span>
            {channelStats.map(([ch, count]) => (
              <span key={ch} className="font-mono">
                {ch}: {count}
              </span>
            ))}
          </div>
        )}
      </div>
      {gateDialog}
    </div>
  );
}
