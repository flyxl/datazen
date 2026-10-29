import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { Select } from '@datazen/ui';
import { HttpProxyTunnelFields } from '../HttpProxyTunnelFields';
import { WebSocketTunnelFields } from '../WebSocketTunnelFields';
import type {
  HttpProxyTunnelFieldsValue,
  WebSocketTunnelFieldsValue,
} from '../tunnelFieldContracts';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

/**
 * [tester] — regression cover for the `data-testid` swallow in
 * `@datazen/ui`'s `Select` (see `packages/ui/src/Select.tsx` and
 * `packages/ui/src/dataAttrs.ts`).
 *
 * `Select` declares its own prop list, and TypeScript exempts hyphenated JSX
 * attribute names from excess-property checking. So `<Select data-testid="x" />`
 * compiled while the attribute was discarded at render: the
 * `new-conn-http-proxy-scheme`, `new-conn-ws-mode` and `dashboard-target-select`
 * locators marked nothing, and a test written against the project's `data-*`
 * binding rule would go green against an element that did not exist.
 *
 * These locators are live now and land on the listbox trigger. Asserting them
 * is the point: a locator that silently stops resolving is how this class of
 * defect stays invisible in the first place.
 */
describe('[tester] Select data-* forwarding', () => {
  it('lands a hyphenated data-testid on the listbox trigger', () => {
    render(
      <Select
        value="http"
        options={[
          { value: 'http', label: 'HTTP' },
          { value: 'https', label: 'HTTPS' },
        ]}
        onChange={() => {}}
        data-testid="probe-live"
      />,
    );

    const trigger = screen.getByTestId('probe-live');
    expect(trigger.tagName).toBe('BUTTON');
    expect(trigger).toHaveAttribute('aria-haspopup', 'listbox');
  });

  it('still forwards the explicitly supported triggerDataAttrs escape hatch', () => {
    render(
      <Select
        value="http"
        options={[{ value: 'http', label: 'HTTP' }]}
        onChange={() => {}}
        triggerDataAttrs={{ 'data-testid': 'probe-hatch' }}
      />,
    );

    expect(screen.getByTestId('probe-hatch')).toBeInTheDocument();
  });

  it('resolves the http proxy / websocket scheme and mode locators', () => {
    const http: HttpProxyTunnelFieldsValue = {
      httpProxyHost: '',
      setHttpProxyHost: vi.fn(),
      httpProxyPort: '8080',
      setHttpProxyPort: vi.fn(),
      httpProxyScheme: 'http',
      setHttpProxyScheme: vi.fn(),
      httpProxyUsername: '',
      setHttpProxyUsername: vi.fn(),
      httpProxyPassword: '',
      setHttpProxyPassword: vi.fn(),
      httpProxyTimeout: '30',
      setHttpProxyTimeout: vi.fn(),
    };
    const httpRender = render(<HttpProxyTunnelFields form={http} />);
    // Both halves are asserted on purpose. The `aria-haspopup` attribute alone
    // would still pass if the tunnel-field trigger stopped being a <button>,
    // so the tag is checked independently of the attribute that carries it.
    expect(httpRender.container.querySelector('button[aria-haspopup="listbox"]')).not.toBeNull();
    expect(screen.getByTestId('new-conn-http-proxy-scheme')).toHaveAttribute(
      'aria-haspopup',
      'listbox',
    );
    httpRender.unmount();

    const ws: WebSocketTunnelFieldsValue = {
      wsUrl: '',
      setWsUrl: vi.fn(),
      wsMode: 'datazen_v1',
      setWsMode: vi.fn(),
      wsAuthToken: '',
      setWsAuthToken: vi.fn(),
      wsTimeout: '30',
      setWsTimeout: vi.fn(),
    };
    const wsRender = render(<WebSocketTunnelFields form={ws} />);
    expect(wsRender.container.querySelector('button[aria-haspopup="listbox"]')).not.toBeNull();
    expect(screen.getByTestId('new-conn-ws-mode')).toHaveAttribute('aria-haspopup', 'listbox');
    wsRender.unmount();
  });

  it('drives the settings editor through its triggerDataAttrs locator', () => {
    const onChange = vi.fn();
    render(
      <Select
        value="ssh"
        options={[{ value: 'ssh', label: 'SSH' }]}
        onChange={onChange}
        triggerDataAttrs={{ 'data-testid': 'tunnel-edit-kind' }}
      />,
    );
    const trigger = screen.getByTestId('tunnel-edit-kind');
    expect(trigger).toHaveAttribute('aria-haspopup', 'listbox');

    fireEvent.click(trigger);
    // Options are picked on mousedown (so the listbox does not lose focus first).
    // The selected option's accessible name also carries its "✓" mark.
    fireEvent.mouseDown(screen.getByRole('option', { name: /^SSH/ }));
    expect(onChange).toHaveBeenCalledWith('ssh');
  });
});
