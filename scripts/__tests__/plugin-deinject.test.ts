/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  cleanGeneratedTsContent,
  cleanPluginInitContent,
  deinjectCargoContent,
  deinjectCapabilities,
  deinjectManagedContent,
  emptyMarkerBlock,
} from '../plugin-deinject.mjs';
import { CLEAN_CONTENTS, INJECTED_CONTENTS } from './fixture';

describe('plugin-deinject', () => {
  it('empties cargo marker bodies', () => {
    expect(emptyMarkerBlock(INJECTED_CONTENTS['Cargo.toml'], 'plugin-patches')).toBe(
      CLEAN_CONTENTS['Cargo.toml'],
    );
    expect(deinjectCargoContent(INJECTED_CONTENTS['src-tauri/Cargo.toml'])).toBe(
      CLEAN_CONTENTS['src-tauri/Cargo.toml'],
    );
  });

  it('strips plugin ACL but keeps windows edits', () => {
    const injectedWithWindow = INJECTED_CONTENTS[
      'src-tauri/capabilities/default.json'
    ].replace('"connection-*"', '"connection-*", "docs-singleton"');
    const out = deinjectCapabilities(injectedWithWindow);
    expect(out).toContain('docs-singleton');
    expect(out).not.toContain('kiwi:');
    expect(out).toContain('core:default');
  });

  it('routes fully-generated files to stash baseline', () => {
    expect(
      deinjectManagedContent(
        'src/plugins/generated.ts',
        INJECTED_CONTENTS['src/plugins/generated.ts'],
        { stashContent: CLEAN_CONTENTS['src/plugins/generated.ts'] },
      ),
    ).toBe(CLEAN_CONTENTS['src/plugins/generated.ts']);
  });

  it('falls back to canonical stubs when stash is missing', () => {
    expect(
      deinjectManagedContent(
        'src/plugins/generated.ts',
        INJECTED_CONTENTS['src/plugins/generated.ts'],
        { stashContent: null },
      ),
    ).toBe(cleanGeneratedTsContent());
    expect(
      deinjectManagedContent(
        'src-tauri/src/plugin_init.rs',
        INJECTED_CONTENTS['src-tauri/src/plugin_init.rs'],
        { stashContent: null },
      ),
    ).toBe(cleanPluginInitContent());
  });
});
