import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useLanguagePackStore } from '../languagePackStore';
import type { LanguagePackSummary } from '../../commands/languagePacks';

const { listMock, removeMock } = vi.hoisted(() => ({
  listMock: vi.fn(),
  removeMock: vi.fn(),
}));

vi.mock('../../commands/languagePacks', () => ({
  LANGUAGE_PACKS_CHANGED_EVENT: 'language-packs:changed',
  languagePackCommands: {
    listLanguagePacks: (...args: unknown[]) => listMock(...args),
    removeLanguagePack: (...args: unknown[]) => removeMock(...args),
  },
}));

vi.mock('../../platform/contracts', () => ({
  getPlatformOrNull: () => null,
}));

const PACK: LanguagePackSummary = {
  code: 'ja',
  label: '日本語',
  version: '1.0.0',
  author: 'DataZen',
  description: null,
  messageCount: 3,
  fileName: 'ja',
};

describe('languagePackStore', () => {
  beforeEach(() => {
    listMock.mockReset();
    removeMock.mockReset();
    useLanguagePackStore.setState({ packs: [], loaded: false, error: null });
  });

  it('fetch populates the list', async () => {
    listMock.mockResolvedValue([PACK]);
    await useLanguagePackStore.getState().fetch();
    const state = useLanguagePackStore.getState();
    expect(state.packs).toEqual([PACK]);
    expect(state.loaded).toBe(true);
    expect(state.error).toBeNull();
    expect(state.byCode('ja')).toEqual(PACK);
    expect(state.byCode('de')).toBeUndefined();
  });

  it('fetch failure records the error but stays loaded', async () => {
    listMock.mockRejectedValue(new Error('boom'));
    await useLanguagePackStore.getState().fetch();
    const state = useLanguagePackStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.error).toBe('boom');
    expect(state.packs).toEqual([]);
  });

  it('remove delegates and refetches', async () => {
    listMock.mockResolvedValue([PACK]);
    removeMock.mockResolvedValue(undefined);
    useLanguagePackStore.setState({ packs: [PACK], loaded: true });
    await useLanguagePackStore.getState().remove('ja');
    expect(removeMock).toHaveBeenCalledWith('ja');
    expect(useLanguagePackStore.getState().packs).toEqual([PACK]);
  });

  it('remove failure surfaces the error and rethrows', async () => {
    listMock.mockResolvedValue([]);
    removeMock.mockRejectedValue(new Error('language pack not found: ja'));
    await expect(useLanguagePackStore.getState().remove('ja')).rejects.toThrow(
      'language pack not found: ja',
    );
    expect(useLanguagePackStore.getState().error).toBe('language pack not found: ja');
  });
});
