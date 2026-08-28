import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { SchemaDiffEndpointsBar } from '../SchemaDiffEndpointsBar';

const stableT = (key: string) => key;

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT, language: 'en' }),
}));

const baseProps = {
  sourceId: 'src-1',
  targetId: 'tgt-1',
  sourceDatabase: 'app',
  targetDatabase: 'app',
  sourceSchema: 'public',
  targetSchema: 'public',
  sourceDatabases: ['app'],
  targetDatabases: ['app'],
  sourceSchemas: ['public'],
  targetSchemas: ['public'],
  connOptions: [
    { value: 'src-1', label: 'PG (postgresql)' },
    { value: 'tgt-1', label: 'MySQL (mysql)' },
  ],
  targetOptions: [
    { value: 'src-1', label: 'PG (postgresql)' },
    { value: 'tgt-1', label: 'MySQL (mysql)' },
  ],
  isCrossDialect: true,
  busy: false,
  onSourceChange: vi.fn(),
  onTargetChange: vi.fn(),
  onSourceDatabaseChange: vi.fn(),
  onTargetDatabaseChange: vi.fn(),
  onSourceSchemaChange: vi.fn(),
  onTargetSchemaChange: vi.fn(),
  onSwap: vi.fn(),
  onCompare: vi.fn(),
};

describe('SchemaDiffEndpointsBar', () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('renders source/target endpoints, schema selects, and compare action', () => {
    render(<SchemaDiffEndpointsBar {...baseProps} />);

    expect(screen.getByTestId('schema-diff-source')).toBeTruthy();
    expect(screen.getByTestId('schema-diff-target')).toBeTruthy();
    expect(screen.getByTestId('schema-diff-source-database')).toBeTruthy();
    expect(screen.getByTestId('schema-diff-target-database')).toBeTruthy();
    expect(screen.getByTestId('schema-diff-source-schema')).toBeTruthy();
    expect(screen.getByTestId('schema-diff-target-schema')).toBeTruthy();
    expect(screen.getByTestId('schema-diff-cross-dialect-note')).toBeTruthy();
    expect(screen.getByTestId('schema-diff-compare')).toBeTruthy();
    expect(screen.getByText('schemaDiff.compare')).toBeTruthy();
  });

  it('invokes swap and compare handlers', () => {
    const onSwap = vi.fn();
    const onCompare = vi.fn();
    render(<SchemaDiffEndpointsBar {...baseProps} onSwap={onSwap} onCompare={onCompare} />);

    fireEvent.click(screen.getByTestId('schema-diff-swap'));
    fireEvent.click(screen.getByTestId('schema-diff-compare'));

    expect(onSwap).toHaveBeenCalledTimes(1);
    expect(onCompare).toHaveBeenCalledTimes(1);
  });

  it('hides schema selects when schema lists are empty', () => {
    render(
      <SchemaDiffEndpointsBar
        {...baseProps}
        sourceSchemas={[]}
        targetSchemas={[]}
        isCrossDialect={false}
      />,
    );

    expect(screen.queryByTestId('schema-diff-source-schema')).toBeNull();
    expect(screen.queryByTestId('schema-diff-target-schema')).toBeNull();
    expect(screen.queryByTestId('schema-diff-cross-dialect-note')).toBeNull();
  });
});
