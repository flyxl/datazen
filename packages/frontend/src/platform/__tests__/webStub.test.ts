import { afterEach, describe, expect, it } from 'vitest';

import { ApiError } from '@datazen/backend-client';
import { clearPlatform, requireCapabilities, requirePlatform } from '../contracts';
import { clearPlatformServices } from '@datazen/backend-client';
import { assembleWebPlatformStub, createWebBackendTransportStub } from '../web/stub';

/**
 * Form-split F5 web stub behavior: every method throws the §13 code (no
 * silent lies), and the assembled bindings resolve the web capability table.
 * The compile-time half of the guarantee is the stub itself surviving
 * `typecheck` — see plan §5.5 item 3.
 */

afterEach(() => {
  clearPlatform();
  clearPlatformServices();
});

describe('web backend transport stub', () => {
  it('every call throws CapabilityUnsupported — an explicit gap, not an empty answer', async () => {
    const transport = createWebBackendTransportStub();
    await expect(transport.call('getSettings', undefined)).rejects.toBeInstanceOf(ApiError);
    await expect(transport.call('listConnections', undefined)).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
    await expect(transport.call('getTableData', {} as never)).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
  });
});

describe('web platform stub assembly', () => {
  it('binds every port family the desktop assembly binds', () => {
    const bindings = assembleWebPlatformStub();
    expect(requirePlatform().events).toBe(bindings.events);
    expect(requirePlatform().windows).toBe(bindings.windows);
  });

  it('resolves the web capability table: no updater, build-constant version', () => {
    assembleWebPlatformStub();
    expect(requireCapabilities().updater).toBe(false);
    expect(requireCapabilities().versionSource).toBe('build-constant');
    expect(requireCapabilities().settingsSections.has('mcpServer')).toBe(false);
    expect(requireCapabilities().settingsSections.has('general')).toBe(true);
  });
});
