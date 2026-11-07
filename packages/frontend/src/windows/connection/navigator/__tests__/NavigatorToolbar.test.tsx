import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { NavigatorToolbar } from '../NavigatorToolbar';

afterEach(cleanup);

const props = {
  t: (key: string) => key,
  searchQuery: '',
  setSearchQuery: vi.fn(),
  onNewConnection: vi.fn(),
  onNewGroup: vi.fn(),
  onCollapseAll: vi.fn(),
};

describe('NavigatorToolbar', () => {
  it('does not expose the database object search entry', () => {
    render(<NavigatorToolbar {...props} />);

    expect(screen.getByTestId('connection-search-input')).toBeInTheDocument();
    expect(screen.queryByTestId('global-object-search-toggle')).not.toBeInTheDocument();
  });
});
