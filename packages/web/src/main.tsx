/**
 * Browser entrypoint.
 *
 * The standard React host remains the default. The opt-in ?ui=rune path loads
 * the shared Rust view without importing the React application bundle.
 */
async function bootstrap(): Promise<void> {
  const requestedUi = new URLSearchParams(window.location.search).get('ui');

  if (requestedUi === 'rune') {
    const { startRuneUi } = await import('./rune-ui');
    await startRuneUi();
    return;
  }

  const [{ startSharedApp }, { assembleWebPlatform }] = await Promise.all([
    import('@datazen/frontend/app'),
    import('./platform/web/assembly'),
  ]);

  assembleWebPlatform();
  startSharedApp();
}

void bootstrap();
