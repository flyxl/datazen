import { fileCommands } from '../../commands/file';

export async function exportChartAsPng(
  chartElement: HTMLElement,
  filename: string,
): Promise<void> {
  const { save } = await import('@tauri-apps/plugin-dialog');
  const filePath = await save({
    defaultPath: `${filename}.png`,
    filters: [{ name: 'PNG', extensions: ['png'] }],
  });
  if (!filePath) return;

  const { toPng } = await import('html-to-image');
  const style = getComputedStyle(document.documentElement);
  const bgColor = style.getPropertyValue('--bg-base').trim() || '#1a1a2e';
  const dataUrl = await toPng(chartElement, { backgroundColor: bgColor });
  const base64 = dataUrl.includes(',') ? dataUrl.split(',')[1]! : dataUrl;
  await fileCommands.writeFileBase64(filePath, base64);
}

export async function exportChartAsSvg(
  chartElement: HTMLElement,
  filename: string,
): Promise<void> {
  const svgEl = chartElement.querySelector('svg');
  if (!svgEl) return;

  const { save } = await import('@tauri-apps/plugin-dialog');
  const filePath = await save({
    defaultPath: `${filename}.svg`,
    filters: [{ name: 'SVG', extensions: ['svg'] }],
  });
  if (!filePath) return;

  const svgData = new XMLSerializer().serializeToString(svgEl);
  await fileCommands.writeFile(filePath, svgData);
}
