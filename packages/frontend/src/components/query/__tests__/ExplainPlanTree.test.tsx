import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { ExplainPlanTree } from '../ExplainPlanTree';

describe('ExplainPlanTree', () => {
  it('renders driver-neutral plan tree nodes', () => {
    const { getByText } = render(
      <ExplainPlanTree
        planTree={{
          id: 'root',
          label: 'Seq Scan · users',
          cost: 12.5,
          rows: 100,
          details: [{ key: 'Filter', value: 'id > 1' }],
          children: [
            {
              id: 'root.0',
              label: 'Index Scan · users_pkey',
              cost: 0.5,
              details: [],
              children: [],
            },
          ],
        }}
      />,
    );

    expect(getByText('Seq Scan · users')).toBeInTheDocument();
    expect(getByText('Index Scan · users_pkey')).toBeInTheDocument();
    expect(getByText('cost 12.50')).toBeInTheDocument();
  });
});
