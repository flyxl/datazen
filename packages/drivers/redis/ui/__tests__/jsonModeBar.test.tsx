/**
 * R3 — JSON three-state editor journey.
 *
 * Covers the pure display transforms (lossless raw / pretty / minify), the
 * presentational mode bar, and the ReJSON editor's raw-text edit → save path
 * (asserting the exact `json_set` payload).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

// Components take `useI18n` from the single @datazen/ui runtime; keep the
// assertions locale-independent by overriding only that hook.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

const redisInvoke = vi.fn();
vi.mock('../shared/redisInvoke', () => ({
  redisCommandInvoke: (...a: unknown[]) => redisInvoke(...a),
}));

import {
  formatJson,
  isValidJson,
  JSON_DISPLAY_MODES,
  JSON_TEXT_MODES,
} from '../value-editors/jsonModes';
import { JsonModeBar } from '../value-editors/JsonModeBar';
import { JsonEditor } from '../value-editors/JsonEditor';

const DOC = { a: 1, b: [2, 3], c: { d: true } };
const RAW = JSON.stringify(DOC);

beforeEach(() => {
  redisInvoke.mockReset();
  redisInvoke.mockImplementation((_p: string, command: string) => {
    if (command === 'modules_list') return Promise.resolve(['ReJSON']);
    if (command === 'json_get') {
      return Promise.resolve({ value: DOC, rawText: RAW });
    }
    return Promise.resolve({ ok: true });
  });
});

afterEach(() => {
  cleanup();
});

describe('jsonModes (pure transforms)', () => {
  it('raw mode returns the input verbatim', () => {
    expect(formatJson(RAW, 'raw')).toBe(RAW);
  });

  it('pretty mode uses 2-space indentation', () => {
    expect(formatJson(RAW, 'pretty')).toBe(JSON.stringify(DOC, null, 2));
  });

  it('minify mode produces a single line', () => {
    expect(formatJson(RAW, 'minify')).toBe(RAW);
  });

  it('switching raw → pretty → minify is lossless (parse-equal)', () => {
    const pretty = formatJson(RAW, 'pretty');
    const minified = formatJson(pretty, 'minify');
    expect(JSON.parse(minified)).toEqual(DOC);
    expect(JSON.parse(formatJson(minified, 'pretty'))).toEqual(DOC);
  });

  it('invalid JSON passes through untouched rather than being destroyed', () => {
    const broken = '{"a":';
    expect(formatJson(broken, 'pretty')).toBe(broken);
    expect(isValidJson(broken)).toBe(false);
    expect(isValidJson(RAW)).toBe(true);
  });

  it('exposes the four display modes and three text modes', () => {
    expect(JSON_DISPLAY_MODES).toEqual(['tree', 'raw', 'pretty', 'minify']);
    expect(JSON_TEXT_MODES).toEqual(['raw', 'pretty', 'minify']);
  });
});

describe('JsonModeBar', () => {
  it('renders the active mode with selected styling and reports selection', () => {
    const onSelect = vi.fn();
    render(<JsonModeBar modes={JSON_DISPLAY_MODES} active="pretty" onSelect={onSelect} />);
    expect(screen.getByTestId('redis-json-mode-pretty').getAttribute('aria-selected')).toBe('true');
    expect(screen.getByTestId('redis-json-mode-tree').getAttribute('aria-selected')).toBe('false');
    fireEvent.click(screen.getByTestId('redis-json-mode-minify'));
    expect(onSelect).toHaveBeenCalledWith('minify');
  });
});

describe('JsonEditor three-state integration', () => {
  function editor() {
    return render(<JsonEditor dbSessionId="sess-1" dbIndex={2} redisKey="doc:key" />);
  }

  it('defaults to the tree view and exposes the mode bar', async () => {
    editor();
    expect(await screen.findByTestId('redis-json-mode-tree')).toBeTruthy();
    expect(screen.getByTestId('redis-json-mode-raw')).toBeTruthy();
  });

  it('shows the verbatim raw text when the Raw tab is selected', async () => {
    editor();
    fireEvent.click(await screen.findByTestId('redis-json-mode-raw'));
    const box = await screen.findByTestId('redis-json-text');
    expect((box as HTMLTextAreaElement).value).toBe(RAW);
  });

  it('reformats the buffer when switching raw → pretty without losing data', async () => {
    editor();
    fireEvent.click(await screen.findByTestId('redis-json-mode-raw'));
    const box = (await screen.findByTestId('redis-json-text')) as HTMLTextAreaElement;
    fireEvent.click(screen.getByTestId('redis-json-mode-pretty'));
    expect(box.value).toBe(JSON.stringify(DOC, null, 2));
    expect(JSON.parse(box.value)).toEqual(DOC);
  });

  it('saves the edited raw document back through json_set at the root path', async () => {
    editor();
    fireEvent.click(await screen.findByTestId('redis-json-mode-raw'));
    const box = (await screen.findByTestId('redis-json-text')) as HTMLTextAreaElement;
    const edited = JSON.stringify({ a: 99, b: [2, 3], c: { d: false } });
    fireEvent.change(box, { target: { value: edited } });
    fireEvent.click(screen.getByTestId('redis-json-text-save'));

    await waitFor(() => {
      const setCall = redisInvoke.mock.calls.find((c) => c[1] === 'json_set');
      return setCall != null;
    });
    const [, command, args] = redisInvoke.mock.calls.find(
      (c) => c[1] === 'json_set',
    ) as unknown as [string, string, Record<string, unknown>];
    expect(command).toBe('json_set');
    expect(args.key).toBe('doc:key');
    expect(args.path).toBe('$');
    expect(JSON.parse(String(args.value))).toEqual({ a: 99, b: [2, 3], c: { d: false } });
  });

  it('rejects invalid raw JSON before issuing a write', async () => {
    editor();
    fireEvent.click(await screen.findByTestId('redis-json-mode-raw'));
    const box = (await screen.findByTestId('redis-json-text')) as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: '{"a":' } });
    fireEvent.click(screen.getByTestId('redis-json-text-save'));
    expect(await screen.findByText('redis.invalidJson')).toBeTruthy();
    expect(redisInvoke.mock.calls.some((c) => c[1] === 'json_set')).toBe(false);
  });
});
