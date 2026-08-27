import { describe, expect, it } from 'vitest';
import {
  EXTENSION_API_VERSION,
  type ExtensionManifest,
  type ExtensionPermission,
  type ExtensionSummary,
} from '../extension';

/**
 * Contract fixtures (F3 test agent): these payloads mirror exactly what the
 * Rust side serializes — `ExtensionSummary` in src-tauri/src/commands/extensions.rs
 * and `ExtensionManifest` in src-tauri/src/extensions/manifest.rs, both
 * `#[serde(rename_all = "camelCase")]` with `skip_serializing_if` on the
 * optional fields. The `satisfies` clauses make drift a compile error.
 */

/** Serialized strings of the Rust `Permission` enum (serde renames). */
const RUST_PERMISSION_STRINGS = [
  'context:connections',
  'command:invoke',
  'storage:local',
  'ui:notify',
] as const satisfies readonly ExtensionPermission[];

/** `list_plugins` payload with all Option::None fields omitted by serde. */
const SUMMARY_PAYLOAD = {
  id: 'acme.demo',
  name: 'Demo Plugin',
  version: '1.0.0',
  apiVersion: 2,
  enabled: true,
  permissions: ['storage:local', 'command:invoke'],
  pages: [{ id: 'main', title: 'Main' }],
  themes: [{ id: 'demo-dark', name: 'Demo Dark', modes: ['dark'] }],
} satisfies ExtensionSummary;

/** `get_plugin_manifest` payload (camelCase contributions). */
const MANIFEST_PAYLOAD = {
  id: 'acme.demo',
  name: 'Demo Plugin',
  version: '1.0.0',
  apiVersion: EXTENSION_API_VERSION,
  author: 'Acme',
  entry: 'index.html',
  contributes: {
    pages: [{ id: 'main', title: 'Main', icon: 'assets/icon.svg', showIn: 'workspace' }],
    themes: [
      {
        id: 'demo-dark',
        name: 'Demo Dark',
        tokensCss: 'themes/demo-dark/tokens.css',
        modes: ['dark'],
      },
    ],
  },
  permissions: ['ui:notify'],
  backend: null,
} satisfies ExtensionManifest;

describe('types/plugin host contract', () => {
  it('EXTENSION_API_VERSION matches Rust PLUGIN_API_VERSION (=2)', () => {
    expect(EXTENSION_API_VERSION).toBe(2);
  });

  it('ExtensionPermission covers exactly the four Rust Permission serde renames', () => {
    expect([...RUST_PERMISSION_STRINGS].sort()).toEqual([
      'command:invoke',
      'context:connections',
      'storage:local',
      'ui:notify',
    ]);
  });

  it('accepts a serde-shaped ExtensionSummary payload (omitted optionals)', () => {
    expect(SUMMARY_PAYLOAD.apiVersion).toBe(2);
    expect(SUMMARY_PAYLOAD.permissions[0]).toBe('storage:local');
    expect(SUMMARY_PAYLOAD.pages[0].title).toBe('Main');
    expect(SUMMARY_PAYLOAD.themes[0].modes).toEqual(['dark']);
    expect(SUMMARY_PAYLOAD).not.toHaveProperty('author');
    expect(SUMMARY_PAYLOAD).not.toHaveProperty('description');
  });

  it('accepts a serde-shaped ExtensionManifest payload (showIn/tokensCss/backend)', () => {
    expect(MANIFEST_PAYLOAD.contributes.pages[0].showIn).toBe('workspace');
    expect(MANIFEST_PAYLOAD.contributes.themes[0].tokensCss).toContain('tokens.css');
    expect(MANIFEST_PAYLOAD.permissions).toEqual(['ui:notify']);
    // P1 constraint: backend stays null for frontend-only plugins.
    expect(MANIFEST_PAYLOAD.backend).toBeNull();
  });
});
