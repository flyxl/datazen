import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { WorkspaceDefaultCards } from '../WorkspaceDefaultCards';
import type { WorkspacePageEntry } from '../workspacePages';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

function makeEntry(overrides: Partial<WorkspacePageEntry> = {}): WorkspacePageEntry {
  return {
    key: 'acme.bill-audit:quota-check',
    pluginId: 'acme.bill-audit',
    pageId: 'quota-check',
    title: 'Quota Check',
    version: '1.0.0',
    author: 'Acme',
    description: 'Compare bills against quotas',
    ...overrides,
  };
}

let pages: WorkspacePageEntry[];
const onOpen = vi.fn();
const onOpenPlugins = vi.fn();

beforeEach(() => {
  pages = [];
  onOpen.mockClear();
  onOpenPlugins.mockClear();
});

afterEach(cleanup);

describe('WorkspaceDefaultCards', () => {
  it('renders a card per page with icon/name/description/version/author', () => {
    pages = [
      makeEntry(),
      makeEntry({
        key: 'acme.afi:pricing',
        pluginId: 'acme.afi',
        pageId: 'pricing',
        title: 'Pricing Viewer',
        version: '2.1.4',
        author: undefined,
        description: undefined,
      }),
    ];

    render(<WorkspaceDefaultCards pages={pages} onOpen={onOpen} onOpenPlugins={onOpenPlugins} />);

    expect(screen.getAllByTestId('workspace-default-card')).toHaveLength(2);
    expect(screen.getByText('Quota Check')).toBeInTheDocument();
    expect(screen.getByText('Compare bills against quotas')).toBeInTheDocument();
    expect(screen.getByText('v1.0.0 · Acme')).toBeInTheDocument();
    expect(screen.getByText('v2.1.4')).toBeInTheDocument();
  });

  it('opens the corresponding tab when a card is clicked', () => {
    pages = [makeEntry()];

    render(<WorkspaceDefaultCards pages={pages} onOpen={onOpen} />);

    fireEvent.click(screen.getByTestId('workspace-default-card'));
    expect(onOpen).toHaveBeenCalledWith(pages[0]);
  });

  it('shows the empty-state guidance with a shortcut to the plugins page', () => {
    render(<WorkspaceDefaultCards pages={[]} onOpen={onOpen} onOpenPlugins={onOpenPlugins} />);

    expect(screen.getByTestId('workspace-empty')).toBeInTheDocument();
    expect(screen.queryByTestId('workspace-default-card')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-open-plugins'));
    expect(onOpenPlugins).toHaveBeenCalledOnce();
    expect(onOpen).not.toHaveBeenCalled();
  });
});
