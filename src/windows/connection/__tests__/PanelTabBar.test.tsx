import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Panel } from '../../../stores/panelStore';
import { PanelTabBar } from '../PanelTabBar';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string>) =>
      key === 'panel.closeTab' ? `panel.closeTab ${params?.title ?? ''}` : key,
  }),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: (selector: (state: { schemas: Map<string, unknown> }) => unknown) =>
    selector({ schemas: new Map() }),
}));

vi.mock('../../../components/ThemedIcon', () => ({
  ThemedIcon: () => <span aria-hidden="true" />,
}));

afterEach(cleanup);

const panels: Panel[] = [
  {
    id: 'panel-one',
    type: 'query',
    title: 'Query 1',
    connectionId: 'connection-1',
    dbSessionId: 'session-1',
    connectionName: 'Local',
    databaseType: 'postgres',
  },
  {
    id: 'panel-two',
    type: 'query',
    title: 'Query 2',
    connectionId: 'connection-1',
    dbSessionId: 'session-1',
    connectionName: 'Local',
    databaseType: 'postgres',
  },
];

describe('PanelTabBar accessibility and keyboard navigation', () => {
  it('exposes tabs and labelled close buttons', () => {
    render(
      <PanelTabBar
        panels={panels}
        activePanelId="panel-one"
        onSelectPanel={vi.fn()}
        onClosePanel={vi.fn()}
        onContextMenu={vi.fn()}
      />,
    );

    expect(screen.getByRole('tablist', { name: 'panel.tabListLabel' })).toBeInTheDocument();
    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(2);
    expect(tabs[0]).toHaveAttribute('aria-selected', 'true');
    expect(tabs[0]).toHaveAttribute('tabindex', '0');
    expect(tabs[1]).toHaveAttribute('tabindex', '-1');
    expect(
      screen.getByRole('button', { name: 'panel.closeTab Local · Query 1' }),
    ).toBeInTheDocument();
  });

  it('moves focus and selects the adjacent tab with arrow keys', () => {
    const onSelectPanel = vi.fn();
    render(
      <PanelTabBar
        panels={panels}
        activePanelId="panel-one"
        onSelectPanel={onSelectPanel}
        onClosePanel={vi.fn()}
        onContextMenu={vi.fn()}
      />,
    );

    const tabs = screen.getAllByRole('tab');
    tabs[0].focus();
    fireEvent.keyDown(tabs[0], { key: 'ArrowRight' });

    expect(document.activeElement).toBe(tabs[1]);
    expect(onSelectPanel).toHaveBeenCalledWith('panel-two');
  });
});
