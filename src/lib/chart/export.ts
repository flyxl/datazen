import { fileCommands } from '../../commands/file';
import { readCssColor } from './colors';

export async function exportChartAsPng(
  chartElement: HTMLElement,
  filename: string,
): Promise<void> {
  const { toPng } = await import('html-to-image');
  const bgColor = readCssColor('--c-surface', '#1a1a2e');
  const dataUrl = await toPng(chartElement, { backgroundColor: bgColor });
  const base64 = dataUrl.includes(',') ? dataUrl.split(',')[1]! : dataUrl;
  await fileCommands.saveBase64WithDialog(base64, `${filename}.png`, 'PNG', ['png']);
}

export async function exportChartAsSvg(
  chartElement: HTMLElement,
  filename: string,
): Promise<void> {
  const svgEl = chartElement.querySelector('svg');
  if (!svgEl) return;

  const svgData = new XMLSerializer().serializeToString(svgEl);
  await fileCommands.saveTextWithDialog(svgData, `${filename}.svg`, 'SVG', ['svg']);
}
