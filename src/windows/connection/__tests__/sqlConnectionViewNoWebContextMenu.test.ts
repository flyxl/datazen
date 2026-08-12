import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = fs.readFileSync(path.resolve(import.meta.dirname, '../SqlConnectionView.tsx'), 'utf8');

describe('SqlConnectionView has no whole-area Web ContextMenu', () => {
  it('does not import components/ui/ContextMenu', () => {
    expect(SRC).not.toMatch(/from ['"].*components\/ui\/ContextMenu['"]/);
  });

  it('does not render <ContextMenu', () => {
    expect(SRC).not.toContain('<ContextMenu');
  });
});
