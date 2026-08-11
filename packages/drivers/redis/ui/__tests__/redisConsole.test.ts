import { describe, expect, it, beforeEach } from 'vitest';
import {
  filterCompletions,
  getCompletionPrefix,
  REDIS_COMMANDS,
} from '../redisCommands';
import {
  loadConsoleHistory,
  navigateConsoleHistory,
  pushConsoleHistory,
  saveConsoleHistory,
} from '../consoleHistory';

describe('filterCompletions', () => {
  it('prefers commands then keys', () => {
    const out = filterCompletions('GE', ['GET', 'SET'], ['user:1', 'gear']);
    expect(out).toEqual(['GET', 'gear']);
  });

  it('matches case-insensitively', () => {
    const out = filterCompletions('get', REDIS_COMMANDS, ['GET-key']);
    expect(out[0]).toBe('GET');
  });

  it('returns empty list for empty prefix', () => {
    expect(filterCompletions('', REDIS_COMMANDS, ['a'])).toEqual([]);
  });

  it('dedupes command and key with same spelling', () => {
    const out = filterCompletions('ping', ['PING'], ['ping']);
    expect(out).toEqual(['PING']);
  });
});

describe('getCompletionPrefix', () => {
  it('reads the token before the cursor on the current line', () => {
    expect(getCompletionPrefix('GET foo\nSE', 10)).toBe('SE');
    expect(getCompletionPrefix('  GET k', 7)).toBe('k');
  });
});

describe('consoleHistory', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('persists per connection id', () => {
    saveConsoleHistory('conn-a', ['PING']);
    saveConsoleHistory('conn-b', ['GET x']);
    expect(loadConsoleHistory('conn-a')).toEqual(['PING']);
    expect(loadConsoleHistory('conn-b')).toEqual(['GET x']);
  });

  it('dedupes and prepends on push', () => {
    pushConsoleHistory('c1', 'GET a');
    pushConsoleHistory('c1', 'SET b 1');
    pushConsoleHistory('c1', 'GET a');
    expect(loadConsoleHistory('c1')).toEqual(['GET a', 'SET b 1']);
  });

  it('navigates up and down through history', () => {
    const history = ['latest', 'older'];
    let state = { index: null as number | null, draft: '' };

    const up1 = navigateConsoleHistory(history, state, 'up');
    expect(up1.text).toBe('latest');
    expect(up1.index).toBe(0);

    const up2 = navigateConsoleHistory(history, up1, 'up');
    expect(up2.text).toBe('older');
    expect(up2.index).toBe(1);

    const down1 = navigateConsoleHistory(history, up2, 'down');
    expect(down1.text).toBe('latest');
    expect(down1.index).toBe(0);

    const down2 = navigateConsoleHistory(history, down1, 'down');
    expect(down2.text).toBe('');
    expect(down2.index).toBeNull();
  });
});
