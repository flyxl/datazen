import { describe, expect, it, beforeEach } from 'vitest';
import { nextPanelId, resetPanelIdCounter, resolveNextActive } from '../panelTypes';
import type { QueryPanel } from '../panelTypes';

describe('[tester] panelTypes', () => {
  beforeEach(() => {
    resetPanelIdCounter();
  });

  it('nextPanelId generates unique prefixed ids', () => {
    expect(nextPanelId('tbl')).toMatch(/^panel-tbl-\d+$/);
    expect(nextPanelId('tbl')).not.toBe(nextPanelId('tbl'));
  });

  it('resolveNextActive keeps current when removing unrelated panel', () => {
    const panels: QueryPanel[] = [
      {
        id: 'a',
        type: 'query',
        title: 'A',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
      {
        id: 'b',
        type: 'query',
        title: 'B',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
    ];
    expect(resolveNextActive(panels, 'b', 'a')).toBe('a');
  });

  it('resolveNextActive selects neighbor when active panel is removed', () => {
    const panels: QueryPanel[] = [
      {
        id: 'a',
        type: 'query',
        title: 'A',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
      {
        id: 'b',
        type: 'query',
        title: 'B',
        connectionId: 'c',
        dbSessionId: 's',
        connectionName: 'n',
        databaseType: 'postgresql',
      },
    ];
    expect(resolveNextActive(panels, 'b', 'b')).toBe('a');
    expect(resolveNextActive([panels[0]], 'a', 'a')).toBeNull();
  });
});
