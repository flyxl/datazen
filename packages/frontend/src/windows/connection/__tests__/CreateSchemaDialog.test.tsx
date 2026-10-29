import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { CreateSchemaDialog } from '../CreateSchemaDialog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const driverExecute = vi.fn();
const useConnectionCommand = vi.fn();

vi.mock('../../../commands/driver', () => ({
  driverCommands: {
    execute: (...args: unknown[]) => driverExecute(...args),
  },
}));

vi.mock('../../../hooks/useConnectionCommand', () => ({
  useConnectionCommand: (...args: unknown[]) => useConnectionCommand(...args),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  useConnectionCommand.mockReturnValue({ definition: undefined });
  driverExecute.mockResolvedValue({});
});

describe('CreateSchemaDialog', () => {
  it('pins create_schema to the target database', async () => {
    const onCreated = vi.fn();
    render(
      <CreateSchemaDialog
        open
        dbSessionId="conn-pg"
        database="goecoride"
        onCreated={onCreated}
        onClose={() => undefined}
      />,
    );

    fireEvent.change(screen.getByPlaceholderText('my_schema'), {
      target: { value: 'analytics' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'common.createSchema' }));

    await waitFor(() => {
      expect(driverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-pg',
        command: 'create_schema',
        input: { name: 'analytics' },
        database: 'goecoride',
      });
      expect(onCreated).toHaveBeenCalled();
    });
  });
});
