import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { ContextPicker } from '../ContextPicker';
import type { ContextEntry } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const mockListFiles = vi.fn<(query?: string) => Promise<ContextEntry[]>>();

vi.mock('../../../commands/context', () => ({
  contextCommands: {
    listFiles: (...args: Parameters<typeof mockListFiles>) => mockListFiles(...args),
  },
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
});

const sampleEntries: ContextEntry[] = [
  { name: 'schema.sql', path: 'schema.sql', isDir: false, size: 1024 },
  { name: 'notes.md', path: 'notes.md', isDir: false, size: 256 },
  {
    name: 'docs',
    path: 'docs',
    isDir: true,
    children: [
      { name: 'api.yaml', path: 'docs/api.yaml', isDir: false, size: 512 },
    ],
  },
];

function renderPicker(query = '', onSelect = vi.fn(), onClose = vi.fn()) {
  const anchorRef = { current: document.createElement('div') };
  return {
    onSelect,
    onClose,
    ...render(
      <ContextPicker
        query={query}
        onSelect={onSelect}
        onClose={onClose}
        anchorRef={anchorRef}
      />,
    ),
  };
}

describe('ContextPicker', () => {
  it('shows loading state initially', () => {
    mockListFiles.mockReturnValue(new Promise(() => {}));
    const { getByText } = renderPicker();
    expect(getByText('common.loading')).toBeInTheDocument();
  });

  it('renders file entries after loading', async () => {
    mockListFiles.mockResolvedValue(sampleEntries);
    const { getByText } = renderPicker();

    await waitFor(() => {
      expect(getByText('schema.sql')).toBeInTheDocument();
    });

    expect(getByText('notes.md')).toBeInTheDocument();
    expect(getByText('docs')).toBeInTheDocument();
    expect(getByText('api.yaml')).toBeInTheDocument();
  });

  it('shows empty message when no files', async () => {
    mockListFiles.mockResolvedValue([]);
    const { getByText } = renderPicker();

    await waitFor(() => {
      expect(getByText('context.noFiles')).toBeInTheDocument();
    });
  });

  it('shows no-results message when query has no matches', async () => {
    mockListFiles.mockResolvedValue([]);
    const { getByText } = renderPicker('nonexistent');

    await waitFor(() => {
      expect(getByText('context.noResults')).toBeInTheDocument();
    });
  });

  it('passes query to listFiles', async () => {
    mockListFiles.mockResolvedValue([]);
    renderPicker('schema');

    await waitFor(() => {
      expect(mockListFiles).toHaveBeenCalledWith('schema');
    });
  });

  it('calls onSelect when file is clicked', async () => {
    mockListFiles.mockResolvedValue(sampleEntries);
    const { getByText, onSelect } = renderPicker();

    await waitFor(() => {
      expect(getByText('schema.sql')).toBeInTheDocument();
    });

    fireEvent.click(getByText('schema.sql'));
    expect(onSelect).toHaveBeenCalledWith(sampleEntries[0]);
  });

  it('calls onClose on Escape key', async () => {
    mockListFiles.mockResolvedValue(sampleEntries);
    const { onClose } = renderPicker();

    await waitFor(() => {
      expect(mockListFiles).toHaveBeenCalled();
    });

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('navigates with arrow keys and selects with Enter', async () => {
    mockListFiles.mockResolvedValue(sampleEntries);
    const { onSelect } = renderPicker();

    await waitFor(() => {
      expect(mockListFiles).toHaveBeenCalled();
    });

    // Wait for entries to render
    await vi.waitFor(() => {
      expect(document.querySelectorAll('button[data-active]').length).toBeGreaterThan(0);
    });

    // First entry is active by default, move down
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'Enter' });

    expect(onSelect).toHaveBeenCalledWith(sampleEntries[1]);
  });

  it('displays file sizes in human-readable format', async () => {
    mockListFiles.mockResolvedValue([
      { name: 'small.txt', path: 'small.txt', isDir: false, size: 100 },
      { name: 'medium.sql', path: 'medium.sql', isDir: false, size: 2048 },
    ]);
    const { getByText } = renderPicker();

    await waitFor(() => {
      expect(getByText('100B')).toBeInTheDocument();
      expect(getByText('2.0KB')).toBeInTheDocument();
    });
  });

  it('displays directory badge', async () => {
    mockListFiles.mockResolvedValue([
      {
        name: 'schemas',
        path: 'schemas',
        isDir: true,
        children: [
          { name: 'a.sql', path: 'schemas/a.sql', isDir: false, size: 10 },
        ],
      },
    ]);
    const { getByText } = renderPicker();

    await waitFor(() => {
      expect(getByText('context.dir')).toBeInTheDocument();
    });
  });
});
