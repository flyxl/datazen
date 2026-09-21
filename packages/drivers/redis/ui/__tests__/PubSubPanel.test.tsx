import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { PubSubPanel } from '../observe/PubSubPanel';

// jsdom does not implement scrollIntoView
Element.prototype.scrollIntoView = vi.fn();

// Components take `useI18n` from the single @datazen/ui runtime; keep the
// assertions locale-independent by overriding only that hook.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      if (params) {
        let result = key;
        for (const [k, v] of Object.entries(params)) {
          result = result.replace(`{${k}}`, String(v));
        }
        return result;
      }
      return key;
    },
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock('../shared/redisInvoke', () => ({
  redisCommandInvoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('../shared/useRedisGate', () => ({
  useRedisGate: () => ({ gateWrite: async () => true, gateDialog: null }),
}));

describe('PubSubPanel', () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('renders subscribe and publish sections', () => {
    render(<PubSubPanel dbSessionId="test-session" />);
    expect(screen.getByText('redis.pubsubSubscribe')).toBeTruthy();
    expect(screen.getByText('redis.pubsubPublish')).toBeTruthy();
  });

  it('clear messages button empties message list', async () => {
    render(<PubSubPanel dbSessionId="test-session" />);
    const clearBtn = screen.getByText('redis.pubsubClearMessages');
    expect(clearBtn).toBeTruthy();
    // Button should be disabled when no messages
    expect((clearBtn as HTMLButtonElement).disabled).toBe(true);
  });

  it('shows empty state when no messages', () => {
    render(<PubSubPanel dbSessionId="test-session" />);
    expect(screen.getByText('redis.pubsubEmpty')).toBeTruthy();
  });

  it('search input is present for filtering messages', () => {
    render(<PubSubPanel dbSessionId="test-session" />);
    const searchInput = screen.getByPlaceholderText('redis.pubsubSearchPlaceholder');
    expect(searchInput).toBeTruthy();
  });

  it('subscribe button calls invoke with channels and patterns', async () => {
    mockInvoke.mockResolvedValueOnce('sub-id-1');
    render(<PubSubPanel dbSessionId="test-session" />);

    const channelsInput = screen.getByPlaceholderText('redis.pubsubChannelsPlaceholder');
    const patternsInput = screen.getByPlaceholderText('redis.pubsubPatternsPlaceholder');

    await act(async () => {
      fireEvent.change(channelsInput, { target: { value: 'news' } });
      fireEvent.change(patternsInput, { target: { value: 'user.*' } });
    });

    const subscribeBtn = screen.getByText('redis.pubsubSubscribeAction');
    await act(async () => {
      fireEvent.click(subscribeBtn);
    });

    expect(mockInvoke).toHaveBeenCalledWith('redis', 'pubsub_subscribe', {
      dbSessionId: 'test-session',
      channels: ['news'],
      patterns: ['user.*'],
    });
  });

  it('unsubscribe button calls invoke and removes subscription', async () => {
    mockInvoke
      .mockResolvedValueOnce('sub-id-2') // subscribe
      .mockResolvedValueOnce(undefined); // unsubscribe

    render(<PubSubPanel dbSessionId="test-session" />);

    const channelsInput = screen.getByPlaceholderText('redis.pubsubChannelsPlaceholder');
    await act(async () => {
      fireEvent.change(channelsInput, { target: { value: 'alerts' } });
    });

    const subscribeBtn = screen.getByText('redis.pubsubSubscribeAction');
    await act(async () => {
      fireEvent.click(subscribeBtn);
    });

    // Subscription should appear with type badge
    expect(screen.getByText('redis.pubsubTypeChannel')).toBeTruthy();

    // Click unsubscribe button (X icon)
    const unsubBtn = screen.getByTitle('redis.pubsubUnsubscribe');
    await act(async () => {
      fireEvent.click(unsubBtn);
    });

    expect(mockInvoke).toHaveBeenCalledWith('redis', 'pubsub_unsubscribe', {
      dbSessionId: 'test-session',
      subscriptionId: 'sub-id-2',
    });
  });

  it('shows channel type badge for channel subscriptions', async () => {
    mockInvoke.mockResolvedValueOnce('sub-id-3');
    render(<PubSubPanel dbSessionId="test-session" />);

    const channelsInput = screen.getByPlaceholderText('redis.pubsubChannelsPlaceholder');
    await act(async () => {
      fireEvent.change(channelsInput, { target: { value: 'test-ch' } });
    });

    const subscribeBtn = screen.getByText('redis.pubsubSubscribeAction');
    await act(async () => {
      fireEvent.click(subscribeBtn);
    });

    expect(screen.getByText('redis.pubsubTypeChannel')).toBeTruthy();
    expect(screen.queryByText('redis.pubsubTypePattern')).toBeNull();
  });

  it('shows pattern type badge for pattern subscriptions', async () => {
    mockInvoke.mockResolvedValueOnce('sub-id-4');
    render(<PubSubPanel dbSessionId="test-session" />);

    const patternsInput = screen.getByPlaceholderText('redis.pubsubPatternsPlaceholder');
    await act(async () => {
      fireEvent.change(patternsInput, { target: { value: 'log.*' } });
    });

    const subscribeBtn = screen.getByText('redis.pubsubSubscribeAction');
    await act(async () => {
      fireEvent.click(subscribeBtn);
    });

    expect(screen.getByText('redis.pubsubTypePattern')).toBeTruthy();
  });

  it('publish calls invoke with channel and message', async () => {
    mockInvoke.mockResolvedValueOnce(3);
    render(<PubSubPanel dbSessionId="test-session" />);

    const channelInput = screen.getByPlaceholderText('redis.pubsubPublishChannelPlaceholder');
    const messageInput = screen.getByPlaceholderText('redis.pubsubPublishMessagePlaceholder');

    await act(async () => {
      fireEvent.change(channelInput, { target: { value: 'my-channel' } });
      fireEvent.change(messageInput, { target: { value: 'hello world' } });
    });

    const publishBtn = screen.getByText('redis.pubsubPublishAction');
    await act(async () => {
      fireEvent.click(publishBtn);
    });

    expect(mockInvoke).toHaveBeenCalledWith('redis', 'pubsub_publish', {
      dbSessionId: 'test-session',
      channel: 'my-channel',
      message: 'hello world',
    });
  });
});
