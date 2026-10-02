// 조각 미니 그림.
import type { Cell } from '../types';
import { normCells } from '../presses';

export function miniShape(cells: Cell[], size: number, color?: string): string {
  const n = normCells(cells);
  let h = 0;
  let w = 0;
  const on = new Set<string>();
  for (const [r, c] of n) {
    h = Math.max(h, r + 1);
    w = Math.max(w, c + 1);
    on.add(`${r},${c}`);
  }
  let html = '';
  for (let r = 0; r < h; r++) {
    for (let c = 0; c < w; c++) {
      const isOn = on.has(`${r},${c}`);
      html += `<i class="${isOn ? 'on' : ''}" style="width:${size}px;height:${size}px${isOn && color ? `;background:${color}` : ''}"></i>`;
    }
  }
  return `<div class="mini" style="grid-template-columns:repeat(${w},${size}px)">${html}</div>`;
}

export function esc(s: string): string {
  return s.replace(/[&<>"]/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[ch] ?? ch);
}

export function fmt(n: number): string {
  return Math.round(n).toLocaleString();
}
