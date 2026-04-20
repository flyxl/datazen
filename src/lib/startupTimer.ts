const origin = performance.now();
let last = origin;

function wallClock(): string {
  const d = new Date();
  const pad2 = (n: number) => String(n).padStart(2, '0');
  const pad3 = (n: number) => String(n).padStart(3, '0');
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}.${pad3(d.getMilliseconds())}`;
}

export function mark(label: string) {
  const now = performance.now();
  const ts = Math.round(now - origin);
  const delta = Math.round(now - last);
  last = now;
  console.log(`${wallClock()} [startup] +${String(ts).padStart(5)}ms  Δ${String(delta).padStart(4)}ms  ${label}`);
}
