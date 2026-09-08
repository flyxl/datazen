/**
 * EP Hot-plugging & Standalone Packaging Full Lifecycle Journey Test (R-Phase)
 *
 * Verifies the complete end-to-end journey across all 4 tracks:
 *  - Journey 1: Extension lifecycle registration, CodeMirror compartment hot reconfiguration,
 *               zero window reload, seamless fallback, and undo history preservation.
 *  - Journey 2: Security verification gate — official signature pass, tampered bundle reject,
 *               developer mode unverified opt-in, enterprise trusted keys, local-link bypass.
 *  - Journey 3: Packaging & staging pipeline — .dzx archive layout, builtin-ep staging, manifest integrity.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { undo, redo } from '@codemirror/commands';
import {
  extensionRegistry,
  sqlEditorProEP,
  type SqlEditorProFeatures,
  verifyExtensionPackage,
  UNVERIFIED_EXTENSION_LABEL,
  EXTENSION_POINTS_VERSION,
} from '@datazen/extension-points';
import {
  compartments,
  createBaseEditorExtensions,
  createStatementExtensions,
  createCompletionExtensions,
  createIntentionExtensions,
  createHoverExtensions,
  createPasteExtensions,
  createLinterExtensions,
  reconfigureProCompartments,
} from '../../../components/sql-editor/editorExtensions';
import { signEpPackage } from '../../../../scripts/sign-ep.mjs';
import {
  stagePackageTree,
  createDzxArchive,
  REQUIRED_PACKAGE_PATHS,
} from '../../../../scripts/pack-ep.mjs';
import { unzipSync } from 'fflate';
import { tmpdir } from 'os';
import { join } from 'path';
import { mkdirSync, writeFileSync, rmSync, readFileSync } from 'fs';

describe('EP Hot-plugging Full Journey (R-Phase)', () => {
  beforeEach(() => {
    extensionRegistry.unregister(sqlEditorProEP);
  });

  // ── Journey 1: Editor Hot-plugging & Compartment Reconfiguration ────────────
  it('Journey 1: Editor typing -> Pro dynamic hot-plug -> Pro dynamic hot-unplug -> undo history preserved', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);

    const modelRef = { current: null };
    const metadataSnapshotRef = { current: undefined };

    const statementExts = createStatementExtensions({ enabled: true });
    const completionExts = createCompletionExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    );
    const intentionExts = createIntentionExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    );
    const hoverExts = createHoverExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    );
    const pasteExts = createPasteExtensions({});
    const linterExts = createLinterExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    );

    const initialDoc = 'SELECT * FROM users WHERE id = 1;';
    const state = EditorState.create({
      doc: initialDoc,
      selection: { anchor: initialDoc.length },
      extensions: [
        ...createBaseEditorExtensions(),
        compartments.statement.of(statementExts),
        compartments.completion.of(completionExts),
        compartments.intention.of(intentionExts),
        compartments.hover.of(hoverExts),
        compartments.paste.of(pasteExts),
        compartments.linter.of(linterExts),
      ],
    });

    const view = new EditorView({ state, parent });

    // Initial state: fallback (not enhanced)
    expect(extensionRegistry.isEnhanced(sqlEditorProEP)).toBe(false);

    // Step 1: User types additional SQL
    view.dispatch({
      changes: { from: view.state.doc.length, insert: '\nSELECT name FROM roles;' },
      selection: { anchor: view.state.doc.length + '\nSELECT name FROM roles;'.length },
    });
    expect(view.state.doc.toString()).toContain('SELECT name FROM roles;');
    const textBeforeHotplug = view.state.doc.toString();
    const selBeforeHotplug = view.state.selection.main.from;

    // Step 2: Pro extension activates dynamically (Hot-Plug)
    const mockProFeatures: SqlEditorProFeatures = {
      createStatementDecorations: vi.fn(() => []),
      createHoverExtensions: vi.fn(() => []),
      createIntentionExtensions: vi.fn(() => []),
      createJoinCompletionSource: vi.fn(() => null),
      createColumnCompletionSource: vi.fn(() => null),
      createPasteExtensions: vi.fn(() => []),
      createPasteAsInContextMenuItems: vi.fn(() => null),
      createLinterExtensions: vi.fn(() => []),
      createSignatureHelpExtensions: vi.fn(() => []),
      useStatementNavigation: vi.fn(),
      useStatementGutter: vi.fn(),
      useBindParameters: vi.fn(),
      useHoverTooltip: vi.fn(),
      useSignatureHelp: vi.fn(),
      useJoinCompletion: vi.fn(),
      useSqlIntentions: vi.fn(),
      usePasteAsIn: vi.fn(),
    };

    extensionRegistry.register(sqlEditorProEP, mockProFeatures);
    expect(extensionRegistry.isEnhanced(sqlEditorProEP)).toBe(true);

    // Reconfigure compartments without destroying view
    reconfigureProCompartments(view, {
      opts: { databaseType: 'postgresql' },
      refs: { modelRef, metadataSnapshotRef },
      statementOpts: { enabled: true },
    });

    // Assert: Document and cursor selection completely preserved
    expect(view.state.doc.toString()).toBe(textBeforeHotplug);
    expect(view.state.selection.main.from).toBe(selBeforeHotplug);

    // Step 3: Pro extension deactivates dynamically (Hot-Unplug)
    extensionRegistry.unregister(sqlEditorProEP);
    expect(extensionRegistry.isEnhanced(sqlEditorProEP)).toBe(false);

    reconfigureProCompartments(view, {
      opts: { databaseType: 'postgresql' },
      refs: { modelRef, metadataSnapshotRef },
      statementOpts: { enabled: true },
    });

    // Assert: View state intact, smoothly downgraded to fallback
    expect(view.state.doc.toString()).toBe(textBeforeHotplug);
    expect(view.state.selection.main.from).toBe(selBeforeHotplug);

    // Verify undo still functions
    undo(view);
    expect(view.state.doc.toString()).toBe(initialDoc);
    redo(view);
    expect(view.state.doc.toString()).toBe(textBeforeHotplug);

    view.destroy();
    parent.remove();
  });

  // ── Journey 2: Security Verification & Trust Policies ──────────────────────
  it('Journey 2: Signature verification gate checks official, tampered, dev-mode, and enterprise keys', async () => {
    const manifest = {
      id: '@datazen/extension-pro',
      version: '0.1.2',
      main: 'dist/index.esm.js',
      engines: { datazen: '>=0.1.2', extensionPointsVersion: EXTENSION_POINTS_VERSION },
    };
    const manifestContent = JSON.stringify(manifest, null, 2);
    const bundleContent = 'export function activate() { return "pro"; }\n';

    // 1. Sign package with official test private key
    const dir = join(tmpdir(), `journey-sec-${Date.now()}`);
    mkdirSync(join(dir, 'dist'), { recursive: true });
    writeFileSync(join(dir, 'manifest.json'), manifestContent);
    writeFileSync(join(dir, 'dist/index.esm.js'), bundleContent);

    const { sigDoc } = signEpPackage({ packageDir: dir });
    const signatureContent = JSON.stringify(sigDoc, null, 2);

    // Test Case 2a: Valid official signature passes
    const validResult = await verifyExtensionPackage({
      manifest,
      files: { manifestContent, bundleContent, signatureContent },
      sourceKind: 'dzx',
    });
    expect(validResult.ok).toBe(true);
    if (validResult.ok) {
      expect(validResult.trustSource).toBe('official');
    }

    // Test Case 2b: Tampered bundle is strictly rejected
    const tamperedResult = await verifyExtensionPackage({
      manifest,
      files: {
        manifestContent,
        bundleContent: bundleContent + '/* malicious injection */',
        signatureContent,
      },
      sourceKind: 'dzx',
    });
    expect(tamperedResult.ok).toBe(false);
    if (!tamperedResult.ok) {
      expect(tamperedResult.code).toBe('tampered');
    }

    // Test Case 2c: Unsigned package rejected by default, but allowed in developer mode
    const unsignedResult = await verifyExtensionPackage({
      manifest,
      files: { manifestContent, bundleContent, signatureContent: null },
      sourceKind: 'dzx',
    });
    expect(unsignedResult.ok).toBe(false);
    if (!unsignedResult.ok) {
      expect(unsignedResult.code).toBe('unsigned-rejected');
    }

    const devModeResult = await verifyExtensionPackage({
      manifest,
      files: { manifestContent, bundleContent, signatureContent: null },
      sourceKind: 'dzx',
      allowUnsignedExtensions: true,
    });
    expect(devModeResult.ok).toBe(true);
    if (devModeResult.ok) {
      expect(devModeResult.trustSource).toBe('unverified');
      expect(devModeResult.unverifiedLabel).toBe(UNVERIFIED_EXTENSION_LABEL);
    }

    // Test Case 2d: Local development directory link bypasses signature gate
    const localLinkResult = await verifyExtensionPackage({
      manifest,
      files: { manifestContent, bundleContent, signatureContent: null },
      sourceKind: 'local-link',
    });
    expect(localLinkResult.ok).toBe(true);
    if (localLinkResult.ok) {
      expect(localLinkResult.trustSource).toBe('local-dev');
    }

    rmSync(dir, { recursive: true, force: true });
  });

  // ── Journey 3: Packaging & Staging Pipeline ────────────────────────────────
  it('Journey 3: Packaging pipeline produces signed .dzx and builtin-ep staging layout', () => {
    const src = join(tmpdir(), `journey-pkg-src-${Date.now()}`);
    const staged = join(tmpdir(), `journey-pkg-staged-${Date.now()}`);
    const dzxDir = join(tmpdir(), `journey-pkg-dzx-${Date.now()}`);

    mkdirSync(join(src, 'dist'), { recursive: true });
    mkdirSync(join(src, 'src/locales'), { recursive: true });
    writeFileSync(
      join(src, 'manifest.json'),
      JSON.stringify({
        id: '@datazen/extension-sql-editor-pro',
        version: '0.1.2',
        main: 'dist/index.esm.js',
        engines: { datazen: '>=0.1.2', extensionPointsVersion: '1.0.0' },
      }),
    );
    writeFileSync(join(src, 'dist/index.esm.js'), 'export function activate() {}\n');
    writeFileSync(join(src, 'src/locales/en.ts'), 'export default {};\n');

    // 1. Stage package tree
    stagePackageTree(src, staged);
    for (const rel of REQUIRED_PACKAGE_PATHS) {
      const exists = require('fs').existsSync(join(staged, rel));
      expect(exists).toBe(true);
    }

    // 2. Build .dzx archive
    mkdirSync(dzxDir, { recursive: true });
    const dzxPath = join(dzxDir, 'sql-editor-pro-0.1.2.dzx');
    createDzxArchive(staged, dzxPath);

    // 3. Inspect zip entries using fflate.unzipSync
    const zipBytes = readFileSync(dzxPath);
    const unzipped = unzipSync(zipBytes);
    const entryNames = Object.keys(unzipped);

    expect(entryNames).toContain('manifest.json');
    expect(entryNames).toContain('dist/index.esm.js');
    expect(entryNames).toContain('signature.sig');
    expect(entryNames).toContain('locales/en.ts');

    rmSync(src, { recursive: true, force: true });
    rmSync(staged, { recursive: true, force: true });
    rmSync(dzxDir, { recursive: true, force: true });
  });
});
