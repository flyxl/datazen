export async function exportChartAsPng(
  chartElement: HTMLElement,
  filename: string,
): Promise<void> {
  const { toPng } = await import('html-to-image');
  const style = getComputedStyle(document.documentElement);
  const bgColor = style.getPropertyValue('--bg-base').trim() || '#1a1a2e';
  const dataUrl = await toPng(chartElement, { backgroundColor: bgColor });

  const link = document.createElement('a');
  link.download = `${filename}.png`;
  link.href = dataUrl;
  link.click();
}

export async function exportChartAsSvg(
  chartElement: HTMLElement,
  filename: string,
): Promise<void> {
  const svgEl = chartElement.querySelector('svg');
  if (!svgEl) return;

  const svgData = new XMLSerializer().serializeToString(svgEl);
  const blob = new Blob([svgData], { type: 'image/svg+xml;charset=utf-8' });
  const url = URL.createObjectURL(blob);

  const link = document.createElement('a');
  link.download = `${filename}.svg`;
  link.href = url;
  link.click();

  URL.revokeObjectURL(url);
}
