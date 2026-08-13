/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  cleanFullyGeneratedContent,
  cleanGeneratedLocalesContent,
  cleanGeneratedTsContent,
  cleanPluginInitContent,
  deinjectCargoContent,
  deinjectCapabilities,
  deinjectManagedContent,
  emptyBeginEndSection,
  emptyMarkerBlock,
  isFullyGeneratedManagedFile,
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
    const injectedWithWindow = INJECTED_CONTENTS['src-tauri/capabilities/default.json'].replace(
      '"connection-*"',
      '"connection-*", "docs-singleton"',
    );
    const out = deinjectCapabilities(injectedWithWindow);
    expect(out).toContain('docs-singleton');
    expect(out).not.toContain('kiwi:');
    expect(out).toContain('core:default');
  });

  it('leaves gitignored codegen files unchanged', () => {
    expect(
      deinjectManagedContent(
        'src/plugins/generated.ts',
        INJECTED_CONTENTS['src/plugins/generated.ts'],
      ),
    ).toBe(INJECTED_CONTENTS['src/plugins/generated.ts']);
  });

  it('classifies fully-generated codegen paths', () => {
    expect(isFullyGeneratedManagedFile('src/plugins/generated.ts')).toBe(true);
    expect(isFullyGeneratedManagedFile('src/plugins/generated-locales.ts')).toBe(true);
    expect(isFullyGeneratedManagedFile('src-tauri/src/plugin_init.rs')).toBe(true);
    expect(isFullyGeneratedManagedFile('Cargo.toml')).toBe(false);
  });

  it('returns canonical empty codegen stubs', () => {
    expect(cleanGeneratedTsContent()).toContain('export type DatabaseType = never');
    expect(cleanGeneratedLocalesContent()).toContain('export type PluginTranslationKey = never');
    expect(cleanPluginInitContent()).toContain('No plugins with Tauri commands enabled');
    expect(cleanFullyGeneratedContent('src/plugins/generated.ts')).toBe(cleanGeneratedTsContent());
    expect(cleanFullyGeneratedContent('src/plugins/generated-locales.ts')).toBe(
      cleanGeneratedLocalesContent(),
    );
    expect(cleanFullyGeneratedContent('src-tauri/src/plugin_init.rs')).toBe(
      cleanPluginInitContent(),
    );
    expect(() => cleanFullyGeneratedContent('Cargo.toml')).toThrow(/no clean stub/);
  });

  it('emptyMarkerBlock / emptyBeginEndSection no-op when markers are absent', () => {
    expect(emptyMarkerBlock('no markers here', 'plugin-patches')).toBe('no markers here');
    expect(emptyBeginEndSection('no sections', 'PLUGIN DEPS')).toBe('no sections');
  });

  it('deinjectCapabilities throws when permissions is not an array', () => {
    expect(() => deinjectCapabilities('{"identifier":"default","permissions":{}}')).toThrow(
      /not an array/,
    );
  });
});
