#!/usr/bin/env node
/**
 * Strip resolve-drivers injection from managed files while keeping user edits.
 *
 * - Cargo.toml: empty `# <<plugin-*>>` (and legacy BEGIN/END) marker bodies
 * - capabilities: drop `kiwi:` / `olap:` / `superset:` permissions; keep windows etc.
 * - plugin_init.rs / generated.ts: fully generated → use stash baseline
 */

export const PLUGIN_ACL_IDS = ['kiwi', 'olap', 'superset'];

export const FULLY_GENERATED_MANAGED = [
  'src-tauri/src/plugin_init.rs',
  'src/plugins/generated.ts',
];

export function isFullyGeneratedManagedFile(relPath) {
  return (
    relPath.endsWith('plugin_init.rs') || relPath.endsWith('generated.ts')
  );
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Empty `# <<name>>` … `# <</name>>` body (keep markers).
 * @param {string} content
 * @param {string} name
 */
export function emptyMarkerBlock(content, name) {
  const startTag = `# <<${name}>>`;
  const endTag = `# <</${name}>>`;
  const re = new RegExp(
    escapeRegex(startTag) + '[\\s\\S]*?' + escapeRegex(endTag),
  );
  if (!re.test(content)) return content;
  return content.replace(re, `${startTag}\n${endTag}`);
}

/**
 * Empty `# --- BEGIN tag ---` … `# --- END tag ---` body.
 * @param {string} content
 * @param {string} tag
 */
export function emptyBeginEndSection(content, tag) {
  const begin = `# --- BEGIN ${tag} (managed by resolve-drivers.mjs, do not edit) ---`;
  const end = `# --- END ${tag} ---`;
  const re = new RegExp(
    escapeRegex(begin) + '[\\s\\S]*?' + escapeRegex(end),
  );
  if (!re.test(content)) return content;
  return content.replace(re, `${begin}\n${end}`);
}

/**
 * @param {string} content
 */
export function deinjectCargoContent(content) {
  let out = content;
  for (const name of [
    'plugin-dependencies',
    'plugin-features',
    'plugin-patches',
  ]) {
    out = emptyMarkerBlock(out, name);
  }
  for (const tag of ['PLUGIN DEPS', 'PLUGIN FEATURES', 'PLUGIN PATCHES']) {
    out = emptyBeginEndSection(out, tag);
  }
  return out;
}

/**
 * Remove plugin ACL permission strings; preserve windows and other fields.
 * Layout matches resolve-drivers syncPluginCapabilities.
 * @param {string} content
 * @param {string[]} [pluginIds]
 */
export function deinjectCapabilities(content, pluginIds = PLUGIN_ACL_IDS) {
  const cap = JSON.parse(content);
  if (!Array.isArray(cap.permissions)) {
    throw new Error('capabilities.permissions is not an array');
  }
  const idSet = new Set(pluginIds);
  cap.permissions = cap.permissions.filter((entry) => {
    if (typeof entry !== 'string') return true;
    return !idSet.has(entry.split(':')[0]);
  });
  const windows = Array.isArray(cap.windows) ? cap.windows : [];
  const windowsJson = `[${windows.map((w) => JSON.stringify(w)).join(', ')}]`;
  return [
    `{`,
    `  "identifier": ${JSON.stringify(cap.identifier)},`,
    `  "description": ${JSON.stringify(cap.description)},`,
    `  "windows": ${windowsJson},`,
    `  "permissions": [`,
    cap.permissions.map((p) => `    ${JSON.stringify(p)}`).join(',\n'),
    `  ]`,
    `}`,
    ``,
  ].join('\n');
}

/**
 * Produce commit-safe content: user edits kept, plugin injection removed.
 *
 * @param {string} relPath
 * @param {string} workContent
 * @param {{ stashContent?: string | null, pluginIds?: string[] }} [opts]
 * @returns {string}
 */
export function deinjectManagedContent(relPath, workContent, opts = {}) {
  const pluginIds = opts.pluginIds ?? PLUGIN_ACL_IDS;

  if (relPath.endsWith('Cargo.toml')) {
    return deinjectCargoContent(workContent);
  }
  if (relPath.endsWith('capabilities/default.json')) {
    return deinjectCapabilities(workContent, pluginIds);
  }
  if (isFullyGeneratedManagedFile(relPath)) {
    if (opts.stashContent == null) {
      throw new Error(
        `[plugin-deinject] stash baseline required to restore ${relPath}`,
      );
    }
    return opts.stashContent;
  }
  return workContent;
}
