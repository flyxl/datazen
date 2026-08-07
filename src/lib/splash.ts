export function hideSplash(splash: HTMLElement | null): void {
  if (!splash) return;
  splash.classList.add('hide');
  window.setTimeout(() => splash.remove(), 350);
}
